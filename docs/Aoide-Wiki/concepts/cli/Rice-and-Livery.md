---
type: concept
created: 2026-08-19
updated: 2026-08-27
tags: [aoide, cli, rice, livery, song]
---

# Rice & Livery Commands — the Self-Ricing Command Surface

The `rice`, `cover`, and `livery` command groups are part of `lyra`, the AoideOS
paint binary ([[Overview]]), and drive the [[Self-Ricing]] loop:
scaffold a song ([[Song-Anatomy]]), hot-load it live, iterate inside a routed
draft with take history, and validate/resolve/emit its notes through the
native [[livery]] engine. Handlers live in
`pkgs/aoide/crates/song/src/commands/{rice,draft,mode,cover,livery,take}.rs`;
`rice declare`'s handler and the `rice transpose` stub registration live in
`pkgs/aoide/crates/lyra/src/commands/stubs.rs`.

Path resolution (all in `pkgs/aoide/crates/storage/src/fs.rs`): one runtime
root carries every runtime tree — `$AOIDE_ROOT` when set to an absolute
path (relative/empty ignored), default `~/.aoide` — with `song/stage/`,
`song/songbook/<name>/` (and `song/songbook/<song>/drafts/`),
`song/covers/`, `state/`, and `run/qml/` all hanging off it. `~/Aoide` is
purely the dev git checkout, reached through the separate `$AOIDE_FLAKE_ROOT`
seam (absolute, default `~/Aoide`); the nix options `aoide.root`/
`aoide.checkout` export the two envs. `$AOIDE_STAGE_DIR` (absolute) wins
above `$AOIDE_ROOT` for the stage dir; `song/` derives as the stage dir's
parent, so the whole song tree rides that one override, and `run/qml/`
resolves one level further up (the stage dir's parent's parent —
`$AOIDE_ROOT/run/qml/` on the default layout, relocated by an
`$AOIDE_STAGE_DIR` override too). `$AOIDE_STATE_DIR` (absolute) likewise
overrides `state/`. All three binaries call `fs::migrate_root_once()` at
process start: on first run it moves a pre-existing
`~/Aoide/{song/stage,state,log}` tree into the current root, each piece
gated on its own override being unset. All file
writes in this group route through `aoide_storage::fs::atomic_write`
(temp `<stem>.tmp.<pid>` then rename, symlink-transparent — a write to a
symlinked `stage/livery.json` lands in the link target). Shared stage
mutators serialise through a `.stage.lock` `flock` in the stage dir
(`with_stage_lock`, which nests for the thread that already holds it).

Every command takes `--json`. Without it the CLI prints the human `message`
line; with it, an envelope `{status, command, message, gated, changed?, data?}`
(`pkgs/aoide/crates/protocol/src/output.rs`). Exit codes: 0 ok, 1 error,
2 usage, 64 not-implemented. Exception: the three `livery` commands print the
engine's raw bytes (carried in `data.stdout`) instead of the message in text
mode — see `livery lint` below. Names (`<name>`, `<song>`, draft names)
everywhere must match `^[a-z0-9][a-z0-9-]*$`.

### lyra rice lint

```
lyra rice lint [<name>|<path>] [--json]
```

- **Reads:** the target livery file — `<path>` literally if it exists as a
  file, else `song/songbook/<name>/livery.json`, else (no arg)
  `song/stage/livery.json`. Validates in-process with the native livery
  engine (`pkgs/aoide/crates/song/src/lint.rs` → `livery::lint`); no external
  binary.
- **Output:** ok → `"livery schema validation passed"`, data
  `{livery, schemaVersion: "0", engine: "livery"}`. Failure → exit 1, data
  `{livery, errors: [...]}` (a missing/unreadable file or invalid JSON is
  also a lint failure, exit 1). No staged livery and no arg → usage, exit 2,
  `reason: "no-staged-livery"`.
- **Notes:** read-only; not gated. Schema failures are `status: "error"`,
  exit 1 — never an `ok` with errors embedded.

### lyra rice stage

```
lyra rice stage [<name>] [--json]
```

- **Reads:** `song/stage/mode.json` (entrypoint guard), then the notes for
  `<name>` (must parse as JSON; full schema validation is `rice lint`'s job)
  — the DECLARED twin `song/declared/livery.json` when its own `"song"` field
  equals `<name>`, else that song's committed
  `song/songbook/<name>/livery.json` (CONTRACTS.md §4). No `<name>`:
  re-resolves the current song from the staged livery's own `"song"` field
  (`song/stage/livery.json`).
  Env: `$HYPRLAND_INSTANCE_SIGNATURE` (guards the compositor apply),
  `$AOIDE_SESSION_ID` (stamped on an auto-take), `$AOIDE_STAGE_DIR`.
- **Writes:** `song/stage/livery.json` (atomic, symlink-transparent — in
  Draft mode the write lands in the routed draft file; the `"song"` field is
  injected into the staged copy); `song/stage/cover.json` (`{"path": …}`,
  only when a cover is derivable as `song/covers/<name>.{webp,png,jpg,jpeg}`
  — otherwise left untouched); widget QML bodies synced from
  `song/songbook/<name>/widgets/*.qml` into `run/qml/songs/<name>/`;
  `run/qml/songs/registry.json` rewritten for the song from the livery's
  `.widgets` key. In Staging mode with an explicit `<name>`, also updates
  `song/stage/mode.json`'s `song` (in Draft mode the marker is deliberately
  untouched). In Draft mode, auto-mints a take (cause `"stage"`): writes
  `song/songbook/<song>/drafts/<draft>/takes/NNNN.json` and
  `takes/head.json` — non-fatal on failure (`take: null, takeError` in data).
- **Pipes to / output:** best-effort `hyprctl --batch "keyword …; …"` with
  geometry + border keywords (`general:gaps_out`, `general:gaps_in`,
  `general:border_size`, `general:col.active_border`,
  `general:col.inactive_border`, `decoration:rounding`,
  `decoration:blur:enabled|size|passes`) — keyword-only, never
  `hyprctl reload`, skipped silently off Hyprland, never fatal
  (`pkgs/aoide/crates/song/src/live.rs`). If the widget-body sync changed
  anything, spawns `quickshell -p <run_qml_dir>/shell.qml ipc call shell
  reload` (best-effort; success judged on empty stdout, not the exit code —
  `pkgs/aoide/crates/song/src/ipc.rs`). Data keys: `{name, livery, cover,
  hyprctl, widgets, slots, registry, reload, seam}`.
- **Notes:** refuses with `reason: "declarative-mode-locked"` (exit 1) while
  `rice mode declarative` is locked. Always stages plain declared content —
  never auto-loads a draft. Nothing is committed. Re-staging identical
  content still writes (and still mints an auto-take in Draft mode).

### lyra rice compose

```
lyra rice compose <name> [--from <song>] [--force] [--json]
```

- **Reads:** `--from`'s notes (`--from` default `"sonata"`; must exist and
  parse as JSON), resolved two tiers by `resolve_from_notes_path`
  (`pkgs/aoide/crates/song/src/commands/rice.rs`): the host songbook's
  `song/songbook/<from>/livery.json` under the runtime root FIRST, else
  `<templates>/<from>/livery.json` in the shipped score templates —
  `pkgs/lyra-songbook`, a verbatim copy of the committed `song/songbook/`
  tree plus prebaked `manifest.json`/`registry.json` baked by
  `lib/songbook.nix` (the same generator the checkout-host `nix eval` path
  calls) into `share/lyra/songbook`. The templates dir itself resolves two
  tiers, absolute-path-wins: `$AOIDE_SONG_TEMPLATES`, else a sibling of the
  binary's own directory (`<exe_dir>/../share/lyra/songbook`), gated on that
  directory existing.
- **Writes:** exactly four files under the runtime root's
  `song/songbook/<name>/` (never the templates dir — compose TO always
  lands in the host songbook), all atomic:
  `rice.nix` (self-gating skeleton copying `--from`'s palette/window/geometry
  into `aoide.livery.*` under `config.aoide.song == "<name>"`),
  `livery.json` (verbatim mirror of `--from`'s), `design/intent.md`
  (honest-empty template), `widgets/.gitkeep`.
- **Output:** data `{name, from, nextSteps: [...]}`.
- **Notes:** errors (exit 1) on invalid name (`invalid-name`), invalid
  `--from` (`invalid-from`), `--from == <name>` (`from-equals-name`), target
  exists without `--force` (`already-exists`), missing `--from` song
  (`from-song-not-found` — not found in the host songbook NOR the templates
  dir; the message names both locations it checked, and says so when no
  templates dir resolved at all). Writes stay inside `song/songbook/<name>/`
  (house rule 1). Nothing live is touched; go live with `rice mode stage
  <name>`. The manifest/registry regen that `rice stage`'s widget sync runs
  (`aoide-song::widgets::eval_songbook`) never shells to `nix` on a
  repo-less host (no `flake.nix` at `$AOIDE_FLAKE_ROOT`): it merges the
  baked templates baseline, overlays the existing on-disk file's entries for
  songs still present in the host songbook (gone dirs pruned), and patches
  the currently-staged song's own freshly-scanned entry in last — a scan
  that only ever produces the compose shape, no `_widgets/` shelf. A song
  with a `_widgets/` shelf still needs a real checkout: resolving a shelf
  takes `composeSong` in the nix evaluator, which the templates fallback
  cannot run. `$AOIDE_SONG_TEMPLATES` is wired only onto lyra-execing units
  (`modules/nucleus/shellbridge.nix`'s shellbridge service) plus
  `modules/nucleus/aoided.nix`'s `environment.sessionVariables` (gated on
  `aoide.lyra.enable`) — core-only units don't carry it.

### lyra rice draft save

```
lyra rice draft save <name> [--json]
```

- **Reads:** `song/stage/livery.json` (required — must parse and carry a
  `"song"` field; reads transparently through a Draft-mode symlink), and
  `song/stage/cover.json` if present.
- **Writes:** `song/songbook/<song>/drafts/<name>/livery.json` (atomic copy
  of the current stage) and, if the stage has one,
  `…/drafts/<name>/cover.json`; removes a stale draft `cover.json` when the
  stage no longer has one (the draft mirrors the stage exactly at save time).
- **Output:** data `{name, song, livery, cover}`.
- **Notes:** upserts; works in any mode — never gated by the declarative
  lock, never touches `mode.json`. Errors: `no-staged-livery`,
  `invalid-json`, `no-resolvable-song`, `invalid-name` (exit 1; usage exit 2
  when `<name>` is missing).

### lyra rice draft list

```
lyra rice draft list [<song>] [--json]
```

- **Reads:** with `<song>`, `song/songbook/<song>/drafts/*/`; without, walks
  every song under `song/songbook/`. Only directories containing a
  `livery.json` count (others silently skipped). Also reads
  `song/stage/mode.json` to flag the currently-routed draft.
- **Output:** `"N draft(s)"`; data `{drafts: [{song, name, savedAt,
  current}], scope}` — `savedAt` is the draft `livery.json`'s mtime as
  ISO-8601 UTC; `current` is true iff `mode.json` is in Draft mode routed to
  that draft.
- **Notes:** read-only. An empty scope is `ok` with an empty list, never an
  error.

### lyra rice draft drop

```
lyra rice draft drop <name> [--json]
```

- **Reads:** `song/stage/livery.json`'s `"song"` field (to resolve which
  song's `drafts/` to act under) and `song/stage/mode.json`.
- **Writes:** deletes `song/songbook/<song>/drafts/<name>/` recursively.
- **Output:** `"dropped draft `<name>` for `<song>`"`; data `{name, song}`.
- **Notes:** NOT idempotent — a missing draft is an error
  (`draft-not-found`, exit 1). Refuses with `draft-is-live` when it is the
  draft `rice mode draft` currently routes the stage into; switch modes
  first.

### lyra rice mode status

```
lyra rice mode status [--json]
```

- **Reads:** `song/stage/mode.json` only.
- **Output:** `"rice mode: <staging|declarative|draft>"`; data `{mode, song,
  draft, since}`. An absent or corrupt marker reports the `declarative`
  default (`song`/`draft`/`since` null) — never an error.
- **Notes:** read-only. Marker shape (`pkgs/aoide/crates/storage/src/mode.rs`):
  `{mode, song?, draft?, stagingSong?, since}`; `draft` is non-null iff
  `mode == "draft"`. `stagingSong` is the durable "what was I staging" memory
  a bare `rice mode stage` falls back to.

### lyra rice mode stage

```
lyra rice mode stage [<name>] [--json]
```

- **Reads:** `song/stage/mode.json`; for a bare call, resolves the song from
  the marker's `stagingSong` first, then `song/stage/livery.json`'s `"song"`
  field. Also enumerates `/proc` for the stray-process sweep.
- **Writes:** tears down any Draft-mode routing symlink at
  `song/stage/livery.json` first, then — when a song resolves — everything
  `rice stage` writes (stage livery/cover, widget sync, registry sync, live
  `hyprctl`/Quickshell IPC); finally `song/stage/mode.json`
  (`mode: "staging"`, `song`, `stagingSong`, `since`).
- **Output:** data `{mode: "staging", song, reaped: [{pid, reason,
  cmdline}]}`.
- **Notes:** unlocks staging writers AND hot-loads immediately — never a
  bare flag-flip except on a genuinely fresh box with no resolvable song.
  This is also how you leave Draft mode. A failed stage (unknown song) does
  not flip the marker. The stray-process sweep (leftover preview harnesses,
  duplicate `shell.qml`, stale `hyprlock` — `pkgs/aoide/crates/song/src/reap.rs`)
  is best-effort, never fatal.

### lyra rice mode declarative

```
lyra rice mode declarative [<name>] [--json]
```

- **Reads:** `song/stage/mode.json`; with no `<name>`, resolves the declared
  song off `song/declared/livery.json`'s `"song"` field, falling back to
  `song/stage/livery.json`'s own `"song"` field only when no such twin exists
  (CONTRACTS.md §4).
- **Writes:** tears down any Draft-mode routing symlink; when a song
  resolves, re-pins `song/stage/livery.json` (plus cover/widget/registry
  sync, same as `rice stage`) from that song's COMMITTED
  `song/songbook/<name>/livery.json` — or, for the song the declared twin
  names, from the twin, venue recolour included; then writes
  `song/stage/mode.json`
  (`mode: "declarative"`, `draft` cleared; `stagingSong` carried forward
  unchanged).
- **Output:** data `{mode: "declarative", song}`.
- **Notes:** locks staging — afterwards `rice stage` and `cover set` refuse
  with `declarative-mode-locked`. The no-name form re-pins the declared song
  from its own notes rather than freezing the stage: unsaved live edits are
  discarded
  (`rice draft save` first to keep them). A failed re-pin does not flip the
  marker. Locking while already locked with nothing resolvable is a no-op
  `ok`.

### lyra rice mode draft

```
lyra rice mode draft <name> [--json]
```

- **Reads:** `song/stage/mode.json` (guard + marker carry-over);
  `song/stage/livery.json`'s `"song"` field to resolve `<song>`.
- **Writes:** if `song/songbook/<song>/drafts/<name>/livery.json` doesn't
  exist yet, forks it from the current stage (the same write `rice draft
  save` performs, incl. `cover.json` mirror). Then removes whatever sits at
  `song/stage/livery.json` and creates it as a SYMLINK to
  `song/songbook/<song>/drafts/<name>/livery.json`. Finally writes
  `song/stage/mode.json` (`mode: "draft"`, `song`, `draft`, `since`).
- **Output:** data `{mode: "draft", song, draft}`.
- **Notes:** refuses while declarative-locked (`declarative-mode-locked`) and
  when no song is staged (`no-resolvable-song`). From here on every write to
  the stage file — `rice stage`, a hand-edit — lands directly in the draft
  via `atomic_write`'s symlink transparency; no save step. Only
  `livery.json` is routed — `cover set`/`stage/cover.json` are not. Drafts
  sit under the runtime root, outside the git checkout entirely; a
  `rice declare` copy lands them in the checkout, where `.gitignore`
  (`song/songbook/*/drafts/`) keeps them untracked and `lib/checks.nix`
  `noSongRead` bans nix-eval reads of them.

### lyra cover set

```
lyra cover set <path|name> [--json]
```

- **Reads:** `song/stage/mode.json` (entrypoint guard); stats the resolved
  cover file (absolute `<path>` literal; a bare name resolves against
  `song/covers/`). In Draft mode, also reads the staged livery/cover for the
  auto-take.
- **Writes:** `song/stage/cover.json` — atomic, pretty `{"path": "<abs>"}` +
  trailing newline. NOT symlink-routed in Draft mode (only `livery.json`
  is). In Draft mode auto-mints a take (cause `"cover-set"`): `takes/NNNN.json`
  + `takes/head.json` under the routed draft, non-fatal on failure.
- **Output:** data `{cover, coverJson, seam}` (+ `take` when in Draft mode).
  The Quickshell wallpaper surface (`AoideWallpaper.qml`) FileView-watches
  `stage/cover.json` and hot-swaps live — no IPC call.
- **Notes:** refuses while declarative-locked (`declarative-mode-locked`).
  A path naming no existing file is `cover-not-found`, exit 1 — never stages
  a wallpaper that can't render. Nothing is committed; the baked
  `AOIDE_WALLPAPER` remains the boot/rebuild fallback.

### lyra livery emit

```
lyra livery emit <target> [<name>|<path>] [--out PATH] [--template STR] [--json]
```

- **Reads:** the livery file (`<path>` literal if it exists, else
  `song/songbook/<name>/livery.json`, else `song/stage/livery.json` — the
  name/path is args[1], after `<target>`). Validates (closed v0 schema) and
  resolves (aliases deref'd, component fallbacks applied) in-process.
- **Writes:** with `--out PATH`, the emitted bytes are atomic-written to PATH
  instead of printed.
- **Output:** TEXT MODE PRINTS RAW BYTES (from `data.stdout`), not the
  message line — lyra special-cases the `livery` group
  (`pkgs/aoide/crates/lyra/src/lib.rs`). Per target
  (`pkgs/aoide/crates/song/src/livery/emit/`):
  - `stage` — pretty stage JSON: `{schemaVersion, palette, bar, notif,
    window, base16?}`, component fallbacks applied, never null.
  - `hyprctl` — two shell-quoted argv lines, one per line:
    `hyprctl keyword general:col.active_border rgb(rrggbb)` then
    `general:col.inactive_border`.
  - `osc` — raw terminal sequences `ESC ] Ps ; rgb:rrrr/gggg/bbbb BEL`
    concatenated: OSC 11 (bg), 10 (fg), 12 (cursor/accent), then OSC 4 slots
    0/4/1/7 (bg/accent/urgent/fg).
  - `file` — renders `--template`, substituting `{{group.key}}` placeholders
    (group ∈ palette, base16, bar, notif, window); requires `--template`.
  - `--out` prints the compact envelope `{"ok":true,"wrote":"<path>"}`.
  `--json` keeps the full Outcome envelope with `data.stdout` holding the
  bytes. Errors: usage exit 2 on missing target (`missing-target`) or unknown
  target (`unknown-target`); exit 1 on validation failure, `resolve-failed`,
  `emit-failed` (incl. malformed/unknown template placeholders), or
  `write-failed`.
- **Notes:** pure emit only — these backends produce bytes; they never apply
  anything to the host (no `hyprctl` is spawned here; live apply is
  `rice stage`'s `live::apply_live`). osc/hyprctl outputs are golden-tested
  byte-identical against the Node-engine fixtures.

### lyra livery resolve

```
lyra livery resolve [<name>|<path>] [--json]
```

- **Reads:** the livery file (same resolution rule as `livery emit`).
- **Output:** raw bytes in text mode: the resolved JSON — insertion order
  `schemaVersion, palette, [base16], bar, notif, window`, flat
  fully-resolved values (aliases deref'd, cycle-guarded; component
  null/empty → palette fallback). Data `{livery, stdout}`. Validation
  failure → exit 1 with `{ok: false, errors, livery, stdout}`; a resolution
  error → exit 1, `reason: "resolve-failed"`.
- **Notes:** read-only. Byte-identical to the Node engine's
  `JSON.stringify(resolved, null, 2)`.

### lyra livery lint

```
lyra livery lint [<name>|<path>] [--json]
```

- **Reads:** the livery file (same resolution rule).
- **Output:** raw compact envelope in text mode: `{"ok":true,
  "schemaVersion":"0"}` or `{"ok":false,"errors":[…]}` (+ newline). Data
  `{ok, schemaVersion, livery, stdout}` on success; `{ok: false, errors,
  livery, stdout}` on failure (exit 1). Unreadable file / invalid JSON →
  exit 1 with the Node engine's message shape (`cannot read <file>: …` /
  `invalid JSON in <file>: …`).
- **Notes:** read-only; validates against the closed v0 schema
  (`pkgs/aoide/crates/song/src/livery/schema.rs`). `rice lint` wraps the same
  engine but renders the standard Outcome message instead of the raw
  envelope.

### lyra rice declare

```
lyra rice declare <name> [--json]
```

- **Reads:** the composed song `$AOIDE_ROOT/song/songbook/<name>/`
  (`fs::songbook_dir`), recursively, byte by byte; and the checkout at
  `aoide_storage::fs::flake_root()` (`$AOIDE_FLAKE_ROOT` absolute override,
  default `~/Aoide`), which must exist as a directory.
- **Writes:** copies the song tree into the checkout's
  `song/songbook/<name>/` (`pkgs/aoide/crates/lyra/src/commands/stubs.rs`
  `handle_rice_declare` — `implemented: true`, `gated: true`). Byte-diff
  copy: a destination file whose bytes already match the source is left
  untouched, so a repeat declare with nothing new is a no-op. Nothing beyond
  the copy — no `git add`, no rebuild proposal, no `nix eval`; committing
  and gating the rebuild stay the User's own steps ([[Rebuild-Gate]]): the
  agent proposes, never admits.
- **Output:** `"copied N file(s) into <dst>"`, or `"`<name>` already
  matches <dst> — nothing to copy"` on a byte-identical re-declare; data
  `{name, checkout}`, plus the changed-file list on the envelope's
  `changed` field.
- **Notes:** `<name>` is validated against `^[a-z0-9][a-z0-9-]*$` BEFORE
  either path is built — a traversal-shaped name is refused by construction.
  Errors: usage exit 2 on missing `<name>` (`missing-name`); exit 1 on
  `invalid-name`, `no-composed-song` (nothing at the source — the message
  says to run `lyra rice compose <name>` first), `no-checkout` (no checkout
  at `$AOIDE_FLAKE_ROOT`), `copy-failed`.

### lyra rice transpose

```
lyra rice transpose <rice> <palette> [--json]
```

- **Notes:** STUB — `implemented: false` (not gated): `dispatch()`
  short-circuits before any handler, returning the not-implemented envelope
  and exit 64 — lyra's only remaining stub. Contract per the schema summary:
  replay song `<rice>` in another key (palette) from the song's
  `song/songbook/<rice>/palette/` directory. That palette directory layout is
  asserted only by the schema summary — unverified elsewhere in source.

### lyra rice take

```
lyra rice take [--json]
```

- **Reads:** `song/stage/mode.json` (must be Draft mode — `resolve_draft`),
  the routed draft's current `song/stage/livery.json` (required) and
  `song/stage/cover.json` (optional). Env: `$AOIDE_SESSION_ID` (recorded as
  `sessionId` when set).
- **Writes:** `song/songbook/<song>/drafts/<draft>/takes/NNNN.json` (a
  write-once `TakeRecord`: `{take, parent?, at, sessionId?, cause, livery,
  cover?}` — 4-digit zero-padded filename, monotone counter, never
  renumbered) and updates `takes/head.json` (`{"head": N}`). One
  `with_stage_lock` around the whole read-allocate-write.
- **Output:** `"take NNNN minted — from take MMMM"` (or "the draft's first
  take"); data `{take, parent, at, cause: "explicit", sessionId}`.
- **Notes:** Draft mode only — outside it, exit 1 with
  `reason: "not-in-draft-mode"`. A bare mint of whatever is staged, hanging
  off the current head; no selection, no comparison, no revert. The same
  snapshot core fires automatically (non-fatally) on every successful
  Draft-mode `rice stage` (cause `"stage"`) and `cover set`
  (cause `"cover-set"`).

### lyra rice take list

```
lyra rice take list [--json]
```

- **Reads:** the routed draft's whole `takes/` directory (`NNNN.json`
  records, `head.json`, `marks.json`) plus `song/stage/mode.json`.
- **Output:** human text is an ASCII tree: `"N take(s) for <song>/<draft> —
  head → MMMM"` then depth-first rows `NNNN  <ISO at>  <cause> [marks] [←
  head]`, drawing `├─`/`└─` only at actual forks; takes unreachable from a
  root (dangling parent or a parent cycle) render as top-level `detached`
  entries. `--json` data: `{song, draft, head, takes: [{take, parent, mark,
  at, sessionId, cause}]}` (`mark` is a comma-joined string of letters or
  null).
- **Notes:** read-only, no lock. Draft mode only (`not-in-draft-mode`). An
  empty store is `ok` with an empty `takes` array. There is exactly one view
  (whole tree); no `--all` flag exists.

### lyra rice take mark

```
lyra rice take mark <letter> [--take N] [--json]
```

- **Reads:** `song/stage/mode.json` (Draft-mode gate), the target take file
  (existence check), `takes/marks.json` (tolerates missing/corrupt as empty),
  and `takes/head.json` when `--take` is omitted (defaults to the head).
- **Writes:** exactly one atomic rewrite of
  `song/songbook/<song>/drafts/<draft>/takes/marks.json`
  (`{"<letter>": <take>}` map) — take records are never touched. Under one
  `with_stage_lock`.
- **Output:** `"mark X stamped on take NNNN"` or `"mark X moved from take
  MMMM to take NNNN"`; data `{mark, take, moved, from}`.
- **Notes:** `<letter>` is a single uppercase A-Z, rejected otherwise
  (`invalid-mark`; lowercase is an error, never normalized). A letter already
  in use MOVES to the new take — a normal correction, not an error.
  Re-stamping the take it already names is a no-op affirmation (`moved:
  false`). No head/takes at all → `take-not-found`, exit 1. No prompting,
  ever — flags/`--json` only.

### lyra rice take diff

```
lyra rice take diff [--take N | --mark <letter>] [--json]
```

- **Reads:** `song/stage/mode.json` (Draft-mode gate), the base take's record
  from `takes/`, `takes/marks.json` + `takes/head.json` (for base resolution),
  and the routed draft's CURRENT staged `song/stage/livery.json`.
- **Output:** `"no change since take NNNN"` or `"diff since take NNNN (K
  key(s) changed):"` followed by `+ path: new` / `- path: old` / `~ path: old
  -> new` lines (compact single-line JSON values). Data `{base, diff: [{path,
  old, new}]}`; the nothing-to-diff-against case is `ok` with `{base: null,
  diff: []}`.
- **Notes:** read-only, takes NO lock. Key-wise VALUE diff walking dotted
  paths — never a text diff; key reordering is not a change, arrays compare
  as opaque values. Base default: the nearest marked take on the head's OWN
  ancestry (head first), falling back to the head's parent; `--take N` /
  `--mark <letter>` override (`take-not-found` / `mark-not-found`, exit 1).
  Malformed flags are usage errors (exit 2) checked before the draft gate.

### lyra rice take prune

```
lyra rice take prune [--older-than <N>d|<N>h] [--keep <N>] [--all-but-marks] [--force] [--json]
```

- **Reads:** the routed draft's `takes/` store (records, `head.json`,
  `marks.json`) and `song/stage/mode.json`.
- **Writes:** deletes each pruned `takes/NNNN.json`; rewrites (atomic) any
  surviving take whose `parent` the splice changed (children re-parent to the
  pruned take's parent, so ancestry keeps resolving); rewrites
  `takes/marks.json` dropping letters that named pruned takes. One
  `with_stage_lock` around the whole splice-persist-delete pass.
- **Output:** flag-driven success → `"pruned K take(s) for <song>/<draft>:
  …"`; data `{pruned, reparented: [{take, from, to}], droppedMarks,
  protectedByMark, skippedNowProtected}`. Bare off-tty → `ok` DRY RUN,
  nothing written: data `{dryRun: true, candidates, protectedByMark,
  protectedAncestry}`. Empty flag-driven candidate set → `ok` "nothing to
  prune" (idempotent no-op).
- **Notes:** Draft mode only. The head and its WHOLE ancestry are never
  candidates under any flag combination — re-checked against a fresh read at
  prune time (`skippedNowProtected` reports candidates that became protected
  between planning and execution). Multiple selector flags combine as AND.
  Marked takes are protected unless `--force`. `--older-than` accepts only
  `<N>d`/`<N>h` (malformed → usage exit 2, `invalid-older-than`). Bare
  invocation on a real CLI tty (Door::Cli AND stdin AND stdout are terminals
  — `pkgs/aoide/crates/protocol/src/pick.rs`) opens a multi-select picker
  (`inquire::MultiSelect` renders the tty path); choosing rows IS the
  confirm (no y/n prompt), aborting is usage exit 2 (`no-selection`).

### lyra rice back

```
lyra rice back [--take N | --mark <letter>] [--json]
```

- **Reads:** `song/stage/mode.json` (Draft-mode gate), the target take
  record, `takes/marks.json` (for `--mark`; a letter naming a pruned/absent
  take reads as `mark-not-found`), the current staged content (drift check),
  and — for the write-back — verifies `song/stage/livery.json` IS a symlink
  (refuses `routing-broken` rather than silently writing a plain stage file).
- **Writes (one `with_stage_lock` around everything):** if the staged
  content differs from the head take, first mints a drift take (cause
  `"drift"`: `takes/NNNN.json` + `head.json`) so un-taken edits survive;
  atomic-writes the target take's livery into `song/stage/livery.json`
  (symlink-transparent → lands in the draft file); restores or removes
  `song/stage/cover.json` to match the take (stage only — the draft dir's
  archived `cover.json` is never written); re-syncs
  `run/qml/songs/registry.json` from the reverted livery's `.widgets` key;
  finally moves `takes/head.json` to the target.
- **Pipes to / output:** best-effort `hyprctl --batch` geometry/border apply
  from the reverted livery (same `live::apply_live` as `rice stage`). No
  Quickshell IPC reload — every file written here is FileView-watched; the
  IPC lane exists only for dynamically-loaded widget bodies, which a revert
  never touches. Data `{from, to, mark, drifted, hyprctl, registry}`.
- **Notes:** Draft mode only. A revert is NOT a take — only the head cursor
  moves; the next snapshot parents off wherever it points (implicit
  branching, no branch command). Bare invocation on a real CLI tty opens a
  numbered picker defaulting to the head's parent (one-step undo = bare
  Enter), rendered by `inquire` behind the `pick.rs` seam; off a tty (agent
  doors, pipes) a bare call refuses with usage exit 2 (`no-selection`) —
  flags/`--json` bypass the picker either way. Malformed flags are usage
  errors checked before the lock is taken.

## Related

- [[Self-Ricing]]
- [[Song-Anatomy]]
- [[Song-Vocabulary]]
- [[Ricing-Protocol]]
- [[livery]]
- [[Rebuild-Gate]]
- [[aoide-cli]]
