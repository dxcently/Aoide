# aoide-desktop-shell

The one place the app starts a process or sends a signal (`exec.rs`), the app's
own doors (`doors.rs`), and the asks a door raises and the answers to them
(`asks.rs`). It is the layer the window calls; the window is a later crate.

Its dependencies are serde, serde_json, libc, sha2 (a dialog's digest), the
door crate, and `aoide-protocol` by path (for `is_unsafe` and `visible`).

## Seams

- `exec.rs`: `check_pin`, `Schema`, `argv`, `child_env`, `Exec` (`load`,
  `call`, `deadline`, `door_env`, `spawn_door`), `terminate`, and the pidfd
  helpers `open_pidfd` and `terminate_pidfd`. Only this file starts a process
  or sends a signal. Behind `fakes`, `make_fifo` makes the FIFO the token test
  puts at the token path, and `test_doors` starts and signals a door for the
  door tests.
- `doors.rs`: `Doors`, which keeps the app's record of its doors, launches one,
  re-attaches the records at start, watches each door's stream, and closes a
  door.
- `asks.rs`: `prepare_answer`, `send_answer`, and the `Dialog` a yes shows.
- `src/bin/fake-aoide.rs` and `src/bin/fake-eidolon.rs`, behind the `fakes`
  feature: the stand-ins the tests run in place of the installed programs.

## How a child starts

```
exec.rs     Exec::load(aoide, home, env)
              |
              +-- check_pin      absolute, a regular file this user can run,
              |                  not under <home>/.local/bin
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
  executable regular file, and any target under `<home>/.local/bin`. It returns
  the canonical path, which is the only path the app runs.
- `argv` builds the argument list from `aoide schema --json`. It refuses, before
  anything runs, an unknown path, an undeclared flag, a bool flag given a value,
  a valued flag given none, a missing required argument, and a NUL byte.
- `child_env` builds the environment from the app's own, minus the variables
  the child must not see. An `eidolon` child loses `AOIDE_SESSION_ID`, the
  `AOIDE_MELETE_*` variables and the vault keys. An `aoide` child loses the
  `AOIDE_MELETE_*` variables unless its first word is `melete`, and the vault
  keys.
- The vault key names are the `tokenEnv` names in `aoide config --json`. While
  they are unknown, every call but `schema` and `config` is refused before a
  child starts. The names are unknown when the config call fails or does not
  parse, or when a `tokenEnv` is not `[A-Za-z_][A-Za-z0-9_]*`, or names `PATH`,
  `HOME`, `XDG_RUNTIME_DIR` or `AOIDE_ROOT`. A bad `tokenEnv` is shown through
  `visible`.
- `run` is private. Only `Exec` reaches it, and only with the pinned path.
- Each call runs under one deadline, 30 s unless `Exec::deadline` shortens it.
  It covers the spawn, the child's exit and the reading of its output. A child
  still running at the deadline is sent SIGTERM and has 5 s to exit, and the
  call is refused as a timeout. A child that exits while a process it started
  still holds its output pipe is refused at the same deadline, with no signal.
  Nothing sends SIGKILL.
- `open_pidfd` opens a pidfd for a pid, and `terminate_pidfd` sends SIGTERM
  through one. A pidfd names one process, so the signal cannot reach a process
  that later takes the pid. No caller in this crate uses them.
- `door_env` is `child_env` for `eidolon web`, with `XDG_RUNTIME_DIR` set to the
  checked runtime dir. `spawn_door` starts a door through `spawn_detached`. The
  door runs in a session of its own and is not the app's child: std's child,
  the intermediate, calls `setsid`, forks the door and writes the door's pid to
  a pipe. If that write fails, the intermediate SIGTERMs the door and exits
  non-zero, so no door runs unnamed. Otherwise it exits zero, and init adopts
  the door. The app reads the pid and reaps the intermediate. The door's stdin
  is `/dev/null`, and its stdout and stderr are the door log. std's
  close-on-exec pipe reports an exec failure of the door as `spawn`'s error.
- `terminate` sends SIGTERM. It refuses pid 0 and any pid past `i32::MAX`. The
  caller checks the pid's start time and command line at that moment.

## How a door is kept

```
doors.rs    Doors::open(exec, eidolon, home, root, runtime, out)
              |
              +-- runtime_from_env, check_runtime  refused before any read
              +-- Store::load                      doors.json, or refused
              +-- reattach, per record             verify, open_stream, watch
              |
              v
            launch(plan)                 plan: launch_plan (pin and cwd)
              |
              +-- spawn_door               detached, its log under state/
              +-- await_banner             both banner lines, 30 s
              +-- verify                   pid, start time, cmdline, token
              +-- Store::update            the record, sessionId null
              +-- open_stream              hello's session fills the record
              +-- start_watch              one watch thread per door
```

- The record is `$AOIDE_ROOT/state/desktop/doors.json`, mode 0600. Each change
  is a read-modify-write under an exclusive lock on `doors.json.lock`, ending in
  a temp file, a sync, a rename and a directory sync. A file that does not parse
  is refused, and it is never rewritten.
- A record is trusted only while `verify` passes. Its pid is live and not a
  zombie, with the recorded start time. Its command line is the launch argv for
  the current pin and the recorded cwd, and its token file is the token path of
  its port. `open_stream` also needs the first frame to be `hello`, and its
  session to be the recorded one when there is one. A record that fails a check
  is dropped and never signalled, even when its pid is live. A record whose pin
  changed fails the command line check, so its door runs on, and the app keeps
  no record of it.
- The watch thread only reads. It keeps the stream's asks, forwards each frame
  on the caller's channel, and never signals a door. A stream that ends while
  the door lives reopens after a second. A stream that ends with the door
  gone drops the record and sends `Gone`.
- The turn state is `Unknown` until a `turn-state` frame. The wire sends one
  only when the turn changes, and a new stream carries none.
- A close reads the turn the stream last showed, ends the watch (2 s bound), and
  re-checks the door. A door that fails the re-check is dropped, and nothing is
  signalled. A turn not known to be idle refuses the close with `Busy`, and the
  door is watched again. Otherwise the close sends SIGTERM, waits up to 10 s for
  the pid to exit, and removes the record. A door still live after 10 s keeps
  its record, and is no longer watched until the app next opens.
- Stop now re-checks the door before it cancels, so a door that fails the check
  gets no request. A 204 from the cancel waits up to 10 s for the turn to end,
  then the door closes. A 409 says the door is idle, so the turn is taken as
  idle, and the door closes at once.
- A turn that starts or ends while no stream is open is not seen. After a
  reconnect the turn is `Unknown` until a frame arrives. A close after the turn
  is refused, a marked door stays marked, and a stop now closes on its 409.

## How an ask is answered

```
asks.rs     Doors::prepare_answer(launch, ask, label)
              |  refused unless this door's stream carries the ask,
              |  and the ask offers the label
              v
            Prepared, with the dialog a yes shows (none for a no)
              |
              v
            Doors::send_answer(prepared, shown)
              +-- verify                the door's checks; a failure sends
              |                         nothing
              +-- dialog again          from the stored ask, not the screen
              +-- digest == shown       else refused, nothing sent
              +-- capped yes            refused, nothing sent
              +-- POST /api/answer      204 Answered, 409 NotPending,
                                        422 Rejected with the reason,
                                        401 or other: an error
```

- The asks a stream carries are kept per door, by id. Each `ask` adds one, each
  `ask-settled` removes it, and a new stream clears them before its replay.
- A yes on an approval is confirmed on a dialog that shows the whole input: the
  input as canonical JSON, keys sorted at every depth, each line through
  `visible`. Above 4 KiB or 60 lines the dialog offers no yes, and shows the
  curl line for this door instead. That line reads the token from its file,
  names no token, and carries the label prepared, not the input.
- A question's answer, and a no, skip the confirm. Only an approval's `yes`
  approves, since eidolon gives its consent on that label alone.
- A 204 leaves the ask listed until the door's `ask-settled` arrives. A 409
  means nothing is pending under that id, a 422 refuses with the door's reason,
  and a 401 or any other status is a `DoorError`. None of them removes the ask.
- Between streams the last stream's asks stay listed, so an answer to one the
  door settled in the gap comes back 409.

## The fakes

`src/bin/fake-eidolon.rs` takes `web --bind 127.0.0.1:0 --cwd DIR` and nothing
else. A test drives it with control words in `DIR/.fake-turn`: `running`,
`idle`, `end`, and `cancel-409`, which queues one 409 for the next cancel. It
raises an approval with `ask-approval ID JSON`, and settles one with
`settle ID LABEL`. `answer-409`, `answer-422` and `answer-401` each queue one
answer of that status. It logs each control word applied, each request served
(an answer with its body), and the SIGTERM it receives, to `DIR/.fake-log`, in
order.

`src/bin/fake-aoide.rs` prints the fixture schema for `schema --json` and the
fixture config for `config --json`. `FAKE_AOIDE_CONFIG` replaces the config
text, and `FAKE_AOIDE_NO_SCHEMA` or `FAKE_AOIDE_NO_CONFIG` makes that call
print nothing. Any other call sleeps `FAKE_AOIDE_SLEEP` seconds, writes
`FAKE_AOIDE_STDOUT_BYTES` bytes to stdout, the text of `FAKE_AOIDE_STDERR` and
`FAKE_AOIDE_STDERR_BYTES` bytes to stderr, and exits with `FAKE_AOIDE_EXIT`.
Each call is appended to the file `FAKE_AOIDE_RECORD` names.
