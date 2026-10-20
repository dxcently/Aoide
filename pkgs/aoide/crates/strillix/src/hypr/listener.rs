//! The Hyprland event listener: one `socket2` stream, the window events that keep the
//! desktop's session windows and terminal roster current, and the 5 s tick between them.

use super::sweep::resolve_windows;
use super::terminals::sync_untracked_terminal_windows;
use aoide_conduct::graph::{clear_closed_window, sync_codex_app_threads};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

/// Path to the Hyprland event socket (`socket2`):
/// `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket2.sock`. Returns
/// `None` when not under a Hyprland session (no signature, or no runtime dir) —
/// the listener then degrades to "disabled" rather than crashing.
pub fn hypr_event_socket_path() -> Option<PathBuf> {
    let sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
    if sig.trim().is_empty() {
        return None;
    }
    let runtime = std::env::var("XDG_RUNTIME_DIR").ok()?;
    Some(
        PathBuf::from(runtime)
            .join("hypr")
            .join(sig)
            .join(".socket2.sock"),
    )
}

/// One parsed Hyprland `socket2` event we act on (window lifecycle only). Every
/// other event line maps to `None` and is ignored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HyprWindowEvent {
    /// A window opened / moved / retitled — (re)resolve pending session windows.
    /// `address` is the raw event address (no `0x` prefix); we resolve off the
    /// live `hyprctl clients` list rather than trusting it, so it is advisory.
    Appeared { address: String },
    /// A window closed — clear its address off whatever session stored it, so the
    /// roster stops advertising a dead jump target (the reaper then removes the
    /// record via its pid signal).
    Closed { address: String },
}

/// Parse ONE `socket2` line (`EVENT>>DATA`) into a [`HyprWindowEvent`], or `None`
/// for the many events we ignore. Pure and total (unit-tested): a line without
/// `>>`, an unhandled event name, or an empty address all yield `None` — never a
/// panic. The address is Hyprland's bare hex (e.g. `55aabb`); [`aoide_conduct::graph::normalize_addr`]
/// reconciles it with `hyprctl`'s `0x…` form at compare time.
pub fn parse_hypr_window_event(line: &str) -> Option<HyprWindowEvent> {
    let (event, data) = line.split_once(">>")?;
    match event {
        // openwindow>>ADDR,WORKSPACE,CLASS,TITLE · movewindow>>ADDR,WORKSPACE
        // movewindowv2>>ADDR,WSID,WSNAME · windowtitle>>ADDR
        // windowtitlev2>>ADDR,TITLE — in every case ADDR is the first field.
        "openwindow" | "movewindow" | "movewindowv2" | "windowtitle" | "windowtitlev2" => {
            let addr = data.split(',').next().unwrap_or("").trim();
            if addr.is_empty() {
                return None;
            }
            Some(HyprWindowEvent::Appeared {
                address: addr.to_string(),
            })
        }
        // closewindow>>ADDR — the whole payload is the address.
        "closewindow" => {
            let addr = data.trim();
            if addr.is_empty() {
                return None;
            }
            Some(HyprWindowEvent::Closed {
                address: addr.to_string(),
            })
        }
        _ => None,
    }
}

/// Is `e` a read-timeout expiry rather than a genuine socket failure? A blocking
/// read on a Unix stream whose `SO_RCVTIMEO` elapses returns `EAGAIN`, which
/// Rust surfaces as [`ErrorKind::WouldBlock`] on Unix (Windows uses `TimedOut`);
/// std documents either kind, so we accept both. Pure/testable, in the style of
/// [`is_terminal_class`] & friends — the listener uses it to tell "nothing
/// happened for 5s, re-tick" apart from "socket dropped, reconnect".
pub(crate) fn is_read_timeout(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    )
}

/// Run the Hyprland window→session event listener FOREVER — the shellbridge
/// service spawns this on a background thread so it can never block or kill the
/// socket accept loop. It connects to the `socket2` event stream and keeps
/// `sessions.json` authoritative: an opened/moved/retitled window (re)resolves
/// pending session windows, a closed window is cleared. Degrades gracefully — no
/// Hyprland signature logs once and returns (headless/non-Hypr aoide is
/// unaffected); a failed connect or a dropped socket logs and retries after a
/// short backoff. NEVER panics. A ~5s read timeout on the connection also
/// re-ticks the untracked-terminal sync (see [`is_read_timeout`]) so a bare tty's
/// cwd/title doesn't go stale between window events.
///
/// Every one of those same ticks also calls [`sync_codex_app_threads`] — for
/// PROMPTNESS only. The reaper's own tick (`reap.rs`) is the primary,
/// portable call site (it runs wherever `aoided` runs, Hyprland or not);
/// this listener merely reconciles sooner on a host where it happens to be
/// running. Wherever [`resolve_pending_session_windows`] also runs (reconnect,
/// `Appeared`), it comes AFTER the codex sync so a thread enrolled just now
/// is stamped with its window in the same pass.
pub fn run_hypr_window_listener() {
    use std::io::{BufRead, BufReader};
    let Some(sock) = hypr_event_socket_path() else {
        eprintln!(
            "[aoide/shellbridge] no HYPRLAND_INSTANCE_SIGNATURE — window-event listener disabled"
        );
        return;
    };
    // Populate the untracked-terminal roster once at startup, so the Terminals
    // widget has a complete tty roster the instant shellbridge comes up — not
    // only after the next window event fires.
    sync_codex_app_threads();
    sync_untracked_terminal_windows();
    loop {
        match UnixStream::connect(&sock) {
            Ok(stream) => {
                // On every (re)connect, sweep any windows that opened while we
                // were not listening (service start mid-session, or a reconnect).
                sync_codex_app_threads();
                resolve_windows();
                sync_untracked_terminal_windows();
                // Coarse read timeout so a blocking read wakes every ~5s even
                // when no window event fires — the periodic tick that refreshes
                // cwd/title for bare, untracked terminals. Best-effort: if it
                // fails the listener still works, just event-driven only.
                let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
                for line in BufReader::new(stream).lines() {
                    let line = match line {
                        Ok(line) => line,
                        // Timeout expiry (no event for ~5s): stay on this SAME
                        // connection, re-tick the untracked roster, keep reading.
                        // The read itself blocked for the full 5s, so this paces
                        // itself — no busy-loop.
                        Err(e) if is_read_timeout(&e) => {
                            sync_codex_app_threads();
                            sync_untracked_terminal_windows();
                            continue;
                        }
                        // Any other error: socket dropped → reconnect.
                        Err(_) => break,
                    };
                    match parse_hypr_window_event(&line) {
                        Some(HyprWindowEvent::Appeared { .. }) => {
                            sync_codex_app_threads();
                            resolve_windows();
                            sync_untracked_terminal_windows();
                        }
                        Some(HyprWindowEvent::Closed { address }) => {
                            clear_closed_window(&address);
                            sync_codex_app_threads();
                            sync_untracked_terminal_windows();
                        }
                        None => {}
                    }
                }
            }
            Err(e) => {
                eprintln!("[aoide/shellbridge] hypr event socket connect failed ({e}); retrying");
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}
