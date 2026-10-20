//! The Hyprland event listener: one `socket2` stream, the window events the desktop reports to core
//! through the door, and the 5 s tick between them. What the listener owes core is [`Owed`]: a call
//! core answers is settled unless aoided stops listening during it, and a call that did not run
//! stays owed.

use super::client::hyprctl_clients;
use super::door::{self, Latch};
use super::sweep::resolve_windows;
use super::terminals::{Roster, RosterGate};
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
    /// A window opened: the codex threads, the pending session windows and the roster are reconciled.
    /// `address` is the raw event address (no `0x` prefix); the sweep resolves off the live `hyprctl
    /// clients` list rather than trusting it, so it is advisory.
    Appeared { address: String },
    /// A known window moved or was retitled: the pending session windows and the roster are
    /// reconciled. Nothing new opened, so no codex sync is owed.
    Changed { address: String },
    /// A window closed: core is told its address (`session window closed`), which ends the sessions
    /// that ran in it.
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
        // openwindow>>ADDR,WORKSPACE,CLASS,TITLE is the one appearance. movewindow>>ADDR,WORKSPACE,
        // movewindowv2>>ADDR,WSID,WSNAME, windowtitle>>ADDR and windowtitlev2>>ADDR,TITLE change a
        // window already known. In every case ADDR is the first field.
        "openwindow" | "movewindow" | "movewindowv2" | "windowtitle" | "windowtitlev2" => {
            let addr = data.split(',').next().unwrap_or("").trim();
            if addr.is_empty() {
                return None;
            }
            let address = addr.to_string();
            Some(if event == "openwindow" {
                HyprWindowEvent::Appeared { address }
            } else {
                HyprWindowEvent::Changed { address }
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

/// The calls the desktop still owes core. A closed window is reported once per event, a codex sync
/// runs on each appearance and each reconnect, a sweep runs on each tick, each reconnect and each
/// window event but a close, and the roster goes out when its content changes ([`RosterGate`]). A
/// yes from core settles a call, and so does a no given while aoided listens; a no given while
/// aoided stops listening leaves the call owed. A call that did not run stays owed and is tried
/// again on a later tick; a call with aoided not listening waits, unsent, until it does.
#[derive(Default)]
pub(crate) struct Owed {
    closed: Vec<String>,
    codex: bool,
    roster: RosterGate,
    latch: Latch,
}

impl Owed {
    /// One window event, and what it owes core now. An appearance owes the codex sync: a
    /// retitle or a move reconciles the sweep and the roster, which do nothing unless they change.
    pub(crate) fn event(&mut self, event: HyprWindowEvent) {
        match event {
            HyprWindowEvent::Appeared { .. } => {
                self.codex = true;
                self.settle_calls();
                self.sweep();
                self.settle_roster();
            }
            HyprWindowEvent::Changed { .. } => {
                self.settle_calls();
                self.sweep();
                self.settle_roster();
            }
            HyprWindowEvent::Closed { address } => {
                self.window_closed(address);
                self.settle_calls();
                self.settle_roster();
            }
        }
    }

    /// A window closed: owe core its address.
    pub(crate) fn window_closed(&mut self, address: String) {
        if !self.closed.contains(&address) {
            self.closed.push(address);
        }
    }

    /// A (re)connect is the start of the listener and of every restart: owe core a codex sync, and
    /// forget the sent roster so it goes out again.
    pub(crate) fn reconnected(&mut self) {
        self.codex = true;
        self.roster.forget();
    }

    /// Run the owed calls that need no compositor read: each closed window, and the codex sync when
    /// one is owed.
    pub(crate) fn settle_calls(&mut self) {
        let latch = &mut self.latch;
        self.closed.retain(|address| {
            let reply = door::call(&["session", "window", "closed", address.as_str()], None);
            !latch.reply(
                format!("closed {address}"),
                &format!("reporting closed window {address}"),
                reply,
            )
        });
        if self.codex {
            let reply = door::call(&["session", "codex", "sync"], None);
            self.codex = !self.latch.reply("codex".to_string(), "the codex thread sync", reply);
        }
    }

    /// The sweep: bind each pending session to its window through core.
    pub(crate) fn sweep(&mut self) {
        resolve_windows(&mut self.latch);
    }

    /// Publish the roster when it changed. Nothing when there is no compositor to read.
    pub(crate) fn settle_roster(&mut self) {
        if let Some(clients) = hyprctl_clients() {
            self.roster.settle(Roster::of(&clients), &mut self.latch);
        }
    }

    /// A tick with no event: owe whatever is still pending.
    pub(crate) fn tick(&mut self) {
        self.settle_calls();
        self.sweep();
        self.settle_roster();
    }

    #[cfg(test)]
    pub(crate) fn logged(&self) -> usize {
        self.latch.logged()
    }
}

/// Run the Hyprland window→session event listener FOREVER — the shellbridge
/// service spawns this on a background thread so it can never block or kill the
/// socket accept loop. It connects to the `socket2` event stream and reports each
/// window event to core through the door: an appearing window owes a codex sync,
/// a sweep and the roster; a moved or retitled window owes a sweep and the roster;
/// a closed window owes its address; a pending bind and a changed roster go out on the same ticks.
/// Degrades gracefully — no Hyprland signature logs once and
/// returns (headless/non-Hypr aoide is unaffected); a failed connect or a dropped
/// socket logs and retries after a short backoff. It does not panic. A ~5s read timeout
/// on the connection re-ticks the pending calls (see [`is_read_timeout`]) so a bare
/// tty's cwd and title do not go stale between window events.
pub fn run_hypr_window_listener() {
    use std::io::{BufRead, BufReader};
    let Some(sock) = hypr_event_socket_path() else {
        eprintln!(
            "[aoide/shellbridge] no HYPRLAND_INSTANCE_SIGNATURE — window-event listener disabled"
        );
        return;
    };
    let mut owed = Owed::default();
    loop {
        match UnixStream::connect(&sock) {
            Ok(stream) => {
                // Every (re)connect sweeps the windows that opened while we were not listening.
                owed.reconnected();
                owed.settle_calls();
                owed.sweep();
                owed.settle_roster();
                // Coarse read timeout so a blocking read wakes every ~5s even when no window
                // event fires. Best-effort: if it fails the listener still works, just event-driven.
                let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
                for line in BufReader::new(stream).lines() {
                    let line = match line {
                        Ok(line) => line,
                        // Timeout expiry (no event for ~5s): stay on this SAME connection and
                        // re-tick. The read itself blocked for the full 5s, so this paces itself.
                        Err(e) if is_read_timeout(&e) => {
                            owed.tick();
                            continue;
                        }
                        // Any other error: socket dropped → reconnect.
                        Err(_) => break,
                    };
                    if let Some(event) = parse_hypr_window_event(&line) {
                        owed.event(event);
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
