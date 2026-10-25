# aoide-desktop-door

The client half of `eidolon web`, the loopback door a session serves. The app
reaches a session through this crate and no other: it sends the door's four
routes, and it decodes what the door's events stream carries.

It is blocking HTTP/1.1 over `std::net::TcpStream`, with no async runtime, no
HTTP library and no GUI crate. Its dependencies are serde, serde_json and libc
(for the `O_NOFOLLOW` and `O_NONBLOCK` flags of the token open).

## Seams

- `client.rs`: `Door`, with `say`, `cancel`, `answer` and `events`. Each
  route's statuses map to its outcome type or to a `DoorError`.
- `events.rs`: `Events`, a blocking iterator of `Frame`s over the stream, and
  `Closer`, which ends the stream from another thread.
- `frame.rs`: `Frame`, `Ask` and the projections a transcript draws, and
  `decode` for one `data:` payload.
- `token.rs`: `runtime_from_env`, `check_runtime`, `token_path`, and the token
  read that every request makes.
- `listener.rs`: `check`, which refuses a connection whose answering socket
  another uid owns, and `decide`, the verdict on its owner.
- `http.rs`: the request writer, the reply head, the three body framings, and
  the one deadline a request's send and its reply share (`Within`, `Wire`).
- `error.rs`: `DoorError`, every way a call can fail.
- `fake.rs`, behind the `fake` feature: a std-thread door on 127.0.0.1 that
  speaks the wire as far as the tests need, and records every request.

## How the door composes

```
client.rs   Door::say, cancel, answer, events
              |
              v
client.rs   exchange(): read the token (token.rs), connect, check the
              |             listener (listener.rs), write the request
              v
http.rs     read_head under the deadline the send began, then the body reader:
              |             length, chunked, or to EOF
     +--------+-----------------------+
     |                                |
  call()                          events()
  whole reply, refused past       Events (events.rs): SSE lines, 1 MiB cap
  64 KiB                              |
                                      v
                                frame.rs decode: Frame, or Frame::Other
```

- `token.rs` reads the bearer token from its file under the token-file rules.
  `token_path` is the one spelling of `<runtime>/eidolon/web-<port>.token`, and
  `check_runtime` is the runtime dir's check. `read_token` uses both. `Token`
  prints no value.
- `listener.rs` finds the accepted socket's row in `/proc/net/tcp` (or
  `tcp6`) and reads its owner. The row is matched by its local address (the
  door) and its remote address (this client's), so it is the socket that
  answers this connection.
- `events.rs` turns the stream into a blocking iterator of `Frame`s, and hands
  out a `Closer` that shuts the stream's socket down from another thread.
- `frame.rs` decodes one `data:` payload into a typed `Frame`.
- `fake.rs` is a std-thread door on 127.0.0.1. A client that closes an open
  stream ends it, and `streaming()` says whether one is open.
