//! A fake `eidolon web` for this crate's tests and the later crates'. It speaks
//! the wire as far as those tests need: the token check (401, empty body), the
//! `Host` check on POSTs (403), the body cap (413), `/api/events` as chunked
//! SSE, and `/api/say`, `/api/cancel` and `/api/answer` with their statuses.
//!
//! Any other path answers 200 and is recorded, so a test can assert the client
//! sent no request outside the four routes it uses.

use std::fs::{self, OpenOptions, Permissions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use serde_json::json;

/// The door's cap on a POST body (`eidolon/crates/web/src/serve.rs`'s `MAX_BODY`).
pub const MAX_BODY: usize = 8 * 1024 * 1024;

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
    Frame(String),
    End,
}

struct Shared {
    token: String,
    host: String,
    head: Mutex<Vec<String>>,
    replies: Mutex<Vec<(String, u16, String)>>,
    requests: Mutex<Vec<Recorded>>,
    live: Mutex<Option<Sender<Live>>>,
}

/// A fake door on `127.0.0.1:0`, serving on background threads until the test ends.
pub struct FakeDoor {
    addr: SocketAddr,
    token_file: PathBuf,
    shared: Arc<Shared>,
}

impl FakeDoor {
    /// Binds, and writes the token file as the door does: `0600`, named for the
    /// port, under `runtime/eidolon`, which is made `0700` if missing.
    pub fn start(runtime: &Path, token: &str) -> FakeDoor {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1:0");
        let addr = listener.local_addr().expect("the bound address");
        let eidolon = runtime.join("eidolon");
        fs::create_dir_all(&eidolon).expect("the eidolon dir");
        fs::set_permissions(&eidolon, Permissions::from_mode(0o700)).expect("eidolon dir at 0700");
        let token_file = eidolon.join(format!("web-{}.token", addr.port()));
        write_private(&token_file, token);
        let shared = Arc::new(Shared {
            token: token.to_string(),
            host: addr.to_string(),
            head: Mutex::new(Vec::new()),
            replies: Mutex::new(Vec::new()),
            requests: Mutex::new(Vec::new()),
            live: Mutex::new(None),
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

    /// The frames sent before `caught-up`: hello, the replay, any pending asks.
    pub fn set_head(&self, frames: &[&str]) {
        *self.shared.head.lock().unwrap() = frames.iter().map(|f| f.to_string()).collect();
    }

    /// Makes the next request to `path` answer `status` with `body`.
    pub fn reply(&self, path: &str, status: u16, body: &str) {
        self.shared.replies.lock().unwrap().push((path.to_string(), status, body.to_string()));
    }

    /// Sends one frame, a JSON document, on the open events stream.
    pub fn push(&self, frame: &str) {
        self.send(Live::Frame(frame.to_string()));
    }

    /// Ends the open events stream with its terminal chunk.
    pub fn end(&self) {
        self.send(Live::End);
    }

    /// Every request so far, in order.
    pub fn requests(&self) -> Vec<Recorded> {
        self.shared.requests.lock().unwrap().clone()
    }

    fn send(&self, live: Live) {
        let guard = self.shared.live.lock().unwrap();
        let tx = guard.as_ref().expect("an events stream is open");
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
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target.clone(), String::new()),
    };
    shared.requests.lock().unwrap().push(Recorded {
        method: method.clone(),
        path: path.clone(),
        query,
        headers: headers.clone(),
        body: body.clone(),
    });

    if path == "/api" || path.starts_with("/api/") {
        let want = format!("Bearer {}", shared.token);
        if header(&headers, "authorization") != Some(want.as_str()) {
            return respond(&mut writer, 401, &[], b"");
        }
    }
    if method == "POST" && header(&headers, "host") != Some(shared.host.as_str()) {
        let refusal = json!({"error": "Host does not match the bound address"});
        return respond_json(&mut writer, 403, &refusal);
    }
    if method == "POST" && body.len() > MAX_BODY {
        let refusal = json!({"error": format!("a body over {MAX_BODY} bytes is not read")});
        return respond_json(&mut writer, 413, &refusal);
    }

    if method == "GET" && path == "/api/events" {
        return stream_events(&mut writer, shared);
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

fn chunk(w: &mut impl Write, data: &str) -> io::Result<()> {
    write!(w, "{:x}\r\n{data}\r\n", data.len())?;
    w.flush()
}

/// Opens the stream (registering the live channel first, so a test that has
/// read the head can push), sends the head frames and `caught-up`, then the
/// pushed frames until an end.
fn stream_events(w: &mut impl Write, shared: &Shared) -> io::Result<()> {
    let (tx, rx) = channel();
    *shared.live.lock().unwrap() = Some(tx);
    let head: Vec<String> = shared.head.lock().unwrap().clone();
    w.write_all(
        b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\n\
          Transfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
    )?;
    w.flush()?;
    for frame in head.iter().map(String::as_str).chain(std::iter::once(r#"{"type":"caught-up"}"#)) {
        chunk(w, &format!("data: {frame}\n\n"))?;
    }
    while let Ok(Live::Frame(frame)) = rx.recv() {
        chunk(w, &format!("data: {frame}\n\n"))?;
    }
    w.write_all(b"0\r\n\r\n")?;
    w.flush()?;
    *shared.live.lock().unwrap() = None;
    Ok(())
}
