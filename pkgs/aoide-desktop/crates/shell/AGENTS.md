# AGENTS.md: aoide-desktop-shell

The invariants to hold while editing this crate. The seams and the composition
are in README.md.

## The exec module (exec.rs)

- `exec.rs` is the only file that starts a process or sends a signal. The gate
  holds that: `pkgs/aoide-desktop/AGENTS.md` (The gate) says how. The words
  `Command::new(`, `pre_exec`, `libc::fork`, `setsid`, `libc::kill`, `execv` and
  `posix_spawn` appear only in `exec.rs`. The words `unsafe` and `unsafe_code`
  appear only in `exec.rs` and the fake eidolon's file. `disallowed_methods`,
  `disallowed_types`, `clippy::all` and `clippy::style` appear only in
  `exec.rs`, each matched as a whole word, so `clippy::allow_attributes` is not
  `clippy::all`.
- `spawn_door` starts only the `Pinned` program it is given, at its canonical
  path. Its pre-exec closure makes only `setsid`, `fork`, `write`, `kill` and
  `_exit`, which POSIX lists as async-signal-safe, and allocates nothing. `fork`
  runs glibc's atfork handlers, which take malloc's locks before the fork and
  release them on both sides. The closure runs in the intermediate, which has
  one thread, so those locks are free. `spawn_detached`'s doc says the same.
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
- `check_pin` refuses a file, or the directory it lies in, that group or other
  can write, and one owned by a uid that is neither root nor this user
  (`PinError::Writable`, `PinError::Owner`). No test can make a directory
  another uid owns, so the owner branch is not tested (see Not tested).
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
  is signalled through its pidfd and no other way.
- `pidfd_exited` polls a pidfd with a zero timeout: it reports whether the
  process has ended, and never waits. Only the watch thread uses it.
- `terminate(pid)` sends SIGTERM to an unreaped child this process started, and
  refuses the same pids. The caller checks the pid's start time and command line
  at that moment. The door tests reach it through `test_doors::terminate`.
- A non-zero exit is not an error. `call` returns the child's status and its
  stderr, verbatim, for the caller to show.

## The doors (doors.rs)

- A door is signalled only through its pidfd, by `close_now` (a close) or by
  `refuse` (a launch that fails before its record is written). `close_now` runs
  `verify` first, at that moment. `refuse` runs none: its pidfd was opened as
  the pid arrived, so it names the process that pid named then, whatever the
  banner says. `verify` checks the pin and the working directory, the pid's
  start time and zombie state, its command line, and its token path. It opens
  no connection. `Doors::drop` signals nothing.
- A request under a door's token goes out only after `verify` (launch,
  re-attach, stop now, answer), or from the watch thread's reconnect. The
  reconnect runs `verify` and the hello's session check before it uses the
  stream.
- The identity checks (pid gone or zombie, start time, command line, pin) drop a
  record and send `Event::Dropped`, which names the pid. Every other failure
  keeps the record and sends `Event::Unwatched` from its watch. Both events
  carry `why` through `visible`.
- `Doors::open` returns its events in a list and sends none on the channel.
  `launch` returns `DoorsError::Unwatched` for a record no watch could start
  for. Re-attach of a record whose check fails for another reason starts its
  watch unattached, so the watch reports the failure.
- The events channel is bounded at `EVENTS_BOUND` (64). Every watch event goes
  through `deliver`, which waits for room, retrying each 20 ms until the channel
  takes it or the watch stops. A full channel holds the watch and its stream;
  nothing is dropped while a watch runs.
- `Unwatched` is sent once per failure streak. The watch then tries again every
  `REOPEN` while its pidfd's process lives, and the frames resume when a try
  works. An identity failure on a try sends `Dropped` and ends the watch.
- A door keeps at most `ASKS_BOUND` (64) pending asks. An ask past that is
  neither kept nor forwarded, and the stream sends one `AsksOverflow` naming the
  first ask it refused.
- The watch thread reads the stream. It updates the shared state under one lock,
  keeps the door's pending asks, forwards each frame, and marks `CloseDue` when
  an armed door's turn ends. The caller answers `CloseDue` with `close_due`. The
  watch writes the record in two places only: an identity failure (`failed`) and
  a door that is gone (`forget`), each through `Store::update`.
- A watch that ended is not watched. `launch_ids` leaves it out, and `watch`
  refuses it, so its calls send nothing.
- A drop or a forget whose store write fails keeps its record. The next open
  checks it again and reports it again.
- The close is ordered: a pidfd for the recorded pid, held by the watch; then
  `verify`, where an identity failure drops the record and its watch and returns
  `Close::Dropped`, and any other failure keeps both and returns
  `DoorsError::Unverified`, signalling nothing either way. Under the watch's
  lock, a turn that is not `Idle` is `Busy`, and the watch is left as it was,
  turn state included. An idle turn marks the watch closing and shuts its
  stream. SIGTERM goes through the pidfd, and the exit is awaited for
  `EXIT_BOUND` (10 s). An exit detaches the watch (2 s bound) and removes the
  record: `Exited`. A door still live clears the mark and keeps its record and
  its watch: `StillLive`. The wait never escalates.
- While the closing mark stands, the watch takes its stream's end as the
  close's. It neither drops the door nor reconnects. A stream that ended reopens
  once the mark clears.
- The close's residual (decision 35): a turn the door starts between the lock
  and the signal is cut.
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
  SIGTERM, so a launch that refuses on its banner blocks up to 40 s.
- Once the stream's head is in, `open_stream` reads the hello with no deadline:
  the events stream drops the door's deadline once its head is read. A door that
  answers 200 and sends no hello holds the launch until it sends one or closes.
  The head itself is bounded by the door's 30 s deadline. No bound on the hello
  is built.
- The turn state is known only from frames. `close_after_turn` refuses an
  `Unknown` turn. A stop now takes the cancel's own answer: 409 means idle, and
  204 waits up to 10 s for the frames that say the turn ended. If none come, the
  result is `NoTurnEnd`, and the door keeps running, its record stays, and
  nothing is signalled. After a reconnect the turn is `Unknown`, so a turn that
  ended in the gap is never seen: its stop now's 204 waits the same 10 s and
  reports `NoTurnEnd`. A second stop now closes when the door answers 409, as an
  idle door does, and a 204 waits again.
- A failed hello at launch or re-attach leaves the record with a null session,
  and the watch checks it again on each try.
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
- A store directory that is a link is refused at open. A dangling one opens with
  no records, and its first write is refused.

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
  came first with `answers_before_sentinel`. For a SIGTERM, the test sends the
  sentinel with `no_signal_before_sentinel` and asserts the fake logged no
  SIGTERM before it. The fake logs a signal in order with the requests it
  serves, and a door that was signalled and exited serves no sentinel, so that
  wait fails. Every positive wait is at least 5 s.
- The door's port comes from the ephemeral pool that other tests' clients also
  draw from, so a bind on that 127.0.0.1 port can fail with `AddrInUse` after
  the door dies. `loopback-2` serves the door on 127.0.0.2, which no client
  binds, so the stop-now test binds there. The sibling test
  `an_answer_to_a_door_that_fails_its_checks_sends_no_request` still binds
  127.0.0.1 and carries the same risk.
- The fake eidolon's start-up modes are listed in README.md's fakes section.
- The two hostile children in `exec.rs`'s tests are shell scripts that the test
  writes, pins and runs through `run`: a grandchild that holds stdout, and a
  child that ignores SIGTERM. Each is bounded and exits by itself within 8 s.

## Not tested

- The intermediate's failed pid write, which SIGTERMs the door and exits
  non-zero: no seam makes that write fail.
- `check_pin`'s owner branch, which refuses a target or directory owned by a uid
  that is neither root nor this user: no test can make a directory another uid
  owns. The mode rule is tested.
- A reconnect's zombie, start time or command line failure: the same checks at
  open are tested, and a reconnect's pin change is tested. A door that dies
  under a watch is reaped by init, so no test holds its zombie, and a live pid's
  command line cannot change in a test.
- The owner check of the store's directory (`check_private`'s uid branch, for
  the store's directory and the directories made under it). No test can make a
  directory another uid owns. The link check is tested.
- The session fill and the watch's start failing after the record is written: no
  seam makes the store's update or the thread's spawn fail. The hello failing at
  launch is tested.
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
