---
type: concept
created: 2026-07-26
updated: 2026-08-17
tags: [aoide, extensibility, declarative, widget, agent]
---

# Widget Maker — Extensible & Declarative by Construction

The headline capability of **AoideOS** (the distribution — not the shell-only
Aoide core, whose job is orchestration): a **specialized widget maker for your
system** — a platform whose whole job is to integrate whatever you see fit and
surface it as a widget, a command, or a system hook. It ships useful features
(see [[Feature-Set]]), but those are exemplars. The real product is the
**ability to make more of them, on demand, declaratively.**

## Why Aoide can do this: the agent is a coding agent

[[Melete]] is, at the end of the day, a **coding agent** — and that is the
leverage. Most desktops are extensible only through whatever plugin API the
authors foresaw. Aoide is extensible through **code the agent writes**, so the
extension surface is the entire system, not a fixed plugin slot. Ask for an
integration and the agent doesn't hunt for a plugin; it **writes** one — a nix
module + a QML widget + an adapter — then previews it and, on your approval,
adopts it.

This is the same engine as [[Self-Ricing]], generalized. Self-ricing is the agent
generating *appearance* declaratively; widget-making is the agent generating
*capability* declaratively. One loop, two kinds of output.

## Declarative, not scripted

Every integration lands as **declarative Nix**, never an imperative pile of
scripts:

- A new capability is a **dendrite** ([[Snowflake-Anatomy]]) — a nix module the
  walker auto-registers. Enabling it is one host flag; there is no plugin
  registry to hand-edit. This extends to **packages** too: a capability that
  ships a binary drops `pkgs/<name>/default.nix` and `lib/pkgs.nix` self-registers
  it (flake output, host + vm overlays, and a `pkg-<name>` check) — again no
  hand-list to edit.
- Its UI is a **Quickshell widget** ([[Quickshell]]) reading `song/stage/*.json`,
  themed only by [[livery]].
- Its wiring is an **adapter** on the [[aoided]] event stream
  ([[Desktop-Architecture]]).
- The result is reproducible, diffable, and removed by flipping the same flag —
  system state is always fully described by the flake, never by accumulated side
  effects.

## The make-a-widget loop

**Status:** specified; `aoide make` not implemented (`aoide schema --json` marks
it `not-implemented`, exit 64).

Extending the system reuses the rice loop's shape ([[Self-Ricing]]):

```
  aoide make <intent>        e.g. "show my scheduled jobs" · "bridge notifs to Telegram"
        │  agent writes: dendrite (nix) + widget (QML) + adapter
        ▼
  lint / checks              contracts + livery schema + no cross-module reads
        │ pass
        ▼
  preview                    widget renders live from song/stage/*.json (no rebuild)
        │
        ▼
  adopt  ◄─── User gates     committed as a dendrite; gated rebuild makes it permanent
```

A development agent builds the same dendrite + widget + adapter by hand,
previews it live, and adopts it exactly as described below; [[Gadget-Dock]]'s
seven gadgets and [[Terminal-Commander]] are the shipped proof the pattern
works. Preview is the sketch; adopt is the truth — identical discipline to
ricing. A bad generation can never reach the running system without the
[[Governance|gate]], and every step lands in the single audit log.

## The hard line: a widget is a render surface

House rule 7 ([[Plugin-Architecture#The corollary for Quickshell: render
surfaces only]]), applied at the widget level. A widget **paints**; it is
never the place a capability *lives*. State, policy, IPC, and system access
sit behind an agnostic bridge — a CLI verb, a stage file (`CONTRACTS.md`
§4), an IPC socket — that a shell can reach with no desktop running. The
widget picks that bridge up by name and draws it: a new API lands as a
bridge FIRST, the QML picks it up second, never the reverse. The test for a
file never seen before — delete every `.qml` in the repo; a capability not
reachable from a terminal after that was in the wrong place. Sonata's own
`design/widget-structure.md` carries the same rule for the ricing agents
that read that file directly.

## The staging engine — a song overrides desktop chrome

A narrower, sibling mechanism to the make-a-widget loop above: not the agent
building a *new* capability, but a **song** ([[Song-Anatomy]]) replacing a
piece of *existing* chrome with its own QML. **The staging engine**
(`StagingEngine.qml`) resolves the active song's livery tokens
(`LiveryState`/`livery`) to per-slot QML; a host surface embeds one of two
fixed per-slot anchors, which asks the engine whether the active song
dressed that slot and loads its file, or falls back to shared chrome:

- **`WidgetSlot.qml`** — an `Item`. Sizes itself off the loaded child and
  parents it into an existing layout (a popout body, a repeater delegate,
  …), for a slot whose widget root is an ordinary `Item`.
- **`SurfaceSlot.qml`** — a non-visual `QtObject`. For a slot whose root IS
  its own `PanelWindow` — its own Overlay layer surface, own WlrLayershell
  namespace, own keyboard focus, own `GlobalShortcut`. Does no visual
  parenting/sizing; it resolves and creates the widget and exposes the live
  instance as `.item` for the host to call directly (e.g. the bar's clef
  calling `.toggle()` on the powermenu slot's live instance).

Both anchors build the widget the same way —
`Qt.createComponent` → `Component.createObject(parent, initialProperties)`,
never a declarative `Loader` — because QML resolves `required property` at
OBJECT CREATION time, and a `Loader` only lets you assign properties after
the item has already loaded, too late to satisfy a `required` one. Both
resolve through the SAME baseline-fallback chain (`StagingEngine.resolveSong`,
`CONTRACTS.md §5`): the active song's own file if it authored the slot, else
**sonata's** (the shipped baseline every song falls back to), else —
`WidgetSlot` only — the anchor's own facet-side `fallback` Component, else
nothing. `SurfaceSlot` has no facet-side fallback: sonata's file IS the
floor, since a window-owning slot moves as one whole unit with nothing left
behind in the facet to fall back to.

A song authors a slot by dropping `songbook/<name>/widgets/<slot>.qml`
([[Song-Anatomy]]) — the build imposes no fixed slot enum: the quickshell
facet's build copies **any** `widgets/*.qml` file a committed song carries
into `$out/qml/songs/<name>/<slot>.qml`, keyed by filename, alongside a
generated `manifest.json` (`{song: [slots]}`) the engine reads to answer
`has(song, slot)` / `source(song, slot)`. Being carried is not being
rendered, though: a slot only shows on screen once a host surface embeds a
`WidgetSlot` or `SurfaceSlot` anchor naming that exact slot — presence is
the host's call (which surfaces exist to be dressed), which variant fills an
enabled slot is the song's (via the manifest). Five slots are wired today:
`calendar`, `notifications`, and `bar` via `WidgetSlot`; `powermenu`
(internally "Exodos" — `song/songbook/sonata/widgets/powermenu.qml`,
screenshotted standalone via `ExodosPreview.qml`) and `launcher` via
`SurfaceSlot`. The full catalog — which slot names have a wired anchor,
which anchor kind, what extra properties each anchor passes, and the
WlrLayershell namespace each `SurfaceSlot` window carries — lives in
`modules/facets/quickshell/qml/slots.md`, alongside the shape every widget
file follows: an `Item` root for a `WidgetSlot` widget (sized off its
`implicit*`; a `SurfaceSlot` widget's root is a `PanelWindow` instead),
`required property var livery`/`bridge` injected by every anchor
unconditionally, any slot-specific extras declared as their own `required
property`, and never `config.*`.

Because `LiveryState.songName` is what `aoide rice stage <name>` stages,
switching the staged song **hot-swaps every anchor's loaded body live — no
rebuild, no restart** — the same stage-without-rebuild discipline as the
rice loop itself ([[Self-Ricing]]), just applied to widget bodies instead of
colour. Adding a *new* song's widget files to the carried set still needs a
rebuild (the facet has to know to copy them); swapping which already-carried
song is active does not.

That's the *song-switch* case — a pointer swap onto an already-carried body,
which never needed a restart. Editing the *content* of an already-carried
widget file (the same song stays staged, its `widgets/<slot>.qml` changes) is
a separate case: it used to require a manual `systemctl --user restart
aoide-quickshell.service`, because Quickshell's built-in file watcher doesn't
track `Qt.createComponent`-loaded QML at all ([[Quickshell]]). `aoide rice
stage` now closes that gap too — it re-syncs the changed widget file into
`run/qml/songs/<name>/` and, if that sync actually changed something, triggers
`AoideIpc.qml`'s `Quickshell.reload(false)` via `aoide quickshell reload` under the
hood, rebuilding the whole scene fresh so the edit renders without a restart
(the same IPC hot-reload mechanism described in [[Quickshell]]). The "new
song's widget files still need a rebuild" caveat above is untouched by
this — that's the Nix build carrying new files into the store, not rendering.

**Containment invariant** (`CONTRACTS.md §5`): a loaded song widget receives
only `livery` (`LiveryState`) and `bridge` (`ShellBridge`), plus whatever
slot-specific extras the anchor declares (e.g. notifications' `notification`,
launcher's `clipboard`/`ledger`) — never nix `config.*`. This doesn't loosen
the song-shape rule elsewhere in this page: a song's `rice.nix` still sets
only `aoide.livery`; widget bodies are committed QML files the build
carries, not nix options, so a song widget is structurally incapable of
reaching host/facet options through this surface. `greeter`/`lockscreen`/
`osd`/`nowPlaying` remain unbuilt slots — no host anchor exists for them
yet.

## A third way onto the screen — the declared widget-type registry

The staging engine above resolves a slot NAME the facet already anchored.
`aoide.arrangement.widgets` (`modules/nucleus/options.nix`, sibling of
`aoide.livery` — [[Song-Anatomy]]) is a third, independent way a song puts
something on screen, beyond the two above: filling an anchor the facet
already wired, or owning a whole surface like `bar.qml`/`herald-center`
outright. Here a song **registers an entirely new slot** via nix — a slot
the facet never anchored — instead of dressing one that already exists.

A song's `rice.nix` declares `aoide.arrangement.widgets.<slot> = { kind =
"surface" | "dock"; … }`; the declaration is twin-written into that song's
`livery.json` under a flat top-level `.widgets` key (same twin-write every
other song field uses) and validated strictly by `rice lint`
(`pkgs/aoide/crates/song/src/livery/schema.rs`) — closed, and the two kinds'
extra fields are mutually exclusive. `kind = "surface"` owns its own
`PanelWindow`/layer-shell surface (namespace/layer/shortcut/blur fields,
powermenu/launcher-style); `kind = "dock"` mounts as an `Item` into
`AoidePanel`'s existing gadget column alongside the shipped gadgets (an
`order` field only, for deterministic layout among multiple declared `dock`
entries — the registry has no stable key order across the build-time nix
walk and the native hot-sync). The slot's QML body still lives at the
ordinary `song/songbook/<name>/widgets/<slot>.qml` path.

Crucially, this is **reuse, not a fourth rendering mechanism**: a declared
slot resolves through the exact same `WidgetSlot`/`SurfaceSlot` primitives
above. `SongSurfaces.qml` hosts every `kind = "surface"` entry as an
`Instantiator` of `SurfaceSlot`s; `SongGadgets.qml` hosts every `kind =
"dock"` entry as a `Repeater` of `WidgetSlot`s, sorted by `order` and
mounted as the column's last children. Both keep the fixed injected-prop
contract (`livery` + `bridge` only) and the same baseline-fallback resolution
`resolveSong` already gives the anchored catalog — a declared slot with no
actual `widgets/<slot>.qml` body warns (`[aoide/surfaceslot]` /
`[aoide/songgadgets]`) and renders nothing, the same inert-not-error posture
as an unauthored anchored slot. `song/songbook/etude/` is the worked
example: it declares one `kind = "surface"` entry (`demo`) and proves the
whole pipeline — nix option → build-time `registry.json` walk (a build
artifact parallel to, but never merged into, `manifest.json` — a different
question: which slots are TYPE declarations, not which slot bodies exist) →
`rice lint` → `rice stage`/`preview` hot-sync → render — end to end. Full
mechanism: `CONTRACTS.md` §5's "declared widget-type registry" subsection;
full field/table reference: `modules/facets/quickshell/qml/slots.md`.

## What this makes Aoide

- **Integrate whatever you see fit** — messaging, fleet control, timers, a
  sensor, a webhook, a home-automation panel: if it can be a nix module + a
  widget, the agent can build it. [[Feature-Set]] is the shipped starter set, not
  the ceiling.
- **Self-extending** — the agent grows the system's capabilities the way it grows
  its taste in self-ricing: by doing, then writing it down declaratively.
- **Always describable** — because every extension is declarative Nix under
  `modules/`, the whole machine — look *and* capability — is one reproducible
  clone you can merge, diff, and roll back.

## Related

- [[Plugin-Architecture]] — the design philosophy this page's mechanisms implement
- [[Feature-Set]]
- [[Self-Ricing]]
- [[Snowflake-Anatomy]]
- [[Agent-Interface]]
- [[Full-Architecture]]
- [[Melete]]
- [[Gadget-Dock]]
- [[Widget-Bridge-Contract]]
- [[Song-Anatomy]] — where a song's `widgets/` folder lives on disk
