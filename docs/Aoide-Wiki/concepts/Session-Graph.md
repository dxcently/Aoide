---
type: concept
created: 2026-07-26
updated: 2026-07-26
tags: [aoide, graph, session, terminal, agent, cli]
---

# Session Graph — the Project/Session DAG

[[Terminal-Commander]] began as a flat roster: one row per agent session. The
session graph grows that roster into a **DAG of projects and sessions** — who
spawned whom, and which project each session belongs to — with a terminal
viewer and a management layer behind one CLI group, `aoide graph` (8 commands,
all implemented). Like every command it lives in the single `schema.rs` table,
so the CLI door and the MCP door gained the group together
([[Agent-Interface]]).

*Grounded in the repo at commit 0b3a3fd; extended at 41be90f (focus liveness,
the desktop surfaces) and 8f4034e (the dock popup as primary DAG affordance).
Verified green (`nix flake check` + the vm-boot check). Implementation:
`pkgs/aoide/src/graph.rs` — see [[Codebase]].*

## The graph model

**Nodes** are projects (`project:<name>`) and sessions (`session:<sessionId>`).
Commands that take a node (`graph focus`, `view --focus`) accept either the
full node id or the bare id.

**Edges** come in two kinds:

- **`anchors`** (project → session): a session is anchored to the registered
  project whose path is the **longest cwd prefix** — matched path-component-
  aware, so a session in a nested project anchors to the inner one, not the
  outer.
- **`spawned`** (session → session): recorded as an optional
  `parentSessionId` on the child's `sessions.json` record.

Precedence: a session with a resolved parent carries **only** its `spawned`
edge (no redundant anchors edge); a *dangling* `parentSessionId` (parent no
longer in the roster) falls back to project anchoring. Sessions matching no
project group under a synthetic `(unanchored)` root. Each session's live
state is the latest hook phase from `hooks.json` merged over its raw roster
state ([[shellbridge]] writes both files).

## The viewer — `graph view` and `graph emit`

`graph view` renders the DAG as a Unicode box-drawing tree in the terminal;
`--focus <id>` marks a node with `▶`, and `--json` emits the structured graph
document instead. A sample render:

```
◆ demo  /tmp/demo
└─ ● abc123  claude  Notification  /tmp/demo
   └─ ▶ ● def456  claude  idle  /tmp/demo/sub
◆ (unanchored)
└─ ● zzz789  melete-run  done  /opt/elsewhere
```

`graph emit` writes the same resolved document atomically to
`song/stage/graph.json` — the identical write-temp-then-rename pattern as the
notes emitter — so [[Quickshell]] can hot-reload it.

**The desktop surfaces are built** (commits 1fedd58 and 41be90f). The graph
now renders in two places besides the terminal, both hot-reloading
`graph.json`:

- **`DagGraphGadget`** — the compact view inside the [[Gadget-Dock]], now the
  **primary DAG affordance** on the desktop: since 8f4034e, `SUPER+G` summons
  the dock popup (bridge path `aoide shell dock toggle`, open-and-pin — a
  stub verb like the launcher bind; the `aoide shell` group is still future
  CLI work, open thread), and the dock contains the DAG gadget.
- **`AoideSessionGraph`** — the standalone overlay (surface #9,
  `aoide.surfaces.sessionGraph`), now **dormant**: it has no keybind and is
  reachable only via the bridge, kept intact as the seam for a future
  dedicated full-screen DAG view. It draws the DAG as an indented tree:
  projects `◆`, sessions `●` with agent/state/short-cwd, spawned children
  nested, incoming-edge-free sessions under the synthetic `(unanchored)`
  root. The traversal is cycle-guarded (a visited set), dangling edges are
  filtered, and a safety net surfaces any unvisited session at depth 0.
  Colours come entirely from notes (accent = running, urgent =
  awaiting/Notification, dimmed fg = done). A row click calls
  `bridge.focusSession` — the [[shellbridge]] session-jump gate; QML never
  shells out.

Both instantiate the shared **`GraphModel.qml`** (`buildRows()` extracted from
the overlay) — the canonical QML graph model; any future graph consumer must
use it too.

## The management layer

- **`graph project add|remove|list`** — keep the project registry
  `song/stage/projects.json`. Idempotent: `add` re-registers an existing name,
  `remove` of an absent project succeeds.
- **`graph link <child> <parent>`** — record a spawned-by edge by setting
  `parentSessionId` on the child's session record. Self-links and cycles are
  rejected (the handler walks the parent chain), exit 1.
- **`graph focus <session>`** — jump to the session's window via `hyprctl
  dispatch focuswindow address:…` — the existing [[Terminal-Commander]]
  session-jump flow, now reachable from the CLI. Since d03dcf2 it first
  **verifies the window is alive** through `hyprctl clients -j` before
  dispatching, because `focuswindow` exits 0 even for a vanished window: a
  gone terminal yields a structured `window-not-found` exit 1 with no
  dispatch. Address matching is case- and `0x`-prefix-tolerant (pure,
  unit-tested helpers `normalize_addr` / `window_present`). The full failure
  vocabulary: `session-not-found`, `no-window-address`,
  `hyprctl-unavailable`, `hyprctl-failed`, `window-not-found`.
- **`graph prune`** — drop sessions whose **raw** roster state is `done` (not
  the merged hook phase) along with their hook records; children of a pruned
  session get `parentSessionId` cleared (un-orphaned rather than dangling).
  Everything removed or cleared is reported.

A durability rule spans the layer: the stage rewriters **round-trip unknown
fields** (serde flatten), so graph management never clobbers fields other
writers add to the same records.

## Liveness reaping — the SIGKILL problem

`graph prune` only drops sessions whose raw state is already `done` — an
*orderly* exit. Conduct-by-default made an *disorderly* exit common: a
terminal closed with `SUPER+Q` or killed outright tears down the `conduct`/
`wrap` process uncatchably, so it can never run its own `graph session end`.
Left alone, the record strands `running` in the roster forever (observed:
22 dead `conduct-*` sessions piled up in ~8 minutes of normal use).

**`aoide graph reap`** is the out-of-band sweep that resolves these. It marks
a session **dead** when *either* independent signal fires — never both
required:

- **window gone** — the record has a non-empty `windowAddress` that is no
  longer among the live addresses `hyprctl clients -j` reports, or
- **pid gone** — the record has a `pid` whose `/proc/<pid>` no longer exists.

The never-false-reap guard: if Hyprland can't be queried at all (no
`HYPRLAND_INSTANCE_SIGNATURE`, `hyprctl` missing/failed), the window signal is
**unknown**, not false — it contributes nothing, and the sweep degrades to
pid-only liveness rather than reaping every windowed session it merely failed
to see. A session with *neither* signal recorded (empty `windowAddress` and
no `pid` — e.g. a hook-only session that hasn't discovered either yet) is
left alone: absence of evidence is never evidence of death. The predicate
(`is_session_dead`) is pure and unit-tested against fake live-address sets and
a fake `proc_exists`.

Dead sessions are marked `done` (session + hook record) and then run through
the same `prune_done` path — dropped, orphaned `parentSessionId` links
cleared, `graph.json` re-staged atomically. `graph reap` never errors on
"nothing to reap" and never errors on an unreachable compositor.

It runs on a systemd user timer (`aoide-graph-reap`, next to shellbridge in
[[shellbridge]]'s unit): first sweep 15s after the graphical session comes up,
then every ~12s thereafter — cheap enough (one `hyprctl` call + a stage read,
a write only when something changed) to run continuously without agents ever
seeing a stale "haunting" session.

## Contracts (CONTRACTS.md §4, still v0)

- `projects.json` v0: `{schemaVersion, projects: [{name, path}]}`.
- `graph.json` v0: `{schemaVersion, nodes: […], edges: [{from, to, kind}]}` —
  fully resolved, so Quickshell never recomputes anchoring.
- `sessions.json` records gained the **additive** optional `parentSessionId`
  (no version bump).

## Open seams

Three of the original seams closed at d03dcf2/1fedd58: the `stage_dir()` /
`AOIDE_STAGE_DIR` mismatch is fixed and documented as a contract seam
("Stage-dir resolution", `CONTRACTS.md §4` — see [[shellbridge]]); the
`focuswindow` exit-0 ambiguity is resolved by the liveness check above; and
the Quickshell DAG surface exists (overlay + dock gadget). Still open (log
threads): shellbridge does not yet stamp `parentSessionId` at spawn time (so
`graph link` remains the only source, not the manual override), and the
`aoide shell` verbs the keybinds reference are not yet in the command schema.

## Related

- [[Terminal-Commander]]
- [[shellbridge]]
- [[aoide-cli]]
- [[Quickshell]]
- [[Gadget-Dock]]
- [[Agent-Interface]]
- [[Codebase]]
