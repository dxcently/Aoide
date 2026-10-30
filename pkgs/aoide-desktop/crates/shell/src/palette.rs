//! The palette: the commands a window offers, from `aoide schema --json`.
//! A command is offered when the window's grant set names it and the floor does
//! not hold it. The floor is re-checked at every call, so a grant that names a
//! floored command still refuses.

use serde::Serialize;

use crate::exec::untrusted::visible;
use crate::exec::{CommandSpec, Exec, ExecError, Flag, Output, Schema};
use crate::blocking;

/// The main window's grant set: every path the schema declares. The main window is
/// the person's own, so its palette is the schema less the floor. A frame passes a
/// narrower set of its own (V21 section 4.3).
pub fn main_grants(schema: &Schema) -> Vec<Vec<&str>> {
    schema.commands().map(|(path, _)| path.iter().map(String::as_str).collect()).collect()
}

/// The classes the floor refuses by name, each as the path words it starts with
/// (V21 section 4.4). A path that starts with one is refused, granted or not.
pub const FLOOR: &[(&str, &[&str])] = &[
    ("custody", &["secrets"]),
    ("custody", &["secret"]),
    ("trust", &["pair"]),
    ("trust", &["mesh"]),
    ("trust", &["node", "allow"]),
    ("trust", &["node", "add"]),
    ("trust", &["node", "remove"]),
    ("trust", &["node", "address"]),
    ("trust", &["node", "hub"]),
    ("trust", &["identity"]),
    ("trust", &["session", "grant"]),
    ("trust", &["extension", "grant"]),
    ("self-change", &["config", "set"]),
    ("self-change", &["hooks", "install"]),
    ("self-change", &["update"]),
    ("self-change", &["daemon"]),
    ("self-change", &["node", "advertise"]),
    ("self-change", &["extension", "add"]),
    ("self-change", &["extension", "remove"]),
    ("melete tools", &["melete", "call"]),
    ("mneme writes", &["mneme", "call"]),
];

/// Why a call from the palette is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The path never ends on its own (`blocking::PATHS`).
    Blocking(String),
    /// The schema declares no command at this path.
    Unknown(String),
    /// The floor holds the command, granted or not.
    Floored { path: String, why: &'static str },
    /// The main window is not granted the command.
    NotGranted(String),
}

/// A call refused by the palette, or one that ran and failed.
#[derive(Debug)]
pub enum CallError {
    Refused(Refusal),
    Exec(ExecError),
}

/// One offered command, every string spelled.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Row {
    pub path: Vec<String>,
    pub brief: String,
    pub args: Vec<ArgRow>,
    pub flags: Vec<FlagRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ArgRow {
    pub name: String,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FlagRow {
    pub name: String,
    pub kind: String,
}

/// The offered commands: each command that `check` allows, in path order.
pub fn palette(schema: &Schema, grants: &[Vec<&str>]) -> Vec<Row> {
    schema
        .commands()
        .filter(|(path, _)| {
            let words: Vec<&str> = path.iter().map(String::as_str).collect();
            check(schema, grants, &words, &[]).is_ok()
        })
        .map(|(path, spec)| row(path, spec))
        .collect()
}

fn row(path: &[String], spec: &CommandSpec) -> Row {
    Row {
        path: path.iter().map(|w| visible(w)).collect(),
        brief: visible(&spec.brief),
        args: spec.args.iter().map(|a| ArgRow { name: visible(&a.name), required: a.required }).collect(),
        flags: spec
            .flags
            .iter()
            .map(|(name, kind)| FlagRow { name: visible(name), kind: visible(kind) })
            .collect(),
    }
}

/// Whether the main window may run `words` with `flags`. Each refusal is checked
/// in this order: a blocking path, a `--node` or `--typed` flag, an undeclared
/// command, the floor, and last the grant.
pub fn check(schema: &Schema, grants: &[Vec<&str>], words: &[&str], flags: &[Flag<'_>]) -> Result<(), Refusal> {
    let path = words.join(" ");
    if blocking::is_blocking(words) {
        return Err(Refusal::Blocking(path));
    }
    if flags.iter().any(|f| f.name == "node" || f.name == "typed") {
        return Err(Refusal::Floored { path, why: "--node and --typed never run from the palette" });
    }
    let spec = schema.command(words).ok_or_else(|| Refusal::Unknown(path.clone()))?;
    if let Some(why) = floor_reason(words, spec) {
        return Err(Refusal::Floored { path, why });
    }
    if grants.iter().any(|granted| granted.as_slice() == words) {
        Ok(())
    } else {
        Err(Refusal::NotGranted(path))
    }
}

/// The floor's reason for `words`, if it holds the command. The schema's marks
/// come first, then the named classes.
fn floor_reason(words: &[&str], spec: &CommandSpec) -> Option<&'static str> {
    if spec.approves {
        return Some("approves a hold or types into a session");
    }
    if spec.launches {
        return Some("starts an agent or a program");
    }
    if spec.internal {
        return Some("is hook plumbing, not an operator command");
    }
    if !spec.implemented {
        return Some("is a stub");
    }
    FLOOR.iter().find(|(_, prefix)| words.starts_with(prefix)).map(|&(class, _)| class)
}

/// Runs `words` through the palette: `check` first, then `Exec::call`. A refused
/// call starts no child.
pub fn call(
    exec: &Exec,
    grants: &[Vec<&str>],
    words: &[&str],
    flags: &[Flag<'_>],
    positionals: &[&str],
) -> Result<Output, CallError> {
    check(exec.schema(), grants, words, flags).map_err(CallError::Refused)?;
    exec.call(words, flags, positionals, None).map_err(CallError::Exec)
}
