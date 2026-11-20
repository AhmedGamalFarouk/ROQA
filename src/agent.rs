//! AI agent status: what a coding agent reported with `roqa.exe agent <name> <state>` (working, needs you, done),
//! and the progress any script reports with `roqa.exe progress <name> <0-100|done>`.
//! The client drops one small file per report in a spool folder and signals the resident process, which reads and
//! removes each (a name cannot travel in a named event). The tracker is pure: the host asks it for the pill text, the
//! eyes' state and the one deadline it must wake for.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    sync::{
        Mutex, PoisonError,
        atomic::{AtomicU32, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Working,
    Needs,
    Done,
}
impl State {
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "working" => Some(Self::Working),
            "needs" => Some(Self::Needs),
            "done" => Some(Self::Done),
            _ => None,
        }
    }
    pub fn word(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Needs => "needs",
            Self::Done => "done",
        }
    }
}

/// One line of the spool: an agent's state, or a script's progress in percent (100 is done).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Report {
    State(State),
    Percent(u8),
    /// A permission question: the hook's process id and the line to show.
    Ask(u32, String),
}

/// How long a permission question waits for a click, in seconds, before the agent's own prompt takes over.
pub const ASK_SECS: u64 = 30;

/// "0-100" or "done" as `roqa.exe progress` takes it.
pub fn percent(word: &str) -> Option<u8> {
    match word {
        "done" => Some(100),
        _ => word.parse().ok().filter(|p| *p <= 100),
    }
}

/// The progress pill's words: "Build 42%", "Build done".
pub fn progress_text(name: &str, percent: u8) -> String {
    let shown = title(name);
    if percent >= 100 {
        format!("{shown} done")
    } else {
        format!("{shown} {percent}%")
    }
}

/// How long a "needs you" or "done" shows on the eyes (the pill's notice lasts as long).
const SHOWN: Duration = Duration::from_secs(8);
/// A "working" report holds until the agent says otherwise; this ends one that never did (an interrupted run).
const STALE: Duration = Duration::from_secs(10 * 60);
/// Agents tracked at once; the least recent is dropped for a new one.
const MAX: usize = 8;
/// The longest agent name kept.
const NAME: usize = 24;
/// A spool file is one short line; a bigger one is not read.
const SPOOL_CAP: u64 = 4096;
/// Spool files read at once; the rest wait for the next call.
const SPOOL_FILES: usize = 64;

/// A name as typed: letters, digits, `-`, `_` and `.`, at most `NAME` characters.
pub fn clean(name: &str) -> Option<&str> {
    let ok = !name.is_empty()
        && name.len() <= NAME
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    ok.then_some(name)
}

/// The words the pill shows: "Codex needs you", "Gemini is done".
pub fn text(name: &str, state: State) -> String {
    let shown = title(name);
    match state {
        State::Working => format!("{shown} is working"),
        State::Needs => format!("{shown} needs you"),
        State::Done => format!("{shown} is done"),
    }
}

fn title(name: &str) -> String {
    let mut chars = name.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

struct Entry {
    name: String,
    state: State,
    until: Instant,
}

#[derive(Default)]
pub struct Agents {
    /// Least recent first.
    list: Vec<Entry>,
}
impl Agents {
    /// Records a report. Returns the pill text, or `None` for a repeated "working" (it only extends the hold).
    pub fn set(&mut self, name: &str, state: State, now: Instant) -> Option<String> {
        let quiet = self.list.iter().any(|e| {
            e.name == name && e.state == state && state == State::Working && now < e.until
        });
        self.list.retain(|e| e.name != name);
        if self.list.len() == MAX {
            self.list.remove(0);
        }
        let hold = if state == State::Working {
            STALE
        } else {
            SHOWN
        };
        self.list.push(Entry {
            name: name.to_owned(),
            state,
            until: now + hold,
        });
        (!quiet).then(|| text(name, state))
    }
    pub fn clear(&mut self) {
        self.list.clear();
    }
    /// Drops what ran out.
    pub fn prune(&mut self, now: Instant) {
        self.list.retain(|e| now < e.until);
    }
    /// What the eyes show: the most recent "needs you" or "done" still on show, else "working" while any agent
    /// works.
    pub fn state(&self, now: Instant) -> Option<State> {
        let live = || self.list.iter().filter(|e| now < e.until);
        live()
            .filter(|e| e.state != State::Working)
            .next_back()
            .or_else(|| live().find(|e| e.state == State::Working))
            .map(|e| e.state)
    }
    /// The one instant the host must wake for.
    pub fn deadline(&self) -> Option<Instant> {
        self.list.iter().map(|e| e.until).min()
    }
}

fn spool() -> Option<PathBuf> {
    let mut path = PathBuf::from(std::env::var_os("LOCALAPPDATA")?);
    path.push("ROQA");
    path.push("agent");
    Some(path)
}

/// A permission question from the hook `roqa.exe agent ask <name>`, as a spool line `ask <id> <name> <text>`: `id`
/// is the hook process, whose answer events the click will set.
pub fn write_ask(name: &str, id: u32, text: &str) -> std::io::Result<()> {
    append(&format!("ask {id} {name} {text}\n"))
}

/// The client's half: appends `name state` (or `name 42%`) for the resident process. Called only after its event
/// opened.
pub fn write(name: &str, report: Report) -> std::io::Result<()> {
    let word = match report {
        Report::State(state) => state.word().to_owned(),
        Report::Percent(percent) => format!("{percent}%"),
        // A question has its own writer, `write_ask`.
        Report::Ask(..) => return Ok(()),
    };
    append(&format!("{name} {word}\n"))
}

fn append(line: &str) -> std::io::Result<()> {
    append_in(&spool().ok_or(std::io::ErrorKind::NotFound)?, line)
}

/// One file per report, written under a `.tmp` name and then renamed, so the resident never reads half a report and
/// never deletes one that arrived while it was reading: a file it has listed is complete and is its alone.
// shortcut: a `.tmp` left by a hook killed mid-write stays (a few bytes); sweep old ones if that ever shows up.
#[expect(
    clippy::disallowed_methods,
    reason = "the spool is the agent hook's own one-file-per-report drop box, not the store"
)]
fn append_in(dir: &Path, line: &str) -> std::io::Result<()> {
    static COUNT: AtomicU32 = AtomicU32::new(0);
    fs::create_dir_all(dir)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let name = format!(
        "{stamp:020}-{}-{}",
        std::process::id(),
        COUNT.fetch_add(1, Ordering::Relaxed)
    );
    let tmp = dir.join(format!("{name}.tmp"));
    fs::write(&tmp, line)?;
    fs::rename(&tmp, dir.join(format!("{name}.txt")))
}

/// The resident half: the reports since the last call, oldest first. Anything unreadable is dropped. A batch is
/// bounded (`SPOOL_FILES`); when files are left it signals the Agent event again, so the next pass reads them.
pub fn take() -> Vec<(String, Report)> {
    let Some(dir) = spool() else {
        return Vec::new();
    };
    static STUCK: Mutex<BTreeSet<PathBuf>> = Mutex::new(BTreeSet::new());
    let mut stuck = STUCK.lock().unwrap_or_else(PoisonError::into_inner);
    let (reports, more) = take_in(&dir, &mut stuck);
    if more {
        let _ = crate::ipc::Command::Agent.signal();
    }
    reports
}

/// The reports of up to `SPOOL_FILES` files, and whether more files are waiting. Only that many names are listed.
/// A file that cannot be removed is read once and remembered in `stuck`: later drains skip it, so it neither
/// blocks the files behind it nor reports twice.
// shortcut: `stuck` grows with undeletable files until ROQA restarts; retry their removal if that ever matters.
fn take_in(dir: &Path, stuck: &mut BTreeSet<PathBuf>) -> (Vec<(String, Report)>, bool) {
    let Ok(entries) = fs::read_dir(dir) else {
        return (Vec::new(), false);
    };
    let mut files: Vec<_> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "txt") && !stuck.contains(path))
        .take(SPOOL_FILES + 1)
        .collect();
    // The names start with the time, so this is the order they were written in.
    // shortcut: the listing is the file system's order (NTFS lists by name); sort the whole spool if that changes.
    files.sort();
    // Every file taken now leaves the listing (removed or stuck), so each drain makes progress.
    let over = files.len() > SPOOL_FILES;
    let reports = files
        .into_iter()
        .take(SPOOL_FILES)
        .filter_map(|path| {
            let text = fs::metadata(&path)
                .ok()
                .filter(|meta| meta.len() <= SPOOL_CAP)
                .and_then(|_| fs::read_to_string(&path).ok());
            if fs::remove_file(&path).is_err() {
                stuck.insert(path);
            }
            text
        })
        .flat_map(|text| text.lines().filter_map(parse).collect::<Vec<_>>())
        .collect();
    (reports, over)
}

fn parse(line: &str) -> Option<(String, Report)> {
    if let Some(ask) = line.strip_prefix("ask ") {
        let mut parts = ask.trim_end().splitn(3, ' ');
        let id = parts.next()?.parse().ok()?;
        let name = clean(parts.next()?)?;
        let text = parts.next().filter(|text| !text.is_empty())?;
        return Some((name.to_owned(), Report::Ask(id, text.to_owned())));
    }
    let (name, word) = line.split_once(' ')?;
    let word = word.trim();
    let report = match word.strip_suffix('%') {
        Some(number) => Report::Percent(percent(number)?),
        None => Report::State(State::parse(word)?),
    };
    Some((clean(name)?.to_owned(), report))
}

/// The hook `roqa.exe agent ask <name>`: reads the permission question from stdin, shows it in the notch and waits
/// for the click, then prints Allow or Deny as the hook's answer. It prints nothing, so the agent asks as usual,
/// when ROQA is not running, the input is not a tool question or no click comes within `ASK_SECS`.
pub fn ask(name: &str) {
    use std::io::Read;
    use windows::Win32::{Foundation::WAIT_OBJECT_0, System::Threading::WaitForMultipleObjects};
    let mut input = String::new();
    let _ = std::io::stdin().take(64 * 1024).read_to_string(&mut input);
    let (Some(name), Some((line, whole))) =
        (clean(name), crate::agent_hooks::permission_line(&input))
    else {
        return;
    };
    // Answer in the terminal: say so on the pill, and print nothing so the agent asks at once.
    let needs_you = || {
        let _ = crate::ipc::Command::Agent.signal_after(|| {
            let _ = write(name, Report::State(State::Needs));
        });
    };
    if !whole || !crate::graphics::ask_fits(&line) {
        needs_you();
        return;
    }
    let id = std::process::id();
    let Ok(events) = crate::ipc::answer_events(id) else {
        return;
    };
    let mut written = Ok(());
    let shown = crate::ipc::Command::Agent.signal_after(|| written = write_ask(name, id, &line));
    // A question that never reached the spool cannot be answered: back to the terminal at once.
    if shown.is_err() || written.is_err() {
        return;
    }
    let handles = [events[0].0, events[1].0, events[2].0];
    // SAFETY: the handles stay open until this function returns.
    #[expect(
        clippy::disallowed_methods,
        reason = "the ask is a bounded wait on a worker for the user's answer"
    )]
    let woke = unsafe { WaitForMultipleObjects(&handles, false, (ASK_SECS * 1000) as u32) };
    let decision = match woke.0.wrapping_sub(WAIT_OBJECT_0.0) {
        0 => r#"{"behavior":"allow"}"#,
        1 => r#"{"behavior":"deny","message":"Denied from ROQA"}"#,
        // 2 is ROQA saying it cannot show the question: the terminal asks, and the pill says so.
        2 => return needs_you(),
        // Anything else is the wait running out.
        _ => return,
    };
    println!(
        r#"{{"hookSpecificOutput":{{"hookEventName":"PermissionRequest","decision":{decision}}}}}"#
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_use_the_agent_name() {
        assert_eq!(text("codex", State::Needs), "Codex needs you");
        assert_eq!(text("gemini", State::Done), "Gemini is done");
        assert_eq!(text("aider", State::Working), "Aider is working");
    }

    #[test]
    fn names_and_lines_are_checked() {
        assert!(clean("claude").is_some() && clean("my-agent_2.x").is_some());
        assert!(clean("").is_none() && clean("a b").is_none() && clean("a\"b").is_none());
        assert!(clean(&"x".repeat(NAME + 1)).is_none());
        let done = Some(("codex".into(), Report::State(State::Done)));
        assert_eq!(parse("codex done"), done);
        assert_eq!(parse("codex done\r"), done);
        assert!(parse("codex later").is_none() && parse("codex").is_none());
    }

    #[test]
    #[expect(
        clippy::disallowed_methods,
        reason = "the test plants a half-written spool file"
    )]
    fn the_spool_keeps_every_report_in_order_and_skips_one_still_being_written() {
        let dir = std::env::temp_dir().join(format!("roqa-spool-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut stuck = BTreeSet::new();
        assert!(take_in(&dir, &mut stuck).0.is_empty(), "no folder yet");
        append_in(&dir, "codex working\n").unwrap();
        append_in(&dir, "ask 7 claude Bash: ls\n").unwrap();
        fs::write(dir.join("0-1-1.tmp"), "codex done\n").unwrap();
        let first = take_in(&dir, &mut stuck);
        assert_eq!(
            first,
            (
                vec![
                    ("codex".into(), Report::State(State::Working)),
                    ("claude".into(), Report::Ask(7, "Bash: ls".into())),
                ],
                false
            )
        );
        // A report written after the first drain began is not in it and not lost.
        append_in(&dir, "build 50%\n").unwrap();
        assert_eq!(
            take_in(&dir, &mut stuck),
            (vec![("build".into(), Report::Percent(50))], false)
        );
        assert!(take_in(&dir, &mut stuck).0.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_burst_over_one_batch_is_all_read_by_the_following_drains() {
        let dir = std::env::temp_dir().join(format!("roqa-burst-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut stuck = BTreeSet::new();
        for step in 0..=SPOOL_FILES {
            append_in(&dir, &format!("build {}%\n", step % 100)).unwrap();
        }
        let (first, more) = take_in(&dir, &mut stuck);
        assert!(
            more && first.len() == SPOOL_FILES,
            "one batch, one file left"
        );
        let (rest, more) = take_in(&dir, &mut stuck);
        assert!(!more && rest == [("build".into(), Report::Percent(SPOOL_FILES as u8))]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    #[expect(
        clippy::disallowed_methods,
        reason = "the test holds a spool file open so it cannot be removed"
    )]
    fn files_that_cannot_be_removed_neither_block_nor_repeat() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = std::env::temp_dir().join(format!("roqa-stuck-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut stuck = BTreeSet::new();
        // A directory with a spool name is listed, read as nothing and cannot be removed as a file.
        for n in 0..SPOOL_FILES - 1 {
            fs::create_dir_all(dir.join(format!("{n:020}-1-1.txt"))).unwrap();
        }
        // A readable file whose removal is refused (opened without share-delete), then a valid report behind them.
        let held = dir.join(format!("{:020}-1-2.txt", SPOOL_FILES));
        fs::write(
            &held,
            "codex done
",
        )
        .unwrap();
        let _open = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&held)
            .unwrap();
        append_in(
            &dir,
            "build 50%
",
        )
        .unwrap();
        let (first, more) = take_in(&dir, &mut stuck);
        assert!(more, "the report behind a full batch is still waiting");
        assert_eq!(first, [("codex".into(), Report::State(State::Done))]);
        // The next drain skips the stuck names, reaches the new report and reads nothing twice.
        assert_eq!(
            take_in(&dir, &mut stuck),
            (vec![("build".into(), Report::Percent(50))], false)
        );
        assert_eq!(take_in(&dir, &mut stuck), (Vec::new(), false));
        drop(_open);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_question_line_carries_the_hook_and_the_words() {
        assert_eq!(
            parse("ask 4242 claude Bash: git push\r"),
            Some(("claude".into(), Report::Ask(4242, "Bash: git push".into())))
        );
        assert!(parse("ask x claude Bash").is_none() && parse("ask 1 claude").is_none());
        assert!(parse("ask 1 bad!name text").is_none());
    }

    #[test]
    fn progress_takes_a_percent_or_done() {
        assert_eq!(percent("42"), Some(42));
        assert_eq!(percent("done"), Some(100));
        assert!(percent("101").is_none() && percent("-1").is_none() && percent("x").is_none());
        assert_eq!(progress_text("build", 42), "Build 42%");
        assert_eq!(progress_text("build", 100), "Build done");
        assert_eq!(
            parse("build 42%"),
            Some(("build".into(), Report::Percent(42)))
        );
        assert!(parse("build 142%").is_none());
    }

    #[test]
    fn the_most_recent_report_shows_and_working_holds_the_eyes() {
        let t0 = Instant::now();
        let mut agents = Agents::default();
        assert_eq!(
            agents.set("claude", State::Working, t0).as_deref(),
            Some("Claude is working")
        );
        assert!(
            agents.set("claude", State::Working, t0).is_none(),
            "a repeat is quiet"
        );
        agents.set("codex", State::Working, t0);
        assert_eq!(agents.state(t0), Some(State::Working));
        let t1 = t0 + Duration::from_secs(1);
        assert_eq!(
            agents.set("codex", State::Done, t1).as_deref(),
            Some("Codex is done")
        );
        assert_eq!(agents.state(t1), Some(State::Done));
        // The done report runs out; Claude still works.
        let t2 = t1 + SHOWN;
        agents.prune(t2);
        assert_eq!(agents.state(t2), Some(State::Working));
        assert_eq!(agents.deadline(), Some(t0 + STALE));
        agents.set("claude", State::Needs, t2);
        assert_eq!(agents.state(t2), Some(State::Needs));
        agents.set("claude", State::Done, t2);
        agents.prune(t2 + SHOWN);
        assert_eq!(agents.state(t2 + SHOWN), None);
        assert_eq!(agents.deadline(), None);
    }

    #[test]
    fn a_working_report_that_never_ends_goes_stale() {
        let t0 = Instant::now();
        let mut agents = Agents::default();
        agents.set("claude", State::Working, t0);
        assert_eq!(agents.state(t0 + STALE), None);
        for n in 0..MAX + 3 {
            agents.set(&format!("a{n}"), State::Working, t0);
        }
        assert_eq!(agents.list.len(), MAX);
    }
}
