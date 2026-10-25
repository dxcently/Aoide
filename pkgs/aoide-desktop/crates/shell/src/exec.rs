//! The one module in the desktop workspace that starts a process. Every child
//! the app runs goes through here: its program is pinned by `check_pin`, its
//! argv is built against the schema the program prints, its environment is
//! scrubbed by `child_env`, and it runs directly, never through a shell.
#![allow(unsafe_code)]

use std::collections::BTreeMap;
use std::ffi::{CString, OsStr, OsString};
use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;

use serde::Deserialize;

/// The most a child may write to one stream before its call is refused.
pub const OUTPUT_CAP: usize = 16 * 1024 * 1024;

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
        }
    }
}

/// Checks a program path before it is run, and returns its canonical path.
pub fn check_pin(path: &Path, home: &Path) -> Result<PathBuf, PinError> {
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
    Ok(canonical)
}

fn can_execute(path: &Path) -> bool {
    let Ok(c) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: `c` is NUL-terminated and lives across the call.
    unsafe { libc::access(c.as_ptr(), libc::X_OK) == 0 }
}

/// One command as `aoide schema --json` declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub args: Vec<Arg>,
    /// Each flag's name, with its declared type (`bool`, `string`, `int`, ...).
    pub flags: BTreeMap<String, String>,
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
    args: Vec<Arg>,
    #[serde(default)]
    flags: Vec<FlagDoc>,
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
                (c.path, CommandSpec { args: c.args, flags })
            })
            .collect();
        Ok(Schema { commands })
    }

    /// Runs `aoide schema --json` once, with the environment a child gets.
    fn load(aoide: &Path, base_env: &[(OsString, OsString)]) -> Result<Schema, ExecError> {
        let env = child_env(Child::Aoide, "schema", base_env, None)?;
        let args = [OsString::from("schema"), OsString::from("--json")];
        let out = run(aoide, &args, &env, None)?;
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
pub fn child_env(
    child: Child,
    first_word: &str,
    base: &[(OsString, OsString)],
    vault_keys: Option<&[String]>,
) -> Result<Vec<(OsString, OsString)>, ExecError> {
    let keys = match (child, vault_keys) {
        (_, Some(keys)) => keys,
        (Child::Eidolon, None) => return Err(ExecError::VaultKeysUnknown),
        (Child::Aoide, None) => &[],
    };
    let melete = child == Child::Aoide && first_word == "melete";
    Ok(base
        .iter()
        .filter(|(name, _)| !scrubbed(child, melete, name, keys))
        .cloned()
        .collect())
}

fn scrubbed(child: Child, melete: bool, name: &OsStr, keys: &[String]) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    let melete_var = name.starts_with("AOIDE_MELETE_") && !melete;
    let session = child == Child::Eidolon && name == "AOIDE_SESSION_ID";
    melete_var || session || keys.iter().any(|key| key == name)
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
    aoide: PathBuf,
    schema: Schema,
    vault_keys: Option<Vec<String>>,
    base_env: Vec<(OsString, OsString)>,
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
            vault_keys: None,
            base_env,
        };
        exec.vault_keys = exec
            .call(&["config"], &[], &[], None)
            .ok()
            .filter(|out| out.status.success())
            .and_then(|out| std::str::from_utf8(&out.stdout).ok().and_then(|t| parse_vault_keys(t).ok()));
        Ok(exec)
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
        let line = argv(&self.aoide, &self.schema, words, flags, positionals)?;
        let first = words.first().copied().unwrap_or("");
        let env = child_env(Child::Aoide, first, &self.base_env, self.vault_keys.as_deref())?;
        run(&self.aoide, &line[1..], &env, stdin)
    }

    /// The environment a door gets: the child env for `eidolon web`, with
    /// `XDG_RUNTIME_DIR` set to the runtime dir the app checked, so the door
    /// writes its token where the app reads it.
    pub fn door_env(&self, runtime: &Path) -> Result<Vec<(OsString, OsString)>, ExecError> {
        let mut env = child_env(Child::Eidolon, "web", &self.base_env, self.vault_keys.as_deref())?;
        env.retain(|(name, _)| name.as_os_str() != "XDG_RUNTIME_DIR");
        env.push((OsString::from("XDG_RUNTIME_DIR"), runtime.as_os_str().to_owned()));
        Ok(env)
    }

    /// Starts `argv` as a door: detached, in a session of its own, with stdin
    /// on /dev/null and stdout and stderr on `log`. Returns the door's pid.
    /// `argv[0]` is the program `check_pin` returned.
    pub fn spawn_door(&self, argv: &[OsString], runtime: &Path, log: fs::File) -> Result<u32, ExecError> {
        let program = argv.first().map(PathBuf::from).unwrap_or_default();
        if !program.is_absolute() {
            return Err(ExecError::Pin(PinError::Relative(program)));
        }
        let env = self.door_env(runtime)?;
        spawn_detached(argv, &env, log)
    }
}

/// The one place a process starts. Only `Exec` reaches it, and only with a
/// pinned program.
fn run(
    program: &Path,
    args: &[OsString],
    env: &[(OsString, OsString)],
    stdin: Option<&[u8]>,
) -> Result<Output, ExecError> {
    let mut child = Command::new(program)
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
    let feeder = child
        .stdin
        .take()
        .zip(stdin.map(<[u8]>::to_vec))
        .map(|(mut pipe, bytes)| thread::spawn(move || pipe.write_all(&bytes)));
    let out_reader = thread::spawn(move || capture(stdout));
    let err_reader = thread::spawn(move || capture(stderr));
    let status = child.wait().map_err(ExecError::Io)?;
    if let Some(feeder) = feeder {
        let _ = feeder.join();
    }
    let (stdout, stdout_over) = join(out_reader)?;
    let (stderr, stderr_over) = join(err_reader)?;
    if stdout_over {
        return Err(ExecError::OutputTooLarge { stream: "stdout" });
    }
    if stderr_over {
        return Err(ExecError::OutputTooLarge { stream: "stderr" });
    }
    Ok(Output { status, stdout, stderr })
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

fn join<T>(reader: thread::JoinHandle<io::Result<T>>) -> Result<T, ExecError> {
    reader
        .join()
        .map_err(|_| ExecError::Io(io::Error::other("an output reader panicked")))?
        .map_err(ExecError::Io)
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
    /// An eidolon launch needs the vault names, and they are unknown.
    VaultKeysUnknown,
    /// The child wrote more than [`OUTPUT_CAP`] bytes to this stream.
    OutputTooLarge { stream: &'static str },
    /// The program path was refused.
    Pin(PinError),
    /// The program could not be started.
    Spawn(io::Error),
    /// Reading the child's output or waiting on it failed.
    Io(io::Error),
}

impl fmt::Display for ExecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Schema(why) => write!(f, "aoide schema --json did not parse: {why}"),
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
            Self::VaultKeysUnknown => write!(
                f,
                "the vault key names are unknown because aoide config could not be read, so eidolon is not launched"
            ),
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

/// Starts `argv` in a session of its own, outside this process's tree.
///
/// std forks a child (the intermediate), which calls `setsid`, forks the door,
/// writes the door's pid to a pipe and exits. The app reaps the intermediate,
/// so init adopts the door. Between fork and exec the door's side makes only
/// `setsid`, `fork`, `write` and `_exit`, which POSIX lists as async-signal-safe,
/// and allocates nothing. `fork` runs glibc's atfork handlers, which take
/// malloc's locks before the fork and release them on both sides; the intermediate
/// has one thread, so those locks are free when the closure runs. The door's own
/// exec failure travels back through std's close-on-exec pipe as `spawn`'s error.
fn spawn_detached(argv: &[OsString], env: &[(OsString, OsString)], log: fs::File) -> Result<u32, ExecError> {
    let (mut pid_in, pid_out) = io::pipe().map_err(ExecError::Io)?;
    let pid_fd = pid_out.as_raw_fd();
    let stdout = log.try_clone().map_err(ExecError::Io)?;
    let mut command = Command::new(&argv[0]);
    command
        .args(&argv[1..])
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
                    let written = libc::write(pid_fd, bytes.as_ptr().cast(), bytes.len());
                    libc::_exit(if written == bytes.len() as isize { 0 } else { 1 })
                }
            }
        });
    }
    let mut intermediate = command.spawn().map_err(ExecError::Spawn)?;
    drop(pid_out);
    let mut bytes = [0u8; 4];
    let read = pid_in.read_exact(&mut bytes);
    let _ = intermediate.wait();
    read.map_err(ExecError::Io)?;
    Ok(u32::from_ne_bytes(bytes))
}

/// Sends SIGTERM to `pid`. The caller checks the start time and the command
/// line at this moment, since a pid can be reused; this refuses the values
/// `kill` would read as a process group or as no process at all.
pub fn terminate(pid: u32) -> io::Result<()> {
    let pid = i32::try_from(pid)
        .ok()
        .filter(|p| *p > 0)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "a pid to signal is 1 to i32::MAX"))?;
    // SAFETY: a plain syscall taking a pid and a signal number; no memory is passed.
    if unsafe { libc::kill(pid, libc::SIGTERM) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
