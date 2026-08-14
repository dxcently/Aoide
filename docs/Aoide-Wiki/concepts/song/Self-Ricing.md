---
type: concept
created: 2026-07-25
updated: 2026-08-14
tags: [aoide, rice, agent]
source: "[[references/AOIDE-HANDOFF]]"
---

# Self-Ricing — the Headline Feature

Aoide ships the rice engine as a builtin. The engine provides the loop, the schema, and the preview mechanism. Everything else — the songs, the preferences, the accumulated taste — it learns by doing.

**Status today:** the loop below is the *designed* shape. `rice lint` (runs the native [[livery]] engine), `rice stage`, and `rice compose` are implemented. `rice stage` stages `stage/livery.json`, [[Quickshell]] hot-reloads it live via `FileView`, and geometry + window-border colours apply to the running compositor over `hyprctl` in the same step (terminal-OSC fan-out is not yet wired into it) — while `rice mode declarative` is locked (below), `rice stage` refuses instead of writing. Beyond a live `rice stage`, `stage/livery.json` is also reseeded from the active song's committed notes on every activation ([[Codebase#Runtime contracts (socket + stage files)]]), so a host that boots without ever staging still carries the correct stage twin. `rice gen`, `rice adopt`, and `rice transpose` are declared but not yet implemented (stub, exit `64`) — narrate those as planned, not as a working pipeline.

## The Rice Loop

```
aoide rice gen <prompt|wallpaper>   (planned)
    ↓  reads songbook/ first, always
aoide rice compose <name> [--from]  (real — scaffolds a new song directly, below)
rice lint                           (real — livery schema validation)
    ↓  fail → reject + songbook note
rice stage                          (real — stages stage/livery.json + live hyprctl apply; refuses while `rice mode declarative` is locked)
    ↓  quickshell hot-reload + live geometry/border colours (terminal OSC fan-out planned)
aoide rice adopt <name>             (planned — User gates this step)
    ↓  committed to song/songbook/<song>/
    ↓  gated rebuild
```

Staging is the sketch — a live compositor call, no rebuild; `aoide.song` selecting a song and rebuilding is the truth — it bakes `hyprland.conf` (geometry, borders), themes every nix-manageable app via [[Stylix]], assembles and deploys the song's widgets, and sets the host's boot default. GTK/Qt surfaces require app restarts and are adopt-only, accepted by design.

## Composing a song

`aoide rice compose <name> [--from <song>] [--force] [--json]` scaffolds a new committed song directly, without going through `gen`:
`song/songbook/<name>/rice.nix` (a self-gating `lib.mkIf (config.aoide.song
== "<name>")` block copying the `palette`/`window`/`geometry` tiers from
`--from`, defaulting to `default` — the only `.nix` file the scaffold
writes, satisfying the song-shape check), a `livery.json` mirror of the
same values, an honest-empty `design/intent.md` pointing at the widget-slot
catalog and the update playbook rather than fabricating design rationale,
and `widgets/.gitkeep`. No `hypr/` directory — geometry lives in `rice.nix`
alongside palette and window, not as a separate tier of files. Because it is
an ordinary schema command (`gated: false`, in `schema --json` and the MCP
tool list), an agent can bootstrap a song through the same door a human
would.

## Staging vs Declarative Mode

`rice stage` and `cover set` are the only two writers of `stage/livery.json`
and `stage/cover.json` anywhere in the codebase. `stage/mode.json` — a
gitignored runtime stage-file, the same category as `stage/design.json` —
records which of two modes currently owns those writes:

- **`staging`** — hot-load unlocked; `rice stage`/`cover set` write live.
- **`declarative`** — nix/home-manager is the only writer; `rice stage`/
  `cover set` refuse outright with a `declarative-mode-locked` error naming
  `rice mode stage` as the way to unlock. An absent `stage/mode.json` reads
  as `declarative` — the safe default, since nothing has ever unlocked
  staging writes.

`aoide rice mode status` reports the current mode plus, in `staging`, which
song/draft it is pointed at and since when. `aoide rice mode stage
[<name>]` unlocks staging; given a name, it also stages that song
immediately, combining unlock-and-stage into one call. `aoide rice mode
declarative [<name>]` locks staging; given a name, it re-pins
`stage/livery.json` to that song's committed notes first and only writes
the lock marker after that write succeeds, so the re-pin can never trip the
lock it is about to set — this works even from the unmarked default state.
Called with no name, it locks whatever is already staged as-is: a freeze,
not a re-derivation of the staged truth.

The enforcement lives at the two write entrypoints only —
`handle_rice_stage_entry` in `crates/song/src/commands/rice.rs`,
`handle_cover_set_entry` in `crates/song/src/commands/cover.rs` — with no
background reconciler watching `stage/mode.json`. `rice stage` and `cover
set` are the only two places in the codebase that ever write those two
stage files, so guarding their entrypoints closes every path a drift could
take; [[aoided]] itself is still a one-shot skeleton with no event loop.

`rice design enter`/`exit` (the separate design-mode marker,
`stage/design.json`) calls the same underlying staging logic `rice stage`
does, guard-free — it is not gated by `rice mode`. The two markers are
independent: entering design mode neither reads nor writes
`stage/mode.json`, and staging mode neither reads nor writes
`stage/design.json`.

## Geometry

A song may set `aoide.livery.geometry` — gaps, border size, rounding, and
blur, every field optional — alongside its palette and window tiers; see
[[livery#The geometry tier]] for the field list and the fallback/live-apply
mechanism. A song that sets no geometry performs with the compositor
facet's own defaults, unchanged.

## Shipped Defaults Are Immutable

Upstream ships the `default` song under `song/songbook/default/`. Evolution lands as new song folders under `song/songbook/`; a bad generation can never overwrite the shipped look.

`rice gen` (once implemented) starts from the shipped default rice + wallpaper unless explicitly told otherwise — the baseline is always defined, never guessed.

## Songbook Discipline — the "Self" in Self-Ricing

The agent reads `songbook/` and the relevant song's `design/` before every `gen`. It appends after every adopt or reject. Without this write-back the system is a theme generator; with it the system accumulates taste.

- `songbook/learnings.md` — cross-cutting observations.
- `songbook/preferences.md` — accumulated from adopt/reject decisions.
- `songbook/update-playbook.md` — schema migration instructions.
- `song/songbook/<song>/design/` — per-song design intent, palette rationale, iteration log.

Design folders are ordinary content-pipeline folders, pre-approved as system-owned. Aoide ingests its own design notes so the agent can query past rice reasoning like any other domain.

The discipline is already in use: the default song's design memory (`song/songbook/default/design/intent.md`, Iteration Log) records the user's declared aesthetic — Windows-7-sidebar chrome with ASCII/box-drawing note theming, realized as the [[Gadget-Dock]] — so future generations inherit that design decision as the song's memory.

## Coverage Tiers

| Tier | Scope | Command |
|---|---|---|
| Solo | Key + cover (palette + wallpaper) | `rice gen` (minimal) |
| Ensemble | Core surfaces — the gen default | `rice gen` |
| Full orchestration | Everything incl. greeter, per-app, chimes | `rice gen --full` |

`rice lint` reports instrumentation coverage. A full rice missing its lockscreen fails loudly. (`rice gen`/`--full` are planned — see Status above.)

## Adopt, Select, Replay

**Adopt** (planned) commits a generated song to `song/songbook/<song>/` — it becomes durable, versioned fleet-available score. Every host that pulls the fork can then perform it.

**`aoide.song`** is the per-host selector (str, default `"default"`). A single line in `hosts/<host>/default.nix` selects which song the host performs:

```nix
aoide.song = "sonata";
```

Songs self-register via the `song/songbook/` walk in `lib/mkHost.nix`; each song's `rice.nix` guards itself with `lib.mkIf (config.aoide.song == "<name>")`. No explicit import list: committing a song makes it available to all hosts. (This walker/registration mechanism is real and shipped — only the CLI verbs that generate/commit songs are stubbed.)

**Replay** is performing an adopted song at a different host. The song carries only livery (palette + component tiers); the host supplies its own specifics (hardware, monitors) and its own enabled instruments (facets, dendrites). A host lacking an instrument does not sound that part — coverage degrades gracefully through the tiers above.

**Transpose** vs **replay**: transpose = same venue, new key (new palette). Replay = same score, new venue (different host). Both are one-line operations on an adopted song (`rice transpose` itself is planned; the replay declaration — `aoide.song = "<name>";` — is real).

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
└── design/       design wiki: intent, palette rationale, log
```

Covers themselves live in the shared `song/covers/` library, not per-song — any song's `rice.nix` references a file there by literal path.

`song/` is the agent's writable domain. The agent commits there and nowhere else. Adopt = commit + gated rebuild.

## Related

- [[Song-Vocabulary]]
- [[livery]]
- [[aoided]]
- [[Content-Pipeline]]
- [[Snowflake-Anatomy]]
- [[Stylix]]
- [[Gadget-Dock]]
