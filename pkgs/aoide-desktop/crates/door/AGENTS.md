# AGENTS.md: aoide-desktop-door

The invariants to hold while editing this crate. The seams and the composition
are in README.md.

## Invariants

- Loopback only. `Door::new` refuses any address that is not 127.0.0.0/8 or
  `::1` before anything connects.
- The listener is this user's. After the connect and before a byte is written,
  `listener::check` finds the accepted socket's row in `/proc/net/tcp` (`tcp6`
  for an IPv6 door): its local address is the door's, its remote address is
  this connection's local address. Its owner, the row's eighth column, must be
  the effective uid. Another uid, or no row after 50 tries 10 ms apart, is
  `Unowned` and nothing is written. The check proves the listener runs as this
  user. It does not prove the process is `eidolon`: a same-uid listener passes
  (ruling 29).
- The token is read on every request. The runtime directory and its `eidolon`
  directory must be real directories (no symlinks), owned by the effective uid,
  at mode 0700. The token file must be exactly
  `<runtime>/eidolon/web-<port>.token`, a regular file owned by the effective
  uid, with no group or other bits, one link, and at most 4 KiB; a larger file
  is refused, not truncated. It must be graphic ASCII, with no space. Nothing
  is trimmed: the door writes no newline. The open is `O_NOFOLLOW | O_NONBLOCK`,
  so a FIFO is refused by the stat and never waited on.
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
- A reply over 64 KiB is refused, not cut short: `call` returns `Malformed`, and
  a `Content-Length` over 64 KiB is refused before its body is read. An error
  status (400 and up) is decided by its head: a reset that cuts its body short
  leaves the status, so a 413 is `TooLarge`.
- A head or a trailer block holds at most 100 header lines and 64 KiB in all,
  and a line at most 16 KiB. Over either cap is `Malformed`. A chunk size that
  overflows is `Malformed`, and so is a body shorter than its length.
- A call's head and body share one 30 s deadline: each read's timeout is the
  time left. An events stream's head has 30 s, and its body none once the head
  is read. The connect has 5 s.
- A write that fails is reported only if no reply came: a door may answer 413
  before it has read the body.
- A `data:` frame over 1 MiB is dropped and counted by `dropped_oversize()`,
  and decoding resumes at the next frame boundary. The line and frame buffers
  never exceed the cap.
- The first frame of an events stream is `hello`, and its `protocol` is
  exactly the number 1. It is read as a JSON value, so a string, a float or a
  number past u32 is refused. A first frame that is not `hello` is
  `Malformed`; a `protocol` other than 1 is `Protocol { got }`. Each ends the
  stream, and so does a second `hello` (`Malformed`). An unknown frame type is
  `Frame::Other`. An unknown field is ignored. Neither is an error.
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

## Not tested

- The other-uid refusals: a listener of another uid, and a token file of
  another uid. Tests never run as root, so no file or socket of another uid
  can be made. The parser tests in `listener.rs` cover another uid's row.
- The FIFO refusal is tested in the shell crate, which can make a FIFO through
  `exec`'s fakes-only helper. This crate forbids `unsafe`.

## The fake door

- The fake door ends a stream its client has closed, as hyper does, so a test
  of a close sees the same drain the real door has.
- `refuse_at_once` answers before the body is read and closes with the body
  unread. The kernel resets such a connection: the reply's head arrives, and
  its body may not, so the client decides an error by its head.

## Extension points

- A new route goes in `client.rs` with its status map. Teach `fake.rs` the
  route, and test each status.
- A new frame type goes in `frame.rs`: its name in `KNOWN`, a `Known` variant
  and a `Frame` variant, with a decode test. Until then it is `Frame::Other`.

## Docs that need an update in the same commit

- A change to a route, a status, a frame, or the token rule updates README.md
  and this file in the same commit, and `fake.rs` when the fake must speak it.
