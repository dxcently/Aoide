//! Structured output envelopes and exit codes.
//!
//! Every command returns an [`Outcome`]; the dispatcher renders it as either
//! JSON (`--json`) or a human line, and maps its status to a process exit code.
//! This is the "every command emits `--json`, structured errors, meaningful
//! exit codes, reports exactly what changed" contract (CONTRACTS.md §3).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io;
use std::path::Path;

/// Canonical exit codes (must match `schema.rs` `exit_codes`).
///
/// The rule (CONTRACTS.md §3): `USAGE` (2) means the invocation can never be
/// valid whatever the world looks like — a missing, unknown or ill-typed
/// argument, flag or value, or a command typed at the wrong binary. `ERROR`
/// (1) means a valid invocation the world refused — not found, unreachable,
/// locked, not paired. `NOT_IMPLEMENTED` (64) is a stub. [`Kind`] carries the
/// split: `Kind::Usage` exits 2, `Kind::Refused`/`Kind::Failed` exit 1.
pub mod exit {
    pub const OK: i32 = 0;
    pub const ERROR: i32 = 1;
    pub const USAGE: i32 = 2;
    pub const NOT_IMPLEMENTED: i32 = 64;
}

/// Coarse status of a command, one-to-one with an exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Ok,
    Error,
    Usage,
    NotImplemented,
}

impl Status {
    pub fn exit_code(self) -> i32 {
        match self {
            Status::Ok => exit::OK,
            Status::Error => exit::ERROR,
            Status::Usage => exit::USAGE,
            Status::NotImplemented => exit::NOT_IMPLEMENTED,
        }
    }
}

/// Why a command said no — which side of the exit rule it falls on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// The invocation can never be valid (exit 2).
    Usage,
    /// A valid invocation the world refused: not found, locked, not paired (exit 1).
    Refused,
    /// A valid invocation that broke part-way (exit 1).
    Failed,
}

impl Kind {
    fn status(self) -> Status {
        match self {
            Kind::Usage => Status::Usage,
            Kind::Refused | Kind::Failed => Status::Error,
        }
    }
}

/// What the person does next. A refusal always names one: a command to run,
/// a setting to change, a wait, or the reason there is nothing to do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Fix {
    Run(String),
    Set(String),
    Wait(String),
    None(&'static str),
}

/// A taught error: what was refused, why, and the fix
/// (`pkgs/aoide/crates/AGENTS.md`, "Taught errors").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub kind: Kind,
    pub what: String,
    pub why: String,
    pub fix: Fix,
}

impl Refusal {
    pub fn new(kind: Kind, what: impl Into<String>, why: impl Into<String>, fix: Fix) -> Self {
        Refusal { kind, what: what.into(), why: why.into(), fix }
    }

    pub fn into_outcome(self, command: impl Into<String>) -> Outcome {
        Outcome::refuse(command, self.kind, self.what, self.why, self.fix)
    }
}

/// An I/O or parse failure in words, split into the part a person reads and
/// the raw text for logs (`os error N`, serde internals).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cause {
    pub why: String,
    pub detail: String,
}

/// Put an [`io::Error`] on `path` into words. `op` is the verb that failed
/// (`read`, `write`, `create`); the OS error number stays in `detail`.
pub fn io_cause(op: &str, path: &Path, e: &io::Error) -> Cause {
    let p = path.display();
    let why = match e.kind() {
        io::ErrorKind::NotFound => format!("`{p}` was not found"),
        io::ErrorKind::PermissionDenied => format!("permission denied: cannot {op} `{p}`"),
        io::ErrorKind::AlreadyExists => format!("`{p}` already exists"),
        io::ErrorKind::InvalidData => format!("`{p}` holds data this version cannot read"),
        kind => {
            let words = kind.to_string();
            if words.contains("uncategorized") || words.contains("other error") {
                format!("cannot {op} `{p}`: the operating system refused")
            } else {
                format!("cannot {op} `{p}`: {words}")
            }
        }
    };
    Cause { why, detail: format!("{op} {p}: {e}") }
}

/// Put a [`serde_json::Error`] from reading `path` into words; the position
/// stays in the words, serde's own text in `detail`.
pub fn serde_cause(path: &Path, e: &serde_json::Error) -> Cause {
    let p = path.display();
    let at = format!("line {}, column {}", e.line(), e.column());
    let why = match e.classify() {
        serde_json::error::Category::Io => format!("`{p}` could not be read"),
        serde_json::error::Category::Syntax => format!("`{p}` is not valid JSON ({at})"),
        serde_json::error::Category::Data => format!("`{p}` has the wrong shape ({at})"),
        serde_json::error::Category::Eof => format!("`{p}` ends early; it may be truncated"),
    };
    Cause { why, detail: format!("{p}: {e}") }
}

/// The structured result of running one command.
///
/// `Deserialize` (P-D6, `docs/architecture/AOIDED.md`'s "L4 — graph
/// residency"): `aoide_client::daemon::daemon_dispatch` reconstructs the
/// `Outcome` a routed handler produced on the OTHER side of the daemon
/// socket from the wire's `{"outcome": <this shape>}` reply — the exact
/// same shape [`Outcome::render`]'s `--json` branch already serializes,
/// round-tripped losslessly since every field here is already `pub`. Purely
/// additive: no field, tag, or rename changed, so every existing
/// `--json` consumer outside this repo is unaffected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Outcome {
    pub status: Status,
    /// The command path that produced this, e.g. `rice.declare`.
    pub command: String,
    /// Human-readable summary line.
    pub message: String,
    /// Whether this operation would route through the user rebuild gate.
    pub gated: bool,
    /// Exactly what changed (empty when nothing changed — idempotency signal).
    /// `default` alongside `skip_serializing_if` (P-D6): the field is OMITTED
    /// when empty on the way out, so a round trip through `Deserialize` needs
    /// the matching default to reconstruct that same empty `Vec` back rather
    /// than erroring on a "missing field."
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changed: Vec<String>,
    /// Free-form structured payload (schema output, audit records, etc.).
    /// `default` alongside `skip_serializing_if`, same P-D6 round-trip reason
    /// as `changed` above.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl Outcome {
    pub fn new(command: impl Into<String>, status: Status, message: impl Into<String>) -> Self {
        Outcome {
            status,
            command: command.into(),
            message: message.into(),
            gated: false,
            changed: Vec::new(),
            data: None,
        }
    }

    pub fn ok(command: impl Into<String>, message: impl Into<String>) -> Self {
        Outcome::new(command, Status::Ok, message)
    }

    pub fn error(command: impl Into<String>, message: impl Into<String>) -> Self {
        Outcome::new(command, Status::Error, message)
    }

    pub fn usage(command: impl Into<String>, message: impl Into<String>) -> Self {
        Outcome::new(command, Status::Usage, message)
    }

    /// A taught refusal: `what` is the message, `data.refusal` carries
    /// `{kind, what, why, fix}`, and the text render prints `why`/`fix`.
    pub fn refuse(
        command: impl Into<String>,
        kind: Kind,
        what: impl Into<String>,
        why: impl Into<String>,
        fix: Fix,
    ) -> Self {
        let what = what.into();
        let refusal = json!({ "kind": kind, "what": what, "why": why.into(), "fix": fix });
        Outcome::new(command, kind.status(), what).with_data(json!({ "refusal": refusal }))
    }

    /// Attach raw text (OS or serde wording) for logs and `--json`; the text
    /// render never prints it.
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        let mut obj = match self.data.take() {
            Some(Value::Object(m)) => m,
            _ => serde_json::Map::new(),
        };
        obj.insert("detail".into(), Value::String(detail.into()));
        self.data = Some(Value::Object(obj));
        self
    }

    /// The structured "not-implemented" stub every mutating skeleton returns.
    pub fn not_implemented(command: impl Into<String>, gated: bool) -> Self {
        let cmd = command.into();
        Outcome {
            status: Status::NotImplemented,
            message: format!(
                "`aoide {}` is a walking-skeleton stub: arg-parsing and schema are real, \
                 the live-system action is not yet implemented.",
                cmd.replace('.', " ")
            ),
            gated,
            changed: Vec::new(),
            data: None,
            command: cmd,
        }
    }

    pub fn gated(mut self, gated: bool) -> Self {
        self.gated = gated;
        self
    }

    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }

    pub fn changed(mut self, items: impl IntoIterator<Item = String>) -> Self {
        self.changed = items.into_iter().collect();
        self
    }

    /// Render + exit-code, honouring `--json`.
    pub fn render(&self, json: bool) -> (String, i32) {
        let code = self.status.exit_code();
        if json {
            let body = serde_json::to_string_pretty(self)
                .unwrap_or_else(|e| format!("{{\"status\":\"error\",\"message\":\"{e}\"}}"));
            (body, code)
        } else {
            let mut line = format!("[{}] {}: {}", tag(self.status), self.command, self.message);
            line.push_str(&self.refusal_lines());
            line.push_str(&self.detail_lines());
            if !self.changed.is_empty() {
                line.push_str(&format!("\n  changed: {}", self.changed.join(", ")));
            }
            if self.gated {
                line.push_str("\n  (gated: routes through the user rebuild gate)");
            }
            (line, code)
        }
    }
}

impl Outcome {
    /// `why:`/`fix:` lines read back from `data.refusal` (so a refusal that
    /// crossed the daemon socket renders the same).
    fn refusal_lines(&self) -> String {
        let Some(r) = self.data.as_ref().and_then(|d| d.get("refusal")) else {
            return String::new();
        };
        let mut out = String::new();
        if let Some(why) = r.get("why").and_then(Value::as_str) {
            out.push_str(&format!("\n  why: {why}"));
        }
        if let Some((kind, v)) = r.get("fix").and_then(Value::as_object).and_then(|m| m.iter().next()) {
            let v = v.as_str().unwrap_or_default();
            let fix = match kind.as_str() {
                "set" => format!("set {v}"),
                "wait" => format!("wait {v}"),
                "none" => format!("none: {v}"),
                _ => v.to_string(),
            };
            out.push_str(&format!("\n  fix: {fix}"));
        }
        out
    }

    /// The generic detail shape: on an error or usage outcome, every
    /// top-level `data` field that is a list of strings prints as an indented
    /// block under its key (`errors`, `problems`, …). Lists of anything else,
    /// and every `Ok` payload, stay `--json`-only.
    fn detail_lines(&self) -> String {
        if !matches!(self.status, Status::Error | Status::Usage) {
            return String::new();
        }
        let Some(Value::Object(data)) = &self.data else {
            return String::new();
        };
        let mut out = String::new();
        for (key, v) in data {
            let Some(items) = v.as_array().filter(|a| !a.is_empty()) else { continue };
            let lines: Vec<&str> = items.iter().filter_map(Value::as_str).collect();
            if lines.len() != items.len() {
                continue;
            }
            out.push_str(&format!("\n  {key}:"));
            for l in lines {
                out.push_str(&format!("\n    {l}"));
            }
        }
        out
    }
}

fn tag(s: Status) -> &'static str {
    match s {
        Status::Ok => "ok",
        Status::Error => "error",
        Status::Usage => "usage",
        Status::NotImplemented => "not-implemented",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn a_refusal_renders_what_why_fix_and_exits_by_kind() {
        let o = Outcome::refuse(
            "secrets.exec",
            Kind::Refused,
            "secret `db-prod` is not registered",
            "the broker holds no policy by that name",
            Fix::Run("aoide secrets add db-prod".into()),
        );
        let (text, code) = o.render(false);
        assert_eq!(
            text,
            "[error] secrets.exec: secret `db-prod` is not registered\n  why: the broker holds no policy by that name\n  fix: aoide secrets add db-prod"
        );
        assert_eq!(code, exit::ERROR);

        let usage = Outcome::refuse("x", Kind::Usage, "w", "y", Fix::None("nothing to do"));
        assert_eq!(usage.render(false).1, exit::USAGE);
        assert!(usage.render(false).0.ends_with("fix: none: nothing to do"));

        let set = Outcome::refuse("x", Kind::Failed, "w", "y", Fix::Set("AOIDE_ROOT".into()));
        assert!(set.render(false).0.ends_with("fix: set AOIDE_ROOT"));
        let wait = Outcome::refuse("x", Kind::Failed, "w", "y", Fix::Wait("for the far operator".into()));
        assert!(wait.render(false).0.ends_with("fix: wait for the far operator"));
    }

    #[test]
    fn json_carries_the_refusal_and_the_raw_detail_text_mode_never_prints_it() {
        let o = Outcome::refuse("x", Kind::Failed, "w", "y", Fix::Run("z".into())).with_detail("read /a: No such file (os error 2)");
        let v: Value = serde_json::from_str(&o.render(true).0).unwrap();
        assert_eq!(v["data"]["refusal"]["what"], "w");
        assert_eq!(v["data"]["refusal"]["why"], "y");
        assert_eq!(v["data"]["refusal"]["fix"]["run"], "z");
        assert_eq!(v["data"]["refusal"]["kind"], "failed");
        assert!(v["data"]["detail"].as_str().unwrap().contains("os error 2"));
        assert!(!o.render(false).0.contains("os error"));
    }

    #[test]
    fn a_refusal_survives_the_daemon_round_trip_and_renders_the_same() {
        let o = Outcome::refuse("x", Kind::Refused, "w", "y", Fix::Run("z".into()));
        let back: Outcome = serde_json::from_str(&o.render(true).0).unwrap();
        assert_eq!(back.render(false), o.render(false));
    }

    #[test]
    fn io_cause_speaks_words_and_keeps_the_os_number_in_detail() {
        let p = Path::new("/srv/a.json");
        let missing = io_cause("read", p, &io::Error::from_raw_os_error(2));
        assert_eq!(missing.why, "`/srv/a.json` was not found");
        assert!(missing.detail.contains("os error 2"));
        let denied = io_cause("write", p, &io::Error::from(io::ErrorKind::PermissionDenied));
        assert_eq!(denied.why, "permission denied: cannot write `/srv/a.json`");
        for c in [&missing, &denied, &io_cause("read", p, &io::Error::other("boom"))] {
            assert!(!c.why.contains("os error"), "{}", c.why);
        }
    }

    #[test]
    fn serde_cause_names_the_position_in_words() {
        let err = serde_json::from_str::<Value>("{\n  \"a\": ,\n}").unwrap_err();
        let c = serde_cause(Path::new("/s/p.json"), &err);
        assert_eq!(c.why, "`/s/p.json` is not valid JSON (line 2, column 8)");
        assert!(c.detail.contains("expected value"));
    }

    #[test]
    fn an_error_outcome_prints_its_lists_of_strings_and_an_ok_one_prints_none() {
        let bad = Outcome::error("rice.lint", "reported problems").with_data(json!({ "errors": ["a is wrong", "b is wrong"], "livery": "/x" }));
        assert_eq!(bad.render(false).0, "[error] rice.lint: reported problems\n  errors:\n    a is wrong\n    b is wrong");
        let good = Outcome::ok("x", "fine").with_data(json!({ "errors": ["never shown"] }));
        assert_eq!(good.render(false).0, "[ok] x: fine");
        let objects = Outcome::error("x", "m").with_data(json!({ "findings": [{ "a": 1 }] }));
        assert_eq!(objects.render(false).0, "[error] x: m");
    }
}
