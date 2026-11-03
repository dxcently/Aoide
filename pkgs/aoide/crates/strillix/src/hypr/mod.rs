//! The desktop's Hyprland adapter. The compositor is read here and nowhere else. The desktop reports
//! what it sees to core through the door (`aoide session window ...`, `aoide session codex sync`),
//! one gated and audited call per change, and core reads no window itself. The bridge and the
//! shellbridge command use the names re-exported below.

mod client;
mod door;
mod focus;
mod listener;
mod sweep;
mod terminals;
mod watch;
#[cfg(test)]
pub(crate) mod testutil;
#[cfg(test)]
mod tests;

pub use client::hyprctl_clients;
pub use focus::{
    focus_session, focus_window, focus_workspace, focused_workspace, FocusError, NO_COMPOSITOR,
};
pub use listener::{hypr_event_socket_path, run_hypr_window_listener, HyprWindowEvent};
pub use watch::{follow_window_events, WatchEnd, WatchEvent};
