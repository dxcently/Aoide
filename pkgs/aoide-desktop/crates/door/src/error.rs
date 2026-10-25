use std::fmt;
use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;

use crate::PROTOCOL;

/// Every way a door call can fail. The answers a route expects (see
/// `SayOutcome`, `CancelOutcome`, `AnswerOutcome`) are `Ok` values, not errors.
#[derive(Debug)]
pub enum DoorError {
    /// The address is not `127.0.0.0/8` or `::1`. Refused before any connect.
    NotLoopback(SocketAddr),
    /// `XDG_RUNTIME_DIR` is unset or empty. There is no temp-dir fallback.
    RuntimeUnset,
    /// A6 refused the runtime dir, the `eidolon` dir or the token file.
    Refused { path: PathBuf, why: String },
    Connect { addr: SocketAddr, source: io::Error },
    Io(io::Error),
    /// A reply or frame that does not decode.
    Malformed(String),
    /// A `hello` whose protocol is not [`PROTOCOL`]; the stream ends behind it.
    Protocol { got: u64 },
    /// 401: the token presented is not the door's.
    Unauthorized,
    /// 403: the `Host` header is not the bound address.
    HostRefused,
    /// 413: the body is over the door's cap.
    TooLarge,
    /// Any other status, which this route does not take.
    Unexpected { status: u16 },
}

impl fmt::Display for DoorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DoorError::NotLoopback(addr) => {
                write!(f, "refusing {addr}: the door serves loopback only (127.0.0.0/8 or ::1)")
            }
            DoorError::RuntimeUnset => {
                f.write_str("XDG_RUNTIME_DIR is not set; the door token has no other place to live")
            }
            DoorError::Refused { path, why } => write!(f, "refused {}: {why}", path.display()),
            DoorError::Connect { addr, source } => write!(f, "cannot connect to {addr}: {source}"),
            DoorError::Io(e) => write!(f, "door i/o: {e}"),
            DoorError::Malformed(what) => write!(f, "malformed door data: {what}"),
            DoorError::Protocol { got } => {
                write!(f, "the door speaks protocol {got}; this client speaks {PROTOCOL}")
            }
            DoorError::Unauthorized => f.write_str("the door refused the token (401); the token file may be stale"),
            DoorError::HostRefused => f.write_str("the door refused the Host header (403)"),
            DoorError::TooLarge => f.write_str("the door refused the body as too large (413)"),
            DoorError::Unexpected { status } => {
                write!(f, "the door answered {status}, which this route does not take")
            }
        }
    }
}

impl std::error::Error for DoorError {}

impl From<io::Error> for DoorError {
    fn from(e: io::Error) -> Self {
        DoorError::Io(e)
    }
}
