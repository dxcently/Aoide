//! The shellbridge socket framework: the unix socket that widgets send newline-delimited
//! JSON commands to, and the loop every bridge socket runs.
//!
//! A binary serves its socket with [`run_on`] and a [`Socket`] of its verbs. A line is
//! offered first to the socket's `front` hook, when it has one, and then to the verb table
//! by its `cmd` word. A line nothing claims is audited to the socket's own log as
//! `unparseable` and dropped, answering nothing. Only a socket whose `front` hook answers
//! writes a reply on a line the table does not serve.
//!
//! Every connection passes the cross-uid floor in [`serve`] before its first line is read:
//! a peer whose user is not this process's own, or cannot be read, is refused unanswered.
//! That floor closes a cross-user gap only. A same-user process is not stopped here.
//!
//! The core-specific commands (their parser, dispatch and reply shapes) are not in this
//! module: the binary that serves the core socket supplies them through `front`.

use crate::audit::{audit, Door, EventClass};
use crate::peercred::{own_user, peer_cred, PeerCred};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader};
#[cfg(unix)]
pub use std::os::unix::net::{UnixListener, UnixStream};
#[cfg(windows)]
pub use crate::win_unix::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

/// What a line tells the connection loop: keep reading (`Next`), or the line took the
/// connection's one reply channel and the connection is done (`Answered`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Next,
    Answered,
}

/// One verb a binary registers on a shellbridge socket: the wire `cmd` word it claims,
/// and the handler that serves a line naming it.
pub struct Verb {
    pub cmd: &'static str,
    pub serve: fn(&str, &mut Option<UnixStream>) -> Flow,
}

/// The hook a socket runs on every non-empty line before its verb table. It returns
/// `Some(flow)` when it claimed the line, and `None` to pass the line on to the table.
pub type Front = fn(&str, &mut Option<UnixStream>) -> Option<Flow>;

/// What one shellbridge socket serves: an optional `front` hook, the verbs it registers,
/// and the audit log its drops and refusals land in.
pub struct Socket {
    pub verbs: &'static [Verb],
    pub log: fn() -> PathBuf,
    pub front: Option<Front>,
}

/// The `cmd` word a wire line names, when it parses as one JSON object. Total: malformed
/// JSON names nothing. A verb whose caller is parked on a reply reads it to recognise its
/// own line.
pub fn wire_cmd(line: &str) -> Option<String> {
    serde_json::from_str::<Value>(line.trim())
        .ok()?
        .get("cmd")?
        .as_str()
        .map(str::to_string)
}

/// What a line no socket hook or verb served leaves behind: the `unparseable` audit (a
/// byte count, never the payload). It answers nothing.
pub fn drop_unparsed(log: &Path, line: &str) {
    let _ = audit(
        log,
        Door::Daemon,
        EventClass::Audit,
        "shellbridge",
        "unparseable",
        &format!("dropped one unparseable or unknown command line ({} bytes)", line.len()),
    );
}

/// The cross-uid floor, pure and unit-tested without a real different-user connection: a
/// peer credential in, an `Option<String>` refusal reason out. `None` (admitted) only when
/// the peer's kernel-attested user equals this process's own. An unidentified peer, a
/// failed read, or a peer whose pid was reused mid-read is refused the same fail-closed way
/// a mismatched user is.
pub fn cross_uid_gate(peer: Option<PeerCred>) -> Option<String> {
    let own = own_user();
    match (peer.and_then(|p| p.user()), own) {
        (Some(peer), Some(own)) if peer == own => None,
        (Some(peer), Some(own)) => Some(format!(
            "shellbridge connection refused: peer {} does not match this process's own {}",
            peer.label(),
            own.label()
        )),
        (_, None) => Some(
            "shellbridge connection refused: this process's own user could not be determined"
                .to_string(),
        ),
        (None, _) => Some(
            "shellbridge connection refused: the peer's user could not be determined (the kernel \
             identity read failed)"
                .to_string(),
        ),
    }
}

/// Serve `socket` on `path` until the listener closes. A stale socket from an unclean
/// shutdown would make the bind fail with EADDRINUSE, so it is unlinked first (the path is
/// single-owner per user). A bind failure is audited to the socket's log and reported in
/// the returned status; nothing here panics.
pub fn run_on(path: &Path, socket: &'static Socket) -> Value {
    let log = (socket.log)();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::remove_file(path);
    let listener = match UnixListener::bind(path) {
        Ok(l) => l,
        Err(e) => {
            let _ = audit(
                &log,
                Door::Daemon,
                EventClass::Audit,
                "shellbridge",
                "bind-failed",
                &format!("could not bind {}: {e}", path.display()),
            );
            return json!({
                "process": "shellbridge",
                "state": "error",
                "error": format!("bind {}: {e}", path.display()),
                "socket": path.to_string_lossy(),
            });
        }
    };

    let _ = audit(
        &log,
        Door::Daemon,
        EventClass::Audit,
        "shellbridge",
        "started",
        &format!("shellbridge online; listening on {}", path.display()),
    );

    // Serve forever. `serve` never returns and never panics on client input.
    serve(&listener, socket);

    // Reached when `incoming()` ends (listener closed): a clean stop, so systemd's
    // `Restart=on-failure` can bring us back.
    json!({ "process": "shellbridge", "state": "stopped", "socket": path.to_string_lossy() })
}

/// Accept connections forever, one thread per connection, behind the cross-uid floor.
/// `socket` names the hook and verbs it serves and the log its refusals land in.
pub fn serve(listener: &UnixListener, socket: &'static Socket) {
    for conn in listener.incoming() {
        match conn {
            Ok(stream) => {
                let peer = peer_cred(&stream);
                if let Some(reason) = cross_uid_gate(peer) {
                    let _ = audit(
                        &(socket.log)(),
                        Door::Daemon,
                        EventClass::Audit,
                        "shellbridge",
                        "peercred-refused",
                        &reason,
                    );
                    continue; // Dropped, unconditionally: never reaches `handle_conn`.
                }
                std::thread::spawn(move || handle_conn(stream, socket));
            }
            Err(e) => eprintln!("[aoide/shellbridge] accept error (continuing): {e}"),
        }
    }
}

/// Handle ONE client connection, on its OWN thread (`serve` spawns one per accepted
/// connection): read newline-delimited JSON lines and act on each. A non-empty line goes to
/// the socket's `front` hook first. A line the hook declines goes to the verb table by its
/// `cmd` word, and a line neither claims is audited and dropped. `Flow::Answered` from either
/// ends the connection. A read error (a dropped connection) ends only this connection.
/// Containment is per-connection: a panic dies with its own thread, and a slow or idle
/// connection never starves another. No read timeout is set on the accepted stream: a client
/// may idle between commands.
fn handle_conn(stream: UnixStream, socket: &'static Socket) {
    let mut reply = stream.try_clone().ok();
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            // Dropped connection / read error: done with this connection only.
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }
        let flow = match socket.front.and_then(|front| front(&line, &mut reply)) {
            Some(flow) => flow,
            None => match wire_cmd(&line).and_then(|cmd| socket.verbs.iter().find(|v| v.cmd == cmd)) {
                Some(verb) => (verb.serve)(&line, &mut reply),
                None => {
                    drop_unparsed(&(socket.log)(), &line);
                    Flow::Next
                }
            },
        };
        if flow == Flow::Answered {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::peercred::PeerUser;
    use std::io::{BufRead, BufReader, Write};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    /// A scratch directory private to one test: its own name, this process's pid, so
    /// parallel tests and parallel runs never share a log or a socket. Created, never
    /// cleared: a log path is asked for again by the reader after the writer made it.
    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aoide-protocol-sb-{}-{tag}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    /// A socket path short enough for `sun_path` on every host (108 bytes on Unix,
    /// 107 on native Windows), named by its test tag.
    #[cfg(unix)]
    fn short_sock(tag: &str) -> PathBuf {
        scratch(tag).join("s.sock")
    }

    /// Serve `socket` on a fresh listener at `sock`, on a thread that is not joined.
    #[cfg(unix)]
    fn spawn_serve(sock: &Path, socket: &'static Socket) {
        let listener = UnixListener::bind(sock).expect("bind scratch socket");
        std::thread::spawn(move || serve(&listener, socket));
    }

    /// A client whose every read gives up after three seconds: a loop that never answers
    /// fails its test with a timeout, never hangs it.
    #[cfg(unix)]
    fn connect(sock: &Path) -> UnixStream {
        let client = UnixStream::connect(sock).expect("connect");
        client.set_read_timeout(Some(Duration::from_secs(3))).expect("read timeout");
        client
    }

    /// Wait for `needle` to appear in `log`, for up to two seconds, and return the log.
    fn wait_for(log: &Path, needle: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut text = String::new();
        while Instant::now() < deadline {
            text = std::fs::read_to_string(log).unwrap_or_default();
            if text.contains(needle) {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        text
    }

    fn ping_verb(_line: &str, reply: &mut Option<UnixStream>) -> Flow {
        if let Some(mut reply) = reply.take() {
            let _ = writeln!(reply, "pong");
        }
        Flow::Answered
    }

    /// Answers on the connection and keeps it open: the loop must read on.
    fn tick_verb(_line: &str, reply: &mut Option<UnixStream>) -> Flow {
        if let Some(reply) = reply.as_mut() {
            let _ = writeln!(reply, "tick");
        }
        Flow::Next
    }

    fn verb_log() -> PathBuf {
        scratch("verb").join("log")
    }

    static VERB_SOCKET: Socket = Socket {
        verbs: &[Verb { cmd: "ping", serve: ping_verb }],
        log: verb_log,
        front: None,
    };

    fn drop_log() -> PathBuf {
        scratch("drop").join("log")
    }

    static DROP_SOCKET: Socket = Socket { verbs: &[], log: drop_log, front: None };

    fn nofront_log() -> PathBuf {
        scratch("nofront").join("log")
    }

    /// No front hook: the table serves `ping`, and the unclaimed `tick` is dropped.
    static NOFRONT_SOCKET: Socket = Socket {
        verbs: &[Verb { cmd: "ping", serve: ping_verb }],
        log: nofront_log,
        front: None,
    };

    fn front_log() -> PathBuf {
        scratch("front").join("log")
    }

    /// Claims only `{"cmd":"front"}`, answering on the connection and ending it.
    fn answering_front(line: &str, reply: &mut Option<UnixStream>) -> Option<Flow> {
        if wire_cmd(line).as_deref() != Some("front") {
            return None;
        }
        if let Some(mut reply) = reply.take() {
            let _ = writeln!(reply, "front");
        }
        Some(Flow::Answered)
    }

    static FRONT_SOCKET: Socket = Socket {
        verbs: &[Verb { cmd: "ping", serve: ping_verb }],
        log: front_log,
        front: Some(answering_front),
    };

    static OFFERED: AtomicUsize = AtomicUsize::new(0);

    /// Counts every line offered to it and declines all of them.
    fn counting_front(_line: &str, _reply: &mut Option<UnixStream>) -> Option<Flow> {
        OFFERED.fetch_add(1, Ordering::SeqCst);
        None
    }

    fn counted_log() -> PathBuf {
        scratch("counted").join("log")
    }

    static COUNTED_SOCKET: Socket = Socket {
        verbs: &[
            Verb { cmd: "tick", serve: tick_verb },
            Verb { cmd: "ping", serve: ping_verb },
        ],
        log: counted_log,
        front: Some(counting_front),
    };

    /// Declines every line and has no verb for it: the loop must drop it, audited.
    fn declining_front(_line: &str, _reply: &mut Option<UnixStream>) -> Option<Flow> {
        None
    }

    fn declined_log() -> PathBuf {
        scratch("declined").join("log")
    }

    static DECLINED_SOCKET: Socket = Socket {
        verbs: &[Verb { cmd: "ping", serve: ping_verb }],
        log: declined_log,
        front: Some(declining_front),
    };

    #[cfg(unix)]
    #[test]
    fn a_table_verb_is_served_and_ends_its_connection() {
        let sock = short_sock("table");
        spawn_serve(&sock, &VERB_SOCKET);

        let mut client = connect(&sock);
        client.write_all(b"{\"cmd\":\"ping\"}\n").unwrap();
        client.flush().unwrap();
        let mut reader = BufReader::new(client);
        let mut answer = String::new();
        reader.read_line(&mut answer).unwrap();
        assert_eq!(answer, "pong\n");
        let mut rest = String::new();
        assert_eq!(reader.read_line(&mut rest).unwrap(), 0, "the verb answered Answered, so the connection closed");
    }

    #[cfg(unix)]
    #[test]
    fn an_unclaimed_line_is_audited_unparseable_and_dropped_unanswered() {
        let sock = short_sock("unclaimed");
        spawn_serve(&sock, &DROP_SOCKET);

        let line = "{\"cmd\":\"nobody\"}";
        let mut client = connect(&sock);
        client.write_all(format!("{line}\n").as_bytes()).unwrap();
        client.flush().unwrap();
        client.set_read_timeout(Some(Duration::from_millis(400))).unwrap();
        let mut silence = String::new();
        let heard = BufReader::new(&client).read_line(&mut silence);
        assert!(heard.is_err() || silence.is_empty(), "an unclaimed line is never answered: {silence:?}");

        let text = wait_for(&drop_log(), "unparseable");
        assert!(text.contains("unparseable"), "{text}");
        assert!(text.contains(&format!("({} bytes)", line.len())), "the audit names a byte count, not the payload: {text}");
        assert!(!text.contains("nobody"), "the payload never reaches the log: {text}");
    }

    #[cfg(unix)]
    #[test]
    fn a_front_answer_ends_the_connection_before_the_table_is_consulted() {
        let sock = short_sock("front");
        spawn_serve(&sock, &FRONT_SOCKET);

        let mut client = connect(&sock);
        client.write_all(b"{\"cmd\":\"front\"}\n{\"cmd\":\"ping\"}\n").unwrap();
        client.flush().unwrap();
        let mut reader = BufReader::new(client);
        let mut answer = String::new();
        reader.read_line(&mut answer).unwrap();
        assert_eq!(answer, "front\n");
        let mut rest = String::new();
        assert_eq!(
            reader.read_line(&mut rest).unwrap(),
            0,
            "Answered from the front ends the connection: the ping after it is never served, got {rest:?}"
        );
    }

    /// A declining front sees each non-empty line before the table: the two `tick`s are
    /// declined, served by a Next verb, and read on; the `ping` is served and ends the
    /// connection. The front is offered exactly the three non-empty lines.
    #[cfg(unix)]
    #[test]
    fn a_declining_front_is_offered_every_line_and_the_table_serves_them_in_order() {
        let sock = short_sock("counted");
        spawn_serve(&sock, &COUNTED_SOCKET);

        let mut client = connect(&sock);
        client.write_all(b"{\"cmd\":\"tick\"}\n\n{\"cmd\":\"tick\"}\n{\"cmd\":\"ping\"}\n").unwrap();
        client.flush().unwrap();
        let mut reader = BufReader::new(client);
        let mut answers = Vec::new();
        for _ in 0..3 {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            answers.push(line);
        }
        assert_eq!(answers, ["tick\n", "tick\n", "pong\n"]);
        let mut rest = String::new();
        assert_eq!(reader.read_line(&mut rest).unwrap(), 0, "the ping's Answered ends the connection");
        assert_eq!(OFFERED.load(Ordering::SeqCst), 3, "the blank line is skipped before the front");
    }

    /// A socket with no front serves its table and drops what the table does not claim:
    /// the `tick` gets no answer, and the `ping` after it is the first line the client reads.
    #[cfg(unix)]
    #[test]
    fn a_socket_without_a_front_serves_its_table_and_drops_the_rest() {
        let sock = short_sock("nofront");
        spawn_serve(&sock, &NOFRONT_SOCKET);

        let mut client = connect(&sock);
        client.write_all(b"{\"cmd\":\"tick\"}\n{\"cmd\":\"ping\"}\n").unwrap();
        client.flush().unwrap();
        let mut reader = BufReader::new(client);
        let mut answer = String::new();
        reader.read_line(&mut answer).unwrap();
        assert_eq!(answer, "pong\n", "the dropped tick answered nothing, so the ping is the first reply");

        let text = wait_for(&nofront_log(), "unparseable");
        assert!(text.contains("unparseable"), "{text}");
    }

    /// A line the front declines and no verb claims is dropped and audited, and the
    /// connection stays open for the next line.
    #[cfg(unix)]
    #[test]
    fn a_line_the_front_declines_and_no_verb_claims_is_dropped_audited() {
        let sock = short_sock("declined");
        spawn_serve(&sock, &DECLINED_SOCKET);

        let mut client = connect(&sock);
        client.write_all(b"{\"cmd\":\"nobody\"}\n{\"cmd\":\"ping\"}\n").unwrap();
        client.flush().unwrap();
        let mut reader = BufReader::new(client);
        let mut answer = String::new();
        reader.read_line(&mut answer).unwrap();
        assert_eq!(answer, "pong\n", "the dropped line answered nothing and the connection stayed open");

        let text = wait_for(&declined_log(), "unparseable");
        assert_eq!(text.matches("\"status\":\"unparseable\"").count(), 1, "one drop, one audit: {text}");
    }

    #[test]
    fn the_cross_uid_floor_admits_this_process_and_refuses_anyone_else() {
        let own = own_user().expect("this process's own user");
        assert_eq!(cross_uid_gate(Some(PeerCred::for_user(&own))), None);

        let other = match own {
            PeerUser::Uid(uid) => PeerUser::Uid(uid + 1),
            PeerUser::Sid(sid) => PeerUser::Sid(format!("{sid}-not-this-user")),
        };
        assert!(cross_uid_gate(Some(PeerCred::for_user(&other))).is_some());
    }

    #[test]
    fn the_cross_uid_floor_fails_closed_on_an_unidentified_peer() {
        assert!(cross_uid_gate(None).is_some());
    }

    #[cfg(unix)]
    #[test]
    fn a_real_same_process_socketpair_is_admitted() {
        let (a, _b) = UnixStream::pair().expect("socketpair");
        assert_eq!(cross_uid_gate(peer_cred(&a)), None);
    }

    #[test]
    fn wire_cmd_names_a_line_and_nothing_else() {
        assert_eq!(wire_cmd("{\"cmd\":\"power\",\"x\":1}").as_deref(), Some("power"));
        assert_eq!(wire_cmd("not json"), None);
        assert_eq!(wire_cmd("{\"cmd\":3}"), None);
    }
}
