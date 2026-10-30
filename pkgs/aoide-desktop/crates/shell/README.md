# aoide-desktop-shell

The app's own doors (`doors.rs`), and the asks a door raises and the answers to
them (`asks.rs`). It reaches a process or a signal only through the exec crate,
which it re-exports as `exec`. It is the layer the window calls; the window is a
later crate.

Its dependencies are serde, serde_json, libc (for its flags and error numbers),
sha2 (a dialog's digest), the door crate, and the exec crate.

## Seams

- `exec`, the exec crate (`crates/exec/README.md`): the one crate that starts a
  process or sends a signal. The shell starts a process or sends a signal only
  through it.
- `doors.rs`: `Doors`, which keeps the app's record of its doors, launches one,
  re-attaches the records at start, watches each door's stream, and closes a
  door. Its watch threads send on a bounded channel of `EVENTS_BOUND` (64)
  events; `Doors::open` takes the sending half.
- `asks.rs`: `prepare_answer`, `send_answer`, and the `Dialog` a yes shows.
- `palette.rs`: `palette` lists the commands the main window offers, from
  `schema --json`, and `check` decides one call. `call` runs a checked call
  through `Exec::call`. `main_grants` is the main window's grant set: every
  path the schema declares, since the main window is the person's own. A frame
  passes its own narrower set. `FLOOR` lists the named classes the floor
  refuses. A command is offered only when the grant set names it and the floor
  does not hold it, and the floor is checked on every call, granted or not.
- `blocking.rs`: `PATHS`, the paths of the core commands that never end on
  their own (the serve family, the daemon, the conductor, the melete adapter,
  the watches, and trace `--follow`). They run only as held children, never
  through `palette::call`. The test in `tests/palette.rs` pins the list.
- `chat/`: `say` sends to a session, and `follow` opens its stream as a `Chat`
  whose `next` yields a `Push`. `chat/cases.rs` holds the follow cases, one
  entry each. `chat/record.rs` is the one place a stream line is parsed.
- `src/bin/fake-aoide.rs` and `src/bin/fake-eidolon.rs`, behind the `fakes`
  feature: the stand-ins the tests run in place of the installed programs.

## How a door is kept

```
doors.rs    Doors::open(exec, root, runtime, out)
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
              +-- verify                   cwd, pid, start time, cmdline, token
              +-- Store::update            the record, sessionId null
              +-- open_stream              hello's session fills the record
              +-- start_watch              one watch thread, holding its pidfd
```

- A failure up to and including `Store::update` refuses the launch. Its door is
  signalled through its pidfd and waited for up to 10 s, and the error names its
  pid and its log. Without a pidfd nothing is signalled, and the error says so.
  A launch that refuses the door blocks up to `BANNER_BOUND` + `EXIT_BOUND`
  (30 + 10 s), and a launch whose stream opens up to `BANNER_BOUND` +
  `CONNECT` + `IO` (30 + 5 + 30 s). The `Doors` lock is held that long, so
  another door's call waits behind the launch.
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
  The check is on each directory itself, not its parents. A store directory that
  is a link, dangling or not, refuses the open.
- The banner is read once through from the log's start, and both lines must lie
  in its first 64 KiB. A door that exits first, a banner past 64 KiB, an address
  that is not loopback, and a token path that is not graphic ASCII or not the
  token path of its port each refuse the launch at once. A banner not within
  30 s refuses it at that bound. Each refusal quotes the log's last 4 KiB, as
  data.
- A record is trusted only while `verify` passes. Its pid is live and not a
  zombie, with the recorded start time. Its command line is the record's
  `program` followed by the launch arguments for the recorded cwd, and its token
  file is the token path of its port. `open_stream` also needs the first frame
  to be `hello`, within the stream's deadline, and its session to be the
  recorded one when there is one. A stream whose hello does not come in time
  does not open.
- A door's identity is what its launch spawned. `launch` records the canonical
  path that `launch_plan` pinned as the record's `program`. The pin is checked
  at launch only, so a rebuild or a re-pin leaves a running door its own, and a
  door launched through a link stays its own when the link is re-pointed. A
  record without `program` does not parse, so the open is refused.
- A record whose pid is gone or a zombie, whose start time or command line
  differs, fails its identity check. It is dropped and
  never signalled, and `Event::Dropped` names its pid. Any other failure (a
  stream that does not open, a hello that fails, a working directory that moved,
  an I/O error) keeps the record. Both events carry `why` through `visible`.
- `Doors::open` returns its events in the list it returns, and sends none on the
  channel: one `Dropped` for each record that fails its identity check
  (removed), and one `Unwatched` for each record no watch could start for (kept,
  and the next open tries again). A removal that cannot be written is still
  reported, and the next open checks that record again; every record is read.
- A record whose check fails for any other reason is watched anyway, unattached.
  Its watch sends `Event::Unwatched` once per failure streak, then tries again
  every second (`REOPEN`) while its pidfd's process lives. The frames resume
  when a try works. An identity failure on a try ends the watch with `Dropped`.
- The watch holds the door's pidfd, which is opened before its checks, so the
  checks and any signal name the same process. It reads the stream, updates the
  shared state under one lock, keeps the door's pending asks, and reports each
  event through `deliver`: frames, `CloseDue`, `Revoked`, `AsksOverflow`,
  `Unwatched`, `Dropped` and `Gone`. It never signals a door.
- `deliver` waits for room. Each try takes the state lock, returns if the watch
  is stopped, and calls `try_send` under that lock. On a full channel the lock
  is dropped and the try is retried every 20 ms. `detach` sets `stop` under the
  same lock, so no event follows a detach, and a close never waits on a full
  channel. A full channel holds the watch on its current event, with its stream
  open, and nothing is dropped while a watch runs. The drain is the caller's
  contract: a caller that stops draining holds the watch on whatever event is
  next, so a stop now that waits for a turn's end runs its bound and reports
  `NoTurnEnd`.
- A door keeps at most 64 pending asks. An ask past that is neither kept nor
  forwarded, and the stream sends one `AsksOverflow`, naming the first ask it
  refused. It stays hidden until the stream reopens, when its replay offers it
  again.
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
  and returns `Close::Dropped { pid, why }`. Any other failure keeps the record,
  its watch and the mark, signals nothing, and returns
  `DoorsError::Unverified(why)`. Under the watch's lock, a turn that is not
  `Idle` is `Busy`, and the watch is left as it was, turn state included. An
  idle turn sets `closing` and shuts its stream. SIGTERM goes through the
  pidfd; a signal that fails, but for a door already gone, clears `closing` and
  returns its error, and the record, the watch and the mark stay. The exit is
  awaited for 10 s. An exit detaches the watch (2 s bound) and removes the
  record: `Exited`. A door still live clears `closing`, withdraws an armed mark,
  and keeps its record and its watch: `StillLive`. The wait never escalates.
- While `closing` stands, the watch takes its stream's end as the close's: it
  neither drops the door nor reconnects. Once `closing` clears, a stream that
  ended reopens, as any stream does while its door lives.
- A turn the door starts between the lock and the signal is cut. That is the
  residual of the close's order.
- `close_after_turn` closes an `Idle` door now, marks a `Running` one
  (`Armed`), and refuses an `Unknown` one. A marked door whose turn ends sends
  `CloseDue` once, through `deliver`. `close_due` closes it if the mark still
  stands; a turn still running keeps the mark (`StaysArmed`), and any other
  answer leaves the mark as the close left it. A close whose door outlives its
  signal withdraws the mark, whichever call made the close,
  `close_after_turn`'s idle arm included; a close that fails with an error
  leaves it, and the next turn end tries again. `revoke` withdraws a mark.
- A stream that ends withdraws an armed mark, and the watch sends
  `Event::Revoked { launch_id, why }`, with `why` through `visible`. A close
  holds the mark while it ends its own stream, and a door that is gone sends
  `Gone` instead, so neither withdraws it.
- Stop now re-checks the door before it cancels, so a door that fails the check
  gets no request. That check takes the close's split: an identity failure is
  `Close::Dropped`, and any other is `DoorsError::Unverified`. A 204 from the
  cancel waits up to 10 s for the turn to end, then the door closes. If the turn
  does not end, the result is `NoTurnEnd`: the door keeps running, its record
  stays, and nothing is signalled. A 409 says the door is idle, so the turn is
  taken as idle, and the door closes at once. A turn that outlives every cancel
  is never closed by the app: the user ends it outside the app. A stop now that
  reports `NoTurnEnd` or `Busy`, or fails with an error, leaves an armed mark
  standing; a close whose door outlives its signal withdraws it.
- A turn that starts or ends while no stream is open is not seen. After a
  reconnect the turn is `Unknown` until a frame arrives. A stream's end
  withdraws an armed mark, unless a close holds `closing`, so the mark does not
  stand across a reconnect. A stop now across a reconnect cannot see a turn that
  ended in the gap: its 204 waits the 10 s and reports `NoTurnEnd`. A second
  stop now closes when the door answers 409, as an idle door does.

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

## How the palette decides a call

```
palette::call(exec, grants, words, flags, positionals)
  check(exec.schema(), grants, words, flags)
    1. blocking::is_blocking(words)          -> Refusal::Blocking
    2. a --node or --typed flag              -> Refusal::Floored
    3. schema.command(words) is None         -> Refusal::Unknown
    4. the floor: a schema mark (approves,
       launches, internal, or not
       implemented), then FLOOR's prefixes   -> Refusal::Floored
    5. words not in grants                   -> Refusal::NotGranted
  exec.call(words, flags, positionals)       only when check passed
```

The grant set is the list of paths the window holds: `main_grants(schema)` for
the main window, every declared path, and a frame's own list. A granted command
that the floor holds is refused, so a grant can never name a floored command.

`palette::call` runs no confirm. A confirm before a palette run belongs to the
window that shows the palette (Part C); `call` is the check and the spawn only.

## How a chat is followed

```
chat::say(exec, id, text)
  exec.call(send, --id=<id>, --submit, --yes, -- <text>)
  --yes: the composer types it, so it is the person's own act
  the child's env is child_env's for a send: no session id, no autogate

chat::follow(exec, id)                    cases::CASES, in order
  for each case (trace, then watch):
    exec.follow(case.words, case.flags, [id])     a held child
    next(5 s)
      a line, or silence  -> Chat { case, pending }   this case answers
      Eof (stdout closed) -> end(); its stderr is the reason; next case
  every case refused      -> FollowError::Refused { reasons }

Chat::next(wait)  -> record::line(text)
                       a JSON object: Push::Record, strings spelled
                       any other text: Push::Other, spelled
                     Dropped(n) for a line past the exec crate's cap, n bytes
Chat::close()     -> SIGINT through the held child; the events after it
```

- `trace` runs `session trace --json --follow --tail=200 -- <id>`, and
  `watch` runs `session watch --json -- <id>`. A trace prints raw journal
  lines, not the one-shot's steps projection: projecting them is a later mode.
- A watch's first line is a JSON frame, and its other lines are text.

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

A held follow is `session trace --follow` or `session watch`, on the id it
names. An id starting `hook-` keeps no trace and no PTY, so both refuse; an id
starting `pty-` keeps no trace, so only `session watch` answers. Its first
line is a JSON frame. Each then prints the lines of the file
`FAKE_AOIDE_FOLLOW_FILE` names, one every 30 ms, and waits. SIGINT ends it: it
prints `{"ok":true,"followed":true}` and exits zero.
