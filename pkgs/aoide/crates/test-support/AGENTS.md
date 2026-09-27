# AGENTS.md — aoide-test-support

## Invariants

- **Dev-dependency only.** No production crate may depend on this one, and
  nothing here gains a dependency beyond `aoide-protocol` — the moment a
  production edge appears, test scaffolding has become product surface.
- **ONE env mutex per test binary.** A crate that consumes this one delegates
  its own `env_lock()` here rather than declaring a second static: two mutexes
  over one process-global env exclude nothing, which is exactly the
  cross-module race this crate exists to close. Acquisition stays
  poison-tolerant (`.lock().unwrap_or_else(|e| e.into_inner())`) — a bare
  `.unwrap()` turns one failing test into a suite-wide poison cascade.
- **A fixture that EXPECTS a delivery waits through the delivery rig**
  (`expect_delivery`/`accept_one`/`read_delivery`), never a hand-rolled
  `accept`/`read_to_end` thread. Such a fixture binds a listener at the
  target's socket and then drives the code that is supposed to dial it; a
  delivery that never arrives — or a peer that connects and then goes silent —
  must FAIL that one test, by name, inside `DELIVERY_BUDGET`. Unbounded, it
  parks the thread and (under `env_lock`) every test queued behind it, so one
  real failure reads as a suite that never returns. Write `what` as the
  fixture's OWN expectation ("…is DELIVERED, not held pending"), never a
  restatement of the function name: the message is what names the broken arm.
- **A fixture that expects SILENCE asserts directly** — a non-blocking
  `accept()` answering `WouldBlock` — and needs no rig.
- **The rig is host-neutral by construction.** It is written against the
  `cfg`-split `UnixListener`/`UnixStream` (std's on Unix,
  `aoide_protocol::win_unix` on native Windows) and branches on `cfg` nowhere.
  Its READ budget is the guarantee on either host: `win_unix::accept` clears
  the inherited non-blocking flag best-effort and `recv_into` honours
  `SO_RCVTIMEO` on its own — so don't "simplify" the bound into a bare
  blocking read, and don't drop `set_read_timeout`.
- **Scratch paths go through `short_tmp`.** A hand-composed temp path with a
  socket name appended overruns `sun_path` (108 bytes, terminator included) on
  either host and silently names a DIFFERENT socket.
- **A feed file is created through `owner_only_file`.** Native Windows reads a
  feed's policy back off the object and refuses inherited ACEs, which is the
  shape a plain `std::fs::write` creates — the fixture that skips this fails as
  a deadline, not as the policy fact it is.

## Extension points

- **A new shared fixture fact** lands here as one `pub` fn with its own doc —
  but only once TWO crates need it; a fixture one crate wants stays in that
  crate's test module.
- **A new rig verb** (beyond accept, read, expect) is a fourth public name in
  `src/lib.rs`, used by the fixtures that need it — never a second copy of the
  three that exist.

## Docs update required in the same commit

- `README.md` when a named seam is added or removed.
- This file when the rig's contract or `DELIVERY_BUDGET` changes.
- A consumer crate's own `AGENTS.md` when a test-author rule it states changes
  here.
- `pkgs/aoide/crates/AGENTS.md` for cross-crate invariants — not restated here.
