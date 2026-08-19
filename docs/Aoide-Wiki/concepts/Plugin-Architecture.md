---
type: concept
created: 2026-08-19
tags: [aoide, architecture, extensibility, plugin, design]
source: "[[references/AOIDE-HANDOFF]]"
---

# Plugin Architecture — Everything Enters by Existing

Not a versioned interface — the rule the versioned interfaces (`CONTRACTS.md`
§1–§7) are downstream of, and house rule 7 in `AGENTS.md`/`aoide guide`.
**A capability enters Aoide by *existing* at a conventional path, declares
what it needs by *name*, and is removable without a trace.** Nothing enters
by being added to a list.

## The rule, and where the repo already runs it

- **Discovery by existing, not by import list.** `lib/walk.nix` finds
  `modules/dendrites/*`, `modules/facets/*`, `pkgs/*`, and
  `song/songbook/*/rice.nix` by walking the tree ([[Codebase#The walker and
  host assembly (`lib/`)]], [[Snowflake-Anatomy]]); a new folder is the whole
  registration, no import list to hand-edit.
- **Self-gating, not switched from outside.** Each discovered module gates
  itself on its own `enable`/`aoide.song`, rather than a parent module
  turning it on.
- **Resolution by name, not by import.** A widget resolves through
  `StagingEngine.resolveSong(song, slot)` — by slot *name*, falling back to
  sonata — so no host surface ever imports a concrete widget
  ([[Widget-Maker#The staging engine — a song overrides desktop chrome]]).
- **A closed, named service set.** Facets read `aoide.livery` and
  `aoide.arrangement` and nothing else — a closed pair of named services,
  never another module's internals.

## Spatial and temporal composability

Two names for the two halves of the rule, taken from **Cordis** — *A
Programming Paradigm for Spatiotemporal Composability* (Shi, Zhang & Cui;
preprint 2026-08-13, `github.com/cordiverse/paper`), the plugin kernel under
DeepSeek Harness, where the model adapter, tool registry, session log, and
agent loop are all swappable the same way. Cited for the vocabulary, not
adopted as a framework — Aoide's own mechanisms above predate the citation.

- **Spatial composability** — a component declares its dependencies by
  service *name* and waits for them, instead of importing an implementation.
  Aoide's spatial seam: `aoide.livery`/`aoide.arrangement`, slot names, the
  stage files (`CONTRACTS.md` §4), and the `aoide schema --json` tree both
  doors generate from.
- **Temporal composability** — a component's effects are *revertible*:
  removing it unwinds everything it installed. Aoide's temporal seam:
  `rice draft save`/`drop`, the staging → declare → rebuild boundary, and
  NixOS generations underneath. A change that cannot be backed out is not
  finished.

This is Nix's own thesis — declarative, additive, atomically reversible —
applied above the nix layer, which is why the CLI, MCP, and A2A doors are one
implementation with three façades rather than three features
([[Agent-Interface]], [[A2A-Door]]).

**What it forbids, concretely:** a registry an author must edit to be seen; a
module reaching into another module; a capability that only exists inside one
consumer; an effect with no inverse. When a design choice is open, the rule
picks the option that can be deleted.

## The corollary for Quickshell: render surfaces only

**Quickshell paints; it never *is* the capability.** Every QML file in
`modules/facets/quickshell/` and `song/songbook/*/widgets/` is a render
surface that picks up an agnostic bridge or API by name. State, policy, IPC,
and system access live behind a bridge — a CLI verb, a stage file
(`CONTRACTS.md` §4), an IPC socket — reachable **with only a shell, no
desktop running**.

The test, applicable to a file never seen before:

> Delete every `.qml` in the repo. Is this capability still reachable from a
> terminal? **No → it is in the wrong place.**

So: a new API lands as a bridge FIRST, and the QML picks it up second — never
the reverse, never only in QML. A widget may read, arrange, animate, and
draw; it may not hold the only copy of a fact, shell out to do work a verb
should do, or decide policy — gates, permissions, and admission live in
[[aoided]]. [[Widget-Maker#The hard line: a widget is a render surface]] and
[[Widget-Bridge-Contract#The rules a widget is built by]] carry this rule at
the widget level; sonata's own `design/widget-structure.md` states it for the
ricing agents that read that file directly.

## Related

- [[Codebase]] — the walker and host-assembly mechanics this rule names
- [[Snowflake-Anatomy]] — the dendrite/facet discovery model
- [[Widget-Maker]] — the staging engine and the render-surface corollary in
  practice
- [[Widget-Bridge-Contract]] — the widget-side rules the corollary produces
- [[Self-Ricing]] — temporal composability's `rice draft`/staging boundary
- [[Agent-Interface]] · [[A2A-Door]] — the one-schema, many-façades doors
- [[Full-Architecture]]
