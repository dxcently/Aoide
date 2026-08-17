# Aoide Contracts

Versioned interfaces. The flake's `checks` fail a merge that breaks one; `aoide
update` detects contract bumps and routes them through the update playbook
before the rebuild discovers them (see `concepts/Governance` in the wiki).

Each contract carries a version. Bumping a version is a breaking change and must
land with a migration note in `song/songbook/update-playbook.md`.

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
system. Rides `song/stage/livery.json` for live application: `aoide rice
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
`catalog/`, `index/`, `log/` (content-pipeline + audit runtime), `state/`
(account/usage runtime, e.g. `state/usage.json`; disposable, never committed),
and `run/` (the deployed Quickshell tree rsynced from the store by
home-manager, `modules/facets/quickshell/default.nix`; disposable, never the
checked-in source). None of these are ever committed, so none are a contract
change to add to or write into.

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

### `song/stage/livery.json` — **v0**

The resolved, flattened note values for Quickshell (QML reads this; hot-reload
at rehearsal). Derived from the same `aoide.livery` as the baked `rice.nix`
fan-out, so preview and adopted state cannot diverge. `stage/livery.json` is
the canonical name (the stage file was renamed by the livery merge);
writers mirrored to the legacy name and readers fell back during the
transition window, so a running desktop never read a missing stage file. The
mirror + fallback were dropped in Phase 4 of the livery merge —
`stage/livery.json` is the sole stage note file.

Beyond `aoide rice preview <name>`/`cover set`/other emitters writing this
live, it is also **seeded from the active song's committed notes on every
activation** (`home.activation.aoideSeedStage`,
`modules/facets/quickshell/default.nix`) — so a host that boots without ever
running `rice preview` still has a correct live stage twin from boot.

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
field (string) — the name `aoide rice preview <name>` was invoked with. Set by
`handle_rice_preview` (mirrors the `parentSessionId` additive precedent in
§4's sessions.json). Absent means "no song identity" (a notes file staged some
other way). `LiveryState.qml`'s `songName` property reads it to resolve
per-song flavor widgets (§5) — readers must tolerate both forms.

**Additive in v0:** the staged file MAY also carry an optional top-level
`geometry` block, mirroring §1's geometry tier (`gapsOut`/`gapsIn`/
`borderSize`/`rounding`/`blurEnabled`/`blurSize`/`blurPasses`, each `nullOr`).
Absent means "this song carries no geometry opinion" (§1's additive-optional
tier). `aoide rice preview` reads it (alongside `window.border`/
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
absent — the `SessionRecord`/`A2aAgent` Option convention). `song` names the
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

`rice draft save <name>` is a SEPARATE, mode-independent verb: it forks
whatever's currently live (reading transparently through a routing symlink
if one is active) into a new-or-updated draft snapshot without switching
modes — upserting (re-saving an existing name overwrites its `livery.json`
and, if the current stage no longer carries a cover, removes a stale
`cover.json`). `rice draft drop <name>` deletes a draft outright; a missing
name is an error, not idempotent-silent, and dropping the CURRENTLY-ROUTED
draft is refused (`draft-is-live`) rather than silently also tearing down
the routing and falling back to `staging` — switch modes first
(`rice mode stage`/`rice mode declarative`), then drop it.

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
`aoide graph reap` sweep). Distinct from `activity`, which stays what it was:
the tool running RIGHT NOW, hook-set and cleared when the turn settles. `tool`
survives that settle, so a resting session still shows what it last reached
for; a reader wanting "is something running" reads `activity`/`state`, not
this. Same lifecycle as `say`: absent for shells and until the session's first
tool call, readers tolerate both forms and round-trip fields they do not know.

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

### `song/stage/herald.json` — **v0**

The notification ledger the Quickshell herald draws from. dunst owns
`org.freedesktop.Notifications` but draws NOTHING (`skip_display` on every
rule); it hands each notification to `aoide herald push` through its `script`
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
prompt, published by `aoide graph permit` rather than by dunst; it carries the
waiting `sessionId` and is drawn with real approve/deny buttons whose verdict
routes back through the shellbridge to `graph send`, the one gated injection
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

### `state/usage.json` — **v0**

Account/usage runtime — lives in the gitignored root-runtime `state/` dir
(§2), NOT `song/stage/`: this is account/global state, not song-scoped or
rehearsal state. **State-dir resolution** mirrors the stage-dir seam above:
`$AOIDE_STATE_DIR` when set to an **absolute** path, else `~/Aoide/state`
derived from `$AOIDE_USER`/`$HOME`. Written by `aoide usage` (`aoide.usage.*`,
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

### `state/a2a-agents.json` — **v0**

The client-side registry of EXTERNAL A2A agents this aoide has registered by
AgentCard URL (`aoide a2a agent add`; §6, client side). Lives in the same
gitignored root-runtime `state/` dir as `usage.json` (state-dir resolution as
above), NOT `song/stage/`. Written atomically by `aoide a2a agent add/remove`;
read by `aoide a2a agent list/send` and folded into the session DAG
(`graph.json`) as `kind:"a2a"` nodes. Keyed by the `name` from the fetched
card — re-adding the same name replaces in place (dedupe). Each entry's `url`
is the RESOLVED `message/send` endpoint (the card's own `url`/first-interface
url, else the origin of the fetched card URL), i.e. what `agent send` POSTs to,
not the card URL. **Additive / tolerate-missing:** an absent file is simply "no
agents registered" (never an error), and readers round-trip fields they do not
know.

```json
{
  "schemaVersion": "0",
  "agents": [
    { "name": "peer", "url": "http://10.0.0.5:8710/", "description": "…", "registeredAt": "2026-08-01T12:00:00Z" }
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

Naming no song performs song `"sonata"` — the shipped standard
(`song/songbook/sonata/rice.nix`), the guaranteed-present baseline.
**Renamed (2026-08-14):** the shipped standard song was `default`;
`song/songbook/default/` is now retired outright (its Pantheon design
grammar relocated to
`docs/Aoide-Wiki/references/pantheon/pantheon-grammar.md`, its recorded
aesthetic distilled into `song/songbook/learnings.md` — the rest is
git-recoverable history, not deleted knowledge). No shape change, no version
bump: `aoide.song`'s default and every "shipped baseline" reference in this
section simply name `sonata` now. Full dated entry:
`docs/Aoide-Wiki/ingest/log.md`.

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
  preview possible. The manifest shape is unchanged (still
  `{ "<song>": ["<slot>", …] }`) by generalizing the walk from a fixed slot
  enum to "every file under `widgets/`" — additive, no contract-version
  bump.
- **Runtime resolution:** `LiveryState.qml`'s `songName` property (above)
  names the active song; the staging engine (`StagingEngine.qml`) reads the manifest and answers
  "does `<song>` dress `<slot>`"; `WidgetSlot.qml` is the fixed per-slot
  anchor a host surface embeds — it loads the song's file when authored, else
  falls back to shared chrome (or renders nothing, when no fallback exists).
  `aoide rice preview <name>` (§4) drives this live, no rebuild: it stages
  `song` into `livery.json`, `LiveryState`'s `songName` updates, and every
  `WidgetSlot` re-resolves. **Additive (2026-08-15) — widget bodies ride the
  same call:** `rice stage`/`preview` also syncs the song's whole
  `widgets/` tree into `run/qml/songs/<name>/` (`crate::widgets` in
  `crates/song/`, byte-compared so an unchanged file is never rewritten —
  avoids flicker/reload of every widget on a palette-only stage) and
  regenerates that song's `run/qml/songs/manifest.json` entry, so an edit
  to an EXISTING widget file reaches Quickshell's own file-watcher live too.
  A brand-new slot file still needs a service restart to be discovered
  (the manifest is only read at startup) — **may no longer require a
  restart** now that `aoide quickshell reload` (Quickshell IPC hot-reload
  trigger) rebuilds the whole scene fresh from `shell.qml`, which should
  also re-read `manifest.json`; unconfirmed against a live instance, don't
  rely on this until verified.
- **Fixed injected-prop contract:** a loaded widget receives `notes`
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
**sonata**'s (mirrors `aoide.song`'s own default — the shipped baseline
every song can fall back to), else `""` (the anchor's own facet-side
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
stage`/`preview` hot-syncs one song's entry live, no rebuild
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
(`notes` + `bridge` only — no `shared`, deliberately, so a declared widget
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
stage`/`preview` hot-sync → `SongSurfaces.qml` render — end to end. No
committed song declares a `kind = "dock"` entry yet.

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
| Task (one unit of work) | a *turn* — what `graph send` injects into a session |
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
  reuses [`crate::graph::session_send`] (the same gated door `graph send`
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
like `graph send`'s pending/`--yes`/autogate dance — a JSON-RPC request
cannot block mid-flight on a human clicking "approve". In its place: the
spawn target is fixed at rebuild time (never client-chosen), the door is
loopback/user-scoped by default (same as the rest of §6's security posture),
and every inject/spawn/error is audited through `Door::A2a`, the same single
audit log every other door writes.

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
  cross-device analogue of `graph send`'s local "sender is the target's own
  parent" autogate rule. An unmarked/unknown remote sender is held
  **pending**, reusing `graph send`'s EXISTING `pending.json` queue
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

### Session-DAG integration (client side)

An external A2A agent, once registered (`aoide a2a agent add <url>`), folds
into the session DAG as a node of `kind: "a2a"` (mirroring
`graph/model.rs`'s `SessionRecord.kind` tag) — `build_graph` reads
`state/a2a-agents.json` (§4) and emits, per agent, a ROOT node
`{ id: "a2a:<name>", kind: "a2a", name, url, state: "idle" }` (plus
`description` when non-empty). No edges (an a2a agent anchors to nothing), so
the existing `spawned`/`anchors` machinery is untouched; a missing/empty
registry folds nothing (additive). The node is keyed by the `name` from its
fetched AgentCard — the same handle `aoide a2a agent remove <name>` takes.

`aoide a2a agent add <url>` accepts either a full
`…/.well-known/agent-card.json` URL or a bare origin (the well-known path is
appended); it curl-GETs the card, requires at least `name`, keeps
`description`, and records the card's own `url`/first-interface url (else the
fetch origin) as the endpoint. `aoide a2a agent send <name> "<message>"` is
the **drive verb** (the outbound half of bidirectional A2A): it POSTs a
JSON-RPC `message/send` to that endpoint and reports the returned
Task/Message. These external calls are **unauthenticated** for the MVP (no
`securityScheme` handling yet) and loopback/LAN-oriented, consistent with §6's
security posture; the message text is untrusted data, never executed.

### Status

The option surface (`aoide.a2a.enable`/`bindAddress`/`port`/`spawnAgent`), the
`kind:"a2a"` DAG fold, and the `a2a serve` command are **real**: the
AgentCard, `tasks/get`, and `message/send` (Phase B2: inject-or-spawn
execution, above) all run. The CLIENT side is now **real** too (Phase D):
`a2a agent add|list|remove` maintain the `state/a2a-agents.json` registry
(§4) and `a2a agent send` drives a registered agent. This section is
**additive**: it introduces a new contract, carries no version bump to §1–§5,
and needs no playbook migration entry (nothing existing changed shape).

---

## 7. Peer federation door — **v0** (2026-08-14)

Aoide-to-aoide federation: one aoide instance can register ANOTHER aoide
instance as a **peer** by URL and pull its resolved session graph into its
own, folded in as a subtree. Built on §6's existing A2A door — ONE new
JSON-RPC method (`aoide/graphSummary`), a client-side peer registry +
per-peer pull cache, and an additive fold in the same `build_graph`
function §6's `kind:"a2a"` fold already uses.

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
`state/` dir (§2, state-dir resolution as in §4's `state/a2a-agents.json`
entry) — **not** `song/stage/`, a deliberate divergence from an earlier
draft of this contract that sketched `song/stage/peers.json`: a peer roster
is account/global external-registry state, exactly like the sibling
`state/a2a-agents.json` (§4) it mirrors byte-for-byte in shape/discipline,
not song-scoped rehearsal state. Written atomically
(`aoide_storage::peer_store`); **additive/tolerate-missing** — an absent
file is simply "no peers registered", never an error; readers round-trip
fields they do not know.

```json
{
  "schemaVersion": "0",
  "peers": [
    { "name": "yomi-strix", "url": "http://yomi-strix:8710/", "autogate": false, "addedAt": "2026-08-14T00:00:00Z" }
  ]
}
```

`autogate` (bool, default `false`) is the cross-device analogue of `graph
send`'s local "sender is the target's own parent" rule (§6's amendment
above): a peer marked `true` here has its INBOUND `message/send` auto-deliver
without the pending queue, even though its connection is non-loopback. An
unmarked/unknown sender is never autogated.

`aoide peer add <name> <url> [--autogate]` verifies the peer FIRST — fetches
its `/.well-known/agent-card.json` (mirroring `a2a agent add`'s
verification-before-registering pattern exactly) — and only registers on
success; a peer that fails the fetch is never added. Unlike `a2a agent
add`'s upsert-replace-on-readd, **a duplicate `name` is rejected cleanly**
(CONTRACTS.md's own judgment-call divergence: a peer's local nickname should
never be silently repointed at a different URL by a second `add`). `aoide
peer remove <name>` deregisters; a **missing name is an error**, not
idempotent-silent — following `rice draft drop <name>`'s precedent (§4) over
`a2a agent remove`'s tolerate-missing stance, a deliberate choice called out
here since the two existing verbs this one could have mirrored disagree.
`aoide peer list` enumerates the registry.

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
`url` field carries). `graph` is EXACTLY what `aoide graph view --json` /
`graph emit` resolve (`aoide_conduct::graph::resolve_graph_document`, the
SAME function both those verbs and this method call) — no second graph
vocabulary is invented for the wire.

### The `peer:*` node-id convention (graph fold)

`build_graph` (`aoide-conduct::graph::doc`, the same function that already
folds registered §6 `kind:"a2a"` agents in as opaque root nodes) ADDITIVELY
folds each registered peer in as a root node, one level richer than the a2a
fold: `{ id: "peer:<name>", kind: "peer", name, url, state, children? }`.

- A **fresh** cache (see the TTL rule above) contributes `state: "fresh"`
  plus `children: { nodes, graph's edges }` — the peer's OWN
  already-resolved subtree, nested VERBATIM, never flattened into this
  document's own top-level `nodes`/`edges` (unlike the a2a fold's one opaque
  node, this folds in a peer's whole graph one level richer — so a peer's
  ids can never collide with a local id or another peer's).
- A **stale or never-pulled** peer still surfaces immediately (visible the
  moment `peer add` runs, before any pull ever succeeds) with `state:
  "stale"` and NO `children` — never a crash, never a silently-dropped peer.
  `error` carries the last pull failure's reason when present.

Local graph verbs (`graph focus`/`prune`/`reap`/`link`) keep ignoring
`peer:*` ids exactly as they already ignore `a2a:*` ids today — confirmed by
test (`conduct::graph::verbs::tests::local_only_verbs_ignore_peer_ids_exactly_like_a2a_ids_today`),
not just assumed to generalize: none of those verbs read `peer_store` (or
`a2a_store`) at all, they operate purely on `sessions.json`'s
`SessionRecord`s, so a `peer:*`/`a2a:*` id is simply never a session id they
could ever match.

### CLI surface

`aoide peer add <name> <url> [--autogate]` / `list` / `remove <name>` / `pull
[<name>]` / `status` — registered as their own command group, directly after
`a2a agent add/list/remove/send` in `schema --json`'s order (nothing
existing reorders). `peer pull` with no name pulls EVERY registered peer;
with a name, just that one.

### Status

Real: the registry, the cache, `aoide/graphSummary`, the CLI verbs, and the
graph fold all run. **Out of scope for v0** (explicitly, not an oversight):
WAN/NAT-traversal/relay reachability for peers not on the same network;
Melete-side consumption (a polling Rune skill, first-class `graph_view`
rendering) — both are later, separately-directed work. This section is
**additive**: it introduces `state/peers.json` + `state/peer-cache/`, the
`aoide/graphSummary` method, and the `peer:*` node-id convention, and amends
§6's `message/send` gating behavior (dated above) — no version bump to
§1–§6, no playbook migration entry (nothing existing changed shape beyond
the called-out §6 amendment).

---

## 8. Screen capture sidecar + pointer synthesis — **v0**

The `aoide screen` family (`pkgs/aoide/crates/conduct/src/screen/`) writes a
JSON sidecar (`<capture>.json`, same stem as the image) next to every
`screen shot`/`screen diff` capture. `screen point` has nine verbs, six of
which synthesize real pointer input against a native Wayland backend
(`idle`/`save` are read-only queries, `restore` warps via `hyprctl` instead
of synthesizing). This section is the sidecar field contract, the
image↔screen scale contract, and the
`*-*` reason-code vocabulary every `screen` verb's structured error draws
from. See `concepts/orchestration/Screen-Control` in the wiki for the
verb-by-verb usage this contract backs.

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

Every `screen` verb's structured error carries a backend-agnostic
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
`screen::synth` — no shell-out. Every `screen point` verb assembles a `Seq`
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
retired from `modules/dendrites/vision.nix` — `screen::point`'s verbs
all cross the pointer-synthesis boundary through `screen::synth`
in-process, and nothing else speaks for the pointer.

**Confirmed at the protocol level** the same day, with a `wl_pointer`
event logger (`wev`) as the receiving client — the events an app actually
gets, not pixels inferred from them:

| Verb | What the client received |
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
- **One full `aoide screen point click <button>` clears it.** Only the
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
