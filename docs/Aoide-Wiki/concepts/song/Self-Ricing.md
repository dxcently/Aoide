---
type: concept
created: 2026-07-25
updated: 2026-08-14
tags: [aoide, rice, agent]
source: "[[references/AOIDE-HANDOFF]]"
---

# Self-Ricing — the Headline Feature

Aoide ships the rice engine as a builtin. The engine provides the loop, the schema, and the staging mechanism. Everything else — the songs, the preferences, the accumulated taste — it learns by doing.

**Status today:** the loop below is real end to end except its final commit step. `rice lint` (runs the native [[livery]] engine), `rice stage`, `rice compose`, the `rice draft` group (`save`/`list`/`drop`), and the `rice mode` group (`status`/`stage`/`declarative`/`draft`) are all implemented. `rice stage` stages `stage/livery.json`, [[Quickshell]] hot-reloads it live via `FileView`, and geometry + window-border colours apply to the running compositor over `hyprctl` in the same step (terminal-OSC fan-out is not yet wired into it) — while `rice mode declarative` is locked (below), `rice stage` refuses instead of writing. Beyond a live `rice stage`, `stage/livery.json` is also reseeded from the active song's committed notes on every activation ([[Codebase#Runtime contracts (socket + stage files)]]), so a host that boots without ever staging still carries the correct stage twin. `rice declare` and `rice transpose` are declared but not yet implemented (stub, exit `64`) — narrate those as planned, not as a working pipeline. There is no `rice gen`: a speculative prompt/wallpaper generator was scoped early on but never built and cut outright (khoa 2026-08-14) rather than left as a permanent stub with no design behind it — `rice compose` is the real, working scaffolding entry point.

## The Rice Loop

```
aoide rice compose <name> [--from <song>]   (real — scaffolds a new song, below)
    ↓  copies palette/window/geometry from --from (default "sonata")
aoide rice mode stage <name>                (real — unlocks staging, stages declared content)
    ↓  edit the song's files (hand or agent)
rice lint                                    (real — livery schema validation)
    ↓  fail → reject + songbook note
aoide rice mode draft <draft-name>           (real — ROUTES stage/livery.json into a saved
    ↓                                          draft via a symlink; forks it from the
    ↓                                          current stage if new; see Drafts below)
    ⋯ iterate freely: every future write (rice stage, a hand-edit, Quickshell's own
       reload) lands directly in the draft file — no separate save step; switch to a
       different saved iteration any time with another `rice mode draft <name>` ⋯
aoide rice mode declarative                  (real — tears the routing down, re-pins
    ↓                                          the declared truth; the draft itself
    ↓                                          stays saved on disk)
aoide rice declare <name>                    (planned — User gates this step)
    ↓  committed to song/songbook/<song>/
    ↓  gated rebuild
```

Staging is the sketch — a live compositor call, no rebuild; `aoide.song` selecting a song and rebuilding is the truth — it bakes `hyprland.conf` (geometry, borders), themes every nix-manageable app via [[Stylix]], assembles and deploys the song's widgets, and sets the host's boot default. GTK/Qt surfaces require app restarts and are declare-only, accepted by design.

## Composing a song

`aoide rice compose <name> [--from <song>] [--force] [--json]` scaffolds a new committed song directly — the starting point of the rice loop above:
`song/songbook/<name>/rice.nix` (a self-gating `lib.mkIf (config.aoide.song
== "<name>")` block copying the `palette`/`window`/`geometry` tiers from
`--from`, defaulting to `sonata` — the only `.nix` file the scaffold
writes, satisfying the song-shape check), a `livery.json` mirror of the
same values, an honest-empty `design/intent.md` pointing at the widget-slot
catalog and the update playbook rather than fabricating design rationale,
and `widgets/.gitkeep`. No `hypr/` directory — geometry lives in `rice.nix`
alongside palette and window, not as a separate tier of files. Because it is
an ordinary schema command (`gated: false`, in `schema --json` and the MCP
tool list), an agent can bootstrap a song through the same door a human
would.

## Drafts — durable scratch, reached by ROUTING not copying

Between staging a song and declaring it, drafts give the rice loop a way to
keep more than one live iteration around without committing any of them. A
draft is a saved snapshot living at
`song/songbook/<song>/drafts/<draft-name>/livery.json` (+ `cover.json` when
the stage had one at save time) — nested under the song it varies, not a
flat top-level dir: a draft is a variation of an already-composed song, so
it belongs inside that song's own directory, which the mechanism enforces
for free (a song can only ever be staged, and therefore have a resolvable
"current song," once it already exists under `songbook/`).

Gitignored (`song/songbook/*/drafts/`, same category as `song/stage/`) and
banned from nix-eval reads (`lib/checks.nix`'s `noSongRead`) — a draft is
durable scratch, never committed or declared truth. That distinction from
`song/songbook/<name>/`'s own committed files is the entire point.

**Reaching a draft is `rice mode draft <name>`'s job — a distinct THIRD
mode, not a heuristic.** `RiceMode` is `Staging | Declarative | Draft`.
Entering `Draft` mode points `stage/livery.json` at a SYMLINK into the
draft's own file (forking it from whatever's currently in the stage first,
if the name doesn't exist yet). From then on, every writer of the stage
file — `rice stage`, a hand-edit, Quickshell's own FileView reload —
transparently lands in the draft, with zero code anywhere aware that
routing exists: `aoide_storage::fs::atomic_write` resolves and writes
through a symlink at its destination rather than replacing it (POSIX
`rename()` would otherwise silently replace the symlink itself on the very
first write). Routing, not guessing — no mtime comparison, no auto-prefer
heuristic; a draft is only ever live because something explicitly pointed
the stage at it.

- **`rice mode draft <name>`** — routes `stage/livery.json` into
  `songbook/<song>/drafts/<name>/livery.json` (`<song>` auto-resolved off
  the current stage, same as `rice mode stage`'s no-arg form). Forks the
  draft from the current stage first if it's a new name. Sets the marker to
  `{mode: draft, song, draft: <name>}`. Refuses while `rice mode
  declarative` is locked.
- **`rice draft save <name>`** — an INDEPENDENT verb: explicitly forks
  whatever's currently live into a new or updated draft snapshot, without
  switching modes. Useful for preserving a second variant while still
  working in a first one, or saving a snapshot from plain `Staging` without
  ever entering `Draft` mode. Upserts (re-saving an existing name
  overwrites it, clearing a stale `cover.json` the current stage no longer
  carries). Never touches `mode.json`.
- **`rice draft list [<song>]`** — enumerates saved drafts: name, song,
  saved-at, and whether each is the currently-loaded one (`mode.json`'s
  `draft` field, which is set if-and-only-if `mode == draft`). No arg walks
  every song's drafts; `<song>` scopes to just that song's.
- **`rice draft drop <name>`** — deletes a draft outright. Missing name is
  an error (not idempotent-silent). Refuses if it's the draft `Draft` mode
  currently has the stage routed to — switch modes first
  (`rice mode stage`/`rice mode declarative`, both of which tear the
  routing down), then drop it; a `drop` that also silently changed your
  mode would be the more surprising behavior.

There used to be a fourth verb, `rice draft stage <name>` (copy-based: read
the draft, write it into the stage as a one-shot snapshot). It's gone,
fully superseded by `rice mode draft`'s routing — keeping both would be two
spellings of "go live with this draft."

**Leaving `Draft` mode:** `rice mode stage`/`rice mode declarative` both
tear the routing symlink down FIRST (removing it, so their own
declared-content write creates a real file) before doing anything else —
neither ever leaves a dangling symlink behind. Both `rice stage <name>` and
`rice mode stage` ALWAYS mean plain declared content now — no
draft-awareness of any kind, no auto-prefer heuristic. (Bare `rice stage`
with no name still auto-resolves the current song, the same convenience
`rice mode stage`'s own no-arg form has — that's independently useful and
unrelated to drafts.)

**The round-trip this exists for:**

```
rice mode stage sonata          → stages declared sonata; marker {staging, sonata, ∅}
(hand-edit stage/livery.json)
rice draft save neon-night      → forks the edit into a NEW draft; songbook/sonata
                                   itself untouched; marker unchanged (still staging)
rice mode declarative           → tears down nothing (wasn't routed), re-pins from the
                                   COMMITTED songbook, locks; the draft stays on disk
rice mode stage                 → unlocks, re-stages declared sonata — PLAIN declared
                                   content, no auto-prefer; marker {staging, sonata, ∅}
rice mode draft neon-night      → ROUTES stage/livery.json into the saved draft (it
                                   already exists, so no fork); marker {draft, sonata,
                                   neon-night}
(edit stage/livery.json again)  → lands DIRECTLY in the draft file — no save step
rice mode declarative           → tears the routing down, re-pins declared; locks
rice mode stage                 → plain declared sonata again, no auto-prefer
rice mode draft neon-night      → routes back to the SAME draft file, still carrying
                                   every edit made while it was live
```

Multiple drafts coexist independently — saving `amber-dusk` alongside
`neon-night` never touches it, and `rice draft list` shows both.

## Staging vs Declarative Mode

`rice stage` and `cover set` are the only two writers of `stage/livery.json`
and `stage/cover.json` anywhere in the codebase (unaffected by which of
`stage`/`declarative`/`draft` currently owns the routing — see below).
`stage/mode.json` — a gitignored runtime stage-file — records which of
**three** modes currently owns those writes (`RiceMode`:
`Staging | Declarative | Draft`):

- **`staging`** — hot-load unlocked; `rice stage`/`cover set` write live,
  always as a plain real file.
- **`declarative`** — nix/home-manager is the only writer; `rice stage`/
  `cover set` refuse outright with a `declarative-mode-locked` error naming
  `rice mode stage` as the way to unlock. An absent `stage/mode.json` reads
  as `declarative` — the safe default, since nothing has ever unlocked
  staging writes.
- **`draft`** — `stage/livery.json` is a SYMLINK routed into a saved
  `songbook/<song>/drafts/<name>/livery.json` via `rice mode draft <name>`;
  see [[Self-Ricing#Drafts — durable scratch, reached by ROUTING not copying]]
  for the full mechanism. `rice stage`/`cover set` still write normally —
  they carry zero awareness that routing exists.

`rice stage` doesn't only hot-load the palette/notes tier any more —
it also syncs the song's widget QML **bodies**
(`song/songbook/<name>/widgets/*.qml`) into the live runtime tree
(`run/qml/songs/<name>/`, `crate::widgets` in `crates/song/`), so an edit to
an EXISTING widget file reaches the desktop through Quickshell's own
file-watcher, no rebuild. This rides the same mode gate as everything
else in this section — locked under `declarative`, allowed under
`staging`/`draft` — so there is no separate lock to reason about. A
brand-new widget file is the one thing this doesn't cover: `manifest.json`
is only read at Quickshell startup, so a new slot still needs a
`systemctl --user restart aoide-quickshell.service` to be discovered.

`aoide rice mode status` reports the current mode plus, in `staging`/
`draft`, which song (and, in `draft`, which draft) it is pointed at and
since when. `aoide rice mode stage [<name>]` unlocks staging AND leaves
`draft` mode if currently in it (tearing the routing symlink down first);
given a name, it also stages that song's declared content immediately,
combining unlock-and-stage into one call. `aoide rice mode declarative
[<name>]` locks staging (also leaving `draft` mode the same way) — with a
name, or with none: it re-pins `stage/livery.json` to the resolved song's
committed notes FIRST (the current stage's own song when no name is given,
same auto-resolve `rice mode stage` uses) and only writes the lock marker
after that write succeeds, so the re-pin can never trip the lock it is
about to set. This means a bare `rice mode declarative` **discards whatever
unsaved live edits sat in the stage** — `rice draft save` first to keep
them (the CLI's own success message says as much). Only when no song can be
resolved at all (a genuinely fresh box with no stage file yet) does it fall
back to a bare lock with nothing to re-pin.

The enforcement lives at the two write entrypoints only —
`handle_rice_stage_entry` in `crates/song/src/commands/rice.rs`,
`handle_cover_set_entry` in `crates/song/src/commands/cover.rs` — with no
background reconciler watching `stage/mode.json`. `rice stage` and `cover
set` are the only two places in the codebase that ever write those two
stage files, so guarding their entrypoints closes every path a drift could
take; [[aoided]] itself is still a one-shot skeleton with no event loop.
The routing symlink's own transparency lives one layer lower, in
`aoide_storage::fs::atomic_write` itself (resolves and writes through a
symlink at its destination rather than letting POSIX `rename()` replace
it) — general behavior, not draft-specific, since every stage-file writer
in the codebase routes through that one function.

Every `rice mode stage` unlock also runs a best-effort stray-process sweep
(`pkgs/aoide/crates/song/src/reap.rs`) before it does anything else, so
unlocking staging always starts from a known-clean process slate rather
than just flipping a marker. It kills three specific leftovers a prior
hot-load/preview session can strand: a `quickshell`/`qs -p <file>`
screenshot-harness process whose `<file>` ends `Preview.qml`
(`CalendarPreview.qml`, `ExodosPreview.qml`, …); a second live
`quickshell -p …/shell.qml` process that isn't the one
`aoide-quickshell.service` is tracking (two would fight over the same
layer-shell namespaces); and any `hyprlock` process at all — if this code
can run interactively, the box isn't actually locked, so a live `hyprlock`
here is always stale. The sweep is best-effort and never fatal to the
unlock: an unreadable `/proc` entry, a process that races away mid-sweep, or
a `kill` that fails is skipped, never propagated — this is hygiene, not a
precondition staging must pass. It runs only on `rice mode stage`'s unlock,
not on `rice mode declarative`/`rice mode draft`.

## Geometry

A song may set `aoide.livery.geometry` — gaps, border size, rounding, and
blur, every field optional — alongside its palette and window tiers; see
[[livery#The geometry tier]] for the field list and the fallback/live-apply
mechanism. A song that sets no geometry performs with the compositor
facet's own defaults, unchanged.

## The Shipped Baseline Is Guarded, Not Frozen

`aoide.song` defaults to `"sonata"` — the shipped standard, guaranteed
present under `song/songbook/sonata/`. A missing baseline is a loud nix
eval failure, never a silent no-op. `sonata` is upstream-owned and
evolving: like any other upstream-owned tree (nucleus, facets), upstream
MAY update or iterate on it.

Every OTHER song — anything composed via `rice compose` under a name other
than `sonata` — upstream never touches. That guarantee is absolute and
unchanged by `sonata` itself being both the shipped baseline and actively
iterated. What actually protects a song from being clobbered was never
"upstream doesn't touch this path" in the first place — it's that nothing
in this system overwrites silently: `rice compose` without `--force`
refuses to touch a song that already exists, no generator writes into a
song unprompted, and `rice declare` (planned) is User-gated by design.

## Songbook Discipline — the "Self" in Self-Ricing

The agent reads `songbook/` and the relevant song's `design/` before iterating. It appends after every declare or reject. Without this write-back the system is a theme generator; with it the system accumulates taste.

- `songbook/learnings.md` — cross-cutting observations.
- `songbook/preferences.md` — accumulated from declare/reject decisions.
- `songbook/update-playbook.md` — schema migration instructions.
- `song/songbook/<song>/design/` — per-song design intent, palette rationale, iteration log.

Design folders are ordinary content-pipeline folders, pre-approved as system-owned. Aoide ingests its own design notes so the agent can query past rice reasoning like any other domain.

The discipline is already in use: the retired `default` song's declared aesthetic — Windows-7-sidebar chrome with ASCII/box-drawing note theming, realized as the [[Gadget-Dock]] — is distilled into `song/songbook/learnings.md` now that `default` itself is gone, so future generations still inherit that design decision as cross-cutting memory.

`rice lint` validates a rice against the note schema; a full rice missing an instrumented surface (e.g. the lockscreen) fails loudly.

## Declare, Select, Replay

**Declare** (planned) commits a staged rice to `song/songbook/<song>/` — it becomes durable, versioned fleet-available score. Every host that pulls the clone can then perform it.

**`aoide.song`** is the per-host selector (str, default `"sonata"`). A single line in `hosts/<host>/default.nix` selects which song the host performs:

```nix
aoide.song = "sonata";
```

Songs self-register via the `song/songbook/` walk in `lib/mkHost.nix`; each song's `rice.nix` guards itself with `lib.mkIf (config.aoide.song == "<name>")`. No explicit import list: committing a song makes it available to all hosts. (This walker/registration mechanism is real and shipped — only `rice declare`/`rice transpose` are stubbed.)

**Replay** is performing a declared song at a different host. The song carries only livery (palette + component tiers); the host supplies its own specifics (hardware, monitors) and its own enabled instruments (facets, dendrites). A host lacking an instrument does not sound that part — coverage degrades gracefully with what's actually instrumented.

**Transpose** vs **replay**: transpose = same venue, new key (new palette). Replay = same score, new venue (different host). Both are one-line operations on a declared song (`rice transpose` itself is planned; the replay declaration — `aoide.song = "<name>";` — is real).

## Keys and Transposition

Each song's `songbook/<song>/palette/` holds its transpose keys — the palette variants that song can swap among. Wallpaper extraction emits new keys as standalone artifacts. `rice transpose <song> <key>` (planned) replays a song in another key. This is only possible because the semantic and component tiers never touch raw color values directly.

## Per-Song Structure

```
song/songbook/<song>/
├── rice.nix      pure nix: livery import + config swaps (wallpaper note points at song/covers/<file>)
├── livery.json   livery values
├── palette/      transpose keys
├── sounds/       chimes / notification audio
├── icons/        per-song icon overrides
├── widgets/      per-song widget bodies
├── design/       design wiki: intent, palette rationale, log
└── drafts/       (gitignored, runtime-only) saved rice-draft scratch — see Drafts above
```

Covers themselves live in the shared `song/covers/` library, not per-song — any song's `rice.nix` references a file there by literal path.

`song/` is the agent's writable domain. The agent commits there and nowhere else. Declare = commit + gated rebuild.

## Related

- [[Song-Vocabulary]]
- [[livery]]
- [[aoided]]
- [[Content-Pipeline]]
- [[Snowflake-Anatomy]]
- [[Stylix]]
- [[Gadget-Dock]]
