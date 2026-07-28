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

**`aoide make` itself is planned, not shipped** (`aoide schema --json` marks it
`not-implemented`, exit 64) — this is the target shape, not a running command
today. Until it lands, a development agent builds the same dendrite + widget +
adapter by hand, previews it live, and adopts it exactly as described below;
[[Gadget-Dock]]'s seven gadgets and [[Terminal-Commander]] are the shipped
proof the pattern works. Preview is the sketch; adopt is the truth — identical
discipline to ricing. A bad generation can never reach the running system
without the [[Governance|gate]], and every step lands in the single audit log.

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
