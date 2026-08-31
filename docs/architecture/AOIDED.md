# The aoided workstream — event bus, fourth door, remote reach, graph residency, harness summoning

Design for the five-rung ladder that turns `aoided` from a self-check skeleton
(`pkgs/aoide/crates/server/src/daemon.rs:29` — `run()` audits one startup
record, prints status JSON, exits in ~25ms; unit held alive by
`Type=oneshot` + `RemainAfterExit`, `modules/nucleus/aoided.nix`) into the
resident process the house rules already describe: one policy surface, one
gate, one audit log, everything flowing through it. Core stays
cargo-buildable on any Linux — nothing below shells out to nix or assumes
NixOS; only the deployment module (`aoided.nix`) is nix territory.

The shape, end state:

```
                                   ┌────────────────────────────────────────┐
   secrets broker (system uid)     │  aoided (user daemon, Type=simple)     │
   /run/aoide-secrets/events.jsonl │                                        │
        │  tail (Follower)         │  tick loop ──┬─ secrets-feed mirror    │
        └─────────────────────────▶│              ├─ hand-edit watcher (#69)│
                                   │              ├─ session reap           │
   session commands / hook door ──▶│  socket ─────┴─ registry dispatch      │
   (unix socket, fallback: files)  │   $XDG_RUNTIME_DIR/aoide/aoided.sock   │
                                   │              │                         │
                                   │  events.jsonl◀ (append-only, capped)   │
                                   └──────┬───────────────┬─────────────────┘
                                          │ tail          │ projection writes
                              desktop surfaces,      state/stage/*.json
                              `aoide events tail`    (readers untouched)

   remote: claude.ai ──▶ Tier-3 MCP connector (user-enabled) ──▶ any mesh
   host's doors ──▶ A2A message/send + bearer ──▶ peer boxes.
   aoided itself NEVER listens on a network.
```

Every rung reuses a live-proven mechanism rather than inventing one: the
events feed generalizes the secrets broker's feed
(`pkgs/aoide/crates/secrets/src/broker.rs:1151`), the socket framing
generalizes the secrets wire ("one request line → zero or more interim lines
→ exactly one final reply line", `broker.rs:388` + `secrets/README.md`), the
door is the existing registry dispatch (`pkgs/aoide/crates/cli/src/dispatch.rs:51`)
behind the existing `Door::Daemon` variant
(`pkgs/aoide/crates/protocol/src/audit.rs:61`), and remote reach is the
existing A2A door (`pkgs/aoide/crates/server/src/a2a.rs`).

---

## L1 — the event bus

### Feed primitives move to `aoide-protocol`

The secrets crate holds both halves of the pattern, live-proven under
`ProtectHome=true` sandboxing and broker restarts:

- **Writer**: `append_events_feed` (`secrets/src/broker.rs:1151`) — one JSON
  object per line, append-only, `EVENTS_MAX_BYTES` cap (`broker.rs:1120`,
  1 MiB) with truncate-to-empty-in-place past the cap, explicit chmod on
  create, best-effort (every failure `eprintln!`'d, never propagated).
- **Reader**: `watch::Follower` (`secrets/src/watch.rs:513`) — open at EOF,
  delta reads only, `(dev, ino)` stat-the-path comparison on every poll
  (`watch.rs:551`) so a delete-and-recreate across a daemon restart reopens
  at 0, `len() < pos` reopen for in-place truncation, partial trailing line
  held across polls.

These extract into a new `aoide_protocol::feed` module — protocol is the DAG
leaf both `aoide-server` and `aoide-secrets` already depend on
(`aoide-secrets` depends on protocol/libc/serde/serde_json only, its
`AGENTS.md` invariant, so protocol is the ONLY crate that can host a shared
primitive without a new dependency edge). Shim discipline
(`pkgs/aoide/crates/AGENTS.md`, "No cross-crate copying"): the code MOVES,
`secrets::watch::Follower` becomes `pub use aoide_protocol::feed::Follower`,
and the broker's private `append_events_feed` delegates to
`feed::FeedWriter { path, cap, create_mode }`. Secrets' existing tests are
the acceptance suite for the move — byte-identical behavior.

```rust
// aoide_protocol::feed (new module)
pub struct FeedWriter { path: PathBuf, cap: u64, create_mode: u32 }
impl FeedWriter {
    /// Append one JSON line. Best-effort: log-and-swallow, never Err to
    /// the caller's hot path. Truncate-in-place past `cap` before writing.
    pub fn append(&self, payload: &serde_json::Value);
}
pub struct Follower { /* moved verbatim from secrets::watch */ }
```

### aoided's own feed and record shape

- Feed path: `$AOIDE_DAEMON_EVENTS` override, else sibling of the daemon
  socket — `$XDG_RUNTIME_DIR/aoide/events.jsonl` (the same
  `$XDG_RUNTIME_DIR/aoide/` directory the conduct session sockets already
  own, `pkgs/aoide/crates/conduct/src/graph/conduct.rs:50`). Same
  resolve-once-pass-as-parameter discipline as
  `secrets::socket::events_path` (`secrets/src/socket.rs:74`).
- Cap: 1 MiB, truncate-in-place — the feed is ephemeral cues on tmpfs, not
  an audit trail. The audit trail stays `~/Aoide/log`
  (`protocol/src/audit.rs:17`), unbounded, unchanged.
- Record shape, one per line (versioned, additive-only):

```json
{"v":0,"ts":1756100000,"class":"secret","kind":"parked","source":"secrets-mirror","payload":{"id":"a3f1-2","secret":"totp","consumer":"melete"}}
```

`class` is the existing `EventClass` kebab vocabulary
(`protocol/src/audit.rs:73` — `audit`/`gate`/`rice`/`content`/
`notification`/`secret`); `kind` is producer-specific; `payload` is
name-only data. `EventClass::Secret`'s no-`untrusted_data` ban and
`Notification`'s data-never-code rule carry over verbatim: a forwarded
title rides inside `payload` as an opaque string field, never anywhere a
consumer would parse as a command.

### ONE BUS: the secrets mirror

aoided TAILS the group-readable secrets feed
(`/run/aoide-secrets/events.jsonl`, 0640, group `aoide-secrets-access` —
the operator user aoided runs as is already in that group) with a
`feed::Follower`, and re-publishes each parsed event as a name-only mirror
record (`class:"secret"`, `source:"secrets-mirror"`). Rules:

- The secrets feed stays AUTHORITATIVE. The mirror is a convenience fan-in
  so desktop surfaces subscribe in one place; anything that needs to ACT
  (approve/dismiss a parked ask) speaks the secrets socket ops
  (`client::pending`/`approve`/`dismiss`), exactly as `secrets watch` does.
- Mirror records are trigger, never truth — the same "tail is a TRIGGER,
  `pending` is the AUTHORITY" rule `secrets/AGENTS.md` states. A missed
  line is reconciled by the consumer, not re-guaranteed by the bus.
- Values never appear; the source feed is name-only by construction and the
  mirror copies fields by name (`id`/`secret`/`consumer`/`timeoutSecs`),
  never wholesale.
- Secrets feed absent (broker not installed/running): the Follower's
  NotFound arm already returns no lines; the mirror simply stays quiet.

### A SECOND live writer: the pairing events feed (P-P5)

Unlike the secrets mirror above (aoided TAILS a foreign feed and
re-publishes onto its own), the A2A door's `a2a serve` process appends
DIRECTLY onto aoided's own feed — the first producer that isn't aoided
itself. `emit_pairing_event` (`aoide-server::a2a`) opens its own
`feed::FeedWriter` on the identical resolved path/cap/create-mode and
appends a `class:"gate"`, `source:"a2a-door"` record from the Ok arm of
each of the three pairing-ceremony wire methods (CONTRACTS §6's "Pairing
wire"/"Pairing events feed" subsections) — `pair-parked`/`pair-revealed`/
`pair-awaiting-confirm`, payload fields by name only
(`id`/`name`/`originAddr`/`url`/`direction`), never a SAS/pubkey/nonce/
commitment.

**Two writers, one file — a named, accepted race, not an oversight.**
`aoided` and `a2a serve` are separate processes; both can observe the
feed at or past `EVENTS_CAP_BYTES` and both truncate-in-place rather than
rotate. A cap-truncate race at the exact boundary can lose a line from
either writer's append. This is accepted rather than fixed with a shared
lock or a single-writer proxy because the feed was already documented as
ephemeral cues, never the durable record (the single audit log, written
at all three call sites regardless of feed success, is that record — the
SAME "best-effort, never blocks the caller's own hot path" posture
`emit_notify`/`append_events_feed` already hold for the secrets side),
and because every consumer (`aoide pair watch`) already re-derives
its actionable set from `aoide_storage::pairing` directly on a safety
tick rather than trusting the feed's own completeness — the identical
"tail is a TRIGGER, the storage-backed state is the AUTHORITY" rule the
secrets mirror above already lives by. A future SECOND concurrent writer
of any kind inherits this same acceptance; a caller that needs a
lossless record reaches for the audit log, never this feed.

### The first producer: the hand-edit watcher (#69)

A tick-driven stat sweep over the broker-owned file roster — the stage files
(`state/stage/sessions.json`, `hooks.json`, `projects.json`, `graph.json`,
`pending.json`, `herald.json`; paths from `aoide_storage::stage`,
`storage/src/stage.rs:17-29`). The daemon records `(mtime, len)` after each
of its own writes; a tick that finds a file changed with no daemon write in
between emits `{"class":"audit","kind":"hand-edit","payload":{"file":"sessions.json"}}`
and re-baselines. Detection and narration only — the daemon never reverts a
hand edit.

### The loop and the unit

`aoide_server::daemon` grows `serve(config) -> io::Result<!>`-shaped
`run_loop`: bind the socket (next section), spawn the accept thread, then
tick (~1s): poll the secrets Follower, run the hand-edit sweep, run the
periodic jobs L4 adds. Thread model mirrors the secrets broker
(`broker.rs:315`): thread-per-connection via fallible
`thread::Builder::spawn` (drop one connection, never unwind the accept
loop), 250ms backoff on accept error. The `aoided` binary
(`cli/src/bin/aoided.rs`) calls it with the assembled registry and dispatch
fn injected — same DI seam as `mcp::serve_stdio`
(`server/src/mcp.rs:37`; `aoide-server` never reaches for a crate-global
registry, its `AGENTS.md` invariant). `aoide daemon` (the CLI command,
`server/src/commands.rs:54`) keeps its one-shot self-check semantics on
non-Cli doors and gains a `--serve` special-case at `run_cli` for parity
with `a2a serve`; the systemd unit execs the `aoided` binary as today.

The unit flips per the revert note it carries (`modules/nucleus/aoided.nix`,
"When the daemon grows its real loop, revert to Type=simple + Restart"):
`Type=simple`, `Restart=on-failure`, `RestartSec=5s`, drop
`RemainAfterExit`. The `BindsTo`-anchored doors (aoide-mcp, aoide-a2a) keep
working — a resident daemon is exactly what they were written against.

Terminal reachability (house rule 7's test): `aoide events tail
[--class <c>...] [--json]` — a new core command that follows the daemon feed
with a `feed::Follower` and prints matching lines. QML/desktop surfaces
pick the bus up through this bridge or by tailing the feed file; neither
needs the daemon socket.

---

## L2 — the fourth door: registry dispatch over the daemon socket

### Socket

`$AOIDE_DAEMON_SOCKET` override, else `$XDG_RUNTIME_DIR/aoide/aoided.sock`.
Bind mirrors `secrets::broker::bind_socket` (`broker.rs:275`): create
parent, remove stale socket, bind, chmod `0600` (user-private — unlike the
secrets socket there is no cross-uid audience; `$XDG_RUNTIME_DIR` is 0700
anyway, the chmod is belt-and-braces).

### Framing

Newline-delimited JSON, the secrets wire contract verbatim: one request
line → zero or more interim lines (`"interim":true`) → exactly one final
reply line. One `write_json_line`-shaped formatter, one
`read_final_reply`-shaped client reader (the precedent:
`broker.rs`/`client.rs`; the framing rule is stated in
`secrets/AGENTS.md`). Three ops, closed set:

```json
{"v":0,"op":"ping"}
{"v":0,"op":"subscribe","classes":["secret","gate"]}
{"v":0,"op":"dispatch","path":["graph","view"],"args":[],"flags":{"json":"true"}}
```

- `ping` → `{"ok":true,"daemon":"aoided","pid":…,"version":"…",
  "sealPubkeyHex":"…"}`. The liveness probe L4's clients use;
  `sealPubkeyHex` (LANE IDENTITY P-ID2, `CONTRACTS.md` §4's `seal`
  paragraph) is this process's current seal-signing public key — never
  secret, but a same-uid-writable FILE next to the seal it vouches for
  would be cryptographically void, so it is fetched fresh from a live
  round trip instead. That round trip is NOT attacker-proof under OQ1-A:
  `bind_socket` unlink-then-binds with no flock/pidfile, so a same-uid
  attacker can already race or evict the real listener and answer with a
  forged key — LANE IDENTITY P-ID3 added a CROSS-uid peercred floor to
  this socket's accept, which does not close this (a same-uid attacker
  racing the bind is the same uid the floor admits); a `flock`/pidfile
  guard on `bind_socket` would be the actual fix, not attempted there. It
  buys no more than the raw per-session socket already leaves open.
- `subscribe` → the connection becomes a stream: each matching bus event is
  written as an interim line as it happens; the final reply only comes on
  shutdown. Classes are explicit — an empty/absent `classes` delivers
  NOTHING (default-deny per class, the `Subscription` posture
  `protocol/src/policy.rs` already models).
- `dispatch` → build an `Invocation { path, args, flags, door: Door::Daemon }`
  and call the injected dispatch fn (the exact
  `invocation_from_call` shape the MCP door uses, `server/src/mcp.rs:86` —
  but taking path/args/flags literally rather than un-flattening tool
  arguments). Final reply: `{"outcome": <the full Outcome envelope>}`.

### Policy: no new allowlist

`Door::Daemon` already exists and is already audited
(`protocol/src/audit.rs:61`). The per-command door policy is the handlers'
own, already in force:

- CLI-only commands refuse with the door-hint `Outcome` — the secrets admin
  surface (`require_cli`, `secrets/src/commands.rs:33` and its
  `*_is_cli_only_*` tests at `commands.rs:1171+`) and the long-running
  launches (`handle_a2a_serve`'s non-Cli arm,
  `server/src/commands.rs:39-51`) already branch on `inv.door`. Over the
  daemon door they refuse exactly as they do over MCP/A2A, with no edit.
- Everything else dispatches; `dispatch()` appends the audit record with
  `door: daemon` and marks gated commands uniformly
  (`cli/src/dispatch.rs:68-88`). Gated stays gated: the daemon door
  surfaces `gated: true` and never admits — admission remains the user's.
- `run_cli`'s `special` closure commands (raw stdout, long-running servers)
  never reach dispatch specially over this door; their plain handlers'
  non-Cli arms answer, same as MCP today.

This is the documented extension point: "a new door type gets its own
`serve_*` function here, taking Registry/dispatcher the same injected way"
(`pkgs/aoide/crates/server/AGENTS.md`). Untrusted input stops in
`serve_daemon`'s parse/validate boundary — malformed line → one error
reply, connection survives; oversized line (cap 1 MiB) → error, drop
connection.

One schema, four doors: CLI, MCP, A2A, Daemon all dispatch the same
registry. `CONTRACTS.md §3` gains the daemon-door paragraph; no schema
shape change.

---

## L3 — remote reach rides the A2A door

aoided grows NO network listener, ever — unix socket and nothing else.
Remote reach composes what already exists:

- **Ingress**: the Tier-3 Aoide MCP connector, user-enabled only, never by
  an agent (root `AGENTS.md`, Tier 3). A claude.ai session lands on some
  mesh host's door.
- **Enumerate**: `aoide session --hosts` / the peer registry
  (`storage/src/peer_store.rs:35` `Peer`, `:61` `PeerRegistry`) name the
  mesh; each peer's capability card is its A2A AgentCard, derived from that
  binary's own `schema --json` with `implemented`-only skills
  (`CONTRACTS.md §6`, "AgentCard"). No second inventory.
- **Route**: `send --to peer/<query>` already delivers over A2A
  `message/send` (`conduct/src/graph/send.rs:8-16`, `deliver_remote`), and
  the receiving peer's own gate governs (`a2a.rs:566` `decide_send_action`,
  `:724` `do_inject`, `:790` `spawn_inject_prompt`). Bearer auth is the
  landing-in-parallel bearer lane on the existing token surface
  (`a2a.rs:348` `token_authorized`, `peer_store.rs:196` `token_bytes_eq`,
  `:216` `is_autogated_peer_token`); this workstream consumes it, never
  reimplements it.

### The hub option

Any machine can conduct — the conductor is whichever session drives. One
additive, default-off marker names an always-on host (e.g. sakaki) as the
standing orchestrator:

- `Peer` gains `hub: bool` (`#[serde(default)]`, additive/v0-safe — the
  same discipline every `SessionRecord` addition follows,
  `CONTRACTS.md §4`).
- New command `aoide peer hub <name> [--clear]` — sets the flag on exactly one
  peer (setting it moves it; `--clear` removes it). Registered in the
  client crate's peer command group, appended last (golden discipline).
- Consumers: address resolution prefers the hub as the default remote
  target when a `--to` query matches no local session and names no explicit
  peer; the messaging inbox relay uses the hub as the persistent drop
  point. Both are preference, not protocol — a mesh with no hub behaves
  exactly as today. Flags default off (house rule).

---

## L4 — graph residency

Sessions register and tick over the daemon socket; the stage files become
projections of daemon state, written by the daemon so every existing reader
(bare `graph`, the conductor, QML via `graph.json`) is untouched.

### Routing

The session-write family — `session start/phase/end`,
`session hook`, and `session reap` — routes through a thin client.
Two members named by the original design deliberately do NOT route:
`session wrap` spawns a long-lived child with inherited stdio and blocks
on it, which a stateless request/reply socket cannot carry (its direct
registration writes are folded by the daemon's tick reconcile like any
out-of-band write), and the `subagent-*` operations are crate-internal
helpers reached only through `session hook`'s own mapping, never
separately dispatched:

```rust
// aoide-client (outbound half, per the server/client split)
pub fn daemon_dispatch(inv: &Invocation) -> Option<Outcome>
// connect to the daemon socket; Some(outcome) when the daemon answered,
// None on connect failure/timeout (~100ms budget) — the caller falls back.
```

Each routed handler tries `daemon_dispatch` first and falls back to the
current direct stage-write path on `None`. The daemon executes the SAME
handler code (its dispatch is the same registry); no logic forks. This
keeps "runs anywhere with a shell" true: no daemon, no difference.

### Ownership and reconciliation

- While the daemon runs, its in-memory roster is the working truth; every
  mutation lands in memory and is immediately projected to
  `sessions.json`/`hooks.json`/`graph.json` via the existing atomic writes
  (`storage/src/fs.rs:240` `atomic_write_bytes`) under the existing stage
  lock (`fs.rs:289` `with_stage_lock`).
- The files remain the durable, canonical at-rest record. Daemon startup
  loads them; each tick compares stage mtimes against its own last write
  (the same baseline the #69 watcher keeps) and folds in any out-of-band
  write (a fallback-path CLI write, a hand edit) before projecting again.
  Fold rule: per-record newest-`updatedAt` wins; unknown fields round-trip
  (the readers-tolerate/rewriters-round-trip contract, `CONTRACTS.md §4`).
- All writers — daemon, fallback CLI, anything — keep taking
  `with_stage_lock`. There is never a lock-free write path.

### Liveness

The reaper model is untouched: `is_session_dead`'s never-guess predicate,
the staleness bands, socket-probe liveness, pre-boot ghosts, orphan-socket
sweep (`conduct/src/reap.rs:151`, `:732`). What changes is only WHERE it
runs: the ~12s systemd timer keeps firing `aoide session reap`, which now
routes through `daemon_dispatch` like every other session command — daemon up,
the sweep runs in the daemon against its roster; daemon down, the direct
path runs as today. Additionally the daemon tick runs the same reap
internally on the same ~12s cadence, making the timer a redundant backstop
rather than the mechanism. No second liveness mechanism is introduced
(`conduct/AGENTS.md` invariant).

---

## L5 — harness summoning

### `spawn --windowed`

A core command flag on the existing `spawn`
(`conduct/src/graph/spawn.rs`): instead of detaching a headless
`conduct --headless` child, exec a terminal that runs the same conducted
command.

- Terminal template: `$AOIDE_TERMINAL` (env; a config-file key can join
  later without changing the contract). Whitespace-split into argv; a
  `{cmd}` placeholder token is replaced by the conduct command's argv, and
  a template without the placeholder gets it appended
  (`kitty -e` and `foot sh -c '{cmd}'` both work).
- No template set → taught error naming the env var and one example. No
  display (`$WAYLAND_DISPLAY`/`$DISPLAY` both absent) → taught error
  ("headless host — use `spawn` without `--windowed`"). No nix
  anywhere; `lyra` never enters this path — a terminal emulator is a shell
  concern, not paint.
- The child is `aoide conduct -- <agent cmd>` exactly as the headless path
  builds it, so registration, control socket, and the parent-autogate lane
  (`send.rs:128` `sender_is_parent`) all come for free.

### AgentProfile grows launch/resume knowledge

Two additive fields on `AgentProfile` (`protocol/src/agents.rs:112`), per
the "a new harness is a table entry, not a scatter of conditionals" charter:

```rust
/// argv that launches this harness fresh (claude: &["claude"]).
pub launch: &'static [&'static str],
/// argv that resumes a prior harness session by ITS OWN id
/// (claude: ["claude", "--resume", <id>]). None = harness cannot resume.
pub resume_args: Option<fn(harness_session_id: &str) -> Vec<String>>,
```

### Capturing the harness's own session id

`SessionRecord` (`storage/src/records.rs:29`) gains additive
`harnessSessionId: Option<String>`. The hook door (`session hook`,
`conduct/src/graph/send.rs`) stamps it from the raw hook payload's own
`session_id` on every event that carries one — so it lands for wrapped
sessions AND for hook-only sessions aoide never birthed. For claude the
record id and harness id often coincide; the field is stamped regardless so
resume never has to guess which kind of record it holds.

### The durable session ledger

Reaping prunes `sessions.json`; resume needs memory that survives it. New
append-only ledger at `state/session-ledger.jsonl` (under
`aoide_storage::fs::state_dir`, `fs.rs:48` — real disk, not tmpfs), one
line per session at the moment it leaves the roster (clean `session end`
and reap both write it, which after L4 is one code path):

```json
{"v":0,"sessionId":"…","agent":"claude","harnessSessionId":"…","cwd":"/home/khoa/Aoide","title":"…","petname":"…","startedAt":"…","endedAt":"…","resumedFrom":null}
```

Append-only, never truncated, never a lookup key for live state —
`sessions.json` stays the roster; the ledger is history.

### `resurrect --project <x>`

Resolve `<x>` against `projects.json` and anchor every ledger entry to it
(the same longest-path-prefix rule bare `graph` uses). Selection then
branches on the flags: `--all` widens to every anchored entry, `--id`
narrows to one specific `sessionId`, and bare (neither flag) resumes the
project's WHOLE undying set (`state/undying.json`, durable-sessions plan
P-C4) — every anchored entry currently marked durable
(`session grant undying on|off`), minus any id already alive (non-`done`) in
`sessions.json`, deduped by `sessionId` keeping the entry with the newest
`endedAt` (an id that was resurrected and exited again can appear twice in
the append-only ledger). `--all` and `--id` are unchanged escapes: both
widen or narrow past the undying set regardless of the mark. An empty
bare-mode selection is an honest `Outcome::ok` no-op naming the undying set
as empty for the project, never a silent success. Every surviving candidate
then resolves through TWO arms (`resolve_candidate`, P-C6): the harness arm,
unchanged, filters to harnesses with a verified `resume_args`; a candidate
the harness arm finds nothing for falls to the TERMINAL arm — a `restore`
snapshot present (P-C5) marks it a conducted shell, not a harness, so it
resolves `[<login shell>, "-l"]` (the same `$SHELL` → passwd → `/bin/sh`
order `modules/dendrites/kitty.nix`'s own wrapper uses to pick a shell for a
brand-new terminal) rather than a `--resume <id>` no shell could ever honor.
A candidate neither arm resolves is skipped with a taught message naming it,
never a guessed invocation. Every resolved candidate spawns via the
windowed path with its resolved argv. Rules:

- The revived session is a NEW `sessionId` — ids are never recycled. The
  new record carries additive `resumedFrom: Option<String>` naming the
  ledger entry's sessionId; any graph (re)stage projects it as a `resumed`
  edge beside `spawned`/`anchors` (`CONTRACTS.md §4`, graph.json — additive edge
  kind). If the old id was undying, the mark transfers onto the new id in
  the same step (one `save_undying` call, never left on the now-dead old id).
- **Post-spawn restore delivery (P-C6).** Once a terminal candidate's spawn
  actually registers, its `restore` snapshot decides what — if anything —
  lands in the new pty, through `session_send` in-process, never a direct
  socket write. Not idle, with a foreground `argv`: re-exec it (`--yes
  --submit`) — the session was demonstrably running it when it left — EXCEPT
  when `argv[0]`'s basename is `sudo`, which is never re-exec'd unattended
  (only the cwd restores; a privileged command popping a password prompt in
  a terminal nobody is watching is not a restore). Idle, with a clean
  `typed` line: preload it (`--yes`, and permanently no `--submit`) so it
  sits in the new prompt until a human presses Enter — nothing runs without
  a keystroke. Idle with no `typed`: nothing is delivered, a terminal
  reopened at its own cwd already being the complete answer. The two
  delivery branches are never unified behind a shared boolean parameter —
  each hardcodes its own flag map, so a later refactor cannot flip the
  no-submit path into a submitting one by threading a stray `true` through
  a shared helper.
- The conductor gains a keybind invoking resurrect for the focused
  project (`conductor/src/commands.rs` — the TUI already maps keys onto
  typed commands).
- Per-project auto-resume: additive `autoResume: bool` (default false) on
  the `projects.json` entry, set by `project add --auto-resume` /
  a `project set` flag. Trigger point: the daemon's boot sweep
  (`run_boot_auto_resume`) calls this command core unconditionally for
  every `autoResume` project — no liveness check of its own; liveness is
  the bare-mode selection's per-candidate exclusion above, so one live
  terminal in a project never suppresses reviving the rest of its undying
  set.

---

## Phase plan

Serialized; each phase is one Sonnet-executor brief. "Blast radius" names
the crates whose `cargo test -p` runs the phase needs, so the orchestrator
can serialize against other cargo lanes (never `--workspace`; conduct/server
bind real sockets). Golden discipline: any command-set change updates
`cli/src/registry.rs`'s snapshot in the same commit. Every phase updates the
touched directories' `README.md`/`AGENTS.md` in the same commit, written
timeless — the page states what IS; the change record lives in the commit
message.

**P-D1 — feed primitives extraction.**
Move `Follower` + generalize the append side into `aoide_protocol::feed`
(`FeedWriter`); `secrets::watch`/`broker` consume via `pub use`/delegation.
No behavior change anywhere.
Tests: new `feed` unit tests (cap-truncate, `(dev,ino)` reopen, partial
line); the full existing secrets suite green, unmodified — it is the
acceptance suite.
Blast: `aoide-protocol`, `aoide-secrets`. Gate: none.

**P-D2 — the resident loop, socket skeleton, unit flip.**
`aoide_server::daemon::run_loop` + `serve_daemon` (bind, accept-thread,
`ping` + `subscribe` ops, framing, own events feed via `FeedWriter`);
`bin/aoided.rs` wires registry+dispatch in; tick loop with no producers
yet. `modules/nucleus/aoided.nix`: `Type=simple`, `Restart=on-failure`,
drop `RemainAfterExit` (the flip the unit's own comment reserves).
Tests: serve_daemon over a tempdir socket (ping round-trip, subscribe
receives an injected event, malformed line survives); feed file created and
capped.
Blast: `aoide-server`, `aoide-cli`. Gate: REBUILD (user-gated) + live check
that aoide-mcp/aoide-a2a stay up under the flipped unit and that a daemon
restart is transparent to a running `events tail` (the `(dev,ino)` reopen).

**P-D3 — producers + `events tail`.**
The secrets-feed mirror (Follower on `/run/aoide-secrets/events.jsonl`,
name-only re-publish), the #69 hand-edit watcher, and the `aoide events
tail` command (new command path → golden +1).
Tests: mirror parses the five secrets shapes and never copies unknown
fields wholesale; hand-edit sweep fires on an out-of-band mtime change and
not on the daemon's own write; `events tail --class` filters.
Blast: `aoide-server`, `aoide-cli`. Gate: live check on yomi (real broker
feed, park an ask, see the mirror line).

**P-D4 — the dispatch op (fourth door).**
`op:"dispatch"` → `Invocation { door: Door::Daemon }` → injected dispatch;
Outcome envelope as the final reply; interim discipline reserved (no
interim producers yet). Prove the door policy by test, not new code.
Tests: a plain command dispatches and audits `"door":"daemon"`; a CLI-only
secrets admin command returns the door-hint and mutates nothing; a gated command
returns `gated: true`; `mcp.serve`/`a2a.serve` return their non-Cli
outcomes.
Blast: `aoide-server`, `aoide-cli` (+ `aoide-secrets` tests exercised via
its own crate run). Gate: none beyond tests.

**P-D5 — the hub option.**
`Peer.hub` additive field, `aoide peer hub <name> [--clear]` (golden +1),
hub-preference in addr resolution and the inbox relay default. Document the
claude.ai → Tier-3 connector → mesh → A2A + bearer route in
`CONTRACTS.md §6` as the remote-reach statement; confirm no aoided
listener exists to document.
Depends on: the bearer lane having landed (consumes `token_authorized` +
peer tokens as-is).
Tests: hub set/move/clear idempotent + round-trip; resolution prefers the
hub only when nothing else matches; `hub` absent deserializes false.
Blast: `aoide-storage`, `aoide-client`, `aoide-cli`. Gate: none (flag
default off).

**P-D6 — graph residency.**
`aoide_client::daemon_dispatch` (connect-or-None), routing in the
session-write family + `graph reap` (today `session reap`), daemon-side roster with
startup-load/tick-reconcile/projection-write, reap in the daemon tick.
Tests: routed command round-trips through a test daemon and the projection
matches the direct-path bytes; fallback path on a dead socket is
byte-identical to today; out-of-band write is folded (newest-updatedAt) on
the next tick; reap over the socket reaps.
Blast: `aoide-client`, `aoide-conduct`, `aoide-server`, `aoide-storage`.
Widest phase — serialize hard against other cargo lanes. Gate: LIVE
(desktop mesh: conduct terminals registering through the daemon, SUPER+Q a
terminal and watch the reap, kill the daemon and watch fallback).

**P-D7 — windowed spawn + resume knowledge.**
`graph spawn --windowed` (today `spawn --windowed`; template parse pure + unit-tested; taught errors
for no-template/headless), `AgentProfile.launch`/`resume_args` for
claude/kimi/pi (pi: `resume_args: None` unless verified), hook-door
`harnessSessionId` stamp.
Tests: template split/placeholder pure tests; profile table tests; hook
payload stamps the field additively (legacy records untouched).
Blast: `aoide-protocol`, `aoide-conduct`. Gate: LIVE (a windowed spawn
opens a real terminal under the compositor; conducted + parent-autogate
verified).

**P-D8 — ledger + resurrect + auto-resume flag.**
The session ledger write at roster-exit, `graph resurrect --project`
(today `resurrect --project`; golden +1), `resumedFrom` record field + `resumed` graph edge, conductor
keybind, `projects.json` `autoResume` (default off) wired to the decided
trigger.
Tests: ledger appends exactly once per exit (clean end and reap); resurrect
mints a NEW id with `resumedFrom` set and never reuses one; a
no-`resume_args` harness is skipped with a taught message; autoResume
default-off round-trip.
Blast: `aoide-storage`, `aoide-conduct`, `aoide-conductor`, `aoide-cli`.
Gate: LIVE (resurrect a real claude session by harness id; keybind).

---

## Invariants and kill-list

Things this design must never do — each is a review-blocking violation:

1. **No aoided network listener.** Unix socket under `$XDG_RUNTIME_DIR`
   only. Remote reach is the A2A door; a TCP/vsock/abstract-namespace
   listener on aoided is out, permanently.
2. **No second event authority.** The secrets feed stays authoritative for
   secrets events; the mirror is name-only and trigger-only. No consumer
   acts on a mirror line without reconciling against the owning surface.
3. **No secret value on the bus, ever** — not in a payload, not in an
   error, not in a mirror. Name-only by construction, both ends.
4. **No new allowlist for the daemon door.** Door policy lives in the
   handlers' existing `inv.door` branches; adding a daemon-door
   permission table is the drift this rung exists to avoid.
5. **Session ids are never recycled.** Resurrection mints a new id +
   `resumedFrom`; the ledger is append-only.
6. **The rebuild stays user-gated.** The daemon proposes and surfaces;
   it never admits, and no bus event triggers a rebuild.
7. **Stage files stay the read contract.** No reader is ever required to
   speak the daemon socket; projections keep every existing consumer
   working, and all stage writes stay under `with_stage_lock` +
   atomic-write.
8. **One liveness mechanism.** The reap predicate is the only session
   condemner; the daemon relocates it, never duplicates or second-guesses
   it.
9. **No cross-crate copying.** Feed primitives move to protocol with
   shims; a second Follower implementation anywhere is a defect.
10. **Registry order is append-only; golden snapshots update in the same
    commit** as any command-set change.
11. **`lyra` never enters the summoning path**; `--windowed` is core, the
    terminal template is env/config, no nix shell-out anywhere in core.
12. **Untrusted text stays data.** Forwarded notification/app-title text on
    the bus is an opaque payload field; nothing parses it as a command
    (house rule 4).
13. **No workspace-wide cargo in any phase gate** — per-crate tests only
    (conduct/server bind real sockets; `--workspace` deadlocks locally).

---

## Risks, ranked

1. **TOP RISK — L4 dual-writer reconciliation.** Daemon memory, the
   fallback direct-write path, and the reap timer can all touch
   `sessions.json`. The design bounds this (all writers under
   `with_stage_lock`; files canonical at rest; newest-updatedAt fold each
   tick), but a reconcile bug shows up as ghost/duplicated sessions on the
   live desktop — subtle, stateful, and the reaper's own dedup logic sits
   on top. Mitigation: P-D6's projection-equals-direct-path byte test, the
   live SUPER+Q/kill-daemon drills in its gate, and shipping routing command
   family by command family rather than all at once if the executor reports
   instability.
2. **Unit-flip regression (P-D2).** The oneshot workaround exists because
   a clean exit under `Type=simple` cascaded through `BindsTo` and took
   the doors down (found live on osaka). A crash-looping first loop build
   does the same dance. Mitigation: `Restart=on-failure` + the live gate's
   explicit doors-stay-up check before anything else lands on the daemon.
3. **Feed-cap truncation racing a slow subscriber.** Truncate-in-place can
   eat unread lines for a laggy `events tail`. Accepted by design (rule 2:
   trigger, not truth) — but every consumer built on the bus must carry a
   reconcile, and a future consumer that forgets will look "randomly
   lossy". The invariant list is the guard.
4. **Hook-burst socket contention (L4).** A busy mesh fires hooks
   constantly; a slow daemon turns a 100ms connect budget into per-hook
   latency inside Claude Code's hook path. Mitigation: the budget is hard,
   fallback is silent, and the hook door's write is already
   fire-and-forget from the harness's perspective.
5. **Bearer-lane sequencing (P-D5).** The hub/routing phase consumes the
   parallel bearer work; landing order matters. Mitigation: P-D5 is
   deliberately thin and can trail.
6. **Golden/schema churn.** Three new command paths across P-D3/P-D5/P-D8;
   each is one snapshot edit in the same commit — routine, but three
   chances to forget.

---

## Open knobs

Exactly one:

- **The auto-resume trigger point** (`autoResume`, per-project, default
  off), decided at P-D8's briefing: daemon start. On `run_loop` entry (and
  only there — never on restart-within-session: guarded on "no resurrect
  already performed this boot", using the boot-epoch read the reaper
  already has, `reap.rs:472`), the daemon resurrects every `autoResume`
  project unconditionally — no liveness check at this layer. Liveness is
  handled one level down, per candidate, inside `resurrect`'s own
  bare-mode selection (durable-sessions plan P-C4): an already-alive
  undying id is dropped before anything is spawned, so a project whose
  whole undying set is already live resolves to an empty-set no-op rather
  than being skipped wholesale — a project-wide skip here would suppress
  reviving a multi-session undying set's other, actually-dead members over
  one live terminal. Daemon start is the one trigger that exists on
  headless and desktop alike, fires once per boot by construction, and
  needs no new event source.

  The daemon-start trigger carries one hard limitation, found live at
  the durable-sessions boot sweep (task #96, gate 6): a systemd user
  unit's environment freezes at spawn, before any compositor runs, so
  the daemon can never see `WAYLAND_DISPLAY` — a windowed resurrect
  from the boot trigger has no terminal to open and degrades to the
  taught no-terminal error while headless revives work fine. The
  desktop-correct trigger is a graphical-session-side unit (WantedBy
  graphical-session.target, the popup watcher's own pattern) running
  `aoide resurrect` for the auto-resume projects once the compositor
  env exists. That unit is deliberately NOT built: `autoResume` is
  per-project and default off, nobody has flipped it since the sweep,
  and the daemon trigger already covers the headless case correctly.
  Whoever flips the flag on a desktop host builds the
  graphical-session unit then, with this paragraph as the design.
