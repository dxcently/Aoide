//! The desktop's focus verbs: jump to a window, a session's window, or a workspace, and read
//! the focused workspace. Each is one `hyprctl dispatch` (or one client read) over the request
//! socket. Refusals carry a stable reason so the bridge can say what happened.

use aoide_conduct::graph::{load_stage, normalize_addr, sessions_path, SessionsFile};
use serde_json::Value;

/// The taught refusal an omitted workspace gets when no compositor can say which one is focused.
pub const NO_COMPOSITOR: &str =
    "no compositor adapter here to say which workspace is focused — give the workspace id";

/// Does `want` name a live window in the parsed `hyprctl clients -j` array?
/// Pure over the already-decoded JSON so it is unit-testable without a
/// compositor. Matches on the normalised `address` field of any client.
pub(crate) fn window_present(clients: &[Value], want: &str) -> bool {
    let want = normalize_addr(want);
    clients.iter().any(|c| {
        c.get("address")
            .and_then(Value::as_str)
            .map(|a| normalize_addr(a) == want)
            .unwrap_or(false)
    })
}

/// The compositor's currently FOCUSED workspace id — the one fact an omitted
/// `<workspace>` needs (`aoide workspace set <project>`, `aoide workspace root`).
/// `hyprctl activeworkspace -j`'s `id`, read through the same
/// `HYPRLAND_INSTANCE_SIGNATURE` gate as [`hyprctl_clients`]. `None` whenever
/// the compositor cannot be consulted authoritatively — no signature, a
/// missing/failed `hyprctl`, or JSON with no integer `id` — which the caller
/// turns into a taught error asking for the number, never a guess. Note it is
/// the CALLER's process that can answer this: `aoided` runs as a service with
/// no compositor environment, so a forwarded invocation always carries a
/// resolved integer.
pub fn focused_workspace() -> Option<i64> {
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none() {
        return None;
    }
    let out = std::process::Command::new("hyprctl")
        .args(["activeworkspace", "-j"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let v: Value = serde_json::from_slice(&out.stdout).ok()?;
    v.get("id").and_then(Value::as_i64)
}

/// A structured failure from [`focus_window`]. `reason` is a stable machine
/// code (`hyprctl-unavailable` / `hyprctl-failed` / `window-not-found` /
/// `no-window-address`) reused verbatim by every focus-jump caller's own
/// error envelope (the conductor TUI and the shellbridge socket loop).
#[derive(Debug, Clone)]
pub struct FocusError {
    pub reason: &'static str,
    pub message: String,
}

/// The shared verify-then-dispatch used by BOTH the conductor TUI's Enter-key
/// jump ([`focus_session`], called directly — no CLI command exists for this
/// anymore) and the shellbridge socket loop (a widget click). `hyprctl
/// dispatch focuswindow`
/// exits 0 even when the target window is already gone, so we first list live
/// clients (`hyprctl clients -j`) and confirm the address is actually present
/// before dispatching — a vanished terminal is a `window-not-found` error, not
/// a silent no-op. Returns `Ok(())` only on a dispatched focus; every failure
/// (empty address, missing/failed hyprctl, unparseable JSON, absent window) is
/// a structured `Err` — this fn NEVER panics, so a socket loop can call it on
/// arbitrary input without risk.
pub fn focus_window(addr: &str) -> Result<(), FocusError> {
    let addr = addr.trim();
    if addr.is_empty() {
        return Err(FocusError {
            reason: "no-window-address",
            message: "empty window address".to_string(),
        });
    }

    // Verify the window exists before dispatching: focuswindow can't tell us.
    let clients: Vec<Value> = match std::process::Command::new("hyprctl")
        .args(["clients", "-j"])
        .output()
    {
        Err(e) => {
            return Err(FocusError {
                reason: "hyprctl-unavailable",
                message: format!("hyprctl unavailable: {e}"),
            });
        }
        Ok(out) if !out.status.success() => {
            return Err(FocusError {
                reason: "hyprctl-failed",
                message: format!("hyprctl clients failed (exit {:?})", out.status.code()),
            });
        }
        Ok(out) => match serde_json::from_slice(&out.stdout) {
            Ok(Value::Array(a)) => a,
            _ => {
                return Err(FocusError {
                    reason: "hyprctl-failed",
                    message: "hyprctl clients: unparseable JSON".to_string(),
                });
            }
        },
    };
    if !window_present(&clients, addr) {
        return Err(FocusError {
            reason: "window-not-found",
            message: format!("window {addr} is gone (terminal closed?)"),
        });
    }

    let dispatch = format!("address:{addr}");
    match std::process::Command::new("hyprctl")
        .args(["dispatch", "focuswindow", &dispatch])
        .output()
    {
        Err(e) => Err(FocusError {
            reason: "hyprctl-unavailable",
            message: format!("hyprctl unavailable: {e}"),
        }),
        Ok(out) if !out.status.success() => Err(FocusError {
            reason: "hyprctl-failed",
            message: format!("hyprctl dispatch failed (exit {:?})", out.status.code()),
        }),
        Ok(_) => Ok(()),
    }
}

/// Resolve a `sessionId` to a live jump and dispatch it — the socket-reachable
/// counterpart to the CLI [`focus`]. shellbridge drives this on a roster
/// row-click: QML sends only the sessionId (which every row already holds), and
/// the DAEMON owns the id→window resolution, so a widget never carries a stale or
/// empty address (the bug this fixes: the old socket command took a window address,
/// but the widgets passed a sessionId, so every tracked-row jump silently
/// no-op'd as `window-not-found`). Prefers the exact `windowAddress` (which also
/// brings its workspace forward); if that isn't resolved yet but the `workspace`
/// is known, falls back to switching to that workspace. Never panics — a socket
/// loop calls it on arbitrary input.
pub fn focus_session(id: &str) -> Result<(), FocusError> {
    let id = id.strip_prefix("session:").unwrap_or(id).trim();
    if id.is_empty() {
        return Err(FocusError {
            reason: "no-session-id",
            message: "empty session id".to_string(),
        });
    }
    let file: SessionsFile = match load_stage(&sessions_path()) {
        Ok(f) => f,
        Err(_) => {
            return Err(FocusError {
                reason: "stage-unreadable",
                message: "could not read sessions.json".to_string(),
            });
        }
    };
    let Some(rec) = file.sessions.iter().find(|s| s.session_id == id) else {
        return Err(FocusError {
            reason: "session-not-found",
            message: format!("unknown session `{id}`"),
        });
    };
    // Prefer the exact window — focuswindow also brings its workspace forward.
    if !rec.window_address.trim().is_empty() {
        return focus_window(&rec.window_address);
    }
    // The window isn't resolved yet (discovery is best-effort), but if we know
    // the workspace we can still take the user there.
    if let Some(ws) = rec.workspace {
        return focus_workspace(ws);
    }
    Err(FocusError {
        reason: "no-window-address",
        message: format!("session `{id}` has no window or workspace to focus"),
    })
}

/// Switch to a Hyprland workspace by numeric id (`hyprctl dispatch workspace
/// <id>`) — the fallback jump for a session whose window address isn't resolved
/// yet but whose workspace is known. Structured `Err` on a missing/failed
/// hyprctl; never panics.
pub fn focus_workspace(ws: i64) -> Result<(), FocusError> {
    match std::process::Command::new("hyprctl")
        .args(["dispatch", "workspace", &ws.to_string()])
        .output()
    {
        Err(e) => Err(FocusError {
            reason: "hyprctl-unavailable",
            message: format!("hyprctl unavailable: {e}"),
        }),
        Ok(out) if !out.status.success() => Err(FocusError {
            reason: "hyprctl-failed",
            message: format!(
                "hyprctl dispatch workspace failed (exit {:?})",
                out.status.code()
            ),
        }),
        Ok(_) => Ok(()),
    }
}
