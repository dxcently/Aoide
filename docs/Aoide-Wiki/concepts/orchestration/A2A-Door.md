---
type: concept
created: 2026-08-01
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
dependency-free JSON-RPC-2.0-over-HTTP server — a blocking accept loop and a
hand-parsed HTTP/1.1 layer, no new crates, the same offline-lock discipline the
CLI and MCP servers keep. It is **off by default** (`aoide.a2a.enable = false`,
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
  terminal frame `final: true`, polls the stage on a short tick, and is bounded
  in lifetime, so a never-terminal session (an idle agent) still closes cleanly
  at the cap and frees its connection slot.

## Client — aoide driving external agents

`aoide a2a agent add|list|remove|send` is the outbound half. `agent add <url>`
fetches an external agent's AgentCard (a bare origin has the well-known path
appended), requires at least a `name`, and registers it into
`state/a2a-agents.json`. Each registered agent folds into the [[Session-Graph]]
as a root node of `kind: "a2a"`, so a remote agent appears in the DAG beside
aoide's own sessions. `agent send <name> <message>` is the **drive verb**: it
POSTs a JSON-RPC `message/send` to the agent's endpoint and reports the returned
Task or Message. The external endpoints carry no local credential, so the fetch
is a plain user-initiated request; the message text is untrusted data, never
executed.

## The mapping — A2A concepts to aoide's own vocabulary

aoide's session vocabulary already has an A2A shape, so the binding is a
translation, not a new model:

| A2A concept | aoide equivalent |
|---|---|
| AgentCard @ `/.well-known/agent-card.json` | discovery derived from the command registry (`schema --json`) — one schema, same as the MCP tool list |
| Task (one unit of work) | a **turn** — what `graph send` injects into a session |
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
gate like `graph send`'s pending/approve dance. The admission is moved entirely
to **rebuild time** instead.

- **Admitted at rebuild time.** Enabling the door (`aoide.a2a.enable`) and
  setting the spawn target (`aoide.a2a.spawnAgent`) are nix options, so both go
  through the normal [[Rebuild-Gate|rebuild gate]] — turning them on is the
  user's admission, made once at rebuild, not per request.
- **Bounded per request.** A `message/send` spawn runs **only** the configured
  `spawnAgent` executable — the client names the prompt, never the command. If
  `spawnAgent` is empty (the default), spawning is unavailable and the door
  returns a structured error rather than launching anything. An inject steers a
  session already running under aoide's conductor, the same door `graph send`
  uses. A forwarded A2A message is **data**, routed through the dispatcher, never
  executed — the A2A door adds no new trust tier.
- **Audited.** Every inject, spawn, and error writes to the single audit log as
  `Door::A2a`, the same log every other door writes. Loopback by default.

## Related

- [[Agent-Interface]]
- [[Governance]]
- [[Session-Graph]]
- [[Conductor-Channel]]
- [[Gadget-Dock]]
- [[Terminal-Commander]]
- [[aoide-cli]]
