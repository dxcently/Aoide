//! The desktop's Hyprland adapter. The compositor is read and driven here and nowhere else:
//! the core engine reads no window, and a session's window reaches it through
//! `aoide session window attach`, which the sweep in this module execs. The bridge and the
//! shellbridge command use the names re-exported below.

mod client;
mod focus;
mod listener;
mod sweep;
mod terminals;
#[cfg(test)]
mod testutil;
#[cfg(test)]
mod tests;

pub use client::hyprctl_clients;
pub use focus::{
    focus_session, focus_window, focus_workspace, focused_workspace, FocusError, NO_COMPOSITOR,
};
pub use listener::run_hypr_window_listener;
pub use sweep::resolve_windows;
