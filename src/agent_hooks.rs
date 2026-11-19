//! "Add hook" for each coding agent: puts the one-line report (`roqa.exe agent <name> <state>`) into the agent's own
//! hook file, only after the user has seen exactly what is added, with a timestamped backup next to the file.
//! A hook file shared with other tools is merged, never replaced; "Undo" removes only ROQA's lines. Each
//! mechanism below was read from the agent's own documentation (Claude Code, Codex, Gemini CLI, Antigravity, Cursor,
//! GitHub Copilot CLI hooks pages; opencode's plugin types) and checked against the local installs.
//! There is no JSON library in the program, so a small one lives here: it keeps key order and rewrites the
//! file with two-space indents; a file it cannot read is never touched.

use crate::agent::State;
use std::{
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

/// A JSON value; numbers stay as written.
#[derive(Clone, Debug, PartialEq)]
enum Json {
    Null,
    Bool(bool),
    Num(String),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

fn obj<const N: usize>(fields: [(&str, Json); N]) -> Json {
    Json::Obj(fields.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}
fn string(text: &str) -> Json {
    Json::Str(text.to_owned())
}
fn number(n: u32) -> Json {
    Json::Num(n.to_string())
}

struct Reader<'a> {
    text: &'a [u8],
    at: usize,
}
impl Reader<'_> {
    fn space(&mut self) {
        while self.text.get(self.at).is_some_and(u8::is_ascii_whitespace) {
            self.at += 1;
        }
    }
    fn eat(&mut self, byte: u8) -> Option<()> {
        self.space();
        (self.text.get(self.at) == Some(&byte)).then(|| self.at += 1)
    }
    fn word(&mut self, word: &str) -> Option<()> {
        self.text[self.at..]
            .starts_with(word.as_bytes())
            .then(|| self.at += word.len())
    }
    fn value(&mut self, depth: u32) -> Option<Json> {
        self.space();
        if depth > 32 {
            return None;
        }
        match *self.text.get(self.at)? {
            b'{' => {
                self.at += 1;
                let mut fields = Vec::new();
                if self.eat(b'}').is_some() {
                    return Some(Json::Obj(fields));
                }
                loop {
                    self.space();
                    let Json::Str(key) = self.value(depth + 1)? else {
                        return None;
                    };
                    self.eat(b':')?;
                    fields.push((key, self.value(depth + 1)?));
                    if self.eat(b',').is_none() {
                        self.eat(b'}')?;
                        return Some(Json::Obj(fields));
                    }
                }
            }
            b'[' => {
                self.at += 1;
                let mut items = Vec::new();
                if self.eat(b']').is_some() {
                    return Some(Json::Arr(items));
                }
                loop {
                    items.push(self.value(depth + 1)?);
                    if self.eat(b',').is_none() {
                        self.eat(b']')?;
                        return Some(Json::Arr(items));
                    }
                }
            }
            b'"' => self.quoted().map(Json::Str),
            b't' => self.word("true").map(|()| Json::Bool(true)),
            b'f' => self.word("false").map(|()| Json::Bool(false)),
            b'n' => self.word("null").map(|()| Json::Null),
            _ => {
                let start = self.at;
                while self.text.get(self.at).is_some_and(|b| {
                    b.is_ascii_digit() || matches!(b, b'-' | b'+' | b'.' | b'e' | b'E')
                }) {
                    self.at += 1;
                }
                let raw = std::str::from_utf8(&self.text[start..self.at]).ok()?;
                raw.parse::<f64>().ok()?;
                Some(Json::Num(raw.to_owned()))
            }
        }
    }
    fn quoted(&mut self) -> Option<String> {
        self.at += 1;
        let mut out = Vec::new();
        loop {
            let byte = *self.text.get(self.at)?;
            self.at += 1;
            match byte {
                b'"' => return String::from_utf8(out).ok(),
                b'\\' => {
                    let escape = *self.text.get(self.at)?;
                    self.at += 1;
                    let ch = match escape {
                        b'"' | b'\\' | b'/' => char::from(escape),
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => self.unicode()?,
                        _ => return None,
                    };
                    out.extend(ch.to_string().into_bytes());
                }
                0..=0x1f => return None,
                _ => out.push(byte),
            }
        }
    }
    fn hex4(&mut self) -> Option<u32> {
        let digits = std::str::from_utf8(self.text.get(self.at..self.at + 4)?).ok()?;
        self.at += 4;
        u32::from_str_radix(digits, 16).ok()
    }
    fn unicode(&mut self) -> Option<char> {
        let first = self.hex4()?;
        if (0xd800..0xdc00).contains(&first) {
            self.word("\\u")?;
            let low = self.hex4()?;
            char::from_u32(0x10000 + ((first - 0xd800) << 10) + low.checked_sub(0xdc00)?)
        } else {
            char::from_u32(first)
        }
    }
}

impl Json {
    fn parse(text: &str) -> Option<Self> {
        let mut reader = Reader {
            text: text.trim_start_matches('\u{feff}').as_bytes(),
            at: 0,
        };
        let value = reader.value(0)?;
        reader.space();
        (reader.at == reader.text.len()).then_some(value)
    }
    fn write(&self, out: &mut String, indent: usize) {
        let pad = |out: &mut String, n: usize| out.push_str(&"  ".repeat(n));
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Self::Num(n) => out.push_str(n),
            Self::Str(s) => {
                out.push('"');
                for c in s.chars() {
                    match c {
                        '"' => out.push_str("\\\""),
                        '\\' => out.push_str("\\\\"),
                        '\n' => out.push_str("\\n"),
                        '\r' => out.push_str("\\r"),
                        '\t' => out.push_str("\\t"),
                        c if c < ' ' => {
                            let _ = write!(out, "\\u{:04x}", c as u32);
                        }
                        c => out.push(c),
                    }
                }
                out.push('"');
            }
            Self::Arr(items) if items.is_empty() => out.push_str("[]"),
            Self::Obj(fields) if fields.is_empty() => out.push_str("{}"),
            Self::Arr(items) => {
                out.push_str("[\n");
                for (i, item) in items.iter().enumerate() {
                    pad(out, indent + 1);
                    item.write(out, indent + 1);
                    out.push_str(if i + 1 < items.len() { ",\n" } else { "\n" });
                }
                pad(out, indent);
                out.push(']');
            }
            Self::Obj(fields) => {
                out.push_str("{\n");
                for (i, (key, value)) in fields.iter().enumerate() {
                    pad(out, indent + 1);
                    Self::Str(key.clone()).write(out, 0);
                    out.push_str(": ");
                    value.write(out, indent + 1);
                    out.push_str(if i + 1 < fields.len() { ",\n" } else { "\n" });
                }
                pad(out, indent);
                out.push('}');
            }
        }
    }
    fn text(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, 0);
        out
    }
    fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Self::Obj(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    fn is_empty(&self) -> bool {
        matches!(self, Self::Arr(v) if v.is_empty()) || matches!(self, Self::Obj(v) if v.is_empty())
    }
}

/// `mine` merged into `base`: objects by key, arrays appended, any other value already there kept.
fn merge(base: &mut Json, mine: Json) {
    match (base, mine) {
        (Json::Obj(have), Json::Obj(add)) => {
            for (key, value) in add {
                match have.iter_mut().find(|(k, _)| *k == key) {
                    Some((_, existing)) => merge(existing, value),
                    None => have.push((key, value)),
                }
            }
        }
        (Json::Arr(have), Json::Arr(add)) => have.extend(add),
        _ => {}
    }
}

/// The key ROQA's own entry has in a file keyed by hook name (Antigravity).
const NAMED: &str = "roqa-agent-status";

/// A hook line is ROQA's when its command runs a `roqa` program with `agent <id>`; the program may have moved.
fn ours(item: &Json, id: &str) -> bool {
    matches!(item.get("command"), Some(Json::Str(c))
        if c.to_ascii_lowercase().contains("roqa") && c.contains(&format!(" agent {id} ")))
}

/// Removes ROQA's lines of `id` and the groups, lists and objects that only they filled. Returns whether
/// anything went.
fn strip(value: &mut Json, id: &str) -> bool {
    match value {
        Json::Arr(items) => {
            let mut changed = false;
            items.retain_mut(|item| {
                if ours(item, id) {
                    changed = true;
                    return false;
                }
                let had_hooks = item.get("hooks").is_some_and(|h| !h.is_empty());
                let went = strip(item, id);
                let emptied = went && had_hooks && item.get("hooks").is_none();
                changed |= went;
                !emptied
            });
            changed
        }
        Json::Obj(fields) => {
            let mut changed = false;
            fields.retain_mut(|(key, inner)| {
                if key == NAMED {
                    changed = true;
                    return false;
                }
                let went = strip(inner, id);
                changed |= went;
                !(went && inner.is_empty())
            });
            changed
        }
        _ => false,
    }
}

/// What ROQA reports for an agent: the event it hooks and the state it names.
type Events = &'static [(&'static str, State)];
const W: State = State::Working;
const N: State = State::Needs;
const D: State = State::Done;

#[derive(Clone, Copy)]
enum Kind {
    /// `{"hooks": {Event: [{"hooks": [{"type":"command","command":…}]}]}}` in a file shared with other tools:
    /// Claude Code, Codex and Gemini CLI. `named` adds the hook's name; `timeout` is in the agent's own unit.
    Groups {
        events: Events,
        named: bool,
        timeout: u32,
    },
    /// Antigravity's `hooks.json`: one entry per hook name, `{Event: [{"type":"command","command":…}]}`.
    Named(Events),
    /// Cursor's `hooks.json`: `{"version":1,"hooks":{Event:[{"command":…}]}}`.
    Flat(Events),
    /// Copilot reads every file in its hooks folder; ROQA owns one, run without a shell.
    Copilot(Events),
    /// opencode loads every script in its plugins folder; ROQA owns one.
    Plugin,
    /// Claude Code's `PermissionRequest` hook: the question goes to the notch (`roqa.exe agent ask claude`) and
    /// waits there for Allow or Deny. Its own id, `ask`, keeps it apart from the status lines.
    Ask,
}

pub struct Spec {
    pub id: &'static str,
    pub label: &'static str,
    /// Folder whose presence means the agent is installed, and the hook file, both under the user's home.
    present: &'static str,
    file: &'static str,
    kind: Kind,
}

/// The agents ROQA can wire, each from its documented mechanism:
/// Claude Code `hooks` in settings.json; Codex `hooks.json` (the user trusts new hooks once in `/hooks`);
/// Gemini CLI `hooks` in settings.json; Antigravity CLI `hooks.json`; Cursor `hooks.json`; GitHub Copilot CLI
/// `~/.copilot/hooks/*.json`; opencode plugin file.
pub static AGENTS: [Spec; 8] = [
    Spec {
        id: "claude",
        label: "Claude Code",
        present: ".claude",
        file: ".claude/settings.json",
        kind: Kind::Groups {
            events: &[("UserPromptSubmit", W), ("Notification", N), ("Stop", D)],
            named: false,
            timeout: 5,
        },
    },
    Spec {
        id: "codex",
        label: "Codex",
        present: ".codex",
        file: ".codex/hooks.json",
        kind: Kind::Groups {
            events: &[
                ("UserPromptSubmit", W),
                ("PermissionRequest", N),
                ("Stop", D),
            ],
            named: false,
            timeout: 5,
        },
    },
    Spec {
        id: "gemini",
        label: "Gemini CLI",
        present: ".gemini/settings.json",
        file: ".gemini/settings.json",
        kind: Kind::Groups {
            events: &[("BeforeAgent", W), ("Notification", N), ("AfterAgent", D)],
            named: true,
            timeout: 5000,
        },
    },
    Spec {
        id: "antigravity",
        label: "Antigravity",
        present: ".gemini/antigravity-cli",
        file: ".gemini/config/hooks.json",
        kind: Kind::Named(&[("PreInvocation", W), ("Stop", D)]),
    },
    Spec {
        id: "cursor",
        label: "Cursor",
        present: ".cursor",
        file: ".cursor/hooks.json",
        kind: Kind::Flat(&[("beforeSubmitPrompt", W), ("stop", D)]),
    },
    Spec {
        id: "copilot",
        label: "Copilot CLI",
        present: ".copilot",
        file: ".copilot/hooks/roqa-agent-status.json",
        kind: Kind::Copilot(&[
            ("userPromptSubmitted", W),
            ("notification", N),
            ("agentStop", D),
        ]),
    },
    Spec {
        id: "opencode",
        label: "opencode",
        present: ".config/opencode",
        file: ".config/opencode/plugins/roqa-agent-status.js",
        kind: Kind::Plugin,
    },
    Spec {
        id: "ask",
        label: "Claude Code questions",
        present: ".claude",
        file: ".claude/settings.json",
        kind: Kind::Ask,
    },
];

/// What the Settings row shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    NotInstalled,
    Ready,
    Added,
}

pub fn home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(PathBuf::from)
}

pub fn status(spec: &Spec, home: &Path) -> Status {
    if !home.join(spec.present).exists() {
        return Status::NotInstalled;
    }
    let path = home.join(spec.file);
    let added = match spec.kind {
        Kind::Copilot(_) | Kind::Plugin => path.exists(),
        _ => read(&path)
            .and_then(|text| Json::parse(&text))
            .is_some_and(|mut json| strip(&mut json, spec.id)),
    };
    if added { Status::Added } else { Status::Ready }
}

fn read(path: &Path) -> Option<String> {
    let capped = fs::metadata(path).ok()?.len() <= 1 << 20;
    capped.then(|| fs::read_to_string(path).ok()).flatten()
}

/// The program as the agent's shell will run it: short, forward slashes and no characters a shell reads.
fn shell_path(exe: &Path) -> Result<String, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{Win32::Storage::FileSystem::GetShortPathNameW, core::PCWSTR};
    let wide: Vec<u16> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut short = [0u16; 520];
    // SAFETY: `wide` is NUL-terminated and `short` is a writable buffer the call fills.
    let len = unsafe { GetShortPathNameW(PCWSTR(wide.as_ptr()), Some(&mut short)) } as usize;
    let text = if len == 0 || len >= short.len() {
        exe.to_string_lossy().into_owned()
    } else {
        String::from_utf16_lossy(&short[..len])
    };
    let text = text.trim_start_matches(r"\\?\").replace('\\', "/");
    if text
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | ':' | '.' | '_' | '-' | '~'))
    {
        Ok(text)
    } else {
        Err(
            "ROQA sits in a folder name the hook cannot use. Move roqa.exe to a plain folder."
                .into(),
        )
    }
}

fn command(exe: &str, id: &str, state: State) -> String {
    format!("{exe} agent {id} {}", state.word())
}

/// The lines ROQA adds, for the agent whose program sits at `exe`.
fn mine(spec: &Spec, exe: &Path) -> Result<Json, String> {
    let event = |name: &str, entry: Json| (name.to_owned(), Json::Arr(vec![entry]));
    Ok(match spec.kind {
        Kind::Groups {
            events,
            named,
            timeout,
        } => {
            let exe = shell_path(exe)?;
            let hooks = events.iter().map(|&(name, state)| {
                let mut hook = vec![
                    ("type".to_owned(), string("command")),
                    (
                        "command".to_owned(),
                        Json::Str(command(&exe, spec.id, state)),
                    ),
                    ("timeout".to_owned(), number(timeout)),
                ];
                if named {
                    hook.insert(0, ("name".to_owned(), string(NAMED)));
                }
                event(name, obj([("hooks", Json::Arr(vec![Json::Obj(hook)]))]))
            });
            obj([("hooks", Json::Obj(hooks.collect()))])
        }
        Kind::Named(events) => {
            let exe = shell_path(exe)?;
            let hooks = events.iter().map(|&(name, state)| {
                event(
                    name,
                    obj([
                        ("type", string("command")),
                        ("command", Json::Str(command(&exe, spec.id, state))),
                    ]),
                )
            });
            obj([(NAMED, Json::Obj(hooks.collect()))])
        }
        Kind::Flat(events) => {
            let exe = shell_path(exe)?;
            let hooks = events.iter().map(|&(name, state)| {
                event(
                    name,
                    obj([("command", Json::Str(command(&exe, spec.id, state)))]),
                )
            });
            obj([
                ("version", number(1)),
                ("hooks", Json::Obj(hooks.collect())),
            ])
        }
        Kind::Copilot(events) => {
            let program = exe.to_string_lossy().trim_start_matches(r"\\?\").to_owned();
            let hooks = events.iter().map(|&(name, state)| {
                let mut hook = vec![
                    ("type", string("command")),
                    ("exec", Json::Str(program.clone())),
                    (
                        "args",
                        Json::Arr(["agent", spec.id, state.word()].map(string).to_vec()),
                    ),
                    ("timeoutSec", number(5)),
                ];
                if state == State::Needs {
                    hook.insert(
                        1,
                        ("matcher", string("permission_prompt|elicitation_dialog")),
                    );
                }
                event(
                    name,
                    Json::Obj(hook.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()),
                )
            });
            obj([
                ("version", number(1)),
                ("hooks", Json::Obj(hooks.collect())),
            ])
        }
        Kind::Plugin => Json::Str(plugin(&exe.to_string_lossy())),
        Kind::Ask => {
            let hook = obj([
                ("type", string("command")),
                (
                    "command",
                    Json::Str(format!("{} agent ask claude", shell_path(exe)?)),
                ),
                // A little over the 30 seconds ROQA waits for a click.
                ("timeout", number(40)),
            ]);
            let group = obj([("hooks", Json::Arr(vec![hook]))]);
            obj([(
                "hooks",
                obj([("PermissionRequest", Json::Arr(vec![group]))]),
            )])
        }
    })
}

/// The longest question line sent, in characters: a longer one loses its middle, so its start and end stay.
const LONGEST: usize = 400;

/// The words the notch shows for a permission question, from the hook's JSON on stdin: the tool and its
/// command, path or address, with its spaces squeezed. `None` when it is not a tool question.
pub fn permission_line(input: &str) -> Option<String> {
    let json = Json::parse(input)?;
    let Some(Json::Str(tool)) = json.get("tool_name") else {
        return None;
    };
    let detail = json.get("tool_input").and_then(|input| {
        ["command", "file_path", "url", "pattern", "path", "query"]
            .into_iter()
            .find_map(|key| match input.get(key) {
                Some(Json::Str(text)) => Some(text.as_str()),
                _ => None,
            })
    });
    let line = detail.map_or_else(|| tool.clone(), |detail| format!("{tool}: {detail}"));
    let line: Vec<_> = line.split_whitespace().collect();
    let line: Vec<char> = line.join(" ").chars().collect();
    Some(if line.len() > LONGEST {
        let half = LONGEST / 2;
        line[..half]
            .iter()
            .chain(&['\u{2026}'])
            .chain(&line[line.len() - half..])
            .collect()
    } else {
        line.into_iter().collect()
    })
}

/// opencode's plugin: it names the program once and reports busy, idle and permission events.
fn plugin(exe: &str) -> String {
    let program = Json::Str(exe.trim_start_matches(r"\\?\").to_owned()).text();
    format!(
        r#"// Added by ROQA: tells the dock when opencode works, needs you or is done. Undo it in ROQA's settings.
import {{ spawn }} from "node:child_process";
const roqa = {program};
const tell = (state) => {{
  try {{
    spawn(roqa, ["agent", "opencode", state], {{ stdio: "ignore", windowsHide: true }}).on("error", () => {{}}).unref();
  }} catch {{}}
}};
const busy = new Set();
export const RoqaAgentStatus = async () => ({{
  event: async ({{ event }}) => {{
    const id = event.properties?.sessionID;
    if (event.type === "session.status" && event.properties?.status?.type === "busy") {{
      if (!busy.has(id)) {{ busy.add(id); tell("working"); }}
    }} else if (event.type === "session.idle") {{
      busy.delete(id);
      tell("done");
    }} else if (event.type === "permission.updated" || event.type === "permission.asked") {{
      tell("needs");
    }}
  }},
}});
"#
    )
}

/// A change the user has been shown and may confirm.
pub struct Plan {
    pub path: PathBuf,
    /// What the dialog shows: the exact lines added, or what goes.
    pub shown: String,
    /// The file afterwards; `None` deletes it.
    after: Option<String>,
}

impl Plan {
    /// Whether `other` (planned later) would write the same file text: nothing changed while the user decided.
    pub fn same_as(&self, other: &Plan) -> bool {
        self.path == other.path && self.after == other.after
    }
}

/// The change for adding (`add`) or undoing the hook of `spec`. Nothing is written.
pub fn plan(spec: &Spec, home: &Path, exe: &Path, add: bool) -> Result<Plan, String> {
    let path = home.join(spec.file);
    let own_file = matches!(spec.kind, Kind::Copilot(_) | Kind::Plugin);
    if !add && own_file {
        return Ok(Plan {
            shown: format!("ROQA deletes its own file {}.", path.display()),
            path,
            after: None,
        });
    }
    let mine = mine(spec, exe)?;
    if add && own_file {
        let shown = match &mine {
            Json::Str(script) => script.clone(),
            json => json.text(),
        };
        return Ok(Plan {
            after: Some(shown.clone()),
            shown,
            path,
        });
    }
    let unreadable = || {
        format!(
            "{} is not plain JSON, so ROQA left it alone.",
            path.display()
        )
    };
    let mut json = match fs::metadata(&path) {
        Ok(_) => read(&path)
            .and_then(|text| Json::parse(&text))
            .filter(|json| matches!(json, Json::Obj(_)))
            .ok_or_else(unreadable)?,
        Err(_) => Json::Obj(Vec::new()),
    };
    let found = strip(&mut json, spec.id);
    if !add {
        if !found {
            return Err("ROQA has no hook there.".into());
        }
        return Ok(Plan {
            shown: format!("ROQA removes only its own lines from {}.", path.display()),
            after: Some(format!("{}\n", json.text())),
            path,
        });
    }
    let shown = mine.text();
    merge(&mut json, mine);
    Ok(Plan {
        after: Some(format!("{}\n", json.text())),
        shown,
        path,
    })
}

/// Writes a confirmed plan: first a timestamped copy of the file, then the new text or its removal.
pub fn apply(plan: &Plan, stamp: &str) -> Result<(), String> {
    let path = &plan.path;
    let failed = |e: std::io::Error| format!("Could not write {}: {e}", path.display());
    if path.exists() {
        let mut backup = path.as_os_str().to_owned();
        backup.push(format!(".roqa-backup-{stamp}"));
        // A second change in the same second keeps the first copy: it holds the untouched original.
        if !Path::new(&backup).exists() {
            fs::copy(path, backup).map_err(failed)?;
        }
    }
    match &plan.after {
        None => fs::remove_file(path).map_err(failed),
        Some(text) => {
            // A linked file (a dotfiles repo) is written through: the new file replaces the real one, not the link.
            let real = if fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink())
            {
                fs::canonicalize(path).unwrap_or_else(|_| path.clone())
            } else {
                path.clone()
            };
            if let Some(folder) = real.parent() {
                fs::create_dir_all(folder).map_err(failed)?;
            }
            let mut temp = real.as_os_str().to_owned();
            temp.push(".roqa-tmp");
            fs::write(&temp, text).map_err(failed)?;
            fs::rename(&temp, &real).map_err(failed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXE: &str = r"C:\roqa\roqa.exe";

    fn spec(id: &str) -> &'static Spec {
        AGENTS.iter().find(|s| s.id == id).unwrap()
    }
    /// A scratch home with `files` written under it; removed by the caller.
    fn home(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let home = std::env::temp_dir().join(format!("roqa-hooks-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        for (file, text) in files {
            let path = home.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        home
    }
    fn add(id: &str, home: &Path) {
        let plan = plan(spec(id), home, Path::new(EXE), true).unwrap();
        apply(&plan, "20260101-000000").unwrap();
    }
    fn undo(id: &str, home: &Path) {
        let plan = plan(spec(id), home, Path::new(EXE), false).unwrap();
        apply(&plan, "20260101-000001").unwrap();
    }

    #[test]
    fn a_change_made_while_the_dialog_was_open_is_seen_and_a_second_backup_in_a_second_keeps_the_first()
     {
        let home = home("race", &[(".claude/settings.json", CLAUDE)]);
        let file = home.join(".claude/settings.json");
        let (claude, exe) = (spec("claude"), Path::new(EXE));
        let seen = plan(claude, &home, exe, true).unwrap();
        assert!(seen.same_as(&plan(claude, &home, exe, true).unwrap()));
        fs::write(&file, r#"{"model":"other"}"#).unwrap();
        assert!(!seen.same_as(&plan(claude, &home, exe, true).unwrap()));
        // Two applies with one stamp: the backup still holds what the first one found.
        let first = plan(claude, &home, exe, true).unwrap();
        apply(&first, "20260101-000000").unwrap();
        undo("claude", &home);
        let backup = home.join(".claude/settings.json.roqa-backup-20260101-000000");
        assert_eq!(fs::read_to_string(backup).unwrap(), r#"{"model":"other"}"#);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn a_linked_settings_file_stays_a_link() {
        let home = home("link", &[("dotfiles/settings.json", CLAUDE)]);
        let link = home.join(".claude/settings.json");
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        // Creating a link needs a privilege or developer mode: without it there is nothing to check.
        if std::os::windows::fs::symlink_file(home.join("dotfiles/settings.json"), &link).is_ok() {
            add("ask", &home);
            assert!(
                fs::symlink_metadata(&link)
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
            assert!(
                fs::read_to_string(home.join("dotfiles/settings.json"))
                    .unwrap()
                    .contains("agent ask claude")
            );
        }
        let _ = fs::remove_dir_all(&home);
    }

    const CLAUDE: &str = r#"{"model":"x","hooks":{"Stop":[{"hooks":[{"type":"command","command":"node a.cjs stop","timeout":10}],"_src":"é\u00e9"}],"PostToolUse":[]}}"#;

    #[test]
    fn the_question_hook_adds_beside_the_status_lines_and_undoes_alone() {
        let home = home("ask", &[(".claude/settings.json", CLAUDE)]);
        add("claude", &home);
        add("ask", &home);
        let file = home.join(".claude/settings.json");
        let text = fs::read_to_string(&file).unwrap();
        assert!(text.contains("PermissionRequest") && text.contains("agent ask claude"));
        assert!(text.contains("node a.cjs stop"), "their hook stays");
        assert_eq!(status(spec("ask"), &home), Status::Added);
        undo("ask", &home);
        assert_eq!(status(spec("ask"), &home), Status::Ready);
        assert_eq!(
            status(spec("claude"), &home),
            Status::Added,
            "status lines stay"
        );
        assert!(
            !fs::read_to_string(&file)
                .unwrap()
                .contains("PermissionRequest")
        );
        undo("claude", &home);
        let mut original = Json::parse(CLAUDE).unwrap();
        assert_eq!(
            Json::parse(&fs::read_to_string(&file).unwrap()).unwrap(),
            original
        );
        assert!(!strip(&mut original, "ask"));
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn the_question_line_names_the_tool_and_what_it_touches() {
        let line = |text: &str| permission_line(text);
        assert_eq!(
            line(r#"{"tool_name":"Bash","tool_input":{"command":"git  push\n--force"}}"#)
                .as_deref(),
            Some("Bash: git push --force")
        );
        assert_eq!(
            line(r#"{"tool_name":"Edit","tool_input":{"file_path":"C:\\a.txt"}}"#).as_deref(),
            Some("Edit: C:\\a.txt")
        );
        assert_eq!(
            line(r#"{"tool_name":"Task","tool_input":{}}"#).as_deref(),
            Some("Task")
        );
        let long = format!(
            r#"{{"tool_name":"Bash","tool_input":{{"command":"{}end"}}}}"#,
            "a ".repeat(400)
        );
        let kept = line(&long).unwrap();
        assert!(kept.starts_with("Bash: a a") && kept.ends_with("a end"));
        assert_eq!(
            kept.chars().count(),
            LONGEST + 1,
            "the ellipsis joins the halves"
        );
        assert_eq!(line("not json"), None);
        assert_eq!(line(r#"{"hook_event_name":"Stop"}"#), None);
    }

    #[test]
    fn json_round_trips_and_rejects_bad_text() {
        let json = Json::parse(CLAUDE).unwrap();
        assert_eq!(Json::parse(&json.text()), Some(json));
        for bad in [
            "",
            "{",
            r#"{"a":1,}"#,
            r#"{"a" 1}"#,
            "[1] x",
            r#""\x""#,
            "nul",
        ] {
            assert!(Json::parse(bad).is_none(), "{bad}");
        }
        assert_eq!(
            Json::parse(r#""\ud83d\ude00""#),
            Some(Json::Str("😀".into()))
        );
    }

    #[test]
    fn adding_merges_and_undo_restores_the_other_hooks() {
        let home = home("claude", &[(".claude/settings.json", CLAUDE)]);
        add("claude", &home);
        let file = home.join(".claude/settings.json");
        let added = Json::parse(&fs::read_to_string(&file).unwrap()).unwrap();
        let stop = added.get("hooks").unwrap().get("Stop").unwrap();
        assert!(
            matches!(stop, Json::Arr(items) if items.len() == 2),
            "kept theirs, added ours"
        );
        assert_eq!(status(spec("claude"), &home), Status::Added);
        assert!(
            home.join(".claude/settings.json.roqa-backup-20260101-000000")
                .exists()
        );
        add("claude", &home); // again: still one set of lines
        let again = Json::parse(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(again, added);
        undo("claude", &home);
        let undone = Json::parse(&fs::read_to_string(&file).unwrap()).unwrap();
        // Their hook stays; the lists ROQA alone filled are gone, theirs (even an empty one) are not.
        let mut original = Json::parse(CLAUDE).unwrap();
        assert_eq!(undone, original.clone());
        assert!(!strip(&mut original, "claude"));
        assert_eq!(status(spec("claude"), &home), Status::Ready);
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn each_shared_file_kind_adds_and_undoes() {
        for (id, file, start) in [
            (
                "codex",
                ".codex/hooks.json",
                r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"x"}]}]}}"#,
            ),
            (
                "gemini",
                ".gemini/settings.json",
                r#"{"ui":{"theme":"Dracula"}}"#,
            ),
            (
                "antigravity",
                ".gemini/config/hooks.json",
                r#"{"mine":{"Stop":[{"command":"y"}]}}"#,
            ),
            (
                "cursor",
                ".cursor/hooks.json",
                r#"{"version":1,"hooks":{"stop":[{"command":"z"}]}}"#,
            ),
        ] {
            let home = home(
                id,
                &[
                    (".gemini/antigravity-cli/x", ""),
                    (".gemini/settings.json", "{}"),
                    (file, start),
                    (".codex/x", ""),
                    (".cursor/x", ""),
                ],
            );
            fs::write(home.join(file), start).unwrap();
            assert_eq!(status(spec(id), &home), Status::Ready, "{id}");
            add(id, &home);
            assert_eq!(status(spec(id), &home), Status::Added, "{id}");
            let text = fs::read_to_string(home.join(file)).unwrap();
            assert!(
                text.contains(&format!("agent {id} working"))
                    && text.contains(&format!("agent {id} done")),
                "{id}"
            );
            undo(id, &home);
            assert_eq!(
                Json::parse(&fs::read_to_string(home.join(file)).unwrap()),
                Json::parse(start),
                "{id}"
            );
            let _ = fs::remove_dir_all(home);
        }
    }

    #[test]
    fn own_files_are_created_and_deleted() {
        for id in ["copilot", "opencode"] {
            let home = home(id, &[(".copilot/x", ""), (".config/opencode/x", "")]);
            add(id, &home);
            assert_eq!(status(spec(id), &home), Status::Added);
            let text = fs::read_to_string(home.join(spec(id).file)).unwrap();
            assert!(text.contains(r"C:\\roqa\\roqa.exe") || text.contains(r"C:\roqa\roqa.exe"));
            undo(id, &home);
            assert!(!home.join(spec(id).file).exists());
            let _ = fs::remove_dir_all(home);
        }
    }

    #[test]
    fn an_unreadable_file_or_a_missing_agent_is_left_alone() {
        let home = home("bad", &[(".claude/settings.json", "{ // comment\n}")]);
        assert!(plan(spec("claude"), &home, Path::new(EXE), true).is_err());
        assert_eq!(status(spec("cursor"), &home), Status::NotInstalled);
        assert!(plan(spec("claude"), &home, Path::new(EXE), false).is_err());
        let _ = fs::remove_dir_all(home);
        let spaced = plan(
            spec("claude"),
            Path::new("."),
            Path::new(r"C:\no such dir\roqa.exe"),
            true,
        );
        assert!(
            spaced.is_err(),
            "a program path with spaces is refused for shell hooks"
        );
    }
}
