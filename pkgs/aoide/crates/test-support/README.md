# aoide-test-support

Shared test scaffolding, one crate. Every domain crate's test modules ask this
for what a fixture needs from the HOST and the PROCESS — env-var save/restore,
a scratch dir that fits a socket path, the process-wide env mutex, the bounded
delivery rig, the fixture note payloads several command groups read — so none
of those exists as a copy per crate.

**Dev-dependency only.** Nothing in a production build may edge on this crate;
`Cargo.toml`'s own description says so, and no crate carries it under
`[dependencies]`.

## Named seams (what it exposes)

- `env_lock` — the one mutex serialising tests that mutate process-global env,
  per test BINARY. Each domain crate's own `env_lock()` delegates here (or
  mirrors it, where it is not a dev-dependency), so two spellings in one
  process are still one lock.
- `EnvSaver` — capture-and-restore on drop, so a panicking assertion never
  leaks env into the next test.
- `short_tmp`/`unique_tmp` — scratch dirs; `short_tmp` is HASHED to stay inside
  the `sun_path` budget a socket needs on either host.
- `raw_header` — one header's value from a raw HTTP head (request or
  response), case-insensitive and trimmed, pure; the socket tests read the
  signed-answer header through it.
- `owner_only_file` — a feed file in the shape THIS host's reader demands
  (native Windows reads its policy off a DACL, which has no Unix equivalent).
- `built_aoide_bin` — the real `aoide` binary beside the running test's own
  executable.
- `fake_hyprctl` — a `hyprctl` shim on `PATH`: `activeworkspace` answers the
  workspace, any other query one client. The desktop's client-list fake is
  strillix's `hypr::testutil`.
- `MigrationEnvGuard` — clears the migration env (`HOME`, the roots,
  `STRILLIX_MIGRATE`) for one test, and restores it on drop.
- `sv` — an argv literal as owned `String`s.
- `on_small_stack` (and its `SMALL_STACK`) / `finishes_within` — the two
  fixtures that make a hostile document's SHAPE a test: the first runs the
  work on a 256 KiB stack, so a walk that recurses per level of a depth the
  document chose aborts the binary by name; the second bounds the work's wall
  time, so a super-linear one fails at the limit instead of holding the suite.
- **The delivery rig**: `DELIVERY_BUDGET`, `accept_one`, `read_delivery`,
  `expect_delivery` — a fixture that expects a delivery waits for it, bounded,
  in one place.
- `registry_walk` — assertions that take one binary's `Registry` and check
  every command against what it declares (`every_example_parses`,
  `required_is_enforced` with a sentinel handler, `defaults_applied`,
  `brief_fits`, `suggestion_or_list`, `every_command_has_a_kind`);
  `aoide-cli` and `strillix` each call all of them.
- The fixture note payloads (`VALID_NOTES`, `NOTES_WITH_WINDOW`,
  `NOTES_WITH_GEOMETRY`, `NOTES_WITH_INTERPOLATION`), `inv`,
  `isolated_mail_root`.

## What it consumes

`aoide-protocol` only — the `Invocation` fixture shape, the
`cfg`-split local-socket types, and the owner-only file seam.

## How it composes

A `[dev-dependencies]` entry of every domain crate and of the root package
(`crates/AGENTS.md`'s "Per-crate tests only"). Nothing reaches it at runtime:
a production build never compiles it into a binary, and its own test binary is
empty by construction — what proves a seam here is the consumer whose fixture
uses it. That is also why the rig's contract is stated once, in `src/lib.rs`'s
own doc comments, and held by each crate's suite rather than restated per
crate.
