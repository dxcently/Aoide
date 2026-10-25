//! The desktop app's own doors and the asks they raise. The shell reaches a
//! process or a signal only through the exec crate, which it re-exports as
//! `exec`.

pub mod asks;
pub mod doors;
pub use aoide_desktop_exec as exec;
