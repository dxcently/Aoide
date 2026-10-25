//! The app's own doors: launch, record, re-attach, watch and close (plan §2.6).
//!
//! A door is an `eidolon web` process this app started. A door is signalled
//! only when its checks hold at that moment, and only through a pidfd that
//! names it. The record is `$AOIDE_ROOT/state/desktop/doors.json`. Each door
//! has one watch thread: it reads the door's events, updates the shared state,
//! and forwards to the caller over a bounded channel. The watch never closes or
//! signals a door. The asks a stream carries are held per door, and answered
//! only at that door (asks.rs).
//!
//! Nothing here reads an environment variable. The runtime dir arrives as the
//! value of `XDG_RUNTIME_DIR`, and `$AOIDE_ROOT` as a path.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::net::SocketAddr;
use std::os::fd::OwnedFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use aoide_desktop_door::{
    check_runtime, runtime_from_env, token_path, AnswerOutcome, Ask, CancelOutcome, Closer, Door, DoorError, Events,
    Frame,
};
use aoide_protocol::untrusted::{is_unsafe, visible};
use serde::{Deserialize, Serialize};

use crate::asks::{self, Prepared, Sent, Shown};
use crate::exec::{check_pin, euid, open_pidfd, pidfd_exited, terminate_pidfd, Exec, ExecError, Pinned};

/// The bound on the events channel. The caller creates the channel with this size.
pub const EVENTS_BOUND: usize = 64;
/// The most asks a door keeps pending. An ask past it is shown but not kept.
pub const ASKS_BOUND: usize = 64;
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
/// How far into its log a door's banner must lie, in bytes.
const BANNER_HEAD: usize = 64 * 1024;
/// The tail of a door's log quoted when a launch is refused.
const TAIL: usize = 4096;
/// How often a waiting launch, or a watch retrying a terminal notice, looks again.
const POLL: Duration = Duration::from_millis(20);

/// A door's turn, from its `turn-state` frames. A new stream starts `Unknown`,
/// because the wire sends a turn state only when it changes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TurnState {
    #[default]
    Unknown,
    Running,
    Idle,
}

/// What the watch threads forward to the caller, over one channel of `EVENTS_BOUND`.
#[derive(Debug)]
pub enum Event {
    /// A frame from a door's events stream. Boxed, since a frame is much larger
    /// than the other variants.
    Frame { launch_id: String, frame: Box<Frame> },
    /// A door marked for closing has gone idle. The caller answers `close_due`.
    CloseDue { launch_id: String },
    /// A door's stream ended and the door is gone. Its record is dropped.
    Gone { launch_id: String },
    /// A record whose door is not the one recorded: the pid gone or a zombie,
    /// another start time, another command line, or a changed pin. The record
    /// is dropped, and the pid, which was not signalled, is named.
    Dropped { launch_id: String, pid: u32, why: String },
    /// A door that could not be checked or used, for any other reason. Sent once
    /// per failure streak: its record is kept, and the watch tries again each
    /// second while the process lives, resuming the frames when a try works.
    Unwatched { launch_id: String, why: String },
    /// An ask past `ASKS_BOUND`: it is not forwarded and not kept, so it cannot be
    /// answered at this door. `ask_id` names the first ask a stream refused. Sent
    /// once per stream.
    AsksOverflow { launch_id: String, ask_id: u64 },
}

/// What a close did. Nothing is signalled unless the checks held at that moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Close {
    /// SIGTERM was sent and the door exited within 10 s. Its record and its
    /// watch are removed. `watch_joined` is false when the watch thread outlived
    /// its 2 s bound.
    Exited { watch_joined: bool },
    /// SIGTERM was sent and the door was still live after 10 s. Its record and
    /// its watch stay, and the watch reopens the stream.
    StillLive,
    /// The door's identity fails its checks: its pid is gone, another process,
    /// or another command line. Nothing was signalled, and its record and its
    /// watch are dropped. `why` names the failed check, through `visible`.
    Dropped { pid: u32, why: String },
    /// The turn is not idle. Nothing was signalled, and the watch is left as it
    /// was, turn state included.
    Busy,
    /// The mark waits for the running turn to end.
    Armed,
    /// A new turn runs, so the armed mark stays armed.
    StaysArmed,
    /// The mark was revoked, so nothing was done.
    Revoked,
    /// No `running=false` came within 10 s of a cancel. Nothing was signalled,
    /// and the record and the watch are kept.
    NoTurnEnd,
}

#[derive(Debug)]
pub enum DoorsError {
    Door(DoorError),
    Exec(ExecError),
    Io(io::Error),
    /// A launch plan refused: the pin, or the working directory.
    Plan(String),
    /// The record file or its directory is refused as it stands. It is never
    /// rewritten.
    Store(String),
    /// A door started but did not pass its checks before its record was written.
    /// It was signalled through its own pidfd, if one could be opened, and its
    /// pid and log are named, so the user can find it.
    Refused { pid: u32, log: PathBuf, why: String },
    /// A close the door's turn state does not allow yet.
    Turn(String),
    /// An answer refused before any request: its door fails its checks, the
    /// ask is not pending there, or the answer is not offered.
    Ask(String),
    /// No watched door has this launch id.
    Unknown(String),
    /// A close whose checks fail for a reason other than identity. The record and
    /// the watch are kept, and nothing is signalled.
    Unverified(String),
    /// A launch whose record is written, for which no watch could start. The
    /// record stays, and the next open re-attaches it.
    Unwatched { launch_id: String, why: String },
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
            DoorsError::Unverified(why) => write!(f, "nothing was signalled: {why}"),
            DoorsError::Unwatched { launch_id, why } => {
                write!(f, "launch {launch_id} is recorded, but no watch could start for it: {why}")
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

/// What a launch runs: the pinned `eidolon` with `web --bind 127.0.0.1:0 --cwd D`.
/// Only `launch_plan` builds one, and `Doors::launch` spawns its program and args
/// and no other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchPlan {
    cwd: PathBuf,
    program: Pinned,
    args: Vec<OsString>,
}

/// Pins `eidolon`, and checks the working directory.
pub fn launch_plan(eidolon: &Path, home: &Path, cwd: &Path) -> Result<LaunchPlan, DoorsError> {
    let pinned = check_pin(eidolon, home).map_err(|e| DoorsError::Exec(ExecError::Pin(e)))?;
    check_cwd(cwd).map_err(DoorsError::Plan)?;
    Ok(LaunchPlan { cwd: cwd.to_path_buf(), program: pinned, args: launch_args(cwd) })
}

/// The working directory is absolute, canonical, an existing directory, and free
/// of every character the app refuses.
fn check_cwd(cwd: &Path) -> Result<(), String> {
    let shown = visible(&cwd.to_string_lossy());
    if !cwd.is_absolute() {
        return Err(format!("the working directory {shown} is relative; give its full path"));
    }
    let canonical = fs::canonicalize(cwd).map_err(|_| format!("the working directory {shown} does not exist"))?;
    if canonical.as_os_str() != cwd.as_os_str() {
        return Err(format!("the working directory {shown} is not its canonical path"));
    }
    if !cwd.is_dir() {
        return Err(format!("the working directory {shown} is not a directory"));
    }
    let text = cwd.to_str().ok_or("the working directory is not UTF-8")?;
    if text.chars().any(is_unsafe) {
        return Err(format!("the working directory {shown} holds a character the app refuses"));
    }
    Ok(())
}

/// The arguments of `eidolon web` for `cwd`, after the program.
fn launch_args(cwd: &Path) -> Vec<OsString> {
    vec!["web".into(), "--bind".into(), "127.0.0.1:0".into(), "--cwd".into(), cwd.as_os_str().to_owned()]
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
/// read-modify-write under an exclusive lock, ending in one atomic replace. The
/// record is read through an `O_NOFOLLOW` open, so a link is refused.
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

    /// Makes the store's directory, and takes its exclusive lock.
    fn lock(&self) -> Result<File, DoorsError> {
        make_private_dir(&self.dir)?;
        let lock = open_private(&self.dir.join("doors.json.lock"))?;
        lock.lock()?;
        Ok(lock)
    }

    /// The record under the lock. A store directory that does not exist yet holds
    /// no record, and opening the store makes no directory for it.
    fn load(&self) -> Result<Vec<Entry>, DoorsError> {
        if !self.dir.exists() {
            return Ok(Vec::new());
        }
        let _lock = self.lock()?;
        self.read()
    }

    /// The record, read without taking the lock. The caller holds it.
    fn read(&self) -> Result<Vec<Entry>, DoorsError> {
        let mut bytes = Vec::new();
        match OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW).open(self.path()) {
            Ok(mut file) => {
                file.read_to_end(&mut bytes)?;
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) if e.raw_os_error() == Some(libc::ELOOP) => return Err(store_refusal(&self.path(), "is a link")),
            Err(e) => return Err(e.into()),
        }
        serde_json::from_slice(&bytes).map_err(|_| {
            DoorsError::Store(format!(
                "{} does not parse, so it is left as it is; move it aside to start fresh",
                self.path().display()
            ))
        })
    }

    fn update(&self, change: impl FnOnce(&mut Vec<Entry>)) -> Result<(), DoorsError> {
        let _lock = self.lock()?;
        let mut entries = self.read()?;
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
        open_dir(&self.dir)?.sync_all()?;
        Ok(())
    }
}

/// What the checks read: the pinned eidolon, the home it is pinned under, and
/// the runtime dir the door's token lives in.
#[derive(Debug, Clone)]
struct Pins {
    eidolon: PathBuf,
    home: PathBuf,
    runtime: PathBuf,
}

/// Why a record's door fails its checks. An identity failure means the record
/// does not describe the process it names, so the record goes. Any other means
/// the door could not be checked or used, so the record stays.
enum Check {
    Identity(String),
    Other(String),
}

impl Check {
    fn why(self) -> String {
        match self {
            Check::Identity(why) | Check::Other(why) => why,
        }
    }
}

/// The app's doors: each one launched or re-attached, and watched.
pub struct Doors {
    exec: Exec,
    pins: Pins,
    store: Store,
    out: SyncSender<Event>,
    watches: BTreeMap<String, Watch>,
}

impl Doors {
    /// Reads the record and re-attaches each door. Its events come back in the
    /// list it returns, and none is sent on the channel: `Event::Dropped` for a
    /// record whose door is not the one recorded, which is dropped and never
    /// signalled, and `Event::Unwatched` for a record no watch could start for,
    /// which is kept, so the next open tries again. A door whose check fails for
    /// any other reason is watched anyway, and its watch reports the failure and
    /// tries again. An unset or refused runtime dir, a record that is a link or
    /// does not parse, and a directory that is not private, refuse the open
    /// before anything is touched.
    pub fn open(
        exec: Exec,
        eidolon: &Path,
        home: &Path,
        root: &Path,
        runtime: Option<&OsStr>,
        out: SyncSender<Event>,
    ) -> Result<(Doors, Vec<Event>), DoorsError> {
        let runtime = runtime_from_env(runtime)?;
        check_runtime(&runtime)?;
        let mut doors = Doors {
            exec,
            pins: Pins { eidolon: eidolon.to_path_buf(), home: home.to_path_buf(), runtime },
            store: Store::new(root),
            out,
            watches: BTreeMap::new(),
        };
        let mut events = Vec::new();
        for entry in doors.store.load()? {
            match doors.reattach(&entry) {
                Ok(()) => {}
                Err(Check::Identity(why)) => {
                    let id = entry.launch_id.clone();
                    doors.store.update(|es| es.retain(|e| e.launch_id != id))?;
                    events.push(Event::Dropped { launch_id: id, pid: entry.pid, why: visible(&why) });
                }
                Err(Check::Other(why)) => {
                    events.push(Event::Unwatched { launch_id: entry.launch_id.clone(), why: visible(&why) });
                }
            }
        }
        Ok((doors, events))
    }

    /// Launches a door the user has confirmed. Its banner, its identity and its
    /// token path are checked before its record is written. A door that fails
    /// such a check is signalled through its own pidfd, is not recorded, and the
    /// error names its pid and log. Once the record is written, a stream that
    /// does not open starts the watch unattached, which reports it. A launch for
    /// which no watch can start keeps the record and returns
    /// `DoorsError::Unwatched` with the launch id. Nothing is sent on the channel.
    pub fn launch(&mut self, plan: &LaunchPlan) -> Result<String, DoorsError> {
        check_runtime(&self.pins.runtime)?;
        self.exec.door_env(&self.pins.runtime)?;
        let id = unique();
        let dir = self.store.dir.join("doors");
        make_private_dir(&dir)?;
        let log_path = dir.join(format!("{id}.log"));
        let log = create_private(&log_path)?;
        let pid = self.exec.spawn_door(&plan.program, &plan.args, &self.pins.runtime, log)?;
        let door = Unrecorded::adopt(pid, log_path.clone());
        let Some(starttime) = door.starttime else {
            return Err(door.refuse("it exited before its checks".into()));
        };
        let banner = await_banner(&log_path, pid).map_err(|why| door.refuse(why))?;
        let mut entry = Entry {
            launch_id: id.clone(),
            pid,
            starttime,
            addr: banner.addr.to_string(),
            token_file: banner.token_file.to_string_lossy().into_owned(),
            session_id: None,
            cwd: plan.cwd.to_string_lossy().into_owned(),
            log: log_path.to_string_lossy().into_owned(),
        };
        verify(&self.pins, &entry).map_err(|check| door.refuse(check.why()))?;
        self.store.update(|es| es.push(entry.clone())).map_err(|e| door.refuse(e.to_string()))?;
        let pidfd = door.pidfd.ok().flatten().ok_or_else(|| DoorsError::Unwatched {
            launch_id: id.clone(),
            why: "its pidfd is not open".into(),
        })?;
        let first = match open_stream(&self.pins.runtime, &entry) {
            Ok((events, session)) => {
                entry.session_id = Some(session.clone());
                self.store
                    .update(|es| {
                        for e in es.iter_mut().filter(|e| e.launch_id == id) {
                            e.session_id = Some(session.clone());
                        }
                    })
                    .map_err(|e| DoorsError::Unwatched { launch_id: id.clone(), why: e.to_string() })?;
                Some(events)
            }
            Err(_) => None,
        };
        let watch = self
            .start_watch(entry, banner.addr, first, pidfd)
            .map_err(|e| DoorsError::Unwatched { launch_id: id.clone(), why: e.to_string() })?;
        self.watches.insert(id.clone(), watch);
        Ok(id)
    }

    /// The ids of the doors whose watch runs. An ended watch is not watched.
    pub fn launch_ids(&self) -> Vec<String> {
        self.watches.iter().filter(|(_, w)| !w.ended()).map(|(id, _)| id.clone()).collect()
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
    /// stands. A turn that is not idle keeps the mark armed.
    pub fn close_due(&mut self, id: &str) -> Result<Close, DoorsError> {
        if !self.watch(id)?.link.state.lock().unwrap().armed {
            return Ok(Close::Revoked);
        }
        match self.close_now(id)? {
            Close::Busy => Ok(Close::StaysArmed),
            other => Ok(other),
        }
    }

    /// Stop now: cancels the turn if one runs, waits for it to end, then closes.
    /// A door that fails its checks is dropped before the cancel, so its token
    /// is never sent to whatever answers at its address. An idle door answers
    /// the cancel with 409, which is its own word that it is idle, so it closes
    /// at once.
    pub fn stop_now(&mut self, id: &str) -> Result<Close, DoorsError> {
        let entry = self.watch(id)?.entry.clone();
        if verify(&self.pins, &entry).is_err() {
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
        let dialog = asks::dialog(&ask, label);
        Ok(Prepared { launch_id: id.to_owned(), ask_id, label: label.to_owned(), dialog })
    }

    /// Sends a prepared answer to the door that carried its ask, once the door
    /// passes its checks. The dialog is built again from the stored ask, and
    /// `shown` must permit it (`asks::permits`): a yes goes only under the digest
    /// of the dialog on screen, and a capped yes only from the app's own pane.
    pub fn send_answer(&self, prepared: &Prepared, shown: Shown) -> Result<Sent, DoorsError> {
        let entry = self.watch(&prepared.launch_id)?.entry.clone();
        verify(&self.pins, &entry).map_err(|check| {
            DoorsError::Ask(format!("the door failed its checks, so nothing was sent: {}", check.why()))
        })?;
        let fresh = self.prepare_answer(&prepared.launch_id, prepared.ask_id, &prepared.label)?;
        asks::permits(fresh.dialog(), shown).map_err(DoorsError::Ask)?;
        match self.door(&prepared.launch_id)?.answer(prepared.ask_id, &prepared.label, None)? {
            AnswerOutcome::Answered => Ok(Sent::Answered),
            AnswerOutcome::NotPending => Ok(Sent::NotPending),
            AnswerOutcome::Rejected { reason } => Ok(Sent::Rejected { reason }),
        }
    }

    fn watch(&self, id: &str) -> Result<&Watch, DoorsError> {
        self.watches
            .get(id)
            .filter(|w| !w.ended())
            .ok_or_else(|| DoorsError::Unknown(format!("no watched door has launch id {id}")))
    }

    fn door(&self, id: &str) -> Result<Door, DoorsError> {
        let watch = self.watch(id)?;
        Ok(Door::new(watch.addr, &self.pins.runtime, Path::new(&watch.entry.token_file))?)
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

    /// The close, in order. The pidfd the watch holds names the door. Then
    /// `verify`: an identity failure drops the record and the watch and signals
    /// nothing, and returns `Close::Dropped`; any other failure keeps both, signals
    /// nothing, and returns `DoorsError::Unverified`. Under the watch's lock, a turn
    /// that is not `Idle` is `Busy`, and the watch is left as it was. An idle turn
    /// marks the watch closing and shuts its stream, so the door can drain. SIGTERM
    /// goes through the pidfd, and the exit is awaited for `EXIT_BOUND`. An exit
    /// removes the record and the watch. A door still live clears the mark, and
    /// keeps its record and its watch.
    fn close_now(&mut self, id: &str) -> Result<Close, DoorsError> {
        let (entry, link, pidfd) = {
            let watch = self.watch(id)?;
            (watch.entry.clone(), watch.link.clone(), watch.pidfd.clone())
        };
        match verify(&self.pins, &entry) {
            Ok(()) => {}
            Err(Check::Identity(why)) => {
                self.drop_watch(id)?;
                return Ok(Close::Dropped { pid: entry.pid, why: visible(&why) });
            }
            Err(Check::Other(why)) => return Err(DoorsError::Unverified(visible(&why))),
        }
        {
            let mut shared = link.state.lock().unwrap();
            if shared.turn != TurnState::Idle {
                return Ok(Close::Busy);
            }
            shared.closing = true;
            if let Some(closer) = shared.closer.take() {
                let _ = closer.close();
            }
        }
        link.moved.notify_all();
        if let Err(e) = terminate_pidfd(&pidfd) {
            if e.raw_os_error() != Some(libc::ESRCH) {
                clear_closing(&link);
                return Err(e.into());
            }
        }
        if !exited_by(entry.pid, entry.starttime, Instant::now() + EXIT_BOUND) {
            clear_closing(&link);
            return Ok(Close::StillLive);
        }
        let watch_joined = self.drop_watch(id)?;
        Ok(Close::Exited { watch_joined })
    }

    /// Ends one watch, and removes its record. Reports whether its thread ended
    /// within `WATCH_BOUND`.
    fn drop_watch(&mut self, id: &str) -> Result<bool, DoorsError> {
        let mut watch = self
            .watches
            .remove(id)
            .ok_or_else(|| DoorsError::Unknown(format!("no watched door has launch id {id}")))?;
        let joined = watch.detach();
        self.store.update(|es| es.retain(|e| e.launch_id != id))?;
        Ok(joined)
    }

    /// Re-attaches one record: its checks, then its stream, then its watch. A
    /// check that fails for any reason but identity still starts the watch,
    /// unattached, and the watch reports the failure and tries again. The pidfd is
    /// opened before the checks, so they describe the process it names.
    fn reattach(&mut self, entry: &Entry) -> Result<(), Check> {
        let pidfd = match open_pidfd(entry.pid) {
            Ok(fd) => Some(fd),
            Err(e) if e.raw_os_error() == Some(libc::ESRCH) => None,
            Err(e) => return Err(Check::Other(format!("its pidfd could not be opened ({e})"))),
        };
        let stream = match verify(&self.pins, entry) {
            Ok(()) => open_stream(&self.pins.runtime, entry).map_err(Check::Other),
            Err(check) => Err(check),
        };
        let pidfd = pidfd.ok_or_else(|| Check::Identity("the door is not running".into()))?;
        let addr = parse_addr(&entry.addr).map_err(Check::Other)?;
        let (first, entry) = match stream {
            Ok((events, session)) => {
                let mut entry = entry.clone();
                if entry.session_id.is_none() {
                    let id = entry.launch_id.clone();
                    self.store
                        .update(|es| {
                            for e in es.iter_mut().filter(|e| e.launch_id == id) {
                                e.session_id = Some(session.clone());
                            }
                        })
                        .map_err(|e| Check::Other(e.to_string()))?;
                    entry.session_id = Some(session);
                }
                (Some(events), entry)
            }
            Err(Check::Identity(why)) => return Err(Check::Identity(why)),
            Err(Check::Other(_)) => (None, entry.clone()),
        };
        let id = entry.launch_id.clone();
        let watch = self.start_watch(entry, addr, first, pidfd).map_err(|e| Check::Other(e.to_string()))?;
        self.watches.insert(id, watch);
        Ok(())
    }

    /// Starts the watch thread, which holds the door's pidfd. With a stream open
    /// it reads that stream first; without one it begins by reconnecting.
    fn start_watch(
        &self,
        entry: Entry,
        addr: SocketAddr,
        first: Option<Events>,
        pidfd: OwnedFd,
    ) -> Result<Watch, DoorsError> {
        let link = Arc::new(Link::default());
        if let Some(events) = &first {
            link.state.lock().unwrap().closer = Some(events.closer()?);
        }
        let pidfd = Arc::new(pidfd);
        let worker = Worker {
            link: link.clone(),
            entry: entry.clone(),
            pins: self.pins.clone(),
            store: self.store.clone(),
            out: self.out.clone(),
            pidfd: pidfd.clone(),
            failing: Cell::new(false),
        };
        let thread = thread::Builder::new().spawn(move || worker.run(first))?;
        Ok(Watch { entry, addr, link, pidfd, thread: Some(thread) })
    }
}

/// The checks that need no connection, in order: the pin, the working directory,
/// the pid's state and start time, its command line, the recorded address, and
/// the token file as the path of its port. A failure of the pin, the pid, the
/// start time or the command line is an identity failure. Any other is not.
fn verify(pins: &Pins, entry: &Entry) -> Result<(), Check> {
    let pinned = check_pin(&pins.eidolon, &pins.home)
        .map_err(|e| Check::Identity(format!("the pin no longer passes: {e}")))?;
    check_cwd(Path::new(&entry.cwd)).map_err(Check::Other)?;
    let argv: Vec<OsString> = std::iter::once(pinned.path().as_os_str().to_owned())
        .chain(launch_args(Path::new(&entry.cwd)))
        .collect();
    let stat = match stat_of(entry.pid) {
        Ok(stat) => stat,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(Check::Identity("the door is not running".into())),
        Err(e) => return Err(Check::Other(format!("its stat could not be read: {e}"))),
    };
    if stat.state == 'Z' {
        return Err(Check::Identity("the door has exited and is a zombie".into()));
    }
    if stat.starttime != entry.starttime {
        return Err(Check::Identity("the pid was started at another time".into()));
    }
    let cmdline = match fs::read(format!("/proc/{}/cmdline", entry.pid)) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(Check::Identity("the door is not running".into())),
        Err(e) => return Err(Check::Other(format!("its command line could not be read: {e}"))),
    };
    if cmdline != argv_bytes(&argv) {
        return Err(Check::Identity("its command line is not the launch's".into()));
    }
    let addr = parse_addr(&entry.addr).map_err(Check::Other)?;
    if Path::new(&entry.token_file) != token_path(&pins.runtime, addr.port()).as_path() {
        return Err(Check::Other("its token file is not the token path of its port".into()));
    }
    Door::new(addr, &pins.runtime, Path::new(&entry.token_file)).map(|_| ()).map_err(|e| Check::Other(e.to_string()))
}

/// Opens the door's stream and reads its `hello`, which must be first, and
/// whose session must be the recorded one when there is one.
fn open_stream(runtime: &Path, entry: &Entry) -> Result<(Events, String), String> {
    let addr = parse_addr(&entry.addr)?;
    let door = Door::new(addr, runtime, Path::new(&entry.token_file)).map_err(|e| e.to_string())?;
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

/// One door the app watches: its record, its address, its shared state and its
/// watch thread.
struct Watch {
    entry: Entry,
    addr: SocketAddr,
    link: Arc<Link>,
    /// The door's pidfd, opened before its checks. A close signals through it.
    pidfd: Arc<OwnedFd>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Doors {
    /// Ends every watch, as the app's exit does. No door is signalled.
    fn drop(&mut self) {
        for watch in self.watches.values_mut() {
            watch.detach();
        }
    }
}

impl Watch {
    /// Whether the watch thread has ended. An ended watch is not watched: its
    /// record is kept or gone, and the next open decides which.
    fn ended(&self) -> bool {
        self.link.state.lock().unwrap().exited
    }

    /// Closes the stream and waits up to 2 s for the watch thread to end. It
    /// signals nothing. Returns whether the thread ended in time.
    fn detach(&mut self) -> bool {
        {
            let mut shared = self.link.state.lock().unwrap();
            shared.stop = true;
            if let Some(closer) = shared.closer.take() {
                let _ = closer.close();
            }
        }
        self.link.moved.notify_all();
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
    /// Set by a close that is taking the door down, until it clears the mark or
    /// drops the watch. While it stands, the stream's end is the close's.
    closing: bool,
    stop: bool,
    closer: Option<Closer>,
    exited: bool,
    gone: bool,
    /// The current stream has reported an ask past `ASKS_BOUND`.
    overflowed: bool,
    /// The asks the current stream carried, by ask id: the replay on a new
    /// stream rebuilds them, and a settled ask leaves them.
    asks: BTreeMap<u64, Ask>,
}

/// Clears a close's mark, and wakes the watch thread so it resumes.
fn clear_closing(link: &Link) {
    link.state.lock().unwrap().closing = false;
    link.moved.notify_all();
}

/// Marks the watch thread as ended, however it ends.
struct Ended(Arc<Link>);

impl Drop for Ended {
    fn drop(&mut self) {
        self.0.state.lock().unwrap().exited = true;
        self.0.moved.notify_all();
    }
}

/// The watch thread's half: it reads, updates the shared state, and delivers what
/// it finds. It holds the door's pidfd, which says whether the process lives.
struct Worker {
    link: Arc<Link>,
    entry: Entry,
    pins: Pins,
    store: Store,
    out: SyncSender<Event>,
    pidfd: Arc<OwnedFd>,
    /// Whether the last reconnect failed, so `Unwatched` is sent once per streak.
    failing: Cell<bool>,
}

/// What a frame leaves the watch to deliver, once the shared state is updated.
struct Seen {
    /// Whether the frame is forwarded. An ask past `ASKS_BOUND` is not.
    forward: bool,
    /// The first ask a stream refuses, named once per stream.
    overflow: Option<u64>,
    /// Whether an armed door's turn has just ended.
    due: bool,
}

/// What a reconnect leaves the watch to do next.
enum Next {
    Stream(Events),
    /// A close took the mark while the stream opened. The watch waits for it.
    Hold,
    /// The reconnect failed for a reason other than identity. The watch tries again.
    Retry,
    Stop,
}

impl Worker {
    /// Reads the first stream when one is open, then reconnects after each end
    /// while the door lives. A stream that ends while a close holds the mark is the
    /// close's: the reader waits for the mark to clear. A door that is gone takes
    /// its record with it.
    fn run(self, first: Option<Events>) {
        let _ended = Ended(self.link.clone());
        let mut next = first;
        loop {
            if let Some(events) = next.take() {
                self.drain(events);
                self.end_stream();
            }
            if self.hold() || self.pause() {
                return;
            }
            if !self.alive() {
                self.forget();
                return;
            }
            match self.reconnect() {
                Next::Stream(events) => next = Some(events),
                Next::Hold | Next::Retry => {}
                Next::Stop => return,
            }
        }
    }

    /// Whether the door's process lives, by the pidfd the watch holds.
    fn alive(&self) -> bool {
        !pidfd_exited(&self.pidfd)
    }

    /// Forwards one stream's frames, each through `deliver`, so the watch waits for
    /// room rather than losing a frame. The asks are cleared first, and the
    /// stream's replay rebuilds them.
    fn drain(&self, events: Events) {
        {
            let mut shared = self.link.state.lock().unwrap();
            shared.asks.clear();
            shared.overflowed = false;
        }
        for frame in events {
            let Ok(frame) = frame else {
                return;
            };
            if self.stopped() {
                return;
            }
            let seen = self.observe(&frame);
            if seen.forward {
                self.deliver(Event::Frame { launch_id: self.entry.launch_id.clone(), frame: Box::new(frame) });
            }
            if let Some(ask_id) = seen.overflow {
                self.deliver(Event::AsksOverflow { launch_id: self.entry.launch_id.clone(), ask_id });
            }
            if seen.due {
                self.deliver(Event::CloseDue { launch_id: self.entry.launch_id.clone() });
            }
        }
    }

    /// Records what a frame changes: the turn state, an ask, or an ask settled.
    /// Returns what the watch delivers for it.
    fn observe(&self, frame: &Frame) -> Seen {
        let mut seen = Seen { forward: true, overflow: None, due: false };
        {
            let mut shared = self.link.state.lock().unwrap();
            match frame {
                Frame::TurnState { running } => {
                    shared.turn = if *running { TurnState::Running } else { TurnState::Idle };
                    seen.due = !*running && shared.armed;
                }
                Frame::Ask(ask) => {
                    let id = asks::ask_id(ask);
                    if shared.asks.contains_key(&id) || shared.asks.len() < ASKS_BOUND {
                        shared.asks.insert(id, ask.clone());
                    } else {
                        seen.forward = false;
                        if !std::mem::replace(&mut shared.overflowed, true) {
                            seen.overflow = Some(id);
                        }
                    }
                }
                Frame::AskSettled { ask_id, .. } => {
                    shared.asks.remove(ask_id);
                }
                _ => {}
            }
        }
        self.link.moved.notify_all();
        seen
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

    /// Waits while a close holds the mark. Returns whether the watch stopped.
    fn hold(&self) -> bool {
        let shared = self.link.state.lock().unwrap();
        let shared = self.link.moved.wait_while(shared, |s| s.closing && !s.stop).unwrap();
        shared.stop
    }

    /// Waits one reopen interval, or until the watch is stopped. Returns whether it was stopped.
    fn pause(&self) -> bool {
        let shared = self.link.state.lock().unwrap();
        let (shared, _) = self.link.moved.wait_timeout_while(shared, REOPEN, |s| !s.stop).unwrap();
        shared.stop
    }

    /// Re-checks the door, then opens its stream and checks its hello, before the
    /// stream is used.
    fn reconnect(&self) -> Next {
        let opened = match verify(&self.pins, &self.entry) {
            Ok(()) => open_stream(&self.pins.runtime, &self.entry).map_err(Check::Other),
            Err(check) => Err(check),
        };
        let events = match opened {
            Ok((events, _session)) => events,
            Err(check) => return self.failed(check),
        };
        let closer = match events.closer() {
            Ok(closer) => closer,
            Err(e) => return self.failed(Check::Other(e.to_string())),
        };
        let mut shared = self.link.state.lock().unwrap();
        if shared.stop {
            return Next::Stop;
        }
        if shared.closing {
            return Next::Hold;
        }
        shared.closer = Some(closer);
        self.failing.set(false);
        Next::Stream(events)
    }

    /// Handles a failed reconnect. An identity failure drops the record and ends the
    /// watch, and says so. Any other failure is reported once per streak, and the
    /// watch tries again.
    fn failed(&self, check: Check) -> Next {
        match check {
            Check::Identity(why) => {
                let id = self.entry.launch_id.clone();
                let _ = self.store.update(|es| es.retain(|e| e.launch_id != id));
                self.deliver(Event::Dropped { launch_id: id, pid: self.entry.pid, why: visible(&why) });
                Next::Stop
            }
            Check::Other(why) => {
                if !self.failing.replace(true) {
                    let launch_id = self.entry.launch_id.clone();
                    self.deliver(Event::Unwatched { launch_id, why: visible(&why) });
                }
                Next::Retry
            }
        }
    }

    /// The door is gone: its record goes, and the caller hears it.
    fn forget(&self) {
        let id = self.entry.launch_id.clone();
        let _ = self.store.update(|es| es.retain(|e| e.launch_id != id));
        self.link.state.lock().unwrap().gone = true;
        self.link.moved.notify_all();
        self.deliver(Event::Gone { launch_id: id });
    }

    /// Sends one event, waiting for room: the watch retries `try_send` each 20 ms
    /// until the channel takes it or the watch stops. A channel whose receiver is
    /// gone takes the event as sent.
    fn deliver(&self, event: Event) {
        let mut event = event;
        loop {
            match self.out.try_send(event) {
                Ok(()) | Err(TrySendError::Disconnected(_)) => return,
                Err(TrySendError::Full(back)) => {
                    if self.stopped() {
                        return;
                    }
                    event = back;
                    thread::sleep(POLL);
                }
            }
        }
    }
}

/// A door a launch has started and not yet recorded. Its pidfd names the door
/// from the moment its pid arrives, so every failure before the record signals
/// that process and no other.
struct Unrecorded {
    pid: u32,
    log: PathBuf,
    pidfd: io::Result<Option<OwnedFd>>,
    starttime: Option<u64>,
}

impl Unrecorded {
    /// Opens the pidfd and reads the start time as the pid arrives. A pid already
    /// gone has no pidfd, and nothing needs signalling.
    fn adopt(pid: u32, log: PathBuf) -> Unrecorded {
        let pidfd = match open_pidfd(pid) {
            Ok(fd) => Ok(Some(fd)),
            Err(e) if e.raw_os_error() == Some(libc::ESRCH) => Ok(None),
            Err(e) => Err(e),
        };
        let starttime = stat_of(pid).ok().map(|s| s.starttime);
        Unrecorded { pid, log, pidfd, starttime }
    }

    /// SIGTERMs the door through its pidfd, then waits up to `EXIT_BOUND` for its
    /// exit. Without a pidfd it signals nothing, and the error says so.
    fn refuse(&self, why: String) -> DoorsError {
        let why = match &self.pidfd {
            Ok(Some(fd)) => match terminate_pidfd(fd) {
                Err(e) if e.raw_os_error() != Some(libc::ESRCH) => {
                    format!("{why}; its SIGTERM through its pidfd failed: {e}")
                }
                _ => match self.starttime {
                    Some(starttime) if !exited_by(self.pid, starttime, Instant::now() + EXIT_BOUND) => {
                        format!("{why}; it did not exit within 10 s of SIGTERM")
                    }
                    _ => why,
                },
            },
            Ok(None) => why,
            Err(e) => format!("{why}; its pidfd could not be opened ({e}), so it was not signalled"),
        };
        DoorsError::Refused { pid: self.pid, log: self.log.clone(), why }
    }
}

struct Banner {
    addr: SocketAddr,
    token_file: PathBuf,
}

/// Reads the door's log from its start, once through, until both banner lines
/// are in. Both lie within the first 64 KiB, or the launch is refused at once.
/// A door that exits first, and 30 s without a banner, refuse too. Each refusal
/// quotes the log's last 4 KiB, which is data.
fn await_banner(log: &Path, pid: u32) -> Result<Banner, String> {
    let end = Instant::now() + BANNER_BOUND;
    let mut file = open_log(log).map_err(|e| refusal_with_tail(log, &format!("its log could not be read: {e}")))?;
    let mut head = Vec::new();
    loop {
        let mut chunk = [0u8; 8192];
        let read = file
            .read(&mut chunk)
            .map_err(|e| refusal_with_tail(log, &format!("its log could not be read: {e}")))?;
        let room = BANNER_HEAD - head.len();
        head.extend_from_slice(&chunk[..read.min(room)]);
        if let Some(banner) = parse_banner(&head).map_err(|why| refusal_with_tail(log, &why))? {
            return Ok(banner);
        }
        if head.len() == BANNER_HEAD {
            return Err(refusal_with_tail(log, "its banner is not within the first 64 KiB of its log"));
        }
        if read == 0 {
            if !running(pid) {
                return Err(refusal_with_tail(log, "it exited before its banner"));
            }
            if Instant::now() >= end {
                return Err(refusal_with_tail(log, "no banner came within 30 s"));
            }
            thread::sleep(POLL);
        }
    }
}

/// The banner from the head's complete lines: the address and the token path.
/// None until both lines are in. A line that is in and is refused is an error.
fn parse_banner(head: &[u8]) -> Result<Option<Banner>, String> {
    let Some(last) = head.iter().rposition(|&b| b == b'\n') else {
        return Ok(None);
    };
    let text = String::from_utf8_lossy(&head[..=last]);
    let addr = text.lines().find_map(|l| l.strip_prefix("listening on http://")?.strip_suffix('/'));
    let token = text.lines().find_map(|l| l.strip_prefix("token file: "));
    let (Some(addr), Some(token)) = (addr, token) else {
        return Ok(None);
    };
    let addr = addr
        .parse::<SocketAddr>()
        .map_err(|_| format!("the banner's address {} is not a socket address", visible(addr)))?;
    if token.is_empty() || !token.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(format!("the banner's token path {} is not graphic ASCII", visible(token)));
    }
    Ok(Some(Banner { addr, token_file: PathBuf::from(token) }))
}

fn refusal_with_tail(log: &Path, why: &str) -> String {
    format!("{why}; its log's last 4 KiB reads: {}", log_tail(log))
}

fn log_tail(log: &Path) -> String {
    let Ok(mut file) = open_log(log) else {
        return "(the log could not be opened)".into();
    };
    let len = file.metadata().map_or(0, |m| m.len());
    let mut tail = Vec::new();
    if file.seek(SeekFrom::Start(len.saturating_sub(TAIL as u64))).is_ok() {
        let _ = file.read_to_end(&mut tail);
    }
    visible(&String::from_utf8_lossy(&tail))
}

fn open_log(log: &Path) -> io::Result<File> {
    OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW).open(log)
}

/// Whether the door's pid is still the door: not a zombie, and started at `starttime`.
fn live(pid: u32, starttime: u64) -> bool {
    stat_of(pid).is_ok_and(|s| s.state != 'Z' && s.starttime == starttime)
}

fn running(pid: u32) -> bool {
    stat_of(pid).is_ok_and(|s| s.state != 'Z')
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
        thread::sleep(POLL);
    }
}

/// The state and start time of `/proc/<pid>/stat`. Its fields are read after
/// the last `)`, since the command name may hold one.
struct Stat {
    state: char,
    starttime: u64,
}

fn stat_of(pid: u32) -> io::Result<Stat> {
    let malformed = || io::Error::new(io::ErrorKind::InvalidData, "the stat line is malformed");
    let text = fs::read_to_string(format!("/proc/{pid}/stat"))?;
    let after = text.rfind(')').ok_or_else(malformed)?;
    let fields: Vec<&str> = text[after + 1..].split_whitespace().collect();
    let state = fields.first().and_then(|f| f.chars().next()).ok_or_else(malformed)?;
    let starttime = fields.get(19).and_then(|f| f.parse::<u64>().ok()).ok_or_else(malformed)?;
    Ok(Stat { state, starttime })
}

/// The command line as `/proc` holds it: each argument followed by a NUL.
fn argv_bytes(argv: &[OsString]) -> Vec<u8> {
    argv.iter().flat_map(|arg| arg.as_bytes().iter().copied().chain(std::iter::once(0))).collect()
}

fn parse_addr(text: &str) -> Result<SocketAddr, String> {
    text.parse().map_err(|_| "the recorded address is not a socket address".to_owned())
}

/// Makes `path` a directory the app writes under. Each directory this creates,
/// and `path` itself, must be a real directory owned by the euid at mode 0700,
/// or the open is refused. A link in the way is refused.
fn make_private_dir(path: &Path) -> Result<(), DoorsError> {
    let mut missing = Vec::new();
    let mut at = path;
    loop {
        match fs::symlink_metadata(at) {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    return Err(store_refusal(at, "is a link"));
                }
                break;
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                missing.push(at);
                match at.parent() {
                    Some(parent) => at = parent,
                    None => break,
                }
            }
            Err(e) => return Err(e.into()),
        }
    }
    for dir in missing.iter().rev() {
        match DirBuilder::new().mode(0o700).create(dir) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
    }
    for dir in missing.iter().rev().copied().chain(std::iter::once(path)) {
        check_private(dir)?;
    }
    Ok(())
}

/// Refuses a path that is not, at this moment, a directory owned by the euid at
/// mode 0700, and any link.
fn check_private(path: &Path) -> Result<(), DoorsError> {
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(store_refusal(path, "is not a directory, or is a link"));
    }
    if meta.uid() != euid() || meta.mode() & 0o777 != 0o700 {
        return Err(store_refusal(path, "is not owned by this user at mode 0700"));
    }
    Ok(())
}

fn store_refusal(path: &Path, what: &str) -> DoorsError {
    DoorsError::Store(format!("{} {what}, and the app refuses to use it", path.display()))
}

fn open_dir(path: &Path) -> io::Result<File> {
    OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY).open(path)
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
