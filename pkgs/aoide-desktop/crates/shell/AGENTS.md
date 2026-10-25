# AGENTS.md: aoide-desktop-shell

The invariants to hold while editing this crate. The seams and the composition
are in README.md.

## The exec module (exec.rs)

- `exec.rs` is the only file that calls the process and signal APIs. The gate
  holds that: `pkgs/aoide-desktop/AGENTS.md` (The gate) says how. The words
  `Command::new(`, `pre_exec`, `libc::fork`, `setsid`, `libc::kill`, `execv` and
  `posix_spawn` appear only in `exec.rs`. The words `unsafe` and `unsafe_code`
  appear only in `exec.rs` and the fake eidolon's file. `disallowed_methods`,
  `disallowed_types`, `disallowed_method`, `disallowed_type`, `clippy::all` and
  `clippy::style` appear only in `exec.rs`, each matched as a whole word on the
  text with its ASCII whitespace removed, so `clippy::allow_attributes` is not
  `clippy::all`. The forms `include!(` and `#[path` appear in no source.
- `spawn_door` starts only the `Pinned` program it is given, at its canonical
  path. Its pre-exec closure makes only `setsid`, `fork`, `write`, `kill` and
  `_exit`, which POSIX lists as async-signal-safe, and allocates nothing. `fork`
  runs glibc's atfork handlers, which take malloc's locks before the fork and
  release them on both sides. The closure runs in the intermediate, which has
  one thread, so those locks are free. `spawn_detached`'s doc says the same. If
  the write of the pid fails, the closure sends SIGTERM to that pid. The door is
  the intermediate's unreaped child then, so the pid is its own. That is the one
  signal not sent through a pidfd.
- `spawn_door` returns the door as it arrived. `spawn_detached` opens the pidfd
  and reads the start time as the intermediate's pid comes off the pipe, before
  it reaps the intermediate. The window that is left runs from the write to that
  read: a door that exits and is reaped by init inside it can lose its pid to
  another process. A launch that then refuses SIGTERMs that process through the
  pidfd. No test reaches that window.
- Between fork and exec, in order: std's `pre_exec` makes std fork, and the
  child runs its atfork handlers; the closure then calls `setsid` and
  `libc::fork`, still in that one-threaded child; the grandchild returns from
  the closure and execs the door. libc 0.2.189, the version the desktop lock
  binds, has no `_Fork`, so the second fork is `libc::fork`, which is safe here
  because the child has one thread when the closure runs.
- The door's environment is `Exec::door_env`: `child_env(Child::Eidolon, "web",
  ...)`, with `XDG_RUNTIME_DIR` replaced by the checked runtime dir.
- Every child runs from a `Pinned` program. `check_pin` is the only maker of
  one, and `run` and `spawn_door` take one. `run` is private, and only `Exec`
  calls it. Do not add a second way in.
- `check_pin` checks the target and every directory above it, up to `/`: each
  is owned by root or this user, and none is writable by group or other, unless
  it is a directory with its sticky bit set. The target also has no setuid or
  setgid bit (`PinError::Owner`, `PinError::Writable`, `PinError::SetId`). The
  tests' scratch dirs lie under `TMPDIR`, so `TMPDIR`'s ancestors must pass the
  same rule. No test can make a directory another uid owns, so the owner branch
  is not tested (see Not tested).
- No shell, ever. The argv is a vector of words, never a string handed to a
  shell. A child runs by its absolute path with `env_clear()`.
- The argv is `[aoide, <path words>, --json, <flags>, (-- <positionals>)]`. A
  valued flag is `--name=value`. A bool flag is written bare, because aoide's
  parser never lets a declared bool take the next word. A bare `--` ends flag
  parsing, so `--` is written only when there are positionals.
- A call that does not match the schema is refused before anything runs: an
  unknown path, an undeclared flag, a bool given a value, a valued flag given
  none, a missing required argument, or a NUL byte in any word.
- An `eidolon` child loses `AOIDE_SESSION_ID`, every `AOIDE_MELETE_*` variable
  and every vault key variable. An `aoide` child loses every `AOIDE_MELETE_*`
  variable unless its first word is `melete`, and every vault key variable.
  Names are compared as bytes, so a name that is not UTF-8 is scrubbed too.
- The vault key names come from `aoide config --json`. While they are unknown,
  every call but `schema` and `config` is refused before a child starts, and an
  `eidolon` launch is refused too. The names are unknown when the config call
  fails or does not parse, or when a `tokenEnv` is not
  `[A-Za-z_][A-Za-z0-9_]*`, or names `PATH`, `HOME`, `XDG_RUNTIME_DIR` or
  `AOIDE_ROOT`. A bad `tokenEnv` is shown through `visible`. The pattern has no
  length bound and is case-sensitive, so a 10 KiB name and `path` pass, and
  `HOME=x` does not.
- Each stream is kept up to 16 MiB. Past that, the child still runs to its end,
  the extra bytes are dropped, and the call is refused.
- Each call runs under one deadline, 30 s, from the spawn to the end of the
  reading of its output. `Exec::deadline` can only shorten it: the shorter of
  the two is taken. A child still running at the deadline is sent SIGTERM and
  has 5 s to exit, and the call is refused as a timeout. A child that has
  exited, while a process it started still holds its output pipe, is refused at
  the same deadline, and no signal is sent to that process. That process keeps
  the call's two reader threads and their pipes until it exits; freeing them at
  the deadline is not implemented. A child that ignores SIGTERM for 5 s is left
  running and unreaped. Nothing sends SIGKILL: `Child::kill` is a clippy
  disallowed method, and the word is barred from every source.
- `open_pidfd` opens a pidfd for a pid, and `terminate_pidfd` sends SIGTERM
  through one, and only SIGTERM. Both refuse pid 0, pid 1, any pid past
  `i32::MAX`, and this process's own pid. A pidfd names one process, so the
  signal cannot reach a process that later takes the pid. Outside exec.rs's own
  tests, `arrive` and the door code in `doors.rs` are the only callers: a door
  is signalled through its pidfd, except for the pid signal in the spawn
  closure.
- `pidfd_exited` polls a pidfd with a zero timeout: it reports whether the
  process has ended, and never waits. Only the watch thread uses it.
- `terminate(pid)` sends SIGTERM to an unreaped child this process started, and
  refuses the same pids. Its one production caller is `run`, on the child it has
  not reaped, so the pid is that child's. The door tests reach it through
  `test_doors::terminate`.
- A non-zero exit is not an error. `call` returns the child's status and its
  stderr, verbatim, for the caller to show.

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
  through `deliver`, which waits for room, retrying each 20 ms until the channel
  takes it. A stopped watch sends nothing more, checked before each try. A full
  channel holds the watch and its stream; nothing is dropped while a watch runs.
  The drain is the caller's contract: a caller that stops draining holds the
  watch on its next event, so a stop now that waits for a turn's end runs its
  bound and reports `NoTurnEnd`.
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
  close holds `closing` or the door is gone. `close_due` withdraws the mark
  after any answer but `Busy`. Stop now does not: after a `StillLive` close it
  leaves the mark standing.
- A watch that ended is not watched. `launch_ids` leaves it out, and `watch`
  refuses it, so its calls send nothing.
- A drop or a forget whose store write fails keeps its record. The next open
  checks it again and reports it again.
- The close is ordered: a pidfd for the recorded pid, held by the watch; then
  `verify`, where an identity failure drops the record and its watch and returns
  `Close::Dropped`, and any other failure keeps both and returns
  `DoorsError::Unverified`, signalling nothing either way. Under the watch's
  lock, a turn that is not `Idle` is `Busy`, and the watch is left as it was,
  turn state included. An idle turn sets `closing` and shuts its
  stream. SIGTERM goes through the pidfd, and the exit is awaited for
  `EXIT_BOUND` (10 s). An exit detaches the watch (2 s bound) and removes the
  record: `Exited`. A door still live clears `closing` and keeps its record and
  its watch: `StillLive`. The wait never escalates.
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
- The stream's send, head and hello share the door's 30 s deadline, `IO`. The
  events stream holds it until the hello is read, so a door that answers 200 and
  sends no hello ends its stream at that deadline, and `open_stream` refuses it
  with the error named. A watch's reopen is bounded the same way (`CONNECT` +
  `IO`, 35 s).
- The turn state is known only from frames. `close_after_turn` refuses an
  `Unknown` turn. A stop now takes the cancel's own answer: 409 means idle, and
  204 waits up to 10 s for the frames that say the turn ended. If none come, the
  result is `NoTurnEnd`, and the door keeps running, its record stays, and
  nothing is signalled. After a reconnect the turn is `Unknown`, so a turn that
  ended in the gap is never seen: its stop now's 204 waits the same 10 s and
  reports `NoTurnEnd`. A second stop now closes when the door answers 409, as an
  idle door does, and a 204 waits again. A turn that outlives every cancel is
  never closed by the app: the user ends it outside the app. Stop now does not
  withdraw an armed mark, so after a `StillLive` close the mark stands.
- A failed hello at launch or re-attach leaves the record with a null session,
  and the watch checks it again on each try. A record with a null session takes
  a hello's session only at the next open: a watch's reconnect does not write
  it.
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
- `tests/token.rs` puts a FIFO at the token path with `exec::make_fifo`. The
  door crate forbids `unsafe`, so the FIFO is made here, in `exec.rs`.
- The door tests start and signal a door through `exec::test_doors`, behind
  `fakes`. It is the one way the tests reach `spawn_door` and `terminate`.
- A negative check ends with a sentinel request the fake records, and passes
  only if nothing came before the sentinel. For a request, the test lists what
  came first with `answers_before_sentinel`. For a SIGTERM,
  `no_sigterm_in_whole_log_once_sentinel_served` sends the sentinel, waits until
  the fake has served it, and asserts that the fake's whole log holds no
  SIGTERM. A door that was signalled and exited serves no sentinel, so that
  wait fails. Every positive wait is at least 5 s.
- The door's port comes from the ephemeral pool that other tests' clients also
  draw from, so a bind on that 127.0.0.1 port can fail with `AddrInUse` after
  the door dies. `loopback-2` serves the door on 127.0.0.2, which no client
  binds. The two tests that bind a dead door's address, the stop-now test and
  `an_answer_to_a_door_that_fails_its_checks_sends_no_request`, both use it.
- The fake eidolon's start-up modes are listed in README.md's fakes section.
- The two hostile children in `exec.rs`'s tests are shell scripts that the test
  writes, pins and runs through `run`: a grandchild that holds stdout, and a
  child that ignores SIGTERM. Each is bounded and exits by itself within 8 s.
- A test that runs a program it wrote writes it through a child: `write_program`
  runs `/bin/sh` to write the file. A concurrent test's fork would hold this
  process's writer open until its own exec, and the program's exec would then
  fail with ETXTBSY. The pid a program writes is polled for up to 5 s, since the
  call that runs the program can be refused first.

## Not tested

- The intermediate's failed pid write, which SIGTERMs the door and exits
  non-zero: no seam makes that write fail.
- `check_pin`'s owner branch, which refuses a target or directory owned by a uid
  that is neither root nor this user: no test can make a directory another uid
  owns. The mode rule is tested.
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
- A new kind of child goes in `exec.rs`, runs only a `Pinned` program, and has
  its own environment rule in `child_env`. If it needs vault keys, it is refused
  while their names are unknown.
- A new door command goes in `doors.rs`. It calls `verify` before any request
  under the token or any signal, and it has a test for each refusal.
