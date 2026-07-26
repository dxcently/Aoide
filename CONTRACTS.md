# Aoide Contracts

Versioned interfaces. The flake's `checks` fail a merge that breaks one; `aoide
update` detects contract bumps and routes them through the update playbook
before the rebuild discovers them (see `concepts/Governance` in the wiki).

Each contract carries a version. Bumping a version is a breaking change and must
land with a migration note in `song/songbook/update-playbook.md`.

---

## 1. Note schema — **v0**

The single seam between the frozen nix layer and the live desktop. Facets read
`aoide.notes` and **nothing else**. Declared in `modules/nucleus/options.nix`.

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

Hex format: `#?[0-9a-fA-F]{6}` (leading `#` optional). The notes package
(`pkgs/notes`, Agent A) owns the authoritative `rice lint` validator; the
option type is a permissive gate only.

**Migration to v1:** the update playbook migrates `song/repertoire/*/rice.nix`
and `notes.json` from v0 to v1 when the design-system workstream lands v1.

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

Facets (`modules/facets/`) are the same shape but MAY read `aoide.notes` and
MAY declare `aoide.surfaces.<name>.owner` — they read no other module.

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
`checks.no-song-read`). Emitted by the notes package (Agent A) and shellbridge
(Wave 1); read by Quickshell.

### `song/stage/notes.json` — **v0**

The resolved, flattened note values for Quickshell (QML reads this; hot-reload
at rehearsal). Derived from the same `aoide.notes` as the baked `rice.nix`
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
(`modules/rime/default/rice.nix`), the guaranteed-present baseline.

### Self-registration (dendrite discipline)

Committed songs live under `song/repertoire/<name>/rice.nix`. `lib/mkHost.nix`
walks `song/repertoire` (via `lib/walk.nix`, same as `modules/`) into every
host, so **adding a song is a new folder — never an edit to an import list**.
The empty repertoire (just `.gitkeep`) walks to `[]` and is tolerated.

Each song's `rice.nix` **self-gates**, exactly like a dendrite:

```nix
# song/repertoire/<name>/rice.nix
{ lib, config, ... }:
{
  config = lib.mkIf (config.aoide.song == "<name>") {
    aoide.notes.palette = { bg = "…"; fg = "…"; accent = "…"; urgent = "…"; };
    # component tier (bar/notif/window) — null falls back to palette
  };
}
```

### Rules (host-agnostic discipline)

- A song sets **ONLY `aoide.notes`** (palette + component tiers) and — later —
  cover/chime references inside `song/`.
- A song **NEVER** sets host options (monitors, hardware, services) and
  **NEVER** enables facets or dendrites. Those are the venue's decision.
- All note values are **literal nix** — a song never reads `song/` runtime
  paths (`stage/` · `backstage/` · `auditions/`), same as the standard.
- Shelving/subfolders follow the walker rules (a `/_` path is skipped).

### Repertoire is versioned score, not runtime

`checks.no-song-read` (§4) bans reading `song/` **runtime** dirs at eval
(`stage/` · `backstage/` · `auditions/` · `catalog/` · `index/`). It
deliberately does **not** list `song/repertoire/`: committed songs there are
versioned score, legitimately walked at eval. Walking repertoire never trips
the check.

### Enforcement

`checks.song-shape` structurally asserts every walked repertoire path is a
`rice.nix` (a song's module entry) — catching a stray `.nix` that could set
arbitrary host options. The **full** "only defines `aoide.notes`" invariant is
a documented convention here (isolated per-module option-diffing is
disproportionate for v0; see the `TODO(song-shape v1)` in `lib/checks.nix`).

**Migration to v1:** the update playbook migrates `song/repertoire/*/rice.nix`
and `notes.json` from v0 to v1 with the note schema (§1).

---

## Versioning

- A contract version is a single integer, tracked in this file's section
  heading (`— v0`).
- The note schema version is also surfaced in `stage/notes.json`
  (`schemaVersion`) and in `aoide schema --json` (`schemaVersion`).
- Bumping any version requires: (1) update this file, (2) add a playbook
  migration, (3) update the corresponding `checks` so the new contract is
  asserted.
