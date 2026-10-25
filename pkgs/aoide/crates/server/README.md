# aoide-server

`aoided` and the door serve-loops: A2A JSON-RPC/HTTP/SSE, MCP-over-stdio,
listeners, sessions, snapshots, the audit sink. Untrusted input stops here —
the inbound half of the two-door contract (the outbound half is
`aoide-client`).

The A2A door (`a2a.rs`) never approves a gated send, and it never delivers
one. Every inbound inject the door admits is held in `pending.json` for a
human, whoever the caller is. Admission is decided by identity, never by
address: with a door-wide token configured, a caller that presents neither that
token nor a node identity (a verified node's own bearer, or a verified
signature holding `message` in its mesh) gets the uniform answer and nothing
queues. The queue holds a bounded number of entries per source: past it, a
proven node is refused `-32000` and any other caller gets the uniform answer. A
prompt over the per-entry byte cap is refused `-32602` for every caller, since
its size is the caller's own text. A spawn's opening turn is held the same way, verdict `held-pending`, or
`queue-full` at the cap. The gate that decides this is `aoide_conduct::graph::
Door::can_approve`; CONTRACTS.md's pending-queue section has the table.

The MCP door (`mcp.rs`) never approves either, but it is attested like the CLI.
`serve_stdio` dispatches in its own process, so the sealed session found by
walking that process's ancestry is the caller's identity: a send from a server
launched under a sealed session earns that session's parent and sibling rules
and delivers, and every other MCP send parks. A server with no sealed ancestor
(the tier-3 network façade in `modules/dendrites/aoided.nix`, a bare terminal)
parks them all. Attestation sees who launched the server, not who writes to its
stdin, so a network bridge someone runs inside a conducted terminal lends that
terminal's rights to its remote callers.

## Named seams (what it exposes)

- The resident daemon also serves the core shellbridge: `run_loop` spawns
  `aoide_conduct::shellbridge::run_core` once its own socket is bound, so
  `aoide/shellbridge.sock` (the core arms, not gated on strillix) starts with aoided.
- `daemon` — the `aoided` daemon. `run` is the one-shot policy self-check
  `aoide daemon` still runs. `run_loop`/`serve_daemon` (P-D2,
  `docs/architecture/AOIDED.md`) are the RESIDENT daemon `bin/aoided.rs`
  execs: lock `state/` owner-only first (`tighten_state_tree`, refusing to
  start when it cannot; CONTRACTS.md §4), then take the socket lock
  `<socket>.lock` (`daemon::acquire_daemon_lock`) and the state lock
  `aoided.lock` in the state dir (`daemon::acquire_state_lock`), both
  non-blocking `flock`s held for the process's life, so a held lock
  refuses the start before anything is bound. Then bind the control socket
  (`$AOIDE_DAEMON_SOCKET`, else `$XDG_RUNTIME_DIR/aoide/aoided.sock`,
  `daemon::socket_path`), spawn a thread-per-connection accept loop
  (fallible `Builder::spawn`, accept-error backoff — the secrets broker's
  own accept-loop discipline, reused by convention), then tick forever
  (~1s; P-D3's two producers, below). Newline-delimited JSON, the secrets
  wire framing verbatim, request lines read via `read_capped_line`'s
  `fill_buf`/`consume` loop (P-D4 — the byte cap is checked on every buffer
  fill, not only after a `\n` arrives, closing the P-D2-flagged gap where a
  newline-less stream could grow one connection's buffer unbounded). Three
  ops: `ping` (liveness), `subscribe` (follow the daemon's own events feed
  — `$AOIDE_DAEMON_EVENTS`, else a sibling of the socket,
  `daemon::events_path` — filtered by an explicit `classes` array,
  default-deny), and `dispatch` (P-D4, the fourth door) — builds an
  `Invocation { path, args, flags, door: Door::Daemon }` LITERALLY from the
  wire's `path`/`args`/`flags` and runs it through the injected `dispatch`
  fn, replying with one `{"outcome": <the full Outcome envelope>}` line.
  Door policy is not reimplemented here: every command's own `inv.door`
  branch (a CLI-only admin command's refusal, a gated command's `gated: true`,
  `mcp.serve`/`a2a.serve`'s non-Cli metadata reply) runs exactly as it
  already does over MCP/A2A, since the injected fn IS
  `cli::dispatch::dispatch` — no daemon-specific allowlist exists or is
  planned (`docs/architecture/AOIDED.md`'s "L2" section). `registry` still
  rides along the DI seam unused — no op resolves a dotted tool name
  against it the way MCP's `tools/call` does.
- `producers` (P-D3, `docs/architecture/AOIDED.md`'s "L1" section) — the
  daemon tick's two producers, both constructed once at `run_loop` startup
  and ticked every iteration. `SecretsMirror` tails the secrets broker's
  OWN events feed, its location resolved via `aoide_secrets::socket::
  socket_path`/`events_path` (`daemon::run_loop`'s own construction site —
  plain, wire-type-free `PathBuf` resolvers, reused rather than re-derived;
  this crate's `aoide-secrets` dependency also covers the A2A door's
  inbound bearer resolve). The mirror IS deliberately kept off that
  crate's wire/record TYPES for parsing, so "never copy an unknown field"
  is structural — `mirror_secrets_line` reads a bare `serde_json::Value`
  and copies exactly four named fields, `id`/`secret`/`consumer`/
  `timeoutSecs`, for one of the five recognized outcomes,
  `released`/`parked`/`completed`/`dismissed`/`expired`) and re-publishes
  each as a `class:"secret",source:"secrets-mirror"` record on the
  daemon's own feed. `HandEditWatcher` stat-sweeps a fixed six-file roster
  (`daemon::stage_roster`: `sessions.json`/`hooks.json`/`projects.json`/
  `graph.json` from `aoide-storage`, `pending.json`/`notifications.json` from
  `aoide-conduct`) each tick and fires a `class:"audit",kind:"hand-edit"`
  event for any file whose `(mtime, len)` no longer matches its baseline
  (the #69 hand-edit watcher) — detection and narration only, this daemon
  never reverts a hand edit. `note_own_write` is the seam P-D6's own
  tick-reconcile/reap writes (below) fold into so they are never reported
  back as a hand edit. **The watcher itself is a single `Arc<Mutex<
  HandEditWatcher>>` (task #92), shared between the tick thread and
  `handle_conn`'s connection threads** — not tick-private: a dispatched
  session command (`session start/end`/etc., `{"op":"dispatch"}`) writes
  stage files on ITS OWN connection thread, not the tick's, so
  `rebaseline_stage_roster` re-baselines the WHOLE roster after every
  completed dispatch (roster-wide, not a per-command "which files did this
  write" table) before that connection's reply goes out — closing the
  window where the daemon's own routed write got reported back to itself as
  a hand edit one tick later.
- **Graph residency (P-D6, `docs/architecture/AOIDED.md`'s "L4")** —
  `run_loop`'s tick, after narrating the hand-edit sweep above, does two
  more things every iteration: `reconcile_graph_projection(changed_files)`
  re-derives `state/stage/graph.json` (via `aoide_conduct::graph::restage_graph`,
  the same function every project/session mutation site already calls — the
  `graph emit` CLI command was retired in favor of the resync now spelled `session prune` — no forked
  logic) whenever `sessions.json`/`hooks.json` is among the
  files the sweep just reported changed, so an out-of-band write (the
  direct-fallback CLI path, or a hand edit) is folded into the projection
  on the very next tick rather than waiting for the next dispatch to touch
  it; `run_internal_reap` calls the SAME `aoide_conduct::reap::
  reap_and_announce` `session reap` always runs, every `REAP_EVERY_TICKS`
  (12) ticks (~12s, matching the systemd timer's own cadence), re-baselining
  `HandEditWatcher` via `note_own_write` for whatever it touched so its own
  sweep is never mistaken for a hand edit next tick. Neither producer keeps
  a separate in-memory roster — every dispatch (routed or internal) reads
  the stage files fresh, so "fold in the newest write" falls directly out
  of "the file on disk is the single source of truth at every instant";
  `daemon::handle_conn`'s `dispatch` op is also how a REMOTE `graph
  session start/phase/end/hook`/`session reap` call actually executes once
  routed here — `internal_invocation` builds the same shape of
  `Invocation { door: Door::Daemon, .. }` for the tick's own internal
  calls, so the tick's writes and a routed client's writes go through
  literally the same code.
- **Boot-time auto-resume trigger (P-D8, `docs/architecture/AOIDED.md`'s
  "L5"/"Open knobs")** — `daemon::run_boot_auto_resume`, called exactly
  ONCE at `run_loop`'s entry, before the tick loop starts (never from
  inside it). Boot-epoch guarded: `epoch_already_fired` (a pure predicate,
  unit-testable with no `/proc/stat` involved) compares a one-line marker
  file under `state_dir` (`daemon::auto_resume_marker_path`) against
  `aoide_conduct::reap::boot_epoch()` — reused directly rather than
  re-derived, that function's own doc names this exact caller — so a
  `Restart=on-failure` restart within the SAME boot is a no-op, and only a
  real reboot (a changed epoch) reopens the guard. On a fresh boot, for
  EVERY `autoResume` project (`projects.json`, P-D8) — unconditionally, no
  liveness check here — calls `aoide_conduct::graph::session_resurrect`
  in-process (`Door::Daemon`) — the identical command core `resurrect
  --project` runs over the CLI, the same in-process-call pattern
  `run_internal_reap` already uses for `session reap`. Liveness lives one
  layer down: `session_resurrect`'s own bare-mode selection
  (`undying_selection`, durable-sessions plan P-C4) drops any undying id
  already alive in `sessions.json` per candidate before spawning anything,
  so a project where every undying session is already live resolves to the
  empty-set `Outcome::ok` no-op rather than being skipped wholesale — a
  per-project skip here would have suppressed reviving a multi-session
  undying set's other, actually-dead members over one live terminal. That
  function never hard-errors on a per-candidate spawn failure either; a
  headless host's taught "no `$AOIDE_TERMINAL`" error is only
  `eprintln!`'d here, never propagated — the tick/loop itself is never at
  risk.
- **The sealed session credential (LANE IDENTITY P-ID1, `docs/architecture/
  CONTRACTS.md` §4's `seal` field) — scaffolding, no gate consumes it
  yet.** `daemon::seal_keypair` mints this daemon PROCESS's own ed25519
  keypair exactly once (`aoide_storage::identity::mint_ephemeral`, held in
  a module-level `OnceLock`) and NEVER writes it to disk — under OQ1-A
  (the plan file's User-answered threat-model question) the seal's
  secrecy rests on process liveness plus Yama `ptrace_scope`, not file
  permissions, since a same-uid attacker can read any file the operator
  owns, including `state/identity/`'s own on-disk node-wire key.
  `daemon::mint_seal(session_id, pid, origin_class)` builds a
  `SealedIdentity` (reading `pid_starttime` via `aoide_conduct::
  graph::pid_starttime`, no second `/proc` parse) and signs it under that
  key (`aoide_storage::sealed_id::mint_seal`), and returns the starttime it
  signed over as well. `handle_conn`'s `dispatch` op calls
  `daemon::seal_freshly_registered_session` right after a successful
  `session start` dispatch: if the just-written record already carries a
  `pid` (today, only `session_conduct`'s own direct registration does — the
  bare wire path does not), it mints a seal and stamps it, with that
  starttime as `sealedPidStarttime`, via `aoide_conduct::graph::stamp_seal`.
  The start door (`aoide_conduct::graph::session_start`) decides a start on a
  protected wrap, and it never reads the signature. **This
  is honest scaffolding, not a security boundary**: the pid sealed over is
  whatever the record already carries, not yet a peercred-verified
  CONNECTING pid. See CONTRACTS.md §4's `seal` paragraph and `aoide-storage`'s
  own README for the full mechanism.
- **The dispatch socket's own accept gets a cross-uid floor, and two
  attribution leaks close (LANE IDENTITY P-ID3).** `daemon::accept_loop`
  reads `aoide_secrets::peercred::peer_cred` on every accepted connection
  and refuses one whose peer uid doesn't match this daemon's own euid,
  fail-closed on an unidentified peer — the same `admin_gate` shape the
  secrets broker already holds, restated here for this THIRD socket
  (`daemon::cross_uid_gate`, `shellbridge::cross_uid_gate` in
  `aoide-conduct`). Cross-uid only: every legitimate connector already
  shares the daemon's own uid under OQ1-A. Separately,
  `invocation_from_dispatch_request` stamps an absent `from` flag
  explicit-empty so a `send` handler running INSIDE this process (a
  dispatched request runs its handler on the daemon's own thread) never
  falls back to reading the DAEMON's own ambient `AOIDE_SESSION_ID` as if
  it were the connecting client's attribution (G8); `a2a::do_inject` does
  the identical stamp for a remote inject, so it never picks up `aoide a2a
  serve`'s own ambient env either (G9). The hook door's half of the same
  class is the OTHER direction — this daemon STAMPS the fact rather than
  emptying it: `invocation_from_dispatch_request` writes its own
  `SO_PEERCRED` peer pid under `aoide_conduct::graph::DAEMON_PEER_PID_FLAG`
  unconditionally (a wire value for that key is discarded), so a
  daemon-served `session hook` checks the caller's parent claim against the
  connection's real pid instead of the daemon's env or the caller's word.
  Neither closes the GATE itself
  (`aoide_conduct::graph::send::real_attested_sender`, out of this phase's
  scope fence) — see `aoide-conduct`'s own README/AGENTS for the honest
  accounting of what that leaves open.
- `events` — `tail`, the blocking loop behind `aoide events tail` (P-D3):
  follows the daemon's own events feed with a `Follower` and prints every
  line whose `class` passes an (optional, comma-separated) filter, `--json`
  verbatim or narrated otherwise. `poll_once` is the bounded, non-blocking
  core a test drives directly; `tail` is the thin `SIGINT`-handling wrapper
  around it, mirroring `aoide_secrets::watch`'s own tail-loop shape.
- `mcp` — `serve_stdio`, the MCP stdio server, which dispatches in-process:
  the process attestation walks is this one. `initialize` answers
  unconditionally with `capabilities.experimental["claude/channel"]` and
  `instructions` (P-M5c-2, `docs/architecture/CLAUDE-CHANNEL-PROOF.md`,
  CONTRACTS.md §3's "MCP door" subsection). When `AOIDE_SESSION_ID` is set
  and non-empty, `serve_stdio` unlink-then-binds
  `aoide_conduct::graph::channel_socket_path(<that id>)`, spawns one
  listener thread for the lifetime of the MCP subprocess, and unlinks the
  socket on return — no record, no command, the socket's own presence is
  the whole registration (house rule 7). Each line received on that socket
  becomes one `notifications/claude/channel` push, `{"content": <the
  line>, "meta": {"mailbox": <name>}}` when the line names a mailbox
  (`{}` otherwise — meta keys stay bare identifiers, never hyphenated).
  Stdout is one `Arc<Mutex<_>>` writer shared between the request loop and
  the listener thread, so a pushed notification and a `tools/call` reply
  can never interleave on the wire.
- `a2a` — the serve half of A2A (JSON-RPC/HTTP/SSE); the client half stays in
  `aoide-client`. Both `message/send` arms park what they are given and deliver
  nothing, so neither files into the mailbase (messaging plan P-M1,
  `state/mail/boxes/`). `do_inject` (Inject, an EXISTING session) reuses
  `aoide_conduct::graph::session_send` with `Door::A2a`, which parks the send in
  `pending.json` under the source the door derived (`inject_source`:
  `node:<name>` for a caller that proved the node, `addr:<address>` for every
  other). Nothing reaches the session's socket,
  so no receipt is filed. `do_spawn` (Spawn, a BRAND-NEW session) queues its
  opening turn the same way: `opening_turn_worker` stamps `pending`, then
  `hold_pending` queues the prompt attributed `node:<name>` and the verdict is
  `held-pending` (`queue-full` when that node already holds the queue's
  per-source cap); an EMPTY opening turn records `skipped-empty` and queues
  nothing. The verdict lands on the record (`stamp_opening_turn`: one verdict
  from the list in `CONTRACTS.md`, or `unknown` when a boot pass reconciles a
  verdict lost with the process that owed it), and `tasks/get` reports it as the
  task's `status.message` — an A2A `Message` object, never a bare string — so a
  peer whose opening turn never ran is told so instead of reading a bare
  `submitted`. `aoide node spawn` prints that same line, so the operator does
  not have to reach for `tasks/get` to see it. The worker pool is bounded
  (`OPENING_TURN_WORKERS_MAX`, 8 waits in flight, the handler's own slot being
  released too early to be a bound): past it the opening turn is stamped `busy`,
  never silently dropped. The one mailbase-filing path that touches this crate
  is `mail_deposit`'s own call into `aoide_storage::mail::deposit` (below),
  which files a letter or receipt arriving over the wire from a peer node, so it
  shares no code path with the two A2A send arms. **`do_spawn`'s bounded
  liveness check (task #103)** gives the just-launched wrapper process (`aoide
  conduct`) a short window (400ms) to prove it's still alive via
  `Child::try_wait()` before acking `submitted` — a wrapper whose own exec of
  the configured agent fails (a missing `spawnAgent` binary on this unit's PATH)
  exits inside that window and gets a taught JSON-RPC error instead, closing the
  gap where a caller was handed a session id for a spawn that had already
  failed. **The spawned child's environment and cwd are sanitized, never trusted
  from the daemon's own ambient env.** `spawn_child_command` explicitly
  `.env_remove()`s both `AOIDE_SESSION_ORIGIN` (P-ID0/G16/G5 — an
  unauthenticated origin claim) and `AOIDE_SESSION_ID`: the `aoide-a2a` unit's
  own environment can carry the OPERATOR's live terminal session id, inherited
  from whatever shell the unit itself descends from, and without this removal a
  freshly-spawned child would adopt it as its `parentSessionId` via
  `window.rs`'s ambient-parent fallback — a spawned agent parented under an
  unrelated human terminal. A real `aoide conduct` launched from an agent's own
  shell is untouched by this: it inherits whatever ITS OWN wrap exported, the
  ordinary local-inheritance path; only this door's spawn clears the ambient
  value first. The child's working directory follows the same distrust:
  `resolve_spawn_cwd` reads `--spawn-cwd` then `AOIDE_A2A_SPAWN_CWD` (mirroring
  `resolve_spawn_agent`'s own precedence, resolved once at `a2a serve` launch),
  but `do_spawn` only ever calls `current_dir` on a value
  `resolve_bounded_spawn_cwd` has matched byte-identically against a REGISTERED
  project root (`Project::roots()`, loaded off
  `aoide_storage::stage::projects_path()` the same way session state is loaded
  off its own stage file) that still exists as a directory on disk. Anything
  else — unregistered, relative, a root that no longer exists — is ignored, the
  spawn inherits the daemon's own cwd exactly as before, and the reject is
  audited exactly once (`Door::A2a`, `EventClass::Audit`, `"a2a.message/send"`,
  `"skipped"`) so a misconfigured value degrades quietly instead of ever placing
  a spawn in an arbitrary directory. **Inbound bearer verification (task #84)**
  resolves the door's expected `Authorization: Bearer` token through
  `aoide-secrets`'s broker rather than only reading a static token file:
  `--bearer-secret <name>` (or `AOIDE_A2A_BEARER_SECRET`) names a secret,
  resolved FRESH on every connection via
  `aoide_secrets::client::resolve_bounded` as consumer `a2a-door`, with a short
  (~2s) timeout so a misconfigured `requireTotp` secret refuses immediately
  instead of parking the door open. Unset stays the pre-existing token-file
  behavior exactly; set takes precedence over `--token-file`. A
  broker-unreachable or denied resolve fails CLOSED — the connection is refused
  the same way a wrong bearer is, never held open and never treated as
  "unconfigured." A secret that resolves EMPTY (or only whitespace) counts as a
  resolve failure, and so does a configured `--token-file` that is missing or
  empty (`launch_token`): an empty expected token would read as "no token
  configured" and restore the loopback standing. The resolved value is never
  cached, logged, or placed in any audit line — see `CONTRACTS.md`'s "Secrets
  wire"/§6 sections for the wire contract and the resolve-consumer honesty note.
  The bearer gates UNSIGNED callers only (it strips the loopback standing a
  tunnel or proxy confers): the read arms go through `read_admitted`, which also
  admits a verified signed caller whose grant in the request's mesh holds
  `read`, so a paired peer's `node pull` works on a bearer-gated door. **The
  pairing ceremony's three methods (P-P2, Design A/task #119, CONTRACTS.md §6's
  "Pairing wire" subsection)** — `pair_request` (`aoide/pairRequest`),
  `pair_reveal` (`aoide/pairReveal`), and `pair_poll` (`aoide/pairPoll` —
  REPLACES the old `aoide/pairApprove` reverse callback) — join this same
  JSON-RPC dispatch table. `pair_request`/`pair_reveal` stay deliberately
  UNGATED by `read_ok`/bearer verification: the ceremony's whole point is
  establishing a credential where none exists yet, so gating either on one would
  be circular. `pair_poll` is self-authenticating instead (below) — neither
  door-gated nor fully open. None grants anything beyond a `pubkey`/`verified`
  node record, and that record commits only on BOTH ends' own separate human
  confirmation — the grant set is stamped by
  `aoide_storage::node_store::upsert_paired_node` itself, the moment a node
  first becomes verified (P-P3, PAIRING.md decision 5) — INTO THE MESH THAT
  PAIRING NAMES, and read per request through
  `aoide_server::a2a::grant_in_mesh(mesh, caller_key)` (P-CHARTER: a grant is
  given in one mesh and holds only there) — from the grant the CLI half resolved
  (`[pairing] defaultGrant` or `--allow`) — never by these methods directly, and
  never off the wire. `pair_request` validates every field (64-hex pubkey,
  64-hex commitment, a `valid_node_name` name, a non-empty `://`-bearing url)
  before calling `aoide_storage::pairing::park_inbound` — malformed input never
  reaches the parked-state file, and a park past the configured cap is refused
  with a distinct `-32000`. It also reads the OPTIONAL `selfVia` param (task
  #131 — the requester's own self-asserted reach-back hop claim, for the case
  where the requester's own door is loopback-only and this request is arriving
  over ITS tunnel, so nothing about the connection itself can answer "how do I
  dial the requester back") with no validation of its own: absent, wrong type,
  or empty all collapse to `None` alike, since the field is never load-bearing
  enough to refuse a pairing request over — only to enrich the eventual `aoide
  pair` commit when present. `pair_reveal` is the ceremony's third message: it
  checks a POSTed nonce against the parked entry's earlier commitment
  (`aoide_storage::pairing::reveal_inbound`) — a match stores the nonce so a SAS
  becomes derivable; a mismatch DROPS the parked entry outright and answers the
  distinct `-32002` (a bad reveal is exactly the shape a MITM's forced retry
  would take, so it is not treated as a recoverable hiccup — the reveal is
  unauthenticated, so a third party who obtains a live pending id can destroy
  that one ceremony attempt with a bogus reveal: an accepted
  denial-of-one-attempt, never an impersonation, and the operators simply re-run
  the ceremony). `pair_poll` is the REQUESTER's own follow-up, POSTed to the
  APPROVER's door over the SAME forward dial `pair_request`/`pair_reveal`
  already used — it carries a SELF-CONTAINED signature (`{id, timestampIso,
  nonceHex, signatureHex}`, verified inline against the parked entry's own
  `pubkey_hex` via `aoide_storage::wire_auth::verify_signature_hex`, never
  through P-P4's `verify_signed_request` — no `Node` record exists yet for that
  to key off) and NEVER writes a node record. It only READS
  `InboundPairingRequest::approved` (set PURELY LOCALLY, from
  `aoide-client::commands::approve_inbound`, never from a wire handler) and
  returns `{"status":"pending"}` uniformly for an unknown id, a wrongly-signed
  poll, or a genuinely-not-yet-approved one — never a distinct code that would
  let an outsider learn which case they hit (the existing-oracle discipline,
  mirroring `message/send`'s own `contextId` amendment above). Only a verified,
  approved poll gets `{"status":"approved","pubkeyHex":"<B's own pubkey>"}`. The
  requester's own node record commits later, entirely inside `aoide-client`,
  once that instance's own operator polls and confirms the SAS. All three audit
  via the existing `Door::A2a` audit sink (`a2a.pairRequest`/
  `a2a.pairReveal`/`a2a.pairPoll`), same as every other A2A method — `pair_poll`
  audits only its successful release, never a routine "still pending" poll.
  **The pairing events feed (P-P5, CONTRACTS.md §6's "Pairing events feed"
  subsection)** — `emit_pairing_event`, called from the Ok arms of
  `pair_request` (`pair-parked`) and `pair_reveal` (`pair-revealed`) only;
  `pair_poll` never calls it (Design A retired the third kind,
  `pair-awaiting-confirm` — that section's own doc has the reasoning), never
  from a mismatch or unknown-id arm either. `a2a serve` is a separate process
  from `aoided`, so it opens its OWN `aoide_protocol::feed::FeedWriter` onto the
  SAME `crate::daemon::events_path`/`EVENTS_CAP_BYTES`-capped feed file `aoided`
  already writes through — two independent writers sharing one truncate-in-place
  file, so a cap-truncate race at the 1 MiB boundary can lose a line; accepted,
  because this feed is ephemeral cues and the audit log above (already written
  at all three call sites) is the durable record. Every emitted record is
  `class: "gate"`, `source: "a2a-door"`, and a `payload` carrying
  `id`/`name`/`originAddr`/`url`/`direction` BY NAME ONLY — never a SAS, pubkey,
  nonce, or commitment; a watcher re-derives the SAS locally from
  `aoide_storage::pairing::list_inbound`/`list_outbound`, so this line is only
  ever a trigger, never trusted data. Best-effort throughout
  (`FeedWriter::append`'s own posture) — an unwritable events path never fails
  the ceremony. **The Spawn arm's gate (P-P3, narrowed again by P-P4, PAIRING.md
  decision 6 + the wire-authentication section)** — `message_send`'s
  `SendAction::Spawn` arm no longer consults `token_authorized` (the door-wide
  bearer, 2026-08-19's own amendment) at all: it requires `spawn_admitted`,
  which accepts ONLY a `NodeRung::Signature` resolution to a node that is BOTH
  `verified` and carries `"spawn"` in its grant IN THE MESH THE REQUEST NAMES
  (`may_spawn`, pure and directly unit-tested against `Grant` fixtures — no test
  in this file drives `do_spawn`'s real OS-level process spawn, same house rule
  every other Spawn-arm test already follows). That resolution comes from
  `verify_signed_request` (P-P4) — called once per connection in
  `handle_connection`, before any dispatch — which resolves the caller BY KEY
  (#63 P-ID5): the request's `X-Aoide-Timestamp`/`X-Aoide-Nonce`/
  `X-Aoide-Signature` headers are verified by trying the signature against every
  verified node's stored public key, and the record whose key verifies IS the
  caller; `X-Aoide-Node` is attribution only — a claimed-vs-resolved mismatch
  audits as attribution drift, and its one identity-adjacent role is the
  exact-name tiebreak among verified records sharing the verifying pubkey
  (CONTRACTS.md §6's P-P4 amendment has the full canonical-string/header shape,
  check order, collision semantics, and pinned vectors). The KEY-RESOLVED
  identity threads down as `signed_caller: Option<SignedCaller>` — the resolved
  record's `name` AND the stored `key` that verified the signature, so no
  consumer has to re-find the record by name to reach its key (LOW-1: a second
  lookup could pair this request's name with a key that never verified
  anything); when present, `message_send` resolves EXCLUSIVELY against it, never
  falling back to `aoide_storage::node_store::resolve_node`'s own two-rung
  ladder (a node's own `token_file` — `NodeRung::Token` — else the TCP origin
  against a node's `url` — `NodeRung::Addr`), even on a registry-lookup miss.
  Neither the `Addr` nor the `Token` resolution reaches Spawn — a bare
  TCP-source-IP-vs-`url` match carries no possession proof, and a bare
  shared-secret token is replayable and identical across every request the true
  node or an impersonator ever sends; both rungs still resolve a node identity
  for attribution below, just never for Spawn. A caller resolved via `Token` to
  a genuinely paired node gets a taught
  `-32006` telling it plainly to sign requests (its aoide is too old, or is
  failing to sign); a `Signature`-resolved node missing the `spawn` capability
  gets the exact `node allow` fix; every other shape gets the original "pair
  first, then allow" message. The resolved node's name also threads two ways
  past the gate: `do_spawn` calls `stamp_spawn_provenance` (LANE IDENTITY P-ID0,
  G16/G5, review round 1) to stamp `SessionRecord.origin = "node:<name>"`
  DIRECTLY on the just-spawned record once it registers — this door is the ONLY
  place a `node:*` value may originate, not the child's own env, since any
  same-uid process can set an env var on itself before invoking `aoide conduct`
  directly (`aoide-conduct`'s `session_conduct` refuses exactly that shape from
  its env read, and a third path, `graph/resurrect.rs::origin_to_carry`, refuses
  it again when reading a revived session's own ledger entry back —
  `state/session-ledger.jsonl` is unsealed, so a same-uid process could
  otherwise forge the shape there too). The same stamp carries the caller's
  `remoteParent` when it presented a valid `metadata["aoide/from"]` claim (P-RSA
  S3) — one registration wait, two change-once stamps, and the value is built
  from the VERIFIED caller `verify_signed_request` handed down (the resolved
  record's `name` and the stored `key` that verified THIS request), never from a
  header or body string (`claimable_caller` in `a2a.rs` holds the rung rule,
  `claimed_remote_parent` the value and the `-32602`, which the caller applies
  on the spawn side only). `stamp_spawn_provenance` polls for the record's
  registration on a best-effort budget of about 3s (300 polls, 10ms apart); a
  disclosed behavior change from the pre-P-ID0 synchronous env write — a child
  that registers slower than that window now loses its stamp, logged by name
  (`eprintln`) rather than silently, and the poll never retries unboundedly past
  it. Neither of these closes the FILE: a hand-crafted `sessions.json`/ledger
  line is still a readable, unflagged string on disk — sealing that is
  P-ID1/P-ID2, still open. The Inject arm's own `resolved_node` (a SEPARATE
  lookup from Spawn's) answers one question: its `from` attribution rides
  `do_inject`'s `--from` flag onto the `pending.json` entry that every inject
  writes. Attribution is not a gate, and no `aoide/from` claim changes whether
  an Inject parks. A shell target needs no check of its own at this door,
  because nothing here types into a session; the shell refusal that guards the
  local ping-back and doorbell is conduct's `auto_type_refusal`. **The grant
  lookup refuses an unreadable config**: `effective_mesh` is fallible, an
  unnamed request's grant is `Grant::none()` when the config will not load, and
  `spawn`/`mailDeposit`/`mailPoll`/`binding` refuse with `-32010` naming the
  file (`mesh_or_refusal`) rather than naming a guessed mesh. A request that
  NAMED its mesh never reads the config, and a caller that consults no grant (an
  unsigned loopback inject, an ordinary `tasks/get`) is untouched. **`tasks/get`
  reads a session's watch frame (P-RSA S6, CONTRACTS.md §6).**
  `params.metadata["aoide/frame"]` asks for the frame the local `session watch`
  renders; it rides as one `data` artifact, and is answered only for a caller
  that resolved through the SIGNATURE rung to a `verified` node whose grant in
  the request's mesh includes `read` (`output_read_admitted`, with
  `may_message`/`may_spawn`'s shape one capability over). Unsigned, bearer and
  address rungs are refused with the SAME text whether the session exists or not
  — the gate is no existence oracle. A frame needs no `remoteParent` key; only
  the ping-back history below does.
  `watch_frame` is conduct's existing `gather` with `raw = false`; the frame
  leaves through `Frame::for_wire` (this box's paths and the suggested command
  struck) and the door clamps the tail to 200 lines, each letter's body to 40
  lines and the frame to 256 KiB, shedding the oldest letter then the oldest
  output line and setting `truncated`. A frame read audits under its own label,
  `a2a.tasks/get.frame`. The refusal code is this arm's own, `-32011` — minted
  like Spawn's `-32006` and `mailDeposit`'s `-32010` — so a refused read and a
  refused signature (`verify_signed_request`'s `-32007`, always decided before
  this arm runs) are told apart by the code alone, never by matching prose. The
  refusal TEXT is unchanged: one sentence for every refused read, the same
  whether the named session exists or not. **`tasks/get` also serves a session's
  ping-back history (P-RSA S8, CONTRACTS.md §6).**
  `params.metadata["aoide/linesAfter"]` — the `seq` the caller has already seen
  — answers with the same status read plus the child's ring after that cursor,
  as one `data` message under `Task.history` (`messageId: "pingback"`, `{events,
  gap, last}` under the part's `data`). This one read is gated by
  `output_read_admitted` AND the key match: the child's record's own
  `remoteParent.key` must equal the key the caller's signature verified against
  (`history_admitted`), because those events belong to that parent and the
  2026-09-25 ruling widens the FRAME, never this. The refusal reuses `-32011`
  (the lane brief names no code of its own for this arm) with its own text
  naming the missing key. A request carrying BOTH output keys is judged by the
  stricter one: the frame alone is still admitted for a `read`-granted caller
  whose key is foreign, and a request that also asks for history is refused as a
  history read rather than answered with a frame around a silently missing ring.
  A history read audits under its own label, `a2a.tasks/get.history`.
  **`aoide/mailDeposit` (P-M2, `docs/architecture/MAIL.md`, CONTRACTS.md §6's
  new subsection) is the SECOND capability-gated method, after Spawn, and the
  first one not gated on `spawn`.** `mail_deposit` resolves the caller the
  identical KEY-RESOLVED way Spawn does (`ctx. signed_caller` against the
  registry), then requires `deposit_admitted` — `may_message` (`"message"` in
  the caller's grant for the mesh the request names, `grant_in_mesh`), mirroring
  `may_spawn` one capability over, with no historical Addr/Token rung to migrate
  off since `message` was introduced signature-only from the start. A refusal is
  `-32010` — a NEW code, distinct from both `-32006` (Spawn's own) and `-32007`
  (`verify_signed_request`'s own incomplete-headers/signature-mismatch code) —
  in one of THREE shapes, the P-CHARTER one preempting the other two: the mesh
  is charter-shaped with an undecidable operator key (told that, told the reason
  word, pointed at `aoide mesh charter show`; the operator keys themselves stay
  in the host's audit line), paired-but-not-allowed (told the exact `node allow
  <name> message on` fix) or anything else (told to pair, then allow). Past the
  gate, the envelope's own content is entirely `aoide_storage::mail::deposit`'s
  job — recomputing `msgid`, verifying the ORIGIN signature (the two-lookup
  identity model: hop via `signed_caller`, origin via the one key on record for
  `header.from.node`), deduping, and filing. `mail_deposit` self-audits
  UNCONDITIONALLY under `a2a.aoide/mailDeposit`, at both the admission refusal
  and the deposit outcome — mirroring `pair_request`'s "audit every call" shape
  rather than `message_send`'s narrower one, since a deposit never passes
  through `cli/src/dispatch.rs`'s own per-command audit and this is the only
  place a flood becomes visible. A filed **letter** owes nothing: its
  `accepted` answer is its receipt (ruling 131), so the door mints, spools and
  dials nothing for it, and a duplicate answers `duplicate` the same way.
  **A container addressed to another node is not this receiver's to open**
  (`deposit_sealed`'s `Hopped` arm): the door writes no mail line, records the
  hop in the audit log, spools it toward the node the four steps pick, best-effort drains that hop
  (never one the route said is HELD), and answers `accepted` with the hop named
  — nothing filed as correspondence, no reader armed, and no answer owed back,
  because the letter is not here. The audit stamp and the chain agree on the
  caller's name through the charter line (`declared_caller_name` →
  `routing::declared_name`), never the `nodes.json` nickname. It never rings the
  doorbell itself (P-M5a-2c: the resident daemon is the policy and audit
  boundary for every ring, so this door files and answers and stops there) — a
  remotely-deposited letter arms its readers exactly like a locally-filed one,
  but is rung only by the next daemon-side trigger for that name (a reader's
  Stop hook, a local filing, `mail ring` by hand), until a later slice (P-M5b-2)
  gives this door its own forward path to the daemon. A filed **receipt from an
  older node** never rings — it is not arming mail (`aoide_storage::mail`'s own
  `arms` predicate) — and instead retires the own letter it confirms, if any,
  via `aoide_storage::outbox::retire_by_ack` (a lookup keyed on the receipt's
  own verified origin and acked msgid; a forged or stale receipt retires
  nothing, and a transit, charter or receipt entry is never retired by it). A
  duplicate receipt goes through the same call. Every `outbox` call here runs
  strictly AFTER `mail::deposit` has already released its own lock — `mail` and
  `outbox` share
  the identical non-reentrant stage-lock primitive, so nesting one inside the
  other would deadlock a process against itself. **`aoide/mailPoll` (P-M3,
  `docs/architecture/MAIL.md`, CONTRACTS.md §6) is the relay-first half of the
  same model: a member with no inbound address asks a node it reaches for
  everything that node spooled toward it.** `mail_poll` demands `{node}` — a
  missing/empty one is `-32602`, refused before any lookup, the same
  shape-before-existence precedence `pair_poll` holds — resolves the caller the
  identical KEY-RESOLVED way `mail_deposit` does, and then requires
  `poll_admitted`: `may_message` AND `params.node == the caller's own resolved
  name`, because MAIL.md §Wire's rule is "the caller's verified identity must BE
  `node` (no polling on another's behalf)". A refusal is the SAME `-32010` the
  deposit arm mints, in three shapes (claiming a node this request is not signed
  as; paired but `message` not in its grant FOR THE REQUEST'S MESH, told the
  exact `node allow … message on --mesh <m>` fix; no verified signature
  resolution at all). The answer is exactly `{envelopes: […]}` — the sealed
  envelopes, oldest first, from `aoide_storage::outbox::poll_payloads`: every
  `hold` entry toward that node plus every `now` entry whose own attempts have
  been failing. **A poll writes no `tries`, no bookmark and no new state file
  anywhere**: its hand-over may seal an entry or park it, and writes no try and
  no outcome for a container it hands over. An entry retires only on its
  confirmation: a valid receipt from an older node naming an own letter,
  `mail outbox rm`, a held own
  letter the poll of the hop it was spooled toward files, or an own letter whose
  session ended (ruling 131). A re-poll hands the same envelopes over again by
  construction. It self-audits UNCONDITIONALLY under its own
  `a2a.aoide/mailPoll` label, carrying the caller and the handed-over COUNT —
  the pull direction's flood signal, visible at the node that spooled the
  letters. The answer is BOUNDED at `aoide_storage::outbox::POLL_BATCH_CAP`
  (50), oldest first: the poller's `filed` names retire what it took, so the
  next poll advances, and an unbounded batch would be a body the client's own
  `MAX_RESPONSE_BYTES` refuses. **`down` is the door's now**: the two mail
  methods read the declaration set once per request and refuse a caller their
  mesh declares `down` with that word as a RESULT (audited), beside the
  `message` half — the per-request quarantine `aoide node allow <node> message
  off` (MAIL.md §Status). An UNLOADABLE set refuses both methods with
  `config-invalid` and keeps serving everything else: no zone checks means no
  mail, never mail with the walls down. **An unloadable set is answered BEFORE
  caller resolution, to every caller**: where the request's signature verifies
  against the key its own `X-Aoide-Node` holds on the charter in force, the door
  answers the refused `config-invalid` result — fixed text, with the load error
  audited only, never sent — after the clock checks, the charter signature and
  the same audience and nonce steps as any other frame. So a replay of those
  bytes to this listener, once the config returns, is refused by its nonce set.
  A request whose signature
  does not verify, or naming a node that charter does not carry, still reads
  `-32007`.
- **Audience (`params.to`).** A verified request whose `params.to` is not this
  door's own identity key is refused `-32012` whatever its method. A request
  with no `to` is admitted only for the four old-client methods
  (`aoide/mailDeposit`, `aoide/mailPoll`, `aoide/binding`,
  `aoide/charterFetch`), audited `untagged`; for any other method it is refused
  `-32012` with a line naming the upgrade. `admit_verified` runs this check
  after verification and before the nonce, and a refusal here is unsigned and
  records nothing. What it stops: a signed
  request built for one door, replayed at another door that trusts the same
  signer, when the request is tagged. What it does not stop: an untagged call to
  one of the four methods, which stays open until the planned switch (not
  built). On the same listener its replay is refused by the nonce set. Across
  listeners the method's own check bounds it: a deposit is filed once while
  its line stays in the box, by msgid; a poll retires only what its poller
  acknowledges; a binding older than the stored one is refused as stale only
  while the stored one still verifies under the pinned key; a charterFetch
  changes no charter, key or grant. It audits a caller refused for not being
  on the LAN and each answer it serves; its other refusals write no line. An
  untagged frame is not bound to a door, so a node holding a signer's untagged
  frames from the last 120 seconds can replay them at another door that pairs
  the same signer. That door applies the same checks to its own mailbox and
  outbox, and each replay fills the signer's nonce set there; once the set holds
  the cap (512 by default), the signer's own frames at that door are refused
  -32013 until they expire. Also not stopped: PAIRPOLL records no nonce and has
  no start check, so a replay inside its skew window re-releases this node's own
  key and binding, a public read; a process running as this box's uid can sign
  as the box, since the key is on this box's disk; and each listener process
  keeps its own nonce sets (see Nonces, below).
- **Signed answers (relay-proof).** A JSON-RPC POST to `/` whose signature
  verified is answered with `X-Aoide-Answer-Signature`: the identity key signs
  `wire_auth::answer_string` over the request's own signature and nonce, the
  status and the exact body bytes (`answer_headers`). A signed request is a POST
  and nothing else: any other method is refused `-32007` before its signature is
  checked. Everything else is unsigned, and each write site says why in a
  comment there: an unparsed or unsigned request, every GET (the card included),
  every refusal (`-32007`, `-32008`, `-32009`, `-32012` and `-32013`), the
  `config-invalid` answer above, the busy 503s, and the SSE stream. That answer
  is checked only against the raw charter in force: the charter a node governs
  is read through the same config that failed to load (`charter::governing`), so
  there is none to sign under. The door's key signs nothing an unverified sender
  chose. A key that is not on disk, or will not load, leaves the answer unsigned
  and audited (`a2a.answer-unsigned`), and the answer path never mints one
  (`identity::load_existing`). The client refuses an unsigned answer.
- **Nonces are single-use in any letter case, per key.** `nonce_check` keys both
  the pubkey and the nonce trimmed and lowercased, the form `canonical_string`
  signs, into that key's own set (`NONCE_SETS`). A captured request whose nonce
  header is upper-cased is therefore a replay, not a fresh nonce. Each set holds
  at most `AOIDE_NONCE_CAP` (default 512) unexpired entries; a key at its
  cap is refused `-32013` over HTTP 503, unsigned, and nothing is recorded. A
  key's expired entries are dropped when that key is next checked, and no key's
  flood reaches another key's set. Each listener process keeps its own sets.
- **Start check.** A signed request stamped at or before the door's own start
  plus five seconds (`serve` and `serve_mail` record the start once, before they
  bind) is refused `-32008`, naming both times. The nonce sets are per listener
  process and a restart clears them, so this is what keeps a restart from
  re-admitting a frame an earlier process accepted: such a frame is stamped at
  most five seconds past its accept time. That holds while the door's clock does
  not step back across a restart; if it steps back, such a frame can be accepted
  again until its stamp plus 120 seconds. A client whose clock runs behind the
  door's is refused for up to five seconds plus that lag after a restart, and
  its link backs off, doubling, while it is refused.
- **The mail adapter (`aoide mail serve`, H1) — a second LISTENER, and not a
  second door.** `serve_mail` binds `127.0.0.1` and nothing else (no bind
  option, no flag, no env var: the front that faces the mesh is a
  TLS-terminating one, and the A2A door is never fronted by it), and its whole
  route is `route_mail`: `GET /.well-known/agent-card.json` answers
  `stripped_card`'s three keys ALWAYS (the mail profile has no door-wide token,
  so there is no credentialed caller to hand the full card to) and `POST /`
  dispatches through `mail_rpc` — the same three-armed function the door's own
  `handle_jsonrpc` falls through to first, so the two listeners cannot drift on
  what a mail call means. Everything else is `-32601` by construction:
  `message/send` (inject and spawn), `tasks/get`, `graphSummary`, the `pair*`
  ceremony and the SSE takeover are not refused here, they are unreachable —
  the functions behind them are never called from this process. A method name
  it cannot serve audits under its OWN label (`a2a.mail-adapter.refused
  message/send`; likewise `message/stream`, `tasks/resubscribe`, `tasks/cancel`), never a door method's name emitted by a listener that has no
  such method; a mail method keeps the label the door emits for it.
  `mailPoll` is sealed-only in the pull direction as well as `mailDeposit` in
  the push one: an entry spooled toward a poller that held no binding is
  WITHHELD (named in the answer's `withheld` list with `sealed-required`, and
  counted in the audit line) instead of crossing an HTTPS hop in the clear. A
  relayed entry is WITHHELD the same way unless this box can seal it to its
  destination (`reseal_entry`): the poller is the hop, not the destination.
  `handle_connection` takes a `Listener` (plus `Option<&InboundBearerConfig>`,
  `None` for the adapter) and is the ONE place that difference is checked, so
  the shared transport hardening (`MAX_BODY`/`MAX_LINE`/`MAX_HEADERS`/
  `MAX_REQUEST`/`MAX_CONN`, per-connection signature verification with its
  process-local nonce sets, the single audit log) is inherited rather than
  duplicated. Audit lines name their listener and the connection's origin
  (`HTTP {status} from loopback via mail-adapter`; the door's own detail stays
  exactly `HTTP {status}`) — the origin alone could not distinguish the two,
  since a front dials this box from loopback. That holds on the EARLY paths
  too: a malformed request and a refused signature are tagged by the same
  `Listener`, so an operator can attribute a hostile flood to the listener it
  hit. The Host header is never
  consulted (through a tunnel it is the front's hostname, not this process's
  business). The admission is the door's own too — one `verify_signed_request`
  (charter rung included), one `mail_rpc` calling the same `mail_deposit`/
  `mail_poll`/`node_binding`, one `grant_in_mesh` reading the caller's grant in
  the mesh its request SIGNED — so a charter-listed key is admitted and a
  revoked one refused through the adapter exactly as through the door.
- `discovery` — the discovery advertisement's SEND half (P-P6 + task
  #120, `docs/architecture/PAIRING.md`'s "Discovery
  (advertise-but-locked)" section, CONTRACTS.md §6's "Discovery
  advertisement" subsection). `a2a::serve` calls `spawn_advertiser`
  exactly once, at launch — the thread always exists, but a tick only
  SENDS when `a2a::resolve_discovery_advertise`'s launch-time force
  (`--discovery-advertise`/`AOIDE_DISCOVERY_ADVERTISE`, mirroring
  `resolve_bind_port`'s own precedence) OR the runtime switch
  (`aoide_storage::advertise::enabled`, flipped by `aoide node advertise
  on|off`, read fresh every tick) says on — both off by default: silent
  ticks, no socket, nothing on the wire. A sending tick (~30s, jittered)
  binds a fresh ephemeral UDP socket with `SO_BROADCAST`, sends one
  `aoide_storage::advertise` line (name + ssh hop claim ONLY — no door
  URL, no key, no identity file touched) to the fixed
  broadcast-address+port, and drops the socket — fire-and-forget, no
  connection state held between ticks. Fallible spawn
  (`thread::Builder::spawn`, `daemon.rs`'s discipline, not `a2a.rs`'s own
  plain `thread::spawn` for connection handlers — that one is reserved for
  a NEW socket-based door, not a background worker thread) — a refused OS
  thread costs discovery only, never the door. The RECEIVE half
  (`node discover`/`aoide pair`'s hostname-target sweep) lives in
  `aoide-client::discover` instead; this crate stays inbound/serve-only.
- `commands` — this crate's CLI commands: `daemon`, `a2a serve`, `events tail` (P-D3,
  appended newest — CLI-only, the same door-policy shape `a2a serve`/
  `aoide_secrets::commands::handle_secrets_watch` already hold for a
  foreground/blocking command).

## What it consumes

`aoide-protocol`, `aoide-storage`, `aoide-conduct`, `aoide-secrets` (task
#84 — the A2A door's inbound bearer resolve, `aoide_secrets::client::
resolve_bounded`, reused rather than a second wire client written here).
`aoide-client` is a dev-dependency only (one round-trip test) — production
code never calls into the outbound client from here.

## How it composes

Sits above `conduct` (reads/writes session state via `do_inject`/`do_spawn`/
`tasks/get`) and `storage`. **The registry-parameter seam**: `mcp::serve_stdio`
and `a2a::serve` take the assembled `Registry` and dispatcher as parameters
rather than reaching for a crate-global singleton — that singleton doesn't
exist until the app crate (`cli`/`strillix`) assembles it, and `server → cli`
would invert the dependency direction the whole split exists to forbid.
