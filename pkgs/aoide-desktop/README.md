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
  app starts a process or sends a signal, and the app's own doors (`doors.rs`):
  their launch, record, re-attach, watch and close. Its dependencies are serde,
  serde_json, libc, the door crate and `aoide-protocol` (a path dependency, for
  `is_unsafe` and `visible`).

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
  `token_path` is the one spelling of `<runtime>/eidolon/web-<port>.token`, and
  `check_runtime` is the runtime dir's check. `read_token` uses both. `Token`
  prints no value.
- `events.rs` turns the stream into a blocking iterator of `Frame`s, and hands
  out a `Closer` that shuts the stream's socket down from another thread.
- `frame.rs` decodes one `data:` payload into a typed `Frame`.
- `fake.rs`, behind the `fake` feature, is a std-thread door on 127.0.0.1. It
  speaks the wire as far as the tests need and records every request. A client
  that closes an open stream ends it, and `streaming()` says whether one is
  open.

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
- `door_env` is `child_env` for `eidolon web`, with `XDG_RUNTIME_DIR` set to the
  checked runtime dir. `spawn_door` starts a door through `spawn_detached`. The
  door runs in a session of its own and is not the app's child: std's child,
  the intermediate, calls `setsid`, forks the door, writes the door's pid to a
  pipe and exits, so init adopts the door. The app reads the pid and reaps the
  intermediate. The door's stdin is `/dev/null`, and its stdout and stderr are
  the door log. std's close-on-exec pipe reports an exec failure of the door as
  `spawn`'s error.
- `terminate` sends SIGTERM. It refuses pid 0 and any pid past `i32::MAX`. The
  caller checks the pid's start time and command line at that moment.

## How a door is kept

```
doors.rs    Doors::open(exec, eidolon, home, root, runtime, out)
              |
              +-- runtime_from_env, check_runtime  refused before any read
              +-- Store::load                      doors.json, or refused
              +-- reattach, per record             verify, open_stream, watch
              |
              v
            launch(plan)                 plan: launch_plan (pin and cwd)
              |
              +-- spawn_door               detached, its log under state/
              +-- await_banner             both banner lines, 30 s
              +-- verify                   pid, start time, cmdline, token
              +-- Store::update            the record, sessionId null
              +-- open_stream              hello's session fills the record
              +-- start_watch              one watch thread per door
```

- The record is `$AOIDE_ROOT/state/desktop/doors.json`, mode 0600. Each change
  is a read-modify-write under an exclusive lock on `doors.json.lock`, ending in
  a temp file, a sync, a rename and a directory sync. A file that does not parse
  is refused, and it is never rewritten.
- A record is trusted only while `verify` passes. Its pid is live and not a
  zombie, with the recorded start time. Its command line is the launch argv for
  the current pin and the recorded cwd, and its token file is the token path of
  its port. `open_stream` also needs the first frame to be `hello`, and its
  session to be the recorded one when there is one. A record that fails a check
  is dropped and never signalled, even when its pid is live. A record whose pin
  changed fails the command line check, so its door runs on, and the app keeps
  no record of it.
- The watch thread only reads. It forwards each frame on the caller's channel
  and never signals a door. A stream that ends while the door lives reopens
  after a second. A stream that ends with the door gone drops the record and
  sends `Gone`.
- The turn state is `Unknown` until a `turn-state` frame. The wire sends one
  only when the turn changes, and a new stream carries none.
- A close reads the turn the stream last showed, ends the watch (2 s bound), and
  re-checks the door. A door that fails the re-check is dropped, and nothing is
  signalled. A turn not known to be idle refuses the close with `Busy`, and the
  door is watched again. Otherwise the close sends SIGTERM, waits up to 10 s for
  the pid to exit, and removes the record. A door still live after 10 s keeps
  its record, and is no longer watched until the app next opens.
- Stop now re-checks the door before it cancels, so a door that fails the check
  gets no request. A 204 from the cancel waits up to 10 s for the turn to end,
  then the door closes. A 409 says the door is idle, so the turn is taken as
  idle, and the door closes at once.
- A turn that starts or ends while no stream is open is not seen. After a
  reconnect the turn is `Unknown` until a frame arrives. A close after the turn
  is refused, a marked door stays marked, and a stop now closes on its 409.

The fake eidolon is `src/bin/fake-eidolon.rs`, behind the `fakes` feature. It
takes `web --bind 127.0.0.1:0 --cwd DIR` and nothing else. A test drives it with
control words in `DIR/.fake-turn`: `running`, `idle`, `end`, and `cancel-409`,
which queues one 409 for the next cancel. It logs each control word applied,
each request served, and the SIGTERM it receives, to `DIR/.fake-log`, in order.
