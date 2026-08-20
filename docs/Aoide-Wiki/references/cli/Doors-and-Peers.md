# Doors & Federation Verbs — MCP, A2A, Peers, Daemon

This group covers the doors onto the one command schema and the federation
surface: the stdio [[Agent-Interface|MCP]] façade (`mcp serve`), the [[aoided]]
policy skeleton (`daemon`, `adapter melete`), the desktop state bridge
(`shellbridge`), the interactive session UI (`conductor`), the [[A2A-Door]]
server (`a2a serve`) and its outbound client verbs (`a2a agent *`), and
[[Peer-Federation]] (`peer *`). Handlers are spread across
`pkgs/aoide/crates/server/src/{commands,mcp,daemon,a2a}.rs` (the server
domain), `pkgs/aoide/crates/client/src/{commands,adapter,peer,wire}.rs`
(the outbound half), `pkgs/aoide/crates/conductor/src/` (the TUI), and
`pkgs/aoide/crates/conduct/src/shellbridge.rs`; `mcp serve`'s registration
stays in `pkgs/aoide/crates/cli/src/commands/infra.rs`. The long-running
launches (`mcp serve --stdio`, `a2a serve`, `conductor`) are special-cased in
`pkgs/aoide/crates/cli/src/lib.rs::run_cli`.

Shared state paths (`pkgs/aoide/crates/storage/src/fs.rs`): `state/` is
`$AOIDE_STATE_DIR` when set to an absolute path, else `~/Aoide/state/`;
`song/stage/` is `$AOIDE_STAGE_DIR` when absolute, else `~/Aoide/song/stage/`.
The audit log (`pkgs/aoide/crates/protocol/src/audit.rs::default_audit_log`)
is `$AOIDE_AUDIT_LOG`, else `/home/$AOIDE_USER/Aoide/log`, else
`$HOME/Aoide/log`. Every one-shot command appends one NDJSON audit record
through the single dispatcher (`cli/src/dispatch.rs`); the doors add their own
records on top. All registry writes in this group are atomic temp-then-rename
(`aoide_storage::fs::atomic_write`). Registry reads tolerate a missing or
corrupt file as an empty list.

Every command takes `--json`. Without it the CLI prints the human `message`
line; with it, an envelope `{status, command, message, gated, changed?, data?}`
(`pkgs/aoide/crates/protocol/src/output.rs`). Exit codes: 0 ok, 1 error,
2 usage, 64 not-implemented (no command on this page is a stub). Outbound HTTP
in the `a2a agent`/`peer` verbs is `curl -sS --max-time 15` shelled out with
the URL/body in argv or on stdin (no local credential is involved).

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
  `--audit-log` → `$AOIDE_AUDIT_LOG` → `/home/$AOIDE_USER/Aoide/log` →
  `$HOME/Aoide/log`.
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
  full event loop is future work. The standalone `aoided` binary
  (`pkgs/aoide/crates/cli/src/bin/aoided.rs`, takes `--audit-log <path>`,
  prints the status JSON to stdout) is the same code path — what a systemd
  unit launches. See [[Governance]].

### aoide shellbridge

```
aoide shellbridge [--run] [--json]
```

- **Reads:** env `$XDG_RUNTIME_DIR` (socket parent; falls back to
  `/run/user/1000`), `$AOIDE_DEFAULT_SONG` (the rice-mode toggle's
  declarative-direction song, baked in by `modules/nucleus/shellbridge.nix`);
  `song/stage/mode.json` (the toggle's current mode). Newline-delimited JSON
  commands on its socket (below).
- **Writes:** seeds `song/stage/sessions.json` and `song/stage/hooks.json`
  with their empty v0 registry shapes, atomic and only when absent or corrupt
  (`seed_if_absent` — a populated roster survives a restart); binds
  `$XDG_RUNTIME_DIR/aoide/shellbridge.sock` (a stale socket file is removed
  first); maintains `song/stage/herald.json` (notification ledger, v0,
  capped at 20 cards, atomic read-modify-write on the accept loop);
  appends audit records (door `daemon`) to the default audit log for every
  dispatched command.
- **Output:** blocks forever serving the socket (systemd unit is
  `Type=simple`); returns an error document only on bind failure
  (`state: "error"`, data `{socket, stageDir, wrote}`). Socket verbs
  (`pkgs/aoide/crates/conduct/src/shellbridge.rs`): `focuswindow` /
  `focussession` (Hyprland jump via `crate::graph`), `power`
  (`lock|logout|suspend|hibernate|reboot|shutdown` → spawns `hyprlock`,
  `hyprctl dispatch exit`, or `systemctl suspend|hibernate|reboot|poweroff`,
  detached with a reaper thread), `ricemode` (re-execs the same binary as
  `rice mode <stage|declarative> [song] --json`, waits, then fires a detached
  `notify-send "Aoide" <message>`), `refreshusage` (detached re-exec
  `usage --json`; the gadget picks up the `state/usage.json` write itself),
  `rechecksessions` (detached re-exec `graph reap --announce --json`),
  `heraldpush` / `heraldverdict` / `heralddismiss` (the ledger; a verdict
  types the answer into the waiting session through `graph send`). Also
  spawns the Hyprland window→session listener thread at startup
  (`graph::run_hypr_window_listener`). Malformed/unknown lines are logged to
  stderr and skipped — nothing kills the accept loop.
- **Notes:** not gated. The `--run` flag is registered in the schema but the
  handler (`server/src/commands.rs::handle_shellbridge`) never consults it —
  bare `aoide shellbridge` runs the blocking loop either way. Client half:
  `shellbridge::send_line` is how `aoide herald push` and `graph permit` reach
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

- **Reads:** `song/stage/{projects,sessions,hooks}.json` (mtime-polled every
  ~500 ms), `song/stage/livery.json` (palette → ANSI-256 theme), and the audit
  log (the LOG panel tails it). Env: `$AOIDE_STAGE_DIR`, `$AOIDE_AUDIT_LOG` —
  both are honoured, so a tempdir is a full offline test rig
  (`pkgs/aoide/crates/conductor/src/lib.rs` documents the seed script).
- **Output:** `run_cli` dispatches the launch first (an audit record), then
  hands the tty to a ratatui/crossterm loop: alternate screen + raw mode, five
  panels (DAG, SESSIONS, PROJECTS, LOG, STATUS), keys `1`–`5`/`Tab`/`BackTab`
  to switch, `j`/`k` select, `?` help, `q`/`Ctrl-C` quit. Terminal state is
  restored on every exit path including panic (Drop guard + panic hook).
- **Notes:** not gated. The conductor is a frontend only: every action goes
  through the same dispatcher as `Door::Cli`, so conductor actions are
  audited exactly like typed commands. On a non-CLI door (e.g. an MCP
  `tools/call` for `conductor`) it returns a "run from a terminal" outcome
  instead of blocking that door. Distinct from `aoide conduct`, which wraps
  one process into the conductor channel.

### aoide a2a serve

```
aoide a2a serve [--bind <addr>] [--port <n>] [--spawn-agent <cmd>]
                [--peer-name <name>] [--token-file <path>] [--json]
```

- **Reads:** flag → env → default resolution, once at launch: bind
  (`--bind` → `$AOIDE_A2A_BIND` → `127.0.0.1`), port (`--port` →
  `$AOIDE_A2A_PORT` → `8710`), spawn agent (`--spawn-agent` →
  `$AOIDE_A2A_SPAWN_AGENT` → empty = spawning disabled), instance name
  (`--peer-name` → `$AOIDE_A2A_PEER_NAME` → OS hostname → `"aoide"`), token
  (`--token-file` → `$AOIDE_A2A_TOKEN_FILE` → empty = no token required; the
  secret is read off the file once at launch, trimmed, never logged). Per
  request: `song/stage/sessions.json` (task/session state),
  `state/peers.json` (autogate match — by caller address, or by an
  autogate-marked peer's own `tokenFile`, read fresh off disk per request).
  The `aoide-a2a` systemd unit (`modules/nucleus/aoided.nix`) sets
  `AOIDE_A2A_BIND`/`PORT`/`SPAWN_AGENT`/`TOKEN_FILE` from the nix options;
  `AOIDE_A2A_PEER_NAME` is currently flag/env-only.
- **Writes:** an audit record (door `a2a`) for every handled request, spawn,
  and SSE open/close in the audit log; the `message/send` inject path reuses
  `graph send`'s `session_send`, so a held send writes the session's
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
    `autogate` peer. Once a token is configured, loopback stops being a trust
    signal (unauthenticated → never auto-delivers) and Spawn requires a valid
    `Authorization: Bearer <token>` (`-32005` otherwise). Spawn unconfigured →
    `-32004`; session not conductable → `-32004`; unknown contextId →
    `-32001`. A held-pending send returns a `submitted` Task immediately.
  - `aoide/graphSummary` — `{schemaVersion: "0", instance: {name, url,
    emittedAt}, graph}` wrapping the same resolved graph document
    `graph view`/`graph emit` build (CONTRACTS.md §7).
  - `message/stream` / `tasks/resubscribe` — the socket stays open as SSE
    (`text/event-stream`, one `data: <json>` frame per status change, final
    frame a `TaskStatusUpdateEvent` with `final: true`); stream lifetime
    capped at 600 s, status polled every 750 ms.
  - Hardening caps: body ≤ 1 MiB, header line ≤ 8 KiB, ≤ 100 headers, 15 s
    absolute per-request budget, 10 s read timeout, 64 in-flight connections
    (past that a fast 503). Unknown method → `-32601`, parse error → `-32700`.
- **Notes:** not gated at the CLI level; the security model is bind-address +
  the rebuild-gated `aoide.a2a.spawnAgent` option + the optional bearer token.
  A client never supplies a command — the spawn path only ever launches the
  operator-configured executable. Localhost, user-only, off by default. See
  [[A2A-Door]].

### aoide a2a agent add

```
aoide a2a agent add <url> [--json]
```

- **Reads:** `state/a2a-agents.json` (tolerate-missing → empty); fetches the
  remote AgentCard over HTTP via curl — a bare origin has
  `/.well-known/agent-card.json` appended (`client/src/wire.rs::resolve_card_url`).
- **Writes:** upserts the parsed card into `state/a2a-agents.json` (v0,
  atomic write; keyed by the card `name`, re-add replaces in place). The
  stored `url` is the resolved `message/send` endpoint, not the card URL.
  `changed: ["state/a2a-agents.json"]`.
- **Output:** ok → `"registered|updated A2A agent \`<name>\` → <url> (<n>
  total)"`, data `{agent, count, replaced}`. Failures: `reason:
  fetch-failed | fetch-http-error | card-unparseable | card-invalid |
  registry-write-failed`, exit 1. Missing arg → usage, exit 2.
- **Notes:** registered agents fold into the session DAG as `kind:"a2a"`
  nodes (`graph/doc.rs::build_graph`).

### aoide a2a agent list

```
aoide a2a agent list [--json]
```

- **Reads:** `state/a2a-agents.json` (absent → empty, never an error).
- **Output:** human lines `<name> · <url>[ · <description>]`; data
  `{agents, count}`.
- **Notes:** read-only.

### aoide a2a agent remove

```
aoide a2a agent remove <name> [--json]
```

- **Reads:** `state/a2a-agents.json`.
- **Writes:** the registry, atomic, only when the name existed (`changed:
  ["state/a2a-agents.json"]`).
- **Output:** data `{removed, name, count}`. A missing name is ok with
  `removed: false` — idempotent, not an error (contrast `peer remove`).
- **Notes:** not gated.

### aoide a2a agent send

```
aoide a2a agent send <name> <message> [--json]
```

- **Reads:** `state/a2a-agents.json` (unknown name → exit 1, `reason:
  unknown-agent`).
- **Writes:** none locally.
- **Output:** POSTs a JSON-RPC `message/send` (fresh `messageId`
  `aoide-<pid>-<nanos>`) to the agent's stored endpoint via curl. Ok →
  `"sent to \`<name>\` … — task <id> [<state>]"`, data `{name, url,
  messageId, response}`. Non-200 → `send-http-error`; a JSON-RPC error body on
  HTTP 200 → `agent-error`; unreachable/timeout → `send-failed` (exit 1
  throughout).
- **Notes:** `aoide screen send --agent` calls this handler in-process with a
  synthesized invocation — same driver, never a second transport.

### aoide peer add

```
aoide peer add <name> <url> [--autogate] [--token-file <path>] [--json]
```

- **Reads:** `state/peers.json`; verifies the peer by fetching its AgentCard
  (`curl` GET on the resolved card URL) before registering anything — a peer
  that fails the fetch is never added. Name must match
  `^[a-z0-9][a-z0-9-]*$` (it is joined into `state/peer-cache/<name>.json`;
  the shape check is the path-traversal guard) — else exit 1, `reason:
  invalid-name`.
- **Writes:** inserts into `state/peers.json` (v0, atomic write). A duplicate
  name is rejected cleanly (`reason: duplicate-name`, exit 1) — unlike
  `a2a agent add`'s replace-on-re-add, a nickname is never silently repointed.
- **Output:** `"registered peer \`<name>\` → <url>[ (autogate)] (<n> total)"`,
  data `{peer: {name, url, autogate, tokenFile?, addedAt}, count}`.
- **Notes:** `--autogate` marks the peer trusted: its inbound `message/send`
  on this instance's A2A door auto-delivers instead of queueing pending,
  matched by caller address or by the per-peer `--token-file` secret (the form
  that survives a reverse proxy/tunnel, where every caller's address is the
  proxy's). See [[Peer-Federation]].

### aoide peer list

```
aoide peer list [--json]
```

- **Reads:** `state/peers.json` (absent → empty).
- **Output:** human lines `<name> · <url>[ · autogate]`; data `{peers,
  count}`.
- **Notes:** read-only.

### aoide peer remove

```
aoide peer remove <name> [--json]
```

- **Reads:** `state/peers.json`; name shape validated as in `peer add`.
- **Writes:** the registry (atomic); also deletes the peer's cache file
  `state/peer-cache/<name>.json` if present (best-effort), so a peer re-added
  under the same name never starts from a stale leftover.
- **Output:** data `{removed: true, name, count}`. A missing name is an
  error (`reason: unknown-peer`, exit 1) — deliberately NOT idempotent-silent,
  the documented divergence from `a2a agent remove` (CONTRACTS.md §7).
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
- **Output:** `"<n> peer(s) registered"`, data `{peers: [{name, url,
  autogate, state, fetchedAt, error}]}` where `state` is `fresh` (pulled
  within the 300 s TTL and not marked stale), `stale`, or `never-pulled` — the
  same three-way classification the graph fold uses, so this and the DAG never
  disagree.
- **Notes:** read-only. TTL constant: `PEER_CACHE_TTL_SECS` in
  `pkgs/aoide/crates/storage/src/peer_store.rs`.

## Related

- [[A2A-Door]] — the inbound A2A contract (CONTRACTS.md §6)
- [[Peer-Federation]] — same-network aoide-to-aoide federation (§7)
- [[Agent-Interface]] — the one-schema, two-doors MCP contract
- [[aoided]] — the daemon entity and policy surface
- [[shellbridge]] — the stage-state bridge concept
- [[Melete]] — the adapter's consumer side
- [[aoide-cli]] — the CLI trunk (`aoide schema --json` is the backstop for
  every signature on this page)
- [[Governance]] — the single audit log, the gate, the doors
