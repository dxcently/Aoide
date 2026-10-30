//! The one crate in the desktop workspace that starts a process or sends a
//! signal. Every child the app runs goes through here: its program is pinned
//! by `check_pin`, its argv is built against the schema the program prints,
//! its environment is scrubbed by `child_env` (which refuses every verb but
//! `schema` and `config` while the vault names are unknown), it runs directly,
//! never through a shell, and each call runs under one deadline.

use std::collections::BTreeMap;
use std::ffi::{CString, OsStr, OsString};
use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use aoide_protocol::untrusted::visible;
use serde::Deserialize;

/// The app reaches aoide-protocol only through this crate, and of it only
/// `untrusted`.
pub use aoide_protocol::untrusted;

/// The most a child may write to one stream before its call is refused.
pub const OUTPUT_CAP: usize = 16 * 1024 * 1024;
/// The deadline a call runs under, unless a test shortens it.
const DEADLINE: Duration = Duration::from_secs(30);
/// How long a child that was sent SIGTERM at its deadline has to exit.
const EXIT_BOUND: Duration = Duration::from_secs(5);
/// How often a call checks whether its child has exited.
const POLL: Duration = Duration::from_millis(10);
/// The names a vault key may not take: the app's own variables.
const RESERVED: [&str; 4] = ["PATH", "HOME", "XDG_RUNTIME_DIR", "AOIDE_ROOT"];

/// Why a program path was refused before it could be run.
#[derive(Debug)]
pub enum PinError {
    /// The path is not absolute, so no pin can hold it.
    Relative(PathBuf),
    /// The path does not exist, or its target cannot be read.
    Missing { path: PathBuf, source: io::Error },
    /// The target is not a regular file this user can execute.
    NotExecutable(PathBuf),
    /// The target lies under `<home>/.local/bin`, which is never pinned.
    InLocalBin(PathBuf),
    /// `path`, the target or a directory above it, is writable by group or other.
    Writable(PathBuf),
    /// `path`, the target or a directory above it, is owned by `uid`, which is
    /// neither root nor this user.
    Owner { path: PathBuf, uid: u32 },
    /// The target has its setuid or setgid bit set, so it would run as another user.
    SetId(PathBuf),
}

impl fmt::Display for PinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Relative(path) => {
                write!(f, "the program path {} is relative; only absolute paths are pinned", path.display())
            }
            Self::Missing { path, source } => {
                write!(f, "the program {} does not exist: {source}", path.display())
            }
            Self::NotExecutable(path) => {
                write!(f, "{} is not an executable regular file for this user", path.display())
            }
            Self::InLocalBin(path) => {
                write!(f, "{} lies under .local/bin in the home directory, which is never pinned", path.display())
            }
            Self::Writable(path) => write!(
                f,
                "{} is writable by group or other; a pin needs a path no other user can replace",
                path.display()
            ),
            Self::Owner { path, uid } => {
                write!(f, "{} is owned by uid {uid}, which is neither root nor this user", path.display())
            }
            Self::SetId(path) => {
                write!(f, "{} has its setuid or setgid bit set, so it would run as another user", path.display())
            }
        }
    }
}

/// A program path that `check_pin` has checked. Only `check_pin` makes one, so a
/// spawn takes a pinned program or nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pinned(PathBuf);

impl Pinned {
    /// The canonical path of the checked program.
    pub fn path(&self) -> &Path {
        &self.0
    }
}

/// Checks a program path before it is run, and returns it pinned at its canonical path.
/// Every directory above the path, up to `/`, is checked as well (see `check_owned`).
pub fn check_pin(path: &Path, home: &Path) -> Result<Pinned, PinError> {
    if !path.is_absolute() {
        return Err(PinError::Relative(path.to_path_buf()));
    }
    let canonical = fs::canonicalize(path).map_err(|source| PinError::Missing {
        path: path.to_path_buf(),
        source,
    })?;
    if !fs::metadata(&canonical).is_ok_and(|m| m.is_file()) || !can_execute(&canonical) {
        return Err(PinError::NotExecutable(canonical));
    }
    if let Ok(local_bin) = fs::canonicalize(home.join(".local/bin")) {
        if canonical.starts_with(&local_bin) {
            return Err(PinError::InLocalBin(canonical));
        }
    }
    let uid = euid();
    for ancestor in canonical.ancestors() {
        check_owned(ancestor, uid)?;
    }
    Ok(Pinned(canonical))
}

/// Refuses a file or directory another user could replace: owned by a uid that is
/// neither root nor `uid`, or writable by group or other. A directory whose sticky
/// bit is set may be writable by other, as `/tmp` is; a file with its setuid or
/// setgid bit set is refused.
fn check_owned(path: &Path, uid: u32) -> Result<(), PinError> {
    let meta = fs::metadata(path).map_err(|source| PinError::Missing { path: path.to_path_buf(), source })?;
    let mode = meta.mode();
    let sticky_dir = meta.is_dir() && mode & 0o1000 != 0;
    if mode & 0o022 != 0 && !sticky_dir {
        return Err(PinError::Writable(path.to_path_buf()));
    }
    if !meta.is_dir() && mode & 0o6000 != 0 {
        return Err(PinError::SetId(path.to_path_buf()));
    }
    if meta.uid() != 0 && meta.uid() != uid {
        return Err(PinError::Owner { path: path.to_path_buf(), uid: meta.uid() });
    }
    Ok(())
}

fn can_execute(path: &Path) -> bool {
    let Ok(c) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: `c` is NUL-terminated and lives across the call.
    unsafe { libc::access(c.as_ptr(), libc::X_OK) == 0 }
}

/// One command as `aoide schema --json` declares it. A document that omits a mark
/// reads it as false, and omitted `doors` reads as the CLI's pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub args: Vec<Arg>,
    /// Each flag's name, with its declared type (`bool`, `string`, `int`, ...).
    pub flags: BTreeMap<String, String>,
    /// The one-line brief: `brief` when the schema gives one, else `summary`.
    pub brief: String,
    /// The doors that may call it, by their names (`cli`, `daemon`, ...).
    pub doors: Vec<String>,
    /// The verb approves or denies a hold, re-parents, or types into a session.
    pub approves: bool,
    /// The verb starts an agent, or arms something that starts a program later.
    pub launches: bool,
    /// Hook plumbing that a harness drives, not an operator command.
    pub internal: bool,
    /// False for a stub. An omitted `implemented` reads false too.
    pub implemented: bool,
    /// The command's kind, when the schema names one.
    pub kind: Option<String>,
}

/// One positional argument of a command.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Arg {
    pub name: String,
    #[serde(default)]
    pub required: bool,
}

/// The commands a program declares, keyed by their path words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schema {
    commands: BTreeMap<Vec<String>, CommandSpec>,
}

#[derive(Deserialize)]
struct SchemaDoc {
    commands: Vec<CommandDoc>,
}

#[derive(Deserialize)]
struct CommandDoc {
    path: Vec<String>,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    brief: String,
    #[serde(default)]
    args: Vec<Arg>,
    #[serde(default)]
    flags: Vec<FlagDoc>,
    #[serde(default)]
    doors: Option<Vec<String>>,
    #[serde(default)]
    approves: bool,
    #[serde(default)]
    launches: bool,
    #[serde(default)]
    internal: bool,
    #[serde(default)]
    implemented: bool,
    #[serde(default)]
    kind: Option<String>,
}

#[derive(Deserialize)]
struct FlagDoc {
    name: String,
    #[serde(rename = "type")]
    kind: String,
}

impl Schema {
    /// Parses the document `aoide schema --json` prints.
    pub fn parse(text: &str) -> Result<Schema, ExecError> {
        let doc: SchemaDoc = serde_json::from_str(text).map_err(|e| ExecError::Schema(e.to_string()))?;
        let commands = doc
            .commands
            .into_iter()
            .map(|c| {
                let flags = c.flags.into_iter().map(|f| (f.name, f.kind)).collect();
                let brief = if c.brief.is_empty() { c.summary } else { c.brief };
                let doors = c.doors.unwrap_or_else(|| vec!["cli".to_owned(), "daemon".to_owned()]);
                let spec = CommandSpec {
                    args: c.args,
                    flags,
                    brief,
                    doors,
                    approves: c.approves,
                    launches: c.launches,
                    internal: c.internal,
                    implemented: c.implemented,
                    kind: c.kind,
                };
                (c.path, spec)
            })
            .collect();
        Ok(Schema { commands })
    }

    /// Runs `aoide schema --json` once, with the environment a child gets.
    fn load(aoide: &Pinned, base_env: &[(OsString, OsString)]) -> Result<Schema, ExecError> {
        let env = child_env(Child::Aoide, "schema", base_env, Err("the vault names are not read yet"))?;
        let args = [OsString::from("schema"), OsString::from("--json")];
        let out = run(aoide, &args, &env, None, "schema --json", DEADLINE)?;
        if !out.status.success() {
            return Err(bootstrap("schema --json", &out));
        }
        let text = std::str::from_utf8(&out.stdout).map_err(|e| ExecError::Schema(e.to_string()))?;
        Schema::parse(text)
    }

    /// The command at `words`, such as `["send"]`. A word with a space is not a path.
    pub fn command(&self, words: &[&str]) -> Option<&CommandSpec> {
        let key: Vec<String> = words.iter().map(|w| (*w).to_owned()).collect();
        self.commands.get(&key)
    }

    /// Every command the schema declares, in path order.
    pub fn commands(&self) -> impl Iterator<Item = (&[String], &CommandSpec)> {
        self.commands.iter().map(|(path, spec)| (path.as_slice(), spec))
    }
}

/// One flag of a call: a bare flag, or a valued one written `--name=value`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flag<'a> {
    pub name: &'a str,
    pub value: Option<&'a str>,
}

/// The argv for a call: `[aoide, <words>, --json, <flags>, (-- <positionals>)]`.
pub fn argv(
    aoide: &Path,
    schema: &Schema,
    words: &[&str],
    flags: &[Flag<'_>],
    positionals: &[&str],
) -> Result<Vec<OsString>, ExecError> {
    for word in words {
        nul(word, "a command word")?;
    }
    for flag in flags {
        nul(flag.name, "a flag name")?;
        if let Some(value) = flag.value {
            nul(value, "a flag value")?;
        }
    }
    for text in positionals {
        nul(text, "a positional")?;
    }

    let path = words.join(" ");
    let command = schema
        .command(words)
        .ok_or_else(|| ExecError::UnknownCommand(path.clone()))?;
    resolve(schema, words, positionals)?;
    let mut line = vec![aoide.as_os_str().to_owned()];
    line.extend(words.iter().map(|w| OsString::from(*w)));
    line.push(OsString::from("--json"));
    for flag in flags {
        let kind = command.flags.get(flag.name).ok_or_else(|| ExecError::Undeclared {
            command: path.clone(),
            flag: flag.name.to_owned(),
        })?;
        match (kind == "bool", flag.value) {
            (true, None) => line.push(OsString::from(format!("--{}", flag.name))),
            (true, Some(_)) => {
                return Err(ExecError::BareFlagTakesNoValue {
                    command: path.clone(),
                    flag: flag.name.to_owned(),
                })
            }
            (false, Some(value)) => line.push(OsString::from(format!("--{}={value}", flag.name))),
            (false, None) => {
                return Err(ExecError::FlagNeedsValue {
                    command: path.clone(),
                    flag: flag.name.to_owned(),
                })
            }
        }
    }
    for (index, arg) in command.args.iter().enumerate() {
        if arg.required && index >= positionals.len() {
            return Err(ExecError::MissingArgument {
                command: path.clone(),
                name: arg.name.clone(),
            });
        }
    }
    if !positionals.is_empty() {
        line.push(OsString::from("--"));
        line.extend(positionals.iter().map(|p| OsString::from(*p)));
    }
    Ok(line)
}

/// The command aoide runs for `words` and `positionals`. aoide reads everything
/// after `--` as a positional, then takes the longest declared path those words
/// begin with. So a positional can name a longer command than `words` does: that
/// is refused here, before any child starts. On success the path is `words`.
fn resolve<'a>(schema: &Schema, words: &[&'a str], positionals: &[&str]) -> Result<Vec<&'a str>, ExecError> {
    let called = words.len();
    let all: Vec<&str> = words.iter().copied().chain(positionals.iter().copied()).collect();
    let longest = (called..=all.len()).rev().find(|&n| schema.command(&all[..n]).is_some()).unwrap_or(called);
    if longest > called {
        return Err(ExecError::Resolves { called: words.join(" "), resolved: all[..longest].join(" ") });
    }
    Ok(words.to_vec())
}

fn nul(text: &str, what: &'static str) -> Result<(), ExecError> {
    if text.contains('\0') {
        Err(ExecError::NulByte { what })
    } else {
        Ok(())
    }
}

/// Which program a child is, for its environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Child {
    Aoide,
    Eidolon,
}

/// The environment a child gets: `base`, minus what that child must not see.
/// While the vault names are unknown only `schema` and `config` run: any other
/// aoide verb could pass the vault keys on to the agent it starts.
pub fn child_env(
    child: Child,
    first_word: &str,
    base: &[(OsString, OsString)],
    vault_keys: Result<&[String], &str>,
) -> Result<Vec<(OsString, OsString)>, ExecError> {
    let keys = match (child, vault_keys) {
        (_, Ok(keys)) => keys,
        (Child::Aoide, Err(_)) if matches!(first_word, "schema" | "config") => &[],
        (_, Err(why)) => return Err(ExecError::VaultKeysUnknown(why.to_owned())),
    };
    let melete = child == Child::Aoide && first_word == "melete";
    let send = child == Child::Aoide && first_word == "send";
    Ok(base
        .iter()
        .filter(|(name, _)| !scrubbed(child, melete, send, name, keys))
        .cloned()
        .collect())
}

/// Whether a child's environment drops `name`. Names compare as bytes, so a name
/// that is not UTF-8 is scrubbed by the same rules. A `send` child drops the
/// session id, the two autogate switches and the session origin: a send the
/// person types is approved by its own `--yes`, is never node-originated, and the
/// child must not carry a way to approve another.
fn scrubbed(child: Child, melete: bool, send: bool, name: &OsStr, keys: &[String]) -> bool {
    let name = name.as_bytes();
    let melete_var = name.starts_with(b"AOIDE_MELETE_") && !melete;
    let session = name == b"AOIDE_SESSION_ID" && (child == Child::Eidolon || send);
    let autogate = send
        && (name == b"AOIDE_CONDUCT_AUTOGATE"
            || name == b"AOIDE_CONDUCT_SIBLING_AUTOGATE"
            || name == b"AOIDE_SESSION_ORIGIN");
    melete_var || session || autogate || keys.iter().any(|key| key.as_bytes() == name)
}

#[derive(Deserialize)]
struct ConfigDoc {
    data: ConfigData,
}

#[derive(Deserialize)]
struct ConfigData {
    config: ConfigBody,
}

#[derive(Deserialize)]
struct ConfigBody {
    #[serde(default)]
    context: ContextBody,
}

#[derive(Default, Deserialize)]
struct ContextBody {
    #[serde(default)]
    vaults: BTreeMap<String, VaultBody>,
}

#[derive(Deserialize)]
struct VaultBody {
    #[serde(rename = "tokenEnv")]
    token_env: String,
}

/// The `tokenEnv` names of the vaults in `aoide config --json`'s data.
pub fn parse_vault_keys(config_json: &str) -> Result<Vec<String>, ExecError> {
    let doc: ConfigDoc = serde_json::from_str(config_json).map_err(|e| ExecError::Config(e.to_string()))?;
    Ok(doc
        .data
        .config
        .context
        .vaults
        .into_values()
        .map(|vault| vault.token_env)
        .collect())
}

/// A finished child: its status, and what it wrote, exactly as it wrote it.
#[derive(Debug)]
pub struct Output {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// The aoide children the app starts, from one pinned program.
pub struct Exec {
    aoide: Pinned,
    schema: Schema,
    vault_keys: Result<Vec<String>, String>,
    base_env: Vec<(OsString, OsString)>,
    deadline: Duration,
}

impl Exec {
    /// Pins `aoide`, reads its schema, and reads its vault names. A config that
    /// cannot be read leaves the names unknown; a schema that cannot be read
    /// refuses the load.
    pub fn load(aoide: &Path, home: &Path, base_env: Vec<(OsString, OsString)>) -> Result<Exec, ExecError> {
        let aoide = check_pin(aoide, home).map_err(ExecError::Pin)?;
        let schema = Schema::load(&aoide, &base_env)?;
        let mut exec = Exec {
            aoide,
            schema,
            vault_keys: Err("the config has not been read".to_owned()),
            base_env,
            deadline: DEADLINE,
        };
        exec.vault_keys = vault_names(exec.call(&["config"], &[], &[], None));
        Ok(exec)
    }

    /// Shortens the deadline a call runs under: the shorter of `deadline` and the
    /// deadline already set, so a later call never lengthens an earlier one.
    pub fn deadline(mut self, deadline: Duration) -> Exec {
        self.deadline = self.deadline.min(deadline);
        self
    }

    /// Runs one aoide command. `words` is its path, `flags` its flags, and
    /// `positionals` follow a bare `--`. `stdin` is closed unless bytes are given.
    /// A non-zero exit is an `Ok` whose `stderr` is the child's own text.
    pub fn call(
        &self,
        words: &[&str],
        flags: &[Flag<'_>],
        positionals: &[&str],
        stdin: Option<&[u8]>,
    ) -> Result<Output, ExecError> {
        let resolved = resolve(&self.schema, words, positionals)?;
        let first = resolved.first().copied().unwrap_or("");
        let env = child_env(Child::Aoide, first, &self.base_env, self.keys())?;
        let line = argv(self.aoide.path(), &self.schema, words, flags, positionals)?;
        run(&self.aoide, &line[1..], &env, stdin, &words.join(" "), self.deadline)
    }

    fn keys(&self) -> Result<&[String], &str> {
        self.vault_keys.as_deref().map_err(String::as_str)
    }

    /// The schema the program printed, for the shell's palette and its floor.
    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    /// Starts one aoide command as a held child, checked as `call` checks it. Its
    /// stdout comes back a line at a time from `Follow::next`, until `Follow::end`
    /// interrupts it. A held child runs under no deadline: it is the one that ends
    /// when the caller ends it.
    pub fn follow(&self, words: &[&str], flags: &[Flag<'_>], positionals: &[&str]) -> Result<Follow, ExecError> {
        let resolved = resolve(&self.schema, words, positionals)?;
        let first = resolved.first().copied().unwrap_or("");
        let env = child_env(Child::Aoide, first, &self.base_env, self.keys())?;
        let line = argv(self.aoide.path(), &self.schema, words, flags, positionals)?;
        spawn_held(&self.aoide, &line[1..], &env)
    }

    /// The environment a door gets: the child env for `eidolon web`, with
    /// `XDG_RUNTIME_DIR` set to the runtime dir the app checked, so the door
    /// writes its token where the app reads it.
    pub fn door_env(&self, runtime: &Path) -> Result<Vec<(OsString, OsString)>, ExecError> {
        let mut env = child_env(Child::Eidolon, "web", &self.base_env, self.keys())?;
        env.retain(|(name, _)| name.as_os_str() != "XDG_RUNTIME_DIR");
        env.push((OsString::from("XDG_RUNTIME_DIR"), runtime.as_os_str().to_owned()));
        Ok(env)
    }

    /// Starts `program` with `args` as a door: detached, in a session of its own,
    /// with stdin on /dev/null and stdout and stderr on `log`. Returns the door as
    /// it arrived, see `Arrived`. The program is the one `check_pin` pinned, and
    /// no other is started.
    pub fn spawn_door(
        &self,
        program: &Pinned,
        args: &[OsString],
        runtime: &Path,
        log: fs::File,
    ) -> Result<Arrived, ExecError> {
        let env = self.door_env(runtime)?;
        spawn_detached(program.path(), args, &env, log)
    }
}

/// The most a followed line may hold. A longer line is read to its end and
/// sent as `Event::Dropped`, so one line cannot grow a buffer without bound.
pub const LINE_CAP: usize = 1024 * 1024;
/// The most a followed child may have queued for its reader, in bytes. An event
/// costs its line's bytes and `EVENT_COST`, so a flood of empty lines is bounded too.
pub const QUEUE_BYTES: usize = 8 * 1024 * 1024;
const EVENT_COST: usize = 64;

/// The queue cost of one event.
fn cost(event: &Event) -> usize {
    match event {
        Event::Line(line) => line.len() + EVENT_COST,
        Event::Dropped(_) | Event::Eof => EVENT_COST,
    }
}

/// The bytes a followed child's queue holds. The reader waits here for room before
/// it sends, and the reader is released when the queue is closed.
#[derive(Default)]
struct Room {
    used: Mutex<RoomState>,
    freed: Condvar,
}

#[derive(Default)]
struct RoomState {
    bytes: usize,
    closed: bool,
}

impl Room {
    /// Waits until `cost` bytes fit, then takes them. False once the queue is closed.
    fn reserve(&self, cost: usize) -> bool {
        let mut state = self.used.lock().unwrap_or_else(PoisonError::into_inner);
        while !state.closed && state.bytes + cost > QUEUE_BYTES {
            state = self.freed.wait(state).unwrap_or_else(PoisonError::into_inner);
        }
        if state.closed {
            return false;
        }
        state.bytes += cost;
        true
    }

    fn release(&self, cost: usize) {
        let mut state = self.used.lock().unwrap_or_else(PoisonError::into_inner);
        state.bytes = state.bytes.saturating_sub(cost);
        self.freed.notify_all();
    }

    fn close(&self) {
        let mut state = self.used.lock().unwrap_or_else(PoisonError::into_inner);
        state.closed = true;
        self.freed.notify_all();
    }
}

/// The reader's end of a followed child's queue: each event waits for room first.
struct Out {
    tx: mpsc::Sender<Event>,
    room: Arc<Room>,
}

impl Out {
    fn send(&self, event: Event) -> Result<(), ()> {
        if !self.room.reserve(cost(&event)) {
            return Err(());
        }
        self.tx.send(event).map_err(|_| ())
    }
}

/// One thing a held child did, in order.
#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    /// One line of the child's stdout, without its newline, as it was written.
    Line(Vec<u8>),
    /// A line past `LINE_CAP`, dropped unread: its length in bytes.
    Dropped(usize),
    /// The child closed its stdout.
    Eof,
}

/// A held child, from `Exec::follow`. It runs until `end` interrupts it. A
/// `Follow` dropped before `end` is interrupted the same way, and its child is
/// reaped by a thread of its own.
pub struct Follow {
    child: Option<std::process::Child>,
    pidfd: OwnedFd,
    lines: mpsc::Receiver<Event>,
    room: Arc<Room>,
    stderr: mpsc::Receiver<io::Result<(Vec<u8>, bool)>>,
}

/// How a followed child ended: its status, the events it printed after `end`
/// was called, and its stderr.
#[derive(Debug)]
pub struct Ended {
    /// `None` when the child was still running `EXIT_BOUND` after its interrupt.
    /// It is then left running and unreaped, and `stderr` is empty.
    pub status: Option<ExitStatus>,
    /// The events from `end` on, up to the child's end of stdout or the bound.
    pub rest: Vec<Event>,
    pub stderr: Vec<u8>,
}

impl Follow {
    /// The pid of the held child.
    pub fn id(&self) -> u32 {
        self.child.as_ref().map_or(0, std::process::Child::id)
    }

    /// The next event, or `None` when none comes within `wait`.
    pub fn next(&self, wait: Duration) -> Option<Event> {
        match self.lines.recv_timeout(wait) {
            Ok(event) => {
                self.room.release(cost(&event));
                Some(event)
            }
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => Some(Event::Eof),
        }
    }

    /// Interrupts the child through its pidfd, collects what it prints until its
    /// stdout closes, and reaps it. The interrupt is SIGINT: core handles that
    /// signal alone, so a followed command prints its end line on it.
    pub fn end(mut self) -> Result<Ended, ExecError> {
        let mut child = self.child.take().expect("a followed child is held until it ends");
        let signalled = signal_pidfd(&self.pidfd, libc::SIGINT).or_else(ignore_gone);
        let until = Instant::now() + EXIT_BOUND;
        let mut rest = Vec::new();
        loop {
            match self.lines.recv_timeout(until.saturating_duration_since(Instant::now())) {
                Ok(Event::Eof) | Err(_) => break,
                Ok(event) => {
                    self.room.release(cost(&event));
                    rest.push(event);
                }
            }
        }
        let exit_by = Instant::now() + EXIT_BOUND;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) if Instant::now() < exit_by => thread::sleep(POLL),
                Ok(None) | Err(_) => break None,
            }
        };
        let stderr = match status {
            Some(_) => self.stderr.recv_timeout(EXIT_BOUND).ok().and_then(Result::ok).map(|(b, _)| b).unwrap_or_default(),
            None => {
                thread::spawn(move || {
                    let _ = child.wait();
                });
                Vec::new()
            }
        };
        signalled.map_err(ExecError::Io)?;
        Ok(Ended { status, rest, stderr })
    }
}

impl Drop for Follow {
    fn drop(&mut self) {
        self.room.close();
        if let Some(mut child) = self.child.take() {
            let _ = signal_pidfd(&self.pidfd, libc::SIGINT);
            thread::spawn(move || {
                let _ = child.wait();
            });
        }
    }
}

/// A process that has already exited is no error to interrupt.
fn ignore_gone(e: io::Error) -> io::Result<()> {
    if e.raw_os_error() == Some(libc::ESRCH) {
        Ok(())
    } else {
        Err(e)
    }
}

/// Starts `program` with `args` as a held child: stdin closed, stdout and stderr
/// piped. Its stdout is read on a thread, one line at a time, into a bounded
/// channel. Its stderr is captured to `OUTPUT_CAP` on another thread.
fn spawn_held(program: &Pinned, args: &[OsString], env: &[(OsString, OsString)]) -> Result<Follow, ExecError> {
    let mut child = Command::new(program.path())
        .args(args)
        .env_clear()
        .envs(env.iter().cloned())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(ExecError::Spawn)?;
    let pidfd = match open_pidfd(child.id()) {
        Ok(fd) => fd,
        Err(e) => {
            drop(retire_unreaped(child));
            return Err(ExecError::Io(e));
        }
    };
    let stdout = child.stdout.take().ok_or_else(missing_pipe)?;
    let stderr = child.stderr.take().ok_or_else(missing_pipe)?;
    let (tx, lines) = mpsc::channel();
    let room = Arc::new(Room::default());
    let out = Out { tx, room: Arc::clone(&room) };
    thread::spawn(move || {
        let _ = read_lines(stdout, &out);
        let _ = out.send(Event::Eof);
    });
    let (err_tx, stderr_rx) = mpsc::channel();
    thread::spawn(move || err_tx.send(capture(stderr)));
    Ok(Follow { child: Some(child), pidfd, lines, room, stderr: stderr_rx })
}

/// Ends a child this process started and has not reaped, after its pidfd would not
/// open. Nothing has reaped it, so its pid is still its own, and this is the one
/// pid signal outside `run`. The child is reaped on a thread. The handle gives its
/// exit status.
fn retire_unreaped(child: std::process::Child) -> thread::JoinHandle<io::Result<ExitStatus>> {
    let mut child = child;
    let _ = signal_unreaped(child.id(), libc::SIGINT);
    thread::spawn(move || child.wait())
}

/// Sends `signal` to `pid`, an unreaped child this process started. Only
/// `retire_unreaped` calls it, with SIGINT. It refuses the pids `terminate` refuses.
fn signal_unreaped(pid: u32, signal: libc::c_int) -> io::Result<()> {
    let pid = signal_target(pid).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "a pid to signal is 2 to i32::MAX, and not this process")
    })?;
    // SAFETY: a plain syscall taking a pid and a signal number; no memory is passed.
    if unsafe { libc::kill(pid, signal) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Splits `pipe` into lines and sends each on `out`, waiting for room in the queue. A line past `LINE_CAP` is
/// still read to its newline, but its bytes past the cap are dropped, and it is
/// sent as `Dropped`. Returns when the pipe ends or `out`'s receiver is gone.
fn read_lines(mut pipe: impl Read, out: &Out) -> io::Result<()> {
    let mut line = Vec::new();
    let mut total = 0usize;
    let mut buf = [0u8; 8192];
    loop {
        let n = match pipe.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        let mut piece = &buf[..n];
        while let Some(at) = piece.iter().position(|&b| b == b'\n') {
            absorb(&mut line, &mut total, &piece[..at]);
            let event = take_line(&mut line, &mut total);
            if out.send(event).is_err() {
                return Ok(());
            }
            piece = &piece[at + 1..];
        }
        absorb(&mut line, &mut total, piece);
    }
    if total > 0 && out.send(take_line(&mut line, &mut total)).is_err() {
        return Ok(());
    }
    Ok(())
}

/// Adds `piece` to the line: `total` counts every byte, `line` keeps up to the cap.
fn absorb(line: &mut Vec<u8>, total: &mut usize, piece: &[u8]) {
    *total += piece.len();
    let room = LINE_CAP.saturating_sub(line.len());
    line.extend_from_slice(&piece[..piece.len().min(room)]);
}

/// The finished line as its event, and the reader reset for the next one.
fn take_line(line: &mut Vec<u8>, total: &mut usize) -> Event {
    let event = if *total > LINE_CAP {
        line.clear();
        Event::Dropped(*total)
    } else {
        Event::Line(std::mem::take(line))
    };
    *total = 0;
    event
}

/// Starts a child for one call and waits for it. Only `Exec` reaches it, and only with a
/// pinned program. The call runs under `deadline` from its start, through the
/// child's exit and the reading of its output. A child still running at the
/// deadline is sent SIGTERM and given `EXIT_BOUND` to exit, and the call is
/// refused either way. A process the child started may hold an output pipe
/// after the child exits; that holds the call to the same deadline.
fn run(
    program: &Pinned,
    args: &[OsString],
    env: &[(OsString, OsString)],
    stdin: Option<&[u8]>,
    command: &str,
    deadline: Duration,
) -> Result<Output, ExecError> {
    let until = Instant::now() + deadline;
    let mut child = Command::new(program.path())
        .args(args)
        .env_clear()
        .envs(env.iter().cloned())
        .stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(ExecError::Spawn)?;
    let stdout = child.stdout.take().ok_or_else(missing_pipe)?;
    let stderr = child.stderr.take().ok_or_else(missing_pipe)?;
    if let Some((mut pipe, bytes)) = child.stdin.take().zip(stdin.map(<[u8]>::to_vec)) {
        thread::spawn(move || pipe.write_all(&bytes));
    }
    let (out_tx, out_rx) = mpsc::channel();
    let (err_tx, err_rx) = mpsc::channel();
    thread::spawn(move || out_tx.send(capture(stdout)));
    thread::spawn(move || err_tx.send(capture(stderr)));
    let Some(status) = wait_until(&mut child, until).map_err(ExecError::Io)? else {
        terminate(child.id()).map_err(ExecError::Io)?;
        let _ = wait_until(&mut child, Instant::now() + EXIT_BOUND);
        return Err(ExecError::Timeout { command: command.to_owned(), deadline });
    };
    let (stdout, stdout_over) = collect(&out_rx, until, command, deadline)?;
    let (stderr, stderr_over) = collect(&err_rx, until, command, deadline)?;
    if stdout_over {
        return Err(ExecError::OutputTooLarge { stream: "stdout" });
    }
    if stderr_over {
        return Err(ExecError::OutputTooLarge { stream: "stderr" });
    }
    Ok(Output { status, stdout, stderr })
}

/// The child's status once it exits, or `None` when `until` passes first.
fn wait_until(child: &mut std::process::Child, until: Instant) -> io::Result<Option<ExitStatus>> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Ok(None);
        }
        thread::sleep(POLL.min(left));
    }
}

/// One stream's bytes, and whether any were dropped past the cap. The reader
/// must finish by `until`.
fn collect(
    reads: &mpsc::Receiver<io::Result<(Vec<u8>, bool)>>,
    until: Instant,
    command: &str,
    deadline: Duration,
) -> Result<(Vec<u8>, bool), ExecError> {
    match reads.recv_timeout(until.saturating_duration_since(Instant::now())) {
        Ok(read) => read.map_err(ExecError::Io),
        Err(RecvTimeoutError::Timeout) => Err(ExecError::Timeout { command: command.to_owned(), deadline }),
        Err(RecvTimeoutError::Disconnected) => Err(ExecError::Io(io::Error::other("an output reader panicked"))),
    }
}

/// Reads a child's stream to its end, keeping at most `OUTPUT_CAP` bytes. Past
/// the cap the bytes are read and dropped, so the child never blocks on a full
/// pipe; the flag reports that something was dropped.
fn capture(mut pipe: impl Read) -> io::Result<(Vec<u8>, bool)> {
    let mut kept = Vec::new();
    let mut over = false;
    let mut buf = [0u8; 8192];
    loop {
        match pipe.read(&mut buf) {
            Ok(0) => return Ok((kept, over)),
            Ok(n) => {
                let room = OUTPUT_CAP - kept.len();
                over |= n > room;
                kept.extend_from_slice(&buf[..n.min(room)]);
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
}

fn missing_pipe() -> ExecError {
    ExecError::Io(io::Error::other("the child has no pipe"))
}

fn bootstrap(call: &str, out: &Output) -> ExecError {
    let why = match String::from_utf8_lossy(&out.stderr).trim() {
        "" => out.status.to_string(),
        text => text.to_owned(),
    };
    ExecError::Bootstrap {
        call: call.to_owned(),
        why,
    }
}

/// Why a call was refused, or a child could not be run.
#[derive(Debug)]
pub enum ExecError {
    /// `aoide schema --json` did not parse.
    Schema(String),
    /// `aoide config --json` did not parse.
    Config(String),
    /// A call the app makes at start failed.
    Bootstrap { call: String, why: String },
    /// The schema declares no command at this path.
    UnknownCommand(String),
    /// The command declares no flag of this name.
    Undeclared { command: String, flag: String },
    /// A bare (bool) flag was given a value.
    BareFlagTakesNoValue { command: String, flag: String },
    /// A valued flag was given no value.
    FlagNeedsValue { command: String, flag: String },
    /// A required positional argument is missing.
    MissingArgument { command: String, name: String },
    /// A word holds a NUL byte, which an argv cannot carry.
    NulByte { what: &'static str },
    /// The vault key names are unknown, so this call is refused.
    VaultKeysUnknown(String),
    /// The call did not finish within its deadline, and is refused.
    Timeout { command: String, deadline: Duration },
    /// The child wrote more than [`OUTPUT_CAP`] bytes to this stream.
    OutputTooLarge { stream: &'static str },
    /// The program path was refused.
    Pin(PinError),
    /// The program could not be started.
    Spawn(io::Error),
    /// The positionals name a longer declared command than `called`, which aoide
    /// would run as `resolved`. Refused before any child starts.
    Resolves { called: String, resolved: String },
    /// Reading the child's output or waiting on it failed.
    Io(io::Error),
}

impl fmt::Display for ExecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Schema(why) => write!(f, "aoide schema --json did not parse: {why}"),
            Self::Resolves { called, resolved } => {
                write!(f, "`{called}` with these positionals is `{resolved}`, a different command; nothing starts")
            }
            Self::Config(why) => write!(f, "aoide config --json did not parse: {why}"),
            Self::Bootstrap { call, why } => write!(f, "aoide {call} failed at start: {why}"),
            Self::UnknownCommand(path) => write!(f, "the installed aoide declares no command `{path}`"),
            Self::Undeclared { command, flag } => write!(f, "`aoide {command}` declares no flag --{flag}"),
            Self::BareFlagTakesNoValue { command, flag } => {
                write!(f, "--{flag} on `aoide {command}` is a bare switch and takes no value")
            }
            Self::FlagNeedsValue { command, flag } => write!(f, "--{flag} on `aoide {command}` needs a value"),
            Self::MissingArgument { command, name } => write!(f, "`aoide {command}` needs {name}"),
            Self::NulByte { what } => write!(f, "{what} holds a NUL byte, which a command line cannot carry"),
            Self::VaultKeysUnknown(why) => write!(
                f,
                "the vault key names are unknown ({why}), so this call is refused: only schema and config run until aoide config reads"
            ),
            Self::Timeout { command, deadline } => {
                write!(f, "aoide {command} did not finish within {deadline:?}; the call is refused and nothing is retried")
            }
            Self::OutputTooLarge { stream } => {
                write!(f, "the child wrote more than {OUTPUT_CAP} bytes to {stream}, so the call is refused")
            }
            Self::Pin(pin) => write!(f, "{pin}"),
            Self::Spawn(e) => write!(f, "the program could not be started: {e}"),
            Self::Io(e) => write!(f, "reading the program's output failed: {e}"),
        }
    }
}

impl std::error::Error for ExecError {}

/// Starts `program` with `args` in a session of its own, outside this process's
/// tree.
///
/// Between fork and exec, in order: std's `pre_exec` makes std fork, so the
/// closure runs in the intermediate, a one-thread child of a larger process
/// whose atfork handlers have run. The closure calls `setsid`, then `libc::fork`
/// for the door. The intermediate writes the door's pid to the pipe and exits
/// zero. If the write fails, it signals the door and exits non-zero. A door that
/// ignores that SIGTERM runs unnamed. `write`, `kill`, `_exit` and `fork` are
/// async-signal-safe, and the closure allocates nothing. libc 0.2.189 binds no
/// `_Fork`, so the second fork is `libc::fork`: it is safe here because the
/// intermediate has one thread when the closure runs. Std's close-on-exec pipe
/// carries the door's own exec failure back to the caller as `spawn`'s error.
/// The door is pinned as its pid arrives, before the intermediate is reaped.
fn spawn_detached(
    program: &Path,
    args: &[OsString],
    env: &[(OsString, OsString)],
    log: fs::File,
) -> Result<Arrived, ExecError> {
    let (mut pid_in, pid_out) = io::pipe().map_err(ExecError::Io)?;
    let pid_fd = pid_out.as_raw_fd();
    let stdout = log.try_clone().map_err(ExecError::Io)?;
    let mut command = Command::new(program);
    command
        .args(args)
        .env_clear()
        .envs(env.iter().cloned())
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(log));
    // SAFETY: the closure makes only async-signal-safe calls and allocates
    // nothing; `pid_fd` stays open until the parent drops `pid_out`, after spawn.
    unsafe {
        command.pre_exec(move || {
            if libc::setsid() < 0 {
                return Err(io::Error::last_os_error());
            }
            match libc::fork() {
                -1 => Err(io::Error::last_os_error()),
                0 => Ok(()),
                door => {
                    let bytes = (door as u32).to_ne_bytes();
                    if libc::write(pid_fd, bytes.as_ptr().cast(), bytes.len()) == bytes.len() as isize {
                        libc::_exit(0)
                    }
                    libc::kill(door, libc::SIGTERM);
                    libc::_exit(1)
                }
            }
        });
    }
    let mut intermediate = command.spawn().map_err(ExecError::Spawn)?;
    drop(pid_out);
    let mut bytes = [0u8; 4];
    let arrived = pid_in.read_exact(&mut bytes).map(|()| arrive(u32::from_ne_bytes(bytes)));
    let _ = intermediate.wait();
    arrived.map_err(ExecError::Io)
}

/// The pid `kill` and `pidfd_open` take for `pid`, if it names a process this
/// module may signal: not 0 or 1, not past `i32::MAX`, and not this process.
fn signal_target(pid: u32) -> Option<i32> {
    i32::try_from(pid).ok().filter(|p| *p > 1 && pid != std::process::id())
}

/// Sends SIGTERM to `pid`, an unreaped child this process started. Its one
/// production caller is `run`, which signals a child it has not reaped, so the
/// pid is still that child's. Doors are signalled through `terminate_pidfd`. This
/// refuses the values `kill` would read as a process group, as init, as this
/// process, or as no process at all.
pub(crate) fn terminate(pid: u32) -> io::Result<()> {
    let pid = signal_target(pid).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "a pid to signal is 2 to i32::MAX, and not this process")
    })?;
    // SAFETY: a plain syscall taking a pid and a signal number; no memory is passed.
    if unsafe { libc::kill(pid, libc::SIGTERM) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// The effective uid of this process. The app's private directories belong to it.
pub fn euid() -> u32 {
    // SAFETY: geteuid takes no arguments and cannot fail.
    unsafe { libc::geteuid() }
}

/// A pidfd for `pid`: a descriptor that names one process, so a signal sent
/// through it cannot reach a process that later takes the number.
pub fn open_pidfd(pid: u32) -> io::Result<OwnedFd> {
    let pid = signal_target(pid).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "a pid to open is 2 to i32::MAX, and not this process")
    })?;
    // SAFETY: a plain syscall taking a pid and flags; no memory is passed.
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0 as libc::c_uint) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let fd = libc::c_int::try_from(fd).map_err(io::Error::other)?;
    // SAFETY: the syscall returned `fd` as a new descriptor that nothing else owns.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

/// A door as its launch first sees it: the pid the intermediate wrote, a pidfd
/// naming that pid, and its start time. A pid already gone has no pidfd and no
/// start time.
pub struct Arrived {
    pub pid: u32,
    pub pidfd: io::Result<Option<OwnedFd>>,
    pub starttime: Option<u64>,
}

/// Pins `pid` as it arrives. The pidfd names whichever process holds `pid` at
/// this call, so a door reaped before this call can lose its pid to another
/// process. The window is from the intermediate's write to this call.
fn arrive(pid: u32) -> Arrived {
    let pidfd = match open_pidfd(pid) {
        Ok(fd) => Ok(Some(fd)),
        Err(e) if e.raw_os_error() == Some(libc::ESRCH) => Ok(None),
        Err(e) => Err(e),
    };
    let starttime = stat_of(pid).ok().map(|s| s.starttime);
    Arrived { pid, pidfd, starttime }
}

/// The state and start time of `/proc/<pid>/stat`. Its fields are read after
/// the last `)`, since the command name may hold one.
pub struct Stat {
    pub state: char,
    pub starttime: u64,
}

pub fn stat_of(pid: u32) -> io::Result<Stat> {
    let malformed = || io::Error::new(io::ErrorKind::InvalidData, "the stat line is malformed");
    let text = fs::read_to_string(format!("/proc/{pid}/stat"))?;
    let after = text.rfind(')').ok_or_else(malformed)?;
    let fields: Vec<&str> = text[after + 1..].split_whitespace().collect();
    let state = fields.first().and_then(|f| f.chars().next()).ok_or_else(malformed)?;
    let starttime = fields.get(19).and_then(|f| f.parse::<u64>().ok()).ok_or_else(malformed)?;
    Ok(Stat { state, starttime })
}

/// Whether the process `fd` names has exited. A pidfd is readable once its
/// process ends, so a zero-timeout poll answers without reading any pid.
pub fn pidfd_exited(fd: &OwnedFd) -> bool {
    let mut watch = libc::pollfd { fd: fd.as_raw_fd(), events: libc::POLLIN, revents: 0 };
    // SAFETY: `watch` is one valid pollfd, and the count names exactly one.
    let ready = unsafe { libc::poll(&mut watch, 1, 0) };
    ready > 0 && (watch.revents & libc::POLLIN) != 0
}

/// Sends SIGTERM to the process `fd` names, and to no other.
pub fn terminate_pidfd(fd: &OwnedFd) -> io::Result<()> {
    signal_pidfd(fd, libc::SIGTERM)
}

/// Sends `signal` to the process `fd` names, and to no other.
fn signal_pidfd(fd: &OwnedFd, signal: libc::c_int) -> io::Result<()> {
    // SAFETY: a plain syscall taking a descriptor, a signal and a null siginfo.
    let sent = unsafe {
        libc::syscall(
            libc::SYS_pidfd_send_signal,
            fd.as_raw_fd(),
            signal,
            std::ptr::null::<libc::siginfo_t>(),
            0 as libc::c_uint,
        )
    };
    if sent == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// A FIFO at `path`, for the door's token-path test. Fakes only: the app makes
/// no FIFO of its own, and the door crate may not call libc.
#[cfg(feature = "fakes")]
pub fn make_fifo(path: &Path) -> io::Result<()> {
    let c = CString::new(path.as_os_str().as_bytes()).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    // SAFETY: `c` is a NUL-terminated path that lives across the call.
    if unsafe { libc::mkfifo(c.as_ptr(), 0o600) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// What the door tests need, started and signalled the way the app does it.
#[cfg(feature = "fakes")]
pub mod test_doors {
    use std::ffi::OsString;
    use std::fs;
    use std::io;
    use std::path::Path;

    use super::{Exec, ExecError, Pinned};

    /// Starts `program` with `args` as a door, as `Exec::spawn_door` does.
    pub fn spawn(
        exec: &Exec,
        program: &Pinned,
        args: &[OsString],
        runtime: &Path,
        log: fs::File,
    ) -> Result<u32, ExecError> {
        exec.spawn_door(program, args, runtime, log).map(|arrived| arrived.pid)
    }

    /// Sends SIGTERM to `pid`, as the app does to a child it started.
    pub fn terminate(pid: u32) -> io::Result<()> {
        super::terminate(pid)
    }

    /// Copies `from` to `to` through a child, `cat "$1" > "$2"`, which must succeed.
    /// This process never holds a writer of a program it runs: a concurrent test's
    /// fork would carry the writer until its own exec, and the program's exec would
    /// fail with ETXTBSY. The copy's mode is the one the child's write leaves.
    pub fn copy_program(from: &Path, to: &Path) -> io::Result<()> {
        let status = std::process::Command::new("/bin/sh")
            .args(["-c", "cat \"$1\" > \"$2\"", "sh"])
            .arg(from)
            .arg(to)
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(io::Error::other(format!("the copy of {} failed: {status}", from.display())))
        }
    }

    /// Runs `program` with no arguments as a child of this process, and leaves
    /// it unreaped. Returns its pid once it is a zombie, so the pid has exited.
    pub fn zombie(program: &Path) -> io::Result<u32> {
        let child = std::process::Command::new(program)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
        let pid = child.id();
        std::mem::forget(child);
        let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !is_zombie(pid) {
            if std::time::Instant::now() >= end {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "the child did not exit within 10 s"));
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        Ok(pid)
    }

    fn is_zombie(pid: u32) -> bool {
        fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|text| {
            text.rfind(')').and_then(|at| text[at + 1..].split_whitespace().next()) == Some("Z")
        })
    }

    /// The set that holds `signal` alone.
    fn one_set(signal: libc::c_int) -> libc::sigset_t {
        // SAFETY: a zeroed sigset_t is valid, and sigemptyset initializes it before sigaddset uses it.
        unsafe {
            let mut set: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut set);
            libc::sigaddset(&mut set, signal);
            set
        }
    }

    /// Blocks `signal` for this thread and every thread it starts later, so that
    /// `wait_signal` takes it. Both calls return an error number, not errno.
    fn block_signal(signal: libc::c_int) -> io::Result<()> {
        let set = one_set(signal);
        // SAFETY: `set` is a valid signal set; the block is set before any thread starts.
        let rc = unsafe { libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut()) };
        if rc == 0 {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(rc))
        }
    }

    /// Waits until a `signal` that `block_signal` blocked arrives, and takes it.
    fn wait_signal(signal: libc::c_int) -> io::Result<()> {
        let set = one_set(signal);
        let mut taken: libc::c_int = 0;
        // SAFETY: `set` is a valid signal set, and `taken` is a valid out pointer.
        let rc = unsafe { libc::sigwait(&set, &mut taken) };
        if rc == 0 {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(rc))
        }
    }

    /// Blocks SIGTERM for `wait_sigterm`; see `block_signal`.
    pub fn block_sigterm() -> io::Result<()> {
        block_signal(libc::SIGTERM)
    }

    /// Waits for the SIGTERM that `block_sigterm` blocked, and takes it.
    pub fn wait_sigterm() -> io::Result<()> {
        wait_signal(libc::SIGTERM)
    }

    /// Blocks SIGINT for `wait_sigint`, the signal a held child is ended with.
    pub fn block_sigint() -> io::Result<()> {
        block_signal(libc::SIGINT)
    }

    /// Waits for the SIGINT that `block_sigint` blocked, and takes it.
    pub fn wait_sigint() -> io::Result<()> {
        wait_signal(libc::SIGINT)
    }
}

/// The vault key names, or why they are unknown. A `tokenEnv` that is not a
/// variable name, or that names one of the app's own variables, makes them
/// unknown; the reason shows the bad value as `visible` spells it.
fn vault_names(out: Result<Output, ExecError>) -> Result<Vec<String>, String> {
    let out = out.map_err(|e| format!("aoide config could not run: {e}"))?;
    if !out.status.success() {
        return Err(format!("aoide config exited with {}", out.status));
    }
    let text = std::str::from_utf8(&out.stdout).map_err(|_| "aoide config printed no UTF-8".to_owned())?;
    let keys = parse_vault_keys(text).map_err(|e| e.to_string())?;
    match keys.iter().find(|key| !is_variable_name(key) || RESERVED.contains(&key.as_str())) {
        Some(bad) => Err(format!("tokenEnv {} names no key the app may read", visible(bad))),
        None => Ok(keys),
    }
}

/// `[A-Za-z_][A-Za-z0-9_]*`: the names a child can read from its environment.
fn is_variable_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::process::ExitStatusExt;

    use super::*;

    #[test]
    fn a_positional_cannot_extend_a_path_into_a_longer_declared_one() {
        let text = r#"{"commands":[
            {"path":["config"],"summary":"Show.","args":[],"flags":[],"implemented":true},
            {"path":["config","set"],"summary":"Set.","args":[],"flags":[],"implemented":true},
            {"path":["session"],"summary":"Bare.","args":[],"flags":[],"implemented":true},
            {"path":["session","watch"],"summary":"Watch.","args":[],"flags":[],"implemented":true}
        ]}"#;
        let schema = Schema::parse(text).expect("the schema parses");
        let aoide = Path::new("/usr/bin/aoide");
        for (words, positionals, resolved) in [
            (&["config"][..], &["set"][..], "config set"),
            (&["session"][..], &["watch", "id"][..], "session watch"),
        ] {
            match argv(aoide, &schema, words, &[], positionals) {
                Err(ExecError::Resolves { called, resolved: got }) => {
                    assert_eq!(called, words.join(" "));
                    assert_eq!(got, resolved);
                }
                other => panic!("{words:?} with {positionals:?} must refuse: {other:?}"),
            }
        }
        assert!(argv(aoide, &schema, &["config"], &[], &["x"]).is_ok(), "a positional naming no path is fine");
    }

    #[test]
    fn an_unreaped_child_whose_pidfd_will_not_open_is_interrupted_and_reaped() {
        let child = std::process::Command::new("sleep").arg("30").spawn().expect("sleep starts");
        let status = retire_unreaped(child).join().expect("the reaper ends").expect("the child is reaped");
        assert_eq!(status.signal(), Some(libc::SIGINT), "{status:?}");
    }

    #[test]
    fn a_followed_child_queues_at_most_the_byte_budget_for_its_reader() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        struct Flood {
            sent: Arc<AtomicUsize>,
        }
        impl Read for Flood {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                const TOTAL: usize = 200 * 1024 * 1024;
                let done = self.sent.load(Ordering::SeqCst);
                if done >= TOTAL {
                    return Ok(0);
                }
                let n = buf.len().min(TOTAL - done);
                for (i, b) in buf[..n].iter_mut().enumerate() {
                    *b = if (done + i + 1).is_multiple_of(512 * 1024) { b'\n' } else { b'a' };
                }
                self.sent.store(done + n, Ordering::SeqCst);
                Ok(n)
            }
        }

        let sent = Arc::new(AtomicUsize::new(0));
        let flood = Flood { sent: Arc::clone(&sent) };
        let (tx, rx) = mpsc::channel();
        let out = Out { tx, room: Arc::new(Room::default()) };
        thread::spawn(move || {
            let _ = read_lines(flood, &out);
        });
        thread::sleep(Duration::from_secs(1));
        let read_ahead = sent.load(Ordering::SeqCst);
        drop(rx);
        assert!(
            read_ahead <= QUEUE_BYTES + LINE_CAP,
            "{read_ahead} bytes were read ahead of a reader that takes none"
        );
    }

    #[test]
    fn a_command_keeps_its_brief_doors_marks_and_kind() {
        let text = r#"{"commands":[
            {"path":["send"],"summary":"Long.","brief":"Send text.","args":[],"flags":[],
             "approves":true,"launches":false,"internal":false,"implemented":true,
             "kind":"act","doors":["mcp"]},
            {"path":["spawn"],"summary":"Start one.","args":[],"flags":[],"launches":true,
             "implemented":true},
            {"path":["session","hook"],"summary":"Hook.","args":[],"flags":[],"internal":true,
             "implemented":true},
            {"path":["content","approve"],"summary":"A stub.","args":[],"flags":[]},
            {"path":["config"],"summary":"Show config.","args":[],"flags":[],"implemented":true}
        ]}"#;
        let schema = Schema::parse(text).expect("the schema parses");
        let send = schema.command(&["send"]).expect("send is declared");
        assert_eq!(send.brief, "Send text.");
        assert_eq!(send.doors, ["mcp"]);
        assert!(send.approves && !send.launches && !send.internal && send.implemented);
        assert_eq!(send.kind.as_deref(), Some("act"));
        let spawn = schema.command(&["spawn"]).expect("spawn is declared");
        assert!(spawn.launches && !spawn.approves);
        assert!(schema.command(&["session", "hook"]).expect("declared").internal);
        let stub = schema.command(&["content", "approve"]).expect("declared");
        assert!(!stub.implemented, "an omitted implemented reads false");
        let config = schema.command(&["config"]).expect("config is declared");
        assert_eq!(config.brief, "Show config.", "an omitted brief reads the summary");
        assert_eq!(config.doors, ["cli", "daemon"], "omitted doors read the CLI's pair");
        assert_eq!(config.kind, None);
        assert!(!config.approves && !config.launches && !config.internal);
        assert_eq!(schema.commands().count(), 5);
    }

    #[test]
    fn a_pidfd_opens_for_a_live_child_and_not_for_a_pid_that_is_gone_or_refused() {
        let mut sleeper = Command::new("sleep").arg("30").spawn().expect("sleep starts");
        let fd = open_pidfd(sleeper.id()).expect("the pidfd opens");
        let gone = open_pidfd(i32::MAX as u32).expect_err("no process has that pid");
        assert_eq!(gone.raw_os_error(), Some(libc::ESRCH));
        for pid in [1, std::process::id()] {
            let refused = open_pidfd(pid).expect_err("pid 1 and this process are refused");
            assert_eq!(refused.kind(), io::ErrorKind::InvalidInput, "{pid}");
        }
        terminate_pidfd(&fd).expect("SIGTERM is sent");
        let _ = sleeper.wait();
    }

    #[test]
    fn terminate_pidfd_signals_the_process_it_was_opened_for() {
        let mut sleeper = Command::new("sleep").arg("30").spawn().expect("sleep starts");
        let fd = open_pidfd(sleeper.id()).expect("the pidfd opens");
        terminate_pidfd(&fd).expect("SIGTERM is sent");
        let status = sleeper.wait().expect("the child is reaped");
        assert_eq!(status.signal(), Some(libc::SIGTERM));
    }

    #[test]
    fn terminate_refuses_pid_one_without_a_signal() {
        let refused = terminate(1).expect_err("pid 1 is refused");
        assert_eq!(refused.kind(), io::ErrorKind::InvalidInput);
    }

    /// Set for the child that `terminate_refuses_this_process_without_a_signal` starts.
    const CHILD: &str = "AOIDE_DESK_TERMINATE_CHILD";

    #[test]
    fn terminate_refuses_this_process_without_a_signal() {
        if std::env::var_os(CHILD).is_some() {
            // Without the refusal this child signals itself, and the parent sees it die.
            let refused = terminate(std::process::id()).expect_err("this process is refused");
            assert_eq!(refused.kind(), io::ErrorKind::InvalidInput);
            return;
        }
        let output = Command::new(std::env::current_exe().expect("the test binary is known"))
            .args(["--exact", "tests::terminate_refuses_this_process_without_a_signal"])
            .env(CHILD, "1")
            .output()
            .expect("the child runs");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(output.status.success(), "the child's own pid was refused: {}", output.status);
        assert!(stdout.contains("1 passed"), "the child ran no test, so the refusal is unproven: {stdout}");
    }

    /// A fresh scratch dir for `tag`, mode 0700, named for this process.
    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aoide-desk-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("the scratch dir is made");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).expect("the scratch dir is 0700");
        dir
    }

    /// Writes `body` as a `#!/bin/sh` program at `path`, with `mode`.
    fn write_program(path: &Path, body: &str, mode: u32) {
        // A child writes it: a concurrent fork would hold this writer open, and the exec fails with ETXTBSY.
        let status = Command::new("/bin/sh")
            .args(["-c", "printf '%s' \"$1\" > \"$2\"", "sh"])
            .arg(format!("#!/bin/sh\n{body}\n"))
            .arg(path)
            .status()
            .expect("sh runs");
        assert!(status.success(), "the program is written: {status}");
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("the program's mode is set");
    }

    /// A program under a scratch dir only this user can enter, running `body`, pinned
    /// as `check_pin` pins it.
    fn hostile(tag: &str, body: &str) -> (PathBuf, Pinned) {
        let dir = scratch(&format!("hostile-{tag}"));
        let program = dir.join("child");
        write_program(&program, body, 0o755);
        let pinned = check_pin(&program, &dir.join("home")).expect("the child pins");
        (dir, pinned)
    }

    fn path_env() -> Vec<(OsString, OsString)> {
        vec![(OsString::from("PATH"), std::env::var_os("PATH").unwrap_or_else(|| "/usr/bin:/bin".into()))]
    }

    /// The pid the program wrote to `pidfile`. The call that starts the program may be
    /// refused before the program writes it, so this polls for up to 5 s.
    fn pid_in(pidfile: &Path) -> i32 {
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(pid) = fs::read_to_string(pidfile).ok().and_then(|text| text.trim().parse::<i32>().ok()) {
                return pid;
            }
            assert!(Instant::now() < until, "the pid is written within 5 s");
            std::thread::sleep(POLL);
        }
    }

    /// A process that has not exited: its stat reads, and it is not a zombie.
    /// It does not compare starttime, so a pid another process has taken reads
    /// as alive; these tests hold their processes for seconds.
    fn alive(pid: i32) -> bool {
        u32::try_from(pid).is_ok_and(|pid| stat_of(pid).is_ok_and(|s| s.state != 'Z'))
    }

    #[test]
    fn a_grandchild_holding_stdout_is_refused_at_the_deadline_and_no_signal_reaches_it() {
        let (dir, program) = hostile("grandchild", "sleep 3 &\necho $! > \"$1\"");
        let pidfile = dir.join("grandchild.pid");
        let started = Instant::now();
        let args = [pidfile.clone().into_os_string()];
        let err = run(&program, &args, &path_env(), None, "hostile", Duration::from_secs(1))
            .expect_err("the call is refused");
        assert!(matches!(err, ExecError::Timeout { .. }), "{err:?}");
        assert!(started.elapsed() < Duration::from_secs(3), "refused at the deadline, not at the exit");
        assert!(alive(pid_in(&pidfile)), "the grandchild is alive: no signal reached it");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_child_that_ignores_sigterm_is_refused_after_the_exit_bound_and_is_alive_then() {
        let body = "trap '' TERM\necho $$ > \"$1\"\ni=0\nwhile [ $i -lt 80 ]; do sleep 0.1; i=$((i + 1)); done";
        let (dir, program) = hostile("ignores-term", body);
        let pidfile = dir.join("child.pid");
        let started = Instant::now();
        let args = [pidfile.clone().into_os_string()];
        let err = run(&program, &args, &path_env(), None, "hostile", Duration::from_secs(1))
            .expect_err("the call is refused");
        assert!(matches!(err, ExecError::Timeout { .. }), "{err:?}");
        assert!(started.elapsed() >= Duration::from_secs(1) + EXIT_BOUND, "{:?}", started.elapsed());
        assert!(alive(pid_in(&pidfile)), "the child ignored SIGTERM and is alive");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_program_under_a_group_writable_grandparent_is_refused() {
        let dir = scratch("grandparent");
        let parent = dir.join("parent");
        fs::create_dir_all(&parent).expect("the parent is made");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o770)).expect("the grandparent is group-writable");
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o700)).expect("the parent is private");
        let program = parent.join("child");
        write_program(&program, "exit 0", 0o755);
        let err = check_pin(&program, &dir.join("home")).expect_err("a group-writable grandparent is refused");
        assert!(matches!(err, PinError::Writable(_)), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_program_in_a_private_dir_under_a_sticky_other_writable_dir_is_pinned() {
        let dir = scratch("sticky");
        let private = dir.join("private");
        fs::create_dir_all(&private).expect("the private dir is made");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o1777)).expect("the sticky dir is set");
        fs::set_permissions(&private, fs::Permissions::from_mode(0o700)).expect("the private dir is set");
        let program = private.join("child");
        write_program(&program, "exit 0", 0o755);
        check_pin(&program, &dir.join("home")).expect("a sticky directory above the program does not refuse it");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_setuid_or_setgid_program_is_refused() {
        for mode in [0o4755, 0o2755] {
            let dir = scratch(&format!("setid-{mode:o}"));
            let program = dir.join("child");
            write_program(&program, "exit 0", mode);
            let err = check_pin(&program, &dir.join("home")).expect_err("a setuid or setgid program is refused");
            assert!(err.to_string().contains("setuid or setgid"), "{err}");
            let _ = fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn a_variable_name_is_a_letter_or_underscore_then_letters_digits_or_underscores() {
        for ok in ["KEY", "_K", "k_9"] {
            assert!(is_variable_name(ok), "{ok}");
        }
        for bad in ["", "1KEY", "A-B", "A B", "KÉY", "A\nB", "A=B", "A\0B"] {
            assert!(!is_variable_name(bad), "{bad:?}");
        }
    }
}
