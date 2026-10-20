//! The untracked-terminal roster from the desktop side: each terminal-class client becomes a
//! `TermWindow` (its cwd read from `/proc`), and the core reconciler publishes the `win:*`
//! records from the list.

use super::client::hyprctl_clients;
use aoide_conduct::graph::{proc_cwd, sync_untracked_terminal_rows, TermWindow};
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

/// Publish the roster from the live clients: the core reconciler sees every terminal window
/// now, and removes any `win:*` record whose window has closed. Returns true iff
/// `sessions.json` changed. No compositor, no change.
pub fn sync_untracked_terminal_windows() -> bool {
    let Some(clients) = hyprctl_clients() else {
        return false;
    };
    let windows: Vec<TermWindow> = clients.iter().filter_map(term_window_from_client).collect();
    sync_untracked_terminal_rows(&windows)
}

