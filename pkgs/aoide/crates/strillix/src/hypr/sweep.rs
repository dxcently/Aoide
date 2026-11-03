//! The session-window sweep: hands core the windows the compositor shows and asks it which session
//! with no window runs in which of them, and which bound session moved to another workspace
//! (`window_bindings`). Each answer is one core call (`session window attach`) through the door, and
//! core writes the binding. The sweep decides nothing and does not write `sessions.json`.

use super::client::hyprctl_clients;
use super::door::{self, Latch, Reply};
use super::terminals::term_window_from_client;
use aoide_conduct::graph::{load_stage, sessions_path, window_bindings, SessionRecord, SessionsFile};
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

/// The sweep over one stage snapshot and one client snapshot: the clients become `TermWindow`s,
/// core's `window_bindings` decides, and `attach` records each binding (session, window,
/// workspace) and reports whether it was written; the production one execs the core door, a test
/// one calls the library.
pub(crate) fn resolve_windows_in(
    sessions: &[SessionRecord],
    clients: &[Value],
    attach: &mut dyn FnMut(&str, &str, Option<i64>) -> bool,
) -> bool {
    let windows: Vec<_> = clients.iter().filter_map(term_window_from_client).collect();
    let mut changed = false;
    for binding in window_bindings(sessions, &windows) {
        changed |= attach(&binding.session, &binding.window, binding.workspace);
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
