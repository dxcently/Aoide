# AGENTS.md: aoide-desktop-shell

The invariants to hold while editing this crate. The seams and the composition
are in README.md.

## The exec crate

- The shell reaches a process or a signal only through `exec`, the exec crate
  (`crates/exec/AGENTS.md`). The workspace forbids `unsafe`,
  `disallowed_methods` and `disallowed_types` in this crate, its fake programs
  included, so no attribute here lifts them.
- The rules for a child, its pinned program, argv, environment and deadline,
  are the exec crate's. That crate's AGENTS.md states them.

## The doors (doors.rs)

- A door is signalled only through its pidfd, by `close_now` (a close) or by
  `refuse` (a launch that fails before its record is written). `close_now` runs
  `verify` first, at that moment. `refuse` runs none: its pidfd was opened as
  the pid arrived, so it names the process that pid named then, whatever the
  banner says. `verify` checks the working directory, the pid's start time and
  zombie state, its command line against the record's `program`, and its token
  path. It opens no connection. `Doors::drop` signals nothing.
- A door's identity is what its launch spawned. `launch` records the canonical
  path `launch_plan` pinned as `program`, and `verify` requires the command line
  to be that program with `launch_args` for the recorded cwd. `check_pin` runs
  at launch only, so a rebuild or a re-pin leaves a running door its own. A
  record without `program` does not parse, so the open is refused.
- A request under a door's token goes out only after `verify` (launch,
  re-attach, stop now, answer), or from the watch thread's reconnect. The
  reconnect runs `verify` and the hello's session check before it uses the
  stream.
- The identity checks (pid gone or zombie, start time, command line) drop a
  record and send `Event::Dropped`, which names the pid. Every other failure
  keeps the record and sends `Event::Unwatched` from its watch. Both events
  carry `why` through `visible`.
- `Doors::open` returns its events in a list and sends none on the channel.
  `launch` returns `DoorsError::Unwatched` for a record no watch could start
  for. Re-attach of a record whose check fails for another reason starts its
  watch unattached, so the watch reports the failure. The loop has no `?`: a
  failed removal of a dropped record is ignored, its `Dropped` is still listed,
  and every later record is still read.
- The events channel is bounded at `EVENTS_BOUND` (64). Every watch event goes
  through `deliver`, which waits for room. Each try takes the state lock, checks
  `stop`, and calls `try_send` under that lock; a full channel is retried each
  20 ms with the lock dropped. `detach` sets `stop` under the same lock, so no
  event follows a detach. No test reaches the window between the stop check and
  `try_send`, which the lock holds shut: a change that drops the lock between
  them passes every test. A full channel holds the watch and its stream; nothing
  is dropped while a watch runs. The drain is the caller's contract: a caller
  that stops draining holds the watch on its next event, so a stop now that
  waits for a turn's end runs its bound and reports `NoTurnEnd`.
- `Unwatched` is sent once per failure streak. The watch then tries again every
  `REOPEN` while its pidfd's process lives, and the frames resume when a try
  works. An identity failure on a try sends `Dropped` and ends the watch.
- A door keeps at most `ASKS_BOUND` (64) pending asks. An ask past that is
  neither kept nor forwarded, and the stream sends one `AsksOverflow` naming the
  first ask it refused. It stays hidden until the stream reopens, when its
  replay offers it again.
- The watch thread reads the stream. It updates the shared state under one lock,
  keeps the door's pending asks, forwards each frame, and marks `CloseDue` when
  an armed door's turn ends. The caller answers `CloseDue` with `close_due`. The
  watch writes the record in two places only: an identity failure (`failed`) and
  a door that is gone (`forget`), each through `Store::update`.
- A stream's end withdraws an armed mark and sends `Event::Revoked`, unless a
  close holds `closing` or the door is gone. A close whose door outlives its
  signal (`StillLive`) withdraws an armed mark, whichever call made the close,
  so the door is not asked again. A close that fails with an error leaves the
  mark, and the next turn end tries again. A stop now that sees no turn end, or
  whose close finds `Busy`, leaves the mark: the turn it was set for still runs.
- A watch that ended is not watched. `launch_ids` leaves it out, and `watch`
  refuses it, so its calls send nothing.
- A drop or a forget whose store write fails keeps its record. The next open
  checks it again and reports it again.
- The close is ordered: a pidfd for the recorded pid, held by the watch; then
  `verify`, where an identity failure drops the record and its watch and
  returns `Close::Dropped`, and any other failure keeps both, and the mark, and
  returns `DoorsError::Unverified`, signalling nothing either way. Under the
  watch's lock, a turn that is not `Idle` is `Busy`, and the watch is left as
  it was, turn state included. An idle turn sets `closing` and shuts its stream.
  SIGTERM goes through the pidfd; a signal that fails, but for a door already
  gone, clears `closing` and returns its error, and the record, the watch and
  the mark stay. The exit is awaited for `EXIT_BOUND` (10 s). An exit detaches
  the watch (2 s bound) and removes the record: `Exited`. A store write that
  fails on an exit keeps the record and the watch, clears `closing`, and
  returns the error, with the mark as it was. A door still live
  clears `closing`, withdraws an armed mark, and keeps its record and its watch:
  `StillLive`. The wait never escalates.
- While `closing` stands, the watch takes its stream's end as the close's. It
  neither drops the door nor reconnects. A stream that ended reopens once
  `closing` clears.
- The close's residual: it checks the turn under the lock and signals after it,
  so a turn the door starts between the two is cut.
- A launch that fails up to and including `Store::update` signals its door
  through the pidfd, waits up to 10 s, and returns an error that names its pid
  and its log. A launch that has written its record keeps it: a stream that does
  not open starts the watch unattached, and a launch for which no watch can
  start returns `DoorsError::Unwatched`.
- The banner is read once through from the log's start, both lines in its first
  64 KiB. A door that exits first, a banner past 64 KiB, an address that is not
  loopback, or a token path that is not graphic ASCII or not its port's token
  path refuses the launch at once. A banner not within 30 s refuses it at that
  bound. Each refusal quotes the log's last 4 KiB, as data.
- A refusal waits up to `EXIT_BOUND` (10 s) for the door's exit after its
  SIGTERM. A launch that refuses the door blocks up to `BANNER_BOUND` +
  `EXIT_BOUND` (30 + 10 s), and a launch whose stream opens blocks up to
  `BANNER_BOUND` + `CONNECT` + `IO` (30 + 5 + 30 s). The `Doors` lock is held
  that long, so another door's call waits behind the launch.
- The stream's send, head and every read until its hello share the door's 30 s
  deadline, `IO`. The events stream holds it until the hello is read, so a door
  that answers 200 and sends no hello, or drips one, ends its stream at that
  deadline, and `open_stream` refuses it with the error named. That keeps the
  launch bounds above. A watch's reopen is bounded the same way (`CONNECT` +
  `IO`, 35 s).
- The turn state is known only from frames. `close_after_turn` refuses an
  `Unknown` turn. Its idle arm re-reads the turn under the lock: a turn that
  started since the read arms the mark (`Armed`), so no request is refused for a
  turn that is running. The fakes-only `race_turn_between_locks` is the test
  seam for that window. A stop now takes the cancel's own answer: 409 means idle, and
  204 waits up to 10 s for the frames that say the turn ended. If none come,
  the result is `NoTurnEnd`, and the door keeps running, its record stays, and
  nothing is signalled. After a reconnect the turn is `Unknown`, so a turn that
  ended in the gap is never seen: its stop now's 204 waits the same 10 s and
  reports `NoTurnEnd`. A second stop now closes when the door answers 409, as
  an idle door does, and a 204 waits again. A turn that outlives every cancel
  is never closed by the app: the user ends it outside the app. A stop now that
  reports `NoTurnEnd` or `Busy`, or fails with an error, leaves an armed mark
  standing; a close whose door outlives its signal withdraws it.
- A failed hello at launch or re-attach leaves the record with a null session.
  A reconnect checks the door's identity and reads its hello, and takes no
  session from it, so a record with a null session takes a hello's session only
  at the next open.
- A record file that does not parse is refused and never rewritten. A record
  file that is a link is refused.
- The record and the door's log are created 0600 with `O_NOFOLLOW`, and the log
  with `create_new`. The store's directory and the directories made under it are
  made 0700 and checked before use: not a link, owned by the euid, mode 0700.
- The check is on each directory itself. `make_private_dir` walks up from the
  directory it is given to the first one that exists, and checks only that one
  for a link; nothing above it is checked. The one-descriptor store that would
  close that gap is not built: it opens the store's directory once with
  `O_NOFOLLOW | O_DIRECTORY`, checks it with `fstat`, and `openat`s the record,
  its lock and its temp file.
- A store directory that is a link is refused at open, dangling or not: `load`
  reads its metadata without following it, and the lock refuses it.

## The asks (asks.rs)

- An answer goes only to the door whose stream carried its ask. `prepare_answer`
  refuses, with no request, when no watched door carries the ask, or the ask
  does not offer the label.
- `send_answer` runs `verify` before its request, builds the dialog again from
  the stored ask, and sends only when `asks::permits` accepts what was shown: no
  dialog for an answer that has none, and for a yes the digest of the dialog
  built now. A capped yes is taken only as `Shown::Pane`; under `Shown::Confirm`
  it is refused. Every refusal comes before any request.
- The dialog is the call as `visible_json`: one object of `callId`, `tool`,
  `prompt`, `reason` (only when set) and `input`, pretty, keys sorted at every
  depth, and every character `visible` would spell written as a JSON `\u`
  escape. The cap is 4 KiB or 60 lines of that text. The dialog is never a curl
  line.
- A same-id ask the door sends again replaces the stored one, and a change to
  any of its call's fields gives it a new digest. A yes confirmed on the earlier
  text is refused, since `send_answer` builds the dialog again from the stored
  ask. Residual: a door that changes its pending ask after that check and before
  the answer lands applies the yes to the change; only Eidolon can close that.
- The stream's JSON is Eidolon's own serialization of its value, so the dialog
  shows the input the door sent. That value holds no duplicate key and no
  number outside what serde_json keeps, so the stream is read without a
  duplicate-key check and no code does one. The sorting and the spelling are
  `aoide-protocol`'s, which `asks.rs` calls.
- `curl_line` returns None for an approval's yes. Its body is
  `visible_json_line` of the answer, so it holds no raw character `visible`
  would spell. Every value in the line, the token path included, is one
  single-quoted POSIX shell word (`sq`). The launch refuses a token path that is
  not graphic ASCII.
- The curl line never holds the token. It reads the token file with
  `$(cat ...)`.
- An ask leaves `pending_asks` only when the door's `ask-settled` arrives. A
  204, 409, 422 or 401 leaves it listed.
- A stream's asks are cleared when it opens, and rebuilt from its replay.

## The fakes and the tests

- The fake eidolon waits up to 20 s for its events stream to close after
  SIGTERM, then removes its token and exits 0. Past 20 s it exits 1 and leaves
  its token. A test that signals it ends the stream first.
- The fake aoide and the fake eidolon are bins with `required-features =
  ["fakes"]`. Their tests run with `--all-features`.
- The FIFO and the door helpers the tests use, `make_fifo` and `test_doors`,
  are the exec crate's, and its AGENTS.md says what each is for.
- A test that runs a program it wrote writes it through a child: exec's tests
  use `write_program`, and the door tests use `test_doors::copy_program`. A
  writer this process holds open is inherited by a concurrent test's fork, and
  that test's exec of the program fails with ETXTBSY.
- A negative check ends with a sentinel request the fake records, and passes
  only if nothing came before the sentinel. For a request, the test lists what
  came first with `answers_before_sentinel`. For a SIGTERM,
  `no_sigterm_in_whole_log_once_sentinel_served` sends the sentinel, waits until
  the fake has served it and 200 ms more, then asserts that the fake's whole log
  holds no SIGTERM. A door that was signalled and exited serves no sentinel, so
  that wait fails. Every positive wait is at least 5 s.
- The door's port comes from the ephemeral pool that other tests' clients also
  draw from, so a bind on that 127.0.0.1 port can fail with `AddrInUse` after
  the door dies. `loopback-2` serves the door on 127.0.0.2, which no client
  binds. The two tests that bind a dead door's address, the stop-now test and
  `an_answer_to_a_door_that_fails_its_checks_sends_no_request`, both use it.
- The fake eidolon's start-up modes are listed in README.md's fakes section.

## Not tested

- A reconnect's zombie, start time or command line failure: the same checks at
  open are tested. A door that dies under a watch is reaped by init, so no test
  holds its zombie, and a live pid's command line cannot change in a test.
- The owner check of the store's directory (`check_private`'s uid branch, for
  the store's directory and the directories made under it). No test can make a
  directory another uid owns. The link check is tested.
- The session fill and the watch's start failing after the record is written: no
  seam makes the store's update or the thread's spawn fail. The hello failing at
  launch is tested.
- `open`'s write of a dropped record failing, which `open` ignores: no seam
  makes a store write fail between `load` and that write, so its loop is
  untested.
- A record write that fails before the record is written, which refuses the
  launch: no seam makes the rename fail in a directory the test's uid owns.

## Extension points

- A new aoide command needs no code. Its path, flags and arguments come from
  `aoide schema --json`.
- A new door command goes in `doors.rs`. It calls `verify` before any request
  under the token or any signal, and it has a test for each refusal.
