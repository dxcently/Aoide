---
type: concept
created: 2026-08-01
updated: 2026-08-28
tags: [aoide, agent, a2a, orchestration, interop]
source: "[[references/AOIDE-HANDOFF]]"
---

# A2A Door — aoide as an A2A agent, and an A2A client

A2A (**Agent2Agent**, a Linux Foundation protocol) is aoide's **third door**,
alongside the [[Agent-Interface|CLI trunk and the MCP façade]]. All three
derive from the one self-registering command registry (`aoide schema --json`),
so none of them can drift from the others. Where the CLI is the local surface
and MCP the structured tool layer, A2A is the **standard wire** by which aoide
interoperates with *other* agents: it speaks the A2A **v0.3.x JSON-RPC-2.0 /
HTTP binding** (method names like `message/send`, `tasks/get`; lowercase-kebab
task states like `input-required`).

The door is **bidirectional**. aoide is both:

- a discoverable A2A **agent** (a server other agents can address), and
- an A2A **client** that registers external A2A agents and drives them.

## Server — aoide as a discoverable agent

`aoide a2a serve` (and the `aoide-a2a.service` user unit) raises a hand-rolled,
dependency-free JSON-RPC-2.0-over-HTTP server: a blocking accept loop over a
hand-parsed HTTP/1.1 layer, no new crates. It keeps the same offline-lock
discipline the CLI and MCP servers hold. It is **off by default** (`aoide.a2a.enable = false`,
the same house policy as the MCP façade) and **loopback-bound**
(`aoide.a2a.bindAddress` defaults to `127.0.0.1`, `aoide.a2a.port` to `8710`);
exposing it to a network is a deliberate per-host choice, never the default.

The server serves:

- **AgentCard** at `/.well-known/agent-card.json` — generated from the command
  registry, filtered to **implemented** commands only (a stub is never
  advertised as a live skill). This is the same one-schema discipline that
  produces the MCP tool list: one command inventory, no second list to drift.
  The card carries the static metadata the schema does not (`url`, `version`,
  `capabilities.streaming`, input/output modes).
- **`tasks/get`** — a task's status, read from the session stage.
- **`message/send`** — the invoke door. It either **injects** the message into
  an existing conducted session named by `contextId`, or **spawns** a new
  conducted agent. The spawn runs a **configured** agent
  (`aoide.a2a.spawnAgent`), never a client-supplied command — the client
  supplies only the prompt, and that prompt is the spawned session's opening
  turn. The door parks it in `pending.json` for a human rather than typing it.
  A spawn may also name a task
  (`metadata["aoide/task"]`), which makes the child the SAME
  [[Managed-Task-Wrapper|managed task run]] a local `aoide spawn --task`
  produces: task mailbox, exit report, a record the routine prune retains. A
  spawn is always headless — the door has no terminal to hand the child — and
  its wrapper argv is the local one's, built by one shared builder. See
  [[#Security and governance]].
- **`message/stream`** and **`tasks/resubscribe`** — Server-Sent-Events
  streaming of task-status updates. A stream emits on state change, marks the
  terminal frame `final: true`, and polls the stage on a short tick. It is
  bounded in lifetime, so a never-terminal session (an idle agent) still
  closes cleanly at the cap and frees its connection slot.

## Client — aoide driving external agents

The outbound half lives in the `node` family ([[Node-Federation]]), not on the
A2A door itself. `node add <name> <url>` verifies a remote aoide instance by
fetching its AgentCard (a bare origin has the well-known path appended) before
registering it into `state/nodes.json`. `node pull` refreshes its cached
graph over `aoide/graphSummary`. `node spawn <name> -- <text>` starts a new
session on a PAIRED node's own configured agent: it POSTs a signed,
spawn-shaped `message/send` to the node's A2A door, and the node's own
paired+signature+grant gate is the sole authority. `send --to
node/<name>` (or `--to <name>/<session>`) drives an EXISTING remote session:
it resolves the target against the node's cached graph and sends over the
same `message/send` RPC — mutually exclusive with `--id`. A remote send is
always attempted, and the receiving node parks it for its own approver. Each
registered node folds into the [[Session-Graph]] as a root node, so a remote
instance appears in the DAG beside aoide's own sessions. The message text is
untrusted data, never executed.

## The mapping — A2A concepts to aoide's own vocabulary

aoide's session vocabulary already has an A2A shape, so the binding is a
translation, not a new model:

| A2A concept | aoide equivalent |
|---|---|
| AgentCard @ `/.well-known/agent-card.json` | discovery derived from the command registry (`schema --json`) — one schema, same as the MCP tool list |
| Task (one unit of work) | a **turn** — what `send` injects into a session |
| `contextId` | a **session** ([[Session-Graph|SessionRecord]]) |
| TaskState `WORKING` | canonical state `working` |
| TaskState `INPUT_REQUIRED` | canonical state `awaiting` |
| TaskState `AUTH_REQUIRED` | the **`needsSudo`** signal — a conducted shell blocked at a `sudo` password prompt |
| TaskState `COMPLETED` | canonical state `stopped` — a completed *task* is an ended *turn*; the *session* lives on |
| TaskState `SUBMITTED` | canonical state `idle` — aoide's at-rest/cold state, acknowledged but not actively processing |

Because a Task is a turn and a `contextId` is a session, `COMPLETED` maps to
`stopped` (the turn ended, the agent is back at its prompt), not to session end.
`AUTH_REQUIRED` is a **narrowing**: A2A's auth-required covers any
client-supplied credential, of which `needsSudo` is aoide's one instance —
`needsSudo` is a signal carried alongside `state`, so a session that is both
`awaiting` and `needsSudo` surfaces as `AUTH_REQUIRED` (auth-required takes
precedence). The dock renders this same signal as the Conductor gadget's
[[Gadget-Dock|sudo lock badge]].

## Security and governance

The A2A door fits the [[Governance]] model — the user admits, agents propose,
git records — with one adaptation forced by the wire: **a request/response
cannot block on a human admission**, so nothing a remote caller sends is
delivered on the wire at all. Every admitted remote send parks in the
receiving node's `pending.json`, the same queue `send` uses, and a human (or
the conductor) approves it there. Enabling the door is admitted at
**rebuild time**.

- **Admitted at rebuild time.** Enabling the door (`aoide.a2a.enable`) and
  setting the spawn target (`aoide.a2a.spawnAgent`) are nix options, so both go
  through the normal [[Rebuild-Gate|rebuild gate]] — turning them on is the
  user's admission, made once at rebuild, not per request.
- **The spawn agent needs its own PATH.** A systemd user unit's default PATH
  is minimal and carries none of the system profile, so a bare-word
  `spawnAgent` (e.g. `claude`) can fail to resolve even though an
  interactive shell finds it fine — found live when a door-summoned spawn on
  a headless node failed with `failed to conduct \`claude\`: No such file or
  directory`. `aoide.a2a.spawnPath` (list of packages, default empty) rides
  the `aoide-a2a` unit's PATH so `spawnAgent` resolves; a host enabling spawn
  names the agent AND the package(s) that provide it, the same
  explicit-package convention `shellbridge` and the secrets popup watcher
  already follow. `openssh` is always on that PATH: the door drains the
  outbox after every deposit and the tunnel spawns `ssh` by bare name, so
  without it every door-side delivery fails with a spawn error and its
  back-off holds the daemon drain off too.
- **Bounded per request.** A `message/send` spawn runs **only** the configured
  `spawnAgent` executable — the client names the prompt, never the command. If
  `spawnAgent` is empty (the default), spawning is unavailable and the door
  returns a structured error rather than launching anything. The configured
  command is default-denied: it runs only as a proven harness (`claude`,
  `env claude`), and the rule is stated once in `CONTRACTS.md` (the spawnAgent
  paragraph). An inject steers a session already running under aoide's
  conductor, through the same gated door `send` uses: it parks in that
  session's pending queue until a terminal or the conductor approves it. A
  forwarded A2A message is **data**, routed
  through the dispatcher, never executed — the A2A door adds no new trust tier.
- **A spawn's child is the local wrapper, always headless.** The door has no
  terminal to hand a child, so its spawn is `conduct --spawned --headless`,
  built by the ONE argv builder a local `aoide spawn` uses
  ([[Managed-Task-Wrapper]]). A caller may also name a task
  (`metadata["aoide/task"]`) — an illegal slug is refused `-32602` with a
  taught message (the echoed value cleaned and capped by the shared sanitizer),
  before any id, argv or process exists — and that turns the child into a full
  managed task run on the far node: a mailbox named by the caller's slug, an
  exit report, and a record `session watch` resolves. A slug a live run already
  holds is refused `-32602` too, through the same admission step a local
  `spawn --task` consults — the door reaches the wrapper WITHOUT bypassing the
  wrapper's own checks, so a peer cannot squat the operator's task name.
  Three consequences a caller should know. The mailbox namespace is FLAT and
  shares the roster's label space: no name is reserved (`conductor` included),
  and the slug becomes the run's own session name in the roster and the
  [[Session-Graph|graph]] — the same power a local `spawn --task` has always
  had, now reachable by a paired peer. A door-summoned run is retained only
  until its report is filed, then swept like any finished session, so a peer
  cannot grow this node's roster without bound. And because the child is
  headless, it is REACHABLE, not merely watchable: a ping-back line or a
  mail-side doorbell write lands in its input, where a non-headless wrap with no
  channel would have been skipped. All of it is the local wrapper's own
  semantics, reached through this door — not a second set of rules.
- **Inject never delivers at this door.** An inject that is admitted parks in
  `pending.json`, the same queue the local `send` uses. Attribution is not a
  gate: a node caller's entry carries `node:<name>`, whichever rung resolved
  the node. The door cannot approve, because `door_can_approve` is true only
  for the local CLI and the daemon. A network caller therefore queues a steer
  and cannot release it. A human releases it with
  `aoide session pending approve`, or the conductor does.
- **Inject admission.** What an inject decides is only whether the caller
  reaches that queue at all. With no door-wide token configured, every inject
  does. Once one is configured, a `contextId` send from a caller that proved
  neither that token nor a node identity gets the uniform answer: the
  synthetic `submitted` Task (#50), which names no session and touches neither
  `sessions.json` nor `pending.json`. An unauthenticated caller therefore
  cannot tell a real `contextId` from a bogus one. A node identity is ONLY the
  node's own bearer (the Token rung, matched against its `tokenFile`) or a
  verified per-request signature (the Signature rung). An address match alone
  admits nothing.
- **Pairing gates Spawn; the door-wide bearer gates only unsigned reads.**
  A spawn runs only for an identified, paired node: `spawn_admitted`
  requires a request resolved on the Signature rung (a verified per-request
  ed25519 signature against some `verified` node's stored pubkey —
  [[Node-Federation]]'s Signature rung) whose grant in the mesh the request
  names holds `"spawn"`. Every refusal is `-32006` with a shape-specific taught
  message — a paired-but-unsigned caller is told to sign, a signed caller
  lacking the grant is told the exact `node allow` fix, everyone else is told
  to pair and sign. `aoide.a2a.tokenFile` (read once at `a2a serve` launch,
  compared with a length-independent byte scan `node_store::token_bytes_eq`)
  authenticates the read arms (`tasks/get`, the AgentCard GET,
  `aoide/graphSummary`), still `-32005`-gating them, and it is the token the
  Inject admission above tests. It never reaches Spawn. Empty (the default)
  leaves the read arms open exactly as an untokenized server always was. The
  bearer gates UNSIGNED callers only: its job is to tell the operator apart
  from callers a tunnel or proxy makes look local. A signed caller is
  governed by its grants instead — a verified signature whose grant in the
  request's mesh includes `read` reads `tasks/get`, `tasks/resubscribe` and
  `aoide/graphSummary` with no bearer, so a paired peer's `node pull` works
  against a bearer-gated door; a signed caller without `read` is refused like
  an unsigned one. The AgentCard GET and `message/stream` stay bearer-only.
- **Where the grant comes from.** Every gated arm asks one lookup,
  `a2a::grant_in_mesh`, and it has two sources: the caller's PAIRED RECORD in
  the mesh its signed request names, and — when a charter governs that mesh at
  this node — the caller's line on that CHARTER, read by identity key and by
  no name. A request that names no mesh is RESOLVED to the home mesh and then
  judged by that mesh's rules like any other: the charter's line where one
  governs, nothing at all where the mesh is charter-shaped with an undecidable
  operator key, and paired records only where no charter is shaped for it. A
  key a charter does not list holds nothing in that mesh however it is paired.
  Local `aoide node allow <name> <cap> off --mesh <m>` narrows a charter line and
  wins over it; nothing local widens one. See
  [[HTTPS-Mesh-API]]'s "Trust per mesh".
- **A secrets-broker-resolved bearer takes precedence over the file.**
  `a2a serve --bearer-secret <name>` (or `AOIDE_A2A_BEARER_SECRET`) names a
  secret this door resolves through the local [[Secrets-Broker]] as consumer
  `a2a-door`, fresh on every connection rather than once at launch — the
  point of routing it through the broker is that a `secrets rm` or policy
  edit takes effect immediately, with no daemon restart. A broker resolve
  failure (unreachable, denied, or a 2-second timeout) fails CLOSED: the door
  substitutes a per-call unguessable sentinel value as the expected token
  rather than falling back to no-token-configured behavior, so every bearer
  check on that connection is denied. When `bearer-secret` is unset, the
  file mechanism above is unchanged.
- **Loopback earns nothing by its address.** Loopback is a peer address like
  any other, and it proves no identity. Once a door-wide token is configured,
  a loopback caller that did not present it gets the same uniform answer as
  any other unauthenticated caller. There is no separate "trust loopback"
  toggle, because behind a reverse proxy or tunnel (`ssh -R`, a tailscale
  funnel, cloudflared, nginx) the server sees the proxy's own loopback address
  for every caller, so an unconditional loopback trust would hand a remote
  attacker the same standing as the operator. A caller that presents the valid
  token reaches the queue, and its send still parks.
- **Per-node tokens identify WHICH node, not just whether one is trusted.**
  `Node.tokenFile` (`state/nodes.json`, set via `node add --token-file
  <path>`) is a separate, per-node secret from the server-wide `tokenFile`
  above, and like it a legacy escape for unpaired callers. A presented token
  that matches a node's own token file resolves that node
  (`node_store::resolve_node`'s Token rung, tried first), and that node's
  Inject is never answered with the uniform response. A shared secret could
  never tell two nodes apart, so identifying which node called needs one file
  per node, not one flag for the whole door. The node's own token authenticates
  no read and never reaches Spawn.
- **An address match names a node and proves nothing.** The address rung
  (`NodeRung::Addr`) resolves a node for attribution only, with an address
  match's own limits: a first-match tie-break, live DNS, and spoofability from
  where it matches (CONTRACTS.md §6, review F5). Once a door-wide token is
  configured it admits nothing.
- **The `aoide/from` claim is honoured only on the signature rung.** A claim
  from any other caller is ignored with one audit line. A spawn stamps an
  honoured claim on the child as its `remoteParent`, and a malformed one is
  refused `-32602` on a spawn only. An inject carrying one proceeds as if it
  had none, since no claim changes whether an Inject parks.
- **The outbound direction has its own bearer.** `node add --bearer-secret
  <name>` records a secret THIS instance resolves through the local secrets
  broker, as consumer `a2a-client`, on every outbound call to that node —
  the opposite direction from `Node.tokenFile` above (what the node presents
  to us). See [[Node-Federation]] for the registry shape.
- **Audited.** Every inject, spawn, and error — including Spawn's `-32006`
  pairing-gate refusals and the signed-request `-32007`/`-32008`/`-32009`
  failures — writes to the single audit log as `Door::A2a`, the
  same log every other door writes. Loopback by default, off by default.

## Related

- [[Agent-Interface]]
- [[Node-Federation]] — the aoide-to-aoide door built on top of this one
- [[Pairing-Ceremony]] — the commit-then-reveal handshake whose wire methods (`aoide/pairRequest`/`aoide/pairReveal`/`aoide/pairPoll`) ride this door
- [[Node-Transport]] — reaching this door through a tunnel when a paired node isn't directly HTTP-reachable
- [[Governance]]
- [[Session-Graph]]
- [[Conductor-Channel]]
- [[Gadget-Dock]]
- [[Terminal-Commander]]
- [[aoide-cli]]
