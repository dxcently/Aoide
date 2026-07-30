# `song/` — orientation

The performed half's home on disk: committed *score* (`songbook/`) plus
gitignored *live state* (`stage/`, `auditions/`). See the wiki's
Song-Anatomy for the authoritative role map and Self-Ricing for the rice
loop (`gen → lint → preview → adopt`) this tree exists to serve. This file
is a repo-local map, not a replacement for either.

## Tree

```
song/
├── songbook/                   committed score — per-song homes + cross-cutting design memory
│   ├── default/                 the shipped baseline — MERGE-ONLY, upstream-owned, never adopted-over
│   │   ├── rice.nix               aoide.drachma.* under `aoide.song == "default"` guard
│   │   ├── drachma.json           resolved drachma values (Catppuccin Mocha bootstrap)
│   │   ├── design/                house design grammar lives HERE
│   │   │   ├── pantheon.md          the Pantheon wireframe/glyph grammar every song instantiates
│   │   │   ├── intent.md            this rice's own design intent
│   │   │   └── cover-ref.txt        no bundled cover — points at song/covers/ instead
│   │   └── references/            source stills backing the grammar (screenshots, art-direction)
│   ├── <name>/                  one song per folder, e.g. sonata (self-registers — no import list)
│   │   ├── rice.nix                pure nix: sets ONLY aoide.drachma.*, guarded by aoide.song == "<name>"
│   │   ├── drachma.json            this song's resolved drachma values
│   │   ├── palette/                (planned) transpose keys — variants `rice transpose` swaps among
│   │   ├── sounds/                 (planned) notification + system sounds (chimes dimension)
│   │   ├── icons/                  (planned) per-song icon overrides
│   │   ├── widgets/                (planned) per-song widget bodies
│   │   └── design/                 this song's CURRENT key + elements, as performed
│   │       ├── intent.md             palette rationale, contrast table, iteration log
│   │       └── <song-specific>.md    e.g. sonata's `greek-grammar.md` — a song's own
│   │                                  cross-cutting grammar, where it diverges from Pantheon
│   ├── learnings.md              (planned) cross-cutting observations
│   ├── preferences.md            (planned) accumulated from adopt/reject decisions
│   └── update-playbook.md        (planned) schema-migration playbook
├── covers/                     shared wallpaper library — any song's rice.nix points here by literal path
│                                  (../../covers/<file>), not a per-song assets/ dir
├── stage/                      LIVE runtime state — gitignored, atomic write-temp-then-rename only
│   ├── drachma.json               fully-resolved drachma (no nulls) — hot-reloaded by DrachmaState.qml
│   ├── cover.json                 live wallpaper seed { "path": ... } — read by AoideWallpaper.qml
│   └── sessions.json / hooks.json / projects.json / graph.json   session-graph state (shellbridge, aoide graph)
└── auditions/                  (planned) the propose gate for generated-but-unadopted rices — gitignored
```

Root is closed: every per-song asset, key, sound, icon, widget body lands
inside that song's `songbook/<name>/` — never a new top-level `song/` dir
(the Song Map, wiki `Song-Vocabulary`).

## Performance path — how a song reaches the screen

```
songbook/<song>/rice.nix
   sets ONLY aoide.drachma.{palette,base16,bar,notif,window,wallpaper}
   guarded: lib.mkIf (config.aoide.song == "<song>")
        │
        │  nix build / rebuild bakes the guarded song's drachma
        ▼
stage/drachma.json  ◄── also: `aoide rice preview` stages this ephemerally
   fully resolved — colours concrete, no null            (CONTRACTS.md §4:
        │                                                  atomic writes only)
        │  FileView watches the file; onFileChanged → reload()
        ▼
qml/DrachmaState.qml   (singleton — mirrored under modules/facets/quickshell/qml/)
   hot-reload: every widget's binding updates in one pass, no QML restart
        │
        ▼
every widget reads palette roles off DrachmaState
   (paletteBg / paletteFg / paletteAccent / paletteHot / base16 role map)


covers/<name>                              stage/cover.json
   the shared wallpaper library     ──►     { "path": "/abs/path" }
   rice.nix's wallpaper note          seeded from the song's baked wallpaper;
   points here by literal path        shellbridge/quickshell re-seeds it
        │                             live (AOIDE_WALLPAPER env re-seeds
        │                             across rebuilds)
        └──────────────┬──────────────────────┘
                        ▼
              qml/AoideWallpaper.qml
   FileView on cover.json; falls back to the BAKED song wallpaper
   (immutable store path via AOIDE_WALLPAPER) if cover.json is absent/garbage
```

The `no-song-read` flake check forbids any nix module reading `stage/` at
build time — runtime state can never become load-bearing for the build.

## Creation vs. application

| | writes | reads |
|---|---|---|
| **Creation** (`rice gen` → `lint` → `preview` → `adopt`) | `auditions/` (propose), then `songbook/<song>/` on adopt | `songbook/` + that song's `design/` — always, before every `gen` |
| **Application** (performing an adopted song) | nothing — pure selection | `songbook/<song>/rice.nix` fanned into `stage/drachma.json` at build/preview time |

The swap is one line, host-agnostic (no other edit needed — a song sets
only `aoide.drachma`, no host options, no facet toggles):

```nix
# hosts/<host>/default.nix
aoide.song = "sonata";
```

This re-fans the whole `aoide.drachma` tree for that host. **Replay** =
same score, new host (this line, elsewhere). **Transpose** = same host,
new key (`rice transpose <song> <key>`, planned — swaps among that song's
`palette/` variants).

## Design memory — where it lives

- **House grammar** (cross-cutting, every song instantiates it):
  `songbook/default/design/pantheon.md` — the Pantheon wireframe-depth +
  glyph grammar. Owned by the default rice because the default rice is the
  one thing every song shares.
- **Per-song current state** (this song's key, contrast, surface-element
  values, iteration log): `songbook/<name>/design/intent.md`, e.g.
  `songbook/sonata/design/intent.md`.

Design folders are ordinary content-pipeline folders, pre-approved as
system-owned — Aoide ingests its own design notes so past rice reasoning
is queryable like any other content.

## Related (wiki)

Song-Anatomy · Self-Ricing · Song-Vocabulary · drachma · Snowflake-Anatomy
· Ricing-Protocol
