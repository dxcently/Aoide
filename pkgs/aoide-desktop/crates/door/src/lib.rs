//! The loopback door client for `eidolon web`, the client half of the wire
//! `eidolon/crates/web` serves to one session.
//!
//! Blocking HTTP/1.1 over `std::net::TcpStream`: no async runtime, no HTTP
//! library, no GUI crate. The token is read from its file on every request,
//! under A6's checks (`token`), and is never printed or logged.
#![deny(unsafe_code)]

mod client;
mod error;
mod events;
#[cfg(feature = "fake")]
pub mod fake;
mod frame;
mod http;
mod token;

pub use client::{AnswerOutcome, CancelOutcome, Door, Mode, SayOutcome};
pub use error::DoorError;
pub use events::{Closer, Events};
pub use frame::{Ask, AskOption, Frame, Timing, ToolUse, Usage};
pub use token::runtime_from_env;

/// The wire version `hello` carries (`eidolon/crates/web/src/lib.rs`).
pub const PROTOCOL: u32 = 1;
