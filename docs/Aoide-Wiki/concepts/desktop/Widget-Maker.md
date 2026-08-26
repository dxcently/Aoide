---
type: concept
created: 2026-07-26
updated: 2026-08-25
tags: [aoide, extensibility, declarative, widget, agent]
---

# Widget Maker — Extensible and Declarative by Construction

AoideOS integrates whatever you see fit and surfaces it as a widget, a command, or a system hook. [[Feature-Set]] lists what ships; those are exemplars of one underlying capability: making more of them, on demand, declaratively.

## Why: the agent is a coding agent

[[Melete]] is a coding agent, so the extension surface is the whole system rather than a fixed plugin slot the authors foresaw. Ask for an integration and the agent writes one — a nix module, a QML widget, an adapter — then previews it and, on approval, adopts it.

This is the same engine as [[Self-Ricing]], generalized: self-ricing is the agent generating *appearance* declaratively, widget-making is the agent generating *capability* declaratively.

## Declarative, not scripted

Every integration lands as declarative Nix:

- A new capability is a **dendrite** ([[Snowflake-Anatomy]]) — a nix module the walker auto-registers; enabling it is one host flag, no plugin registry to edit. Packages work the same way: `pkgs/<name>/default.nix` plus `lib/pkgs.nix` self-registers the flake output, host/vm overlays, and a `pkg-<name>` check.
- Its UI is a **Quickshell widget** ([[Quickshell]]) reading `song/stage/*.json`, themed only by [[livery]].
- Its wiring is an **adapter** on the [[aoided]] event stream ([[Desktop-Architecture]]).
- The result is reproducible and diffable, removed by flipping the same flag — system state is always fully described by the flake.

## The make-a-widget loop

**Status:** specified; `aoide make` not implemented (`aoide schema --json` marks it `not-implemented`, exit 64).

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

A development agent builds the same dendrite + widget + adapter by hand, previews it live, and adopts it the same way; [[Gadget-Dock]]'s gadgets and [[Terminal-Commander]] are built through this loop. Preview is the sketch; adopt is the truth. A bad generation can never reach the running system without the [[Governance|gate]], and every step lands in the single audit log.

## The hard line: a widget is a render surface

House rule 7 ([[Plugin-Architecture#The corollary for Quickshell: render surfaces only]]), applied at the widget level. A widget **paints**; it is never where a capability *lives*. State, policy, IPC, and system access sit behind an agnostic bridge — a CLI command, a stage file (`CONTRACTS.md` §4), an IPC socket — reachable with no desktop running. The widget picks that bridge up by name and draws it: a bridge lands first, the QML picks it up second. The test: delete every `.qml` in the repo — a capability not reachable from a terminal after that was in the wrong place. Sonata's own `design/widget-structure.md` carries the same rule for the ricing agents that read it directly.

## The staging engine — a song overrides desktop chrome

A narrower, sibling mechanism to the make-a-widget loop: a **song** ([[Song-Anatomy]]) replacing a piece of *existing* chrome with its own QML, rather than the agent building a new capability. **The staging engine** (`StagingEngine.qml`) resolves the active song's livery tokens to per-slot QML; a host surface embeds one of two fixed per-slot anchors, which asks the engine whether the active song dressed that slot and loads its file, or falls back to shared chrome:

- **`WidgetSlot.qml`** — an `Item`. Sizes itself off the loaded child and parents it into an existing layout, for a slot whose widget root is an ordinary `Item`.
- **`SurfaceSlot.qml`** — a non-visual `QtObject`, for a slot whose root IS its own `PanelWindow` (own Overlay layer, own WlrLayershell namespace, own keyboard focus, own `GlobalShortcut`). Resolves and creates the widget, exposing the live instance as `.item` for the host to call directly.

Both anchors build the widget via `Qt.createComponent` → `Component.createObject`, never a declarative `Loader` — QML resolves `required property` at object-creation time, and a `Loader` only lets properties be assigned after the item has already loaded. Both resolve through the same baseline-fallback chain (`StagingEngine.resolveSong`, `CONTRACTS.md §5`): the active song's own file if it authored the slot, else sonata's (the shipped baseline every song falls back to), else — `WidgetSlot` only — the anchor's own facet-side `fallback` Component, else nothing. `SurfaceSlot` has no facet-side fallback, since a window-owning slot moves as one whole unit with nothing left behind to fall back to.

A song authors a slot by dropping `songbook/<name>/widgets/<slot>.qml` ([[Song-Anatomy]]); the build imposes no fixed slot enum — the quickshell facet copies any `widgets/*.qml` file a committed song carries into `$out/qml/songs/<name>/<slot>.qml`, alongside a generated `manifest.json` (`{song: [slots]}`, distinct from the type-declaration `registry.json` below) the engine reads to answer `has(song, slot)`/`source(song, slot)`. A slot only renders once a host surface embeds a `WidgetSlot`/`SurfaceSlot` anchor naming it — presence is the host's call, which variant fills it is the song's, via the manifest. Six slots are wired today: `calendar`, `herald-center`, `bar` via `WidgetSlot`; `herald` (the notification popup), `powermenu`, and `launcher` via `SurfaceSlot`. The full catalog lives in `modules/facets/quickshell/qml/slots.md`, alongside the shape every widget file follows: an `Item` root for a `WidgetSlot` widget, a `PanelWindow` root for `SurfaceSlot`; `required property var livery`/`bridge` injected by every anchor unconditionally, slot-specific extras as their own `required property`, never `config.*`.

Because `LiveryState.songName` is what `lyra rice stage <name>` stages, switching the staged song hot-swaps every anchor's loaded body live — no rebuild, no restart — the same discipline as the rice loop itself, applied to widget bodies instead of colour. Adding a *new* song's widget files to the carried set still needs a rebuild; swapping which already-carried song is active does not.

Editing the *content* of an already-carried widget file needs no restart: `lyra rice stage` re-syncs the changed widget file into `run/qml/songs/<name>/` and, if that changed something, triggers `AoideIpc.qml`'s `Quickshell.reload(false)` via `lyra quickshell reload`, rebuilding the scene fresh so the edit renders live — closing the gap left by Quickshell's own file watcher, which doesn't track `Qt.createComponent`-loaded QML ([[Quickshell]]).

**Containment invariant** (`CONTRACTS.md §5`): a loaded song widget receives only `livery` and `bridge`, plus whatever slot-specific extras the anchor declares — never nix `config.*`. A song's `rice.nix` still sets only `aoide.livery`; widget bodies are committed QML files the build carries, not nix options, so a song widget is structurally incapable of reaching host/facet options through this surface. `greeter`/`lockscreen`/`osd`/`nowPlaying` remain unbuilt slots — no host anchor exists for them yet.

## A third way onto the screen — the declared widget-type registry

The staging engine above resolves a slot NAME the facet already anchored. `aoide.arrangement.widgets` (`modules/nucleus/options.nix`, sibling of `aoide.livery`) is a third, independent way a song puts something on screen: a song **registers an entirely new slot** via nix — one the facet never anchored — instead of dressing one that already exists.

A song's `rice.nix` declares `aoide.arrangement.widgets.<slot> = { kind = "surface" | "dock"; … }`; the declaration is twin-written into that song's `livery.json` under a flat `.widgets` key and validated strictly by `rice lint` (`pkgs/aoide/crates/song/src/livery/schema.rs`) — closed, and the two kinds' extra fields are mutually exclusive. `kind = "surface"` owns its own `PanelWindow`/layer-shell surface (namespace/layer/shortcut/blur fields); `kind = "dock"` mounts as an `Item` into `AoidePanel`'s existing gadget column alongside the shipped gadgets, ordered by an `order` field. The slot's QML body still lives at the ordinary `song/songbook/<name>/widgets/<slot>.qml` path.

This is reuse, not a fourth rendering mechanism: a declared slot resolves through the same `WidgetSlot`/`SurfaceSlot` primitives above. `SongSurfaces.qml` hosts every `kind = "surface"` entry as an `Instantiator` of `SurfaceSlot`s; `SongGadgets.qml` hosts every `kind = "dock"` entry as a `Repeater` of `WidgetSlot`s, sorted by `order`. A declared slot with no actual `widgets/<slot>.qml` body warns and renders nothing, the same inert posture as an unauthored anchored slot. `song/songbook/etude/` declares one `kind = "surface"` entry (`demo`) and exercises the whole pipeline: nix option → build-time `registry.json` walk → `rice lint` → `rice stage` hot-sync → render. Full mechanism: `CONTRACTS.md` §5; full field/table reference: `modules/facets/quickshell/qml/slots.md`.

## What this makes Aoide

- **Integrate whatever you see fit** — messaging, fleet control, timers, a sensor, a webhook, a home-automation panel: if it can be a nix module + a widget, the agent can build it. [[Feature-Set]] is the shipped starter set, not the ceiling.
- **Self-extending** — the agent grows the system's capabilities the way it grows its taste in self-ricing: by doing, then writing it down declaratively.
- **Always describable** — every extension is declarative Nix under `modules/`, so the whole machine — look and capability — is one reproducible clone.

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
