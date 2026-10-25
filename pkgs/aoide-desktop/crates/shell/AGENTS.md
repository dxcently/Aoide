# AGENTS.md: aoide-desktop-shell

The invariants to hold while editing this crate. The seams and the composition
are in README.md.

## The exec module (exec.rs)

- `exec.rs` is the only file that starts a process or sends a signal. Clippy
  holds that: `clippy.toml` at `pkgs/aoide-desktop/` lists the paths, the
  workspace denies the lints that check them, so a call through an alias of one
  is an error. The text scan in `crates/shell/tests/gates.rs` is the backstop.
  The words `Command::new(`, `pre_exec`, `libc::fork`, `setsid`, `libc::kill`,
  `execv` and `posix_spawn` appear only in `exec.rs`, and the word `unsafe`
  only in `exec.rs` and the fake eidolon's file.
- `spawn_door` starts a door only from an absolute path. Its pre-exec closure
  makes only `setsid`, `fork`, `write`, `kill` and `_exit`, which POSIX lists as
  async-signal-safe, and allocates nothing. `fork` runs glibc's atfork handlers,
  which take malloc's locks before the fork and release them on both sides. The
  closure runs in the intermediate, which has one thread, so those locks are
  free. `spawn_detached`'s doc says the same.
- Between fork and exec, in order: std's `pre_exec` makes std fork, and the
  child runs its atfork handlers; the closure then calls `setsid` and
  `libc::fork`, still in that one-threaded child; the grandchild returns from
  the closure and execs the door. libc 0.2.189, the version the desktop lock
  binds, has no `_Fork`, so the second fork is `libc::fork`, which is safe here
  because the child has one thread when the closure runs.
- The door's environment is `Exec::door_env`: `child_env(Child::Eidolon, "web",
  ...)`, with `XDG_RUNTIME_DIR` replaced by the checked runtime dir.
- Every child runs from a pinned path. `run` is private, and only `Exec` calls
  it, after `check_pin`. Do not add a second way in.
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
- The vault key names come from `aoide config --json`. While they are unknown,
  every call but `schema` and `config` is refused before a child starts, and an
  `eidolon` launch is refused too. The names are unknown when the config call
  fails or does not parse, or when a `tokenEnv` is not
  `[A-Za-z_][A-Za-z0-9_]*`, or names `PATH`, `HOME`, `XDG_RUNTIME_DIR` or
  `AOIDE_ROOT`. A bad `tokenEnv` is shown through `visible`.
- Each stream is kept up to 16 MiB. Past that, the child still runs to its end,
  the extra bytes are dropped, and the call is refused.
- Each call runs under one deadline, 30 s unless `Exec::deadline` shortens it,
  from the spawn to the end of the reading of its output. A child still running
  at the deadline is sent SIGTERM and has 5 s to exit, and the call is refused
  as a timeout. A child that has exited, while a process it started still holds
  its output pipe, is refused at the same deadline, and no signal is sent. A
  child that ignores SIGTERM for 5 s is left running and unreaped. Nothing
  sends SIGKILL: `Child::kill` is a clippy disallowed method, and the word is
  barred from every source.
- `open_pidfd` opens a pidfd for a pid, and `terminate_pidfd` sends SIGTERM
  through one, and only SIGTERM. A pidfd names one process, so the signal
  cannot reach a process that later takes the pid. Outside exec.rs's own tests,
  the door code in `doors.rs` is the only caller: a door is signalled through
  its pidfd and no other way.
- A non-zero exit is not an error. `call` returns the child's status and its
  stderr, verbatim, for the caller to show.

## The doors (doors.rs)

- A door is signalled only through its pidfd, by `close_now` (a close) or by
  `refuse` (a launch that fails before its record is written). Each runs
  `verify` first, at that moment. `verify` checks the pin and the working
  directory, the pid's start time and zombie state, its command line, and its
  token path. It opens no connection. `Doors::drop` signals nothing.
- A request under a door's token goes out only after `verify` (launch, re-attach,
  stop now, answer), or from the watch thread's reconnect. The reconnect runs
  `verify` and the hello's session check before it uses the stream.
- The identity checks (pid gone or zombie, start time, command line, pin) drop a
  record and send `Event::Dropped`, which names the pid. Every other failure
  keeps the record and sends `Event::Unwatched`; the next `Doors::open` tries
  again. Both events carry `why` through `visible`.
- The events channel is bounded at `EVENTS_BOUND` (64). Frames and asks go with
  `try_send`. A full channel ends that stream, and the next stream replays the
  asks still pending, so only display frames are lost. `CloseDue` stays owed
  until the channel takes it. Terminal notices (`Gone`, `Dropped`, `Unwatched`)
  retry every 20 ms until taken or the watch stops. A notice sent from the
  caller's thread, at open or at launch, is one `try_send`: past the bound it is
  not sent, and the record's own change still stands.
- A door keeps at most `ASKS_BOUND` (64) pending asks. An ask past that is not
  kept, and the stream sends one `AsksOverflow`.
- The watch thread only reads. It updates the shared state under one lock, keeps
  the door's pending asks, forwards each frame, and sets `due` when an armed
  door's turn ends. The caller answers `CloseDue` with `close_due`.
- A watch that ended is not watched. `launch_ids` leaves it out, and `watch`
  refuses it, so its calls send nothing.
- The close is ordered: a pidfd for the recorded pid; `verify`, where a failure
  drops the record and its watch and signals nothing; under the watch's lock, a
  turn that is not `Idle` is `Busy`, and the watch is left as it was, turn state
  included; an idle turn marks the watch closing and shuts its stream; SIGTERM
  through the pidfd; the exit is awaited for `EXIT_BOUND` (10 s). An exit detaches
  the watch (2 s bound) and removes the record: `Exited`. A door still live clears
  the mark and keeps its record and its watch: `StillLive`. The wait never
  escalates.
- While the closing mark stands, the watch takes its stream's end as the close's.
  It neither drops the door nor reconnects. A stream that ended reopens once the
  mark clears.
- The close's residual (decision 35): a turn the door starts between the lock and
  the signal is cut.
- A launch that fails up to and including `Store::update` signals its door
  through the pidfd, waits up to 10 s, and returns an error that names its pid
  and its log. A launch that fails after `Store::update` keeps the record, returns
  the launch id, and sends `Event::Unwatched`.
- The banner is read once through from the log's start, both lines in its first
  64 KiB. A door that exits first, a banner not within 30 s, a banner past 64 KiB,
  an address that is not loopback, or a token path that is not graphic ASCII or
  not its port's token path, each refuses the launch at once. Each refusal quotes
  the log's last 4 KiB, as data.
- The turn state is known only from frames. `close_after_turn` refuses an
  `Unknown` turn. A stop now takes the cancel's own answer: 409 means idle, and
  204 waits up to 10 s for the frames that say the turn ended. If none come, the
  result is `NoTurnEnd`, and the door keeps running, its record stays, and nothing
  is signalled.
- A failed hello after the record is written leaves the record with a null
  session, and the next open checks it again.
- A record file that does not parse is refused and never rewritten. A record file
  that is a link is refused.
- The record and the door's log are created 0600 with `O_NOFOLLOW`, and the log
  with `create_new`. Their directories are made 0700, and checked before use.

## The asks (asks.rs)

- An answer goes only to the door whose stream carried its ask. `prepare_answer`
  refuses, with no request, when no watched door carries the ask, or the ask
  does not offer the label.
- `send_answer` runs `verify` before its request, builds the dialog again from
  the stored ask, and sends only when `asks::permits` accepts what was shown: no
  dialog for an answer that has none, and for a yes the digest of the dialog
  built now. A capped yes is taken only as `Shown::Pane`; under `Shown::Confirm`
  it is refused. Every refusal comes before any request.
- The dialog is the input as `visible_json`: pretty, keys sorted at every depth,
  and every character `visible` would spell written as a JSON `\u` escape. The
  cap is 4 KiB or 60 lines of that text. The dialog is never a curl line.
- The stream's JSON is Eidolon's own serialization of its value, so the dialog
  shows the input the door sent. That value holds no duplicate key and no
  number outside what serde_json keeps, so the stream is read without a
  duplicate-key check and no code does one. The sorting and the spelling are
  `aoide-protocol`'s, which `asks.rs` calls.
- `curl_line` returns None for an approval's yes. Its body is `visible_json_line`
  of the answer, so it holds no raw character `visible` would spell. The token
  path is quoted raw: the launch refuses a token path that is not graphic ASCII.
- The curl line never holds the token. It reads the token file with `$(cat ...)`,
  and every value is one POSIX shell word.
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
- A negative check ("no request reached the door") ends with a sentinel request
  the fake records, and passes only if nothing came before the sentinel. Every
  positive wait is at least 5 s.
- The fake eidolon's start-up modes are listed in README.md's fakes section.

## Not tested

- A child that exits while a grandchild holds its output pipe: the deadline
  holds the call, but no test can start the grandchild. The text gate keeps
  `Command::new(` in `exec.rs`, so no fake or test can start one.
- The intermediate's failed pid write, which SIGTERMs the door and exits
  non-zero: no seam makes that write fail.
- A child that ignores SIGTERM past the 5 s bound: the fake aoide exits on
  SIGTERM, so the left-running path never runs.
- A reconnect's zombie, start time or command line failure: the same checks at
  open are tested, and a reconnect's pin change is tested. A door that dies under
  a watch is reaped by init, so no test holds its zombie, and a live pid's command
  line cannot change in a test.
- The session fill and the watch's start failing after the record is written: no
  seam makes the store's update or the thread's spawn fail. The hello failing at
  launch is tested.
- A record write that fails before the record is written, which refuses the
  launch: no seam makes the rename fail in a directory the test's uid owns.

## Extension points

- A new aoide command needs no code. Its path, flags and arguments come from
  `aoide schema --json`.
- A new kind of child goes in `exec.rs`, with its own environment rule in
  `child_env`. If it needs vault keys, it is refused while their names are
  unknown.
- A new door command goes in `doors.rs`. It calls `verify` before any request
  under the token or any signal, and it has a test for each refusal.
