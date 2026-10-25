#![cfg(feature = "fake")]

use std::fs::{self, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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
    assert!(matches!(first, Err(DoorError::Protocol { got: 2 })), "{first:?}");
    assert!(events.next().is_none(), "the stream ends behind the refusal");
}

#[test]
fn an_unknown_frame_and_an_unknown_field_arrive_as_data_and_do_not_end_the_stream() {
    let (_rt, fake, door) = fixture("unknown");
    let mut events = door.events().unwrap();
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
