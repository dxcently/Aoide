#![cfg(feature = "fakes")]

use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use aoide_desktop_door::{token_path, Ask, DoorError};
use aoide_desktop_shell::asks::{ask_id, Sent};
use aoide_desktop_shell::doors::{launch_plan, Close, Doors, DoorsError, Event, LaunchPlan, TurnState};
use aoide_desktop_shell::exec::{terminate, Exec, ExecError};
use serde_json::{json, Value};

const FAKE_AOIDE: &str = env!("CARGO_BIN_EXE_fake-aoide");
const FAKE_EIDOLON: &str = env!("CARGO_BIN_EXE_fake-eidolon");

static NEXT: AtomicU64 = AtomicU64::new(0);

/// A scratch tree the test owns. Each door the test started is told to exit
/// when the tree drops, and only while the start time still matches.
struct World {
    dir: PathBuf,
    started: Mutex<Vec<(u32, u64)>>,
}

impl World {
    fn new(tag: &str) -> World {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("aoide-desk-doors-{tag}-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        for sub in ["home", "door", "runtime"] {
            fs::create_dir_all(dir.join(sub)).expect("a scratch dir is made");
        }
        fs::set_permissions(dir.join("runtime"), fs::Permissions::from_mode(0o700)).expect("the runtime dir is 0700");
        World {
            dir: fs::canonicalize(&dir).expect("the scratch dir canonicalizes"),
            started: Mutex::new(Vec::new()),
        }
    }

    fn home(&self) -> PathBuf {
        self.dir.join("home")
    }

    fn cwd(&self) -> PathBuf {
        self.dir.join("door")
    }

    fn runtime(&self) -> PathBuf {
        self.dir.join("runtime")
    }

    fn root(&self) -> PathBuf {
        self.dir.join("root")
    }

    fn store(&self) -> PathBuf {
        self.root().join("state/desktop/doors.json")
    }

    fn exec(&self) -> Exec {
        self.exec_with(&[])
    }

    fn exec_with(&self, extra: &[(&str, &str)]) -> Exec {
        let mut env = vec![
            (OsString::from("PATH"), OsString::from("/usr/bin:/bin")),
            (OsString::from("FAKE_AOIDE_RECORD"), self.dir.join("aoide-record.jsonl").into_os_string()),
        ];
        env.extend(extra.iter().map(|(k, v)| (OsString::from(k), OsString::from(v))));
        Exec::load(Path::new(FAKE_AOIDE), &self.home(), env).expect("the fake aoide loads")
    }

    fn open(&self, out: Sender<Event>) -> Doors {
        self.open_with(self.exec(), out)
    }

    fn open_with(&self, exec: Exec, out: Sender<Event>) -> Doors {
        let runtime = self.runtime();
        Doors::open(exec, Path::new(FAKE_EIDOLON), &self.home(), &self.root(), Some(runtime.as_os_str()), out)
            .expect("the doors open")
    }

    fn plan(&self) -> LaunchPlan {
        launch_plan(Path::new(FAKE_EIDOLON), &self.home(), &self.cwd()).expect("the plan is made")
    }

    /// Launches a door through `Doors`, and keeps its pid for the drop.
    fn launch(&self, doors: &mut Doors) -> String {
        let id = doors.launch(&self.plan()).expect("the door launches");
        self.adopt(self.pid(&id));
        id
    }

    /// Launches a door in `cwd`, which keeps its own control and log files.
    fn launch_in(&self, doors: &mut Doors, cwd: &Path) -> String {
        let plan = launch_plan(Path::new(FAKE_EIDOLON), &self.home(), cwd).expect("the plan is made");
        let id = doors.launch(&plan).expect("the door launches");
        self.adopt(self.pid(&id));
        id
    }

    fn adopt(&self, pid: u32) {
        let starttime = stat_of(pid).expect("the door runs").starttime;
        self.started.lock().unwrap().push((pid, starttime));
    }

    /// The argv the launch plan makes, for a door started outside `Doors`.
    fn argv(&self) -> Vec<OsString> {
        let eidolon = fs::canonicalize(FAKE_EIDOLON).expect("the fake eidolon is found");
        vec![
            eidolon.into_os_string(),
            "web".into(),
            "--bind".into(),
            "127.0.0.1:0".into(),
            "--cwd".into(),
            self.cwd().into_os_string(),
        ]
    }

    fn record(&self) -> Vec<Value> {
        let text = fs::read_to_string(self.store()).unwrap_or_default();
        if text.is_empty() {
            return Vec::new();
        }
        serde_json::from_str(&text).expect("the record is a JSON array")
    }

    fn record_ids(&self) -> Vec<String> {
        self.record().iter().map(|e| e["launchId"].as_str().unwrap().to_owned()).collect()
    }

    fn entry(&self, id: &str) -> Value {
        self.record().into_iter().find(|e| e["launchId"] == id).expect("the record holds the launch")
    }

    fn pid(&self, id: &str) -> u32 {
        self.entry(id)["pid"].as_u64().expect("a pid") as u32
    }

    fn write_record(&self, entries: Value) {
        fs::create_dir_all(self.store().parent().unwrap()).unwrap();
        fs::write(self.store(), serde_json::to_vec(&entries).unwrap()).unwrap();
    }

    /// Writes one control word for the door in the working dir, atomically.
    fn command(&self, word: &str) {
        self.command_at(&self.cwd(), word);
    }

    /// Writes one control word and waits until the fake has applied it.
    fn apply(&self, word: &str) {
        let line = format!("control {word}");
        let before = self.count(&line);
        self.command(word);
        assert!(until(Duration::from_secs(5), || self.count(&line) == before + 1), "the fake applies {word}");
    }

    fn command_at(&self, cwd: &Path, word: &str) {
        let tmp = cwd.join(".fake-turn.tmp");
        fs::write(&tmp, word).unwrap();
        fs::rename(&tmp, cwd.join(".fake-turn")).unwrap();
    }

    fn log(&self) -> Vec<String> {
        self.log_at(&self.cwd())
    }

    fn log_at(&self, cwd: &Path) -> Vec<String> {
        fs::read_to_string(cwd.join(".fake-log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn count(&self, line: &str) -> usize {
        self.log().iter().filter(|l| l.as_str() == line).count()
    }

    fn position(&self, line: &str) -> Option<usize> {
        self.log().iter().position(|l| l.as_str() == line)
    }
}

impl Drop for World {
    fn drop(&mut self) {
        for (pid, starttime) in self.started.lock().unwrap().drain(..) {
            if stat_of(pid).is_some_and(|s| s.starttime == starttime && s.state != 'Z') {
                let _ = terminate(pid);
                until(Duration::from_secs(25), || !alive(pid));
            }
        }
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// Fields of `/proc/<pid>/stat`: state, parent, session and start time (field
/// 22, read after the last `)`, since the command name may hold either bracket).
struct Stat {
    state: char,
    ppid: u32,
    session: u32,
    starttime: u64,
}

fn stat_of(pid: u32) -> Option<Stat> {
    let text = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let fields: Vec<&str> = text[text.rfind(')')? + 1..].split_whitespace().collect();
    Some(Stat {
        state: fields.first()?.chars().next()?,
        ppid: fields.get(1)?.parse().ok()?,
        session: fields.get(3)?.parse().ok()?,
        starttime: fields.get(19)?.parse().ok()?,
    })
}

/// A zombie has exited, so it is not alive.
fn alive(pid: u32) -> bool {
    stat_of(pid).is_some_and(|s| s.state != 'Z')
}

fn until(within: Duration, mut ready: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + within;
    loop {
        if ready() {
            return true;
        }
        if Instant::now() >= end {
            return false;
        }
        thread::sleep(Duration::from_millis(5));
    }
}

fn event_where(rx: &Receiver<Event>, pick: impl Fn(&Event) -> bool) -> Event {
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        let left = end.saturating_duration_since(Instant::now());
        let event = rx.recv_timeout(left).expect("the event arrives within 10 s");
        if pick(&event) {
            return event;
        }
    }
}

fn port_of(entry: &Value) -> u16 {
    entry["addr"].as_str().unwrap().rsplit(':').next().unwrap().parse().unwrap()
}

/// Starts a door without `Doors`, the way the app does, so a test can give a
/// record a field the app never writes. Returns its pid, its address and its
/// token file, as the banner names them.
fn start_door_outside(world: &World) -> (u32, String, PathBuf) {
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let log_path = world.dir.join(format!("outside-{n}.log"));
    let log = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&log_path)
        .expect("the log opens");
    let pid = world.exec().spawn_door(&world.argv(), &world.runtime(), log).expect("the door starts");
    world.adopt(pid);
    let mut text = String::new();
    until(Duration::from_secs(30), || {
        text = fs::read_to_string(&log_path).unwrap_or_default();
        text.contains("token file: ")
    });
    let addr = text
        .lines()
        .find_map(|l| l.strip_prefix("listening on http://")?.strip_suffix('/'))
        .expect("the banner names the address")
        .to_owned();
    let token = text.lines().find_map(|l| l.strip_prefix("token file: ")).expect("the banner names the token");
    (pid, addr, PathBuf::from(token))
}

// --- the launch ---

#[test]
fn the_door_is_not_this_process_child_and_leads_a_session_of_its_own() {
    let world = World::new("session");
    let (tx, _rx) = mpsc::channel();
    let mut doors = world.open(tx);
    let id = world.launch(&mut doors);
    let stat = stat_of(world.pid(&id)).expect("the door runs");
    assert_ne!(stat.ppid, std::process::id(), "the door is not this process's child");
    assert_ne!(stat.session, stat_of(std::process::id()).unwrap().session, "the door leads its own session");
}

#[test]
fn the_record_holds_the_launch_fields_and_is_mode_0600() {
    let world = World::new("record");
    let (tx, _rx) = mpsc::channel();
    let mut doors = world.open(tx);
    let id = world.launch(&mut doors);
    let entry = world.entry(&id);
    let pid = world.pid(&id);
    assert_eq!(entry["starttime"].as_u64().unwrap(), stat_of(pid).unwrap().starttime);
    assert!(entry["addr"].as_str().unwrap().starts_with("127.0.0.1:"));
    assert_eq!(entry["tokenFile"].as_str().unwrap(), token_path(&world.runtime(), port_of(&entry)).to_str().unwrap());
    assert_eq!(entry["sessionId"].as_str().unwrap(), world.cwd().join("session.eid").to_str().unwrap());
    assert_eq!(entry["cwd"].as_str().unwrap(), world.cwd().to_str().unwrap());
    let log = world.root().join(format!("state/desktop/doors/{id}.log"));
    assert_eq!(entry["log"].as_str().unwrap(), log.to_str().unwrap());
    let mode = fs::metadata(world.store()).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600, "doors.json is mode 0600");
}

#[test]
fn a_cwd_that_is_relative_missing_not_canonical_or_holds_u202e_is_refused() {
    let world = World::new("cwd");
    let eidolon = Path::new(FAKE_EIDOLON);
    let refused = |cwd: &Path| launch_plan(eidolon, &world.home(), cwd).is_err();
    assert!(refused(Path::new("door")), "a relative directory");
    assert!(refused(&world.dir.join("missing")), "a missing directory");
    assert!(refused(&world.cwd().join(".")), "a path that is not its canonical form");
    let bidi = world.dir.join("bi\u{202e}di");
    fs::create_dir(&bidi).unwrap();
    assert!(refused(&bidi), "a directory whose name holds U+202E");
    assert!(!refused(&world.cwd()), "a plain canonical directory is accepted");
}

#[test]
fn an_unset_or_unsafe_runtime_dir_opens_nothing_and_starts_no_door() {
    let world = World::new("runtime");
    let (tx, _rx) = mpsc::channel();
    let unset = Doors::open(world.exec(), Path::new(FAKE_EIDOLON), &world.home(), &world.root(), None, tx.clone());
    assert!(unset.is_err(), "an unset runtime dir is refused");
    fs::set_permissions(world.runtime(), fs::Permissions::from_mode(0o755)).unwrap();
    let loose = Doors::open(
        world.exec(),
        Path::new(FAKE_EIDOLON),
        &world.home(),
        &world.root(),
        Some(world.runtime().as_os_str()),
        tx,
    );
    assert!(loose.is_err(), "a runtime dir that is not 0700 is refused");
    assert!(!world.root().join("state").exists(), "nothing was written");
    assert!(!world.cwd().join(".fake-log").exists(), "no door was started");
}

#[test]
fn unknown_vault_names_refuse_a_launch_before_anything_starts() {
    let world = World::new("vault");
    let (tx, _rx) = mpsc::channel();
    let mut doors = world.open_with(world.exec_with(&[("FAKE_AOIDE_NO_CONFIG", "1")]), tx);
    let refused = doors.launch(&world.plan());
    assert!(matches!(refused, Err(DoorsError::Exec(ExecError::VaultKeysUnknown))));
    assert!(!world.root().join("state").exists(), "no log and no record were made");
    assert!(!world.cwd().join(".fake-log").exists(), "no door was started");
}

#[test]
fn a_doors_file_that_does_not_parse_is_refused_and_never_rewritten() {
    let world = World::new("corrupt");
    fs::create_dir_all(world.store().parent().unwrap()).unwrap();
    fs::write(world.store(), b"{ not json").unwrap();
    let (tx, _rx) = mpsc::channel();
    let refused = Doors::open(
        world.exec(),
        Path::new(FAKE_EIDOLON),
        &world.home(),
        &world.root(),
        Some(world.runtime().as_os_str()),
        tx,
    );
    assert!(refused.is_err(), "the unparseable record is refused");
    assert_eq!(fs::read(world.store()).unwrap(), b"{ not json", "the record is not rewritten");
}

// --- the record, at start ---

#[test]
fn a_forged_record_whose_cmdline_is_not_the_launch_argv_is_dropped_and_never_signalled() {
    let world = World::new("forged-argv");
    let (pid, addr, token) = start_door_outside(&world);
    let other = world.dir.join("elsewhere");
    fs::create_dir_all(&other).unwrap();
    world.write_record(json!([{
        "launchId": "forged", "pid": pid, "starttime": stat_of(pid).unwrap().starttime,
        "addr": addr, "tokenFile": token, "sessionId": null, "cwd": other, "log": "x",
    }]));
    let (tx, _rx) = mpsc::channel();
    let _doors = world.open(tx);
    assert!(world.record().is_empty(), "the forged record is dropped");
    assert!(alive(pid), "the door is left alone");
    assert_eq!(world.count("SIGTERM"), 0, "nothing signalled the door");
}

#[test]
fn a_forged_record_with_a_wrong_start_time_is_dropped_and_never_signalled() {
    let world = World::new("forged-start");
    let (pid, addr, token) = start_door_outside(&world);
    world.write_record(json!([{
        "launchId": "forged", "pid": pid, "starttime": stat_of(pid).unwrap().starttime + 1,
        "addr": addr, "tokenFile": token, "sessionId": null, "cwd": world.cwd(), "log": "x",
    }]));
    let (tx, _rx) = mpsc::channel();
    let _doors = world.open(tx);
    assert!(world.record().is_empty(), "the forged record is dropped");
    assert!(alive(pid), "the door is left alone");
    assert_eq!(world.count("SIGTERM"), 0, "nothing signalled the door");
}

#[test]
fn a_record_whose_pid_is_dead_is_dropped() {
    let world = World::new("dead");
    let (pid, addr, token) = start_door_outside(&world);
    let starttime = stat_of(pid).unwrap().starttime;
    terminate(pid).expect("the door takes SIGTERM");
    assert!(until(Duration::from_secs(10), || !alive(pid)), "the door exits");
    world.write_record(json!([{
        "launchId": "dead", "pid": pid, "starttime": starttime,
        "addr": addr, "tokenFile": token, "sessionId": null, "cwd": world.cwd(), "log": "x",
    }]));
    let (tx, _rx) = mpsc::channel();
    let _doors = world.open(tx);
    assert!(world.record().is_empty(), "the dead record is dropped");
}

#[test]
fn a_record_whose_session_is_not_the_hello_session_is_dropped_and_never_signalled() {
    let world = World::new("forged-session");
    let (pid, addr, token) = start_door_outside(&world);
    world.write_record(json!([{
        "launchId": "forged", "pid": pid, "starttime": stat_of(pid).unwrap().starttime,
        "addr": addr, "tokenFile": token, "sessionId": "/elsewhere.eid", "cwd": world.cwd(), "log": "x",
    }]));
    let (tx, _rx) = mpsc::channel();
    let _doors = world.open(tx);
    assert!(world.record().is_empty(), "the record whose session differs is dropped");
    assert!(alive(pid), "the door is left alone");
    assert_eq!(world.count("SIGTERM"), 0, "nothing signalled the door");
}

#[test]
fn a_killed_apps_door_reattaches_from_the_record() {
    let world = World::new("reattach");
    let (tx1, _rx1) = mpsc::channel();
    let mut first = world.open(tx1);
    let id = world.launch(&mut first);
    drop(first);
    assert!(alive(world.pid(&id)), "the door outlives the app");
    let (tx2, _rx2) = mpsc::channel();
    let second = world.open(tx2);
    assert_eq!(second.launch_ids(), vec![id.clone()], "the door is attached again");
    assert_eq!(second.turn(&id).unwrap(), TurnState::Unknown, "its turn state is not known until a change");
    world.command("running");
    assert!(until(Duration::from_secs(5), || second.turn(&id).unwrap() == TurnState::Running));
}

// --- the watch and the close ---

#[test]
fn an_idle_close_leaves_no_token_file_and_removes_the_record() {
    let world = World::new("idle");
    let (tx, _rx) = mpsc::channel();
    let mut doors = world.open(tx);
    let id = world.launch(&mut doors);
    let pid = world.pid(&id);
    let token = token_path(&world.runtime(), port_of(&world.entry(&id)));
    assert!(token.exists(), "the door wrote its token");
    world.command("idle");
    assert!(until(Duration::from_secs(5), || doors.turn(&id).unwrap() == TurnState::Idle));
    let outcome = doors.close_after_turn(&id).unwrap();
    assert!(matches!(outcome, Close::Exited { .. }), "{outcome:?}");
    assert!(!token.exists(), "the stream closed before the signal, so the door drained and removed its token");
    assert!(world.record_ids().is_empty(), "the record is removed");
    assert_eq!(world.count("SIGTERM"), 1, "one signal, SIGTERM");
    let signals: Vec<String> = world.log().into_iter().filter(|l| l.starts_with("SIG")).collect();
    assert_eq!(signals, vec!["SIGTERM".to_owned()], "the signal record holds SIGTERM and nothing else");
    assert!(!alive(pid), "the door has exited");
}

#[test]
fn close_after_turn_while_the_turn_state_is_unknown_is_refused_and_signals_nothing() {
    let world = World::new("unknown");
    let (tx, _rx) = mpsc::channel();
    let mut doors = world.open(tx);
    let id = world.launch(&mut doors);
    assert_eq!(doors.turn(&id).unwrap(), TurnState::Unknown);
    let refused = doors.close_after_turn(&id).expect_err("the close is refused");
    assert!(refused.to_string().contains("not known until a turn starts or ends"), "{refused}");
    assert_eq!(world.count("SIGTERM"), 0, "nothing was signalled");
    assert!(alive(world.pid(&id)), "the door still runs");
}

#[test]
fn stop_now_cancels_before_it_signals() {
    let world = World::new("stop");
    let (tx, _rx) = mpsc::channel();
    let mut doors = world.open(tx);
    let id = world.launch(&mut doors);
    world.command("running");
    assert!(until(Duration::from_secs(5), || doors.turn(&id).unwrap() == TurnState::Running));
    let outcome = doors.stop_now(&id).unwrap();
    assert!(matches!(outcome, Close::Exited { .. }), "{outcome:?}");
    let cancel = world.position("POST /api/cancel").expect("the cancel was served");
    let signal = world.position("SIGTERM").expect("the door was signalled");
    assert!(cancel < signal, "the cancel comes before the signal: {:?}", world.log());
}

#[test]
fn stop_now_on_an_idle_door_closes_at_once_on_a_409() {
    let world = World::new("stop-idle");
    let (tx, _rx) = mpsc::channel();
    let mut doors = world.open(tx);
    let id = world.launch(&mut doors);
    world.command("idle");
    assert!(until(Duration::from_secs(5), || doors.turn(&id).unwrap() == TurnState::Idle));
    world.command("cancel-409");
    assert!(until(Duration::from_secs(5), || world.count("control cancel-409") == 1), "the 409 is queued");
    let started = Instant::now();
    let outcome = doors.stop_now(&id).unwrap();
    assert!(matches!(outcome, Close::Exited { .. }), "{outcome:?}");
    assert!(started.elapsed() < Duration::from_secs(5), "a 409 closes at once, without the 10 s wait");
    assert!(world.position("POST /api/cancel").is_some(), "the cancel was served");
}

#[test]
fn stop_now_after_a_reconnect_closes_on_the_409() {
    let world = World::new("stop-reconnect");
    let (tx, _rx) = mpsc::channel();
    let mut doors = world.open(tx);
    let id = world.launch(&mut doors);
    world.command("end");
    assert!(until(Duration::from_secs(10), || world.count("GET /api/events") == 2), "the stream reopened");
    assert_eq!(doors.turn(&id).unwrap(), TurnState::Unknown, "a reopened stream carries no turn state");
    world.command("cancel-409");
    assert!(until(Duration::from_secs(5), || world.count("control cancel-409") == 1), "the 409 is queued");
    let outcome = doors.stop_now(&id).unwrap();
    assert!(matches!(outcome, Close::Exited { .. }), "an idle door closes on its 409: {outcome:?}");
}

#[test]
fn stop_now_on_a_door_that_fails_its_checks_sends_no_request() {
    let world = World::new("stop-checks");
    let (tx, _rx) = mpsc::channel();
    let mut doors = world.open(tx);
    let id = world.launch(&mut doors);
    let port = port_of(&world.entry(&id));
    let token = token_path(&world.runtime(), port);
    let secret = fs::read(&token).expect("the door's token is read");
    let pid = world.pid(&id);
    world.command("end");
    terminate(pid).expect("the door takes SIGTERM");
    assert!(until(Duration::from_secs(25), || !alive(pid)), "the door exits");
    // A token left behind, and a listener on the door's port: a stop now must
    // not send the door's token to that listener.
    fs::write(&token, &secret).expect("the token is left behind");
    fs::set_permissions(&token, fs::Permissions::from_mode(0o600)).unwrap();
    let listener = std::net::TcpListener::bind(("127.0.0.1", port)).expect("the port is free");
    listener.set_nonblocking(true).unwrap();
    let outcome = doors.stop_now(&id).unwrap();
    assert_eq!(outcome, Close::Dropped, "{outcome:?}");
    assert!(listener.accept().is_err(), "no connection reached the port");
    assert!(world.record_ids().is_empty(), "the record is dropped");
}

#[test]
fn a_turn_extended_by_a_new_one_is_not_signalled_until_the_last_ends() {
    let world = World::new("mark");
    let (tx, rx) = mpsc::channel();
    let mut doors = world.open(tx);
    let id = world.launch(&mut doors);
    let due = |e: &Event| matches!(e, Event::CloseDue { .. });
    world.command("running");
    assert!(until(Duration::from_secs(5), || doors.turn(&id).unwrap() == TurnState::Running));
    assert_eq!(doors.close_after_turn(&id).unwrap(), Close::Armed);
    world.command("idle");
    event_where(&rx, due);
    world.command("running");
    assert!(until(Duration::from_secs(5), || doors.turn(&id).unwrap() == TurnState::Running));
    assert_eq!(doors.close_due(&id).unwrap(), Close::StaysArmed, "a new turn runs, so the mark stays armed");
    assert_eq!(world.count("SIGTERM"), 0, "no signal while the new turn runs");
    world.command("idle");
    event_where(&rx, due);
    let outcome = doors.close_due(&id).unwrap();
    assert!(matches!(outcome, Close::Exited { .. }), "{outcome:?}");
    assert_eq!(world.count("SIGTERM"), 1, "one signal, after the last turn");
}

#[test]
fn a_revoked_mark_is_never_signalled() {
    let world = World::new("revoked");
    let (tx, _rx) = mpsc::channel();
    let mut doors = world.open(tx);
    let id = world.launch(&mut doors);
    world.command("running");
    assert!(until(Duration::from_secs(5), || doors.turn(&id).unwrap() == TurnState::Running));
    assert_eq!(doors.close_after_turn(&id).unwrap(), Close::Armed);
    doors.revoke(&id).unwrap();
    world.command("idle");
    assert!(until(Duration::from_secs(5), || doors.turn(&id).unwrap() == TurnState::Idle));
    assert!(matches!(doors.close_due(&id).unwrap(), Close::Revoked));
    assert_eq!(world.count("SIGTERM"), 0, "a revoked mark signals nothing");
    assert!(alive(world.pid(&id)), "the door still runs");
}

#[test]
fn a_stream_that_ends_while_its_door_lives_is_opened_again() {
    let world = World::new("reopen");
    let (tx, _rx) = mpsc::channel();
    let mut doors = world.open(tx);
    let id = world.launch(&mut doors);
    world.command("end");
    assert!(
        until(Duration::from_secs(10), || world.count("GET /api/events") >= 2),
        "a second events request is made: {:?}",
        world.log()
    );
    assert_eq!(doors.launch_ids(), vec![id.clone()], "the door is still watched");
    assert!(alive(world.pid(&id)), "the door still runs");
}

#[test]
fn a_stream_that_ends_with_its_door_gone_drops_the_record() {
    let world = World::new("gone");
    let (tx, rx) = mpsc::channel();
    let mut doors = world.open(tx);
    let id = world.launch(&mut doors);
    let pid = world.pid(&id);
    world.command("end");
    terminate(pid).expect("the door takes SIGTERM");
    assert!(until(Duration::from_secs(10), || !alive(pid)), "the door exits");
    event_where(&rx, |e| matches!(e, Event::Gone { .. }));
    assert!(world.record_ids().is_empty(), "the record of a door that ended on its own is dropped");
}

// --- the asks ---

const APPROVAL: &str = r#"{"path":"a.txt"}"#;

/// The ids of the asks a door's stream carries now, oldest first.
fn ask_ids(doors: &Doors, id: &str) -> Vec<u64> {
    doors.pending_asks(id).unwrap().iter().map(ask_id).collect()
}

/// The input of the first ask a door carries, when that ask is an approval.
fn input_of(doors: &Doors, id: &str) -> Option<Value> {
    match doors.pending_asks(id).ok()?.first()? {
        Ask::Approval { input, .. } => Some(input.clone()),
        Ask::Question { .. } => None,
    }
}

/// The answer lines a door's fake served, each with the body the door sent.
fn answers_at(world: &World, cwd: &Path) -> Vec<String> {
    world.log_at(cwd).into_iter().filter(|l| l.starts_with("POST /api/answer")).collect()
}

/// The answer line for `ask` and `label`, with the body the door sends.
fn answer_line(ask: u64, label: &str) -> String {
    format!("POST /api/answer {}", json!({"ask_id": ask, "answer": label}))
}

/// Waits until the fake has logged `count` answers under `cwd`, then returns them.
fn answers_once(world: &World, cwd: &Path, count: usize) -> Vec<String> {
    until(Duration::from_secs(5), || answers_at(world, cwd).len() == count);
    answers_at(world, cwd)
}

/// The answers logged under `cwd` after a pause, so a request that was sent has
/// time to be logged before the list is read.
fn answers_after_a_pause(world: &World, cwd: &Path) -> Vec<String> {
    thread::sleep(Duration::from_millis(200));
    answers_at(world, cwd)
}

#[test]
fn an_answer_goes_only_to_the_door_whose_stream_carried_its_ask() {
    let world = World::new("asks-route");
    let (tx, _rx) = mpsc::channel();
    let mut doors = world.open(tx);
    let first = world.launch(&mut doors);
    fs::create_dir_all(world.dir.join("other")).unwrap();
    let other = fs::canonicalize(world.dir.join("other")).unwrap();
    let second = world.launch_in(&mut doors, &other);
    world.apply(&format!("ask-approval 1 {APPROVAL}"));
    assert!(until(Duration::from_secs(5), || ask_ids(&doors, &first) == vec![1]), "the first door carries ask 1");
    assert!(ask_ids(&doors, &second).is_empty(), "the second door carries no ask");
    let refused = doors.prepare_answer(&second, 1, "no").expect_err("the second door prepares no answer");
    assert!(matches!(refused, DoorsError::Ask(_)), "{refused}");
    let prepared = doors.prepare_answer(&first, 1, "no").expect("the first door prepares its answer");
    assert_eq!(doors.send_answer(&prepared, None).unwrap(), Sent::Answered);
    assert_eq!(answers_once(&world, &world.cwd(), 1), vec![answer_line(1, "no")], "the answer carries the ask and the label");
    assert!(answers_after_a_pause(&world, &other).is_empty(), "no request reached the door that carries no such ask");
}

#[test]
fn a_204_keeps_the_card_until_the_door_settles_the_ask() {
    let world = World::new("asks-204");
    let (tx, _rx) = mpsc::channel();
    let mut doors = world.open(tx);
    let id = world.launch(&mut doors);
    world.apply(&format!("ask-approval 1 {APPROVAL}"));
    assert!(until(Duration::from_secs(5), || ask_ids(&doors, &id) == vec![1]), "the door carries ask 1");
    let prepared = doors.prepare_answer(&id, 1, "no").unwrap();
    assert_eq!(doors.send_answer(&prepared, None).unwrap(), Sent::Answered);
    assert_eq!(ask_ids(&doors, &id), vec![1], "a 204 leaves the card until the door settles the ask");
    world.apply("settle 1 no");
    assert!(until(Duration::from_secs(5), || ask_ids(&doors, &id).is_empty()), "the settled ask leaves the card");
}

#[test]
fn a_409_is_not_pending_and_keeps_the_card_until_the_settle() {
    let world = World::new("asks-409");
    let (tx, _rx) = mpsc::channel();
    let mut doors = world.open(tx);
    let id = world.launch(&mut doors);
    world.apply(&format!("ask-approval 1 {APPROVAL}"));
    assert!(until(Duration::from_secs(5), || ask_ids(&doors, &id) == vec![1]), "the door carries ask 1");
    world.apply("answer-409");
    let prepared = doors.prepare_answer(&id, 1, "no").unwrap();
    assert_eq!(doors.send_answer(&prepared, None).unwrap(), Sent::NotPending);
    assert_eq!(answers_once(&world, &world.cwd(), 1), vec![answer_line(1, "no")], "the request was served");
    assert_eq!(ask_ids(&doors, &id), vec![1], "the card stays until the door settles the ask");
}

#[test]
fn a_422_is_rejected_with_the_doors_reason_and_keeps_the_card() {
    let world = World::new("asks-422");
    let (tx, _rx) = mpsc::channel();
    let mut doors = world.open(tx);
    let id = world.launch(&mut doors);
    world.apply(&format!("ask-approval 1 {APPROVAL}"));
    assert!(until(Duration::from_secs(5), || ask_ids(&doors, &id) == vec![1]), "the door carries ask 1");
    world.apply("answer-422");
    let prepared = doors.prepare_answer(&id, 1, "no").unwrap();
    let outcome = doors.send_answer(&prepared, None).unwrap();
    assert_eq!(outcome, Sent::Rejected { reason: "the ask is not taken".into() });
    assert_eq!(ask_ids(&doors, &id), vec![1], "a rejected answer keeps the card");
}

#[test]
fn a_401_is_an_error_and_keeps_the_card() {
    let world = World::new("asks-401");
    let (tx, _rx) = mpsc::channel();
    let mut doors = world.open(tx);
    let id = world.launch(&mut doors);
    world.apply(&format!("ask-approval 1 {APPROVAL}"));
    assert!(until(Duration::from_secs(5), || ask_ids(&doors, &id) == vec![1]), "the door carries ask 1");
    world.apply("answer-401");
    let prepared = doors.prepare_answer(&id, 1, "no").unwrap();
    let refused = doors.send_answer(&prepared, None).expect_err("a 401 is an error");
    assert!(matches!(refused, DoorsError::Door(DoorError::Unauthorized)), "{refused}");
    assert_eq!(ask_ids(&doors, &id), vec![1], "an error keeps the card");
}

#[test]
fn a_yes_above_the_cap_is_refused_before_any_request() {
    let world = World::new("asks-cap");
    let (tx, _rx) = mpsc::channel();
    let mut doors = world.open(tx);
    let id = world.launch(&mut doors);
    let big = json!({"blob": "x".repeat(5 * 1024)});
    world.apply(&format!("ask-approval 1 {big}"));
    assert!(until(Duration::from_secs(5), || ask_ids(&doors, &id) == vec![1]), "the door carries ask 1");
    let prepared = doors.prepare_answer(&id, 1, "yes").unwrap();
    let dialog = prepared.dialog().expect("a yes on an approval has a dialog");
    assert!(dialog.capped(), "a 5 KiB input is over the cap");
    let refused = doors.send_answer(&prepared, Some(dialog.digest())).expect_err("a capped yes is refused");
    assert!(matches!(refused, DoorsError::Ask(_)), "{refused}");
    assert!(answers_after_a_pause(&world, &world.cwd()).is_empty(), "no answer was sent");
}

#[test]
fn a_changed_ask_is_refused_and_a_yes_is_sent_only_with_the_digest_on_screen() {
    let world = World::new("asks-digest");
    let (tx, _rx) = mpsc::channel();
    let mut doors = world.open(tx);
    let id = world.launch(&mut doors);
    world.apply(&format!("ask-approval 1 {APPROVAL}"));
    assert!(until(Duration::from_secs(5), || ask_ids(&doors, &id) == vec![1]), "the door carries ask 1");
    let stale = doors.prepare_answer(&id, 1, "yes").unwrap();
    let stale_digest = stale.dialog().expect("a yes has a dialog").digest();
    world.apply(r#"ask-approval 1 {"path":"b.txt"}"#);
    assert!(
        until(Duration::from_secs(5), || input_of(&doors, &id) == Some(json!({"path": "b.txt"}))),
        "the door carries the changed ask"
    );
    let refused = doors.send_answer(&stale, Some(stale_digest)).expect_err("the changed ask is refused");
    assert!(matches!(refused, DoorsError::Ask(_)), "{refused}");
    let fresh = doors.prepare_answer(&id, 1, "yes").unwrap();
    let refused = doors.send_answer(&fresh, None).expect_err("a yes shown with no digest is refused");
    assert!(matches!(refused, DoorsError::Ask(_)), "{refused}");
    assert!(answers_after_a_pause(&world, &world.cwd()).is_empty(), "no answer was sent so far");
    let digest = fresh.dialog().expect("a yes has a dialog").digest();
    assert_eq!(doors.send_answer(&fresh, Some(digest)).unwrap(), Sent::Answered);
    assert_eq!(answers_once(&world, &world.cwd(), 1), vec![answer_line(1, "yes")]);
}

#[test]
fn an_ask_settled_while_the_stream_is_down_is_not_replayed_on_reopen() {
    let world = World::new("asks-replay");
    let (tx, _rx) = mpsc::channel();
    let mut doors = world.open(tx);
    let id = world.launch(&mut doors);
    world.apply(&format!("ask-approval 1 {APPROVAL}"));
    world.apply(r#"ask-approval 2 {"path":"b.txt"}"#);
    assert!(until(Duration::from_secs(5), || ask_ids(&doors, &id) == vec![1, 2]), "the door carries asks 1 and 2");
    world.apply("end");
    world.apply("settle 1 no");
    assert!(until(Duration::from_secs(10), || world.count("GET /api/events") == 2), "the stream reopens");
    assert!(
        until(Duration::from_secs(5), || ask_ids(&doors, &id) == vec![2]),
        "the reopened stream replays only ask 2"
    );
}

#[test]
fn an_answer_to_a_door_that_fails_its_checks_sends_no_request() {
    let world = World::new("asks-checks");
    let (tx, _rx) = mpsc::channel();
    let mut doors = world.open(tx);
    let id = world.launch(&mut doors);
    world.apply(&format!("ask-approval 1 {APPROVAL}"));
    assert!(until(Duration::from_secs(5), || ask_ids(&doors, &id) == vec![1]), "the door carries ask 1");
    let prepared = doors.prepare_answer(&id, 1, "no").unwrap();
    let port = port_of(&world.entry(&id));
    let pid = world.pid(&id);
    world.apply("end");
    terminate(pid).expect("the door takes SIGTERM");
    assert!(until(Duration::from_secs(25), || !alive(pid)), "the door exits");
    // A listener on the door's port: an answer must not reach it.
    let listener = std::net::TcpListener::bind(("127.0.0.1", port)).expect("the port is free");
    listener.set_nonblocking(true).unwrap();
    let refused = doors.send_answer(&prepared, None).expect_err("a door that is gone is refused");
    assert!(matches!(refused, DoorsError::Ask(_)), "{refused}");
    assert!(listener.accept().is_err(), "no connection reached the port");
}

