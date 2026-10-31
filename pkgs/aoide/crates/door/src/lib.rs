//! The loopback door client for `eidolon web`, the client half of the wire
//! `eidolon/crates/web` serves to one session.
//!
//! Blocking HTTP/1.1 over `std::net::TcpStream`: no async runtime, no HTTP
//! library, no GUI crate. The token is read from its file on every request,
//! under A6's checks (`token`). The socket that answers is checked to belong to
//! this user (`listener`) before a byte is written. The token is never printed
//! or logged.

mod client;
mod error;
mod events;
#[cfg(feature = "fake")]
pub mod fake;
mod frame;
mod http;
mod listener;
mod token;

pub use client::{AnswerOutcome, CancelOutcome, Door, Mode, SayOutcome};
pub use error::DoorError;
pub use events::{Closer, Events};
pub use frame::{Ask, AskOption, Frame, Timing, ToolUse, Usage};
pub use token::{check_runtime, runtime_from_env, token_path};

/// The wire version `hello` carries (`eidolon/crates/web/src/lib.rs`).
pub const PROTOCOL: u32 = 1;
