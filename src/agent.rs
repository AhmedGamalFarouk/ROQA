//! AI agent status: what a coding agent reported with `roqa.exe agent <name> <state>` (working, needs you, done),
//! and the progress any script reports with `roqa.exe progress <name> <0-100|done>`.
//! The client appends one line to a spool file and signals the resident process, which reads and removes it
//! (a name cannot travel in a named event). The tracker is pure: the host asks it for the pill text, the eyes'
//! state and the one deadline it must wake for.

use std::{
    fs,
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
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

/// The longest question line ROQA offers Allow for: the notch's two lines hold about this many characters, and a
/// longer command would be cut, so it is never approved half-read.
const ASK_WHOLE: usize = 60;

/// Whether the notch can show the whole question line.
fn shown_whole(line: &str) -> bool {
    line.chars().count() <= ASK_WHOLE
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
/// The spool is a few short lines; more than this is not read.
const SPOOL_CAP: u64 = 4096;

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
    let mut path = spool().ok_or(std::io::ErrorKind::NotFound)?;
    fs::create_dir_all(&path)?;
    path.push("agent.txt");
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    // One write, so two clients cannot interleave a line.
    file.write_all(line.as_bytes())
}

/// The resident half: the reports since the last call, oldest first. Anything unreadable is dropped.
pub fn take() -> Vec<(String, Report)> {
    let Some(path) = spool().map(|p| p.join("agent.txt")) else {
        return Vec::new();
    };
    let text = fs::metadata(&path)
        .ok()
        .filter(|m| m.len() <= SPOOL_CAP)
        .and_then(|_| fs::read_to_string(&path).ok());
    // shortcut: a line appended between the read and the removal is lost; reports are seconds apart.
    let _ = fs::remove_file(&path);
    text.iter()
        .flat_map(|t| t.lines())
        .filter_map(parse)
        .collect()
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
    let (Some(name), Some(line)) = (clean(name), crate::agent_hooks::permission_line(&input))
    else {
        return;
    };
    if !shown_whole(&line) {
        // Answer in the terminal: say so on the pill, and print nothing so the agent asks at once.
        let _ = crate::ipc::Command::Agent.signal_after(|| {
            let _ = write(name, Report::State(State::Needs));
        });
        return;
    }
    let id = std::process::id();
    let Ok(events) = crate::ipc::answer_events(id) else {
        return;
    };
    let shown = crate::ipc::Command::Agent.signal_after(|| {
        let _ = write_ask(name, id, &line);
    });
    if shown.is_err() {
        return;
    }
    let handles = [events[0].0, events[1].0];
    // SAFETY: both handles stay open until this function returns.
    let woke = unsafe { WaitForMultipleObjects(&handles, false, (ASK_SECS * 1000) as u32) };
    let decision = match woke.0.wrapping_sub(WAIT_OBJECT_0.0) {
        0 => r#"{"behavior":"allow"}"#,
        1 => r#"{"behavior":"deny","message":"Denied from ROQA"}"#,
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
    fn only_a_question_the_notch_shows_whole_gets_an_allow() {
        assert!(shown_whole("Bash: git push origin main"));
        assert!(!shown_whole(&format!("Bash: {}", "x".repeat(ASK_WHOLE))));
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
