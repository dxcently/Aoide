# `rice/` — orientation

The performed half's home on disk: committed *score* (`rices/`) plus
gitignored *live state* (`stage/`, `auditions/`). See the wiki's
Rice-Anatomy for the authoritative role map and Self-Ricing for the rice
loop (`gen → lint → preview → adopt`) this tree exists to serve. This file
is a repo-local map, not a replacement for either.

## Tree

```
rice/
├── rices/                   committed score — per-rice homes + cross-cutting design memory
│   │                              (sonata, the shipped baseline, is upstream-owned + evolving, like
│   │                              nucleus/dendrites; every OTHER rice is clone-owned, never adopted-over)
│   ├── <name>/                  one rice per folder, e.g. sonata (found by one typed scan, built in only when a host selects it)
│   │   ├── rice.nix                the rice's body: system (ten strillix providers + theme) and home (strillix, hyprland.conf)
│   │   ├── hyprland.conf           the rice's binds, unbinds, layout and animations (no theme keys)
│   │   ├── theme.json             this rice's resolved theme values
│   │   ├── palette/                (planned) transpose keys — variants `rice transpose` swaps among
│   │   ├── sounds/                 (planned) notification + system sounds (chimes dimension)
│   │   ├── icons/                  (planned) per-rice icon overrides
│   │   ├── widgets/                (planned) per-rice widget bodies
│   │   └── design/                 this rice's CURRENT key + elements, as performed
│   │       ├── intent.md             palette rationale, contrast table, iteration log
│   │       └── <rice-specific>.md    e.g. sonata's `greek-grammar.md` — a rice's own
│   │                                  cross-cutting grammar, where it diverges from Pantheon
│   ├── learnings.md              (planned) cross-cutting observations
│   ├── preferences.md            (planned) accumulated from adopt/reject decisions
│   └── update-playbook.md        (planned) schema-migration playbook
├── wallpapers/                     shared wallpaper library — any rice's rice.nix points here by literal path
│                                  (../../wallpapers/<file>), not a per-rice assets/ dir
├── stage/                      LIVE runtime state — gitignored, atomic write-temp-then-rename only
│   ├── theme.json                fully-resolved theme (no nulls) — hot-reloaded by ThemeState.qml
│   ├── wallpaper.json                 live wallpaper seed { "path": ... } — read by AoideWallpaper.qml
│   └── sessions.json / hooks.json / projects.json / graph.json   session-graph state (shellbridge, aoide graph)
└── auditions/                  (planned) the propose gate for generated-but-unadopted rices — gitignored
```

Root is closed: every per-rice asset, key, sound, icon, widget body lands
inside that rice's `rices/<name>/` — never a new top-level `rice/` dir
(the Rice Map, wiki `Rice-Vocabulary`).

## Performance path — how a rice reaches the screen

```
rices/<rice>/rice.nix
   a body: system.module sets ONLY strillix.theme.{palette,base16,bar,notif,window,wallpaper}
   selected by habit.rice.declared = "<rice>" (no guard: only the performed body is read)
        │
        │  nix build / rebuild bakes the guarded rice's theme
        ▼
stage/theme.json   ◄── also: `strx rice stage` stages this ephemerally
   fully resolved — colours concrete, no null            (CONTRACTS.md §4:
        │                                                  atomic writes only)
        │  FileView watches the file; onFileChanged → reload()
        ▼
qml/ThemeState.qml    (singleton — shipped by pkgs/strillix-shell/qml/)
   hot-reload: every widget's binding updates in one pass, no QML restart
        │
        ▼
every widget reads palette roles off ThemeState
   (paletteBg / paletteFg / paletteAccent / paletteHot / base16 role map)


wallpapers/<name>                              stage/wallpaper.json
   the shared wallpaper library     ──►     { "path": "/abs/path" }
   rice.nix's wallpaper note          seeded from the rice's baked wallpaper;
   points here by literal path        shellbridge/quickshell re-seeds it
        │                             live (AOIDE_WALLPAPER env re-seeds
        │                             across rebuilds)
        └──────────────┬──────────────────────┘
                        ▼
              qml/AoideWallpaper.qml
   FileView on wallpaper.json; falls back to the BAKED rice wallpaper
   (immutable store path via AOIDE_WALLPAPER) if wallpaper.json is absent/garbage
```

The `rice-runtime-untracked` flake check fails the tree if a `rice/` runtime
dir is committed, so runtime state can never become load-bearing for the build.

## Creation vs. application

| | writes | reads |
|---|---|---|
| **Creation** (`compose` → `mode stage` → `lint` → `draft save` → `declare`) | `auditions/` (propose), then `rices/<rice>/` on declare | `rices/` + that rice's `design/` — always, before iterating |
| **Application** (performing an adopted rice) | nothing — pure selection | `rices/<rice>/rice.nix` fanned into `stage/theme.json` at build/preview time |

The swap is one line, host-agnostic (no other edit needed — a rice sets
only `strillix.theme`, no host options, no lane toggles):

```nix
# hosts/<host>/default.nix
habit.rice.declared = "sonata";
```

This re-fans the whole `strillix.theme` tree for that host. **Replay** =
same score, new host (this line, elsewhere). **Transpose** = same host,
new key (`rice transpose <rice> <key>`, planned — swaps among that rice's
`palette/` variants).

## Design memory — where it lives

- **House grammar** (cross-cutting, formerly instantiated by every rice):
  retired along with the `default` rice that owned it — kept as historical
  reference at `docs/Aoide-Wiki/references/pantheon/pantheon-grammar.md`,
  outside the rices entirely. `sonata` (the shipped baseline now) draws
  its own grammar instead: `rices/sonata/design/greek-grammar.md`.
- **Per-rice current state** (this rice's key, contrast, surface-element
  values, iteration log): `rices/<name>/design/intent.md`, e.g.
  `rices/sonata/design/intent.md`.

Design folders are ordinary content-pipeline folders, pre-approved as
system-owned — Aoide ingests its own design notes so past rice reasoning
is queryable like any other content.

## Related (wiki)

Rice-Anatomy · Self-Ricing · Rice-Vocabulary · theme · Snowflake-Anatomy
· Ricing-Protocol
