---
type: concept
created: 2026-07-28
updated: 2026-10-12
tags: [aoide, rice, rice, architecture]
source: "[[references/AOIDE-HANDOFF]]"
---

# Rice Anatomy — the Performed Half's Tree

The sibling of [[Snowflake-Anatomy]] (which maps the frozen `modules/` tree).
This page maps the **rice surface** — the performed half's home on disk, in
two roots: the committed *score* (the rices, in the dev git checkout) and
the *live state* (the stage the desktop reads, off the runtime root). It is a
**role map**: what each subfolder is for, whether it is
committed or runtime, and who writes it. The vocabulary those roles are named
in lives in [[Rice-Vocabulary]]; this page grounds the words in the actual
directories.

The committed score lives in the dev git checkout at `$AOIDE_FLAKE_ROOT/rice/`
(default `~/Aoide/rice`; `strx onboard` links `~/rice` → the checkout's
`rice/` dir, never clobbering an existing file or symlink, and seeds the
checkout's `rices/preferences.md` when absent). The checkout's
`rices/` is the **rice agent's writable committed domain** (house rule
#1, `AGENTS.md`): the agent commits *there* and, for score, nowhere else. The
runtime half hangs off `$AOIDE_ROOT` (absolute-path-wins, default `~/.aoide`):
the stage the desktop reads (`$STRILLIX_ROOT/stage/`), the composed host
rices `strx rice compose` writes (`$AOIDE_ROOT/rices/<name>/`),
saved drafts (`$AOIDE_ROOT/rices/<rice>/drafts/`), plus `run/qml/`,
`state/`, and `log`. `strx rice declare <name>` is the seam between the two
roots: it copies a composed rice from the runtime rices into the checkout's
`rices/<name>/`, less its top-level `history/` and `drafts/` (the
machine's undo history and scratch, which stay behind) — no `git add`, no rebuild; the user gates those.

## The subfolders

| Subfolder | Role | Tree | Written by |
|---|---|---|---|
| `rices/` (checkout) | per-rice homes + cross-cutting design memory | committed score | rice agent |
| `/stage/` | live preview state — the seam the desktop reads | runtime (`$AOIDE_ROOT`) | [[theme]] · `strx rice` · QML |
| `rices/<name>/` | the composed host rices | runtime (`$AOIDE_ROOT`) | `strx rice compose` |
| `rices/<rice>/drafts/` | saved drafts | runtime (`$AOIDE_ROOT`) | `strx rice draft save` |

Everything in the checkout's `rice/` is versioned score; the runtime surface
sits outside it under `$AOIDE_ROOT`, created on demand.

> **The shipped standard rice lives in the rices like any other.**
> The shipped standard, `rices/sonata/`, is upstream-owned and
> evolving; a host performs it by naming it (`aoide.rice = "sonata";`).
> Every other rice is clone-owned, and upstream never touches it. See [[Self-Ricing#The Shipped Baseline Is Guarded, Not
> Frozen]] for the full guarantee.
> `pkgs/strillix-rices` bakes the committed `rices/` tree into
> `share/strillix/rices` as read-only templates (with prebaked
> `manifest.json`/`registry.json`), so a repo-less host — no checkout on disk
> at all — can still `strx rice compose --from <rice>` a shipped rice.

## Committed score — the versioned half

### `rices/<name>/` — one rice each

A rice is SELECTED, not walked: a host module names what it performs
(`habit.rice.declared`) and what it keeps built in to stage without a rebuild
(`habit.rice.available`); `lib/rices.nix` finds `rices/<rice>/` by one
typed scan and `lib/strillix.nix` wires exactly the selected rices' `rice.nix`
files in as bodies, and only the performed one is imported ([[Self-Ricing]],
[[Snowflake-Anatomy]]). Rices committed today: `cadenza` (the phosphor key, a
dark green-CRT console — the rice **yomi-strix performs**), `sonata` (the
shipped standard, a light Greek key with no wallpaper: a marble ground baked from
`palette.bg`), and the rices the rices also carries — `barcarolle` (the
Windows 7 Aero glass), `nocturne`. Each folder
holds:

| Subfolder/file | Holds |
|---|---|
| `rice.nix` | a body (`system`: the ten strillix providers and a `module` setting `aoide.theme.*` (palette + base16 + component tiers + geometry + fonts + wallpaper) and `aoide.layout.*`; `home`: the `strillix` renderer and `hyprland.conf`). **No** host options, no host capabilities — so one score replays at any venue (`CONTRACTS.md §5`, [[Rice-Vocabulary#Replay — any rice, any host]]). The wallpaper note points at a file in the shared `/wallpapers/` library (`../../wallpapers/<file>`), not a per-rice `assets/` dir. |
| `theme.json` | the rice's resolved theme values — the [[theme]] schema: `palette`, `polarity`, `base16`, `bar`/`notif`/`window`, `geometry`. The baked-only tiers (`wallpaper`, `fonts`) live in `rice.nix` alone and have no twin here — a convention, not a gate: the schema never walks a document's top-level keys, so a stray copy would pass `rice lint` and ride the activation seed into the stage file unread. |
| `palette/` | this rice's transpose keys — the palette variants `rice transpose <rice> <key>` swaps among |
| `sounds/` | notification + system sounds (the chimes dimension) |
| `icons/` | per-rice icon overrides |
| `widgets/` | per-rice widget bodies — QML files the staging engine resolves per slot ([[Widget-Maker#The staging engine — a rice overrides desktop chrome]]) |
| `_widgets/` | the widget RECORDS (one plain nix function per slot, rolled up by `_widgets/default.nix`, `lib/rice.nix`) — what `borrow` hands a rice that lends its bodies |
| `design/` | the rice's design memory — `intent.md` (palette rationale, iteration log), ingested like any content ([[Self-Ricing#Rices Discipline — the "Self" in Self-Ricing]]) |

**The rice's `widgets/` is what paints.** Each surface resolves its body by slot
NAME through the staging engine, falling back to sonata
(`pkgs/strillix-shell/qml/slots.md` is the slot catalogue), so sonata carries the
bodies its slots name — bar, calendar, conductor, dock, notifications, notifications-center,
launcher, meters, powermenu, power, terminals, usage, wallpaper,
wallpaper-picker, plus the frame/popout helpers they share
([[Gadget-Dock#What the dock holds]]). A content or sizing edit
([[Widget-Maker#Sizing — content decides, never the screen]]) lands in the
rice's own `widgets/` file: the lane's `pkgs/strillix-shell/qml/` carries the
rice-blind skeleton, the slot/host mechanism, and the preview harness — never
a second copy of a body.

### `rices/` root — cross-cutting design memory

Alongside the per-rice folders, the rices root holds the memory that
crosses every rice: `learnings.md`, `preferences.md`, `update-playbook.md`
(the schema-migration playbook). The agent reads these before every
iteration and appends after every declare/reject — the write-back that is
the "self" in [[Self-Ricing]].

Rice design memory lives in the rices, per rice: each rice's current
design elements live in its own `design/intent.md` (sonata's
records the key, the glass values, and the surface elements as performed).
The cross-cutting Pantheon grammar belonging to the retired `default` rice
lives only as historical reference, at
`docs/Aoide-Wiki/references/pantheon/pantheon-grammar.md` — not in any
live rice's design folder.
The wiki carries only the protocol ([[Ricing-Protocol|Ricing Protocol]], in
`concepts/rice/`) — design content about a rice goes to that rice's `design/`
folder in the repo, not the wiki.

## Runtime state — the gitignored half

### `stage/` — the seam the desktop reads

The live preview/runtime state, hot-reloaded by [[Quickshell]] and never
committed. Written only by **atomic write-temp-then-rename**, so a reader never
sees a torn file (`CONTRACTS.md §4`). Split by owner into two trees, both off
the runtime root (`$AOIDE_ROOT`, default `~/.aoide`):
`$STRILLIX_ROOT/stage/` holds rice/paint staging,
`$AOIDE_ROOT/state/stage/` holds CONDUCTING state. The files:

| File | Holds | Written by | Tree |
|---|---|---|---|
| `theme.json` | the fully-resolved theme values (colours concrete, no `null`) | [[theme]] `emit stage` / `strx rice stage`, the strillix lane's activation seed (`home.activation.aoideSeedStage`), `strx reload` | `/stage/` |
| `wallpaper.json` | the live wallpaper: `{"path": …}` (the rice's own DEFAULT) plus `"pick": true` when it is the user's own choice, `"kind": "static"/"video"/"we"` naming what the pick is (a scene carries `"weId"` instead of a path), and, for a picked still, `"tile": true` when it repeats at 1:1 and `"position": "tile"/"fill"` when `wallpaper set --position` forced one | `strx rice stage` (the rice's derivable wallpaper), `strx wallpaper set`/`wallpaper set --clear`/`wallpaper set --from-skwd`, `strx rice back` | `/stage/` |
| `wallpaper-provider` | one word: the name of the SETTER that paints a rice's wallpaper here — `quickshell` (the shell's own layer) or a provider of the `wallpaper` capability; absent means `quickshell` | the strillix lane's `home.activation.aoideSeedStage` (it publishes the fact `aoide.wallpaper.provider`) | `/stage/` |
| `theme.json` | the DECLARED rice's notes, venue `aoide.theme.override` and the host's geometry applied, `"rice"` naming it — the read-only twin `rice mode declarative` re-pins that rice from ([[theme]]) | the strillix lane's `home.activation.aoideSeedStage` | `/declared/` |
| `venue.json` | the slots the venue's `aoide.theme.override` recolours and the geometry the host sets in the declared rice, per tier (`{}` with neither) — what staging lays over the runtime copy of that rice ([[theme]]) | the strillix lane's `home.activation.aoideSeedStage` | `/declared/` |
| `sessions.json` | the agent-session roster (`sessionId, agent, windowAddress, workspace, cwd, state, startedAt`, optional `parentSessionId`) | [[shellbridge]] + `aoide session` | `state/stage/` |
| `hooks.json` | live Claude Code hook phases | shellbridge + `aoide session` | `state/stage/` |
| `projects.json` | the project-anchor registry | `aoide project` | `state/stage/` |
| `graph.json` | the resolved project/session DAG | restaged automatically on every `aoide graph` mutation | `state/stage/` |

Which files are present is runtime-dependent (e.g. `wallpaper.json` appears once a
wallpaper is staged — the rice's own wallpaper as its default, or the user's pick
on top of it, CONTRACTS.md §4; `AOIDE_WALLPAPER` on the Quickshell unit is the
baked fallback the layer reads when no wallpaper is staged, see
[[Quickshell]]). Who paints that pick is the host's choice, not the rice's: the
word in `wallpaper-provider` names the provider — `quickshell`, the shell's own
layer, or an external engine's provider ([[skwd-wall]]) — and an external provider
means the shell's layer paints nothing while a pick applies. Both stage dirs resolve via the same `AOIDE_STAGE_DIR`
absolute-path override, so relocating it relocates both trees at once; with
no override each falls back to its own default under `$AOIDE_ROOT`
(`stage_dir()` → `$STRILLIX_ROOT/stage`, `conducting_stage_dir()` →
`$AOIDE_ROOT/state/stage`, itself `$AOIDE_STATE_DIR` when absolute else
`$AOIDE_ROOT/state` — [[shellbridge]]). On first run the binaries migrate any
pre-existing `~/Aoide/{rice/stage,state,log}` trees into `$AOIDE_ROOT`
(`fs::migrate_root_once`). The flake's `rice-runtime-untracked` check forbids
any nix module reading `/stage/` at build time, so runtime state can
never become load-bearing for the build.

## Repo Layout (the rice surface)

```
$AOIDE_FLAKE_ROOT/rice/  (dev git checkout — committed score, default ~/Aoide/rice)
├── rices/            committed rices + cross-cutting design memory
│   ├── sonata/          the shipped standard — upstream-owned, evolving; a LIGHT Greek key with no wallpaper (a marble ground baked from palette.bg)
│   │   ├── rice.nix · theme.json
│   │   ├── palette/ · sounds/ · icons/ · widgets/ · design/
│   ├── learnings.md · update-playbook.md   ← sparse today
├── wallpapers/              shared wallpaper library — yuki-sonata.png
└── rice.md              repo-local map of this tree

$AOIDE_ROOT/             (runtime root, default ~/.aoide — created on demand)
├── rice/
│   ├── stage/           live preview state — theme.json · wallpaper.json · mode.json · grimoire.json
│   ├── declared/        the declared twin, the venue's slots and geometry, the declared terminal opacity — theme.json · venue.json · terminal-opacity.conf, written by the activation seed; rices/<rice>.json, each built-in rice's refresh record, written by `strx rice refresh`
│   └── rices/        composed host rices (rice compose) · <rice>/drafts/ (rice draft save)
├── state/               conducting state — state/stage/ holds sessions/hooks/projects/graph
├── run/qml/             live-deployed QML tree the desktop shell reads
└── log                  the audit log
```

**The root is closed** ([[Snowflake-Anatomy]]): new committed content lands at its
designated place inside the checkout tree — every per-rice asset, key, sound, icon, and
widget body lands inside that rice's `rices/<name>/` — never a new
top-level `rice/` dir. The lookup is the Rice Map
([[Rice-Vocabulary#The Rice Map]]).

## Related

- [[Rice-Vocabulary]] — the vocabulary these roles are named in
- [[Snowflake-Anatomy]] — the frozen sibling (`modules/`)
- [[Self-Ricing]] — the rice loop, rices discipline, and coverage tiers
- [[theme]] — the engine that resolves/lints/emits the stage file, the `aoide.theme` seam `theme.json` carries
- [[shellbridge]] · [[Session-Graph]] — the writers of the runtime stage files
- [[Ricing-Protocol]] — the ricing protocol this page's per-rice design memory
  supports
- [[Widget-Maker]] — the staging engine, which resolves a rice's `widgets/` files to live desktop chrome
- [[Gadget-Dock]] — the dock/gadget bodies, painted by the owning rice's `widgets/`
