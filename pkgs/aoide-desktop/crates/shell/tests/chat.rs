#![cfg(feature = "fakes")]

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use aoide_desktop_shell::chat::cases::CASES;
use aoide_desktop_shell::chat::record::{self, AskOption, Push, Role, State};
use aoide_desktop_shell::chat::{follow, say, FollowError};
use aoide_desktop_shell::exec::Exec;
use serde_json::{json, Value};

const FAKE: &str = env!("CARGO_BIN_EXE_fake-aoide");
const PROTOCOL_FIXTURE: &str = include_str!("../../../../aoide/crates/protocol/tests/fixtures/chat-v1.jsonl");
const HOSTILE_FIXTURE: &str = include_str!("fixtures/chat-hostile.jsonl");
const ENVELOPE: &[u8] = br#"{"command":"session.chat","gated":false,"message":"chat closed","status":"ok"}"#;
const WAIT: Duration = Duration::from_secs(5);

/// A scratch dir for `tag`, holding the fake's follow file of `lines` and the
/// exec the fake loads with `extra` on top. Returns the exec and the record of
/// every call.
fn load(tag: &str, lines: &[&[u8]], extra: &[(&str, &str)]) -> (Exec, PathBuf) {
    let dir = std::env::temp_dir().join(format!("aoide-desk-chat-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("a scratch dir is made");
    let follow_file = dir.join("follow.txt");
    let mut bytes = Vec::new();
    for line in lines {
        bytes.extend_from_slice(line);
        bytes.push(b'\n');
    }
    fs::write(&follow_file, bytes).expect("the follow file is written");
    let record = dir.join("record.jsonl");
    let mut env = vec![
        (OsString::from("PATH"), OsString::from("/usr/bin:/bin")),
        (OsString::from("FAKE_AOIDE_RECORD"), record.clone().into_os_string()),
        (OsString::from("FAKE_AOIDE_FOLLOW_FILE"), follow_file.into_os_string()),
    ];
    env.extend(extra.iter().map(|(k, v)| (OsString::from(k), OsString::from(v))));
    let exec = Exec::load(Path::new(FAKE), &dir.join("home"), env).expect("the fake loads");
    (exec, record)
}

fn records(record: &Path) -> Vec<Value> {
    fs::read_to_string(record)
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).expect("a record line is JSON"))
        .collect()
}

fn argv_of(call: &Value) -> Vec<String> {
    call["args"].as_array().expect("the args are recorded").iter().map(|a| a.as_str().unwrap().to_owned()).collect()
}

fn env_names(call: &Value) -> Vec<String> {
    let mut names: Vec<String> = call["env"].as_object().expect("the env is recorded").keys().cloned().collect();
    names.sort();
    names
}

#[test]
fn say_sends_with_its_id_submit_and_yes_and_the_text_as_one_positional() {
    let (exec, record) = load("say", &[], &[]);
    say(&exec, "s1", "hi there").expect("the send runs");
    let last = records(&record).pop().expect("the send is recorded");
    assert_eq!(argv_of(&last), ["send", "--json", "--id=s1", "--submit", "--yes", "--", "hi there"]);
}

#[test]
fn text_that_starts_with_dashes_and_holds_a_newline_stays_one_positional() {
    let (exec, record) = load("say-text", &[], &[]);
    say(&exec, "s1", "--yes\nrm -rf").expect("the send runs");
    let args = argv_of(&records(&record).pop().expect("the send is recorded"));
    assert_eq!(args.len(), 7, "{args:?}");
    assert_eq!(args[6], "--yes\nrm -rf");
}

#[test]
fn say_scrubs_the_session_id_and_both_autogate_switches_from_the_send_child() {
    let (exec, record) = load(
        "say-env",
        &[],
        &[
            ("AOIDE_SESSION_ID", "sess-1"),
            ("AOIDE_CONDUCT_AUTOGATE", "1"),
            ("AOIDE_CONDUCT_SIBLING_AUTOGATE", "1"),
        ],
    );
    say(&exec, "s1", "hi").expect("the send runs");
    let names = env_names(&records(&record).pop().expect("the send is recorded"));
    for dropped in ["AOIDE_SESSION_ID", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_CONDUCT_SIBLING_AUTOGATE"] {
        assert!(!names.iter().any(|n| n == dropped), "{dropped} reaches the send child: {names:?}");
    }
}

#[test]
fn a_session_whose_chat_answers_is_followed_with_follow() {
    let (exec, record) = load("chat-args", &[b"{\"kind\":\"say\"}"], &[]);
    let mut chat = follow(&exec, "eidolon-1").expect("the chat answers");
    assert_eq!(chat.case(), "chat");
    assert_eq!(chat.next(WAIT), Some(Push::Record(json!({"kind": "say"}))));
    let args = argv_of(&records(&record).pop().expect("the chat is recorded"));
    assert_eq!(args, ["session", "chat", "--json", "--follow", "--", "eidolon-1"]);
    chat.close().expect("the stream ends");
}

#[test]
fn a_line_arrives_spelled_and_a_line_that_is_not_json_arrives_as_other() {
    let record_line = br#"{"kind":"say","text":"a\u202eb\u001b[31m"}"#;
    let text_line = "not json \u{1b}[0m \u{202e}".as_bytes();
    let (exec, _) = load("spelled", &[record_line, text_line], &[]);
    let mut chat = follow(&exec, "eidolon-1").expect("the chat answers");
    assert_eq!(
        chat.next(WAIT),
        Some(Push::Record(json!({"kind": "say", "text": "a\\u{202e}b\\x1b[31m"})))
    );
    assert_eq!(chat.next(WAIT), Some(Push::Other("not json \\x1b[0m \\u{202e}".to_owned())));
    chat.close().expect("the stream ends");
}

#[test]
fn the_envelope_a_follow_prints_at_its_end_arrives_as_the_end_and_the_reader_survives_it() {
    let (exec, _) = load("envelope", &[b"{\"kind\":\"say\"}"], &[]);
    let mut chat = follow(&exec, "eidolon-1").expect("the chat answers");
    assert!(chat.next(WAIT).is_some());
    let rest = chat.close().expect("the stream ends");
    assert_eq!(rest, [Push::End]);
}

#[test]
fn a_session_with_a_pty_and_no_trace_is_watched_and_its_frame_comes_first() {
    let (exec, _) = load("watch", &[b"plain line"], &[]);
    let mut chat = follow(&exec, "pty-1").expect("the watch answers");
    assert_eq!(chat.case(), "watch");
    assert_eq!(chat.next(WAIT), Some(Push::Record(json!({"frame": "live", "id": "pty-1"}))));
    assert_eq!(chat.next(WAIT), Some(Push::Other("plain line".to_owned())));
    chat.close().expect("the stream ends");
}

#[test]
fn a_hook_only_session_is_refused_and_each_case_says_why() {
    let (exec, _) = load("refused", &[], &[]);
    match follow(&exec, "hook-1") {
        Err(FollowError::Refused(refused)) => {
            let cases: Vec<&str> = refused.reasons.iter().map(|r| r.case).collect();
            assert_eq!(cases, ["chat", "watch"]);
            assert!(refused.reasons.iter().all(|r| r.why.contains("keeps no stream")), "{refused:?}");
        }
        Err(other) => panic!("the session is refused as a session, not an exec error: {other}"),
        Ok(_) => panic!("a hook-only session is followed"),
    }
}

#[test]
fn every_case_runs_a_verb_the_schema_declares() {
    let (exec, _) = load("registered", &[], &[]);
    let mut names: Vec<&str> = CASES.iter().map(|c| c.name).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), CASES.len(), "each case is named once");
    for case in CASES {
        assert!(exec.schema().command(case.words).is_some(), "the schema declares `{}`", case.words.join(" "));
    }
}

#[test]
fn the_chat_record_is_parsed_in_one_module_only() {
    const FORMS: &[&str] = &["from_str", "from_slice", "from_reader", "from_value", "Deserializer"];
    let chat = Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("chat");
    for entry in fs::read_dir(&chat).expect("the chat module is read") {
        let file = entry.expect("an entry").path();
        let text = fs::read_to_string(&file).expect("a source is read");
        let parser = file.file_name().is_some_and(|name| name == "record.rs");
        for form in FORMS {
            assert!(
                parser || !text.contains(form),
                "{} parses JSON with `{form}` outside record.rs",
                file.display()
            );
        }
    }
}

fn lines(text: &str) -> Vec<Push> {
    text.lines().map(record::line).collect()
}

fn line_of(push: &Push) -> &record::Line {
    match push {
        Push::Line(line) => line,
        other => panic!("a chat line is expected, not {other:?}"),
    }
}

#[test]
fn the_protocol_fixture_arrives_as_lines_with_the_fields_the_page_renders() {
    let pushes = lines(PROTOCOL_FIXTURE);
    let ids: Vec<&str> = pushes.iter().map(|p| line_of(p).id.as_str()).collect();
    assert_eq!(ids, ["1", "2", "2.0", "2.1", "3", "4", "5", "6", "7", "8", "9"]);
    let at = |n: usize| line_of(&pushes[n]);
    assert_eq!((at(0).role, at(0).text.as_deref()), (Role::User, Some("hello <script>")));
    assert_eq!((at(1).role, at(1).thinking), (Role::Agent, false));
    assert!(at(2).thinking, "the thinking flag is kept");
    let call = at(3).tool.as_ref().expect("a call");
    assert_eq!((call.name.as_str(), call.input.as_deref(), call.result.as_deref()), ("read", Some("{\"path\":\"slots.md\"}"), None));
    assert!(at(5).tool.as_ref().expect("a result").error, "a failed result says so");
    assert_eq!((at(6).role, at(6).from.as_deref()), (Role::System, Some("peer-7")));
    assert_eq!(at(7).state, Some(State::Waiting));
    let approval = at(8).ask.as_ref().expect("an approval");
    assert_eq!((approval.kind.as_str(), approval.answers.as_slice()), ("approval", ["yes".to_owned(), "no".to_owned()].as_slice()));
    let question = at(9).ask.as_ref().expect("a question");
    assert_eq!(question.options[1], AskOption { label: "dusk".to_owned(), description: "cool palette".to_owned() });
    assert_eq!(at(10).ask.as_ref().expect("a permission ask").fingerprint.as_deref(), Some("3f9a"));
}

#[test]
fn an_unknown_role_is_system_and_an_unknown_field_is_ignored() {
    let pushes = lines(r#"{"v":1,"id":"x","role":"robot","text":"hi","future":{"a":1},"state":"asleep"}"#);
    let line = line_of(&pushes[0]);
    assert_eq!((line.role, line.text.as_deref(), line.state), (Role::System, Some("hi"), None));
}

#[test]
fn an_object_that_is_no_v1_line_stays_a_record() {
    for text in [r#"{"v":2,"id":"x","role":"agent"}"#, r#"{"v":1,"role":"agent"}"#, r#"{"v":1,"id":"x"}"#, r#"{"id":"x","role":"agent"}"#] {
        assert!(matches!(record::line(text), Push::Record(_)), "{text}");
    }
}

#[test]
fn hostile_text_in_every_string_of_a_line_is_spelled_and_left_as_text() {
    let pushes = lines(HOSTILE_FIXTURE);
    assert_eq!(pushes.len(), 3);
    for push in &pushes {
        let wire = serde_json::to_string(line_of(push)).expect("a line serializes");
        assert!(!wire.contains('\u{202e}') && !wire.contains('\u{1b}'), "an escape reaches the page: {wire}");
        assert!(wire.contains("\\u{202e}") || wire.contains("\\x1b"), "the escape is spelled: {wire}");
    }
    assert_eq!(line_of(&pushes[0]).text.as_deref(), Some("a\\u{202e}b \\x1b[31mred <img src=x onerror=alert(1)>"));
    assert_eq!(line_of(&pushes[1]).from.as_deref(), Some("peer\\u{202e}7"));
    assert_eq!(line_of(&pushes[1]).tool.as_ref().expect("a tool").name, "re\\x1bad");
    let ask = line_of(&pushes[2]).ask.as_ref().expect("an ask");
    assert_eq!(ask.prompt, "ok\\u{202e}?");
    assert_eq!(ask.answers, ["y\\x1b[0m"]);
    assert_eq!(ask.options[0].label, "<img src=x onerror=alert(1)>");
}

#[test]
fn a_line_repeating_an_id_arrives_once() {
    let one = br#"{"v":1,"id":"1","role":"agent","text":"a"}"#;
    let again = br#"{"v":1,"id":"1","role":"agent","text":"a again"}"#;
    let two = br#"{"v":1,"id":"2","role":"agent","text":"b"}"#;
    let (exec, _) = load("dedupe", &[one, again, two], &[]);
    let mut chat = follow(&exec, "eidolon-1").expect("the chat answers");
    let first = chat.next(WAIT).expect("the first line");
    let second = chat.next(WAIT).expect("the next line");
    assert_eq!((line_of(&first).id.as_str(), line_of(&second).id.as_str()), ("1", "2"));
    chat.close().expect("the stream ends");
}

#[test]
fn nothing_follows_the_closing_envelope() {
    let one = br#"{"v":1,"id":"1","role":"agent","text":"a"}"#;
    let late = br#"{"v":1,"id":"2","role":"agent","text":"late"}"#;
    let (exec, _) = load("stop", &[one, ENVELOPE, late], &[]);
    let mut chat = follow(&exec, "eidolon-1").expect("the chat answers");
    assert!(matches!(chat.next(WAIT), Some(Push::Line(_))));
    assert_eq!(chat.next(WAIT), Some(Push::End));
    assert_eq!(chat.next(WAIT), None, "the line after the envelope is never read out");
    assert_eq!(chat.close().expect("the stream ends"), [], "and none comes out at the close");
}

#[test]
fn a_blank_line_on_the_stream_is_never_pushed() {
    let (exec, _) = load("blank", &[b"{\"kind\":\"say\"}", b"", b"   ", b"not json"], &[]);
    let mut chat = follow(&exec, "eidolon-1").expect("the chat answers");
    assert_eq!(chat.next(WAIT), Some(Push::Record(json!({"kind": "say"}))));
    assert_eq!(chat.next(WAIT), Some(Push::Other("not json".to_owned())), "blank lines are dropped, not pushed as Other");
}

#[test]
fn a_line_that_is_not_utf8_arrives_as_other_and_a_line_past_the_cap_as_dropped() {
    let not_utf8: &[u8] = b"\xff\xfe{\"kind\":\"say\"}";
    let long = vec![b'a'; 1024 * 1024 + 10];
    let (exec, _) = load("bytes", &[not_utf8, &long, br#"{"kind":"say"}"#], &[]);
    let mut chat = follow(&exec, "eidolon-1").expect("the chat answers");
    assert!(matches!(chat.next(WAIT), Some(Push::Other(_))), "a non-UTF-8 line is text");
    assert_eq!(chat.next(WAIT), Some(Push::Dropped(1024 * 1024 + 10)), "a line past the cap is dropped, in bytes");
    assert_eq!(chat.next(WAIT), Some(Push::Record(json!({"kind": "say"}))), "the stream goes on");
    chat.close().expect("the stream ends");
}
