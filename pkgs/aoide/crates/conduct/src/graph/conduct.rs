//! `aoide conduct` — the controllable conducted session: a child spawned onto
//! a terminal, a per-session injection socket `send` types into, and the
//! multiplexer that shuttles stdin/stdout/injections between them. The
//! capability itself — the terminal, this process's real console, the
//! injection inbox and the one readiness wait — lives in `super::pty`, one
//! seam with an arm per host; nothing here names an fd, a signal or a
//! `HANDLE`.
//!
//! What is left in this file is registration and policy: spawn FIRST so a
//! failed exec registers no ghost, the roster upsert and its stamps, the log
//! tee, the shell tick, and the end vocabulary (`exit`/`signal`/`timeout`/
//! `stopped`) derived ONLY from the status this process observed.

use super::doc::restage_graph;
use super::model::{
    canonical_state, load_stage, sessions_path, write_stage, RestoreSnapshot, SessionRecord,
    SessionsFile, STAGE_GRAPH_VERSION,
};
use super::pty::{spawn_on_pty, wait_ready, Console, Ended, Inbox, Pty, PtyChild, WinSize};
#[cfg(unix)]
use super::pty::signal_name;
use super::session_store::{
    do_session_end, do_session_start, set_session_log_path, stamp_headless, stamp_origin,
    stamp_session_exit, stamp_shell, stamp_spawned, stamp_task,
};
use super::window::{discover_window_address, resolve_registration_parent};
use aoide_protocol::Invocation;
use aoide_protocol::output::Outcome;
use aoide_storage::attest::is_node_origin;
use aoide_storage::fs::with_stage_lock;
use aoide_storage::time::now_iso_utc;
use serde_json::json;
use std::path::PathBuf;

/// The command's basename (the agent-name default), e.g. `/usr/bin/claude` →
/// `claude`.
fn command_basename(program: &str) -> String {
    std::path::Path::new(program)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| program.to_string())
}

/// The shell basenames [`program_is_a_shell`] resolves to — the ONE list, in
/// the one place shell-likeness is decided: the P-C5 capture path
/// (`is_shell`, the refresh tick, the `typed` buffer, the restore snapshot),
/// the auto-typing refusals (ping-back, the doorbell's PTY arm, the A2A
/// door) and `spawn`'s own gates all read THIS verdict, never a private copy
/// of the names.
const SHELL_BASENAMES: &[&str] = &[
    "bash", "zsh", "fish", "sh", "dash", "ksh", "mksh", "pdksh", "csh", "tcsh", "nu", "xonsh",
    "busybox", "elvish", "osh", "ash", "yash",
];

/// Launchers that only FORWARD to the program after them: skipped whole,
/// options and all, so `env bash`, `nice -n 5 bash`, `stdbuf -oL bash`,
/// `timeout 60 bash` and `setsid bash` all resolve to `bash` rather than to
/// their own basename.
const FORWARDING_LAUNCHERS: &[&str] = &[
    "env", "exec", "setsid", "nohup", "stdbuf", "chrt", "ionice", "nice",
];

/// Among a launcher's own options, the ones taking their value as a SEPARATE
/// token (`nice -n 5`, `stdbuf -o L`, `timeout -k 5`) rather than attached
/// (`stdbuf -oL`). Per LAUNCHER, never one global set: `env -i` takes no value
/// while `stdbuf -i M` does, and a shared table would have to guess — a guess
/// here swallows the program name and misses a shell, which is the direction
/// this predicate must never err in. Anything else merely numeric (a
/// priority, a duration) is skipped by [`skip_launcher_options`] too.
fn option_takes_its_value(launcher: &str, arg: &str) -> bool {
    match launcher {
        "env" => matches!(arg, "-u" | "--unset" | "-C" | "--chdir" | "-S" | "--split-string"),
        "nice" => matches!(arg, "-n" | "--adjustment"),
        "stdbuf" => matches!(arg, "-i" | "-o" | "-e"),
        "ionice" => matches!(arg, "-c" | "-n" | "-p" | "-u"),
        "chrt" => matches!(arg, "-p" | "-T" | "-P" | "-D" | "-R" | "-O" | "-b"),
        "timeout" => matches!(arg, "-k" | "--kill-after" | "-s" | "--signal"),
        "script" => matches!(arg, "-c" | "-t" | "-T" | "-F" | "-B" | "--log-out" | "--log-in"),
        _ => false,
    }
}

/// A `VAR=value` prefix (`env FOO=bar bash`, and `env`'s own assignment
/// form) — never a program name.
fn is_assignment(arg: &str) -> bool {
    match arg.split_once('=') {
        Some((name, _)) => {
            !name.is_empty()
                && !name.starts_with('-')
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        None => false,
    }
}

fn is_number(arg: &str) -> bool {
    !arg.is_empty() && arg.chars().all(|c| c.is_ascii_digit() || c == '.')
}

/// Skip a launcher's own leading options: anything dashed, a dashed option's
/// separate value ([`option_takes_its_value`]), and — when
/// `numbers_are_options` — any bare number. Stops at the first token that
/// could be a program.
///
/// The flag exists for exactly one caller shape: a bare number is a priority
/// or a schedule argument for every forwarding launcher, but it is the
/// DURATION itself in `timeout <duration> <cmd>`, which the caller skips as
/// its own positional token. Skipping it twice there would step over the
/// program name.
fn skip_launcher_options(
    argv: &[String],
    mut i: usize,
    launcher: &str,
    numbers_are_options: bool,
) -> usize {
    while i < argv.len() {
        let arg = argv[i].as_str();
        if option_takes_its_value(launcher, arg) {
            i += 2;
        } else if arg.starts_with('-') || (numbers_are_options && is_number(arg)) {
            i += 1;
        } else {
            break;
        }
    }
    i
}

/// `nix develop|shell … -c <cmd>` / `--command[=<cmd>]`: those subcommands
/// exist to run a command in a dev environment, so their payload is the
/// question. With no `-c`/`--command` at all, nix opens an INTERACTIVE shell
/// — which is this predicate's question, and "shell" is the safe answer.
fn nix_runs_a_shell(argv: &[String], i: usize) -> bool {
    let sub = argv.get(i + 1).map(|s| command_basename(s)).unwrap_or_default();
    if sub != "develop" && sub != "shell" {
        return false;
    }
    let mut j = i + 2;
    while j < argv.len() {
        match argv[j].as_str() {
            "-c" | "--command" => return j + 1 >= argv.len() || program_is_a_shell(&argv[j + 1..]),
            // The attached form carries a whole command LINE in one token.
            a if a.starts_with("--command=") => {
                let words: Vec<String> = a["--command=".len()..]
                    .split_whitespace()
                    .map(str::to_string)
                    .collect();
                return program_is_a_shell(&words);
            }
            _ => j += 1,
        }
    }
    true
}

/// Where `script -q <typescript> [<command> …]`'s payload starts, or `None`
/// when it carries no command of its own — then `script` runs the user's own
/// `$SHELL`, which is a shell. (`script -c <cmd>` is caught by the option
/// skip: it runs its argument through a shell, so both tokens are options.)
fn script_command(argv: &[String], i: usize) -> Option<usize> {
    let mut j = skip_launcher_options(argv, i + 1, "script", true);
    if j < argv.len() {
        j += 1; // the typescript FILE
    }
    (j < argv.len()).then_some(j)
}

/// Whether a conducted command IS a shell — one question, one list, one
/// implementation for every reader: the P-C5 capture path (`is_shell`
/// below), the auto-typing refusals (ping-back, the doorbell's PTY arm, the
/// A2A door) and `spawn`'s own gates.
///
/// Derived from the WRAPPED ARGV, never the roster display name (P-C5
/// follow-up, task #100 — the P-C7 soak's live finding: `spawn --agent
/// soak-a -- bash` ran a real interactive shell whose record never ticked,
/// because the old gate compared `agent == "shell"` and a caller is free to
/// label a shell anything it likes). The first token alone could not answer
/// it either: a shell is routinely reached through a launcher, so the walk
/// below skips [`FORWARDING_LAUNCHERS`] and their options until it reaches
/// the program that will actually exec. `su`/`doas`/`sudo` are the one
/// resolved-TO case — with no command of their own they land in a login
/// shell — and answering `true` there is the safe direction. A path basename
/// counts (`/usr/bin/bash`), so kitty.nix's terminal wrapper needs no
/// special case: it always execs the resolved login shell explicitly
/// (`$SHELL`/passwd/`/bin/sh`, `<login_shell> -l`) as the conducted command.
///
/// The LIMIT, stated because a refusal's silence must not read as a
/// clearance: this reads ARGV, never file contents, so a wrapper script that
/// execs a shell (`-- ./rig.sh`) is NOT a shell by this predicate and stays
/// receptive — the allowlisted names above are the whole claim.
///
/// A false POSITIVE (something shell-like that is not) costs a skipped
/// auto-typing, the silent-safe direction; a false negative is the hole this
/// exists to close, so every unresolved case errs toward "shell".
pub fn program_is_a_shell(argv: &[String]) -> bool {
    let mut i = 0;
    while i < argv.len() {
        if is_assignment(&argv[i]) {
            i += 1;
            continue;
        }
        let base = command_basename(&argv[i]);
        match base.as_str() {
            b if FORWARDING_LAUNCHERS.contains(&b) => {
                i = skip_launcher_options(argv, i + 1, &b, true)
            }
            // `timeout <duration> <cmd>`: the duration is a positional, not an
            // option, so the options are skipped without touching bare
            // numbers and then exactly one token goes — however the duration
            // is spelled (`60`, `1m`, `0.5s`).
            "timeout" => i = skip_launcher_options(argv, i + 1, "timeout", false) + 1,
            "nix" => return nix_runs_a_shell(argv, i),
            "script" => match script_command(argv, i) {
                Some(next) => i = next,
                None => return true,
            },
            "su" | "doas" | "sudo" => return true,
            b => return SHELL_BASENAMES.contains(&b),
        }
    }
    false
}

/// [`program_is_a_shell`]'s verdict, read back off a RECORD: `shell` (the
/// durable registration fact below) OR a `Some` restore snapshot. Not a
/// second predicate — [`SHELL_BASENAMES`] above stays the only place
/// shell-likeness is decided, and both fields are written by this file
/// alone.
///
/// `rec.shell` is the honest primitive: `conduct` stamps it at REGISTRATION
/// from its own argv, so it is there for the whole life of the session —
/// including the window before the multiplexer's opening tick, and across a
/// re-registration of the same id. `restore.is_some()` is kept as the second
/// arm because it predates the field and is the same verdict for every
/// session already running when it landed; a lane refusing on either read is
/// refusing on this one question.
///
/// This is how a caller who was NOT there at spawn time — the daemon's
/// ping-back, the doorbell, the A2A door — can tell a shell from a harness
/// WRAP: `agent` cannot answer it (it is a label a caller chooses, and
/// `--agent pi -- bash` is exactly the mismatch that made `agent == "shell"`
/// insufficient), and the wrapped argv is not on the record. Never type into
/// what this returns true for.
///
/// The converse is NOT claimed: a harness reached through a shell
/// (`spawnAgent = "bash -lc <harness>"`, `conduct --agent <harness> --
/// sh -c <harness>`) reads as a shell here for the whole run, because argv
/// says so and the pty's foreground process is not a fact this record
/// carries. The cost is a silent skip — the ping-back, the doorbell's PTY
/// arm and every unapproved remote inject — never a line typed somewhere it
/// would run, so it stays the safe direction rather than being narrowed by a
/// hook-shaped guess.
///
/// `pub`, not `pub(in crate::graph)`: `aoide-server`'s A2A door applies the
/// same read before its own inject (`server/src/a2a.rs`), the same
/// pre-Phase-3b visibility `conduct_socket_path` carries one screen down.
pub fn wrapped_program_is_a_shell(rec: &SessionRecord) -> bool {
    rec.shell || rec.restore.is_some()
}

pub(in crate::graph) fn unix_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The per-session conductor control socket:
/// `$XDG_RUNTIME_DIR/aoide/session-<id>.sock` — the same user-scoped runtime-dir
/// convention as shellbridge's socket (never networked), resolved by the ONE
/// seam both of them call (`aoide_storage::runtime_dir::socket_dir`): a missing
/// `XDG_RUNTIME_DIR` falls back to `/run/user/<this process's euid>`, and on
/// native Windows to `%LOCALAPPDATA%\aoide`.
/// `pub`, not `pub(crate)` (pre-Phase-3b visibility): this crosses the
/// aoide-conduct → aoide-server crate boundary too, since `aoide-server`'s
/// `a2a` (Phase 4c) resolves a just-spawned conducted session's control-socket
/// path via `aoide_conduct::graph::conduct_socket_path` directly — root no
/// longer re-exports this symbol at all (dropped in Phase 4c as dead once the
/// only caller moved into `aoide-server`).
pub fn conduct_socket_path(id: &str) -> PathBuf {
    aoide_storage::runtime_dir::socket_dir().join(format!("session-{id}.sock"))
}

/// The per-session Claude Code channel socket (P-M5c-2,
/// `docs/architecture/CLAUDE-CHANNEL-PROOF.md`): `$XDG_RUNTIME_DIR/aoide/
/// channel-<id>.sock`, one authority for the path shared with
/// [`conduct_socket_path`] above (same runtime-dir seam — see its own doc for
/// the fallback, which is `/run/user/<euid>` and not a hard-coded uid,
/// differing only by the `channel-` prefix). `aoide-server`'s stdio MCP
/// server binds it for the lifetime of that MCP subprocess — never
/// `aoided`'s — so a doorbell ring can push a one-way `notifications/claude/
/// channel` event into an idle interactive Claude Code session. `pub` for
/// the identical reason `conduct_socket_path` is: this crosses the
/// `aoide-conduct` → `aoide-server` boundary too.
pub fn channel_socket_path(id: &str) -> PathBuf {
    aoide_storage::runtime_dir::socket_dir().join(format!("channel-{id}.sock"))
}

// ── the PTY capability lives in `super::pty` ─────────────────────────────
//
// The terminal the child is spawned onto, this process's real console and its
// resize source, the injection inbox, and the one readiness wait are the seam
// in `super::pty` — an arm per host behind one contract, with the host
// differences named there in a table rather than discovered here. This file
// multiplexes over that seam and owns the policy around it.

/// The managed-task context a `conduct` run carries into its own child: the
/// task slug and the absolute path of the write-once instruction sidecar
/// (`spawn --task`, `docs/Aoide-Wiki/concepts/orchestration/
/// Managed-Task-Wrapper.md`). Both are exported into the CHILD's environment
/// ([`CHILD_TASK_ENV`]/[`CHILD_TASK_INSTRUCTIONS_ENV`]) so the instructions
/// are genuinely reachable by the agent itself, never a viewer-only sidecar;
/// neither value is a secret, and nothing else about this process's
/// environment is dumped anywhere.
#[derive(Debug, Clone, Default)]
pub(in crate::graph) struct TaskContext {
    pub slug: String,
    pub instructions_path: Option<String>,
    /// The mailbox this run's exit report is addressed to (`--report-to`),
    /// validated here with the same name predicate every mailbox name is.
    pub report_to: Option<String>,
}

/// The environment convention a managed task's child (the agent itself)
/// reads: `AOIDE_TASK` names its task slug and child inbox, while
/// `AOIDE_TASK_INSTRUCTIONS` names the absolute path of its write-once
/// instruction sidecar. Set beside the long-standing
/// `AOIDE_SESSION_ID` export ([`CHILD_SESSION_ENV`]), nothing removed,
/// nothing else added. `pub(in crate::graph)` because the seam that exec's the
/// child names them (`super::pty`), once per host.
pub(in crate::graph) const CHILD_SESSION_ENV: &str = "AOIDE_SESSION_ID";
pub(in crate::graph) const CHILD_TASK_ENV: &str = "AOIDE_TASK";
pub(in crate::graph) const CHILD_TASK_INSTRUCTIONS_ENV: &str = "AOIDE_TASK_INSTRUCTIONS";

/// Drop every harness session marker from `cmd`'s environment before exec
/// ([`aoide_protocol::agents::session_env_markers`]): the variables that say
/// "this process runs inside a `<harness>` session" belong to the session that
/// spawned the child, never to the child. Inheriting them is not cosmetic —
/// claude reads its own `CLAUDE_CODE_CHILD_SESSION` and turns transcript
/// saving off (its TUI says so) and silently disables the project `.mcp.json`
/// servers carried by that marker, which is the doorbell channel
/// (`TASK-REGISTER.md` §3).
///
/// ONE list, because there is ONE place an agent child is exec'd
/// ([`spawn_on_pty`]): `spawn`'s headless and windowed arms, plain `conduct`,
/// `resurrect`'s reopen and the A2A door's remote child all reach an agent
/// through it. The aoide parent edge is untouched by construction —
/// `AOIDE_SESSION_ID` is not a harness marker, and `spawn_on_pty` exports the
/// child's own value a line above this call. Unix's own shape: native Windows
/// takes a `Command`-shaped environment nowhere — `CreateProcessW` is handed a
/// block — so that arm applies the same list while BUILDING the block
/// (`super::pty`'s `environment_block`), and this `Command`-shaped helper has
/// no caller there.
#[cfg(unix)]
pub(in crate::graph) fn scrub_session_markers(cmd: &mut std::process::Command) {
    for name in aoide_protocol::agents::session_env_markers() {
        cmd.env_remove(name);
    }
}

/// Where the pty-master's output goes. `Stdout` alone never happens
/// anymore in practice (every conduct opens its log — see
/// [`open_session_log`]) but stays as the pre-open-failure default and the
/// shape a headless-only reader still exercises directly in tests. `Log`
/// is headless conduct: no controlling tty, so the log IS the only sink.
/// `StdoutAndLog` is interactive conduct (task #15, "everything tees"):
/// the real stdout, unchanged, PLUS the same master-read bytes mirrored
/// into the per-session log.
enum OutputSink {
    Stdout,
    Log(std::fs::File),
    StdoutAndLog(std::fs::File),
}
impl OutputSink {
    /// Mirror `bytes` to the sink. The `Stdout` arm writes this process's own
    /// stdout (`std::io`'s portable handle, retrying a short write). The
    /// `Log`/`StdoutAndLog` log
    /// write appends (retrying a short write, same as `write_all`'s own
    /// retry loop) and, on a write error, DEGRADES rather than killing the
    /// session — mirroring `write_all` itself, which just stops
    /// mirroring on an unrecoverable write error instead of tearing down
    /// the conducted child. This matters most for `StdoutAndLog`: the
    /// interactive pump is raw-mode and latency-sensitive, so a full disk
    /// or a yanked log file must never stall or kill a live terminal —
    /// only the log side of the tee drops.
    fn write(&mut self, bytes: &[u8]) {
        match self {
            OutputSink::Stdout => Self::write_stdout(bytes),
            OutputSink::Log(f) => Self::write_log(f, bytes),
            OutputSink::StdoutAndLog(f) => {
                Self::write_stdout(bytes);
                Self::write_log(f, bytes);
            }
        }
    }
    fn write_stdout(bytes: &[u8]) {
        super::pty::write_stdout(bytes);
    }
    fn write_log(f: &mut impl std::io::Write, bytes: &[u8]) {
        let mut data = bytes;
        while !data.is_empty() {
            match f.write(data) {
                Ok(0) => break,
                Ok(n) => data = &data[n..],
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
    }
}

/// Open (creating if needed) this session's per-session pty log at
/// `state/sessions/<id>.log`, the ONE open+stamp path both the headless and
/// interactive arms of [`session_conduct`] call — never duplicated per
/// path. Private end to end and structurally so, not by umask luck: the
/// directory is locked down and the file opened through the crate's own
/// private-create seams (`storage::fs::secure_private_dir` /
/// `open_private_append`), which on Unix means `0700` + an explicit `0600`
/// mode — so a permissive umask (e.g. `0000`) can never widen either past what
/// the ruling requires (task #15: local, private, no opt-out flag) — and on
/// native Windows means the owner-only DACL attached at creation and read
/// back. Returns `None` on any failure (an unwritable state dir, a
/// permissions call that errors, a filesystem that will not keep the policy)
/// — the caller degrades to `OutputSink::Stdout` on `None`, same best-effort
/// posture as every other side-channel write in this file.
fn open_session_log(id: &str) -> Option<(std::fs::File, PathBuf)> {
    let dir = aoide_storage::fs::session_logs_dir();
    aoide_storage::fs::secure_private_dir(&dir).ok()?;
    let log_path = dir.join(format!("{id}.log"));
    let f = aoide_storage::fs::open_private_append(&log_path).ok()?;
    Some((f, log_path))
}

/// Read a process's live working directory — `/proc/<pid>/cwd` on Unix, which
/// follows the shell's `cd`.
///
/// **No native-Windows arm, and this is a taught refusal by NAME**: the
/// process-table seam (`aoide_protocol::win_proc`) answers a pid's parent, its
/// start time, its liveness, its argv and its token user — the working
/// DIRECTORY is none of those, and reaching it needs the target's own PEB,
/// which no seam in this tree exposes. So the Windows arm answers the same
/// "unknown" a refused `/proc` read gives, and the caller's own fallback (the
/// `cwd` its session record stamped at registration) is what a roster refresh
/// shows there.
#[cfg(unix)]
pub(in crate::graph) fn proc_cwd(pid: i32) -> Option<String> {
    std::fs::read_link(format!("/proc/{pid}/cwd"))
        .ok()
        .and_then(|p| p.to_str().map(str::to_string))
}

#[cfg(windows)]
pub(in crate::graph) fn proc_cwd(_pid: i32) -> Option<String> {
    None
}

/// Known interactive text-editor binaries whose activity display should read
/// as "<editor> <file>" (or bare "<editor>" with no file), not the raw
/// invocation. Dotfiles routinely wrap these with startup flags (an alias
/// injecting `--cmd 'lua …'`, a resolved absolute binary path) that make the
/// full cmdline read as noise rather than "what's being edited".
const EDITOR_BASENAMES: &[&str] = &["nvim", "vim", "vi", "nano", "emacs", "hx", "micro"];

/// If `argv[0]`'s basename is a known editor, return a friendly `"<editor>
/// <file>"` (or bare `"<editor>"` with no file argument) — pure and
/// unit-tested. The file is the LAST argument that doesn't look like a flag
/// AND doesn't contain a space: flags precede the file operand in normal
/// usage, so scanning from the end finds the real file even past a `--cmd
/// '…'`-style startup injection (whose value sits earlier in argv, before the
/// file) — and the no-space guard additionally rejects a bare, file-less
/// invocation whose flag VALUE doesn't start with `-` either (e.g. `nvim --cmd
/// 'lua x=1'` with no file): a real single-file operand essentially never
/// contains a space, while an option's value routinely does. `None` for a
/// non-editor binary, so the caller falls back to the generic full-cmdline
/// display.
fn friendly_editor_command(argv: &[String]) -> Option<String> {
    let base = std::path::Path::new(argv.first()?).file_name()?.to_str()?;
    if !EDITOR_BASENAMES.contains(&base) {
        return None;
    }
    let file = argv[1..]
        .iter()
        .rev()
        .find(|a| !a.starts_with('-') && !a.contains(' '));
    Some(match file {
        Some(f) => {
            let name = std::path::Path::new(f)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(f);
            format!("{base} {name}")
        }
        None => base.to_string(),
    })
}

/// The generic (non-editor) command label: `argv` joined into a one-liner, but
/// with `argv[0]` collapsed to its BASENAME first (the rest of argv is left
/// untouched). On NixOS many wrapped packages re-exec with `argv[0]` set to the
/// full resolved `/nix/store/<hash>-<name>/bin/<name>` path (confirmed live:
/// `yazi`) rather than the bare command the user typed, so a raw join would show
/// an ugly store path; collapsing `argv[0]` yields a clean `yazi` while leaving
/// arguments (which may legitimately be paths) intact. `None` for empty argv.
fn generic_command_label(argv: &[String]) -> Option<String> {
    let first = argv.first()?;
    let base = std::path::Path::new(first)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(first.as_str());
    let mut parts: Vec<&str> = Vec::with_capacity(argv.len());
    parts.push(base);
    parts.extend(argv[1..].iter().map(String::as_str));
    let joined = parts.join(" ");
    let joined = joined.trim();
    if joined.is_empty() {
        None
    } else {
        Some(joined.to_string())
    }
}

/// A short one-line label for a process's command: a known editor shows as
/// `"<editor> <file>"` ([`friendly_editor_command`]); anything else shows the
/// process's own argv joined with `argv[0]` collapsed to its basename
/// ([`generic_command_label`], e.g. `cargo test`), falling back to `comm`.
/// Clipped to a roster-friendly width. Used as a conducted shell's live
/// `activity`. Both facts come from the one process-table reader pair below
/// ([`proc_argv`]/[`proc_comm`]), never from a `/proc` path spelled here.
fn proc_command(pid: i32) -> Option<String> {
    let clip = |s: &str| -> String {
        let one = s.split_whitespace().collect::<Vec<_>>().join(" ");
        if one.chars().count() <= 48 {
            one
        } else {
            let head: String = one.chars().take(47).collect();
            format!("{head}…")
        }
    };
    if let Some(argv) = proc_argv(pid) {
        if let Some(friendly) = friendly_editor_command(&argv) {
            return Some(clip(&friendly));
        }
        if let Some(label) = generic_command_label(&argv) {
            return Some(clip(&label));
        }
    }
    proc_comm(pid).map(|c| clip(&c)).filter(|c| !c.is_empty())
}

/// Pure NUL-split of a raw `/proc/<pid>/cmdline` buffer into argv — split out
/// of [`proc_command`]/[`proc_argv`] so the split itself is unit-testable
/// against a synthesized buffer without a real `/proc` read. Empty segments
/// (a trailing NUL, or two in a row) are dropped. Unix's own shape: the
/// Windows arm of [`proc_argv`] asks the process-table seam, which re-splits
/// with that host's own parser.
#[cfg(unix)]
fn parse_cmdline(raw: &[u8]) -> Vec<String> {
    raw.split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| String::from_utf8_lossy(p).into_owned())
        .collect()
}

/// RAW `argv` for a pid — the actual invocation, uncollapsed
/// and UNCLIPPED, unlike [`proc_command`]'s DISPLAY label (basename-collapsed
/// `argv[0]`, 48-char-truncated). A restore snapshot's `argv` must be an
/// exact re-exec candidate, not a shortened label — reusing `proc_command`
/// here would re-exec the wrong binary or a truncated one. `None` when the
/// argv is unreadable (the process already gone, a permissions edge case) or
/// empty.
///
/// **Host-split, one fact**: Unix reads `/proc/<pid>/cmdline` and NUL-splits
/// it; native Windows asks the process-table seam for the same invocation
/// (`aoide_protocol::win_proc::command_argv`, which reads
/// `ProcessCommandLineInformation` and re-splits it with this host's own
/// `CommandLineToArgvW`). Both arms answer the same list-or-`None`, and the
/// pure splitter above stays the Unix arm's alone — nothing about the Windows
/// answer is inferred from a `/proc` shape.
pub(in crate::graph) fn proc_argv(pid: i32) -> Option<Vec<String>> {
    #[cfg(unix)]
    {
        let raw = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
        let argv = parse_cmdline(&raw);
        if argv.is_empty() {
            None
        } else {
            Some(argv)
        }
    }
    #[cfg(windows)]
    {
        let pid = u32::try_from(pid).ok()?;
        aoide_protocol::win_proc::command_argv(pid).ok().filter(|a| !a.is_empty())
    }
}

/// Just the process's own executable name (Unix's `comm`, e.g. `bash`) — the
/// label for an idle shell sitting at its bare prompt (no foreground command),
/// so the roster still reads as the shell PROCESS rather than going blank.
/// Native Windows answers out of the same process-table snapshot
/// (`win_proc::processes`), where the name carries that host's own `.exe`
/// suffix — left attached, exactly as `win_proc::Proc`'s own doc says folding
/// it is the asking caller's policy and not the reader's. `None` for a pid
/// this host cannot see.
fn proc_comm(pid: i32) -> Option<String> {
    #[cfg(unix)]
    {
        std::fs::read_to_string(format!("/proc/{pid}/comm"))
            .ok()
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty())
    }
    #[cfg(windows)]
    {
        let want = u32::try_from(pid).ok()?;
        aoide_protocol::win_proc::processes()
            .ok()?
            .into_iter()
            .find(|p| p.pid == want)
            .map(|p| p.exe)
            .filter(|e| !e.is_empty())
    }
}

/// Whether a pid has any child process right now. `Some(true)` = has children,
/// `Some(false)` = none, `None` = this host cannot answer (a pid that is not
/// in the table at all — the same "unreadable" a missing `/proc/<pid>` is).
/// A conducting `sudo` at its password prompt has NO children yet (it forks
/// the command/monitor only after auth); once it has forked, auth is done.
///
/// Unix reads `/proc/<pid>/task/<pid>/children`; native Windows asks the same
/// ToolHelp snapshot `win_proc` takes once per call, for any row whose parent
/// is this pid — the same question with the same three verdicts, so the
/// `sudo`-gate logic above needs no second reader.
fn proc_has_children(pid: i32) -> Option<bool> {
    #[cfg(unix)]
    {
        std::fs::read_to_string(format!("/proc/{pid}/task/{pid}/children"))
            .ok()
            .map(|s| !s.trim().is_empty())
    }
    #[cfg(windows)]
    {
        let want = u32::try_from(pid).ok()?;
        let table = aoide_protocol::win_proc::processes().ok()?;
        if !table.iter().any(|p| p.pid == want) {
            return None;
        }
        Some(table.iter().any(|p| p.parent == want))
    }
}

/// Is a conducted SHELL currently blocked on a `sudo` password prompt? A
/// conducted shell is sudo-blocked iff the `[sudo] password for` prompt was
/// just seen (booster), OR the foreground process IS `sudo` AND it has not yet
/// forked its command child (`Some(false)` children == still in the PAM
/// conversation, i.e. at the prompt). `Some(true)` (command already running,
/// e.g. cached-cred `sudo nixos-rebuild`) and `None` (children unreadable →
/// don't trust the primary, rely on the booster) both DON'T fire the primary.
/// Pure and unit-tested; the real caller passes `proc_comm(fg) == Some("sudo")`,
/// `proc_has_children(fg)`, and the booster's recency check.
///
/// WHY the children gate: `sudo` stays the process-group leader for the WHOLE
/// runtime of `sudo <cmd>` on a PAM system (it forks the command/monitor
/// child; it never execs in place), so `comm(fg) == "sudo"` alone would also
/// be true for a multi-minute `sudo nixos-rebuild switch` running on CACHED
/// credentials — no prompt at all. Gating on "sudo has not yet forked a
/// child" narrows the primary signal to the actual PAM conversation window.
fn sudo_awaiting(fg_is_sudo: bool, fg_has_children: Option<bool>, booster_recent: bool) -> bool {
    booster_recent || (fg_is_sudo && fg_has_children == Some(false))
}

/// Update a conducted session's live shell fields — `cwd`, `activity` (the
/// current foreground command, or cleared), `state` (idle at the prompt,
/// working while a command runs, or forced `awaiting` while blocked on
/// `sudo`), `needsSudo`, and `restore` (the P-C5 continuous capture snapshot,
/// see [`RestoreSnapshot`]) — CHANGE-ONLY, under the stage lock, and re-stage
/// graph.json only when it actually wrote. Called ~1 Hz from conduct's PTY
/// tick for SHELL sessions (an agent's state/activity come from hooks, so
/// conduct never drives those). No-op for an unregistered id.
fn do_session_refresh(
    id: &str,
    cwd: Option<&str>,
    activity: Option<&str>,
    state: &str,
    needs_sudo: bool,
    restore: RestoreSnapshot,
) {
    with_stage_lock(|| {
        let mut file: SessionsFile = match load_stage(&sessions_path()) {
            Ok(f) => f,
            Err(_) => return,
        };
        let mut changed = false;
        for s in file.sessions.iter_mut() {
            if s.session_id != id {
                continue;
            }
            if let Some(c) = cwd {
                if s.cwd != c {
                    s.cwd = c.to_string();
                    changed = true;
                }
            }
            let act = activity.filter(|a| !a.is_empty()).map(str::to_string);
            if s.activity != act {
                s.activity = act;
                changed = true;
            }
            let canon = canonical_state(state);
            if s.state != canon {
                s.state = canon.to_string();
                changed = true;
            }
            // Change-only, and cleared to `None` (never written as
            // `Some(false)`) so the key disappears the moment the prompt clears.
            let want = if needs_sudo { Some(true) } else { None };
            if s.needs_sudo != want {
                s.needs_sudo = want;
                changed = true;
            }
            if s.restore.as_ref() != Some(&restore) {
                s.restore = Some(restore.clone());
                changed = true;
            }
        }
        if changed {
            if file.schema_version.is_empty() {
                file.schema_version = STAGE_GRAPH_VERSION.to_string();
            }
            if write_stage(&sessions_path(), &file).is_ok() {
                let _ = restage_graph();
            }
        }
    });
}

/// Which process's cwd a conducted shell should report, given the pty's
/// foreground pgid `fg` and the `shell_pid`, using an injectable `cwd_of`
/// lookup (pure and unit-tested; the real caller passes [`proc_cwd`]). At the
/// bare prompt (`fg <= 0` or `fg == shell_pid`) it's the shell's own cwd. While
/// a foreground command runs it's the FOREGROUND process's OWN cwd — which may
/// navigate independently of the shell (e.g. yazi/ranger's live directory
/// browsing calls `chdir()` on themselves, not the parent shell), so the shell's
/// cwd alone would freeze at launch time and never reflect what the foreground
/// process is actually showing — falling back to the shell's cwd when the
/// foreground process's is unreadable (a permissions edge case, or it just
/// exited in a race).
fn cwd_for(fg: i32, shell_pid: i32, cwd_of: impl Fn(i32) -> Option<String>) -> Option<String> {
    if fg <= 0 || fg == shell_pid {
        cwd_of(shell_pid)
    } else {
        cwd_of(fg).or_else(|| cwd_of(shell_pid))
    }
}

/// Pure resolution of a conducted shell's (state, activity, needs_sudo) from
/// the pty foreground pgid. Injected lookups make it unit-testable. Real caller
/// passes (proc_comm, proc_command, proc_has_children). The shell (spawned
/// under `setsid`) is its own process group leader, so a foreground pgid equal
/// to the shell pid means "at the bare prompt" (idle); anything else is a
/// command running in the foreground (working, its command captured as
/// `activity`). `booster_recent` is the text-scan signal from
/// `conduct_multiplex` (a `[sudo] password for` prompt seen crossing
/// master→stdout within the last ~2s); when [`sudo_awaiting`] is true off
/// either signal, `state` is FORCED to `awaiting` regardless of the
/// idle/working computation above — a sudo prompt needs the user NOW.
fn shell_snapshot(
    fg: i32,
    shell_pid: i32,
    booster_recent: bool,
    comm_of: impl Fn(i32) -> Option<String>,
    command_of: impl Fn(i32) -> Option<String>,
    children_of: impl Fn(i32) -> Option<bool>,
) -> (&'static str, Option<String>, bool) {
    let (mut state, activity) = if fg <= 0 || fg == shell_pid {
        // At the bare prompt: idle, but label the row with the shell PROCESS
        // itself (e.g. `bash`) so the terminal roster is never blank.
        ("idle", comm_of(shell_pid))
    } else {
        // A foreground command is running: its cmdline (e.g. `nvim notes.md`,
        // `cargo test`) — the file being edited / the process at work.
        ("working", command_of(fg))
    };
    let fg_is_sudo = fg > 0 && comm_of(fg).as_deref() == Some("sudo");
    let children = if fg_is_sudo { children_of(fg) } else { None };
    let needs_sudo = sudo_awaiting(fg_is_sudo, children, booster_recent);
    if needs_sudo {
        state = "awaiting";
    }
    (state, activity, needs_sudo)
}

/// Pure resolution of a conducted shell's P-C5 restore snapshot from the pty
/// foreground pgid — mirrors [`shell_snapshot`]'s shape exactly (injected
/// `argv_of` lookup, pure, unit-tested). `idle` reuses the SAME `fg <= 0 ||
/// fg == shell_pid` predicate `shell_snapshot` computes for `state` — kept as
/// its own field here rather than read back off `state` later, since the
/// reap sweep overwrites `state` to `"done"` before its ledger write.
/// `argv` is `None` while idle (no foreground process to capture) and RAW
/// (uncollapsed, unclipped) `/proc/<fg>/cmdline` otherwise — never
/// `proc_command`'s DISPLAY label, which would re-exec the wrong or a
/// truncated binary. `typed` is gated to `idle` HERE, structurally, rather
/// than trusted to the caller: a shell mid-command has no prompt line to
/// reconstruct, so any `typed` the caller passes while working is dropped.
fn restore_snapshot(
    fg: i32,
    shell_pid: i32,
    cwd: Option<String>,
    typed: Option<String>,
    argv_of: impl Fn(i32) -> Option<Vec<String>>,
) -> RestoreSnapshot {
    let idle = fg <= 0 || fg == shell_pid;
    RestoreSnapshot {
        cwd,
        idle,
        argv: if idle { None } else { argv_of(fg) },
        typed: if idle { typed } else { None },
    }
}

/// One conduct-tick refresh for a SHELL session: read the child terminal's
/// foreground process group and the live cwd, and push cwd + the current
/// command + the idle/working state via [`shell_snapshot`], plus the P-C5
/// restore snapshot via [`restore_snapshot`]. `typed` comes from
/// `conduct_multiplex`'s own typed-line buffer (`None` for a headless session,
/// which never reads stdin) — this function has no access to the keystroke
/// stream itself.
///
/// The foreground process group comes from the seam
/// ([`Pty::foreground_pgid`]), which is `tcgetpgrp(2)` on Unix and `0` — "no
/// foreground command" — on native Windows, where no such group exists. The
/// live cwd is `proc_cwd`'s, which has its own named refusal there. So a
/// conducted shell's tick on Windows reports the idle state with the shell's
/// own process name and the cwd its record stamped at registration, and says
/// so here rather than pretending to know what is running.
fn conduct_refresh_shell(
    id: &str,
    pty: &Pty,
    shell_pid: i32,
    booster_recent: bool,
    typed: Option<String>,
) {
    let fg = pty.foreground_pgid();
    let cwd = cwd_for(fg, shell_pid, proc_cwd);
    let (state, activity, needs_sudo) = shell_snapshot(
        fg,
        shell_pid,
        booster_recent,
        proc_comm,
        proc_command,
        proc_has_children,
    );
    let restore = restore_snapshot(fg, shell_pid, cwd.clone(), typed, proc_argv);
    do_session_refresh(id, cwd.as_deref(), activity.as_deref(), state, needs_sudo, restore);
}

/// True iff the `[sudo] password for` prompt appears at the start of a line in
/// `chunk` (buffer start, or right after `\n`/`\r`). The line-start guard keeps
/// `grep '[sudo] password'` / `cat auth.log`-style output from false-triggering,
/// while real sudo prints its prompt at line start. A needle split across two
/// reads is tolerated (missed here, caught next tick by the primary).
fn scan_for_sudo_prompt(chunk: &[u8]) -> bool {
    const NEEDLE: &[u8] = b"[sudo] password for";
    chunk
        .windows(NEEDLE.len())
        .enumerate()
        .any(|(i, w)| w == NEEDLE && (i == 0 || chunk[i - 1] == b'\n' || chunk[i - 1] == b'\r'))
}

/// Cap on the P-C5 typed-line buffer, bytes.
const TYPED_LINE_CAP: usize = 4096;

/// The P-C5 typed-but-unsubmitted prompt-line buffer, reconstructed from the
/// raw keystroke stream written into the pty master — from BOTH real stdin
/// and injection connections (`conduct_multiplex`'s two write sites both
/// `feed` it the same way, since both land in the SAME shell readline
/// buffer). REFUSAL-based, not reconstruction-based: readline editing (arrow
/// keys, `^R` history search, Tab completion, `^U`/`^W` kills) means the
/// keystroke stream is no longer the prompt buffer, so replaying it verbatim
/// would be WRONG, not merely lossy — a silently wrong `typed` puts text the
/// operator never composed one keystroke from running. Any control byte
/// below `0x20` other than the two that SUBMIT the line (`\r`/`\n`), or
/// `0x7f` (DEL), POISONS the buffer for the current line; `\r`/`\n`
/// themselves CLEAR it (submitted) and lift any earlier poison, since the
/// NEXT line starts clean. `typed()` additionally refuses non-UTF-8 and an
/// empty line. Scoped to one CONDUCT process's lifetime — never persisted,
/// never read back after the fact (see the module's own P-C5 note on why a
/// `/proc` snapshot can't recover this).
struct TypedLineBuffer {
    buf: Vec<u8>,
    poisoned: bool,
}
impl TypedLineBuffer {
    fn new() -> Self {
        TypedLineBuffer { buf: Vec::new(), poisoned: false }
    }
    /// Feed bytes written to the master. A line that runs past
    /// `TYPED_LINE_CAP` POISONS rather than truncating: a clipped line is
    /// wrong text, not merely short, and this buffer's whole contract is to
    /// refuse the cases it cannot reconstruct exactly. The buffer stops
    /// growing at the cap either way, so it never grows unbounded, and a
    /// later `\r`/`\n` still clears normally.
    fn feed(&mut self, bytes: &[u8]) {
        for &b in bytes {
            match b {
                b'\r' | b'\n' => {
                    self.buf.clear();
                    self.poisoned = false;
                }
                0x7f => self.poisoned = true,
                b if b < 0x20 => self.poisoned = true,
                _ => {
                    if self.buf.len() < TYPED_LINE_CAP {
                        self.buf.push(b);
                    } else {
                        self.poisoned = true;
                    }
                }
            }
        }
    }
    /// Feed bytes that arrived over an INJECTION connection rather than the
    /// operator's own stdin. They reach the same readline buffer, so the
    /// line stops being reconstructable — but they are not what anyone
    /// TYPED, and `graph send` prefixes a delivered payload with its
    /// provenance (`from <petname> (…tail): `), so replaying them would
    /// preload a line no human composed and that would not even run. The
    /// line is poisoned instead. A `\r`/`\n` still ends it, so an injection
    /// that submits leaves the NEXT line clean rather than poisoning
    /// everything after it.
    fn feed_injected(&mut self, bytes: &[u8]) {
        for &b in bytes {
            match b {
                b'\r' | b'\n' => {
                    self.buf.clear();
                    self.poisoned = false;
                }
                _ => self.poisoned = true,
            }
        }
    }
    /// The current line, or `None` when poisoned, empty (nothing typed since
    /// the last submit), or not valid UTF-8.
    fn typed(&self) -> Option<String> {
        if self.poisoned || self.buf.is_empty() {
            return None;
        }
        std::str::from_utf8(&self.buf).ok().map(str::to_string)
    }
}

/// Whether a P-C5 typed-line buffer should even be instantiated: only an
/// INTERACTIVE shell (`is_shell && read_stdin`) has a real readline prompt to
/// reconstruct. A headless conduct never reads stdin at all
/// (`read_stdin == false`, unconditionally — there is no controlling tty to
/// read from), so it has no typed line, ever; a non-shell (an agent harness)
/// has no shell prompt to begin with. Pure so the gating itself is
/// unit-testable without spawning anything.
fn typed_capture_active(is_shell: bool, read_stdin: bool) -> bool {
    is_shell && read_stdin
}

/// The wrapper's own END vocabulary — a CLOSED set, derived ONLY from the
/// status THIS process observed and its own wall-clock deadline:
/// `exit` (with a real code), `signal` (the agent died by a signal — no code,
/// ever), `timeout` (the deadline fired), `stopped` (no status at all: reaped
/// or killed outside the wrapper). Pure, so every row is a table test. Nothing
/// here reads a harness trace, which is what makes the wrapper's
/// error/timeout/completion reporting independent of the ping-back path.
///
/// The status arrives as the seam's own [`Ended`], so this one table serves
/// both hosts; `signal` is Unix's alone, because no other host has signals to
/// be killed by.
fn classify_end(timed_out: bool, status: Option<Ended>) -> (String, Option<String>, Option<i32>) {
    if timed_out {
        // The wrapper's kill is the WRAPPER's act, not the child's exit: no
        // exit code is claimed for it (the post-kill status travels
        // separately, as `killedWith`).
        return ("timeout".to_string(), None, None);
    }
    match status {
        #[cfg(unix)]
        Some(Ended::Signal(signo)) => ("signal".to_string(), Some(signal_name(signo)), None),
        Some(Ended::Code(code)) => ("exit".to_string(), None, Some(code)),
        Some(Ended::Unknown) | None => ("stopped".to_string(), None, None),
    }
}

/// Where the multiplex loop ended: whether the wrapper's own deadline fired,
/// whether the DIRECT CHILD had already exited while a descendant still held
/// the PTY slave (its own fact, never a timeout), and — for a deadline kill
/// only — the post-kill status as SECONDARY evidence (`killedWith`), never
/// presented as the child's result.
#[derive(Debug, Clone, PartialEq, Eq)]
struct MultiplexEnd {
    timed_out: bool,
    killed_with: Option<String>,
    /// `Some(true)` only where the host could answer the question and the
    /// answer was "a descendant still holds the terminal"; `None` where it
    /// cannot (see `Pty::hung_up`), so a host that cannot answer never has a
    /// guessed fact stamped into every run's outcome.
    pty_held_after_exit: Option<bool>,
}

/// The single-thread multiplexer. Shuttles: real stdin → terminal (you type
/// normally), terminal → real stdout (you read normally), and every byte an
/// injection connection delivers → terminal (INJECTION). A resize the host
/// reports re-sizes the child's terminal. Returns HOW it ended
/// ([`MultiplexEnd`]); the caller reads the child's real status off the seam's
/// cached [`Ended`].
///
/// Every fd, signal and handle this loop touches is behind `super::pty`'s
/// seam, so the loop itself is one function on both hosts: it asks
/// [`wait_ready`] which of its three inputs is readable, services them, and
/// ticks.
fn conduct_multiplex(
    pty: &mut Pty,
    console: &mut Console,
    mut inbox: Option<&mut Inbox>,
    child: &mut PtyChild,
    id: &str,
    is_shell: bool,
    read_stdin: bool,
    sink: &mut OutputSink,
    deadline: Option<std::time::Instant>,
) -> MultiplexEnd {
    // Headless: no controlling tty to read from — the wait is never asked
    // about stdin, and the flag starts already-EOF so the loop never touches
    // it.
    let mut stdin_eof = !read_stdin;
    let mut buf = [0u8; 8192];

    // Live cwd/command tick for a conducted SHELL. A `tail -f` (or any quiet TUI)
    // never produces I/O, so we can't hang the refresh off output — instead the
    // wait gets a ~1s timeout and the tick fires on the elapsed clock. Agents
    // also wake to observe child exit when a descendant keeps the PTY open.
    let shell_pid = child.id();
    const WAIT_TIMEOUT_MS: i32 = 1000;
    let tick_period = std::time::Duration::from_millis(950);
    let mut last_tick = std::time::Instant::now();
    // The sudo-prompt TEXT-SCAN booster: the instant a `[sudo] password for`
    // prompt is seen crossing terminal→stdout, latch a timestamp so the next
    // tick(s) within `SUDO_BOOSTER_WINDOW` treat the shell as sudo-blocked
    // even if the foreground pgid read hasn't caught `sudo` yet.
    let mut sudo_prompt_seen_at: Option<std::time::Instant> = None;
    const SUDO_BOOSTER_WINDOW: std::time::Duration = std::time::Duration::from_secs(2);
    // P-C5: the typed-line buffer only exists for an interactive shell — a
    // headless conduct never reads stdin (no prompt line to reconstruct) and
    // an agent harness has no shell prompt at all.
    let mut typed_buf = if typed_capture_active(is_shell, read_stdin) {
        Some(TypedLineBuffer::new())
    } else {
        None
    };
    if is_shell {
        let typed = typed_buf.as_ref().and_then(|b| b.typed());
        conduct_refresh_shell(id, pty, shell_pid, false, typed); // stamp initial cwd/state now.
    }

    let mut timed_out = false;
    // The DIRECT CHILD's own exit governs completion, so a descendant holding
    // the PTY slave open (the master never hangs up then) can no longer keep a
    // finished run "running" — nor make the deadline fire on a child that
    // exited long ago.
    let mut child_exited = false;
    let mut pty_held_after_exit: Option<bool> = None;
    let mut exited_at: Option<std::time::Instant> = None;
    loop {
        // Nothing left to read from a child that has already exited: stop.
        // (Reached only after every readable byte was serviced, so a finished
        // child's last output is never lost.) The seam says how long "nothing
        // readable right now" must hold before that is believed — a pty's
        // buffered output is already in the master, while a pseudo console
        // renders on its own pipeline and can still be a beat behind.
        if child_exited && !pty.has_output() {
            let settled =
                exited_at.is_none_or(|at| at.elapsed() >= pty.after_exit_settle());
            if settled {
                break;
            }
        }
        if child.exited() && !child_exited {
            child_exited = true;
            exited_at = Some(std::time::Instant::now());
            // The host's own answer, kept as an Option: a host that cannot ask
            // "is a descendant still holding the terminal" contributes NOTHING
            // rather than a default, so `ptyHeldAfterExit` appears only where
            // it is a fact.
            pty_held_after_exit = pty.hung_up().map(|hung| !hung);
        }
        // A6: the wrapper's own WALL-CLOCK deadline — but a child that has
        // ALREADY exited is finished, never "timed out": the deadline is
        // consulted only while the direct child is still running. Quiet output
        // is never a trigger and continuous output is never a reprieve; only
        // the clock decides.
        if !child_exited && deadline.is_some_and(|d| std::time::Instant::now() >= d) {
            timed_out = true;
            break;
        }
        // Service a pending resize before blocking again: a SIGWINCH latch
        // re-read on Unix, the console's own size compared on Windows.
        if let Some(ws) = console.resized() {
            pty.resize(&ws);
        }

        // Bounded wait: `min(remaining, wait_timeout)`, checked on both sides —
        // a huge `--timeout` can neither overflow into a negative (blocking)
        // wait nor spin, and a conducted shell's own ~1s tick still applies.
        // Once the child has exited the wait is zero, so the loop only drains
        // what is already there and stops.
        let wait_ms: i32 = if child_exited {
            0
        } else {
            match deadline {
                Some(d) => {
                    let remaining = d.saturating_duration_since(std::time::Instant::now());
                    let ms = remaining.as_millis().min(i32::MAX as u128) as i32;
                    WAIT_TIMEOUT_MS.min(ms.max(1))
                }
                None => WAIT_TIMEOUT_MS,
            }
        };
        let ready = wait_ready(pty, console, inbox.as_deref(), !stdin_eof, wait_ms);
        // Refresh the conducted shell's live cwd/command/state on the ~1s clock
        // (a plain timeout also ticks; a busy shell ticks at most this often).
        if is_shell && last_tick.elapsed() >= tick_period {
            let booster_recent = sudo_prompt_seen_at
                .map(|t| t.elapsed() < SUDO_BOOSTER_WINDOW)
                .unwrap_or(false);
            let typed = typed_buf.as_ref().and_then(|b| b.typed());
            conduct_refresh_shell(id, pty, shell_pid, booster_recent, typed);
            last_tick = std::time::Instant::now();
        }

        // child terminal → stdout, and EOF (every holder of the slave gone).
        if ready.master {
            match pty.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    // Booster text-scan: shells only, a cheap byte-substring search
                    // (never a String allocation of the whole buffer) over exactly
                    // what was just read, gated to a line-start match.
                    if is_shell && scan_for_sudo_prompt(&buf[..n]) {
                        sudo_prompt_seen_at = Some(std::time::Instant::now());
                    }
                    sink.write(&buf[..n]);
                    // A byte that arrived is the settle window's own reset: it
                    // is quiet, not the clock, that ends a drain.
                    if child_exited {
                        exited_at = Some(std::time::Instant::now());
                    }
                }
                // A readiness that turned out to be nothing is not an end; a
                // hard read error is.
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(_) => break,
            }
        }

        // real stdin → child terminal.
        if ready.stdin && !stdin_eof {
            match console.read(&mut buf) {
                Ok(0) => stdin_eof = true, // our own stdin closed; keep bridging the rest.
                Ok(n) => {
                    if let Some(tb) = typed_buf.as_mut() {
                        tb.feed(&buf[..n]);
                    }
                    pty.write(&buf[..n]);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(_) => stdin_eof = true,
            }
        }

        // injection connections → child terminal.
        if ready.inbox {
            if let Some(inbox) = inbox.as_mut() {
                for chunk in inbox.drain() {
                    // Injected bytes reach the same readline buffer, but
                    // they are not what anyone TYPED — and a delivered
                    // payload carries its provenance prefix, so replaying
                    // them would preload a line no human composed.
                    if let Some(tb) = typed_buf.as_mut() {
                        tb.feed_injected(&chunk);
                    }
                    pty.write(&chunk);
                }
            }
        }
    }

    // A real exit status is read by the CALLER (the seam caches it, so its own
    // `ended` returns the same value immediately) — this loop only reports HOW
    // it ended. A deadline kill happens here, on the DIRECT child this process
    // spawned: never a roster-resolved id, never an ancestry walk, so no
    // unrelated session can ever be the target. The post-kill status is kept as
    // `killedWith` — secondary evidence, never the result.
    let killed_with = if timed_out {
        child.kill();
        child
            .wait_for(std::time::Duration::from_secs(2))
            .map(|ended| killed_with_label(&ended))
    } else {
        None
    };

    MultiplexEnd { timed_out, killed_with, pty_held_after_exit }
}

/// The `killedWith` label: the post-kill status as SECONDARY evidence, in the
/// same vocabulary the outcome uses — `signal(KILL)` where the host has
/// signals, `exit(<code>)` where the kill was a `TerminateProcess` and the
/// code is therefore this seam's own.
fn killed_with_label(ended: &Ended) -> String {
    match ended {
        #[cfg(unix)]
        Ended::Signal(signo) => format!("signal({})", signal_name(*signo)),
        Ended::Code(code) => format!("exit({code})"),
        _ => "stopped".to_string(),
    }
}

/// `aoide conduct [--agent A] [--parent P] [--id I] -- <command …>` — the
/// controllable conducted session on a terminal. Registration semantics
/// (spawn FIRST so a failed exec registers no ghost; running → done; exit
/// mirrored, real code in `data.exitCode`; `AOIDE_SESSION_ID` exported) PLUS:
/// its own terminal + controlling tty, a per-session injection socket, the
/// `conductable`/`socket` fields on the record so `graph send` can steer it, and
/// a best-effort `windowAddress` (phase ② discovery) so the focus jump
/// (`focus_session`) can reach it. Both hosts: the terminal itself is
/// `super::pty`'s seam.
pub fn session_conduct(inv: &Invocation) -> Outcome {
    let cmd = "conduct";
    if inv.args.is_empty() {
        return Outcome::usage(
            cmd,
            "usage: aoide conduct [--agent <name>] [--parent <sessionId>] [--id <id>] -- <command …>",
        );
    }
    let program = inv.args[0].clone();
    let agent = inv
        .flags
        .get("agent")
        .cloned()
        .unwrap_or_else(|| command_basename(&program));
    let id = inv
        .flags
        .get("id")
        .cloned()
        .unwrap_or_else(|| format!("conduct-{}-{}", std::process::id(), unix_ts()));
    let cwd = std::env::current_dir()
        .ok()
        .map(|p| p.to_string_lossy().into_owned());
    let socket_path = conduct_socket_path(&id);

    // Seed the child's terminal with the real console's geometry so a TUI
    // opens correctly sized. A headless conduct usually has NO console
    // (spawned by another process), which would leave the terminal at
    // 0 rows x 0 cols — a geometry full-screen TUIs misrender against or
    // refuse outright. Fall back to a conventional 80x24 there; the
    // interactive no-console case keeps its historical None so nothing
    // changes (on native Windows the seam's own arm substitutes that same
    // conventional size, because a 0x0 pseudo console is not a console).
    let headless = inv.flag_present("headless");
    let ws = Console::size().or(if headless { Some(WinSize::conventional()) } else { None });

    // Managed task wrapper mode (`spawn --task <slug>`): the slug (also the
    // task mailbox name) and the absolute path of the write-once instruction
    // sidecar `spawn` already wrote. Read BEFORE the child is spawned,
    // because both are exported into that child's own environment
    // (`spawn_on_pty`'s `AOIDE_TASK`/`AOIDE_TASK_INSTRUCTIONS`) — the
    // instructions must be genuinely reachable by the agent, not a
    // viewer-only sidecar.
    let report_to = inv
        .flags
        .get("report-to")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if let Some(name) = &report_to {
        if !aoide_storage::node_store::valid_node_name(name) {
            return Outcome::usage(
                cmd,
                format!(
                    "--report-to `{name}` is not a legal mailbox name (^[a-z0-9][a-z0-9-]*$)"
                ),
            );
        }
    }
    let task = inv
        .flags
        .get("task")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(|slug| TaskContext {
            slug,
            instructions_path: inv
                .flags
                .get("instructions-path")
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            report_to,
        });
    // A6's deadline: a wall-clock instant computed BEFORE the child exists, so
    // an unusable value refuses without spawning anything. Checked arithmetic —
    // a `--timeout` too large to add to this instant is a taught usage error,
    // never a panic and never a silently ignored deadline.
    let timeout_secs = match inv.flags.get("timeout") {
        None => None,
        Some(raw) => match raw.trim().parse::<u64>() {
            Ok(secs) if secs > 0 => Some(secs),
            _ => {
                return Outcome::usage(
                    cmd,
                    format!(
                        "--timeout must be a positive whole number of seconds (got `{raw}`); \
                         omit it for no deadline"
                    ),
                )
            }
        },
    };
    let deadline = match timeout_secs {
        Some(secs) => match std::time::Instant::now()
            .checked_add(std::time::Duration::from_secs(secs))
        {
            Some(at) => Some(at),
            None => {
                return Outcome::usage(
                    cmd,
                    format!("--timeout {secs} is too large to represent as a deadline"),
                )
            }
        },
        None => None,
    };

    // Spawn FIRST: a failed exec must register no session (parity with `wrap`).
    let (mut child, mut pty) = match spawn_on_pty(&program, &inv.args[1..], &id, ws, task.as_ref()) {
        Ok(v) => v,
        Err(e) => return Outcome::error(cmd, format!("failed to conduct `{program}`: {e}")),
    };

    // Bind the per-session injection socket (best-effort: a bind failure leaves
    // the session running but un-injectable — recorded as conductable=false).
    let inbox = Inbox::bind(&socket_path, &id).ok();
    let conductable = inbox.is_some();
    let socket_str = socket_path.to_string_lossy().into_owned();

    // Phase ②: best-effort window-address discovery (never fails/slows
    // conduct) — INTERACTIVE only. `headless` has no controlling tty at all,
    // but its `/proc` ancestry still passes straight through whatever
    // launched it (a shell, an agent, a terminal) — `setsid()` (below)
    // detaches its TTY session, never its OS parent — so an unconditional
    // discovery here found and stamped the ENCLOSING terminal's window onto
    // a headless wrap's own record (task #89, review round 2): a `graph
    // spawn` run from inside an agent's shell tool re-acquired its
    // grandparent terminal's window every time, defeating the windowless-
    // lineage fix below (its whole premise is that a headless wrap's own
    // record NEVER holds a window).
    let window = if headless { None } else { discover_window_address() };

    // Automatic parenting (task #89): explicit `--parent` > this registering
    // process's own `/proc` ancestry matched against a live agent's
    // `hookAncestry` > the ambient `AOIDE_SESSION_ID` env — see
    // `window::resolve_registration_parent`'s own doc for the full
    // reasoning (a nested headless `conduct --headless` launched from
    // inside an agent's shell tool — `graph spawn`'s own re-exec — otherwise
    // registers parentless/sibling instead of as that agent's child).
    let sessions_snapshot = load_stage::<SessionsFile>(&sessions_path())
        .map(|f| f.sessions)
        .unwrap_or_default();
    let parent = resolve_registration_parent(
        inv.flags.get("parent").map(String::as_str),
        &id,
        &sessions_snapshot,
    );

    // Register running + conductable with its socket, so `graph send` resolves it.
    let _ = do_session_start(
        &id,
        Some(&agent),
        cwd.as_deref(),
        window.as_deref(),
        parent.as_deref(),
        Some(conductable),
        if conductable {
            Some(socket_str.as_str())
        } else {
            None
        },
        // A5: in task mode the session's NAME is the task's slug (a tracked
        // regular identity — `kind` stays a normal session, never a `sub:`
        // sub-agent card), so the roster and the DAG node read as the task.
        task.as_ref().map(|t| t.slug.as_str()),
        // Record THIS conduct process's pid (not the PTY child's): conduct owns
        // the session lifecycle — the `do_session_end` at the bottom of this fn
        // always resolves the record on any NORMAL exit. Only conduct's own
        // uncatchable death (SUPER+Q SIGKILLs the whole kitty→shell→conduct tree)
        // leaves the record stranded `running`, and then `/proc/<this-pid>`
        // vanishes: the reaper's pid signal. (It is also the pid already embedded
        // in the default `conduct-<pid>-<ts>` id.)
        Some(std::process::id()),
    );
    // Stamp `headless` unconditionally (not only once the log file opens
    // below) — it is a registration fact about THIS wrap's own record, the
    // PERMANENT signal `windowless_by_lineage` and
    // `resolve_pending_session_windows` key off (task #89, review round 2):
    // even if the discovery gate above or the listener skip below somehow
    // missed, this field is what makes a headless wrap's own windowlessness
    // survive a corrupted `windowAddress`.
    if headless {
        stamp_headless(&id);
    }
    // `spawned`, on the same footing and for the same reason: a registration
    // fact about THIS record, stamped here in the child rather than by
    // `spawn` after the fact, because `spawn` returns `registered: false`
    // once its socket wait times out — and a spawn that went that wrong is
    // the one most likely to end up abandoned. Both launch modes reach here;
    // `--windowed` is a spawn no less than the headless default.
    if inv.flag_present("spawned") {
        stamp_spawned(&id);
    }
    // `shell`, on the same footing: a registration fact about THIS record,
    // stamped from the argv this process is actually conducting (launchers
    // resolved by `program_is_a_shell`), so every lane that refuses to type
    // into a shell can read it off the record without having been here. Not
    // once-only like the two above — a re-registration of the same id must
    // follow the CURRENT argv, setting it for `-- env bash` and clearing it
    // for a harness.
    stamp_shell(&id, program_is_a_shell(&inv.args));
    // Managed task wrapper: stamp the run's own two task keys (`task` =
    // the slug AND its mailbox name, `instructionsPath` = the write-once
    // sidecar the spawner wrote). One writer — this child — so there is no
    // race with `spawn`, which has already returned by now.
    if let Some(task) = &task {
        stamp_task(&id, &task.slug, task.instructions_path.as_deref(), task.report_to.as_deref());
        // A1: the CHILD is its task mailbox's own reader — enrolled by session
        // id, idempotent and best-effort, so a direct `conduct --task` (never
        // launched by `spawn`) still gets its reader. `enrol_reader` never
        // moves an existing mark, so calling it on both paths is safe.
        let _ = aoide_storage::mail::enrol_reader(&task.slug, &id);
        // A4: a ROLE report mailbox (`--report-to <name>`) can carry a real
        // reader and a real doorbell latch, so the parent is enrolled under it.
        // Never under the parent's own id: that reader key IS the mailbox name,
        // which the ringer never arms (and `enrol_reader` refuses
        // `reader == name`) — a session-id mailbox is readable but not
        // ringable, and the lane records that reason.
        if let Some(name) = task.report_to.as_deref() {
            if let Some(p) = parent.as_deref().filter(|p| *p != name) {
                let _ = aoide_storage::mail::enrol_reader(name, p);
            }
        }
    }
    // Origin, LOCAL-CLASS ONLY (P-P3, `docs/architecture/PAIRING.md`
    // decision 7; tightened at LANE IDENTITY P-ID0, G16/G5): inherited
    // process env is exactly what a same-uid process can set on ITSELF
    // before invoking `aoide conduct` directly, so a `node:*` shape read
    // here is unauthenticated and must never be trusted — that shape now
    // comes ONLY from `aoide-server::a2a::do_spawn` stamping the record
    // directly at the door where the node name IS authenticated
    // (`stamp_spawn_provenance` in `crates/server/src/a2a.rs`), never threaded
    // through this env var. A taught refusal, not a panic: a hostile
    // `AOIDE_SESSION_ORIGIN=node:X` simply fails to stamp.
    //
    // `remoteParent` has no counterpart read here AT ALL (P-RSA S3): no env
    // var, no `--parent`-shaped flag, nothing for this function to refuse —
    // the door is its only writer, and a local `conduct`/`spawn` cannot even
    // name one. That absence is the invariant (CONTRACTS.md §4's
    // `remoteParent` paragraph, `stamp_remote_parent`'s own doc), pinned by
    // a test rather than guarded by dead code.
    if let Ok(origin) = std::env::var("AOIDE_SESSION_ORIGIN") {
        if is_node_origin(&origin) {
            eprintln!(
                "aoide conduct: refusing to stamp origin `{origin}` from AOIDE_SESSION_ORIGIN — a node:* origin may only be stamped by the a2a door itself"
            );
        } else if !origin.is_empty() {
            stamp_origin(&id, &origin);
        }
    }

    // Every conduct-owned pty tees its master-read output to the per-session
    // log (task #15, the "everything tees" ruling — no opt-out, interactive
    // included, not just `--headless`). `--headless` has no controlling tty
    // at all, so the log is the ONLY sink and the multiplexer never reads
    // stdin (there is nothing to read it from); interactive keeps writing
    // to the real stdout exactly as before and additionally mirrors the
    // same bytes into the log. Only the pty's OWN output crosses this tee —
    // what the pty emits, master-read side — never raw typed stdin: a
    // no-echo `sudo` password prompt is never echoed back down the master
    // by anything but the child's own tty, so it never lands in the log
    // either, headless or interactive. (The `headless` flag itself is read
    // above, where the pty winsize fallback needs it.)
    let mut sink = OutputSink::Stdout;
    if let Some((f, log_path)) = open_session_log(&id) {
        set_session_log_path(&id, &log_path.to_string_lossy());
        sink = if headless { OutputSink::Log(f) } else { OutputSink::StdoutAndLog(f) };
    }
    // A log that can't be opened (e.g. an unwritable state dir, or a
    // permissions call that fails) must not kill the session — degrade to
    // `Stdout` alone, same posture as the socket-bind best-effort above.

    // Raw-mode the real console + arm its resize source (interactive only — a
    // headless session has no console to raw-mode or resize). The guard
    // restores the console on EVERY path below — normal return and unwind
    // alike.
    let mut console = Console::attach(headless);
    if !headless {
        if let Some(ws) = ws {
            pty.resize(&ws);
        }
    }

    // `conduct_multiplex` already waited on the child, and the seam CACHES
    // that status — so this read returns the SAME `Ended` immediately rather
    // than blocking or lying. It is how the REAL status is read here:
    // `Ended::Signal` carries no code (the agent was killed), and that absence
    // is the honest answer — a kill is never dressed as a numeric code on the
    // record or in the report (plan §3.1: absent, never 0).
    let mut inbox = inbox;
    let end = conduct_multiplex(
        &mut pty,
        &mut console,
        inbox.as_mut(),
        &mut child,
        &id,
        program_is_a_shell(&inv.args),
        !headless,
        &mut sink,
        deadline,
    );
    // The child's REAL status, read off the seam's cache (the multiplexer
    // already waited, so this returns the same value at once), and the
    // discriminated end: exit / signal / timeout / stopped. A signal death has
    // NO exit code — the old `code().unwrap_or(-1)` collapse is gone.
    let ended = child.ended();
    let (outcome, signal, exit_code) = classify_end(end.timed_out, Some(ended));

    // Restore the console, close the channel and unlink the socket, resolve
    // the session — whatever happened. The terminal closes AFTER the output is
    // drained (the multiplexer returned only once it was), which is the order
    // `ClosePseudoConsole` requires.
    console.restore();
    pty.close();
    drop(inbox);
    let _ = std::fs::remove_file(&socket_path);
    // A managed task run's END facts, stamped before the roster exit below so
    // the record carries them the moment it goes `done`. The code is the one
    // `conduct_multiplex` just returned — the child's REAL status, never an
    // inference. `exitCode`/`endedAt` are wrapper-run facts, so an ordinary
    // session's record is not touched.
    if task.is_some() {
        stamp_session_exit(&id, exit_code, Some(&outcome), &now_iso_utc());
    }
    let _ = do_session_end(&id);

    let changed = vec![format!("session {id}: running → done")];
    // The end facts, as they actually are: `outcome` is the discriminator, and
    // `exitCode`/`signal`/`timeoutSecs`/`killedWith` appear only where they are
    // facts. No invented fields: an absent key means the fact does not exist.
    let mut data = json!({
        "sessionId": id,
        "agent": agent,
        "outcome": outcome,
        "conductable": conductable,
        "socket": socket_str,
    });
    if let Some(code) = exit_code {
        data["exitCode"] = json!(code);
    }
    if let Some(signo) = &signal {
        data["signal"] = json!(signo);
    }
    if outcome == "timeout" {
        data["timeoutSecs"] = json!(timeout_secs);
        if let Some(killed) = &end.killed_with {
            data["killedWith"] = json!(killed);
        }
    }
    // The PTY outliving its child is its own fact (a descendant holding the
    // slave after the agent exited), never a timeout and never a reason to
    // pretend the run is still going — and it is stamped only where the host
    // answered the question (`Pty::hung_up`'s `Option`).
    if end.pty_held_after_exit == Some(true) {
        data["ptyHeldAfterExit"] = json!(true);
    }
    let result = match (outcome.as_str(), exit_code) {
        ("exit", Some(0)) => format!("`{agent}` finished (conducted session `{id}`)"),
        ("exit", Some(code)) => format!("`{agent}` exited {code} (conducted session `{id}`)"),
        ("signal", _) => format!(
            "`{agent}` died by signal {} (conducted session `{id}`)",
            signal.clone().unwrap_or_default()
        ),
        ("timeout", _) => format!(
            "`{agent}` hit its --timeout of {}s and was killed (conducted session `{id}`)",
            timeout_secs.unwrap_or_default()
        ),
        _ => format!("`{agent}` stopped without a status (conducted session `{id}`)"),
    };
    let outcome_ok = outcome == "exit" && exit_code == Some(0);
    if outcome_ok {
        Outcome::ok(cmd, result).changed(changed).with_data(data)
    } else {
        Outcome::error(cmd, result).changed(changed).with_data(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::session_store::upsert_session;
    use crate::graph::testutil::*;

    /// `spawn_on_pty`'s env shaping, pinned without spawning anything: the
    /// PARENT harness's session markers are removed, the aoide parent edge and
    /// the operator's own variables are not. The command under test is built
    /// exactly as [`spawn_on_pty`] builds its own, so a marker that slipped
    /// back into that function would have to slip past this assertion too.
    /// Unix's own shape of the pin: the Windows arm hands `CreateProcessW` a
    /// built environment block and pins that instead
    /// (`pty::tests::environment_block_shapes_the_childs_own_environment`),
    /// against the same marker list.
    #[cfg(unix)]
    #[test]
    fn spawn_on_pty_drops_the_parent_harnesss_session_markers_and_keeps_the_rest() {
        let id = "scrub-target";
        let mut cmd = std::process::Command::new("/bin/true");
        cmd.args(["--flag"])
            .env("AOIDE_SESSION_ID", id)
            // The operator's own environment: a config home, a credential, a
            // preference — none of them a session fact of the parent.
            .env("CLAUDE_CONFIG_DIR", "/home/someone/.claude")
            .env("ANTHROPIC_API_KEY", "sk-not-a-real-key")
            .env("CLAUDE_EFFORT", "high")
            // The parent's session facts, each of which the child must not see.
            .env("CLAUDECODE", "1")
            .env("CLAUDE_CODE_CHILD_SESSION", "1")
            .env("CLAUDE_CODE_SESSION_ID", "parent-uuid")
            .env("CLAUDE_PID", "12345");
        scrub_session_markers(&mut cmd);

        let envs: std::collections::HashMap<_, _> = cmd.get_envs().collect();
        assert_eq!(
            envs.get(std::ffi::OsStr::new("AOIDE_SESSION_ID")),
            Some(&Some(std::ffi::OsStr::new(id))),
            "the child's own aoide id is exported, never scrubbed"
        );
        for name in [
            "CLAUDE_CONFIG_DIR",
            "ANTHROPIC_API_KEY",
            "CLAUDE_EFFORT",
        ] {
            assert!(
                matches!(envs.get(std::ffi::OsStr::new(name)), Some(Some(_))),
                "the operator's own variable was scrubbed: {name}"
            );
        }
        for name in aoide_protocol::agents::session_env_markers() {
            if name == "AOIDE_SESSION_ID" {
                continue;
            }
            assert!(
                matches!(envs.get(std::ffi::OsStr::new(name)), Some(None)),
                "session marker not removed from the child: {name}"
            );
        }
        // An unlisted name is never touched — the list is names, not a policy
        // about anything that looks like a harness variable.
        assert!(!envs.contains_key(std::ffi::OsStr::new("PATH")));
    }

    /// L4 (branch review): the scrub pinned through the ONE exec point itself,
    /// not just its helper — a test that calls `scrub_session_markers` on a
    /// Command it built for the purpose cannot catch `spawn_on_pty` dropping
    /// the call. This reads the real child's own environment back out of
    /// `/proc`, so the invariant holds where the agent actually starts.
    /// GATED on Unix with its reason: the subject is `spawn_on_pty`'s own
    /// `pre_exec` environment, and `spawn_on_pty` is the PTY capability (see
    /// the module note) — the fixture spawns `sh` on a real pty.
    #[cfg(unix)]
    #[test]
    fn spawn_on_pty_execs_the_agent_with_the_markers_actually_gone() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "CLAUDECODE",
            "CLAUDE_CODE_CHILD_SESSION",
            "CLAUDE_PID",
            "CLAUDE_CODE_SESSION_ID",
            "CLAUDE_CONFIG_DIR",
            "AOIDE_SESSION_ID",
        ]);
        // The environment a claude-session child would inherit, plus one of
        // the operator's own variables (which must survive).
        std::env::set_var("CLAUDECODE", "1");
        std::env::set_var("CLAUDE_CODE_CHILD_SESSION", "1");
        std::env::set_var("CLAUDE_PID", "4242");
        std::env::set_var("CLAUDE_CODE_SESSION_ID", "parent-uuid");
        std::env::set_var("CLAUDE_CONFIG_DIR", "/home/someone/.claude");
        std::env::set_var("AOIDE_SESSION_ID", "not-this-childs");

        let args: Vec<String> = vec!["-c".to_string(), "sleep 30".to_string()];
        let (child, _master) = spawn_on_pty("sh", &args, "scrub-exec-child", None, None)
            .expect("a pty child starts");
        let pid = child.id();
        // The pty child is a session leader; its own environ is the fact. The
        // child sleeps well past this read (a short-lived one raced the
        // `/proc/<pid>/environ` read into a panic under load), and the read
        // itself retries briefly for the same reason.
        let mut environ = Vec::new();
        for _ in 0..20 {
            if let Ok(bytes) = std::fs::read(format!("/proc/{pid}/environ")) {
                environ = bytes;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        assert!(!environ.is_empty(), "own child, own /proc — no environ read back");
        let env: Vec<String> = environ
            .split(|b| *b == 0)
            .map(|v| String::from_utf8_lossy(v).into_owned())
            .collect();

        for gone in ["CLAUDECODE", "CLAUDE_CODE_CHILD_SESSION", "CLAUDE_PID", "CLAUDE_CODE_SESSION_ID"] {
            assert!(
                !env.iter().any(|v| v.starts_with(&format!("{gone}="))),
                "the exec'd child still carries {gone}: {env:?}"
            );
        }
        assert!(
            env.iter().any(|v| v == "CLAUDE_CONFIG_DIR=/home/someone/.claude"),
            "the operator's own variable must survive the scrub"
        );
        assert!(
            env.iter().any(|v| v == "AOIDE_SESSION_ID=scrub-exec-child"),
            "the child exports its OWN aoide id: {env:?}"
        );

        let mut child = child;
        child.kill();
        let _ = child.wait_for(std::time::Duration::from_secs(2));
    }

    #[test]
    fn channel_socket_path_shares_conduct_socket_paths_parent_and_differs_only_by_prefix() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["XDG_RUNTIME_DIR"]);
        let root = unique_stage("channel-socket-path");
        std::env::set_var("XDG_RUNTIME_DIR", &root);

        let id = "sess-1";
        let conduct = conduct_socket_path(id);
        let channel = channel_socket_path(id);

        assert_eq!(
            conduct.parent(),
            channel.parent(),
            "both sockets live in the same aoide runtime dir"
        );
        assert_eq!(
            conduct.file_name().unwrap().to_str().unwrap(),
            format!("session-{id}.sock")
        );
        assert_eq!(
            channel.file_name().unwrap().to_str().unwrap(),
            format!("channel-{id}.sock")
        );
    }

    /// LANE IDENTITY P-ID2 test infra: write `payload` to `socket` from a
    /// process that is genuinely NOT a descendant of the calling (test)
    /// process — a plain `fork()`'d child is still this process's own
    /// child, so a double-fork daemonizes it: the middle child exits
    /// immediately, orphaning the grandchild to whatever reaps orphans
    /// (traditionally pid 1, or a sandbox's own subreaper) — either way,
    /// NOT this test process, so `pid_ancestry` walking up from the
    /// grandchild's pid never reaches back here. Everything the grandchild
    /// touches post-fork (`addr`/`payload`) is built BEFORE the fork call;
    /// the grandchild itself only ever calls async-signal-safe raw `libc`
    /// syscalls (`socket`/`connect`/`write`/`close`/`_exit`) — the same
    /// discipline `spawn_on_pty`'s own `pre_exec` closure documents, never
    /// touching Rust's allocator or any lock a sibling test thread might
    /// hold. The grandchild retries its OWN connect in a raw poll loop (no
    /// `std::thread`, no channel) since the socket may not exist yet the
    /// instant this returns.
    ///
    /// GATED on Unix, with its reason: the fixture IS the `fork(2)` +
    /// `sockaddr_un` dance — "a process that is genuinely not this process's
    /// descendant" is a kernel-ancestry fact proven by double-forking, and
    /// native Windows has neither `fork` nor an equivalent that orphans a
    /// grandchild away from the tree it was made in. Its callers are gated
    /// with it.
    #[cfg(unix)]
    fn spawn_unrelated_writer(socket_path: &std::path::Path, payload: &[u8]) {
        use std::os::unix::ffi::OsStrExt;
        let path_bytes = socket_path.as_os_str().as_bytes();
        let mut addr: libc::sockaddr_un = unsafe { std::mem::zeroed() };
        addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
        assert!(path_bytes.len() < addr.sun_path.len(), "test socket path too long: {socket_path:?}");
        for (slot, byte) in addr.sun_path.iter_mut().zip(path_bytes.iter()) {
            *slot = *byte as libc::c_char;
        }
        let addr_len = std::mem::size_of::<libc::sockaddr_un>() as libc::socklen_t;
        let payload_owned = payload.to_vec();

        // SAFETY: the middle child only calls `setsid`/`fork`/`_exit`
        // (async-signal-safe); the grandchild only touches the
        // already-built `addr`/`payload_owned` and raw socket syscalls,
        // then `_exit`s — never returns into Rust's normal unwind/cleanup
        // path, never allocates, never touches a lock.
        let pid1 = unsafe { libc::fork() };
        if pid1 == 0 {
            unsafe {
                libc::setsid();
                let pid2 = libc::fork();
                if pid2 == 0 {
                    let fd = libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0);
                    if fd >= 0 {
                        for _ in 0..300 {
                            let rc = libc::connect(
                                fd,
                                &addr as *const libc::sockaddr_un as *const libc::sockaddr,
                                addr_len,
                            );
                            if rc == 0 {
                                libc::write(
                                    fd,
                                    payload_owned.as_ptr() as *const libc::c_void,
                                    payload_owned.len(),
                                );
                                break;
                            }
                            let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 10_000_000 };
                            libc::nanosleep(&mut ts, std::ptr::null_mut());
                        }
                        libc::close(fd);
                    }
                    libc::_exit(0);
                }
                libc::_exit(0); // the middle child exits immediately — orphans the grandchild.
            }
        } else if pid1 > 0 {
            unsafe {
                let mut status: libc::c_int = 0;
                libc::waitpid(pid1, &mut status, 0); // reap the middle child — no zombie left behind.
            }
        }
    }

    /// GATED on Unix with its reason: `OutputSink` is the pty-master's own
    /// output router (module note) — it exists only where there is a master fd
    /// to route, and its `Stdout` arm writes to `STDOUT_FILENO` by number.
    #[cfg(unix)]
    #[test]
    fn output_sink_log_appends_bytes_and_they_read_back() {
        let dir = unique_stage("output-sink-log");
        let path = dir.join("s.log");
        let f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .unwrap();
        let mut sink = OutputSink::Log(f);
        sink.write(b"hello ");
        sink.write(b"world\n");
        let got = std::fs::read_to_string(&path).unwrap();
        assert_eq!(got, "hello world\n");
        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn friendly_editor_command_shows_editor_and_file_by_basename() {
        // A plain file argument, editor resolved to a full store path.
        assert_eq!(
            friendly_editor_command(&argv(&[
                "/etc/profiles/per-user/khoa/bin/nvim",
                "modules/facets/quickshell/qml/TerminalsGadget.qml",
            ])),
            Some("nvim TerminalsGadget.qml".to_string())
        );
        // Bare invocation, no file → just the editor name.
        assert_eq!(
            friendly_editor_command(&argv(&["/run/current-system/sw/bin/nvim"])),
            Some("nvim".to_string())
        );
        // A dotfiles-style startup flag with a value ahead of the real file: the
        // LAST non-flag argument (scanning from the end) is the file, not the
        // flag's value.
        assert_eq!(
            friendly_editor_command(&argv(&[
                "nvim",
                "--cmd",
                "lua vim.g.x=1",
                "notes.md",
            ])),
            Some("nvim notes.md".to_string())
        );
        // A value-taking flag with NO file: the flag's value ("lua vim.g.x=1")
        // contains a space, so it's rejected as a candidate file too — falls
        // through to bare "nvim", not the flag's value misread as a filename.
        assert_eq!(
            friendly_editor_command(&argv(&["nvim", "--cmd", "lua vim.g.x=1"])),
            Some("nvim".to_string())
        );
        // A non-editor binary → None, so the caller falls back to the generic
        // full-cmdline display.
        assert_eq!(
            friendly_editor_command(&argv(&["cargo", "test"])),
            None
        );
        assert_eq!(friendly_editor_command(&argv(&[])), None);
    }
    #[test]
    fn generic_command_label_collapses_argv0_basename_only() {
        // A full nix-store path single-arg command (confirmed live: a wrapped
        // `yazi` re-execs with argv[0] set to its store path) collapses to the
        // bare basename, not the ugly store path.
        assert_eq!(
            generic_command_label(&argv(&[
                "/nix/store/0qviwy1qgq5i5yy947wv4d7656mb56vs-yazi-0.4.2/bin/yazi",
            ])),
            Some("yazi".to_string())
        );
        // A full store path WITH args: argv[0] collapses to its basename, the
        // arguments (which may legitimately be paths) stay intact.
        assert_eq!(
            generic_command_label(&argv(&[
                "/nix/store/hash-ripgrep-14.1.0/bin/rg",
                "pattern",
                "file.rs",
            ])),
            Some("rg pattern file.rs".to_string())
        );
        // A plain bare command (no path) is unaffected.
        assert_eq!(
            generic_command_label(&argv(&["yazi"])),
            Some("yazi".to_string())
        );
        // Args after a bare command are left untouched.
        assert_eq!(
            generic_command_label(&argv(&["cargo", "test"])),
            Some("cargo test".to_string())
        );
        // Empty argv → None (caller falls back to `comm`).
        assert_eq!(generic_command_label(&argv(&[])), None);
    }
    #[test]
    fn sudo_awaiting_gates_the_primary_signal_on_children() {
        // The booster fires regardless of the primary signal's inputs.
        assert!(sudo_awaiting(false, None, true));
        assert!(sudo_awaiting(true, Some(true), true));
        assert!(sudo_awaiting(true, Some(false), true));
        assert!(sudo_awaiting(true, None, true));
        // fg IS sudo, and it has forked NO children yet: still at the PAM
        // prompt — the primary signal fires.
        assert!(sudo_awaiting(true, Some(false), false));
        // fg IS sudo, but it HAS forked a child: auth already succeeded and the
        // command is running (e.g. cached-cred `sudo nixos-rebuild switch` — a
        // multi-minute build with no prompt at all). The primary must NOT fire.
        assert!(!sudo_awaiting(true, Some(true), false));
        // fg IS sudo but children are unreadable: don't trust the primary,
        // fall back to the booster alone (which is false here).
        assert!(!sudo_awaiting(true, None, false));
        // fg is not sudo at all: primary never fires, no booster.
        assert!(!sudo_awaiting(false, None, false));
        assert!(!sudo_awaiting(false, Some(false), false));
        assert!(!sudo_awaiting(false, Some(true), false));
    }
    #[test]
    fn shell_snapshot_idle_at_bare_prompt() {
        // fg == shell_pid: idle, activity from comm_of(shell_pid), no sudo.
        let (state, activity, needs_sudo) = shell_snapshot(
            100,
            100,
            false,
            |_| Some("bash".to_string()),
            |_| panic!("command_of should not be consulted at the bare prompt"),
            |_| panic!("children_of should not be consulted when fg isn't sudo"),
        );
        assert_eq!(state, "idle");
        assert_eq!(activity.as_deref(), Some("bash"));
        assert!(!needs_sudo);
    }
    #[test]
    fn shell_snapshot_working_reads_foreground_command() {
        // fg != shell_pid, and it's not sudo: working, activity from command_of(fg).
        let (state, activity, needs_sudo) = shell_snapshot(
            200,
            100,
            false,
            |_| Some("cargo".to_string()),
            |pid| {
                assert_eq!(pid, 200);
                Some("cargo test".to_string())
            },
            |_| panic!("children_of should not be consulted when fg isn't sudo"),
        );
        assert_eq!(state, "working");
        assert_eq!(activity.as_deref(), Some("cargo test"));
        assert!(!needs_sudo);
    }
    #[test]
    fn shell_snapshot_forces_awaiting_while_sudo_has_no_children() {
        // fg IS sudo, with no children yet (still at the password prompt):
        // state is FORCED to awaiting and needs_sudo is true, regardless of
        // what command_of would have said.
        let (state, activity, needs_sudo) = shell_snapshot(
            300,
            100,
            false,
            |_| Some("sudo".to_string()),
            |_| Some("sudo nixos-rebuild switch".to_string()),
            |pid| {
                assert_eq!(pid, 300);
                Some(false)
            },
        );
        assert_eq!(state, "awaiting");
        assert_eq!(activity.as_deref(), Some("sudo nixos-rebuild switch"));
        assert!(needs_sudo);
    }
    #[test]
    fn shell_snapshot_cached_cred_sudo_does_not_force_awaiting() {
        // fg IS sudo, but it has already forked its command child (cached
        // creds, no prompt): state stays "working" off the normal computation,
        // needs_sudo is false — the F1 fix's whole point.
        let (state, activity, needs_sudo) = shell_snapshot(
            300,
            100,
            false,
            |_| Some("sudo".to_string()),
            |_| Some("sudo nixos-rebuild switch".to_string()),
            |pid| {
                assert_eq!(pid, 300);
                Some(true)
            },
        );
        assert_eq!(state, "working");
        assert_eq!(activity.as_deref(), Some("sudo nixos-rebuild switch"));
        assert!(!needs_sudo);
    }
    #[test]
    fn scan_for_sudo_prompt_requires_line_start() {
        const NEEDLE: &str = "[sudo] password for";
        // At the very start of the buffer: true.
        assert!(scan_for_sudo_prompt(NEEDLE.as_bytes()));
        // Right after a newline: true.
        let after_nl = format!("hello\n{NEEDLE}");
        assert!(scan_for_sudo_prompt(after_nl.as_bytes()));
        // Right after a carriage return (a pty commonly emits \r\n): true.
        let after_cr = format!("hello\r{NEEDLE}");
        assert!(scan_for_sudo_prompt(after_cr.as_bytes()));
        // Mid-line — e.g. a compiler error message or `grep` output quoting the
        // needle — must NOT false-trigger.
        let mid_line = format!("foo.rs:9:{NEEDLE} x");
        assert!(!scan_for_sudo_prompt(mid_line.as_bytes()));
        // No needle at all.
        assert!(!scan_for_sudo_prompt(b"just some ordinary shell output"));
        // Empty / too-short buffers must not panic.
        assert!(!scan_for_sudo_prompt(b""));
        assert!(!scan_for_sudo_prompt(b"[sudo]"));
    }
    #[test]
    fn cwd_for_prefers_foreground_process_then_falls_back_to_shell() {
        const SHELL_PID: i32 = 100;
        const FG_PID: i32 = 200;
        // A foreground command runs and its cwd is readable and DIFFERS from the
        // shell's (yazi navigated elsewhere): the foreground process's own cwd
        // wins, so the widget follows it rather than freezing at launch.
        let lookup = |pid: i32| match pid {
            SHELL_PID => Some("/home/khoa".to_string()),
            FG_PID => Some("/home/khoa/dxflake".to_string()),
            _ => None,
        };
        assert_eq!(
            cwd_for(FG_PID, SHELL_PID, lookup),
            Some("/home/khoa/dxflake".to_string())
        );
        // A foreground command runs but its cwd is unreadable (None — a perms
        // edge case, or it exited in a race): fall back to the shell's cwd.
        let unreadable_fg = |pid: i32| match pid {
            SHELL_PID => Some("/home/khoa".to_string()),
            _ => None,
        };
        assert_eq!(
            cwd_for(FG_PID, SHELL_PID, unreadable_fg),
            Some("/home/khoa".to_string())
        );
        // At the bare prompt (fg == shell_pid, and the fg <= 0 "no fg" case):
        // always the shell's own cwd, never consulting any other pid.
        assert_eq!(
            cwd_for(SHELL_PID, SHELL_PID, lookup),
            Some("/home/khoa".to_string())
        );
        assert_eq!(cwd_for(0, SHELL_PID, lookup), Some("/home/khoa".to_string()));
    }
    #[cfg(unix)]
    #[test]
    fn parse_cmdline_splits_on_nul_and_drops_empty_segments() {
        // A synthesized raw `/proc/<pid>/cmdline` buffer: NUL-separated,
        // trailing NUL included (the kernel's real shape).
        let raw = b"nvim\0notes.md\0";
        assert_eq!(parse_cmdline(raw), vec!["nvim".to_string(), "notes.md".to_string()]);
        // Two NULs in a row (an empty argv element) never produces an empty
        // string in the output.
        let raw2 = b"cargo\0\0test\0";
        assert_eq!(parse_cmdline(raw2), vec!["cargo".to_string(), "test".to_string()]);
        assert_eq!(parse_cmdline(b""), Vec::<String>::new());
    }
    #[test]
    fn proc_argv_is_none_for_an_unreadable_pid() {
        // A pid this large cannot exist as a real process on this box — the
        // `/proc/<pid>/cmdline` read fails, and `proc_argv` must degrade to
        // `None` rather than propagate the error. No process spawned.
        assert_eq!(proc_argv(i32::MAX), None);
    }
    #[test]
    fn restore_snapshot_idle_at_prompt_yields_no_argv() {
        const SHELL_PID: i32 = 100;
        let snap = restore_snapshot(
            SHELL_PID,
            SHELL_PID,
            Some("/home/khoa/Aoide".to_string()),
            Some("cargo test".to_string()),
            |_| panic!("argv_of should not be consulted at the bare prompt"),
        );
        assert!(snap.idle);
        assert_eq!(snap.argv, None);
        assert_eq!(snap.cwd.as_deref(), Some("/home/khoa/Aoide"));
        // `typed` passes through unchanged while idle.
        assert_eq!(snap.typed.as_deref(), Some("cargo test"));
    }
    #[test]
    fn restore_snapshot_working_yields_full_uncollapsed_argv_and_drops_typed() {
        const SHELL_PID: i32 = 100;
        const FG_PID: i32 = 200;
        let snap = restore_snapshot(
            FG_PID,
            SHELL_PID,
            Some("/home/khoa/Aoide".to_string()),
            // A stale typed buffer from before the foreground command started
            // — must be dropped, never leak through while a command runs.
            Some("leftover".to_string()),
            |pid| {
                assert_eq!(pid, FG_PID);
                Some(vec!["nvim".to_string(), "--cmd".to_string(), "lua x=1".to_string()])
            },
        );
        assert!(!snap.idle);
        assert_eq!(
            snap.argv,
            Some(vec!["nvim".to_string(), "--cmd".to_string(), "lua x=1".to_string()])
        );
        assert_eq!(snap.typed, None, "a shell mid-command has no prompt line to reconstruct");
    }
    #[test]
    fn typed_capture_active_gates_on_shell_and_stdin() {
        // Only an interactive shell has a real readline prompt to capture.
        assert!(typed_capture_active(true, true));
        // A headless conduct never reads stdin — no typed line, ever, even
        // for a conducted shell.
        assert!(!typed_capture_active(true, false));
        // A non-shell (agent harness) has no shell prompt at all, headless
        // or not.
        assert!(!typed_capture_active(false, true));
        assert!(!typed_capture_active(false, false));
    }
    #[test]
    fn program_is_a_shell_reads_the_wrapped_argv_never_the_agent_label() {
        // The task #100 defect, table-driven: shell-likeness is a property of
        // WHAT is being conducted (the wrapped argv), never of WHO it is
        // labelled as (`--agent <name>`). `spawn --agent soak-a -- bash` is a
        // real interactive shell that must tick the same as a plain `bash`
        // conduct — the old `agent == "shell"` gate missed exactly this case.
        let cases: &[(&[&str], bool)] = &[
            (&["bash"], true),
            (&["zsh"], true),
            (&["fish"], true),
            (&["sh"], true),
            (&["/bin/bash"], true),
            (&["/usr/bin/zsh"], true),
            (&["/run/current-system/sw/bin/fish"], true),
            // The wider list the guard lanes need: a login shell that is not
            // one of the four the capture path happens to start with.
            (&["dash"], true),
            (&["ksh"], true),
            (&["mksh"], true),
            (&["csh"], true),
            (&["tcsh"], true),
            (&["nu"], true),
            (&["xonsh"], true),
            (&["busybox"], true),
            (&["elvish"], true),
            (&["osh"], true),
            (&["ash"], true),
            (&["yash"], true),
            // Reached through a launcher: the basename of the FIRST token
            // would answer "env"/"nice"/…, never "shell".
            (&["env", "bash"], true),
            (&["env", "FOO=bar", "bash"], true),
            (&["env", "-u", "FOO", "bash"], true),
            (&["env", "-i", "bash"], true),
            (&["exec", "bash"], true),
            (&["setsid", "bash"], true),
            (&["nohup", "bash"], true),
            (&["nice", "bash"], true),
            (&["nice", "-n", "5", "bash"], true),
            (&["stdbuf", "-oL", "bash"], true),
            (&["stdbuf", "-o", "L", "bash"], true),
            (&["chrt", "-f", "10", "bash"], true),
            (&["ionice", "-c", "2", "bash"], true),
            (&["timeout", "60", "bash"], true),
            (&["timeout", "1m", "bash"], true),
            (&["timeout", "-k", "5", "60", "bash"], true),
            (&["script", "-q", "/dev/null", "bash"], true),
            (&["bash", "-l"], true),
            (&["/bin/sh", "-c", "sleep 1"], true),
            // `nix develop|shell`: the `-c` payload is the question, and with
            // no payload nix opens an interactive shell.
            (&["nix", "develop", "-c", "bash"], true),
            (&["nix", "develop", "--command", "bash"], true),
            (&["nix", "shell", "nixpkgs#foo", "-c", "bash"], true),
            (&["nix", "develop"], true),
            // The one resolved-TO case: no command of their own lands in a
            // login shell, and that is the safe answer.
            (&["su", "-", "khoa"], true),
            (&["doas", "bash"], true),
            (&["sudo", "-u", "root", "bash"], true),
            // The launcher is now transparent — what it forwards to decides.
            (&["env", "claude"], false),
            (&["nice", "-n", "5", "cargo", "test"], false),
            (&["timeout", "600", "claude"], false),
            (&["nix", "develop", "-c", "claude"], false),
            (&["nix", "build", "."], false),
            // Plain non-shells.
            (&["claude"], false),
            (&["kimi"], false),
            (&["pi"], false),
            (&["cargo"], false),
            (&["/usr/bin/vim"], false),
            // A shell-shaped binary named something else entirely still
            // reads by its OWN basename, not any caller-chosen label — this
            // function never sees `--agent` at all.
            (&["bashful"], false),
            // The stated LIMIT, pinned so it is a known edge and not a
            // surprise: argv is all this reads, so a script that execs a
            // shell is not a shell to it.
            (&["./rig.sh"], false),
            (&["/home/khoa/bin/deploy.sh", "--prod"], false),
        ];
        for (argv, expected) in cases {
            let argv: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
            assert_eq!(
                program_is_a_shell(&argv),
                *expected,
                "program_is_a_shell({argv:?}) should be {expected}"
            );
        }
    }
    #[test]
    fn typed_line_buffer_accumulates_plain_text() {
        let mut tb = TypedLineBuffer::new();
        tb.feed(b"cargo");
        tb.feed(b" test");
        assert_eq!(tb.typed().as_deref(), Some("cargo test"));
    }
    #[test]
    fn typed_line_buffer_injected_bytes_poison_rather_than_accumulate() {
        // Found in the P-C7 live soak: `graph send` prefixes a delivered
        // payload with its provenance, so an injected line captured as
        // "typed" read `from quiet-birch (…1892): echo hello` — a line no
        // human composed, which would not even run if preloaded. Injection
        // reaches the same readline buffer as stdin, so the line is no
        // longer reconstructable either way. Refuse it.
        let mut tb = TypedLineBuffer::new();
        tb.feed(b"echo ");
        tb.feed_injected(b"from quiet-birch (...1892): hello");
        assert_eq!(tb.typed(), None, "an injected line is never reported as typed");

        // A submitting injection still ends the line, so the NEXT one starts
        // clean rather than inheriting the poison forever.
        tb.feed_injected(b"\n");
        tb.feed(b"mine");
        assert_eq!(tb.typed().as_deref(), Some("mine"));
    }
    #[test]
    fn typed_line_buffer_carriage_return_and_newline_both_clear() {
        let mut tb = TypedLineBuffer::new();
        tb.feed(b"echo hi\r");
        assert_eq!(tb.typed(), None, "a submitted line is empty, not the old text");
        tb.feed(b"next line\n");
        assert_eq!(tb.typed(), None);
        tb.feed(b"third");
        assert_eq!(tb.typed().as_deref(), Some("third"));
    }
    #[test]
    fn typed_line_buffer_escape_byte_poisons_the_line() {
        // An ESC (0x1b) — the lead byte of every arrow-key/cursor escape
        // sequence — means the keystroke stream is no longer the prompt
        // buffer. Refuse, don't guess.
        let mut tb = TypedLineBuffer::new();
        tb.feed(b"echo hi");
        tb.feed(&[0x1b, b'[', b'A']); // an up-arrow sequence.
        assert_eq!(tb.typed(), None);
        // The poison holds even if more plain text follows on the SAME line.
        tb.feed(b"more");
        assert_eq!(tb.typed(), None);
        // Submitting clears the poison — the NEXT line starts clean.
        tb.feed(b"\n");
        tb.feed(b"clean");
        assert_eq!(tb.typed().as_deref(), Some("clean"));
    }
    #[test]
    fn typed_line_buffer_ctrl_u_poisons_the_line() {
        // ^U (0x15) — a readline line-kill — is exactly the "keystroke stream
        // isn't the prompt buffer anymore" case this mechanism exists for.
        let mut tb = TypedLineBuffer::new();
        tb.feed(b"garbage");
        tb.feed(&[0x15]);
        assert_eq!(tb.typed(), None);
    }
    #[test]
    fn typed_line_buffer_del_byte_poisons_the_line() {
        let mut tb = TypedLineBuffer::new();
        tb.feed(b"oops");
        tb.feed(&[0x7f]); // backspace/DEL.
        assert_eq!(tb.typed(), None);
    }
    #[test]
    fn typed_line_buffer_tab_completion_poisons_the_line() {
        let mut tb = TypedLineBuffer::new();
        tb.feed(b"carg");
        tb.feed(&[0x09]); // Tab — completion may rewrite the whole line.
        assert_eq!(tb.typed(), None);
    }
    #[test]
    fn typed_line_buffer_overflow_poisons_rather_than_truncating() {
        let mut tb = TypedLineBuffer::new();
        // Well past TYPED_LINE_CAP — must not panic or grow unbounded, and
        // must refuse: a clipped line is WRONG text, not merely short, and
        // handing back a prefix would be exactly the silent guess this
        // buffer exists to avoid.
        tb.feed(&[b'x'; 5000]);
        assert_eq!(tb.typed(), None, "an overflowed line must never yield a truncated prefix");
        // Submitting clears the poison — the NEXT line starts clean.
        tb.feed(b"\n");
        tb.feed(b"ok");
        assert_eq!(tb.typed().as_deref(), Some("ok"));
    }
    #[test]
    fn typed_line_buffer_invalid_utf8_yields_none() {
        let mut tb = TypedLineBuffer::new();
        tb.feed(&[0xff, 0xfe]); // not valid UTF-8, and not a poisoning byte.
        assert_eq!(tb.typed(), None);
    }
    #[test]
    fn typed_line_buffer_empty_line_yields_none() {
        let tb = TypedLineBuffer::new();
        assert_eq!(tb.typed(), None);
    }
    /// GATED on Unix with its reason: the fixture writes to the injection
    /// socket from a DOUBLE-FORKED process (`spawn_unrelated_writer`), whose
    /// whole point is a caller that is genuinely not this process's
    /// descendant — a `fork(2)` fact native Windows has no equivalent for.
    #[cfg(unix)]
    #[test]
    fn conduct_injects_socket_bytes_into_the_child() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR"]);

        let root = unique_stage("conduct-inject");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("XDG_RUNTIME_DIR", &root); // socket → <root>/aoide/session-*.sock

        let id = "conduct-test";
        let socket = conduct_socket_path(id);
        let proof = root.join("proof.txt");

        // The child reads ONE line from its (pty) stdin and writes it to a file,
        // then exits — proof the injected bytes reached the child's stdin.
        let script = format!("IFS= read -r line; printf '%s' \"$line\" > {}", proof.display());

        // LANE IDENTITY P-ID2: inject from a DETACHED process (never a
        // same-process thread — since `session_conduct` runs IN this test
        // process, a same-pid connection would now be correctly refused as
        // a self-injection, `spawn_unrelated_writer`'s own doc) — this is
        // exactly the "some other, unrelated sender" shape the accept
        // loop's peercred check must still let through. `conduct` blocks in
        // THIS thread until the wrapped child exits.
        spawn_unrelated_writer(&socket, b"MARKER-42\n");

        let out = session_conduct(&conduct_invocation(&["sh", "-c", &script], &[("id", id)]));

        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        assert_eq!(out.data.as_ref().unwrap()["exitCode"], 0);
        assert_eq!(out.data.as_ref().unwrap()["conductable"], true);

        // The child received the injected line on its stdin.
        let got = std::fs::read_to_string(&proof).unwrap_or_default();
        assert_eq!(got, "MARKER-42", "child received the injected bytes");

        // Registered conductable with its socket, then resolved done.
        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec = s.sessions.iter().find(|r| r.session_id == id).unwrap();
        assert_eq!(rec.state, "done");
        assert_eq!(rec.conductable, Some(true));
        assert!(rec
            .socket
            .as_deref()
            .unwrap()
            .ends_with("session-conduct-test.sock"));
        // Socket unlinked on exit.
        assert!(!socket.exists(), "the control socket is unlinked on exit");

        let _ = std::fs::remove_dir_all(&root);
    }
    /// LANE IDENTITY P-ID2's own integration test: the accept loop reads a
    /// REAL `SO_PEERCRED` off a REAL connecting process and refuses it when
    /// that process's `/proc` ancestry roots back to THIS session's own
    /// pid — a plain `fork()`'d DIRECT CHILD of the test process is exactly
    /// that shape here, since `session_conduct` also runs IN this test
    /// process (module doc's own note on why `spawn_unrelated_writer`
    /// double-forks instead, for the OPPOSITE case). The wrapped child
    /// races a backgrounded `cat` against a bounded `sleep` (no `timeout`
    /// binary dependency — plain POSIX job control) so the test terminates
    /// whether or not anything ever arrives on stdin; the proof file staying
    /// EMPTY is the refusal, not a hang.
    /// GATED on Unix with its reason: the fixture forks a `sh -c` writer that
    /// speaks to the injection socket with raw `sockaddr_un`/`connect(2)`
    /// syscalls — no `fork`, no `sockaddr_un` and no `sh` script on native
    /// Windows (see `spawn_unrelated_writer`'s own gate).
    /// The OTHER side of that gate is what the Windows end-to-end run
    /// (`a_connection_to_a_live_inbox_types_into_the_conducted_child`) has:
    /// there the connector is this process — the conducted child's PARENT — and
    /// it is admitted, because the gate walks UP from the connector and a
    /// parent is not a descendant of its own child.
    #[cfg(unix)]
    #[test]
    fn accept_refuses_a_connection_from_within_its_own_session_subtree() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR"]);

        let root = unique_stage("conduct-self-refuse");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("XDG_RUNTIME_DIR", &root);

        let id = "conduct-self-refuse-test";
        let socket = conduct_socket_path(id);
        let proof = root.join("proof.txt");
        let script = format!(
            "cat > {p} & CP=$!; sleep 1; kill $CP 2>/dev/null; wait $CP 2>/dev/null; true",
            p = proof.display()
        );

        // A DIRECT CHILD of this test process — genuinely "within its own
        // session subtree" from the accept loop's own perspective, since
        // `std::process::id()` there IS this test process's real pid.
        let path_bytes: Vec<u8> = {
            use std::os::unix::ffi::OsStrExt;
            socket.as_os_str().as_bytes().to_vec()
        };
        assert!(path_bytes.len() < 100, "test socket path too long for sockaddr_un: {socket:?}");
        let pid = unsafe { libc::fork() };
        if pid == 0 {
            // SAFETY: only raw, async-signal-safe syscalls post-fork — same
            // discipline `spawn_unrelated_writer` documents. `path_bytes`
            // was built and owned BEFORE the fork call.
            unsafe {
                let mut addr: libc::sockaddr_un = std::mem::zeroed();
                addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
                for (slot, byte) in addr.sun_path.iter_mut().zip(path_bytes.iter()) {
                    *slot = *byte as libc::c_char;
                }
                let addr_len = std::mem::size_of::<libc::sockaddr_un>() as libc::socklen_t;
                let fd = libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0);
                if fd >= 0 {
                    for _ in 0..300 {
                        let rc = libc::connect(
                            fd,
                            &addr as *const libc::sockaddr_un as *const libc::sockaddr,
                            addr_len,
                        );
                        if rc == 0 {
                            let payload = b"SELF-INJECTED\n";
                            libc::write(fd, payload.as_ptr() as *const libc::c_void, payload.len());
                            break;
                        }
                        let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 10_000_000 };
                        libc::nanosleep(&mut ts, std::ptr::null_mut());
                    }
                    libc::close(fd);
                }
                libc::_exit(0);
            }
        }

        let _out = session_conduct(&conduct_invocation(&["sh", "-c", &script], &[("id", id)]));
        if pid > 0 {
            unsafe {
                let mut status: libc::c_int = 0;
                libc::waitpid(pid, &mut status, 0); // reap — no zombie left behind.
            }
        }

        let got = std::fs::read_to_string(&proof).unwrap_or_default();
        assert!(got.is_empty(), "a connection from within the session's own subtree must never reach the pty: got {got:?}");

        let _ = std::fs::remove_dir_all(&root);
    }
    /// The MUST-FIX itself (LANE IDENTITY P-ID2, review round 1): a
    /// DISTINCT, legitimately-registered CHILD session — a REAL OS
    /// descendant of the target, exactly the shape `session_conduct`'s own
    /// non-detaching registration produces for a nested `conduct` — must
    /// be DELIVERED, not refused. The earlier (buggy) shape of this guard
    /// refused ANY connection whose ancestry merely contained the target's
    /// pid, which silently broke this exact, single most common flow: a
    /// child sending to its own live parent via `aoide send --id <parent>
    /// --yes`. Here a single `fork()`'d DIRECT CHILD stands in for that
    /// child session — registered with its OWN session id and its OWN
    /// (real, live) pid BEFORE it connects, so `identity::
    /// is_self_originated`'s nearest-first resolution finds ITS OWN
    /// session first, never the target's, even though the target genuinely
    /// sits one level up in its real `/proc` ancestry.
    /// GATED on Unix with its reason: the fixture forks a `sh -c` writer
    /// speaking raw `sockaddr_un`/`connect(2)` — no `fork`, no `sockaddr_un`
    /// and no `sh` script on native Windows (see `spawn_unrelated_writer`'s
    /// own gate).
    #[cfg(unix)]
    #[test]
    fn accept_delivers_from_a_distinct_child_session_that_is_a_real_os_descendant() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR"]);

        let root = unique_stage("conduct-child-delivers");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("XDG_RUNTIME_DIR", &root);

        let target_id = "conduct-child-delivers-target";
        let socket = conduct_socket_path(target_id);
        let proof = root.join("proof.txt");
        // Same shape as `conduct_injects_socket_bytes_into_the_child`: read
        // ONE line off stdin and prove it arrived.
        let script = format!("IFS= read -r line; printf '%s' \"$line\" > {}", proof.display());

        let path_bytes: Vec<u8> = {
            use std::os::unix::ffi::OsStrExt;
            socket.as_os_str().as_bytes().to_vec()
        };
        assert!(path_bytes.len() < 100, "test socket path too long for sockaddr_un: {socket:?}");
        let pid = unsafe { libc::fork() };
        if pid == 0 {
            // SAFETY: only raw, async-signal-safe syscalls post-fork — same
            // discipline `spawn_unrelated_writer` documents. `path_bytes`
            // was built and owned BEFORE the fork call.
            unsafe {
                let mut addr: libc::sockaddr_un = std::mem::zeroed();
                addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
                for (slot, byte) in addr.sun_path.iter_mut().zip(path_bytes.iter()) {
                    *slot = *byte as libc::c_char;
                }
                let addr_len = std::mem::size_of::<libc::sockaddr_un>() as libc::socklen_t;
                let fd = libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0);
                if fd >= 0 {
                    for _ in 0..300 {
                        let rc = libc::connect(
                            fd,
                            &addr as *const libc::sockaddr_un as *const libc::sockaddr,
                            addr_len,
                        );
                        if rc == 0 {
                            let payload = b"FROM-CHILD-SESSION\n";
                            libc::write(fd, payload.as_ptr() as *const libc::c_void, payload.len());
                            break;
                        }
                        let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 10_000_000 };
                        libc::nanosleep(&mut ts, std::ptr::null_mut());
                    }
                    libc::close(fd);
                }
                libc::_exit(0);
            }
        }

        // Still the (original) parent test process: register the FORKED
        // CHILD's real pid as its OWN, distinct session — BEFORE the
        // target's accept loop ever processes a connection, so there is no
        // race against the retry-connecting grandchild above.
        if pid > 0 {
            crate::graph::session_store::do_session_start(
                "conduct-child-delivers-child",
                Some("claude"),
                Some("/w"),
                None,
                Some(target_id),
                None,
                None,
                None,
                Some(pid as u32),
            );
        }

        let out = session_conduct(&conduct_invocation(&["sh", "-c", &script], &[("id", target_id)]));
        if pid > 0 {
            unsafe {
                let mut status: libc::c_int = 0;
                libc::waitpid(pid, &mut status, 0); // reap — no zombie left behind.
            }
        }

        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let got = std::fs::read_to_string(&proof).unwrap_or_default();
        assert_eq!(
            got, "FROM-CHILD-SESSION",
            "a distinct, legitimately-registered child session must be delivered, not refused"
        );

        let _ = std::fs::remove_dir_all(&root);
    }
    /// GATED on Unix with its reason: the fixture's child is a POSIX `sh -c`
    /// script and the assertions are POSIX facts (a signal number, a shell
    /// exit code). The capability itself is native on both hosts — the
    /// Windows arm's own end-to-end runs are
    /// `conduct_runs_a_child_on_a_pseudo_console_and_resolves_its_session`,
    /// `a_pseudo_console_child_is_read_resized_typed_into_and_observed_to_exit`
    /// and `typing_into_the_pseudo_console_reaches_the_childs_own_stdin`.
    #[cfg(unix)]
    #[test]
    fn conduct_mirrors_a_nonzero_child_exit() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR"]);

        let root = unique_stage("conduct-fail");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("XDG_RUNTIME_DIR", &root);

        let out = session_conduct(&conduct_invocation(
            &["sh", "-c", "exit 7"],
            &[("id", "conduct-fail"), ("agent", "sevens")],
        ));
        assert_eq!(out.status, aoide_protocol::output::Status::Error);
        assert_eq!(out.data.as_ref().unwrap()["exitCode"], 7);

        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec = s.sessions.iter().find(|r| r.session_id == "conduct-fail").unwrap();
        assert_eq!(rec.state, "done");
        assert_eq!(rec.agent, "sevens");

        let _ = std::fs::remove_dir_all(&root);
    }
    /// Task #103's underlying defect, at the layer that already gets it
    /// right: `spawn_on_pty`'s `cmd.spawn()` fails SYNCHRONOUSLY the instant
    /// the configured program isn't found (ENOENT) — no fork, no exec, no
    /// child ever runs — and `session_conduct`'s "spawn FIRST" ordering
    /// (its own doc comment, above) means that failure is caught before
    /// `do_session_start` ever writes a record. This is the sync half of
    /// #103's fix: this crate already registers no ghost for a missing
    /// binary; the a2a door's OWN bounded liveness check (`aoide-server`'s
    /// `do_spawn`/`poll_bounded_exit`) is what closes the remaining gap,
    /// where the door acked `submitted` before this synchronous failure —
    /// running one process removed, as a detached child — was ever visible
    /// to it.
    /// GATED on Unix with its reason: the PTY-backed conduct channel, refused
    /// by name on native Windows (see `conduct_mirrors_a_nonzero_child_exit`).
    #[cfg(unix)]
    #[test]
    fn conduct_of_a_nonexistent_binary_registers_no_session_at_all() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR"]);

        let root = unique_stage("conduct-missing-bin");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("XDG_RUNTIME_DIR", &root);

        let missing = "/definitely/does/not/exist/aoide-test-nonexistent-agent-xyz";
        let out = session_conduct(&conduct_invocation(
            &[missing],
            &[("id", "conduct-missing-bin")],
        ));
        assert_eq!(out.status, aoide_protocol::output::Status::Error, "msg: {}", out.message);
        assert!(
            out.message.contains("failed to conduct"),
            "taught refusal naming the failed exec: {}",
            out.message
        );

        // A missing stage file is `SessionsFile::default()` (empty) per
        // `load_stage`'s own contract — either shape (file absent, or
        // present but empty) proves the same thing: no phantom entry, not
        // even a transient one that later needs the reaper.
        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert!(
            s.sessions.iter().all(|r| r.session_id != "conduct-missing-bin"),
            "a failed exec must register no ghost session — found one: {:?}",
            s.sessions.iter().find(|r| r.session_id == "conduct-missing-bin")
        );

        let _ = std::fs::remove_dir_all(&root);
    }
    /// The interactive twin of the test above (task #15, "everything
    /// tees" — no `--headless` flag here at all): an ordinary conducted
    /// session mirrors its pty output into the SAME per-session log a
    /// headless session always has, IN ADDITION to stdout, and stamps
    /// `logPath` exactly the same way.
    /// GATED on Unix with its reason: the PTY-backed conduct channel, refused
    /// by name on native Windows (see `conduct_mirrors_a_nonzero_child_exit`).
    #[cfg(unix)]
    #[test]
    fn conduct_interactive_also_mirrors_pty_output_to_the_log_and_stamps_log_path() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR"]);

        let root = unique_stage("conduct-interactive-tee");
        let stage = root.join("stage");
        let state = root.join("state");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("AOIDE_STATE_DIR", &state);
        std::env::set_var("XDG_RUNTIME_DIR", &root);

        let out = session_conduct(&conduct_invocation(
            &["sh", "-c", "echo mark-interactive"],
            &[("id", "conduct-interactive-tee")],
        ));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        assert_eq!(out.data.as_ref().unwrap()["exitCode"], 0);

        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec = s.sessions.iter().find(|r| r.session_id == "conduct-interactive-tee").unwrap();
        assert_eq!(rec.state, "done");
        let log_path = rec.log_path.clone().expect("interactive conduct also stamps logPath");
        assert!(log_path.ends_with("conduct-interactive-tee.log"));
        let logged = std::fs::read_to_string(&log_path).unwrap();
        assert!(logged.contains("mark-interactive"), "log contents: {logged:?}");

        let _ = std::fs::remove_dir_all(&root);
    }
    /// Structural, not umask luck (task #15): the log lands `0600` and its
    /// `state/sessions/` parent `0700`, regardless of whatever umask the
    /// test process happens to run under.
    /// GATED on Unix with its reason: the test drives `session_conduct`, which
    /// is the refused PTY capability on native Windows, and the fact it asserts
    /// is a POSIX MODE (`0600`/`0700` as bits). The policy itself is not left
    /// uncovered there — that host's spelling of the same policy is read back
    /// by `owner_only::file_privacy`/`dir_privacy`, which `aoide-storage`'s own
    /// tests exercise on ThinkChiyo.
    #[cfg(unix)]
    #[test]
    fn session_log_and_its_directory_are_created_with_private_permissions() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR"]);

        let root = unique_stage("conduct-log-perms");
        let stage = root.join("stage");
        let state = root.join("state");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("AOIDE_STATE_DIR", &state);
        std::env::set_var("XDG_RUNTIME_DIR", &root);

        let out = session_conduct(&conduct_invocation(
            &["sh", "-c", "echo mark-perms"],
            &[("id", "conduct-log-perms"), ("headless", "true")],
        ));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);

        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec = s.sessions.iter().find(|r| r.session_id == "conduct-log-perms").unwrap();
        let log_path = rec.log_path.clone().expect("logPath must be stamped");

        use std::os::unix::fs::PermissionsExt;
        let file_mode = std::fs::metadata(&log_path).unwrap().permissions().mode() & 0o777;
        assert_eq!(file_mode, 0o600, "log file mode: {file_mode:o}");
        let dir_mode =
            std::fs::metadata(aoide_storage::fs::session_logs_dir()).unwrap().permissions().mode()
                & 0o777;
        assert_eq!(dir_mode, 0o700, "sessions dir mode: {dir_mode:o}");

        let _ = std::fs::remove_dir_all(&root);
    }
    /// Native Windows, the PTY capability's contract THERE, end to end:
    /// a real child on a real pseudo console, its output read, its geometry
    /// set at spawn, a resize accepted while it ran, and its exit code
    /// observed. This replaces the refusal test this slice retired: what
    /// used to be "nothing here makes a `CreatePseudoConsole` call" is now the
    /// capability, and this is the run that says so.
    ///
    /// The child is `cmd /C echo`, which both this host's console and the C
    /// runtime's own command-line parser understand — the same parser the
    /// seam quotes for.
    #[cfg(windows)]
    #[test]
    fn a_pseudo_console_child_is_read_resized_typed_into_and_observed_to_exit() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let root = unique_stage("conpty-seam");
        std::env::set_var("AOIDE_STAGE_DIR", root.join("stage"));
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::set_var("AOIDE_AUDIT_LOG", root.join("log"));
        std::env::set_var("XDG_RUNTIME_DIR", &root);

        let (mut child, mut pty) = spawn_on_pty(
            "cmd",
            &["/C".to_string(), "echo seam-marker".to_string()],
            "conpty-seam",
            Some(WinSize { rows: 24, cols: 80, xpixel: 0, ypixel: 0 }),
            None,
        )
        .expect("a child starts on a pseudo console");
        assert!(child.id() > 0, "a real process, with a real pid");

        let out = read_to_exit(&mut pty, &mut child);
        let text = String::from_utf8_lossy(&out).into_owned();
        let code = ended_code(&mut child);
        // Resize while nothing is reading: `ResizePseudoConsole` on a live
        // pseudo console must be accepted (and is what the width test below
        // reads back).
        pty.resize(&WinSize { rows: 30, cols: 100, xpixel: 0, ypixel: 0 });
        pty.close();
        assert!(
            text.contains("seam-marker"),
            "the child's output was read: {text:?} (exit code {code:?})"
        );
        assert_eq!(ended_code(&mut child), Some(0), "the child exited with its own code");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A line typed into the pseudo console reaches the child's own standard
    /// input and comes back through its own echo — the shape `aoide send`
    /// depends on, pinned at the seam, with no session and no inbox in the way
    /// (the session-level run is
    /// `a_connection_to_a_live_inbox_types_into_the_conducted_child`, and the
    /// accept gate's own refusal of a true descendant has its gate-and-test on
    /// the Unix side).
    #[cfg(windows)]
    #[test]
    fn typing_into_the_pseudo_console_reaches_the_childs_own_stdin() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let root = unique_stage("conpty-type");
        std::env::set_var("AOIDE_STAGE_DIR", root.join("stage"));
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::set_var("XDG_RUNTIME_DIR", &root);

        let (mut child, mut pty) = spawn_on_pty(
            "powershell",
            &[
                "-NoProfile".to_string(),
                "-Command".to_string(),
                "$x = Read-Host; Write-Output ('GOT-' + $x)".to_string(),
            ],
            "conpty-type",
            Some(WinSize::conventional()),
            None,
        )
        .expect("a child starts on a pseudo console");

        // A single write, because the channel is a channel: the child's console
        // input buffers, and the earlier shape's "type until it lands" loop was
        // the symptom of the child's stdin being the very pipe conhost reads
        // (two readers on one pipe) — the documented shape gives it to the
        // console instead. The child is killed on the way out whatever
        // happened, so a failed run leaves no reader behind.
        pty.write(b"typed-from-the-parent\r");
        let mut out = Vec::new();
        let mut buf = [0u8; 8192];
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while std::time::Instant::now() < deadline {
            while pty.has_output() {
                match pty.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => out.extend_from_slice(&buf[..n]),
                    Err(_) => break,
                }
            }
            if String::from_utf8_lossy(&out).contains("GOT-typed-from-the-parent") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        let text = String::from_utf8_lossy(&out).into_owned();
        child.kill();
        let _ = child.wait_for(std::time::Duration::from_secs(3));
        pty.close();

        assert!(
            text.contains("GOT-typed-from-the-parent"),
            "the child read the typed line off its console: {text:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A newline-free chunk is ON stdout the moment `write_stdout` returns —
    /// the property the interactive pump depends on and the one Rust's
    /// `Stdout` (`LineWriter`) does not have: it holds bytes until the last
    /// newline in the chunk or its ~1 KiB buffer fills, which is latency a
    /// raw-mode TUI cannot afford. Measured against the seam itself, with fd 1
    /// redirected to a pipe and read back NON-BLOCKING (a buffered stdout would
    /// leave the pipe empty and read 0 rather than blocking).
    #[cfg(unix)]
    #[test]
    fn a_newline_free_chunk_reaches_stdout_without_waiting() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let payload = b"\x1b[1mno-newline-here".to_vec();
        let mut fds = [0i32; 2];
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0, "the fixture's pipe");
        let (read_fd, write_fd) = (fds[0], fds[1]);
        let saved = unsafe { libc::dup(libc::STDOUT_FILENO) };
        assert!(saved >= 0, "dup of this process's stdout");
        assert_eq!(
            unsafe { libc::dup2(write_fd, libc::STDOUT_FILENO) },
            libc::STDOUT_FILENO
        );

        crate::graph::pty::write_stdout(&payload);

        unsafe { libc::fcntl(read_fd, libc::F_SETFL, libc::O_NONBLOCK) };
        // Everything on fd 1 lands in this pipe while it is redirected — the
        // harness's own result lines from other threads included — so drain it
        // and look for the payload as one unbroken run.
        let mut got = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            let n = unsafe {
                libc::read(read_fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len())
            };
            if n <= 0 {
                break;
            }
            got.extend_from_slice(&buf[..n as usize]);
        }
        // Restore this process's stdout whatever the read said.
        unsafe {
            libc::dup2(saved, libc::STDOUT_FILENO);
            libc::close(saved);
            libc::close(read_fd);
            libc::close(write_fd);
        }
        assert!(
            got.windows(payload.len()).any(|w| w == payload.as_slice()),
            "the chunk was not on stdout yet — a buffered write would hold it here; got {got:?}"
        );
    }

    /// The end-to-end Windows test the review's finding 1 asked for: a LIVE
    /// conducted session, a connection to its own inbox, and the bytes that
    /// connection delivers ARRIVING AT THE CHILD. Everything else in this
    /// file's Windows set drives the seam directly or asserts registration
    /// alone; this is the run that proves the loop's own readiness wait can
    /// report the inbox, which is the whole point of `aoide send`.
    ///
    /// What is SEPARATE here is the SESSION, not the connector: the session is
    /// a real `aoide conduct --headless` child process, driven exactly as
    /// `aoide send` drives one, while the connection is made from this test
    /// process — the session's PARENT, which the self-injection gate lets
    /// through (it walks UP from the connector looking for the connector's own
    /// nearest registered session; a parent is not a descendant of its child,
    /// so nothing here resolves to the session being typed into).
    #[cfg(windows)]
    #[test]
    fn a_connection_to_a_live_inbox_types_into_the_conducted_child() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_SESSION_ID",
        ]);
        let root = unique_stage("conduct-inbox-e2e");
        let stage = root.join("stage");
        let state = root.join("state");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("AOIDE_STATE_DIR", &state);
        std::env::set_var("AOIDE_AUDIT_LOG", root.join("log"));
        std::env::set_var("XDG_RUNTIME_DIR", &root);

        let id = "conduct-inbox-e2e";
        let socket = conduct_socket_path(id);
        let mut conduct = std::process::Command::new(built_aoide_bin())
            .arg("conduct")
            .arg("--headless")
            .arg("--id")
            .arg(id)
            .arg("--")
            .arg("powershell")
            .arg("-NoProfile")
            .arg("-Command")
            .arg("$x = Read-Host; Write-Output ('GOT-' + $x)")
            .env("AOIDE_STAGE_DIR", &stage)
            .env("AOIDE_STATE_DIR", &state)
            .env("AOIDE_AUDIT_LOG", root.join("log"))
            .env("XDG_RUNTIME_DIR", &root)
            .env_remove("AOIDE_SESSION_ID")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("aoide conduct starts");

        // Budgets with room in them: this run is the ONLY one that proves the
        // channel end to end, so a loaded box must not be able to make it
        // flake — the child then has 60 s to answer, and a failure kills it.
        let door_deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while !socket.exists() && std::time::Instant::now() < door_deadline {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(
            socket.exists(),
            "the session's injection socket was never bound: {}",
            socket.display()
        );

        use std::io::Write as _;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let mut exited = false;
        let mut delivered = false;
        while std::time::Instant::now() < deadline {
            // ONE line, once the door exists: the child's console input buffers
            // a line it is not reading yet, so there is nothing to retry — and
            // a second connection would be a second line.
            if !delivered {
                if let Ok(mut door) = aoide_protocol::win_unix::UnixStream::connect(&socket) {
                    let _ = door.write_all(b"INBOX-hello\r\n");
                    delivered = true;
                }
            }
            if conduct.try_wait().ok().flatten().is_some() {
                exited = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        if !exited {
            let _ = conduct.kill();
            let _ = conduct.wait();
        }
        assert!(exited, "the conducted child never answered the injected line");

        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec = s.sessions.iter().find(|r| r.session_id == id).expect("registered");
        let log = std::fs::read_to_string(rec.log_path.clone().expect("logPath stamped"))
            .unwrap_or_default();
        assert!(
            log.contains("GOT-INBOX-hello"),
            "the injected bytes reached the child's own stdin: {log:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// N2 of the re-review, and the shape `ssh` without a tty gives
    /// (`producer | aoide conduct`): an INTERACTIVE (not `--headless`) conduct
    /// whose own stdin is a PIPE must still be a channel. `GetConsoleMode` says
    /// it is not a console, so it is not offered to `WaitForMultipleObjects`
    /// (a pipe's read handle is ALWAYS signalled and would win the array
    /// forever), and it is POLLED and read without blocking instead — the pipe
    /// is asked how many bytes it has and exactly those are read, so an empty
    /// one answers "nothing yet" rather than parking the multiplex in a
    /// blocking `ReadFile` (measured, before the polling: the log held nothing
    /// past conhost's own header, the child never answered, and the process
    /// never returned). This run drives that shape end to end: the child's
    /// output is mirrored, an INJECTED line reaches its own stdin, the child
    /// answers, and the session resolves.
    ///
    /// The line is typed a bounded number of times, as an operator types until
    /// the child takes it; a single connection can land before `powershell` has
    /// begun reading.
    #[cfg(windows)]
    #[test]
    fn an_interactive_conduct_with_a_piped_stdin_still_types_into_its_child() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_SESSION_ID",
        ]);
        let root = unique_stage("conduct-piped-stdin");
        let stage = root.join("stage");
        let state = root.join("state");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("AOIDE_STATE_DIR", &state);
        std::env::set_var("AOIDE_AUDIT_LOG", root.join("log"));
        std::env::set_var("XDG_RUNTIME_DIR", &root);

        let id = "conduct-piped-stdin";
        let socket = conduct_socket_path(id);
        let mut conduct = std::process::Command::new(built_aoide_bin())
            .arg("conduct")
            .arg("--id")
            .arg(id)
            .arg("--")
            .arg("powershell")
            .arg("-NoProfile")
            .arg("-Command")
            .arg("$x = Read-Host; Write-Output ('GOT-' + $x)")
            .env("AOIDE_STAGE_DIR", &stage)
            .env("AOIDE_STATE_DIR", &state)
            .env("AOIDE_AUDIT_LOG", root.join("log"))
            .env("XDG_RUNTIME_DIR", &root)
            .env_remove("AOIDE_SESSION_ID")
            .stdin(std::process::Stdio::piped()) // the pipe this test is about
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("aoide conduct starts");

        let door_deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while !socket.exists() && std::time::Instant::now() < door_deadline {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(socket.exists(), "the session's injection socket was never bound");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let mut exited = false;
        let mut attempted = 0;
        while std::time::Instant::now() < deadline {
            if attempted < 6 {
                use std::io::Write as _;
                if let Ok(mut door) = aoide_protocol::win_unix::UnixStream::connect(&socket) {
                    let _ = door.write_all(b"piped-line\r\n");
                    attempted += 1;
                }
            }
            if conduct.try_wait().ok().flatten().is_some() {
                exited = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec = s.sessions.iter().find(|r| r.session_id == id);
        let conductable = rec.and_then(|r| r.conductable);
        let log = rec
            .and_then(|r| r.log_path.clone())
            .map(|p| std::fs::read_to_string(p).unwrap_or_default())
            .unwrap_or_default();
        if !exited {
            let _ = conduct.kill();
            let _ = conduct.wait();
        }

        assert_eq!(conductable, Some(true), "the channel was bound");
        assert!(
            log.contains("GOT-piped-line"),
            "a non-headless conduct with a piped stdin types the injected line into its child \
             (the parked loop typed nothing and never returned): {log:?}"
        );
        assert_eq!(rec.map(|r| r.state.clone()).as_deref(), Some("done"));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The other half of the same contract, and the half the re-review found
    /// missing: a PIPED STDIN's bytes reach the child. Unix gets this from
    /// `poll` (an empty pipe is not `POLLIN`, so it is bridged without ever
    /// blocking); Windows now polls the pipe the same way it polls the output
    /// pipe — `PeekNamedPipe` for the count, `ReadFile` for exactly that many —
    /// and treats the writer's close as the Unix arm treats a 0-byte read. This
    /// is `producer | aoide conduct`, whose bytes used to be dropped here.
    ///
    /// The test holds the producer's write end itself, writes one line, and
    /// closes it (the EOF the loop must survive), then requires the child to
    /// answer with that line and the session to resolve.
    #[cfg(windows)]
    #[test]
    fn a_piped_stdin_forwards_the_producers_bytes_to_the_child() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_SESSION_ID",
        ]);
        let root = unique_stage("conduct-piped-producer");
        let stage = root.join("stage");
        let state = root.join("state");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("AOIDE_STATE_DIR", &state);
        std::env::set_var("AOIDE_AUDIT_LOG", root.join("log"));
        std::env::set_var("XDG_RUNTIME_DIR", &root);

        let id = "conduct-piped-producer";
        let mut conduct = std::process::Command::new(built_aoide_bin())
            .arg("conduct")
            .arg("--id")
            .arg(id)
            .arg("--")
            .arg("powershell")
            .arg("-NoProfile")
            .arg("-Command")
            .arg("$x = Read-Host; Write-Output ('FROM-STDIN-' + $x)")
            .env("AOIDE_STAGE_DIR", &stage)
            .env("AOIDE_STATE_DIR", &state)
            .env("AOIDE_AUDIT_LOG", root.join("log"))
            .env("XDG_RUNTIME_DIR", &root)
            .env_remove("AOIDE_SESSION_ID")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("aoide conduct starts");

        // The producer's write waits for the CLIENT to exist: conhost drops
        // input written into a pseudo console before a client is attached
        // (measured — the same reason the injected-line run above types a
        // bounded number of times), and the client's own title sequence in the
        // log is the moment it is there. After that, one write and one EOF,
        // which is what a real `producer | aoide conduct` does.
        let attach_deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let reader = |rec_path: &Option<String>| -> String {
            rec_path
                .as_ref()
                .map(|p| std::fs::read_to_string(p).unwrap_or_default())
                .unwrap_or_default()
        };
        let log_path = load_stage::<SessionsFile>(&sessions_path())
            .ok()
            .and_then(|s| {
                s.sessions
                    .iter()
                    .find(|r| r.session_id == id)
                    .and_then(|r| r.log_path.clone())
            });
        while std::time::Instant::now() < attach_deadline {
            if reader(&log_path).contains("powershell.exe") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        {
            use std::io::Write as _;
            let mut sink = conduct.stdin.take().expect("the producer's write end");
            sink.write_all(b"producer-line\r\n").expect("the producer writes");
            sink.flush().expect("the producer flushes");
        } // dropping the write end is the producer's EOF.

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let mut exited = false;
        while std::time::Instant::now() < deadline {
            if conduct.try_wait().ok().flatten().is_some() {
                exited = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec = s.sessions.iter().find(|r| r.session_id == id);
        let log = rec
            .and_then(|r| r.log_path.clone())
            .map(|p| std::fs::read_to_string(p).unwrap_or_default())
            .unwrap_or_default();
        if !exited {
            let _ = conduct.kill();
            let _ = conduct.wait();
        }
        assert!(exited, "the session resolved; log={log:?}");
        assert!(
            log.contains("FROM-STDIN-producer-line"),
            "the producer's bytes reached the child's own stdin: {log:?}"
        );
        assert_eq!(rec.map(|r| r.state.clone()).as_deref(), Some("done"));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// `aoide conduct` itself, on Windows, headless: the whole command — spawn,
    /// register, mirror the child's output into its per-session log, resolve
    /// the record `done` with the child's real exit code. This is the gate the
    /// old refusal test cannot be: the capability is here, so the command runs.
    #[cfg(windows)]
    #[test]
    fn conduct_runs_a_child_on_a_pseudo_console_and_resolves_its_session() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
        ]);

        let root = unique_stage("conduct-conpty");
        let stage = root.join("stage");
        let state = root.join("state");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("AOIDE_STATE_DIR", &state);
        std::env::set_var("AOIDE_AUDIT_LOG", root.join("log"));
        std::env::set_var("XDG_RUNTIME_DIR", &root);

        let id = "conduct-conpty";
        let socket = conduct_socket_path(id);
        let out = session_conduct(&conduct_invocation(
            &["cmd", "/C", "echo conduct-on-a-pseudo-console"],
            &[("id", id), ("headless", "true")],
        ));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let data = out.data.as_ref().expect("an outcome body");
        assert_eq!(data["exitCode"], 0, "the child's own code: {data}");
        assert_eq!(data["conductable"], true, "its injection socket was bound: {data}");

        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec = s.sessions.iter().find(|r| r.session_id == id).expect("registered");
        assert_eq!(rec.state, "done");
        assert!(
            data.get("ptyHeldAfterExit").is_none(),
            "a host that cannot answer whether a descendant holds the console must not \
             stamp the fact: {data}"
        );
        let log = std::fs::read_to_string(rec.log_path.clone().expect("logPath stamped"))
            .unwrap_or_default();
        assert!(
            log.contains("conduct-on-a-pseudo-console"),
            "the child's output was read off the pseudo console and mirrored: {log:?}"
        );
        assert!(!socket.exists(), "the control socket is unlinked on exit");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A resize is not merely accepted: the CHILD's own console reports the
    /// geometry the parent set. `mode con` asks the console device
    /// (`CONOUT$`) rather than this process's stream handles, so its
    /// `Columns:` line is the pseudo console's answer, read after a resize
    /// that happened while the child was running.
    #[cfg(windows)]
    #[test]
    fn a_resize_reaches_the_childs_own_console() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let root = unique_stage("conpty-resize");
        std::env::set_var("AOIDE_STAGE_DIR", root.join("stage"));
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::set_var("XDG_RUNTIME_DIR", &root);

        let (mut child, mut pty) = spawn_on_pty(
            "cmd",
            &[
                "/C".to_string(),
                "ping -n 2 127.0.0.1 >nul & mode con".to_string(),
            ],
            "conpty-resize",
            Some(WinSize { rows: 24, cols: 80, xpixel: 0, ypixel: 0 }),
            None,
        )
        .expect("a child starts on a pseudo console");

        std::thread::sleep(std::time::Duration::from_millis(300));
        pty.resize(&WinSize { rows: 30, cols: 100, xpixel: 0, ypixel: 0 });

        let out = read_to_exit(&mut pty, &mut child);
        let text = String::from_utf8_lossy(&out).into_owned();
        pty.close();

        assert!(
            text.contains("Columns:") && text.contains("100"),
            "the child's own console reports the resized geometry: {text:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The task #100 defect, end to end: `spawn --agent soak-a -- bash` (the
    /// P-C7 soak's live shape) conducts a REAL shell under a caller-chosen
    /// agent label that is not the literal string `"shell"`. Under the old
    /// `agent == "shell"` gate this session's roster record never ticked at
    /// all — `restore` stayed `None` forever, so a later `resurrect` had
    /// nothing beyond a default cwd. The gate now reads the wrapped
    /// command's own basename, so this session gets captured regardless of
    /// what it is labelled.
    /// GATED on Unix with its reason: the PTY-backed conduct channel, refused
    /// by name on native Windows (see `conduct_mirrors_a_nonzero_child_exit`).
    #[cfg(unix)]
    #[test]
    fn a_shell_conducted_under_a_non_shell_agent_label_still_gets_captured() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR"]);

        let root = unique_stage("conduct-non-shell-label");
        let stage = root.join("stage");
        let state = root.join("state");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("AOIDE_STATE_DIR", &state);
        std::env::set_var("XDG_RUNTIME_DIR", &root);

        let out = session_conduct(&conduct_invocation(
            &["sh", "-c", "sleep 1"],
            &[("id", "conduct-non-shell-label"), ("agent", "soak-a")],
        ));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);

        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec = s.sessions.iter().find(|r| r.session_id == "conduct-non-shell-label").unwrap();
        assert_eq!(rec.agent, "soak-a", "the display label stays whatever the caller chose");
        assert!(
            rec.restore.is_some(),
            "a real shell must be captured regardless of its agent label — restore was never populated"
        );

        let _ = std::fs::remove_dir_all(&root);
    }
    /// M1/the durable field, end to end: REGISTRATION itself answers "is the
    /// wrapped command a shell", launcher and all, without waiting for a tick
    /// and without reading `agent`. `env sh -c …` is the launcher shape the
    /// 4-name basename read used to miss entirely.
    /// GATED on Unix with its reason: the PTY-backed conduct channel, refused
    /// by name on native Windows (see `conduct_mirrors_a_nonzero_child_exit`).
    #[cfg(unix)]
    #[test]
    fn registration_stamps_shell_from_the_argv_it_conducts() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR"]);

        let root = unique_stage("conduct-shell-stamp");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::set_var("XDG_RUNTIME_DIR", &root);

        for (id, argv, expected) in [
            ("stamp-env-shell", vec!["env", "sh", "-c", "sleep 1"], true),
            ("stamp-harness", vec!["cat", "/dev/null"], false),
        ] {
            let out = session_conduct(&conduct_invocation(
                &argv,
                &[("id", id), ("agent", "pi")],
            ));
            assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
            let s: SessionsFile = load_stage(&sessions_path()).unwrap();
            let rec = s.sessions.iter().find(|r| r.session_id == id).unwrap();
            assert_eq!(
                rec.shell, expected,
                "`{}` conducted as {argv:?} must stamp shell={expected}",
                rec.agent
            );
        }

        let _ = std::fs::remove_dir_all(&root);
    }
    /// The field is NOT once-only like `headless`/`spawned`: an id re-registers
    /// under its CURRENT argv, so `--env sh` sets it and a later `--cat` on the
    /// same id CLEARS it. The other direction leaves a record claiming a shell
    /// forever, which is a silently dead lane rather than a hole — but it is
    /// still wrong, and this is the test that says so.
    /// GATED on Unix with its reason: the PTY-backed conduct channel, refused
    /// by name on native Windows (see `conduct_mirrors_a_nonzero_child_exit`).
    #[cfg(unix)]
    #[test]
    fn re_registering_an_id_flips_the_shell_stamp_both_ways() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR"]);

        let root = unique_stage("conduct-shell-restamp");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::set_var("XDG_RUNTIME_DIR", &root);

        let id = "re-registered";
        let out = session_conduct(&conduct_invocation(
            &["env", "sh", "-c", "sleep 1"],
            &[("id", id), ("agent", "pi")],
        ));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert!(s.sessions.iter().find(|r| r.session_id == id).unwrap().shell);

        let out = session_conduct(&conduct_invocation(&["cat", "/dev/null"], &[("id", id), ("agent", "pi")]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert!(
            !s.sessions.iter().find(|r| r.session_id == id).unwrap().shell,
            "re-registering the same id as a non-shell must CLEAR the stamp"
        );

        let _ = std::fs::remove_dir_all(&root);
    }
    /// GATED on Unix with its reason: the PTY-backed conduct channel, refused
    /// by name on native Windows (see `conduct_mirrors_a_nonzero_child_exit`).
    #[cfg(unix)]
    #[test]
    fn conduct_headless_mirrors_pty_output_to_the_log_and_stamps_log_path() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR"]);

        let root = unique_stage("conduct-headless");
        let stage = root.join("stage");
        let state = root.join("state");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("AOIDE_STATE_DIR", &state);
        std::env::set_var("XDG_RUNTIME_DIR", &root);

        let out = session_conduct(&conduct_invocation(
            &["sh", "-c", "echo mark-headless"],
            &[("id", "conduct-headless"), ("headless", "true")],
        ));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        assert_eq!(out.data.as_ref().unwrap()["exitCode"], 0);

        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec = s.sessions.iter().find(|r| r.session_id == "conduct-headless").unwrap();
        assert_eq!(rec.state, "done");
        let log_path = rec.log_path.clone().expect("headless conduct stamps logPath");
        assert!(log_path.ends_with("conduct-headless.log"));
        let logged = std::fs::read_to_string(&log_path).unwrap();
        assert!(logged.contains("mark-headless"), "log contents: {logged:?}");

        let _ = std::fs::remove_dir_all(&root);
    }
    /// Review round 2 of task #89: `--headless` registration must stamp the
    /// PERMANENT `headless` marker AND never call window discovery at all.
    /// The discovery GATE itself (`if headless { None } else {
    /// discover_window_address() }`) can't be distinguished from "discovery
    /// ran and simply found nothing" in this test environment (no
    /// `HYPRLAND_INSTANCE_SIGNATURE` — `discover_window_address()` would
    /// return `None` either way, gated or not), so this pins the one thing
    /// that IS honestly observable without a live compositor: the stamped
    /// `headless` flag, which is what makes the wrap's windowlessness
    /// permanent regardless of what any discovery path does or doesn't find.
    /// GATED on Unix with its reason: the PTY-backed conduct channel, refused
    /// by name on native Windows (see `conduct_mirrors_a_nonzero_child_exit`).
    #[cfg(unix)]
    #[test]
    fn headless_conduct_registration_stamps_the_permanent_headless_marker() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR"]);

        let root = unique_stage("conduct-headless-marker");
        let stage = root.join("stage");
        let state = root.join("state");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("AOIDE_STATE_DIR", &state);
        std::env::set_var("XDG_RUNTIME_DIR", &root);

        let out = session_conduct(&conduct_invocation(
            &["sh", "-c", "sleep 1"],
            &[("id", "conduct-headless-marker"), ("headless", "true")],
        ));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);

        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec = s
            .sessions
            .iter()
            .find(|r| r.session_id == "conduct-headless-marker")
            .unwrap();
        assert!(rec.headless, "a --headless registration must stamp headless=true");
        assert_eq!(
            rec.window_address, "",
            "off-Hyprland (no HYPRLAND_INSTANCE_SIGNATURE) this holds even ungated — the marker \
             is the permanent, gate-independent signal windowless_by_lineage actually keys off"
        );

        // An INTERACTIVE (non-headless) registration never stamps the marker.
        let out2 = session_conduct(&conduct_invocation(
            &["sh", "-c", "true"],
            &[("id", "conduct-interactive-marker")],
        ));
        assert_eq!(out2.status, aoide_protocol::output::Status::Ok, "msg: {}", out2.message);
        let s2: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec2 = s2
            .sessions
            .iter()
            .find(|r| r.session_id == "conduct-interactive-marker")
            .unwrap();
        assert!(!rec2.headless, "an interactive registration must never stamp headless=true");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// GATED on Unix with its reason: this asserts `session_conduct`'s
    /// node-origin REFUSAL, which is reached only after the command can run at
    /// all — on native Windows the whole command is refused earlier, by name,
    /// for want of a controlling tty (see `conduct_mirrors_a_nonzero_child_exit`).
    #[cfg(unix)]
    #[test]
    fn conduct_registration_refuses_a_node_origin_from_the_env_var() {
        // LANE IDENTITY P-ID0 (G16/G5): `AOIDE_SESSION_ORIGIN` is inherited
        // process env — a same-uid process can set it on ITSELF before
        // invoking `aoide conduct` directly, so a `node:*` shape read here
        // must never be trusted. `session_conduct` now refuses exactly this
        // shape rather than stamping it; a genuine node origin is stamped
        // by `aoide-server::a2a::do_spawn` calling `stamp_origin` directly
        // on the record (proven in that crate's own test, which this crate
        // cannot see).
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR", "AOIDE_SESSION_ORIGIN"]);

        let root = unique_stage("conduct-origin-marker");
        let stage = root.join("stage");
        let state = root.join("state");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("AOIDE_STATE_DIR", &state);
        std::env::set_var("XDG_RUNTIME_DIR", &root);

        std::env::set_var("AOIDE_SESSION_ORIGIN", "node:yomi-strix");
        let out = session_conduct(&conduct_invocation(
            &["sh", "-c", "true"],
            &[("id", "conduct-origin-node")],
        ));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec = s.sessions.iter().find(|r| r.session_id == "conduct-origin-node").unwrap();
        assert_eq!(
            rec.origin, None,
            "a node:* shape read off inherited env must be refused, never stamped"
        );

        // A non-node env value is local-class and still stamps — the
        // refusal is specific to the `node:` shape, not to the env read
        // entirely.
        std::env::set_var("AOIDE_SESSION_ORIGIN", "local");
        let out_local = session_conduct(&conduct_invocation(
            &["sh", "-c", "true"],
            &[("id", "conduct-origin-local-class")],
        ));
        assert_eq!(out_local.status, aoide_protocol::output::Status::Ok, "msg: {}", out_local.message);
        let s_local: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec_local =
            s_local.sessions.iter().find(|r| r.session_id == "conduct-origin-local-class").unwrap();
        assert_eq!(rec_local.origin.as_deref(), Some("local"));

        // No env var set at all — a plain local registration never gets one.
        std::env::remove_var("AOIDE_SESSION_ORIGIN");
        let out2 = session_conduct(&conduct_invocation(
            &["sh", "-c", "true"],
            &[("id", "conduct-origin-local")],
        ));
        assert_eq!(out2.status, aoide_protocol::output::Status::Ok, "msg: {}", out2.message);
        let s2: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec2 = s2.sessions.iter().find(|r| r.session_id == "conduct-origin-local").unwrap();
        assert_eq!(rec2.origin, None, "a locally-launched conduct never stamps an origin");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn session_refresh_drives_shell_cwd_command_and_state() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("refresh");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        let now = "2026-01-01T00:00:00Z";
        let mut sessions = Vec::new();
        upsert_session(
            &mut sessions, "sh", Some("shell"), Some("/w"), None, None, None, None, None,
            Some(std::process::id()), now,
        );
        write_stage(
            &sessions_path(),
            &SessionsFile { schema_version: "0".into(), sessions },
        )
        .unwrap();

        let idle_restore = RestoreSnapshot { cwd: Some("/proj".into()), idle: true, argv: None, typed: None };
        let working_restore = RestoreSnapshot {
            cwd: Some("/proj".into()),
            idle: false,
            argv: Some(vec!["cargo".into(), "test".into()]),
            typed: None,
        };

        // At the prompt: idle, no activity, cwd tracked.
        do_session_refresh("sh", Some("/proj"), None, "idle", false, idle_restore.clone());
        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert_eq!(s.sessions[0].state, "idle");
        assert_eq!(s.sessions[0].cwd, "/proj");
        assert_eq!(s.sessions[0].activity, None);
        assert_eq!(s.sessions[0].needs_sudo, None);
        assert_eq!(s.sessions[0].restore, Some(idle_restore.clone()));

        // A foreground command: working + the command as activity, and the
        // restore snapshot's raw argv persists change-only alongside it.
        do_session_refresh(
            "sh", Some("/proj"), Some("cargo test"), "working", false, working_restore.clone(),
        );
        let s2: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert_eq!(s2.sessions[0].state, "working");
        assert_eq!(s2.sessions[0].activity.as_deref(), Some("cargo test"));
        assert_eq!(s2.sessions[0].needs_sudo, None);
        assert_eq!(s2.sessions[0].restore, Some(working_restore));

        // Blocked on sudo: state=awaiting and needsSudo=true, regardless of the
        // `state` string passed in (the caller already resolves the force in
        // `conduct_refresh_shell`, but do_session_refresh itself just persists
        // both fields change-only).
        do_session_refresh("sh", Some("/proj"), Some("sudo"), "awaiting", true, idle_restore.clone());
        let s3: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert_eq!(s3.sessions[0].state, "awaiting");
        assert_eq!(s3.sessions[0].needs_sudo, Some(true));

        // The prompt clears: needs_sudo=false CLEARS the field back to None
        // (never left as Some(false)) — change-only, so the key disappears.
        do_session_refresh("sh", Some("/proj"), None, "idle", false, idle_restore.clone());
        let s4: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert_eq!(s4.sessions[0].needs_sudo, None);
        let raw = std::fs::read_to_string(sessions_path()).unwrap();
        assert!(!raw.contains("needsSudo"), "cleared key must be absent: {raw}");

        // An unknown id is a safe no-op (never panics, never inserts).
        do_session_refresh("nope", Some("/x"), Some("x"), "working", false, idle_restore);
        let s5: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert_eq!(s5.sessions.len(), 1);

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }
}
