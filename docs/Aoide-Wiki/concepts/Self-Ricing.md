---
type: concept
created: 2026-07-25
updated: 2026-07-26
tags: [aoide, rice, agent]
source: "[[references/AOIDE-HANDOFF]]"
---

# Self-Ricing — the Headline Feature

Aoide ships the rice engine as a builtin. The engine provides the loop, the schema, and the preview mechanism. Everything else — the songs, the preferences, the accumulated taste — it learns by doing.

## The Rice Loop

```
aoide rice gen <prompt|wallpaper>
    ↓  reads songbook/ first, always
rice lint                   (drachma schema validation)
    ↓  fail → reject + songbook note
rice preview                (ephemeral: stage/drachma.json)
    ↓  quickshell hot-reload · hyprctl · terminal OSC
aoide rice adopt <name>     (User gates this step)
    ↓  committed to song/repertoire/<song>/
    ↓  gated rebuild
```

Preview is the sketch; adopt is the truth. GTK/Qt surfaces require app restarts and are adopt-only (not a v1 gap — accepted by design).

## Shipped Defaults Are Immutable

Upstream ships 2–3 default rices under `modules/rime/default/`. Evolution lands as new folders under `song/repertoire/`; a bad generation can never overwrite the shipped look.

`rice gen` starts from the shipped default rice + wallpaper unless explicitly told otherwise — the baseline is always defined, never guessed.

## Songbook Discipline — the "Self" in Self-Ricing

The agent reads `songbook/` and the relevant `liner/` before every `gen`. It appends after every adopt or reject. Without this write-back the system is a theme generator; with it the system accumulates taste.

- `songbook/learnings.md` — cross-cutting observations.
- `songbook/preferences.md` — accumulated from adopt/reject decisions.
- `songbook/update-playbook.md` — schema migration instructions.
- `song/repertoire/<song>/liner/` — per-song design intent, palette rationale, iteration log.

Liner folders are ordinary content-pipeline folders, pre-approved as system-owned. Aoide ingests its own liner notes so the agent can query past rice reasoning like any other domain.

The discipline is already in use: the default rice's liner (`modules/rime/default/liner/intent.md`, Iteration Log) records the user's declared aesthetic — Windows-7-sidebar chrome with ASCII/box-drawing note theming, realized as the [[Gadget-Dock]] — so future generations inherit that design decision as the rice's memory.

## Coverage Tiers

| Tier | Scope | Command |
|---|---|---|
| Solo | Key + cover (palette + wallpaper) | `rice gen` (minimal) |
| Ensemble | Core surfaces — the gen default | `rice gen` |
| Full orchestration | Everything incl. greeter, per-app, chimes | `rice gen --full` |

`rice lint` reports instrumentation coverage. A full rice missing its lockscreen fails loudly.

## Adopt, Select, Replay

**Adopt** commits a generated song to `song/repertoire/<song>/` — it becomes durable, versioned fleet-available score. Every host that pulls the fork can then perform it.

**`aoide.song`** is the per-host selector (str, default `"default"`). A single line in `hosts/<host>/default.nix` selects which song the host performs:

```nix
aoide.song = "moonlight";
```

Songs self-register via the `song/repertoire/` walk in `lib/mkHost.nix`; each song's `rice.nix` guards itself with `lib.mkIf (config.aoide.song == "<name>")`. No explicit import list: committing a song makes it available to all hosts.

**Replay** is performing an adopted song at a different host. The song carries only notes (palette + component tiers); the host supplies its own specifics (hardware, monitors) and its own enabled instruments (facets, dendrites). A host lacking an instrument does not sound that part — coverage degrades gracefully through the tiers below.

**Transpose** vs **replay**: transpose = same venue, new key (new palette). Replay = same score, new venue (different host). Both are one-line operations on an adopted song.

## Keys and Transposition

`song/keys/` is the shared palette library. A key is rice-independent — many songs can reference one by name. Wallpaper extraction emits new keys as standalone artifacts. `rice transpose <rice> <palette>` replays any song in another key. This is only possible because the semantic and component note tiers never touch raw color values directly.

## Per-Song Structure

```
song/repertoire/<song>/
├── rice.nix      pure nix: notes import + config swaps
├── drachma.json    note values
├── liner/        design wiki: intent, palette rationale, log
└── assets/       song-specific art and references
```

`song/` is the agent's writable domain. The agent commits there and nowhere else. Adopt = commit + gated rebuild.

## Related

- [[Song-Vocabulary]]
- [[Notes]]
- [[Content-Pipeline]]
- [[Snowflake-Anatomy]]
- [[Stylix]]
- [[Gadget-Dock]]
