//! The desktop app's child processes and its own doors. `exec` is the one
//! module that calls the process and signal APIs; the others reach a process
//! or a signal only through it.

pub mod asks;
pub mod doors;
pub mod exec;
