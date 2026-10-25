//! The desktop app's child processes and its own doors. `exec` is the one
//! module that starts a process or sends a signal.

pub mod asks;
pub mod doors;
pub mod exec;
