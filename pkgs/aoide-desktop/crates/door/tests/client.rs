#![cfg(feature = "fake")]

use std::fs::{self, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use aoide_desktop_door::fake::{FakeDoor, MAX_BODY};
use aoide_desktop_door::{AnswerOutcome, Ask, CancelOutcome, Door, DoorError, Frame, Mode, SayOutcome};
use serde_json::{json, Value};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// A runtime directory the test owns, removed with everything in it.
struct Runtime(PathBuf);

impl Runtime {
    fn new(tag: &str) -> Runtime {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("aoide-door-client-{tag}-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::set_permissions(&root, Permissions::from_mode(0o700)).unwrap();
        Runtime(root)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn fixture(tag: &str) -> (Runtime, FakeDoor, Door) {
    let rt = Runtime::new(tag);
    let fake = FakeDoor::start(rt.path(), "tok-123");
    let door = Door::new(fake.addr(), rt.path(), &fake.token_file()).unwrap();
    (rt, fake, door)
}

/// A token file for a door this test runs on `port` itself, made as `FakeDoor`
/// makes one: `runtime/eidolon` at 0700, the file at 0600.
fn own_token(rt: &Runtime, port: u16, token: &str) -> PathBuf {
    let eidolon = rt.path().join("eidolon");
    fs::create_dir_all(&eidolon).unwrap();
    fs::set_permissions(&eidolon, Permissions::from_mode(0o700)).unwrap();
    let file = eidolon.join(format!("web-{port}.token"));
    fs::write(&file, token).unwrap();
    fs::set_permissions(&file, Permissions::from_mode(0o600)).unwrap();
    file
}

fn body_json(body: &[u8]) -> Value {
    serde_json::from_slice(body).expect("the request body is json")
}

#[test]
fn a_non_loopback_address_is_refused() {
    let rt = Runtime::new("nonloop");
    let token = rt.path().join("x.token");
    for addr in ["192.0.2.1:9", "10.0.0.1:80", "[2001:db8::1]:9"] {
        let err = Door::new(addr.parse().unwrap(), rt.path(), &token).err().expect("refused");
        assert!(matches!(err, DoorError::NotLoopback(_)), "{addr}: {err}");
    }
    assert!(Door::new("127.5.5.5:4477".parse().unwrap(), rt.path(), &token).is_ok());
    assert!(Door::new("[::1]:4477".parse().unwrap(), rt.path(), &token).is_ok());
}

#[test]
fn a_stale_token_maps_to_unauthorized_with_no_body_read() {
    let (_rt, fake, door) = fixture("401");
    fake.set_token_file("not-the-token");
    assert!(matches!(door.say("hi", Mode::Send), Err(DoorError::Unauthorized)));
    assert!(matches!(door.cancel(), Err(DoorError::Unauthorized)));
}

#[test]
fn a_403_maps_to_host_refused() {
    let (_rt, fake, door) = fixture("403");
    fake.reply("/api/say", 403, r#"{"error":"Host does not match the bound address"}"#);
    assert!(matches!(door.say("hi", Mode::Send), Err(DoorError::HostRefused)));
}

#[test]
fn a_body_over_the_door_cap_maps_to_too_large() {
    let (_rt, _fake, door) = fixture("413");
    let big = "x".repeat(MAX_BODY);
    assert!(matches!(door.say(&big, Mode::Send), Err(DoorError::TooLarge)));
}

#[test]
fn say_reads_queued_from_its_202_and_refused_from_its_422() {
    let (_rt, fake, door) = fixture("say-status");
    assert_eq!(door.say("hi", Mode::Send).unwrap(), SayOutcome::Queued { queued: false });
    fake.reply("/api/say", 202, r#"{"queued":true}"#);
    assert_eq!(door.say("again", Mode::Steer).unwrap(), SayOutcome::Queued { queued: true });
    fake.reply("/api/say", 422, r#"{"error":"image is malformed"}"#);
    assert_eq!(
        door.say("pic", Mode::Send).unwrap(),
        SayOutcome::Refused { reason: "image is malformed".into() }
    );
}

#[test]
fn say_sends_its_text_and_its_mode_in_both_modes() {
    let (_rt, fake, door) = fixture("say-modes");
    door.say("hello there", Mode::Send).unwrap();
    door.say("nudge", Mode::Steer).unwrap();
    let said: Vec<Value> = fake
        .requests()
        .iter()
        .filter(|r| r.path == "/api/say")
        .map(|r| body_json(&r.body))
        .collect();
    assert_eq!(said, vec![json!({"text": "hello there", "mode": "send"}), json!({"text": "nudge", "mode": "steer"})]);
}

#[test]
fn cancel_maps_204_to_cancelled_and_409_to_idle() {
    let (_rt, fake, door) = fixture("cancel");
    assert_eq!(door.cancel().unwrap(), CancelOutcome::Cancelled);
    fake.reply("/api/cancel", 409, r#"{"error":"no turn is running"}"#);
    assert_eq!(door.cancel().unwrap(), CancelOutcome::Idle);
}

#[test]
fn answer_maps_204_409_and_422() {
    let (_rt, fake, door) = fixture("answer");
    assert_eq!(door.answer(3, "yes", None).unwrap(), AnswerOutcome::Answered);
    fake.reply("/api/answer", 409, r#"{"error":"that ask is no longer pending"}"#);
    assert_eq!(door.answer(3, "yes", None).unwrap(), AnswerOutcome::NotPending);
    fake.reply("/api/answer", 422, r#"{"error":"that ask does not offer that answer"}"#);
    assert_eq!(
        door.answer(3, "maybe", None).unwrap(),
        AnswerOutcome::Rejected { reason: "that ask does not offer that answer".into() }
    );
}

#[test]
fn the_answer_body_is_exactly_ask_id_and_answer_plus_note_when_given() {
    let (_rt, fake, door) = fixture("answer-body");
    door.answer(3, "yes", None).unwrap();
    door.answer(4, "no", Some("not on main")).unwrap();
    let sent: Vec<Value> = fake
        .requests()
        .iter()
        .filter(|r| r.path == "/api/answer")
        .map(|r| body_json(&r.body))
        .collect();
    assert_eq!(sent[0], json!({"ask_id": 3, "answer": "yes"}));
    assert_eq!(sent[1], json!({"ask_id": 4, "answer": "no", "note": "not on main"}));
}

#[test]
fn a_status_a_route_does_not_take_is_unexpected() {
    let (_rt, fake, door) = fixture("unexpected");
    fake.reply("/api/cancel", 500, "{}");
    assert!(matches!(door.cancel(), Err(DoorError::Unexpected { status: 500 })));
    fake.reply("/api/answer", 400, "{}");
    assert!(matches!(door.answer(1, "yes", None), Err(DoorError::Unexpected { status: 400 })));
}

#[test]
fn events_decode_the_head_the_pushed_frames_and_the_goodbye() {
    let (_rt, fake, door) = fixture("events");
    fake.set_head(&[
        r#"{"type":"hello","session":"s1","cwd":"/w","model":"m","yolo":false,"pending":1,"protocol":1}"#,
        r#"{"type":"text-delta","text":"hi"}"#,
        r#"{"type":"ask","kind":"question","ask_id":4,"prompt":"pick","options":[{"label":"a"},{"label":"b"}]}"#,
    ]);
    let mut events = door.events().unwrap();
    let hello = events.next().unwrap().unwrap();
    assert!(matches!(hello, Frame::Hello { ref session, pending: 1, protocol: 1, .. } if session == "s1"), "{hello:?}");
    assert_eq!(events.next().unwrap().unwrap(), Frame::TextDelta { text: "hi".into() });
    assert!(matches!(events.next().unwrap().unwrap(), Frame::Ask(Ask::Question { ask_id: 4, .. })));
    assert_eq!(events.next().unwrap().unwrap(), Frame::CaughtUp);
    fake.push(r#"{"type":"turn-state","running":true}"#);
    assert_eq!(events.next().unwrap().unwrap(), Frame::TurnState { running: true });
    fake.push(r#"{"type":"ask-settled","ask_id":4,"how":"answered","answer":"a"}"#);
    assert_eq!(
        events.next().unwrap().unwrap(),
        Frame::AskSettled { ask_id: 4, how: "answered".into(), answer: Some("a".into()), note: None }
    );
    fake.push(r#"{"type":"goodbye","text":"closing"}"#);
    assert_eq!(events.next().unwrap().unwrap(), Frame::Goodbye { text: "closing".into() });
    fake.end();
    assert!(events.next().is_none());
}

#[test]
fn a_protocol_2_hello_ends_the_stream_with_a_taught_error() {
    let (_rt, fake, door) = fixture("protocol");
    fake.set_head(&[r#"{"type":"hello","session":"s","cwd":"/","model":"m","yolo":false,"pending":0,"protocol":2}"#]);
    let mut events = door.events().unwrap();
    let first = events.next().expect("one item");
    assert!(matches!(&first, Err(DoorError::Protocol { got }) if *got == json!(2)), "{first:?}");
    assert!(events.next().is_none(), "the stream ends behind the refusal");
}

#[test]
fn an_unknown_frame_and_an_unknown_field_arrive_as_data_and_do_not_end_the_stream() {
    let (_rt, fake, door) = fixture("unknown");
    let mut events = door.events().unwrap();
    assert!(matches!(events.next().unwrap().unwrap(), Frame::Hello { .. }));
    assert_eq!(events.next().unwrap().unwrap(), Frame::CaughtUp);
    fake.push(r#"{"type":"future-frame","n":1}"#);
    match events.next().unwrap().unwrap() {
        Frame::Other { kind, json } => {
            assert_eq!(kind, "future-frame");
            assert_eq!(json, json!({"type": "future-frame", "n": 1}));
        }
        other => panic!("expected Other, got {other:?}"),
    }
    fake.push(r#"{"type":"turn-state","running":true,"later":"x"}"#);
    assert_eq!(events.next().unwrap().unwrap(), Frame::TurnState { running: true });
}

#[test]
fn a_2_mib_frame_is_dropped_and_counted_and_the_next_one_decodes() {
    let (_rt, fake, door) = fixture("oversize");
    let mut events = door.events().unwrap();
    assert!(matches!(events.next().unwrap().unwrap(), Frame::Hello { .. }));
    assert_eq!(events.next().unwrap().unwrap(), Frame::CaughtUp);
    fake.push(&format!(r#"{{"type":"text-delta","text":"{}"}}"#, "x".repeat(2 << 20)));
    fake.push(r#"{"type":"turn-state","running":false}"#);
    assert_eq!(events.next().unwrap().unwrap(), Frame::TurnState { running: false });
    assert_eq!(events.dropped_oversize(), 1);
}

#[test]
fn the_client_sends_four_routes_with_no_query_token_and_the_bound_host() {
    let (_rt, fake, door) = fixture("record");
    door.say("hi", Mode::Send).unwrap();
    door.cancel().unwrap();
    door.answer(1, "yes", None).unwrap();
    let mut events = door.events().unwrap();
    assert!(matches!(events.next().unwrap().unwrap(), Frame::Hello { .. }));
    assert_eq!(events.next().unwrap().unwrap(), Frame::CaughtUp);
    fake.end();
    assert!(events.next().is_none());
    let requests = fake.requests();
    assert_eq!(requests.len(), 4);
    let known = ["/api/say", "/api/cancel", "/api/answer", "/api/events"];
    let host = fake.addr().to_string();
    for request in &requests {
        assert!(known.contains(&request.path.as_str()), "an unexpected route: {}", request.path);
        assert!(request.query.is_empty(), "no ?token= on {}: {}", request.path, request.query);
        assert_eq!(request.header("Host"), Some(host.as_str()));
        assert_eq!(request.header("Connection"), Some("close"));
        assert_eq!(request.header("Authorization"), Some("Bearer tok-123"));
    }
}

#[test]
fn a_token_file_not_at_0600_is_refused_before_any_request() {
    let (_rt, fake, door) = fixture("token-mode");
    fs::set_permissions(fake.token_file(), Permissions::from_mode(0o644)).unwrap();
    assert!(matches!(door.say("hi", Mode::Send), Err(DoorError::Refused { .. })));
    assert!(fake.requests().is_empty(), "A6 refuses before anything is sent");
}

const HELLO: &str = r#"{"type":"hello","session":"/s/t.eid","cwd":"/c","model":"m","yolo":false,"pending":0,"protocol":1}"#;

/// Polls `ready` until it holds or `within` passes. The fake records its stream
/// on its own thread, so a test waits for that instead of guessing a sleep.
fn wait_until(within: Duration, ready: impl Fn() -> bool) -> bool {
    let end = Instant::now() + within;
    while Instant::now() < end {
        if ready() {
            return true;
        }
        thread::sleep(Duration::from_millis(5));
    }
    ready()
}

#[test]
fn a_closer_ends_a_read_blocked_in_another_thread_within_two_seconds() {
    let (_rt, fake, door) = fixture("closer");
    fake.set_head(&[HELLO]);
    let mut events = door.events().unwrap();
    let closer = events.closer().unwrap();
    let (head_read, head) = mpsc::channel();
    let (done, ended) = mpsc::channel();
    thread::spawn(move || {
        let read: Vec<_> = events.by_ref().take(2).collect();
        let _ = head_read.send(read.len());
        let rest = events.count();
        let _ = done.send(rest);
    });
    assert_eq!(head.recv_timeout(Duration::from_secs(5)).expect("hello and caught-up arrive"), 2);
    closer.close().unwrap();
    ended.recv_timeout(Duration::from_secs(5)).expect("the blocked read returns within 5 s of close");
}

#[test]
fn streaming_is_true_while_a_client_holds_the_stream_and_false_within_five_seconds_of_its_close() {
    let (_rt, fake, door) = fixture("streaming");
    fake.set_head(&[HELLO]);
    let events = door.events().unwrap();
    let closer = events.closer().unwrap();
    assert!(wait_until(Duration::from_secs(5), || fake.streaming()), "the stream opens");
    assert!(fake.streaming(), "still open while the client holds it");
    closer.close().unwrap();
    assert!(wait_until(Duration::from_secs(5), || !fake.streaming()), "the fake sees the client close");
    drop(events);
}

/// `n` header lines, each a distinct name with `value`.
fn many_headers(n: usize, value: &str) -> String {
    (0..n).map(|i| format!("X-Filler-{i}: {value}\r\n")).collect()
}

const QUEUED: &str = r#"{"queued":true}"#;

/// A 202 for `say`: `headers` before its Content-Length, then `body`.
fn say_reply(headers: &str, body: &str) -> String {
    format!("HTTP/1.1 202 Accepted\r\n{headers}Content-Length: {}\r\n\r\n{body}", body.len())
}

#[test]
fn a_head_of_100_header_lines_is_read() {
    let (_rt, fake, door) = fixture("head-100");
    let head = say_reply(&many_headers(99, "v"), QUEUED);
    fake.reply_raw("/api/say", &[(Duration::ZERO, head.as_bytes())], true);
    assert_eq!(door.say("hi", Mode::Send).unwrap(), SayOutcome::Queued { queued: true });
}

#[test]
fn a_head_of_more_than_100_header_lines_is_malformed() {
    let (_rt, fake, door) = fixture("head-101");
    let head = say_reply(&many_headers(100, "v"), QUEUED);
    fake.reply_raw("/api/say", &[(Duration::ZERO, head.as_bytes())], true);
    assert!(matches!(door.say("hi", Mode::Send), Err(DoorError::Malformed(_))));
}

#[test]
fn a_head_over_64_kib_is_malformed_even_within_100_lines() {
    let (_rt, fake, door) = fixture("head-bytes");
    let head = say_reply(&many_headers(40, &"v".repeat(2000)), QUEUED);
    fake.reply_raw("/api/say", &[(Duration::ZERO, head.as_bytes())], true);
    assert!(matches!(door.say("hi", Mode::Send), Err(DoorError::Malformed(_))));
}

#[test]
fn an_error_status_whose_body_is_cut_short_keeps_its_status() {
    let (_rt, fake, door) = fixture("413-cut");
    let head = "HTTP/1.1 413 Payload Too Large\r\nContent-Length: 100\r\n\r\n{\"error\"";
    fake.reply_raw("/api/say", &[(Duration::ZERO, head.as_bytes())], true);
    assert!(matches!(door.say("hi", Mode::Send), Err(DoorError::TooLarge)));
}

#[test]
fn trailers_past_the_cap_are_malformed() {
    let (_rt, fake, door) = fixture("trailers");
    let head = format!(
        "HTTP/1.1 202 Accepted\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{QUEUED}\r\n0\r\n{}\r\n",
        QUEUED.len(),
        many_headers(101, "t"),
    );
    fake.reply_raw("/api/say", &[(Duration::ZERO, head.as_bytes())], true);
    assert!(matches!(door.say("hi", Mode::Send), Err(DoorError::Malformed(_))));
}

#[test]
fn a_chunk_size_that_overflows_is_malformed() {
    let (_rt, fake, door) = fixture("chunk-overflow");
    let head = "HTTP/1.1 202 Accepted\r\nTransfer-Encoding: chunked\r\n\r\nFFFFFFFFFFFFFFFFF\r\n{}\r\n0\r\n\r\n";
    fake.reply_raw("/api/say", &[(Duration::ZERO, head.as_bytes())], true);
    assert!(matches!(door.say("hi", Mode::Send), Err(DoorError::Malformed(_))));
}

#[test]
fn a_chunked_body_cut_inside_a_chunk_is_malformed() {
    let (_rt, fake, door) = fixture("chunk-cut");
    let head = "HTTP/1.1 202 Accepted\r\nTransfer-Encoding: chunked\r\n\r\n14\r\n{\"queued\"";
    fake.reply_raw("/api/say", &[(Duration::ZERO, head.as_bytes())], true);
    assert!(matches!(door.say("hi", Mode::Send), Err(DoorError::Malformed(_))));
}

#[test]
fn a_body_shorter_than_its_content_length_is_malformed() {
    let (_rt, fake, door) = fixture("short-body");
    let head = format!("HTTP/1.1 202 Accepted\r\nContent-Length: {}\r\n\r\n{QUEUED}", QUEUED.len() + 5);
    fake.reply_raw("/api/say", &[(Duration::ZERO, head.as_bytes())], true);
    assert!(matches!(door.say("hi", Mode::Send), Err(DoorError::Malformed(_))));
}

#[test]
fn a_huge_content_length_is_refused_before_its_body_is_read() {
    let (_rt, fake, door) = fixture("huge-length");
    let head = "HTTP/1.1 202 Accepted\r\nContent-Length: 1000000000000\r\n\r\n";
    fake.reply_raw("/api/say", &[(Duration::ZERO, head.as_bytes())], false);
    let started = Instant::now();
    assert!(matches!(door.say("hi", Mode::Send), Err(DoorError::Malformed(_))));
    assert!(started.elapsed() < Duration::from_secs(2), "refused, not read to the deadline");
}

#[test]
fn a_413_sent_at_once_before_the_body_is_read_is_too_large() {
    let (_rt, fake, door) = fixture("413-at-once");
    fake.refuse_at_once("/api/say", 413, r#"{"error":"a body over 8388608 bytes is not read"}"#);
    let big = "x".repeat(32 << 20);
    match door.say(&big, Mode::Send) {
        Err(DoorError::TooLarge) => {}
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_413_that_declares_a_huge_length_is_too_large() {
    let (_rt, fake, door) = fixture("413-huge");
    let head = "HTTP/1.1 413 Payload Too Large\r\nContent-Length: 1000000000000\r\n\r\n";
    fake.reply_raw("/api/say", &[(Duration::ZERO, head.as_bytes())], true);
    assert!(matches!(door.say("hi", Mode::Send), Err(DoorError::TooLarge)));
}

/// A door that accepts, takes the request's head, and never reads its body: the
/// send cannot finish, so the call is refused within one deadline from the send's
/// first byte. The body's encoding happens before the send and is not counted.
#[test]
fn a_send_nobody_reads_is_refused_within_one_deadline() {
    use std::io::{BufRead, BufReader};

    let rt = Runtime::new("send-deadline");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let token = own_token(&rt, addr.port(), "tok-123");
    let (head_at, head_seen) = mpsc::channel();
    thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(&stream);
        let mut line = String::new();
        loop {
            line.clear();
            if reader.read_line(&mut line).unwrap() <= 2 {
                break;
            }
        }
        let _ = head_at.send(Instant::now());
        thread::sleep(Duration::from_secs(10));
    });
    let door = Door::new(addr, rt.path(), &token).unwrap().with_timeout(Duration::from_secs(2));
    let big = "x".repeat(32 << 20);
    assert!(door.say(&big, Mode::Send).is_err(), "a send nobody reads is refused");
    let refused_at = Instant::now();
    let sent_at = head_seen.recv_timeout(Duration::from_secs(5)).expect("the head arrives");
    let took = refused_at.duration_since(sent_at);
    assert!(took < Duration::from_secs(3), "refused {took:?} after the send began, over 2 s plus 1 s");
}

#[test]
fn the_events_head_has_a_deadline() {
    let (_rt, fake, door) = fixture("events-head");
    let door = door.with_timeout(Duration::from_millis(300));
    let head = b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n";
    fake.reply_raw("/api/events", &[(Duration::ZERO, head.as_slice())], false);
    let started = Instant::now();
    assert!(door.events().is_err(), "a head that never completes is refused");
    assert!(started.elapsed() < Duration::from_secs(2), "refused at the deadline");
}

#[test]
fn the_events_body_has_no_deadline_once_its_head_is_read() {
    let (_rt, fake, door) = fixture("events-body");
    let door = door.with_timeout(Duration::from_millis(300));
    let hello = format!("data: {HELLO}\n\n");
    let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n";
    let first = format!("{:x}\r\n{hello}\r\n", hello.len());
    let pieces: [(Duration, &[u8]); 3] = [
        (Duration::ZERO, head.as_bytes()),
        (Duration::ZERO, first.as_bytes()),
        (Duration::from_millis(900), b"0\r\n\r\n"),
    ];
    fake.reply_raw("/api/events", &pieces, true);
    let mut events = door.events().unwrap();
    assert!(matches!(events.next(), Some(Ok(Frame::Hello { .. }))));
    assert!(events.next().is_none(), "idle for three deadlines, then a clean end");
}

#[test]
fn a_reply_has_one_deadline_across_its_head_and_body() {
    let (_rt, fake, door) = fixture("total");
    let door = door.with_timeout(Duration::from_millis(500));
    let head = "HTTP/1.1 202 Accepted\r\nContent-Length: 100\r\n\r\n";
    let mut pieces: Vec<(Duration, &[u8])> = vec![(Duration::ZERO, head.as_bytes())];
    pieces.extend((0..10).map(|_| (Duration::from_millis(150), b"x".as_slice())));
    fake.reply_raw("/api/say", &pieces, false);
    let started = Instant::now();
    assert!(matches!(door.say("hi", Mode::Send), Err(DoorError::Io(_))));
    assert!(started.elapsed() < Duration::from_millis(1200), "one total deadline, not one per read");
}

#[test]
fn a_first_frame_that_is_not_hello_is_malformed_and_ends_the_stream() {
    let (_rt, fake, door) = fixture("first-not-hello");
    fake.set_head(&[r#"{"type":"text-delta","text":"hi"}"#]);
    let mut events = door.events().unwrap();
    assert!(matches!(events.next(), Some(Err(DoorError::Malformed(_)))));
    assert!(events.next().is_none(), "the stream ends behind it");
}

#[test]
fn a_second_hello_is_malformed_and_ends_the_stream() {
    let (_rt, fake, door) = fixture("second-hello");
    fake.set_head(&[HELLO, HELLO]);
    let mut events = door.events().unwrap();
    assert!(matches!(events.next(), Some(Ok(Frame::Hello { .. }))));
    assert!(matches!(events.next(), Some(Err(DoorError::Malformed(_)))));
    assert!(events.next().is_none(), "the stream ends behind it");
}

#[test]
fn a_protocol_that_is_not_a_number_is_a_protocol_error() {
    let (_rt, fake, door) = fixture("protocol-string");
    let hello = r#"{"type":"hello","session":"s","cwd":"/","model":"m","yolo":false,"pending":0,"protocol":"1"}"#;
    fake.set_head(&[hello]);
    let mut events = door.events().unwrap();
    assert!(matches!(events.next(), Some(Err(DoorError::Protocol { .. }))));
}

#[test]
fn a_protocol_beyond_u32_is_a_protocol_error() {
    let (_rt, fake, door) = fixture("protocol-big");
    let hello = r#"{"type":"hello","session":"s","cwd":"/","model":"m","yolo":false,"pending":0,"protocol":4294967297}"#;
    fake.set_head(&[hello]);
    let mut events = door.events().unwrap();
    assert!(matches!(events.next(), Some(Err(DoorError::Protocol { .. }))));
}

#[test]
fn a_frame_nested_10000_deep_is_malformed_and_does_not_overflow_the_stack() {
    let (_rt, fake, door) = fixture("deep");
    let mut events = door.events().unwrap();
    assert!(matches!(events.next(), Some(Ok(Frame::Hello { .. }))));
    assert_eq!(events.next().unwrap().unwrap(), Frame::CaughtUp);
    fake.push(&format!("{}{}", "[".repeat(10_000), "]".repeat(10_000)));
    assert!(matches!(events.next(), Some(Err(DoorError::Malformed(_)))));
}

#[test]
fn a_frame_that_is_not_utf8_is_malformed() {
    let (_rt, fake, door) = fixture("utf8");
    let mut events = door.events().unwrap();
    assert!(matches!(events.next(), Some(Ok(Frame::Hello { .. }))));
    assert_eq!(events.next().unwrap().unwrap(), Frame::CaughtUp);
    fake.push_bytes(b"{\"type\":\"text-delta\",\"text\":\"\xff\"}");
    assert!(matches!(events.next(), Some(Err(DoorError::Malformed(_)))));
}

#[test]
fn an_events_body_shorter_than_its_content_length_ends_in_malformed() {
    let (_rt, fake, door) = fixture("events-short");
    let hello = format!("data: {HELLO}\n\n");
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\n\r\n",
        hello.len() + 40
    );
    let pieces: [(Duration, &[u8]); 2] = [(Duration::ZERO, head.as_bytes()), (Duration::ZERO, hello.as_bytes())];
    fake.reply_raw("/api/events", &pieces, true);
    let mut events = door.events().unwrap();
    assert!(matches!(events.next(), Some(Ok(Frame::Hello { .. }))));
    assert!(matches!(events.next(), Some(Err(DoorError::Malformed(_)))));
}
