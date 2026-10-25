# aoide-desktop

The desktop app for Aoide: a Tauri 2 window over the orchestration the terminal
already drives. This directory is its own Cargo workspace. It is not a member
of `pkgs/aoide`'s workspace and keeps its own `Cargo.lock`, so GUI crates never
reach the core lock. Its crates are GUI-free libraries; the window is a later
crate.

## Crates

- `crates/door` (`aoide-desktop-door`): the client half of `eidolon web`, the
  loopback door a session serves. It is blocking HTTP/1.1 over
  `std::net::TcpStream`, with no async runtime, no HTTP library and no GUI
  crate. Its dependencies are serde, serde_json and libc.

## How the door composes

```
client.rs   Door::say, cancel, answer, events
              |
              v
client.rs   exchange(): read the token (token.rs), connect, write (http.rs)
              |
              v
http.rs     read_head, then the body reader: length, chunked, or to EOF
              |
     +--------+-----------------------+
     |                                |
  call()                          events()
  whole reply, capped at 64 KiB   Events (events.rs): SSE lines, 1 MiB cap
                                      |
                                      v
                                frame.rs decode: Frame, or Frame::Other
```

- `token.rs` reads the bearer token from its file under the token-file rules.
  `Token` prints no value.
- `events.rs` turns the stream into a blocking iterator of `Frame`s.
- `frame.rs` decodes one `data:` payload into a typed `Frame`.
- `fake.rs`, behind the `fake` feature, is a std-thread door on 127.0.0.1. It
  speaks the wire as far as the tests need and records every request.
