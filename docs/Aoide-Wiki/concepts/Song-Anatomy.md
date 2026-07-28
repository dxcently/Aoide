---
type: concept
created: 2026-07-28
tags: [aoide, song, rice, architecture]
source: "[[references/AOIDE-HANDOFF]]"
---

# Song Anatomy — the Performed Half's Tree

The sibling of [[Snowflake-Anatomy]] (which maps the frozen `modules/` tree).
This page maps the **`song/` tree** — the performed half's home on disk: the
committed *score* (songs, covers, the songbook) and the gitignored *live state*
(the stage the desktop reads). It is a **role map**: what each subfolder is for,
whether it is committed or runtime, and who writes it. The vocabulary those
roles are named in lives in [[Song-Vocabulary]]; this page grounds the words in
the actual directories.

`song/` lives at `~/Aoide/song` (onboard links `~/song` → `~/Aoide/song`). It is
the **song agent's writable domain** (house rule #1, `AGENTS.md`): the agent
commits *there* and, for score, nowhere else. Runtime dirs inside it are
gitignored and created on demand.

## The subfolders

| Subfolder | Role | Committed? | Written by |
|---|---|---|---|
| `repertoire/<name>/` | a committed song (one rice) | **committed** | song agent (`rice gen`/`adopt`) |
| `songbook/` | cross-cutting design memory | **committed** | song agent (write-back) |
| `covers/` | wallpaper art | **committed** | song agent |
| `keys/` | shared palette library | **committed** | song agent |
| `chimes/` | notification + system sounds | **committed** | song agent |
| `stage/` | live preview/runtime state — the seam the desktop reads | gitignored runtime | [[drachma]] · [[shellbridge]] · `aoide graph` |
| `backstage/` | runtime plumbing | gitignored runtime | runtime |
| `auditions/` | the propose gate | gitignored runtime | runtime |

Only `stage/`, `backstage/`, and `auditions/` are gitignored (see the repo
`.gitignore` and [[Song-Vocabulary#The Song Map]]); everything else is versioned
score. The committed-but-currently-sparse folders (`songbook/`, `keys/`,
`chimes/`) hold only a `.gitkeep` today — their contracts exist; their content
is largely aspirational.

> **The shipped default song is *not* here.** `aoide.song`'s default,
> `"default"`, is the standard rice baked into the **frozen** half at
> `modules/rime/default/` ([[Snowflake-Anatomy]]), not under `song/`. `song/`
> holds the *evolved* songs an agent adopts; a bad generation can never
> overwrite the shipped look.

## Committed score — the versioned half

### `repertoire/<name>/` — one song each

A song self-registers by living here: `lib/mkHost.nix` walks `song/repertoire/`
alongside `modules/`, and each song's `rice.nix` guards itself with
`lib.mkIf (config.aoide.song == "<name>")`, so committing a folder makes the
song fleet-available with no import list to edit ([[Self-Ricing]],
[[Snowflake-Anatomy]]). Songs present today: **`sonata`** (the cream Alma-Tadema
*Unconscious Rivals* LIGHT key, currently performed on yomi-strix) and
**`hero`** (its own dusk-plum key, from `covers/hero.webp`). Each folder holds:

- **`rice.nix`** — pure nix: sets `aoide.drachma.*` (palette + base16 + component
  tiers + wallpaper) under the `aoide.song` guard. It sets **only** `aoide.drachma`
  — no host options, no facet toggles — so one score replays at any venue
  (`CONTRACTS.md §5`, [[Song-Vocabulary#Replay — any song, any host]]).
- **`drachma.json`** — the song's note values as a v0 W3C design-tokens
  container (the [[drachma]] schema: `palette`, `base16`, `bar`/`notif`/`window`).
- **`liner/`** — the song's **per-song design memory**: `intent.md` (palette
  rationale, iteration log). This is the song agent's design record, ingested
  like any content ([[Self-Ricing#Songbook Discipline — the "Self" in Self-Ricing]]).
  (`sonata/liner/intent.md` exists; note it may lag the song's current palette.)

### `songbook/` — cross-cutting design memory

The **cross-cutting** counterpart to the per-song liner: `learnings.md`,
`preferences.md`, `update-playbook.md` (the schema-migration playbook). The
agent reads it before every `rice gen` and appends after every adopt/reject —
the write-back that is the "self" in [[Self-Ricing]]. **This is where the design
grammar and ricing memory that currently sit in the dev wiki
([[design/Pantheon-Grammar|Pantheon Grammar]], the [[design/Ricing-Protocol|
Ricing Protocol]]'s worked examples) properly belong** — a pending migration
into the song agent's domain, flagged in the handoff. Today the folder is a
placeholder.

### `covers/` · `keys/` · `chimes/`

- **`covers/`** — wallpaper art referenced by a song's `aoide.drachma.wallpaper`
  (committed, never a runtime infix). Holds `Alma-Tadema_Unconscious_Rivals.jpg`
  (sonata's cover) and `hero.webp`.
- **`keys/`** — the shared palette library. A key is rice-independent: many
  songs can name one, and `rice transpose <song> <key>` replays a song in
  another key. Empty today.
- **`chimes/`** — notification and system sounds (the `chimes` arrangement
  dimension). Empty today.

## Runtime state — the gitignored half

### `stage/` — the seam the desktop reads

The live preview/runtime state, hot-reloaded by [[Quickshell]] and never
committed. Written only by **atomic write-temp-then-rename**, so a reader never
sees a torn file (`CONTRACTS.md §4`). The files:

| File | Holds | Written by |
|---|---|---|
| `drachma.json` | the fully-resolved note values (colours concrete, no `null`) | [[drachma]] `emit stage` / `aoide rice preview` |
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

### `backstage/` · `auditions/`

`backstage/` is runtime plumbing; `auditions/` is the propose gate for
generated-but-unadopted rices. Both are gitignored and created on demand.

## Repo Layout (the `song/` surface)

```
~/Aoide/song/
├── repertoire/<name>/   committed songs — rice.nix · drachma.json · liner/
│   ├── sonata/          the cream Alma-Tadema LIGHT key (selected)
│   └── hero/            the dusk-plum key
├── songbook/            cross-cutting design memory (write-back)   ← sparse today
├── covers/              wallpaper art
├── keys/                shared palette library                      ← empty today
├── chimes/              notification + system sounds                ← empty today
├── stage/               live runtime state (gitignored)
├── backstage/           runtime plumbing (gitignored)
└── auditions/           propose gate (gitignored)
```

**The root is closed** ([[Snowflake-Anatomy]]): new content lands at its
designated place inside this tree — covers → `covers/`, chimes → `chimes/`,
per-song assets → `repertoire/<name>/` — never a new top-level `song/` dir. The
lookup is the Song Map ([[Song-Vocabulary#The Song Map]]).

## Related

- [[Song-Vocabulary]] — the vocabulary these roles are named in
- [[Snowflake-Anatomy]] — the frozen sibling (`modules/`)
- [[Self-Ricing]] — the rice loop, songbook discipline, and coverage tiers
- [[Notes]] — the `aoide.drachma` seam `drachma.json` carries
- [[drachma]] — the mint that resolves/lints/emits the stage file
- [[shellbridge]] · [[Session-Graph]] — the writers of the runtime stage files
- [[design/Ricing-Protocol]] · [[design/Pantheon-Grammar]] — the design memory
  bound for `songbook/`
