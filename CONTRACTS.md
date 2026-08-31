# Aoide Contracts

Versioned interfaces. The flake's `checks` fail a merge that breaks one; `aoide
update` detects contract bumps and routes them through the update playbook
before the rebuild discovers them (see `concepts/Governance` in the wiki).

Each contract carries a version. Bumping a version is a breaking change and must
land with a migration note in `song/songbook/update-playbook.md`.

---

## 0. Design philosophy — everything is a plugin

Not a versioned interface; the rule the interfaces below are shaped by. It is
recorded here because every contract in this file is downstream of it.

**Everything is a plugin.** A capability enters Aoide by *existing* at a
conventional path, declaring what it needs by *name*, and being removable
without a trace. Nothing enters by being added to a list.

The repo already runs this way and did before it had a name for it: `lib/walk.nix`
discovers `modules/dendrites/*`, `modules/facets/*`, `pkgs/*` and
`song/songbook/*/rice.nix` by walking the tree, so **adding a capability is a new
folder, never an edit to an import list** (§2, §5). Each of those modules
self-gates on its own `enable`/`aoide.song` rather than being switched on from
outside. A widget resolves through `StagingEngine.resolveSong(song, slot)` — by
slot *name*, falling back to sonata — so no surface ever imports a concrete
widget (§5). Facets read only the closed namespace whitelist house rule 5
enumerates (`AGENTS.md` owns the list): named services, never another
module's internals.

Two names for the halves, taken from **Cordis** — *A Programming Paradigm for
Spatiotemporal Composability* (Shi, Zhang & Cui; preprint 2026-08-13,
`github.com/cordiverse/paper`), the plugin kernel under DeepSeek Harness, where
the model adapter, tool registry, session log and agent loop are all swappable
the same way:

- **Spatial composability** — a component declares its dependencies by service
  name and waits for them, instead of importing an implementation. Ours:
  `aoide.livery`/`aoide.arrangement`, slot names, the stage files in §4, the
  `aoide schema --json` tree that both doors generate from.
- **Temporal composability** — a component's effects are *revertible*: removing
  it unwinds everything it installed. Ours: `rice draft save`/`drop`, the
  staging→declare→rebuild boundary, and NixOS generations underneath. A change
  that cannot be backed out is not finished.

This is Nix's own thesis (declarative, additive, atomically reversible) applied
above the nix layer, and it is why the two doors — CLI and MCP — are one
implementation with two façades rather than two features.

**What it forbids, concretely:** a registry an author must edit to be seen; a
module reaching into another module; a capability that only exists inside one
consumer; an effect with no inverse. When a design choice is open, take the one
that can be deleted.

### The corollary for Quickshell: render surfaces only

**Quickshell paints; it never *is* the capability.** Every QML file in
`modules/facets/quickshell/` and `song/songbook/*/widgets/` is a render surface
that picks up an agnostic bridge or API by name. State, policy, IPC and system
access live behind a bridge (a CLI command, a stage file in §4, an IPC socket) that
is reachable **with only a shell**.

The test, applicable to a file you have never seen:

> Delete every `.qml` in the repo. Is this capability still reachable from a
> terminal? **No → it is in the wrong place.**

So: **a new API lands as a bridge first, and the QML picks it up second** — never
the reverse, and never only in QML. A surface may read, arrange, animate and
draw; it may not own the only copy of a fact, shell out to do work a command should
do, or decide policy.

#### The paint test — facet QML vs song QML

The terminal test above decides **bridge vs QML**. This second test decides
where a file that already passed it lives: the facet keeps agnostic bridges and
APIs, the song keeps everything that paints (§5). **The facet is not a component
library** — a shared visual component's home is the song's `widgets/` dir under
an uppercase name, not `modules/facets/quickshell/qml/`.

> A file stays in `modules/facets/quickshell/qml/` **iff all three are YES**:
>
> 1. **Song-blind.** Does the file name zero aesthetic decisions? Reading
>    `livery.paletteFg` is fine — that is picking up an API. *Deciding* that a
>    gauge is drawn `[▓▓░░]`, that a frame wears a pediment, that a face is a
>    kaomoji, that a morph takes 340ms because that is the house tier, or that
>    the accent cycle runs rust→murex→aegean — those are not.
> 2. **Song-plural.** Would a second, unrelated song use this file **unchanged**?
>    Not "could be adapted to". Unchanged.
> 3. **Bridge or mechanism.** Is its job one of exactly three: (a) publish system
>    or aoide state as **data**; (b) resolve, host, or inject a song's own QML;
>    (c) be the process entry point that wires (a) into (b)? "It draws something
>    reusable" is not a fourth category.
>
> Any NO → it belongs in `song/songbook/<song>/widgets/`.
>
> **Tie-breaker**, when an agent honestly cannot call question 2: *would a
> reviewer file this file's diff under "design change"?* If yes, it is song. A
> facet file's diff is always a mechanism change.
>
> **What the test is not.** It is not "does it paint" — `WidgetSlot` is an
> `Item` and `SurfaceSlot` hosts a `PanelWindow`, and both are facet. It is not
> "is it a `QtObject`" — `MoodFaces` and `MorphState` are `QtObject`s and both
> are song. It is not line count — `AudioColonnade` is 1940 lines of song and
> `AoideIpc` is 24 lines of facet. It is not "is it shared" — shared across
> *widgets* is not shared across *songs*, and only the second earns a facet home.
>
> **The corollary for a new API**: a new capability lands as a facet bridge that
> answers **with data**, never with a component to instantiate, and the song
> picks it up by name. Concretely: a facet bridge exposes `paletteAccent`; it
> does not expose `ctxBar()`. If the natural shape of the new thing is "a
> component every widget instantiates", it is not an API — it is a song helper,
> and it goes in `widgets/` with an uppercase name.

Question 3 is the load-bearing one, and it is where every genuine argument in
this tree lives. The same text sits in
`modules/facets/quickshell/qml/slots.md` — one wording, two homes, because a
ricing agent reading about where to put a helper is exactly the agent who needs
the rule.

---

## 1. Note schema — **v0**

The single seam between the frozen nix layer and the live desktop. Facets read
`aoide.livery` (the dress) and `aoide.arrangement` (the structure) and
**nothing else** — an enumerated, closed pair, not an open `aoide.*` (AGENTS.md
house rule 5). Both are declared in `modules/nucleus/options.nix`.

Notes are Aoide's design-token layer; the container format remains the W3C
design-tokens format. v0 lives inside the (future) W3C design-tokens container;
the semantic and component tiers are the open v1 design-system work. v0 is
deliberately minimal:

### Palette tier (closed — base16-derived)

| Key       | Type          | Default     | base16 |
| --------- | ------------- | ----------- | ------ |
| `palette.bg`     | hex `#rrggbb` | `#1e1e2e` | base00 |
| `palette.fg`     | hex `#rrggbb` | `#cdd6f4` | base05 |
| `palette.accent` | hex `#rrggbb` | `#89b4fa` | base0D |
| `palette.urgent` | hex `#rrggbb` | `#f38ba8` | base08 |

### Component tier (v0 overrides — `bar.*` / `notif.*` / `window.*`)

Each field is `nullOr hex`; `null` means "fall back to the palette". Facets
apply the fallback, not the option system.

| Key                     | Falls back to     |
| ----------------------- | ----------------- |
| `bar.bg`                | `palette.bg`      |
| `bar.fg`                | `palette.fg`      |
| `bar.accent`            | `palette.accent`  |
| `notif.bg`              | `palette.bg`      |
| `notif.fg`              | `palette.fg`      |
| `notif.urgent`          | `palette.urgent`  |
| `window.border`         | `palette.accent`  |
| `window.borderInactive` | `palette.bg`      |

Hex format: `#?[0-9a-fA-F]{6}` (leading `#` optional). The livery engine
(native Rust, `crates/song/src/livery/`) owns the authoritative `rice lint`
validator; the option type is a permissive gate only.

### Geometry tier (v0 optional overrides — `geometry.*`)

Additive-optional (same status as the base16 tier): every field is `nullOr`,
defaulting to `null`. A notes file with no `geometry` block behaves exactly
as before — the compositor facet applies the fallback, not the option
system. Rides `song/stage/livery.json` for live application: `lyra rice
stage` live-applies this tier (plus `window.border`/`borderInactive`) via
best-effort, guarded `hyprctl keyword` calls — see §4's staged-geometry
paragraph — in addition to baking the value at build time into
`hyprland.conf`.

| Key                  | Type          | Fallback | Hyprland keyword       |
| --------------------- | ------------- | -------- | ----------------------- |
| `geometry.gapsOut`    | `nullOr int`  | `8`      | `general:gaps_out`      |
| `geometry.gapsIn`     | `nullOr int`  | `6`      | `general:gaps_in`       |
| `geometry.borderSize` | `nullOr int`  | `2`      | `general:border_size`   |
| `geometry.rounding`   | `nullOr int`  | `0`      | `decoration:rounding`   |
| `geometry.blurEnabled`| `nullOr bool` | `true`   | `decoration:blur:enabled` |
| `geometry.blurSize`   | `nullOr int`  | `8`      | `decoration:blur:size`  |
| `geometry.blurPasses` | `nullOr int`  | `3`      | `decoration:blur:passes`|

Border *colours* (`window.border` / `window.borderInactive`, component tier
above) already map to `col.active_border` / `col.inactive_border` and are
unaffected by this tier.

### Cover-art tier (v0 — the wallpaper note)

| Key         | Type            | Default | Falls back to                          |
| ----------- | --------------- | ------- | -------------------------------------- |
| `wallpaper` | `nullOr path`   | `null`  | facet's deterministic solid-colour PNG |

A literal nix path (copied to the store — never a `song/` runtime read). The
stylix facet bakes it as the base-context image; `null` bakes the solid-colour
fallback derived from `palette.bg`.

### Override tier (v0 additive — venue recolour, `override.*`)

`aoide.livery.override.{bg,fg,accent,urgent,hot}` (each `nullOr` hex, default
`null`) is the one tier of the livery the VENUE authors, never the song.
Semantics — the **recolour rule**: each set anchor rewrites every livery
colour equal to the song's AUTHORED value for that anchor (palette, base16
slots, component-tier literals), computed in one simultaneous pass against the
authored values, in BOTH fan-outs — the baked Stylix scheme and the
`song/stage/livery.json` activation seed. A recolour, never a re-key: slots
not carrying an overridden anchor's value stay the song's (the ramp is the
song's voice; wanting a different ramp is wanting a different song). `hot`
overridden while the song left it `null` sets `palette.hot` directly.
Consumers apply the rule through `lib/livery.nix`'s `resolve` — identical
rules on every consumer, so the fan-outs cannot disagree. Read-side only: the
option system keeps storing the song's authored values inert; no config-side
`mkForce`, so no option-system recursion.

| Key                | Type          | Default | Recolours       |
| ------------------ | ------------- | ------- | ---------------- |
| `override.bg`      | `nullOr hex`  | `null`  | `palette.bg` and its twins |
| `override.fg`      | `nullOr hex`  | `null`  | `palette.fg` and its twins |
| `override.accent`  | `nullOr hex`  | `null`  | `palette.accent` and its twins |
| `override.urgent`  | `nullOr hex`  | `null`  | `palette.urgent` and its twins |
| `override.hot`     | `nullOr hex`  | `null`  | `palette.hot` and its twins; sets it directly when the song left it `null` |

Additionally, `override.base16.<slot>` / `override.{bar,notif,window}.<field>` set
that ONE named key slot-exactly — no propagation, no participation in the
recolour pass — and a named key wins over the recolour on its own slot;
naming a `base16` slot when the performed song carries no base16 scheme is an
eval error.

**Migration to v1:** the update playbook migrates `song/songbook/*/rice.nix`
and `livery.json` from v0 to v1 when the design-system workstream lands v1.

---

## 2. Dendrite shape — **v0**

A dendrite is a walker-discovered module under `modules/dendrites/` that guards
its `config` on a per-feature or role flag. Discovery imports the file; gating
decides activation (dxflake pattern, verbatim).

```nix
# modules/dendrites/<name>.nix
{ config, lib, ... }:
{
  options.aoide.<name>.enable = lib.mkEnableOption "<name>";
  config = lib.mkIf config.aoide.<name>.enable {
    # a dendrite carries its own dependencies (narrowest scope wins)
  };
}
```

Rules:

- **Guard on `aoide.<name>.enable`** (per-feature) or an aggregation/role flag.
- **A dendrite never reads another module** — only `config.aoide.*` options it
  declares itself, plus stock NixOS options.
- **Growth is additive**: new dendrites are new files; upstream merges stay
  conflict-free by construction.
- **Shelving opt-out**: prefix a filename with `_` (`_wip.nix`) to hide it from
  the walker without deleting it. Any path containing `/_` is skipped.
- Subfolders under `modules/dendrites/` are grouping only; the walker registers
  every file regardless.

Facets (`modules/facets/`) are the same shape but MAY read `aoide.livery` and
MAY declare `aoide.surfaces.<name>.owner` — they read no other module.

### Repo shape (the root is closed)

The repo root is **closed**: new content lands **inside the existing tree at
its designated place** — never a new root entry:

- covers (wallpapers) → `song/songbook/<song>/assets/`
- chimes (sounds) → `song/songbook/<song>/sounds/`
- per-song assets → `song/songbook/<song>/`
- module assets → next to their module, as a directory dendrite/facet

Content paths are looked up in the Song Map (`concepts/Song-Vocabulary` in the
wiki). Creating a new **closed** root entry is a **contract change**, not a
convenience: it lands here first, with review — not sprayed into the tree.

The root's actual contents, machine-readable — one `name kind` pair per line,
`kind` one of `closed-file` / `closed-dir` (the closed set this section
governs — adding one is the contract change above), `runtime-file` /
`runtime-dir` (gitignored working-tree state, never committed), or
`clutter-symlink` (nix build output, see below). A `name` ending in `*` is a
glob, not a literal entry:

```
AGENTS.md        closed-file
.claude          closed-dir
CONTRACTS.md     closed-file
docs             closed-dir
flake.lock       closed-file
flake.nix        closed-file
.git             closed-dir
.gitignore       closed-file
hosts            closed-dir
lib              closed-dir
modules          closed-dir
pkgs             closed-dir
README.md        closed-file
song             closed-dir
statix.toml      closed-file
.pi              runtime-dir
.pi-subagents    runtime-dir
log              runtime-file
run              runtime-dir
state            runtime-dir
result*          clutter-symlink
```

`runtime-*` entries: `.pi/` and `.pi-subagents/` (pi / subagent session
runtime), `log` (content-pipeline + audit runtime, newline-delimited JSON —
a **file**, not a directory), `state/` (account/usage runtime, e.g.
`state/usage.json`), and `run/` (the deployed Quickshell tree rsynced from
the store by home-manager, `modules/facets/quickshell/default.nix`). All are
disposable and never committed. `.gitignore` also reserves `catalog/` and
`index/` for the same content-pipeline runtime; neither exists on disk today.

`clutter-symlink` covers the nix build-output symlinks `.gitignore` matches
with `result` and `result-*` — an expected byproduct of `nix build`, not part
of the closed root and not governed runtime state either. They are listed as
the glob `result*` rather than by name because how many exist, and what each
is called, changes with every build. `aoide soundcheck`
(`pkgs/aoide/crates/upkeep/`) computes the live set from `git check-ignore`
and reports it under its own `clutter` check at `info` severity; this list
reuses that term rather than inventing another one.

None of the `runtime-*` or `clutter-*` entries are ever committed, so none
are a contract change to add to or write into. Only the `closed-*` set is
governed; "closed" is this section's own term, and membership in it is not
the same question as whether git tracks the entry (`.git` is closed and
untracked both).

### Package shape (`pkgs/` is walked too)

`pkgs/` self-registers exactly like `modules/` and `song/songbook/`. Drop
`pkgs/<name>/default.nix` — a `callPackage`-able derivation taking standard
nixpkgs args — and `lib/pkgs.nix` (the packages walker) discovers it into **all
four** consumers from one source:

- the flake `packages.<system>.<name>` output,
- the host overlay (`lib/mkHost.nix` → `pkgs.<name>` inside every module),
- the vm overlay (`tests/vm-boot.nix` — literally the same import), and
- a `pkg-<name>` flake check that builds it.

Rules:

- **Shelving opt-out**: prefix the dir with `_` (`pkgs/_wip/`) to hide it from
  the walker without deleting it (same `_` convention as `modules/`).
- **Names must not shadow nixpkgs**: because the overlay injects the name into
  `pkgs`, a collision would silently mask a stock attribute. The overlay's
  collision guard `throw`s a legible error on an accidental clash. A
  **deliberate** shadow of an unrelated attribute (Aoide's `melete` harness vs
  nixpkgs' `melete` font) is a reviewed exemption listed in
  `lib/pkgs.nix`'s `intentionalShadows`.
- **Special args**: `callPackage` auto-fills standard nixpkgs args. A package
  needing a non-standard arg overrides it with an explicit `//` after the call
  in `lib/pkgs.nix`'s documented escape hatch — kept there (not in `flake.nix`)
  so the flake output and both overlays still read the one set.

Adding a package is **one new folder** — never an edit to `flake.nix` or `lib/`.

---

## 3. `aoide schema --json` output — **v0**

The machine-readable capability backstop at any agent tier. The MCP tool list
generates from it (`concepts/Agent-Interface`). Owned by the CLI (Agent B).

Top-level shape (stable keys):

```json
{
  "schemaVersion": "0",
  "aoide": "0.0.2",
  "commands": [
    {
      "path": ["rice", "compose"],
      "summary": "Scaffold a new song under song/songbook/<name>/ by copying --from's notes.",
      "args": [
        { "name": "name", "type": "string", "required": true }
      ],
      "flags": [
        { "name": "from", "type": "string", "description": "Source song to copy notes from (default \"sonata\")." }
      ],
      "gated": false,
      "implemented": true,
      "exitCodes": { "0": "ok", "2": "usage", "1": "error" }
    }
  ]
}
```

Contract guarantees:

- Every command takes and emits `--json`.
- Errors are structured with meaningful exit codes.
- All operations idempotent; output reports exactly what changed.
- `gated: true` marks operations that route through the user rebuild gate.
- `implemented` (bool) — does this command actually run, or is it still a
  walking-skeleton stub (`dispatch()` returns the not-implemented envelope
  without calling a handler)? **Additive** (CONTRACTS.md §6 phase B): no
  version bump, same additive discipline as §1/§4's optional tiers — it lets
  a discovery consumer filter to only the live commands without a second
  command inventory. The A2A AgentCard (§6) is the first such consumer: its
  `skills` list is exactly the commands with `implemented: true`.
- `examples` (optional string array) — invocation samples shown by
  `aoide <command> --help`. **Additive**, same discipline as `implemented`:
  omitted from `schema --json` entirely when empty, so a command without
  examples serializes byte-identical to before the field existed; consumers
  simply see the key when present.
- `internal` (optional bool, default omitted = `false`) — hook-plumbing a
  harness drives, never a human typing it directly (`session start/phase/
  end/hook`, task #101 R1 of the command-defrag lane). **Additive**, same
  discipline as `implemented`/`examples`: omitted from `schema --json`
  entirely when `false`, so a non-internal command serializes byte-identical
  to before the field existed. It hides noise from the HUMAN listing only
  (`aoide guide`'s command table skips `internal` rows) — every other door
  (`schema --json`, the MCP tool list, the A2A AgentCard's `skills`) still
  enumerates internal commands in full; `internal` is not a second
  `implemented`-style capability filter, it is purely a display hint for one
  consumer.

**Per-binary schema (two binaries, P-A5 of the binary-split workstream,
`docs/architecture/PACKAGE-LAYOUT.md`, "Two binaries").** This schema is
per-binary, not singular: each binary's `schema --json` is its OWN
contract, with its own golden command-path snapshot — the two never
merge into one document, and neither one's golden test knows the other's
count.

- `aoide schema --json` — the core contract (the `protocol`/`storage`/
  `client`/`conduct`/`server`/`conductor`/`upkeep`/`secrets`/`cli` command
  surface: conducting, the project/session graph, A2A, peers, presence,
  the daemon, usage, hooks, the message inbox, the secrets broker, the
  Melete MCP client).
  `crates/cli/src/registry.rs`'s golden test pins the authoritative
  command-path set; `aoide schema --json` is the live enumeration. Notes
  on individual commands, newest first: `inbox list|read|clear`, appended
  newest, messaging workstream C6; `secrets serve|exec|add|rm|grant|revoke`,
  appended newest, Workstream SECRETS P-V2; `secrets enroll`, appended
  newest, Workstream SECRETS P-V3; spelled `vault ...` until the P-V4b
  rename — paths rename in place, registration order and count unchanged;
  `secrets put`, appended newest, Workstream SECRETS P-V4c — the write
  half: backend `set` templates plus the built-in `file` backend, both
  documented in the "Secrets home" subsection below; `secrets set-totp`,
  appended newest, Workstream SECRETS P-V4e — flips an existing policy's
  `requireTotp` bit without hand-editing `policy.json`; the same phase
  also added `secrets enroll --show` (reprint an existing enrollment, no
  rotation, no new path) and a tty-hidden-input prompt for `secrets put`
  (no new path either — both ride the existing `enroll`/`put` commands);
  `secrets automate`/`secrets expose`, appended newest, Workstream SECRETS
  P-N1 — the per-secret automation gate (`on`/`off`/`grant`/`revoke`,
  "Secrets wire" subsection below) and the `remote` reachability flag
  (`on`/`off`, no non-local door reads it yet); `secrets pending`/`secrets
  approve`/`secrets dismiss`, appended newest, Workstream SECRETS P-N2
  — a TOTP-gated `resolve` with no code now PARKS instead of refusing
  outright (the requesting connection blocks until an operator completes
  the ask, or a configurable timeout elapses); see the "Secrets wire"
  subsection below for the full parked-ask lifecycle and the wire's new
  `wait`/`pending`/`approve`/`dismiss` shapes. `secrets watch`, appended
  newest, tracker #71 Part 1 — a foreground, line-mode terminal surface
  that tail-follows the mirrored aoide log and narrates every broker event
  (`released`/`parked`/`completed`/`dismissed`/`expired`), prompting inline
  for a parked ask when stdin is a terminal; `--json` emits one event
  object per line instead — see `crates/secrets/README.md`'s "Watching
  events" section for the exact shape; `secrets migrate`, appended newest,
  P-G2 (task #72) — moves an existing secret's stored value from its
  policy's current backend to a target one (default `age`) and flips the
  policy row, an admin command mirroring `add`/`rm`/`grant`'s direct-home
  shape — see `crates/secrets/README.md`'s "Migrating a secret between
  backends" section for the full flow; `events tail`, appended newest, P-D3
  (`docs/architecture/AOIDED.md`) — a foreground, line-mode follow of
  `aoided`'s own events feed (the secrets-feed mirror's name-only lines,
  the #69 hand-edit watcher, and any future tick producer), `--class`
  filtering to matching event classes (comma-separated), blocking until
  Ctrl-C; CLI-only, the same door-policy shape `secrets watch` already holds
  for a foreground/blocking command — see `crates/server/README.md`'s
  "Named seams" section for the producer/tail mechanism; `peer hub`,
  appended newest, P-D5 (`docs/architecture/AOIDED.md`'s "The hub option")
  — designates one registered peer as THE hub (`peer_store::Peer.hub`,
  additive/v0-safe, same discipline `SessionRecord.headless` set the
  precedent for); `--clear` removes the designation; both directions are
  idempotent and report exactly what changed (set/moved/cleared/no-op)
  — see §6's "Remote reach" subsection for how the hub composes with
  the rest of the mesh; `resurrect` (`graph resurrect` at the time),
  appended newest, P-D8 (`docs/architecture/AOIDED.md`'s "L5 — harness
  summoning") — revives a project's resumable sessions off the durable
  session ledger (`state/session-ledger.jsonl`, §4 below); `--project
  <name>` resolves against `projects.json`. Selection: `--all` widens
  to every anchored ledger entry, `--id <ledgerSessionId>` narrows to
  one specific entry, and bare (neither flag) resumes the project's
  WHOLE undying set (`state/undying.json`, durable-sessions plan P-C4
  — see this section's own `state/undying.json` subsection below) —
  anchored entries currently marked durable, minus any id already alive
  (non-`done`) in `sessions.json`, deduped by `sessionId` keeping the
  newest `endedAt`. `--all`/`--id` never consult the undying mark. An
  empty bare-mode selection is an `Outcome::ok` no-op naming the undying
  set as empty for the project. Each surviving candidate then resolves
  through TWO arms (durable-sessions plan P-C6): a harness with a verified
  `resume_args` (`aoide_protocol::agents::AgentProfile`) spawns windowed
  running `<harness> --resume <id>`; a candidate the harness arm finds
  nothing for falls to the TERMINAL arm — a captured `restore` snapshot
  (P-C5) marks it a conducted shell, so it spawns windowed running its
  own login shell (`$SHELL` → passwd → `/bin/sh`, `-l`) instead. A
  candidate neither arm resolves is skipped with a taught message naming it,
  never a guessed invocation. Once a terminal candidate's spawn registers,
  its `restore` snapshot drives one more step, in-process through `send`,
  never a direct socket write: a foreground command it was demonstrably
  running re-execs with `--yes --submit` (never for a recorded `sudo …`,
  which only restores the cwd); an idle session's clean unsubmitted
  `typed` line preloads with `--yes` and permanently no `--submit`,
  so it sits in the new prompt until a human presses Enter; idle with no
  `typed` delivers nothing. Also the command core the daemon's own boot-time
  auto-resume trigger calls in-process — see `state/stage/projects.json`'s
  `autoResume` paragraph below for that trigger's own contract. `identity`,
  appended newest, P-P1 (`docs/architecture/PAIRING.md`) — this instance's
  lazily-minted ed25519 keypair (`aoide-storage`'s new `identity` module,
  `state/identity/`): prints the public key (hex), a short display
  fingerprint, and the mint timestamp; minting on the first call is a
  `changed` entry, every later call an idempotent read. The private
  key never appears in `schema --json`, an `Outcome`, or any log —
  see §4's "`state/identity/`" subsection below for the wire/storage
  shape. `peer pair request|pending|approve|reject`, appended newest,
  P-P2 (`docs/architecture/PAIRING.md`) — the pairing ceremony's CLI
  half, both directions behind the SAME four commands (no fifth command
  for the requester's own confirm step — `approve`/`reject` dispatch by
  direction): `request <url>` sends a commitment (`aoide/pairRequest`) to
  another instance's A2A door, immediately reveals it (`aoide/pairReveal`,
  same invocation, two sequential POSTs), and parks the outbound half
  locally (`aoide_storage::pairing`); `pending` lists BOTH this instance's
  own parked INBOUND requests (no SAS until revealed — an unrevealed entry
  shows `"revealed": false`) and its own OUTBOUND requests (`"direction":
  "outbound"`, SAS always shown, tagged with the entry's own `state`);
  `approve <id>` tries the inbound queue first — refusing an unrevealed
  entry outright — then the outbound queue, re-deriving the SAS either
  way before anything commits: on the inbound (approver) side the gate
  is the TYPED pairing code (task #120 P3 — the operator types the
  code as read off the requester's screen, `--code NNN-NNN` scripted;
  3 cumulative mismatches auto-deny the request, and `--yes` never
  bypasses this), and a match writes a `pubkey`/`verified` peer record
  on this end PURELY LOCALLY (Design A, task #119 — no wire call at
  all) and marks the parked entry approved for the requester's own poll
  to find; on the outbound (requester) side, this POLLS the approver's
  door (`aoide/pairPoll`, over the SAME forward dial the request already
  used) and, once approved, confirms `y`/`yes` (`--yes` scripted — the
  requester's own screen already printed the code) then commits the peer
  record directly; `reject <id>` is a clean local refusal on either queue,
  no wire call, no peer record — on an outbound entry this doubles as the
  ceremony's abort command, usable at any stage. See §6's "Pairing wire"
  and §7's "Peer record" subsections below for the exact wire shapes, the
  commitment/reveal construction, and SAS derivation. `peer allow <name>
  <cap> on|off`, appended newest, P-P3 (`docs/architecture/PAIRING.md`
  decision 5) — flips one capability in a peer's own closed `allows`
  set (`"read"`/`"spawn"`, `aoide_storage::peer_store::PEER_CAPABILITIES`
  — never a per-capability serde bool scatter); idempotent (`on` on an
  already-on capability, or `off` on an already-off one, both report a
  no-op), refuses an unknown peer or an unknown capability string with
  a distinct taught error for each (the capability check runs before
  the peer lookup). A paired peer is stamped `[pairing] defaultGrant`
  (`config.toml`, `["read"]` unless widened) the moment it FIRST becomes
  verified, or the `--allow` typed on that one `peer pair approve`
  (`upsert_paired_peer`, both ceremony commit sites) — this command is for narrowing or
  widening that grant afterward, and is the ONLY other writer of the
  field. See §6's "Security posture" (the P-P3 amendment) and §7's
  "Peer record" subsections below for the gate this feeds and the wire
  shape. `peer spawn <name> [--yes] -- <text…>`, appended newest, P-P5b
  (`docs/architecture/PAIRING.md`) — makes the spawn gate above actually
  REACHABLE: POSTs a signed, spawn-shaped `message/send` (`contextId`
  omitted) to a PAIRED peer's own A2A door, `<text…>` riding as the
  prompt `do_spawn` types into the newly spawned session's first turn
  (which agent runs is the PEER's own configured `aoide.a2a.spawnAgent`,
  never client-chosen). Refuses an unknown or unpaired peer LOCALLY with
  a taught error naming `peer pair` (an unsigned request could never
  satisfy the remote's `PeerRung::Signature`-only gate anyway); every
  OTHER refusal (allows lacking `spawn`, an unsigned/too-old caller, clock
  skew) is the remote door's own call, surfaced verbatim — this command
  never re-implements or second-guesses that gate. `--yes` skips a LOCAL
  `y`/`N` confirmation only (mirrors `peer pair approve`'s idiom); the
  remote door's own gate is the sole security authority either way. See
  §6's "Security posture" and §7's "CLI surface" subsections below for
  the wire shape and the live gate this closes. `peer discover [--secs
  N]`/`peer invite <name> [--secs N] [--yes]`, appended newest, P-P6
  (`docs/architecture/PAIRING.md`'s "Discovery (advertise-but-locked)"
  section) — the LAN discovery advertisement's CLI half. `discover`
  listens on a fixed UDP port a few seconds (default ~4) and prints every
  DISTINCT (name, source) heard (name, claimed ssh hop `user`@`host`,
  observed source address, first/last heard, a heard count) — read-only,
  it never writes `state/peers.json`. `invite` runs its own discover
  sweep, resolves `<name>` against what was heard, and on EXACTLY one
  match runs the SAME `peer pair request` core through a shared function
  (never a copy) against that advertisement's OBSERVED source address;
  zero or multiple matches refuse with a taught error listing every
  name that was heard. `--yes` skips only the local proceed-confirm
  — the ceremony's own SAS confirmation (both operators, both ends)
  is untouched either way. Advertising (the OTHER half — `a2a serve`
  emitting its OWN advertisement) is off by default, switched at runtime by
  `peer advertise on|off` (task #120) or forced for a process's lifetime
  by `AOIDE_DISCOVERY_ADVERTISE`/`aoide.a2a.discoveryAdvertise` — see
  §6's "Discovery advertisement" subsection below for the wire format,
  the pinned constants, and the "discovery grants nothing" statement. Core
  is nix-independent (cargo build, no nix shell-outs) — see the HARD
  CONSTRAINT note in the binary-split plan; the secrets broker holds to
  the same constraint (plain unix socket + shell-outs, no nix eval). Held
  by the `nix-independence` flake check (`lib/checks.nix`), which derives
  core's crate closure from `pkgs/aoide/Cargo.toml` and asserts it never
  reaches `aoide-song`/`aoide-screen`/`aoide-lyra` and never shells out to
  nix. `config`/`config set`, appended newest, task #135 P-C — the portable
  runtime config file (`$AOIDE_ROOT/config.toml`, `aoide_storage::config`),
  the direct consequence of that same portability constraint: a CORE
  command's configuration cannot live in a NixOS option, because on a
  non-nix host that option does not exist and "rebuild to change a grant"
  is not an operation. `config` prints the effective values, the path they
  resolved from, and whether that path is MANAGED (rendered read-only
  elsewhere, `$AOIDE_CONFIG`) or UNMANAGED (this host's own editable file);
  `config set <section>.<key> <value>` is the one schema-validated write,
  refusing a managed or unwritable config, an unknown key, and an
  out-of-vocabulary value with a taught error apiece and nothing written in
  any of them. Nix is ONE authoring front-end
  (`modules/nucleus/config.nix`, whole-file or nothing) that renders the
  file to a read-only store path and points `$AOIDE_CONFIG` at it — see
  §4's `config.toml` subsection below for the resolution order, the schema,
  and the intent-vs-state line.
- `lyra schema --json` — the AoideOS-surface contract: onboard/rice/draft/
  mode/cover/livery/quickshell/screen/shellbridge/herald/take/element,
  the painted surface. `crates/lyra/src/registry.rs`'s golden test pins the
  authoritative command-path set; `lyra schema --json` is the live
  enumeration — `mcp.serve` must itself be a registered path for
  `aoide_protocol::door::parse` to ever reach lyra's `special` closure on
  `mcp serve --stdio`, one of the root-coupled extras beyond the named
  groups, alongside `onboard` (P-I3, docs/architecture/ONBOARD.md), `secrets
  ask` (P3), and `pair ask`/`pair confirm` (P-PV3, task #132, one dialog
  shape per pairing direction). `element seed` (L-E1,
  docs/architecture/ELEMENTS.md) renders a song's committed
  `elements/*/element.json` — non-QML rice targets (waybar, dunst, anything
  with a config file) — into `run/elements/`. Lyra alone may shell out to
  nix (`song/widgets.rs`, and `onboard`'s own `nix eval`/`nix-instantiate`).

A consumer wanting the whole desktop's capability inventory reads both.
This was never a version bump: `schemaVersion` stays `"0"` on both —
this section has never promised a fixed command inventory, only a
document SHAPE, and the shape above is unchanged for either binary. The
A2A AgentCard (§6) advertises whichever registry the serving binary
assembled — core's card advertises only core's registry, and only its
implemented commands (the card's `implemented` filter drops stubs, §6),
since `a2a serve` is core-only and lyra never registers it.

### Daemon wire — the fourth door (`docs/architecture/AOIDED.md`, P-D2/P-D4)

`aoided`'s own control socket (`$AOIDE_DAEMON_SOCKET`, else
`$XDG_RUNTIME_DIR/aoide/aoided.sock`, `0600` user-private — no cross-uid
audience) speaks the secrets wire's framing verbatim: one newline-delimited
JSON request line → zero or more interim lines (`"interim":true`) → exactly
one final reply line. Three ops, a closed set:

```json
{"v":0,"op":"ping"}
{"v":0,"op":"subscribe","classes":["secret","gate"]}
{"v":0,"op":"dispatch","path":["graph","view"],"args":[],"flags":{"json":"true"}}
```

- `ping` → `{"ok":true,"daemon":"aoided","pid":…,"version":"…",
  "sealPubkeyHex":"…"}` — the liveness probe. `sealPubkeyHex` (LANE
  IDENTITY P-ID2) is this daemon PROCESS's current seal-signing public
  key (§4's `seal` paragraph) — never secret, but the only channel any
  caller can trust for it: a same-uid-writable FILE sitting next to the
  seals it verifies would let whoever can forge a seal also forge the
  "trusted" key that vouches for it, so it is exposed here instead, on a
  LIVE reply only the real running daemon can write.
- `subscribe` → the connection becomes a one-way stream: each event on the
  daemon's own events feed (§1's event-bus record shape, name-only) whose
  `class` is in the request's `classes` array is written as an interim
  line as it happens. An empty/absent `classes` delivers NOTHING —
  default-deny per class, the same posture `Subscription` already models.
- `dispatch` (P-D4 — the fourth door onto the one schema this section
  already documents: CLI, MCP, A2A, and now Daemon) → `path`/`args`/`flags`
  build an `Invocation { door: Door::Daemon, .. }` LITERALLY — no dotted
  tool-name lookup the way MCP's `tools/call` resolves one. The final
  reply is one `{"outcome": <the full Outcome envelope, this section's own
  shape>}` line.

**No new allowlist.** `Door::Daemon` (`aoide-protocol`) is audited exactly
like the other three; the per-command door policy already in force for
MCP/A2A — a CLI-only admin command's refusal, a gated command's `gated: true`,
a long-running server command's (`mcp.serve`/`a2a.serve`) non-Cli metadata
reply — applies unchanged, since `dispatch` routes through the SAME
dispatcher every door calls. This module introduces no daemon-specific
permission table, and none is planned. Untrusted input stops at the parse/
validate boundary: a malformed line gets one error reply and the connection
survives; a request line over the 1 MiB cap is dropped mid-stream (checked
incrementally, not only after a `\n` finally arrives) with one error reply
first when a peer is still there to receive it.

---

## 4. Stage file formats — **v0**

Two live-side (rehearsal) state trees, both gitignored runtime, never
committed, never load-bearing for the nix build (enforced by
`checks.no-song-read`), split by who owns them (command-defrag lane,
2026-08-27 — root `AGENTS.md`'s "Aoide (core) vs AoideOS/Lyra" boundary
applied to the stage tree itself):

- **`song/stage/`** — rice/paint staging. `livery.json`, `mode.json`, the
  draft-routing symlink target, `grimoire.json`. Emitted by the notes
  package, `lyra rice`/`cover`/`draft`, and QML itself (`grimoire.json`);
  read by Quickshell. This is lyra's tree.
- **`state/stage/`** — CONDUCTING state: `sessions.json`, `hooks.json`,
  `projects.json`, `graph.json`, `pending.json`, `herald.json`. Written by
  shellbridge and the `aoide`/`aoided` binaries (`aoide graph`, `aoide
  herald`); read by the conductor TUI and by Quickshell. This is core's
  tree — the `aoide`/`aoided` binaries alone read and write it, never
  `lyra`, and QML READS it (display only, CONTRACTS.md §0's corollary) but
  never writes it.

**The runtime root (L-C2, lyra-carrier lane, task #107).** Every runtime
tree below — `song/stage/`, `state/` (and `state/stage/`), `run/qml/`,
the composed `songbook/` — hangs off ONE root: `$AOIDE_ROOT` when set to an
**absolute** path, else `<home>/.aoide` derived from `$AOIDE_USER`/`$HOME`.
Nix-free — core code carries this default with no NixOS assumption.
`~/Aoide` is no longer a runtime root on any host; it is purely the dev git
checkout, reached through the separate `$AOIDE_FLAKE_ROOT` seam (default
`<home>/Aoide` — §1's `flake_root()`, unchanged by this lane) that
`soundcheck`, `rice declare`'s commit-in step, and the songbook `nix eval`
registry regen all read the checkout through.

**The shipped score templates (L-C3, same lane).** A repo-less host — no
`$AOIDE_FLAKE_ROOT` checkout on disk at all — has nothing to compose FROM
and no `flake.nix` for the songbook `nix eval` regen above to evaluate
against. `$AOIDE_SONG_TEMPLATES` names a THIRD dir for exactly that case:
`<templates>/<song>/livery.json` is what `rice compose --from <song>`
(`aoide-song`'s `commands::rice`) falls back to reading once
`songbook_dir(from)` comes up empty; `<templates>/manifest.json` and
`<templates>/registry.json` are what `aoide-song::widgets`'s registry/
manifest regeneration falls back to reading once `flake_root()` has no
`flake.nix` to shell `nix eval` against — both nix-free, never a `nix`
invocation. Two tiers, absolute-path-wins like every override above:
`$AOIDE_SONG_TEMPLATES` itself, else a sibling of `current_exe()`'s
directory (`<exe_dir>/../share/lyra/songbook`, gated on that directory
actually existing — `aoide_protocol::bin`'s sibling-binary resolver shape,
applied to a directory). On a NixOS host the env tier always wins, but only
where it can be READ: `AOIDE_SONG_TEMPLATES` is paint data (only
`aoide-song` reads it, only `lyra` links `aoide-song`), so it is wired ONLY
onto units whose process execs `lyra` — `modules/nucleus/shellbridge.nix`'s
main `shellbridge` service — plus `modules/nucleus/aoided.nix`'s
`environment.sessionVariables` (an operator's own interactive `lyra rice
compose`), itself gated on `aoide.lyra.enable` so a host that never installs
`lyra` never carries the var into its shells either. It does NOT ride the
core-only units (`aoided` itself, `aoide-mcp`/`aoide-a2a`/`aoide-usage`/
`aoide-pair-watch`, `aoide-graph-reap`, `aoide-secrets-watch`, the melete
adapter) the way `$AOIDE_ROOT`/`$AOIDE_FLAKE_ROOT` do — those exec plain
`aoide`, never `lyra`, so carrying `pkgs.lyra-songbook`'s closure onto them
would drag paint data onto a headless core for nothing. `pkgs/lyra-songbook`
bakes a verbatim copy of the committed `song/songbook/` tree plus
`manifest.json`/`registry.json` (via `lib/songbook.nix`, the SAME generator
the checkout-host `nix eval` path and the quickshell facet's own build-time
carry both call) at nix build time. The sibling-of-binary tier exists for a
future non-nix tarball install instead. Neither tier resolving is a taught
error naming both locations, never a panic; a song with a `_widgets/` shelf
(borrowed widget ownership) still needs a real flake checkout even with
templates present — resolving a shelf requires `composeSong` in the nix
evaluator, which the templates fallback cannot run.

The registry/manifest regeneration is a THREE-layer merge, not a bare
baked-baseline-plus-current-song write: the baked file is the baseline,
then the EXISTING on-disk file's entries for every song that still has a
directory in the host songbook are overlaid on top of it (a song whose
directory is gone is pruned, never kept immortal), then the
currently-staged song's own freshly-scanned entry is patched in last,
winning over both. Without the overlay layer, composing and staging a
second song on a repo-less host would silently drop the first song's
entry — it lives in neither the frozen baseline (a runtime composition) nor
the second song's own scan.

**Stage-dir resolution (the CLI ↔ unit seam), one function per tree.**
`song/stage/`: precedence `$AOIDE_STAGE_DIR` when set to an **absolute**
path → else `$AOIDE_ROOT/song/stage`. `state/stage/`: same
`$AOIDE_STAGE_DIR` absolute-path precedence (the one env var the systemd
unit and every test already set continues to win for BOTH trees at once — a
relocated stage tree relocates as a unit, and stays authoritative ABOVE
`$AOIDE_ROOT`) → else `$AOIDE_ROOT/state/stage`, itself
`$AOIDE_ROOT/state` (`$AOIDE_STATE_DIR` when absolute, else composed off
`$AOIDE_ROOT`) with `/stage` appended. A relative or empty override is
ignored for either tree, or for `$AOIDE_ROOT` itself (a runtime path is
never resolved against an arbitrary cwd). On the default layout, with no
override set, the two trees resolve to two different directories under the
same root, as intended; an `$AOIDE_STAGE_DIR` override (every test fixture
that sets one) still names one directory for both, exactly as it did before
the split.

**The one-shot migrations — two, composing.** (1) S1 (command-defrag lane):
the first time a process resolves `state/stage/` under the no-override
fallback, it moves each of the six conducting files found at the sibling
`song/stage/` location (wherever that currently resolves) into the new one
— `rename` when possible (same filesystem), copy-then-remove-source
otherwise — skipping any file already present at the new path (a fresher
`state/stage/` file is never clobbered) and never touching a rice file
(`livery.json`, `mode.json`, `grimoire.json`, `cover.json`) even when it
sits in the same old directory. Guarded to run at most once per process;
idempotent across repeated boots (`aoide-storage`'s
`migrate_conducting_stage`, `fs.rs`). (2) L-C2: moves a pre-L-C2 host's
`~/Aoide/{song/stage,state,log}` trees wholesale into the new root's
equivalents — each piece gated independently on its OWN override
(`$AOIDE_STAGE_DIR`/`$AOIDE_STATE_DIR`/`$AOIDE_AUDIT_LOG`) being unset, so a
host that already relocated one tree never has a sibling moved out from
under it. Idempotent, and deliberately NOT wired into any path getter — the
three real binaries (`aoide`, `aoided`, `lyra`) call it explicitly, once, at
process start (`fs::migrate_root_once`; see that function's own doc for why
a path getter is the wrong place to hang a process-wide side effect, the
kind `with_stage_lock`'s pervasive fan-out makes unsafe there). Composes
correctly with S1 in either order: whichever migration a given process's
`main()` runs first, S1 always resolves its OWN old/new dirs dynamically
via `stage_dir()`/`state_dir()`, so it finds files wherever L-C2 most
recently left them.

### `$AOIDE_ROOT/config.toml` — **v0** (task #135 P-C, the portable runtime config)

The one file in this section that carries INTENT rather than state. Every
other format below records what HAPPENED — a session's live record, a peer's
committed grant, a parked ask, a switch that was flipped. This one records
what an operator WANTS, ahead of anything happening, and nothing ever
migrates between the two: a value written by code lives in `state/*.json`, a
value written by a human lives here.

It exists because core is portable. `aoide`/`aoided` are cargo-buildable on
any Linux, with no nix shell-outs and no NixOS assumption (root `AGENTS.md`),
so a CORE command's configuration cannot live in a NixOS module option — on a
non-nix host that option does not exist, and "rebuild to change a grant" is
not an operation. `peer_store::Peer.hub` set the precedent: a runtime field,
set by a runtime command, portable by construction.

**Resolution**, two tiers, absolute-path-wins like every other override in §4
above, and IDENTICAL at every entry point (`aoide`, `aoided`, the stdio MCP
façade all reach one function, so there is no per-door variant to drift):

1. `$AOIDE_CONFIG` set to an absolute path → that file, **managed** —
   rendered read-only by something else, so `aoide config set` refuses it
   with a taught error naming both it and the unmanaged path to use instead.
2. else `$AOIDE_ROOT/config.toml` → **unmanaged**, the operator's own
   editable file and the one `aoide config set` writes.

A relative or empty `$AOIDE_CONFIG` is ignored outright rather than resolved
against a cwd. A MISSING file is every default, never an error (the same
tolerate-missing stance `state/advertise.json` and `state/peers.json` hold). A
file that EXISTS but does not parse, carries an unknown key or section, or
holds a value outside its vocabulary is a LOUD error naming the offence — this
file carries grants, so a typo must never resolve to a silently-ignored key.

**TOML, one runtime format.** This is the one file a human edits, and the
reasoning behind a grant belongs beside it, which JSON has nowhere to put.
YAML's implicit coercion and whitespace sensitivity are the opposite posture
to the one a grants file needs. `pkgs.formats.toml` renders the nix side
through the same format, so both worlds speak one schema.

**Nix POINTS, never copies.** `modules/nucleus/config.nix`'s
`aoide.config.settings` renders through `pkgs.formats.toml` to a read-only
store path and `AOIDE_CONFIG` names it (exported into interactive shells via
`environment.sessionVariables`, and into every aoide user unit via the user
manager's `DefaultEnvironment`). Immutability IS the provenance: there is no
marker field to lie or go stale, and the two worlds never write the same path,
so a rebuild structurally cannot eat a CLI edit. Management is
**whole-file-or-nothing** — `aoide.config.enable` is off by default (same
house policy as every other door), and partial management (nix owning one
section while the CLI owns another) is deliberately not offered: two writers
on one document is the split-brain the design exists to avoid.

Schema v0 — exactly one section, and a new section lands with the consumer
that reads it, never ahead of one:

```toml
# Comments are the point of the format: this is the one file a human edits.
[pairing]
defaultGrant = ["read"]
```

- `pairing.defaultGrant` (list of strings, default `["read"]`) — the
  capability set a peer is granted when it FIRST becomes verified. The
  vocabulary IS §7's own closed peer-capability set
  (`aoide_storage::peer_store::PEER_CAPABILITIES`, `"read"`/`"spawn"` — the
  same one `peer allow` enforces), never a second list; an unknown
  capability is refused by name, exactly as an unknown key is. Read by both
  ceremony commit sites (`approve_inbound`/`approve_outbound`, through
  `resolve_grant`) unless that invocation named `--allow`; a config that does
  not load REFUSES the commit rather than falling back to the built-in
  default, because the one file carrying grants must fail loudly.

The schema lives in code as a walkable TABLE (`aoide_storage::config::SCHEMA`
— sections, keys, each key's vocabulary, and how to read it off a typed
config), and the validator, the `aoide config` listing, and `aoide config
set` all walk that one table. `aoide config set <section>.<key> <value>` is
the only writer: it refuses a managed config, an unknown key, an
out-of-vocabulary value, and a config already on disk that does not load —
each with a taught error and nothing written — then edits the file's own TEXT
in place (`toml_edit`, so an operator's comments survive a write that a
serialize-the-struct round trip would erase), re-parses the result through
the identical gate the next read applies, and commits it atomically.

### `song/stage/livery.json` — **v0**

The resolved, flattened note values for Quickshell (QML reads this; hot-reload
at rehearsal). Derived from the same `aoide.livery` as the baked `rice.nix`
fan-out, so staged and adopted state cannot diverge. `stage/livery.json` is
the canonical name (the stage file was renamed by the livery merge);
writers mirrored to the legacy name and readers fell back during the
transition window, so a running desktop never read a missing stage file. The
mirror + fallback were dropped in Phase 4 of the livery merge —
`stage/livery.json` is the sole stage note file.

Beyond `lyra rice stage <name>`/`cover set`/other emitters writing this
live, it is also **seeded from the active song's committed notes on every
activation** (`home.activation.aoideSeedStage`,
`modules/facets/quickshell/default.nix`) — so a host that boots without ever
running `rice stage` still has a correct live stage twin from boot.

**Additive in v0:** this path MAY be a SYMLINK rather than a plain file —
`rice mode draft <name>` (§4's `stage/mode.json` entry) routes it into a
saved `song/songbook/<song>/drafts/<name>/livery.json`. Readers never need
to care (following a symlink is transparent to any read); writers going
through `aoide_storage::fs::atomic_write` transparently write through it
too. `rice mode stage`/`rice mode declarative` remove the symlink (leaving
a plain real file) whenever they run.

```json
{
  "schemaVersion": "0",
  "palette": { "bg": "#1e1e2e", "fg": "#cdd6f4", "accent": "#89b4fa", "urgent": "#f38ba8" },
  "bar":    { "bg": "#1e1e2e", "fg": "#cdd6f4", "accent": "#89b4fa" },
  "notif":  { "bg": "#1e1e2e", "fg": "#cdd6f4", "urgent": "#f38ba8" },
  "window": { "border": "#89b4fa", "borderInactive": "#1e1e2e" }
}
```

Component values are **fully resolved** in the stage file (fallbacks already
applied) — Quickshell reads concrete colours, never `null`.

Writes are atomic (write-temp-then-rename) so a hot-reload never reads a torn
file.

**Additive in v0:** the staged file MAY carry an optional top-level `song`
field (string) — the name `lyra rice stage <name>` was invoked with. Set by
`handle_rice_stage` (mirrors the `parentSessionId` additive precedent in
§4's sessions.json). Absent means "no song identity" (a notes file staged some
other way). `LiveryState.qml`'s `songName` property reads it to resolve
per-song flavor widgets (§5) — readers must tolerate both forms.

**Additive in v0:** the staged file MAY also carry an optional top-level
`geometry` block, mirroring §1's geometry tier (`gapsOut`/`gapsIn`/
`borderSize`/`rounding`/`blurEnabled`/`blurSize`/`blurPasses`, each `nullOr`).
Absent means "this song carries no geometry opinion" (§1's additive-optional
tier). `lyra rice stage` reads it (alongside `window.border`/
`borderInactive`) to build its best-effort `hyprctl keyword` batch — a missing
block, or a missing/null field within it, is skipped rather than defaulted;
readers must tolerate both forms.

### `song/stage/mode.json` — **v0**

Which of THREE modes the rice system is in — concepts/Self-Ricing's
mode-toggle extension (`RiceMode`: `Staging | Declarative | Draft`). Absent
means `declarative` — the safe default, since nothing has ever unlocked
staging writes; never an error.

- **`staging`** — hot-load unlocked: `rice stage`/`cover set` write live, as
  plain real files.
- **`declarative`** — nix/home-manager is the only writer; staging writers
  refuse.
- **`draft`** — `stage/livery.json` is a SYMLINK routed into
  `song/songbook/<song>/drafts/<name>/livery.json` via `rice mode draft
  <name>`. `rice stage`/`cover set` still write normally — neither is
  symlink-aware; the routing is transparent (see below).

```json
{
  "mode": "draft",
  "song": "sonata",
  "draft": "neon-night",
  "stagingSong": "etude",
  "since": "2026-08-14T00:00:00Z"
}
```

`song`/`draft`/`stagingSong`/`since` are optional (omitted, not `null`, when
absent — the `SessionRecord`/`Peer` Option convention). `song` names the
song a `staging`- or `draft`-mode session is pointed at. `draft` is
`Some(name)` **if and only if** `mode == "draft"` — every other mode always
carries `draft` absent; nothing in the codebase ever sets one without the
other. `rice mode draft <name>` sets both together on entry; `rice mode
stage`/`rice mode declarative` both clear `draft` (and tear the routing
symlink down) whenever they transition OUT of `draft` mode. `rice draft drop
<name>` refuses rather than clearing this field, if `<name>` is the
currently-routed draft (see below). Nothing else branches on `draft` — it
is purely observational, surfaced by `rice mode status`. `since` is when the
current mode was entered.

**Additive in v0 (khoa, 2026-08-17):** `stagingSong` remembers the last song
actively used in `Staging` mode — distinct from `song`, which `rice mode
declarative` legitimately overwrites to reflect whatever's now actually
active. Locking declarative must never touch or clear `stagingSong`, so a
later bare `rice mode stage` (no name — what the bar toggle sends) can still
resolve back to what was being staged, instead of losing that memory the
moment a declarative round-trip overwrites `song`. Absent when never set (a
fresh `mode.json`, or one written before this field existed); readers fall
back to resolving off `stage/livery.json`'s own `"song"` field in that case.

### `song/songbook/<song>/drafts/<name>/` — **v0**

A saved rice-draft: a scratch snapshot for iterating on more than one live
variant of a song without declaring any of them (concepts/Self-Ricing's
draft extension). Nested under the song it varies — `<song>` is always
resolved off the CURRENT stage, never a free-floating namespace, because a
draft is fundamentally a variation of an ALREADY COMPOSED song.

```
song/songbook/sonata/drafts/neon-night/
├── livery.json   a real file — the draft's own content
└── cover.json    snapshot of stage/cover.json, ONLY if the stage had one
                   at fork/save time (cover.json is NEVER symlinked/routed —
                   only livery.json is)
```

No metadata file: the draft name is the directory name, the base song is
the directory it's nested under, and saved-at is `livery.json`'s mtime.
Gitignored (`song/songbook/*/drafts/`, same category as `song/stage/`) and
banned from nix-eval reads (`lib/checks.nix`'s `noSongRead` — matched by
regex, `.*/song/songbook/[^/]+/drafts/.*`, since the runtime dir nests at a
variable depth a flat infix can't name) — a draft is durable scratch,
**never committed or declared truth**; that distinction from
`song/songbook/<name>/`'s own committed files is the entire point.

**Reached by ROUTING, not copying.** `rice mode draft <name>` (§4's
`stage/mode.json` entry) points `stage/livery.json` at a SYMLINK into the
draft's own `livery.json` — forking it from whatever's currently in the
stage first if the name is new, reusing the SAME write `rice draft save`
performs. From then on, every writer of `stage/livery.json` (`rice stage`,
a hand-edit, Quickshell's own FileView reload) transparently lands in the
draft file, because `aoide_storage::fs::atomic_write` resolves and writes
through a symlink at its destination rather than letting POSIX `rename()`
replace it — general behavior in that one function, not draft-specific.

`rice draft save <name>` is a SEPARATE, mode-independent command: it forks
whatever's currently live (reading transparently through a routing symlink
if one is active) into a new-or-updated draft snapshot without switching
modes — upserting (re-saving an existing name overwrites its `livery.json`
and, if the current stage no longer carries a cover, removes a stale
`cover.json`). `rice draft drop <name>` deletes a draft outright; a missing
name is an error, not idempotent-silent, and dropping the CURRENTLY-ROUTED
draft is refused (`draft-is-live`) rather than silently also tearing down
the routing and falling back to `staging` — switch modes first
(`rice mode stage`/`rice mode declarative`), then drop it.

### `state/stage/sessions.json` / `hooks.json` — **v0**

The shellbridge roster + live hook phases (full field tables in
`modules/nucleus/shellbridge.nix`). Session records: `{ sessionId, agent,
windowAddress, cwd, state, startedAt }`; hook records: `{ sessionId, phase,
updatedAt }`.

**Additive in v0:** a session record MAY carry an optional `parentSessionId`
(string) naming the session that spawned it — the graph's spawned-by edge. Set
by `aoide graph link` (cycle-checked), cleared by `aoide session prune` when the
parent is removed. Absent means "no spawned-by edge"; readers must tolerate
both forms, and rewriters must round-trip fields they do not know.

**Additive in v0:** a session record MAY also carry an optional `contextTokens`
(integer) — the input-side token count (`input_tokens +
cache_creation_input_tokens + cache_read_input_tokens`) of the freshest
`type:"assistant"` line's `message.usage` in the session's on-disk Claude Code
transcript, i.e. "how full is this session's context window at its last
request" (`output_tokens` is deliberately excluded — that's what the turn just
produced, not what sat in the window when it was sent). Refreshed at the same
hook boundaries as `model`/`say`. Mirrors the `needsSudo`/`model` additive
precedent: absent for shells and until the session has produced an assistant
turn, and readers must tolerate both forms and round-trip fields they do not
know. The dock reads the published `contextCeiling` field (below) to turn the
raw count into a meter, rather than deriving its own percentage ceiling from
`model` client-side.

**Additive in v0:** a session record MAY also carry an optional `contextCeiling`
(integer) — the context-window ceiling, in tokens, for the session's current
`model` (200k or 1M, per the model-id split `aoide_protocol::context_ceiling_for_model`
encodes). aoide computes this from `model` at the same hook boundaries as
`contextTokens`/`model`; clients read the published fact instead of deriving
their own ceiling. Re-derived whenever `model` changes, so a mid-session model
switch re-caps the meter automatically. Same lifecycle as `contextTokens`:
absent for shells and until the session has produced an assistant turn, and a
legacy record without it falls back to a conservative 200k client-side.

**Additive in v0:** a session record MAY also carry an optional `tool` (string)
— the agent's latest TOOL CALL as a one-line label (`"Bash: cargo test"`): the
tool's name plus the first argument naming its subject, read off the same
transcript tail as `say` at the same refresh points (hook boundaries, and every
`aoide session reap` sweep). Distinct from `activity`, which stays what it was:
the tool running RIGHT NOW, hook-set and cleared when the turn settles. `tool`
survives that settle, so a resting session still shows what it last reached
for; a reader wanting "is something running" reads `activity`/`state`, not
this. Same lifecycle as `say`: absent for shells and until the session's first
tool call, readers tolerate both forms and round-trip fields they do not know.

**Additive in v0:** a session record MAY also carry an optional `logPath`
(string) — the absolute path to the pty-master transcript of a HEADLESS
`aoide conduct` session (`state/sessions/<sessionId>.log`, below). Stamped
once, right after the session registers, by `aoide conduct --headless`; every
INTERACTIVE session (conduct with a real controlling tty, a hook-only agent)
never sets it. Absent means "no headless log" (the common
case); readers must tolerate both forms and round-trip fields they do not
know.

**Additive in v0:** a session record MAY also carry an optional `petname`
(string, `<word>-<word>`) — a human-readable display handle minted once, at
record creation, from a fixed adjective/noun wordlist. Unique only among
CONCURRENT live sessions (a `state:"done"` record's name is free to reuse);
never re-minted on a later update/resume/restart, never a lookup key, and
never encodes the host or a session's role — those are resolved separately at
render time. `sessionId` stays the sole canonical key everywhere. Absent means
"minted before this field existed" (a legacy record); readers must tolerate
both forms and round-trip fields they do not know.

**Additive in v0:** a session record MAY also carry an optional `kind`
(string) — what KIND of thing the record is, published so a widget never
infers it from the `agent` string: `agent` (a Claude/agent session), `shell`
(a conducted terminal), `subagent` (a Task the agent spawned — a leaf of the
conductor tree, its own child record with its own harness-native-subagent
classifier, parented to the harness session from the hook payload, closed on
its own stop event and swept if its parent ends), or `a2a` (§6 — an external
A2A agent folded into the session DAG). Absent means "unclassified" (a
legacy record); `upsert_session` backfills it once, at the next touch, from
`agent != "shell"`. `conductable` (bool) marks a session spawned under `aoide
conduct` (it owns a PTY + control socket) — the same-window eviction and the
reaper's own dedup pass both treat a `conductable` record as the control-
socket owner, never a foreground-agent duplicate, regardless of its
published `kind`.

**Additive in v0 (task #89):** a session record MAY also carry an optional
`hookAncestry` (array of integer pids, at most 8, self-first) — the
hook-firing process's own `/proc` `ppid` walk, stamped ONCE at a hook
session's own SessionStart/self-heal registration (`aoide session hook`)
and never re-stamped afterward (a birth fact, not a live signal). A
later `conduct`/`spawn` registration with no explicit `--parent` walks
ITS OWN `/proc` ancestry and looks for a live, not-`done`, agent-kind session
whose `hookAncestry` intersects it — the closest (deepest) matching ancestor
wins as parent, so a nested headless `conduct`/`spawn` launched from inside
an agent's shell tool lands as that agent's CHILD rather than, via a stale
ambient `AOIDE_SESSION_ID`, as a sibling of the enclosing terminal. Absent
means "no ancestry recorded" (a legacy record, or any non-hook registration);
readers must tolerate both forms and round-trip fields they do not know.
Consumed internally for parent resolution only — never rendered.

**Additive in v0 (task #89, review round 2):** a session record MAY also
carry an optional `headless` (bool, default/absent means `false`) — a
PERMANENT, self-reported registration fact stamped exactly once, by `aoide
conduct --headless` right after registration, and never cleared afterward.
It is deliberately distinct from "`windowAddress` happens to be empty right
now": the first implementation of windowless lineage (below) keyed off the
empty address alone, and on a live compositor a headless wrap's OWN record
still passes a `/proc` ppid-ancestry walk straight through to its ENCLOSING
terminal (`setsid()` detaches the tty/session-leader relationship, not the
OS parent-child one), so the unconditional discovery call would silently
stamp that terminal's window onto the wrap and re-poison the whole lineage
chain. `headless` closes that gap: it is `true` for a headless wrap
regardless of what its `windowAddress` field happens to hold, so an errant
address can never be mistaken for a real window. Absent means "not a
headless wrap, or a legacy record" (ordinary address-emptiness still
applies); readers must tolerate both forms and round-trip fields they do
not know.

**Additive in v0 (P-D7, `docs/architecture/AOIDED.md`'s "L5"):** a session
record MAY also carry an optional `harnessSessionId` (string) — the
harness's OWN session id, straight off the raw hook payload's own
`session_id` field. Stamped by `aoide session hook` on EVERY event
that carries one — not only at registration, and not gated on whether the
event maps to a graph action at all (an event the door has no other use for
still stamps it, as long as a record with that id already exists) — and
regardless of whether it equals this record's own `sessionId` (for a
hook-registered record the two values are the same today, since the record's
own id is minted FROM this field; the field is still stamped unconditionally
so a later consumer, e.g. `aoide resurrect`'s ledger reader, never has
to know which registration path produced a given record to find the id a
harness's own resume flag needs). Absent means "no hook has touched this
record yet" (a legacy record, or a wrap with no hook-driven agent inside
it); readers must tolerate both forms and round-trip fields they do not
know.

**Additive in v0 (P-D8, `docs/architecture/AOIDED.md`'s "L5"):** a session
record MAY also carry an optional `resumedFrom` (string) — the durable
session ledger's `sessionId` (see `state/session-ledger.jsonl` below) this
record was revived from, stamped once by `aoide resurrect` (or the
daemon's own boot-time auto-resume trigger, which calls the same command
core in-process) right after the resurrected session registers. Session
ids are never recycled: a resurrected session always mints a FRESH
`sessionId`, and `resumedFrom` is the only link back to the ledger entry
it continues. A (re)staged `graph.json` — automatic at every project/session
mutation, or via `aoide session prune`'s manual resync — projects a populated
`resumedFrom` as an additive `resumed` edge (see `graph.json` below) beside the ordinary
`spawned`/`anchors` edges. Absent means "not a resurrection" (the ordinary
case, and every legacy record); readers must tolerate both forms and
round-trip fields they do not know.

**Additive in v0 (P-P3, `docs/architecture/PAIRING.md` decision 7; record
authority tightened at LANE IDENTITY P-ID0, G16/G5, review round 1):** a
session record MAY also carry an optional `origin` (string) — `"peer:<name>"`
for a session `aoide-server`'s A2A door spawned on behalf of an identified,
PAIRED peer (§6's P-P3 amendment above). `aoide_conduct::graph::
session_store::stamp_origin` (now `pub`, crossing the crate boundary) is the
sole STAMP function, change-once like `headless`, with exactly two legitimate
callers: `aoide-server`'s `a2a::do_spawn` calls it DIRECTLY on the
just-spawned record, from the door where the peer name is actually
authenticated — the only place a `peer:*` value may originate. `graph/
conduct.rs::session_conduct` calls it for a LOCAL-CLASS value off its own
inherited `AOIDE_SESSION_ORIGIN` env, and REFUSES a `peer:*` shape read from
that env (a taught refusal, not a panic): inherited env is exactly what a
same-uid process can set on itself before invoking `aoide conduct` directly.
A THIRD path exists and is refused the same way, not silently trusted:
`aoide resurrect` (`graph/resurrect.rs::origin_to_carry`) reads a revived
session's `origin` back off its OWN durable ledger entry
(`state/session-ledger.jsonl`, below) to restore a LOCAL-class session's
class across a revival (G6) — but `origin_to_carry` refuses to carry a
`peer:*` shape found there, because the ledger is an UNSEALED append-only
file: a same-uid process can append a line claiming `origin:"peer:X"` and
then run the ungated local `resurrect`, which has no door and no seal behind
it to re-mint that authority. So at the record-STAMP layer, `peer:*` now
comes ONLY from the door or is refused everywhere else it could be read
back in. **This does not mean the on-disk files are sealed.**
`state/session-ledger.jsonl` and `sessions.json` are both still plain,
same-uid-writable files — a hand-crafted `sessions.json` entry claiming
`"origin":"peer:X"` is still readable-as-truth by anything that reads the
file directly (a future consumer, a dashboard, a careless `jq`); P-ID0 only
closes the STAMP path a live `aoide resurrect`/`conduct` invocation takes.
Sealing the files themselves so a forged on-disk value can be told apart
from a genuine one is P-ID1 (the daemon-signed credential, below) — minted
and stored, verified on the per-session control socket's own accept and
consumed by the send gate as of P-ID2, and both remaining sockets
(shellbridge's verdict socket, `aoided`'s own dispatch socket) get a
peercred floor of their own as of P-ID3 (below). Absent means "not a
peer-initiated spawn" (a locally-launched `conduct`/`spawn`, the ordinary
case, and every legacy record); readers must tolerate both forms. Unlike
`resumedFrom`, `origin` gets NO `graph.json` projection — like `headless`/
`hookAncestry`, it is consumed internally (projected verbatim into
`state/session-ledger.jsonl`'s own `origin` field at session exit, below,
AND read back on `aoide resurrect` to carry a LOCAL-class session's own
provenance forward onto its revived record — G6, same phase, `peer:*`
excluded per above) rather than rendered into the live graph. **The RAW field is attribution,
never a gate**: a same-uid process can still forge a LOCAL-class `origin`
string, so nothing gates a security decision on the field as read off
disk. The AUTHENTICATED form is the sealed credential below (P-ID1+): a
consumer trusts only the `originClass` carried inside a seal that VERIFIES
against the daemon's live key — the secrets broker's origin gate (P-ID4,
the "Secrets home" section's origin-gate paragraph) is the model consumer,
and the consumer NAME presenting a request stays unauthenticated either
way (a separate, unbuilt axis — the lane accounting below). What P-ID0
closes: the specific `peer:*` forgery shape (a local process claiming to
BE a peer-spawned session, whether via env or via an unsealed ledger line)
is refused at every record-STAMP path this codebase drives; what the files
themselves are and are not is the paragraph above, and the
sweep-relaunder residual (the origin-gate paragraph in "Secrets home")
names the forged-LOCAL-row shape that still slips through under OQ1-A.

**Additive in v0 (LANE IDENTITY P-ID1/P-ID2) — the sealed session
credential.** A session record MAY also carry an optional `seal` (string,
hex) — an ed25519 signature (`aoide_storage::sealed_id::mint_seal`) over a
canonical `\x00`-separated string built from five fields, each followed by
the separator: `sessionId`, `pid`, `pidStarttime`, `originClass`,
`issuedAt` — and an optional `sealedIssuedAt` (number, unix seconds),
stamped alongside `seal` in the SAME call. `sealedIssuedAt` exists because
`issuedAt` has no live fact a verifier can re-derive it from (unlike
`pidStarttime`, below): without storing it, checking a signature would mean
brute-forcing every plausible mint instant, which P-ID1's own original test
suite did as a test-only expedient before this field existed. Both fields
share ONE lifecycle — always `Some` together, always `None` together.
**`sessionId`/`originClass` ride VERBATIM — no trim, no case-folding**
(review fix: the original shape copied `wire_auth::canonical_string`'s
trim+lowercase wholesale, but `sessionId` is the session store's own
case-sensitive primary key and `originClass` is P-ID4's own origin-gate
lookup key — folding either would let a seal minted for one exact identity
verify against a differently-cased one). `pid`/`pidStarttime`/`issuedAt`
are their plain canonical decimal digit strings. `pidStarttime` is
`/proc/<pid>/stat`'s field 22 (1-indexed) — at MINT time read by
`aoide_conduct::graph::pid_starttime` over whatever pid the record carries;
at VERIFY time (P-ID2, below) a verifier does NOT trust a stored
`pidStarttime` at all — it re-reads `/proc/<pid>/stat` FRESH for the SAME
`pid` and reconstructs the identity from that live value, never from
anything persisted. This is the pid-reuse defense: a stale mint-time value
for a pid the OS has since recycled to a different process simply fails to
match the live read. **A `pidStarttime` of `0` at mint time (the documented
degrade when `/proc/<pid>/stat` is unreadable then — a pid that had already
vanished) is UNVERIFIABLE, never "verified": no live process ever reports
starttime `0`, so a verifier's fresh `/proc` read can never produce a
matching `0` — a stored `0` cannot be revalidated, not a weaker-but-valid
seal.**

**The signing key is NOT `state/identity/`'s on-disk peer-wire key.** Under
OQ1-A (the User-answered threat-model question, LANE IDENTITY's design pass)
the seal's secrecy rests on PROCESS LIVENESS, not file permissions: `aoided`
mints a SEPARATE ed25519 keypair once per process, at startup
(`aoide_storage::identity::mint_ephemeral`), and holds it ONLY in memory —
never written to disk, never the `identity/ed25519.key` file above. The
reasoning: a same-uid attacker (this codebase's whole threat model, see
"LANE IDENTITY (#63)" in the plan file) can read any `0600` file under the
operator's own uid, including the on-disk peer-wire key, so a seal signed
with THAT key would not be secret against it. It cannot, however, read
another live process's heap without `ptrace` — and `ptrace` of a non-child
same-uid process is exactly what Yama `ptrace_scope>=1` blocks by default on
the target host. **Yama is the trust root; if it is off, this degrades
honestly to liveness-only** (still requires a live `aoided` process to have
signed it, but no longer resists a same-uid `ptrace` attach) — stated here
rather than hidden, the same honesty discipline `origin`'s own paragraph
above holds for its own boundary. **Verifying a seal needs the daemon's
CURRENT public key, fetched fresh over its existing `ping` reply's
`sealPubkeyHex` field** — never cached, never read from a file: a pubkey
sitting next to the seal it vouches for on the SAME same-uid-writable
`sessions.json` would let whoever can already forge a seal also forge the
"trusted" key that verifies it, which is cryptographically void, so a live
round trip to the actual running daemon process is the only channel worth
asking (`aoide_storage::attest::daemon_seal_pubkey_hex` — the ONE
implementation as of P-ID4, with `aoide_client::daemon::
daemon_seal_pubkey_hex` delegating; the walk + fresh-starttime verify live
beside it in the same module, shared by the send gate and the secrets
broker's origin gate — the credential's consumers, see the "Secrets home"
section's origin-gate paragraph for the second one). **This channel is
only as trustworthy as the daemon socket's same-uid exclusivity, and under
OQ1-A that is NOT attacker-proof**: `bind_socket` unlink-then-binds with
no `flock`/pidfile guard, so a same-uid attacker can already race or evict
the real listener and bind its own in its place, serving a forged
`sealPubkeyHex` from an impostor `ping` reply. **P-ID3's dispatch-socket
peercred floor does NOT close this** — it is a CROSS-uid floor (see
below); a same-uid attacker racing or evicting the real listener is by
definition the SAME uid the floor admits, so this residual is
OQ1-A-inherent and stays open (a `flock`/pidfile guard on `bind_socket`
would be the actual fix, not attempted here). This buys the attacker
nothing beyond the door already open (the raw per-session socket a
same-uid process can already connect to directly, OQ1-A-inherent): the
seal's value is against a caller that follows its OWN tooling honestly
(`aoide send`, a script, an agent that never forges argv/env but also
never opens a raw socket) — it does not, and was never claimed to, resist
a same-uid attacker willing to impersonate the daemon itself. An
unreachable OR impostor daemon still means every seal is UNVERIFIABLE
against ANY key the caller can independently confirm is the real one, not
a fallback to trust — the credential's whole security property rests on
a live, genuine daemon existing to ask, and confirming genuineness is
outside this phase's scope.

`aoide_conduct::graph::session_store::stamp_seal` is the sole STAMP
function (change-once, the same shape `stamp_origin` set the precedent
for), with TWO legitimate callers as of P-ID2: `aoide-server`'s daemon
`dispatch` handler (P-ID1's original caller — mints right after a
successful `session start` dispatch whose record already carries a pid),
and the daemon's own tick-driven `seal_unsealed_live_sessions` sweep,
which closes the gap P-ID1 shipped as scaffolding: the common real-world
registration — `aoide conduct -- <agent>` — writes its record DIRECTLY,
never through `dispatch` at all, so under P-ID1 alone it was never sealed.
The sweep seals ANY live, pid-carrying session missing one within one tick
(~1s) of registering, regardless of which path registered it.

**The per-session control socket's accept reads `SO_PEERCRED` and the send
gate consumes the seal, as of P-ID2.** `aoide_conduct::graph::conduct`'s
per-session injection socket (`$XDG_RUNTIME_DIR/aoide/session-<id>.sock`,
above) reads the CONNECTING process's kernel-truth uid/pid off every
accepted connection and refuses one outright — never forwarded to the
pty — when the CONNECTOR'S OWN nearest live registered session (walking
its ancestry nearest-first, `aoide_conduct::graph::identity::
is_self_originated`) resolves to the socket's OWN session. **Deliberately
NOT "the target's pid appears anywhere in the connector's ancestry"**
(review round 1 MUST-FIX): `session_conduct` registers WITHOUT detaching,
so a legitimate CHILD session's pid is a genuine OS descendant of its
parent's registered pid — a raw-ancestry-containment check refused the
single most common flow, a child sending to its own live parent, with a
bare broken pipe `--yes` cannot route around (this runs downstream of the
gate, at the TARGET's own accept). Resolving the connector's NEAREST
session instead means a nested child's own registration is found first,
never its parent's, so only a connection whose nearest resolvable session
genuinely IS the target gets refused; an unresolvable connector fails
OPEN (allowed) — this is a narrow UX/loop defense, not the security
boundary. This is the un-bypassable replacement for the OLD client-side
`is_self_send` guard `graph/send.rs` used to carry: that guard only ever
protected a well-behaved caller of `aoide send`; a raw connection to a
session's own socket bypassed it entirely, and still can for any OTHER
identity claim (see the still-open door named below). The SEND GATE itself
(`sender_is_parent`/`siblings_share_live_parent` in `graph/send.rs`) keys
on a KERNEL-ATTESTED sender session instead of the OLD, forgeable
`AOIDE_SESSION_ID` env read: `aoide send` walks ITS OWN real `/proc`
ancestry (as unforgeable a kernel fact, for the SAME real process, as a
peercred read of it would be) to find a session whose seal verifies.
`AOIDE_SESSION_ID` remains ONLY as attribution (audit lines, the
provenance prefix `resolve_sender` builds) — removed from every GATE
predicate. **What P-ID2 does NOT close**: a genuinely unrelated same-uid
process connecting directly to a session's socket (bypassing `aoide send`
entirely) still injects with no gate at all — the per-session socket's
accept refuses only the ONE narrow self-injection shape above, since the
socket carries raw bytes with no envelope and so cannot tell an
explicitly-`--yes`'d send apart from an ordinary one at the receiving end
without a wire protocol this phase does not add.
Absent `seal`/`sealedIssuedAt` means "no daemon has sealed this record yet"
(a session that predates a daemon's current life, or one from an
old build); readers must tolerate both forms. No `graph.json`
projection — like `origin`/`headless`/`hookAncestry`, it is consumed
internally, never rendered into the live graph.

**The remaining two sockets get a peercred floor (LANE IDENTITY P-ID3).**
Phase 0 mapped every legitimate connector to each: shellbridge's socket
serves the QML herald/bar widgets clicking a summons verdict or a
focus/power/rice-mode/usage-refresh/recheck action, `aoide herald push`
(dunst's own script hook), and `session permit` raising its own summons —
every one of them the OPERATOR's own uid, none of them the daemon's;
`aoided`'s dispatch socket serves the CLI's own `daemon_dispatch` proxy (a
session-write handler trying the resident daemon before its direct
stage-write fallback) and any other same-uid caller of `aoide` itself.
`aoide_conduct::shellbridge::serve` and `aoide_server::daemon::accept_loop`
both now read `SO_PEERCRED` on accept (`aoide_conduct::graph::
identity::peer_cred`, widened `pub(crate)` from `pub(in crate::graph)` so
`shellbridge.rs` — a sibling of `graph`, not a descendant — reuses it
rather than a second reimplementation; `aoide-server` reuses
`aoide_secrets::peercred` instead, already `pub`, already a dependency, so
no widening needed there) and refuse a connection outright — never
forwarded — whose peer uid does not match the process's own euid,
fail-closed exactly like the secrets broker's `admin_gate` on an
unidentified peer (`cross_uid_gate`, restated identically in both files,
pure and unit-tested without a real different-uid connection). **This is a
CROSS-uid floor, not a same-uid guarantee** — under OQ1-A every legitimate
connector named above already runs as the SAME uid a prompt-injected agent
would, so a same-uid process forging `{"cmd":"heraldverdict",...}` on
shellbridge's socket, or a raw `{"op":"dispatch","path":["send"],...}` on
`aoided`'s, is refused by neither floor. This is the identical OQ1-A
residual the per-session control socket already carries (P-ID2's own "what
P-ID2 does NOT close" paragraph above) restated at these two sockets, not a
new gap this phase opened — stated honestly rather than papered over with a
floor shaped to look like more than a cross-uid check.

Two ATTRIBUTION leaks close alongside the floor, both the SAME shape a
`send`/`session_send` handler running INSIDE a long-lived door process
(rather than inside `aoide send`'s own short-lived CLI invocation) can fall
into: `resolve_sender` (`graph/send.rs`) falls back to reading
`AOIDE_SESSION_ID` off the CALLING process's own env whenever a request
carries no `--from` — attribution only, never the gate (`sender_is_parent`/
`siblings_share_live_parent` above never consult it). For a `send` reaching
`deliver_local` from INSIDE `aoided`'s own process (a raw
`{"op":"dispatch","path":["send"],...}` request, or the CLI's own
`daemon_dispatch` proxy for OTHER session-write commands riding the same
wire), "the calling process's own env" is `aoided`'s — not the connecting
client's, which never crosses this socket at all. `aoide_server::daemon::
invocation_from_dispatch_request` now stamps an absent `from` flag
EXPLICIT-EMPTY (`--from ""`, `resolve_sender`'s own documented "no
attribution" form, the same mechanism `session pending approve`'s re-drive
already relies on) rather than leaving it absent to fall through to
`aoided`'s ambient env (G8). `aoide_server::a2a::do_inject` — which calls
`session_send` the same direct, in-process way for an inbound A2A message —
does the identical stamp when its own resolved `from` is `None`, so a
remote inject can never pick up whatever `AOIDE_SESSION_ID` the `aoide a2a
serve` process happened to inherit at launch either (G9). **Neither
attribution fix touches the GATE.** `real_attested_sender`'s
`std::process::id()` walks whichever process is actually executing
`deliver_local` — for a request proxied through `aoided`'s dispatch socket
or injected through `a2a serve`, that is the DOOR's own process, not the
original caller's. In production that process's real `/proc` ancestry
(`init -> systemd -> aoided`, or `init -> ... -> aoide a2a serve`) never
resolves a live sealed session, so a proxied `send` with no `--yes`/
autogate already fails closed to `pending` by construction today — not
because either fix re-derives the connecting caller's real identity (it
doesn't), but because the door process's own ancestry is architecturally
incapable of impersonating one. Threading the connecting peer's real pid
into the gate itself — so a proxied `send` resolves the ACTUAL caller
rather than merely failing closed — would touch `graph/send.rs`'s gate,
out of this phase's scope fence; deferred, not forgotten.

**The lane's accounting — what LANE IDENTITY (#63) enforces, and what it
leaves open.** Enforced, end to end: `origin` is write-once and
door-stamped (P-ID0); every live, pid-carrying session carries a
daemon-sealed credential (P-ID1, the `seal` field above); the send gate
and the per-session socket's accept key on kernel facts plus a verified
seal, never env (P-ID2); shellbridge and the dispatch socket hold a
cross-uid peercred floor (P-ID3); the secrets broker's `allowRemoteOrigin`
gate is the credential's first policy consumer (P-ID4, the "Secrets home"
section); the peer wire resolves identity by verifying KEY, never claimed
name (P-ID5, §6). Open, each named deliberately rather than implied
closed: (1) consumer-name authentication — the seal authenticates the
SESSION and its CLASS, never the self-asserted `consumer` string; a
separate, unbuilt axis. (2) OQ1-B, the own-uid daemon — not taken; Yama
`ptrace_scope>=1` plus process liveness stay the trust root, and the
same-uid daemon-impersonation and sweep-relaunder residuals (above, and
the origin-gate paragraph) are inherent to that choice. (3) A cross-uid
attestation channel — the packaged `aoide-secrets` broker cannot reach the
operator's daemon socket or session roster, so the origin gate is dormant
in that deployment. (4) An authenticated registration path — the closer
for the sweep-relaunder shape. (5) Per-surface dispatch-door gates riding
the attested-caller lookup. (For readers following the plan file's "LANE
IDENTITY (#63)": G1–G17 are its gap catalogue, OQ1-A/OQ1-B its
threat-model fork — OQ1-A is the answered choice.)

**Additive in v0 (P-C5, durable-sessions plan):** a session record MAY also
carry an optional `restore` (object) — a conducted SHELL's continuously-
captured restore snapshot, written change-only by `aoide-conduct`'s PTY
tick (`conduct_refresh_shell`, ~1 Hz) alongside `cwd`/`activity`/`state`:

```json
"restore": { "cwd": "/home/khoa/Aoide", "idle": true, "argv": null, "typed": "cargo test -p aoide-conduct" }
```

- `cwd` — the shell's live working directory at the last tick (`None` when
  unreadable).
- `idle` — whether the pty's foreground process group was the bare shell
  itself, the SAME predicate `state` is derived from. Kept as its own field
  rather than read back off `state` later: the reap sweep overwrites `state`
  to `"done"` BEFORE its ledger write (`reap.rs`'s own ordering, `session
  reap`), so idleness would otherwise be unrecoverable by the time the
  ledger line is written.
- `argv` — RAW, uncollapsed, unclipped `argv` off `/proc/<fg>/cmdline`
  (`proc_argv`) while a foreground command runs; `None` while idle. Never
  the DISPLAY label `activity` carries (`proc_command` basename-collapses
  `argv[0]` and truncates at 48 chars) — re-exec'ing that string would run
  the wrong or a truncated binary.
- `typed` — the reconstructed unsubmitted prompt line, REFUSAL-based: `Some`
  only for a clean, unedited keystroke run since the last submit (bytes from
  BOTH real stdin and an injection connection count, since both land in the
  same shell readline buffer); any readline-editing byte (an escape
  sequence, `^R`, Tab, `^U`/`^W`) or invalid UTF-8 poisons it to `None`
  instead of guessing. Only ever populated when `idle` is true — a shell
  mid-command has no prompt line. A silently WRONG `typed` would put text
  the operator never composed one keystroke from running; `None` is a fully
  acceptable product of this capture, a guess is not.

Absent for every non-shell session and every legacy record predating this
field; readers must tolerate both forms. No `graph.json` projection — like
`headless`/`hookAncestry`/`origin` above, it is consumed internally
(projected verbatim into `state/session-ledger.jsonl`'s own `restore` field
at session exit, below) rather than rendered into the live graph. Never
computed at reap time: by the time a sweep condemns a session its process is
already gone (that is the signal it reaped on), so a `/proc` read there
returns nothing, every time — the snapshot is always the last one the live
tick took, up to ~950ms stale at worst.

**Windowless lineage (task #89, corrected in review round 2):** a session is
windowless BY CONSTRUCTION — no `windowAddress`, no window-owning pid ever
attached to it — in either of two cases: (1) it IS ITSELF a conducted
(`conductable`) session with `headless == true`, or with `headless` absent
and its own `windowAddress` empty; or (2) its `parentSessionId` chain passes
through a session matching case (1). Enforcing this is not "four sites" —
it is a discovery GATE plus a listener SELF-CHECK plus the four historical
backfill call sites:
- the discovery gate: `aoide conduct --headless`'s own registration path
  never calls window discovery at all when `headless` is set — the
  unconditional call was the review-round-2 defect; a headless wrap's own
  record must never even attempt to discover a window, not merely have one
  filtered out downstream;
- the listener self-check: the shellbridge window-event listener
  (`resolve_pending_session_windows`) and every hook-time backfill site
  (`aoide session hook`, `graph/send.rs`'s two `discover_window()`
  call sites, `graph/window.rs::ensure_session_window`) all route through
  `windowless_by_lineage`, which now checks the session's OWN record first
  (case 1 above) before ever walking its parent chain (case 2) — so a
  headless wrap's own record is caught by the same function that catches
  its descendants, not by a second parallel mechanism.

A session with no parent, or whose chain anchors in a windowed conducted
session directly (an interactive `conduct` with a real `windowAddress`),
keeps the ordinary backfill. The same-window eviction
(immediately above `graph.json`'s render, below) is lineage-safe on top of
this: it never retires a member of the newly registering session's own
lineage (every ancestor AND descendant, walking `parentSessionId`), only a
same-window record with NO lineage relation to it — the legitimate
compact/resume-twin case. The reaper's own same-window dedup pass
(`aoide session reap`'s `superseded_agent_duplicates`) carries the identical
lineage carve-out as defense in depth: a windowless-by-construction session
never enters a same-window dedup group in the first place, but if a bug
upstream ever lets one acquire a window anyway, the dedup pass still will
not retire its own lineage — only a genuine no-lineage same-window twin
collapses.

### `state/stage/projects.json` — **v0**

Registered project anchor roots for the graph. Written by
`aoide project add/remove` (atomic, idempotent); read by bare
`aoide graph` and by `restage_graph` at every mutation site.

```json
{ "schemaVersion": "0", "projects": [ { "name": "aoide", "path": "/home/khoa/Aoide" } ] }
```

**Additive in v0 (P-D8, `docs/architecture/AOIDED.md`'s "L5"/"Open
knobs"):** a project entry MAY also carry an optional `autoResume` (bool,
default/absent means `false`, `skip_serializing_if` keeps a `false` value
off the wire — the same additive-bool discipline `SessionRecord.headless`
set the precedent for). Set via `project add --auto-resume`
(idempotent-upsert; no `project set`/`edit` command exists yet to flip
it back off — hand-edit `projects.json` in the meantime). Consumed by the
daemon's own boot-time auto-resume trigger (`aoide-server`'s `daemon.rs`,
the decided answer to this design's one open knob): once per BOOT — never
on a same-boot `Restart=on-failure` restart, guarded by a marker recording
the boot epoch (`btime` out of `/proc/stat`) the trigger last ran under —
for EVERY `autoResume` project, unconditionally, the daemon calls `aoide
resurrect --project <name>` in-process (`Door::Daemon`), the
identical command core the CLI command runs. There is no liveness check at
this layer (durable-sessions plan P-C4): that used to gate on the whole
project (any non-`done` session anchored to it skipped the call entirely),
which was wrong once a project could carry MULTIPLE durable sessions — one
live terminal would have suppressed reviving the rest. Liveness now lives
inside `resurrect`'s own bare-mode selection, per undying candidate
(see this section's own `resurrect` entry above), so a project whose
whole undying set is already live simply resolves to an empty-set
`Outcome::ok` no-op. A per-candidate spawn failure (e.g. a headless host
with no `$AOIDE_TERMINAL`) degrades gracefully — logged, never a crashed
tick.

### `state/stage/graph.json` — **v0**

The **fully resolved** project/session DAG, written (atomic) automatically by
every project/session mutation and by `aoide session prune`'s manual resync,
for Quickshell to hot-reload — like stage notes, QML reads concrete
values and computes nothing. Project nodes anchor session nodes by cwd
(longest path-prefix wins, so nested projects anchor correctly); `spawned`
edges come from `parentSessionId`. A session with a resolved parent carries
only its `spawned` edge; root sessions carry an `anchors` edge (or none when
unanchored).

**Additive edge kind in v0 (P-D8, `docs/architecture/AOIDED.md`'s "L5"):**
a session node whose `sessionsFile` record carries a populated
`resumedFrom` (above) ALSO gets a `resumed` edge, `from` the resurrected
session `to` the ledger `sessionId` it names — beside, never instead of,
its own `spawned`/`anchors` edge (the ledger `sessionId` it names is not
itself necessarily a node in the current graph — it may have already been
pruned from `sessions.json` by the time the resurrection happens, since the
ledger is exactly the memory that survives that prune; a `resumed` edge's
`to` is therefore a bare id reference, not a guaranteed node lookup).

```json
{
  "schemaVersion": "0",
  "nodes": [
    { "id": "project:aoide", "kind": "project", "name": "aoide", "path": "/home/khoa/Aoide" },
    { "id": "session:abc123", "kind": "session", "agent": "claude", "cwd": "/home/khoa/Aoide", "state": "running", "windowAddress": "0x…", "startedAt": "…" }
  ],
  "edges": [
    { "from": "project:aoide", "to": "session:abc123", "kind": "anchors" },
    { "from": "session:abc123", "to": "session:def456", "kind": "spawned" }
  ]
}
```

A session node MAY carry the sessions.json `petname` field above, present
under the same rule.

### `state/stage/herald.json` — **v0**

The notification ledger the Quickshell herald draws from. dunst owns
`org.freedesktop.Notifications` but draws NOTHING (`skip_display` on every
rule); it hands each notification to `lyra herald push` through its `script`
hook, which forwards it over the shellbridge socket. **The shellbridge daemon
is the single writer** — dunst runs its scripts asynchronously, so two
notifications arriving together would otherwise race a read-modify-write and
one would be lost. QML only ever READS this file; a click sends a socket
command (`heraldverdict` / `heralddismiss`) and never writes.

Newest LAST. A non-empty `stackTag` replaces the entry holding the same tag
rather than appending, and the ledger is capped at 20 (matching the dunstrc's
`history_length`). `progress` is `-1` for "no value"; `timeoutMs` is `0` for
"never expires" — the QML herald owns the dismiss clock, because a
notification dunst never displays is never expired by dunst either.

`kind` is `toast` or `summons`. A summons is an agent blocked on a permission
prompt, published by `aoide session permit` rather than by dunst; it carries the
waiting `sessionId` and is drawn with real approve/deny buttons whose verdict
routes back through the shellbridge to `send`, the one gated injection
door. Sender text is DATA on both sides of the seam: carried verbatim, never
parsed as markup or as a command.

```json
{
  "schemaVersion": "0",
  "notifications": [
    { "id": "7", "app": "notify-send", "summary": "…", "body": "…", "icon": "/nix/store/…png",
      "urgency": "critical", "progress": 40, "category": "", "stackTag": "",
      "timeoutMs": 0, "receivedAt": "2026-08-17T12:00:00Z", "kind": "toast", "sessionId": "" }
  ]
}
```

### `state/stage/pending.json` — **v0**

The held-injection queue: entries `aoide send` writes when its gate
doesn't clear immediate delivery (no `--yes`, no autogate match), and the
A2A door's own `message/send` Inject path reuses VERBATIM when its admission
check doesn't clear a caller either (`crates/server/src/a2a.rs::do_inject`) —
one queue, two writers, no second pending-queue implementation. Read and
resolved by `aoide session pending list/approve/deny`: `list` enumerates every
entry (a malformed one — a stale hand-edited line — surfaces as
`"state": "malformed"` rather than failing the read); `approve` re-drives the
entry through the SAME gated injection door with `--yes`, in-process; `deny`
drops it. Either resolution REMOVES the entry from this file — the record of
what happened is the audit log (`session.pending.approve` / `.deny`), not a
persisted "resolved" archive. An entry carries no id of its own; `list`'s
`id` is its array position, which shifts on the next resolve. The optional
`from` field is the sender attribution (`--from`, else `AOIDE_SESSION_ID`)
resolved when the entry was queued; it is carried through `approve`'s
re-drive so the delivered send still names whoever queued it, not whoever
approved it, and is absent (never a parse failure) on an entry written
before this field existed. The queued `submit:true` flag means only "submit
the line" — the concrete keystroke (`\n`, or `\r` for kimi) is resolved at
DELIVERY time from the target session's own agent profile, never memoized at
queue time. `list`'s HUMAN line renders both the target and
`from` through the canonical display grammar (petnames plan P3 —
`<host>/<role>/<petname> (…<tail4>)`, terse petname+tail for `from`); the
JSON fields `data.pending[].sessionId`/`.from` stay the entry's raw
canonical ids, verbatim, always.

```json
{
  "schemaVersion": "0",
  "pending": [
    { "sessionId": "conduct-6364-1786576226", "text": "/compact keep only operational records", "submit": true, "queuedAt": "2026-08-13T14:01:10Z", "from": "conduct-1122-1786570000" }
  ]
}
```

`send`'s delivered payload carries the same attribution: when a sender
resolves and the text names the node (see `names_the_node` — a bare keystroke
answer like a permission-verdict digit never does), the payload is prefixed
`from <sender>: ` on its first line only, so the receiving agent can see who
sent it — attribution, not authentication; both `--from` and
`AOIDE_SESSION_ID` are ordinary same-user process state, spoofable by
anyone who can already write to the target's control socket.

### `song/stage/grimoire.json` — **v0**

The Grimoire launcher's own usage ledger (`GrimoireLedger.qml`), separate
from every other stage file in this section: it is QML-created, not staged
by `aoided`, and has no CLI command of its own — a fresh install has no
`grimoire.json` until the first launch. Tracks how often each `.desktop`
entry is launched so the Grimoire's frequency chapter ("most commonly
opened") can rank real usage instead of guessing. Song-agnostic by design:
the ledger is a data seam, not chrome, so launch-frequency history stays
put across a `rice stage`/song switch rather than moving with the song.

**The single writer is the QML itself** — `GrimoireLedger.qml` calls
`DesktopEntry.execute()` directly with no `aoided` command in between (the
same no-new-command idiom `launcher.qml` already established) and persists
through a `FileView` with `atomicWrites: true` (write-temp-then-rename, so a
hot-reload or a crash mid-write never reads a torn file). Parsing follows
`LiveryState.qml`'s `FileView` idiom: a guarded try/catch degrades a
missing/garbage file to an empty map rather than throwing. Capped at the
top 200 entries by count so a machine with years of uptime doesn't grow the
file unbounded; pruning only ever drops the coldest tail, never an entry a
chapter/search is about to render.

```json
{
  "schemaVersion": "0",
  "launches": {
    "firefox": { "count": 22, "lastAt": "2026-08-25T04:18:38.610Z" }
  }
}
```

### `state/inbox.json` — **v0** (messaging plan P-C6, 2026-08-21)

The durable per-host message inbox: every message that actually lands in a
LOCAL session, filed by exactly TWO writers — no third site anywhere in the
tree:

1. `aoide_conduct::graph::send::deliver_local`'s success path — covers a
   direct `send --id`, a `--to` resolving local (re-drives
   `deliver_local` unchanged), a `session pending approve` re-drive, AND the
   A2A door's own `message/send` Inject arm
   (`crates/server/src/a2a.rs::do_inject`), which builds a `send --id`
   invocation and calls `session_send` too — the SAME "one queue, two
   writers, no second implementation" shape `pending.json` (above) already
   set, except this branch alone collapses to one writer, because the a2a
   door never bypasses `session_send` for an EXISTING session. `do_inject`
   files no entry of its own — see its doc comment.
2. `aoide-server`'s `spawn_inject_prompt` (`crates/server/src/a2a.rs`) — the
   FIRST turn of a brand-new A2A-spawned session (`do_spawn`, which fires
   whenever an incoming `message/send` carries no `contextId` or asks to
   spawn). This canNOT go through writer 1: the target `SessionRecord`
   isn't in `sessions.json` yet at the moment the prompt is typed — it's
   written by the spawned CHILD process itself once ITS OWN `aoide conduct`
   starts up, a race `spawn_inject_prompt`'s own connect-and-retry loop
   already exists to survive (the socket may not even exist yet). Routing
   through the session registry here would just trade the socket race for
   a registration race, so this site talks to the raw socket directly and
   files its own entry right after the write.

An OUTBOUND `--to peer/<x>` send (`deliver_remote`) never files here: the
message lands in the REMOTE peer's own inbox, via whichever of that peer's
own two writers actually delivers it.

Lives in the gitignored root-runtime `state/` dir (§2), NOT inside either
stage tree (`state/stage/` or `song/stage/`) — same tier as
`usage.json`/`peers.json`, never reset by a stage reseed.
Read/resolved by `aoide inbox list/read/clear`: `list` shows unread entries
by default (`--all` includes read ones); `read <n>` marks one entry read by
its array position (`n`, same as `pending list`'s id scheme) — but unlike a
pending entry, marking read does NOT remove the entry, so positions stay
stable across repeated `read` calls; the only thing that can still shift a
position is the 200-entry cap's oldest-drop when a NEW message arrives
between your `list` and your `read` (same "re-list if you're racing a
writer" discipline `pending.json` documents, triggered by the cap instead of
every resolution); `clear` empties the file unconditionally — no `--yes`, no
gate, matching `session.prune`'s precedent (the command name is the whole blast
radius, nothing selective to confirm, unlike `rice draft drop`/`rice take
prune` which destroy a NAMED or AMBIGUOUS subset). Capped at 200 entries,
oldest-drop (`herald::LEDGER_CAP`'s fold-and-cap precedent, CONTRACTS.md §4
above), atomic writes.

`context` is an OPTIONAL, OPAQUE `serde_json::Value` passthrough — reserved
for a planned Mneme (memory-manager) integration (#14/#16) that does not
exist yet. v0 round-trips whatever a future producer sets, byte-for-byte,
and never reads or interprets it; no call site in this tree sets it today
(every `inbox::receive` call passes `None`), so the key is absent from every
entry currently written.

```json
{
  "schemaVersion": "0",
  "entries": [
    {
      "from": "conduct-1122-1786570000",
      "target": "conduct-6364-1786576226",
      "text": "status update on the migration",
      "receivedAt": "2026-08-21T14:01:10Z",
      "read": false
    }
  ]
}
```

`from` is always present (empty string for an anonymous/unattributed
sender — never omitted, unlike `pending.json`'s optional `from`): writer 1's
LOCAL branch uses the same `resolve_sender` output the audit line and the
delivered payload's provenance prefix already compute; neither of the two
A2A-reached filings (`do_inject`'s share of writer 1, and writer 2 —
`spawn_inject_prompt`) has a caller identity to offer today (#51 owns real
cross-host provenance — this never invents any) so both are honestly empty.
No conductor pane yet (rides a later phase) and no outbox retry for a peer
that was unreachable at send time (the sender already gets a clean error
from `deliver_remote`; nothing queues a retry) — both deliberately deferred,
not built.

### `state/usage.json` — **v0**

Account/usage runtime — lives in the gitignored root-runtime `state/` dir
(§2), NOT inside either stage tree: this is account/global state, not
song-scoped, rehearsal, or broker-owned registry state. **State-dir resolution** mirrors the stage-dir seam above:
`$AOIDE_STATE_DIR` when set to an **absolute** path, else `$AOIDE_ROOT/state`
(§4's runtime-root note). Written by `aoide usage` (`aoide.usage.*`,
`modules/nucleus/options.nix`; opt-in poller service, off by default).

The `local` block is a rollup computed straight off this machine's own Claude
Code transcripts (`~/.claude/projects/**/*.jsonl`) — no network, no
credentials, an ESTIMATE (approximate pricing) for THIS machine only, never a
billing source of truth.

The `live` block is now a **REAL fetch** (was a stub in the first cut):
`aoide usage` reads the consumer OAuth token from `~/.claude/.credentials.json`
(`.claudeAiOauth.accessToken`) and calls Claude Code's own account-usage
endpoint. **The source is UNOFFICIAL / ToS-gray** (an undocumented endpoint;
personal, read-only, this account only), so the fetch is best-effort and
**degrades to `ok:false` + a short tokenless reason** on ANY failure (no
credentials, an "authorized for Claude Code only" rejection, a transport
error, a non-200, or an unparseable body). The token is never logged, printed,
or written into `state/usage.json` or any error string. Readers must still
tolerate `live.ok == false` and must guard every sub-field: on `ok:true` the
block carries the optional `fiveHour` / `sevenDay` / `sevenDayOpus` /
`sevenDaySonnet` (each `{utilization, resetsAt}`) and `extraUsage`
(`{isEnabled, monthlyLimit, usedCredits}`); on `ok:false` it is just
`{ok:false, error}` (shape-compatible with the original stub).

```json
{
  "schemaVersion": "0",
  "fetchedAt": "2026-08-01T12:00:00Z",
  "live": {
    "ok": true,
    "fiveHour":       { "utilization": 42.5, "resetsAt": "2026-08-01T18:00:00Z" },
    "sevenDay":       { "utilization": 12.0, "resetsAt": "2026-08-07T00:00:00Z" },
    "sevenDayOpus":   { "utilization": 5.0,  "resetsAt": "2026-08-07T00:00:00Z" },
    "sevenDaySonnet": { "utilization": 7.0,  "resetsAt": "2026-08-07T00:00:00Z" },
    "extraUsage":     { "isEnabled": true, "monthlyLimit": 100.0, "usedCredits": 3.5 }
  },
  "local": {
    "note": "local estimate, this machine only",
    "today": { "tokens": 1200000, "costUsd": 4.10 },
    "week":  { "tokens": 8000000, "costUsd": 27.0 }
  }
}
```

When the live fetch fails (the common case off a personal box), the `live`
block degrades to, e.g., `{ "ok": false, "error": "unauthorized (consumer
OAuth restricted to Claude Code)" }` and the widget falls back to `local`.

### `state/sessions/<sessionId>.log` — **v0**

The pty-master transcript of one HEADLESS `aoide conduct --headless` session —
raw bytes read off the pty, mirrored verbatim as they arrive (no framing, no
encoding, not necessarily valid UTF-8). Lives in the same gitignored
root-runtime `state/` dir as `usage.json` (state-dir resolution as above), one
file per headless session, created on first write and opened append-only for
the session's whole lifetime. Append-only and UNROTATED — a deliberate known
gap, not a design goal: nothing truncates or rolls this file, so a
long-running or noisy headless session grows its log without bound. Written
only by `aoide conduct --headless`; an interactive `conduct` session never
creates one. The session's `sessions.json` record (above) publishes this
file's absolute path as `logPath` the moment it's open.

### `state/session-ledger.jsonl` — **v0** (P-D8, `docs/architecture/AOIDED.md`'s "L5")

The durable session HISTORY that survives `sessions.json` pruning —
`sessions.json` is the live roster (reaping and `session end` both remove a
record from it); this ledger is append-only memory of every session that
has ever LEFT the roster, which `aoide resurrect` reads to find
something to revive. Lives under `state_dir` (`aoide_storage::fs::
state_dir`) alongside `usage.json`/`sessions/<sessionId>.log` — real disk,
never tmpfs, since it must survive a reboot the way those don't need to.

One line per session, appended at the exact moment it leaves the roster —
a clean `aoide session end` and a `session reap` sweep are the only two
producers, both routed through the SAME shared write (never two
independently-written call sites), so a given session contributes exactly
one line, never zero, never two, regardless of which path retired it.
Never truncated, never rewritten in place, and never a lookup key for LIVE
state — a reader wanting "is this session still running" still asks
`sessions.json`, never this file.

```json
{"v":0,"sessionId":"s1","agent":"claude","harnessSessionId":"claude-uuid-123","cwd":"/home/khoa/Aoide","title":"fix the thing","petname":"brave-otter","startedAt":"2026-08-20T10:00:00Z","endedAt":"2026-08-20T12:00:00Z","resumedFrom":null,"origin":null,"restore":null}
```

Every field serializes unconditionally (no `skip_serializing_if`,
deliberately unlike the additive-optional fields on `sessions.json`'s live
records above) — a closed historical shape reflects exactly what the
record looked like the instant it exited, not a growing set of optional
fields a future reader has to guess the absence of. A malformed or
unparseable line is skipped on read rather than failing the whole file (the
same read tolerance `pending.json`/`herald.json` already extend to a
corrupt entry). `origin` (P-P3, decision 7 above) is the one exception to
"always `null` unless resurrection-related" — it is `"peer:<name>"` for a
session a paired peer's A2A spawn created, projected verbatim off the live
`SessionRecord.origin` at the exact moment `ledger_session_exit` appends
this line, `null` otherwise (including every legacy line written before
this field existed, tolerated on read the same as any other field here).

`restore` (P-C5, durable-sessions plan) is `null` for every non-shell
session and every line predating this field, and otherwise the SAME
`{cwd, idle, argv, typed}` object `sessions.json`'s own `restore` field
carries (see above) — projected verbatim off `SessionRecord.restore` at the
exact instant `ledger_session_exit` appends this line, never re-derived and
never a `/proc` read here: the reap sweep's own ordering
(`state = "done"` BEFORE this call) means the process this line is about may
already be gone by the time it is written, so the record's own
continuously-captured snapshot is the only honest source. A populated
`restore` object's OWN fields also always serialize (never
`skip_serializing_if`, even here) — the SAME `RestoreSnapshot` type backs
both `sessions.json`'s additive-optional field and this always-present one,
so a caller reading a populated `restore` sees the identical complete shape
in either file.

### `state/undying.json` — **v0** (durable-sessions plan)

The undying mark: the set of session ids marked DURABLE, so a project's
whole undying set can be resurrected together. Prototyped under the name
"carry" (task #96, `state/carry.json`); shipped under this name at
command-defrag lane U1 (2026-08-27) — same shape and discipline throughout,
only the vocabulary changed. Its command surface moved once more at the
session-surface redesign (command-defrag lane X, 2026-08-28): the scripted
spelling is now `aoide session grant undying on|off`, and the picker is
`aoide session grant undying` bare — both relocated verbatim from `session
undying`/bare `session`, which this absorbs and retires (hard cutover, no
alias). Four writers, none routed through a stage lock
or `daemon_dispatch` (this is not a stage-tree file): `aoide session grant
undying on|off` sets or clears the mark directly; `aoide spawn --undying` adds the
newly spawned id once it registers — including from TWO further internal
call sites, still the SAME writer, not a fifth (U2's own design ruling,
below): `resurrect`'s bare-manifest mode passes the identical `--undying`
flag on both its own internal spawn invocations (the ledger-enrichment
path's spawn and the clean-spawn path's), so a manifest-revived session
gets marked through the exact code `spawn --undying` itself runs, gated
the exact same way (registered, not merely launched); `aoide resurrect`
(flag mode, the ORIGINAL revival path) transfers an undying old id onto
the freshly spawned session that replaces it; `aoide session grant
undying`'s picker (U3, command-defrag lane U; relocated to this spelling at
lane X) writes the SAME store for every LOCAL row a
confirm touches — one `load_undying`, N `set_undying` mutations, one
`save_undying`, the same discipline `session grant undying`'s own single-id
write holds, widened to cover a whole confirm's diff at once. A PEER row the
picker touches never reaches this store at all — the id lives on the peer,
so the picker writes a `.aoide/project.json` spec instead (see that file's
own section below). Lives under
`state_dir` (`aoide_storage::fs::state_dir`) alongside
`usage.json`/`session-ledger.jsonl`, NOT inside either stage tree — an
undying mark is durable operator state, never staged rehearsal/registry
state. Distinct from the ledger above: the ledger is an append-only record
of every session that has ever left the roster, while `undying.json` is a
small, freely-mutated SET — marked on, marked off, and its entries
transferred wholesale when an undying session is resurrected under a new
id.

```json
{ "schemaVersion": "0", "undying": [ { "sessionId": "conduct-1234-1756…", "markedAt": "2026-08-26T10:00:00Z" } ] }
```

**Additive / tolerate-missing:** a missing, corrupt, or wrong-shape file
reads as an empty undying set, never an error — a mark on an id that never
produced a ledger line is inert, not an error condition. Written atomically
(`aoide_storage::fs::atomic_write`, not `atomic_write_private`: a session id
is the same class of data `sessions.json`/`peers.json` already keep at
default mode). An undying id is a plain string, meaningful whether the
session is live, dead-with-a-ledger-line, or dead-without-one. Re-marking
an already-undying id refreshes its `markedAt` rather than duplicating the
entry.

**The one-shot migration.** The first time a process resolves
`load_undying`, it renames a pre-existing `state/carry.json` onto
`state/undying.json` when the new path is absent — the same atomic,
narrated-on-failure, never-clobbers discipline this file's own "one-shot
migration" note (above, `song/stage/`/`state/stage/`) established for the S1
split, applied to a single small file: a plain `rename` (both paths share
`state_dir`, so this is always same-filesystem), skipped when a fresher
`undying.json` already exists (a process that already migrated is never
overwritten), and narrated rather than panicking on a failed rename. A
process that never had a `carry.json` (a fresh install, or one that already
migrated) pays one cheap `exists()` check and nothing else — no process-wide
guard is needed the way the six-file S1 migration warranted one. A file the
migration just renamed but that has not yet been re-saved through
`save_undying` still holds its pre-rename content (keyed `"carried"`, not
`"undying"`); the read side tolerates that legacy key until the next
`save_undying` normalizes it.

**The resurrect transfer.** When `resurrect` spawns a replacement for
a ledger entry whose old `sessionId` is currently undying, it moves the mark
onto the new id in ONE `save_undying` call — add the new id, then drop the
old, never two separate writes. The old id must not survive the transfer or
an ancestor chain would double-resurrect on the next sweep; conversely, an
old id that was never undying gets no mark on its successor — the transfer
only fires for a pair that started undying, it never grows the undying set
on an ordinary `--all`/`--id` revive. Within the single in-memory vector the
new id is added BEFORE the old one is dropped, so a crash between that edit
and the write leaves the OLD id undying — retryable on the next sweep —
rather than leaving neither undying, which would be silent loss. A spawn
that fails outright transfers nothing, for the same reason: the old id
stays undying so the next sweep retries it.

**Driving selection (P-C4).** `undying.json` is also the fourth reader: bare
`resurrect --project <x>` (no `--all`/`--id`) reads it to resume a
project's WHOLE undying set rather than a single entry — every anchored
ledger entry currently in the set, minus any id already alive in
`sessions.json`, deduped by `sessionId` keeping the newest `endedAt`. See
this file's `resurrect` and `projects.json`/`autoResume` entries
above for the full selection contract and the daemon's boot-sweep
consumer.

### `.aoide/project.json` — **v0** (command-defrag lane U1, 2026-08-27;
consumed by `resurrect`'s bare mode at U2, same lane)

A project's own SESSION SPECS — never inside `state_dir` or either stage
tree, but NOT committed either: this file lives INSIDE a project root
(`<project_root>/.aoide/project.json`), self-ignoring (below), so
`resurrect` can find it on the host that conducts the project without a
`projects.json` registration first. Distinct from `state/undying.json`
above in every way that matters except one — both are HOST-LOCAL, neither
syncs via git. `undying.json` marks LIVE session IDS durable on ONE host,
gitignored runtime state, outside any project; `project.json` names the
SHAPE of sessions a project wants — never a session id, never a timestamp,
nothing host-specific except each spec's own `host` field. A project can
hold both at once: an `undying` mark and a `project.json` spec each name
intent that only ever means anything on the host that wrote it.

```json
{
  "version": 0,
  "sessions": [
    { "host": "yomi", "dir": ".", "agent": "claude" },
    { "host": "yomi", "dir": "crates/aoide", "command": "cargo watch -x check" }
  ]
}
```

`dir` is PROJECT-RELATIVE, ALWAYS — never an absolute path: this file is
host-local, but the project directory it sits beside can still move on
that same host (a re-clone elsewhere, a rename), and an absolute `dir`
would silently stop being correct the moment it did.
`aoide_storage::manifest::save_manifest` refuses the WHOLE save before
writing anything if any spec's `dir` is absolute, naming the offending
`host` in a taught error. `command` is optional (`skip_serializing_if`,
absent unless given) — a spec with none just names where an agent should
be conducted, no fixed argv. When given, a clean-spawning `resurrect`
(below) splits it on WHITESPACE ONLY — no shell-quote awareness, no `sh -c`
involved — the same naive tokenizing `spawn.rs`'s own `$AOIDE_TERMINAL`
parser already uses; an argument that itself needs an embedded space
CANNOT be expressed in `command` today. A hand-editor should know this
before reaching for quotes that will not do what they look like they do.
No `sessionId`, no `markedAt`/timestamp of any kind: a manifest is a
durable declaration of intent, not a record of any particular past run.

**Tolerant, both directions.** No `#[serde(deny_unknown_fields)]` anywhere
in `aoide_storage::manifest` — the file persists on disk indefinitely,
host-local, so it must stay readable across an `aoide` upgrade or
downgrade on that same host, not only the exact build that wrote it; an
unrecognized top-level or per-spec field is silently tolerated, not
refused. Reading: `load_manifest(project_root)` returns `None` for a
MISSING file (the ordinary case — most projects declare nothing) with no
narration at all, and ALSO `None` — but narrated to stderr first — for a
present file that is unreadable or fails to parse, so an operator learns
something is wrong without the caller needing a second error type.

**`.aoide/` self-ignores.** The first `save_manifest` into a project root
seeds `.aoide/.gitignore` with `*\n` if one is not already there — the
manifest is a deliberately HOST-LOCAL decision (one focused host owns the
shape it wants for a project), not something meant to sync via git the way
the project's own source does. An existing `.gitignore` there (an operator
customization, or one committed on purpose to override the default) is
NEVER overwritten by a later save.

**Containment (U2).** `aoide_storage::manifest::resolve_spec_dir(project_root,
dir)` is the read-side twin of `save_manifest`'s write-side absolute-`dir`
refusal: joins a spec's `dir` onto `project_root` and normalizes it
LEXICALLY (no filesystem access, so it resolves before the directory
necessarily exists), refusing — never silently clamping — any `..` that
would resolve outside `project_root` after normalization, and any
still-absolute `dir` a hand-edited or newer-build-written file might carry.
`resurrect`'s bare-manifest mode (below) is this guard's first caller, on
every spec before it is ever used as a `--cwd`. **The guard is lexical, so
it is a STRING check on `dir`, not a filesystem-real check on where that
string ends up** — a `dir` with no `..` at all can still name a path that,
at USE time, is (or passes through) a symlink pointing outside the project
root; `resolve_spec_dir` has no opinion about that, and nothing downstream
re-checks it. Accepted, not a gap to close: the manifest's whole trust
model is host-local and operator-authored — the operator who writes a
`.aoide/project.json` spec already controls what's on their own disk,
including any symlinks inside their own project.

**Discovery.** `aoide_storage::manifest::walk_up(start)` walks from `start`
up through every parent directory, git-style, for the NEAREST
`.aoide/project.json` — stopping at the filesystem root, nearest-wins (a
directory further up is never consulted even when the nearest one turns out
unreadable). Pure with respect to everything but the filesystem itself: no
env var, no `state_dir`/`stage_dir` indirection, just the path handed in —
this is the seam bare `resurrect` (task #101, Lane U, U2) calls to work
from a project a given host has a manifest for, with or without any
`state/undying.json` marks of its own. LEXICAL, not realpath: each step is
a bare `Path::parent()`, never a `readlink`/`canonicalize` — a manifest
reached through a symlinked directory component is still found (the
per-level existence check follows it, ordinary `stat` semantics), but the
walk never resumes from the symlink's own target ancestry once past it.

**Bare `resurrect` (U2, command-defrag lane U).** `resurrect` with none of
`--project`/`--all`/`--id` walks up from cwd via `walk_up`; found, it
revives that manifest's specs directly — no `projects.json` registration
needed at all — instead of falling through to the flag-mode selection
above. Not found, the command falls through to the ordinary
`--project`-required check; ITS OWN usage error then names the manifest
miss too (`no .aoide/project.json above <cwd> and no --project/--all/--id
given`) since neither path had anything to go on. Naming BOTH misses is
conditioned on having genuinely tried the walk — an invocation that DID
give one of the three flags (`--id` alone, say, with no `--project`) never
attempts a manifest walk at all and gets the ORIGINAL, accurate
`--project`-missing usage error instead; a review fix (U2 review round 1)
closed an earlier bug where that case wrongly got the manifest-miss
wording despite never having looked for one. Each spec resolves
independently (one spec's failure never aborts the rest, same posture
flag-mode's per-candidate loop already holds): a spec whose `host` does
not match this host's own (`aoide_storage::display::local_host_name`) is
SUMMONED through the peer door (U4, below), never skipped. A
local spec's `dir` resolves through `resolve_spec_dir` (above); the
**enrichment rule** then decides HOW to bring it up — the NEWEST entry in
THIS HOST's own session ledger whose `cwd`/`agent` match the resolved
`dir`/the spec's `agent` (host is implicit: the ledger is host-local state,
and only same-host specs reach this point at all) is revived through the
exact SAME `resolve_candidate`/`resurrect_one` path `--id` drives — its
harness resume args or terminal restore snapshot, exactly as if the
operator had named that ledger entry directly. No match — a spec this host
has never actually run, the ordinary case straight off a fresh checkout —
clean-spawns instead: windowed (`AOIDE_TERMINAL`), the spec's own `command`
when given, else the agent's registered `AgentProfile::launch` default; an
agent with neither is a taught `failed[]` entry, never a guessed argv. The
manifest DECIDES WHAT exists; the ledger only ever decides HOW. Every row
in the outcome — `revived-from-ledger`, `clean-spawned`, `summoned-remote`,
a bare `skipped` (an unresolved harness with no `restore` snapshot either),
or `failed` — carries a `disposition` key naming which of these it is, so
a consumer filtering the outcome by disposition never silently drops a row
that fell through `resolve_candidate`/`resurrect_one`'s own flag-mode
shapes.

**Remote summon (U4, command-defrag lane U).** A spec whose `host` names a
DIFFERENT box than this one is summoned through the existing peer door,
not skipped: `spec.host` resolves against `state/peers.json`
(`aoide_storage::peer_store::load_peers`) the exact same way U3's picker
WRITES it — a peer NICKNAME, not a literal DNS/OS hostname. Three local
refusals, checked in order, all landing in `failed[]` (never `skipped[]` —
the spec was tried and refused, not given up on) before the wire is ever
touched: no peer registered under that name (taught, names `peer add`); a
registered but UNVERIFIED peer (the same local-only refusal
`aoide-client::commands::handle_peer_spawn` already holds toward its CLI
callers — an unsigned request can never satisfy the remote door's
`Signature`-rung spawn gate, PAIRING.md decision 6); or neither a `command`
nor a registered default launch to summon WITH. Past those three,
`aoide-conduct::graph::resurrect::summon_remote` calls
`aoide_client::commands::spawn_on_peer` — the identical signed
spawn-shaped `message/send` (`context_id: None`) `aoide peer spawn` drives,
never a re-implementation of the wire and never a shell-out to the `aoide`
CLI (the `conduct` → `client` dependency edge, documented in `conduct`'s
own `Cargo.toml`, existed already for the roster core's live peer probe
(reached via bare `session`/`--hosts`) and gained
this second tenant). No confirm prompt: the manifest spec IS the
operator's own standing declaration, the same posture the local clean-spawn
already takes toward a spec's `command`. Text summoned is the spec's own
`command` verbatim when given, else the agent's registered default launch
joined back into one line; **which AGENT actually runs is the PEER's own
configured `aoide.a2a.spawnAgent`, never chosen here** — the summoned text
only ever becomes that agent's first typed turn
(`aoide-server::a2a::do_spawn`'s `spawn_inject_prompt`), so `spec.agent` is
informational on the remote leg, unlike the local leg where it picks the
actual harness. Every remaining refusal — an unreachable peer, the remote
door's own gate/autogate/allow-set refusal — surfaces VERBATIM into
`failed[]` as `spawn_on_peer`'s own error text; per-spec isolation holds
exactly as every other row in this loop does. A summoned session is never
marked undying on THIS host: the resurrected id lives on the peer, and
`state/undying.json` only ever names ids that live here (the same
reasoning U3's picker already holds toward a peer row's own mark).

**The cwd limitation.** The peer-spawn wire carries NO working-directory
field at all — `decide_send_action`/`do_spawn` (`aoide-server::a2a`) take
only a prompt and the pre-configured `spawn_agent` executable, nothing
else — so a spec's `dir` cannot be pushed onto the peer through this call;
it is not silently dropped so much as never representable on this wire
version. A spec wanting a specific directory on the peer must say so
inside its own `command` (`git -C <absolute path on the peer> …`), an
honest limitation rather than a guessed `--cwd` the wire has nowhere to
carry. Adding a wire field is a later phase's job — the fleet's doors run
older binaries this phase must stay compatible with, so the wire itself is
never touched here.

**Manifest-revived sessions are marked undying (orchestrator design
ruling, U2 review round 1) — LOCAL revivals only.** A remote summon (U4,
above) never reaches this: the resurrected id lives on the peer, not here.
Both the LOCAL enrichment path and the LOCAL clean-spawn
path pass the same `--undying` flag `resurrect_one`/`clean_spawn_from_spec`
hand to their own internal `spawn` invocation — the identical
mark-after-registration mechanic `aoide spawn --undying` runs (above), not
a re-implementation, and gated the same way (marked only once the new
session actually registers). Rationale: the manifest spec IS the durable
declaration of what should exist — marking its revived session undying
means a LATER bare `resurrect --project <name>` (or the daemon's boot
sweep) finds it in the undying set without needing to re-walk or
re-consult the manifest at all, so flag-mode and manifest-mode revival
converge on the SAME durable set instead of tracking two independent
notions of "what this project wants running." This is deliberately
DIFFERENT from flag-mode's own transfer rule immediately above (which only
ever marks a NEW id when the OLD ledger id it replaces was already
undying) — a manifest-mode revival marks unconditionally, because there is
no ordinary-revive case to protect against here: every manifest-mode spawn
already came from an explicit, operator-authored declaration.

**The picker's peer writer (U3, command-defrag lane U; review round 1 fixed
the batch-poisoning defect below, same phase; relocated to `aoide session
grant undying` bare at the session-surface redesign, command-defrag lane X,
2026-08-28 — bare `aoide session` itself now renders the roster instead).**
`aoide session grant undying` bare opens
a tty multi-select over local sessions AND every registered peer's CACHED
sessions (`peer_store::load_peer_cache`, no live pull); a local row's mark
toggles `state/undying.json` (above), but a PEER row's id lives on the
peer, so this file is the write target instead — a confirmed mark appends
`{host: <peer name>, dir, agent}` (never a `command`) into the CURRENT
project's manifest, an unmark removes EVERY spec matching `{host, dir,
agent}` (`Vec::retain`, not a first-match removal — a hand-duplicated entry
is cleaned up in one unmark, not one per copy) if any are present. "Current
project" is resolved the exact same way `resurrect`'s bare mode resolves
it — `walk_up` from cwd — and NEVER auto-created: no manifest above cwd
means every peer mark/unmark in that confirm is reported `skipped[]` with a
taught reason, while any LOCAL rows in the SAME confirm still write
normally. `dir` resolves via a purely lexical `Path::strip_prefix` against
the current project's own root — the peer session's cwd relativizes when it
literally starts with that same root string (the real case for a project
checked out at the same path on more than one host); a cwd that does NOT
relativize has no savable spec AT ALL and is REJECTED before it ever
reaches the manifest, `skipped[]` with a taught reason, the same as the
no-manifest case. There is deliberately no raw-cwd fallback: `save_manifest`
refuses the WHOLE batch on any absolute `dir`, so a fallback spec here would
not merely be an inferior write — it would silently sink every OTHER
legitimate peer change queued in the same confirm, reporting them all as
`changed` when nothing was actually persisted. This is deliberately
narrower than `resolve_spec_dir`'s own containment guard (above), which
only ever reads a `dir` that already exists — remote summoning across
genuinely different root paths is U4's door path, not guessed here. Dedupe
on write: an identical `{host, dir, agent}` spec already present is a
no-op, reported as such, never a duplicate row. The manifest write itself
is all-or-nothing per confirm: `changed[]` only ever names a `{peer, dir}`
pair AFTER `save_manifest` actually persisted it — a failed save (the
validation refusal above would only ever fire on a hand-corrupted manifest
now that the picker itself never produces an absolute `dir`; a plain I/O
error is the realistic case) folds every pending peer change for that
confirm into `skipped[]` instead.

### `state/identity/` — **v0** (P-P1, `docs/architecture/PAIRING.md`)

This instance's ed25519 keypair (`aoide-storage::identity`, the pairing
workstream's substrate — the ceremony itself is P-P2, `state/peer-pairing-
inbound.json`/`state/peer-pairing-outbound.json` below). Two files, NOT one
JSON record — deliberately split so the sensitive half never shares a file
with anything derivable:

- `ed25519.key` — the raw 32-byte private seed, written ONCE at mint into
  `identity/` (locked to `0700` via `aoide_storage::fs::secure_private_dir`
  before anything is written into it) via
  `aoide_storage::fs::atomic_write_private` — the temp file is created
  ALREADY at `0600` and renamed into place, never `0600`'d after the fact
  — and never rewritten after. **Never a JSON value, never inside a
  `Serialize`/`Deserialize` type, never printed, never logged, never on any
  wire** (`PAIRING.md`'s kill-list) — `aoide identity`'s `--json` output
  below is the ONLY externally-visible shape this identity has.
- `created_at` — a plain ISO-8601 UTC string, `0644` (not sensitive),
  written once alongside the key.

Lazy-minted on first need (`aoide identity`, or a future `peer pair` — both
route through the same `identity::load_or_mint`); every call after the
first is an idempotent read of the same two files.

**Not the same key `aoided` signs a sealed session credential with (LANE
IDENTITY P-ID1, §4's `seal` field above).** This on-disk keypair's role
stays exactly the peer wire (`wire_auth.rs`'s signed A2A requests) — the
daemon's seal key is a SEPARATE keypair, minted in-process via
`identity::mint_ephemeral` and held only in memory, deliberately never
written here or anywhere else on disk. See §4's `seal` paragraph for why.

`aoide identity --json`:

```json
{
  "pubkeyHex": "676d9f745100a03b0a9832e9a8ebb36a140d964289b2d65076c71a7f6b9c36a",
  "fingerprint": "67:6d:9f:74:51:00:a0:3b",
  "createdAt": "2026-08-25T00:00:00Z"
}
```

`pubkeyHex` is the full 32-byte public key, hex, no separator.
`fingerprint` is the same key's first 8 bytes, hex, colon-separated — a
short display label, distinct from P-P2's SAS (short authentication
string), which is derived from BOTH sides' keys plus nonces at pairing
time, not from one side's key alone. Nothing here is authenticated against
a peer until the pairing ceremony below runs.

### `state/peer-pairing-inbound.json` / `state/peer-pairing-outbound.json` — **v0** (P-P2, `docs/architecture/PAIRING.md`)

The pairing ceremony's own parked state (`aoide-storage::pairing`) — two
separate files for the two directions a box can be in mid-ceremony,
gitignored root-runtime `state/` (same tier as `state/identity/` above),
each additive/tolerate-missing (an absent file is simply "nothing
pending", never an error). Ids are STABLE 8-hex-char values
(`gen_request_id`), never array-position — unlike `state/stage/pending.json`
(§4 above), a pairing correlation must survive both processes exiting and
an asynchronous `aoide/pairPoll` arriving arbitrarily later (Design A, task
#119 — this used to be an asynchronous `aoide/pairApprove` callback landing
on A's side; now it's A itself polling B's side, whenever it gets around to
it, possibly long after `peer pair` exited).

`peer-pairing-inbound.json` — requests THIS instance has parked as the
approver (`park_inbound`, written by `aoide/pairRequest`'s handler):

```json
{ "schemaVersion": "0", "requests": [
  { "id": "a1b2c3d4", "pubkeyHex": "<requester's 64-hex pubkey>",
    "name": "box-a", "originAddr": "203.0.113.4", "url": "http://box-a:8710/",
    "requesterNonceHex": "<32-hex>", "approverNonceHex": "<32-hex>",
    "requestedAt": "2026-08-25T00:00:00Z", "expiresAt": "2026-08-25T04:00:00Z",
    "approved": false, "tries": 0, "selfVia": "ssh://khoa@box-a" } ] }
```

`approved` (Design A, task #119 — additive, `#[serde(default)]`, absent on
a legacy record loads `false`): flipped `true` by `peer pair approve`
(`mark_inbound_approved`) once THIS instance's own operator confirms the SAS
— purely local, no wire call. Never removed on approval — an approved entry
stays parked, exactly so `aoide/pairPoll` (§6's pairing-wire subsection) can
still find and release it whenever the requester gets around to polling; it
is cleaned up only by the ordinary expiry sweep, same as any other entry.
`tries` (task #120 P3 — additive, `#[serde(default)]`, absent loads `0`):
how many wrong pairing codes have been typed against this entry
(`record_inbound_code_try`), cumulative across `peer pair approve`
invocations and across the interactive prompt and scripted `--code` paths
alike; the third cumulative mismatch auto-denies (the CLI's own
`take_inbound` removal, audited `auto-deny-on-code-mismatch`), so a
persisted value is always below 3.
`selfVia` (task #131 — additive, `#[serde(default, skip_serializing_if =
"Option::is_none")]`, absent on a legacy record loads `None` and is never
written back when absent) is the wire's own `selfVia` claim, carried
through verbatim from `aoide/pairRequest`'s params with no validation here
(never eagerly parsed — the same "only ever parsed at dial time" stance
every other recorded `via` string already holds). `peer pair approve`
reads it at commit time: present, the resulting peer record gets `url:
http://127.0.0.1:<port>/` (loopback-as-seen-from-the-far-side — B, reached
only through A's own tunnel, can never dial `entry.url`'s
requester-observed host directly), where `<port>` is A's OWN door port
parsed off `entry.url` (`aoide-client::commands::port_from_url` — A's own
`self_url` already encodes it; falling back to `AOIDE_A2A_PORT`/`8710`
only when that parse fails, never defaulting to it outright — an earlier
pass of this fix wrongly read B's OWN port here, which names nothing
about A), and `via` set to the claim itself;
absent, the record gets `entry.url` verbatim and `via` stays unset — the
same shape this file's commit path always produced before task #131.

`peer-pairing-outbound.json` — requests THIS instance sent as the
requester and is still waiting to poll for approval on
(`park_outbound`, written by `peer pair`):

```json
{ "schemaVersion": "0", "requests": [
  { "id": "a1b2c3d4", "url": "http://box-b:8710/", "name": "box-b",
    "pubkeyHex": "<approver's 64-hex pubkey>", "requesterNonceHex": "<32-hex>",
    "requestedAt": "2026-08-25T00:00:00Z", "expiresAt": "2026-08-25T04:00:00Z" } ] }
```

Both are read via `list_inbound`/`list_outbound` (lazy-sweep expired
entries on read, no background timer) and consumed exactly once via
`take_inbound`/`take_outbound` — an inbound entry is taken only by approve
(BEFORE approval) or reject, never by `aoide/pairPoll` itself, which only
ever READS it; an outbound entry is taken once its own confirm-then-commit
lands, or by reject. A pubkey mismatch on a poll's release leaves the
outbound entry exactly as it was, never re-parked or destroyed. Neither
file, nor anything derived from it, ever carries a private key — only the
two sides' public keys and nonces, the same public transcript the SAS
above is derived from. `aoide peer pair watch` (§6's "Pairing events
feed" subsection, P-P5) re-derives its own actionable set from exactly
these two files via `list_inbound`/`list_outbound` — the events feed a
watcher tails is a trigger only, never a second source of truth for
what's parked here.

### Secrets home — NOT under `state/`, contract lives in `crates/secrets/README.md`

Workstream SECRETS's broker (`aoide secrets serve`) keeps `policy.json`,
`backends.json`, and its own append-only `audit.log` in a SECRETS HOME
directory (`aoide-secrets`'s `home::secrets_home()`: `$AOIDE_SECRETS_HOME`
env override, else a placeholder default until P-V4's nix module provisions
`/var/lib/aoide-secrets`). Deliberately absent from this section, on
purpose, not an oversight:

- It is broker-uid-owned, not operator-uid-owned like every file above —
  `state/` is this box's OPERATOR's gitignored runtime tree
  (`aoide_protocol::aoide_home()`-relative); the secrets home belongs to a
  DIFFERENT (eventually separately-provisioned) uid entirely once P-V4
  deploys it, so folding it under `state/` would misstate who owns it.
- Its shapes are still v0 and still change fast (P-V3 added `totp.secret`
  and the replay ledger's `totp-replay.json` persistence; P-V4 fixes the
  real path and permissions) — `crates/secrets/README.md`'s "Named seams"
  section is the living source of truth, updated in the SAME commit as any
  shape change (this crate's own `AGENTS.md`), rather than a second copy
  here that can drift.
- The FILE shapes (`policy.json`/`backends.json`/`totp.secret`/
  `totp-replay.json`) stay `crates/secrets/README.md`'s "Named seams"
  territory, not this document's — they change with the crate, not with
  this document's release cadence, and nothing outside the `aoide-secrets`
  crate reads them directly.

### Secrets backends — `file`/`age` are the only supported implementations (P-G1, task #70)

`aoide`'s own two built-in backends — `file` (plain `0600` files under
`<secrets_home>/store/`) and `age` (age-encrypted `0600` files under
`<secrets_home>/values/`, decrypted with an identity this crate lazily
mints on the first `age`-backed `put`, never on a `get`) — are the ONLY
backend IMPLEMENTATIONS this crate supports (User ruling 2026-08-23).
`pass`/`gopass`/`bw`/`sops` remain DOCUMENTATION-ONLY presets
(`crates/secrets/README.md`'s "Backend presets"): copy the shape into
`backends.json` by hand, but integrating with any of those tools is
unsupported, untested territory this crate makes no promise about —
`backend.rs` carries no per-backend knowledge of any of the four and no
test in this crate exercises them.

`secrets add` with no `--backend` flag records `age` — the DEFAULT since
P-G1 (`file` was the sole implementation, and a hard requirement, before
it); `--backend file` is the explicit fallback. The default only affects a
brand-new `add`; an already-recorded policy's `backend` field is never
touched by it. A `Backend` may also carry an OPTIONAL third template,
`has` (`#[serde(default)]`, task #70) — `has_value` runs it directly (exit
0 = has a value) when present, falling back to the pre-existing
`get`-and-discard probe when absent, so a `backends.json` written before
this phase loads and behaves identically either way.

**Bounded backend shell-outs (task #74).** Every backend `get`/`set`/`has`
template shell-out, and the `age` backend's own `age-keygen` identity
mint, routes through one bound (`backend::run_backend_command`/
`backend::wait_bounded`): `AOIDE_SECRETS_BACKEND_TIMEOUT` (whole seconds,
env-only, default 10s) caps how long a template may run. Past the
deadline, the WHOLE PROCESS GROUP the shell-out spawned is `SIGKILL`ed —
never just the immediate `sh`, so a pipeline the template itself forked
can't survive as an orphan — and reaped, and the caller gets a taught
error naming the backend, the op (`get`/`set`/`has`), and the env knob —
never the template text, which can't carry a secret value in the first
place (a value only ever rides a template's own stdin/stdout, never its
command line).

**Atomic SET + code-owned built-in templates (task #82).** Both built-in
`set` templates write to a `.tmp` (`file`) / `.age.tmp` (`age`) sibling in
the SAME directory first, then `mv` it over the real destination path — a
`set` interrupted at any point (broker killed, disk full, a timed-out
template) leaves either the OLD value completely intact or a harmless
orphaned `.tmp`/`.age.tmp` file, never a torn value at the real path. A
future `get`/`has` never sees a partial file, and a failed `set` can no
longer destroy a good existing value on its way to failing.
**`backends.json`'s `file`/`age` entries are CODE-OWNED, not
operator-editable** — `backend::resolve_backend` is the one seam
`fetch_value`/`has_value`/`store_value` route through: for these two
recognized built-in names, the `get`/`set`/`has` TEXT stored on disk is
ignored in favor of this binary's own current compiled default, so a
template fix or improvement (this task's atomicity change, or a future
one) reaches every already-deployed `backends.json` the moment the broker
restarts, with no migration step. Presence of the name in `backends.json`
is UNCHANGED — still required, still what `seed_default_backends`/
`backfill_missing_backends` guarantee — only the TEXT under those two
names stops being authoritative. Any OTHER backend name (`pass`/`gopass`/
`bw`/`sops`, or an operator-custom entry) is read from `backends.json`
exactly as stored, unaffected.

**`secrets migrate <name> [--backend <target>]` (P-G2, task #72)** moves a
secret's stored value from its policy's current backend to a target one
(default `age`) with a fixed ordering, never reordered: fetch from the
source → (mint the target identity if needed) → store on the TARGET →
flip and durably save `policy.json` → remove the OLD value LAST. A failure
before the policy save leaves everything untouched (old value in place,
policy unflipped); removing the old value only ever happens after the
flip has already saved, and is BEST-EFFORT — a removal failure is
reported honestly in the success message but never rolls back or blocks
the already-successful migration. Old-value removal is
BUILT-IN-SOURCE-ONLY and path-derived, never a guess: it recognizes
exactly `file` and `age` as source backends (the same paths their own
`set` templates write to) and does nothing for any other backend name,
built-in or not — a `pass`/`gopass`/`bw`/`sops` row, or an operator-custom
entry, is never touched or removed by `migrate`. `migrate` is an admin
command (euid-guarded exactly like `add`/`rm`/`grant`, direct-home, no
socket) with NO cross-process lock against a concurrently-running broker
daemon — a `migrate` racing a live `secrets put`/`exec` against the same
secret through the running daemon is an unprotected window, a known
limitation, not fixed here.

### Secrets wire — the machine-consumer contract (P-V4c, parking P-N2)

Promoted out of "living in the crate only" (the promotion criterion this
section states below) because P-V4c makes it explicit: the unix-socket
JSON-lines wire is a FIRST-CLASS API, not merely `secrets exec`'s private
implementation detail. A service (verba voluntia, an aoide-side Melete
model) is meant to speak this wire DIRECTLY — connect the socket, write a
request line, read a reply line — with no LLM and no `aoide` binary in the
loop at all; `secrets exec`/`secrets put` are convenience wrappers over the
same two ops for a human or an agent at a terminal, not the only door onto
them. The canonical implementation (and the one place a wire CHANGE lands
first) is still `crates/secrets/src/broker.rs`'s module doc — this section
restates it for a consumer who never reads this repo's Rust. **The A2A door
and its client are the first real machine consumers of this wire**
(`aoide_secrets::client::resolve_bounded`, exported from this crate as a
library function rather than copied into either caller, per the cross-crate
"no copying" discipline `pkgs/aoide/crates/AGENTS.md` holds): the door
resolves its own inbound `Authorization: Bearer` expectation as consumer
`a2a-door` (§6's security posture), and its outbound peer client resolves a
per-peer bearer to present as consumer `a2a-client` (§7's `Peer.bearerSecret`)
— both self-asserted, both subject to the "consumer is self-asserted" honesty
note below, both using `resolve_bounded`'s bounded, `wait:false` shape so
neither can be wedged parked on a misconfigured `requireTotp` secret the way
a human at a terminal might tolerate. **The CONNECT itself is bounded too
(rider task, alongside #75/#81/#82)** — every client-side op in
`aoide_secrets::client` (`resolve`/`resolve_bounded`/`put`/`pending`/
`approve`/`dismiss`) connects through a hand-rolled 5-second overall bound
(`client::connect_bounded`, over `libc`, since `std`'s `UnixStream` has no
`connect_timeout`), not only the reads `resolve_bounded`'s own
`set_read_timeout` already bounded — a machine consumer hitting a broker
with a saturated accept backlog (plausible when many callers legitimately
hold a parked connection for up to `park::park_timeout()`, default 300s)
now fails fast instead of hanging on the connect syscall itself. The
mechanism is a BOUNDED RETRY of the `connect(2)` syscall on `EAGAIN` (what
Linux actually returns for a saturated `AF_UNIX` backlog — immediately,
never `EINPROGRESS`, so there is no fd event to wait on), plus `poll()` on
`EINPROGRESS` (a genuine half-open connection) — not a single poll loop;
an earlier version of this function handled only the `EINPROGRESS` case
and made the saturated-backlog scenario worse, caught and fixed on review.

**Transport**: connect `$AOIDE_SECRETS_SOCKET` (else the canonical deployed
path `/run/aoide-secrets/secrets.sock`, P-V4d — corrected from an earlier
secrets-home-relative default after the first live deployment found it sent
an env-less client to the wrong path) as a unix stream socket. One JSON
object per line, newline-terminated, on both sides.

**The events feed (P-G4, task #77) is a sibling FILE beside this socket, not
a second wire op.** A machine consumer that wants a PUSH signal instead of
polling `resolve`/`pending` may instead tail-follow
`$AOIDE_SECRETS_EVENTS` (else `socket_path`'s own parent directory joined
with `events.jsonl` — the default deployed path is
`/run/aoide-secrets/events.jsonl`, next to `secrets.sock`): one JSON object
per line, the exact bare `{"event": "<kind>", ...}` payload shapes
`crates/secrets/README.md`'s "Broker notifications" documents, capped at
1 MiB (a truncate-to-empty in place past the cap, never rotated — a
consumer tailing it must treat any shrink as "reopen and read from the
new start," the same handling `aoide-secrets`'s own `watch::Follower`
gives it). This exists because the deployed broker unit's
`ProtectHome=true` blocks the OTHER mirrored destination below
(`$AOIDE_ROOT/log`) from ever landing — see that crate's `README.md` for the
full incident and mechanism; this paragraph only states the shape for a
consumer that never reads this repo's Rust. `aoide secrets watch` (the
in-crate consumer) surfaces a parked ask through this feed in about a
second; its own `client::pending` poll remains the AUTHORITY, reconciling
once at startup and again on a 30-second safety tick that is a
RECONCILIATION BACKSTOP (a missed line, a truncation, a broker restart),
not the primary delivery path — see that crate's README's "Watching
events" for the full mechanism.

The framing (P-N2c,
FIX 1): write ONE request line, then read **zero or more INTERIM lines
followed by exactly one FINAL reply line** — an interim line is any line
whose object carries `"interim":true`; the first line without it is the
final reply and ends that request. Today the only interim line is the park
announcement below; a future op/mode extends the wire by adding a new
interim shape or a new `op`, never by overloading the final-reply shape or
widening `wait` (a bool, see below) into something richer. The connection
may be reused for further request/reply pairs or dropped after one — but a
dropped connection is NOT free of state: a `resolve` that parked (below) IS
per-connection state, held in the broker's in-memory registry until an
`approve`/`dismiss` from another connection or the timeout resolves it: it
is not delivered to a `resolve` reply from any other connection. Dropping
the parked connection does not cancel the ask — it abandons it, unreachable
until it times out (a corollary of the ask outliving that one socket).

**`resolve`** — read a secret's value:
```text
-> {"op":"resolve","secret":"<name>","consumer":"<consumer>","totp":"<code>"?,"argv0":"<cmd>"?,"reason":"<text>"?}
<- {"ok":true,"value":"<value>"}
<- {"ok":false,"error":"<message>"}
```
`totp`/`argv0`/`reason` are optional. `reason` (P3) is free-text,
self-asserted, DISPLAY-ONLY context for why this ask exists — it only ever
matters when the resolve PARKS (below): it rides onto the parked-ask
registry row, the `pending` reply, and the `parked` events-feed line, for a
popup/prompt surface to show. It never gates anything; `secrets exec`
derives it automatically from the wrapped command when `--reason` isn't
given (`crates/secrets/README.md`'s "Secrets wire" section has the full
derivation). Error strings (never containing the secret's
value): `"secret not found"`; `"consumer not authorized for this secret"`;
`"requireTotp is set but no TOTP enrollment exists on this host yet"`;
`"requireTotp is set but no totp code was provided"`; `"malformed totp
code"`; `"totp code invalid or expired"`; `"totp code already used"`
(replay); `"unknown backend `<name>`"`; `"backend `<name>` exited
<status>"` (the backend's own stderr never rides this reply — it is
`eprintln!`'d to the broker's own stderr only).

**Parking (P-N2): a `requireTotp` resolve with no code now WAITS instead of
refusing outright.** Before this phase, `"requireTotp is set but no totp
code was provided"` was an immediate refusal. Now, when a policy's
`totp_required` is `true`, an enrollment exists on this host, and `totp` is
absent/empty on the wire, the broker instead PARKS the ask: it registers
`{id, secret, consumer, requestedAt}` in an in-memory registry and holds the
REQUESTING CONNECTION open — never storing or fetching a value at this
point — until an operator completes it from a SEPARATE connection
(`approve`/`dismiss` below) or a timeout elapses. The connection's own
thread blocks on this wait; the broker's accept loop itself never blocks
(thread-per-connection, `crates/secrets/src/broker.rs`'s module doc), so
every OTHER connection — including a totally unrelated `resolve` — is
admitted and served normally while one sits parked. A `resolve` WITH a
non-empty `totp` is completely unaffected by this phase — same fast path,
same errors, as before. An `automation`-open listed consumer (the gate
above) never parks either, exactly as it never required a code before.

**The park announces itself (P-N2c, FIX 1).** The instant an ask parks, the
broker writes an INTERIM line (this section's Transport paragraph) down the
SAME requesting connection, before ever blocking on the wait:
```text
<- {"interim":true,"parked":true,"id":"<id>","timeoutSecs":<N>}
```
then later the final reply (granted/denied/dismissed/timed-out) as
described below. Without this line a caller has no way to learn its own
ask's id short of a separate `pending` call raced against the park — `aoide
secrets exec` prints it to STDERR as `parked as ask <id> — complete with:
aoide secrets approve <id> --totp <code>  (or dismiss <id>); times out in
<N>s` so an interactive caller is never left staring at a silent hang
indistinguishable from a wedged broker.

**Ids are nonce-prefixed (P-N2c, FIX 4), not a bare counter.** An id has the
shape `<4-hex-nonce>-<counter>` (e.g. `3f2a-7`) — the nonce is 2 random
bytes read once per broker process start and shared by every id that
process ever mints; the counter still increments per-ask, unreused, exactly
as before. This exists so a held id can never silently address a DIFFERENT
ask after a broker restart: the counter alone would restart at 1, so a
stale id typed against a freshly-restarted broker could otherwise approve
an unrelated ask that happens to reuse the same small number. An id whose
nonce doesn't match the CURRENT process's — including any pre-P-N2c bare-
counter id — is simply unknown, the same `"unknown pending id"` error a
never-existed id gets; it is never routed to a same-numbered ask under a
different nonce.

**A registry-wide cap bounds how many asks may park at once (P-N2c, FIX
3b)**, default 32, `AOIDE_SECRETS_PARK_CAP` env override (whole number;
same env-only precedent as the timeout knob below). Beyond the cap, a
codeless `resolve` gets the SAME immediate refusal `wait:false` produces
(never a park), with a reason naming the cap and its env knob so a caller
knows to retry inline or wait for an operator to clear a pending ask. This
exists because an unbounded in-memory registry is an unbounded-memory
denial-of-service surface; the cap makes that bound explicit and
operator-tunable instead of implicit.

`resolve` gained one new optional field:
```text
-> {"op":"resolve","secret":"<name>","consumer":"<consumer>","wait":false}
<- {"ok":false,"error":"requireTotp is set but no totp code was provided"}
```
`wait` is optional and defaults to `true` (absent = wait/park, the new
default behavior) — `wait:false` restores the EXACT pre-P-N2 immediate
refusal for a machine caller that has no way to type a code. There is no
CLI flag for this — it is wire-only, reachable only by a direct socket
speaker (this section's own "first-class API" framing above).

On timeout (default 300 seconds, `AOIDE_SECRETS_PARK_TIMEOUT` env override
in whole seconds — no config-file knob exists in the secrets home for this;
env-only, same precedent as `AOIDE_SECRETS_HOME`/`AOIDE_SECRETS_SOCKET`),
the parked connection gets:
```text
<- {"ok":false,"error":"the pending TOTP ask for `<name>` timed out after <N>s (AOIDE_SECRETS_PARK_TIMEOUT to change the default) — resolve again with an inline `--totp <code>`, or approve the next ask before it expires with `aoide secrets approve <id> --totp <code>`"}
```
naming the timeout, the env knob, and BOTH completion paths — the ask is
removed from the registry once timed out (a late `approve`/`dismiss` against
that id then gets the same `"unknown pending id"` error a never-existed id
would).

**`pending`** — list every parked ask (never a value):
```text
-> {"op":"pending"}
<- {"ok":true,"pending":[{"id":"<id>","secret":"<name>","consumer":"<consumer>","requestedAt":<unix-seconds>,"peerUid":<uid-or-null>,"reason":<string-or-null>,"origin":{"username":<string-or-null>,"pid":<int-or-null>,"comm":<string-or-null>,"hostname":<string-or-null>}},...]}
```
Never errors (an empty queue is `{"ok":true,"pending":[]}`); not audited —
a read of in-memory state only, same precedent `session pending list` already
sets. `peerUid` (task #73) is ADDITIVE over the pre-#73 shape — the
kernel-truth `SO_PEERCRED` uid of the connection that parked this ask
(`null` when it could not be read), alongside the pre-existing
self-asserted `consumer` name; see "Peer identity" below. `reason`/`origin`
(P3) are additive again — `reason` mirrors whatever the parking `resolve`
sent (above); `origin` is best-effort "who/where," captured ONCE at park
time from the SAME `SO_PEERCRED` stamp `peerUid` reads (username/pid/comm)
plus the broker's own hostname — every field UNTRUSTED DISPLAY DATA,
`null` per field when unknown, never an error.

**`approve`** — complete a parked ask with a code, releasing the value down
the ORIGINAL parked connection (never this reply):
```text
-> {"op":"approve","id":"<id>","totp":"<code>"}
<- {"ok":true}
<- {"ok":false,"error":"<message>"}
```
Validated with the SAME RFC 6238 verify + single-use replay ledger an
inline `resolve` code uses — the code is consumed identically either way.
An invalid/expired/already-used code leaves the ask PARKED (never removed)
and the ledger UNBURNED, so a caller can simply retry with the right code;
only a code that validates resolves the ask one way or another. Errors:
`"unknown pending id `<id>`"`; `"malformed request: `id` is required"`;
`"malformed request: `totp` is required"`; plus every `resolve`-shaped TOTP
error above (`"totp code invalid or expired"`, `"totp code already used"`,
...). This reply NEVER carries a `value` field — the value only ever
reaches the ORIGINAL parked connection's own `resolve` reply.

**`approve` re-runs the FULL authorization gate at release time (P-N2c, FIX
2), not just the TOTP check.** Before this fix, `approve` validated only
the code and then fetched by backend/key — a `secrets revoke` issued WHILE
an ask sat parked did nothing to stop that ask's eventual release, and the
same gap would have silently bypassed a future `remote` gate too. Now,
AFTER the code validates (so it is consumed from the replay ledger either
way — this is deliberate: a burned code beats a reusable one, even on a
path that goes on to deny) and BEFORE any value is fetched, the broker
re-checks the SAME exists + consumer-authorization gate `resolve` itself
runs, against the ask's STORED consumer. A revoked/removed consumer at this
point denies BOTH replies — the approver's own `approve` reply and the
original parked caller's `resolve` reply get the identical
`"consumer not authorized for this secret"` (or `"secret not found"`)
error; the ask is removed from the registry either way, never left
dangling.

**`dismiss`** — refuse a parked ask outright, no code needed, and
**peer-uid-gated (task #73)**:
```text
-> {"op":"dismiss","id":"<id>"}
<- {"ok":true}
<- {"ok":false,"error":"unknown pending id `<id>`"}
<- {"ok":false,"error":"<peer-uid-mismatch refusal, names both uids>"}
```
The parked connection gets `{"ok":false,"error":"the pending TOTP ask was
dismissed before a code was provided"}` on its own `resolve` reply (P-N2c:
no "by an operator" claim — any member of the consumers group that can
reach the socket can dismiss, not only an operator, so the message no
longer asserts who); the dismisser's own reply only confirms the dismissal
happened. **Task #73 narrows who "any member... that can reach the socket"
actually means**: the dismissing connection's own kernel-truth peer uid
(`SO_PEERCRED`, "Peer identity" below) must match the ask's OWN stamped
peer uid (recorded at park time), or the broker's own effective uid — an
unmatched dismiss is refused with a taught error naming both uids, and the
ask is left exactly where it was (never consumed by a failed unauthorized
attempt). An unidentified dismissing connection (peer cred unreadable) is
NEVER authorized, even against an ask whose own peer uid is also
unidentified — fail closed, never open, on a missing kernel fact. `approve`
is UNCHANGED by this — it stays open to any local caller reaching the
socket; the TOTP code is its gate, not identity.

Audit (both destinations, name-only, same discipline as `resolve`/`put`):
park/approve/dismiss/timeout each write one line — the id, the secret name,
the consumer (park only), and the outcome — never a value, never a code.

**The automation gate (P-N1) narrows, never widens, `requireTotp`.** A
policy's `automation` field (`{"enabled":<bool>,"consumers":[<name>,...]}`,
absent on an old `policy.json` means `{enabled:false,consumers:[]}`) can
only ever RELAX the TOTP requirement for the consumers it names — it can
never impose one, and it never touches the `consumers`-authorization gate
above it. Gate order is: exists -> consumer authorized -> remote-origin
(P-ID4, below) -> TOTP required
(`requireTotp AND NOT (automation.enabled AND the requesting consumer is
IN automation.consumers)`) -> fetch. When `requireTotp` is `false`,
automation has nothing to relax and every caller resolves exactly as
before this field existed; when `requireTotp` is `true` and automation is
CLOSED (`enabled:false`) or the requesting consumer isn't LISTED, `totp`
is checked exactly as it always has been. Admin commands: `secrets automate
<name> on|off` flips `enabled`; `secrets automate <name> grant|revoke
<consumer>` edits `consumers` (same name validation as every other
consumer/secret name in this crate). **Honesty note, same shape as
`resolve`'s own `consumer` field above:** `automation.consumers` names are
matched against the SAME self-asserted wire `consumer` field, so an
automation-open secret is effectively code-free for any local socket
caller claiming a listed name — the sealed session credential (§4's
identity section) authenticates the calling SESSION and its origin CLASS,
never this string, so consumer-NAME authentication remains a separate,
unbuilt axis. This is a documented limitation, not a bug, mirroring the
replay-ledger ruling `crates/secrets/AGENTS.md` already carries for the
identical reason.

`policy.json` also gained a `remote` boolean (P-N1, default `false`,
absent on an old file means `false`). **No behavior change today** — no
non-local entry point onto this broker exists yet — but it is a crate
invariant (`crates/secrets/AGENTS.md`): every non-local entry point added
later (mesh replication, a network door) MUST refuse a secret whose
`remote` is `false` before ever touching its backend. `secrets expose
<name> on|off` flips it.

**The origin gate (LANE IDENTITY P-ID4) — `allowRemoteOrigin`, the third
policy axis, and the first real consumer of the sealed session
credential.** `policy.json` gained an `allowRemoteOrigin` boolean (default
`false`, absent on an old file means `false`; `secrets allow-remote-origin
<name> on|off` flips it, same admin family as `expose`). The three axes
are distinct and never conflated: `remote` = may this secret be SERVED
through a non-local entry point (transport); `automation` = may a listed
consumer skip TOTP (code); `allowRemoteOrigin` = may a session that a
REMOTE PEER created resolve this secret LOCALLY (caller provenance). The
broker resolves each connection's caller from kernel facts alone —
`SO_PEERCRED` pid -> real `/proc` ancestry -> sealed session record ->
seal verified against the daemon's LIVE `ping`-fetched public key
(`aoide_storage::attest::attested_caller`, the SAME walk/verify the send
gate uses, fresh-starttime pid-reuse defense included; nothing the wire
asserts ever enters this) — and a caller whose sealed `originClass` is
`peer:*` is refused, before the TOTP/park branch, unless the secret's
`allowRemoteOrigin` is on; the refusal names the flag, the session, and
its origin, and audits name-only. **The boundary, exactly:** this gate
NARROWS positively-attested remote-origin sessions; it does not
authenticate local ones. An UNIDENTIFIED caller (no sealed session in its
ancestry, an unreachable daemon, an unreadable roster) is NOT refused by
this gate — same-uid honesty (OQ1-A) means local unidentified callers
were always admitted, and this gate keys ONLY on positive attestation.
The same residual composes through the reseal sweep: a same-uid process
can append an unsealed roster row asserting `origin: local` for its own
pid, and the daemon's sweep signs whatever origin the row asserts —
laundering a forged local class into a POSITIVE attestation, not merely
detaching into unidentified. Both shapes are the one OQ1-A attacker;
neither is closable without OQ1-B or an authenticated registration path.
In the packaged cross-uid deployment (`modules/nucleus/secrets.nix` runs
the broker as the `aoide-secrets` system user) the operator's daemon
socket (`0600` inside their `0700` `$XDG_RUNTIME_DIR`) and their
`state/stage/sessions.json` are both unreachable from the broker, so
every caller there resolves UNIDENTIFIED today — the gate bites wherever
the broker runs as the operator's own uid (the cargo-only/dev deployment
`crates/secrets/src/home.rs` documents); a cross-uid attestation channel
is a named, deliberately-unbuilt remainder, not an improvised file drop
(a pubkey file next to a same-uid-writable roster is exactly the channel
the identity section above rules out). Consumer-name authentication
remains a separate, unbuilt axis — the seal authenticates the SESSION and
its CLASS, never the self-asserted `consumer` string.

**`put`** — write a secret's value (P-V4c; `overwrite`/`exists`/`replaced`
added P-67, "warn before overwrite"):
```text
-> {"op":"put","secret":"<name>","value":"<value>","overwrite":<bool>?}
<- {"ok":true,"replaced":<bool>}
<- {"ok":false,"exists":true,"error":"<message>"}
<- {"ok":false,"error":"<message>"}
```
No `consumer` field, and NEVER gated by `requireTotp` — `put` is CLI-only/
admin-side (never agent-facing), so there is no separate consumer identity
to authorize and no code check to run (`crates/secrets/src/broker.rs`'s
module doc has the full reasoning). `put` never creates a policy — `secrets
add` owns that — so error strings mirror `resolve`'s policy-side ones:
`"secret not found"`; `"backend `<name>` has no `set` template"`; `"backend
`<name>` exited <status>"`. The granted reply carries no `value` field at
all — nothing to leak, since `put`'s payload flows client-to-broker, never
back.

`overwrite` is OPTIONAL — absent means `false`. When it is false and the
named secret ALREADY has a stored value (probed via the SAME `get` template
`resolve` would run, broker-side only — `crates/secrets/src/backend.rs`'s
`has_value`), the broker refuses with the distinct `{"exists":true}` flag
above rather than silently overwriting; a consumer of this wire must check
`exists`, never string-match the `error` text, to detect this case. Sending
`overwrite:true` stores unconditionally and reports whether it replaced an
existing value (`"replaced":true`) or was a first-ever store
(`"replaced":false`) — `secrets put`'s own `--force` flag is what sets
`overwrite:true` on the wire; without it, a CLI caller on a terminal is
prompted `y/N` and, on yes, retried with `overwrite:true` automatically.
**Wire compatibility**: an OLD client (no `overwrite` field at all) talking
to a NEW broker now gets the `exists` refusal on a second `put` instead of
a silent overwrite — a deliberate tightening, not a bug, since absent has
always meant `false`. A NEW client talking to an OLD broker has its
`overwrite` field silently ignored (old brokers accept and discard unknown
fields) and the put silently overwrites, exactly as every `put` did before
this feature — acceptable during a mixed-version deploy window, not a
regression from before P-67 existed.

**Malformed-request errors** (any op): `"malformed request: not valid
JSON"`; `"malformed request: missing `op`"`; `"unknown op `<name>`"`;
`"malformed request: `secret` and `consumer` are required"` (`resolve`);
`"malformed request: `secret` is required"` (`put`); `"malformed request:
`id` is required"` (`approve`/`dismiss`); `"malformed request: `totp` is
required"` (`approve`).

**The trust model — two gates, not one.** Reaching the socket AT ALL is
gate one: the socket is `0660`, group `aoide-secrets-access` (P-V4's
deployment) — anything that can connect has already proven group
membership, which is a coarse, host-level "this uid may talk to the
broker" boundary. `policy.json`'s per-secret `consumers[]` list is gate
two, finer-grained, keyed on the wire's `consumer` field.

**Honesty note: `consumer` is SELF-ASSERTED.** Nothing on the wire
authenticates it — any process that has already cleared gate one (socket
group membership) can claim to be any consumer name and receive that
name's grants. This is a documented, deliberate limitation, not a bug:
authenticated consumer identity is a separate, not-yet-planned scope
(#51-adjacent — the same honesty note the replay ruling in
`crates/secrets/AGENTS.md` already carries for the identical reason). A
policy's `consumers[]` list is a courtesy label on top of the real
boundary (socket group membership), not a cryptographic one, until that
lands. **Task #73 does not change this** — see "Peer identity" immediately
below for the separate, orthogonal fact it DOES add.

**Peer identity (`SO_PEERCRED`, task #73).** Every accepted connection's
kernel-truth `uid`/`gid`/`pid` is read once, at connection start, via
`SO_PEERCRED` (`crates/secrets/src/peercred.rs`) — the connecting
process's REAL uid, verified by the kernel, independent of anything the
wire request itself claims. A read failure is an UNIDENTIFIED connection
(`None`), never a panic, never a fabricated uid; every decision keyed on it
fails CLOSED, never open. This is recorded ALONGSIDE the self-asserted
`consumer` name above, never in place of it — `consumer` is still
unauthenticated; the peer uid is a separate fact. Two places this fact is
used: `pending`'s reply carries each ask's stamped `peerUid`
(additive field, above), and `dismiss` is gated on it (above) — every
resolve/park/approve/dismiss/put audit line also carries the acting
connection's peer uid now, alongside the pre-existing self-asserted name.

**Admin mutations (task #79) — a new op family, built on Peer identity
above.** Every admin CRUD command (`add`/`rm`/`grant`/`revoke`/`set-totp`/
`automate`/`expose`/`migrate`) is now reachable over this SAME socket, as
the daemon's SINGLE-WRITER path — the live broker becomes the one process
serializing every `policy.json`/backend-store mutation, closing the TOCTOU
a concurrent `put`/`exec` and an admin command used to have when both raced a
direct-home write:
```text
-> {"op":"admin","verb":"add","name":"<name>","backend":"<backend>","key":"<key>","requireTotp":<bool>?,"consumers":[<name>,...]?}
-> {"op":"admin","verb":"rm","name":"<name>"}
-> {"op":"admin","verb":"grant"|"revoke","name":"<name>","consumer":"<consumer>"}
-> {"op":"admin","verb":"set-totp"|"expose","name":"<name>","state":"on"|"off"}
-> {"op":"admin","verb":"automate","name":"<name>","action":"on"|"off"|"grant"|"revoke","consumer":"<consumer>"?}
-> {"op":"admin","verb":"migrate","name":"<name>","target":"<backend>"}
<- {"ok":true,"message":"<summary>","changed":["policy:<name>"]}
<- {"ok":false,"error":"<message>"}
```
`changed` is empty on an idempotent no-op, the same discipline every CLI
`Outcome` already holds.

**Gate: ONLY the broker's own effective uid, full stop — stricter than
`dismiss`'s.** Where `dismiss` (above) admits either the ask's own stamped
peer uid or the broker's own uid, `{"op":"admin"}` admits ONLY the
broker's own effective uid — reusing the SAME wording the direct-write
path's admin-identity guard already gives (root explicitly refused, not a
bypass — "plain `sudo` runs as root, and root CAN write here regardless of
file ownership"), so a refusal here teaches the identical fix. An
unidentified connection (peer cred unreadable) is refused outright, the
same fail-closed default `dismiss` holds.

**The CLI tries this socket FIRST; a direct write is the no-daemon
fallback, never a silent downgrade past a live one.** `aoide secrets
<command>` connects and sends the `{"op":"admin"}` request above; only when
the connect itself fails with "nothing is listening" (no socket file, or a
stale one with nothing behind it) does the CLI fall back to writing
`policy.json` directly (the pre-#79 behavior, still euid-guarded the same
way). Any OTHER socket failure — including the broker's own authoritative
`{"ok":false}` denial — is reported as the command's result outright,
never silently downgraded into the fallback; a live-but-sick daemon can
never be bypassed into a direct write racing underneath it. Every admin
command's `Outcome` names which path actually ran (`data: {"path":"broker"}`
or `{"path":"direct"}`).

If a secrets file shape ever needs to be READ by something outside the
`aoide-secrets` crate (a future admin tool, a debugging script), that is the
signal to promote its shape into a numbered subsection here — nothing
about "broker-owned" is permanent, only "not yet a cross-crate contract".

---

## 5. Song shape — **v0**

A **song** (rice) is a committed, **host-agnostic** score. The design thesis:
a song is host-agnostic; ANY host in the fleet performs it by naming it, and
the performance adapts to that host's specifics and its enabled facet/dendrite
set. The **venue (host) decides its instruments; the song carries only the
notes.**

### Selection — `aoide.song`

`aoide.song` (nullOr str, default `null`, declared in
`modules/nucleus/options.nix`) names the song this host performs. A host
replays any committed song with **one line**:

```nix
# hosts/<host>/default.nix
aoide.song = "moonlight";
```

Naming no song performs no song: `aoide.song` defaults to null, and every
committed song's `rice.nix` self-gates on `config.aoide.song == "<name>"`,
which is never true against null — so a host that names nothing gets no
song's config, and the paint facets (which read the active song to bake and
deploy) activate only when a song IS named: no QML tree, no shell service
otherwise, not an empty surface. A host wanting the desktop names its song
explicitly; `sonata` is the shipped standard (`song/songbook/sonata/rice.nix`),
the guaranteed-present baseline every fleet member can opt into by name.
**Renamed (2026-08-14):** the shipped standard song was `default`;
`song/songbook/default/` is now retired outright (its Pantheon design
grammar relocated to
`docs/Aoide-Wiki/references/pantheon/pantheon-grammar.md`, its recorded
aesthetic distilled into `song/songbook/learnings.md` — the rest is
git-recoverable history, not deleted knowledge). No shape change, no version
bump: every "shipped baseline" reference in this section simply names
`sonata` now. Full dated entry: `docs/Aoide-Wiki/ingest/log.md`.

### Self-registration (dendrite discipline)

Committed songs live under `song/songbook/<name>/rice.nix`. `lib/mkHost.nix`
walks `song/songbook` (via `lib/walk.nix`, same as `modules/`) into every
host, so **adding a song is a new folder — never an edit to an import list**.
The empty songbook (just `.gitkeep`) walks to `[]` and is tolerated.

Each song's `rice.nix` **self-gates**, exactly like a dendrite:

```nix
# song/songbook/<name>/rice.nix
{ lib, config, ... }:
{
  config = lib.mkIf (config.aoide.song == "<name>") {
    aoide.livery.palette = { bg = "…"; fg = "…"; accent = "…"; urgent = "…"; };
    # component tier (bar/notif/window) — null falls back to palette
  };
}
```

### Rules (host-agnostic discipline)

- A song sets **ONLY `aoide.livery`** (palette + component tiers) and
  **`aoide.arrangement`** (declared widget/surface types) and — later —
  cover/chime references inside `song/`.
- A song **NEVER** sets host options (monitors, hardware, services) and
  **NEVER** enables facets or dendrites. Those are the venue's decision.
- All note values are **literal nix** — a song never reads `song/` runtime
  paths (`stage/` · `auditions/`), same as the standard.
- Shelving/subfolders follow the walker rules (a `/_` path is skipped).
- The HOST may additionally set `aoide.livery.override.*` (§1, override tier)
  to recolour the song it performs — venue paint over the song's notes, in
  the same one-line register as `aoide.song`. It never edits the songbook. A
  song must **NEVER** set `aoide.livery.override.*` (a song overriding itself
  is meaningless and forbidden — same documented-convention rank as the
  "only defines `aoide.livery`" invariant, `TODO(song-shape v1)` in
  `lib/checks.nix`).

### The songbook is versioned score, not runtime

`checks.no-song-read` (§4) bans reading `song/` **runtime** dirs at eval
(`stage/` · `auditions/` · `catalog/` · `index/`). It
deliberately does **not** list `song/songbook/`: committed songs there are
versioned score, legitimately walked at eval. Walking the songbook never trips
the check.

### Enforcement

`checks.song-shape` structurally asserts every walked songbook path is a
`rice.nix` (a song's module entry) — catching a stray `.nix` that could set
arbitrary host options. The **full** "only defines `aoide.livery`" invariant is
a documented convention here (isolated per-module option-diffing is
disproportionate for v0; see the `TODO(song-shape v1)` in `lib/checks.nix`).

**Migration to v1:** the update playbook migrates `song/songbook/*/rice.nix`
and `livery.json` from v0 to v1 with the livery schema (§1).

### Per-song flavor widgets

Beyond `aoide.livery` notes, a song MAY also carry its own QML for a fixed
set of "flavor" surfaces — committed files, not nix options:

- **Convention:** `song/songbook/<name>/widgets/<slot>.qml`. ANY `.qml` file
  a song drops under its `widgets/` dir becomes a slot named for its
  basename — not a fixed enum. A song omits files for slots it doesn't
  dress. Authoring a slot file alone does not put it on screen: nothing
  renders until a host surface actually embeds a `WidgetSlot` anchor for
  that slot name. The **wired-slot catalog** — which slots a real anchor
  resolves today, which host embeds each, and each slot's extras/fallback —
  lives in `modules/facets/quickshell/qml/slots.md`, documented there only
  once an anchor is actually wired (same "what IS built" discipline as this
  section).
- **Build:** the quickshell facet's derivation
  (`modules/facets/quickshell/default.nix`) walks every committed song's
  `widgets/*.qml` files to `$out/qml/songs/<name>/<slot>.qml`, plus a
  generated `$out/qml/songs/manifest.json` recording which songs authored
  which slots — ALL songs' bodies land on disk at once (home-manager
  installs the tree recursively), which is what makes cross-song live
  staging possible. The manifest shape is unchanged (still
  `{ "<song>": ["<slot>", …] }`) by generalizing the walk from a fixed slot
  enum to "every file under `widgets/`" — additive, no contract-version
  bump.
- **Runtime resolution:** `LiveryState.qml`'s `songName` property (above)
  names the active song; the staging engine (`StagingEngine.qml`) reads the manifest and answers
  "does `<song>` dress `<slot>`"; `WidgetSlot.qml` is the fixed per-slot
  anchor a host surface embeds — it loads the song's file when authored, else
  falls back to shared chrome (or renders nothing, when no fallback exists).
  `lyra rice stage <name>` (§4) drives this live, no rebuild: it stages
  `song` into `livery.json`, `LiveryState`'s `songName` updates, and every
  `WidgetSlot` re-resolves. **Additive (2026-08-15) — widget bodies ride the
  same call:** `rice stage` also syncs the song's whole
  `widgets/` tree into `run/qml/songs/<name>/` (`crate::widgets` in
  `crates/song/`, byte-compared so an unchanged file is never rewritten —
  avoids flicker/reload of every widget on a palette-only stage) and
  regenerates that song's `run/qml/songs/manifest.json` entry, so an edit
  to an EXISTING widget file reaches Quickshell's own file-watcher live too.
  A brand-new slot file still needs a service restart to be discovered
  (the manifest is only read at startup) — **may no longer require a
  restart** now that `lyra quickshell reload` (Quickshell IPC hot-reload
  trigger) rebuilds the whole scene fresh from `shell.qml`, which should
  also re-read `manifest.json`; unconfirmed against a live instance, don't
  rely on this until verified.
- **Fixed injected-prop contract:** a loaded widget receives `livery`
  (`LiveryState`) and `bridge` (`ShellBridge`) always, plus whatever
  slot-specific extras the anchor declares (e.g. the bar slot's `shared`)
  — **never** nix `config.*`. This does not loosen the song-shape rule above:
  a song's `rice.nix` still sets **ONLY** `aoide.livery` — widgets are
  committed QML files carried by the build, not nix options, and a widget is
  structurally incapable of reaching host/facet options through this surface.
- **Playbook:** `song/songbook/update-playbook.md`.

**Additive (2026-08-14) — baseline-fallback resolution:**
`StagingEngine.resolveSong(song, slot)` resolves a slot through a floor, not
just the one song: the active song's own file if it authored the slot, else
**sonata**'s (the shipped standard song — the baseline every song can fall
back to, a fixed constant independent of `aoide.song`'s own default, which
is null), else `""` (the anchor's own facet-side
`fallback` Component, when it has one, or nothing). `WidgetSlot.resolvedSource`
keys off the RESOLVED song, not a bool, so a live song-switch between two
songs that both provide a slot re-triggers correctly instead of silently
sticking to whichever song loaded first.

**Additive (2026-08-14) — `SurfaceSlot`, for slots that own their own
window:** `WidgetSlot` is an `Item` — it can't host a widget whose root IS a
`PanelWindow` (its own layer, namespace, keyboard focus, `GlobalShortcut`; a
`PanelWindow` can't be parented into a layout the way an `Item` can).
`SurfaceSlot.qml` is the non-visual `QtObject` anchor for these slots: the
same `resolveSong` → `Qt.createComponent` → `Component.createObject(null,
props)` mechanism `WidgetSlot` uses, exposing the live instance as `.item`
for the host to call directly (e.g. the bar's clef calling
`.item.toggle()`). A slot's catalog entry in `slots.md` documents which
anchor kind hosts it — a slot that roots a `PanelWindow` also documents its
WlrLayershell namespace there, since the compositor facet's glass
layerrules match on it; that contract travels with the slot.

**Additive (2026-08-14) — helper files:** a widget needing its own helper
components (e.g. the bar's `WorkspaceRow.qml`) drops them alongside it under
the same song's `widgets/` dir, with an UPPERCASE filename — QML's own
type-file convention (only an uppercase-first name is a valid QML type)
marks these as helpers, never independently a slot. The build now copies
the WHOLE `widgets/` dir per song (not file-by-file), but still only
manifests top-level LOWERCASE-KEBAB `.qml` files as slots; `.gitkeep` is
always skipped. Manifest shape unchanged.

None of this loosens the fixed injected-prop rule above: every extra these
additions introduce (`shared`, the powermenu slot's `.item` handle,
`clipboard`, `ledger`) is a runtime QML object handle, same as
`notification` before it — never nix `config.*`.

**Additive (2026-08-17) — declared widget-type registry:** everything above
this paragraph is the **anchored** catalog — a slot name a host surface
already wired a `WidgetSlot`/`SurfaceSlot` for. `aoide.arrangement.widgets`
(`modules/nucleus/options.nix`, house rule 5's other half of the closed
`aoide.livery` + `aoide.arrangement` pair) is a second, independent
mechanism: it lets a song **register a brand-new slot** via nix, apart from
the fixed catalog above, instead of only dressing a name the facet already
anchored. `arrangement` carries the song's STRUCTURE (which widget/surface
TYPES it brings into existence) where `livery` carries its DRESS — different
questions, hence a separate option tree, but still the same two-namespace
whitelist rule 5 already closes off; a third namespace would need the same
explicit amendment.

An entry is `attrsOf widgetType`, keyed by slot name (the attribute name IS
the slot name), with one required field, `kind`:

- **`kind = "surface"`** — owns its own `PanelWindow`/layer-shell surface
  (powermenu/launcher-style overlays). Extra fields: `namespace` (nullOr
  str; derives `aoide-<slot>` when null, since a plain nix default can't see
  its own attribute key), `layer` (`overlay` | `top`, default `overlay`),
  `shortcut` (nullOr str — a `GlobalShortcut` name a venue's compositor
  config may bind; naming one is not binding it, keeping the song/venue
  split intact), `blur` (bool, default true).
- **`kind = "dock"`** — mounts as an `Item` into `AoidePanel`'s existing
  gadget column, alongside the shipped Conductor/Terminals/Meters/Power/
  Usage gadgets and `herald-center`. Extra field: `order` (nullOr int) — the
  only sort key among multiple declared `dock` entries, because the
  registry is serialized through `serde_json::Value`/`BTreeMap` (no
  `preserve_order` feature enabled), so a song's declared widget keys do
  NOT preserve authored order between the build-time nix walk and the
  native hot-sync. Consumers sort `(order ?? 0, slot-name)` ascending.

The two kinds' extra fields are mutually exclusive — a `namespace`/`layer`/
`shortcut`/`blur` on a `dock` entry, or an `order` on a `surface` entry, is
rejected. Nix option types can't easily express "field X only valid when
kind==Y", so this is enforced natively, not at the nix layer: `rice lint`
(`pkgs/aoide/crates/song/src/livery/schema.rs`) is the authoritative,
closed/strict validator — every field the wrong kind carries is a named
rejection, every unknown key inside an entry is rejected, same closed-set
discipline as the palette/base16/component tiers above. The song's own QML
body for a declared slot still lives at
`song/songbook/<name>/widgets/<slot>.qml`, same convention as any other
slot — declaring a slot's TYPE and authoring its body remain two separate
acts.

**Physical storage:** unchanged by the arrangement/livery option-tree split
— a song's declarations live in that song's `livery.json` under a flat
top-level `.widgets` key, sibling to `.palette`/`.base16`/`.bar`
(`livery.json`'s fields are flat-per-concern, never nested under a
`"livery"` key; precedent: `livery.json` already carries a top-level `song`
key with no `aoide.livery.song` option). No separate file: `livery.json` is
the one stage/draft-routed twin file (`rice mode draft` symlinks it), so
splitting a second file off would have to duplicate that same routing and
keep two files atomically consistent across every flip — a cost the option
tree split doesn't need to pay, since that split is a NIX NAMESPACE decision
about what facets may read, not a file-layout decision.

**`registry.json`:** a build-time artifact parallel to `manifest.json` but
serving a different purpose — NOT merged into it. `manifest.json` answers
"which slot **bodies** exist" (any `.qml` file a song drops under
`widgets/`); `registry.json` answers "which slots did a song **register as
a widget-TYPE declaration**" (a rarer, smaller set — most songs declare
none). The quickshell facet's build
(`modules/facets/quickshell/default.nix`) walks every committed song's
`livery.json` `.widgets // {}` into `$out/qml/songs/registry.json`, shaped
`{ "<song>": { "<slot>": {…declaration…} } }` — every committed song gets an
entry, `{}` when absent, never an error, never a skipped song. `rice
stage` hot-syncs one song's entry live, no rebuild
(`sync_song_registry`, `pkgs/aoide/crates/song/src/widgets.rs`), mirroring
`sync_song_widgets`'s existing `manifest.json` hot-sync. The compositor
facet (`modules/facets/compositor/default.nix`) reads `aoide.arrangement.widgets`
(the nix option, active song only) to generate one layerrule pair per
`kind = "surface"` entry — filtered to `surface` first, since a `dock` entry
has no layer surface of its own and must never generate a namespace/glass
rule.

**Runtime hosts:** two, one per kind, both reading
`stagingEngine.declaredWidgets(song)` (`StagingEngine.qml`, the
`registry.json` FileView) and both keeping the fixed injected-prop contract
(`livery` + `bridge` only — no `shared`, deliberately, so a declared widget
gets no wider surface than any other slot body):

- **`SongSurfaces.qml`** — non-visual host for `kind = "surface"` entries.
  An `Instantiator` of `SurfaceSlot`s (the same anchor a fixed window-owning
  slot like `powermenu` uses), one per declared entry, plus one
  `GlobalShortcut` per entry that names a non-null `shortcut`.
- **`SongGadgets.qml`** — visual host for `kind = "dock"` entries. A
  `Repeater` of `WidgetSlot`s (the same anchor `herald-center` uses),
  mounted as the last children of `AoidePanel`'s gadget column, sorted by
  `order`.

Both reuse the SAME `resolveSong` → `Qt.createComponent` →
`Component.createObject` mechanism and baseline-fallback chain the anchored
catalog above already defines — a declared slot is not a fourth rendering
mechanism, just a new way to name a slot that resolves through the existing
two primitives. A declared slot with no actual `widgets/<slot>.qml` body
(neither the active song nor sonata) resolves through the same
`resolvedSong === ""` signal the anchored catalog already computes and warns
(`[aoide/surfaceslot]` / `[aoide/songgadgets]`) instead of throwing —
declaring a TYPE with no body is inert, not an error.

`song/songbook/etude/` is the worked, real example: its `rice.nix` declares
one `kind = "surface"` entry (`demo`), proving the whole pipeline —
nix option → build-time `registry.json` walk → `rice lint` → `rice
stage` hot-sync → `SongSurfaces.qml` render — end to end. No
committed song declares a `kind = "dock"` entry yet.

### Elements

A song may also carry non-QML rice targets — waybar, dunst, a compositor,
anything with a config file — the same EDIT/SAVE/DRAFT loop the QML
widgets above already get. The design authority is
`docs/architecture/ELEMENTS.md` (task #121); this subsection pins the
descriptor's on-disk contract and `run/elements/`'s runtime shape.
`crates/song/src/elements.rs` is the one parser/render pipeline every
consumer (`element seed`, and later `rice stage`/the elements facet) calls
through — never a second implementation.

**The descriptor:** one `song/songbook/<song>/elements/<element>/element.json`
per element, v0:

```json
{
  "v": 0,
  "element": "waybar",
  "files": [
    { "src": "config.jsonc", "dest": "config", "template": false },
    { "src": "style.css", "template": true }
  ],
  "surfaces": ["bar"],
  "run": { "exec": "waybar -c {run}/config -s {run}/style.css", "via": "unit" },
  "reload": "pkill -SIGUSR2 -x waybar"
}
```

Field rules, all enforced natively (`elements::parse_descriptor`), taught
on refusal:

- `v` — descriptor version. `0` is the only version this parser knows;
  any other value refuses.
- `element` — must match `^[a-z0-9][a-z0-9-]*$` (the same shape
  `compose::valid_song_name` enforces on song names — one function, not a
  second regex) AND must equal the directory it was read from; a mismatch
  refuses.
- `files` — the ordered manifest, at least one entry. `src` is relative to
  the element dir; `dest` is relative to `run/elements/<element>/` and
  defaults to `src`. Both are validated against traversal: every path
  component must be an ordinary segment — no `..`, no leading `/`, no bare
  `.` — or the descriptor refuses. `template` defaults to `false`
  (verbatim byte copy, no UTF-8 requirement); `true` renders the file
  through `livery::emit::file::render` (`{{group.key}}`, group ∈
  palette|base16|bar|notif|window) against the resolved livery — an
  unknown or malformed placeholder is a structured error, never a silent
  no-op.
- `surfaces` — optional list of surface names the element claims; folded
  into `aoide.surfaces` with `owner = "<element>"` once the elements facet
  (L-E3) lands. Not consumed by the cargo pipeline itself.
- `run.exec` — the full command line; the literal token `{run}` is
  substituted with the absolute `run/elements/<element>` path at
  generation time (`elements::substitute_run_token`) — never re-expanded
  at runtime.
- `run.via` — exactly `"unit"` or `"exec-once"`; anything else refuses.
- `reload` / `restart` — optional command overrides for the facet's
  restart derivation (L-E2/L-E3); not read by the cargo render pipeline.

**Render pipeline:** for each element, every declared file is rendered
into memory FIRST — verbatim copy or template render — and only written
(atomically, `aoide_storage::fs::atomic_write_bytes`) once every file for
that element rendered without error. A render error therefore fails only
that one element and leaves its existing `run/elements/<element>/`
completely untouched, never partially overwritten; every other element in
the same call still renders. `_`-prefixed element directories are skipped
(the same shelving convention `_widgets/` gets everywhere else).

**`run/elements/`:** `$AOIDE_ROOT/run/elements/<element>/`
(`aoide_storage::fs::run_elements_dir`) — a sibling of `run/qml`, resolving
off the exact same runtime root and the same `$AOIDE_STAGE_DIR` override.
The ONLY place a running element reads its config from; nothing under
`~/.config` is ever managed or symlinked. `element seed <song>` (lyra-only,
`crates/lyra`) is the shell-reachable bridge that renders one song's whole
`elements/` tree into it, against that song's own committed `livery.json`.

**Landed vs. designed (L-E1 only):** the descriptor parser, the render
pipeline, `run_elements_dir()`, and `element seed` are implemented and
tested. `rice stage` does not yet render elements as part of the stage
loop, and no nix facet yet reads `elements/` at eval or generates a
systemd unit / exec-once line / `aoide.surfaces` claim — those are L-E2
and L-E3, later phases in the same lane, not yet built.

---

## 6. A2A door — **v0**

A2A (Agent2Agent, Linux Foundation) is the third door onto aoide, alongside
the CLI and MCP façade — it exposes an aoide session as a **discoverable
remote agent** other A2A-speaking agents can address, and (client side) lets
aoide fold an *external* A2A agent into its own session DAG. A2A is the
**successor to** ACP (Agent Communication Protocol, BeeAI/IBM) — ACP archived
Aug 2025 and folded into A2A, so aoide builds against A2A directly rather than
a retired spec.

**Version.** This section targets A2A over the **JSON-RPC 2.0 / HTTP binding**;
the method and state names below use that binding's transport spelling
(`message/send`, `tasks/get`, lowercase-kebab states like `input-required`).
Phase B's server implements the **v0.3.x JSON-RPC binding** explicitly: the
AgentCard advertises `"protocolVersion": "0.3.0"`, keeps the flat `"url"`
shape (valid and consumable under 0.3.x), and serves `message/send` /
`tasks/get` with lowercase-kebab states. A2A's current release is **v1.0.0**,
whose gRPC/proto binding spells the same surface differently (`SendMessage`,
`GetTask`, `TASK_STATE_*`) and whose JSON-RPC/HTTP AgentCard form moved to a
top-level `interfaces` array plus a top-level `id`; that v1.0 card shape is an
**additive follow-on** — a later phase can advertise it alongside (or instead
of) the 0.3.x form without a contract break here.

### The mapping (aoide's vocabulary already has an A2A shape)

| A2A concept | aoide equivalent |
| --- | --- |
| AgentCard @ `/.well-known/agent-card.json` | discovery derived from the command registry (`aoide schema --json`) — one schema, same as the MCP tool list |
| Task (one unit of work) | a *turn* — what `send` injects into a session |
| `contextId` (conversation) | a `SessionRecord` (the long-lived session) |
| TaskState `WORKING` | canonical_state `working` |
| TaskState `COMPLETED` | canonical_state `stopped` (the *turn* ended; the session/context lives on) — **also** produced by canonical_state `done` (MVP simplification: both collapse onto `completed` today, even though `done` conceptually ends the whole context, not just a turn — see below) |
| TaskState `INPUT_REQUIRED` | canonical_state `awaiting` |
| TaskState `AUTH_REQUIRED` | the `needsSudo` signal (`SessionRecord.needs_sudo`) |
| TaskState `SUBMITTED` | canonical_state `idle` (aoide's at-rest/cold state — acknowledged but not actively processing; this is the honest A2A state for it, NOT `WORKING`) |
| `message/stream` (initial SSE) / `tasks/resubscribe` (reconnect) | the hooks + transcript tail already driving `say`/`activity`/`model` |
| transport (JSON-RPC 2.0/HTTP + SSE) | a new `aoide a2a serve` door |

Because a **Task is a turn** and a **`contextId` is a session**, a `COMPLETED`
Task maps to `stopped` (the turn ended, the agent is back at the prompt), NOT
`done` — aoide's `done` means the *session/context* ended, which terminates
**every** Task under that `contextId` (the five-state vocabulary lives at
`graph/model.rs::canonical_state`, not §4).

`AUTH_REQUIRED` is a **narrowing**: A2A's auth-required covers any
client-supplied credential, of which `needsSudo` is aoide's only instance
today. `needsSudo` is a signal carried *alongside* `state`, not a state — so a
session that is both `awaiting` **and** `needsSudo` surfaces as `AUTH_REQUIRED`
(**auth-required takes precedence over `INPUT_REQUIRED`**).

`TaskState`'s `FAILED`, `CANCELED`, `REJECTED` (and 0.3.x's `unknown`) have no
canonical_state counterpart yet — aoide's five-state vocabulary has no failure
or cancellation notion and predates this mapping; a later phase either
extends the canonical vocabulary or folds them at the edge. Not resolved in
v0. (`SUBMITTED` is no longer in this unresolved set — canonical_state `idle`
now produces it, per the table above.)

### Transport and MVP surface

Transport is JSON-RPC 2.0 over HTTP, same as upstream A2A; streaming rides
Server-Sent Events. The MVP door serves exactly:

- **AgentCard** at `/.well-known/agent-card.json` — **derived from** `aoide
  schema --json` (the same one-schema discipline the MCP tool list follows,
  concepts/Agent-Interface — no second command inventory to drift), **plus**
  static card metadata the schema does not carry (card `url`, `version`,
  `capabilities`, input/output modes). Only **implemented** commands become
  advertised skills: Phase B adds an additive `implemented` boolean to §3's
  `schema --json` command entries and the card filters on it, so stub commands
  are never advertised as live skills.
- **`message/send`** — invoke; returns a Task. **Phase B2** landed real
  execution — see "Execution: `message/send`" below.
- **`tasks/get`** — poll a Task's status by id.
- **`message/stream`** / **`tasks/resubscribe`** — **Phase C** landed
  Server-Sent-Events streaming; see "Streaming: SSE (Phase C)" below.

`tasks/cancel` remains an **additive** follow-on — v0 does not require it, and
adding it later is not a version bump to this contract (same additive
discipline as §1/§4's optional tiers).

### Streaming: SSE (Phase C)

`message/stream` and `tasks/resubscribe` are served over **Server-Sent
Events** (`a2a.rs::stream_task`). The response is a single long-lived
`text/event-stream` (`Cache-Control: no-cache`, `Connection: close`), and each
frame is a `data: <json>\n\n` line whose JSON is a JSON-RPC result envelope
carrying a task-status update. The server emits **on change**: the first
observation, then only when `status.state` changes. The **terminal** event is
marked `final: true` and shaped as A2A's `TaskStatusUpdateEvent` (`{ taskId,
contextId, status, final:true, kind:"status-update" }`); the client stops on
it. A resolution error (a failed send, or an unknown `tasks/resubscribe` id →
`-32001`) is delivered as a single `data:` event carrying the JSON-RPC error,
then the stream closes.

- **`message/stream`** *sends then streams*: it FIRST runs the same
  inject/spawn `decide_send_action` execution as `message/send`, then streams
  the resulting task's status to completion.
- **`tasks/resubscribe`** *streams an existing task* named by `params.id`.

The stream polls the stage every ~750 ms and is **bounded to 10 minutes**
(`MAX_STREAM`): a never-terminal session (an idle agent) emits a final event
and closes at the cap rather than holding a handler thread — and thus a
`MAX_CONN` connection slot — forever. Concurrent streams are already bounded by
the `MAX_CONN`/`ConnGuard` cap, since a stream runs inside the same guarded
handler thread as any other connection. A client disconnect (write failure) is
detected best-effort and closes the stream. The AgentCard advertises
`capabilities.streaming: true` accordingly.

### Execution: `message/send` (Phase B2)

`message/send` does **both** inject and spawn, decided purely from the
request (`a2a.rs::decide_send_action`, unit-tested for every branch):

- **Inject** into an existing session when the request's `contextId` names a
  KNOWN, conductable(+socketed) session (`SessionRecord.conductable` +
  `.socket`) — and the client did not explicitly ask to spawn. Delivery
  reuses [`crate::graph::session_send`] (the same gated door `send`
  uses), not a reimplementation of the socket write.
- **Spawn** a NEW conducted agent when there is no `contextId`, OR the
  client explicitly asks to spawn via `metadata["aoide/spawn"] == true`
  (checked on the `message` object first, then top-level `params` —
  **this key is aoide-specific**, not part of upstream A2A). The prompt is
  injected as the spawned session's first turn, best-effort, once its control
  socket appears.
- A `contextId` naming a known but NOT conductable session, or naming
  nothing, is a structured JSON-RPC error (`-32004`/`-32001` respectively) —
  never a silent fallback to spawning.

**The command a spawn runs is `aoide.a2a.spawnAgent`** — a nix option, off
(`""`) by default, resolved once at `a2a serve` launch (`--spawn-agent` flag →
`AOIDE_A2A_SPAWN_AGENT` env, set by the `aoide-a2a` systemd unit → the
option's default). **The A2A client supplies the message/prompt only, never
the command** — this is the load-bearing invariant that bounds what an
external A2A caller can do to aoide: task the operator's own
already-configured agent, or steer a session already running under aoide's
conductor, but never execute an arbitrary binary. If `spawnAgent` is empty,
the spawn path returns `{"code": -32004, "message": "A2A spawn not
configured"}` rather than silently doing nothing.

**Security model.** The capability is admitted **at rebuild time**, not
per-request: setting `aoide.a2a.spawnAgent` to a non-empty command is the
user's admission (house policy — "the rebuild is user-gated"), same as any
other nix option. There is deliberately **no interactive per-request gate**
like `send`'s pending/`--yes`/autogate dance — a JSON-RPC request
cannot block mid-flight on a human clicking "approve". In its place: the
spawn target is fixed at rebuild time (never client-chosen), the door is
loopback/user-scoped by default (same as the rest of §6's security posture),
and every inject/spawn/error is audited through `Door::A2a`, the same single
audit log every other door writes.

**Spawn acks only once the wrapper proves it's alive (task #103).**
`do_spawn` launches the configured agent via a detached `aoide conduct`
wrapper process; a successful `cmd.spawn()` there only proves that WRAPPER
started — it says nothing about whether the wrapper's own exec of the
configured agent succeeded, since a missing `spawnAgent` binary fails
inside that separate process (synchronously, from `aoide-conduct`'s own
"spawn FIRST" ordering — no session ever registers for that failure), not
visibly to this door. Before acking, `do_spawn` gives the wrapper a short
bounded window (400ms, `Child::try_wait()` polled) to prove it's still
running; a wrapper that exits inside that window gets a taught JSON-RPC
error (`-32603`, naming the configured program, never the full command
line or any env) instead of a `submitted` Task naming a session that will
never appear in `sessions.json`. Every legitimate spawn pays this as fixed
RPC latency and never notices it.

**MVP simplification, carried over from Phase B:** taskId == contextId ==
sessionId for both inject and spawn (a fresh spawn's Task/contextId/sessionId
are all the newly-minted `a2a-<pid>-<ts>` id). Splitting a Task from its
session for real multi-turn tracking (so a session with several in-flight or
completed turns exposes each as its own addressable Task) remains future
work, same as the `TaskState` gaps noted above.

### Security posture

The A2A door is **off by default** (house policy), identical to the MCP
façade — `aoide.a2a.enable` defaults to `false`. When enabled it binds
**loopback/user-scoped** (`aoide.a2a.bindAddress` defaults to `127.0.0.1`);
exposing it to the network is a deliberate, explicit per-host choice, never
the default.

**Security invariant: never expose this door via `ssh -R` (or any other
tunnel/proxy) terminating on loopback.** `classify_origin` (below) resolves
solely from the TCP connection's own `peer_addr()` — it has no way to see
past a loopback-terminated forward to whoever is really calling on the far
end, so every such caller is misclassified as `PeerOrigin::Loopback` and
inherits that origin's unconditional auto-delivery, silently bypassing the
non-loopback pending gate this whole amendment exists to enforce. A same-box
caller that is genuinely non-local (another process, another agent's
harness reaching in over the network rather than through this box's own
loopback) is bound correctly only by giving the door a real LAN or tailnet
IP (`aoide.a2a.bindAddress`) — never by tunneling a remote caller back onto
`127.0.0.1`. (A caller that dials THROUGH an ssh forward this instance
itself opens outbound — `peer.via`/`--via`, §7 below — is the opposite
direction and unaffected: the far END of that tunnel is this instance's own
peer client, not an inbound caller trying to look local.)

A forwarded A2A message — whether inbound (someone else's agent calling
aoide's door) or outbound (aoide relaying to a registered external agent) —
is **untrusted data** crossing aoided's boundary, exactly like an MCP call:
it is never executed, only routed through the same dispatcher, gate, and
audit log every other door uses. The A2A door adds no new trust tier.

**Amendment (2026-08-14): non-loopback `message/send` is gated, not
auto-delivered.** Landed alongside §7 (peer federation) as the one
must-fix precondition for that feature: the moment binding the A2A door to a
real network address becomes something people actually do (§7's whole
point), the INJECT path's previous unconditional auto-delivery becomes an
open pipe — any reachable caller could inject text into any local
conductable session with zero approval. Fixed in `a2a.rs::do_inject` /
`message_send`:

- The connection's ORIGIN (`TcpStream::peer_addr()` — not any
  client-supplied field, so it can't be spoofed) is classified as
  **loopback**, a resolved **remote** address, or **unknown** (`peer_addr()`
  failed — fails SAFE, treated as remote/unmatched).
- **Loopback is UNCHANGED**: still auto-delivers exactly as before this
  amendment (hard regression requirement — this fix touches ONLY the
  non-loopback path).
- A **remote** origin auto-delivers ONLY when it matches a peer explicitly
  marked `"autogate": true` in `state/peers.json` (§7 below) — the
  cross-device analogue of `send`'s local "sender is the target's own
  parent" autogate rule. An unmarked/unknown remote sender is held
  **pending**, reusing `send`'s EXISTING `pending.json` queue
  machinery verbatim (`conduct::graph::send::session_send`'s own gate — no
  second pending-queue implementation). The synchronous JSON-RPC response
  reports the Task as `submitted` (A2A can't block a request on a human's
  approval); `tasks/get`/the SSE stream reflect the session's real state
  once/if a human approves it and it delivers.
- Every outcome (queued / auto-delivered-via-autogate / normal
  loopback-delivered / error) audits through the SAME `Door::A2a` → single
  audit log path every other door outcome already uses — no second logging
  path.
- Spawn's admission model is **unchanged** by this amendment — it was
  already rebuild-time-only (`aoide.a2a.spawnAgent`, never client-chosen);
  the gap being closed here was specific to Inject's unconditional `--yes`.

**Amendment (2026-08-19): bearer-token authentication — Spawn is gated for
the first time, and loopback stops being an unconditional trust signal.**
The 2026-08-14 amendment above assumed `PeerOrigin::Loopback` (TCP
`peer_addr()` resolving to `127.0.0.0/8`/`::1`) means "the operator, on this
machine." Behind any reverse proxy or tunnel (`ssh -R`, a tailscale funnel,
cloudflared, nginx) that assumption is false: the SERVER's end of the
connection sees the proxy's OWN loopback address for every caller, so a
remote attacker who can reach the proxy inherits loopback's automatic trust.
Worse, Spawn (`SendAction::Spawn` → `do_spawn`) was **origin-blind
entirely** — `message/send`'s doc comment said so outright ("`origin`...
only ever affects the Inject branch") — so ANY caller that reached the port,
proxied or not, could already launch the operator's configured
`aoide.a2a.spawnAgent` with an attacker-chosen prompt, with zero gate beyond
that command being non-empty. Fixed in `a2a.rs`:

- The server's own expected token comes from either of two sources.
  `aoide.a2a.tokenFile` (nix option; `--token-file` flag /
  `AOIDE_A2A_TOKEN_FILE` env at the `a2a serve` layer, mirroring
  `resolve_spawn_agent`'s exact flag→env→default precedence) names a file
  holding it, read ONCE at launch (`resolve_token_file`/
  `read_expected_token`). `aoide.a2a.bearerSecret` (`--bearer-secret` flag /
  `AOIDE_A2A_BEARER_SECRET` env, the same precedence shape) instead names a
  SECRET, resolved through the local secrets broker's unix-socket wire
  (this section's own "Secrets wire" subsection —
  `aoide_secrets::client::resolve_bounded`, self-asserted consumer
  `a2a-door`) — and takes precedence over `tokenFile` when set. Unlike the
  file, it is resolved FRESH on every connection, never once and cached
  (`a2a.rs::resolve_inbound_bearer`): revoking the underlying secret
  (`secrets rm`, a policy edit) takes effect on the very next request, no
  daemon restart, the one deliberate difference between the two
  mechanisms. A broker resolve failure — unreachable, denied, or a bounded
  ~2s socket-read timeout (the wire's own `wait:false` keeps a
  misconfigured `requireTotp` secret from ever parking this door's
  connection the way a human-facing `secrets exec` might tolerate) — FAILS
  CLOSED: every bearer check on that connection denies, the identical
  `-32005`/stripped-card shape a wrong token gets, never a silent fallback
  to the file mechanism or to the pre-token open behavior. Neither source
  configured (the default) is **the off-path**: every decision below
  becomes a no-op and behavior is byte-identical to before this amendment
  — pinned by
  `effective_origin_is_the_identity_function_when_no_token_is_configured`
  and `token_authorized_always_allows_when_no_token_is_configured`
  (`a2a.rs` tests), on top of the existing unmodified
  `should_deliver_now_covers_every_origin_autogate_combination` regression
  pin from the 2026-08-14 amendment.
- The caller presents the token as `Authorization: Bearer <token>`
  (`parse_http_request` now captures it; `extract_bearer` parses the
  scheme). Compared against the expected token with a length-independent
  byte loop (`aoide_storage::peer_store::token_bytes_eq`), not `==`, so a
  secret comparison doesn't take the crudest form of a timing shortcut — no
  new crate for genuine constant-time comparison, per house "zero new deps"
  discipline.
- **Spawn is gated for the first time**: `spawn_authorized(token_configured,
  token_state)` must return `true` before `do_spawn` runs. With no token
  configured this is always `true` (Spawn's admission stays rebuild-time-only,
  unchanged). With one configured, an absent or wrong bearer is a clean
  `-32005` JSON-RPC error, never a silent fallback to the old open behavior.
- **Loopback's trust is coupled to the same switch, with no opt-out**: once
  `tokenFile` is non-empty, `effective_origin` coerces any origin that did
  NOT present the valid token to `PeerOrigin::Unknown` (reusing that
  variant's existing "never trusted" arm in `should_deliver_now` rather than
  adding a fourth origin kind) before `should_deliver_now` ever sees it.
  There is deliberately no separate `trustLoopback` boolean — a single
  switch that cannot be mis-configured into "token required, but a
  proxied/tunneled caller that looks loopback still auto-delivers," which is
  exactly the hole this amendment closes. A caller that DOES present the
  valid token keeps loopback's original standing exactly.
- **Per-peer identification also moved off address**: `Peer.tokenFile`
  (`state/peers.json`, set via `peer add --token-file <path>`) is a SEPARATE
  per-peer secret from the server-wide `tokenFile` above — it resurrects the
  `autogate` flag's original intent (§7) by letting a token, not an
  IP, say WHICH registered peer is calling. `aoide_storage::peer_store::
  is_autogated_peer_token` folds this the same way `is_autogated_peer_addr`
  already did; Inject's `autogate_match` is now the OR of both checks, so an
  operator who never sets a peer's `tokenFile` sees the original
  address-only match, unchanged. This is a per-peer credential, not one
  shared secret — a shared token can't tell two peers apart and would need
  its own global knob; the server-wide `tokenFile` above answers a different
  question ("is this caller authenticated as the operator/self at all,"
  which Spawn and the loopback coupling need) and works independently of
  whether any peer has a `tokenFile` set.
- Every outcome (Spawn's new `-32005` rejection included) still audits
  through the SAME `Door::A2a` log every other §6 outcome already uses — no
  second logging path.

**Amendment (2026-08-20, Phase G): the READ commands are token-gated by the same
switch.** The 2026-08-19 amendment gated Spawn and the Inject-delivery origin,
but left the read commands — `tasks/get`, `aoide/graphSummary`, and the SSE pair
(`message/stream`, `tasks/resubscribe`) — open regardless of the token. On
loopback that is harmless, but the moment the door faces a network,
`aoide/graphSummary` hands any caller the operator's WHOLE resolved session
graph, and `tasks/get`/`tasks/resubscribe` leak any session's live state.
Closed by the same predicate that gates Spawn, now named `token_authorized`
(the old `spawn_authorized`; one predicate, since the question is identical —
does the caller hold a valid token when one is required):

- `handle_jsonrpc` computes the gate once and short-circuits `tasks/get` and
  `aoide/graphSummary` to `-32005` (the shared `unauthorized()` value) when a
  token is configured and no valid bearer is presented — *before* the read
  runs, so a real session id still returns the error, never its state.
- `stream_task` gates BOTH SSE reads at the top, before `message_send` runs,
  so an unauthenticated `message/stream` neither injects nor spawns; the
  `-32005` arrives as the stream's single SSE error event.
- Off-path (no token, today's default) is byte-identical to before — pinned
  by `read_commands_stay_open_when_no_token_is_configured`; the gate itself by
  `read_commands_are_token_gated_when_a_token_is_configured`. `message/send` is
  unchanged (it still runs its own `classify_token` internally for
  `effective_origin`, so it is not re-gated in the dispatcher).

**Amendment (2026-08-20): the unauthenticated AgentCard GET is stripped, not
gated.** Phase G above closed the read commands but left
`GET /.well-known/agent-card.json` untouched — it doesn't fit that
predicate's `-32005` JSON-RPC shape at all: a GET is answered with a card,
never a JSON-RPC envelope, so there is no error code to return, only a
choice of WHICH card. Fixed in `a2a.rs`:

- `stripped_card(full)` picks exactly three fields — `name`,
  `protocolVersion`, `url` — off the FULL card `Value` that
  `agent_card_from_commands` already builds, rather than re-deriving them, so
  the stripped shape can never drift from the real card's own field names.
- `route`'s card arm reuses the SAME `token_authorized`/`classify_token`
  machinery every other gate in this file uses — no second predicate: when a
  token is configured and the presented bearer does not classify `Valid`,
  the response is `stripped_card(&full)`; otherwise it is the full card. The
  HTTP status stays `200` and the audit label stays `a2a.agent-card` in both
  arms.
- What the stripped card WITHHOLDS from an unauthenticated caller: the
  skills inventory (the full command surface), `version`, `capabilities`,
  `description`, `defaultInputModes`, and `defaultOutputModes` — everything
  that is not one of the three served fields. All of it requires a valid
  bearer once a token is configured, same as the Phase G read commands.
- Off-path (no token, today's default) is byte-identical to before — the
  served card is pinned field-for-field against `agent_card_from_commands`
  directly, the same off-path pin style Phase G used.
- Known accepted consequence: `aoide peer add` against a token-protected
  remote only ever verifies the fetched card is well-formed JSON (it never
  parses individual fields), so a stripped card still satisfies the
  verification-before-registering check — the client presents no
  `Authorization` bearer when fetching a peer's card (outbound clients send
  none at all, per the 2026-08-19 amendment's grounding), so it only ever
  sees the stripped shape on a protected peer. Registration still succeeds.

**Amendment (2026-08-20, #50): an unauthenticated `message/send` naming a
context answers UNIFORMLY, not with a hard gate.** Phase G above closed the
read commands and Spawn, but left Inject's `contextId` lookup itself open to two
problems even with a token configured: `decide_send_action` ran
`session_ref_lookup` regardless of authentication, so an unauthenticated
caller could tell a real `contextId` from a bogus one apart by the response
shape alone (`-32001 task not found` vs a `submitted`/injected Task — an
**existence oracle** over every local session id), and a REAL id reached
`do_inject`, which could still **write `pending.json`** with zero credential
presented at all. A hard `-32005` here, mirroring Spawn, would be the WRONG
fix: enrolled peers authenticate this call via their OWN per-peer token
(`Peer.tokenFile` / `is_autogated_peer_token`, 2026-08-19 amendment above),
never the server-wide one, and aoide's own outbound clients send no bearer
by default (`commands.rs`/`wire.rs` — a per-peer `Peer.bearerSecret`, set via
`peer add --bearer-secret <name>` and resolved fresh through the secrets
broker at request time, is the opt-in exception; unconfigured stays the
default, no-bearer behavior) — a hard gate would refuse correctly-enrolled
peers, not just attackers. Fixed in `a2a.rs::message_send`:

- Before `decide_send_action` runs, when a token is configured, the
  presented bearer does not classify `Valid`, AND neither autogate signal
  matches (`ip_autogate` nor the per-peer `token_autogate` — the same OR the
  Inject arm already computed, now hoisted above the decision so it's
  available before AND after it, at the cost of one `load_peers()` per call
  instead of a conditional one), a **context-id send**
  (`context_id.is_some() && !spawn_asked` — the exact negation of
  `decide_send_action`'s own spawn-vs-lookup split, so the two functions can
  never classify the same request differently) short-circuits to a synthetic
  `submitted` Task keyed on the PRESENTED `contextId`, built by the same
  `submitted_task` helper `do_inject`'s held-pending arm uses (one Task
  shape, not two that could drift). `session_ref_lookup` never runs,
  `do_inject` never runs, `pending.json` is never touched.
- Real, bogus, and known-but-not-conductable ids are now byte-identical from
  the outside under this arm — no oracle. The queue-write half of the bug is
  closed the same stroke: an unauthenticated remote can no longer feed the
  operator's approval queue at all once a token is set (before this
  amendment, an unauthenticated "loopback-looking" or unmatched-remote
  caller naming a real conductable session got queued into `pending.json`
  same as an authenticated one would).
- The Spawn arm is untouched and keeps its existing `-32005` — this is
  deliberately the UNIFORM-RESPONSE arm, not a second hard gate; it only
  ever intercepts a context-id send, which Spawn never is.
- A caller who DOES present the valid server-wide token, OR whose token/IP
  matches an autogate-marked peer, is unaffected — falls through to the
  unchanged `decide_send_action` → Inject/Error path exactly as every
  amendment above already described. Note the pre-existing (2026-08-19)
  consequence for the per-peer-token case specifically: with a server-wide
  token configured and no valid bearer, `effective_origin` still coerces
  the caller to Unknown, so the autogate exemption reaches the REAL inject
  machinery but lands held-pending in the approval queue — it does not
  instant-deliver. Fail-safe, and distinct from the guard's synthetic arm,
  which never queues at all.
- Off-path (no token configured, today's default) is byte-identical to
  before — the guard's `token_configured` check makes it a no-op by
  construction, the same off-path-pin discipline as the amendments above.

**Amendment (2026-08-25, P-P3): the Spawn arm's gate flips from "holds a
valid door-wide bearer" to "resolves, via its OWN token, to a PAIRED peer
whose `allows` contains `spawn`."** Every amendment above this one gated
Spawn on `spawn_authorized`/`token_authorized` alone — any caller holding
the server-wide bearer (`aoide.a2a.tokenFile`/`bearerSecret`) could launch
`aoide.a2a.spawnAgent`, whether or not it corresponded to a peer the
operator had ever actually paired with (`peer pair request`/`approve`,
P-P2, decisions above). `docs/architecture/PAIRING.md` decisions 5–7 close
that gap: `allows` (a closed capability set, `aoide_storage::peer_store::
PEER_CAPABILITIES` = `"read"`/`"spawn"`, never a per-capability serde bool
scatter) lives on `Peer`, stamped by `upsert_paired_peer` from the grant its
caller resolved (`config.toml`'s `[pairing] defaultGrant`, or `--allow`) the
moment a peer FIRST becomes verified and left untouched on a later key
rotation (so a revoked capability survives re-pairing); `peer allow <name>
<cap> on|off` (§3 above, +1 → 77) is the only other writer, idempotent,
refusing an unknown peer or unknown capability. Fixed in `a2a.rs::
message_send`:

- `resolve_peer(peers, addr, presented_token)` (`aoide_storage::peer_store`)
  is the caller-identity ladder — a presented bearer matched against ANY
  registered peer's own `tokenFile` first, the connection's origin address
  matched against a peer's `url` second — unlike `is_autogated_peer_token`/
  `is_autogated_peer_addr` above, it checks EVERY registered peer, not only
  ones marked `autogate`, since "which peer is this" is a different
  question from "should this peer skip the pending queue." It returns
  WHICH rung matched alongside the peer (`PeerRung::Token` /
  `PeerRung::Addr`) — the two are not interchangeable strength: `Token` is
  possession of that peer's own `tokenFile` secret, `Addr` is a bare
  TCP-source-IP-vs-`url` match, spoofable by anyone who can reach the door
  from that address or who merely sits behind the same NAT/reverse-proxy as
  the real peer.
- `SendAction::Spawn` requires `spawn_admitted`, which accepts ONLY a
  `PeerRung::Token` resolution to a peer that is both `verified` and
  carries `spawn` in `allows` — `PeerRung::Addr` never reaches `do_spawn`,
  regardless of `allows`. The `Addr` rung remains fully valid for
  Inject's attribution and for the ordinary autogate question below; it is
  excluded from Spawn specifically, because a bare source-address match
  carries no possession proof, and behind any NAT/reverse-proxy deployment
  would otherwise let a shared source address spawn a process attributed
  to whichever peer's `url` it happens to match — a straight line from
  "message delivered a little faster" (the address rung's original,
  legitimate purpose) to "spawn a session as someone else." A caller that
  resolves to no peer at all, an unpaired peer, a paired peer whose
  `allows` lacks `spawn`, or a peer resolved only via the address rung all
  refuse with the SAME taught error (`-32006`, distinct from `-32005`'s
  "no/bad token"): *"spawn refused: spawn requires the caller be
  identified via its own `token_file` (an address match alone never admits
  spawn) — pair first via `peer pair`, set `peer add --token-file
  <path>` if not already configured, then `peer allow <name> spawn on`."*
  The door-wide bearer alone no longer reaches the spawn arm at all — it
  is necessary (Phase G above still gates Spawn's entry point when a token
  is configured) but no longer sufficient.
- **HONESTY NOTE (superseded by the Amendment (2026-08-25, P-P4) below) —
  at the time this P-P3 amendment landed, signed per-request wire
  authentication had NOT yet landed.** Even narrowed to the token rung, `resolve_peer`'s match rides
  the SAME unforged-but-unsigned signal every earlier amendment in this
  section already used for the unrelated autogate question — a bearer
  string compared byte-for-byte against a file on disk. It is not
  cryptographically bound to the caller identity it resolves to: a leaked
  `tokenFile` value resolves to that peer exactly as successfully as the
  real one would, and identically across every request either sends. This
  phase does NOT invent an interim signature or per-request token scheme
  to close that gap — doing so would be exactly the kind of hand-rolled
  crypto the kill-list in `docs/architecture/PAIRING.md` forbids outside
  the ceremony's own `ed25519-dalek` use. The gate is real and closes both
  the "any door-wide bearer holder can spawn" hole AND the "a bare address
  match can spawn" hole; it does not yet make spawn caller-identity
  unforgeable end-to-end — that is P-P4's job.
- Inject's own gate (`should_deliver_now`/autogate/pending-queue) is
  UNTOUCHED by this amendment — only the Spawn arm's admission changed.
  Inject instead gains attribution: a held-pending send from a
  `resolve_peer`-resolved (either rung, non-autogated, non-deliver-now)
  sender carries `"from": "peer:<name>"` in its `pending.json` entry,
  reusing `send --from`'s existing attribution field verbatim rather
  than inventing a new one — scoped to the QUEUED path only
  (`!deliver_now`), never applied to an auto-delivered message, so no
  delivered payload's bytes change (`autogated_peer_delivers_despite_
  being_non_loopback`'s exact-bytes pin stays green).
- A spawned session's own record carries the resolved peer's name too:
  `a2a.rs::do_spawn` sets `AOIDE_SESSION_ORIGIN=peer:<name>` on the child
  process it launches; `aoide-conduct`'s `session_conduct` reads that env
  var right after registration and stamps `SessionRecord.origin`
  (`stamp_origin`, mirroring `stamp_headless`'s change-once discipline) —
  see §7's "Peer record" subsection and `state/session-ledger.jsonl`'s own
  entry above for the full `origin` field shape and its projection into
  the durable ledger at session exit.
- Every outcome (the new `-32006` refusal included) still audits through
  the SAME `Door::A2a` log every other §6 outcome already uses — no second
  logging path.

**Amendment (2026-08-25, P-P4): per-request signed wire authentication for
paired peers — closes the honesty note above.** `docs/architecture/
PAIRING.md`'s "Wire authentication (paired peers)" section: a verified
peer's outbound A2A POSTs are now bound, per request, to a detached ed25519
signature over that exact request's method/path/timestamp/nonce/body —
unforgeable and non-replayable, unlike the token rung's bare shared-secret
comparison. Landed in `aoide_storage::wire_auth` (the shared canonical-
string + sign/verify logic — neither `aoide-client` nor `aoide-server`
touches `ed25519_dalek` directly), `aoide-client::commands::
sign_headers_for_peer` (the signer), and `aoide-server::a2a::
verify_signed_request` (the verifier).

**Headers** — four new ones on any peer POST to the existing `/` A2A door,
present together or not at all:

| Header | Carries |
| --- | --- |
| `X-Aoide-Peer` | The signer's claimed SELF name (`aoide_storage::display::local_host_name()`) — `peer_store::valid_peer_name`-shaped, display/attribution ONLY (#63 P-ID5). Identity is never resolved from it: the caller is the peer record whose stored `pubkey` verifies the signature, and a claimed-vs-resolved mismatch is audited as attribution drift with the resolved name winning everywhere downstream. Its one remaining role beyond attribution is the exact-name tiebreak among multiple verified records that share the verifying pubkey (see "Inbound verification" below). |
| `X-Aoide-Timestamp` | ISO-8601 UTC, the moment the signer minted this request. |
| `X-Aoide-Nonce` | A fresh random hex value per request (`aoide_storage::pairing::random_hex(16)`, the same mint the pairing ceremony already uses). |
| `X-Aoide-Signature` | The ed25519 signature over the canonical string below, hex-encoded (128 hex chars). |

A request presenting SOME but not all four headers is a malformed signed
request — refused outright (`-32007`), never silently downgraded to the
unsigned/token/addr ladder.

**Canonical string** (`aoide_storage::wire_auth::canonical_string`) — the
exact bytes the signature covers, following P-P2's `derive_sas`/
`derive_commit` field-hashing style verbatim (no reason found to diverge):
five fields — `method`, `path`, `timestamp`, `nonce`, and the request
body's SHA-256 digest (hex) — each trimmed and lowercased, NUL-separated
(`\x00`) after EVERY field including the last:

```
canonical = lower(trim(method))    || 0x00
         || lower(trim(path))      || 0x00
         || lower(trim(timestamp)) || 0x00
         || lower(trim(nonce))     || 0x00
         || lower(hex(SHA256(body))) || 0x00
```

The signature itself is `ed25519_dalek::Signer::sign` over these bytes
directly (EdDSA hashes its own message internally via SHA-512 — wrapping
the canonical string in a second SHA-256 first, the way `transcript_digest`
does for the SAS, would add nothing here and would only obscure the pinned
vectors). `path` is `aoide_storage::peer_store::url_path(&peer.url)` on the
signer's side and the HTTP request's own parsed path on the verifier's side
— both derive it the same way a bare loopback/tunnel/reverse-proxy `POST /`
already does, so the two must and do agree byte-for-byte.

**Pinned stability vectors**
(`aoide-storage::wire_auth::tests::canonical_string_stability_vectors_never_drift`;
a future change to the field order, the separator, the case-folding, or
the digest algorithm must move this table in the same commit):

```
canonical_string("POST", "/", "2026-08-25T00:00:00Z", "abcd1234", b"{}")
  == "post\x00/\x002026-08-25t00:00:00z\x00abcd1234\x00\
      44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a\x00"
```
(`sha256("{}") == 44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a`,
independently verifiable.) Case and surrounding whitespace on
method/path/timestamp/nonce never change the canonical string; a different
body, or a different path, always does (via the digest, and directly).

**Identity IS the key; the name is a label (#63 P-ID5).** The resolved
`Peer` is the one whose stored `pubkey` verifies the signature — the
signature proves possession of a key, and the record is found BY that key,
never by the `X-Aoide-Peer` name. Nothing on the signed path trusts a name:
the claimed name is stamped into audit lines (and, on a claimed-vs-resolved
mismatch, an `attribution-drift` audit line naming both), while the
RESOLVED name feeds every downstream consumer — the `allows` lookup, the
`peer:<name>` origin stamp, autogate. Renaming a peer record locally
therefore never breaks inbound signed requests from it, and two peers
colliding on a claimed name cannot cross-resolve.

**Inbound verification** (`aoide-server::a2a::verify_signed_request`, called
once per connection in `handle_connection`, strictly before EITHER the
streaming or the plain-JSON-RPC dispatch path) — checks run cheapest-first,
never spending a signature verification on a request already disqualified
for a cheaper reason:

1. All four headers present, else `-32007`.
2. `X-Aoide-Peer` is a well-formed name, else `-32007` — wire-format
   validity only; the VALUE never selects a record.
3. `X-Aoide-Timestamp` parses as ISO-8601, else `-32007`.
4. **Replay guard, timestamp half**: the timestamp is within
   `aoide_storage::wire_auth::signature_skew_secs()` of this instance's own
   "now" (±120s default, `AOIDE_SIGNATURE_SKEW_SECS` overrides) — else
   `-32008`, a taught error naming BOTH timestamps (the request's claimed
   time and this instance's own "now") and the configured window.
5. **By-key resolution**: the signature — over the canonical string rebuilt
   from the VERIFIER's own parsed request (never trusting a wire-carried
   canonical string) — is tried against every `verified` peer's stored
   `pubkey` in `state/peers.json` (operator-curated small N; one ed25519
   verify is microseconds; an unverified or keyless record never enters the
   trial set, so an unverified peer can never be resolved by signature). No
   key verifies → `-32007` "signature verification failed" — ONE code path
   and ONE message whether the signing key is unknown, the peer is
   unverified/keyless, or a known peer's signature is simply bad: the
   refusal is never an existence oracle over the registry.
6. **Collision semantics**: exactly one record's key verifies → that record
   IS the caller. Multiple verified records sharing the verifying pubkey
   (possible — `upsert_paired_peer` matches by name only, so one remote
   instance paired under two names yields two records with one key): the
   record whose name exactly matches the claimed `X-Aoide-Peer` wins (both
   candidates hold the same PROVEN key, so the tiebreak picks among
   equally-AUTHENTICATED records — it never elevates a name to identity);
   no exact-name match → `-32007` "ambiguous signer", a taught refusal —
   the records' `allows`/`autogate` may differ, so guessing is never
   allowed. AuthZ consequence, stated plainly: the key's holder can claim
   whichever twin's name it likes, so a key's effective grant set is the
   UNION across every record sharing it — revoking a capability from a
   key means revoking it on EVERY such record, or `peer remove`-ing the
   duplicates.
7. **Replay guard, nonce half**: `(verifying pubkey, nonce)` has not been
   seen before by this server process — else `-32009`. Keyed on the PUBKEY,
   not any name: `X-Aoide-Peer` is outside the canonical string, so a
   captured request replayed under a shared-key twin's name still lands on
   the same cache key. The nonce is recorded ONLY after every earlier check
   (including the signature itself) passes, so a forged or garbage nonce
   never consumes a cache slot.

A request that fails ANY of these fails CLOSED — no fallthrough to the
addr/token resolution ladder for a request that claims to be a paired
peer's signed request and isn't one; `handle_connection` returns the
refusal directly rather than continuing to `route`/`stream_task`. A request
carrying NONE of the four headers is untouched by any of this — the
existing token/addr resolution ladder and the door-wide bearer path (read
arms, `tasks/get`, the AgentCard GET) apply exactly as before this phase,
for every caller that never signs.

**Nonce cache** (`aoide-server::a2a::NONCE_CACHE`) — a bounded, in-memory,
PER-`a2a serve`-PROCESS `VecDeque<(pubkey, nonce)>`, capped at 4096 entries
(`NONCE_CACHE_CAP`), FIFO-evicting the oldest entry once full. No file
behind it, unlike everything else this door's peer/pairing state persists
— an `a2a serve` restart clears it outright, a known and accepted
limitation (the same "process-local guard" shape `aoide_storage::pairing`'s
own `PARK_LOCK` already carries): a replay that arrives after a restart
isn't caught by the cache, only by the timestamp window, which is why both
checks run independently rather than either alone. The cap sizes against
plausible signed-request volume within one skew window, not against any
particular deployment's real traffic — bounding worst-case memory against a
hostile or malfunctioning peer, never expected to be reached in normal
operation.

**New JSON-RPC error codes**: `-32007` (signature verification failed —
covers the malformed-header and ambiguous-signer shapes above, plus the
single no-key-verifies refusal that unknown-key/unverified-peer/keyless-
record/bad-signature all collapse into), `-32008` (clock skew beyond the
window), `-32009` (nonce replay).

**Spawn gate narrows again: PeerRung::Signature only.** `aoide_storage::
peer_store::PeerRung` gains a third variant, `Signature` — the new
STRONGEST rung, never produced by `resolve_peer` itself (which has no
access to the raw HTTP request a signature needs); it is yielded only by
`a2a.rs`'s own `verify_signed_request` → `message_send`'s resolution,
which — when `signed_peer_name` is `Some` — resolves EXCLUSIVELY against
that name (`PeerRung::Signature`), with NO fallback to the addr/token
ladder even on a registry-lookup miss (fail-closed: a request that
`verify_signed_request` already proved came from peer X is never silently
re-resolved as if it came from whoever's address or token happens to
match). `spawn_admitted` now accepts ONLY `PeerRung::Signature` — the
Token rung, sufficient after the P-P3 amendment above, no longer reaches
`do_spawn` at all. The `-32006` refusal message is now shape-specific: a
genuinely paired peer resolved via the (now-insufficient) Token rung is
told plainly that its aoide is too old to sign requests, or is failing to
sign them, and to upgrade the caller — not told to re-pair, since pairing
already succeeded and the only gap is the missing signature; a Signature-
resolved peer whose `allows` simply lacks `spawn` is told the exact `peer
allow <name> spawn on` fix; every other shape (Addr rung, no resolution,
an unverified Token match) gets the original "pair first, then allow"
message, now naming the signature requirement too. The CODE stays `-32006`
across every shape (existing callers already match on it). Inject's `from`
attribution and the address/token resolution ladder for every other
purpose are UNCHANGED — this narrowing is scoped to the Spawn arm alone,
exactly as the P-P3 amendment scoped its own narrowing.

**Outbound (client side)** — `aoide-client::commands::sign_headers_for_peer`
is the ONE production call site that ever builds these headers: for a
`peer.verified == true` target it loads this instance's own P-P1 identity
(`aoide_storage::identity::load_or_mint`), mints a nonce, stamps "now,"
signs the canonical string, and returns the four header pairs; for an
unverified/unpaired peer it returns an empty header list, leaving that
call's transport byte-identical to the pre-P-P4 path. Wired into all three
real peer-POST call sites (`pull_one_peer`, `pull_peer_live`,
`send_message_to_peer`) — never into the pairing-ceremony's own wire calls.
`aoide/pairRequest`/`aoide/pairReveal` stay fully unauthenticated by design
(P-P2 above) and always pass an empty header slice; `aoide/pairPoll`
(Design A, task #119, this section's own subsection below) carries a
signature too, but a SEPARATE self-contained one
(`aoide_storage::wire_auth::canonical_string`/`sign_hex` called directly,
never through `sign_headers_for_peer`) — no `Peer` record exists yet at poll
time for that function's `peer.verified` check to key off. `post_json`'s new
`extra_headers` parameter rides as plain
`-H "<name>: <value>"` curl argv literals — unlike the bearer token's
stdin-hiding trick, nothing in a P-P4 signature header is a secret worth
hiding from `/proc/<pid>/cmdline`.

**Private key discipline unchanged.** No new `Serialize`/`Deserialize`
type was added anywhere signing touches; `identity.rs`'s own mechanical
`no_private_material_in_any_serialize_type` gate (P-P1) stays green
untouched. Signing happens in-process, in `aoide-client`, via
`aoide_storage::identity::Keypair::sign` — the raw signing key never
crosses a socket, an `Outcome`, or a log; only the resulting signature
(public, verifiable material) rides the wire.

This subsection is **additive**: four new headers, three new error codes,
one new `PeerRung` variant, and an in-memory-only nonce cache with no
stage-file shape of its own. Carries no version bump to §1–§6 and needs no
playbook migration entry.

**Amendment (P-P5b, `docs/architecture/PAIRING.md`): a method-honesty fix,
and `peer spawn` — the first production caller to sign a SPAWN-shaped
POST.** Two closes, one commit:

- **Method-honesty (P-P4 review finding 2).** `canonical_string`'s
  `method` field used to be TWO independent hardcoded `"POST"` literals —
  one at `aoide-client::commands::sign_headers_for_peer` (the signer), one
  at `aoide-server::a2a::verify_signed_request` (the verifier) — that
  merely happened to agree, never a value either side actually read off
  the request it was building/verifying. The module doc's "binds method"
  claim was true only because nothing today ever signs anything but a
  POST, not because the code checked it. Fixed by threading the REAL
  value through instead: the verifier now reads `&req.method` (the
  HTTP request's own OBSERVED method, already parsed by
  `parse_http_request` — this crate never lacked it, it just wasn't
  being used here) rather than a literal; the signer now reads a single
  named `aoide-client::commands::HTTP_METHOD` constant that ALSO drives
  `post_json`'s own `-X` argument, so the two can never independently
  drift again. Every real request today is genuinely a POST, so this
  changes no byte of any produced canonical string — the pinned vectors
  above are UNCHANGED, and no vector needed to move.
- **`peer spawn <name> [--yes] -- <text…>`** (`aoide-client::commands::
  handle_peer_spawn`, golden 77 → 78, §3 above): the CLI command that
  actually reaches the spawn gate this section's P-P3/P-P4 amendments
  built. Builds the exact spawn-shaped body `do_spawn` consumes —
  `aoide_client::wire::build_message_send_body(text, messageId, None)`,
  `contextId` OMITTED (the same shape `decide_send_action` reads as
  "spawn," regardless of the `aoide/spawn` metadata flag) — and signs it
  via `sign_headers_for_peer`, the FIRST production call site that ever
  signs a request carrying no `contextId`; every earlier real caller
  (`pull_one_peer`, `pull_peer_live`, `send_message_to_peer`) sent a read
  or an Inject (always `contextId: Some(..)`). The CLIENT gates LOCALLY on
  exactly one question — is `name` a registered, `verified` peer at all —
  refusing an unknown or merely-`peer add`-registered (unpaired) name with
  a taught error naming `peer pair`, since an unsigned request
  could never satisfy the remote's `PeerRung::Signature`-only requirement
  regardless. It gates on NOTHING else: `allows` lacking `spawn`, an
  unsigned-but-paired caller, clock skew — every other refusal shape is
  the remote door's own call, and `handle_peer_spawn` surfaces it
  VERBATIM rather than re-deriving or translating it. `--yes` skips a
  purely LOCAL `y`/`N` confirmation (`peer pair approve`'s own idiom) —
  it has no bearing on the remote gate.

### Legacy escapes

Four rungs answer "who is this caller," gathering what the amendments
above accreted across P-P3/P-P4 into one place to read rather than
reconstructing it from three dated entries — they are NOT interchangeable:
the **door-wide bearer** (`aoide.a2a.tokenFile`/`bearerSecret`) and a
peer's own **`token_file`** are both **legacy escapes for an UNPAIRED
caller** (`docs/architecture/PAIRING.md` decision 2) — they authenticate
the READ arms (`tasks/get`, the AgentCard GET, `aoide/graphSummary`) and
Inject's autogate/`effective_origin` coupling, and nothing else; the
door-wide bearer alone never even resolves a peer IDENTITY, and neither
has ever been sufficient for Spawn. **`addr`** (`PeerRung::Addr`, a bare
TCP-source-IP-vs-registered-`url` match) resolves a peer identity for
ATTRIBUTION only — Inject's `from` field, the autogate question — and is
never sufficient for Spawn, since it carries no possession proof at all
(spoofable by anyone who reaches the door from that address, or who sits
behind the same NAT/proxy as the real peer). **Signature**
(`PeerRung::Signature`, P-P4's per-request ed25519 binding) is the one
rung a PAIRED peer earns by completing the ceremony (`peer pair`) and
signing every request with the identity that ceremony verified —
strictly stronger than `token_file`'s bare replayable shared secret, and
the ONLY rung Spawn accepts (`a2a.rs::spawn_admitted`). A verified
signature also outranks loopback for the Inject gate: an ssh `-L` forward
(or any other loopback-terminating proxy) delivers a tunneled peer's
packets from its own end's sshd, so `classify_origin` sees loopback for
every tunneled request regardless of who is really on the other end — but
a request `verify_signed_request` already verified is, by construction, a
remote peer, so `a2a.rs::origin_for_inject` strips `PeerOrigin::Loopback`'s
free pass from it before `should_deliver_now` ever runs, leaving the
signature-rung `autogate` flag (folded into `autogate_match` alongside
`ip_autogate`/`token_autogate`) as the only route back to auto-delivery for
a signed peer, exactly as an operator already granted it. In short: the
read arms and attribution tolerate any of the four; Spawn accepts exactly
one; and once a request is signed, its delivery timing is decided by
autogate, never by which address it happened to arrive from. §7's
"`state/peers.json`" subsection below has the full mechanical detail
(which field backs which rung, `resolve_peer`'s ladder, tie-break order).

### Status

The option surface (`aoide.a2a.enable`/`bindAddress`/`port`/`spawnAgent`/
`tokenFile`) and the `a2a serve` command are **real**: the AgentCard,
`tasks/get`, and `message/send` (Phase B2: inject-or-spawn execution, above)
all run. The CLIENT side of this door is the `peer` family (§7): a registered
peer folds into the session DAG as a `kind:"peer"` node, and `peer spawn`/
`send --to <peer>/<query>` drive `message/send` against it. (An
earlier, pre-pairing client half — `a2a agent add|list|remove|send`,
unsigned and ungated — was deleted outright once `peer` superseded it.)
This section is **additive**: it introduces a new contract, carries no
version bump to §1–§5, and needs no playbook migration entry (nothing
existing changed shape).

### Remote reach (P-D5, `docs/architecture/AOIDED.md`'s L3)

`aoided` — the resident daemon (P-D2/P-D4) — grows **no network listener of
its own, ever**: `bin/aoided.rs` runs only `aoide_server::daemon::run_loop`,
whose `bind_socket` (`daemon.rs`) is a plain `std::os::unix::net::
UnixListener` on a local socket path, nothing else. Verified directly against
`crates/server/src/`: the only `TcpListener::bind` anywhere in that source
tree belongs to `a2a.rs`'s `a2a serve` — a SEPARATE, existing, already-gated
door (`aoide.a2a.enable`, above) a session opts into explicitly, not
something the resident daemon ever stands up on its own.

Composed, an operator's remote reach into the mesh from claude.ai runs
entirely over doors this contract and root `AGENTS.md` already name, in one
fixed order, with no new transport at any hop:

```
claude.ai
  │  Tier-3 Aoide MCP connector (tailnet/funnel, user-enabled ONLY —
  │  root AGENTS.md Tier 3 — never enabled by an agent)
  ▼
some mesh host's aoided-adjacent session
  │  aoide session --hosts / peer registry (peer_store::PeerRegistry) enumerates
  │  the mesh — no second inventory (§7)
  ▼
send --to peer/<query>  (aoide_storage::addr::resolve)
  ▼
message/send over THIS door (§6 above), bearer-authenticated
  (token_authorized / Peer.tokenFile / Peer.bearerSecret) — tunneled to
  whichever registered peers are themselves enabled, one A2A hop per peer
```

The hub (`Peer.hub`, P-D5 — `peer_store::set_hub`/`clear_hub`, driven by
`aoide peer hub <name> [--clear]`) is the last-resort address-resolution
preference this route composes with when a `--to` query names nothing else
reachable: `aoide_storage::addr::resolve_with_hub` wraps `resolve` and
substitutes a designated hub peer only on that function's own `NotFound` —
every earlier precedence tier (exact id, tail4, petname, host/role compound,
`peer/<rest>`) is untouched (`addr.rs`'s own grammar doc). As of this phase
`resolve_with_hub` is a tested library function in `aoide-storage`, not yet
threaded through `send`'s live `--to` call site (`aoide-conduct::graph
::send`, which still calls plain `resolve`) — the same "land the pure
function first, wire a real caller in later" order `addr.rs`'s own tier-5
`peer/<rest>` grammar went through (P-C1 landed it library-only; C3 wired
`send` to it). It never adds a network hop, never opens a port, and is
pure preference: a mesh with no hub set resolves exactly as before this
field existed.

### Pairing wire (P-P2, `docs/architecture/PAIRING.md`)

Three new methods on the SAME existing A2A JSON-RPC/HTTP door (§6) — no new
transport, no new server, no new port. All three are **deliberately
unauthenticated** (`read_ok`/bearer gating never applies to any of them):
the ceremony's whole point is establishing a credential where none exists
yet, so gating it on one would be circular. A parked or revealed request
grants nothing at all — only a fully APPROVED request commits a peer
record, and that record's own `verified: true` plus its `allows`
(P-P3, stamped by `upsert_paired_peer` from `[pairing] defaultGrant` or the
commit's own `--allow`, the moment the peer first becomes verified) is the entire grant this ceremony makes; the
wire methods themselves flip no OTHER gate and change no spawn/bearer
behavior beyond that one stamp — narrowing or widening `allows` afterward
is `peer allow`'s job (§3 above), never re-run by re-pairing. Unknown
methods still get the standard `-32601`;
malformed params get `-32602` before anything is parked, persisted, or
committed; a park-queue-full refusal is the distinct `-32000` (the park cap
below); a reveal's commitment mismatch is the distinct `-32002`.

The ceremony is a standard **commit-then-reveal** handshake (the Bluetooth
SSP idiom) rather than a single round trip carrying both sides' pubkeys and
nonces in the clear: only the first mover (A, the requester) commits to its
own nonce before revealing it, closing the active-MITM gap a single-round-
trip design leaves open (an on-path attacker who sees both real values in
one message controls four of the SAS's six transcript-shaping fields and
can brute-force a 6-digit code against fast SHA-256). B, the approver, may
reveal its own nonce immediately in its synchronous response — a
counterpart choosing its own values only AFTER seeing that response still
cannot force a chosen SAS to match, because A's nonce is fixed by A's
commitment and unknown to anyone until A's own reveal lands.

**`aoide/pairRequest`** — the REQUESTER's box (A) POSTs this to the
APPROVER's box (B)'s A2A door:

```json
{ "jsonrpc": "2.0", "id": 1, "method": "aoide/pairRequest",
  "params": { "pubkeyHex": "<64 lowercase hex>", "name": "box-a",
              "commitHex": "<64 lowercase hex>", "url": "http://box-a:8710/",
              "selfVia": "ssh://khoa@box-a" } }
```

`pubkeyHex` is A's own ed25519 public key (P-P1's `identity::load_or_mint`,
minted on first use if absent); `name` is A's claimed nickname for B's own
registry entry — validated server-side against the same `valid_peer_name`
`peer add`/`peer remove` already enforce, since `peer pair approve` reuses
it verbatim with no separate override; `url` is A's own advertised A2A door
URL, recorded on B's resulting peer record for B's future non-ceremony
calls (Design A, task #119: the ceremony's own completion no longer dials
this URL — nothing "callback"-shaped exists on this wire). `commitHex` is
`SHA256(pubkeyHex || 0x00 || nonceHex || 0x00)`, hex-encoded
(`aoide_storage::pairing::derive_commit`, the same canonical
lowercased/trimmed/NUL-separated field style `derive_sas` already used) —
A's own nonce itself is chosen locally and does NOT ride this message.

`selfVia` (OPTIONAL, task #131) is A's own self-asserted reach-back hop
claim — `ssh://[user@]host`, the LOGIN half defaulting to
`$USER`/`$LOGNAME` and the HOST half defaulting to the LOCAL OUTBOUND
ADDRESS the kernel routes toward the peer being dialed
(`aoide-client::commands::outbound_ip_toward` — a `UdpSocket::connect`
that sends no packet, only resolves a route; falls back to the claimed OS
hostname only if that lookup itself fails), overridable via `--self-via`
on `peer pair`, either arm. The HOST half is deliberately NOT a
claimed hostname by default — a live LAN check found hostnames resolving
only through the router's DHCP-DNS, and two boxes on the same network
coming back as IPv6/link-local mixes: resolution by luck, exactly the
fragility `default_via`'s own "never a claimed host" stance (this
document, `Peer.via` section) exists to avoid; every live `via` row is
IP-based for the same reason. It exists because a request that reaches B
over A's own ssh tunnel arrives, as far as B can observe, from loopback: B
has no way to derive a working `via` for A from the connection itself.
`selfVia` is A's own claim of that hop — the same trust class as `url`
(self-asserted DATA, a transport marker only; trust stays in pubkeys +
SAS, never this field). Absent when A has no such claim, or when A
predates this field; B never refuses a request over its absence.

B parks the request whole, `selfVia` included
(`aoide_storage::pairing::park_inbound`, disk-persisted under
`state/peer-pairing-inbound.json`, STABLE non-array-position ids —
correlation must survive both processes exiting and an async callback
arriving arbitrarily later, unlike `state/stage/pending.json`'s idiom, and
capped — see below) and answers SYNCHRONOUSLY with its own public identity
and a fresh nonce of its own:

```json
{ "jsonrpc": "2.0", "id": 1,
  "result": { "id": "<8 hex chars>", "pubkeyHex": "<B's 64-hex pubkey>",
              "nonceHex": "<B's 32-hex nonce>", "expiresAt": "<ISO-8601>" } }
```

Expiry is generous by design (hours, not minutes — a human has to relay a
code out-of-band): `AOIDE_PAIRING_TIMEOUT` in seconds, else a 4-hour
default (`aoide_storage::pairing::DEFAULT_PAIRING_TIMEOUT_SECS`). Expired
entries are swept lazily on the next `list`/`take` call, never a background
timer. Nothing about parking ever appears in `state/peers.json` until an
explicit approval and confirm (below).

**`aoide/pairReveal`** — A's SECOND POST, sent to the SAME door immediately
after the response above, inside the SAME `peer pair` invocation
(two sequential POSTs, then the CLI prints the SAS):

```json
{ "jsonrpc": "2.0", "id": 1, "method": "aoide/pairReveal",
  "params": { "id": "<the id aoide/pairRequest returned>",
              "nonceHex": "<A's 32-hex nonce>" } }
```

B recomputes `SHA256(pubkeyHex || 0x00 || nonceHex || 0x00)` against the
parked entry's own `commitHex` (`aoide_storage::pairing::reveal_inbound`).
On a match, B stores A's nonce on the entry and answers:

```json
{ "jsonrpc": "2.0", "id": 1, "result": { "ok": true } }
```

On a mismatch B DROPS the parked entry outright (unlike a released
`aoide/pairPoll` pubkey's own mismatch handling below, a false commitment is
not a recoverable data hiccup — it's the exact shape an active attacker's
forced retry would take) and answers `-32002`. The reveal is
unauthenticated like the rest of the bootstrap, so a third party who
obtains a live pending id can destroy that one ceremony attempt with a
bogus reveal — an accepted denial-of-one-attempt (never an
impersonation); the operators re-run the ceremony. An unknown/expired id
is `-32001`. An inbound entry that has not yet been revealed shows in `peer
pair pending` with no SAS (`"revealed": false`); `peer pair approve`
against it refuses outright with a taught "awaiting reveal" error — there
is nothing to confirm until A's nonce is known, since the SAS transcript
needs it.

**`aoide/pairPoll`** (Design A, task #119 — **REPLACES the original
`aoide/pairApprove` reverse callback outright**). The original shape had B
POST a callback BACK to the URL A supplied in its request the moment B's
operator approved — which meant B's door had to dial OUT to A, so a
REQUESTER whose own door binds loopback-only (a door that never accepts a
routable connection at all — the house policy every door in this system
already follows) could never be reached and the ceremony could never
complete (observed live, 2026-08-28: the callback timed out against a
loopback door every time). Now B's `peer pair approve <id>` is **purely
local**: it commits B's own peer record for A, then marks B's own parked
inbound entry's `approved` field `true`
(`aoide_storage::pairing::mark_inbound_approved`, `state/peer-pairing-inbound.json`
§4 above) and leaves it PARKED — never taken — so A can find it later. A's
own `peer pair approve <id>` POLLS for that release instead, over the SAME
forward dial its `aoide/pairRequest`/`aoide/pairReveal` already used (never
a reverse leg):

```json
{ "jsonrpc": "2.0", "id": 1, "method": "aoide/pairPoll",
  "params": { "id": "<the id aoide/pairRequest returned>",
              "timestampIso": "<ISO-8601, when this poll was signed>",
              "nonceHex": "<a fresh 32-hex nonce>",
              "signatureHex": "<A's own signature over the canonical string below>" } }
```

The poll carries its OWN self-contained signature — never P-P4's
header-based scheme, which needs a VERIFIED peer record to check against,
and none exists on B's side until the very id this poll asks about is
approved (a bootstrapping problem P-P4 cannot solve here). A signs
`aoide_storage::wire_auth::canonical_string("PAIRPOLL", id, timestampIso,
nonceHex, &[])` (P-P4's own canonical-string primitive, reused with an empty
body) with A's OWN identity — the SAME key whose `pubkeyHex` rode A's
original `aoide/pairRequest`, so B verifies the signature directly against
the parked entry's own stored `pubkeyHex` (`state/peer-pairing-inbound.json`
§4 above) — the requester's own pubkey, captured at request time, with no
peer-record lookup involved at all.

**Existence-oracle discipline (mirrors this section's own 2026-08-20
amendment for `message/send`'s `contextId` lookup, above): an unauthenticated
or wrongly-signed poller learns nothing an authenticated one couldn't.**
Three cases — the id doesn't exist (never parked, already expired), the
signature doesn't verify against the entry's own stored pubkey, or the entry
exists and verifies but isn't approved yet — all answer with the IDENTICAL:

```json
{ "jsonrpc": "2.0", "id": 1, "result": { "status": "pending" } }
```

The uniformity is byte-level, not timing-level (an unknown id refuses before
the signature verify; a known one pays it), and the signed `nonceHex` is NOT
replay-checked — a captured poll replays inside the skew window, accepted
deliberately: the response is idempotent and releases only B's own pubkey,
which `aoide/pairRequest` already hands to any caller.

Only a poll that BOTH verifies AND finds the entry already approved gets the
release — B's own identity, re-derived fresh (never stored on the parked
entry):

```json
{ "jsonrpc": "2.0", "id": 1, "result": { "status": "approved", "pubkeyHex": "<B's 64-hex pubkey>" } }
```

A then checks that `pubkeyHex` against the value B's OWN synchronous
`aoide/pairRequest` answer already gave it — the SAS/transcript binding, the
gate against a substituted reveal — a mismatch leaves the outbound entry
untouched (still `awaiting-approval`, never re-parked, never dropped — a
legitimate retry after a transient data hiccup is not permanently broken)
and refuses locally (`aoide_storage::pairing::mark_outbound_awaiting_confirm`'s
own `ConfirmMarkError::Mismatch`, unchanged from before — only its trigger
moved from a server-side callback handler to this client-side poll
consumer). On a match, A transitions its outbound entry to
`awaiting-confirm` the SAME way it always did.

**The commit asymmetry is deliberate (decision 4's mutual confirmation),
unchanged by Design A.** B already committed its OWN peer record for A the
moment B's own operator ran `peer pair approve <id>` — now with NO wire call
at all, purely local (`aoide-client::commands::approve_inbound`'s own doc).
B's confirmation is the TYPED pairing code (task #120 P3): B's operator
types the code as read off A's screen, out-of-band, and it is compared
against B's own locally derived SAS — the approve prompt never echoes that
SAS (printing the expected value beside the input would collapse the
comparison into a copy exercise; `peer pending` shows NO code at all
either, P-PV2 — the threat model is the comparison, and a listing either
operator could glance at defeats it the same way an echoed prompt would).
A wrong code counts one try,
persisted on the parked entry across invocations (`--code NNN-NNN` is the
scripted spelling, one try per wrong invocation); the third cumulative
mismatch auto-denies — the same clean removal `peer pair reject` performs,
nothing committed, audited as `auto-deny-on-code-mismatch`. A commits its
OWN record only later, once A's own operator runs `peer pair approve <id>`
a SECOND time — polling first, then (once approved) re-deriving the SAS
from values already held locally and confirming it y/N (A's own screen
already printed the code at request time, so the typed-code gate is B's
side only). Both humans still engage the SAME code before either side
calls the pairing done on their own end. A never-confirmed A leaves B
holding a `verified: true` peer that simply answers nothing until A
confirms; the fix is a visible, expiring outbound entry (`peer
pending`) and an ordinary re-pair, not a special recovery path. `peer pair
reject <id>` against an outbound entry aborts it at ANY stage
(`awaiting-approval` or `awaiting-confirm`) — no wire call, no peer record —
doubling as the ceremony's own missing abort command.

**Park cap.** `park_inbound` refuses beyond
`AOIDE_PAIRING_PARK_CAP` concurrently parked inbound requests (default 32,
`aoide_storage::pairing::DEFAULT_PAIRING_PARK_CAP`), checked under ONE lock
acquisition immediately before insert — the same check-then-insert
discipline `aoide-secrets::park::park_if_room` already holds, closing the
same unauthenticated-and-unbounded-queue shape that function was built to
close. The refusal is the distinct `-32000`, a taught error naming the cap
and its override env var. Outbound entries are operator-created (one `peer
pair` invocation each, never wire-driven) and carry no cap.

**SAS derivation** — UNCHANGED by this ceremony rework: a standard
numeric-comparison Short Authentication String (Bluetooth-SSP-style), no
invented cryptography, still SHA-256 (`aoide_storage::pairing::derive_sas`,
the sanctioned `sha2` dependency, PAIRING.md decision 1) over the same four
public transcript values, each lowercased/trimmed and NUL-separated — only
how the two nonces REACH each side changed (commit-then-reveal instead of
one round trip), never the hash, the field order, or the truncation:

```
digest = SHA256(requesterPubkeyHex || 0x00 || approverPubkeyHex || 0x00
              || requesterNonceHex  || 0x00 || approverNonceHex  || 0x00)
sas    = (be_u32(digest[0..4]) mod 1_000_000) formatted "%03d-%03d"
```

Both sides compute this independently from values they already hold (never
trusting a wire-carried SAS) — A from its own request plus B's
`aoide/pairRequest` answer, B from the parked-and-revealed inbound entry
plus its own identity. Pinned stability test vectors
(`aoide-storage::pairing::tests::derive_sas_stability_vectors_never_drift`):
`derive_sas("a"*64, "b"*64, "c"*16, "d"*16) == "740-729"`; swapping
requester/approver roles on the same four values yields a DIFFERENT code
(`"847-405"`) — the transcript is order-sensitive, not a set.

This subsection is **additive**: it introduces `aoide/pairReveal` alongside
`aoide/pairRequest`/`aoide/pairPoll` on the existing A2A door, and
`state/peer-pairing-inbound.json`/`state/peer-pairing-outbound.json` (§4
shape, both gitignored root-runtime state, both tolerate-missing-as-empty)
gain `commitHex`/`requesterNonceHex` (now optional, absent until revealed)
on the inbound side and `approverNonceHex`/`state` on the outbound side —
all additive, `#[serde(default)]` where a legacy record could otherwise
fail to parse. Carries no version bump to §1–§6 and needs no playbook
migration entry.

### Pairing events feed (P-P5)

`aoide/pairRequest`/`aoide/pairReveal` (above) were audit-only until P-P5:
nothing told a watcher a ceremony milestone had landed short of polling
`peer pending`. `a2a serve`'s `emit_pairing_event` (`aoide-server::a2a`)
now appends one best-effort, never-`?`, never-panicking record onto
`aoided`'s OWN events feed — the SAME `$XDG_RUNTIME_DIR/aoide/events.jsonl`
(`aoide_server::daemon::events_path`) `aoided` itself writes through, via
`aoide_protocol::feed::FeedWriter` — from the Ok arm of each of these
methods, never from a mismatch or unknown-id arm:

```json
{ "v": 0, "ts": 1735000000, "class": "gate", "kind": "pair-parked",
  "source": "a2a-door",
  "payload": { "id": "abc12345", "name": "box-a", "originAddr": "10.0.0.5",
               "url": "http://box-a:8710/", "direction": "inbound" } }
```

Two `kind`s: `pair-parked` (`pairRequest`'s Ok arm), `pair-revealed`
(`pairReveal`'s Ok arm) — `class: "gate"` (the existing `EventClass::Gate`,
`aoide-protocol` §3's registry-adjacent audit module, its first emitter),
`source: "a2a-door"`. `payload` carries fields BY NAME ONLY —
`id`/`name`/`originAddr`/`url`/`direction` — **NEVER a SAS, pubkey,
nonce, or commitment.** A watcher (`aoide peer pair watch`, below) re-
derives the SAS locally from its own identity plus
`aoide_storage::pairing::list_inbound`/`list_outbound` — the feed line is
only ever a TRIGGER to re-check them, never itself trusted data, the
same "tail is a trigger, the storage-backed list is the authority"
stance `aoide-secrets`' own events feed already holds for its notify
mirror.

**Design A (task #119) retired the THIRD `kind`, `pair-awaiting-confirm`.**
It used to fire from `pairApprove`'s Ok arm — the approver's callback
landing on the requester's OWN door, a genuinely REMOTE-triggered local
event worth surfacing. `aoide/pairPoll` (this section's "Pairing wire"
subsection above) never emits an event on the APPROVER's side (a poll
arriving and being answered isn't a state change on that side worth
surfacing — the approver already knows it approved; it did so itself,
locally) — and the REQUESTER's own side never transitions
`OutboundState::AwaitingConfirm` asynchronously anymore either, only
synchronously inside `peer pair approve`'s own poll-then-mark call. A
consequence, honestly stated rather than silently absorbed: **`aoide peer
pair watch --popup`'s outbound-completion auto-detection (below) has no
live trigger anymore** — an outbound entry never becomes `actionable`
(`aoide-client::pair_watch::actionable`) on its own; the operator must run
`peer pair approve <id>` themselves to poll and complete it. The plain CLI
path is unaffected. Wiring active polling into the watch loop's own
30s tick is the follow-on that would close this gap; not built here.

**Two writers, one file (an accepted, named race).** `a2a serve` and
`aoided` are separate processes; both open their own `FeedWriter` onto
the identical path, capped at `EVENTS_CAP_BYTES` (1 MiB) and
truncated-in-place past that cap rather than rotated. A cap-truncate race
at the exact boundary can lose a line from either writer — accepted,
because this feed is ephemeral cues, never the durable record (the
single audit log, written at every call site regardless, is that
record) and because the watcher's own reconcile tick (30s, or on-demand
before any action) re-derives the truth from `aoide_storage::pairing`
directly rather than trusting the feed's own completeness.

**`aoide peer pair watch [--popup] [--json]`** (`aoide-client::
pair_watch`, registered newest in `peer pair`, golden count 74 → 75) is
the foreground follow: tails this feed, narrates each recognized line
(or emits it verbatim under `--json`), and re-derives the actionable set
(an inbound entry once revealed and only while still unapproved, an
outbound entry once `awaiting-confirm` — see the "retired" paragraph above
for what this means under Design A) on a 30s safety tick so a missed or
malformed line never strands a request. **The inbound gate reads
`InboundPairingRequest::approved`, not the SAS alone.** An approved
inbound entry stays parked (§ "Pairing files" above — the requester's own
`aoide/pairPoll` still has to find it) and keeps its derivable SAS
forever, so a SAS-only gate re-raises the typed-code dialog on every tick
for a request the operator already answered. `--popup` swaps that narration for a dialog shaped by
DIRECTION (upgraded P-PV3, task #132) — `lyra pair ask`/`lyra pair
confirm` (`crates/lyra/src/commands/dialog_qml.rs`'s shared quickshell
surface) when `aoide_client::pair_watch::resolve_lyra_bin` feature-detects
it (the identical three-tier check `aoide_secrets::watch::
resolve_lyra_bin` uses), else `zenity --entry`/`zenity --question`,
falling back to zenity on a `lyra` spawn/infra failure for that one
attempt.
**INBOUND (approver): a TYPED-CODE entry dialog, never a bare yes/no.**
`lyra pair ask` — the SAME six-boxes-plus-dash surface `lyra secrets ask`
renders — or `zenity --entry`. Exit 0 hands back the TYPED code, gated
through `InboundGate::Code(<typed>)` — the SAME SAS comparison and
three-cumulative-mismatch auto-deny machinery the CLI tty/`--code` paths
already hold, byte-identical — and the dialog NEVER shows the code
(showing it would collapse the out-of-band comparison into a copy
exercise).
**OUTBOUND (requester): a CONFIRM dialog, never a retype.** `lyra pair
confirm` or `zenity --question`. This instance already generated its own
SAS before the dialog ever opens, so the dialog SHOWS it large and plain
(not a leak — the CLI's own `confirm_sas` already prints it) and the
operator's whole job is a single Approve/Reject action —
`approve_outbound(true, ...)` runs unconditionally on Approve, exactly as
it always has. (An earlier pass within this same phase collected a typed
retype on the outbound arm too, reusing the inbound entry surface with the
code pre-shown — review correctly called that copy-the-pixels theater,
since the code is already on screen in the same window the retry field
sits in; the revert lands in the same commit as the rest of P-PV3.)
Both directions share the dialog's own `"Reject request"` extra
button/dismiss control (a distinct label from `aoide-secrets`' own
`"Dismiss ask"` — two ceremonies, two labels, one shared reader), which
rejects; a bare Cancel/Escape ignores the request for the rest of that
session only; a spawn/infra failure on BOTH binaries backs off the retry
cadence and is NEVER treated as a dismissal. `--popup` is refused up front
when NEITHER `lyra` nor `zenity` resolves; `--popup`+`--json` together is
a usage error. Deployed as the graphical-session USER unit
`aoide-pair-watch.service` (`modules/nucleus/aoided.nix`), gated on
`aoide.a2a.enable && aoide.facets.quickshell.enable &&
aoide.a2a.pairingPopup` — the last of those DEFAULT FALSE (modules' own
"flags default off" house rule): the unit exists and is desktop-facet-
gated the same way `aoide-secrets-watch` is, but the popup itself is
opt-in on top of that, never assumed just because a2a and the quickshell
facet are both on.

This subsection is **additive**: a new events-feed record shape and a
new CLI command, no change to the wire methods above, no version bump.

### Discovery advertisement (P-P6 + task #120,
`docs/architecture/PAIRING.md`'s "Discovery (advertise-but-locked)"
section)

**No new transport, no new door.** A UDP broadcast advertisement —
Aoide's own, never mDNS — the IPv4 limited-broadcast address and a fixed
port both ends of the feature agree on without any handshake, since there
is nothing to negotiate: one JSON line per advertisement, fire-and-forget,
never a connection. Broadcast, not multicast (task #120's #106 fix): the
original multicast group never crossed the User's router — verified live
2026-08-27, each box heard only itself — and broadcast needs no group
membership, no interface pinning, and no capability probing; the listener
is a plain `0.0.0.0` bind on the fixed port, which hears broadcast and
unicast datagrams alike.

**Discovery grants NOTHING.** A heard advertisement feeds `peer
discover`'s printed table, `peer pair`'s hostname arm's target resolution,
and `peer list`'s advertising marks and `◆` pair-candidate rows (§7's CLI
surface) ONLY —
the pairing ceremony above
(`aoide/pairRequest`/`aoide/pairReveal`/`aoide/pairPoll`) is the ONLY
thing that ever writes `state/peers.json`; nothing on this subsection's
own wire ever does. **Discovery is rendezvous, not authentication**: the
wire carries a name and an ssh hop claim and nothing else — no
credential, no public key, no fingerprint (a key on a discovery wire
invites treating discovery as trust; pairing's mutual SAS confirmation
stays the one trust gate), and never a door URL (doors are
loopback-bound; ssh is the only cross-box transport). Hearing an
advertisement proves nothing by itself, exactly the way an AgentCard GET
(§6 above) proves nothing until a caller is actually paired.

**Pinned constants** (`aoide_storage::advertise`, the one crate both the
advertiser and the listener depend on — see that module's own doc for the
full reasoning):

```
dest = 255.255.255.255    (IPv4 limited broadcast — link-local by
                           definition, never crosses a router)
port = 8711               (UDP; one past a2a serve's own TCP 8710 — a
                           mnemonic pairing, not a forced availability;
                           grepped clean against every other bound port
                           in pkgs/aoide — nothing else in this tree binds
                           a UDP port or TCP 8711 at all)
version (v) = 2           (v1 was the multicast-era {v, name, fpr, url}
                           shape — a v1 line is dropped as wrong-version,
                           never half-parsed)
max line = 512 bytes      (checked on the RAW bytes, before any JSON
                           parse, on BOTH ends — aoide_storage::
                           advertise::MAX_LINE_BYTES)
```

**Wire shape** — one JSON line, no envelope, no framing beyond UDP's own
datagram boundary:

```json
{ "v": 2, "name": "yomi-strix", "host": "yomi-strix", "user": "khoa" }
```

`name` is the advertiser's own instance name (`aoide-server::a2a::
resolve_peer_name`'s same value — the identical name `aoide/graphSummary`'s
`instance.name` already carries, §7 below). `host`/`user` are its ssh hop
claim — the makings of a `--via ssh://user@host` transport marker: `host`
the advertiser's own hostname (`aoide_storage::display::local_host_name`),
`user` the login its `a2a serve` process runs as (`$USER` → `$LOGNAME`).
Both are CLAIMS kept for display and for the operator who wants a name
instead of a DHCP lease; the address a consumer actually uses is the
packet's OBSERVED source (below).

**Advertise (off by default).** `a2a serve` — the process that owns the
door, never a separate daemon — always starts the advertise thread, but a
tick only SENDS when the switch is on: `aoide peer advertise on|off`
(task #120) flips `state/advertise.json`
(`aoide_storage::advertise::enabled`/`set_enabled` — idempotent, reports
what changed, absent file = OFF), read fresh by the thread every tick so
a toggle lands within one cadence with no restart; `aoide.a2a.
discoveryAdvertise`/`AOIDE_DISCOVERY_ADVERTISE` (env, truthy
`1`/`true`/`yes`/`all` — the same vocabulary `AOIDE_CONDUCT_AUTOGATE`
already established) or the `--discovery-advertise` flag
(`aoide-server::a2a::resolve_discovery_advertise`, mirroring
`resolve_bind_port`/`resolve_spawn_agent`'s own
flag-then-env-then-default precedence) FORCE it on for that process's
lifetime, OR'd with the switch — the nix-declarative path. A sending tick
(~30s, jittered up to +10s so a LAN full of advertisers doesn't key up in
lockstep) binds a fresh ephemeral UDP socket with `SO_BROADCAST`, sends
exactly one advertisement line to the fixed destination, and drops the
socket — fire-and-forget, no connection state held between ticks. No
identity file is touched to advertise — the wire carries no fingerprint.
**No resident listener exists anywhere** — hearing an advertisement is
always an on-demand sweep, never something `a2a serve` itself does. **The
receiving host's own firewall must admit inbound UDP on this port** — a
default-deny firewall drops the advertisement before any Aoide socket
ever sees it (`docs/architecture/PAIRING.md`'s "Discovery" section has
the diagnosis); the nix module opens this port automatically alongside
`discoveryAdvertise`.

**Discover** — `aoide peer discover [--secs N]` binds the fixed port,
listens `N` seconds (default ~4), and validates every line heard
(`aoide_storage::advertise::parse_and_validate`) BEFORE it is ever
displayed (house rule 4 — an advertisement is untrusted network data):
the size cap first (on the raw bytes), then the JSON parse, then
`v == 2`, `name` (`aoide_storage::peer_store::valid_peer_name`), `host`
(bounded hostname/dotted-quad shape, no metacharacters), and `user`
(bounded POSIX login shape). An advertisement failing any one check is
dropped and counted — never echoed, never partially rendered. Survivors
are deduped by (`name`, source address), keeping the freshest sighting's
fields (a restarted advertiser's new claim wins over a stale one) in a
BOUNDED in-memory fold (`aoide-client::discover::MAX_HEARD`, 64 distinct
entries — a hostile flood past the cap is counted dropped, never grown);
the printed table carries
name/host/user/**srcAddr**/first-heard/last-heard/count, the heard-set a
later phase's `peer list` consumes. `srcAddr` is the UDP packet's own
source IP — captured by the listening socket itself, never sent by the
advertiser — kept on the client's local, unpinned `Heard` record, never
on the wire-shape `Advertisement` above. `host`/`user` are what the
advertiser CLAIMS; `srcAddr` is what was actually OBSERVED, and the
address anything downstream dials. This command NEVER writes
`state/peers.json`.

**`peer pair`'s hostname arm** — `aoide peer pair <target> [--secs N]
[--yes]`, when `<target>` does not read as a URL (P-PV2, the User's locked
spec, superseding the former separate `peer invite` command outright —
hard cutover, no alias) is sugar over the ceremony, nothing more: it runs
its OWN discover sweep (default 45s, not `peer discover`'s own ~4s —
task #129's known miss), resolves `<target>` against what was heard
(exactly one source claiming that name → proceed; zero or more than one →
a taught error listing every name actually heard), composes its dial
target from that advertisement's OBSERVED `srcAddr` on the house door
port (`AOIDE_A2A_PORT` or `8710` — the wire deliberately carries no door
URL to read a port off; a far end on a non-default port takes the
explicit URL-target arm, `peer pair <url>`), and on a single match runs
the EXACT SAME `run_pair_request` core the url arm calls — a shared
function, not a copy — against that composed target, recording a `via`
derived from `srcAddr` plus the claimed `user` for the resulting peer's
future calls. Before dialing, it refuses when the resolved target is this
instance's OWN advertisement: the heard name matching this instance's
own, or the datagram having come from loopback — either one a taught
refusal (a broadcast always loops back to its own sender, so a box that
advertises hears itself every sweep). Known gap: a serve advertising
under a custom `--peer-name` flag escapes the name arm (this arm derives
its own name from env/hostname only) and the self-heard broadcast arrives
on the physical interface, missing the loopback arm — such a pair dials
this box's own door and parks a self-pairing request; confusion, not
compromise, since both SAS codes land in front of the same operator.
`--yes` skips only the local proceed-confirm; the ceremony's own mutual
SAS confirmation (both operators, both ends, decision 4) is untouched and
still the sole authority.

**Bare `aoide pair`** (task #120 P3) is the friendly, interactive entry
onto the same rails: CLI-door + real-tty only (the same
`pick::interactive` gate `aoide session grant undying` bare holds — a
non-tty, non-CLI, or `--json` invocation gets a taught pointer at the scripted spellings,
never a hang), it runs ONE bounded ~2s sweep, filters out this box's own
advertisement, and opens a select menu over the candidates — each row the
already-validated name plus claimed ssh hop and OBSERVED source, claim
and observation side by side. Picking a row IS the proceed-confirmation
and drives the EXACT SAME shared tail `peer pair`'s hostname arm uses
(`pair_with_heard` → `run_pair_request` — one function, never a copy), so
the SAS then prints with the approve step for both ends. Hearing nothing
teaches `peer advertise on` (the other box) and the manual `peer pair
<url> [--via …]` path instead of failing.

**Spoofed advertisements are phishing, and the ceremony catches them** —
an attacker advertising a victim's name can lure a pair attempt, but the
SAS confirmation is mutual: the code on the requester's terminal must
match the code on the REAL counterpart's terminal, and that counterpart's
own operator must approve. An advertisement can misdirect a request; it
cannot survive the code comparison. (With two sources claiming one name
in the same sweep, the hostname arm refuses as ambiguous before dialing
either.)

This subsection introduces no new door and no new field on any existing
wire shape (§4's peer-pairing files are untouched by this feature) — an
advertisement is transient, UDP, never-persisted network traffic, gone
the instant a sweep's deadline passes; its one piece of state is the
advertise switch file above, additive and tolerate-missing. Carries no
version bump to §1–§6 and needs no playbook migration entry.

---

## 7. Peer federation door — **v0** (2026-08-14)

Aoide-to-aoide federation: one aoide instance can register ANOTHER aoide
instance as a **peer** by URL and pull its resolved session graph into its
own, folded in as a subtree. Built on §6's existing A2A door — ONE new
JSON-RPC method (`aoide/graphSummary`), a client-side peer registry +
per-peer pull cache, and an additive fold in `build_graph`
(`graph/doc.rs`).

**Melete-optional**: this federation works standalone; nothing in it
references or requires Melete. A Melete-side consumer (a Rune polling skill,
first-class `graph_view` rendering) is separate, independently-owned work,
not part of this contract.

**Topology-blind by design**: a peer is addressed by a plain URL — the
protocol carries no notion of "same LAN" vs. "tailnet" vs. "the internet".
The house runs Tailscale (peers addressed by tailnet MagicDNS hostname in
practice), but nothing here is tailnet-specific: any reachable URL works the
same way. WAN/NAT-traversal/relay reachability for peers that are NOT on the
same network is explicitly **out of scope for v0** — a later, separate
contract amendment, not designed or assumed here.

### `state/peers.json` — **v0**

The peer registry: OTHER aoide instances this one has registered by URL
(`aoide peer add <name> <url>`). Lives in the gitignored root-runtime
`state/` dir (§2, state-dir resolution as in §4's `state/usage.json`
entry) — **not** `song/stage/`, a deliberate divergence from an earlier
draft of this contract that sketched `song/stage/peers.json`: a peer roster
is account/global external-registry state, not song-scoped rehearsal state.
Written atomically
(`aoide_storage::peer_store`); **additive/tolerate-missing** — an absent
file is simply "no peers registered", never an error; readers round-trip
fields they do not know.

```json
{
  "schemaVersion": "0",
  "peers": [
    { "name": "yomi-strix", "url": "http://yomi-strix:8710/", "autogate": false, "addedAt": "2026-08-14T00:00:00Z" },
    { "name": "watching-peer", "url": "http://watching-peer:8710/", "autogate": false, "addedAt": "2026-08-24T00:00:00Z", "pubkey": "a1b2…", "verified": true, "allows": ["read", "spawn"] }
  ]
}
```

`pubkey` (string, optional, hex, additive per P-P2) and `verified` (bool,
default `false`, additive per P-P2) are set **only** by the pairing
ceremony below (`aoide_storage::peer_store::upsert_paired_peer`) — never by
`peer add`, and a legacy record predating this field loads with `pubkey:
null`/`verified: false` unchanged. A peer entered via `peer add` (no key,
`verified: false`) and one entered via `peer pair` (`verified: true`) are
the SAME registry, two separate paths onto it: `peer add` for the
hand-set-URL escape hatch, `peer pair` for the one ceremony that verifies a
public key on both ends. Re-pairing an EXISTING peer name replaces only
`pubkey`/`verified`/`url` — never `autogate`/`tokenFile`/`bearerSecret`/
`hub`/`allows` — and only after a fresh SAS confirmation (`peer pair
approve`'s own gate: the typed code on the approver's side, y/N on the
requester's), never silently. `aoide peer pair watch` (§6's
"Pairing events feed" subsection, P-P5) is what surfaces a ceremony
reaching this commit point WITHOUT polling `peer pending` by hand —
it never reads or writes this file directly, only
`aoide_storage::pairing`'s own parked-request state, the same source
`upsert_paired_peer` itself commits from.

`allows` (array of strings, additive per P-P3, `docs/architecture/
PAIRING.md` decision 5; omitted from the wire when empty) is a CLOSED
capability set — `aoide_storage::peer_store::PEER_CAPABILITIES` = `"read"`,
`"spawn"`, never a per-capability serde bool scatter. `upsert_paired_peer`
stamps it the moment a peer FIRST becomes `verified` (both ceremony commit
sites — `approve_inbound` and `approve_outbound`) from the grant its caller
resolved: `config.toml`'s `[pairing] defaultGrant` (`["read"]` by default),
or the `--allow` typed on that commit,
and leaves it untouched on a LATER re-pairing of an already-verified
name — a revoked capability survives key rotation. An unpaired (`peer add`)
peer and a legacy record predating this field both load `allows: []`. The
A2A door's Spawn arm (§6's P-P3/P-P4 amendments above) is the one thing
gating on it today: Spawn requires that peer to be `verified` with
`"spawn"` in `allows` **AND** the caller to have resolved via the
SIGNATURE rung specifically (§6's P-P4 amendment) — neither the address
rung nor the (now-insufficient) token rung, regardless of `allows`. `peer
allow <name> <cap> on|off` (§3's command list, §7's CLI surface below) is
the ONLY other writer — idempotent, refuses an unknown peer or an unknown
capability.

`resolve_peer(peers, addr, presented_token)` (`aoide_storage::peer_store`,
P-P3 decision 6) is the caller-identity ladder for the TWO unsigned rungs
— a presented bearer matched against ANY registered peer's own `tokenFile`
first (`PeerRung::Token` on a hit), the connection's origin address matched
against a peer's `url` second (`PeerRung::Addr` on a hit); it returns which
rung matched alongside the `Peer`. Unlike `is_autogated_peer_token`/
`is_autogated_peer_addr` (§6's 2026-08-19 amendment), it checks every
registered peer, not only ones marked `autogate` — "which peer is this" is
a different question from "should this peer skip the pending queue." The
three rungs `PeerRung` now carries are NOT interchangeable strength:
`Addr` is a bare TCP-source-IP-vs-`url` match, spoofable by anyone who can
reach the door from that address or who sits behind the same
NAT/reverse-proxy as the real peer; `Token` is possession of that peer's
own `tokenFile` secret — unforgeable by mere network position, but a bare
shared secret, replayable and identical across every request; `Signature`
(P-P4, the strongest — §6's own amendment for the full wire shape) is
never produced by `resolve_peer` itself, only by `a2a.rs`'s own
`verify_signed_request`, since it needs the raw HTTP request a bearer/addr
resolve never sees. `Addr` and `Token` both resolve a peer identity fine
for attribution (Inject's `from` field, origin-stamping) and for the
ordinary autogate question; Spawn is the one consumer narrow enough to
require `Signature` specifically. Ties resolve deterministically: `peer
add` refuses only a duplicate NAME, never a duplicate `url` host or
`tokenFile` content, so two peers CAN share either — `resolve_peer` then
answers with whichever matches FIRST in registry (array) order, not the
last, not random.

`autogate` (bool, default `false`) is the cross-device analogue of `graph
send`'s local "sender is the target's own parent" rule (§6's amendment
above): a peer marked `true` here has its INBOUND `message/send` auto-deliver
without the pending queue, even though its connection is non-loopback. An
unmarked/unknown sender is never autogated.

`tokenFile` (string, optional, additive per §6's 2026-08-18 amendment; set
via `peer add --token-file <path>`) is a path to a file holding THIS peer's
own shared secret — how an autogate-marked peer is identified by a presented
`Authorization: Bearer <token>` instead of (or alongside) its address, since
address alone is dead behind any proxy/tunnel. Absent by default; an
unmarked peer is identified by address only, exactly as before this field
existed.

`bearerSecret` (string, optional, additive; set via `peer add --bearer-secret
<name>`) is the mirror-image field, for the OTHER direction: the name of a
secret THIS instance resolves through the local secrets broker's unix-socket
wire (this document's "Secrets wire" subsection, self-asserted consumer
`a2a-client`) and presents as `Authorization: Bearer <value>` on every
OUTBOUND call to this peer's own A2A door (`peer pull`, `send --to`,
and the roster core's live presence probe (bare `session`/`--hosts`) —
`aoide-client::commands::
resolve_peer_bearer`/`post_json`). Resolved fresh on every request, never
cached; a resolve failure (broker unreachable, denied, or a bounded ~2s
timeout) fails the outbound call outright with a message naming the secret
and the broker socket, rather than silently sending it unauthenticated. The
resolved value is presented via curl's `-H @-` (read from this process's own
stdin) rather than an argv literal, so it never appears in the outbound curl
child's own `/proc/<pid>/cmdline`; the JSON-RPC body rides a short-lived
scratch file in that case instead of stdin. Absent by default (today's
behavior, unchanged): an unmarked peer's outbound requests carry no
`Authorization` header at all. `bearerSecret` and `tokenFile` answer
different questions and are independent of each other — `tokenFile` is what
THIS peer must present TO us; `bearerSecret` is what we present TO it.

`via` (string, optional, additive per P-S4, ssh-transport lane; set via
`peer add --via`/`peer pair [--via]`/`peer
spawn --via`) is an `ssh://[user@]host[:port]` transport marker
(`aoide_storage::tunnel::parse_via`'s shape). Absent by default (today's
every peer): every outbound call to this peer — every signed POST AND
`peer add`'s own unsigned AgentCard GET, its one verification call —
dials `url` directly, byte-for-byte the pre-P-S4 behavior. When present,
`aoide-client`'s dial resolution opens (or reuses) an internal ssh forward
to `via`'s host and dials `http://127.0.0.1:<local port>` through it
instead, rewriting only the URL's authority — the PATH is preserved
verbatim (`aoide_storage::tunnel::dial_url`), so the P-P4 signature (which
signs over the path, never the host) still verifies on the far end
unchanged; the AgentCard GET carries no signature to preserve, but dials
through the identical rewritten target, since it is otherwise the exact
scenario `--via` exists for (a loopback-bound door reachable only through
the tunnel) — `peer add` would fail verification before ever registering
such a peer if this one call bypassed the funnel. Either way `peer add`
registers the peer under its LOGICAL `url`, never the rewritten one.
`set_peer_via` is the only writer, a sibling to `upsert_paired_peer` rather
than a parameter on it — and a caller passing `None` means "nothing to
record," never "clear a previously-set marker": a plain `peer pair`
re-pair with no `--via` leaves an existing `via` (e.g. one an earlier
`peer pair` hostname-arm pair recorded) untouched. A `--via` flag on the
command itself always beats a peer's own recorded `via`. `peer pair`'s
hostname arm/bare `pair` additionally derive a default `via` from the
discovery advertisement's OBSERVED source
address plus its claimed ssh login (never a claimed host) and record it on
the resulting peer at pairing-approval commit time — and (task #131,
loopback-only doors) that SAME derived default now rides the ceremony's
OWN dial too, unless the advertisement carried no ssh claim at all, in
which case the dial stays direct exactly as before task #131.

The APPROVER side gets its own `via` a different way: `aoide/pairRequest`'s
OPTIONAL `selfVia` param (this document's pairing-wire subsection, task
#131) is the requester's own self-asserted reach-back hop claim, carried
through the parked inbound entry to `peer pair approve`'s commit — present,
the resulting peer's `via` becomes the claim itself (and `url` becomes
`http://127.0.0.1:<port>/`, `<port>` parsed off the requester's OWN
`url` param — never a claimed hostname reaching back onto the
requester-observed host itself, which the approver can never dial
directly through the very tunnel that delivered this request, and never
the approver's OWN `AOIDE_A2A_PORT` either, which names nothing about the
requester's door); absent, the approver's commit leaves `via` unset,
exactly as it always has.
Reaching a
peer's own A2A door remains loopback-bound either way — the tunnel is a
TRANSPORT hop, never a relay; the signed `X-Aoide-Peer` identity still
crosses it end to end. See `docs/architecture/PAIRING.md`'s Transport
section for the full design; a verified signature outranks a tunneled
connection's loopback origin for Inject delivery (`a2a.rs::origin_for_
inject`, this section's "Peer authentication today" paragraph above), so
this transport is safe to use against a real peer.

`aoide peer add <name> <url> [--autogate]` verifies the peer FIRST — fetches
its `/.well-known/agent-card.json` and only registers on success; a peer
that fails the fetch is never added. `--no-verify` skips this fetch
entirely — for a peer that serves no AgentCard at all (a plain A2A client
endpoint, e.g. an inbound-only harness): the peer is recorded exactly as
the verified path records it, `verified: false` either way (a card fetch
was always reachability, never identity — that only ever comes from `peer
pair`), so skipping it changes nothing about what gets written, only
whether the GET runs first. **A duplicate `name` is rejected
cleanly** (never an upsert-replace-on-readd: a peer's local nickname should
never be silently repointed at a different URL by a second `add`). `aoide
peer remove <name>` deregisters; a **missing name is an error**, not
idempotent-silent — following `rice draft drop <name>`'s precedent (§4).
`aoide peer status --json` enumerates the registry — its `data.peers`
carries every registered peer's full row (name/url/autogate/tokenFile/
bearerSecret/hub/pubkey/verified/allows/addedAt) layered with that peer's
last-pull outcome (below); it is THE deep per-peer registry view (the
human-readable `peer status` line stays a terse count; names and URLs live
in `--json`). `aoide peer list` is a different projection, never a registry
re-dump: the one-glance mesh roster — presence, discovery, and running
sessions across every known node — defined under this section's CLI
surface below.

### `state/peer-cache/<name>.json` — **v0**

One peer's last-PULLED `aoide/graphSummary` response, written by `aoide peer
pull [<name>]`. Sibling of `state/peers.json` (same dir family, same
state-dir resolution). **Additive/tolerate-missing**: no file means "never
pulled".

```json
{
  "schemaVersion": "0",
  "name": "yomi-strix",
  "instance": { "name": "yomi-strix", "url": "http://yomi-strix:8710/", "emittedAt": "2026-08-14T00:05:00Z" },
  "graph": { "schemaVersion": "0", "nodes": [], "edges": [] },
  "fetchedAt": "2026-08-14T00:05:03Z",
  "stale": false,
  "lastError": null
}
```

`instance`/`graph`/`fetchedAt` are the peer's own response from its LAST
SUCCESSFUL pull — verbatim (`graph` is that peer's own resolved
`graph.json` v0 document, §4's shape, unmodified). A FAILED pull
(unreachable, timeout, non-200, malformed body) never deletes this file or
clears these fields: it sets `stale: true` and `lastError` to a short
reason, preserving the last-known-good `instance`/`graph` — one peer being
down must never blank it out of the fold, and `peer pull` pulling several
peers must never let one failure abort the others (each peer's outcome is
independent). `aoide peer status` reports each peer's `fresh` /
`stale` / `never-pulled` classification (below) plus `fetchedAt`/`lastError`.

**Freshness TTL**: a named constant,
`aoide_storage::peer_store::PEER_CACHE_TTL_SECS` (5 minutes) — NOT a magic
number re-typed at each call site. A cache entry is `fresh` when `stale ==
false` AND `fetchedAt` is within the TTL of now; otherwise `stale`
(covers both an explicit failure mark and a plain TTL expiry — `peer
status`/the graph fold use the identical classification, so they can never
disagree).

### `aoide/graphSummary` — new A2A JSON-RPC method (§6 door)

One new method on the EXISTING A2A JSON-RPC/HTTP door (§6) — no new
transport, no new server. Takes no params; unknown-method callers still get
the standard `-32601` (confirmed unaffected — this method simply joins the
existing dispatch table in `a2a.rs::handle_jsonrpc`).

```json
{ "schemaVersion": "0",
  "instance": { "name": "yomi-strix", "url": "http://yomi-strix:8710/", "emittedAt": "2026-08-14T00:05:00Z" },
  "graph": { "schemaVersion": "0", "nodes": [ /* … */ ], "edges": [ /* … */ ] } }
```

`instance.name` resolves `--peer-name` flag → `AOIDE_A2A_PEER_NAME` env → the
OS hostname → the literal `"aoide"`, mirroring `resolve_bind_port`/
`resolve_spawn_agent`'s precedence discipline exactly (`a2a::resolve_peer_name`,
new `--peer-name` flag on `a2a serve`). `instance.url` is this instance's own
advertised URL (`http://<bind>:<port>/`, the same string the AgentCard's own
`url` field carries). `graph` is EXACTLY what `aoide graph --json`
resolves (`aoide_conduct::graph::resolve_graph_document`, the SAME function
that command and this method call) — no second graph vocabulary is invented
for the wire.

### The `peer:*` node-id convention (graph fold)

`build_graph` (`aoide-conduct::graph::doc`) ADDITIVELY folds each registered
peer in as a root node: `{ id: "peer:<name>", kind: "peer", name, url,
state, children? }`.

- A **fresh** cache (see the TTL rule above) contributes `state: "fresh"`
  plus `children: { nodes, graph's edges }` — the peer's OWN
  already-resolved subtree, nested VERBATIM, never flattened into this
  document's own top-level `nodes`/`edges` — so a peer's ids can never
  collide with a local id or another peer's.
- A **stale or never-pulled** peer still surfaces immediately (visible the
  moment `peer add` runs, before any pull ever succeeds) with `state:
  "stale"` and NO `children` — never a crash, never a silently-dropped peer.
  `error` carries the last pull failure's reason when present.

Local graph commands (`session prune`/`session reap`/`graph link`) and the focus jump
(`focus_session`/`focus_window`, conductor- and shellbridge-driven, no CLI
command) keep ignoring `peer:*` ids — confirmed by test
(`conduct::graph::manage::tests::local_only_commands_ignore_peer_ids`), not
just assumed to generalize: none of those commands read `peer_store` at
all, they operate purely on `sessions.json`'s `SessionRecord`s, so a
`peer:*` id is simply never a session id they could ever match.

### CLI surface

`aoide peer add <name> <url> [--autogate] [--no-verify] [--token-file <path>]
[--bearer-secret <name>]` / `remove <name>` / `pull [<name>]` /
`status` — registered as their own command group, directly after `a2a
serve` in `schema --json`'s order (nothing existing reorders). `peer pull`
with no name pulls EVERY registered peer; with a name, just that one.
`peer status --json` is this group's list-the-registry command — its
`data.peers` carries every registered peer's full row (name/url/autogate/
tokenFile/bearerSecret/hub/pubkey/verified/allows/addedAt) alongside that
peer's last-pull outcome: the deep per-peer detail view, which `peer list`
below never duplicates.

`aoide peer list [--json]` (task #120 P2, registered appended-newest at the
END of `schema --json`'s order, from `aoide-conduct` — the roster folds
the roster core's own probe (`who.rs`; reached via bare `session`/
`--hosts` — the standalone `who` command it originally backed is retired,
session-surface redesign, command-defrag lane X, 2026-08-28), and
`aoide-client` cannot depend on `aoide-conduct`) is
the one-glance MESH roster: one row per known node — this host first
(`this host`, same as bare `session --hosts`), every registered peer, then every advertising
instance heard on the LAN — with each node's running sessions (agent,
state, petname/short-id) indented beneath it. Marks: `●` paired/local and
online, `○` paired but offline (`last seen <fetchedAt>` off the pull
cache, or `never pulled`), `◆` advertising — appended to a paired row
(`●◆`/`○◆`) when a sweep hears its name, standing alone for an unpaired
pair-candidate row showing the OBSERVED source address. An online paired
row's addr is its `via` ssh marker when set, else its registered `url`
(doors are loopback-bound — a tunneled peer's `url` is `127.0.0.1`, so
the hop is what distinguishes it); an offline row's addr is `—`. Presence and
sessions come from the roster core's own live-probe-with-cache-fallback core
(`aoide-conduct::graph`, one bounded ~2s probe per peer, in parallel —
never a second prober), advertising from ONE bounded discovery sweep
(`aoide-client::discover::run_sweep`, ~2s, run concurrently with the
probes); an offline peer's last-known sessions render labeled `as of
<fetchedAt>`. An empty sweep is normal (firewall asymmetry — §6's
discovery subsection); even a sweep that cannot listen only annotates the
roster (`data.sweep.error`), never fails it. Heard fields stay untrusted
display data behind P-P6's validation gate; the roster writes nothing —
not `state/peers.json`, not `state/peer-cache/`. `--json` emits the same
roster structured: `nodes[]`, each `{mark, name, isLocal, paired,
verified, advertising, presence, addr, lastSeen, sessions[]}`, plus
`sweep` (`{heard, dropped}` or `{error}`).

`aoide peer pair <target> [--name <n>] [--self-url <url>] [--self-via
<ssh-target>] [--via <ssh-target>] [--secs N] [--wait SECS]
[--allow read,spawn] [--yes]` / `peer pending` /
`peer pair approve [<id>] [--yes] [--code NNN-NNN] [--allow read,spawn]` / `peer pair reject
<id>` / `peer pair watch` (P-P2, P-PV2 — the User's locked spec, three
grill rounds, appended newest directly after `peer hub` — §6's "Pairing
wire" subsection above has the exact wire shapes and SAS derivation) — the
pairing ceremony's CLI half. `peer pair`'s `<target>` is SMART: a
URL-shaped target (containing `"://"`) dials it directly (the former `peer
pair request`); anything else resolves it by a discovery beacon sweep
(default 45s — the 4s `peer discover` default proved too short in
practice, task #129 — the former `peer invite`, which DIED outright in
the same cutover, no alias). Either arm sends `aoide/pairRequest`
(`--self-via`, task #131, overrides its default reach-back hop claim),
parks the answer (`state/peer-pairing-outbound.json`, §4), and prints the
derived SAS alongside the pending id (the id is the secondary identifier,
for disambiguating multiple pending requests).
**`peer pair` then BLOCKS through the rest of the ceremony** (task #135
P2): it re-polls the approver's door every 5s for up to `--wait` seconds
(default 600), and on release runs the same confirm-and-commit
`peer pair approve` would, so one command completes this end. Only a
`pending` answer is retried — an unreachable door or a refused reveal
returns on the first tick. A timeout is NOT a failed pair: the request
stays parked and `peer pair approve <id>` still finishes it, which is also
what makes Ctrl-C safe. `--wait 0` restores the park-and-return shape for
scripted callers — and `--allow` beside `--wait 0` is REFUSED, not dropped:
nothing commits on that path and a grant is never persisted on a parked
entry, so it is retyped on the `peer pair approve` that does commit. The
wait's own deadline is MONOTONIC, so an NTP step or a suspend/resume
mid-wait cannot defeat it. `--yes` skips THIS side's own confirmations (the
sweep prompt and the final code confirm), never the far side's typed code.

`peer pending` lists this instance's own parked requests, both
directions, by id/direction/name/state — NEVER the SAS/confirmation
code (P-PV2): the code is read off the requester's own screen and typed
on the approver's, out-of-band, and a listing either operator could glance
at would defeat that comparison.
`peer pair approve` re-derives the SAS from this instance's own identity
(never trusting a wire-carried code); its `<id>` is OPTIONAL when exactly
one request is pending (that one is resolved; zero or multiple pending
without an id is a taught refusal, the multiple case listing every
pending id). On an INBOUND id the gate is the TYPED pairing code (task
#120 P3 — typed at a terminal prompt that never echoes the expected code,
or `--code NNN-NNN` scripted; wrong codes count cumulative, persisted
tries and the third mismatch auto-denies the request — `--yes` never
bypasses this), and a match commits a `pubkey`/`verified` peer record
PURELY LOCALLY (Design A, task #119 — no wire call at all) and marks the
entry approved for later release; on an OUTBOUND id, POLLS
`aoide/pairPoll` first (over the SAME forward dial the request already
used) and, once approved, prompts `y/N` (`--yes` scripted — this side's
own screen already printed the code) and commits. `peer pair reject` is a
clean local refusal — no wire call, no peer record, the matching entry
simply removed. `peer pair approve`/`reject`/`watch` are SUBCOMMANDS of
`peer pair` and WIN over a hostname positional of the same literal
spelling (the registry's own greedy longest-prefix match) — a box
literally named `approve`/`reject`/`watch` cannot be paired by bare
hostname and needs the explicit URL form instead. **A second collision
edge, review-caught:** `peer pair` takes exactly ONE positional; the old
three-token `peer pair request <url>` has no `peer.pair.request` path
left to match, so a typist's muscle memory lands `request`/`<url>` as
`peer.pair`'s OWN two args, past its single declared `target` — refused
outright as a usage error (naming the fold) rather than silently reading
only the first token and burning a full sweep window hunting a host
named "request" while quietly discarding the url.

`aoide peer allow <name> <cap> on|off` (P-P3, `docs/architecture/
PAIRING.md` decision 5, appended newest directly after `peer pair reject`
— §6's P-P3 amendment above and this section's "Peer record" subsection
have the full gate/wire-shape reasoning) flips one capability in a peer's
own closed `allows` set. Idempotent — `on` on an already-granted
capability or `off` on an already-revoked one both report a no-op, never
an error; refuses an unknown peer name or an unknown capability string
(checked before the peer lookup) with a distinct taught error for each.

`aoide peer spawn <name> [--yes] -- <text…>` (P-P5b, `docs/architecture/
PAIRING.md`, appended newest directly after `peer allow` — §6's P-P5b
amendment above has the full body-shape/gating reasoning) POSTs a signed,
spawn-shaped `message/send` (`contextId` omitted) to a PAIRED peer's own
A2A door; `<text…>` is the prompt typed as the newly spawned session's
first turn, never a remote-chosen executable (the PEER's own configured
`aoide.a2a.spawnAgent` is what actually runs). Refuses an unknown or
unpaired (`verified: false`) `name` LOCALLY with a taught error naming
`peer pair`; every OTHER refusal — `allows` lacking `spawn`, an
unsigned-but-paired caller, clock skew — is the remote door's own call,
surfaced verbatim, never re-derived here. `--yes` skips only the LOCAL
`y`/`N` confirmation (`peer pair approve`'s idiom); it has no bearing on
the remote gate, which is the sole security authority.

### Status

Real: the registry, the cache, `aoide/graphSummary`, the CLI commands
(`peer list`'s mesh roster included), and the graph fold all run. The pairing ceremony (P-P2, poll-based completion under
Design A/task #119) is real too: `pubkey`/`verified` on `Peer`, `peer pair
request|pending|approve|reject`, and the `aoide/pairRequest`/`aoide/pairReveal`/
`aoide/pairPoll` A2A methods (§6's "Pairing wire" subsection) all run end to
end. The `allows` closed set, `peer allow`, and
the A2A door's Spawn-arm hard gate (P-P3, narrowed again by P-P4) are real
too, end to end — a paired peer's `allows` genuinely gates the spawn arm,
and ONLY when that peer resolved via a per-request ed25519 SIGNATURE (§6's
P-P4 amendment above, `PeerRung::Signature` specifically — neither the
address rung nor a bare token match admits spawn any more). Signed wire
authentication (P-P4) is real end to end too: outbound signing
(`aoide-client::commands::sign_headers_for_peer`), inbound verification
with replay/skew guards (`aoide-server::a2a::verify_signed_request`), and
the pinned canonical-string vectors (§6's P-P4 amendment above) all run.
`peer spawn` (P-P5b) closes the last gap the P-P4 review found: before it,
every real client→peer call sent a read or an Inject, so this fully-built,
fail-closed gate could only be reached by a hand-crafted signed curl —
`peer spawn` is now the CLI path that actually exercises it.
**Out of scope for
v0** (explicitly, not an oversight): WAN/NAT-traversal/relay reachability
for peers not on the same network; Melete-side consumption (a polling Rune
skill, first-class `graph_view` rendering) — later, separately-directed
work. This section is **additive**: it introduces `state/peers.json` +
`state/peer-cache/`, the `aoide/graphSummary` method, and the `peer:*`
node-id convention; P-P2 additively introduces `Peer.pubkey`/
`Peer.verified`, `state/peer-pairing-inbound.json`/`-outbound.json` (§4),
and the pairing wire (§6); P-P3 additively introduces `Peer.allows`,
`SessionRecord.origin`/`LedgerEntry.origin` (§4), and `peer allow`. Amends
§6's `message/send` gating behavior (dated above) — no version bump to
§1–§6, no playbook migration entry (nothing existing changed shape beyond
the called-out §6 amendments).

---

## 8. Screen capture sidecar + pointer synthesis — **v0**

The `lyra screen` family (`pkgs/aoide/crates/screen/src/`) writes a
JSON sidecar (`<capture>.json`, same stem as the image) next to every
`screen shot`/`screen diff` capture. `screen point` has nine commands, six of
which synthesize real pointer input against a native Wayland backend
(`idle`/`save` are read-only queries, `restore` warps via `hyprctl` instead
of synthesizing). This section is the sidecar field contract, the
image↔screen scale contract, and the
`*-*` reason-code vocabulary every `screen` command's structured error draws
from. See `concepts/orchestration/Screen-Control` in the wiki for the
command-by-command usage this contract backs.

### The sidecar — `<capture>.json`

Fields below describe a capture written today, current `schemaVersion`
`"0"`. Reading an older sidecar back is fine — an absent field simply
means it postdates that capture — but this table isn't a version history,
so it doesn't track which field arrived when.

| Field | Type | Present when |
| --- | --- | --- |
| `schemaVersion` | string | always — `"0"` |
| `capturedAt` | string, ISO-8601 UTC | always |
| `origin` | `{x, y}`, logical (Hyprland) px | always — the captured rect's top-left |
| `size` | `{w, h}`, DEVICE px (post-`scale`) | always — what the image file actually contains |
| `scale` | float | always — `1.0` unless `--scale`/`--fit` was given |
| `region` | `{x, y, w, h}`, logical px | always — the exact requested rect verbatim (never recomputed) |
| `format` | string, `"png"` \| `"jpeg"` | always |
| `quality` | integer 0-100 | always (ignored for png) |
| `monitor` | string | only when captured via `--output` |
| `session` | string | only when captured via `--session` |
| `window` | string | only when captured via `--window` or `--session` |
| `class` | string | only when captured via `--window` or `--session` |
| `title` | string | only when captured via `--window` or `--session` |
| `comment` | string | only when `--comment` was given |
| `cursorDrawn` | bool | `Some(true)` only when `--cursor` was given, omitted otherwise — never `Some(false)` |
| `desktop` | `{cursor, clients, layers}` | omitted only when the hyprctl snapshot call itself failed at capture time — a degraded field, never a failed capture |
| `ocr` | object \| null | always present — `null` until `screen ocr` populates `{text, words}` |
| `diff` | object \| null | always present — `null` until `screen diff` writes its result; written into the AFTER-capture's own sidecar only, never the before-capture's |

### The scale contract

`scale` promises **`1.0` always means 1:1** between image pixels and screen
(logical/Hyprland) pixels — grim's own `-s` is forced explicitly on every
capture so a monitor's compositor-level DPI scale can never silently leak
into this field. The two directions: `screen = origin + image_px / scale`
(image → screen, `--from-shot`'s and `screen ocr`'s transform, via
`transform_point`) and `image_px = (screen - origin) * scale` (screen →
image — the same forward relation `expected_image_size` applies to a
region's `w`/`h` extent, applied here to a point). Both round to the
nearest pixel (`f64::round`), never
truncate. `region` exists specifically to prevent a lossy-inverse hazard:
recovering the original capture rect by dividing `size` back through `scale`
can drift by roughly 1px on either axis for about 1-in-`scale` widths/heights,
so `region` records the exact logical rect once, at write time, rather than
ever reconstructing it.

### The `*-*` reason-code vocabulary

Every `screen` command's structured error carries a backend-agnostic
`data.reason`. For most families the caller never learns which underlying
tool (grim, tesseract, the pointer backend) did the work from the code
alone, only from the free-text detail string if it wants to — those three
are interchangeable and stay anonymous by design. `hyprctl-*` is the
deliberate exception: `hyprctl` isn't a swappable backend, it IS the
compositor being queried or dispatched against, so its two reason codes
name it outright.

- **`pointer-*`** — `unavailable`, `failed`, `drift`, `refused`,
  `not-idle`, `out-of-bounds`, `nothing-saved`, `state-corrupt`,
  `save-failed`
- **`sidecar-*`** — `missing`, `corrupt`, `write-failed`
- **`from-shot-*`** — `out-of-bounds`, `bad-scale`
- **`text-*`** — `no-ocr`, `not-found`, `ambiguous`
- **`diff-*`** — `decode-failed`, `size-mismatch`
- **`capture-*`** — `unavailable`, `failed`, `not-found`
- **`ocr-*`** — `unavailable`, `failed`
- **`hyprctl-*`** — `unavailable`, `failed` — `HyprError::reason()`, reached
  through `hypr_error_outcome` at 28 call sites across the `screen` module
- **misc** — `dest-dir-unwritable`, `no-monitors`, `pick-cancelled`,
  `pick-failed`, `session-not-found`, `session-no-window`,
  `session-store-unreadable`, `window-not-found`

`screen send` sits outside this vocabulary rather than in it: its envelope
carries a `data.sidecarStatus` tag (`"ok"` / `"missing"` / `"corrupt"`,
from `SidecarRead::tag()`) alongside the underlying door's whole response
nested at `data.inner` — a failed send's real reason lives at
`data.inner.reason` (e.g. `"unknown-agent"`), not at the envelope's own
top level.

### The pointer backend

`zwlr_virtual_pointer_v1` is spoken natively, in-process, by
`screen::synth` — no shell-out. Every `screen point` command assembles a `Seq`
of motion/button/wheel steps and hands it to one `synthesize()` call, which
opens a Wayland connection, walks the `Seq`, and tears down. A press and its
release always share a SINGLE `synthesize()` call (`drag`'s press-move-release
is one atomic `Seq`, never two calls) — the stuck-button invariant only
tracks held buttons for the duration of one call, so splitting press and
release across two calls would leave a window where a crash or a killed
process abandons a physically-held button. On every exit path out of that
call, success or failure alike, `synthesize()` issues a release for every
held code and flushes with `WouldBlock` retry; a flush that ultimately
fails surfaces as `pointer-failed`.

**Live-proven** against Hyprland (2026-08-17): `create_virtual_pointer`
accepted with `seat = None`; `point move` lands pixel-exact (readback
matched request); a synthesized click focuses the window under it;
double-click delivers as one gesture; one scroll notch is one physical
wheel detent (3 notches moved kitty exactly 15 lines at its
default×5 multiplier — `WHEEL_VALUE = 15.0` is settled); drag's atomic
press-move-release paints a text selection; hover enter/motion reaches
layer surfaces (bar cell repainted its hover state under the pointer);
a post-run pointer sweep left no selection trail, so every release was
delivered. With that proof landed, the `wlrctl` fallback package is
retired from `modules/dendrites/vision.nix` — `screen::point`'s commands
all cross the pointer-synthesis boundary through `screen::synth`
in-process, and nothing else speaks for the pointer.

**Confirmed at the protocol level** the same day, with a `wl_pointer`
event logger (`wev`) as the receiving client — the events an app actually
gets, not pixels inferred from them:

| Command | What the client received |
|---|---|
| `move` | `enter` + a `motion` stream; surface coords matched the requested screen point exactly |
| `click` | `button` press (state 1) then release (state 0) on the right code, same millisecond |
| `click --count 2` | two press/release pairs 60 ms apart — inside any double-click threshold, with distinct `time`s |
| `scroll <n>` | n separate frames, each `axis` ±15.0 + `axis_value120` ±120 — 120 is `wl_pointer`'s own "one full detent", so one notch is exactly one detent |
| `scroll 0 <n>` | `axis: 1 (horizontal)`, same ±120 per notch — horizontal delivery is real; terminals just ignore the axis |
| `drag` | press, 21 interpolated `motion` events, release 240 ms later — a real drag, not a teleport |
| `hover` | `enter`/`motion` into the app, which repaints its hover state (see the `appeared[]` caveat below) |

**The stuck-button recovery is live-verified** (2026-08-17), by
manufacturing the hazard: a throwaway client pressed a button and called
`_exit(0)` still holding it. Findings, all measured:

- **Hyprland does NOT auto-release on client death.** The press stays
  down after the pressing client is gone — the invariant guards a real
  failure, not a theoretical one.
- **A held button holds an implicit grab.** While stuck, every pointer
  event goes to the grab-owning surface no matter where the cursor is
  (motion arrived at surface coords `-900,280`, far outside that
  window), and no other app receives anything. That grab surviving its
  own presser is what makes a stuck button so damaging.
- **One full `lyra screen point click <button>` clears it.** Only the
  RELEASE reaches the client: the compositor tracks button state per
  code, so the recovery click's press is absorbed as a duplicate and the
  release matches the held state and ends the grab.
- **The button code must match.** Clicking a different button while one
  is stuck changes nothing (measured: middle-click left a stuck left
  button stuck) — recover the exact code that is held.
- **A force-cleared grab sends no `leave`.** The client is simply cut
  off, still believing the pointer is inside it. Compositor behavior,
  noted so nobody reads a missing `leave` as a failed recovery.

---

## Versioning

- A contract version is a single integer, tracked in this file's section
  heading (`— v0`).
- The livery schema version is also surfaced in `stage/livery.json`
  (`schemaVersion`) and in `aoide schema --json` (`schemaVersion`).
- Bumping any version requires: (1) update this file, (2) add a playbook
  migration, (3) update the corresponding `checks` so the new contract is
  asserted.

### Release version (distinct from a contract version above)

Aoide itself carries a release version — `pkgs/aoide/Cargo.toml`'s
`[workspace.package].version`, the single source every crate inherits via
`version.workspace = true` and `pkgs/aoide/default.nix` READS with
`builtins.fromTOML` rather than copying (a literal there would drift on the
first bump that forgot it). This is NOT a contract version: bumping it never
implies a §1–§6 contract broke, and a contract bump never requires a release
bump either — they move independently.

- **Prebeta is `0.0.X`.** Every release today is `0.0.X`; `0.1.0` is beta,
  out of scope until the User calls it.
- **The patch number (`X`) bumps by one at the end of each completed major
  phase** — one edit, in the workspace manifest, in that phase's own landing
  commit. Never per commit and never mid-phase.
- **`aoide schema --json`'s `"aoide"` field is the runtime-readable
  version** (`aoide_protocol::registry::AOIDE_VERSION`, itself
  `env!("CARGO_PKG_VERSION")` off the workspace version above) — the doc
  example a few sections up in this file tracks it and must be bumped in
  the same commit as any release bump.
