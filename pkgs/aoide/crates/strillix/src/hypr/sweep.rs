//! The session-window sweep: decides which session with no window runs in which terminal window,
//! and which bound session moved to another workspace. Each decision is one core call (`session
//! window attach`) through the door, and core writes the binding. The sweep does not write
//! `sessions.json`.

use super::client::{client_workspace_for_address, hyprctl_clients};
use super::door::{self, Latch, Reply};
use aoide_conduct::graph::{
    load_stage, pid_ancestry, sessions_path, windowless_by_lineage, SessionRecord, SessionsFile,
};
use serde_json::Value;

/// Bind every session the clients snapshot can place, and re-stamp every bound session's
/// workspace. Returns true iff core accepted a binding. No compositor, or nothing pending and
/// nothing windowed, costs no client read.
pub(crate) fn resolve_windows(latch: &mut Latch) -> bool {
    let file = match load_stage::<SessionsFile>(&sessions_path()) {
        Ok(file) => {
            latch.recovered("sessions-read");
            file
        }
        Err(e) => {
            latch.problem("sessions-read", &format!("[aoide/shellbridge] the window sweep binds nothing: {e}"));
            return false;
        }
    };
    let has_work = file.sessions.iter().any(|s| {
        !s.window_address.is_empty()
            || s.pid.is_some()
            || (!s.hook_ancestry.is_empty() && s.state != "done")
    });
    if !has_work {
        return false;
    }
    let Some(clients) = hyprctl_clients() else {
        return false;
    };
    resolve_windows_in(&file.sessions, &clients, &mut |id, window, ws| {
        attach_through_core(&mut *latch, id, window, ws)
    })
}

/// The sweep over one stage snapshot and one client snapshot. `attach` records a binding
/// (session, window, workspace) and reports whether it was written; the production one execs
/// the core door, a test one calls the library.
pub(crate) fn resolve_windows_in(
    sessions: &[SessionRecord],
    clients: &[Value],
    attach: &mut dyn FnMut(&str, &str, Option<i64>) -> bool,
) -> bool {
    let mut changed = false;
    for s in sessions {
        let (window, workspace) = if s.window_address.is_empty() {
            if windowless_by_lineage(&s.session_id, sessions) {
                continue;
            }
            let bound = match s.pid {
                Some(pid) => match_window_and_pid(&pid_ancestry(pid as i32), clients)
                    .map(|(addr, _)| addr),
                None if s.state != "done" => window_for_hook_chain(
                    &s.hook_ancestry,
                    clients,
                    |p, owner| pid_ancestry(p).contains(&owner),
                ),
                None => None,
            };
            let Some(addr) = bound else {
                continue;
            };
            let ws = client_workspace_for_address(clients, &addr);
            (addr, ws)
        } else {
            let ws = client_workspace_for_address(clients, &s.window_address);
            (s.window_address.clone(), ws)
        };
        let moved = workspace.is_some() && workspace != s.workspace;
        if s.window_address != window || moved {
            changed |= attach(&s.session_id, &window, workspace);
        }
    }
    changed
}

/// `aoide session window attach <id> <window> [--workspace <n>]` through the core door, the one write
/// a window binding makes. A binding core refused is final: the same session, window and workspace
/// is not asked again. A call that did not run is asked again by the next sweep.
fn attach_through_core(latch: &mut Latch, id: &str, window: &str, workspace: Option<i64>) -> bool {
    let fact = format!("{id} {window} {workspace:?}");
    if latch.is_refused(&fact) {
        return false;
    }
    let ws = workspace.map(|ws| ws.to_string());
    let mut argv = vec!["session", "window", "attach", id, window];
    if let Some(ws) = ws.as_deref() {
        argv.extend(["--workspace", ws]);
    }
    let reply = door::call(&argv, None);
    if reply == Reply::Refused {
        latch.refuse(fact);
    }
    latch.reply(
        format!("attach {id}"),
        &format!("binding session {id} to window {window}"),
        reply,
    );
    reply == Reply::Done
}

/// Address-and-pid variant of the ancestor↔client match: also returns the
/// matched client's `pid` — the terminal window's owning process. The hook door
/// records this pid on the session so the reaper has a `/proc` liveness signal
/// that vanishes with the window (never a false reap: the pid lives exactly as
/// long as the window).
pub(crate) fn match_window_and_pid(ancestry: &[i32], clients: &[Value]) -> Option<(String, u32)> {
    for &pid in ancestry {
        for c in clients {
            if c.get("pid").and_then(Value::as_i64) == Some(pid as i64) {
                if let Some(addr) = c.get("address").and_then(Value::as_str) {
                    if !addr.is_empty() {
                        return Some((addr.to_string(), pid as u32));
                    }
                }
            }
        }
    }
    None
}

/// The window a pid-less hosted session is bound to, read off its stored
/// `hookAncestry` (a window hosts any number of agents, and a harness that never
/// reports a pid is found only through its chain). The owner is the nearest chain
/// pid that owns a client — the same nearest-ancestor rule [`match_window_and_pid`]
/// applies to a recorded pid. The chain binds only when a process of its own
/// stands BENEATH that owner right now: a stale chain whose pids are all dead
/// (`pid_ancestry` of a dead pid is that pid alone) or reused outside the owner's
/// tree binds nothing. `beneath(p, owner)` is that probe, injected for tests;
/// production passes `pid_ancestry(p).contains(&owner)`.
pub(crate) fn window_for_hook_chain(
    chain: &[i32],
    clients: &[Value],
    beneath: impl Fn(i32, i32) -> bool,
) -> Option<String> {
    let (addr, owner) = match_window_and_pid(chain, clients)?;
    let owner = owner as i32;
    let i = chain.iter().position(|&p| p == owner)?;
    chain[..i].iter().any(|&p| beneath(p, owner)).then_some(addr)
}
