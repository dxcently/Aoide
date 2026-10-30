# aoide-desktop-exec

The one crate in the desktop workspace that starts a process or sends a
signal. Every child the app runs starts here, from a program `check_pin` has
pinned, with its argv built against the schema the program prints and its
environment scrubbed. The shell reaches a process or a signal only through
this crate.

Its dependencies are serde and serde_json (the schema and the config a child
prints), libc (the spawn, signal and pidfd calls), and aoide-protocol by path,
for `visible`. It re-exports that crate's `untrusted` module, so the app
reaches aoide-protocol only through here, and of it only `untrusted`.

## Seams

- `check_pin`, which returns a `Pinned` (the only maker of one), `Schema`,
  `argv`, `child_env`, `parse_vault_keys`, `Output`, `ExecError`, and `Exec`
  (`load`, `call`, `follow`, `schema`, `deadline`, `door_env`, `spawn_door`).
  `spawn_door` returns an `Arrived`. `deadline` only shortens: a later call
  never lengthens an earlier one.
- `Schema` keeps, per command, its `brief` (else its summary), `doors`
  (`cli` and `daemon` when the schema omits them), the marks `approves`,
  `launches`, `internal` and `implemented` (each false when omitted), and
  `kind` when the schema names one. `Schema::commands` lists every command.
- `follow` starts a held child and returns a `Follow`. `Follow::next` yields
  an `Event` (a `Line`, a `Dropped` line past `LINE_CAP`, or `Eof`), and
  `Follow::end` returns an `Ended`. `LINE_CAP` is 1 MiB.
- The door helpers the shell's door code calls, all `pub`: `euid`,
  `open_pidfd`, `terminate_pidfd`, `pidfd_exited`, `stat_of` (with its `Stat`)
  and `Arrived`. `terminate` stays crate-private: `run` is its one production
  caller.
- Behind `fakes`, `make_fifo` makes the FIFO the token test puts at the token
  path. `test_doors` starts a door (`spawn`), signals one (`terminate`), copies
  a program through a child (`copy_program`), makes a zombie (`zombie`), and
  blocks and waits for SIGTERM (`block_sigterm` and `wait_sigterm`), which is
  how the fake eidolon takes its signal without a libc call of its own. The
  SIGINT pair (`block_sigint` and `wait_sigint`) does the same for the fake
  aoide's held children.

## How a child starts

```
lib.rs      Exec::load(aoide, home, env)
              |
              +-- check_pin      absolute, a regular file this user can run,
              |                  not under <home>/.local/bin, with no setuid
              |                  or setgid bit, and the file and each directory
              |                  above it, up to /, owned by root or this user
              |                  and not writable by group or other (a
              |                  directory's sticky bit excepted)
              +-- Schema::load   aoide schema --json
              +-- config         aoide config --json   -> vault key names
              |
              v
            Exec::call(words, flags, positionals, stdin)
              |
              +-- child_env      refused while the vault names are unknown,
              |                  unless the call is schema or config
              +-- argv           path must be in the schema, flags declared,
              |                  required arguments given, `--` before text
              |
              v
            run()              Command on the pinned path, env_clear,
                               one deadline (30 s): SIGTERM at it, 5 s to
                               exit; stdout and stderr: 16 MiB each
```

- `check_pin` refuses a relative path, a missing one, a target that is not an
  executable regular file, any target under `<home>/.local/bin`, and a target
  with its setuid or setgid bit set (`PinError::SetId`). It checks the target
  and every directory above it, up to `/`: one owned by a uid that is neither
  root nor this user is refused (`PinError::Owner`), and so is one writable by
  group or other (`PinError::Writable`). A directory with its sticky bit set is
  exempt from the writable rule, since no other user may rename or remove its
  entries, which is why `/nix/store` (1775, root) passes. It returns a `Pinned`
  of the canonical path, which is the only path the app runs.
- `argv` builds the argument list from `aoide schema --json`. It refuses, before
  anything runs, an unknown path, an undeclared flag, a bool flag given a value,
  a valued flag given none, a missing required argument, a NUL byte, and a
  positional that names a longer declared command than the words (`Resolves`:
  aoide takes the longest declared path over words and positionals together).
- `child_env` builds the environment from the app's own, minus the variables
  the child must not see. An `eidolon` child loses `AOIDE_SESSION_ID`, the
  `AOIDE_MELETE_*` variables and the vault keys. An `aoide` child loses the
  `AOIDE_MELETE_*` variables unless its first word is `melete`, and the vault
  keys. The names compare as bytes, so a name that is not UTF-8 meets the same
  rules.
- The vault key names are the `tokenEnv` names in `aoide config --json`. While
  they are unknown, every call but `schema` and `config` is refused before a
  child starts. The names are unknown when the config call fails or does not
  parse, or when a `tokenEnv` is not `[A-Za-z_][A-Za-z0-9_]*`, or names `PATH`,
  `HOME`, `XDG_RUNTIME_DIR` or `AOIDE_ROOT`. A bad `tokenEnv` is shown through
  `visible`. The pattern has no length bound and is case-sensitive: a 10 KiB
  name and a lowercase `path` are accepted, and `HOME=x` is refused.
- `open_pidfd` opens a pidfd for a pid, and `terminate_pidfd` sends SIGTERM
  through one. The refusals and the reach of a pidfd are in AGENTS.md.
  `spawn_door` opens one as the door's pid arrives.
- `pidfd_exited` polls a pidfd with no wait. It answers whether the process has
  ended, and the watch uses it to decide whether its door still lives.
- `door_env` is `child_env` for `eidolon web`, with `XDG_RUNTIME_DIR` set to the
  checked runtime dir. `spawn_door` starts the door through `spawn_detached`,
  with the `Pinned` program and its arguments. The door runs in a session of its
  own and is not the app's child: std's child, the intermediate, calls `setsid`,
  forks the door and writes the door's pid to a pipe. If that write fails, the
  intermediate SIGTERMs the door and exits non-zero. A door that ignores that
  SIGTERM runs unnamed. Otherwise the intermediate exits zero, and init adopts
  the door. The app reads the pid, pins the door as it arrives (`arrive`: its
  pidfd and start time), and then reaps the intermediate. The door's stdin is
  `/dev/null`, and its stdout and stderr are the door log. std's close-on-exec
  pipe reports an exec failure of the door as `spawn`'s error.
- A held child, from `Exec::follow`, runs the same argv and environment checks
  as a call, under no deadline. Its stdout is read on one thread, a line at a
  time, into a channel of 256 events. A line past `LINE_CAP` is read to its end
  and reported as `Dropped` with its length. Its stderr is captured on another
  thread, up to 16 MiB.

```
Exec::follow(words, flags, positionals)
  |
  +-- child_env, argv        as for a call
  |
  v
spawn_held                   stdin closed, stdout and stderr piped
  +-- open_pidfd             refused: the child is terminated and the call fails
  +-- read_lines thread      Line | Dropped | Eof, to a channel of 256
  +-- capture thread         stderr, up to 16 MiB
  |
  v
Follow::next(wait)           Some(event), or None when none came in time
Follow::end()                SIGINT through the pidfd; drains events until the
                             child's stdout closes or 5 s pass; waits 5 s for
                             the exit; returns Ended { status, rest, stderr }
Drop for Follow              SIGINT through the pidfd; reaps on a thread
```

- `terminate` sends SIGTERM to a pid. Its one production caller is `run`, on a
  child it has not reaped. Its refusals are in AGENTS.md.

## Tests

Its unit tests are in `src/lib.rs`. Its integration tests, for argv, detach,
env, exec, pins and token, are in the shell crate's `tests/`, beside the fake
programs they run in `crates/shell/src/bin`. The door tests there reach this
crate through `test_doors`.
