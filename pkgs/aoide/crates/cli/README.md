# aoide-cli (lib `aoide`)

The core app crate — the `aoide`/`aoided` binaries. In Cordis terms
(CONTRACTS.md §0): this crate is a dsh-style BUNDLE, the ordered composition
performed at boot; its assembled `Registry` is the plugin tree. Its explicit
`commands::all()` list is the bundle's PROFILE, not a "registry an author
must edit to be seen" violation — composing a bundle at its root is how
Cordis composes too (`docs/architecture/PACKAGE-LAYOUT.md`, "Cordis
correspondence").

## Named seams (what it exposes)

- `bin/{aoide,aoided}` — the two binary entry points. `aoided` parses its argv
  before it touches the root or binds anything: `--audit-log <path>`,
  `--version`, `--help`, and a taught exit-2 refusal for the rest. Both
  binaries print results through `door::say`, so `aoide … | head` ends quietly
  (exit 141) with SIGPIPE left ignored; `aoide --version` prints
  `aoide <version>`.
- `cli`/`dispatch` — argv parsing and the dispatcher, over
  `aoide_protocol::door::run`'s shared skeleton with core's own `special`
  hook (`mcp serve --stdio`, `a2a serve`, `mail serve` (the mail adapter,
  H1, which blocks in `serve_mail`'s own loopback accept loop),
  `secrets serve`, `secrets exec`,
  `secrets enroll`, `secrets watch`, `events tail`, `pair watch`
  (P-P5), `conductor`, `guide`/`schema` raw output, `workspace root` — one
  bare path on stdout and NOTHING on stdout when it refuses, because a
  launcher substitutes it into an argv). `dispatch` is also the
  one seam where every door's audit line is written, so it owns the rule
  that a line's message is not automatically the command's outcome text:
  the pairing ceremony (`pair`, `pair.reject`, `pair.watch`, and
  `mesh.pair`, which embeds each pair leg's own text) logs the operation
  and the status with the free text withheld — a confirmation or reply
  code rides that text, and the conductor's LOG panel reads this log back.
  Human and `--json` output are untouched by that rule.
- `registry` — the golden command-path snapshot test; its golden list
  is the source of truth for the path set and its count (this prose
  deliberately states no number).
- `guide` — `aoide guide`, the onboarding tier map.
- `commands` — the root-coupled groups that must read the ASSEMBLED
  registry: `meta` (guide/schema), `stubs` (not-yet-implemented
  placeholders), `onboard` (`aoide onboard`, P-I2 — the first-boot flow:
  registers the clone, seeds the songbook, wires harness hooks by calling
  the already-registered `hooks.install` handler directly off the registry,
  probes for `lyra` and delegates the nix half to `lyra onboard` as a child
  process when it resolves, prints the closing guide, then prints this
  machine's **node line** — `aoide_storage::charter::node_line`, the one
  public key plus binding an operator pastes into a charter, and the last
  thing the operator walks away with), `infra` (`mcp
  serve`'s tool-count reporting). Every other command group lives in its
  domain crate and is pulled in here by `commands::all()`.
- `commands::LAYOUT` — how `aoide` lists itself: a tagline and six topic
  sections of heads (Start here · Sessions & conducting · Mesh & mail ·
  Secrets · Agent interfaces · System), adopted by `all()` through
  `Registry::arrange`. Bare `aoide`, `--help`, `help`, every group page and
  `guide`'s table read it; stubs list themselves last.
- `a2a`/`mcp`/`daemon`/`graph`/`output` — thin root-level wiring over the
  matching domain crate for the two binaries' entry points.

## Test fixtures

`help_screen.rs` drives the real binary for the help screens: streams and exit
codes (overview and group pages on stdout at 0, a typo on stderr at 2), width
from `COLUMNS`, and `--color` / `NO_COLOR` / `--json`.

The graph-residency integration tests run the resident daemon in an owned
child process with a fixed environment. Fixture teardown kills and waits for
the child before restoring the test environment; lifecycle tests cover normal
teardown and readiness failure.

`mail_adapter_round_trip.rs` is the same child-process shape one capability
over (H1): the RELAY is a real `aoide mail serve` child with its own
`AOIDE_ROOT`, and this test process is the poller with a second one, because
two isolated roots in one process would race the serve thread against the
client calls reading the global env. Both sides carry hand-written VERIFIED
node records (P-CHARTER supplies them in a real run), the relay presents
itself under its own `AOIDE_A2A_NODE_NAME`, and the fixture ends by killing
and waiting for the child before restoring the environment. `#[ignore]`'d
like the node-connectivity pair: it needs real loopback TCP, real `curl` and
the built binary.

`common/mod.rs` raises the five-edge fixture mesh — two CHARTER
meshes, `home` = {osaka, sakaki(relay), yomi, chiyo} gated into `away` =
{evo, sakaki} through sakaki, with `sakaki` the one node in both. It is
`mail_adapter_round_trip.rs`'s multi-root pattern taken to five boxes: each
box gets its own `AOIDE_ROOT` and its own minted identity, the operator box
roots both meshes and signs both charters, and every other box trusts the
operator key from its `config.toml` and takes the signed pair. It is a
`mod common;` include, so any test file beside it reuses the same mesh, and
it removes its own root when the `Fixture` drops — a failed assertion leaves
no scratch mesh behind. The two deliberately broken charters live under
`tests/fixtures/` as TEMPLATES (`{sakaki}`/`{stranger}` stand in for node
lines, which carry a live key and binding and so can only be minted at run
time). `mesh_transit.rs` is its first caller: no child process and no
network, it reads declarations through `aoide_storage::routing`.

## What it consumes

`aoide-protocol`, `aoide-storage`, `aoide-conduct`, `aoide-client`,
`aoide-server`, `aoide-conductor`, `aoide-upkeep`, `aoide-secrets` (new at
P-V2). The DAG sink for core: depends on everything core needs, nothing
depends on it.

## How it composes

The command paths (count: the golden list in `src/registry.rs`, asserted
as an exact set — core's headless-capable, agent-orchestration surface: the
project/session graph (including `resurrect`, its ledger-backed
session revival), A2A, nodes (including the `node hub` designation, `node address <name> <address>`,
P-D5, the `aoide pair [<name|url|id>]`/`pair reject`/`pair watch`
one-command pairing ceremony, P-P2/P-P5/P-PV2/task #135 P3', the `node allow <name> <cap> on|off [--mesh <m>]`
closed-capability grant/revoke command backing the A2A spawn arm's hard
gate, P-P3 (per mesh since P-CHARTER), `node spawn <name> -- <text…>`, P-P5b, the signed
spawn-shaped `message/send` that actually reaches that gate, and `node
discover [--secs N]`/`node advertise on|off`, P-P6 + task #120, the LAN
discovery advertisement's read-only sweep (`pair`'s own hostname arm
is the sugar-over-the-ceremony half, P-PV2), and this instance's
own advertise switch), presence, the
daemon, its own event bus (`events tail`), usage, hooks, mail (the
addressed, signed, append-only mailbase), the secrets broker, this
instance's own `identity` (P-P1 of the
pairing workstream, `docs/architecture/PAIRING.md`)).
Never depends on
`aoide-song`/`aoide-screen` — painting is `lyra`'s bundle, assembled the
same way against the same domain crates' `commands` modules.
