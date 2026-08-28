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
  supplies only the prompt. See [[#Security and governance]].
- **`message/stream`** and **`tasks/resubscribe`** — Server-Sent-Events
  streaming of task-status updates. A stream emits on state change, marks the
  terminal frame `final: true`, and polls the stage on a short tick. It is
  bounded in lifetime, so a never-terminal session (an idle agent) still
  closes cleanly at the cap and frees its connection slot.

## Client — aoide driving external agents

The outbound half lives in the `peer` family ([[Peer-Federation]]), not on the
A2A door itself. `peer add <name> <url>` verifies a remote aoide instance by
fetching its AgentCard (a bare origin has the well-known path appended) before
registering it into `state/peers.json`. `peer pull` refreshes its cached
graph over `aoide/graphSummary`. `peer spawn <name> -- <text>` starts a new
session on a PAIRED peer's own configured agent: it POSTs a signed,
spawn-shaped `message/send` to the peer's A2A door, and the peer's own
paired+signature+allows gate is the sole authority. `send --to
peer/<name>` (or `--to <name>/<session>`) drives an EXISTING remote session:
it resolves the target against the peer's cached graph and delivers over the
same `message/send` RPC — mutually exclusive with `--id`, and a remote send
always attempts delivery since the receiving peer gates its own inject. Each
registered peer folds into the [[Session-Graph]] as a root node, so a remote
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
cannot block on a human admission**, so there is no interactive per-request
gate like `send`'s pending/approve dance. The admission is moved entirely
to **rebuild time** instead.

- **Admitted at rebuild time.** Enabling the door (`aoide.a2a.enable`) and
  setting the spawn target (`aoide.a2a.spawnAgent`) are nix options, so both go
  through the normal [[Rebuild-Gate|rebuild gate]] — turning them on is the
  user's admission, made once at rebuild, not per request.
- **The spawn agent needs its own PATH.** A systemd user unit's default PATH
  is minimal and carries none of the system profile, so a bare-word
  `spawnAgent` (e.g. `claude`) can fail to resolve even though an
  interactive shell finds it fine — found live when a door-summoned spawn on
  a headless peer failed with `failed to conduct \`claude\`: No such file or
  directory`. `aoide.a2a.spawnPath` (list of packages, default empty) rides
  the `aoide-a2a` unit's PATH so `spawnAgent` resolves; a host enabling spawn
  names the agent AND the package(s) that provide it, the same
  explicit-package convention `shellbridge` and the secrets popup watcher
  already follow.
- **Bounded per request.** A `message/send` spawn runs **only** the configured
  `spawnAgent` executable — the client names the prompt, never the command. If
  `spawnAgent` is empty (the default), spawning is unavailable and the door
  returns a structured error rather than launching anything. An inject steers a
  session already running under aoide's conductor, the same door `send`
  uses. A forwarded A2A message is **data**, routed through the dispatcher, never
  executed — the A2A door adds no new trust tier.
- **Non-loopback callers are gated at request time.** `message/send`
  classifies the caller's address first (`a2a::classify_origin` →
  `PeerOrigin`: `Loopback` / `Remote(IpAddr)` / `Unknown`). A `Remote`
  caller falls back to the same interactive pending-approval queue
  `send` uses — auto-delivering on an autogate match: the OR of the
  address check, the per-peer `tokenFile` check, and the signature-rung
  `autogate` flag on the resolved peer's own record
  ([[Peer-Federation]]); an `Unknown` origin (the address couldn't be read
  at all) is never auto-delivered, failing safe like an unmatched `Remote`.
  A verified signature outranks loopback for this question: a request
  `verify_signed_request` already verified is remote by construction (an
  ssh `-L` forward terminates at loopback on this end), so
  `origin_for_inject` strips `Loopback`'s free pass from it and the peer's
  own `autogate` flag — not the arrival address — decides delivery
  ([[Peer-Transport]]).
  This is the one interactive per-request gate the wire otherwise lacks —
  added for [[Peer-Federation|peer federation]]'s non-loopback case, which
  the original loopback-only design didn't need to cover.
- **Pairing gates Spawn; the door-wide bearer gates only the read arms.**
  A spawn runs only for an identified, paired peer: `spawn_admitted`
  requires a request resolved on the Signature rung (a verified per-request
  ed25519 signature against some `verified` peer's stored pubkey —
  [[Peer-Federation]]'s Signature rung) whose record's `allows` contains
  `"spawn"`. Every refusal is `-32006` with a shape-specific taught message
  — a paired-but-unsigned caller is told to sign, a signed caller lacking
  the grant is told the exact `peer allow` fix, everyone else is told to
  pair and sign. `aoide.a2a.tokenFile` (read once at `a2a serve` launch,
  compared with a length-independent byte scan
  `peer_store::token_bytes_eq`) is a legacy escape for unpaired callers:
  it authenticates the read arms (`tasks/get`, the AgentCard GET,
  `aoide/graphSummary`), still `-32005`-gating them, and answers Inject's
  autogate question — it never reaches Spawn. Empty (the default) leaves
  the read arms open exactly as an untokenized server always was.
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
- **Loopback is trusted unconditionally only while no token is configured.**
  `effective_origin(origin, token_configured, token_state)`
  coerces any caller — loopback included — that did not present the valid
  token to `PeerOrigin::Unknown` before `should_deliver_now` ever sees it,
  which never auto-delivers. There is no separate "trust loopback" toggle: behind a
  reverse proxy or tunnel (`ssh -R`, a tailscale funnel, cloudflared, nginx)
  the server sees the proxy's own loopback address for every caller, so an
  unconditional loopback trust hands a remote attacker the same standing as
  the operator. A caller that presents the valid token keeps
  loopback's original standing exactly.
- **Per-peer tokens identify WHICH peer, not just whether one is trusted.**
  `Peer.tokenFile` (`state/peers.json`, set via `peer add --token-file
  <path>`) is a separate, per-peer secret from the server-wide `tokenFile`
  above — a legacy escape for unpaired callers, like it.
  `peer_store::is_autogated_peer_token` folds a presented token
  against every registered peer's own token file, and Inject's autogate
  match is the OR of the address check, this token check, and the
  signature-rung `autogate` flag — a shared
  secret could never tell two peers apart, so identifying which peer called
  needs one file per peer, not one flag for the whole door.
- **The outbound direction has its own bearer.** `peer add --bearer-secret
  <name>` records a secret THIS instance resolves through the local secrets
  broker, as consumer `a2a-client`, on every outbound call to that peer —
  the opposite direction from `Peer.tokenFile` above (what the peer presents
  to us). See [[Peer-Federation]] for the registry shape.
- **Audited.** Every inject, spawn, and error — including Spawn's `-32006`
  pairing-gate refusals and the signed-request `-32007`/`-32008`/`-32009`
  failures — writes to the single audit log as `Door::A2a`, the
  same log every other door writes. Loopback by default, off by default.

## Related

- [[Agent-Interface]]
- [[Peer-Federation]] — the aoide-to-aoide door built on top of this one
- [[Pairing-Ceremony]] — the commit-then-reveal handshake whose wire methods (`aoide/pairRequest`/`aoide/pairReveal`/`aoide/pairPoll`) ride this door
- [[Peer-Transport]] — reaching this door through a tunnel when a paired peer isn't directly HTTP-reachable
- [[Governance]]
- [[Session-Graph]]
- [[Conductor-Channel]]
- [[Gadget-Dock]]
- [[Terminal-Commander]]
- [[aoide-cli]]
