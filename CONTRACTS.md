# Aoide Contracts

Versioned interfaces. The flake's `checks` fail a merge that breaks one; `aoide
update` detects contract bumps and routes them through the update playbook
before the rebuild discovers them (see `concepts/Governance` in the wiki).

Each contract carries a version. Bumping a version is a breaking change and must
land with a migration note in `song/songbook/update-playbook.md`.

---

## 1. Note schema — **v0**

The single seam between the frozen nix layer and the live desktop. Facets read
`aoide.drachma` and **nothing else**. Declared in `modules/nucleus/options.nix`.

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

Hex format: `#?[0-9a-fA-F]{6}` (leading `#` optional). The note engine
(`pkgs/drachma`, Agent A) owns the authoritative `rice lint` validator; the
option type is a permissive gate only.

### Geometry tier (v0 optional overrides — `geometry.*`)

Additive-optional (same status as the base16 tier): every field is `nullOr`,
defaulting to `null`. A notes file with no `geometry` block behaves exactly
as before — the compositor facet applies the fallback, not the option
system. Rides `song/stage/drachma.json` for live application: `aoide rice
preview` live-applies this tier (plus `window.border`/`borderInactive`) via
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

**Migration to v1:** the update playbook migrates `song/songbook/*/rice.nix`
and `drachma.json` from v0 to v1 when the design-system workstream lands v1.

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

Facets (`modules/facets/`) are the same shape but MAY read `aoide.drachma` and
MAY declare `aoide.surfaces.<name>.owner` — they read no other module.

### Repo shape (the root is closed)

The repo root is **closed**: `modules/`, `hosts/`, `pkgs/`, `lib/`, `docs/`,
`song/`, plus flake furniture (`flake.nix`, `CONTRACTS.md`, `README.md`, `.git*`)
and nothing else. New content lands **inside the existing tree at its designated
place** — never a new root directory:

- covers (wallpapers) → `song/songbook/<song>/assets/`
- chimes (sounds) → `song/songbook/<song>/sounds/`
- per-song assets → `song/songbook/<song>/`
- module assets → next to their module, as a directory dendrite/facet

Content paths are looked up in the Song Map (`concepts/Song-Vocabulary` in the
wiki). Creating a new root directory is a **contract change**, not a
convenience: it lands here first, with review — not sprayed into the tree.

The closed set above is the **committed** root; a handful of **gitignored
root-runtime** directories sit alongside it without being part of it —
`catalog/`, `index/`, `log/` (content-pipeline + audit runtime), and `run/`
(the deployed Quickshell tree rsynced from the store by home-manager,
`modules/facets/quickshell/default.nix`; disposable, never the checked-in
source). None of these are ever committed, so none are a contract change to
add to or write into.

### Package shape (`pkgs/` is walked too)

`pkgs/` self-registers exactly like `modules/` and `song/songbook/`. Drop
`pkgs/<name>/default.nix` — a `callPackage`-able derivation taking standard
nixpkgs args — and `lib/pkgs.nix` (the packages walker) discovers it into **all
four** consumers from one source:

- the flake `packages.<system>.<name>` output,
- the host overlay (`lib/mkHost.nix` → `pkgs.<name>` inside every module),
- the vm overlay (`lib/vmTest.nix` — literally the same import), and
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
  "aoide": "0.0.0",
  "commands": [
    {
      "path": ["rice", "gen"],
      "summary": "Generate a rice from a prompt or wallpaper.",
      "args": [
        { "name": "prompt", "type": "string", "required": false }
      ],
      "flags": [
        { "name": "json", "type": "bool", "description": "Structured I/O." }
      ],
      "gated": false,
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

---

## 4. Stage file formats — **v0**

Live-side (rehearsal) state written to `song/stage/` — gitignored runtime, never
committed, never load-bearing for the nix build (enforced by
`checks.no-song-read`). Emitted by the notes package (Agent A), shellbridge
(Wave 1), and the aoide CLI (`aoide graph`); read by Quickshell.

**Stage-dir resolution (the CLI ↔ unit seam).** Every stage reader/writer
resolves the stage directory through one function; nothing computes it
independently. Precedence: `$AOIDE_STAGE_DIR` when set to an **absolute** path
(the systemd unit sets `AOIDE_STAGE_DIR=%h/Aoide/song/stage`,
`modules/nucleus/shellbridge.nix`) → else `~/Aoide/song/stage` derived from
`$AOIDE_USER`/`$HOME`. A relative or empty value is ignored (a runtime path is
never resolved against an arbitrary cwd). On the default layout both agree; the
override is what lets the unit — or a test/smoke run — relocate the stage tree.

### `song/stage/drachma.json` — **v0**

The resolved, flattened note values for Quickshell (QML reads this; hot-reload
at rehearsal). Derived from the same `aoide.drachma` as the baked `rice.nix`
fan-out, so preview and adopted state cannot diverge.

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
field (string) — the name `aoide rice preview <name>` was invoked with. Set by
`handle_rice_preview` (mirrors the `parentSessionId` additive precedent in
§4's sessions.json). Absent means "no song identity" (a notes file staged some
other way). `DrachmaState.qml`'s `songName` property reads it to resolve
per-song flavor widgets (§5) — readers must tolerate both forms.

**Additive in v0:** the staged file MAY also carry an optional top-level
`geometry` block, mirroring §1's geometry tier (`gapsOut`/`gapsIn`/
`borderSize`/`rounding`/`blurEnabled`/`blurSize`/`blurPasses`, each `nullOr`).
Absent means "this song carries no geometry opinion" (§1's additive-optional
tier). `aoide rice preview` reads it (alongside `window.border`/
`borderInactive`) to build its best-effort `hyprctl keyword` batch — a missing
block, or a missing/null field within it, is skipped rather than defaulted;
readers must tolerate both forms.

### `song/stage/sessions.json` / `hooks.json` — **v0**

The shellbridge roster + live hook phases (full field tables in
`modules/nucleus/shellbridge.nix`). Session records: `{ sessionId, agent,
windowAddress, cwd, state, startedAt }`; hook records: `{ sessionId, phase,
updatedAt }`.

**Additive in v0:** a session record MAY carry an optional `parentSessionId`
(string) naming the session that spawned it — the graph's spawned-by edge. Set
by `aoide graph link` (cycle-checked), cleared by `aoide graph prune` when the
parent is removed. Absent means "no spawned-by edge"; readers must tolerate
both forms, and rewriters must round-trip fields they do not know.

### `song/stage/projects.json` — **v0**

Registered project anchor roots for the graph. Written by
`aoide graph project add/remove` (atomic, idempotent); read by
`aoide graph view/emit`.

```json
{ "schemaVersion": "0", "projects": [ { "name": "aoide", "path": "/home/khoa/Aoide" } ] }
```

### `song/stage/graph.json` — **v0**

The **fully resolved** project/session DAG, written by `aoide graph emit`
(atomic) for Quickshell to hot-reload — like stage notes, QML reads concrete
values and computes nothing. Project nodes anchor session nodes by cwd
(longest path-prefix wins, so nested projects anchor correctly); `spawned`
edges come from `parentSessionId`. A session with a resolved parent carries
only its `spawned` edge; root sessions carry an `anchors` edge (or none when
unanchored).

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

---

## 5. Song shape — **v0**

A **song** (rice) is a committed, **host-agnostic** score. The design thesis:
a song is host-agnostic; ANY host in the fleet performs it by naming it, and
the performance adapts to that host's specifics and its enabled facet/dendrite
set. The **venue (host) decides its instruments; the song carries only the
notes.**

### Selection — `aoide.song`

`aoide.song` (str, default `"default"`, declared in
`modules/nucleus/options.nix`) names the song this host performs. A host
replays any committed song with **one line**:

```nix
# hosts/<host>/default.nix
aoide.song = "moonlight";
```

Naming no song performs song `"default"` — the shipped standard
(`song/songbook/default/rice.nix`), the guaranteed-present baseline.

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
    aoide.drachma.palette = { bg = "…"; fg = "…"; accent = "…"; urgent = "…"; };
    # component tier (bar/notif/window) — null falls back to palette
  };
}
```

### Rules (host-agnostic discipline)

- A song sets **ONLY `aoide.drachma`** (palette + component tiers) and — later —
  cover/chime references inside `song/`.
- A song **NEVER** sets host options (monitors, hardware, services) and
  **NEVER** enables facets or dendrites. Those are the venue's decision.
- All note values are **literal nix** — a song never reads `song/` runtime
  paths (`stage/` · `auditions/`), same as the standard.
- Shelving/subfolders follow the walker rules (a `/_` path is skipped).

### The songbook is versioned score, not runtime

`checks.no-song-read` (§4) bans reading `song/` **runtime** dirs at eval
(`stage/` · `auditions/` · `catalog/` · `index/`). It
deliberately does **not** list `song/songbook/`: committed songs there are
versioned score, legitimately walked at eval. Walking the songbook never trips
the check.

### Enforcement

`checks.song-shape` structurally asserts every walked songbook path is a
`rice.nix` (a song's module entry) — catching a stray `.nix` that could set
arbitrary host options. The **full** "only defines `aoide.drachma`" invariant is
a documented convention here (isolated per-module option-diffing is
disproportionate for v0; see the `TODO(song-shape v1)` in `lib/checks.nix`).

**Migration to v1:** the update playbook migrates `song/songbook/*/rice.nix`
and `drachma.json` from v0 to v1 with the drachma schema (§1).

### Per-song flavor widgets

Beyond `aoide.drachma` notes, a song MAY also carry its own QML for a fixed
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
  preview possible. The manifest shape is unchanged (still
  `{ "<song>": ["<slot>", …] }`) by generalizing the walk from a fixed slot
  enum to "every file under `widgets/`" — additive, no contract-version
  bump.
- **Runtime resolution:** `DrachmaState.qml`'s `songName` property (above)
  names the active song; the staging engine (`StagingEngine.qml`) reads the manifest and answers
  "does `<song>` dress `<slot>`"; `WidgetSlot.qml` is the fixed per-slot
  anchor a host surface embeds — it loads the song's file when authored, else
  falls back to shared chrome (or renders nothing, when no fallback exists).
  `aoide rice preview <name>` (§4) drives this live, no rebuild: it stages
  `song` into `drachma.json`, `DrachmaState`'s `songName` updates, and every
  `WidgetSlot` re-resolves.
- **Fixed injected-prop contract:** a loaded widget receives `notes`
  (`DrachmaState`) and `bridge` (`ShellBridge`) always, plus whatever
  slot-specific extras the anchor declares (e.g. notifications' `notification`)
  — **never** nix `config.*`. This does not loosen the song-shape rule above:
  a song's `rice.nix` still sets **ONLY** `aoide.drachma` — widgets are
  committed QML files carried by the build, not nix options, and a widget is
  structurally incapable of reaching host/facet options through this surface.
- **Playbook:** `song/songbook/update-playbook.md`.

---

## Versioning

- A contract version is a single integer, tracked in this file's section
  heading (`— v0`).
- The drachma schema version is also surfaced in `stage/drachma.json`
  (`schemaVersion`) and in `aoide schema --json` (`schemaVersion`).
- Bumping any version requires: (1) update this file, (2) add a playbook
  migration, (3) update the corresponding `checks` so the new contract is
  asserted.
