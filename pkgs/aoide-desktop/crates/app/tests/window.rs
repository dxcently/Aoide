use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use aoide_desktop_app::{AskPush, AskSink, Confirm, Push, Sink, Window};
use aoide_desktop_shell::exec::Exec;

const WAIT: Duration = Duration::from_secs(5);
const ASKS: &str = include_str!("../../shell/tests/fixtures/ask-watch.jsonl");

/// The shell's fake aoide, built beside this test binary by the run's script.
fn fake_aoide() -> PathBuf {
    let exe = std::env::current_exe().expect("the test binary has a path");
    exe.parent()
        .and_then(Path::parent)
        .expect("the test binary sits in target/<profile>/deps")
        .join("fake-aoide")
}

/// Answers every confirm with `answer` and counts the asks.
#[derive(Default)]
struct Answer {
    answer: bool,
    asked: Mutex<Vec<String>>,
}

impl Confirm for Answer {
    fn confirm(&self, verb: &str, _text: &str) -> bool {
        self.asked.lock().unwrap().push(verb.to_owned());
        self.answer
    }
}

/// Holds each confirm until a release is sent, and reports when one is asked.
struct Hold {
    entered: Mutex<Sender<()>>,
    release: Mutex<Receiver<()>>,
}

impl Confirm for Hold {
    fn confirm(&self, _verb: &str, _text: &str) -> bool {
        self.entered.lock().unwrap().send(()).unwrap();
        self.release.lock().unwrap().recv().unwrap();
        true
    }
}

struct Fixture {
    window: Window,
    record: PathBuf,
    baseline: usize,
}

/// A window over the fake aoide, its follow file holding `lines`, its record
/// of every call in a scratch dir for `tag`. `baseline` counts the calls
/// `Exec::load` made, which the tests do not count.
fn fixture(tag: &str, lines: &[&[u8]], confirm: Box<dyn Confirm>) -> Fixture {
    fixture_with(tag, lines, confirm, &[])
}

/// A session listing as `session find --all --json` prints it: `(id, agent, kind)`.
fn listing(rows: &[(&str, &str, &str)]) -> String {
    let rows: Vec<_> = rows
        .iter()
        .map(|(id, agent, kind)| serde_json::json!({"sessionId": id, "carries": {"agent": agent, "kind": kind}}))
        .collect();
    serde_json::json!({"status": "ok", "command": "session.find", "message": "", "gated": false, "data": {"sessions": rows}})
        .to_string()
}

/// `fixture` with more variables for the fake aoide.
fn fixture_with(tag: &str, lines: &[&[u8]], confirm: Box<dyn Confirm>, extra: &[(&str, String)]) -> Fixture {
    let dir = std::env::temp_dir().join(format!("aoide-app-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("a scratch dir is made");
    let follow = dir.join("follow.txt");
    let mut bytes = Vec::new();
    for line in lines {
        bytes.extend_from_slice(line);
        bytes.push(b'\n');
    }
    fs::write(&follow, bytes).expect("the follow file is written");
    let record = dir.join("record.jsonl");
    let env = [
        ("PATH", "/usr/bin:/bin".to_owned()),
        ("FAKE_AOIDE_RECORD", record.display().to_string()),
        ("FAKE_AOIDE_FOLLOW_FILE", follow.display().to_string()),
    ]
    .into_iter()
    .chain(extra.iter().map(|(k, v)| (*k, v.clone())))
    .map(|(k, v)| (k.into(), v.into()))
    .collect();
    let exec = Exec::load(&fake_aoide(), &dir.join("home"), env).expect("the fake loads");
    let mut fixture = Fixture { window: Window::new(exec, confirm), record, baseline: 0 };
    fixture.baseline = calls(&fixture.record);
    fixture
}

fn calls(record: &Path) -> usize {
    fs::read_to_string(record).unwrap_or_default().lines().count()
}


fn strings(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| (*w).to_owned()).collect()
}

fn collector() -> (Sink, Arc<Mutex<Vec<Push>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let into = seen.clone();
    (Arc::new(move |push| into.lock().unwrap().push(push)), seen)
}

fn wait_for(seen: &Mutex<Vec<Push>>, done: impl Fn(&[Push]) -> bool) {
    let until = Instant::now() + WAIT;
    while !done(&seen.lock().unwrap()) {
        assert!(Instant::now() < until, "the pushes did not arrive within the bound");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn an_act_whose_confirm_says_cancel_starts_no_child() {
    let confirm = Arc::new(Answer { answer: false, ..Default::default() });
    let fx = fixture("act-cancel", &[], Box::new(ArcConfirm(confirm.clone())));
    let result = fx.window.run(&strings(&["note", "add"]), &[], &strings(&["k=v"]));
    assert!(result.is_err(), "a cancelled act returns an error");
    assert_eq!(confirm.asked.lock().unwrap().len(), 1, "the act asks once");
    assert_eq!(calls(&fx.record), fx.baseline, "no child starts after a cancel");
}

#[test]
fn a_list_path_runs_without_a_confirm() {
    let confirm = Arc::new(Answer { answer: false, ..Default::default() });
    let fx = fixture("list", &[], Box::new(ArcConfirm(confirm.clone())));
    let reply = fx.window.run(&strings(&["config"]), &[], &[]).expect("a list runs");
    assert_eq!(reply.code, Some(0));
    assert!(confirm.asked.lock().unwrap().is_empty(), "a list asks no confirm");
    assert_eq!(calls(&fx.record), fx.baseline + 1, "the list starts its child");
}

#[test]
fn a_floored_path_is_refused_with_no_confirm_and_no_child() {
    let confirm = Arc::new(Answer { answer: true, ..Default::default() });
    let fx = fixture("floored", &[], Box::new(ArcConfirm(confirm.clone())));
    let result = fx.window.run(&strings(&["config", "set"]), &[], &strings(&["k=v"]));
    let why = result.expect_err("the floor refuses config set");
    assert!(why.contains("self-change"), "the floor's reason is named, not a bare refusal: {why}");
    assert!(confirm.asked.lock().unwrap().is_empty(), "a refused call asks no confirm");
    assert_eq!(calls(&fx.record), fx.baseline, "a refused call starts no child");
}

#[test]
fn a_confirm_asked_while_one_is_open_is_refused() {
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let hold = Hold { entered: Mutex::new(entered_tx), release: Mutex::new(release_rx) };
    let fx = Arc::new(fixture("busy", &[], Box::new(hold)));
    let first = {
        let fx = fx.clone();
        std::thread::spawn(move || fx.window.run(&strings(&["note", "add"]), &[], &strings(&["a=b"])))
    };
    entered_rx.recv_timeout(WAIT).expect("the first confirm is asked");
    let second = fx.window.run(&strings(&["note", "add"]), &[], &strings(&["a=c"]));
    assert!(second.is_err(), "a second confirm while one is open is refused");
    release_tx.send(()).unwrap();
    first.join().unwrap().expect("the first act runs once its confirm says yes");
}

#[test]
fn every_pushed_string_is_spelled() {
    let line: &[u8] = "plain \u{202e}reversed \u{1b}[31mred".as_bytes();
    let fx = fixture("spelled", &[line], Box::new(Answer::default()));
    let (sink, seen) = collector();
    fx.window.chat_open("s1", sink).expect("the chat opens");
    wait_for(&seen, |pushes| pushes.iter().any(|p| matches!(p, Push::Other { text } if text.contains("plain"))));
    fx.window.chat_close("s1").expect("the chat closes");
    for push in seen.lock().unwrap().iter().filter(|p| matches!(p, Push::Other { text } if text.contains("plain"))) {
        if let Push::Other { text } = push {
            assert!(!text.contains('\u{202e}'), "U+202E is spelled, not raw: {text:?}");
            assert!(!text.contains('\u{1b}'), "ESC is spelled, not raw: {text:?}");
            assert!(text.contains("\\u{202e}"), "U+202E shows as its escape: {text:?}");
            assert!(text.contains("\\x1b"), "ESC shows as its escape: {text:?}");
        }
    }
}

#[test]
fn a_second_open_of_one_id_leaves_one_child() {
    let fx = fixture("twice", &[b"{\"n\":1}"], Box::new(Answer::default()));
    let (sink, _seen) = collector();
    fx.window.chat_open("s1", sink.clone()).expect("the first open");
    fx.window.chat_open("s1", sink).expect("the second open");
    assert_eq!(fx.window.live_children(), 1, "one held child for one id");
    fx.window.chat_close("s1").expect("the chat closes");
    assert_eq!(fx.window.live_children(), 0, "the close leaves none");
}

#[test]
fn chat_close_ends_the_child_by_interrupt_and_reaps_it() {
    let fx = fixture("close", &[b"{\"n\":1}"], Box::new(Answer::default()));
    let (sink, seen) = collector();
    fx.window.chat_open("s1", sink).expect("the chat opens");
    wait_for(&seen, |pushes| !pushes.is_empty());
    fx.window.chat_close("s1").expect("the chat closes");
    assert_eq!(fx.window.live_children(), 0, "the child is reaped");
    let pushes = seen.lock().unwrap();
    assert!(
        pushes.iter().any(|p| matches!(p, Push::End)),
        "the envelope the fake prints on SIGINT arrives after the close: {pushes:?}"
    );
}

/// Shares one `Answer` between the test and the window.
struct ArcConfirm(Arc<Answer>);

impl Confirm for ArcConfirm {
    fn confirm(&self, verb: &str, text: &str) -> bool {
        self.0.confirm(verb, text)
    }
}

fn send_argv(record: &Path) -> Option<Vec<String>> {
    fs::read_to_string(record).unwrap_or_default().lines().find_map(|line| {
        let call: serde_json::Value = serde_json::from_str(line).expect("a record line is JSON");
        let args: Vec<String> = call["args"].as_array()?.iter().map(|a| a.as_str().unwrap().to_owned()).collect();
        (args.first().map(String::as_str) == Some("send")).then_some(args)
    })
}

fn sessions_file(tag: &str, rows: &[(&str, &str, &str)]) -> (&'static str, String) {
    let path = std::env::temp_dir().join(format!("aoide-app-sessions-{tag}-{}.json", std::process::id()));
    fs::write(&path, listing(rows)).expect("the listing is written");
    ("FAKE_AOIDE_SESSIONS", path.display().to_string())
}

#[test]
fn a_send_names_the_harness_the_session_was_listed_with_and_a_shell_names_none() {
    let listed = sessions_file("harness", &[("s1", "claude", "agent"), ("sh1", "shell", "shell")]);
    let fx = fixture_with("harness", &[], Box::new(Answer::default()), &[listed]);
    fx.window.sessions().expect("the sessions are listed");
    fx.window.chat_say("s1", "hi").expect("the send runs");
    assert_eq!(send_argv(&fx.record).expect("a send ran"), ["send", "--json", "--id=s1", "--harness=claude", "--submit", "--yes", "--", "hi"]);
    let _ = fs::remove_file(&fx.record);
    fx.window.chat_say("sh1", "ls").expect("the send runs");
    assert_eq!(send_argv(&fx.record).expect("a send ran"), ["send", "--json", "--id=sh1", "--submit", "--yes", "--", "ls"]);
}

#[test]
fn a_send_to_an_unlisted_session_is_refused_and_starts_no_send() {
    let listed = sessions_file("unlisted", &[("s1", "claude", "agent")]);
    let fx = fixture_with("unlisted", &[], Box::new(Answer::default()), &[listed]);
    let refused = fx.window.chat_say("ghost", "hi").expect_err("an unlisted session is refused");
    assert!(refused.contains("no session `ghost` is listed"), "{refused}");
    assert_eq!(send_argv(&fx.record), None, "no send started");
}

#[test]
fn a_send_core_refuses_reaches_the_page_as_an_error_and_not_a_reply() {
    let listed = sessions_file("refused", &[("s1", "claude", "agent")]);
    let why = "`s1` is not a `claude` session: it runs `kimi`, so nothing was typed";
    let fx = fixture_with(
        "refused",
        &[],
        Box::new(Answer::default()),
        &[listed, ("FAKE_AOIDE_EXIT", "1".to_owned()), ("FAKE_AOIDE_STDERR", why.to_owned())],
    );
    fx.window.sessions().expect("the sessions are listed");
    let error = fx.window.chat_say("s1", "hi").expect_err("a refused send is an error");
    assert!(error.contains("is not a `claude` session"), "{error}");
}

/// The fake aoide's asks file for `ask watch`, holding `text`.
fn asks_file(tag: &str, text: &str) -> (&'static str, String) {
    let path = std::env::temp_dir().join(format!("aoide-app-asks-{tag}-{}.jsonl", std::process::id()));
    fs::write(&path, text).expect("the asks file is written");
    ("FAKE_AOIDE_ASKS_FILE", path.display().to_string())
}

fn ask_collector() -> (AskSink, Arc<Mutex<Vec<AskPush>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let into = seen.clone();
    (Arc::new(move |push| into.lock().unwrap().push(push)), seen)
}

fn wait_for_asks(seen: &Mutex<Vec<AskPush>>, done: impl Fn(&[AskPush]) -> bool) {
    let until = Instant::now() + WAIT;
    while !done(&seen.lock().unwrap()) {
        assert!(Instant::now() < until, "the asks did not arrive within the bound: {:?}", seen.lock().unwrap());
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn opened(seen: &[AskPush]) -> Vec<(String, u64)> {
    seen.iter()
        .filter_map(|p| match p {
            AskPush::Open { ask } => Some((ask.session.clone(), ask.ask_id)),
            _ => None,
        })
        .collect()
}

#[test]
fn the_asks_watch_pushes_each_ask_and_close_ends_it_by_interrupt() {
    let fx = fixture_with("asks-push", &[], Box::new(Answer::default()), &[asks_file("push", ASKS)]);
    let (sink, seen) = ask_collector();
    fx.window.asks_open(sink).expect("the asks open");
    wait_for_asks(&seen, |pushes| opened(pushes).len() == 3);
    assert_eq!(opened(&seen.lock().unwrap()), [("s-1".to_owned(), 1), ("s-1".to_owned(), 2), ("s-1".to_owned(), 3)]);
    assert_eq!(fx.window.live_children(), 1, "one held child for the window");
    fx.window.asks_close().expect("the asks close");
    assert_eq!(fx.window.live_children(), 0, "the child is reaped");
    let pushes = seen.lock().unwrap();
    assert!(
        pushes.iter().any(|p| matches!(p, AskPush::End)),
        "the envelope the fake prints on SIGINT arrives after the close: {pushes:?}"
    );
}

#[test]
fn a_second_asks_open_leaves_one_child() {
    let fx = fixture_with("asks-twice", &[], Box::new(Answer::default()), &[asks_file("twice", ASKS)]);
    let (sink, _seen) = ask_collector();
    fx.window.asks_open(sink.clone()).expect("the first open");
    fx.window.asks_open(sink).expect("the second open");
    assert_eq!(fx.window.live_children(), 1, "one held child for the window");
    fx.window.asks_close().expect("the asks close");
    assert_eq!(fx.window.live_children(), 0, "the close leaves none");
}

#[test]
fn an_ask_that_leaves_the_store_is_pushed_gone() {
    let first = ASKS.lines().next().unwrap();
    let removed = first.replace("\"added\"", "\"removed\"");
    let fx = fixture_with("asks-gone", &[], Box::new(Answer::default()), &[asks_file("gone", &format!("{first}\n{removed}\n"))]);
    let (sink, seen) = ask_collector();
    fx.window.asks_open(sink).expect("the asks open");
    wait_for_asks(&seen, |pushes| pushes.iter().any(|p| matches!(p, AskPush::Gone { .. })));
    fx.window.asks_close().expect("the asks close");
    let pushes = seen.lock().unwrap();
    assert!(matches!(&pushes[0], AskPush::Open { ask } if ask.ask_id == 1));
    assert_eq!(pushes[1], AskPush::Gone { session: "s-1".to_owned(), ask_id: 1 });
}

#[test]
fn an_asks_watch_core_refuses_is_an_error_and_leaves_no_child() {
    let extra = [asks_file("refused", ASKS), ("FAKE_AOIDE_ASKS_REFUSE", "the asks store is unreadable".to_owned())];
    let fx = fixture_with("asks-refused", &[], Box::new(Answer::default()), &extra);
    let (sink, _seen) = ask_collector();
    let why = fx.window.asks_open(sink).expect_err("a refused watch is an error");
    assert!(why.contains("the asks store is unreadable"), "{why}");
    assert_eq!(fx.window.live_children(), 0);
}

#[test]
fn every_pushed_ask_string_is_spelled_and_a_raw_one_is_no_card() {
    let raw = ASKS.lines().nth(1).unwrap().replace("Which rice?", "Which\u{202e} rice?");
    let text = format!("{}\n{raw}\n{}\n", ASKS.lines().next().unwrap(), ASKS.lines().nth(2).unwrap());
    let fx = fixture_with("asks-spelled", &[], Box::new(Answer::default()), &[asks_file("spelled", &text)]);
    let (sink, seen) = ask_collector();
    fx.window.asks_open(sink).expect("the asks open");
    wait_for_asks(&seen, |pushes| pushes.len() == 3);
    fx.window.asks_close().expect("the asks close");
    let pushes = seen.lock().unwrap();
    assert_eq!(opened(&pushes), [("s-1".to_owned(), 1), ("s-1".to_owned(), 3)], "the raw row is no card");
    let wire: Vec<String> = pushes.iter().map(|p| serde_json::to_string(p).expect("a push serializes")).collect();
    for push in &wire {
        assert!(!push.contains('\u{202e}') && !push.contains('\u{1b}'), "a raw escape reaches the page: {push}");
    }
    assert!(wire.iter().any(|p| p.starts_with(r#"{"type":"other","text":"#)), "the raw row comes as a spelled note: {wire:?}");
    assert!(wire.iter().any(|p| p.contains(r#""fingerprint":"0badc0de0badc0de""#) && p.starts_with(r#"{"type":"open""#)), "{wire:?}");
}
