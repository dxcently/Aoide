# AGENTS.md: aoide-desktop-door

The invariants to hold while editing this crate. The seams and the composition
are in README.md.

## Invariants

- Loopback only. `Door::new` refuses any address that is not 127.0.0.0/8 or
  `::1` before anything connects.
- The token is read on every request. The runtime directory and its `eidolon`
  directory must be real directories (no symlinks), owned by the effective uid,
  at mode 0700. The token file must be exactly
  `<runtime>/eidolon/web-<port>.token`, opened with `O_NOFOLLOW`, a regular file
  owned by the effective uid, with no group or other bits, one link, and at most
  4 KiB. It must be graphic ASCII, with no space. Nothing is trimmed: the door
  writes no newline.
- A relative or empty `XDG_RUNTIME_DIR` counts as unset. There is no temp-dir
  fallback. The runtime dir passes `runtime_from_env` and then `check_runtime`
  before any path is built from it. The token path is `token_path`, and
  `read_token` uses that same function.
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
- A reply over 64 KiB is refused, not cut short: `call` returns `Malformed`.
- A `data:` frame over 1 MiB is dropped and counted by `dropped_oversize()`,
  and decoding resumes at the next frame boundary. The line and frame buffers
  never exceed the cap.
- A `hello` whose protocol is not `PROTOCOL` ends the stream with
  `Protocol { got }`. An unknown frame type is `Frame::Other`. An unknown field
  is ignored. Neither is an error.
- No `unsafe` in this crate: `#![deny(unsafe_code)]` is on. The effective uid
  comes from `/proc/self/status`. The open and the stat use std.
- The crate's dependency tree names none of tauri, wry, tao, gtk, webkit2gtk,
  glib, soup, tokio or hyper (`cargo tree -p aoide-desktop-door`).
- A stream is closed through its `Closer`, from any thread. Dropping `Events`
  is not a close: the thread reading an idle stream stays blocked on it, and
  `Closer::close` shuts both halves down so that read returns. The client must
  close its own stream before it signals a door. Eidolon's graceful shutdown
  waits on every open connection (`crates/web/src/serve.rs`, `run`), and its
  stream ends only on a goodbye or a closed channel. hyper 1.11.1 ends a
  connection on read EOF mid-response only with half-close on
  (`src/proto/h1/conn.rs:487-501`, off by default), and Eidolon builds the
  plain `http1::Builder::new()` (`serve.rs:122`). So a SIGTERM to a door with
  an attached stream waits, and its token is removed only after `run` returns
  (`crates/web/src/lib.rs`).

## The fake door

- The fake door ends a stream its client has closed, as hyper does, so a test
  of a close sees the same drain the real door has.

## Extension points

- A new route goes in `client.rs` with its status map. Teach `fake.rs` the
  route, and test each status.
- A new frame type goes in `frame.rs`: its name in `KNOWN`, a `Known` variant
  and a `Frame` variant, with a decode test. Until then it is `Frame::Other`.

## Docs that need an update in the same commit

- A change to a route, a status, a frame, or the token rule updates README.md
  and this file in the same commit, and `fake.rs` when the fake must speak it.
