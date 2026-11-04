//! The host seams and stateless helpers that name neither product: the git
//! probes, time formatting, the host name, the runtime root, SHA-256 and hex,
//! OS randomness, the shell a stored command line is handed to, the bounded
//! local-socket connect, the kernel's word on who is on the other end of a
//! socket, the atomic write and lock family, the stage reader, and, on
//! Windows, the process table, the `AF_UNIX` socket and the owner-only policy.
//!
//! A leaf. It depends on no workspace crate. `README.md` says what each module
//! holds and `AGENTS.md` what an edit there must keep.

pub mod fs;
pub mod git;
pub mod hash;
pub mod host_name;
pub mod host_random;
pub mod host_shell;
pub mod local_socket;
#[cfg(windows)]
pub mod owner_only;
pub mod peercred;
pub mod runtime_dir;
pub mod stage;
pub mod time;
#[cfg(windows)]
pub mod win_proc;
#[cfg(windows)]
pub mod win_unix;
