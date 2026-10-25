//! The app's own doors: launch, record, re-attach, watch and close (plan §2.6).
//!
//! A door is an `eidolon web` process this app started. A door is signalled
//! only when its record passes the checks at that moment. The record is
//! `$AOIDE_ROOT/state/desktop/doors.json`. Each door has one watch thread that
//! reads its events and forwards them. The watch never closes or signals a
//! door: it updates the shared state and forwards. The caller acts on what it
//! forwards, and `CloseDue` says a marked door's turn has ended. The asks a
//! stream carries are held per door as well, and answered only at that door
//! (asks.rs).
//!
//! Nothing here reads an environment variable. The runtime dir arrives as the
//! value of `XDG_RUNTIME_DIR`, and `$AOIDE_ROOT` as a path.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, Write};
use std::net::SocketAddr;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use aoide_desktop_door::{
    check_runtime, runtime_from_env, token_path, AnswerOutcome, Ask, CancelOutcome, Closer, Door, DoorError, Events,
    Frame,
};
use aoide_protocol::untrusted::{is_unsafe, visible};
use serde::{Deserialize, Serialize};

use crate::asks::{self, Dialog, Prepared, Sent};
use crate::exec::{check_pin, terminate, Exec, ExecError};

/// How long a close waits for the watch thread to end.
const WATCH_BOUND: Duration = Duration::from_secs(2);
/// How long a close waits for the signalled door to exit.
const EXIT_BOUND: Duration = Duration::from_secs(10);
/// How long a stop now waits for `running=false` after a cancel answers 204.
const TURN_BOUND: Duration = Duration::from_secs(10);
/// How long a launch waits for the door's two banner lines.
const BANNER_BOUND: Duration = Duration::from_secs(30);
/// The least time between a stream's end and its next open.
const REOPEN: Duration = Duration::from_secs(1);
/// The tail of a door's log quoted when its banner never comes.
const TAIL: usize = 4096;

/// A door's turn, from its `turn-state` frames. A new stream starts `Unknown`,
/// because the wire sends a turn state only when it changes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TurnState {
    #[default]
    Unknown,
    Running,
    Idle,
}

/// What the watch threads forward to the caller, on one channel.
#[derive(Debug)]
pub enum Event {
    /// A frame from a door's events stream. Boxed, since a frame is much larger
    /// than the other variants.
    Frame { launch_id: String, frame: Box<Frame> },
    /// A door marked for closing has gone idle. The caller answers `close_due`.
    CloseDue { launch_id: String },
    /// A door's stream ended and the door is gone. Its record is dropped.
    Gone { launch_id: String },
}

/// What a close did. Nothing is signalled unless the checks held at that moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Close {
    /// SIGTERM was sent and the door exited within 10 s. Its record is removed.
    /// `watch_joined` is false when the watch thread outlived its 2 s bound.
    Exited { watch_joined: bool },
    /// SIGTERM was sent and the door was still live after 10 s. Its record
    /// stays, and the door is not watched again until the app next opens.
    StillLive,
    /// The door no longer passes its checks. Nothing was signalled, and its
    /// record is dropped.
    Dropped,
    /// The turn is not known to be idle. Nothing was signalled, and the door
    /// is watched again.
    Busy,
    /// The mark waits for the running turn to end.
    Armed,
    /// A new turn runs, so the armed mark stays armed.
    StaysArmed,
    /// The mark was revoked, so nothing was done.
    Revoked,
    /// No `running=false` came within 10 s of a cancel. Nothing was signalled.
    NoTurnEnd,
}

#[derive(Debug)]
pub enum DoorsError {
    Door(DoorError),
    Exec(ExecError),
    Io(io::Error),
    /// A launch plan refused: the pin, or the working directory.
    Plan(String),
    /// The record file is refused as it stands. It is never rewritten.
    Store(String),
    /// A door started but did not pass its checks. Nothing was signalled or
    /// recorded for it. Its pid and log are named, so the user can find it.
    Refused { pid: u32, log: PathBuf, why: String },
    /// A close the door's turn state does not allow yet.
    Turn(String),
    /// An answer refused before any request: its door fails its checks, the
    /// ask is not pending there, or the answer is not offered.
    Ask(String),
    /// No watched door has this launch id, or the door could not be re-watched.
    Unknown(String),
}

impl fmt::Display for DoorsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DoorsError::Door(e) => write!(f, "{e}"),
            DoorsError::Exec(e) => write!(f, "{e}"),
            DoorsError::Io(e) => write!(f, "{e}"),
            DoorsError::Plan(why)
            | DoorsError::Store(why)
            | DoorsError::Turn(why)
            | DoorsError::Ask(why)
            | DoorsError::Unknown(why) => f.write_str(why),
            DoorsError::Refused { pid, log, why } => {
                write!(f, "the door (pid {pid}) did not pass its checks: {why}; its log is {}", log.display())
            }
        }
    }
}

impl std::error::Error for DoorsError {}

impl From<io::Error> for DoorsError {
    fn from(e: io::Error) -> Self {
        DoorsError::Io(e)
    }
}

impl From<DoorError> for DoorsError {
    fn from(e: DoorError) -> Self {
        DoorsError::Door(e)
    }
}

impl From<ExecError> for DoorsError {
    fn from(e: ExecError) -> Self {
        DoorsError::Exec(e)
    }
}

/// What a launch runs: the pinned `eidolon` as `web --bind 127.0.0.1:0 --cwd D`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchPlan {
    pub cwd: PathBuf,
    pub argv: Vec<OsString>,
}

/// Pins `eidolon`, and checks the working directory: absolute, canonical, an
/// existing directory, and free of every character the app refuses.
pub fn launch_plan(eidolon: &Path, home: &Path, cwd: &Path) -> Result<LaunchPlan, DoorsError> {
    let pinned = check_pin(eidolon, home).map_err(|e| DoorsError::Exec(ExecError::Pin(e)))?;
    let shown = visible(&cwd.to_string_lossy());
    if !cwd.is_absolute() {
        return Err(DoorsError::Plan(format!("the working directory {shown} is relative; give its full path")));
    }
    let canonical = fs::canonicalize(cwd)
        .map_err(|_| DoorsError::Plan(format!("the working directory {shown} does not exist")))?;
    if canonical.as_os_str() != cwd.as_os_str() {
        return Err(DoorsError::Plan(format!("the working directory {shown} is not its canonical path")));
    }
    if !cwd.is_dir() {
        return Err(DoorsError::Plan(format!("the working directory {shown} is not a directory")));
    }
    let text = cwd.to_str().ok_or_else(|| DoorsError::Plan("the working directory is not UTF-8".into()))?;
    if text.chars().any(is_unsafe) {
        return Err(DoorsError::Plan(format!("the working directory {shown} holds a character the app refuses")));
    }
    let argv = vec![
        pinned.into_os_string(),
        "web".into(),
        "--bind".into(),
        "127.0.0.1:0".into(),
        "--cwd".into(),
        cwd.as_os_str().to_owned(),
    ];
    Ok(LaunchPlan { cwd: cwd.to_path_buf(), argv })
}

/// One door's record, as `doors.json` holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    launch_id: String,
    pid: u32,
    starttime: u64,
    addr: String,
    token_file: String,
    session_id: Option<String>,
    cwd: String,
    log: String,
}

/// `doors.json` under `$AOIDE_ROOT/state/desktop`. Each change is one
/// read-modify-write under an exclusive lock, ending in one atomic replace.
#[derive(Clone)]
struct Store {
    dir: PathBuf,
}

impl Store {
    fn new(root: &Path) -> Store {
        Store { dir: root.join("state/desktop") }
    }

    fn path(&self) -> PathBuf {
        self.dir.join("doors.json")
    }

    fn load(&self) -> Result<Vec<Entry>, DoorsError> {
        match fs::read(self.path()) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| {
                DoorsError::Store(format!(
                    "{} does not parse, so it is left as it is; move it aside to start fresh",
                    self.path().display()
                ))
            }),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(DoorsError::Io(e)),
        }
    }

    fn update(&self, change: impl FnOnce(&mut Vec<Entry>)) -> Result<(), DoorsError> {
        make_private_dir(&self.dir)?;
        let lock = open_private(&self.dir.join("doors.json.lock"))?;
        lock.lock()?;
        let mut entries = self.load()?;
        change(&mut entries);
        self.write(&entries)
    }

    fn write(&self, entries: &[Entry]) -> Result<(), DoorsError> {
        let tmp = self.dir.join(format!("doors.json.{}.{}.tmp", std::process::id(), unique()));
        let mut file = create_private(&tmp)?;
        file.write_all(&serde_json::to_vec_pretty(entries).expect("records serialize"))?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, self.path())?;
        File::open(&self.dir)?.sync_all()?;
        Ok(())
    }
}

/// The app's doors: each one launched or re-attached, and watched.
pub struct Doors {
    exec: Exec,
    eidolon: PathBuf,
    home: PathBuf,
    runtime: PathBuf,
    store: Store,
    out: Sender<Event>,
    watches: BTreeMap<String, Watch>,
}

impl Doors {
    /// Reads the record and re-attaches each door whose checks pass. A record
    /// that fails them is dropped, and its door is never signalled. An unset or
    /// unsafe runtime dir, and a record file that does not parse, refuse the open
    /// before anything is touched.
    pub fn open(
        exec: Exec,
        eidolon: &Path,
        home: &Path,
        root: &Path,
        runtime: Option<&OsStr>,
        out: Sender<Event>,
    ) -> Result<Doors, DoorsError> {
        let runtime = runtime_from_env(runtime)?;
        check_runtime(&runtime)?;
        let mut doors = Doors {
            exec,
            eidolon: eidolon.to_path_buf(),
            home: home.to_path_buf(),
            runtime,
            store: Store::new(root),
            out,
            watches: BTreeMap::new(),
        };
        for entry in doors.store.load()? {
            match doors.reattach(&entry) {
                Ok(watch) => {
                    doors.watches.insert(entry.launch_id.clone(), watch);
                }
                Err(_) => {
                    let id = entry.launch_id;
                    doors.store.update(|es| es.retain(|e| e.launch_id != id))?;
                }
            }
        }
        Ok(doors)
    }

    /// Launches a door the user has confirmed. Its banner, its identity and its
    /// token path are checked before its record is written. A door that fails
    /// such a check keeps running, unrecorded and unsignalled, and the error
    /// names its pid and log. A failed `hello` leaves the record, with a null
    /// session, for the next open to check again.
    pub fn launch(&mut self, plan: &LaunchPlan) -> Result<String, DoorsError> {
        check_runtime(&self.runtime)?;
        self.exec.door_env(&self.runtime)?;
        let id = unique();
        let dir = self.store.dir.join("doors");
        make_private_dir(&dir)?;
        let log_path = dir.join(format!("{id}.log"));
        let log = create_private(&log_path)?;
        let pid = self.exec.spawn_door(&plan.argv, &self.runtime, log)?;
        let refused = |why: String| DoorsError::Refused { pid, log: log_path.clone(), why };
        let banner = await_banner(&log_path, pid).map_err(refused)?;
        let starttime = stat_of(pid)
            .map(|s| s.starttime)
            .ok_or_else(|| refused("it exited before its checks".into()))?;
        let entry = Entry {
            launch_id: id.clone(),
            pid,
            starttime,
            addr: banner.addr.to_string(),
            token_file: banner.token_file.to_string_lossy().into_owned(),
            session_id: None,
            cwd: plan.cwd.to_string_lossy().into_owned(),
            log: log_path.to_string_lossy().into_owned(),
        };
        self.verify(&entry).map_err(refused)?;
        self.store.update(|es| es.push(entry.clone()))?;
        let (events, session) = self.open_stream(&entry).map_err(refused)?;
        self.store.update(|es| {
            for e in es.iter_mut().filter(|e| e.launch_id == id) {
                e.session_id = Some(session.clone());
            }
        })?;
        let mut entry = entry;
        entry.session_id = Some(session);
        let watch = self.start_watch(entry, banner.addr, events)?;
        self.watches.insert(id.clone(), watch);
        Ok(id)
    }

    pub fn launch_ids(&self) -> Vec<String> {
        self.watches.keys().cloned().collect()
    }

    /// The door's turn: `Unknown` until a turn-state frame says otherwise.
    pub fn turn(&self, id: &str) -> Result<TurnState, DoorsError> {
        Ok(self.watch(id)?.link.state.lock().unwrap().turn)
    }

    /// After this turn: an idle door closes now, a running one is marked, and an
    /// unknown one is refused, since its state is not known.
    pub fn close_after_turn(&mut self, id: &str) -> Result<Close, DoorsError> {
        match self.turn(id)? {
            TurnState::Unknown => Err(DoorsError::Turn(
                "this door's turn state is not known until a turn starts or ends; stop now closes it, stopping a turn if one runs"
                    .into(),
            )),
            TurnState::Idle => self.close_now(id),
            TurnState::Running => {
                self.watch(id)?.link.state.lock().unwrap().armed = true;
                Ok(Close::Armed)
            }
        }
    }

    /// Withdraws the mark, if one stands.
    pub fn revoke(&mut self, id: &str) -> Result<(), DoorsError> {
        self.watch(id)?.link.state.lock().unwrap().armed = false;
        Ok(())
    }

    /// The caller's answer to `CloseDue`: closes the door if its mark still
    /// stands and its turn is idle, and keeps the mark otherwise.
    pub fn close_due(&mut self, id: &str) -> Result<Close, DoorsError> {
        let (armed, turn) = {
            let shared = self.watch(id)?.link.state.lock().unwrap();
            (shared.armed, shared.turn)
        };
        if !armed {
            return Ok(Close::Revoked);
        }
        if turn != TurnState::Idle {
            return Ok(Close::StaysArmed);
        }
        self.close_now(id)
    }

    /// Stop now: cancels the turn if one runs, waits for it to end, then closes.
    /// A door that fails its checks is dropped before the cancel, so its token
    /// is never sent to whatever answers at its address. An idle door answers
    /// the cancel with 409, which is its own word that it is idle, so it closes
    /// at once.
    pub fn stop_now(&mut self, id: &str) -> Result<Close, DoorsError> {
        let entry = self.watch(id)?.entry.clone();
        if self.verify(&entry).is_err() {
            return self.close_now(id);
        }
        match self.door(id)?.cancel()? {
            CancelOutcome::Idle => {
                self.watch(id)?.link.state.lock().unwrap().turn = TurnState::Idle;
                self.close_now(id)
            }
            CancelOutcome::Cancelled => {
                if !self.wait_turn(id, TurnState::Idle)? {
                    return Ok(Close::NoTurnEnd);
                }
                self.close_now(id)
            }
        }
    }

    /// The asks this door's stream carries, oldest first.
    pub fn pending_asks(&self, id: &str) -> Result<Vec<Ask>, DoorsError> {
        Ok(self.watch(id)?.link.state.lock().unwrap().asks.values().cloned().collect())
    }

    /// Prepares an answer to an ask this door's stream carries. Nothing is sent.
    /// It refuses when no watched door carries the ask, or the ask does not
    /// offer the label, so an answer aimed at another door never gets a request.
    pub fn prepare_answer(&self, id: &str, ask_id: u64, label: &str) -> Result<Prepared, DoorsError> {
        let watch = self.watch(id)?;
        let ask = watch.link.state.lock().unwrap().asks.get(&ask_id).cloned();
        let Some(ask) = ask else {
            return Err(DoorsError::Ask(format!("ask {ask_id} is not pending at this door, so nothing was sent")));
        };
        if !asks::offers(&ask, label) {
            return Err(DoorsError::Ask(format!("ask {ask_id} does not offer that answer, so nothing was sent")));
        }
        let dialog = asks::dialog(&ask, label, watch.addr, Path::new(&watch.entry.token_file));
        Ok(Prepared { launch_id: id.to_owned(), ask_id, label: label.to_owned(), dialog })
    }

    /// Sends a prepared answer to the door that carried its ask, once the door
    /// passes its checks. The dialog is built again from the stored ask, and
    /// nothing is sent unless its digest is `shown`: the digest of the dialog on
    /// screen, or None when the answer shows none. A yes above the cap is never sent.
    pub fn send_answer(&self, prepared: &Prepared, shown: Option<[u8; 32]>) -> Result<Sent, DoorsError> {
        let entry = self.watch(&prepared.launch_id)?.entry.clone();
        self.verify(&entry)
            .map_err(|why| DoorsError::Ask(format!("the door failed its checks, so nothing was sent: {why}")))?;
        let fresh = self.prepare_answer(&prepared.launch_id, prepared.ask_id, &prepared.label)?;
        if fresh.dialog.as_ref().map(Dialog::digest) != shown {
            return Err(DoorsError::Ask("the ask on screen is not the one stored, so nothing was sent".into()));
        }
        if fresh.dialog.as_ref().is_some_and(Dialog::capped) {
            return Err(DoorsError::Ask(
                "no yes is offered above the dialog's cap, so nothing was sent; answer from a terminal".into(),
            ));
        }
        match self.door(&prepared.launch_id)?.answer(prepared.ask_id, &prepared.label, None)? {
            AnswerOutcome::Answered => Ok(Sent::Answered),
            AnswerOutcome::NotPending => Ok(Sent::NotPending),
            AnswerOutcome::Rejected { reason } => Ok(Sent::Rejected { reason }),
        }
    }

    fn watch(&self, id: &str) -> Result<&Watch, DoorsError> {
        self.watches
            .get(id)
            .ok_or_else(|| DoorsError::Unknown(format!("no watched door has launch id {id}")))
    }

    fn door(&self, id: &str) -> Result<Door, DoorsError> {
        let watch = self.watch(id)?;
        Ok(Door::new(watch.addr, &self.runtime, Path::new(&watch.entry.token_file))?)
    }

    fn wait_turn(&self, id: &str, want: TurnState) -> Result<bool, DoorsError> {
        let link = self.watch(id)?.link.clone();
        let shared = link.state.lock().unwrap();
        let (shared, _) = link
            .moved
            .wait_timeout_while(shared, TURN_BOUND, |s| s.turn != want && !s.gone)
            .unwrap();
        Ok(shared.turn == want)
    }

    /// The close, in five steps: end the watch, re-check the door, SIGTERM it,
    /// wait for it to exit, and remove its record. A turn not known to be idle
    /// refuses the close, and the door is watched again.
    fn close_now(&mut self, id: &str) -> Result<Close, DoorsError> {
        let mut watch = self
            .watches
            .remove(id)
            .ok_or_else(|| DoorsError::Unknown(format!("no watched door has launch id {id}")))?;
        let turn = watch.link.state.lock().unwrap().turn;
        let watch_joined = watch.detach();
        let entry = watch.entry.clone();
        if self.verify(&entry).is_err() {
            self.store.update(|es| es.retain(|e| e.launch_id != id))?;
            return Ok(Close::Dropped);
        }
        if turn != TurnState::Idle {
            let watch = self.reattach(&entry).map_err(DoorsError::Unknown)?;
            self.watches.insert(id.to_owned(), watch);
            return Ok(Close::Busy);
        }
        if let Err(e) = terminate(entry.pid) {
            if e.raw_os_error() != Some(libc::ESRCH) {
                return Err(e.into());
            }
        }
        if !exited_by(entry.pid, entry.starttime, Instant::now() + EXIT_BOUND) {
            return Ok(Close::StillLive);
        }
        self.store.update(|es| es.retain(|e| e.launch_id != id))?;
        Ok(Close::Exited { watch_joined })
    }

    /// Re-attaches one record: its checks, then its stream, then its watch.
    fn reattach(&self, entry: &Entry) -> Result<Watch, String> {
        self.verify(entry)?;
        let (events, session) = self.open_stream(entry)?;
        let addr = parse_addr(&entry.addr)?;
        let mut entry = entry.clone();
        if entry.session_id.is_none() {
            let id = entry.launch_id.clone();
            let _ = self.store.update(|es| {
                for e in es.iter_mut().filter(|e| e.launch_id == id) {
                    e.session_id = Some(session.clone());
                }
            });
            entry.session_id = Some(session);
        }
        self.start_watch(entry, addr, events).map_err(|e| e.to_string())
    }

    /// The checks that need no connection: the pin and the working directory,
    /// the pid's start time and not a zombie, its command line equal to the
    /// launch argv, and its token file the path of its port.
    fn verify(&self, entry: &Entry) -> Result<(), String> {
        let plan = launch_plan(&self.eidolon, &self.home, Path::new(&entry.cwd)).map_err(|e| e.to_string())?;
        let stat = stat_of(entry.pid).ok_or("the door is not running")?;
        if stat.state == 'Z' || stat.starttime != entry.starttime {
            return Err("the pid is not the door that was recorded".into());
        }
        if fs::read(format!("/proc/{}/cmdline", entry.pid)).ok().as_deref() != Some(argv_bytes(&plan.argv).as_slice())
        {
            return Err("its command line is not the launch's".into());
        }
        let addr = parse_addr(&entry.addr)?;
        if Path::new(&entry.token_file) != token_path(&self.runtime, addr.port()).as_path() {
            return Err("its token file is not the token path of its port".into());
        }
        Door::new(addr, &self.runtime, Path::new(&entry.token_file))
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// Opens the door's stream and reads its `hello`, which must be first, and
    /// whose session must be the recorded one when there is one.
    fn open_stream(&self, entry: &Entry) -> Result<(Events, String), String> {
        let addr = parse_addr(&entry.addr)?;
        let door = Door::new(addr, &self.runtime, Path::new(&entry.token_file)).map_err(|e| e.to_string())?;
        let mut events = door.events().map_err(|e| e.to_string())?;
        let session = match events.next() {
            Some(Ok(Frame::Hello { session, .. })) => session,
            _ => return Err("the door's stream does not open with hello".into()),
        };
        if entry.session_id.as_deref().is_some_and(|recorded| recorded != session) {
            return Err("the hello's session is not the recorded one".into());
        }
        Ok((events, session))
    }

    fn start_watch(&self, entry: Entry, addr: SocketAddr, events: Events) -> Result<Watch, DoorsError> {
        let link = Arc::new(Link::default());
        link.state.lock().unwrap().closer = Some(events.closer()?);
        let worker = Worker {
            link: link.clone(),
            launch_id: entry.launch_id.clone(),
            pid: entry.pid,
            starttime: entry.starttime,
            addr,
            token_file: PathBuf::from(&entry.token_file),
            runtime: self.runtime.clone(),
            store: self.store.clone(),
            out: self.out.clone(),
        };
        let thread = thread::Builder::new().spawn(move || worker.run(events))?;
        Ok(Watch { entry, addr, link, thread: Some(thread) })
    }
}

impl Drop for Doors {
    /// Ends every watch, as the app's exit does. No door is signalled.
    fn drop(&mut self) {
        for watch in self.watches.values_mut() {
            watch.detach();
        }
    }
}

/// One door the app watches: its record, its address, its shared state and its
/// watch thread.
struct Watch {
    entry: Entry,
    addr: SocketAddr,
    link: Arc<Link>,
    thread: Option<JoinHandle<()>>,
}

impl Watch {
    /// Closes the stream and waits up to 2 s for the watch thread to end. It
    /// signals nothing. Returns whether the thread ended in time.
    fn detach(&mut self) -> bool {
        {
            let mut shared = self.link.state.lock().unwrap();
            shared.stop = true;
            if let Some(closer) = shared.closer.take() {
                let _ = closer.close();
            }
            self.link.moved.notify_all();
        }
        let shared = self.link.state.lock().unwrap();
        let (shared, _) = self.link.moved.wait_timeout_while(shared, WATCH_BOUND, |s| !s.exited).unwrap();
        let joined = shared.exited;
        drop(shared);
        if joined {
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
        joined
    }
}

/// The state the watch thread and the caller share, under one lock.
#[derive(Default)]
struct Link {
    state: Mutex<Shared>,
    moved: Condvar,
}

#[derive(Default)]
struct Shared {
    turn: TurnState,
    armed: bool,
    stop: bool,
    closer: Option<Closer>,
    exited: bool,
    gone: bool,
    /// The asks the current stream carried, by ask id: the replay on a new
    /// stream rebuilds them, and a settled ask leaves them.
    asks: BTreeMap<u64, Ask>,
}

/// The watch thread's half: it reads, updates the shared state, and forwards.
struct Worker {
    link: Arc<Link>,
    launch_id: String,
    pid: u32,
    starttime: u64,
    addr: SocketAddr,
    token_file: PathBuf,
    runtime: PathBuf,
    store: Store,
    out: Sender<Event>,
}

/// Marks the watch thread as ended, however it ends.
struct Ended(Arc<Link>);

impl Drop for Ended {
    fn drop(&mut self) {
        self.0.state.lock().unwrap().exited = true;
        self.0.moved.notify_all();
    }
}

impl Worker {
    /// Reads the first stream, then reopens after each end while the door
    /// lives. A door that is gone takes its record with it.
    fn run(self, first: Events) {
        let _ended = Ended(self.link.clone());
        let mut next = Some(first);
        loop {
            if let Some(events) = next.take() {
                self.drain(events);
                self.end_stream();
            }
            if self.stopped() {
                return;
            }
            if !live(self.pid, self.starttime) {
                self.forget();
                return;
            }
            if self.pause() {
                return;
            }
            next = self.connect();
        }
    }

    /// Forwards one stream's frames. The stream's replay follows the opening, so
    /// the asks are cleared first and rebuilt from the replay.
    fn drain(&self, events: Events) {
        self.link.state.lock().unwrap().asks.clear();
        for frame in events {
            let Ok(frame) = frame else {
                return;
            };
            if self.stopped() {
                return;
            }
            let due = self.observe(&frame);
            let _ = self.out.send(Event::Frame { launch_id: self.launch_id.clone(), frame: Box::new(frame) });
            if due {
                let _ = self.out.send(Event::CloseDue { launch_id: self.launch_id.clone() });
            }
        }
    }

    /// Records what a frame changes: the turn state, an ask, or an ask settled.
    /// Returns whether a marked door's turn has now ended.
    fn observe(&self, frame: &Frame) -> bool {
        let mut shared = self.link.state.lock().unwrap();
        let due = match frame {
            Frame::TurnState { running } => {
                shared.turn = if *running { TurnState::Running } else { TurnState::Idle };
                !*running && shared.armed
            }
            Frame::Ask(ask) => {
                shared.asks.insert(asks::ask_id(ask), ask.clone());
                false
            }
            Frame::AskSettled { ask_id, .. } => {
                shared.asks.remove(ask_id);
                false
            }
            _ => false,
        };
        self.link.moved.notify_all();
        due
    }

    fn end_stream(&self) {
        let mut shared = self.link.state.lock().unwrap();
        shared.closer = None;
        shared.turn = TurnState::Unknown;
        self.link.moved.notify_all();
    }

    fn stopped(&self) -> bool {
        self.link.state.lock().unwrap().stop
    }

    /// Waits one reopen interval, or until the watch is stopped. Returns whether it was stopped.
    fn pause(&self) -> bool {
        let shared = self.link.state.lock().unwrap();
        let (shared, _) = self.link.moved.wait_timeout_while(shared, REOPEN, |s| !s.stop).unwrap();
        shared.stop
    }

    fn connect(&self) -> Option<Events> {
        let door = Door::new(self.addr, &self.runtime, &self.token_file).ok()?;
        let events = door.events().ok()?;
        let closer = events.closer().ok()?;
        let mut shared = self.link.state.lock().unwrap();
        if shared.stop {
            return None;
        }
        shared.closer = Some(closer);
        Some(events)
    }

    /// The door is gone: its record goes, and the caller hears it.
    fn forget(&self) {
        let id = self.launch_id.clone();
        let _ = self.store.update(|es| es.retain(|e| e.launch_id != id));
        {
            let mut shared = self.link.state.lock().unwrap();
            shared.gone = true;
            self.link.moved.notify_all();
        }
        let _ = self.out.send(Event::Gone { launch_id: id });
    }
}

struct Banner {
    addr: SocketAddr,
    token_file: PathBuf,
}

/// Reads the door's log until its two banner lines are there, for 30 s. A door
/// that dies first is named, with the tail of its log, which is data.
fn await_banner(log: &Path, pid: u32) -> Result<Banner, String> {
    let end = Instant::now() + BANNER_BOUND;
    loop {
        let bytes = fs::read(log).unwrap_or_default();
        let text = String::from_utf8_lossy(&bytes);
        let addr = text
            .lines()
            .find_map(|l| l.strip_prefix("listening on http://")?.strip_suffix('/'))
            .and_then(|a| a.parse::<SocketAddr>().ok());
        let token = text.lines().find_map(|l| l.strip_prefix("token file: "));
        if let (Some(addr), Some(token)) = (addr, token) {
            return Ok(Banner { addr, token_file: PathBuf::from(token) });
        }
        let tail = visible(&String::from_utf8_lossy(&bytes[bytes.len().saturating_sub(TAIL)..]));
        if !running(pid) {
            return Err(format!("it exited before its banner; its log's last 4 KiB reads: {tail}"));
        }
        if Instant::now() >= end {
            return Err(format!("no banner came within 30 s; its log's last 4 KiB reads: {tail}"));
        }
        thread::sleep(Duration::from_millis(20));
    }
}

/// Whether the door's pid is still the door: not a zombie, and started at `starttime`.
fn live(pid: u32, starttime: u64) -> bool {
    stat_of(pid).is_some_and(|s| s.state != 'Z' && s.starttime == starttime)
}

fn running(pid: u32) -> bool {
    stat_of(pid).is_some_and(|s| s.state != 'Z')
}

/// Waits until the pid is gone, a zombie, or started at another time. Returns
/// whether that happened before `end`.
fn exited_by(pid: u32, starttime: u64, end: Instant) -> bool {
    loop {
        if !live(pid, starttime) {
            return true;
        }
        if Instant::now() >= end {
            return false;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

/// The state and start time of `/proc/<pid>/stat`. Its fields are read after
/// the last `)`, since the command name may hold one.
struct Stat {
    state: char,
    starttime: u64,
}

fn stat_of(pid: u32) -> Option<Stat> {
    let text = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let fields: Vec<&str> = text[text.rfind(')')? + 1..].split_whitespace().collect();
    Some(Stat {
        state: fields.first()?.chars().next()?,
        starttime: fields.get(19)?.parse().ok()?,
    })
}

/// The command line as `/proc` holds it: each argument followed by a NUL.
fn argv_bytes(argv: &[OsString]) -> Vec<u8> {
    argv.iter().flat_map(|arg| arg.as_bytes().iter().copied().chain(std::iter::once(0))).collect()
}

fn parse_addr(text: &str) -> Result<SocketAddr, String> {
    text.parse().map_err(|_| "the recorded address is not a socket address".to_owned())
}

fn make_private_dir(path: &Path) -> Result<(), DoorsError> {
    DirBuilder::new().recursive(true).mode(0o700).create(path)?;
    Ok(())
}

fn open_private(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
}

fn create_private(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
}

static SEQ: AtomicU64 = AtomicU64::new(0);

/// A name no other call in this process has made, from the clock and a counter.
fn unique() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    format!("{nanos:x}-{}-{}", std::process::id(), SEQ.fetch_add(1, Ordering::Relaxed))
}
