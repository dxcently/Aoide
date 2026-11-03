//! The event socket (`socket2`), followed for one caller: `window watch` reads it through here.
//! Each connect is `aoide_protocol::local_socket::connect_bounded`, so it returns within its budget
//! even when the listener's backlog is full. A connection that ends emits `Lost` once and waits the
//! backoff before the next connect. A connect that fails ends the follow, and so does an emit that
//! fails, since then nothing is left to take the events. The desktop's listener (`listener.rs`)
//! keeps its own loop for core's calls.

use super::listener::{parse_hypr_window_event, HyprWindowEvent};
use aoide_protocol::local_socket::connect_bounded;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::time::Duration;

/// The bound on each connect: the same 2 s `hyprctl_clients` allows its whole request.
const CONNECT_BUDGET: Duration = Duration::from_secs(2);

/// One thing the follow saw, in the order it happened.
pub enum WatchEvent {
    /// The event socket accepted a connection.
    Connected,
    /// A window event on the open connection.
    Window(HyprWindowEvent),
    /// The connection ended: EOF, or a read that failed. The next connect waits the backoff.
    Lost,
}

/// Why the follow ended.
pub enum WatchEnd {
    /// A connect that failed: the connect's own error.
    NoCompositor(std::io::Error),
    /// `emit` failed, so an event could not be handed on.
    Unwritten(std::io::Error),
}

/// Follows the event socket at `sock`, handing each [`WatchEvent`] to `emit`, until the compositor
/// is gone or `emit` fails. `Lost` is emitted once for each connection that ends, and the next
/// connect waits `backoff`; a connect that fails for any reason ends the follow.
pub fn follow_window_events(
    sock: &Path,
    backoff: Duration,
    mut emit: impl FnMut(&WatchEvent) -> std::io::Result<()>,
) -> WatchEnd {
    loop {
        let stream = match connect_bounded(sock, CONNECT_BUDGET) {
            Ok(stream) => stream,
            Err(e) => return WatchEnd::NoCompositor(e),
        };
        if let Err(e) = emit(&WatchEvent::Connected) {
            return WatchEnd::Unwritten(e);
        }
        for read in BufReader::new(stream).split(b'\n') {
            let Ok(bytes) = read else { break };
            let text = String::from_utf8_lossy(&bytes);
            let Some(event) = parse_hypr_window_event(&text) else { continue };
            if let Err(e) = emit(&WatchEvent::Window(event)) {
                return WatchEnd::Unwritten(e);
            }
        }
        if let Err(e) = emit(&WatchEvent::Lost) {
            return WatchEnd::Unwritten(e);
        }
        std::thread::sleep(backoff);
    }
}
