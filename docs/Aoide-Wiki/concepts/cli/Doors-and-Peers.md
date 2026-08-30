---
type: concept
created: 2026-08-19
updated: 2026-08-29
tags: [aoide, cli, mcp, a2a, peer, daemon]
---

# Doors & Federation Commands — MCP, A2A, Peers, Daemon

This group covers the doors onto the one command schema and the federation
surface: the stdio [[Agent-Interface|MCP]] façade (`mcp serve` — `lyra mcp
serve` is the same façade over lyra's own 48-path registry), the [[aoided]]
policy skeleton (`daemon`, `events tail`, `adapter melete`), the desktop
state bridge (`shellbridge`), the interactive session UI (`conductor`,
detailed at [[Conductor-TUI]]), live presence (`who`), the [[A2A-Door]]
server (`a2a serve`), and [[Peer-Federation]] (`peer *`) — the peer family is
also the outbound A2A client, driving remote instances over the same wire.
Handlers are spread across
`pkgs/aoide/crates/server/src/{commands,mcp,daemon,a2a}.rs` (the server
domain), `pkgs/aoide/crates/client/src/{commands,adapter,peer,wire}.rs`
(the outbound half), `pkgs/aoide/crates/conductor/src/` (the TUI), and
`pkgs/aoide/crates/conduct/src/shellbridge.rs`; `mcp serve`'s registration
stays in `pkgs/aoide/crates/cli/src/commands/infra.rs`. The long-running
launches (`mcp serve --stdio`, `a2a serve`, `conductor`) are special-cased in
`pkgs/aoide/crates/cli/src/lib.rs::run_cli`.

Shared state paths (`pkgs/aoide/crates/storage/src/fs.rs`): every tree hangs
off the runtime root `$AOIDE_ROOT` (absolute-path-wins, default `~/.aoide`).
`state/` is `$AOIDE_STATE_DIR` when set to an absolute path, else
`$AOIDE_ROOT/state/`; the conducting stage tree `state/stage/` is
`$AOIDE_STAGE_DIR` when absolute, else `$AOIDE_ROOT/state/stage/`
(`conducting_stage_dir`) — distinct from the rice stage tree `song/stage/`,
which shares the same `$AOIDE_STAGE_DIR` override but otherwise falls back
to `$AOIDE_ROOT/song/stage/` (`stage_dir`). `~/Aoide` is the dev git
checkout, reached via `$AOIDE_FLAKE_ROOT`, not a runtime path. The audit log
(`pkgs/aoide/crates/protocol/src/audit.rs::default_audit_log`) is
`$AOIDE_AUDIT_LOG`, else `$AOIDE_ROOT/log` (when `$AOIDE_ROOT` is absolute),
else `<home>/.aoide/log`. Every one-shot command appends one NDJSON audit record
through the single dispatcher (`cli/src/dispatch.rs`); the doors add their own
records on top. All registry writes in this group are atomic temp-then-rename
(`aoide_storage::fs::atomic_write`). Registry reads tolerate a missing or
corrupt file as an empty list.

Every command takes `--json`. Without it the CLI prints the human `message`
line; with it, an envelope `{status, command, message, gated, changed?, data?}`
(`pkgs/aoide/crates/protocol/src/output.rs`). Exit codes: 0 ok, 1 error,
2 usage, 64 not-implemented (no command on this page is a stub). Outbound HTTP
in the `peer` commands is `curl -sS --max-time 15` shelled out with the
URL/body in argv or on stdin. A paired peer's calls are signed per-request
with this instance's ed25519 identity key (`state/identity/`); beyond that,
no local credential past an optional peer bearer secret is involved.

### aoide mcp serve

```
aoide mcp serve [--stdio] [--json]
```

- **Reads:** with `--stdio`, newline-delimited JSON-RPC 2.0 on stdin
  (one request per line). Handled methods: `initialize` (protocol version
  `2024-11-05`, serverInfo `{name: "aoide", version: <AOIDE_VERSION>}`,
  tools capability), `tools/list` (generated one-to-one from the command
  registry — each command becomes a tool named by its dotted path, args/flags
  as `inputSchema` properties, `annotations.gated` surfaced), `tools/call`
  (dispatches into the same handler the CLI uses, as `Door::Mcp`).
  `notifications/*` gets no reply; unknown method → `-32601`; unknown tool →
  `-32602`; unparseable line → `-32700`.
- **Writes:** one JSON-RPC response line per request on stdout. Each
  `tools/call` routes through the dispatcher, so it appends to the audit log
  with door `mcp` exactly like a typed command.
- **Output:** without `--stdio` the handler only reports how to launch:
  data `{hint, toolCount}`. With `--stdio` the process blocks serving until
  stdin closes (exit 0) or an I/O error (exit 1,
  `eprintln "aoide mcp serve: …"`). Implementation:
  `pkgs/aoide/crates/server/src/mcp.rs`.
- **Notes:** not gated. Off by default (`aoide.mcp.enable = false`); spawned
  per session as `aoide mcp serve --stdio`. One implementation, two doors —
  the tool list cannot drift from `schema --json`.

### aoide daemon

```
aoide daemon [--audit-log <path>] [--json]
```

- **Reads:** nothing beyond the flag; the audit path resolves as
  `--audit-log` → `$AOIDE_AUDIT_LOG` → `$AOIDE_ROOT/log` (absolute) →
  `<home>/.aoide/log`.
- **Writes:** appends NDJSON `AuditRecord` lines to the audit log: a
  `daemon started` record (door `daemon`, class `audit`) and a gate self-check
  proposal (class `gate`, status `proposed`, `admitted: false` — the rebuild
  stays user-admitted only).
- **Output:** data is the skeleton status document `{daemon: "aoided",
  state: "skeleton", auditLog, singlePolicySurface: true,
  subscriptionModel: "default-deny-per-class", notificationDeniedByDefault,
  rebuildGate: {userGated, agentCanAdmit, lastProposal}, eventClasses}`
  (`pkgs/aoide/crates/server/src/daemon.rs`).
- **Notes:** walking skeleton — the audit append and the gate are real code
  paths; the event bus and subscription model are in-memory types, and the
  full event loop is not implemented. The standalone `aoided` binary
  (`pkgs/aoide/crates/cli/src/bin/aoided.rs`, takes `--audit-log <path>`,
  prints the status JSON to stdout) is the same code path a systemd unit
  launches. See [[Governance]].

### lyra shellbridge

```
lyra shellbridge [--run] [--json]
```

- **Reads:** env `$XDG_RUNTIME_DIR` (socket parent; falls back to
  `/run/user/1000`), `$AOIDE_DEFAULT_SONG` (the rice-mode toggle's
  declarative-direction song, baked in by `modules/nucleus/shellbridge.nix`);
  `song/stage/mode.json` (the toggle's current mode — rice staging, so it
  stays under `song/stage/`). Newline-delimited JSON commands on its socket
  (below).
- **Writes:** seeds `state/stage/sessions.json` and `state/stage/hooks.json`
  with their empty v0 registry shapes, atomic and only when absent or corrupt
  (`seed_if_absent` — a populated roster survives a restart); binds
  `$XDG_RUNTIME_DIR/aoide/shellbridge.sock` (a stale socket file is removed
  first); maintains `state/stage/herald.json` (notification ledger, v0,
  capped at 20 cards, atomic read-modify-write on the accept loop);
  appends audit records (door `daemon`) to the default audit log for every
  dispatched command. All six of these are the CONDUCTING stage files
  (`CONTRACTS.md §4`) — shellbridge writes them into `state/stage/` even
  though the `shellbridge` command itself ships in `lyra`; rice staging
  (`mode.json`, `livery.json`, `grimoire.json`, `cover.json`) stays under
  `song/stage/`, untouched by this split.
- **Output:** blocks forever serving the socket (systemd unit is
  `Type=simple`); returns an error document only on bind failure
  (`state: "error"`, data `{socket, stageDir, wrote}`). Socket commands
  (`pkgs/aoide/crates/conduct/src/shellbridge.rs`): `focuswindow` /
  `focussession` (Hyprland jump via `crate::graph`), `power`
  (`lock|logout|suspend|hibernate|reboot|shutdown` → spawns `hyprlock`,
  `hyprctl dispatch exit`, or `systemctl suspend|hibernate|reboot|poweroff`,
  detached with a reaper thread), `ricemode` (re-execs the same binary as
  `rice mode <stage|declarative> [song] --json`, waits, then fires a detached
  `notify-send "Aoide" <message>`), `refreshusage` (detached re-exec
  `usage --json`; the gadget picks up the `state/usage.json` write itself),
  `rechecksessions` (detached re-exec `session reap --announce --json`),
  `heraldpush` / `heraldverdict` / `heralddismiss` (the ledger; a verdict
  types the answer into the waiting session through `send`). Also
  spawns the Hyprland window→session listener thread at startup
  (`graph::run_hypr_window_listener`). Malformed/unknown lines are logged to
  stderr and skipped — nothing kills the accept loop.
- **Notes:** not gated. The `--run` flag is registered in the schema but the
  handler (`conduct/src/commands/shellbridge.rs::handle_shellbridge`) never
  consults it — bare `lyra shellbridge` runs the blocking loop either way. The
  accept loop reads `SO_PEERCRED` on every connection and refuses one whose
  peer uid differs from the process's own euid — a cross-uid floor,
  fail-closed like the secrets broker's admin gate (identity lane P-ID3);
  a same-uid caller passes, so the floor is no defense against the
  operator's own uid. Client half:
  `shellbridge::send_line` is how `lyra herald push` and `session permit` reach
  the daemon, which stays the single ledger writer. See [[shellbridge]].

### aoide adapter melete

```
aoide adapter melete [--run] [--json]
```

- **Reads:** env `$AOIDE_ADAPTER_SUBSCRIBE` — a comma-separated allow-list of
  event classes (`audit`, `gate`, `rice`, `content`, `notification`); unknown
  names are ignored and nothing is allowed by default.
- **Writes:** appends one `adapter.melete started` audit record (door
  `daemon`) to the default audit log.
- **Output:** one-shot skeleton self-check; data `{process: "adapter.melete",
  state: "skeleton", subscribed: [...], subscriptionModel:
  "default-deny-per-class", notification: {deliveredAsMetadataOnly: true,
  carriesRawBody: false, allowed, shape: {actionId, appName}}}`
  (`pkgs/aoide/crates/client/src/adapter.rs`). The notification shape is the
  security boundary: forwarded payloads carry metadata only, never the raw
  body.
- **Notes:** not gated. `--run` is registered but the handler never consults
  it — the command is the skeleton self-check regardless. See [[Melete]].

### aoide conductor

```
aoide conductor [--json]
```

- **Output:** `run_cli` dispatches the launch first (an audit record), then
  hands the tty to a ratatui/crossterm loop over seven panels, keys
  `1`–`7`/`Tab`/`BackTab` to switch. Full panel, key, and dispatch detail:
  [[Conductor-TUI]].
- **Notes:** not gated. Distinct from `aoide conduct`, which wraps one
  process into the conductor channel rather than raising this UI.

### aoide events tail

```
aoide events tail [--class <c1,c2,…>] [--json]
```

- **Reads:** follows aoided's own events feed —
  `$AOIDE_DAEMON_EVENTS` when set, else a sibling of the daemon socket
  (`$XDG_RUNTIME_DIR/aoide/events.jsonl`). `--class` narrows to a
  comma-separated set of event classes; omitted or empty prints every
  class.
- **Writes:** nothing.
- **Output:** on the CLI door, one line per event as it arrives; blocks
  until Ctrl-C. On any other door it returns a "run it from a terminal"
  outcome instead of blocking that door — the same posture `secrets watch`
  holds for a foreground follow-style command.
- **Notes:** not gated. Narrates the daemon's own tick-driven producers:
  the secrets-broker events mirror (`released`/`parked`/`completed`/
  `dismissed`/`expired`, name-only) and the hand-edit watcher over the
  broker-owned stage files. See [[aoided]].

### aoide who

```
aoide who [<filter>] [--all] [--json]
```

- **Reads:** every registered peer, probed LIVE on every invocation — one
  thread per peer, bounded by curl's own `--max-time` inside
  `aoide_client::commands::pull_peer_live` (~2 s/peer). A projection, never
  a store: it never writes `state/peer-cache/<name>.json`; the cache
  (`state/peer-cache/<name>.json`) is consulted only as the fallback for a
  peer this invocation's live probe fails to reach, so an unreachable peer
  still renders. `<filter>` resolves through `storage::addr::resolve`
  first (a local id/tail4/petname, a host/role/petname line, or
  `peer/<rest>`), falling back to a substring match.
- **Writes:** nothing.
- **Output:** a Unicode roster; `--json` emits the structured presence
  document. Node presence: `online` / `unreachable` / `never-pulled`.
  Session presence: `online` / `stale` / `done` (`done` omitted without
  `--all`).
- **Notes:** not gated. `<filter>` narrows what is DISPLAYED only — every
  registered peer is probed regardless. See [[Peer-Federation]].

### aoide a2a serve

```
aoide a2a serve [--bind <addr>] [--port <n>] [--spawn-agent <cmd>]
                [--peer-name <name>] [--token-file <path>]
                [--bearer-secret <name>] [--json]
```

- **Reads:** flag → env → default resolution, once at launch: bind
  (`--bind` → `$AOIDE_A2A_BIND` → `127.0.0.1`), port (`--port` →
  `$AOIDE_A2A_PORT` → `8710`), spawn agent (`--spawn-agent` →
  `$AOIDE_A2A_SPAWN_AGENT` → empty = spawning disabled), instance name
  (`--peer-name` → `$AOIDE_A2A_PEER_NAME` → OS hostname → `"aoide"`), token
  (`--token-file` → `$AOIDE_A2A_TOKEN_FILE` → empty = no token required; the
  secret is read off the file once at launch, trimmed, never logged),
  bearer secret (`--bearer-secret` → `$AOIDE_A2A_BEARER_SECRET` → empty —
  names a secret in the [[Secrets-Broker|secrets broker]] instead of a file,
  resolved FRESH on every connection as consumer `a2a-door`; takes
  precedence over `tokenFile` when both are set, and a broker resolve
  failure — unreachable, denied, or a bounded ~2 s timeout — fails that
  connection's bearer check closed rather than falling back to the file or
  the pre-token-open behavior). Per request: `state/stage/sessions.json`
  (task/session state), `state/peers.json` (signed-request verification —
  the signature tried against every `verified` peer's stored pubkey — and
  the autogate match: by caller address, by an autogate-marked peer's own
  `tokenFile`, or by the resolved signed peer's `autogate` flag; read fresh
  off disk per request). The `aoide-a2a` systemd unit
  (`modules/nucleus/aoided.nix`) sets
  `AOIDE_A2A_BIND`/`PORT`/`SPAWN_AGENT`/`TOKEN_FILE` from the nix options;
  `AOIDE_A2A_PEER_NAME`/`AOIDE_A2A_BEARER_SECRET` are flag/env-only.
- **Writes:** an audit record (door `a2a`) for every handled request, spawn,
  and SSE open/close in the audit log; the `message/send` inject path reuses
  `send`'s `session_send`, so a held send writes the session's
  `pending.json` and a delivered one writes the session's control socket.
- **Output:** blocks in a thread-per-connection `TcpListener` accept loop
  (`pkgs/aoide/crates/server/src/a2a.rs`); announces
  `aoide a2a: listening on http://<bind>:<port>/` on stderr; a bind failure is
  a clean exit 1. HTTP surface: `GET /.well-known/agent-card.json` (AgentCard
  derived from the registry, `implemented` commands only, protocolVersion
  `0.3.0`, `url: http://<bind>:<port>/`, `capabilities.streaming: true`) and
  `POST /` (JSON-RPC 2.0):
  - `tasks/get` — the task id and contextId are both the aoide sessionId; the
    state maps `working→working`, `stopped→completed`,
    `awaiting→input-required` (`auth-required` when `needsSudo`),
    `idle→submitted`, `done→completed`. Unknown id → `-32001`.
  - `message/send` — inject into a known conductable session (`contextId`),
    or spawn when `metadata["aoide/spawn"] == true` or no contextId: re-execs
    the aoide binary as `conduct --agent a2a --id a2a-<pid>-<ts> --
    <spawnAgent>` (`setsid`, stdio nulled, reaped on a parked thread,
    `$AOIDE_AUDIT_LOG` passed down), then types the prompt as its first turn
    over the conduct socket with a connect-retry budget. A loopback caller's
    inject auto-delivers; non-loopback queues pending unless it matches an
    `autogate` peer (address, per-peer token, or the resolved signed peer's
    own flag). Once a token is configured, loopback stops being a trust
    signal (unauthenticated → never auto-delivers). Spawn answers to
    pairing alone: the caller must resolve on the Signature rung — a
    verified per-request signature against some `verified` peer's stored
    pubkey — with `spawn` in that record's `allows`; every refusal is
    `-32006` with a shape-specific taught message (pair first / sign the
    request / `peer allow <name> spawn on`), and the door-wide bearer
    never reaches the spawn arm. Signed-request failures carry their own
    codes — `-32007` (malformed or unverifiable signature, partial header
    set), `-32008` (timestamp skew), `-32009` (nonce replay). Spawn unconfigured →
    `-32004`; session not conductable → `-32004`; unknown contextId →
    `-32001`. A held-pending send returns a `submitted` Task immediately.
  - `aoide/graphSummary` — `{schemaVersion: "0", instance: {name, url,
    emittedAt}, graph}` wrapping the same resolved graph document bare
    `graph` renders (CONTRACTS.md §7) — every stage mutation restages
    `state/stage/graph.json` automatically, so there is nothing separate to
    build here.
  - `message/stream` / `tasks/resubscribe` — the socket stays open as SSE
    (`text/event-stream`, one `data: <json>` frame per status change, final
    frame a `TaskStatusUpdateEvent` with `final: true`); stream lifetime
    capped at 600 s, status polled every 750 ms.
  - Hardening caps: body ≤ 1 MiB, header line ≤ 8 KiB, ≤ 100 headers, 15 s
    absolute per-request budget, 10 s read timeout, 64 in-flight connections
    (past that a fast 503). Unknown method → `-32601`, parse error → `-32700`.
- **Notes:** not gated at the CLI level; the security model is bind-address +
  the rebuild-gated `aoide.a2a.spawnAgent` option + the pairing gate on Spawn
  (Signature rung + `allows`) + the optional bearer token on the read arms.
  A client never supplies a command — the spawn path only ever launches the
  operator-configured executable, which must also appear on the unit's PATH
  via `aoide.a2a.spawnPath` (a bare-word `spawnAgent` can't resolve
  otherwise — a systemd user unit's default PATH carries none of the system
  profile). Localhost, user-only, off by default. See [[A2A-Door]].

### aoide peer add

```
aoide peer add <name> <url> [--autogate] [--no-verify]
               [--via ssh://[user@]host[:port]]
               [--token-file <path>] [--bearer-secret <name>] [--json]
```

- **Reads:** `state/peers.json`; verifies the peer by fetching its AgentCard
  (`curl` GET on the resolved card URL) before registering anything — a peer
  that fails the fetch is never added; `--no-verify` skips the fetch for a
  peer that serves no card (reachability, never identity — the record lands
  `verified: false` either way). `--via ssh://[user@]host[:port]` records an
  ssh-transport marker: every later call dials through a lazily opened ssh
  forward instead of `url` directly ([[Peer-Transport]]). Name must match
  `^[a-z0-9][a-z0-9-]*$` (it is joined into `state/peer-cache/<name>.json`;
  the shape check is the path-traversal guard) — else exit 1, `reason:
  invalid-name`.
- **Writes:** inserts into `state/peers.json` (v0, atomic write). A duplicate
  name is rejected cleanly (`reason: duplicate-name`, exit 1) — a nickname is
  never silently repointed.
- **Output:** `"registered peer \`<name>\` → <url>[ (autogate)] (<n> total)"`,
  data `{peer: {name, url, autogate, tokenFile?, bearerSecret?, addedAt}, count}`.
- **Notes:** `--autogate` marks the peer trusted: its inbound `message/send`
  on this instance's A2A door auto-delivers instead of queueing pending,
  matched by caller address or by the per-peer `--token-file` secret (the form
  that survives a reverse proxy/tunnel, where every caller's address is the
  proxy's) — that is what THIS peer must present TO us. `--bearer-secret
  <name>` is the mirror direction: a secret this instance resolves through
  the [[Secrets-Broker|secrets broker]] (consumer `a2a-client`, fresh on
  every request, never cached) and presents as `Authorization: Bearer
  <value>` on every outbound call to that peer's own A2A door (`peer pull`,
  `send --to`, `who`'s live probe). Absent by default — an unmarked
  peer's outbound calls carry no bearer header, unchanged. A resolve
  failure fails that outbound call outright rather than sending it
  unauthenticated. See [[Peer-Federation]].

### aoide peer remove

```
aoide peer remove <name> [--json]
```

- **Reads:** `state/peers.json`; name shape validated as in `peer add`.
- **Writes:** the registry (atomic); also deletes the peer's cache file
  `state/peer-cache/<name>.json` if present (best-effort), so a peer re-added
  under the same name never starts from a stale leftover.
- **Output:** data `{removed: true, name, count}`. A missing name is an
  error (`reason: unknown-peer`, exit 1) — deliberately not idempotent-silent
  (CONTRACTS.md §7).
- **Notes:** not gated.

### aoide peer pull

```
aoide peer pull [<name>] [--json]
```

- **Reads:** `state/peers.json`; an existing `state/peer-cache/<name>.json`
  when a pull fails (to preserve the last-good data). Unknown name → exit 1,
  `reason: unknown-peer`.
- **Writes:** `state/peer-cache/<name>.json` (v0, atomic). Success stores
  `{schemaVersion, name, instance?, graph?, fetchedAt, stale: false}` parsed
  from the peer's reply (requires `result.schemaVersion == "0"` and a
  `result.graph` object, else the pull counts as failed). Any failure —
  unreachable, timeout, non-200, malformed — flips `stale: true` and
  `lastError` on the existing entry instead of deleting it; the last-good
  `instance`/`graph` survive.
- **Output:** POSTs `{"method": "aoide/graphSummary", "params": {}}` to each
  selected peer's URL via curl. No `<name>` pulls every registered peer; one
  peer being down never breaks the others. `"pulled <ok>/<n> peer(s)
  successfully"`, data `{results: [{name, ok, fetchedAt?|error?}]}`; no peers
  registered → ok with `results: []`.
- **Notes:** fresh caches fold into the session DAG as `peer:<name>` root
  nodes (`aoide-conduct`'s `build_graph`).

### aoide peer status

```
aoide peer status [--json]
```

- **Reads:** `state/peers.json` plus each peer's `state/peer-cache/<name>.json`.
- **Output:** the human line stays a terse `"<n> peer(s) registered"`; `--json`
  carries the full row per peer — `{peers: [{name, url, autogate, tokenFile?,
  bearerSecret?, hub, pubkey?, verified, allows, addedAt, state, fetchedAt,
  error}]}`. `state` is `fresh` (pulled within the 300 s TTL and not marked
  stale), `stale`, or `never-pulled` — the same three-way classification the
  graph fold uses, so this and the DAG never disagree. This is the deep
  per-peer registry view; `peer list` (below) is the one-glance roster and
  never duplicates it.
- **Notes:** read-only. TTL constant: `PEER_CACHE_TTL_SECS` in
  `pkgs/aoide/crates/storage/src/peer_store.rs`.

### aoide peer hub

```
aoide peer hub <name> [--clear]
```

- **Reads:** `state/peers.json`.
- **Writes:** designates `name` as THE hub — at most one peer holds the
  designation; setting a new hub moves it, clearing the previous holder in
  the same write (P-D5, `docs/architecture/AOIDED.md`'s "The hub option").
  `--clear` removes the designation from `name` instead, if it currently
  holds it. Idempotent both directions — a no-op never touches disk.
- **Output:** `data: {name, change}` where `change` is `set` / `moved` /
  `cleared` / `no-op`; the human message names the prior holder on a move
  (`"hub moved from <old> to <new>"`).
- **Notes:** the hub is a last-resort address-resolution preference, not a
  trust or autogate grant — resolution reaches for it only when nothing
  else matches, never shadowing a real registered name. See
  [[Peer-Federation]].

### aoide peer pair / peer pending

```
aoide peer pair <target> [--name <n>] [--via ssh://[user@]host[:port]]
                  [--self-via ssh://[user@]host] [--json]
aoide peer pending
aoide peer pair approve [<id>] [--yes] [--code NNN-NNN]
aoide peer pair reject <id>
aoide peer pair watch [--popup] [--json]
```

- **Reads:** the parked pairing state `state/peer-pairing-inbound.json` /
  `state/peer-pairing-outbound.json` (`aoide_storage::pairing`,
  tolerate-missing, expired entries swept lazily on read) and this
  instance's ed25519 identity (`state/identity/`, minted lazily on first
  need — `ed25519.key` at 0600 inside the 0700 directory, never appearing
  in any output; `aoide identity` shows the pubkey and its colon-hex
  fingerprint). `peer pair <target>` takes exactly ONE positional — a
  second is refused outright, before either arm runs, naming the fold in
  the refusal, and the `approve`/`reject`/`watch` subcommand names match
  BEFORE a bare hostname positional of the same spelling (a box named
  `approve` pairs only by the explicit URL form). A URL-shaped target
  (`"://"`) POSTs `aoide/pairRequest` then `aoide/pairReveal` — two
  sequential POSTs in one invocation — to that instance's A2A door; a bare
  hostname first runs its own ~45s discovery sweep, resolving the name to
  the matching advertisement's OBSERVED source address on the house door
  port (`AOIDE_A2A_PORT` or 8710 — the wire carries no port to read), and
  an ambiguous or absent name is a taught error listing what WAS heard.
  Both arms drive the same `run_pair_request` core, and the hostname arm
  refuses to pair with this instance itself (heard name matching its own,
  or the datagram from loopback). `approve` on the
  inbound (approver) side is purely local: it commits its record and marks
  the parked entry approved, dialing nobody. `approve` on the outbound
  (requester) side first POLLS `aoide/pairPoll` — a signed POST over the
  SAME forward dial the request used — and completes only when the
  approver's parked entry answers approved. `watch` follows the daemon
  events feed (`$XDG_RUNTIME_DIR/aoide/events.jsonl`, same path
  `events tail` reads) for the `pair-parked`/`pair-revealed` lines
  (`class: "gate"`, `source: "a2a-door"`) and re-derives the actionable
  set from `list_inbound`/`list_outbound` on a 30s reconcile tick — the
  feed line is a trigger, the storage-backed list the authority. The
  actionable set is inbound-only: an outbound entry completes inside the
  operator's own polling `approve`, so watch never surfaces one.
- **Writes:** `peer pair` parks the outbound half; `approve` commits a
  `pubkey`/`verified` peer record into `state/peers.json`
  (`peer_store::upsert_paired_peer`, default `allows: ["read","spawn"]`),
  dispatching by direction — inbound queue first, then outbound — so the
  requester's own confirm is a SECOND `approve` against the outbound
  queue, the one that polls. When the parked request carries the
  requester's `selfVia` claim, the inbound commit also sets the peer's
  `url` to `http://127.0.0.1:<port>/` (port parsed off the requester's
  advertised url) and `via` to the claim in the same write
  ([[Peer-Transport]]); `reject` removes the parked entry
  locally on either queue, no wire call, no record.
- **Output:** `peer pair` prints the derived SAS (the `%03d-%03d`
  confirmation code, `aoide_storage::pairing::derive_sas`) plus the
  request's short id; `peer pending`
  lists both directions, each row carrying host, id, direction, and state —
  NEVER the code, which stays out-of-band (an unrevealed inbound entry
  shows `revealed: false`);
  `approve [<id>]` re-derives the SAS (the id optional when exactly one
  request is pending) and the gate differs by side — an INBOUND
  id asks the operator to TYPE the code as read off the requester's
  screen (the prompt never echoes the expected value; `--code NNN-NNN`
  scripted; a wrong code counts a persisted try, the third cumulative
  mismatch auto-denies, an entry at the try limit is denied on sight, and
  `--yes` never bypasses any of this), an OUTBOUND id polls and then
  prompts `y`/`N` (`--yes` scripted — this side's own screen printed the
  code at request time).
  `watch` blocks until Ctrl-C, narrating each recognized line; `--json`
  emits one event object per line instead, and `--popup` (opt-in, off by
  default — `aoide.a2a.pairingPopup`) raises a dialog shaped by direction —
  `lyra pair ask`/`lyra pair confirm` when `lyra` resolves, `zenity
  --entry`/`zenity --question` otherwise (refused up front only when
  neither resolves; `--popup`+`--json` is a usage error). An inbound
  request COLLECTS a typed code through the SAME gate the CLI's own
  `--code`/tty prompt use, and never shows the code in the dialog (typed
  blind, like the tty prompt); an outbound one SHOWS this instance's own
  locally-derived code and asks for a single Approve/Reject, never a
  retype — the code is already on screen, so retyping it would prove
  nothing an Approve click doesn't already prove. On a non-CLI door
  `watch` returns a "run it from a terminal" outcome, the `events tail`
  posture.
- **Notes:** not gated. The commit-then-reveal ceremony, the poll, the
  commit asymmetry, the park cap (`AOIDE_PAIRING_PARK_CAP`, default 32)
  and the 4-hour expiry (`DEFAULT_PAIRING_TIMEOUT_SECS`):
  [[Pairing-Ceremony]].

### aoide peer allow

```
aoide peer allow <name> <cap> on|off
```

- **Reads/Writes:** `state/peers.json` — flips one capability in the
  peer's closed `allows` set (`PEER_CAPABILITIES`: `"read"` / `"spawn"`).
- **Output:** idempotent — `on` an already-granted or `off` an
  already-revoked capability reports a no-op, never an error. An unknown
  capability string is refused before the peer lookup; an unknown peer
  name is refused after it — a distinct taught error for each.
- **Notes:** the only writer of `allows` besides the pairing ceremony's
  own `["read","spawn"]` stamp; re-pairing never re-runs it, so a revoked
  capability survives a key rotation. When several verified records share
  one pubkey (one remote instance paired under two names), the key's
  effective grants are the UNION across those records — revoking a
  capability from the key means revoking it on every record sharing it.
  The A2A door's Spawn arm reads the
  set ([[A2A-Door]], [[Pairing-Ceremony#What approval commits]]).

### aoide peer spawn

```
aoide peer spawn <name> [--yes] [--via ssh://[user@]host[:port]] -- <text…>
```

- **Reads:** `state/peers.json`; this instance's identity (the POST is
  signed — four headers: the claimed self name `X-Aoide-Peer`, timestamp,
  nonce, and the ed25519 signature over the canonical string). Refuses an
  unknown or unpaired (`verified:
  false`) name LOCALLY with a taught error naming `peer pair`.
- **Output:** POSTs a spawn-shaped `message/send` (no `contextId`) to the
  peer's A2A door; `<text…>` becomes the spawned session's first turn.
  What actually runs is the PEER's configured `aoide.a2a.spawnAgent`,
  never a remote-chosen executable. Every other refusal — `allows`
  lacking `spawn`, an unsigned-but-paired caller, clock skew — is the
  remote door's own call, surfaced verbatim.
- **Notes:** `--yes` skips only the local `y`/`N`; the remote gate
  (paired peer, signature rung, `allows` contains `"spawn"`) is the sole
  security authority. See [[Peer-Federation]].

### aoide peer discover

```
aoide peer discover [--secs N] [--json]
```

- **Reads:** one bounded on-demand sweep of the LAN advertisement wire —
  binds UDP 8711 and listens `N` seconds (default ~4). Every line heard
  is validated before display (size cap on the raw bytes, JSON parse,
  `v == 2`, name/host/user shape checks); a line failing any check is
  dropped and counted, never partially rendered. Survivors dedupe by
  (name, source address) into a bounded in-memory fold (64 entries; a
  flood past the cap is counted dropped).
- **Writes:** nothing — an advertisement never reaches
  `state/peers.json`; discovery is rendezvous, not authentication.
- **Output:** the heard table: name, claimed ssh hop `user@host`, the
  OBSERVED source address `srcAddr`, first/last heard, count. The claimed
  hop is whatever the advertiser typed; `srcAddr` is measured by the
  listening socket and is the address anything downstream dials.
- **Notes:** no resident listener exists anywhere — hearing is always a
  sweep. The receiving host's firewall must admit inbound UDP 8711 (the
  nix module opens it alongside `aoide.a2a.discoveryAdvertise`). See
  [[Peer-Transport]].

### aoide peer advertise

```
aoide peer advertise on|off
```

- **Writes:** `state/advertise.json` — idempotent, reports what changed;
  an absent file is OFF.
- **Notes:** the runtime switch for this instance's LAN advertisement.
  `a2a serve`'s advertise thread reads the file fresh every tick (~30 s,
  jittered), so a toggle lands within one cadence with no restart;
  `aoide.a2a.discoveryAdvertise` / `AOIDE_DISCOVERY_ADVERTISE` /
  `--discovery-advertise` force advertising on for the process's
  lifetime, OR'd with the switch — the nix-declarative path. Advertising
  is off by default; the advertisement itself is a broadcast rendezvous
  claim (`{v, name, host, user}` — never a credential, key, or URL). See
  [[Peer-Transport]].

### aoide peer list

```
aoide peer list [--json]
```

- **Reads:** `state/peers.json`, the pull caches, and the LAN — presence
  and sessions from `who`'s own live-probe-with-cache-fallback core (one
  bounded ~2 s probe per peer, in parallel) plus ONE bounded ~2 s
  discovery sweep run concurrently with the probes
  (`aoide-conduct::graph::peer_list`; lives in `aoide-conduct` because the
  roster folds `who`'s probe core, which `aoide-client` cannot depend on).
- **Writes:** nothing — not `state/peers.json`, not `state/peer-cache/`.
- **Output:** the one-glance mesh roster — one row per known node (this
  host first, then every registered peer, then every advertising instance
  heard), each node's running sessions (agent, state, petname/short-id)
  indented beneath it; an offline peer's last-known sessions render
  labeled `as of <fetchedAt>`. Marks: `●` paired/local and online, `○`
  paired but offline (`last seen <fetchedAt>` or `never pulled`), `◆`
  advertising — appended to a paired row (`●◆`/`○◆`) when a sweep hears
  its name, standing alone for an unpaired pair-candidate row showing the
  observed source address. `--json` emits `nodes[]` (`{mark, name,
  isLocal, paired, verified, advertising, presence, addr, lastSeen,
  sessions[]}`) plus `sweep` (`{heard, dropped}` or `{error}` — an empty
  or unlistenable sweep annotates the roster, never fails it).
- **Notes:** the roster, not the registry — `peer status` keeps the deep
  per-peer detail. See [[Peer-Federation]].

## Related

- [[A2A-Door]] — the inbound A2A contract (CONTRACTS.md §6)
- [[Peer-Federation]] — same-network aoide-to-aoide federation (§7)
- [[Pairing-Ceremony]] — the commit-then-reveal handshake the `peer pair`
  group drives
- [[Agent-Interface]] — the one-schema, two-doors MCP contract
- [[aoided]] — the daemon entity and policy surface
- [[shellbridge]] — the stage-state bridge concept
- [[Melete]] — the adapter's consumer side
- [[aoide-cli]] — the CLI trunk (`aoide schema --json` is the backstop for
  every signature on this page)
- [[Governance]] — the single audit log, the gate, the doors
