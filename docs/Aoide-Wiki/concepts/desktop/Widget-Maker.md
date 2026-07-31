---
type: concept
created: 2026-07-26
updated: 2026-07-28
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
  themed only by [[drachma]].
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
  lint / checks              contracts + drachma schema + no cross-module reads
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

## The staging engine — a song overrides desktop chrome

A narrower, sibling mechanism to the make-a-widget loop above: not the agent
building a *new* capability, but a **song** ([[Song-Anatomy]]) replacing a
piece of *existing* chrome with its own QML. **The staging engine**
(`StagingEngine.qml`) resolves the active song's drachma tokens
(`DrachmaState`/`notes`) to per-slot QML; **`WidgetSlot.qml`** is the fixed
per-slot anchor a host surface embeds, which asks the engine whether the
active song dressed that slot and loads its file, or falls back to shared
chrome. Two slots are live today: `calendar` (`AoideBar`'s calendar popout)
and `notifications` (`AoideNotifications`'s per-card repeater, falling back
to the shared `NotificationCard` when a song hasn't authored one — true of
every song so far).

A song authors a slot by dropping `songbook/<name>/widgets/<slot>.qml`
([[Song-Anatomy]]); the quickshell facet's build carries every committed
song's widget files into `$out/qml/songs/<name>/` alongside a generated
`manifest.json`, which the engine reads to answer `has(song, slot)` /
`source(song, slot)`. Because `DrachmaState.songName` is what
`aoide rice preview <name>` stages, switching the previewed song
**hot-swaps every `WidgetSlot`'s loaded body live — no rebuild, no
restart** — the same preview-without-rebuild discipline as the rice loop
itself ([[Self-Ricing]]), just applied to widget bodies instead of colour.
Adding a *new* song's widget files to the carried set still needs a rebuild
(the facet has to know to copy them); swapping which already-carried song is
active does not.

**Containment invariant** (`CONTRACTS.md §5`): a loaded song widget receives
only `notes` (`DrachmaState`) and `bridge` (`ShellBridge`), plus whatever
slot-specific extras the anchor declares (e.g. notifications' `notification`)
— never nix `config.*`. This doesn't loosen the song-shape rule elsewhere in
this page: a song's `rice.nix` still sets only `aoide.drachma`; widget bodies
are committed QML files the build carries, not nix options, so a song widget
is structurally incapable of reaching host/facet options through this
surface. `greeter`/`lockscreen`/`osd`/`nowPlaying` remain unbuilt slots — no
host anchor exists for them yet.

## What this makes Aoide

- **Integrate whatever you see fit** — messaging, fleet control, timers, a
  sensor, a webhook, a home-automation panel: if it can be a nix module + a
  widget, the agent can build it. [[Feature-Set]] is the shipped starter set, not
  the ceiling.
- **Self-extending** — the agent grows the system's capabilities the way it grows
  its taste in self-ricing: by doing, then writing it down declaratively.
- **Always describable** — because every extension is declarative Nix under
  `modules/`, the whole machine — look *and* capability — is one reproducible
  fork you can merge, diff, and roll back.

## Related

- [[Feature-Set]]
- [[Self-Ricing]]
- [[Snowflake-Anatomy]]
- [[Agent-Interface]]
- [[Full-Architecture]]
- [[Melete]]
- [[Gadget-Dock]]
- [[Widget-Bridge-Contract]]
- [[Song-Anatomy]] — where a song's `widgets/` folder lives on disk
