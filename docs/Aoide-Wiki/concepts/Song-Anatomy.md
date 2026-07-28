---
type: concept
created: 2026-07-28
updated: 2026-07-28
tags: [aoide, song, rice, architecture]
source: "[[references/AOIDE-HANDOFF]]"
---

# Song Anatomy — the Performed Half's Tree

The sibling of [[Snowflake-Anatomy]] (which maps the frozen `modules/` tree).
This page maps the **`song/` tree** — the performed half's home on disk: the
committed *score* (the songbook) and the gitignored *live state* (the stage the
desktop reads). It is a **role map**: what each subfolder is for, whether it is
committed or runtime, and who writes it. The vocabulary those roles are named
in lives in [[Song-Vocabulary]]; this page grounds the words in the actual
directories.

`song/` lives at `~/Aoide/song` (onboard links `~/song` → `~/Aoide/song`). It is
the **song agent's writable domain** (house rule #1, `AGENTS.md`): the agent
commits *there* and, for score, nowhere else. Runtime dirs inside it are
gitignored and created on demand.

## The subfolders

| Subfolder | Role | Committed? | Written by |
|---|---|---|---|
| `songbook/` | per-song homes + cross-cutting design memory | **committed** | song agent |
| `stage/` | live preview/runtime state — the seam the desktop reads | gitignored runtime | [[drachma]] · [[shellbridge]] · `aoide graph` |
| `auditions/` | the propose gate | gitignored runtime | runtime |

Only `stage/` and `auditions/` are gitignored — those two are the whole runtime
surface; everything else under `song/` is versioned score.

> **The shipped default song lives in the songbook like any other.**
> `aoide.song`'s default, `"default"`, is `song/songbook/default/`. It is the
> songbook's one **merge-only** song — upstream-owned by git merge-base, not by
> path ([[Governance]]) — so an agent adopts *new* songs alongside it but never
> overwrites it, and a bad generation can never replace the shipped look.

## Committed score — the versioned half

### `songbook/<name>/` — one song each

A song self-registers by living here: `lib/mkHost.nix` walks `song/songbook/`
alongside `modules/`, and each song's `rice.nix` guards itself with
`lib.mkIf (config.aoide.song == "<name>")`, so committing a folder makes the
song fleet-available with no import list to edit ([[Self-Ricing]],
[[Snowflake-Anatomy]]). Songs present today: **`sonata`** (the cream Alma-Tadema
*Unconscious Rivals* LIGHT key, currently performed on yomi-strix) and
**`hero`** (its own dusk-plum key). Each folder holds:

| Subfolder/file | Holds |
|---|---|
| `rice.nix` | pure nix: sets `aoide.drachma.*` (palette + base16 + component tiers + wallpaper) under the `aoide.song` guard. **Only** `aoide.drachma` — no host options, no facet toggles — so one score replays at any venue (`CONTRACTS.md §5`, [[Song-Vocabulary#Replay — any song, any host]]). |
| `drachma.json` | the song's resolved drachma values — the [[drachma]] schema: `palette`, `base16`, `bar`/`notif`/`window`. |
| `assets/` | wallpaper + cover art |
| `palette/` | this song's transpose keys — the palette variants `rice transpose <song> <key>` swaps among |
| `sounds/` | notification + system sounds (the chimes dimension) |
| `icons/` | per-song icon overrides |
| `widgets/` | per-song widget bodies |
| `design/` | the song's design memory — `intent.md` (palette rationale, iteration log), ingested like any content ([[Self-Ricing#Songbook Discipline — the "Self" in Self-Ricing]]) |

### `songbook/` root — cross-cutting design memory

Alongside the per-song folders, the songbook root holds the memory that
crosses every song: `learnings.md`, `preferences.md`, `update-playbook.md`
(the schema-migration playbook). The agent reads these before every `rice gen`
and appends after every adopt/reject — the write-back that is the "self" in
[[Self-Ricing]]. **This is where the design grammar and ricing memory that
currently sit in the dev wiki ([[design/Pantheon-Grammar|Pantheon Grammar]],
the [[design/Ricing-Protocol|Ricing Protocol]]'s worked examples) properly
belong** — a pending migration into the song agent's domain, flagged in the
handoff.

## Runtime state — the gitignored half

### `stage/` — the seam the desktop reads

The live preview/runtime state, hot-reloaded by [[Quickshell]] and never
committed. Written only by **atomic write-temp-then-rename**, so a reader never
sees a torn file (`CONTRACTS.md §4`). The files:

| File | Holds | Written by |
|---|---|---|
| `drachma.json` | the fully-resolved drachma values (colours concrete, no `null`) | [[drachma]] `emit stage` / `aoide rice preview` |
| `sessions.json` | the agent-session roster (`sessionId, agent, windowAddress, workspace, cwd, state, startedAt`, optional `parentSessionId`) | [[shellbridge]] + `aoide graph session` |
| `hooks.json` | live Claude Code hook phases | shellbridge + `aoide graph session` |
| `projects.json` | the project-anchor registry | `aoide graph project` |
| `graph.json` | the resolved project/session DAG | `aoide graph emit` |
| `cover.json` | the live wallpaper seed (seeded from the song's baked `wallpaper`) | Quickshell wallpaper layer |

Which files are present is runtime-dependent (e.g. `cover.json` appears once a
wallpaper is staged; `AOIDE_WALLPAPER` on the Quickshell unit re-seeds it across
rebuilds — see [[design/Pantheon-Grammar]] round 5). The stage dir is resolved
via the `AOIDE_STAGE_DIR` contract seam ([[shellbridge]]); the flake's
`no-song-read` check forbids any nix module reading `stage/` at build time, so
runtime state can never become load-bearing for the build.

### `auditions/`

The propose gate for generated-but-unadopted rices — gitignored, created on
demand.

## Repo Layout (the `song/` surface)

```
~/Aoide/song/
├── songbook/            committed songs + cross-cutting design memory
│   ├── sonata/          the cream Alma-Tadema LIGHT key (selected)
│   │   ├── rice.nix · drachma.json
│   │   ├── assets/ · palette/ · sounds/ · icons/ · widgets/ · design/
│   ├── hero/             the dusk-plum key (same shape)
│   ├── learnings.md · preferences.md · update-playbook.md   ← sparse today
├── stage/               live runtime state (gitignored)
└── auditions/           propose gate (gitignored)
```

**The root is closed** ([[Snowflake-Anatomy]]): new content lands at its
designated place inside this tree — every per-song asset, key, sound, icon, and
widget body lands inside that song's `songbook/<name>/` — never a new
top-level `song/` dir. The lookup is the Song Map
([[Song-Vocabulary#The Song Map]]).

## Related

- [[Song-Vocabulary]] — the vocabulary these roles are named in
- [[Snowflake-Anatomy]] — the frozen sibling (`modules/`)
- [[Self-Ricing]] — the rice loop, songbook discipline, and coverage tiers
- [[drachma]] — the mint that resolves/lints/emits the stage file, the `aoide.drachma` seam `drachma.json` carries
- [[shellbridge]] · [[Session-Graph]] — the writers of the runtime stage files
- [[design/Ricing-Protocol]] · [[design/Pantheon-Grammar]] — the design memory
  bound for `songbook/`
