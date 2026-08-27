---
type: concept
created: 2026-07-26
updated: 2026-08-27
tags: [aoide, graph, session, terminal, agent, cli]
---

# Session Graph — the Project/Session DAG

[[Terminal-Commander]]'s roster of agent sessions is a **DAG of projects and
sessions**: who spawned whom, and which project each session belongs to. One
CLI group, `aoide graph`, is the DAG's terminal viewer and management layer —
every subcommand is implemented (`view`/`project`/`link`/`session`/
`spawn`/`send`/`pending`/`permit`/`prune`/`reap`, per `aoide
schema --json`). Like every command it registers into the single `commands/`
registry (`commands/graph.rs`, thin registrations over the `graph/` domain
functions — see [[aoide-cli]]), so the CLI door and the MCP door share the
group ([[Agent-Interface]]).

## The graph model

**Nodes** are projects (`project:<name>`) and sessions (`session:<sessionId>`).
Commands that take a node (`view --focus`) accept either the
full node id or the bare id.

**Edges** come in two kinds:

- **`anchors`** (project → session): a session is anchored to the registered
  project whose path is the **longest cwd prefix** — matched path-component-
  aware, so a session in a nested project anchors to the inner one, not the
  outer.
- **`spawned`** (session → session): recorded as an optional
  `parentSessionId` on the child's `sessions.json` record. A kitty-wrapped
  nested terminal gets it stamped directly at spawn (`--parent
  "$AOIDE_SESSION_ID"`); a hook-registered session with no explicit parent
  instead resolves one automatically: its self-first `/proc` ancestry
  (`hookAncestry`, stamped once at registration) is intersected against
  every live agent-kind session's own `hookAncestry`, and the closest
  matching ancestor wins as parent (CONTRACTS.md §4).

Precedence: a session with a resolved parent carries **only** its `spawned`
edge (no redundant anchors edge); a *dangling* `parentSessionId` (parent no
longer in the roster) falls back to project anchoring. Sessions matching no
project group under a synthetic `(unanchored)` root. Each session's live
state is the latest hook phase from `hooks.json` merged over its raw roster
state ([[shellbridge]] writes both files).

## The viewer — `graph view`

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

Every stage mutation restages the same resolved document atomically to
`song/stage/graph.json` — the identical write-temp-then-rename pattern as the
livery emitter — via `restage_graph`, so [[Quickshell]] hot-reloads it without
a separate emit step. `graph prune` is the manual resync when a hand-edit to
one of the stage files needs to be reconciled back into `graph.json`.

The desktop has no standalone DAG-diagram surface today: `graph view`/
`--json` in the terminal and the `aoide conductor` ratatui TUI (DAG/sessions/
projects/log/status panels) are the DAG's renderers. On the desktop, the
[[Gadget-Dock]]'s **Conductor gadget** gives the at-a-glance agent view
instead — a beamed tree of agent/sub-agent sessions, not a literal node/edge
diagram (field contract: [[Widget-Bridge-Contract]]).

The `aoide.surfaces.sessionGraph` owner-registry entry
(`modules/facets/quickshell/default.nix`) is still declared, but no QML file
backs it — the standalone overlay (`AoideSessionGraph.qml` + `GraphRow.qml`)
and the shared `GraphModel.qml` it and the dock's DAG gadget instantiated
are absent from the QML tree (open thread: whether the registry entry
should follow).

## The management layer

- **`graph project add|remove|list`** — keep the project registry
  `song/stage/projects.json`. Idempotent: `add` re-registers an existing name,
  `remove` of an absent project succeeds.
- **`graph link <child> <parent>`** — record a spawned-by edge by setting
  `parentSessionId` on the child's session record. Self-links and cycles are
  rejected (the handler walks the parent chain), exit 1.
- **`focus_session`** — jump to a session's window via `hyprctl dispatch
  focuswindow address:…`, the [[Terminal-Commander]] session-jump primitive.
  Not a CLI subcommand (`graph focus` is deleted): the [[Conductor-TUI]] and
  [[shellbridge]]'s `focussession` socket verb both call this library
  function directly (`aoide-conduct::graph::window`), so the shellbridge
  socket is the only shell-reachable path left to a session jump. It first
  **verifies the window is alive** through `hyprctl clients -j` before
  dispatching, because `focuswindow` exits 0 even for a vanished window: a
  gone terminal yields a structured `window-not-found` error with no
  dispatch. Address matching is case- and `0x`-prefix-tolerant (pure,
  unit-tested helpers `normalize_addr` / `window_present`). The full failure
  vocabulary: `session-not-found`, `no-window-address`,
  `hyprctl-unavailable`, `hyprctl-failed`, `window-not-found`.
- **`graph prune`** — drop sessions whose **raw** roster state is `done` (not
  the merged hook phase) along with their hook records; any `kind:"subagent"`
  descendant of a dropped session cascades away with it (see "Sub-agent
  cascade" below), and children that survive get `parentSessionId` cleared
  (un-orphaned rather than dangling). Everything removed or cleared is
  reported.
- **`graph spawn`** and **`graph send`** — the detached headless-launch command
  and the gated cross-session injection door. Full mechanism in
  [[Conductor-Channel]].
- **`graph pending list|approve|deny`** — the resolve surface over sends held
  without standing authorization (`song/stage/pending.json`); `approve`
  re-drives a held entry through `graph send` with `--yes`.
- **`graph permit`** — publish a permission SUMMONS card to
  `song/stage/herald.json` for a session blocked on a tool/permission prompt;
  the desktop herald surface renders it with approve/deny buttons whose click
  types the verdict back through the session's socket. The hook door raises
  it automatically the moment a session goes `awaiting`.
- **`graph session carry on|off`** — mark or unmark a session DURABLE in
  `state/carry.json`, so a project's whole carried set can later be
  resurrected together. A separate, freely-mutated set beside the ledger:
  writes only `carry.json`, atomic, no stage lock, outside the `song/stage/`
  dual-writer surface entirely. `--id <id>` targets any session id directly,
  including one already gone from the roster — no roster lookup gates the
  write, which is what lets a mark be flipped post-mortem, off a bare ledger
  id. Bare and `--self` both resolve the target from `$AOIDE_SESSION_ID`.
  `graph spawn --carry` marks a session at birth; a bare `graph resurrect
  --project <x>` (no `--all`/`--id`) resumes a project's whole carried set
  and transfers the mark from an old id onto the fresh one that replaces
  it — see [[Graph-and-Conduct]] for the resurrect-selection mechanism and
  the daemon's boot-time auto-resume sweep.

A durability rule spans the layer: the stage rewriters **round-trip unknown
fields** (serde flatten), so graph management never clobbers fields other
writers add to the same records.

## Liveness reaping — the SIGKILL problem

`graph prune` only drops sessions whose raw state is already `done` — an
*orderly* exit. Conduct-by-default makes a *disorderly* exit common: a
terminal closed with `SUPER+Q` or killed outright tears down the `conduct`
process uncatchably, so it can never run its own `graph session end`.
Left alone, the record strands `running` in the roster forever (observed:
22 dead `conduct-*` sessions piled up in ~8 minutes of normal use).

**`aoide graph reap`** is the out-of-band sweep that resolves these. It marks
a session **dead** when *any* of three independent signals fires:

- **window gone** — the record has a non-empty `windowAddress` that is no
  longer among the live addresses `hyprctl clients -j` reports.
- **pid gone** — the record has a `pid` whose `/proc/<pid>` no longer exists.
- **stale abandonment** — every evidence stream (`last_seen`: the max of
  transcript mtime, hook `updatedAt`, and `startedAt`) has gone silent past a
  state-dependent band: 72h for an at-rest (`idle`/`stopped`) record, 7 days
  for a mid-turn (`working`/`awaiting`) record, or 2h for a mid-turn
  `subagent`-kind record (it cannot outlive its parent's own Task-tool call).
  This is the only signal that reaches an agent killed inside a still-open
  terminal or parent process, whose recorded pid is the terminal's own and
  outlives it — window and pid stay live forever, so only across-the-board
  silence catches it. A live pid that is NOT its window's owning pid (per a
  window-owner map built alongside the live-address set) is read as the
  agent's own process and vetoes the staleness signal outright.

The never-false-reap guard: if Hyprland can't be queried at all (no
`HYPRLAND_INSTANCE_SIGNATURE`, `hyprctl` missing/failed), the window signal is
**unknown**, not false — it contributes nothing, and the sweep degrades to
pid-only (or pid+staleness) liveness rather than reaping every windowed
session it merely failed to see. A session with *neither* window nor pid
evidence (e.g. a hook-only session that hasn't discovered either yet) is
still eligible for the staleness signal, never for the other two.

Dead sessions are marked `done` (session + hook record) and then run through
the same `prune_done` path — dropped, `graph.json` re-staged atomically.
`graph reap` never errors on "nothing to reap" and never errors on an
unreachable compositor.

## Sub-agent cascade

A `Task`/`Agent`-tool spawn (see [[Agent-Hooking]]'s `subagent_tools`)
registers its child as a `kind:"subagent"` node. That node carries no `pid`
and no `windowAddress` — it lives and dies inside its parent process, so the
window/pid signals structurally can never fire for one. Its **only** cleanup
path is cascading out when its owning top-level session does: a clean `graph
session end` walks `parentSessionId` transitively and drops the whole
sub-agent subtree, and `prune_done` does the same for anything it is about to
remove — so every `prune_done` caller inherits the cascade: `graph prune`'s
done-sweep, `graph reap`'s liveness sweep (above), and the same-window
duplicate-eviction retirement (below). One shared walk
(`doomed_subagent_descendants`, fixed-point over `parentSessionId`, handles
subagent-of-subagent nesting) backs both paths, so a session that dies
**abnormally** — killed terminal, crashed process, caught by the reap sweep
rather than exiting through `graph session end` — loses its sub-agent
children exactly as cleanly as an orderly exit does.

The same pass also retires **same-window agent duplicates**: a compact/resume
mints a new `session_id` for a window that already holds a live agent record,
and the stale one's `pid` resolves to the terminal rather than the agent, so
liveness alone can't catch it. Among not-`done` agent-kind records sharing one
window past a short grace, the keeper ranks by newest `startedAt` first —
compact/resume always mints its new record strictly after the old one, so
newest-first keeps the live session even before it has written anything —
then a real on-disk transcript, then whether it carries `say`, then
`kind=="agent"`, then lexically-greatest `sessionId` as a final tiebreak. A
group of one, or a group with any member still inside the grace, is left
alone. Shells are exempt, and so is a **conducted PTY host**: an `aoide
conduct -- <agent>` wrapper classifies as `agent` from its child's basename
but is the HOST of the agent inside it, so `is_agent_kind` excludes any
`conductable` record from the dedup entirely — a wrapper-of-agent (`conduct
-- kimi`) is never retired as a duplicate of the session it hosts.

The registration-time half of the same rule spares the new record's **whole
lineage** — every ancestor and descendant reachable by walking
`parentSessionId`, not only its direct parent — since a nested `conduct`/
`spawn` chain can legitimately land a same-window pair that are
grandparent/grandchild, or cousins through a shared ancestor, not just
parent/child. A same-window pair with no lineage relation at all still
collapses instantly: the legitimate compact/resume-twin case. A nested
headless session is windowless by construction: the hook-time
`windowAddress` backfill skips it outright whenever its `parentSessionId`
chain passes through a conducted session with its own `windowAddress` still
empty, so it can never wrongly inherit the ENCLOSING terminal's window and
get treated as that terminal's duplicate.

## Interplay with the window-event listener

[[shellbridge]]'s Hyprland event listener (the authoritative `windowAddress`
source — see [[Terminal-Commander]]) is the *counterpart* to the reaper: it
fills the address at window-creation time and **clears it on `closewindow`**.
So for a terminal closed with `SUPER+Q`, the record's address is usually
already blank by the time the reaper runs, and it is the **pid-gone** signal
(the SIGKILLed `conduct` process's `/proc/<pid>` vanishing) that reaps it —
the two paths agree either way. The window-gone signal remains the reaper's
safety net for the case the listener never saw (service down, off-Hyprland,
or a stamped address whose `closewindow` was missed).

It runs on a systemd user timer (`aoide-graph-reap`, next to shellbridge in
[[shellbridge]]'s unit): first sweep 15s after the graphical session comes up,
then every ~12s thereafter — cheap enough (one `hyprctl` call + a stage read,
a write only when something changed) to run continuously without agents ever
seeing a stale "haunting" session.

## Contracts (CONTRACTS.md §4, still v0)

- `projects.json` v0: `{schemaVersion, projects: [{name, path}]}`.
- `graph.json` v0: `{schemaVersion, nodes: […], edges: [{from, to, kind}]}` —
  fully resolved, so Quickshell never recomputes anchoring.
- `sessions.json` records carry several **additive** optional fields (no
  version bump): `parentSessionId` (spawned edge), `logPath` (a
  headless-conducted session's pty-master log — [[Conductor-Channel]]), and
  `hookAncestry` (up to 8 self-first `/proc` pids, stamped once, the
  auto-parenting seam above). The full field list — `kind`/`conductable`,
  `contextTokens`/`contextCeiling`/`tool`/`petname` and more — is CONTRACTS.md
  §4; each is round-tripped by every rewriter regardless of whether it
  understands the field.

## Open seams

The `stage_dir()` / `AOIDE_STAGE_DIR` mismatch is fixed and documented as a
contract seam ("Stage-dir resolution", `CONTRACTS.md §4` — see
[[shellbridge]]). The `focuswindow` exit-0 ambiguity is resolved by the
liveness check above. Spawn-time parenting is now two-layered: the kitty
wrapper stamps `parentSessionId` directly for the common nested-terminal case
([[Conductor-Channel]]), and a hook-registered session with no explicit
`--parent` resolves one automatically via the `hookAncestry` walk — so `graph
link` is the manual override path for edges outside both, not the only
source. The dock's `SUPER+G` toggle and the launcher's `SUPER+SPACE` both
resolve in-process (Hyprland global shortcuts the QML registers itself)
rather than through an `aoide shell` CLI command; only `SUPER+ESCAPE` (lock)
still execs an `aoide shell lock` command absent from the schema (open
thread, see [[aoide-cli]]).

## Related

- [[Terminal-Commander]]
- [[shellbridge]]
- [[aoide-cli]]
- [[Quickshell]]
- [[Gadget-Dock]]
- [[Agent-Interface]]
- [[Codebase]]
- [[Widget-Bridge-Contract]]
- [[Conductor-Channel]]
- [[Conductor-TUI]]
