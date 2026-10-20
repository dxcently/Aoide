//! `strillix focus session <id>` — the focus verb a shell reaches. The strillix
//! home module's `AOIDE_FOCUS_CMD` wrapper runs it with the session id as its
//! only argument, so the conductor's cue lands on the resolution the bridge's
//! `focussession` arm answers ([`crate::hypr::focus_session`]): the session's
//! window first, its workspace while no window is resolved.

use crate::dispatch::Invocation;
use crate::output::{Fix, Kind, Outcome};
use crate::registry::{arg, cmd, Registry};
use aoide_protocol::Door;
use serde_json::json;

pub fn register(r: &mut Registry) {
    r.insert(cmd!(
        path: ["focus", "session"],
        summary: "Focus a session: jump to its window (and that window's workspace), or to its workspace while no window is resolved. Core answers which window and workspace the session holds; the compositor takes the jump.",
        args: [arg!("id", "string", true, "The session id, with or without the `session:` prefix.")],
        flags: [],
        gated: false,
        implemented: true,
        handler: handle_session,
        examples: ["focus session 4f2a", "focus session session:4f2a --json"],
        brief: "Focus a session's window or workspace.",
        doors: &[Door::Cli],
    ));
}

fn handle_session(inv: &Invocation) -> Outcome {
    let cmd = "focus.session";
    let Some(id) = inv.args.first() else {
        return Outcome::usage(cmd, "a session id is required");
    };
    match crate::hypr::focus_session(id) {
        Ok(()) => Outcome::ok(cmd, format!("focused session {id}")),
        Err(err) => {
            let kind = match err.reason {
                "hyprctl-unavailable" | "hyprctl-failed" => Kind::Failed,
                _ => Kind::Refused,
            };
            Outcome::refuse(cmd, kind, "the session was not focused", err.message, Fix::None(fix_for(err.reason)))
                .with_fields(json!({ "reason": err.reason }))
        }
    }
}

/// What to do about a refusal, by its stable reason.
fn fix_for(reason: &str) -> &'static str {
    match reason {
        "session-not-found" => "give the id of a session core holds",
        "no-aoide" => "run it on a host where aoide is installed",
        "core-unavailable" => "check that the aoide binary runs",
        "hyprctl-unavailable" | "hyprctl-failed" => "run inside a Hyprland session, where hyprctl answers",
        "window-not-found" => "the session's window has closed; start the session again or focus its workspace",
        _ => "the session has no window or workspace to focus yet",
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::bridge::testutil::{env_lock, stub_core_answering};
    use aoide_protocol::output::Status;
    use aoide_test_support::{fake_hyprctl, inv};

    /// The verb takes the session's window: the compositor confirms it is live (the fake lists one at
    /// 0xAA), then focuses it.
    #[test]
    fn the_verb_jumps_to_the_sessions_window() {
        let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_core, _core_root) = stub_core_answering(
            "focus-verb-window",
            r#"{"status":"ok","command":"session.show","message":"session","data":{"windowAddress":"0xAA","workspace":3}}"#,
            0,
        );
        let (_hypr, _hypr_dir) = fake_hyprctl("focus-verb-window");
        let o = handle_session(&inv(&["focus", "session"], &["4f2a"]));
        assert_eq!(o.status, Status::Ok, "{o:?}");
        assert_eq!(o.message, "focused session 4f2a");
    }

    /// A window the compositor no longer lists is refused, not dispatched: `focuswindow` exits 0 even
    /// for a window that is gone, so the listing is the only check there is.
    #[test]
    fn the_verb_refuses_a_window_the_compositor_no_longer_has() {
        let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_core, _core_root) = stub_core_answering(
            "focus-verb-gone",
            r#"{"status":"ok","command":"session.show","message":"session","data":{"windowAddress":"0xBB"}}"#,
            0,
        );
        let (_hypr, _hypr_dir) = fake_hyprctl("focus-verb-gone");
        let o = handle_session(&inv(&["focus", "session"], &["4f2a"]));
        assert_eq!(o.status, Status::Error, "{o:?}");
        assert_eq!(o.data.clone().unwrap_or_default()["reason"], "window-not-found");
    }

    /// An id core does not hold is refused with core's own reason, before any compositor is asked.
    #[test]
    fn the_verb_names_the_core_refusal_for_an_unknown_session() {
        let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_core, _core_root) = stub_core_answering(
            "focus-verb-unknown",
            r#"{"status":"error","command":"session.show","message":"no local session with id `nobody`","data":{"reason":"session-not-found","sessionId":"nobody"}}"#,
            1,
        );
        let o = handle_session(&inv(&["focus", "session"], &["nobody"]));
        assert_eq!(o.status, Status::Error, "{o:?}");
        assert_eq!(o.data.clone().unwrap_or_default()["reason"], "session-not-found");
    }
}
