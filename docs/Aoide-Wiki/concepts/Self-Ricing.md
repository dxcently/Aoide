---
type: concept
created: 2026-07-25
updated: 2026-07-28
tags: [aoide, rice, agent]
source: "[[references/AOIDE-HANDOFF]]"
---

# Self-Ricing — the Headline Feature

Aoide ships the rice engine as a builtin. The engine provides the loop, the schema, and the preview mechanism. Everything else — the songs, the preferences, the accumulated taste — it learns by doing.

**Status today:** the loop below is the *designed* shape. `rice lint` (delegates to [[drachma]]) and `rice preview` are implemented — `rice preview` stages `stage/drachma.json` and the [[Quickshell]] shell hot-reloads it live (the hyprctl / terminal-OSC fan-out is not yet wired into it). `rice gen`, `rice adopt`, and `rice transpose` are declared but not yet implemented (stub, exit `64`) — narrate those as planned, not as a working pipeline.

## The Rice Loop

```
aoide rice gen <prompt|wallpaper>   (planned)
    ↓  reads songbook/ first, always
rice lint                           (real — drachma schema validation)
    ↓  fail → reject + songbook note
rice preview                        (real — stages ephemeral stage/drachma.json)
    ↓  quickshell hot-reload (hyprctl · terminal OSC fan-out planned)
aoide rice adopt <name>             (planned — User gates this step)
    ↓  committed to song/songbook/<song>/
    ↓  gated rebuild
```

Preview is the sketch; adopt is the truth. GTK/Qt surfaces require app restarts and are adopt-only (not a v1 gap — accepted by design).

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

**Replay** is performing an adopted song at a different host. The song carries only notes (palette + component tiers); the host supplies its own specifics (hardware, monitors) and its own enabled instruments (facets, dendrites). A host lacking an instrument does not sound that part — coverage degrades gracefully through the tiers above.

**Transpose** vs **replay**: transpose = same venue, new key (new palette). Replay = same score, new venue (different host). Both are one-line operations on an adopted song (`rice transpose` itself is planned; the replay declaration — `aoide.song = "<name>";` — is real).

## Keys and Transposition

Each song's `songbook/<song>/palette/` holds its transpose keys — the palette variants that song can swap among. Wallpaper extraction emits new keys as standalone artifacts. `rice transpose <song> <key>` (planned) replays a song in another key. This is only possible because the semantic and component note tiers never touch raw color values directly.

## Per-Song Structure

```
song/songbook/<song>/
├── rice.nix      pure nix: notes import + config swaps
├── drachma.json  note values
├── assets/       wallpaper + cover art
├── palette/      transpose keys
├── sounds/       chimes / notification audio
├── icons/        per-song icon overrides
├── widgets/      per-song widget bodies
└── design/       design wiki: intent, palette rationale, log
```

`song/` is the agent's writable domain. The agent commits there and nowhere else. Adopt = commit + gated rebuild.

## Related

- [[Song-Vocabulary]]
- [[Notes]]
- [[Content-Pipeline]]
- [[Snowflake-Anatomy]]
- [[Stylix]]
- [[Gadget-Dock]]
