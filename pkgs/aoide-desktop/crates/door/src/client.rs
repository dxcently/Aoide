use std::io::{BufReader, Read};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};

use crate::error::DoorError;
use crate::events::Events;
use crate::http::{body_reader, framing_error, read_head, write_request, Head, Wire};
use crate::listener;
use crate::token::read_token;

const CONNECT: Duration = Duration::from_secs(5);
/// The deadline a request waits under: a call's head and body together, or an
/// events stream's head (its body has none).
const IO: Duration = Duration::from_secs(30);

/// The longest reply a non-stream route reads.
const MAX_REPLY: u64 = 64 * 1024;

/// `say`'s mode: a new turn or a follow-up, or a steer read before the next call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Send,
    Steer,
}

impl Mode {
    fn word(self) -> &'static str {
        match self {
            Mode::Send => "send",
            Mode::Steer => "steer",
        }
    }
}

/// `POST /api/say`'s answers: 202 with `queued`, or 422 with the door's reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SayOutcome {
    Queued { queued: bool },
    Refused { reason: String },
}

/// `POST /api/cancel`'s answers: 204 when a turn was running, 409 when idle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelOutcome {
    Cancelled,
    Idle,
}

/// `POST /api/answer`'s answers: 204, 409 (nothing pending under that id), or
/// 422 with the reason (the ask stays pending).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnswerOutcome {
    Answered,
    NotPending,
    Rejected { reason: String },
}

/// One `eidolon web` door: its address, the runtime directory its token file
/// lives in, and that token file. The token is read on each request.
pub struct Door {
    addr: SocketAddr,
    runtime: PathBuf,
    token_file: PathBuf,
    timeout: Duration,
}

impl Door {
    /// Refuses an address that is not loopback before anything connects.
    pub fn new(addr: SocketAddr, runtime: &Path, token_file: &Path) -> Result<Door, DoorError> {
        if !addr.ip().is_loopback() {
            return Err(DoorError::NotLoopback(addr));
        }
        Ok(Door { addr, runtime: runtime.to_path_buf(), token_file: token_file.to_path_buf(), timeout: IO })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Shortens the deadline a request waits under, for the tests (30 s otherwise).
    #[cfg(feature = "fake")]
    pub fn with_timeout(mut self, timeout: Duration) -> Door {
        self.timeout = timeout;
        self
    }

    pub fn say(&self, text: &str, mode: Mode) -> Result<SayOutcome, DoorError> {
        let body = json!({"text": text, "mode": mode.word()});
        let (status, bytes) = self.call("POST", "/api/say", Some(&body))?;
        match status {
            202 => {
                let queued = serde_json::from_slice::<Value>(&bytes)
                    .ok()
                    .and_then(|v| v.get("queued").and_then(Value::as_bool));
                queued
                    .map(|queued| SayOutcome::Queued { queued })
                    .ok_or_else(|| DoorError::Malformed("say's 202 carries no queued flag".to_string()))
            }
            422 => Ok(SayOutcome::Refused { reason: error_text(&bytes) }),
            other => Err(common(other)),
        }
    }

    pub fn cancel(&self) -> Result<CancelOutcome, DoorError> {
        let (status, _) = self.call("POST", "/api/cancel", None)?;
        match status {
            204 => Ok(CancelOutcome::Cancelled),
            409 => Ok(CancelOutcome::Idle),
            other => Err(common(other)),
        }
    }

    pub fn answer(&self, ask_id: u64, label: &str, note: Option<&str>) -> Result<AnswerOutcome, DoorError> {
        let mut body = json!({"ask_id": ask_id, "answer": label});
        if let Some(note) = note {
            body["note"] = json!(note);
        }
        let (status, bytes) = self.call("POST", "/api/answer", Some(&body))?;
        match status {
            204 => Ok(AnswerOutcome::Answered),
            409 => Ok(AnswerOutcome::NotPending),
            422 => Ok(AnswerOutcome::Rejected { reason: error_text(&bytes) }),
            other => Err(common(other)),
        }
    }

    pub fn events(&self) -> Result<Events, DoorError> {
        let (head, reader) = self.exchange("GET", "/api/events", None, true)?;
        match head.status {
            200 => {
                let socket = reader.get_ref().stream.try_clone()?;
                Ok(Events::new(body_reader(&head, reader), socket))
            }
            other => Err(common(other)),
        }
    }

    /// One request whose reply is read whole: its head and body share one deadline.
    fn call(&self, method: &str, path: &str, body: Option<&Value>) -> Result<(u16, Vec<u8>), DoorError> {
        let bytes = body.map(|b| serde_json::to_vec(b).expect("a json value serializes"));
        let (head, reader) = self.exchange(method, path, bytes.as_deref(), false)?;
        if let Some(length) = head.length.filter(|length| *length > MAX_REPLY) {
            return Err(DoorError::Malformed(format!("a reply of {length} bytes, over {MAX_REPLY}")));
        }
        let mut out = Vec::new();
        let read = body_reader(&head, reader).take(MAX_REPLY + 1).read_to_end(&mut out);
        // An error status is decided by its head: a reset can cut its body short.
        if let Some(e) = read.err().filter(|_| head.status < 400) {
            return Err(framing_error(e));
        }
        if out.len() as u64 > MAX_REPLY {
            return Err(DoorError::Malformed(format!("a reply over {MAX_REPLY} bytes")));
        }
        Ok((head.status, out))
    }

    /// Reads the token under A6, connects, checks the listener before a byte is
    /// written, writes the request and reads the reply's head. A write that
    /// fails is reported only if no reply came: a door may answer (413) before
    /// it has read the whole body. An events stream keeps no deadline once its
    /// head is read.
    fn exchange(
        &self,
        method: &str,
        path: &str,
        body: Option<&[u8]>,
        streaming: bool,
    ) -> Result<(Head, BufReader<Wire>), DoorError> {
        let token = read_token(&self.runtime, &self.token_file, self.addr.port())?;
        let mut stream = TcpStream::connect_timeout(&self.addr, CONNECT)
            .map_err(|source| DoorError::Connect { addr: self.addr, source })?;
        listener::check(self.addr, stream.local_addr()?)?;
        stream.set_write_timeout(Some(self.timeout))?;
        let sent = write_request(&mut stream, method, path, &self.addr.to_string(), token.as_str(), body);
        let mut reader = BufReader::new(Wire::new(stream.try_clone()?, self.timeout));
        match read_head(&mut reader) {
            Ok(head) => {
                if streaming {
                    reader.get_mut().release()?;
                }
                Ok((head, reader))
            }
            Err(e) => Err(match sent {
                Err(io) => DoorError::Io(io),
                Ok(()) => e,
            }),
        }
    }
}

/// The statuses every route shares. Anything a route does not take is
/// [`DoorError::Unexpected`].
fn common(status: u16) -> DoorError {
    match status {
        401 => DoorError::Unauthorized,
        403 => DoorError::HostRefused,
        413 => DoorError::TooLarge,
        status => DoorError::Unexpected { status },
    }
}

/// The `error` string of a door reply, or the reply as text when it has none.
fn error_text(bytes: &[u8]) -> String {
    serde_json::from_slice::<Value>(bytes)
        .ok()
        .and_then(|v| v.get("error").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| String::from_utf8_lossy(bytes).into_owned())
}
