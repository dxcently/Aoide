//! The host seams that name neither product: OS randomness, the shell a stored
//! command line is handed to, the bounded local-socket connect, the kernel's
//! word on who is on the other end of a socket, and, on Windows, the process
//! table, the `AF_UNIX` socket and the owner-only policy.
//!
//! A leaf. It depends on no workspace crate. `README.md` says what each module
//! holds and `AGENTS.md` what an edit there must keep.

pub mod host_random;
pub mod host_shell;
pub mod local_socket;
#[cfg(windows)]
pub mod owner_only;
pub mod peercred;
#[cfg(windows)]
pub mod win_proc;
#[cfg(windows)]
pub mod win_unix;
