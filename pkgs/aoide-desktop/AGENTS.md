# AGENTS.md: aoide-desktop

The invariants to hold while editing this directory. The layout is in
README.md.

## The door (crates/door)

- Loopback only. `Door::new` refuses any address that is not 127.0.0.0/8 or
  `::1` before anything connects.
- The token is read on every request. The runtime directory and its `eidolon`
  directory must be real directories (no symlinks), owned by the effective uid,
  at mode 0700. The token file must be exactly
  `<runtime>/eidolon/web-<port>.token`, opened with `O_NOFOLLOW`, a regular file
  owned by the effective uid, at mode 0600, with one link and at most 4 KiB.
  It must be printable ASCII. Nothing is trimmed: the door writes no newline.
- A relative or empty `XDG_RUNTIME_DIR` counts as unset. There is no temp-dir
  fallback.
- The token travels only in `Authorization: Bearer`. It never goes in a URL, a
  log line, a `Debug` or `Display`, or an error.
- The routes are `POST /api/say`, `POST /api/cancel`, `POST /api/answer` and
  `GET /api/events`. Nothing calls `/api/launch`.
- Every request carries `Host` (the bound address) and `Connection: close`.
  A POST with a body carries `Content-Type: application/json` and its length.
  `cancel` sends no body, and a GET carries neither.
- The statuses are fixed. 401 is `Unauthorized`, 403 is `HostRefused` and 413
  is `TooLarge`. A route's own answers are `Ok` values: `SayOutcome`,
  `CancelOutcome` and `AnswerOutcome`. Any other status is `Unexpected`.
- A `data:` frame over 1 MiB is dropped and counted by `dropped_oversize()`,
  and decoding resumes at the next frame boundary. The line and frame buffers
  never exceed the cap.
- A `hello` whose protocol is not `PROTOCOL` ends the stream with
  `Protocol { got }`. An unknown frame type is `Frame::Other`. An unknown field
  is ignored. Neither is an error.
- No `unsafe` in this crate: `#![deny(unsafe_code)]` is on. The effective uid
  comes from `/proc/self/status`. The open and the stat use std.

## The exec module (crates/shell)

- `exec.rs` is the only file that starts a process. `Command::new(`, `unsafe {`
  and the `libc` calls live there. The grep gate in
  `crates/shell/tests/gates.rs` holds the rest of the workspace to that.
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
- The gate forbids `aoided.sock`, `dispatch`, `subscribe`, `/api/launch` and
  `sh -c` anywhere under `crates/`, and under `ui/` once it exists.

## Tests

- Use `fake` and the fake aoide only. Never the installed `eidolon` or `aoide`,
  and never a GUI. The fake aoide is a bin with `required-features = ["fakes"]`,
  so the exec tests need `--all-features`.
- The token, client and exec tests create directories under `TMPDIR`, so run
  them with `TMPDIR` set to a directory the tests may write.
- Every cargo command runs with `--offline`.

## Extension points

- A new route goes in `client.rs` with its status map. Teach `fake.rs` the
  route, and test each status.
- A new frame type goes in `frame.rs`: its name in `KNOWN`, a `Known` variant
  and a `Frame` variant, with a decode test. Until then it is `Frame::Other`.
- A new aoide command needs no code. Its path, flags and arguments come from
  `aoide schema --json`.
- A new kind of child goes in `exec.rs`, with its own environment rule in
  `child_env`. If it needs vault keys, it is refused while their names are
  unknown.
