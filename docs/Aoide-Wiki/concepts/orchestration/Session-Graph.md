---
type: concept
created: 2026-07-26
updated: 2026-08-28
tags: [aoide, graph, session, terminal, agent, cli]
---

# Session Graph — the Project/Session DAG

[[Terminal-Commander]]'s roster of agent sessions is a **DAG of projects and
sessions**: who spawned whom, and which project each session belongs to. The
DAG's terminal viewer and management layer spans several top-level command
families — bare `graph` (the render) and `graph link` are the read/analysis
lens; `project add|remove|list`, `session start|phase|end|hook|grant|trace|
permit|pending list|approve|deny|reap|prune`, bare `session` (the roster,
grouped by project or by host), and bare `send`/`spawn`/
`resurrect` are the acting/lifecycle surface (command-defrag task #101, Lane
R). Every subcommand is
implemented, per `aoide schema --json`. Like every command it registers into
the single `commands/` registry (`commands/graph.rs`, thin registrations
over the `graph/` domain functions — see [[aoide-cli]]), so the CLI door and
the MCP door share the group ([[Agent-Interface]]).

## The graph model

**Nodes** are projects (`project:<name>`) and sessions (`session:<sessionId>`).
Commands that take a node (`graph --focus`) accept either the
full node id or the bare id.

**Edges** come in three kinds:

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
- **`leads`** (session → session): a project that names a **lead**
  (`aoide project lead <name> <session>`, CONTRACTS.md §4's
  `projects.json`) hangs its other parentless sessions off that one
  session, one edge per follower. The lead is the node the project
  anchors, whatever the lead's own cwd says; the followers carry this
  edge instead of an `anchors` edge. `--none` clears it, and a lead that
  has left the roster is ignored — those sessions anchor as before.

Precedence: a session with a resolved parent carries **only** its `spawned`
edge (no redundant anchors edge); a *dangling* `parentSessionId` (parent no
longer in the roster) falls back to project anchoring. A parentless session
resolves its project by the lead it is (`Project.lead` points at it) and
then by cwd, and hangs under the project's live lead when that lead is not
itself; the lead is anchored to the project it leads. Sessions matching no
project group under a synthetic `(unanchored)` root. Each session's live
state is the latest hook phase from `hooks.json` merged over its raw roster
state ([[shellbridge]] writes both files).

## The viewer — bare `graph`

Bare `graph` renders the DAG as a Unicode box-drawing tree in the terminal;
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
`state/stage/graph.json` — the identical write-temp-then-rename pattern as the
livery emitter (which stages to `song/stage/` instead — the two trees are
split by owner, core vs. lyra) — via `restage_graph`, so [[Quickshell]]
hot-reloads it without a separate emit step. `session prune` is the manual
resync when a hand-edit to one of the stage files needs to be reconciled
back into `graph.json`.

The desktop has no standalone DAG-diagram surface today: bare `graph`/
`--json` in the terminal and the `aoide conductor` ratatui TUI (DAG/session/
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

- **`project add|edit|remove|list`** — keep the project registry
  `state/stage/projects.json`, where a project is a set of anchor roots, not
  one directory. `add` appends one or more roots to a name (registering it
  first if new); `--new` refuses a name that already exists instead of
  adding to it. `edit` is the exact-replacement editor — it swaps a
  project's whole root list for the one given, atomically, and never
  touches the name or `autoResume`. `remove` drops one root (or, given no
  root, the whole project); idempotent throughout — `add` of an
  already-present root and `remove` of an absent project or root both
  succeed as no-ops.
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
- **`session prune`** — drop sessions whose **raw** roster state is `done` (not
  the merged hook phase) along with their hook records; any `kind:"subagent"`
  descendant of a dropped session cascades away with it (see "Sub-agent
  cascade" below), and children that survive get `parentSessionId` cleared
  (un-orphaned rather than dangling). Everything removed or cleared is
  reported.
- **`spawn`** and **`send`** — the detached headless-launch command
  and the gated cross-session injection door. Full mechanism in
  [[Conductor-Channel]].
- **`session pending list|approve|deny`** — the resolve surface over sends held
  without standing authorization (`state/stage/pending.json`); `approve`
  re-drives a held entry through `send` with `--yes`.
- **`session trace <id>`** — render a session's own trace file, one line per
  record: the run, step by step (see "Agents enrolled from outside" above).
  `--tail N` shows the last N records (default 50), `--follow` re-reads for
  new ones until Ctrl-C (CLI-only), `--json` passes the raw lines through
  unchanged. `<id>` resolves like `send --to` — id, tail4, or petname.
- **`session permit`** — publish a permission SUMMONS card to
  `state/stage/herald.json` for a session blocked on a tool/permission prompt;
  the desktop herald surface renders it with approve/deny buttons whose click
  types the verdict back through the session's socket. The hook door raises
  it automatically the moment a session goes `awaiting`.
- **`session undying on|off`** — mark or unmark a session DURABLE in
  `state/undying.json`, so a project's whole undying set can later be
  resurrected together. A separate, freely-mutated set beside the ledger:
  writes only `undying.json`, atomic, no stage lock, outside the `state/stage/`
  dual-writer surface entirely. `--id <id>` targets any session id directly,
  including one already gone from the roster — no roster lookup gates the
  write, which is what lets a mark be flipped post-mortem, off a bare ledger
  id. Bare and `--self` both resolve the target from `$AOIDE_SESSION_ID`.
  `spawn --undying` marks a session at birth; a bare `resurrect
  --project <x>` (no `--all`/`--id`) resumes a project's whole undying set
  and transfers the mark from an old id onto the fresh one that replaces
  it — see [[Graph-and-Conduct]] for the resurrect-selection mechanism and
  the daemon's boot-time auto-resume sweep. The mark was prototyped under
  the name "carry"; the shipped name is undying, and `state/carry.json`
  migrates to `state/undying.json` on first load.
- **`session` (bare)** — the undying picker: a `tty`+`inquire` multi-select
  over this host's own sessions plus every registered node's CACHED
  sessions, each row pre-checked by its current undying state, confirmed in
  one Enter. A local row's toggle writes `state/undying.json` directly; a
  node row's toggle can't touch that store (the id lives on the node), so it
  writes a `{host, dir, agent}` spec into the CURRENT project's
  `.aoide/project.json` manifest instead (walked up from cwd; never
  auto-created, and a node cwd that doesn't relativize under the local
  project root is rejected with a taught reason). Cli+tty only — a non-CLI
  door, no tty, or `--json` always steers to the scripted spelling, `session
  undying on|off --id <id>`. See [[Graph-and-Conduct]] for the manifest
  shape and the bare-`resurrect` walk-up it feeds.

A durability rule spans the layer: the stage rewriters **round-trip unknown
fields** (serde flatten), so graph management never clobbers fields other
writers add to the same records.

## Agents enrolled from outside — eidolon, and the trace

Two producers write records into this DAG from OUTSIDE, without ever running
`aoide conduct` — one record per native session, keyed by the producer's own
id verbatim (no synthetic prefix; the id is already stable), each gathered on
the reaper's tick and each holding the same "a failed observation changes
nothing" rule.

**eidolon** (`conduct/src/graph/eidolon.rs`) is the on-box agent. Its swarm
presence at `$XDG_RUNTIME_DIR/eidolon/<id>/{meta.json,sock}` is read, never
swept — a stale directory is the producer's own to clean up — and liveness is
the presence socket answering `{"op":"ping"}` with `{"ok":true}` inside a
250ms budget: never a stat, never `/proc`. Unlike a desktop Codex thread this
IS an agent session (`kind:"agent"`), so it is a normal dedup/staleness
candidate and its `parentSessionId` is resolved here (the first conducted
ancestor up the presence pid's own process chain). The sweep owns only the
records it enrolled: `agent:"eidolon"` is a label a caller may choose for a
conduct-owned wrapper of its own (`spawn --task … --agent eidolon -- eidolon
run …`), so ownership is read off the record's own conduct facts (`conductable`,
the control socket, the run's task keys) and only a real presence enrolment is
ever dropped, upserted or enrolled over.

**Its state comes from the trace, when there is one.** eidolon publishes its
journal as one JSON record per line — as a `<log>.jsonl` mirror an installed
generation writes, or through its own read-only export door — and the session's
presence names the journal either way. Aoide resolves it through the same
locator every other transcript reader uses (`TranscriptSpec::locate`, then
`TranscriptSpec::trace`), so the state fold follows whichever generation is
installed. The LAST record decides, and only the five
canonical states are ever written: `TurnSettled` → `idle`, `Cancelled` →
`stopped`, `AskUser` with `answer: null` → `awaiting`, anything else →
`working` (a turn is open). Nothing falls back to the presence's `busy`
while a trace exists — a headless run's `busy` is permanently false, which is
the non-fact the trace replaces. With no trace at all (an older eidolon with
neither a mirror nor a door) the
presence rule applies instead: a TUI owner's `busy` maps to `working`/`idle`,
anything else carries the literal `"unknown"` rather than a guessed verdict,
folded to `idle`.

A mirror that the producer has stopped appending to is not a trace Aoide will
read as live state: a journal strictly newer than its mirror is positive
evidence, and the answer becomes the journal itself (when the door is there) or
the presence rule — never the frozen record.

The file, its line shape, the record table and the state rule are one
contract — `docs/architecture/EIDOLON-TRACE.md`, restated at CONTRACTS.md §4.
The producer owns the file; aoide reads it. `aoide session trace <id>
[--tail N] [--follow] [--json]` renders it step by step, one line per record
(`#<id>  <hh:mm:ss local>  <kind>  <summary>`) — an assistant message shows
its thinking (dimmed, cut) then its text then each `→ tool(name)`, a tool
result shows its first line prefixed `!` when it errored, a settled turn
shows its stop reason and tokens — and `--follow` re-reads for new records
until Ctrl-C. The command is read-only: it writes nothing, takes no stage
lock, and never reaches the daemon. A session whose harness keeps no trace,
or whose presence names none, is a taught error naming which of the two it
is, never an empty listing.

**And the parent hears it.** The same tick that folds the state also reads
the child's new records and, under the resident daemon alone, delivers ONE
line about the child to the parent that spawned it — `[eidolon <petname>]
settled end_turn · 90 calls · 31 min · last: "…"`, `… cancelled …`, `… died
mid-turn …` (its eidolon record was just dropped with the turn still open),
`… asking: "…"`, `… wrapping up · 8 calls left`, `… failing · 3 tool errors
in a row · last: <tool label>`, `… silent 12 min · last: …`. It is not a
send and never becomes one: the send door attests the sender from the running
process's `/proc` ancestry, so inside the daemon the sender is the daemon and
never the child — the line is raw-injected the doorbell's way (a live channel
socket, else the control socket with the wrap's own submit key), with no
gate, no pending entry, no provenance prefix and no rename of the parent.
`state/stage/pingback.json` holds the per-child cursor (`seen`, `silentAt`),
claimed inside one short stage-lock section before the write, so each event
is delivered at most once. A parent that is a bare shell is skipped — a line
typed into a shell would run — as is one whose record is gone, not
conductable, or already `done`. The ruling is the User's (2026-09-17): a
parent hears the children it spawned, and nothing wider.

The contrasting shape is **a desktop Codex/ChatGPT thread**
(`graph/codex_app.rs`): `kind:"app"`, because it is a task inside an app
aoide does not conduct — no agent profile, no hook, no control socket, and it
keeps out of the reaper's agent-arm entirely, since N threads of one app
legitimately share one window address and one app-server pid. Its discovery
is a try-flock on the thread's own writer lock plus one parsed `ps` table.

## Liveness reaping — the SIGKILL problem

`session prune` only drops sessions whose raw state is already `done` — an
*orderly* exit. Conduct-by-default makes a *disorderly* exit common: a
terminal closed with `SUPER+Q` or killed outright tears down the `conduct`
process uncatchably, so it can never run its own `session end`.
Left alone, the record strands `running` in the roster forever (observed:
22 dead `conduct-*` sessions piled up in ~8 minutes of normal use).

**`aoide session reap`** is the out-of-band sweep that resolves these. It marks
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
`session reap` never errors on "nothing to reap" and never errors on an
unreachable compositor.

## Sub-agent cascade

A `Task`/`Agent`-tool spawn (see [[Agent-Hooking]]'s `subagent_tools`)
registers its child as a `kind:"subagent"` node. That node carries no `pid`
and no `windowAddress` — it lives and dies inside its parent process, so the
window/pid signals structurally can never fire for one. Its **only** cleanup
path is cascading out when its owning top-level session does: a clean
`session end` walks `parentSessionId` transitively and drops the whole
sub-agent subtree, and `prune_done` does the same for anything it is about to
remove — so every `prune_done` caller inherits the cascade: `session prune`'s
done-sweep, `session reap`'s liveness sweep (above), and the same-window
duplicate-eviction retirement (below). One shared walk
(`doomed_subagent_descendants`, fixed-point over `parentSessionId`, handles
subagent-of-subagent nesting) backs both paths, so a session that dies
**abnormally** — killed terminal, crashed process, caught by the reap sweep
rather than exiting through `session end` — loses its sub-agent
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

## Session identity — origin and the sealed credential

Two additive record fields carry a session's provenance (LANE IDENTITY,
task #63), plus a third for the SAME question across machines (the remote
sub-agents lane's `remoteParent`, below); `origin` and the sealed credential
are consumed internally and get no `graph.json` projection:

- **`origin`** (string, write-once) — `"node:<name>"` for a session the A2A
  door spawned on behalf of an identified, paired node, a local-class value
  otherwise. `stamp_origin` is the sole stamp function, with two legitimate
  callers: the door's spawn handler (the only place a `node:*` value may
  originate) and `session_conduct`, stamping a local-class value off its
  own inherited `AOIDE_SESSION_ORIGIN` env and refusing a `node:*` shape
  read there — inherited env is exactly what a same-uid process can set on
  itself. `aoide resurrect` carries a local-class `origin` forward off the
  session's own ledger entry and refuses a `node:*` shape found there the
  same way — the ledger is an unsealed append-only file, so a forged
  `node:*` line earns nothing. The raw field is **attribution, never a
  gate**: `sessions.json` and `state/session-ledger.jsonl` stay plain
  same-uid-writable files, so no security decision keys on `origin` as read
  off disk. The authenticated form is the seal.
- **`remoteParent`** (object, `{node, key, sessionId}`) — the SAME spawned-by
  edge across machines, deliberately a SECOND field. `parentSessionId` stays
  LOCAL-only: every reader of it (the autogate grant and sibling rule in
  `graph/send.rs`, `graph/model.rs`'s grouping, `graph link`'s cycle check,
  `taskreport`'s mailbox) treats it as a local id, so a foreign value there
  would dangle at best and match a same-named local session at worst — which
  would hand local autogate to a stranger. The **receiving A2A door** is its
  only writer: `stamp_remote_parent` lands it on the just-registered child
  from the node record the caller's own signature verified (so `key` is the
  identity and `node` only the label the door knew at stamp time), plus the
  caller's session id off the signed `metadata["aoide/from"]` claim. Never a
  name from a header or the body, and never through the child's env — a local
  `conduct`/`spawn` cannot name one at all, and `resurrect` carries none
  forward. Change-only, like `origin`. Same attribution posture as `origin`:
  `sessions.json` stays a plain, same-uid-writable file, so the gate is the
  key comparison the door makes, never this field as read off disk.
- **Both machines show the link, and neither invents an edge for it** (P-RSA
  S4). On the child's node, `graph.json`'s session node and `aoide session
  --json`'s row carry `remoteParent: {node, sessionId}` — the name resolved to
  the CURRENT `nodes.json` entry for `key`, falling back to the label stamped
  at spawn time, so a rename never orphans the link — and the roster line
  appends `↑ <node>/<sessionId>`; the key itself is never republished. On the
  parent's node, the caller-side ledger (`state/stage/remote-children.json`,
  one row per child this box spawned elsewhere) projects onto the parent's own
  node and roster row as `remoteChildren: [{node, sessionId}]` and `↓ <n>
  remote`. The child keeps its ordinary `anchors`/`leads` edge and gets NO
  `spawned` edge for the far parent: `parentSessionId` is what makes a local
  parent, so a remote one never writes it — not even when a local session
  happens to carry the same id. The child's own local descendants need no
  projection at all: the far node's document nests under its `node:<name>` root
  in this one, carrying that child's `spawned` edges, so `par1 → nodeb/C →
  nodeb/G` is walkable off `remoteChildren` plus the fold, with no wire call of
  its own. A ledger row leaves with its parent: `prune_done_scoped` retains
  the file against the ids it removed.
- **`seal` + `sealedIssuedAt`** — the sealed session credential, sharing
  one lifecycle (always both or neither). `aoided` mints an ed25519 keypair
  once per process and holds it in memory only, never on disk — a separate
  key from `state/identity/`'s on-disk node-wire key, which any same-uid
  reader could sign with. The signature covers a canonical NUL-separated
  string of five fields: `sessionId`, `pid`, `pidStarttime`, `originClass`,
  `issuedAt` — `sessionId`/`originClass` verbatim, no trim or case-folding,
  so a seal minted for one exact identity cannot verify against a
  differently-cased one. Two stamp sites cover the two registration paths:
  the daemon's `dispatch` handler mints right after a `session start`, and
  a tick-driven sweep seals any live pid-carrying record still missing one
  within ~1s, however it registered.

Verification is live, never cached. The verifier re-reads
`/proc/<pid>/stat` field 22 fresh for the record's pid (the pid-reuse
defense: a stale value for a recycled pid fails the live read; a stored
`0` — the mint-time degrade when `/proc` was already unreadable — is
unverifiable, never "verified") and fetches the daemon's CURRENT public key
over its `ping` reply's `sealPubkeyHex` field. `aoide_storage::attest` is
the one implementation of the walk + verify, shared by the send gate
([[Conductor-Channel]]) and the secrets broker's origin gate
([[Secrets-Broker]]). A pubkey file sitting beside the seal on the same
same-uid-writable `sessions.json` is cryptographically void — a forger of
the seal forges the "trusted" key with it — so the live round trip is the
only channel asked. The trust root is Yama `ptrace_scope>=1` plus process
liveness (the lane's answered threat-model fork, OQ1-A): with Yama off the
seal degrades to liveness-only. An unreachable or impostor daemon leaves
every seal unverifiable — never a fallback to trust.

**The lane's accounting — enforced vs open.** Enforced end to end:
`origin` is write-once and door-stamped (P-ID0); every live, pid-carrying
session carries the seal (P-ID1); the send gate and the per-session
socket's accept key on kernel facts plus a verified seal, never env
(P-ID2); shellbridge's verdict socket and `aoided`'s dispatch socket hold a
cross-uid `SO_PEERCRED` floor (P-ID3, [[shellbridge]], [[aoided]]); the
secrets broker's `allowRemoteOrigin` gate is the credential's first policy
consumer (P-ID4); the node wire resolves identity by verifying key, never
claimed name (P-ID5, [[Node-Federation]]). Open, each named rather than
implied closed: (1) **consumer-name authentication** — the seal
authenticates the session and its class, never a self-asserted consumer
string; (2) **OQ1-B, the own-uid daemon** — not taken; a same-uid attacker
can race or evict the daemon's listener (`bind_socket` unlink-then-binds,
no flock guard) and serve a forged `sealPubkeyHex`, and a same-uid process
can append an unsealed roster row claiming `origin: local` that the reseal
sweep then signs into a positive attestation (the sweep-relaunder
residual); (3) **a cross-uid attestation channel** — the packaged
`aoide-secrets` broker cannot reach the operator's daemon socket or roster,
so the origin gate is dormant in that deployment; (4) **an authenticated
registration path** — the closer for the sweep-relaunder shape; (5)
**per-surface dispatch-door gates** riding the attested-caller lookup.

## Contracts (CONTRACTS.md §4, still v0)

- `projects.json` v0: `{schemaVersion, projects: [{name, path, roots?}]}` —
  `roots` is the project's FULL ordered anchor-root list (optional only for
  a record predating the field; always written in full by `project
  add`/`edit`/`remove`, never extras-only); `path` is always the first and
  mirrored at `roots[0]`.
- `graph.json` v0: `{schemaVersion, nodes: […], edges: [{from, to, kind}]}` —
  fully resolved, so Quickshell never recomputes anchoring.
- `sessions.json` records carry several **additive** optional fields (no
  version bump): `parentSessionId` (spawned edge), `logPath` (a
  headless-conducted session's pty-master log — [[Conductor-Channel]]),
  `hookAncestry` (up to 8 self-first `/proc` pids, stamped once, the
  auto-parenting seam above), and the identity pair `origin` and
  `seal`/`sealedIssuedAt` (the lane's provenance and credential, above).
  The full field list — `kind`/`conductable`,
  `contextTokens`/`contextCeiling`/`tool`/`petname` and more — is CONTRACTS.md
  §4; each is round-tripped by every rewriter regardless of whether it
  understands the field.

## Open seams

`state/stage/` (the conducting tree this page's commands read and write) and
`song/stage/` (rice/paint staging) resolve through two separate functions —
`conducting_stage_dir()` and `stage_dir()` — that share the same
`$AOIDE_STAGE_DIR` absolute-path override so a relocated stage tree relocates
as a unit, and otherwise fall back to two different default directories
("Stage-dir resolution", `CONTRACTS.md §4` — see [[shellbridge]]). The
`focuswindow` exit-0 ambiguity is resolved by the liveness check above.
Spawn-time parenting is now two-layered: the kitty wrapper stamps
`parentSessionId` directly for the common nested-terminal case
([[Conductor-Channel]]), and a hook-registered session with no explicit
`--parent` resolves one automatically via the `hookAncestry` walk — so `graph
link` is the manual override path for edges outside both, not the only
source (`graph link` is the one other command the `graph` prefix still owns
after the Lane R cutover, alongside the bare render).

The dock's `SUPER+G` toggle and the launcher's `SUPER+SPACE` both resolve
in-process (Hyprland global shortcuts the QML registers itself) rather than
through an `aoide shell` CLI command; only `SUPER+ESCAPE` (lock) still execs
an `aoide shell lock` command absent from the schema (open thread, see
[[aoide-cli]]).

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
- [[Node-Federation]] — the wire half of key-verified identity (P-ID5)
- [[Secrets-Broker]] — the origin gate, the credential's first policy consumer (P-ID4)
- [[Node-Transport]] — the reaper backstop that collects a tunnel a killed session never closed

## Managed task runs

A managed task run (`aoide spawn --task <slug>`, see
[Managed-Task-Wrapper.md](Managed-Task-Wrapper.md)) is an ordinary conducted
session carrying two extra record keys of its own — `task` (the slug, which is
also the CHILD's inbox name) and `instructionsPath` — plus the end facts
`outcome` (`exit`/`signal`/`timeout`/`stopped`), `exitCode` (present only for a
real exit), `endedAt`, and `reportTo` (the mailbox the run's report is addressed
to). `exitCode` is ABSENT (never `0`) when the run was killed, signalled or
reaped.

- **`session watch <id> [--tail N] [--snapshot] [--json]`** — the read-only
  live view of such a run: its instructions, its live output, the CHILD's own
  unread queue (letters sent TO the subagent, read through a cursor-free peek),
  and where it stands. Live is the default (it follows until
  the run ends or Ctrl-C); `--snapshot` is the single bounded frame a door that
  must return asks for. It writes nothing and advances no cursor, and its footer
  reports `running`, `stopped — not running` or `exited <code> at <endedAt>` —
  an exit is never rendered as task success.
- A finished run's exit report is filed to its REPORT mailbox by the daemon tick
  (`state/stage/taskreport.json` is its delivery cursor) — `--report-to`, else
  the parent's id when that is a legal mailbox name, else the role mailbox
  `conductor`. The run's own slug mailbox (`self/<slug>`) carries only the
  letters sent TO the child; the report is never filed there. An unfiled run's
  record, and a filed one, are both retained against the sweep's prune, so a
  completed task stays resolvable.
