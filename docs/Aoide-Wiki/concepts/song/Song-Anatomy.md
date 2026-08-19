---
type: concept
created: 2026-07-28
updated: 2026-08-19
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
| `stage/` | live preview/runtime state — the seam the desktop reads | gitignored runtime | [[livery]] · [[shellbridge]] · `aoide graph` |
| `auditions/` | the propose gate | gitignored runtime | runtime |

Only `stage/` and `auditions/` are gitignored — those two are the whole runtime
surface; everything else under `song/` is versioned score.

> **The shipped standard song lives in the songbook like any other.**
> `aoide.song`'s default, `"sonata"`, is `song/songbook/sonata/`. It is
> upstream-owned and evolving — like any other upstream-owned tree (nucleus,
> facets), upstream MAY still update or iterate on it, by git merge-base, not
> by path ([[Governance]]). Every OTHER song — anything composed via
> `rice compose` under a different name — is clone-owned instead: an agent
> adopts *new* songs alongside the standard but upstream never overwrites
> them, and a bad generation can never replace what you've composed.

## Committed score — the versioned half

### `songbook/<name>/` — one song each

A song self-registers by living here: `lib/mkHost.nix` walks `song/songbook/`
alongside `modules/`, and each song's `rice.nix` guards itself with
`lib.mkIf (config.aoide.song == "<name>")`, so committing a folder makes the
song fleet-available with no import list to edit ([[Self-Ricing]],
[[Snowflake-Anatomy]]). Songs present today: **`sonata`** (a LIGHT dusk key
keyed from its own cover `song/covers/yuki-sonata.png` — the pianist on
mirror-water at dusk — currently performed on yomi-strix). Each folder
holds:

| Subfolder/file | Holds |
|---|---|
| `rice.nix` | pure nix: sets `aoide.livery.*` (palette + base16 + component tiers + wallpaper) under the `aoide.song` guard. **Only** `aoide.livery` — no host options, no facet toggles — so one score replays at any venue (`CONTRACTS.md §5`, [[Song-Vocabulary#Replay — any song, any host]]). The wallpaper note points at a file in the shared `song/covers/` library (`../../covers/<file>`), not a per-song `assets/` dir. |
| `livery.json` | the song's resolved livery values — the [[livery]] schema: `palette`, `base16`, `bar`/`notif`/`window`. |
| `palette/` | this song's transpose keys — the palette variants `rice transpose <song> <key>` swaps among |
| `sounds/` | notification + system sounds (the chimes dimension) |
| `icons/` | per-song icon overrides |
| `widgets/` | per-song widget bodies — QML files the staging engine resolves per slot ([[Widget-Maker#The staging engine — a song overrides desktop chrome]]) |
| `design/` | the song's design memory — `intent.md` (palette rationale, iteration log), ingested like any content ([[Self-Ricing#Songbook Discipline — the "Self" in Self-Ricing]]) |

**sonata's `widgets/` holds fourteen bodies today**: `bar`, `calendar`,
`conductor`, `dock`, `herald`, `herald-center`, `launcher`, `meters`,
`powermenu`, `power`, `terminals`, `usage`, `wallpaper`, `wallpaper-picker`.
Six are wired to a live host anchor (`calendar`, `herald`, `herald-center`,
`powermenu`, `launcher`, `bar` — `modules/facets/quickshell/qml/slots.md`);
the other eight (`dock`, `conductor`, `terminals`, `meters`, `power`,
`usage`, `wallpaper`, `wallpaper-picker`) are twins of facet originals still
doing the live drawing, carried but unanchored until the facet switch lands
([[Gadget-Dock#What the dock holds]]).

### `songbook/` root — cross-cutting design memory

Alongside the per-song folders, the songbook root holds the memory that
crosses every song: `learnings.md`, `preferences.md`, `update-playbook.md`
(the schema-migration playbook). The agent reads these before every
iteration and appends after every declare/reject — the write-back that is
the "self" in [[Self-Ricing]].

Rice design memory lives in the songbook, per song: each song's current
design elements live in its own `design/intent.md` (sonata's
records the key, the glass values, and the surface elements as performed).
The cross-cutting Pantheon grammar the retired `default` song once owned is
no longer live in any song's design folder — kept as historical reference at
`docs/Aoide-Wiki/references/pantheon/pantheon-grammar.md`.
The wiki carries only the protocol ([[Ricing-Protocol|Ricing Protocol]], in
`concepts/song/`) — design content about a rice goes to that rice's `design/`
folder in the repo, not the wiki.

## Runtime state — the gitignored half

### `stage/` — the seam the desktop reads

The live preview/runtime state, hot-reloaded by [[Quickshell]] and never
committed. Written only by **atomic write-temp-then-rename**, so a reader never
sees a torn file (`CONTRACTS.md §4`). The files:

| File | Holds | Written by |
|---|---|---|
| `livery.json` | the fully-resolved livery values (colours concrete, no `null`) | [[livery]] `emit stage` / `aoide rice stage` |
| `sessions.json` | the agent-session roster (`sessionId, agent, windowAddress, workspace, cwd, state, startedAt`, optional `parentSessionId`) | [[shellbridge]] + `aoide graph session` |
| `hooks.json` | live Claude Code hook phases | shellbridge + `aoide graph session` |
| `projects.json` | the project-anchor registry | `aoide graph project` |
| `graph.json` | the resolved project/session DAG | `aoide graph emit` |
| `cover.json` | the live wallpaper seed (seeded from the song's baked `wallpaper`) | Quickshell wallpaper layer |

Which files are present is runtime-dependent (e.g. `cover.json` appears once a
wallpaper is staged; `AOIDE_WALLPAPER` on the Quickshell unit re-seeds it across
rebuilds — the facet bakes the song's `wallpaper` note into the unit env, see
[[Quickshell]]). The stage dir is resolved
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
│   ├── sonata/          the shipped standard — upstream-owned, evolving; the LIGHT dusk key, keyed from song/covers/yuki-sonata.png (selected)
│   │   ├── rice.nix · livery.json
│   │   ├── palette/ · sounds/ · icons/ · widgets/ · design/
│   ├── learnings.md · preferences.md · update-playbook.md   ← sparse today
├── covers/              shared wallpaper library — yuki-sonata.png · sonata.webp · Alma-Tadema_Unconscious_Rivals.jpg
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
- [[livery]] — the engine that resolves/lints/emits the stage file, the `aoide.livery` seam `livery.json` carries
- [[shellbridge]] · [[Session-Graph]] — the writers of the runtime stage files
- [[Ricing-Protocol]] — the ricing protocol this page's per-song design memory
  supports
- [[Widget-Maker]] — the staging engine, which resolves a song's `widgets/` files to live desktop chrome
- [[Gadget-Dock]] — the dock/gadget bodies now twinned into sonata's `widgets/`, pending the facet switch
