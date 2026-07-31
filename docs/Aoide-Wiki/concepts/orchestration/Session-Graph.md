---
type: concept
created: 2026-07-26
updated: 2026-07-30
tags: [aoide, graph, session, terminal, agent, cli]
---

# Session Graph — the Project/Session DAG

[[Terminal-Commander]]'s roster of agent sessions is a **DAG of projects and
sessions** — who spawned whom, and which project each session belongs to —
with a terminal viewer and a management layer behind one CLI group, `aoide
graph` (real, every subcommand implemented — `view`/`project`/`link`/
`session`/`wrap`/`send`/`focus`/`prune`/`reap`/`emit`, per `aoide schema
--json`). Like every command it lives in the single `schema.rs` table, so the
CLI door and the MCP door share the group ([[Agent-Interface]]).

*Verified green (`nix flake check` + the vm-boot check). Implementation:
`pkgs/aoide/src/graph.rs`, with the liveness/dedup reaper in its own
`pkgs/aoide/src/reap.rs` module — see [[Codebase]].*

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
drachma emitter — so [[Quickshell]] can hot-reload it.

The desktop has no standalone DAG-diagram surface today: `graph view`/
`--json` in the terminal and the `aoide conductor` ratatui TUI (DAG/sessions/
projects/log/status panels) are the DAG's renderers. On the desktop, the
[[Gadget-Dock]]'s **Conductor gadget** gives the at-a-glance agent view
instead — a beamed tree of agent/sub-agent sessions
([[Widget-Bridge-Contract]]), not a literal node/edge diagram. `SUPER+G`
summons the dock: an in-process Hyprland global shortcut (`aoide:dock`) the
panel registers itself, not a CLI verb.

The `aoide.surfaces.sessionGraph` owner-registry entry
(`modules/facets/quickshell/default.nix`) is still declared, but no QML file
backs it — the prior standalone overlay (`AoideSessionGraph.qml` +
`GraphRow.qml`) and the shared `GraphModel.qml` it and the dock's former DAG
gadget instantiated are no longer part of the QML tree (open thread: whether
the registry entry should follow).

## The management layer

- **`graph project add|remove|list`** — keep the project registry
  `song/stage/projects.json`. Idempotent: `add` re-registers an existing name,
  `remove` of an absent project succeeds.
- **`graph link <child> <parent>`** — record a spawned-by edge by setting
  `parentSessionId` on the child's session record. Self-links and cycles are
  rejected (the handler walks the parent chain), exit 1.
- **`graph focus <session>`** — jump to the session's window via `hyprctl
  dispatch focuswindow address:…` — the existing [[Terminal-Commander]]
  session-jump flow, reachable from the CLI. It first **verifies the window is
  alive** through `hyprctl clients -j` before
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
*orderly* exit. Conduct-by-default makes a *disorderly* exit common: a
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

The same pass also retires **same-window agent duplicates**: a compact/resume
mints a new `session_id` for a window that already holds a live agent record,
and the stale one's `pid` resolves to the terminal rather than the agent, so
liveness alone can't catch it. Among same-window `agent`-kind records past a
short grace, the keeper is the one with a real on-disk transcript; the rest
are retired. Shells are exempt (a conducted shell legitimately shares its
window with the agent inside it). See [[Widget-Bridge-Contract]] for the full
rule (this same dedup also runs at registration time, in `graph.rs`).

**Interplay with the window-event listener.** [[shellbridge]]'s Hyprland event
listener (the authoritative `windowAddress` source — see
[[Terminal-Commander]]) is the *counterpart* to the reaper: it fills the address
at window-creation time and **clears it on `closewindow`**. So for a terminal
closed with `SUPER+Q`, the record's address is usually already blank by the time
the reaper runs, and it is the **pid-gone** signal (the SIGKILLed `conduct`
process's `/proc/<pid>` vanishing) that reaps it — the two paths agree either
way. The window-gone signal remains the reaper's safety net for the case the
listener never saw (service down, off-Hyprland, or a stamped address whose
`closewindow` was missed).

It runs on a systemd user timer (`aoide-graph-reap`, next to shellbridge in
[[shellbridge]]'s unit): first sweep 15s after the graphical session comes up,
then every ~12s thereafter — cheap enough (one `hyprctl` call + a stage read,
a write only when something changed) to run continuously without agents ever
seeing a stale "haunting" session.

## Contracts (CONTRACTS.md §4, still v0)

- `projects.json` v0: `{schemaVersion, projects: [{name, path}]}`.
- `graph.json` v0: `{schemaVersion, nodes: […], edges: [{from, to, kind}]}` —
  fully resolved, so Quickshell never recomputes anchoring.
- `sessions.json` records carry the **additive** optional `parentSessionId`
  (no version bump).

## Open seams

The `stage_dir()` / `AOIDE_STAGE_DIR` mismatch is fixed and documented as a
contract seam ("Stage-dir resolution", `CONTRACTS.md §4` — see
[[shellbridge]]). The `focuswindow` exit-0 ambiguity is resolved by the
liveness check above. `conduct-by-default` stamps `parentSessionId` at spawn
time for the common case — the kitty wrapper passes `--parent
"$AOIDE_SESSION_ID"` into every nested `aoide conduct` ([[Conductor-Channel]])
— so `graph link` is the manual/override path for edges outside that
nesting, not the only source. The dock's `SUPER+G` toggle and the launcher's
`SUPER+SPACE` both resolve in-process (Hyprland global shortcuts the QML
registers itself) rather than through an `aoide shell` CLI verb; only
`SUPER+ESCAPE` (lock) still execs an `aoide shell lock` command absent from
the schema (open thread, see [[aoide-cli]]).

## Related

- [[Terminal-Commander]]
- [[shellbridge]]
- [[aoide-cli]]
- [[Quickshell]]
- [[Gadget-Dock]]
- [[Agent-Interface]]
- [[Codebase]]
- [[Widget-Bridge-Contract]]
