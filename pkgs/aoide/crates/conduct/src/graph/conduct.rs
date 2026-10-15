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
    canonical_state, clear_screen_ask, hooks_path, load_stage, sessions_path, write_stage,
    HookDeath, HooksFile, RestoreSnapshot, SessionRecord, SessionsFile, STAGE_GRAPH_VERSION,
};
use super::prompt::{Prompt, PromptWatch};
use super::pty::{die_by_signal, spawn_on_pty, take_wrap_signal, wait_ready, Console, Ended, Inbox, Pty, PtyChild, WinSize, WrapSignals};
#[cfg(unix)]
use super::pty::signal_name;
use super::session_store::{
    do_session_end, do_session_start, record_hook_deaths, reserved_id_refusal, set_session_log_path,
    stamp_headless, stamp_origin, stamp_attested_spawner, stamp_session_exit, stamp_spawned, stamp_task,
    stamp_wrap,
};
use super::window::{discover_window_address, spawner_is_attested, resolve_registration_parent};
use aoide_protocol::agents::agent_profile;
use aoide_protocol::Invocation;
use aoide_protocol::output::{Fix, Outcome};
use aoide_storage::attest::is_node_origin;
use aoide_storage::fs::with_stage_lock;
use aoide_storage::time::now_iso_utc;
use serde_json::json;
use std::path::PathBuf;

/// The command's basename (the agent-name default), e.g. `/usr/bin/claude` →
/// `claude`. A login shell's spelling carries a leading `-` (`-bash`, `-zsh`):
/// it names the same shell, so that `-` is dropped. Only a shell name drops it:
/// `-claude` names no harness, and stays `-claude`, so it never passes the
/// ping-back's proof for `claude`.
fn command_basename(program: &str) -> String {
    let base = std::path::Path::new(program)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| program.to_string());
    base.strip_prefix('-')
        .filter(|shell| SHELL_BASENAMES.contains(shell))
        .map(str::to_string)
        .unwrap_or(base)
}

/// The shell basenames [`program_is_a_shell`] resolves to — the ONE list, in
/// the one place shell-likeness is decided: the P-C5 capture path
/// (`is_shell`, the refresh tick, the restore snapshot), the auto-typing
/// refusals (ping-back, the doorbell's PTY arm, the A2A
/// door) and `spawn`'s own gates all read THIS verdict, never a private copy
/// of the names.
const SHELL_BASENAMES: &[&str] = &[
    "bash", "zsh", "fish", "sh", "dash", "ksh", "mksh", "pdksh", "csh", "tcsh", "nu", "xonsh",
    "busybox", "elvish", "osh", "ash", "yash",
];

/// The programs that take a privilege or a login in place of the operator: the
/// one list. `sudo` and `doas` also wrap a command, which the walk reads as a
/// shell whatever it runs ([`walk`]); the rest ask for a password or a root
/// shell on the pty. [`runs_a_privilege_program`] reads it for the restore
/// refusals, for a wrap that is never idle ([`ShellWatch::privileged`]) and,
/// through [`program_is_an_interactive_shell`], for the screen. A basename
/// match, and `busybox <applet>` counts through its applet. Only `sudo`, `doas`
/// and `su` read as a shell, so only they reach the never-idle rule; the rest
/// are non-shell wraps with no restore.
pub(in crate::graph) const PRIVILEGE_PROGRAMS: &[&str] = &[
    "sudo", "doas", "su", "pkexec", "run0", "sudoedit", "runuser", "setpriv", "chroot", "sg",
    "newgrp", "dzdo", "ksu",
];

/// Launchers that only FORWARD to the program after them. The hook door's plumbing
/// verdict ([`is_invocation_plumbing`]) reads this list; the walk reads each
/// launcher's own options from [`launcher`].
const FORWARDING_LAUNCHERS: &[&str] = &[
    "env", "exec", "setsid", "nohup", "stdbuf", "chrt", "ionice", "nice",
];

/// How a launcher option takes its value.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Arity {
    /// No value: `-i`, `--ignore-environment`.
    Flag,
    /// A value, attached (`-n5`, `--adjustment=5`) or as the next token (`-n 5`).
    Value,
    /// A value only when attached (`--ignore-signal=PIPE`), never the next token.
    Attached,
    /// An option that names no program the walk can resolve: a command line
    /// (`env -S`), help, version, or a mode that runs nothing (`ionice -p`). Its
    /// presence refuses the whole launcher.
    Refuse,
}

/// One launcher's own options, as its `--help` lists them: coreutils 9.11 (`env`,
/// `nice`, `stdbuf`, `timeout`, `nohup`), util-linux 2.42 (`chrt`, `ionice`,
/// `setsid`), sudo 1.9.17, bash's `exec` builtin (`help exec`), and OpenBSD's
/// `doas(1)`, whose row is unverified here because doas is not installed.
///
/// DEFAULT-DENY: an option not listed is unknown, and the walk refuses the whole
/// launcher rather than guess how many tokens it takes. A wrong arity reads a value
/// as the program, the one error that could prove a harness the launch does not run,
/// so nothing is inferred from a missing row. Long names match as `getopt_long`
/// matches them: exact, else one unique prefix. Every real long option is listed, so
/// an ambiguous prefix is ambiguous here too.
struct Launcher {
    short: &'static [(char, Arity)],
    long: &'static [(&'static str, Arity)],
    /// Operands before the program: `timeout`'s duration, `chrt`'s priority.
    operands: usize,
    /// `NAME=value` operands before the program, which `env` and `sudo` accept.
    assignments: bool,
    /// A bare `-N` is an option: `nice -5` is the adjustment.
    numeric: bool,
}

static ENV: Launcher = Launcher {
    short: &[
        ('0', Arity::Flag), ('a', Arity::Value), ('C', Arity::Value), ('i', Arity::Flag),
        ('u', Arity::Value), ('v', Arity::Flag), ('S', Arity::Refuse),
    ],
    long: &[
        ("argv0", Arity::Value), ("block-signal", Arity::Attached), ("chdir", Arity::Value),
        ("debug", Arity::Flag), ("default-signal", Arity::Attached), ("help", Arity::Refuse),
        ("ignore-environment", Arity::Flag), ("ignore-signal", Arity::Attached),
        ("list-signal-handling", Arity::Flag), ("null", Arity::Flag),
        ("split-string", Arity::Refuse), ("unset", Arity::Value), ("version", Arity::Refuse),
    ],
    operands: 0,
    assignments: true,
    numeric: false,
};

static EXEC: Launcher = Launcher {
    short: &[('a', Arity::Value), ('c', Arity::Flag), ('l', Arity::Flag)],
    long: &[],
    operands: 0,
    assignments: false,
    numeric: false,
};

static NOHUP: Launcher = Launcher {
    short: &[],
    long: &[("help", Arity::Refuse), ("version", Arity::Refuse)],
    operands: 0,
    assignments: false,
    numeric: false,
};

static SETSID: Launcher = Launcher {
    short: &[
        ('c', Arity::Flag), ('f', Arity::Flag), ('h', Arity::Refuse), ('V', Arity::Refuse),
        ('w', Arity::Flag),
    ],
    long: &[
        ("ctty", Arity::Flag), ("fork", Arity::Flag), ("help", Arity::Refuse),
        ("version", Arity::Refuse), ("wait", Arity::Flag),
    ],
    operands: 0,
    assignments: false,
    numeric: false,
};

static STDBUF: Launcher = Launcher {
    short: &[
        ('e', Arity::Value), ('h', Arity::Refuse), ('i', Arity::Value), ('o', Arity::Value),
        ('V', Arity::Refuse),
    ],
    long: &[
        ("error", Arity::Value), ("help", Arity::Refuse), ("input", Arity::Value),
        ("output", Arity::Value), ("version", Arity::Refuse),
    ],
    operands: 0,
    assignments: false,
    numeric: false,
};

static NICE: Launcher = Launcher {
    short: &[('n', Arity::Value)],
    long: &[
        ("adjustment", Arity::Value), ("help", Arity::Refuse), ("version", Arity::Refuse),
    ],
    operands: 0,
    assignments: false,
    numeric: true,
};

static CHRT: Launcher = Launcher {
    short: &[
        ('a', Arity::Refuse), ('b', Arity::Flag), ('d', Arity::Flag), ('D', Arity::Value),
        ('e', Arity::Flag), ('f', Arity::Flag), ('h', Arity::Refuse), ('i', Arity::Flag),
        ('m', Arity::Refuse), ('o', Arity::Flag), ('p', Arity::Refuse), ('P', Arity::Value),
        ('r', Arity::Flag), ('R', Arity::Flag), ('T', Arity::Value), ('v', Arity::Refuse),
        ('V', Arity::Refuse),
    ],
    long: &[
        ("all-tasks", Arity::Refuse), ("batch", Arity::Flag), ("deadline", Arity::Flag),
        ("ext", Arity::Flag), ("fifo", Arity::Flag), ("help", Arity::Refuse),
        ("idle", Arity::Flag), ("max", Arity::Refuse), ("other", Arity::Flag),
        ("pid", Arity::Refuse), ("reset-on-fork", Arity::Flag), ("rr", Arity::Flag),
        ("sched-deadline", Arity::Value), ("sched-period", Arity::Value),
        ("sched-runtime", Arity::Value), ("verbose", Arity::Refuse), ("version", Arity::Refuse),
    ],
    operands: 1,
    assignments: false,
    numeric: false,
};

static IONICE: Launcher = Launcher {
    short: &[
        ('c', Arity::Value), ('h', Arity::Refuse), ('n', Arity::Value), ('p', Arity::Refuse),
        ('P', Arity::Refuse), ('t', Arity::Flag), ('u', Arity::Refuse), ('V', Arity::Refuse),
    ],
    long: &[
        ("class", Arity::Value), ("classdata", Arity::Value), ("help", Arity::Refuse),
        ("ignore", Arity::Flag), ("pgid", Arity::Refuse), ("pid", Arity::Refuse),
        ("uid", Arity::Refuse), ("version", Arity::Refuse),
    ],
    operands: 0,
    assignments: false,
    numeric: false,
};

static TIMEOUT: Launcher = Launcher {
    short: &[
        ('f', Arity::Flag), ('k', Arity::Value), ('p', Arity::Flag), ('s', Arity::Value),
        ('v', Arity::Flag),
    ],
    long: &[
        ("foreground", Arity::Flag), ("help", Arity::Refuse), ("kill-after", Arity::Value),
        ("preserve-status", Arity::Flag), ("signal", Arity::Value), ("verbose", Arity::Flag),
        ("version", Arity::Refuse),
    ],
    operands: 1,
    assignments: false,
    numeric: false,
};

/// `-i`/`-s` and `-e`/`-l`/`-v` land in a shell or run a mode, so they refuse: a
/// refused launch is heard by the screen and never proven.
static SUDO: Launcher = Launcher {
    short: &[
        ('A', Arity::Flag), ('b', Arity::Flag), ('B', Arity::Flag), ('C', Arity::Value),
        ('D', Arity::Value), ('e', Arity::Refuse), ('E', Arity::Flag), ('g', Arity::Value),
        ('h', Arity::Refuse), ('H', Arity::Flag), ('i', Arity::Refuse), ('K', Arity::Flag),
        ('k', Arity::Flag), ('l', Arity::Refuse), ('n', Arity::Flag), ('P', Arity::Flag),
        ('p', Arity::Value), ('R', Arity::Value), ('s', Arity::Refuse), ('S', Arity::Flag),
        ('T', Arity::Value), ('U', Arity::Value), ('u', Arity::Value), ('v', Arity::Refuse),
        ('V', Arity::Refuse),
    ],
    long: &[
        ("askpass", Arity::Flag), ("background", Arity::Flag), ("bell", Arity::Flag),
        ("chdir", Arity::Value), ("chroot", Arity::Value), ("close-from", Arity::Value),
        ("command-timeout", Arity::Value), ("edit", Arity::Refuse), ("group", Arity::Value),
        ("help", Arity::Refuse), ("host", Arity::Refuse), ("list", Arity::Refuse),
        ("login", Arity::Refuse), ("non-interactive", Arity::Flag),
        ("other-user", Arity::Value), ("preserve-env", Arity::Attached),
        ("preserve-groups", Arity::Flag), ("prompt", Arity::Value),
        ("remove-timestamp", Arity::Flag), ("reset-timestamp", Arity::Flag),
        ("set-home", Arity::Flag), ("shell", Arity::Refuse), ("stdin", Arity::Flag),
        ("user", Arity::Value), ("validate", Arity::Refuse), ("version", Arity::Refuse),
    ],
    operands: 0,
    assignments: true,
    numeric: false,
};

static DOAS: Launcher = Launcher {
    short: &[
        ('C', Arity::Value), ('L', Arity::Refuse), ('n', Arity::Flag), ('s', Arity::Refuse),
        ('u', Arity::Value),
    ],
    long: &[],
    operands: 0,
    assignments: false,
    numeric: false,
};

/// The table for a launcher's basename; `None` for a name the walk does not treat
/// as a launcher.
fn launcher(base: &str) -> Option<&'static Launcher> {
    Some(match base {
        "env" => &ENV,
        "exec" => &EXEC,
        "nohup" => &NOHUP,
        "setsid" => &SETSID,
        "stdbuf" => &STDBUF,
        "chrt" => &CHRT,
        "ionice" => &IONICE,
        "nice" => &NICE,
        "timeout" => &TIMEOUT,
        "sudo" => &SUDO,
        "doas" => &DOAS,
        _ => return None,
    })
}

/// Where a launcher's argv hands off to the program it runs.
enum Handoff {
    /// The program's index in argv.
    Program(usize),
    /// No program follows: `env` alone prints the environment.
    Nothing,
    /// An option the table refuses or does not list.
    Refused,
}

/// Skips a launcher's own options and operands from `start`, as getopt does: an
/// option-looking token is an option only before the first operand, and `--` ends
/// the options. The first token after them is the program.
fn handoff(l: &Launcher, argv: &[String], start: usize) -> Handoff {
    let (mut i, mut operands, mut options) = (start, l.operands, true);
    while i < argv.len() {
        let arg = argv[i].as_str();
        if options && arg == "--" {
            options = false;
            i += 1;
        } else if options && arg.starts_with('-') {
            match take_option(l, arg, argv, i) {
                Some(next) => i = next,
                None => return Handoff::Refused,
            }
        } else if arg == "--" {
            // A `--` after an operand is not the program: `chrt -o 0 -- bash` skips
            // it and runs `bash`, while `timeout 5 -- bash` runs `--`. Either way
            // the walk does not know, so it refuses.
            return Handoff::Refused;
        } else {
            options = false;
            if l.assignments && is_assignment(arg) {
                i += 1;
            } else if operands > 0 {
                operands -= 1;
                i += 1;
            } else {
                return Handoff::Program(i);
            }
        }
    }
    Handoff::Nothing
}

/// The index past the option at `i`, or `None` when the table does not list it or
/// refuses it.
fn take_option(l: &Launcher, arg: &str, argv: &[String], i: usize) -> Option<usize> {
    if let Some(long) = arg.strip_prefix("--") {
        let (name, attached) = match long.split_once('=') {
            Some((name, value)) => (name, Some(value)),
            None => (long, None),
        };
        return match (long_arity(l, name)?, attached.is_some()) {
            (Arity::Flag, false) | (Arity::Value, true) | (Arity::Attached, _) => Some(i + 1),
            (Arity::Value, false) => value_follows(argv, i),
            (Arity::Flag, true) | (Arity::Refuse, _) => None,
        };
    }
    let cluster = &arg[1..];
    if cluster.is_empty() {
        return None;
    }
    if l.numeric && cluster.bytes().all(|b| b.is_ascii_digit()) {
        return Some(i + 1);
    }
    for (at, c) in cluster.char_indices() {
        match short_arity(l, c)? {
            Arity::Flag => {}
            Arity::Value if at + c.len_utf8() < cluster.len() => return Some(i + 1),
            Arity::Value => return value_follows(argv, i),
            Arity::Attached | Arity::Refuse => return None,
        }
    }
    Some(i + 1)
}

/// The index past an option's separate value, when argv has one to give.
fn value_follows(argv: &[String], i: usize) -> Option<usize> {
    (i + 2 <= argv.len()).then_some(i + 2)
}

fn short_arity(l: &Launcher, c: char) -> Option<Arity> {
    l.short.iter().find(|(s, _)| *s == c).map(|(_, a)| *a)
}

/// A long option's arity by name: an exact match, else the one name it prefixes.
fn long_arity(l: &Launcher, name: &str) -> Option<Arity> {
    if let Some((_, arity)) = l.long.iter().find(|(n, _)| *n == name) {
        return Some(*arity);
    }
    let mut hits = l.long.iter().filter(|(n, _)| !name.is_empty() && n.starts_with(name));
    match (hits.next(), hits.next()) {
        (Some((_, arity)), None) => Some(*arity),
        _ => None,
    }
}

/// A `VAR=value` operand. `env` and `sudo` take one before the program
/// (`env FOO=bar bash`); anywhere else it is a program name, which no exec runs.
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
/// below resolves each launcher's own options ([`launcher`]) until it reaches
/// the program that will actually exec. `su` with no command lands in a login
/// shell, and answering `true` there is the safe direction; `sudo`/`doas` with
/// no program, or with an option the walk refuses, answer as a command runner. A path basename
/// counts (`/usr/bin/bash`), so kitty.nix's terminal wrapper needs no
/// special case: it always execs the resolved login shell explicitly
/// (`$SHELL`/passwd/`/bin/sh`, `<login_shell> -l`) as the conducted command.
///
/// The LIMIT, stated because a refusal's silence must not read as a
/// clearance: this reads ARGV, never file contents, so a wrapper script that
/// execs a shell (`-- ./rig.sh`) is NOT a shell by this predicate and stays
/// receptive — the allowlisted names above are the whole claim.
///
/// The ping-back does not trust this predicate's silence. It delivers only to a
/// wrap whose RESOLVED program is the harness its agent names
/// ([`wrapped_program_name`], stamped as `wrapped_program`), so a launcher this
/// walk does not model (`taskset`, `flock`) is withheld there, whatever this
/// predicate says. Here, for the forms it models, a shell answers `true`; a
/// false POSITIVE costs a skipped auto-typing, the silent-safe direction.
pub fn program_is_a_shell(argv: &[String]) -> bool {
    shell_verdict(argv).is_some()
}

/// Whether a conducted command is a shell at its OWN prompt: no `-c` command
/// string and no script operand (`bash`, `bash --norc -i`). A shell that runs a
/// command or a script (`bash -c "apt install x"`, `sh ./run.sh`) is a program
/// like any other, and its questions are heard. A privilege program is never one
/// (`sudo bash`, `su`): it asks for a password first, and the wrap cannot tell
/// that prompt from the root shell's after it, so the session is never idle
/// ([`ShellWatch::privileged`]) and its questions are heard.
///
/// Only the prompt case is silenced by the screen (`shell_snapshot`): a `bash
/// -c` child shares the shell's process group, so the foreground pgid cannot
/// tell its question from the bare prompt.
pub(in crate::graph) fn program_is_an_interactive_shell(argv: &[String]) -> bool {
    shell_verdict(argv) == Some(true) && !runs_a_privilege_program(argv)
}

/// What the launcher walk finds in one argv: whether it is a shell (the answer
/// [`shell_verdict`] gives) and the program it reaches (the answer
/// [`wrapped_program_name`] gives). One walk for both, so the screen's silence and
/// the proof cannot disagree about where the command goes.
#[derive(Default)]
struct Walked {
    shell: Option<bool>,
    program: Option<String>,
}

/// Walks an argv through its launchers, each resolved by its own table ([`launcher`]).
/// A form the walk cannot resolve is `Some(false)`: heard by the screen, never proven.
fn walk(argv: &[String]) -> Walked {
    let Some(first) = argv.first() else {
        return Walked::default();
    };
    let base = command_basename(first);
    if let Some(l) = launcher(&base) {
        // `sudo` and `doas` read as a shell whatever they run, so no automatic
        // reader types into one: a line typed before its program starts answers
        // its own password prompt. A shell under it is at its prompt only when it is the
        // whole command. Of the privilege programs only these two are
        // launchers.
        let wraps = PRIVILEGE_PROGRAMS.contains(&base.as_str());
        return match handoff(l, argv, 1) {
            Handoff::Program(p) if wraps => {
                let inner = walk(&argv[p..]);
                Walked { shell: Some(inner.shell == Some(true)), program: inner.program }
            }
            Handoff::Program(p) => walk(&argv[p..]),
            Handoff::Nothing => Walked { shell: wraps.then_some(false), program: None },
            Handoff::Refused => Walked { shell: Some(false), program: None },
        };
    }
    match base.as_str() {
        "nix" => nix_walk(argv),
        // `script` runs `$SHELL` at its prompt unless `-c` hands it a command. util-linux
        // permutes its options, so a `-c` anywhere after it counts; an over-match only
        // has the screen hear a question.
        "script" => Walked {
            shell: Some(!argv[1..].iter().any(|a| a.starts_with('-') && a.contains('c'))),
            program: None,
        },
        "su" => Walked { shell: Some(!su_runs_a_command(argv)), program: None },
        // `busybox sh` is the shell as one of its applets. Any other applet is a
        // program: every applet runs inside the one busybox binary, so comm reads
        // `busybox` for all of them. A bare `busybox` stays a shell, an over-match
        // that only has the screen hear a question.
        "busybox" => {
            let applet = argv.get(1).map(|a| command_basename(a)).unwrap_or_default();
            let shell = if applet.is_empty() {
                Some(false)
            } else if SHELL_BASENAMES.contains(&applet.as_str()) {
                Some(shell_args_are_interactive(&applet, argv.get(2..).unwrap_or_default()))
            } else {
                None
            };
            Walked { shell, program: Some(base.clone()) }
        }
        name if SHELL_BASENAMES.contains(&name) => Walked {
            shell: Some(shell_args_are_interactive(name, &argv[1..])),
            program: Some(base.clone()),
        },
        _ => Walked { shell: None, program: Some(base.clone()) },
    }
}

/// `nix develop|shell`: the payload after `-c`/`--command` is the question. Only
/// positional installables may stand before `-c`; any dashed token is an option this
/// walk does not list, so it refuses. With no `-c`, nix opens an interactive shell.
fn nix_walk(argv: &[String]) -> Walked {
    let sub = argv.get(1).map(|s| command_basename(s)).unwrap_or_default();
    match sub.as_str() {
        "develop" | "shell" => {}
        // A dashed option before the subcommand is one the walk does not list.
        _ if argv.get(1).is_some_and(|s| s.starts_with('-')) => {
            return Walked { shell: Some(false), program: None }
        }
        // Any other subcommand (`build`, `run`) is a launcher the walk does not model.
        _ => return Walked::default(),
    }
    let mut j = 2;
    while j < argv.len() {
        let a = argv[j].as_str();
        match a {
            // An empty payload is a shell with nothing to run: not at a prompt.
            "-c" | "--command" if j + 1 >= argv.len() => {
                return Walked { shell: Some(false), program: None }
            }
            "-c" | "--command" => return walk(&argv[j + 1..]),
            // The attached form carries a whole command LINE in one token.
            a if a.starts_with("--command=") => {
                let words: Vec<String> = a["--command=".len()..]
                    .split_whitespace()
                    .map(str::to_string)
                    .collect();
                return Walked { shell: walk(&words).shell, program: None };
            }
            a if a.starts_with('-') => return Walked { shell: Some(false), program: None },
            _ => j += 1,
        }
    }
    Walked { shell: Some(true), program: None }
}

/// Whether `su`'s argv hands it a command: `-c` or `--command` in any spelling. Any
/// dashed token with a `c` counts, an over-match that only has the screen hear.
fn su_runs_a_command(argv: &[String]) -> bool {
    argv[1..].iter().any(|a| {
        a.starts_with("--command") || (a.starts_with('-') && !a.starts_with("--") && a.contains('c'))
    })
}

/// [`walk`]'s shell answer. `Some(true)`: a shell at its own prompt. `Some(false)`: a
/// shell that runs a command or a script, or a form the walk refuses. `None`: not a
/// shell.
fn shell_verdict(argv: &[String]) -> Option<bool> {
    walk(argv).shell
}

/// Whether a conducted argv runs a [`PRIVILEGE_PROGRAMS`] program anywhere along
/// its launcher chain: `doas bash`, `env pkexec x`, `nice su -l`, `nix develop -c
/// run0 x`, `busybox su`. The tables [`walk`] reads name the launchers; a
/// launcher the walk does not model (`xargs`, `flock`) is not followed here.
pub(in crate::graph) fn runs_a_privilege_program(argv: &[String]) -> bool {
    let Some(first) = argv.first() else {
        return false;
    };
    let base = command_basename(first);
    if PRIVILEGE_PROGRAMS.contains(&base.as_str()) {
        return true;
    }
    if base == "busybox" {
        let applet = argv.get(1).map(|a| command_basename(a)).unwrap_or_default();
        return PRIVILEGE_PROGRAMS.contains(&applet.as_str());
    }
    if base == "nix" {
        if let Some(line) = argv.iter().find_map(|a| a.strip_prefix("--command=")) {
            let words: Vec<String> = line.split_whitespace().map(str::to_string).collect();
            return runs_a_privilege_program(&words);
        }
        let payload = argv
            .iter()
            .position(|a| matches!(a.as_str(), "-c" | "--command"))
            .map(|j| &argv[j + 1..]);
        return payload.is_some_and(runs_a_privilege_program);
    }
    match launcher(&base).map(|l| handoff(l, argv, 1)) {
        Some(Handoff::Program(p)) => runs_a_privilege_program(&argv[p..]),
        _ => false,
    }
}

/// Whether a shell's own arguments leave it at its prompt: no `-c` or
/// `--command` string and no script operand. `bash`, `bash --norc -i`, `bash -o
/// pipefail`, `fish -C x`, and `bash -` (commands from stdin, on a tty) sit at a
/// prompt; `bash -c x`, `bash -lc x`, `fish --command=x`, `sh ./run.sh` and
/// `bash -- run.sh` run a command or a script. `-C` takes a value only in fish
/// (`fish -C x` runs `x` first); for bash, sh and dash it is noclobber, so
/// `bash -C run.sh` runs the script.
fn shell_args_are_interactive(shell: &str, args: &[String]) -> bool {
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        if arg == "--" {
            return i + 1 >= args.len();
        }
        if arg == "-" {
            return true;
        }
        if arg == "--command" || arg.starts_with("--command=") {
            return false;
        }
        if let Some(long) = arg.strip_prefix("--") {
            i += if matches!(long, "rcfile" | "init-file" | "init-command") { 2 } else { 1 };
        } else if arg.len() > 1 && (arg.starts_with('-') || arg.starts_with('+')) {
            if arg.starts_with('-') && arg.contains('c') {
                return false;
            }
            let takes_value = matches!(arg, "-o" | "+o" | "-O" | "+O") || (arg == "-C" && shell == "fish");
            i += if takes_value { 2 } else { 1 };
        } else {
            return false;
        }
    }
    true
}

/// The program a wrapped argv runs, by basename: the one [`walk`] reaches. `None` when
/// the walk names no program: `script` (a shell whatever its argv), `env -S` (a command
/// line), `su`, a `nix` form without a `-c` payload, and any form a launcher's table
/// refuses. A launcher the walk does not model names itself (`taskset`), which is never
/// a harness. Stamped at registration as `wrapped_program`, the proof every reader checks.
fn wrapped_program_name(argv: &[String]) -> Option<String> {
    walk(argv).program
}

/// The label a wrap takes when no `--agent` names one: the program the launcher
/// walk resolves to (`env claude` is `claude`), else the command's own basename
/// (`taskset`, `script`). `conduct` and `spawn` both default through it, so the
/// label is the name [`is_its_harness`] checks against `wrappedProgram`.
pub(in crate::graph) fn default_label(argv: &[String]) -> String {
    wrapped_program_name(argv).unwrap_or_else(|| argv.first().map(|p| command_basename(p)).unwrap_or_default())
}

/// The registered harness a conducted argv runs, by the launcher walk: the
/// walked program must BE a profile's own launch token. `None` for everything
/// else — a shell, a launcher the walk does not model (`taskset`, `flock`), a
/// wrapper script, a name with no profile. The positive half of
/// [`program_is_a_shell`]: a caller that may only type into a harness asks for
/// this proof, never for the mere absence of a shell.
///
/// `pub` for `aoide-server`'s spawn arm, which refuses a `spawnAgent` without it
/// and then reads readiness and the submit key off the name it returns.
pub fn wrapped_harness(argv: &[String]) -> Option<&'static str> {
    let program = wrapped_program_name(argv)?;
    let profile = agent_profile(&program)?;
    (profile.launch.first().copied() == Some(program.as_str())).then_some(profile.name)
}

/// [`program_is_a_shell`]'s verdict, read back off a RECORD: `shell` (the
/// durable registration fact below) OR a `Some` restore snapshot. Not a
/// second predicate — [`SHELL_BASENAMES`] above stays the one list of shell
/// names, and both fields are written by this file alone.
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

/// Whether a wrap is a harness the launcher walk PROVES: its walked program
/// (`wrapped_program`, stamped at registration) is the launch token of the profile
/// its `agent` names. The positive half of [`wrapped_program_is_a_shell`]. A shell is
/// never a harness, and neither is a launcher form the walk refuses, a wrapper with
/// another name, or a name with no profile: none of them has a proof.
///
/// The AUTOMATIC readers (the ping-back, the doorbell's PTY arm, the A2A door's
/// inject) reach this through [`auto_type_refusal`], the one place their typed
/// lines are refused. `aoide send` is the human's own command and is not gated by
/// this proof.
pub fn is_its_harness(rec: &SessionRecord) -> bool {
    agent_profile(&rec.agent)
        .and_then(|p| p.launch.first().copied())
        .is_some_and(|bin| rec.wrapped_program.as_deref() == Some(bin))
}

/// Why an AUTOMATIC reader may not type a line into this wrap, or `None` when it may.
/// The ping-back, the doorbell's PTY arm and the A2A door's inject all ask this one
/// question and name the answer in their own audit vocabulary. A shell is named
/// first: it is the worse fact, and a `""`/`"shell"` label is a shell whatever the
/// walk says. A harness label alone never clears a wrap; the proof does.
pub fn auto_type_refusal(rec: &SessionRecord) -> Option<AutoTypeRefusal> {
    if matches!(rec.agent.as_str(), "" | "shell") || wrapped_program_is_a_shell(rec) {
        Some(AutoTypeRefusal::Shell)
    } else if !is_its_harness(rec) {
        Some(AutoTypeRefusal::Unproven)
    } else {
        None
    }
}

/// The two refusals [`auto_type_refusal`] gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoTypeRefusal {
    /// The wrap is a shell, by its label or by its walked argv.
    Shell,
    /// The launcher walk does not prove the wrap is its harness.
    Unproven,
}

/// Whether a process is invocation plumbing rather than a harness: a shell or a
/// forwarding launcher, by argv[0]'s basename (a leading `-` marks a login shell,
/// `-zsh`). The hook door's harness walk reads this verdict, and this verdict reads only
/// [`SHELL_BASENAMES`] and [`FORWARDING_LAUNCHERS`], so no other name is plumbing here.
/// [`program_is_a_shell`] resolves `timeout`, `sudo`, `doas`, `nix`, `script` and `su`
/// through its own tables and forms: those names are not in either list, so a hook
/// behind one of them takes that launcher as its harness.
/// `None` (a pid whose argv cannot be read, or an empty one) is plumbing too:
/// a process nobody can read is not a live harness.
pub(in crate::graph) fn is_invocation_plumbing(argv: Option<&[String]>) -> bool {
    let Some(program) = argv.and_then(|a| a.first()) else {
        return true;
    };
    let base = command_basename(program);
    let name = base.trim_start_matches('-');
    SHELL_BASENAMES.contains(&name) || FORWARDING_LAUNCHERS.contains(&name)
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

/// A label folded to one line and clipped to a roster-friendly width.
fn clip_label(s: &str) -> String {
    let one = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() <= 48 {
        one
    } else {
        let head: String = one.chars().take(47).collect();
        format!("{head}…")
    }
}

/// A short one-line label for an argv: a known editor shows as
/// `"<editor> <file>"` ([`friendly_editor_command`]); anything else shows the
/// argv joined with `argv[0]` collapsed to its basename
/// ([`generic_command_label`], e.g. `cargo test`). Clipped ([`clip_label`]).
fn command_label(argv: &[String]) -> Option<String> {
    friendly_editor_command(argv).or_else(|| generic_command_label(argv)).map(|l| clip_label(&l))
}

/// A process's [`command_label`], falling back to its `comm`. Used as a
/// conducted shell's live `activity`. Both facts come from the one
/// process-table reader pair below ([`proc_argv`]/[`proc_comm`]), never from a
/// `/proc` path spelled here.
fn proc_command(pid: i32) -> Option<String> {
    proc_argv(pid)
        .and_then(|argv| command_label(&argv))
        .or_else(|| proc_comm(pid).map(|c| clip_label(&c)).filter(|c| !c.is_empty()))
}

/// Pure NUL-split of a raw `/proc/<pid>/cmdline` buffer into argv — split out
/// of [`proc_command`]/[`proc_argv`] so the split itself is unit-testable
/// against a synthesized buffer without a real `/proc` read. The kernel ends
/// every argument with a NUL, so the last one ends the buffer and makes no word.
/// An empty word between two NULs is an empty argument (`grep "" f`) and stays.
/// Unix's own shape: the Windows arm of [`proc_argv`] asks the process-table
/// seam, which re-splits with that host's own parser.
#[cfg(unix)]
fn parse_cmdline(raw: &[u8]) -> Vec<String> {
    if raw.is_empty() {
        return Vec::new();
    }
    raw.strip_suffix(&[0u8])
        .unwrap_or(raw)
        .split(|b| *b == 0)
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

/// Is the foreground process a `sudo` still at its password prompt? True iff
/// the foreground process IS `sudo` AND it has not yet forked its command child
/// (`Some(false)` children == still in the PAM conversation, i.e. at the
/// prompt). `Some(true)` (command already running, e.g. cached-cred `sudo
/// nixos-rebuild`) and `None` (children unreadable, or no process table at all)
/// both DON'T fire. The text of the prompt is a different witness, read by the
/// `sudo` row of `prompt.rs`'s table; this is the process table's own. Pure and
/// unit-tested; the real caller passes `proc_comm(fg) == Some("sudo")` and
/// `proc_has_children(fg)`.
///
/// WHY the children gate: `sudo` stays the process-group leader for the WHOLE
/// runtime of `sudo <cmd>` on a PAM system (it forks the command/monitor
/// child; it never execs in place), so `comm(fg) == "sudo"` alone would also
/// be true for a multi-minute `sudo nixos-rebuild switch` running on CACHED
/// credentials — no prompt at all. Gating on "sudo has not yet forked a
/// child" narrows the signal to the actual PAM conversation window.
fn sudo_awaiting(fg_is_sudo: bool, fg_has_children: Option<bool>) -> bool {
    fg_is_sudo && fg_has_children == Some(false)
}

/// What a conducted SHELL's tick reads off its terminal: the cwd, the current
/// foreground command (`None` clears it), the idle/working/awaiting state and
/// the P-C5 continuous capture snapshot (see [`RestoreSnapshot`]).
struct ShellFacts<'a> {
    cwd: Option<&'a str>,
    activity: Option<&'a str>,
    state: &'a str,
    restore: RestoreSnapshot,
}

/// Update a conducted session's live fields off its terminal — CHANGE-ONLY,
/// under the stage lock, and re-stage graph.json only when it actually wrote.
/// No-op for an unregistered id.
///
/// A SHELL (`shell: Some`) writes everything [`ShellFacts`] names, ~1 Hz from
/// conduct's PTY tick; its `state` already folds the screen in
/// ([`shell_snapshot`]). Any other wrap is harness-less here (an agent's
/// state/activity come from hooks, so conduct never drives those): it writes
/// only what the SCREEN says, and only when the screen's verdict changes. A
/// question held makes the record `awaiting`; once it is gone the record
/// returns to `idle`, the state it was registered with, and only if the
/// question is what moved it. A wrap that has a hook record answers to that
/// phase alone: the screen never sets over it, and clears what it set.
///
/// `question` is the one screen line kept on the record (`prompt.rs`);
/// `needs_sudo` is the `sudo` row or the process table's witness. A record that
/// is `done` keeps neither and is never set `awaiting`: its episode is over.
fn do_session_refresh(
    id: &str,
    shell: Option<ShellFacts<'_>>,
    needs_sudo: bool,
    question: Option<&str>,
) {
    with_stage_lock(|| {
        let mut file: SessionsFile = match load_stage(&sessions_path()) {
            Ok(f) => f,
            Err(_) => return,
        };
        let hooked = shell.is_none()
            && load_stage::<HooksFile>(&hooks_path())
                .is_ok_and(|h| h.hooks.iter().any(|x| x.session_id == id));
        let (needs_sudo, question) = if hooked { (false, None) } else { (needs_sudo, question) };
        let mut changed = false;
        for s in file.sessions.iter_mut() {
            if s.session_id != id {
                continue;
            }
            let done = canonical_state(&s.state) == "done";
            if let Some(t) = &shell {
                if let Some(c) = t.cwd {
                    if s.cwd != c {
                        s.cwd = c.to_string();
                        changed = true;
                    }
                }
                let act = t.activity.filter(|a| !a.is_empty()).map(str::to_string);
                if s.activity != act {
                    s.activity = act;
                    changed = true;
                }
                let canon = canonical_state(t.state);
                if s.state != canon && !(done && canon == "awaiting") {
                    s.state = canon.to_string();
                    changed = true;
                }
                if s.restore.as_ref() != Some(&t.restore) {
                    s.restore = Some(t.restore.clone());
                    changed = true;
                }
            } else if !hooked && !done {
                let screen_state = if question.is_some() {
                    Some("awaiting")
                } else if s.question.is_some() && canonical_state(&s.state) == "awaiting" {
                    Some("idle")
                } else {
                    None
                };
                if let Some(state) = screen_state.filter(|state| s.state != *state) {
                    s.state = state.to_string();
                    changed = true;
                }
            }
            if done {
                if s.question.is_some() || s.needs_sudo.is_some() {
                    clear_screen_ask(s);
                    changed = true;
                }
            } else {
                // Change-only, and cleared to `None` (never written as
                // `Some(false)`) so the key disappears the moment the prompt clears.
                let want = if needs_sudo { Some(true) } else { None };
                if s.needs_sudo != want {
                    s.needs_sudo = want;
                    changed = true;
                }
                let want = question.map(str::to_string);
                if s.question != want {
                    s.question = want;
                    changed = true;
                }
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

/// The identity of the file a process runs: the device and inode its
/// `/proc/<pid>/exe` names. Two reads of the same identity are the same
/// executable, whatever the process has renamed itself to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ExeId {
    dev: u64,
    ino: u64,
}

/// What a conducted shell's wrap remembers between ticks: two facts fixed at
/// the spawn, and the CPU sample each tick moves forward. The wrap records them
/// itself, from the argv it spawned and the process table; the shell never
/// reports them.
#[derive(Debug)]
struct ShellWatch {
    /// The conducted argv, when it runs a privilege program
    /// ([`runs_a_privilege_program`]). A shell that does, which is `sudo`,
    /// `doas` or `su` and what they wrap, is never idle, and this line is its
    /// `activity` and `restore.argv`, never the root foreground it hands the
    /// terminal to. The other programs of [`PRIVILEGE_PROGRAMS`] are non-shell
    /// wraps, which never reach a tick.
    privileged: Option<Vec<String>>,
    /// The identity of the file the spawn started (see [`shell_baseline`]). A
    /// later identity that differs, or none at all, means the shell exec'd into
    /// its command or made its executable unreadable.
    exe_at_spawn: Option<ExeId>,
    /// `utime + stime` at the previous tick. A rise since then means the shell
    /// itself is running, with no child to show for it.
    cpu_at_last_tick: Option<u64>,
}

impl ShellWatch {
    fn new(privileged: Option<Vec<String>>, exe_at_spawn: Option<ExeId>) -> Self {
        Self { privileged, exe_at_spawn, cpu_at_last_tick: None }
    }
}

/// The identity of the file at `path`, following symlinks: the device and inode
/// that a `/proc/<pid>/exe` read names.
#[cfg(target_os = "linux")]
fn identity_of(path: &std::path::Path) -> Option<ExeId> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(path).ok()?;
    Some(ExeId { dev: meta.dev(), ino: meta.ino() })
}

/// The file the kernel starts when it execs `file`: the file itself, or for a
/// `#!` script its interpreter, which is what `/proc/<pid>/exe` names afterwards.
#[cfg(target_os = "linux")]
fn first_exec_image(file: &std::path::Path) -> Option<std::path::PathBuf> {
    use std::io::Read;
    let mut head = Vec::new();
    std::fs::File::open(file).ok()?.take(256).read_to_end(&mut head).ok()?;
    let Some(line) = head.strip_prefix(b"#!") else {
        return Some(file.to_path_buf());
    };
    let line = &line[..line.iter().position(|b| *b == b'\n').unwrap_or(line.len())];
    let interpreter = String::from_utf8_lossy(line).split_whitespace().next()?.to_owned();
    Some(interpreter.into())
}

/// The baseline the exec test compares against: the identity of the file the
/// spawn started (`image`, from [`super::pty::PtyChild::image`]), read from that
/// path and never from `/proc`, where a shell that has already exec'd shows its
/// command. `busybox` has none: its applets all run inside one executable, so
/// the comm check decides there. It is known by the name of the file the
/// symlinks lead to, so an `sh` that is a symlink to it has none either. A hard
/// link named `sh` is not caught.
#[cfg(target_os = "linux")]
fn shell_baseline(image: Option<&std::path::Path>) -> Option<ExeId> {
    let file = std::fs::canonicalize(first_exec_image(image?)?).ok()?;
    if file.file_name().is_some_and(|name| name == "busybox") {
        return None;
    }
    identity_of(&file)
}

/// Non-Linux hosts have no `/proc/<pid>/exe` to compare with, so there is no
/// baseline and the comm check is the whole exec test.
#[cfg(not(target_os = "linux"))]
fn shell_baseline(_image: Option<&std::path::Path>) -> Option<ExeId> {
    None
}

/// The executable identity of `pid`, or `None` when this host cannot read it. A
/// process that made itself non-dumpable (a setuid program after its exec, or
/// `PR_SET_DUMPABLE 0`) reads `None` to an unprivileged user.
#[cfg(target_os = "linux")]
fn proc_exe(pid: i32) -> Option<ExeId> {
    identity_of(std::path::Path::new(&format!("/proc/{pid}/exe")))
}

/// Non-Linux hosts have no `/proc/<pid>/exe`: the executable check is skipped
/// there, and the comm check is the whole exec test.
#[cfg(not(target_os = "linux"))]
fn proc_exe(_pid: i32) -> Option<ExeId> {
    None
}

/// `utime + stime` of `pid` in clock ticks, from `/proc/<pid>/stat`. `None`
/// on a host with no such file, and the CPU check is skipped there.
#[cfg(target_os = "linux")]
fn proc_cpu_ticks(pid: i32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // Fields after the parenthesised comm, which may hold spaces. State is
    // field 3, so utime (14) and stime (15) sit at indexes 11 and 12.
    let fields: Vec<&str> = stat.rsplit_once(')')?.1.split_whitespace().collect();
    let utime: u64 = fields.get(11)?.parse().ok()?;
    let stime: u64 = fields.get(12)?.parse().ok()?;
    Some(utime + stime)
}

#[cfg(not(target_os = "linux"))]
fn proc_cpu_ticks(_pid: i32) -> Option<u64> {
    None
}

/// Whether a process's comm names a shell. Native Windows comm carries the
/// host's `.exe` suffix; folding it here lets the one list answer on both
/// hosts.
fn comm_is_a_shell(comm: &str) -> bool {
    let name = comm.to_ascii_lowercase();
    let stem: &str = name.strip_suffix(".exe").unwrap_or(&name);
    SHELL_BASENAMES.contains(&stem)
}

/// The one predicate `state` and `restore.idle` are both read off. It is
/// evaluated once per tick in [`conduct_refresh_shell`] and passed to both
/// snapshots, so the two can never disagree. A privileged session
/// ([`ShellWatch::privileged`]) is working before anything is read. Otherwise
/// the reads run in the order CONTRACTS.md's shell-state paragraph gives, and
/// the first that answers ends it. An unreadable CPU sample is no evidence
/// either way, so the reads after it still decide. An unreadable executable is
/// an exec when there is a baseline ([`shell_baseline`]). Past the privilege
/// check, the CPU sample is taken first, so a tick that returns early still
/// moves the baseline forward.
#[allow(clippy::too_many_arguments)]
fn shell_at_prompt(
    fg: i32,
    shell_pid: i32,
    interactive: bool,
    watch: &mut ShellWatch,
    comm_of: impl Fn(i32) -> Option<String>,
    children_of: impl Fn(i32) -> Option<bool>,
    exe_of: impl Fn(i32) -> Option<ExeId>,
    cpu_of: impl Fn(i32) -> Option<u64>,
) -> bool {
    if watch.privileged.is_some() {
        return false;
    }
    let cpu = cpu_of(shell_pid);
    let cpu_rose = matches!((cpu, watch.cpu_at_last_tick), (Some(now), Some(then)) if now > then);
    if cpu.is_some() {
        watch.cpu_at_last_tick = cpu;
    }
    if fg > 0 && fg != shell_pid {
        return false;
    }
    if interactive {
        return true;
    }
    if children_of(shell_pid) == Some(true) {
        return false;
    }
    let exec_changed = match (exe_of(shell_pid), watch.exe_at_spawn) {
        (now, Some(at_spawn)) => now != Some(at_spawn),
        (_, None) => false,
    };
    if exec_changed || cpu_rose {
        return false;
    }
    // With no baseline (CONTRACTS.md names the shapes), comm is the exec test.
    watch.exe_at_spawn.is_some() || comm_of(shell_pid).is_none_or(|comm| comm_is_a_shell(&comm))
}

/// The process a shell that is not idle is running: the foreground group
/// leader,
/// or the shell's own pid when no foreground group answers (a command shell
/// with
/// children, or on a host with no foreground group at all).
fn running_pid(fg: i32, shell_pid: i32) -> i32 {
    if fg > 0 {
        fg
    } else {
        shell_pid
    }
}

/// Pure resolution of a conducted shell's (state, activity, needs_sudo,
/// question) from the shell's `idle` verdict ([`shell_at_prompt`]) and the
/// screen. Injected lookups make it unit-testable. Real caller passes
/// (proc_comm, proc_command, proc_has_children). An idle shell is labelled with
/// its own process name; a working one with the command it runs
/// ([`running_pid`]), as `activity`.
///
/// `prompt` is the screen's verdict (`prompt.rs`), from `conduct_multiplex`.
/// An INTERACTIVE shell (`interactive`, see `program_is_an_interactive_shell`)
/// is never `awaiting` off what it printed at its bare prompt: its own `read -p`
/// or `select` shares the shell's process, which the process table cannot tell
/// from the prompt, so it goes unseen. A shell that runs a command or a script
/// (`bash -c "…"`) has no prompt to silence, and its question is heard. When
/// [`sudo_awaiting`] (the process table's witness) or the screen says so,
/// `state` is FORCED to `awaiting` regardless of the idle/working computation
/// above — a prompt needs the user NOW — and `needs_sudo` is set when either
/// witness was the `sudo` prompt.
#[allow(clippy::too_many_arguments)]
fn shell_snapshot(
    fg: i32,
    shell_pid: i32,
    idle: bool,
    interactive: bool,
    prompt: Option<&Prompt>,
    comm_of: impl Fn(i32) -> Option<String>,
    command_of: impl Fn(i32) -> Option<String>,
    children_of: impl Fn(i32) -> Option<bool>,
) -> (&'static str, Option<String>, bool, Option<String>) {
    let prompt = prompt.filter(|_| !(interactive && idle));
    let (mut state, activity) = if idle {
        // At the bare prompt: idle, but label the row with the shell PROCESS
        // itself (e.g. `bash`) so the terminal roster is never blank.
        ("idle", comm_of(shell_pid))
    } else {
        // A foreground command is running: its cmdline (e.g. `nvim notes.md`,
        // `cargo test`) — the file being edited / the process at work.
        ("working", command_of(running_pid(fg, shell_pid)))
    };
    let fg_is_sudo = fg > 0 && comm_of(fg).as_deref() == Some("sudo");
    let children = if fg_is_sudo { children_of(fg) } else { None };
    let needs_sudo = sudo_awaiting(fg_is_sudo, children) || prompt.is_some_and(Prompt::sudo);
    if needs_sudo || prompt.is_some() {
        state = "awaiting";
    }
    (state, activity, needs_sudo, prompt.map(|p| p.question.clone()))
}

/// Pure resolution of a conducted shell's P-C5 restore snapshot — mirrors
/// [`shell_snapshot`]'s shape exactly (injected `argv_of` lookup, pure,
/// unit-tested). `idle` is the SAME [`shell_at_prompt`] verdict `state` is
/// derived from, passed in rather than recomputed, and kept as its own field:
/// the reap sweep overwrites `state` to `"done"` before its ledger write.
/// `argv` is `None` while idle (no process to capture) and RAW (uncollapsed,
/// unclipped) `/proc/<pid>/cmdline` of [`running_pid`] otherwise — never
/// `proc_command`'s DISPLAY label, which would re-exec the wrong or a
/// truncated binary.
fn restore_snapshot(
    fg: i32,
    shell_pid: i32,
    idle: bool,
    cwd: Option<String>,
    argv_of: impl Fn(i32) -> Option<Vec<String>>,
) -> RestoreSnapshot {
    RestoreSnapshot {
        cwd,
        idle,
        argv: if idle { None } else { argv_of(running_pid(fg, shell_pid)) },
    }
}

/// One conduct-tick refresh for a SHELL session: read the child terminal's
/// foreground process group and the live cwd, and push cwd + the current
/// command + the idle/working/awaiting state via [`shell_snapshot`], plus the
/// P-C5 restore snapshot via [`restore_snapshot`]. `prompt` is the screen's
/// standing question, if any. A privileged session ([`ShellWatch::privileged`])
/// is labelled and recorded by its own conducted line, never by the root
/// foreground it hands the terminal to, so no root command's argv reaches the
/// stage file and resurrect refuses it.
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
    interactive: bool,
    watch: &mut ShellWatch,
    prompt: Option<&Prompt>,
) {
    let fg = pty.foreground_pgid();
    let idle = shell_at_prompt(
        fg,
        shell_pid,
        interactive,
        watch,
        proc_comm,
        proc_has_children,
        proc_exe,
        proc_cpu_ticks,
    );
    let cwd = cwd_for(fg, shell_pid, proc_cwd);
    let own = watch.privileged.as_deref();
    let (state, activity, needs_sudo, question) = shell_snapshot(
        fg,
        shell_pid,
        idle,
        interactive,
        prompt,
        proc_comm,
        |pid| own.map_or_else(|| proc_command(pid), command_label),
        proc_has_children,
    );
    let restore = restore_snapshot(fg, shell_pid, idle, cwd.clone(), |pid| {
        own.map_or_else(|| proc_argv(pid), |argv| Some(argv.to_vec()))
    });
    let facts = ShellFacts { cwd: cwd.as_deref(), activity: activity.as_deref(), state, restore };
    do_session_refresh(id, Some(facts), needs_sudo, question.as_deref());
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

/// How a conducted child ended, as the death fact a hook child it hosted carries
/// ([`HookDeath`]): the same discriminator `classify_end` writes onto the record,
/// with a signal kept as its number. A wrapper's own deadline kill is `timeout`,
/// never the child's exit, and a status the host cannot name is no status at all.
fn hook_death_of(ended: Ended, timed_out: bool) -> HookDeath {
    if timed_out {
        return HookDeath { outcome: Some("timeout".into()), ..Default::default() };
    }
    match ended {
        Ended::Code(code) => HookDeath { outcome: Some("exit".into()), exit_code: Some(code), ..Default::default() },
        #[cfg(unix)]
        Ended::Signal(signo) => HookDeath { outcome: Some("signal".into()), signal: Some(signo), ..Default::default() },
        Ended::Unknown => HookDeath::default(),
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
    /// The end signal the loop answered, if one arrived while it ran.
    end_signal: Option<i32>,
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
    mut shell_watch: ShellWatch,
    interactive: bool,
    watch_prompts: bool,
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
    // The screen: what the terminal last said and when, for a shell and for a
    // wrap with no hook or trace of its own (`prompt.rs`). The tick asks it
    // whether the child has gone quiet on a question; a shell folds the answer
    // into its own refresh, any other wrap publishes it when it changes.
    let mut watch = watch_prompts.then(|| PromptWatch::new(std::time::Instant::now()));
    let mut published: Option<Prompt> = None;
    if is_shell {
        conduct_refresh_shell(id, pty, shell_pid, interactive, &mut shell_watch, None); // stamp initial cwd/state now.
    }

    // An end signal sent to the wrap itself is forwarded to the child once. The
    // child gets a short grace to go, then SIGKILL, so the wrap always reaches
    // its own death record. The child's exit is then recorded as the death.
    const WRAP_SIGNAL_GRACE: std::time::Duration = std::time::Duration::from_secs(5);
    let mut forwarded_at: Option<std::time::Instant> = None;
    let mut end_signal: Option<i32> = None;

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
        if let Some(sig) = take_wrap_signal() {
            end_signal = Some(sig);
            if forwarded_at.is_none() && !child_exited {
                unsafe { libc::kill(shell_pid as libc::pid_t, sig) };
                forwarded_at = Some(std::time::Instant::now());
            }
        }
        if !child_exited && forwarded_at.is_some_and(|at| at.elapsed() >= WRAP_SIGNAL_GRACE) {
            unsafe { libc::kill(shell_pid as libc::pid_t, libc::SIGKILL) };
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
        // Refresh the conducted shell's live cwd/command/state, and any watched
        // wrap's screen question, on the ~1s clock (a plain timeout also ticks;
        // a busy shell ticks at most this often). A child that has exited asks
        // nothing, whatever its last line said.
        if watch.is_some() && last_tick.elapsed() >= tick_period {
            let standing = watch
                .as_ref()
                .filter(|_| !child_exited)
                .and_then(|w| w.standing(std::time::Instant::now()));
            if is_shell {
                conduct_refresh_shell(id, pty, shell_pid, interactive, &mut shell_watch, standing.as_ref());
            } else if standing != published {
                let question = standing.as_ref().map(|p| p.question.as_str());
                do_session_refresh(id, None, standing.as_ref().is_some_and(Prompt::sudo), question);
                published = standing;
            }
            last_tick = std::time::Instant::now();
        }

        // child terminal → stdout, and EOF (every holder of the slave gone).
        if ready.master {
            match pty.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if let Some(w) = watch.as_mut() {
                        w.output(&buf[..n], std::time::Instant::now());
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
                    if let Some(w) = watch.as_mut() {
                        w.input(&buf[..n]);
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
                    if let Some(w) = watch.as_mut() {
                        w.input(&chunk);
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

    MultiplexEnd { timed_out, killed_with, pty_held_after_exit, end_signal }
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
    // `conduct` runs its command in the foreground of this call and registers
    // it under any `--parent` and over any non-conductable `--id`, with no
    // spawn gate in front of it. A door that cannot approve launches through
    // `spawn`, which attributes the launch to a sealed session or holds it for
    // a human.
    if let Some(refusal) = super::common::refuse_unapproving_door(
        inv,
        cmd,
        "conduct is not run for this door".to_string(),
        "conduct launches a command at once and registers it under any parent, and a door that \
         cannot approve launches only through spawn, which a sealed session attributes or a human approves",
        Fix::Run("aoide spawn -- <command>".to_string()),
    ) {
        return refusal;
    }
    // The wrap answers an end signal from the moment it runs: one that lands
    // before its child exists is forwarded at the first pass of the multiplex.
    let _wrap_signals = WrapSignals::install();
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
        .unwrap_or_else(|| default_label(&inv.args));
    let id = inv
        .flags
        .get("id")
        .cloned()
        .unwrap_or_else(|| format!("conduct-{}-{}", std::process::id(), unix_ts()));
    // The `sub:` namespace is the sub-agent hooks' own: a wrap under it would
    // answer to them (`reserved_id_refusal`).
    if let Some(refusal) = reserved_id_refusal(cmd, &id) {
        return refusal;
    }
    // A conducted id is born once: an existing record that is conductable or
    // sealed (now or ever) holds a pid no one may hand to another process, so
    // reusing its id would leave it stranded on the old pid (or invite a
    // takeover of a dead wrap's identity). A fresh id registers fresh.
    if let Ok(f) = load_stage::<SessionsFile>(&sessions_path()) {
        let me = std::process::id();
        if f.sessions.iter().any(|s| {
            s.session_id == id
                && s.pid != Some(me)
                && (s.conductable == Some(true) || s.seal.is_some() || s.ever_sealed == Some(true))
        }) {
            return Outcome::error(
                cmd,
                format!("session id `{id}` belongs to an earlier conducted session; pick a new --id (or omit it)"),
            );
        }
    }
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
    let privileged = runs_a_privilege_program(&inv.args).then(|| inv.args.clone());
    let shell_watch = ShellWatch::new(privileged, shell_baseline(child.image()));

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
    // `attestedSpawner`: whether the KERNEL backed the parent just registered,
    // judged here because this process's `/proc` ancestry is the evidence and
    // a detached spawn loses it the moment its spawner returns. Set or
    // cleared on every registration, so a re-registration under an
    // unconfirmed parent drops it.
    stamp_attested_spawner(
        &id,
        parent.as_deref().filter(|p| spawner_is_attested(p, &id, &sessions_snapshot)),
    );
    // `shell`, on the same footing: a registration fact about THIS record,
    // stamped from the argv this process is actually conducting (launchers
    // resolved by `program_is_a_shell`), so every lane that refuses to type
    // into a shell can read it off the record without having been here. Not
    // once-only like the two above — a re-registration of the same id must
    // follow the CURRENT argv, setting it for `-- env bash` and clearing it
    // for a harness.
    stamp_wrap(&id, program_is_a_shell(&inv.args), wrapped_program_name(&inv.args));
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
    let is_shell = program_is_a_shell(&inv.args);
    let end = conduct_multiplex(
        &mut pty,
        &mut console,
        inbox.as_mut(),
        &mut child,
        &id,
        is_shell,
        shell_watch,
        program_is_an_interactive_shell(&inv.args),
        // The screen speaks for a shell, and for a wrap whose harness has no
        // hook or trace profile to speak for itself. A harness that reports
        // through hooks or a trace is never touched by it.
        is_shell || agent_profile(&agent).is_none(),
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
    let death = hook_death_of(ended, end.timed_out);

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
    // The wrap saw its own harness end: a hook child it hosts whose turn is still
    // open is recorded as died now, before this wrap ends its own record.
    record_hook_deaths(&id, std::process::id(), &death);
    // The wrap ends its own record: it is the process the record names, and it
    // tells no spawner anything its death record above did not.
    let _ = do_session_end(&id, i32::try_from(std::process::id()).ok(), false);
    // An end signal the wrap answered is its own end: it leaves by that signal, so
    // its parent reads the signal, not a status of its own. The record is written
    // above, and the signal is taken again here in case it arrived after the loop.
    if let Some(sig) = end.end_signal.or_else(take_wrap_signal) {
        die_by_signal(sig);
    }

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

    /// The death a wrap records for its own child: a signal is its number, a
    /// deadline kill is `timeout` (the wrapper's act, not the child's exit), and a
    /// status the host cannot name is no status.
    #[cfg(unix)]
    #[test]
    fn the_wrap_names_its_childs_end_in_the_fact_a_hook_child_carries() {
        let signal = hook_death_of(Ended::Signal(9), false);
        assert_eq!(signal, HookDeath { outcome: Some("signal".into()), signal: Some(9), ..Default::default() });
        assert_eq!(hook_death_of(Ended::Code(3), false), HookDeath { outcome: Some("exit".into()), exit_code: Some(3), ..Default::default() });
        assert_eq!(hook_death_of(Ended::Signal(9), true), HookDeath { outcome: Some("timeout".into()), ..Default::default() });
        assert_eq!(hook_death_of(Ended::Unknown, false), HookDeath::default());
    }

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
    fn sudo_awaiting_gates_the_process_table_signal_on_children() {
        // fg IS sudo, and it has forked NO children yet: still at the PAM
        // prompt — the signal fires.
        assert!(sudo_awaiting(true, Some(false)));
        // fg IS sudo, but it HAS forked a child: auth already succeeded and the
        // command is running (e.g. cached-cred `sudo nixos-rebuild switch` — a
        // multi-minute build with no prompt at all). It must NOT fire.
        assert!(!sudo_awaiting(true, Some(true)));
        // fg IS sudo but children are unreadable: don't trust it (the screen's
        // `sudo` row is the other witness).
        assert!(!sudo_awaiting(true, None));
        // fg is not sudo at all: never.
        assert!(!sudo_awaiting(false, None));
        assert!(!sudo_awaiting(false, Some(false)));
        assert!(!sudo_awaiting(false, Some(true)));
    }
    fn asking(question: &str, sudo: bool) -> Prompt {
        Prompt { question: question.to_string(), row: if sudo { "sudo" } else { "y-n" } }
    }
    #[test]
    fn shell_snapshot_idle_at_bare_prompt() {
        // fg == shell_pid: idle, activity from comm_of(shell_pid), no sudo.
        let (state, activity, needs_sudo, question) = shell_snapshot(
            100,
            100,
            true,
            true,
            None,
            |_| Some("bash".to_string()),
            |_| panic!("command_of should not be consulted at the bare prompt"),
            |_| panic!("children_of should not be consulted when fg isn't sudo"),
        );
        assert_eq!(state, "idle");
        assert_eq!(activity.as_deref(), Some("bash"));
        assert!(!needs_sudo);
        assert_eq!(question, None);
    }
    #[test]
    fn shell_snapshot_working_reads_foreground_command() {
        // fg != shell_pid, and it's not sudo: working, activity from command_of(fg).
        let (state, activity, needs_sudo, question) = shell_snapshot(
            200,
            100,
            false,
            true,
            None,
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
        assert_eq!(question, None);
    }
    #[test]
    fn shell_snapshot_forces_awaiting_while_sudo_has_no_children() {
        // fg IS sudo, with no children yet (still at the password prompt):
        // state is FORCED to awaiting and needs_sudo is true, regardless of
        // what command_of would have said. No screen text, so no question.
        let (state, activity, needs_sudo, question) = shell_snapshot(
            300,
            100,
            false,
            true,
            None,
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
        assert_eq!(question, None);
    }
    #[test]
    fn shell_snapshot_cached_cred_sudo_does_not_force_awaiting() {
        // fg IS sudo, but it has already forked its command child (cached
        // creds, no prompt): state stays "working" off the normal computation,
        // needs_sudo is false — the F1 fix's whole point.
        let (state, activity, needs_sudo, _) = shell_snapshot(
            300,
            100,
            false,
            true,
            None,
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
    fn shell_snapshot_folds_the_screens_question_into_awaiting() {
        let ask = asking("Overwrite? [y/N]", false);
        // A foreground command asking: awaiting, the question carried, no lock.
        let (state, activity, needs_sudo, question) = shell_snapshot(
            200,
            100,
            false,
            true,
            Some(&ask),
            |_| Some("cp".to_string()),
            |_| Some("cp -i a b".to_string()),
            |_| panic!("children_of should not be consulted when fg isn't sudo"),
        );
        assert_eq!(state, "awaiting");
        assert_eq!(activity.as_deref(), Some("cp -i a b"));
        assert!(!needs_sudo);
        assert_eq!(question.as_deref(), Some("Overwrite? [y/N]"));
        // An interactive shell's own `read -p` runs in the shell process: at its
        // bare prompt the screen does not speak for the shell, so it is not heard.
        let (state, _, needs_sudo, question) = shell_snapshot(
            100,
            100,
            true,
            true,
            Some(&ask),
            |_| Some("bash".to_string()),
            |_| panic!("command_of should not be consulted at the bare prompt"),
            |_| panic!("children_of should not be consulted when fg isn't sudo"),
        );
        assert_eq!((state, needs_sudo, question), ("idle", false, None));
    }
    #[test]
    fn shell_snapshot_takes_the_lock_badge_from_the_sudo_row_alone() {
        // A command of the shell's own is in the foreground (a `make` that runs
        // `sudo` under a wrapper): awaiting AND needs_sudo, without the process
        // table having caught `sudo` yet.
        let comm = |pid: i32| Some(if pid == 200 { "make" } else { "bash" }.to_string());
        let sudo = asking("[sudo] password for khoa:", true);
        let (state, _, needs_sudo, question) =
            shell_snapshot(200, 100, false, true, Some(&sudo), comm, |_| None, |_| None);
        assert_eq!(state, "awaiting");
        assert!(needs_sudo);
        assert_eq!(question.as_deref(), Some("[sudo] password for khoa:"));
        // Any other row is a question, not the human's sudo password.
        let other = asking("Password:", false);
        let (state, _, needs_sudo, _) = shell_snapshot(200, 100, false, true, Some(&other), comm, |_| None, |_| None);
        assert_eq!((state, needs_sudo), ("awaiting", false));
        // At the bare prompt the sudo row is not the shell's question either.
        let (state, _, needs_sudo, _) = shell_snapshot(100, 100, true, true, Some(&sudo), comm, |_| None, |_| None);
        assert_eq!((state, needs_sudo), ("idle", false));
    }
    #[test]
    fn shell_snapshot_hears_the_question_of_a_shell_that_runs_a_command() {
        // `bash -c "read -p …"`: the shell is the program, so its own read sits at
        // fg == shell_pid. Not interactive, so the bare-prompt guard is off.
        let ask = asking("Overwrite? [y/N]", false);
        let (state, activity, needs_sudo, question) = shell_snapshot(
            100,
            100,
            true,
            false,
            Some(&ask),
            |_| Some("bash".to_string()),
            |_| panic!("command_of should not be consulted at the bare prompt"),
            |_| panic!("children_of should not be consulted when fg isn't sudo"),
        );
        assert_eq!(state, "awaiting");
        assert_eq!(activity.as_deref(), Some("bash"));
        assert!(!needs_sudo);
        assert_eq!(question.as_deref(), Some("Overwrite? [y/N]"));
    }
    #[test]
    fn comm_is_a_shell_folds_case_and_the_windows_suffix() {
        assert!(comm_is_a_shell("sh"));
        assert!(comm_is_a_shell("bash.exe"));
        assert!(comm_is_a_shell("ZSH"));
        assert!(!comm_is_a_shell("sleep"));
        assert!(!comm_is_a_shell("sudo"));
    }
    #[test]
    fn shell_at_prompt_is_the_process_group_alone_for_an_interactive_shell() {
        // An interactive shell at its own prompt is idle whatever sits beneath
        // it: neither the child table nor the comm is consulted.
        assert!(shell_at_prompt(100, 100, true, &mut ShellWatch::new(None, None), |_| panic!("comm is not probed"), |_| panic!("children are not probed"), |_| None, |_| None));
        assert!(shell_at_prompt(0, 100, true, &mut ShellWatch::new(None, None), |_| panic!("comm is not probed"), |_| panic!("children are not probed"), |_| None, |_| None));
    }
    #[test]
    fn shell_at_prompt_is_working_for_a_command_shell_that_has_a_child() {
        // `sh -c 'sleep 30; true'`: the shell stays alive with `sleep` as its
        // child.
        assert!(!shell_at_prompt(100, 100, false, &mut ShellWatch::new(None, None), |_| Some("sh".to_string()), |pid| {
                assert_eq!(pid, 100);
                Some(true)
            }, |_| None, |_| None));
    }
    #[test]
    fn shell_at_prompt_is_working_for_a_command_shell_that_exec_d_its_command() {
        // `sh -c 'sleep 30'`: bash exec-optimizes, so pid 100 IS `sleep`, with
        // no
        // children. It is no longer a shell, so it is working.
        assert!(!shell_at_prompt(100, 100, false, &mut ShellWatch::new(None, None), |_| Some("sleep".to_string()), |_| Some(false), |_| None, |_| None));
    }
    #[test]
    fn shell_at_prompt_is_idle_for_a_command_shell_that_is_still_a_shell_with_no_child() {
        assert!(shell_at_prompt(100, 100, false, &mut ShellWatch::new(None, None), |_| Some("sh".to_string()), |_| Some(false), |_| None, |_| None));
        // A fact that cannot be read answers as an idle shell, as it always
        // has.
        assert!(shell_at_prompt(100, 100, false, &mut ShellWatch::new(None, None), |_| None, |_| None, |_| None, |_| None));
        assert!(shell_at_prompt(100, 100, false, &mut ShellWatch::new(None, None), |_| Some("sh".to_string()), |_| None, |_| None, |_| None));
        // Native Windows: comm carries `.exe`, and a host with no foreground
        // group
        // answers 0, so the bare test is the pid's own process name.
        assert!(shell_at_prompt(0, 100, false, &mut ShellWatch::new(None, None), |_| Some("bash.exe".to_string()), |_| Some(false), |_| None, |_| None));
    }
    /// R3 (F5): a command that runs only builtins has no child and never
    /// execs, so its CPU time is the only sign it is busy. A rise between
    /// ticks reads working; a flat tick reads idle again. The first tick has no
    /// sample.
    #[test]
    fn a_builtin_only_command_loop_reads_working_while_its_cpu_time_rises() {
        let mut watch = ShellWatch::new(None, None);
        let cpu = |ticks: u64| move |_: i32| Some(ticks);
        let read = |watch: &mut ShellWatch, ticks: u64| {
            shell_at_prompt(100, 100, false, watch, |_| Some("sh".to_string()), |_| Some(false), |_| None, cpu(ticks))
        };
        assert!(read(&mut watch, 10), "the first tick has no sample to rise from");
        assert!(!read(&mut watch, 40), "CPU rose between ticks: the loop is running");
        assert!(read(&mut watch, 40), "flat CPU: the loop has ended, so idle again");
        assert!(read(&mut watch, 40), "still flat");
    }
    /// R3: a tick whose CPU cannot be read keeps the last sample, so the next
    /// readable tick compares against the last reading it actually had.
    #[test]
    fn an_unreadable_cpu_tick_keeps_the_last_sample() {
        let mut watch = ShellWatch::new(None, None);
        assert!(shell_at_prompt(100, 100, false, &mut watch, |_| Some("sh".to_string()), |_| Some(false), |_| None, |_| Some(5)));
        assert!(shell_at_prompt(100, 100, false, &mut watch, |_| Some("sh".to_string()), |_| Some(false), |_| None, |_| None));
        assert!(!shell_at_prompt(100, 100, false, &mut watch, |_| Some("sh".to_string()), |_| Some(false), |_| None, |_| Some(9)));
    }
    /// R2 (F4): a process that exec'd into another program is working, whatever
    /// name it gives itself: a `prctl` rename to `bash` changes comm, not the
    /// executable. The same executable reads idle. Without a baseline, an
    /// unreadable executable falls back to the name; with one, see
    /// `an_unreadable_exe_with_a_baseline_is_an_exec`.
    #[test]
    fn an_exec_is_read_by_the_executable_not_the_name() {
        let at_spawn = Some(ExeId { dev: 1, ino: 10 });
        let mut watch = ShellWatch::new(None, at_spawn);
        assert!(
            !shell_at_prompt(100, 100, false, &mut watch, |_| Some("bash".to_string()), |_| Some(false), |_| Some(ExeId { dev: 1, ino: 11 }), |_| None),
            "an exec'd process renamed to bash is working"
        );
        let mut watch = ShellWatch::new(None, at_spawn);
        assert!(
            shell_at_prompt(100, 100, false, &mut watch, |_| Some("bash".to_string()), |_| Some(false), |_| at_spawn, |_| None),
            "the shell itself, still its own executable, is idle"
        );
        let mut watch = ShellWatch::new(None, None);
        assert!(shell_at_prompt(100, 100, false, &mut watch, |_| Some("bash".to_string()), |_| Some(false), |_| None, |_| None));
        assert!(!shell_at_prompt(100, 100, false, &mut watch, |_| Some("sleep".to_string()), |_| Some(false), |_| None, |_| None));
    }
    /// A process that made its own executable unreadable is no longer the one the
    /// spawn started: `PR_SET_DUMPABLE 0`, or a setuid program after its exec.
    /// With a baseline it reads working, whatever comm it carries. Without one,
    /// the name decides, as it always did.
    #[test]
    fn an_unreadable_exe_with_a_baseline_is_an_exec() {
        let mut watch = ShellWatch::new(None, Some(ExeId { dev: 1, ino: 10 }));
        assert!(
            !shell_at_prompt(100, 100, false, &mut watch, |_| Some("bash".to_string()), |_| Some(false), |_| None, |_| None),
            "an unreadable executable with a baseline reads working"
        );
        let mut watch = ShellWatch::new(None, None);
        assert!(shell_at_prompt(100, 100, false, &mut watch, |_| Some("bash".to_string()), |_| Some(false), |_| None, |_| None));
    }
    /// Never idle: a session whose argv runs a privilege program is working
    /// before anything is read. A dumpable stand-in at its password prompt is
    /// still its spawned executable, with no child, flat CPU and a shell's
    /// comm, and those same facts read idle without the privilege.
    #[test]
    fn a_privileged_session_is_working_before_anything_is_read() {
        let at_spawn = Some(ExeId { dev: 1, ino: 10 });
        let mut watch = ShellWatch::new(Some(vec!["sudo".to_string(), "bash".to_string()]), at_spawn);
        assert!(!shell_at_prompt(
            100,
            100,
            false,
            &mut watch,
            |_| panic!("comm is not probed"),
            |_| panic!("children are not probed"),
            |_| panic!("the executable is not probed"),
            |_| panic!("CPU is not probed"),
        ));
        let mut watch = ShellWatch::new(None, at_spawn);
        assert!(shell_at_prompt(100, 100, false, &mut watch, |_| Some("bash".to_string()), |_| Some(false), |_| at_spawn, |_| Some(5)));
    }

    #[test]
    fn shell_at_prompt_is_working_while_a_foreground_command_holds_the_terminal() {
        // A foreground group that is not the shell: working, and neither fact
        // is probed.
        assert!(!shell_at_prompt(200, 100, false, &mut ShellWatch::new(None, None), |_| panic!("comm is not probed"), |_| panic!("children are not probed"), |_| None, |_| None));
        assert!(!shell_at_prompt(200, 100, true, &mut ShellWatch::new(None, None), |_| panic!("comm is not probed"), |_| panic!("children are not probed"), |_| None, |_| None));
        // A host with no foreground group that still has a live child
        // (Windows).
        assert!(!shell_at_prompt(0, 100, false, &mut ShellWatch::new(None, None), |_| Some("cmd.exe".to_string()), |_| Some(true), |_| None, |_| None));
    }
    #[test]
    fn shell_snapshot_a_working_command_shell_labels_its_own_command() {
        // Working off its children (fg is the shell): the activity is the
        // shell's
        // own conducted command line, read off the shell's pid.
        let (state, activity, needs_sudo, _) = shell_snapshot(
            100,
            100,
            false,
            false,
            None,
            |_| Some("sh".to_string()),
            |pid| {
                assert_eq!(pid, 100);
                Some("sh -c 'sleep 30; true'".to_string())
            },
            |_| panic!("children_of is not consulted when fg isn't sudo"),
        );
        assert_eq!(state, "working");
        assert_eq!(activity.as_deref(), Some("sh -c 'sleep 30; true'"));
        assert!(!needs_sudo);
    }
    #[test]
    fn restore_snapshot_a_command_shell_with_no_foreground_group_records_its_own_argv() {
        const SHELL_PID: i32 = 100;
        let argv = vec!["sh".to_string(), "-c".to_string(), "sleep 30; true".to_string()];
        let snap = restore_snapshot(0, SHELL_PID, false, None, |pid| {
            assert_eq!(pid, SHELL_PID);
            Some(argv.clone())
        });
        assert!(!snap.idle);
        assert_eq!(snap.argv, Some(argv));
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
    fn parse_cmdline_keeps_an_empty_argument_between_nuls() {
        // A synthesized raw `/proc/<pid>/cmdline` buffer: a NUL after every
        // argument, the last one included (the kernel's real shape).
        assert_eq!(parse_cmdline(b"nvim\0notes.md\0"), vec!["nvim".to_string(), "notes.md".to_string()]);
        // `grep "" f`: the empty argument is a word, so a re-typed line keeps it.
        assert_eq!(
            parse_cmdline(b"grep\0\0f\0"),
            vec!["grep".to_string(), String::new(), "f".to_string()]
        );
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
            true,
            Some("/home/khoa/Aoide".to_string()),
            |_| panic!("argv_of should not be consulted at the bare prompt"),
        );
        assert!(snap.idle);
        assert_eq!(snap.argv, None);
        assert_eq!(snap.cwd.as_deref(), Some("/home/khoa/Aoide"));
    }
    #[test]
    fn restore_snapshot_working_yields_full_uncollapsed_argv() {
        const SHELL_PID: i32 = 100;
        const FG_PID: i32 = 200;
        let snap = restore_snapshot(
            FG_PID,
            SHELL_PID,
            false,
            Some("/home/khoa/Aoide".to_string()),
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
    }
    #[test]
    fn invocation_plumbing_is_a_shell_or_a_launcher_by_basename() {
        let argv = |parts: &[&str]| -> Vec<String> { parts.iter().map(|s| s.to_string()).collect() };
        assert!(is_invocation_plumbing(Some(&argv(&["/bin/sh", "-c", "x"]))));
        assert!(is_invocation_plumbing(Some(&argv(&["-zsh"]))), "a login shell is a shell");
        assert!(is_invocation_plumbing(Some(&argv(&["/usr/bin/env", "A=1", "claude"]))));
        assert!(is_invocation_plumbing(Some(&argv(&["nice", "-n", "5", "claude"]))));
        assert!(!is_invocation_plumbing(Some(&argv(&["claude", "--resume", "x"]))));
        assert!(!is_invocation_plumbing(Some(&argv(&["node", "/x/claude"]))), "the verdict reads argv[0] only");
        assert!(is_invocation_plumbing(None), "an unreadable pid is plumbing");
        assert!(is_invocation_plumbing(Some(&[])), "an empty argv is plumbing");
    }

    #[test]
    fn an_interactive_shell_is_one_at_its_own_prompt_and_no_command_runner() {
        // The bare-prompt guard's question. A shell given `-c` or a script runs a
        // command, so its questions are heard; a bare shell sits at a prompt.
        let at_prompt: &[&[&str]] = &[
            &["bash"],
            &["/bin/bash", "--norc", "-i"],
            &["bash", "-l"],
            &["sh"],
            &["bash", "-o", "pipefail"],
            &["env", "bash"],
            &["nix", "develop"],
            &["nix", "develop", "-c", "bash"],
            &["script", "-q", "/dev/null"],
            // `-` reads commands from stdin, which on a tty is a prompt.
            &["bash", "-"],
            &["busybox", "sh"],
            &["busybox", "ash"],
            &["-bash"],
            &["/usr/bin/-zsh"],
            &["fish", "-C", "x"],
            // `-C` takes a value only in fish. bash, sh and dash read it as
            // noclobber.
            &["bash", "-C"],
        ];
        let runs_a_command: &[&[&str]] = &[
            // F3: `bash -C run.sh` runs a script; `-C` is no value-taker for
            // bash.
            &["bash", "-C", "run.sh"],
            &["sh", "-C", "run.sh"],
            &["bash", "-c", "read -p 'Overwrite? [y/N] ' x"],
            &["bash", "-lc", "apt install x"],
            &["bash", "-ic", "x"],
            &["sh", "./run.sh"],
            &["bash", "--", "run.sh"],
            &["script", "-c", "make", "out.log"],
            &["script", "-qc", "make", "out.log"],
            &["script", "-qfc", "make", "out.log"],
            &["script", "--command", "make", "out.log"],
            &["script", "--command=make", "out.log"],
            &["script", "--command", "bash"],
            // util-linux script permutes its options, so a `-c` after the typescript
            // file is still the command to run, not an operand.
            &["script", "-q", "/dev/null", "-c", "make"],
            &["env", "-S", "bash"],
            &["env", "--split-string=bash -c x"],
            &["env", "-iS", "bash"],
            &["fish", "--command=make"],
            &["fish", "-c", "x"],
            // A launcher option the table does not list is refused: heard, never silenced.
            &["sudo", "-i"],
            &["sudo", "-h", "host", "bash"],
            &["busybox", "sh", "-c", "x"],
            &["busybox", "ls"],
            &["sudo", "--user", "root", "bash", "-c", "x"],
            &["env", "bash", "-c", "x"],
            &["timeout", "60", "bash", "-c", "x"],
            &["sudo", "bash", "-c", "x"],
            &["sudo", "apt", "install"],
            &["su", "-c", "x", "khoa"],
            &["nix", "develop", "-c", "bash", "-c", "x"],
            &["nix", "develop", "-c", "make"],
            &["claude"],
            &["cargo", "test"],
        ];
        let owned = |case: &[&str]| case.iter().map(|s| s.to_string()).collect::<Vec<String>>();
        for case in at_prompt {
            assert!(program_is_an_interactive_shell(&owned(case)), "{case:?} sits at its prompt");
        }
        for case in runs_a_command {
            assert!(!program_is_an_interactive_shell(&owned(case)), "{case:?} runs a command");
        }
        // A privilege program is never an interactive shell: its prompt is a
        // password until its command starts. It still names a shell for the walk.
        let privilege_prompts: &[&[&str]] = &[
            &["sudo", "bash"],
            &["sudo", "-u", "root", "bash"],
            &["sudo", "--user", "root", "bash"],
            &["sudo", "-D", "/tmp", "bash"],
            &["sudo", "-T", "5", "bash"],
            &["doas", "bash"],
            &["su"],
            &["su", "-"],
            &["su", "-", "khoa"],
        ];
        for case in privilege_prompts {
            assert!(!program_is_an_interactive_shell(&owned(case)), "{case:?} asks for a password");
            assert!(program_is_a_shell(&owned(case)), "{case:?} still names a shell to the walk");
        }
    }
    /// `busybox sleep 30` is a program, not a shell: comm reads `busybox` for every
    /// applet, so a shell verdict would park it at idle. `busybox sh` stays a
    /// shell, and `busybox su` is a privilege program.
    #[test]
    fn a_busybox_applet_that_is_not_a_shell_is_not_a_shell() {
        let owned = |case: &[&str]| case.iter().map(|s| s.to_string()).collect::<Vec<String>>();
        assert!(!program_is_a_shell(&owned(&["busybox", "sleep", "30"])));
        assert!(program_is_a_shell(&owned(&["busybox", "sh"])));
        assert!(!program_is_an_interactive_shell(&owned(&["busybox", "sh", "-c", "x"])));
        assert!(runs_a_privilege_program(&owned(&["busybox", "su", "-l", "khoa"])));
        assert!(!runs_a_privilege_program(&owned(&["busybox", "sleep", "30"])));
    }

    #[test]
    fn wrapped_program_names_what_the_launcher_walk_resolves_to() {
        // The ping-back's positive proof reads this name. A launcher the walk
        // does not model names itself (never a harness); a form that eats or
        // lacks the program names nothing.
        let cases: &[(&[&str], Option<&str>)] = &[
            (&["claude"], Some("claude")),
            (&["/usr/bin/claude", "--resume", "x"], Some("claude")),
            (&["env", "FOO=1", "claude"], Some("claude")),
            (&["nice", "-n", "5", "claude"], Some("claude")),
            (&["timeout", "600", "claude"], Some("claude")),
            (&["sudo", "-u", "root", "claude"], Some("claude")),
            (&["nix", "develop", "-c", "claude"], Some("claude")),
            (&["script", "-q", "/dev/null", "claude"], None),
            (&["taskset", "-c", "0", "claude"], Some("taskset")),
            (&["taskset", "-c", "0", "bash"], Some("taskset")),
            (&["env", "-S", "bash"], None),
            (&["env", "-S", "claude"], None),
            (&["env", "--split-string=claude"], None),
            (&["env", "-iS", "claude"], None),
            (&["env", "-u", "FOO", "claude"], Some("claude")),
            (&["script", "-c", "claude", "out.log"], None),
            (&["script", "-qc", "claude", "out.log"], None),
            (&["script", "--command", "claude", "out.log"], None),
            (&["script", "--command=claude", "out.log"], None),
            (&["script", "--command", "claude"], None),
            (&["-bash"], Some("bash")),
            // A login-looking spelling of a harness is not the harness: only a
            // shell name loses its `-`.
            (&["-claude"], Some("-claude")),
            (&["/usr/bin/-claude"], Some("-claude")),
            (&["su", "-", "khoa"], None),
            (&["nix", "develop"], None),
            (&["env"], None),
            (&[], None),
        ];
        for (argv, want) in cases {
            let argv: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
            assert_eq!(wrapped_program_name(&argv).as_deref(), *want, "wrapped_program_name({argv:?})");
        }
    }

    #[test]
    fn a_launcher_option_the_table_does_not_list_is_refused_never_guessed() {
        // Default-deny: an option the table does not list refuses the whole launcher.
        // It names no program, is never proven, and is heard by the screen.
        let refused: &[&[&str]] = &[
            &["nix", "develop", "--impure", "-c", "claude"],
            &["nix", "develop", "--option", "x", "y", "-c", "bash"],
            &["nix", "--option", "x", "y", "develop", "-c", "bash"],
            &["sudo", "-h", "host", "bash"],
            &["sudo", "-i"],
            &["sudo", "-s", "claude"],
            &["ionice", "-p", "1", "claude"],
            &["chrt", "-p", "1", "claude"],
            &["env", "-"],
            &["env", "-", "claude"],
            &["nice", "-h", "claude"],
            &["timeout", "-x", "5", "claude"],
            &["env", "-S", "claude"],
            &["env", "--split-string=claude"],
        ];
        let owned = |case: &[&str]| case.iter().map(|s| s.to_string()).collect::<Vec<String>>();
        for case in refused {
            let argv = owned(case);
            assert_eq!(wrapped_program_name(&argv), None, "{case:?} names no program");
            assert_eq!(wrapped_harness(&argv), None, "{case:?} is never proven");
            assert!(program_is_a_shell(&argv), "{case:?} is heard as a command runner");
            assert!(!program_is_an_interactive_shell(&argv), "{case:?} is never silenced");
        }
    }

    #[test]
    fn a_value_an_option_takes_is_never_the_program() {
        // The bypasses the review named. Each option takes its value, so the token
        // that looks like a harness is that option's argument, and the program that
        // runs is the shell after it. Long names are read the way getopt reads them,
        // so an abbreviation (`--adj`, `--arg`) takes a value too.
        let shells: &[&[&str]] = &[
            &["env", "-a", "claude", "bash"],
            &["env", "--argv0", "claude", "bash"],
            &["env", "-ia", "claude", "bash"],
            &["env", "--arg", "claude", "bash"],
            &["nice", "--adj", "claude", "bash"],
            &["nice", "--adjustment", "claude", "bash"],
            &["timeout", "--sig", "KILL", "5", "bash"],
            &["stdbuf", "--output", "L", "bash"],
            &["stdbuf", "-o", "L", "bash"],
            &["exec", "-a", "claude", "bash"],
            &["ionice", "-c", "3", "bash"],
            &["chrt", "-T", "5", "10", "bash"],
        ];
        let owned = |case: &[&str]| case.iter().map(|s| s.to_string()).collect::<Vec<String>>();
        for case in shells {
            let argv = owned(case);
            assert_eq!(wrapped_program_name(&argv).as_deref(), Some("bash"), "{case:?}");
            assert!(program_is_an_interactive_shell(&argv), "{case:?} sits at its prompt");
            assert_eq!(wrapped_harness(&argv), None, "{case:?} is never proven as claude");
        }
        // A privilege program wraps the shell, but its prompt is a password first.
        for case in [&["sudo", "-u", "claude", "bash"][..], &["doas", "-u", "claude", "bash"][..]] {
            let argv = owned(case);
            assert_eq!(wrapped_program_name(&argv).as_deref(), Some("bash"), "{case:?}");
            assert!(!program_is_an_interactive_shell(&argv), "{case:?} asks for a password first");
        }
    }

    #[test]
    fn a_dash_dash_after_an_operand_is_refused_and_sudo_is_never_auto_typed() {
        // `chrt` skips a `--` after its priority and `timeout` runs it as the
        // program: the walk cannot tell which, so it names nothing and reads a shell.
        let owned = |case: &[&str]| case.iter().map(|s| s.to_string()).collect::<Vec<String>>();
        for case in [&["chrt", "-o", "0", "--", "bash"][..], &["chrt", "-o", "0", "--", "claude"], &["timeout", "5", "--", "bash"]] {
            let argv = owned(case);
            assert_eq!(wrapped_program_name(&argv), None, "{case:?}");
            assert!(program_is_a_shell(&argv), "{case:?} is never typed into");
            assert_eq!(wrapped_harness(&argv), None, "{case:?}");
        }
        // A harness under `sudo` is still a `sudo`: its password prompt would read
        // the first line typed.
        for case in [&["sudo", "claude"][..], &["sudo", "-u", "root", "claude"], &["doas", "claude"]] {
            assert!(program_is_a_shell(&owned(case)), "{case:?} is never auto-typed");
        }
    }

    #[test]
    fn the_forms_a_launcher_really_runs_are_the_ones_proven() {
        // Controls: every form whose exec resolves to the harness is proven.
        let proven: &[&[&str]] = &[
            &["env", "FOO=1", "claude"],
            &["env", "-i", "FOO=1", "claude"],
            &["env", "-u", "FOO", "claude"],
            &["env", "--unset=FOO", "claude"],
            &["env", "--", "claude"],
            &["nice", "-n", "5", "claude"],
            &["nice", "-5", "claude"],
            &["nice", "--adjustment=5", "claude"],
            &["timeout", "5", "claude"],
            &["timeout", "--kill-after=1", "5", "claude"],
            &["stdbuf", "-oL", "claude"],
            &["stdbuf", "--output=L", "claude"],
            &["nix", "develop", "-c", "claude"],
            &["nix", "develop", "nixpkgs#claude", "-c", "claude"],
            &["chrt", "-f", "10", "claude"],
            &["ionice", "-c", "3", "claude"],
            &["setsid", "-w", "claude"],
            &["exec", "-a", "x", "claude"],
            &["nohup", "claude"],
            &["sudo", "-E", "claude"],
        ];
        let owned = |case: &[&str]| case.iter().map(|s| s.to_string()).collect::<Vec<String>>();
        for case in proven {
            assert_eq!(wrapped_harness(&owned(case)), Some("claude"), "{case:?} runs the harness");
        }
    }

    #[test]
    fn an_env_s_after_the_program_is_the_programs_own_argument() {
        // GNU env stops reading options at its first operand (`env echo claude -S x`
        // prints `claude -S x`), so an -S after the program is that program's argument.
        let argv: Vec<String> = ["env", "claude", "-S", "x"].iter().map(|s| s.to_string()).collect();
        assert_eq!(wrapped_harness(&argv), Some("claude"));
        assert!(!program_is_a_shell(&argv));
    }

    #[test]
    fn an_option_after_an_assignment_is_a_program_name_as_getopt_reads_it() {
        // The first operand ends env's options, so `-i` after `FOO=1` is the program
        // env would try to exec: no such program, and no proof.
        let argv: Vec<String> = ["env", "FOO=1", "-i", "claude"].iter().map(|s| s.to_string()).collect();
        assert_eq!(wrapped_program_name(&argv).as_deref(), Some("-i"));
        assert_eq!(wrapped_harness(&argv), None);
    }

    #[test]
    fn a_top_level_assignment_is_a_program_name_because_conduct_execs_argv_directly() {
        // conduct runs argv[0] with `Command::new` and no shell, so `FOO=1 claude`
        // names a program that does not exist. The walk does not skip it.
        let argv: Vec<String> = ["FOO=1", "claude"].iter().map(|s| s.to_string()).collect();
        assert_eq!(wrapped_program_name(&argv).as_deref(), Some("FOO=1"));
        assert_eq!(wrapped_harness(&argv), None);
    }

    #[test]
    fn an_unlabelled_wrap_is_named_by_the_program_the_walk_resolves_to() {
        // The label is the name the positive proof checks, so a launcher in front
        // of the harness must not become it; a form the walk names nothing for
        // keeps its own basename, which is never a harness.
        let cases: &[(&[&str], &str)] = &[
            (&["claude"], "claude"),
            (&["env", "claude"], "claude"),
            (&["/usr/bin/env", "FOO=1", "claude", "--resume"], "claude"),
            (&["nix", "develop", "-c", "claude"], "claude"),
            (&["taskset", "-c", "0", "claude"], "taskset"),
            (&["script", "-q", "/dev/null"], "script"),
            (&["env", "-S", "claude"], "env"),
            (&["bash"], "bash"),
        ];
        for (argv, label) in cases {
            let argv: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
            assert_eq!(default_label(&argv), *label, "{argv:?}");
        }
    }

    #[test]
    fn wrapped_harness_accepts_only_a_registered_harness_the_walk_proves() {
        // The spawn arm's positive proof. A launcher the walk forwards through
        // resolves to the harness; a launcher it does not model, a shell, a
        // wrapper, or a script that hands a command to `--command` names no
        // harness at all.
        let cases: &[(&[&str], Option<&str>)] = &[
            (&["claude"], Some("claude")),
            (&["claude", "--dangerously-skip-permissions"], Some("claude")),
            (&["/usr/bin/claude", "--resume", "x"], Some("claude")),
            (&["env", "claude"], Some("claude")),
            (&["env", "FOO=1", "claude"], Some("claude")),
            (&["nice", "-n", "5", "claude"], Some("claude")),
            (&["timeout", "600", "claude"], Some("claude")),
            (&["nix", "develop", "-c", "claude"], Some("claude")),
            (&["script", "-q", "/dev/null", "claude"], None),
            (&["kimi"], Some("kimi")),
            (&["pi"], Some("pi")),
            (&["eidolon"], Some("eidolon")),
            (&["bash"], None),
            (&["bash", "-lc", "claude"], None),
            (&["env", "bash"], None),
            (&["env", "-S", "claude"], None),
            (&["env", "--split-string=claude"], None),
            (&["taskset", "-c", "0", "claude"], None),
            (&["/opt/x/my-wrapper"], None),
            (&["no-such-agent"], None),
            (&["-claude"], None),
            (&["script", "--command", "claude", "out.log"], None),
            (&[], None),
        ];
        for (argv, want) in cases {
            let argv: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
            assert_eq!(wrapped_harness(&argv), *want, "wrapped_harness({argv:?})");
        }
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
            // A login shell's spelling (`-bash`) is the same shell.
            (&["-bash"], true),
            (&["-claude"], false),
            (&["script", "-qc", "make", "out.log"], true),
            (&["script", "--command", "bash", "out.log"], true),
            (&["script", "--command", "claude", "out.log"], true),
            (&["script", "--command", "bash"], true),
            (&["script", "--command=bash", "out.log"], true),
            // A script is a shell whatever its argv: the wrapped harness never
            // reads past it, so it is never typed into.
            (&["script", "-q", "/dev/null", "claude"], true),
            (&["env", "-S", "claude"], true),
            (&["env", "--split-string=claude"], true),
            (&["env", "-iS", "claude"], true),
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
    /// `attestedSpawner` is stamped at registration from this process's own
    /// ancestry: a `--parent` whose pid really is above the registering
    /// process is confirmed, one the kernel does not back (however it got
    /// there) is not — and the stamp is set or cleared on every registration.
    /// GATED on Unix with its reason: the PTY-backed conduct channel, refused
    /// by name on native Windows (see `conduct_mirrors_a_nonzero_child_exit`).
    #[cfg(unix)]
    #[test]
    fn conduct_stamps_attested_spawner_only_for_a_parent_the_kernel_backs() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_SESSION_ID"]);

        let root = unique_stage("conduct-attested-spawner");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        std::env::remove_var("AOIDE_SESSION_ID");

        // A conducted wrap running above this very process, and one that is not.
        let mut above = crate::graph::testutil::session("above", "/w", "working", "1", None);
        above.conductable = Some(true);
        above.pid = Some(std::process::id());
        let mut elsewhere = above.clone();
        elsewhere.session_id = "elsewhere".into();
        elsewhere.pid = Some(i32::MAX as u32);
        write_stage(
            &sessions_path(),
            &SessionsFile { sessions: vec![above, elsewhere], ..Default::default() },
        )
        .unwrap();

        for (id, parent, want) in
            [("kid-ok", "above", Some("above")), ("kid-claimed", "elsewhere", None)]
        {
            let out = session_conduct(&conduct_invocation(
                &["sh", "-c", "exit 0"],
                &[("id", id), ("parent", parent)],
            ));
            assert_eq!(out.status, aoide_protocol::output::Status::Ok, "{}", out.message);
            let s: SessionsFile = load_stage(&sessions_path()).unwrap();
            let rec = s.sessions.iter().find(|r| r.session_id == id).unwrap();
            assert_eq!(rec.parent_session_id.as_deref(), Some(parent));
            assert_eq!(rec.attested_spawner.as_deref(), want, "{id}");
        }

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
    /// The `sub:` namespace belongs to the sub-agent hooks. A wrap registered under
    /// it would answer to them, so `conduct --id sub:x` is refused with the taught
    /// refusal before the child exists: nothing runs and nothing is registered.
    /// GATED on Unix with its reason: the control child is an `sh` script.
    #[cfg(unix)]
    #[test]
    fn conduct_refuses_a_sub_prefixed_id_before_spawning_and_registers_nothing() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR"]);
        let root = unique_stage("conduct-reserved-id");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        let ran = root.join("ran");
        let script = format!("touch {}", ran.display());

        let out = session_conduct(&conduct_invocation(&["sh", "-c", &script], &[("id", "sub:x")]));

        assert_eq!(out.status, aoide_protocol::output::Status::Usage, "msg: {}", out.message);
        assert_eq!(out.data.as_ref().unwrap()["reason"], "reserved-id");
        assert!(!ran.exists(), "the child was never spawned");
        let roster: Vec<String> = load_stage::<SessionsFile>(&sessions_path())
            .map(|f| f.sessions.into_iter().map(|s| s.session_id).collect())
            .unwrap_or_default();
        assert!(roster.is_empty(), "nothing is registered: {roster:?}");

        let ok = session_conduct(&conduct_invocation(&["sh", "-c", &script], &[("id", "x")]));
        assert_eq!(ok.status, aoide_protocol::output::Status::Ok, "control: msg: {}", ok.message);
        assert!(ran.exists(), "control: an ordinary id runs its child");
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

        let idle_restore = RestoreSnapshot { cwd: Some("/proj".into()), idle: true, argv: None };
        let working_restore = RestoreSnapshot {
            cwd: Some("/proj".into()),
            idle: false,
            argv: Some(vec!["cargo".into(), "test".into()]),
        };

        let refresh = |activity, state, needs_sudo, question, restore: &RestoreSnapshot| {
            let facts = ShellFacts { cwd: Some("/proj"), activity, state, restore: restore.clone() };
            do_session_refresh("sh", Some(facts), needs_sudo, question);
        };

        // At the prompt: idle, no activity, cwd tracked.
        refresh(None, "idle", false, None, &idle_restore);
        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert_eq!(s.sessions[0].state, "idle");
        assert_eq!(s.sessions[0].cwd, "/proj");
        assert_eq!(s.sessions[0].activity, None);
        assert_eq!(s.sessions[0].needs_sudo, None);
        assert_eq!(s.sessions[0].restore, Some(idle_restore.clone()));

        // A foreground command: working + the command as activity, and the
        // restore snapshot's raw argv persists change-only alongside it.
        refresh(Some("cargo test"), "working", false, None, &working_restore);
        let s2: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert_eq!(s2.sessions[0].state, "working");
        assert_eq!(s2.sessions[0].activity.as_deref(), Some("cargo test"));
        assert_eq!(s2.sessions[0].needs_sudo, None);
        assert_eq!(s2.sessions[0].restore, Some(working_restore));

        // Blocked on sudo: state=awaiting and needsSudo=true, regardless of the
        // `state` string passed in (the caller already resolves the force in
        // `conduct_refresh_shell`, but do_session_refresh itself just persists
        // both fields change-only).
        refresh(Some("sudo"), "awaiting", true, Some("[sudo] password for khoa:"), &idle_restore);
        let s3: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert_eq!(s3.sessions[0].state, "awaiting");
        assert_eq!(s3.sessions[0].needs_sudo, Some(true));
        assert_eq!(s3.sessions[0].question.as_deref(), Some("[sudo] password for khoa:"));

        // The prompt clears: needs_sudo=false CLEARS the field back to None
        // (never left as Some(false)) — change-only, so the key disappears.
        refresh(None, "idle", false, None, &idle_restore);
        let s4: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert_eq!(s4.sessions[0].needs_sudo, None);
        assert_eq!(s4.sessions[0].question, None);
        let raw = std::fs::read_to_string(sessions_path()).unwrap();
        assert!(!raw.contains("needsSudo"), "cleared key must be absent: {raw}");
        assert!(!raw.contains("question"), "cleared key must be absent: {raw}");

        // An unknown id is a safe no-op (never panics, never inserts).
        let facts = ShellFacts {
            cwd: Some("/x"),
            activity: Some("x"),
            state: "working",
            restore: idle_restore,
        };
        do_session_refresh("nope", Some(facts), false, None);
        let s5: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert_eq!(s5.sessions.len(), 1);

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// A wrap with no hook or trace of its own (a codex, an aider): a screen
    /// question makes its record `awaiting` with the line beside it, and taking
    /// the question back returns it to the `idle` it registered with. Nothing
    /// else on the record moves — it is not a shell and gains no snapshot.
    #[test]
    fn a_harnessless_wrap_publishes_its_screens_question_and_takes_it_back() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("refresh-screen");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        let mut sessions = Vec::new();
        upsert_session(
            &mut sessions, "wrap", Some("codex"), Some("/w"), None, None, Some(true), None, None,
            Some(std::process::id()), "2026-01-01T00:00:00Z",
        );
        write_stage(&sessions_path(), &SessionsFile { schema_version: "0".into(), sessions }).unwrap();
        let record = || {
            let s: SessionsFile = load_stage(&sessions_path()).unwrap();
            s.sessions.into_iter().next().unwrap()
        };

        do_session_refresh("wrap", None, false, Some("Apply patch? [y/N]"));
        let rec = record();
        assert_eq!(rec.state, "awaiting");
        assert_eq!(rec.question.as_deref(), Some("Apply patch? [y/N]"));
        assert_eq!(rec.needs_sudo, None);
        assert_eq!((rec.restore, rec.activity, rec.shell), (None, None, false));

        // A different question is the same episode's record, rewritten.
        do_session_refresh("wrap", None, false, Some("Run the tests? [Y/n]"));
        assert_eq!(record().question.as_deref(), Some("Run the tests? [Y/n]"));

        // The `sudo` row asks for the lock badge beside it, and loses it with it.
        do_session_refresh("wrap", None, true, Some("[sudo] password for khoa:"));
        assert_eq!(record().needs_sudo, Some(true));
        do_session_refresh("wrap", None, false, None);
        let rec = record();
        assert_eq!((rec.state.as_str(), rec.question, rec.needs_sudo), ("idle", None, None));
        let raw = std::fs::read_to_string(sessions_path()).unwrap();
        assert!(!raw.contains("question") && !raw.contains("needsSudo"), "{raw}");

        // With nothing standing and nothing to take back, a tick writes nothing.
        let before = std::fs::read_to_string(sessions_path()).unwrap();
        do_session_refresh("wrap", None, false, None);
        assert_eq!(std::fs::read_to_string(sessions_path()).unwrap(), before);

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// A record reaped to `done` keeps its task record, not its episode: a
    /// late screen verdict neither sets it `awaiting` nor writes a question or a
    /// lock badge onto it.
    #[test]
    fn a_done_record_is_never_set_awaiting_by_the_screen() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("refresh-done");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        let mut sessions = Vec::new();
        upsert_session(
            &mut sessions, "wrap", Some("codex"), Some("/w"), None, None, Some(true), None, None,
            Some(std::process::id()), "2026-01-01T00:00:00Z",
        );
        sessions[0].state = "done".into();
        write_stage(&sessions_path(), &SessionsFile { schema_version: "0".into(), sessions }).unwrap();

        do_session_refresh("wrap", None, true, Some("[sudo] password for khoa:"));
        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec = &s.sessions[0];
        assert_eq!(rec.state, "done");
        assert_eq!((rec.question.as_deref(), rec.needs_sudo), (None, None));

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// The screen only takes back a state it set: a wrap whose `state` came from
    /// somebody else (an adapter's `session phase`) and which holds no question
    /// keeps that state through every tick.
    #[test]
    fn the_screen_returns_only_a_state_its_own_question_set() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("refresh-screen-own");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        let mut sessions = Vec::new();
        upsert_session(
            &mut sessions, "wrap", Some("codex"), Some("/w"), None, None, Some(true), None, None,
            None, "2026-01-01T00:00:00Z",
        );
        sessions[0].state = "working".into();
        write_stage(&sessions_path(), &SessionsFile { schema_version: "0".into(), sessions }).unwrap();

        do_session_refresh("wrap", None, false, None);
        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert_eq!(s.sessions[0].state, "working");

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// Precedence: hook phase > screen. A wrap that has a hook record answers to
    /// that phase alone — the screen never sets a state or a question over it,
    /// and takes back what it had set before the hook record existed.
    #[test]
    fn the_screen_never_sets_over_a_hook_phase() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("refresh-screen-hooked");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        let mut sessions = Vec::new();
        upsert_session(
            &mut sessions, "wrap", Some("codex"), Some("/w"), None, None, Some(true), None, None,
            None, "2026-01-01T00:00:00Z",
        );
        write_stage(&sessions_path(), &SessionsFile { schema_version: "0".into(), sessions }).unwrap();

        // Before any adapter reports: the screen speaks.
        do_session_refresh("wrap", None, true, Some("[sudo] password for khoa:"));
        let record = || {
            let s: SessionsFile = load_stage(&sessions_path()).unwrap();
            s.sessions.into_iter().next().unwrap()
        };
        assert_eq!(record().state, "awaiting");

        // An adapter reports a phase for the wrap's own id: now it is the authority.
        let mut hooks = Vec::new();
        crate::graph::session_store::upsert_hook(&mut hooks, "wrap", "working", "2026-01-01T00:01:00Z");
        write_stage(&hooks_path(), &HooksFile { schema_version: "0".into(), hooks }).unwrap();
        do_session_refresh("wrap", None, true, Some("Apply patch? [y/N]"));
        let rec = record();
        assert_eq!(rec.question, None, "what the screen had set is taken back");
        assert_eq!(rec.needs_sudo, None);
        assert_eq!(rec.state, "awaiting", "…but the state is the hook door's to write, not the screen's");

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// Poll `ready` against the first record of `sessions.json` until it holds
    /// or `budget` runs out; the record that satisfied it, or the last one read.
    #[cfg(unix)]
    fn wait_for_record(
        id: &str,
        budget: std::time::Duration,
        ready: impl Fn(&SessionRecord) -> bool,
    ) -> Option<SessionRecord> {
        let deadline = std::time::Instant::now() + budget;
        while std::time::Instant::now() < deadline {
            if let Ok(f) = load_stage::<SessionsFile>(&sessions_path()) {
                if let Some(rec) = f.sessions.into_iter().find(|r| r.session_id == id) {
                    if ready(&rec) {
                        return Some(rec);
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        None
    }

    /// A real conducted pty end to end: the child prints a question and blocks,
    /// the record goes `awaiting` carrying the line after the quiet, an answer
    /// typed into the session's inbox ends it while the child is still running,
    /// and the record returns to a state that is not `awaiting`.
    ///
    /// The session is a real `aoide conduct --headless` PROCESS, driven exactly
    /// as `aoide send` drives one, and the connection comes from this test
    /// process, the session's parent: an in-process `session_conduct` would
    /// register this very pid, and the inbox refuses a connector whose own
    /// nearest session is the one being typed into.
    ///
    /// GATED on Unix with its reason: the children are POSIX programs
    /// (`sh -c`, `awk`) and the answer goes through the Unix-domain inbox.
    ///
    /// `heard` false is an interactive shell at its own prompt: the question is
    /// typed there and never submitted, and the record must stay off `awaiting`
    /// through the whole wait.
    #[cfg(unix)]
    fn a_blocked_child_is_asking_until_answered(
        tag: &str,
        argv: &[&str],
        agent: &str,
        shell: bool,
        heard: bool,
    ) {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_SESSION_ID",
        ]);
        let root = unique_stage(tag);
        let stage = root.join("stage");
        let state = root.join("state");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("AOIDE_STATE_DIR", &state);
        std::env::set_var("AOIDE_AUDIT_LOG", root.join("log"));
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        std::env::remove_var("AOIDE_SESSION_ID");

        let mut conducting = std::process::Command::new(built_aoide_bin())
            .args(["conduct", "--headless", "--id", tag, "--agent", agent, "--"])
            .args(argv)
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
        let reap = |child: &mut std::process::Child| {
            let _ = child.kill();
            let _ = child.wait();
        };

        if !heard {
            // The session must be recorded first, or the negative proves nothing.
            let recorded = wait_for_record(tag, std::time::Duration::from_secs(10), |_| true);
            // Typed at the bare prompt and never submitted: on the screen, not asked.
            use std::io::Write as _;
            let mut door = std::os::unix::net::UnixStream::connect(conduct_socket_path(tag))
                .expect("the session's inbox is bound once it is recorded");
            door.write_all(b"Proceed? [y/N] ").unwrap();
            drop(door);
            // Past QUIET plus a tick: a heard question would be awaiting by now.
            let seen = wait_for_record(tag, std::time::Duration::from_secs(8), |r| r.state == "awaiting");
            reap(&mut conducting);
            assert!(recorded.is_some(), "the session was never recorded");
            assert!(seen.is_none(), "the interactive shell's bare prompt was heard as a question");
            let _ = std::fs::remove_dir_all(&root);
            return;
        }

        // QUIET plus a tick; the budget has room for a loaded box.
        let Some(asking) = wait_for_record(tag, std::time::Duration::from_secs(30), |r| {
            r.state == "awaiting" && r.question.is_some()
        }) else {
            reap(&mut conducting);
            panic!("the quiet question never made the record awaiting");
        };
        assert_eq!(asking.question.as_deref(), Some("Overwrite? [y/N]"));
        assert_eq!(asking.needs_sudo, None, "an ordinary question is not the human's password");
        assert_eq!(asking.shell, shell);
        assert_eq!(asking.restore.is_some(), shell, "only a shell is ticked as one");

        use std::io::Write as _;
        let mut door = std::os::unix::net::UnixStream::connect(conduct_socket_path(tag))
            .expect("the session's inbox is bound while its child runs");
        door.write_all(b"y\n").unwrap();
        drop(door);

        let Some(answered) = wait_for_record(tag, std::time::Duration::from_secs(30), |r| {
            r.state != "awaiting" && r.question.is_none()
        }) else {
            reap(&mut conducting);
            panic!("the answered question never left the record");
        };
        assert_ne!(answered.state, "done", "the child is still running its sleep");

        let status = conducting.wait().expect("conduct returns");
        assert!(status.success(), "conduct exited {status:?}");
        let rec = wait_for_record(tag, std::time::Duration::from_secs(5), |r| r.state == "done").unwrap();
        assert_eq!(rec.question, None);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A shell given a command string runs it, and its `read` is a question like
    /// any program's: the screen hears it even though the shell's own process
    /// is the foreground group (`shell_snapshot`).
    #[cfg(unix)]
    #[test]
    fn a_shell_running_a_command_string_is_heard() {
        a_blocked_child_is_asking_until_answered(
            "screen-bash-c-read",
            &["bash", "-c", "read -p 'Overwrite? [y/N] ' x; sleep 4"],
            "worker",
            true,
            true,
        );
    }

    /// An interactive shell at its own prompt: a question typed there is on the
    /// screen but is not asked, because its `read` would share the prompt's
    /// process (`shell_snapshot`).
    #[cfg(unix)]
    #[test]
    fn an_interactive_shell_at_its_own_prompt_is_never_asked() {
        a_blocked_child_is_asking_until_answered(
            "screen-interactive-bash",
            &["bash", "--norc", "--noprofile", "-i"],
            "worker",
            true,
            false,
        );
    }

    /// A program that is not a shell and reports through neither hooks nor a
    /// trace: the wrap itself speaks for it, and writes no shell snapshot.
    #[cfg(unix)]
    #[test]
    fn a_harnessless_program_blocked_on_a_question_is_asking_until_answered() {
        a_blocked_child_is_asking_until_answered(
            "screen-awk-read",
            &["awk", "BEGIN { printf \"Overwrite? [y/N] \"; fflush(); getline x < \"-\"; system(\"sleep 4\") }"],
            "awk",
            false,
            true,
        );
    }

    /// One real `aoide conduct --headless` session under its own stage, for the
    /// state tests below. Field order is drop order: the env is restored before
    /// the lock is released, and `Drop` kills and removes only what this
    /// started.
    #[cfg(unix)]
    struct LiveSession {
        tag: String,
        root: PathBuf,
        child: std::process::Child,
        _env: EnvVars,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    #[cfg(unix)]
    impl LiveSession {
        fn start(tag: &str, argv: &[&str]) -> Self {
            Self::start_with(tag, argv, None)
        }

        /// `bin` goes first on the session's `PATH`.
        fn start_with(tag: &str, argv: &[&str], bin: Option<&std::path::Path>) -> Self {
            let lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
            let env = EnvVars::save(&[
                "AOIDE_STAGE_DIR",
                "AOIDE_STATE_DIR",
                "XDG_RUNTIME_DIR",
                "AOIDE_AUDIT_LOG",
                "AOIDE_SESSION_ID",
            ]);
            let root = unique_stage(tag);
            let stage = root.join("stage");
            let state = root.join("state");
            std::fs::create_dir_all(&stage).unwrap();
            std::env::set_var("AOIDE_STAGE_DIR", &stage);
            std::env::set_var("AOIDE_STATE_DIR", &state);
            std::env::set_var("AOIDE_AUDIT_LOG", root.join("log"));
            std::env::set_var("XDG_RUNTIME_DIR", &root);
            std::env::remove_var("AOIDE_SESSION_ID");
            let mut conduct = std::process::Command::new(built_aoide_bin());
            conduct.args(["conduct", "--headless"]);
            if let Some(bin) = bin {
                let path = std::env::var_os("PATH").unwrap_or_default();
                let dirs = std::iter::once(bin.to_path_buf()).chain(std::env::split_paths(&path));
                conduct.env("PATH", std::env::join_paths(dirs).expect("a PATH"));
            }
            let child = conduct
                .args(["--id", tag, "--agent", "worker", "--"])
                .args(argv)
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
            LiveSession { tag: tag.to_string(), root, child, _env: env, _lock: lock }
        }

        fn wait_for(&self, budget: std::time::Duration, ready: impl Fn(&SessionRecord) -> bool) -> Option<SessionRecord> {
            wait_for_record(&self.tag, budget, ready)
        }

        /// Writes `bytes` into the session's inbox, the way `aoide send` does.
        fn type_bytes(&self, bytes: &[u8]) {
            use std::io::Write as _;
            let mut door = std::os::unix::net::UnixStream::connect(conduct_socket_path(&self.tag))
                .expect("the session's inbox is bound while it runs");
            door.write_all(bytes).unwrap();
        }

        /// The pid of the program the session conducts: the one child `aoide
        /// conduct` spawns.
        #[cfg(target_os = "linux")]
        fn conducted_pid(&self, budget: std::time::Duration) -> Option<i32> {
            let conduct = self.child.id();
            let deadline = std::time::Instant::now() + budget;
            while std::time::Instant::now() < deadline {
                let children = std::fs::read_to_string(format!("/proc/{conduct}/task/{conduct}/children"));
                if let Some(pid) = children.ok().and_then(|c| c.split_whitespace().next()?.parse().ok()) {
                    return Some(pid);
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            None
        }

        fn wait_exit(&mut self) -> std::process::ExitStatus {
            self.child.wait().expect("conduct returns")
        }
    }

    #[cfg(unix)]
    impl Drop for LiveSession {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    /// A command shell reads WORKING for as long as its command runs, and
    /// reads `done` once the command has ended. Run for two shapes: a leaf
    /// command that the shell exec-optimizes away (its pid becomes the command,
    /// with no children), and a command followed by another step (the shell
    /// stays alive with the command as its child).
    #[cfg(unix)]
    fn a_command_shell_reads_working_until_its_command_ends(tag: &str, argv: &[&str]) {
        let mut session = LiveSession::start(tag, argv);
        let Some(working) = session.wait_for(std::time::Duration::from_secs(30), |r| r.state == "working") else {
            panic!("the command shell never read working while its command ran");
        };
        assert!(
            working.restore.as_ref().is_some_and(|r| !r.idle),
            "a working command shell must not snapshot as idle: {:?}",
            working.restore
        );
        let status = session.wait_exit();
        assert!(status.success(), "conduct exited {status:?}");
        assert!(
            session.wait_for(std::time::Duration::from_secs(10), |r| r.state == "done").is_some(),
            "the command ended but the record never left working"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_command_shell_that_became_its_command_reads_working_while_it_runs() {
        a_command_shell_reads_working_until_its_command_ends("sb-exec-leaf", &["sh", "-c", "sleep 3"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_command_shell_with_a_child_command_reads_working_while_it_runs() {
        a_command_shell_reads_working_until_its_command_ends("sb-child", &["sh", "-c", "sleep 3; true"]);
    }

    /// R2 (F4), end to end: the child execs `python3`, which renames itself
    /// `bash` with `prctl(PR_SET_NAME)`. Its comm now names a shell and its
    /// executable does not, so the shell reads working. Skipped, with a printed
    /// reason, where python3 is not on PATH.
    #[cfg(unix)]
    #[test]
    fn a_command_shell_whose_exec_renames_itself_bash_reads_working() {
        if python3_image().is_none() {
            eprintln!("skipping: python3 is not on PATH, so nothing here can rename itself with prctl");
            return;
        }
        a_command_shell_reads_working_until_its_command_ends(
            "sb-rename",
            &["sh", "-c", "exec python3 -c 'import ctypes,time; ctypes.CDLL(None).prctl(15, b\"bash\", 0, 0, 0); time.sleep(6)'"],
        );
    }

    /// The spoof a non-setuid process can run on itself: it renames itself `bash`
    /// and makes its own executable unreadable with `PR_SET_DUMPABLE 0`. The
    /// shell here IS python3, an `sh` on a private `PATH` symlinked to its ELF,
    /// so nothing execs and the baseline is python3's own file. Once comm says
    /// `bash` and the executable cannot be read, the verdict settles on working,
    /// though there is no child and the CPU is flat: three seconds of ticks that
    /// all read it. A tick from before the prctl may still be on the record, so
    /// the test waits for the settled stretch rather than reading one record.
    /// Skipped as root, which reads every executable, and where python3 is not
    /// on PATH.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_command_shell_renamed_to_bash_and_made_undumpable_reads_working() {
        if unsafe { libc::geteuid() } == 0 {
            eprintln!("skipping: as root every executable stays readable");
            return;
        }
        let Some(python3) = python3_image() else {
            eprintln!("skipping: python3 is not on PATH, so nothing here can rename itself with prctl");
            return;
        };
        let bin = unique_stage("sb-undump-bin");
        std::os::unix::fs::symlink(&python3, bin.join("sh")).expect("an sh that links to python3");
        let session = LiveSession::start_with(
            "sb-undump",
            &["sh", "-c", "import ctypes,time; c=ctypes.CDLL(None); c.prctl(15, b'bash', 0, 0, 0); c.prctl(4, 0, 0, 0, 0); time.sleep(30)"],
            Some(&bin),
        );
        let shell = session.conducted_pid(std::time::Duration::from_secs(10)).expect("the conducted shell starts");
        let exe = format!("/proc/{shell}/exe");
        let spoofed = || {
            std::fs::read_to_string(format!("/proc/{shell}/comm")).is_ok_and(|c| c.trim_end() == "bash")
                && std::fs::metadata(&exe).is_err()
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !spoofed() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(spoofed(), "the shell never renamed itself bash and made its executable unreadable");
        let settle = std::time::Duration::from_secs(3);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        let mut working_since: Option<std::time::Instant> = None;
        let mut last = None;
        while std::time::Instant::now() < deadline && working_since.is_none_or(|t| t.elapsed() < settle) {
            last = session.wait_for(std::time::Duration::from_millis(500), |_| true);
            let working = last.as_ref().is_some_and(|r| {
                r.state == "working" && r.restore.as_ref().is_some_and(|x| !x.idle)
            });
            working_since = if working { working_since.or(Some(std::time::Instant::now())) } else { None };
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert!(
            working_since.is_some_and(|t| t.elapsed() >= settle),
            "an undumpable shell renamed to bash never settled on working: {last:?}"
        );
        drop(session);
        let _ = std::fs::remove_dir_all(&bin);
    }

    /// The baseline is the image the spawn started, and a `#!` script's image is
    /// its interpreter: the file `/proc/<pid>/exe` names after the exec. The
    /// interpreter is a stand-in built here, so the host's own `/bin/sh` (busybox
    /// in the nix sandbox) decides nothing.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_script_baseline_is_its_interpreter() {
        let dir = unique_stage("baseline-script");
        let interpreter = dir.join("standin-sh");
        std::fs::write(&interpreter, b"\x7fELF").expect("the stand-in");
        let script = dir.join("probe.sh");
        std::fs::write(&script, format!("#!{} -e\necho hi\n", interpreter.display())).expect("the script");
        assert_eq!(first_exec_image(&script), Some(interpreter.clone()));
        assert_eq!(first_exec_image(&interpreter), Some(interpreter.clone()));
        assert!(identity_of(&interpreter).is_some());
        assert_eq!(shell_baseline(Some(&script)), identity_of(&interpreter));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `busybox` has no baseline, since its applets never change the executable.
    /// It is known by the file the spawned path resolves to, never by the name:
    /// an `sh` that is a symlink to it has none, nor has a script it interprets,
    /// while a symlink named `busybox` to another file has one. A hard link named
    /// `sh` is not caught: it is the same file under another name, and symlinks
    /// are all the name is read through. The stand-in is built here; a nix
    /// busybox ships no applet links.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_shell_symlinked_to_busybox_has_no_baseline() {
        let dir = unique_stage("baseline-busybox");
        let busybox = dir.join("busybox");
        std::fs::write(&busybox, b"\x7fELF").expect("the stand-in");
        let sh = dir.join("sh");
        std::os::unix::fs::symlink(&busybox, &sh).expect("an sh that is a symlink to busybox");
        assert_eq!(shell_baseline(Some(&sh)), None, "an sh symlinked to busybox");
        assert_eq!(shell_baseline(Some(&busybox)), None);
        let script = dir.join("probe.sh");
        std::fs::write(&script, format!("#!{} sh\n", sh.display())).expect("the script");
        assert_eq!(shell_baseline(Some(&script)), None, "a script busybox interprets");
        let other = dir.join("standin-sh");
        std::fs::write(&other, b"\x7fELF").expect("another stand-in");
        let named = dir.join("named");
        std::fs::create_dir(&named).expect("a dir for the misnamed symlink");
        std::os::unix::fs::symlink(&other, named.join("busybox")).expect("a symlink named busybox");
        assert_eq!(shell_baseline(Some(&named.join("busybox"))), identity_of(&other));
        let hard = named.join("sh");
        std::fs::hard_link(&busybox, &hard).expect("an sh that is a hard link to busybox");
        assert_eq!(shell_baseline(Some(&hard)), identity_of(&busybox), "a hard link is not caught");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Never idle, end to end: a dumpable `sudo` stand-in (a `#!` script on a
    /// private `PATH`) sits at its password prompt with the same executable, no
    /// child and flat CPU, which reads idle for any other program. No tick
    /// reads it idle: the screen still asks for the password, and `restore`
    /// stays not idle.
    #[cfg(unix)]
    #[test]
    fn a_privileged_session_never_reads_idle_at_its_password_prompt() {
        let bin = unique_stage("sb-sudo-bin");
        let sudo = bin.join("sudo");
        std::fs::write(&sudo, "#!/bin/sh\nprintf '[sudo] password for khoa: '\nread -r pw\n").expect("the stand-in");
        std::fs::set_permissions(&sudo, std::os::unix::fs::PermissionsExt::from_mode(0o755)).expect("its mode");
        let session = LiveSession::start_with("sb-sudo", &["sudo", "bash"], Some(&bin));
        let Some(asking) = session.wait_for(std::time::Duration::from_secs(30), |r| r.state == "awaiting" && r.question.is_some()) else {
            panic!("the stand-in's password prompt was never heard");
        };
        assert_eq!(asking.question.as_deref(), Some("[sudo] password for khoa:"));
        assert_eq!(asking.needs_sudo, Some(true));
        let idle = session.wait_for(std::time::Duration::from_secs(4), |r| r.restore.as_ref().is_none_or(|x| x.idle));
        assert!(idle.is_none(), "a privileged session read idle at its prompt: {idle:?}");
        let rec = session.wait_for(std::time::Duration::from_secs(1), |_| true).expect("the record");
        assert_eq!(rec.state, "awaiting");
        assert_eq!(rec.question.as_deref(), Some("[sudo] password for khoa:"));
        drop(session);
        let _ = std::fs::remove_dir_all(&bin);
    }

    /// R3 (F5), end to end: a loop of builtins, with no child and no exec,
    /// reads working while its CPU time rises. A million iterations outlast the
    /// ticks that must see the rise.
    #[cfg(unix)]
    #[test]
    fn a_command_shell_running_only_builtins_reads_working_while_it_burns_cpu() {
        a_command_shell_reads_working_until_its_command_ends(
            "sb-builtin",
            &["sh", "-c", "i=0; while [ $i -lt 1000000 ]; do i=$((i+1)); done"],
        );
    }

    /// Never idle, for the line it records: a privilege program that hands the
    /// terminal to a root foreground is still labelled and recorded by its own
    /// conducted line, so no root command's argv reaches the stage file and
    /// resurrect refuses it. `script` is a stand-in named `argv[0]` on a private
    /// `PATH`, and `handed` holds once it has handed the terminal over.
    #[cfg(target_os = "linux")]
    fn a_privileged_session_records_its_own_line(tag: &str, argv: &[&str], script: &str, handed: impl Fn(i32) -> bool) {
        let bin = unique_stage(&format!("{tag}-bin"));
        let stand_in = bin.join(argv[0]);
        std::fs::write(&stand_in, script).expect("the stand-in");
        std::fs::set_permissions(&stand_in, std::os::unix::fs::PermissionsExt::from_mode(0o755)).expect("its mode");
        let session = LiveSession::start_with(tag, argv, Some(&bin));
        let ticked = session.wait_for(std::time::Duration::from_secs(10), |r| {
            r.restore.as_ref().is_some_and(|x| x.argv.is_some())
        });
        assert!(ticked.is_some(), "the session never recorded a running argv");
        let shell = session.conducted_pid(std::time::Duration::from_secs(10)).expect("the stand-in starts");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !handed(shell) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(handed(shell), "the stand-in never handed the terminal to its root foreground");
        let line: Vec<String> = argv.iter().map(|a| a.to_string()).collect();
        let label = argv.join(" ");
        let own = |r: &SessionRecord| {
            r.restore.as_ref().is_some_and(|x| x.argv.as_ref() == Some(&line))
                && r.activity.as_deref() == Some(label.as_str())
        };
        // Three ticks after the handover, any of which would record the root
        // foreground.
        let leaked = session.wait_for(std::time::Duration::from_secs(3), |r| !own(r));
        assert!(leaked.is_none(), "the root foreground reached the record: {leaked:?}");
        let restore = session
            .wait_for(std::time::Duration::from_secs(1), |_| true)
            .and_then(|r| r.restore)
            .expect("a shell's restore snapshot");
        assert_eq!(restore.argv.as_ref(), Some(&line));
        assert_eq!(super::super::resurrect::restore_refused_because(&restore), Some("a privilege program"));
        drop(session);
        let _ = std::fs::remove_dir_all(&bin);
    }

    /// `doas` execs into the root shell, which runs its command in place: the
    /// conducted pid itself becomes the root foreground.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_doas_session_records_its_own_line_after_it_execs_its_shell() {
        a_privileged_session_records_its_own_line(
            "sb-doas",
            &["doas", "bash"],
            "#!/bin/sh\nexec sleep 30\n",
            |pid| std::fs::read_to_string(format!("/proc/{pid}/comm")).is_ok_and(|c| c.trim_end() == "sleep"),
        );
    }

    /// `su` stays the parent and hands the terminal to its child, whose command
    /// takes the foreground group. Modelled by a job-control script whose
    /// foreground job owns the terminal.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_su_session_records_its_own_line_while_its_child_holds_the_terminal() {
        a_privileged_session_records_its_own_line(
            "sb-su",
            &["su", "-"],
            "#!/bin/sh\nset -m\nsh -c 'sleep 30'\n",
            |pid| terminal_foreground_of(pid).is_some_and(|fg| fg != pid),
        );
    }

    /// The foreground process group of `pid`'s terminal (`tpgid`), off
    /// `/proc/<pid>/stat`.
    #[cfg(target_os = "linux")]
    fn terminal_foreground_of(pid: i32) -> Option<i32> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        stat.rsplit_once(')')?.1.split_whitespace().nth(5)?.parse().ok()
    }

    /// The ELF file `python3` on PATH runs, as python3 reads its own
    /// `/proc/self/exe`: past any `#!` wrapper, so a link to it starts python3
    /// itself. `None` where python3 or its ctypes is missing.
    #[cfg(unix)]
    fn python3_image() -> Option<PathBuf> {
        let out = std::process::Command::new("python3")
            .args(["-c", "import ctypes, os; print(os.readlink('/proc/self/exe'))"])
            .output()
            .ok()
            .filter(|o| o.status.success())?;
        Some(PathBuf::from(String::from_utf8(out.stdout).ok()?.trim_end()))
    }

    /// An interactive shell reads idle at its own prompt, and a command typed
    /// there reads working for as long as it runs. Its bare-prompt rule is the
    /// process-group check alone, which this change leaves as it was.
    #[cfg(unix)]
    #[test]
    fn an_interactive_shell_is_idle_at_its_prompt_and_working_while_its_command_runs() {
        let session = LiveSession::start("sb-interactive", &["bash", "--norc", "--noprofile", "-i"]);
        assert!(
            session.wait_for(std::time::Duration::from_secs(10), |r| r.state == "idle").is_some(),
            "an interactive shell never read idle at its prompt"
        );
        assert!(
            session.wait_for(std::time::Duration::from_millis(2500), |r| r.state != "idle").is_none(),
            "an interactive shell at its own prompt must stay idle"
        );
        session.type_bytes(b"sleep 3\n");
        assert!(
            session.wait_for(std::time::Duration::from_secs(15), |r| r.state == "working").is_some(),
            "a command typed at the interactive prompt never read working"
        );
        assert!(
            session.wait_for(std::time::Duration::from_secs(15), |r| r.state == "idle").is_some(),
            "the interactive shell never returned to idle after its command"
        );
    }
}
