//! The untracked-terminal roster from the desktop side: each terminal-class client becomes a
//! `TermWindow` (its cwd read from `/proc`), and the roster goes to core through the door. Core's
//! reconciler writes the `win:*` records. A roster goes out when its content changes, or when the
//! sessions that claim its windows change ([`RosterGate`]).

use super::door::{self, Latch};
use aoide_conduct::graph::{load_stage, normalize_addr, proc_cwd, sessions_path, SessionsFile, TermWindow};
use serde_json::Value;

/// Extract a [`TermWindow`] from one `hyprctl clients -j` client object, reading
/// its owning process's cwd from `/proc/<pid>/cwd`. `None` for a client with no
/// usable `address`. The one I/O seam over the pure [`reconcile_untracked_terminals`].
pub(crate) fn term_window_from_client(c: &Value) -> Option<TermWindow> {
    let address = c.get("address").and_then(Value::as_str)?.trim().to_string();
    if address.is_empty() {
        return None;
    }
    let class = c
        .get("class")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let title = c
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let workspace = c
        .get("workspace")
        .and_then(|w| w.get("id"))
        .and_then(Value::as_i64);
    // Checked, not `as i32`: hyprctl's JSON pid is an i64, and a value outside
    // i32's range would otherwise silently wrap into an arbitrary in-range
    // pid before it's used both for `/proc/<pid>/cwd` below and, later, a
    // widening `as u32` in `reconcile_untracked_terminals`. `try_from` simply
    // drops an out-of-range value (`None`) rather than wrapping it into a
    // bogus one.
    let pid = c
        .get("pid")
        .and_then(Value::as_i64)
        .and_then(|p| i32::try_from(p).ok())
        .filter(|p| *p > 0);
    // `mapped` defaults to true when absent — a client with no `mapped` field is
    // treated as a shown window rather than silently dropped.
    let mapped = c.get("mapped").and_then(Value::as_bool).unwrap_or(true);
    let cwd = pid.and_then(proc_cwd).unwrap_or_default();
    Some(TermWindow {
        address,
        class,
        title,
        workspace,
        pid,
        mapped,
        cwd,
    })
}

/// What the roster verb would be told this tick: the terminal windows the compositor shows, and the
/// window addresses the tracked sessions already hold. Core keeps a terminal's record while no
/// session claims its window, so a change in either makes a publish owed. A `win:*` record is core's
/// own roster entry, not a claim, so publishing it changes nothing here.
#[derive(Clone, PartialEq)]
pub(crate) struct Roster {
    windows: Vec<TermWindow>,
    claims: Vec<String>,
}

impl Roster {
    /// The roster this tick would publish, from the live clients and the stage's current claims.
    pub(crate) fn of(clients: &[Value]) -> Self {
        let windows: Vec<TermWindow> = clients.iter().filter_map(term_window_from_client).collect();
        let mut claims: Vec<String> = load_stage::<SessionsFile>(&sessions_path())
            .map(|file| {
                file.sessions
                    .iter()
                    .filter(|s| !s.session_id.starts_with("win:") && !s.window_address.is_empty())
                    .map(|s| normalize_addr(&s.window_address))
                    .collect()
            })
            .unwrap_or_default();
        claims.sort();
        claims.dedup();
        Self { windows, claims }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hypr::testutil::{session, unique_stage, EnvVars};
    use aoide_conduct::graph::{write_stage, SessionsFile};
    use serde_json::json;

    /// A `win:*` record is core's roster entry, not a claim. Its window is not held by a session, so
    /// the roster names no claims, and publishing a terminal does not make the next roster owed.
    #[test]
    fn a_win_record_holds_no_claim() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR"]);
        let stage = unique_stage("roster-win-claim");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        let mut win = session("win:aa", "", "running", "", None);
        win.window_address = "0xAA".into();
        let file = SessionsFile { schema_version: "0".into(), sessions: vec![win] };
        write_stage(&aoide_conduct::graph::sessions_path(), &file).unwrap();
        let clients = [json!({ "address": "0xAA", "class": "kitty" })];
        assert!(Roster::of(&clients).claims.is_empty());
        let _ = std::fs::remove_dir_all(&stage);
    }
}

/// Publishes the roster when it differs from the one core last answered. A roster core answers, yes or
/// no, or skips for want of a binary, is recorded as sent: it goes out again when the roster or its
/// claims change, or on a reconnect. A roster that was not sent (aoided not listening, or an exec that
/// failed) stays owed and goes out on a later tick.
#[derive(Default)]
pub(crate) struct RosterGate {
    sent: Option<Roster>,
}

impl RosterGate {
    pub(crate) fn forget(&mut self) {
        self.sent = None;
    }

    pub(crate) fn settle(&mut self, roster: Roster, latch: &mut Latch) {
        if self.sent.as_ref() == Some(&roster) {
            return;
        }
        let payload = serde_json::to_string(&roster.windows).unwrap_or_else(|_| "[]".to_string());
        let reply = door::call(&["session", "window", "roster"], Some(&payload));
        self.sent = latch
            .reply("roster".to_string(), "publishing the terminal roster", reply)
            .then_some(roster);
    }
}
