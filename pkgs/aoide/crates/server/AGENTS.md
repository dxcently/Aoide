# AGENTS.md — aoide-server

## Invariants

- **Inbound/serve only.** This crate is the SERVER half of every door.
  Outbound client behavior (A2A client, adapters) belongs in `aoide-client`,
  never here — even a "just this once" helper reverses the direction the
  split is built to keep.
- **Never reach for a crate-global registry.** `mcp::serve_stdio` and
  `a2a::serve` take `Registry`/dispatcher as PARAMETERS because the
  assembled registry only exists in an app crate (`cli`/`lyra`). Adding a
  `server → cli` (or `→ lyra`) dependency to shortcut this is exactly the
  inversion Phase 4c's DI seam exists to prevent.
- **Untrusted input stops here.** Every door-facing parse/validate boundary
  in this crate is the last line before dispatch; don't push validation
  downstream into `conduct`/`storage` handlers that assume a trusted caller.
- **`mcp::serve_stdio`'s channel socket is the MCP subprocess's own,
  never `aoided`'s** (P-M5c-2, `docs/architecture/CLAUDE-CHANNEL-PROOF.md`):
  bound only when `AOIDE_SESSION_ID` is set and non-empty, for the lifetime
  of that one stdio session — no record, no command, no flag (house rule
  7). Any code that writes to stdout in this module MUST go through the
  shared `Arc<Mutex<_>>` (`write_line`) the request loop and the listener
  thread already both lock — a second unlocked writer reopens the
  torn-JSON-RPC-line interleave this seam exists to close. A
  `notifications/claude/channel` `meta` object's keys stay bare
  identifiers (`mailbox`, never `mail-box`); a hyphenated key is silently
  dropped by the harness on the other end. The listener serves one
  connection to EOF (or a dropped over-cap line) before accepting the
  next, so a caller is expected to connect, write one line, and close —
  the doorbell's own write is exactly connect + one line + close; a
  connection held open past its one line stalls every later caller.
- **The daemon door's own accept loop (`daemon::serve_daemon`/
  `accept_loop`) is thread-per-connection via the FALLIBLE
  `thread::Builder::spawn`, never the panicking `thread::spawn`** — a
  refused OS thread creation drops just the one connection instead of
  unwinding the whole accept loop, plus a short backoff on an accept
  error. This is `aoide_secrets::broker::serve`'s own accept-loop
  discipline, reused here BY CONVENTION (this crate cannot depend on
  `aoide-secrets`'s private `serve` fn) rather than `a2a::serve`'s plain
  `thread::spawn` — don't copy `a2a.rs`'s precedent onto a new socket-based
  door; copy `daemon.rs`'s instead.
- **`producers::SecretsMirror` never depends on `aoide-secrets`'s wire or
  record TYPES for PARSING, even though this crate already carries an
  `aoide-secrets` dependency it freely uses for the broker's socket/events
  PATH (`aoide_secrets::socket::socket_path`/`events_path` — plain,
  wire-type-free `PathBuf` resolvers; `daemon::run_loop` calls them
  directly, the "no cross-crate copying" convention
  (`pkgs/aoide/crates/AGENTS.md`) working as intended, not a boundary to
  route around).** `mirror_secrets_line` parses each feed line as a bare
  `serde_json::Value` and copies exactly the four named fields
  (`id`/`secret`/`consumer`/`timeoutSecs`) it recognizes — never the
  parsed object wholesale, and never through a shared typed struct. This
  is what makes "an unknown field never rides the mirror" a STRUCTURAL
  property instead of a discipline a future edit could quietly break by
  switching to a shared type; don't "simplify" this producer by importing
  `aoide_secrets::watch`'s own event type — the path functions are fine to
  reuse, the record SHAPE never is.
- **Every tick-driven producer (`producers::SecretsMirror::tick`,
  `producers::HandEditWatcher::sweep`, `events::poll_once`) stays a
  bounded, non-blocking, non-sleeping call — the loop/signal-handling
  wrapper around it lives ONE layer up** (`daemon::run_loop`'s tick,
  `events::tail`'s `SIGINT` loop). This is what lets every one of their
  tests call the bounded function directly with a deadline-poll or a plain
  synchronous assertion, never a fixed sleep or a real signal sent into
  the shared test binary — don't fold a `thread::sleep`/signal check into
  one of the bounded functions "to save a caller the loop."
- **`daemon::handle_conn`'s `dispatch` op (P-D4) adds no daemon-specific
  policy, and never will.** Every per-command door check that already runs
  over MCP/A2A (a CLI-only admin command's refusal, a gated command's
  `gated: true`, `mcp.serve`/`a2a.serve`'s non-Cli metadata replies) runs
  IDENTICALLY over this door, because the injected `dispatch` fn IS
  `cli::dispatch::dispatch` — the same function, the same registry, the
  same `inv.door` branches. Don't add a daemon-specific allowlist or
  permission table here "for symmetry with MCP's tool list" —
  `docs/architecture/AOIDED.md`'s "L2" section names a daemon-door
  allowlist a review-blocking violation; a new per-command policy need is
  proved by a failing test against the EXISTING handler's `inv.door`
  branch, never by a new table in this crate.
- **`daemon::accept_loop` refuses a CROSS-uid connection before it ever
  reaches `handle_conn` (LANE IDENTITY P-ID3, G8).** `cross_uid_gate` is a
  pure decision fn (`Option<PeerCred>` in, `Option<String>` refusal reason
  out) mirroring `aoide_secrets::broker::admin_gate`'s exact shape — reuse
  that mirroring for any FUTURE socket-based door in this crate rather than
  inventing a fourth wording. Reads `aoide_secrets::peercred::peer_cred`
  (already `pub`, already a dependency — do not reach into
  `aoide-conduct::graph::identity` for its OWN `pub(crate)`-only
  `peer_cred`, which is unreachable across the crate boundary anyway; do
  not vendor a THIRD `SO_PEERCRED` reader here). **This is a CROSS-uid
  floor only** — it does not, and is not meant to, stop a same-uid process
  from dispatching a request over this socket; every legitimate connector
  (the CLI's `daemon_dispatch` proxy, a hook, the conductor) already shares
  the daemon's own uid under OQ1-A.
- **`invocation_from_dispatch_request` stamps an absent `from` flag
  explicit-empty (LANE IDENTITY P-ID3, G8's attribution half) — never
  leaves it absent.** `send`'s own `resolve_sender` falls back to
  `AOIDE_SESSION_ID` off the CALLING process's env whenever `--from` is
  absent; a `dispatch`ed invocation runs its handler INSIDE this daemon
  process, so that fallback would read `aoided`'s own ambient env, not the
  connecting client's (which never crosses this socket at all). Stamping
  `--from ""` on an absent flag is `resolve_sender`'s own documented
  "explicit no attribution" form — it SKIPS the env fallback outright,
  the same mechanism `a2a::do_inject` uses for the identical leak (G9). Do
  not "fix" this by clearing `AOIDE_SESSION_ID` out of the daemon's own
  process env instead: this crate is thread-per-connection
  (`accept_loop`'s own doc above), and mutating global env from a
  connection-handling thread races every OTHER concurrent connection's own
  env reads — the flags-map stamp is per-request and touches no shared
  state. **This does NOT close the GATE** (`aoide_conduct::graph::
  send::real_attested_sender`, untouched by this phase, out of its scope
  fence) — it walks `std::process::id()`'s own `/proc` ancestry, which for
  a dispatched `send` is `aoided`'s own ancestry, not the connecting
  client's; in production (`init -> systemd -> aoided`) that never resolves
  a live sealed session, so a dispatched `send` with no `--yes`/autogate
  already fails closed to `pending` — not because this fix re-derives the
  real caller's identity, but because the daemon's own ancestry is
  architecturally incapable of impersonating one.
- **`producers::HandEditWatcher` is ONE shared `Arc<Mutex<..>>` instance
  (`daemon::SharedHandEditWatcher`), not tick-private (task #92).** A
  dispatched session command writes stage files on `handle_conn`'s own
  connection thread, never the tick thread, so `daemon::run_loop` hands the
  SAME watcher instance to `accept_loop`/`handle_conn` it ticks itself;
  `daemon::rebaseline_stage_roster` re-baselines the WHOLE roster after
  every completed `dispatch` op, unconditionally — never a per-command "which
  files did this write" table (the same drift trap the door-policy
  invariant above already forbids). Don't reintroduce a tick-private
  `HandEditWatcher::new(..)` inside `run_loop`'s loop body or inside
  `accept_loop`/`handle_conn` — a second instance means two baselines that
  can each independently go stale against the other's writes, reopening
  task #92 by a different door.
- **`daemon::seal_keypair` mints exactly ONE keypair per process, in a
  `OnceLock`, and it must NEVER be written to disk (LANE IDENTITY P-ID1,
  OQ1-A).** This is the daemon's own in-memory seal-signing key, deliberately
  a DIFFERENT keypair from `state/identity/`'s on-disk node-wire key — a
  same-uid attacker can read that on-disk file, so a seal signed with it
  would not be secret against the exact adversary this lane's thesis names.
  Don't "simplify" by reusing `identity::load_or_mint`'s key here, and
  don't add any code path that persists `seal_keypair`'s bytes anywhere —
  its whole secrecy claim is "this process is still alive, and `ptrace`
  against it is blocked" (Yama `ptrace_scope>=1`), which a disk copy would
  destroy outright.
- **`daemon::seal_freshly_registered_session` mints/stamps a seal ONLY for
  a successful `session start` dispatch whose record already carries a
  `pid`, and it is NOT a security gate.** No gate anywhere reads
  `SessionRecord.seal` yet (P-ID2 adds the first verify-on-accept caller) —
  don't make this function's success/failure affect the dispatch reply, and
  don't wire a door/socket decision on `seal`'s presence without first
  reading the LANE IDENTITY plan section (P-ID2/P-ID4's own scope). The pid
  it mints over is the record's OWN `pid` field, not yet a peercred-verified
  connecting pid — stated as scaffolding in its own doc comment, not to be
  quietly upgraded into a security claim by a future edit that forgets the
  boundary.
- **Request-line reads on the daemon socket go through
  `daemon::read_capped_line` (a hand-rolled `fill_buf`/`consume` loop),
  never `BufReader::read_line` (P-D4, closing a P-D2-flagged gap).**
  `read_line` only checks a size cap AFTER a `\n` (or EOF) finally
  arrives — a client that streams past the cap with no trailing newline
  could grow that connection's own buffer for as long as it kept sending.
  `read_capped_line` checks the accumulated length on EVERY buffer fill
  instead, so an over-cap, newline-less stream is disconnected the
  instant it crosses the cap. A future op or connection type reading more
  request lines off this same socket reuses `read_capped_line`, never a
  second hand-rolled read loop.
- **The tick's own reconcile/reap (P-D6) keep NO separate in-memory
  roster.** `reconcile_graph_projection` and `run_internal_reap` both
  re-read the stage files fresh on every call, the same as any other
  dispatch would — this is deliberate, not a missing optimization: since
  "no logic forks" already means a routed dispatch does a plain
  read-modify-write against disk with no daemon-side cache, the tick has
  nothing else to compare against either, and "fold in the newest
  out-of-band write" falls straight out of "the file on disk is the single
  source of truth at every instant." Don't introduce a persistent
  `HashMap<SessionId, SessionRecord>` cache here "for performance" — it
  would reintroduce exactly the two-writer divergence this design avoids.
- **This crate's own `env_lock()` (`lib.rs`)'s first call in a test binary
  also floors `$AOIDE_STAGE_DIR` at a fresh private tempdir, unless a test
  already set one (P-D6 safety net).** `daemon::run_loop`'s tick now
  WRITES through `aoide_conduct::graph::restage_graph`/`aoide_conduct::reap::
  reap_and_announce`; a `run_loop` test spawns that tick loop on a
  background thread it deliberately never joins (so the test itself can
  return once its own assertion holds), so that thread keeps ticking for
  the rest of the test BINARY's life — without this floor it would
  eventually read `$AOIDE_STAGE_DIR` as unset (once whichever test set it
  restores its own prior value) and start reading/writing the REAL
  `~/Aoide/state/stage/*` on this box. Every test that wants its own
  isolated tempdir still calls `env_lock()` first (existing convention)
  and restores what it captured on exit, same as `aoide-conduct`'s sibling
  `$AOIDE_DAEMON_SOCKET` floor (see that crate's own `AGENTS.md`) — don't
  remove or weaken either without re-reading why it exists. **A second
  floor (P-D8, same reasoning, one env var over) does the identical thing
  for `$AOIDE_STATE_DIR`:** `daemon::run_loop`'s entry now also calls
  `run_boot_auto_resume` once, which reads/writes a marker under
  `aoide_storage::fs::state_dir()` — the SAME un-joined background thread
  makes that call too, so without this floor it would eventually touch the
  real `~/Aoide/state/auto-resume-boot-epoch` on this box.
- **`daemon::run_boot_auto_resume` fires exactly ONCE per `run_loop` call,
  strictly BEFORE the tick loop — never move it inside the loop (P-D8).**
  It is a boot-time trigger, not a tick-cadence one: the guard
  (`daemon::epoch_already_fired`, a pure predicate deliberately factored
  out so it is unit-testable without `/proc/stat`) exists specifically to
  make a `Restart=on-failure` restart within the same boot a no-op, which
  only holds if the call happens once at entry, not once per tick. Don't
  fold it into the tick loop "for consistency with reap" — that would
  re-fire it every `REAP_EVERY_TICKS` and defeat the whole guard.
- **`run_boot_auto_resume`'s per-project loop carries no liveness check of
  its own (P-C4, durable-sessions plan).** It used to skip a whole project
  when ANY non-`done` session anchored to it; that was wrong for a
  multi-session undying set, since one live terminal would suppress
  reviving the rest. The skip moved into `aoide_conduct::graph::
  session_resurrect`'s own bare-mode selection, per candidate — this loop
  now just calls it unconditionally for every `autoResume` project. Don't
  put a project-wide `has_live`-shaped check back here; if a project's
  entire undying set is already alive, `session_resurrect` itself resolves
  to the empty-set `Outcome::ok` no-op.

- **`pair_request`/`pair_reveal` (P-P2) are deliberately UNGATED by
  `read_ok`/bearer verification, and this is not an oversight to "fix."**
  The pairing ceremony's entire purpose is establishing a credential where
  none exists yet — gating either on an existing credential would be
  circular. What keeps this safe: a parked/revealed/approved request
  grants NOTHING by itself (no `allows`, no spawn/bearer gate, P-P3's lane
  untouched), every REQUIRED field is validated BEFORE anything is parked
  or resolved (`valid_pubkey_hex`/`valid_nonce_hex`/`valid_commit_hex`/
  `valid_node_name`/`valid_node_url`) — `selfVia` (task #131) is the one
  deliberate exception: OPTIONAL, and read with no validator at all
  (absent/wrong-type/empty all fold to `None`), since it is never
  load-bearing enough to refuse the whole request over; don't add one "for
  consistency" — a malformed claim is `approve_inbound`'s problem alone,
  much later, the same as a malformed `Node.via` anywhere else. The
  commitment check
  (`aoide_storage::pairing::reveal_inbound`) binds a reveal to its own
  earlier request with no signature needed yet (an active MITM cannot
  force a shared SAS by choosing its own values after seeing the real
  ones), and the SAS confirmation (`aoide_storage::pairing::derive_sas`) is
  the actual human-verified gate — it lives in the CLIENT's `aoide pair`
  prompt (BOTH times it fires — once on each end), not in this
  door. Don't add a bearer check to either handler "for consistency with
  `message/send`" — that would break the bootstrap the whole ceremony
  exists to solve. `pair_poll` (Design A, task #119, REPLACES the old
  `aoide/pairApprove` reverse callback) is DIFFERENT: it is
  self-authenticating (its own doc has the mechanism) rather than
  door-level-gated, and this is the correct posture for it too — see the
  next bullet.
- **`pair_poll` verifies its OWN signature inline against the parked
  entry's stored `pubkey_hex` — never through `verify_signed_request`
  (P-P4), and never writes a node record.** No verified `Node` record
  exists on the approver's side until the very id being polled is
  approved, so P-P4's header scheme (which requires one) cannot gate this
  method — `pair_poll` decodes `{id, timestampIso, nonceHex,
  signatureHex}` from its OWN params and calls
  `aoide_storage::wire_auth::verify_signature_hex` directly against
  `InboundPairingRequest.pubkey_hex`. It reads
  [`InboundPairingRequest::approved`] and returns it, never mutates it —
  `mark_inbound_approved` (called from `aoide-client::commands::approve_inbound`,
  not from any wire handler) is the ONLY thing that ever sets that flag,
  purely locally, no wire call. Every non-approved/unverified/unknown
  outcome returns the IDENTICAL `{"status":"pending"}` (the
  existing-oracle discipline, `pair_poll`'s own doc) — don't add a
  distinct error code for "unknown id" or "bad signature" here; that
  would let an outsider learn something a legitimate not-yet-approved
  poller couldn't.
  `aoide-client::commands::approve_outbound` is what checks a released
  pubkey against what THIS instance learned at request time
  (`mark_outbound_awaiting_confirm`'s own `Mismatch` handling, entirely
  client-side now) — don't reintroduce that check here; this handler has
  no basis to know what the REQUESTER already learned.
- **`emit_pairing_event` (P-P5) fires ONLY from an Ok arm, never from a
  mismatch or unknown-id arm, and its `payload` carries fields BY NAME
  ONLY.** Two call sites today (`pair_request`'s `pair-parked`,
  `pair_reveal`'s `pair-revealed`) — `pair_poll` (Design A, task #119)
  deliberately does NOT call this: a poll arriving and being answered
  isn't a state change on the approver's side worth surfacing (it already
  knows it approved; it did so itself, locally), and the requester's own
  side never transitions asynchronously anymore either, only synchronously
  inside `approve_outbound`'s own poll-then-mark call — there is no longer
  a `pair-awaiting-confirm` kind. Add a call site the same way if a future
  milestone genuinely needs one — never widen the payload builder to pass
  a parsed struct wholesale, and never add `sas`/`pubkey*`/`nonce*`/
  `commit*` to the by-name list; a watcher re-derives the SAS locally from
  `aoide_storage::pairing::list_inbound`/`list_outbound`; this feed line is
  a trigger only. `a2a serve` opens
  its own `FeedWriter` onto `aoided`'s events file rather than routing
  through the daemon process (they're separate processes) — don't thread
  a socket call through here to "unify" the two writers; the cap-truncate
  race that creates is already accepted (module doc, CONTRACTS.md §6).
- **`message_send`'s Spawn arm gates on `spawn_admitted`, which requires a
  resolved, paired, spawn-allowed node identified via its OWN PER-REQUEST
  SIGNATURE — never a bare token, and never the address rung (P-P3
  decision 6, narrowed again by P-P4, `docs/architecture/PAIRING.md`'s
  wire-authentication section, CONTRACTS.md §6).** `handle_connection`
  calls `verify_signed_request` exactly once per connection, strictly
  before either dispatch path, and threads its result down as
  `signed_caller: Option<SignedCaller>` through `route`/`stream_task`/
  `RequestCtx` into `message_send`. The identity it threads is the
  KEY-RESOLVED one (#63 P-ID5): `verify_signed_request` finds the record
  BY the stored pubkey that verifies the signature — never by the
  `X-Aoide-Node` header, which is attribution only (a claimed-vs-resolved
  mismatch audits as `attribution-drift` via `attribution_drift_detail`,
  and the resolved name wins everywhere downstream). The `key` rides out of
  the verifier with the `name` for the same reason: a consumer that re-found
  the record by name could pair this request's name with a key that never
  verified anything, so the ONE thing that may stamp, gate, or attribute by
  key reads both off the verifier's own outcome. Don't reintroduce a
  name-based lookup into the verifier, and keep the no-match refusal a
  single code path with a single message — unknown key, unverified node,
  keyless record, and bad signature must stay indistinguishable (no
  existence oracle over the registry); collision semantics (shared-pubkey
  records: exact-claimed-name tiebreak, else ambiguous refusal) are pinned
  in CONTRACTS.md §6. When `Some(name)`, `message_send`
  resolves EXCLUSIVELY against that name (`NodeRung::Signature`) — no
  fallback to `aoide_storage::node_store::resolve_node`'s addr/token
  ladder even on a registry-lookup miss, since a request
  `verify_signed_request` already proved came from a specific node must
  never be silently re-resolved as if it came from whoever's address or
  token happens to match instead. Only when the request carries no
  signature headers at all does `resolve_node` run its own two-rung
  ladder (a presented token against a node's own `token_file` first —
  `NodeRung::Token` — else the TCP origin against that node's `url` —
  `NodeRung::Addr`). `spawn_admitted` accepts ONLY a `NodeRung::Signature`
  resolution, deferring the node-side check to `node_may_spawn(node)`
  (`verified && allows.contains("spawn")`) only in that case — neither the
  address rung nor the (now-insufficient) token rung reaches `do_spawn`
  any more. Both unsigned rungs still resolve a node identity fine for
  every OTHER purpose (Inject's `from` attribution, autogate) — they are
  excluded from Spawn specifically, since neither one is cryptographically
  bound to the one request presenting it: a bare source-address match
  carries no possession proof at all, and a bare shared-secret token is
  replayable and identical across every request the true node or an
  impersonator ever sends. `spawn_refusal` gives a SHAPE-SPECIFIC message
  for the code `-32006` still returns uniformly: a genuinely paired node
  resolved via the Token rung is told its aoide is too old to sign
  requests (upgrade the caller, don't re-pair); a Signature-resolved node
  whose `allows` lacks `spawn` is told the exact `node allow` fix; every
  other shape gets the original "pair first, then allow" message, now
  naming the signature requirement too. The door-wide bearer that gates
  every OTHER arm (read commands, the uniform-response guard, Inject's
  `effective_origin` coupling) is not consulted here at all. Signing
  itself never touches this crate — `aoide_storage::wire_auth` holds the
  canonical-string/verify logic, `aoide-client` holds the signer; this
  crate is verify-only, consistent with "inbound/serve only" above. No
  test in this file drives `do_spawn`'s real OS-level process spawn (an
  established precedent, `spawn_inject_prompts_success_branch_
  files_the_opening_turn_into_the_mailbase`'s own doc comment) — the gate
  itself is proven via the pure `node_may_spawn`/`spawn_admitted`/
  `spawn_refusal` predicates, `verify_signed_request`'s own dedicated test
  section, and `message_send`'s REFUSAL branches only. **P-P5b's own
  `node_spawn_signed_and_allowed_is_admitted_up_to_the_do_spawn_boundary`
  holds the SAME line**: it drives a REAL ed25519 signature (via
  `aoide_client::wire::build_message_send_body`, the dev-dependency edge)
  through the REAL `verify_signed_request` → `spawn_admitted`, proving
  admission all the way to (never through) the `do_spawn` call — the
  refusal-side sibling (`node_spawn_revoked_is_refused_...`) IS safe to
  drive through the real `message_send` because a refusal never reaches
  `do_spawn`. Don't "complete" the admitted-side test by calling
  `message_send`/`do_spawn` themselves — that would be exactly the real
  process spawn this precedent exists to avoid inside a `cargo test`
  binary (`std::env::current_exe()` there is the TEST binary, not a real
  `aoide`).
- **A spawn's wrapper argv comes from `aoide_conduct::graph::build_conduct_args`,
  never a second copy in this crate (P-RSA S10, CONTRACTS.md §6).** The door
  passes `headless = true` ALWAYS (it has no terminal to hand a child; stdio is
  nulled and the child is `setsid`'d) and `--task <slug>` exactly when the
  request named one under `metadata["aoide/task"]`
  (`aoide_protocol::wire::TASK_KEY`, read on the same two spots `aoide/spawn`
  is — `requested_task`). `spawn_argv`/`spawn_task_slug` are `do_spawn`'s own
  two pure halves, split out for the same reason `spawn_child_command` was
  (a `cargo test` binary's `current_exe()` is the harness): the argv is asserted
  with `Command::get_args()` and the env with `get_envs()`, never by driving a
  real spawn. Three invariants to hold while editing:
  (a) **the slug check and the live-slug check stay FIRST in `do_spawn`** —
  before `spawn_session_id` mints an id and before `current_exe()` resolves — so
  both refusals cost one RPC and no process.
  `do_spawn_refuses_an_illegal_and_a_live_held_slug_through_its_own_boundary`
  drives `do_spawn` ITSELF to hold that: it is safe only because both checks
  return before any spawn, and it will fail loudly (the test binary trying to
  conduct itself) if either is moved below `cmd.spawn()`. Do not "simplify" it
  into a pure-function test.
  (b) **the live-slug check is SHARED, not rewritten**:
  `aoide_conduct::graph::live_run_for` + `live_run_refusal` are the wrapper's own
  admission step and its own sentence (M2 of the S10 review — the door composed
  `conduct` directly and so bypassed them, letting a peer squat the operator's
  task name). Never add a second copy of the message here.
  (c) **the slug refusal is `-32602` and spawn-side only** — the same discipline
  S3's malformed `aoide/from` claim holds; an Inject request carrying the key is
  answered exactly as before, because it builds no value from it. A live-held
  slug is `-32602` too (state this node holds, not a capability the caller
  lacks), NOT one of the capability codes `-32004`/`-32006`.
  The predicate is `aoide_storage::node_store::valid_node_name`
  (there is no separate task-slug validator to call: the slug IS the mailbox
  name), and the echoed value goes through
  `aoide_conduct::graph::clean_line` first — a caller's illegal slug may carry
  control or `Cf` bytes and this string reaches an RPC body (L6). Do NOT add
  `--timeout`/`--report-to`/`--instructions-path`/`--parent`
  here without a source for each — a default deadline would kill long remote
  runs, the report stays on the child's node as the `no_mailbox` rail (never a
  letter, Q5), and `parentSessionId` is a LOCAL field (§4).
- **The `metadata["aoide/from"]` parent claim is honoured on the Signature
  rung ONLY, and the value is built from the RESOLVED record, never from wire
  bytes (P-RSA S3, CONTRACTS.md §4/§6).** `parse_message_send_params`' fourth
  field reads `message.metadata[aoide_protocol::wire::FROM_SESSION_KEY]` and
  nothing else — never the top-level `params.metadata` fallback `aoide/spawn`
  also accepts, because this is a claim about WHO is calling and the client's
  outbound builder writes it in that one place; an empty or non-string value is
  "no claim", not a third state. `claimable_caller` is the rung table and
  `claimed_remote_parent` is the whole decision, pure over exactly two inputs:
  the VERIFIED caller (`SignedCaller` — resolved `name` + the stored `key`
  that verified, straight out of `verify_signed_request`) and the claim
  string. `claimable_caller` passes that caller on for the
  `NodeRung::Signature` resolution and yields `None` for every other rung;
  never fold a second question into it (the ignore-audit stays where it was,
  ahead of the uniform-response guard). Four invariants live across the two and
  none may be relaxed: (1) a weaker rung — no resolution, `NodeRung::Token`,
  `NodeRung::Addr` — IGNORES the claim and yields nothing, because a spawn
  already requires the Signature rung and a claim from an unauthenticated
  caller has nobody to attribute it to; the ignore is audited once
  (`a2a.message/send`/`status:"ignored-unsigned-from"`) and must NOT become a
  refusal — a distinct error there would be a new oracle where today there is
  silence. (2) A signed caller's malformed claim is an error the CALLER
  applies: `valid_claimed_session_id` is the SAME predicate the client refuses
  its own claim with (never a second spelling), and the `-32602` is applied on
  the SPAWN side only — the value rode inside the body the caller signed, so a
  bad one is a client bug worth surfacing, but the Inject arm consumes the
  claim as a COMPARISON (the S5 bullet below) and a malformed one there is
  matched by nothing, exactly as an absent one is. `claimed_remote_parent`
  itself stays a pure `Result` so both halves are unit-testable without a
  spawn.
  (3) `node`/`key` come off the `SignedCaller` — the key that actually
  verified this request — with the claim supplying `sessionId` alone; a name
  read off `X-Aoide-Node` or the body would be exactly the forgery the key
  check exists to stop, so never thread a wire string into this value.
  (4) `spawn_session_id` mints `a2a-<pid>-<secs>-<n>` — the monotonic counter
  is load-bearing: pid+second alone collides for two spawns inside one second,
  and two children sharing one id share one `sessions.json` record, so the
  last `stamp_spawn_provenance` would decide whose run it is. The
  stamp is `do_spawn`'s `remote_parent` argument → `stamp_spawn_provenance`,
  which spends the ONE registration retry loop on both its stamps
  (`stamp_origin`, then `stamp_remote_parent`) — never a second poll, never a
  second thread — and the child gets NO env var for it (`spawn_child_command`
  clears `AOIDE_SESSION_ID`/`AOIDE_SESSION_ORIGIN` and adds nothing back),
  the same reason `node:*` origin stopped riding env at P-ID0. This door never
  writes `parentSessionId`: every reader of that field treats it as a LOCAL id,
  so a foreign value there would dangle or grant local autogate
  (CONTRACTS.md §4).
  `verify_signed_request`'s canonical string now reads `&req.method` (the
  request's own OBSERVED method), not a hardcoded `"POST"` literal (P-P5b,
  closing a P-P4 review finding) — a genuine behavior no-op today (every
  signed request is a POST), but the "binds method" claim above is now
  structurally true, not merely coincidentally true.
- **`message_send`'s Inject arm feeds `should_deliver_now` through
  `origin_for_inject` first (P-S6, CONTRACTS.md §6's "Node authentication
  today" standing paragraph) — a VERIFIED signature strips
  `ConnOrigin::Loopback`'s automatic auto-deliver pass, whatever the
  connection's own address looks like, UNLESS that node's own signature
  rung is itself autogate-marked.** An ssh `-L` forward (or any other
  loopback-terminating proxy) delivers a tunneled node's packets from its
  own end's sshd, so `classify_origin` sees loopback for a tunneled request
  exactly like a genuinely local caller — `origin_for_inject(origin,
  signed_caller.is_some() && !sig_autogate)` closes that gap by coercing
  the origin fed to `should_deliver_now` to `ConnOrigin::Unknown` for a
  signed, NON-autogate node (reusing that variant's existing fail-safe arm,
  the same move `effective_origin` already makes for an invalid door-wide
  token — no fourth `ConnOrigin` kind). **The `!sig_autogate` guard is not
  optional plumbing — `should_deliver_now(ConnOrigin::Unknown, _)` ignores
  `autogate_match` entirely (unconditional `false`, see that function's own
  match arm and `uniform_response_guard_never_fires_for_a_per_node_
  autogated_token`'s doc comment for the existing pin), so coercing an
  autogate-marked signed node's origin to `Unknown` would make it
  UN-deliverable, the opposite of the restoration this phase owes.** An
  UNSIGNED request is completely untouched: `origin_for_inject` is the
  identity function when its `signed` argument is `false`, so a genuinely
  local caller's loopback trust is exactly as before this phase. The
  like-for-like half: `autogate_match` folds in a THIRD signal,
  `sig_autogate` — `resolved_node` matched via `NodeRung::Signature` whose
  own `Node.autogate` is `true` — alongside the existing
  `ip_autogate`/`token_autogate`, computed AFTER `resolved_node` now (moved
  down from before it) so this fold can read it; an operator who already
  marked a node auto-deliver keeps that behavior once it starts signing,
  riding the ordinary Loopback/Remote arms (which DO consult
  `autogate_match`) instead of the coercion. Don't gate this on
  `token_configured`/`TokenState` — that's `effective_origin`'s own,
  separate question (an invalid DOOR-WIDE bearer); this narrowing fires on
  `signed_caller`/`sig_autogate` alone, unconditionally. Tests:
  `signed_inject_from_a_non_autogate_node_on_a_loopback_connection_is_held_pending`
  is the actual regression pin (the exact hole a tunnel would otherwise
  open); `signed_inject_from_an_autogate_node_on_a_loopback_connection_still_auto_delivers`
  is the restoration; `origin_for_inject_is_the_identity_function_when_unsigned`
  and `origin_for_inject_downgrades_loopback_once_the_request_is_signed`
  pin the pure predicate directly.
- **A REMOTE PARENT steers the child it spawned without pending — the
  claim's second consumer, riding exactly those two rails (P-RSA S5,
  CONTRACTS.md §6).** `remote_parent_match(caller, claim, target)` is the whole
  predicate, true only for all three at once: the caller
  `verify_signed_request` proved (Signature rung, so every weaker rung's `None`
  can never match), the target record's `remoteParent.key` equal to that
  caller's verified key, and its `remoteParent.sessionId` equal to the claim.
  Both equalities are load-bearing — the key is what this door's own spawn
  stamped from the key that verified THAT request, so a caller can only ever
  match the child of the node it actually is. The target side is
  `session_remote_parent`, a stage read behind
  `claimed_from.as_deref().is_some_and(...)`: paid ONLY by a request that both
  carried a claim and proved a signature, so an ordinary inject and every spawn
  read exactly what they read before. The hit needs BOTH rails `sig_autogate`
  rides, and each rail carries one transport — neither is redundancy. The
  EXEMPTION from `origin_for_inject`'s downgrade carries the loopback/ssh `-L`
  shape: `should_deliver_now(ConnOrigin::Loopback, _)` delivers
  unconditionally, so the exemption alone is enough there, while without it a
  tunneled parent is coerced to `Unknown` — whose arm delivers nothing at all.
  The FOLD into `autogate_match` carries `ConnOrigin::Remote`'s shape: that arm
  consults `autogate_match` and nothing else, so a directly-addressed parent
  pends without the fold (the exemption is inert there — the origin was never
  loopback to begin with). It overrides neither earlier question: the door-wide
  bearer still runs FIRST — a door with `aoide.a2a.tokenFile`/`bearerSecret` set
  admits a remote parent only if it presents the bearer, and `effective_origin`
  still coerces a bearer that does not classify `Valid` — and a node's own
  `autogate` flag need not be on for a parent to steer its child (the same
  independence `send_gate`'s local parent rule has), so pass
  `autogate_match || remote_parent_hit`, never `remote_parent_hit` folded INTO
  `autogate_match`. A hit audits
  `a2a.message/send`/`status:"autogate-remote-parent"`; a MISS adds nothing at
  all — no new error, no new pending wording, the same bytes the same request
  got before. A malformed claim needs no arm here: this door never stamped a
  value outside `valid_claimed_session_id` as any record's `sessionId`, so
  equality is false exactly as for an absent claim, and the `-32602` stays
  spawn-side where the value is actually built. That is also why the client
  stops refusing one on the send path: `send --to` drops an unruly claim, sends
  unclaimed, and names the reason on one warning line
  (`not claiming parent: <reason>`) — the door answers the same send the same
  way either way — while `node spawn` still refuses the call outright. Tests:
  `remote_parent_match_needs_a_signed_caller_its_key_and_its_session` is the
  predicate's whole table; `a_remote_parent_steers_its_child_without_pending_
  with_autogate_off`/`_on` are the hit; `a_genuinely_signed_remote_parent_
  steers_its_child_without_pending` is the non-loopback origin;
  `a_tunneled_remote_parent_steers_its_child_without_pending` is the
  loopback/ssh `-L` origin the exemption exists for (its pending check runs
  before the byte join, so its regression fails instead of hanging);
  `a_door_token_refuses_a_remote_parents_delivery_uniformly` is the bearer
  running first; `a_remote_parent_mismatch_leaves_todays_result_byte_for_byte`
  is the miss.
- **`do_inject`'s `from` attribution (P-P3 decision 7) is scoped to the
  QUEUED path only — never an immediately-delivered payload's bytes.**
  `session_send`'s own `from` mechanism also prefixes DELIVERED text
  (`provenance_prefix`, "from `<sender>`: "), so stamping a resolved
  node's identity unconditionally would change what an already-autogated
  node's delivered message looks like — a regression
  `autogated_node_delivers_despite_being_non_loopback` pins against.
  `message_send`'s Inject arm computes `from` as `None` whenever
  `deliver_now` is `true`, `Some("node:<name>")` only when it's `false`
  (queuing). Don't lift that `!deliver_now` guard without re-reading why
  it's there.
- **`do_inject` stamps the `from` flag EXPLICIT-EMPTY when its own `from`
  parameter is `None` (LANE IDENTITY P-ID3, G9) — never leaves the flag
  absent.** `session_send`'s `resolve_sender` falls back to
  `AOIDE_SESSION_ID` off the CALLING process's env whenever `--from` is
  absent, and `do_inject` calls `session_send` DIRECTLY, in-process — the
  "calling process" for a remote A2A inject is `aoide a2a serve` itself, a
  long-lived process whose own ambient env has nothing to do with whichever
  remote node just sent the message. `from.unwrap_or_default()` (an empty
  `String` when `None`) is `resolve_sender`'s own documented "explicit no
  attribution" form (`--from ""`) — it skips the env fallback outright,
  rather than merely overwriting whatever the env currently holds, so this
  holds regardless of what `a2a serve`'s own env carries at any given
  moment. Don't revert to a bare `if let Some(f) = from { flags.insert(...)
  }` "for symmetry with reading `from`" — that shape is exactly what let
  the daemon's own ambient `AOIDE_SESSION_ID` leak into an unattributed
  inject's pending record before this fix
  (`an_unattributed_inject_never_falls_back_to_this_processs_own_ambient_session_id`
  is the regression pin). This closes only the ATTRIBUTION leak, not the
  GATE — `real_attested_sender` still walks `a2a serve`'s own `/proc`
  ancestry for `do_inject`'s in-process `session_send` call, same posture
  `aoided`'s dispatch-socket `send` holds (`accept_loop`'s own invariant
  above); it already fails closed in practice because `restore_delivery`
  and `do_inject` both only reach the gate with `--yes` already forced or
  the ancestry never resolving a live session.
- **`NONCE_CACHE` (P-P4) is process-local, in-memory, and deliberately NOT
  a `HashSet` — a bounded `VecDeque<(node, nonce)>` capped at
  `NONCE_CACHE_CAP` with FIFO eviction, so it never needs a second
  data structure to know which entry is oldest.** It lives in THIS crate
  (`a2a.rs`), not `aoide-storage::wire_auth` — the module doc there states
  why: it is the one piece of P-P4 state with no durable file behind it at
  all, so it belongs beside its only consumer
  (`verify_signed_request`), the same "ephemeral runtime state stays where
  it's used" reasoning that already keeps `producers.rs`'s tick state out
  of `aoide-storage`. `verify_signed_request` records a nonce ONLY after
  every cheaper check (including the signature itself) already passed —
  don't move the `nonce_is_replay` call earlier "to fail faster"; a forged
  or garbage nonce must never consume a cache slot. The cache keys on the
  VERIFYING PUBKEY, never a node name (#63 P-ID5): `X-Aoide-Node` is
  outside the canonical string, so a name-keyed cache would let a captured
  request replay under a shared-key twin's name — don't "simplify" the key
  back to a name. `handle_connection`
  calls `verify_signed_request` exactly ONCE per connection, strictly
  before both the streaming and the plain-JSON-RPC dispatch branches —
  don't duplicate that call inside `route`/`stream_task`/`handle_jsonrpc`;
  they only ever receive the already-computed `signed_caller`.
- **`a2a::self_url(bind, port)` is the ONE formula the AgentCard's `url`
  field and `route`'s own `aoide/graphSummary` handling call — never a
  second inline `format!("http://{bind}:{port}/")` (P-P6).** Before this
  phase the two independently carried the same literal that merely
  happened to agree (the exact shape the `HTTP_METHOD` precedent already
  warns against, `client/AGENTS.md`). A future change to how the door
  URL is derived (a public-hostname override, a reverse-proxy prefix, …)
  touches this one function and every caller inherits it. The discovery
  advertisement is deliberately NOT a caller — it carries no door URL at
  all (task #120: rendezvous, not authentication;
  `aoide_storage::advertise`'s module doc), and must never regrow one.
- **`discovery::spawn_advertiser` is called from EXACTLY one place —
  `a2a::serve`, before the accept loop starts (P-P6 + task #120).** It is
  never called from `handle_connection`, a per-request path, or anywhere
  else — one thread per `a2a serve` process, for that process's whole
  lifetime, mirroring how `--bind`/`--port` are resolved once at launch
  and held unchanged. Whether a tick SENDS is `resolve_discovery_
  advertise`'s launch-time force OR'd with the runtime switch
  (`aoide_storage::advertise::enabled`), read INSIDE the thread each tick
  — keep the read per-tick, so `aoide node advertise on|off` lands
  without a restart, and don't gate a SECOND call site on the same
  env/flag "for redundancy": a duplicate advertiser thread would just
  double the send rate and complicate the "both off means silence"
  proof.
- **`do_spawn` never acks `submitted` on `cmd.spawn()`'s success alone
  (task #103).** `cmd.spawn()` only proves the WRAPPER `aoide conduct`
  process launched — it says nothing about whether that process's OWN
  attempt to exec the configured agent succeeded, since a missing
  `spawnAgent` binary fails inside a separate, detached process this door
  has no synchronous view into (`aoide-conduct::graph::conduct::
  session_conduct`'s "spawn FIRST" ordering already registers no session
  for that failure — the gap was entirely on THIS side, acking success
  before that outcome was knowable). `poll_bounded_exit` gives the wrapper
  `SPAWN_LIVENESS_ATTEMPTS × SPAWN_LIVENESS_INTERVAL` (40×10ms = 400ms) to
  prove it's still alive via `Child::try_wait()` before the ack goes out; a
  wrapper that exits inside that window gets `spawn_died_immediately_
  message`'s taught refusal (naming the configured program, never the full
  command line or any env) as a proper JSON-RPC error instead of a
  `submitted` Task naming a session that will never appear in
  `sessions.json`. This is a bounded WAIT, not a watcher — no new tick
  producer, no new roster state, and every legitimate spawn (an agent
  meant to run for minutes) simply pays 400ms of fixed RPC latency it never
  notices. That is a stated CONSTRAINT on `spawnAgent`, not an accident:
  the configured agent is assumed to be long-lived, and one that
  legitimately runs to completion in under 400ms would be misreported as
  died-immediately — configure such a command as a job, not a spawnAgent. `poll_bounded_exit`/`spawn_died_immediately_message` are pure
  over an injected poll closure specifically so they're unit-testable
  without a real spawn — `do_spawn` itself is still never driven by a test
  in this file (the existing precedent, `spawn_inject_prompts_success_
  branch_...`'s own doc comment).
- **`session_ref_lookup`'s `has_socket` is DERIVED at read time — the
  stored socket path must exist ON DISK, never merely be a non-empty
  string — while `SessionRef`/`decide_send_action` stay pure and disk-free
  (the identical bug shape `e2758f7` fixed on the `graph` door,
  `aoide-conduct`'s `graph::doc::is_conductable_now`).** `shellbridge.
  service` owns `$XDG_RUNTIME_DIR/aoide` with `RuntimeDirectoryPreserve=no`,
  so a rebuild deletes a live session's control socket without ever
  touching `sessions.json` — a stored `conductable: true` plus a non-empty
  `socket` string can long outlive the file it names. The disk check lives
  ONLY in `session_ref_lookup`, the one place `decide_send_action`'s
  injected `session_lookup` closure is instantiated against the stage file;
  `decide_send_action` itself still takes no I/O and still gates on
  `sref.conductable && sref.has_socket` unchanged. This matters more here
  than on `graph`: a stale `Inject` would tell a remote node its message is
  being delivered when nothing can reach the target, instead of the
  existing `-32004` "session not conductable" refusal. Don't move this
  check into `decide_send_action` or `SessionRef` "for locality" — both
  types' own doc comments state the point of staying stage-file-free and
  unit-testable with a bare closure, no socket or tempdir required.
- **`tasks/get` carries the session's watch frame, and the read that admits
  it is `output_read_admitted` (P-RSA S6, CONTRACTS.md §6).** The request
  signal is `params.metadata["aoide/frame"]` — `params` only, never
  `message.metadata`, the `message/send` fallback `aoide/spawn` also accepts,
  because this is a request about a session rather than a message — and the
  answer is the SAME status read plus the frame as one `data` artifact, so a
  request without the key stays byte-identical. The gate is three clauses at
  once: `read_ok` (the door-wide bearer rule every read arm carries —
  `token_authorized`), a caller resolved through the SIGNATURE rung, and that
  record `verified` with `read` in its `allows` (`node_may_read`,
  `node_may_spawn`'s twin one capability over). Resolution goes through
  `resolved_caller`, which takes the rung from the PROOF rather than a second
  lookup: no `SignedCaller` (unsigned, bearer, address) is `None` whatever the
  registry holds. The remote-parent key match is deliberately NOT required to
  READ — reading is wider than writing, and `send`'s remote-parent delivery
  (S5) and the ping-back history still need the key. The refusal is this arm's
  OWN code, `-32011` — minted like Spawn's `-32006` and `mailDeposit`'s
  `-32010`, and never `-32007`, which stays `verify_signed_request`'s
  incomplete-headers/signature-mismatch family decided BEFORE this arm runs —
  with ONE text for every refusal and the same text whether the named session
  exists or not, so the gate is no existence oracle beyond the status read it
  shares the method with. The gate runs FIRST, so a refusal never depends on
  whether the id exists; a session with no frame to read (unknown id, a `sub:`
  card, a record keeping no conduct-owned PTY) answers `session watch`'s own
  taught refusal under `-32001`, after the gate. A frame read audits under its
  own label, `a2a.tasks/get.frame`; the frame leaves through `Frame::for_wire`
  (this box's paths and the suggested command struck) inside the tail (1..=200),
  letter-body (40 lines) and 256 KiB frame caps. Tests:
  `output_read_admitted_is_a_signed_verified_node_with_read` is the gate's
  whole table and `resolved_caller_needs_a_proof_and_finds_its_own_record` the
  rung rule;
  `an_unsigned_frame_read_is_refused_and_is_no_existence_oracle` pins the one
  text and the `-32011` code, `a_signed_node_without_read_is_refused` the
  revocation shape, and `a_signed_reader_reads_any_sessions_frame` the ruling
  that reading needs no `remoteParent`.
- **`tasks/get` also serves the ping-back history, and THAT read keeps the key
  match (P-RSA S8, CONTRACTS.md §6).** `params.metadata["aoide/linesAfter"]`
  — read by `lines_after`, `params` only and tolerant of a non-numeric value
  (it reads as `0`: a wrong cursor costs duplicates, never a parent its
  child's history) — answers with the same status read plus the ring after
  that cursor as ONE `data` message under `Task.history` (found by
  `messageId: "pingback"`, its part's `data` the whole
  `RingRead{events, gap, last}`). The gate is `output_read_admitted` AND
  `history_admitted`, which is the output gate plus the child's own stamped
  `remoteParent.key` equal to the key the caller's signature verified against:
  the key, never the stored `node` label, never a wildcard, and an empty
  stored key matches nobody. This is the ONE output read the 2026-09-25 ruling
  does NOT widen — a `read`-holding node may watch any frame, but what a child
  published for its parent belongs to that parent. The refusal reuses
  `OUTPUT_READ_REFUSED_CODE` (`-32011`; §4.4 of the lane brief names no code of
  its own for this arm) with its OWN text, because "you are not this child's
  parent" is not the same reason as "you hold no `read`", and the gate still
  runs before the status read so neither is an existence oracle. **A request
  carrying BOTH output keys is judged by the stricter one**: asking for the
  history is asking for the history, so a foreign key is refused for the whole
  request rather than handed a frame around a silently missing ring — while
  the same caller's frame-only request is still admitted. The label is
  `a2a.tasks/get.history` (it wins the tie). Tests:
  `history_is_admitted_only_for_the_key_this_door_stamped` is the table,
  `a_signed_parent_reads_its_childs_ping_back_history` the admitted read and
  its cursor, `a_foreign_key_is_refused_for_history_while_its_frame_is_admitted`
  the refusal, the both-keys rule and the no-existence-oracle pin,
  `an_unsigned_history_read_is_refused` the unsigned shape, and
  `lines_after_reads_only_params_metadata_and_tolerates_any_value` the reader.
- **`aoide/mailDeposit` (P-M2) is the SECOND capability-gated A2A arm,
  after Spawn, and the first not gated on `spawn` — `deposit_admitted`
  mirrors `spawn_admitted` one capability over, but signature-only from
  the start, with no Addr/Token fallback rung to migrate off of the way
  Spawn once had.** `node_may_message` is `verified && allows.contains
  ("message")`, checked only against `ctx.signed_caller`'s KEY-resolved
  node (`resolved: Option<&Node>`, `None` whenever the request carried no
  verified signature at all). `deposit_refusal` returns `-32010` for
  BOTH its shapes (paired-but-not-`message`-allowed, told the exact `node
  allow … message on` fix; everything else, told to pair then allow) —
  **`-32010` is deliberate and must never regress to `-32006` (Spawn's own
  code) or `-32007` (already `verify_signed_request`'s own incomplete-
  headers/signature-mismatch refusal code, CONTRACTS.md §6) — a new
  capability-gated arm mints its own code, it never reuses one two
  refusal families already own.** `mail_deposit` self-audits
  UNCONDITIONALLY, under its own `a2a.aoide/mailDeposit` label, at TWO
  points — the admission refusal, and again after `aoide_storage::mail::
  deposit`'s own outcome — mirroring `pair_request`'s "audit every call"
  shape, deliberately NOT `message_send`'s narrower "only the notable
  branches" one: a deposit never passes through `cli/src/dispatch.rs`'s
  own per-command audit, so this is the ONLY place a flood becomes
  visible, and volume must show whether every one of those deposits
  landed accepted, refused, or malformed. **Every `outbox` call
  (`spool_and_drain_ack`'s spool, `retire_by_ack`) runs AFTER
  `mail::deposit` has already returned and released its own lock — never
  nested inside it.** `mail`'s and `outbox`'s stage locks wrap the
  identical `fs::try_stage_lock`, a plain blocking `flock`, not
  re-entrant; nesting them across a lock boundary would deadlock a
  process against a lock it already holds, not merely contend for it. A
  filed **letter** mints an ack addressed back to the origin, spools it,
  and best-effort drains that node once, synchronously, reusing the SAME
  `aoide_conduct::mail_bridge::drain_node` the daemon tick calls — one
  drain implementation, never a duplicate dial built here. **Neither arm
  of `mail_deposit` ever calls `ring`** (P-M5a-2c: the resident daemon is
  the policy and audit boundary for every ring, and this door is not the
  daemon). A remotely-deposited letter arms its readers exactly like a
  locally-filed one, but is rung only by the next daemon-side trigger for
  that name — a reader's Stop hook, a local filing, `mail ring` by hand —
  until a later slice (P-M5b-2) gives this door its own forward path to
  the daemon; don't reach for `aoide_conduct::graph::ring` here to close
  that gap early. A filed
  **receipt** instead calls `aoide_storage::outbox::retire_by_ack` (spec
  item 7) — a pure lookup keyed on the receipt's own verified
  `from.node`/acked-msgid, so a forged or stale ack finds no matching
  entry and retires nothing; don't add a second, independent verification
  step here, the lookup itself already proves both of spec item 7's
  checks (see that function's own doc). A **duplicate** whose original
  filing was a letter re-sends the ack (the sender's earlier ack evidently
  never arrived); every other duplicate is a silent no-op — the
  `letter`/`receipt` vocabulary has no third shape to stop an ack-of-an-ack
  from ping-ponging forever, so don't ack a receipt. **The resend goes
  through `outbox::write_ack_if_absent`, never `write_entry` directly
  (mail register §26 outbox fix)** — it mints via `mail::mint_ack` on
  every duplicate as before, but only spools when no ack for the same
  `(origin, acked_msgid)` is already sitting undelivered in that node's
  outbox, so a link redelivering the same duplicate thousands of times
  (a dead transport, e.g. a missing `ssh` binary) spools one ack instead
  of one per redelivery. This is NOT a permanent "already acked" ledger:
  once the pending ack's own entry retires (a real delivery, or `mail
  outbox rm`), the next redelivery correctly finds nothing pending and
  respools — preserving the "sender's earlier ack evidently never
  arrived" invariant above for a genuine loss. Don't add a ledger that
  survives the pending entry's own removal here; a prior attempt at
  exactly that broke
  `a2a::tests::a_duplicate_of_a_filed_letter_respools_its_ack`.

## Extension points

- **A new serve-side command** (`daemon`, `shellbridge` registration, `a2a
  serve`, `events tail`) adds a `cmd!`/`register` entry in `commands.rs`,
  wired into the owning app crate's `commands::all()` — core-only today
  (`daemon`/`a2a serve`/`events tail` are core identity, per root
  `AGENTS.md`).
- **A new door type** (beyond CLI/MCP/A2A) gets its own `serve_*` function
  here, taking `Registry`/dispatcher the same injected way.
- **A new daemon tick producer** (P-D3's `SecretsMirror`/`HandEditWatcher`
  are the first two) is a plain struct in `producers.rs` with its own
  bounded `tick`/`sweep` method, constructed once in `daemon::run_loop`
  before its tick loop and called once per iteration — never a producer
  that spawns its OWN thread or sleeps internally (previous invariant).
- **A new capability-gated A2A method (beyond Spawn/Message)** follows
  `spawn_admitted`/`deposit_admitted`'s shape: a `node_may_<verb>`
  predicate (`verified && allows.contains("<verb>")`) consulted only
  against a `NodeRung::Signature` resolution, a dedicated `<verb>_refusal`
  returning ONE NEW reserved code (never `-32006`/`-32007`/`-32010`,
  already spoken for), and — if the method has no per-dispatch audit path
  of its own the way a bare `message/send` command does — a self-audit at
  the door, unconditional, covering every outcome, not just refusals. The
  matching client-side gate (`aoide-client`'s `AGENTS.md`, the
  shallow-local/deep-remote split) stays LOCAL-shallow: this crate's
  predicate is the only place the capability itself is actually checked.

## Docs update required in the same commit

- This `README.md` when a new module or serve-side command is added.
- `CONTRACTS.md §6` when an A2A wire shape changes.
- `CONTRACTS.md §3`'s "MCP door" subsection when the `initialize` shape or
  the channel notification shape changes.
- `CONTRACTS.md §3`'s "Daemon wire" subsection when the daemon socket's own
  wire shape changes (`ping`/`subscribe`/`dispatch`).
- `pkgs/aoide/crates/AGENTS.md` for cross-crate invariants — not restated
  here.
