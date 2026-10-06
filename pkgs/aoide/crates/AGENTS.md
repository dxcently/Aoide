# AGENTS.md — invariants across every crate in `pkgs/aoide/crates/`

Cross-crate rules only. A crate's own `AGENTS.md` holds what's local to it;
this file holds what would otherwise be repeated in all eleven. Points up to
the root `AGENTS.md` for the core-vs-lyra/AoideOS boundary and the house
rules that govern the whole repo.

## Registry order is load-bearing

`schema --json`, the MCP tool list, and the A2A `AgentCard` all derive from
the single `Registry` each app crate assembles
(`cli/src/commands/mod.rs::all()`, `lyra/src/commands/mod.rs::all()`) — see
`aoide-protocol::registry`'s module doc. The assembly order reproduces the
historical table byte-for-byte; **never reorder an existing `register()`
call**, only append. A command's home is its domain crate's own `commands`
module — nothing outside a domain's `register(&mut Registry)` function
enumerates that domain's commands.

## Golden discipline

Both app crates pin their exact command-path set in a golden snapshot test
(`cli/src/registry.rs::command_paths_match_the_golden_snapshot`,
`lyra/src/registry.rs`, same name). Adding, removing, or renaming a command
updates the matching golden list in the SAME commit as the `register()`
change — a red golden test is never "expected," it's the signal a
`commands::all()` edit forgot its snapshot. These golden lists are the SOLE
authority for each binary's command set — no count or tally lives anywhere
else. An external subcommand (task #138 — `aoide foo` falling through to
`aoide-foo` on `PATH`) never enters it: it cannot become a `Command`
(`aoide-protocol::registry::Command` is entirely `&'static`), so it never
reaches `Registry::insert` and the golden never sees it.

## No cross-crate copying

Every crate born from the Phase 2–9 restructure carries a shim-discipline
note in its `lib.rs`: a moved symbol is re-exported at its old path via
`pub use`, never duplicated. The same discipline applies going forward —
reach into another crate's public API (`aoide_conduct::graph::normalize_addr`
is `screen`'s example), never copy its logic in. If a symbol a consumer
needs is `pub(crate)`, widen it; don't fork it.

## Per-crate tests only

`cargo test -p <crate>`, never `cargo test --workspace` on this machine —
`aoide-conduct`/`aoide-server` bind real sockets and a workspace-wide run
deadlocks locally. Each domain crate that touches process-global env
(`AOIDE_STAGE_DIR`, `AOIDE_AUDIT_LOG`, …) carries its own `env_lock()`
(delegating to `aoide-test-support::env_lock()` where it's a dev-dependency)
so its tests serialize against each other without needing to coordinate
across crates in the same process.

A test that replaces a real binary with a shell shim on `PATH` gives that
shim the real one's stdin manners. `post_json` writes the request body
(`--data-binary @-`) or the bearer header (`-H @-`) into curl's stdin and
`render_qr` writes the URI into `qrencode`'s, so every shim standing in for
them opens with `cat > /dev/null`. One that exits without reading leaves the
caller writing into a closed pipe as soon as a loaded machine deschedules it
between spawn and write, and the EPIPE surfaces as an ordinary transport
failure — green on an idle desktop, red on a busy builder.

The same "the fixture must not wait forever" discipline covers the socket
fixtures: a test that binds a listener and waits for a delivery reads it
through `aoide-test-support`'s bounded rig (`expect_delivery`/`accept_one`/
`read_delivery`), never a hand-rolled `accept`/`read_to_end` — a payload the
code under test withholds then FAILS that one test inside `DELIVERY_BUDGET`
instead of parking it, and with it every test queued behind the crate's
`env_lock`. The rig's own contract lives in that crate's `AGENTS.md`.

## Declared validation, and the taught error

A command's argument rules are DECLARED on its registry entry and checked once,
by `Command::check` (`aoide-protocol::registry`), which `door::parse` and every
`dispatch()` (CLI, MCP, A2A, the aoided socket) run before the handler:
`Arg::{required, values, required_after}`, `Flag::{required, default, values, conflicts, value}`,
`Command::one_of`, `Command::brief`. A value that is empty or only whitespace counts as missing for
`required` and `one_of`, exactly like an absent one. A handler never re-checks what is
declared, and never writes its own `missing --x` / `requires --x` /
`usage: aoide …` line: `protocol/tests/no_hand_enforcement.rs` scans every
crate's production source for those spellings and fails when a file holds more
than its `ALLOWED` count. The list only ratchets down — a slice that migrates a
command deletes its hand check and lowers that file's number.
`aoide-test-support::registry_walk` runs the same assertions over both
binaries' registries (`every_example_parses`, `required_is_enforced`,
`defaults_applied`, `brief_fits`, `suggestion_or_list`), so a command is held
to its declarations the day it is registered.

**Taught error** — the one shape every refusal takes, defined here once:

```text
[error] secrets.exec: secret `db-prod` is not registered     what  — the thing refused, named
  why: the broker holds no policy by that name               why   — the cause, in words
  fix: aoide secrets add db-prod                             fix   — what to do next
```

- Built with `Outcome::refuse(cmd, Kind, what, why, Fix)`. `Kind::Usage`
  exits 2, `Kind::Refused` and `Kind::Failed` exit 1; `Fix` is `Run` (a
  command), `Set` (a setting), `Wait` (a duration) or `None(reason)` — a
  refusal with nothing to do says so instead of omitting the line.
- `--json` carries the same three under `data.refusal` plus the raw OS/serde
  text under `data.detail`; the text render never prints `detail`.
- An I/O or parse failure becomes `why` through `io_cause(op, path, &err)` /
  `serde_cause(path, &err)`; `os error N` and serde internals live in
  `detail` only.
- The exit rule: 2 when the invocation can never be valid (missing, unknown or
  ill-typed argument, flag or value; a command typed at the wrong binary),
  1 when a valid invocation was refused by the world (not found, unreachable,
  locked, not paired), 64 for a stub.
- A typed name that is not valid (command, flag, enum value, and — in the
  handlers that own them — node, secret, session, mesh) carries the closest
  match via `aoide_protocol::suggest::closest`, else the list of valid names.
  There is one matcher; never write a second edit distance.
- `Outcome::usage`/`Outcome::error` remain for callers not yet migrated; new
  refusals use `refuse`.

## Listing layout

Each app crate declares how its binary lists its commands once, as a `Layout`
const beside `all()` (`cli/src/commands/mod.rs`, `lyra/src/commands/mod.rs`):
a tagline, then topic sections of heads, each multi-command head with a
one-line blurb (or a bare command whose `brief` says it). It is what the
overview, a group page, the unknown-command lists, did-you-mean ties,
`guide`'s table and the schema's `sections` all read. Never order or section a
listing anywhere else. Stubs are left out of it on purpose and list
themselves last; `registry_walk::{every_head_is_sectioned,
listings_fit_and_hang}` hold both binaries to it.

## Extension points, cross-crate

- **A new domain crate**: add it to `pkgs/aoide/Cargo.toml`'s `[workspace]
  members`, give it a `commands` module with `register(&mut Registry)`, wire
  that into the owning app crate's `commands::all()` (core → `cli`, paint →
  `lyra`), and add its golden README/AGENTS pair here.
- **A new command on an existing crate**: add a `cmd!`/`arg!`/`flag!` entry
  (`aoide-protocol::registry`) inside that crate's own `commands` module;
  the two app crates never need an edit for a command that isn't moving
  binaries. Declare what it requires (`flag!(…, required: true)`,
  `one_of: &[&["id", "to"]]`) rather than checking in the handler. A command
  that is the first of a NEW head also adds that head to its binary's `Layout`
  (a section, and a blurb when the head has several commands) in the same
  commit; a `brief` keeps its list line from being cut mid-sentence.
- **A head moving between `aoide` and `lyra`** updates
  `door::{AOIDE_ONLY,LYRA_ONLY,SHARED}_HEADS` in the same commit; each app
  crate's `cross_binary_heads_match_the_registry` test pins its half.

## What needs a docs update in the same commit

- This crate's own `README.md`/`AGENTS.md` when its seams, deps, or
  invariants change.
- The owning app crate's golden snapshot when the command-path set changes.
- `docs/architecture/PACKAGE-LAYOUT.md` when a crate's charter changes —
  the per-crate READMEs distill it, never contradict it.
