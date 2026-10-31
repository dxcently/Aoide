//! A fake `eidolon web` for this crate's tests and the later crates'. It speaks
//! the wire as far as those tests need: the token check (401, empty body), the
//! `Host` check on POSTs (403), the body cap (413), `/api/events` as chunked
//! SSE, and `/api/say`, `/api/cancel` and `/api/answer` with their statuses.
//!
//! Any other path answers 200 and is recorded, so a test can assert the client
//! sent no request outside the four routes it uses. A test can also answer
//! with bytes of its own: [`FakeDoor::reply_raw`] sends pieces as they are, and
//! [`FakeDoor::refuse_at_once`] refuses before the request body is read.
//!
//! A client that closes an open stream ends it: between frames the fake peeks
//! the socket, as hyper ends a connection whose client has gone
//! (`hyper` 1.11.1, `src/proto/h1/conn.rs`, half-close off, the default).

use std::fs::{self, OpenOptions, Permissions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::json;

/// The door's cap on a POST body (`eidolon/crates/web/src/serve.rs`'s `MAX_BODY`).
pub const MAX_BODY: usize = 8 * 1024 * 1024;

/// The hello a stream opens with unless a test sets its head.
const HELLO: &str =
    r#"{"type":"hello","session":"fake","cwd":"/","model":"fake","yolo":false,"pending":0,"protocol":1}"#;

/// One request as the fake received it.
#[derive(Debug, Clone)]
pub struct Recorded {
    pub method: String,
    pub path: String,
    pub query: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Recorded {
    pub fn header(&self, name: &str) -> Option<&str> {
        header(&self.headers, name)
    }
}

enum Live {
    Frame(Vec<u8>),
    End,
}

/// A raw reply: the pieces written as they are, each after its delay, and
/// whether the connection closes after them (else it is held open).
type Raw = (String, Vec<(Duration, Vec<u8>)>, bool);

struct Shared {
    token: String,
    host: String,
    head: Mutex<Vec<String>>,
    replies: Mutex<Vec<(String, u16, String)>>,
    at_once: Mutex<Vec<(String, u16, String)>>,
    raw: Mutex<Vec<Raw>>,
    requests: Mutex<Vec<Recorded>>,
    /// The open stream's id and its channel. A stream clears it only while it
    /// is still the one open.
    live: Mutex<Option<(u64, Sender<Live>)>>,
    streams: AtomicU64,
}

/// A fake door on `127.0.0.1:0`, serving on background threads until the test ends.
pub struct FakeDoor {
    addr: SocketAddr,
    token_file: PathBuf,
    shared: Arc<Shared>,
}

impl FakeDoor {
    /// Binds on 127.0.0.1 and writes the token file as the door does: `0600`,
    /// named for the port, under `runtime/eidolon`, made `0700` if missing.
    pub fn start(runtime: &Path, token: &str) -> FakeDoor {
        FakeDoor::start_on(Ipv4Addr::LOCALHOST, runtime, token)
    }

    /// `start`, bound on `ip` instead of 127.0.0.1.
    pub fn start_on(ip: Ipv4Addr, runtime: &Path, token: &str) -> FakeDoor {
        let listener = TcpListener::bind((ip, 0)).unwrap_or_else(|e| panic!("bind {ip}:0: {e}"));
        let addr = listener.local_addr().expect("the bound address");
        let eidolon = runtime.join("eidolon");
        fs::create_dir_all(&eidolon).expect("the eidolon dir");
        fs::set_permissions(&eidolon, Permissions::from_mode(0o700)).expect("eidolon dir at 0700");
        let token_file = eidolon.join(format!("web-{}.token", addr.port()));
        write_private(&token_file, token);
        let shared = Arc::new(Shared {
            token: token.to_string(),
            host: addr.to_string(),
            head: Mutex::new(vec![HELLO.to_string()]),
            replies: Mutex::new(Vec::new()),
            at_once: Mutex::new(Vec::new()),
            raw: Mutex::new(Vec::new()),
            requests: Mutex::new(Vec::new()),
            live: Mutex::new(None),
            streams: AtomicU64::new(0),
        });
        let accepting = shared.clone();
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let shared = accepting.clone();
                thread::spawn(move || {
                    let _ = handle(stream, &shared);
                });
            }
        });
        FakeDoor { addr, token_file, shared }
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn token_file(&self) -> PathBuf {
        self.token_file.clone()
    }

    /// Rewrites the token file, `0600`, with `token` (the door's own token does not change).
    pub fn set_token_file(&self, token: &str) {
        write_private(&self.token_file, token);
    }

    /// The frames a stream opens with, before `caught-up`: hello, the replay,
    /// any pending asks. Until a test sets it, the head is one valid hello.
    pub fn set_head(&self, frames: &[&str]) {
        *self.shared.head.lock().unwrap() = frames.iter().map(|f| f.to_string()).collect();
    }

    /// Makes the next request to `path` answer `status` with `body`.
    pub fn reply(&self, path: &str, status: u16, body: &str) {
        self.shared.replies.lock().unwrap().push((path.to_string(), status, body.to_string()));
    }

    /// Makes the next request to `path` answer with `pieces` written as they
    /// are, each after its delay. The connection then closes if `close` is set,
    /// else it stays open until the client closes it (or for 10 s).
    pub fn reply_raw(&self, path: &str, pieces: &[(Duration, &[u8])], close: bool) {
        let pieces = pieces.iter().map(|(delay, bytes)| (*delay, bytes.to_vec())).collect();
        self.shared.raw.lock().unwrap().push((path.to_string(), pieces, close));
    }

    /// Makes the next request to `path` answer `status` with `body` before its
    /// body is read, then close: a refusal sent at once, such as a 413.
    pub fn refuse_at_once(&self, path: &str, status: u16, body: &str) {
        self.shared.at_once.lock().unwrap().push((path.to_string(), status, body.to_string()));
    }

    /// Sends one frame, a JSON document, on the open events stream.
    pub fn push(&self, frame: &str) {
        self.push_bytes(frame.as_bytes());
    }

    /// Sends one frame's bytes as they are on the open events stream.
    pub fn push_bytes(&self, frame: &[u8]) {
        self.send(Live::Frame(frame.to_vec()));
    }

    /// Sends one frame, as `push` does, but returns false, rather than panic, when
    /// no events stream is open or its reader has gone.
    pub fn try_push(&self, frame: &str) -> bool {
        let guard = self.shared.live.lock().unwrap();
        guard.as_ref().is_some_and(|(_, tx)| tx.send(Live::Frame(frame.as_bytes().to_vec())).is_ok())
    }

    /// Ends the open events stream with its terminal chunk.
    pub fn end(&self) {
        self.send(Live::End);
    }

    /// Whether an events stream is open: from a client's request until the
    /// client closes it or the stream ends.
    pub fn streaming(&self) -> bool {
        self.shared.live.lock().unwrap().is_some()
    }

    /// Every request so far, in order.
    pub fn requests(&self) -> Vec<Recorded> {
        self.shared.requests.lock().unwrap().clone()
    }

    fn send(&self, live: Live) {
        let guard = self.shared.live.lock().unwrap();
        let (_, tx) = guard.as_ref().expect("an events stream is open");
        tx.send(live).expect("the events stream is still reading");
    }
}

/// Writes `body` to `path` as `0600` from the start, so no reader sees a wider mode.
fn write_private(path: &Path, body: &str) {
    let _ = fs::remove_file(path);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .expect("create the token file");
    file.write_all(body.as_bytes()).expect("write the token");
    fs::set_permissions(path, Permissions::from_mode(0o600)).expect("token file at 0600");
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
}

fn handle(stream: TcpStream, shared: &Shared) -> io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.trim_end().split(' ');
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or_default().to_string();
    let mut headers = Vec::new();
    loop {
        let mut raw = String::new();
        if reader.read_line(&mut raw)? == 0 {
            break;
        }
        let raw = raw.trim_end();
        if raw.is_empty() {
            break;
        }
        if let Some((k, v)) = raw.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    let length = header(&headers, "content-length").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target.clone(), String::new()),
    };
    let authorized = !(path == "/api" || path.starts_with("/api/"))
        || header(&headers, "authorization") == Some(format!("Bearer {}", shared.token).as_str());
    let host_ok = method != "POST" || header(&headers, "host") == Some(shared.host.as_str());
    if authorized && host_ok {
        if let Some((status, body)) = take_at_once(shared, &path) {
            record(shared, &method, &path, query, &headers, Vec::new());
            return respond(&mut writer, status, &[("Content-Type", "application/json")], body.as_bytes());
        }
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;
    record(shared, &method, &path, query, &headers, body.clone());

    if !authorized {
        return respond(&mut writer, 401, &[], b"");
    }
    if !host_ok {
        let refusal = json!({"error": "Host does not match the bound address"});
        return respond_json(&mut writer, 403, &refusal);
    }
    if method == "POST" && body.len() > MAX_BODY {
        let refusal = json!({"error": format!("a body over {MAX_BODY} bytes is not read")});
        return respond_json(&mut writer, 413, &refusal);
    }
    if let Some((pieces, close)) = take_raw(shared, &path) {
        return send_raw(&writer, &pieces, close);
    }
    if method == "GET" && path == "/api/events" {
        return stream_events(&writer, shared);
    }
    let queued = {
        let mut replies = shared.replies.lock().unwrap();
        replies.iter().position(|(p, _, _)| *p == path).map(|i| replies.remove(i))
    };
    let (status, reply) = match queued {
        Some((_, status, reply)) => (status, reply),
        None => default_reply(&method, &path),
    };
    respond(&mut writer, status, &[("Content-Type", "application/json")], reply.as_bytes())
}

fn record(shared: &Shared, method: &str, path: &str, query: String, headers: &[(String, String)], body: Vec<u8>) {
    shared.requests.lock().unwrap().push(Recorded {
        method: method.to_string(),
        path: path.to_string(),
        query,
        headers: headers.to_vec(),
        body,
    });
}

fn take_at_once(shared: &Shared, path: &str) -> Option<(u16, String)> {
    let mut queue = shared.at_once.lock().unwrap();
    let i = queue.iter().position(|(p, _, _)| p == path)?;
    let (_, status, body) = queue.remove(i);
    Some((status, body))
}

/// One raw reply's pieces, each written after its delay.
type Pieces = Vec<(Duration, Vec<u8>)>;

fn take_raw(shared: &Shared, path: &str) -> Option<(Pieces, bool)> {
    let mut queue = shared.raw.lock().unwrap();
    let i = queue.iter().position(|(p, _, _)| p == path)?;
    let (_, pieces, close) = queue.remove(i);
    Some((pieces, close))
}

/// Writes the pieces, then holds the connection until the client closes it (or
/// for 10 s), or closes it at once when `close` is set.
fn send_raw(stream: &TcpStream, pieces: &[(Duration, Vec<u8>)], close: bool) -> io::Result<()> {
    let mut w = stream;
    for (delay, bytes) in pieces {
        thread::sleep(*delay);
        w.write_all(bytes)?;
        w.flush()?;
    }
    if close {
        return Ok(());
    }
    stream.set_read_timeout(Some(POLL))?;
    let until = Instant::now() + Duration::from_secs(10);
    while Instant::now() < until && !client_gone(stream) {}
    Ok(())
}

fn default_reply(method: &str, path: &str) -> (u16, String) {
    match (method, path) {
        ("POST", "/api/say") => (202, json!({"queued": false}).to_string()),
        ("POST", "/api/cancel") | ("POST", "/api/answer") => (204, String::new()),
        _ => (200, "{}".to_string()),
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        202 => "Accepted",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        409 => "Conflict",
        413 => "Payload Too Large",
        422 => "Unprocessable Entity",
        500 => "Internal Server Error",
        _ => "Status",
    }
}

fn respond(w: &mut impl Write, status: u16, extra: &[(&str, &str)], body: &[u8]) -> io::Result<()> {
    let mut head = format!("HTTP/1.1 {status} {}\r\n", reason(status));
    if status != 204 {
        head.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    head.push_str("Connection: close\r\n");
    for (k, v) in extra {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    w.write_all(head.as_bytes())?;
    w.write_all(body)?;
    w.flush()
}

fn respond_json(w: &mut impl Write, status: u16, value: &serde_json::Value) -> io::Result<()> {
    respond(w, status, &[("Content-Type", "application/json")], value.to_string().as_bytes())
}

fn chunk(w: &mut impl Write, data: &[u8]) -> io::Result<()> {
    write!(w, "{:x}\r\n", data.len())?;
    w.write_all(data)?;
    w.write_all(b"\r\n")?;
    w.flush()
}

/// How often an open stream checks whether its client has closed it.
const POLL: Duration = Duration::from_millis(20);

/// Opens the stream (registering the live channel first, so a test that has
/// read the head can push), sends the head frames and `caught-up`, then the
/// pushed frames until an end, or until the client closes its end.
fn stream_events(stream: &TcpStream, shared: &Shared) -> io::Result<()> {
    let (tx, rx) = channel();
    let id = shared.streams.fetch_add(1, Ordering::Relaxed);
    *shared.live.lock().unwrap() = Some((id, tx));
    stream.set_read_timeout(Some(POLL))?;
    let mut w = stream;
    let head: Vec<String> = shared.head.lock().unwrap().clone();
    w.write_all(
        b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\n\
          Transfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
    )?;
    w.flush()?;
    for frame in head.iter().map(String::as_str).chain(std::iter::once(r#"{"type":"caught-up"}"#)) {
        chunk(&mut w, format!("data: {frame}\n\n").as_bytes())?;
    }
    loop {
        match rx.recv_timeout(POLL) {
            Ok(Live::Frame(frame)) => {
                chunk(&mut w, &[b"data: ".as_slice(), frame.as_slice(), b"\n\n".as_slice()].concat())?
            }
            Ok(Live::End) | Err(RecvTimeoutError::Disconnected) => {
                w.write_all(b"0\r\n\r\n")?;
                w.flush()?;
                break;
            }
            Err(RecvTimeoutError::Timeout) if client_gone(stream) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
    let mut live = shared.live.lock().unwrap();
    if live.as_ref().is_some_and(|(open, _)| *open == id) {
        *live = None;
    }
    Ok(())
}

/// Whether the client has closed its end: a peek reads end of file. A peek
/// that times out finds no byte and no close, so the client is still there.
fn client_gone(stream: &TcpStream) -> bool {
    let mut byte = [0u8; 1];
    match stream.peek(&mut byte) {
        Ok(0) => true,
        Ok(_) => false,
        Err(e) => !matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut),
    }
}
