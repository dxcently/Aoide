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
- `crates/shell` (`aoide-desktop-shell`): the exec module, the one place the
  app starts a process. It runs the aoide commands the app makes, pinned and
  scrubbed. Its dependencies are serde, serde_json and libc.

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

## How a child starts

```
exec.rs     Exec::load(aoide, home, env)
              |
              +-- check_pin      absolute, a regular file this user can run,
              |                  not under <home>/.local/bin
              +-- Schema::load   aoide schema --json
              +-- config         aoide config --json   -> vault key names
              |
              v
            Exec::call(words, flags, positionals, stdin)
              |
              +-- argv           path must be in the schema, flags declared,
              |                  required arguments given, `--` before text
              +-- child_env      base, minus what this child must not see
              |
              v
            run()              Command on the pinned path, env_clear,
                               stdout and stderr each capped at 16 MiB
```

- `check_pin` refuses a relative path, a missing one, a target that is not an
  executable regular file, and any target under `<home>/.local/bin`. It returns
  the canonical path, which is the only path the app runs.
- `argv` builds the argument list from `aoide schema --json`. It refuses, before
  anything runs, an unknown path, an undeclared flag, a bool flag given a value,
  a valued flag given none, a missing required argument, and a NUL byte.
- `child_env` builds the environment from the app's own, minus the variables
  the child must not see. An `eidolon` child loses `AOIDE_SESSION_ID`, the
  `AOIDE_MELETE_*` variables and the vault keys. An `aoide` child loses the
  `AOIDE_MELETE_*` variables unless its first word is `melete`, and the vault
  keys.
- The vault key names are the `tokenEnv` names in `aoide config --json`.
- `run` is private. Only `Exec` reaches it, and only with the pinned path.
