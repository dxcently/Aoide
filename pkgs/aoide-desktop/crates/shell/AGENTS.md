# AGENTS.md: aoide-desktop-shell

The invariants to hold while editing this crate. The seams and the composition
are in README.md.

## The exec module (exec.rs)

- `exec.rs` is the only file that starts a process or sends a signal. The words
  `Command::new(`, `pre_exec`, `libc::fork`, `setsid`, `libc::kill`, `execv` and
  `posix_spawn` may appear only there, and so may the `unsafe {` opener, with
  one exception: the fake eidolon's signal wait. The grep gate in
  `crates/shell/tests/gates.rs` holds the workspace to that.
- `spawn_door` starts a door only from an absolute path. Its pre-exec closure
  makes only `setsid`, `fork`, `write` and `_exit`, which POSIX lists as
  async-signal-safe, and allocates nothing. `fork` runs glibc's atfork handlers,
  which take malloc's locks before the fork and release them on both sides. The
  closure runs in the intermediate, which has one thread, so those locks are
  free. `spawn_detached`'s doc says the same.
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
  an `eidolon` launch is refused, and an `aoide` child keeps the vault variables
  it inherited, since their names cannot be known. The config read that learns
  the names runs under the same rule.
- Each stream is kept up to 16 MiB. Past that, the child still runs to its end,
  the extra bytes are dropped, and the call is refused.
- A non-zero exit is not an error. `call` returns the child's status and its
  stderr, verbatim, for the caller to show.

## The doors (doors.rs)

- A door is signalled only by `close_now`, and only after `verify` passes at
  that moment. `verify` checks the pin and the working directory, the pid's
  start time and zombie state, its command line, and its token path. It opens
  no connection. `Doors::drop` signals nothing.
- A request under a door's token goes out only after `verify` (launch,
  re-attach, stop now, answer), or from the watch thread's reconnect. The
  reconnect checks the pid and its start time, the same check `live` makes, and
  nothing more.
- The watch thread only reads. It updates the shared state under one lock,
  keeps the door's pending asks, forwards each frame, and sends `CloseDue` when
  a marked door's turn has ended. The caller answers with `close_due`.
- The close is five steps: end the watch, re-check the door, SIGTERM, wait up to
  10 s on `/proc`, and remove the record. The wait never escalates. A door still
  live after 10 s keeps its record, and is no longer watched until the app next
  opens.
- A launch whose banner, identity or token path fails before the record is
  written keeps its door running, unrecorded and unsignalled, and its error
  names the pid and the log. A failed `hello` after the record is written leaves
  the record with a null session, and the next open checks it again.
- The turn state is known only from frames. A close needs `Idle` from the stream
  it has, so a turn that starts or ends in a gap between streams is not seen. A
  stop now takes the cancel's own answer: 409 means idle, and 204 waits for the
  frames that say the turn ended.
- A record file that does not parse is refused and never rewritten.
- The record and the door's log are created 0600 with `O_NOFOLLOW`, and the log
  with `create_new`. Their directories are made 0700.

## The asks (asks.rs)

- An answer goes only to the door whose stream carried its ask. `prepare_answer`
  refuses, with no request, when no watched door carries the ask, or the ask
  does not offer the label.
- `send_answer` runs `verify` before its request. It builds the dialog again
  from the stored ask, and sends only when that dialog's digest is the digest
  shown. A yes above the cap is never sent, and a changed ask is refused.
- An ask leaves `pending_asks` only when the door's `ask-settled` arrives. A
  204, 409, 422 or 401 leaves it listed.
- A stream's asks are cleared when it opens, and rebuilt from its replay.
- The dialog for a yes is the input as canonical JSON, each line through
  `visible`. Above 4 KiB or 60 lines it is the curl line for this door instead.
- The curl line never holds the token. It reads the token file with
  `$(cat ...)`, and every value is one POSIX shell word.

## The fakes and the tests

- The fake eidolon waits up to 20 s for its events stream to close after
  SIGTERM, then removes its token and exits 0. Past 20 s it exits 1 and leaves
  its token. A test that signals it ends the stream first.
- The fake aoide and the fake eidolon are bins with `required-features =
  ["fakes"]`. Their tests run with `--all-features`.

## Extension points

- A new aoide command needs no code. Its path, flags and arguments come from
  `aoide schema --json`.
- A new kind of child goes in `exec.rs`, with its own environment rule in
  `child_env`. If it needs vault keys, it is refused while their names are
  unknown.
- A new door command goes in `doors.rs`. It calls `verify` before any request
  under the token or any signal, and it has a test for each refusal.
