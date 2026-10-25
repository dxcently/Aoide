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
  door. Its watch threads send on a bounded channel of `EVENTS_BOUND` (64)
  events; `Doors::open` takes the sending half.
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
  that later takes the pid. `doors.rs` opens one when a door's pid arrives, and
  signals a door only through it.
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
              +-- adopt                    pidfd and start time, as the pid arrives
              +-- await_banner             both banner lines, first 64 KiB, 30 s
              +-- verify                   pid, start time, cmdline, token
              +-- Store::update            the record, sessionId null
              +-- open_stream              hello's session fills the record
              +-- start_watch              one watch thread per door
```

- A failure up to and including `Store::update` refuses the launch. Its door is
  signalled through its pidfd and waited for up to 10 s, and the error names its
  pid and its log. Without a pidfd nothing is signalled, and the error says so.
- A failure after `Store::update` keeps the record and returns the launch id.
  `Event::Unwatched` says why. A failed hello leaves the record with a null
  session, and the next open checks it again.
- The record is `$AOIDE_ROOT/state/desktop/doors.json`, mode 0600. Each change
  is a read-modify-write under an exclusive lock on `doors.json.lock`, ending in
  a temp file, a sync, a rename and a directory sync. A file that does not parse
  is refused, and it is never rewritten. A link in its place is refused
  (`O_NOFOLLOW`). The directories are made 0700, and each is checked before use:
  not a link, owned by the euid, mode 0700.
- The banner is read once through from the log's start, and both lines must lie
  in its first 64 KiB. A door that exits first, a banner not within 30 s, a
  banner past 64 KiB, an address that is not loopback, and a token path that is
  not graphic ASCII or not the token path of its port each refuse the launch at
  once. Each refusal quotes the log's last 4 KiB, as data.
- A record is trusted only while `verify` passes. Its pid is live and not a
  zombie, with the recorded start time. Its command line is the launch argv for
  the current pin and the recorded cwd, and its token file is the token path of
  its port. `open_stream` also needs the first frame to be `hello`, and its
  session to be the recorded one when there is one.
- A record whose pid is gone or a zombie, whose start time or command line
  differs, or whose pin changed, is dropped and never signalled, and
  `Event::Dropped` names its pid. Any other failure (a stream that does not open,
  a hello that fails, an I/O error) keeps the record unwatched, sends
  `Event::Unwatched`, and the next `Doors::open` tries again. Both events carry
  `why` through `visible`.
- The watch thread only reads. It keeps the stream's asks, forwards each frame on
  the caller's channel with `try_send`, and never signals a door. A full channel
  ends that stream, and the stream the watch opens next replays the asks still
  pending: only display frames are lost, and the log keeps them. A `CloseDue`
  the channel refuses stays owed, and the next frame sends it. A terminal notice
  (`Gone`, `Dropped`, `Unwatched`) is retried every 20 ms until it is taken or the
  watch stops. A door keeps at most 64 pending asks; an ask past that is not
  kept, and the stream sends one `AsksOverflow`.
- A stream that ends while the door lives reopens after a second. A stream that
  ends with the door gone drops the record and sends `Gone`. A watch that ended
  is not watched: `launch_ids` leaves it out, and its calls are refused. An
  unwatched door whose record is kept is re-attached by the next open.
- The turn state is `Unknown` until a `turn-state` frame. The wire sends one
  only when the turn changes, and a new stream carries none.
- A close, in order: a pidfd for the recorded pid; `verify`, where a failure
  drops the record and its watch and signals nothing; under the watch's lock, a
  turn that is not `Idle` is `Busy`, and the watch is left as it was, turn state
  included; an idle turn marks the watch closing and shuts its stream; SIGTERM
  through the pidfd; then the exit is awaited for 10 s. An exit detaches the
  watch (2 s bound) and removes the record: `Exited`. A door still live clears
  the mark and keeps its record and its watch: `StillLive`. The wait never
  escalates.
- While the closing mark stands, the watch takes its stream's end as the close's:
  it neither drops the door nor reconnects. Once the mark clears, a stream that
  ended reopens, as any stream does while its door lives.
- A turn the door starts between the lock and the signal is cut. That is the
  residual of the close's order.
- `close_after_turn` closes an `Idle` door now, marks a `Running` one (`Armed`),
  and refuses an `Unknown` one. A marked door whose turn ends sends `CloseDue`
  once, and `close_due` closes it if the mark still stands; a turn still running
  keeps the mark (`StaysArmed`). `revoke` withdraws a mark.
- Stop now re-checks the door before it cancels, so a door that fails the check
  gets no request. A 204 from the cancel waits up to 10 s for the turn to end,
  then the door closes. If the turn does not end, the result is `NoTurnEnd`: the
  door keeps running, its record stays, and nothing is signalled. A 409 says the
  door is idle, so the turn is taken as idle, and the door closes at once.
- A turn that starts or ends while no stream is open is not seen. After a
  reconnect the turn is `Unknown` until a frame arrives. A mark stands across a
  reconnect until it is revoked or its door closes.

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
`idle`, `end`, `cancel-409`, `ask-approval ID JSON`, `settle ID LABEL`,
`answer-409`, `answer-422`, `answer-401`, `flood N`, `asks N`, `hello-bad`,
`hello-good` and `session NAME`. Its header says what each one does. The mode
file `DIR/.fake-mode`, read once at start, picks a start-up: `exit-first`,
`no-banner`, `remote-address`, `token-outside`, `token-space`, `banner-128k`,
`ignore-sigterm` (the first SIGTERM is ignored, the second exits),
`cancel-stuck` (a cancel answers 204 and keeps the turn running), or
`hello-bad`. It logs each control word applied, each request served (an answer
with its body), and each SIGTERM it receives, to `DIR/.fake-log`, in order.

`src/bin/fake-aoide.rs` prints the fixture schema for `schema --json` and the
fixture config for `config --json`. `FAKE_AOIDE_CONFIG` replaces the config
text, and `FAKE_AOIDE_NO_SCHEMA` or `FAKE_AOIDE_NO_CONFIG` makes that call
print nothing. Any other call sleeps `FAKE_AOIDE_SLEEP` seconds, writes
`FAKE_AOIDE_STDOUT_BYTES` bytes to stdout, the text of `FAKE_AOIDE_STDERR` and
`FAKE_AOIDE_STDERR_BYTES` bytes to stderr, and exits with `FAKE_AOIDE_EXIT`.
Each call is appended to the file `FAKE_AOIDE_RECORD` names.
