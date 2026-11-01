use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use aoide_desktop_app::{Confirm, Push, Sink, Window};
use aoide_desktop_shell::exec::Exec;

const WAIT: Duration = Duration::from_secs(5);

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
