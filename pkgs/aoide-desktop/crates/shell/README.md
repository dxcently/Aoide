# aoide-desktop-shell

The one place the app starts a process or sends a signal (`exec.rs`), the app's
own doors (`doors.rs`), and the asks a door raises and the answers to them
(`asks.rs`). It is the layer the window calls; the window is a later crate.

Its dependencies are serde, serde_json, libc, sha2 (a dialog's digest), the
door crate, and `aoide-protocol` by path (for `is_unsafe` and `visible`).

## Seams

- `exec.rs`: `check_pin`, which returns a `Pinned` (the only maker of one),
  `Schema`, `argv`, `child_env`, `Exec` (`load`, `call`, `deadline`,
  `door_env`, `spawn_door`), `terminate`, and the pidfd helpers `open_pidfd`
  and `terminate_pidfd`. Only this file starts a process or sends a signal.
  Behind `fakes`, `make_fifo` makes the FIFO the token test puts at the token
  path, and `test_doors` starts and signals a door for the door tests.
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
              |                  not under <home>/.local/bin, and neither the
              |                  file nor its directory writable by group or
              |                  other, owned by root or this user
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
  executable regular file, and any target under `<home>/.local/bin`. It refuses
  a file, or the directory it lies in, that group or other can write, and one
  owned by a uid that is neither root nor this user (`PinError::Writable` and
  `PinError::Owner`). It returns a `Pinned` of the canonical path, which is the
  only path the app runs.
- `argv` builds the argument list from `aoide schema --json`. It refuses, before
  anything runs, an unknown path, an undeclared flag, a bool flag given a value,
  a valued flag given none, a missing required argument, and a NUL byte.
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
- `terminate` sends SIGTERM to a pid. Its refusals and the caller's check are
  in AGENTS.md.

## How a door is kept

```
doors.rs    Doors::open(exec, eidolon, home, root, runtime, out)
              |                    -> (Doors, Vec<Event>)
              +-- runtime_from_env, check_runtime  refused before any read
              +-- Store::load                      doors.json, or refused
              +-- reattach, per record             pidfd, verify, watch
              |
              v
            launch(plan)                 plan: launch_plan (pin and cwd)
              |
              +-- spawn_door               detached, its log under state/
              +-- adopt                    pidfd and start time at arrival
              +-- await_banner             both banner lines, first 64 KiB, 30 s
              +-- verify                   pid, start time, cmdline, token
              +-- Store::update            the record, sessionId null
              +-- open_stream              hello's session fills the record
              +-- start_watch              one watch thread, holding its pidfd
```

- A failure up to and including `Store::update` refuses the launch. Its door is
  signalled through its pidfd and waited for up to 10 s, and the error names its
  pid and its log. Without a pidfd nothing is signalled, and the error says so.
  A launch that refuses the door blocks up to `BANNER_BOUND` + `EXIT_BOUND`
  (30 + 10 s), and a launch whose stream opens up to `BANNER_BOUND` + `CONNECT` +
  `IO` (30 + 5 + 30 s). The `Doors` lock is held that long, so another door's
  call waits behind the launch.
- A launch that has written its record keeps the record and returns the launch
  id. A stream that does not open at launch starts the watch unattached, and the
  watch reports it. A launch for which no watch can start returns
  `DoorsError::Unwatched { launch_id, why }`. Its record stays, and the next
  open re-attaches it. Nothing goes on the channel from `launch`.
- The record is `$AOIDE_ROOT/state/desktop/doors.json`, mode 0600. Each change
  is a read-modify-write under an exclusive lock on `doors.json.lock`, ending in
  a temp file, a sync, a rename and a directory sync. A file that does not parse
  is refused, and it is never rewritten. A link in its place is refused
  (`O_NOFOLLOW`). The store's directory and the directories made under it are
  made 0700 and checked before use: not a link, owned by the euid, mode 0700.
  The check is on each directory itself, not its parents.
- The banner is read once through from the log's start, and both lines must lie
  in its first 64 KiB. A door that exits first, a banner past 64 KiB, an address
  that is not loopback, and a token path that is not graphic ASCII or not the
  token path of its port each refuse the launch at once. A banner not within
  30 s refuses it at that bound. Each refusal quotes the log's last 4 KiB, as
  data.
- A record is trusted only while `verify` passes. Its pid is live and not a
  zombie, with the recorded start time. Its command line is the launch argv for
  the current pin and the recorded cwd, and its token file is the token path of
  its port. `open_stream` also needs the first frame to be `hello`, within the
  stream's deadline, and its session to be the recorded one when there is one.
  A stream whose hello does not come in time does not open.
- A record whose pid is gone or a zombie, whose start time or command line
  differs, or whose pin changed, fails its identity check. It is dropped and
  never signalled, and `Event::Dropped` names its pid. Any other failure (a
  stream that does not open, a hello that fails, a working directory that moved,
  an I/O error) keeps the record. Both events carry `why` through `visible`.
- `Doors::open` returns its events in the list it returns, and sends none on the
  channel: one `Dropped` for each record that fails its identity check
  (removed), and one `Unwatched` for each record no watch could start for (kept,
  and the next open tries again).
- A record whose check fails for any other reason is watched anyway, unattached.
  Its watch sends `Event::Unwatched` once per failure streak, then tries again
  every second (`REOPEN`) while its pidfd's process lives. The frames resume
  when a try works. An identity failure on a try ends the watch with `Dropped`.
- The watch holds the door's pidfd, which is opened before its checks, so the
  checks and any signal name the same process. It reads the stream, updates the
  shared state under one lock, keeps the door's pending asks, and reports each
  event through `deliver`: frames, `CloseDue`, `AsksOverflow`, `Unwatched`,
  `Dropped` and `Gone`. It never signals a door.
- `deliver` waits for room. It retries `try_send` every 20 ms until the channel
  takes the event or the watch stops, so nothing is dropped while a watch runs.
  A full channel holds the watch on its current event, with its stream open.
- A door keeps at most 64 pending asks. An ask past that is neither kept nor
  forwarded, and the stream sends one `AsksOverflow`, naming the first ask it
  refused.
- A stream that ends while the door lives reopens after a second, and its hello
  is bounded as a launch's is: `CONNECT` + `IO` (35 s) at most. A stream that
  ends with the door gone drops the record and sends `Gone`. A drop or a forget
  whose store write fails keeps its record, and the next open checks it again
  and reports it again.
- A watch that ended is not watched: `launch_ids` leaves it out, and its calls
  are refused. The next open re-attaches every record that is kept.
- The turn state is `Unknown` until a `turn-state` frame. The wire sends one
  only when the turn changes, and a new stream carries none.
- A close, in order: a pidfd for the recorded pid, held by the watch; then
  `verify`. An identity failure drops the record and its watch, signals nothing,
  and returns `Close::Dropped { pid, why }`. Any other failure keeps the record
  and its watch, signals nothing, and returns `DoorsError::Unverified(why)`.
  Under the watch's lock, a turn that is not `Idle` is `Busy`, and the watch is
  left as it was, turn state included. An idle turn marks the watch closing and
  shuts its stream. SIGTERM goes through the pidfd, and the exit is awaited for
  10 s. An exit detaches the watch (2 s bound) and removes the record: `Exited`.
  A door still live clears the mark and keeps its record and its watch:
  `StillLive`. The wait never escalates.
- While the closing mark stands, the watch takes its stream's end as the
  close's: it neither drops the door nor reconnects. Once the mark clears, a
  stream that ended reopens, as any stream does while its door lives.
- A turn the door starts between the lock and the signal is cut. That is the
  residual of the close's order.
- `close_after_turn` closes an `Idle` door now, marks a `Running` one (`Armed`),
  and refuses an `Unknown` one. A marked door whose turn ends sends `CloseDue`
  once, through `deliver`. `close_due` closes it if the mark still stands; a
  turn still running keeps the mark (`StaysArmed`). `revoke` withdraws a mark.
- Stop now re-checks the door before it cancels, so a door that fails the check
  gets no request. That check takes the close's split: an identity failure is
  `Close::Dropped`, and any other is `DoorsError::Unverified`. A 204 from the
  cancel waits up to 10 s for the turn to end, then the door closes. If the turn
  does not end, the result is `NoTurnEnd`: the door keeps running, its record
  stays, and nothing is signalled. A 409 says the door is idle, so the turn is
  taken as idle, and the door closes at once.
- A turn that starts or ends while no stream is open is not seen. After a
  reconnect the turn is `Unknown` until a frame arrives. A mark stands across a
  reconnect until it is revoked or its door closes. A stop now across a
  reconnect cannot see a turn that ended in the gap: its 204 waits the 10 s and
  reports `NoTurnEnd`. A second stop now closes when the door answers 409, as an
  idle door does.

## How an ask is answered

```
asks.rs     Doors::prepare_answer(launch, ask, label)
              |  refused unless this door's stream carries the ask,
              |  and the ask offers the label
              v
            Prepared, with the dialog a yes shows (none for a no)
              |
              v
            Doors::send_answer(prepared, shown: Shown)
              +-- verify                the door's checks; a failure sends
              |                         nothing
              +-- dialog again          from the stored ask, not the screen
              +-- asks::permits         what was shown must match: a yes
              |                         under its digest, a capped yes only
              |                         as Shown::Pane
              +-- POST /api/answer      204 Answered, 409 NotPending,
                                        422 Rejected with the reason,
                                        401 or other: an error
```

- The asks a stream carries are kept per door, by id. Each `ask` adds one, each
  `ask-settled` removes it, and a new stream clears them before its replay.
- A yes on an approval shows the call as `visible_json` text: one object holding
  `callId`, `tool`, `prompt`, `reason` (only when it is set) and `input`,
  pretty, keys sorted at every depth, and every hidden character spelled as a
  JSON escape. That text is the dialog, and its SHA-256 is its digest. Above 4
  KiB or 60 lines the dialog is capped. A same-id ask that the door sends again
  replaces the stored one; a change to any of those fields gives it a new
  digest, so a yes confirmed on the earlier text is refused.
- A yes binds the call that was shown, under ruling 163. The residual: a door
  that changes its pending ask after the check in `send_answer` and before the
  answer lands applies the yes to the change. The shell cannot see that change,
  so only Eidolon can close it.
- What the app showed decides a yes (`Shown`). `None` is for a no and for a
  question's answer. `Confirm(digest)` is the native confirm, which offers no
  yes above the cap. `Pane(digest)` is the app's own pane, which shows the whole
  input and takes a capped yes too. A yes is sent only under the digest of the
  dialog built now from the stored ask. Every refusal comes before any request.
- The curl line answers a no or a question's answer from a terminal. It is
  never built for an approval's yes. Its body is the answer as
  `visible_json_line`, and it reads the token from the token file, so it names
  no token.
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
`idle`, `end`, `cancel-409`, `ask-approval ID JSON`, `ask-tool ID TOOL JSON`,
`settle ID LABEL`, `answer-409`, `answer-422`, `answer-401`, `events-500`,
`flood N`, `asks N`, `hello-bad`, `hello-good` and `session NAME`. Its header
says what each one does. The mode file `DIR/.fake-mode`, read once at start,
picks a start-up: `exit-first`, `no-banner`, `remote-address`, `token-outside`,
`token-space`, `banner-128k`, `ignore-sigterm` (the first SIGTERM is ignored,
the second exits), `cancel-stuck` (a cancel answers 204 and keeps the turn
running), `hello-bad`, or `loopback-2` (the door serves on 127.0.0.2, whatever
`--bind` names). It logs each control word applied, each request served (an
answer with its body), and each SIGTERM it receives, to `DIR/.fake-log`, in
order.

`src/bin/fake-aoide.rs` prints the fixture schema for `schema --json` and the
fixture config for `config --json`. `FAKE_AOIDE_CONFIG` replaces the config
text, and `FAKE_AOIDE_NO_SCHEMA` or `FAKE_AOIDE_NO_CONFIG` makes that call
print nothing. Any other call sleeps `FAKE_AOIDE_SLEEP` seconds, writes
`FAKE_AOIDE_STDOUT_BYTES` bytes to stdout, the text of `FAKE_AOIDE_STDERR` and
`FAKE_AOIDE_STDERR_BYTES` bytes to stderr, and exits with `FAKE_AOIDE_EXIT`.
Each call is appended to the file `FAKE_AOIDE_RECORD` names.
