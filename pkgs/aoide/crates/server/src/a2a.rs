//! The A2A (Agent2Agent) door — a hand-rolled, dependency-free JSON-RPC 2.0
//! over HTTP/1.1 server (CONTRACTS.md §6, Phase B: server MVP, now with
//! `message/send` execution — Phase B2).
//!
//! Zero new crates: a blocking `TcpListener` accept loop (thread-per-
//! connection), a minimal HTTP/1.1 request/response layer hand-parsed off
//! `BufRead`/`Write` (the same "no clap/no tokio, offline cargo lock stays
//! pure" discipline root's `cli.rs` and `aoide-conduct`'s `graph/conduct.rs`
//! already follow), and `serde_json::Value` for the JSON-RPC envelope
//! (mirroring `mcp.rs`'s stdio JSON-RPC server — this is that same shape over
//! a socket instead of stdio).
//!
//! Routes (CONTRACTS.md §6 MVP surface, plus §7's node federation):
//!   - `GET  /.well-known/agent-card.json` — the AgentCard, derived from the
//!     command registry, filtered to `implemented: true`.
//!   - `POST /` — JSON-RPC 2.0: `tasks/get` (real), `message/send` (real —
//!     inject into a known conductable session, or spawn a freshly conducted
//!     one; [`decide_send_action`] below. Every Inject parks in
//!     `state/stage/pending.json` for a human: this door cannot approve a
//!     gated send, whatever node a caller resolves to (see [`message_send`]),
//!     `aoide/graphSummary` (real — CONTRACTS.md §7: wraps
//!     [`resolve_graph_document`] in the federation envelope; see
//!     [`graph_summary`]), anything else → `-32601 method not found`.
//!
//! A forwarded A2A message's TEXT is untrusted DATA, never executed as a
//! command. `message/send`'s inject path parks the text through
//! [`aoide_conduct::graph::session_send`], the same gated door `send` uses,
//! and a human's approval is what types it. Its spawn path never runs a
//! client-supplied command: it only ever launches the
//! operator-configured `aoide.a2a.spawnAgent` executable (a rebuild-gated nix
//! option — the user's admission), with the client-supplied prompt held in
//! `pending.json` as its first turn until a human approves it. See
//! [`decide_send_action`]'s doc comment for the full security model.
//!
//! Extracted from root `src/a2a.rs` (Phase 4c restructure,
//! docs/architecture/PACKAGE-LAYOUT.md) — this is the SERVER half only. The
//! CLIENT half (the node registry, AgentCard URL resolution, the outbound
//! request builder) stays in root/`aoide-client`/`aoide-storage`, unchanged
//! from Phase 4a/4b.
//!
//! **DI seam (the one non-mechanical part of this phase):** [`agent_card`],
//! [`route`], [`handle_connection`], and [`serve`] used to reach the
//! crate-global `dispatch::registry()`. That registry is the fully-assembled
//! command set (`commands::all()`, root-only, not moving until Phase 6
//! `cli`) — a `server` crate can't depend on it without becoming
//! `server → root`. So the registry is a parameter here instead; root
//! `lib.rs`'s `a2a serve` launch site passes `dispatch::registry()` in.
//! `handle_jsonrpc`/`message_send`/`tasks/get` etc. never needed the registry
//! (they only ever touched session state), so they're untouched.

use aoide_conduct::graph::{
    canonical_state, hold_pending, load_stage, now_iso_utc, resolve_graph_document,
    session_send, sessions_path, HoldRefused, Project, ProjectsFile, SessionRecord, SessionsFile,
    PENDING_FILE_FULL, PENDING_FULL, PENDING_TOO_LARGE,
};
use aoide_protocol::output::Status;
use aoide_protocol::registry::{Command, Registry};
use aoide_protocol::wire::{
    AgentCapabilities, AgentCard, AgentSkill, Artifact, JsonRpcResponse, Message, Part, Task,
    TaskStatus, TaskStatusUpdateEvent, FRAME_ARTIFACT_ID, FRAME_KEY, HISTORY_MESSAGE_ID,
    LINES_AFTER_KEY, OUTPUT_READ_REFUSED_CODE, TASK_NOT_FOUND_CODE,
};
use aoide_protocol::{audit, Door, EventClass, Invocation};
use aoide_storage::records::RemoteParent;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, TcpListener, TcpStream};
#[cfg(unix)]
#[cfg(windows)]
use aoide_protocol::win_unix::UnixStream;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[cfg(test)]
use aoide_conduct::graph::write_stage;
#[cfg(all(test, unix))]
use std::os::unix::net::UnixListener;
#[cfg(all(test, windows))]
use aoide_protocol::win_unix::UnixListener;

// ── Hostile-input hardening limits (security review, pre-commit) ────────────
//
// Every one of these exists because the client on the other end of the
// socket is untrusted network input, not a cooperating peer — CONTRACTS.md
// §6 draws no new trust tier for the A2A door, but that only covers the
// JSON-RPC *payload*; the HTTP framing around it still needs the same
// paranoia any Internet-facing parser needs, even bound to loopback by
// default.

/// Hard cap on a single request's body, in bytes. Without this, an untrusted
/// `Content-Length: 999999999999` would drive a ~1 TB `Vec` allocation —
/// which aborts the whole process (not just the connection) on failure.
const MAX_BODY: usize = 1 << 20; // 1 MiB

/// Hard cap on a single request-line or header-line's length, in bytes. A
/// newline-less byte stream must never grow a line buffer without bound.
const MAX_LINE: usize = 8 << 10; // 8 KiB

/// Hard cap on the number of header lines parsed before giving up.
const MAX_HEADERS: usize = 100;

/// Absolute per-connection budget. The per-read timeout below only guards
/// against a fully-idle client; a client that dribbles one byte just inside
/// that timeout, forever, would otherwise hold a handler thread indefinitely.
const MAX_REQUEST: Duration = Duration::from_secs(15);

/// Max in-flight connections. Past this, new connections get a fast `503`
/// instead of a spawned handler thread, so a connection flood can't spawn
/// unbounded threads.
const MAX_CONN: usize = 64;

/// Absolute cap on one SSE stream's lifetime (`message/stream` /
/// `tasks/resubscribe`). A never-terminal session — an idle agent that never
/// reaches `done` — must NOT hold a handler thread (and thus a [`MAX_CONN`]
/// slot) forever; on timeout the loop emits one final event and closes. The
/// [`MAX_CONN`] + [`ConnGuard`] cap already bounds CONCURRENT streams, since
/// `stream_task` runs inside the same guarded handler thread — this cap bounds
/// each individual stream's DURATION on top of that.
const MAX_STREAM: Duration = Duration::from_secs(600); // 10 minutes

/// Poll interval between task-status reads inside an SSE stream loop.
const STREAM_POLL: Duration = Duration::from_millis(750);

/// Count of currently in-flight (spawned) connection-handler threads —
/// paired with [`ConnGuard`] so the count is accurate even across a panic.
static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

// ── The H1 mail adapter's own constants ─────────────────────────────────────
//
// The adapter is a SECOND listener that is NOT the door: a process whose whole
// method set is the three mail methods ([`mail_rpc`]) plus the stripped card.
// What it may bind is therefore not a configurable property — it is the
// capability itself.

/// The taught word the mail adapter refuses plaintext with, in BOTH
/// directions (CONTRACTS.md §6): a plaintext `envelope` deposit, and a
/// plaintext entry the poll would otherwise hand over. One constant, so the
/// deposit refusal, the poll's `withheld` reason and the audit line can never
/// drift on it.
const SEALED_REQUIRED: &str = "sealed-required";

/// The taught word the two mail methods refuse an UNLOADABLE declaration with
/// (MAIL.md §Status: "no mail served with the declaration unloadable"). A
/// broken zone table means no zone check can run, and no zone check means no
/// mail — never "mail with the walls down". It is a refused RESULT, like
/// [`SEALED_REQUIRED`], and the SENDER treats it as a state of the LINK, never a
/// verdict on the letter: the entry stays live and retries on
/// the ordinary back-off. One source, in the crate both ends share
/// (`aoide_storage::charter::CONFIG_INVALID`), because the sender classifies it
/// by this word.
const CONFIG_INVALID: &str = aoide_storage::charter::CONFIG_INVALID;

/// What an unloadable declaration says ON THE WIRE: fixed text, and nothing else.
/// The load error itself names absolute paths and the config's own structure, so
/// it is this host's business and goes to the audit log only — a caller is told
/// that mail is not being served, never where this host keeps its files.
const CONFIG_INVALID_DETAIL: &str = "this host's declaration will not load";

/// The address the mail adapter binds, ever and only. There is deliberately no
/// `--bind`, no `aoide.mail.adapter.bindAddress` and no env var beside this
/// one: a TLS-terminating front (cloudflared, a VPS, a tailnet) is what faces
/// the mesh, and the A2A door is never fronted by one (`aoide.a2a.bindAddress`
/// exists for a user's own deliberate choice; this has no such choice to
/// offer).
const MAIL_ADAPTER_BIND: &str = "127.0.0.1";

/// The mail adapter's port when neither `--port` nor the env var names one.
pub const MAIL_ADAPTER_PORT_DEFAULT: u16 = 8712;

/// The env var the adapter's systemd unit sets (`AOIDE_MAIL_ADAPTER_PORT`).
const MAIL_ADAPTER_PORT_ENV: &str = "AOIDE_MAIL_ADAPTER_PORT";

/// The word the adapter's own audit details carry, so a log line can be told
/// apart from the door's without knowing which port answered it. Tunnel-borne
/// traffic arrives from loopback (the front dials this box), so the origin
/// alone would not distinguish the two listeners.
const MAIL_ADAPTER_AUDIT_TAG: &str = "mail-adapter";

// ── Bind/port resolution (CONTRACTS.md §6 security posture) ─────────────────

/// Resolve the bind address + port for `a2a serve`: `--bind`/`--port` flags →
/// `AOIDE_A2A_BIND`/`AOIDE_A2A_PORT` env (set by the `aoide-a2a` systemd unit,
/// `modules/dendrites/aoided.nix`) → the loopback defaults
/// (`aoide.a2a.bindAddress`/`aoide.a2a.port`, `127.0.0.1`/`8710`).
pub fn resolve_bind_port(inv: &Invocation) -> (String, u16) {
    let bind = inv
        .flags
        .get("bind")
        .cloned()
        .or_else(|| std::env::var("AOIDE_A2A_BIND").ok().filter(|s| !s.is_empty()))
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let port = inv
        .flags
        .get("port")
        .and_then(|p| p.parse::<u16>().ok())
        .or_else(|| {
            std::env::var("AOIDE_A2A_PORT")
                .ok()
                .and_then(|p| p.parse::<u16>().ok())
        })
        .unwrap_or(8710);
    (bind, port)
}

/// Resolve the mail adapter's port (H1): `--port` flag →
/// `AOIDE_MAIL_ADAPTER_PORT` env (set by the adapter's own systemd unit) →
/// [`MAIL_ADAPTER_PORT_DEFAULT`]. There is no bind-address counterpart —
/// [`serve_mail`] binds [`MAIL_ADAPTER_BIND`] and nothing else.
pub fn resolve_mail_adapter_port(inv: &Invocation) -> u16 {
    inv.flags
        .get("port")
        .and_then(|p| p.parse::<u16>().ok())
        .or_else(|| std::env::var(MAIL_ADAPTER_PORT_ENV).ok().and_then(|p| p.parse::<u16>().ok()))
        .unwrap_or(MAIL_ADAPTER_PORT_DEFAULT)
}

/// Resolve `aoide.a2a.spawnAgent`: the command `message/send`'s SPAWN path
/// conducts for a client that names no known session (or explicitly asks to
/// spawn). `--spawn-agent` flag → `AOIDE_A2A_SPAWN_AGENT` env (set by the
/// `aoide-a2a` systemd unit, `modules/dendrites/aoided.nix`) → default `""`
/// (empty = spawning disabled — [`decide_send_action`] returns a structured
/// error rather than launching anything). The client NEVER supplies this
/// command — only the operator, via the rebuild-gated nix option
/// (CONTRACTS.md §6, security model).
pub fn resolve_spawn_agent(inv: &Invocation) -> String {
    inv.flags
        .get("spawn-agent")
        .cloned()
        .or_else(|| std::env::var("AOIDE_A2A_SPAWN_AGENT").ok())
        .unwrap_or_default()
}

/// Resolve `aoide.a2a.spawnCwd`: the working directory `do_spawn`'s spawned
/// child is launched in, when set. `--spawn-cwd` flag → `AOIDE_A2A_SPAWN_CWD`
/// env (set by the `aoide-a2a` systemd unit) → default `""` (empty = inherit
/// the daemon's own cwd, today's behavior). Mirrors [`resolve_spawn_agent`]'s
/// exact precedence shape. The client NEVER supplies this — only the
/// operator. Unbounded by itself: [`do_spawn`] applies the value only when it
/// names a REGISTERED project root (see its own doc comment) — this function
/// just resolves the configured string, the same way [`resolve_spawn_agent`]
/// resolves a command line without validating it.
pub fn resolve_spawn_cwd(inv: &Invocation) -> String {
    inv.flags
        .get("spawn-cwd")
        .cloned()
        .or_else(|| std::env::var("AOIDE_A2A_SPAWN_CWD").ok())
        .unwrap_or_default()
}

/// Resolve `aoide.a2a.tokenFile`: the path to a file holding the shared
/// secret a caller must present (`Authorization: Bearer <token>`) to be
/// trusted as an authenticated caller (CONTRACTS.md §6 amendment,
/// required before Spawn runs at all, and the switch that
/// stops loopback standing in for a caller once it's set.
/// `--token-file` flag → `AOIDE_A2A_TOKEN_FILE` env (set by the `aoide-a2a`
/// systemd unit) → default `""` (empty = no token required — today's fully
/// open behavior, unchanged). Mirrors [`resolve_spawn_agent`]'s exact
/// precedence shape. Only a FILE PATH ever crosses a flag/env var — the
/// secret itself is read off disk once, at `a2a serve` launch
/// ([`read_expected_token`]), never passed as a flag value directly (argv is
/// world-readable via `/proc/*/cmdline`) and never logged.
pub fn resolve_token_file(inv: &Invocation) -> String {
    inv.flags
        .get("token-file")
        .cloned()
        .or_else(|| std::env::var("AOIDE_A2A_TOKEN_FILE").ok())
        .unwrap_or_default()
}

/// Read the expected A2A token off [`resolve_token_file`]'s resolved path.
/// An empty path resolves to `None` outright (feature off, no disk read at
/// all). A missing/unreadable file, or one that's empty/whitespace-only,
/// ALSO resolves to `None` rather than a hard launch failure — MVP
/// tolerance, matching the other `resolve_*` functions' soft-fallback
/// stance. Trimmed once so a trailing newline from `echo >file`/an editor
/// doesn't become part of the secret.
pub fn read_expected_token(token_file: &str) -> Option<String> {
    if token_file.is_empty() {
        return None;
    }
    std::fs::read_to_string(token_file)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// The expected token `a2a serve` carries from launch: empty only when NO
/// token file is configured. A configured file that is missing, unreadable or
/// empty yields [`resolve_failure_sentinel`], so the door stays closed rather
/// than reading as "no token configured".
pub fn launch_token(token_file: &str) -> String {
    if token_file.is_empty() {
        return String::new();
    }
    read_expected_token(token_file).unwrap_or_else(|| {
        eprintln!(
            "aoide a2a: token file `{token_file}` holds no token — refusing every bearer check (fail closed); \
             create the file with the token, then restart aoide-a2a"
        );
        resolve_failure_sentinel()
    })
}

/// Resolve `aoide.a2a.bearerSecret`: the NAME of a secret this door resolves
/// through the LOCAL secrets broker, fresh on every request, as its own
/// expected inbound bearer (task #84) — the first real machine consumer of
/// the secrets broker's unix-socket wire (`CONTRACTS.md`'s "Secrets wire"
/// section). `--bearer-secret` flag → `AOIDE_A2A_BEARER_SECRET` env →
/// default `""` (empty = not configured), mirroring
/// [`resolve_token_file`]'s exact precedence shape. When set, it TAKES
/// PRECEDENCE over the token-file mechanism above — see
/// [`resolve_inbound_bearer`] for the exact precedence and the fail-closed
/// behavior on a broker resolve failure.
pub fn resolve_bearer_secret(inv: &Invocation) -> String {
    inv.flags
        .get("bearer-secret")
        .cloned()
        .or_else(|| std::env::var("AOIDE_A2A_BEARER_SECRET").ok())
        .unwrap_or_default()
}

/// Resolve whether THIS `a2a serve` process is FORCED to advertise for
/// its whole lifetime (P-P6, `docs/architecture/PAIRING.md`'s "Discovery
/// (advertise-but-locked)" section): `--discovery-advertise` flag (bare
/// presence, no value — the same shape `--stdio`/`--all`/`--windowed`
/// already hold elsewhere in this tree) → `AOIDE_DISCOVERY_ADVERTISE` env,
/// truthy in `{1,true,yes,all}` (the exact vocabulary
/// `aoide-conduct::graph::send`'s own `AOIDE_CONDUCT_AUTOGATE` already
/// established — one truthy-env convention, not a second one invented
/// here) → **OFF by default** (PAIRING.md: "off by default" — no
/// advertisement, ever, until an operator opts in explicitly). Mirrors
/// [`resolve_spawn_agent`]/[`resolve_token_file`]'s exact
/// flag-then-env-then-default precedence shape, the idiomatic knob home
/// this door already established for every other operator-facing toggle.
/// This is the nix-declarative half of the switch; the runtime half is
/// `aoide node advertise on|off` (`aoide_storage::advertise::enabled`),
/// OR'd in per tick by the advertise thread
/// (`discovery::spawn_advertiser`), so `false` here still leaves the
/// operator one command away from advertising, no restart.
pub fn resolve_discovery_advertise(inv: &Invocation) -> bool {
    if inv.flag_present("discovery-advertise") {
        return true;
    }
    matches!(
        std::env::var("AOIDE_DISCOVERY_ADVERTISE").ok().as_deref(),
        Some("1") | Some("true") | Some("yes") | Some("all")
    )
}

/// Resolve this instance's `aoide/graphSummary` `instance.name` (CONTRACTS.md
/// §7): `--node-name` flag → `AOIDE_A2A_NODE_NAME` env (set by the
/// `aoide-a2a` systemd unit, mirroring `resolve_bind_port`/
/// `resolve_spawn_agent`'s precedence) → the OS hostname → the literal
/// `"aoide"` if even that fails. The env/hostname tail is
/// `aoide_storage::display::local_host_name` (petnames plan, P2): storage
/// has no `Invocation` to read the flag off, so this crate still resolves
/// the flag itself and only delegates the rest. Resolved once at `a2a serve`
/// launch, same as bind/port/spawn-agent.
pub fn resolve_node_name(inv: &Invocation) -> String {
    // The env/hostname tail (env var -> OS hostname -> "aoide") is delegated
    // to `aoide_storage::display::local_host_name` — the storage crate's copy
    // is byte-identical (conduct/conductor renderers need the same fallback
    // chain and cannot depend on this crate), so this resolves it once
    // instead of keeping a second copy in sync. The `--node-name` flag stays
    // here: storage has no `Invocation` to read a flag off.
    inv.flags
        .get("node-name")
        .cloned()
        .unwrap_or_else(aoide_storage::display::local_host_name)
}

// ── Where a `message/send`/`message/stream` connection originated ───────────
//
// A connection's ORIGIN (not any client-supplied field — TCP `peer_addr()`,
// which a hostile client cannot spoof from off-box) is an attribution input
// only, and the key of an unproven caller's bucket in the pending queue
// ([`inject_source`]). Nothing here delivers: every network send parks in
// `pending.json`.

/// Where one `message/send` (or `message/stream`) request's TCP connection
/// came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnOrigin {
    /// The connection's peer IP is loopback (127.0.0.0/8, `::1`). Loopback earns
    /// nothing by its address: a tunnel that ends on loopback looks local, so
    /// the address proves no identity.
    Loopback,
    /// A non-loopback peer IP. The address can name a registered node (see
    /// [`aoide_storage::node_store::resolve_node`]); it proves no identity by
    /// itself, and an inject from it is recorded under `addr:<address>`, not
    /// under that node.
    Remote(IpAddr),
    /// The peer address could not be determined (e.g. `peer_addr()` failed).
    Unknown,
}

/// Classify a raw `peer_addr()` result into a [`ConnOrigin`]. Pure.
pub fn classify_origin(node_ip: Option<IpAddr>) -> ConnOrigin {
    match node_ip {
        Some(ip) if ip.is_loopback() => ConnOrigin::Loopback,
        Some(ip) => ConnOrigin::Remote(ip),
        None => ConnOrigin::Unknown,
    }
}

/// How one connection's origin reads inside an audit detail (H1): the
/// adapter's lines name it, so a tunnel-borne request (always `loopback`, the
/// front dials this box) is distinguishable from a LAN one. Pure.
fn origin_spelling(origin: ConnOrigin) -> String {
    match origin {
        ConnOrigin::Loopback => "loopback".to_string(),
        ConnOrigin::Remote(ip) => format!("remote {ip}"),
        ConnOrigin::Unknown => "unknown".to_string(),
    }
}

/// Which listener a connection arrived at (H1). A property of the PROCESS,
/// checked in exactly one place — [`handle_connection`]'s dispatch selection —
/// so the adapter cannot grow a second, unreviewed path into the door's table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Listener {
    /// `aoide a2a serve`: the whole door (CONTRACTS.md §6).
    A2a,
    /// `aoide mail serve`: the loopback mail adapter (H1) — [`route_mail`]'s
    /// three methods and the stripped card, nothing else.
    Mail,
}

impl Listener {
    /// The audit detail one handled request leaves behind. The door's is
    /// `HTTP {status}` exactly as it always was; the adapter's names its own
    /// listener and the connection's origin on top of it.
    fn audit_detail(self, status: u16, origin: ConnOrigin) -> String {
        match self {
            Listener::A2a => format!("HTTP {status}"),
            Listener::Mail => format!("HTTP {status} from {} via {MAIL_ADAPTER_AUDIT_TAG}", origin_spelling(origin)),
        }
    }

    /// The same tag on a detail that is NOT an HTTP status — a refusal's own
    /// taught message, or an attribution-drift note. The door's bytes are
    /// unchanged; the adapter's name their listener and origin at the end, so
    /// the two EARLY paths out of `handle_connection` (a malformed request and
    /// a refused signature) are attributable to a listener as well as the
    /// routed ones are.
    fn audit_detail_with(self, detail: &str, origin: ConnOrigin) -> String {
        match self {
            Listener::A2a => detail.to_string(),
            Listener::Mail => format!("{detail} (from {} via {MAIL_ADAPTER_AUDIT_TAG})", origin_spelling(origin)),
        }
    }
}

// ── Bearer-token authentication ──────────────────────────────────────────────
//
// Behind any reverse proxy or tunnel (`ssh -R`, a tailscale funnel,
// cloudflared, nginx) the SERVER's end of the TCP connection sees the proxy's
// own loopback address for every caller, so `classify_origin` cannot tell
// "the operator, locally" from "anyone who can reach the proxy". A bearer
// token is the signal that survives a proxy hop.
//
// [`resolve_token_file`]/[`read_expected_token`] resolve the SERVER's own
// expected token ONCE at `a2a serve` launch, exactly like `spawn_agent`. It
// gates the READ arms, `message/stream` as a whole and the full AgentCard
// ([`token_authorized`]), and, once configured, it lets a caller past the
// uniform-response guard in [`message_send`] that hides whether an
// unauthenticated caller's `contextId` exists. Spawn never consults it: the
// spawn gate is [`spawn_admitted`]'s signature.
//
// Per-node bearers (a node's own `token_file`) are not this token. They are
// matched by [`aoide_storage::node_store::resolve_node`]'s Token rung.

/// The outcome of comparing a presented `Authorization: Bearer <token>`
/// against the server's configured expected token. Pure — no I/O; the token
/// VALUES are already resolved by the time this runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenState {
    /// No `Authorization: Bearer` header was presented at all.
    Absent,
    /// A token was presented and matches the expected token exactly.
    Valid,
    /// A token was presented but does not match.
    Invalid,
}

/// Classify a presented token against the expected one. Only meaningful when
/// a token IS configured (`expected` non-empty) — callers gate on that
/// separately ([`resolve_token_file`]'s empty-means-off convention) rather
/// than folding "not configured" into this enum, so `TokenState` stays a
/// three-way fact about ONE comparison, not a second copy of the
/// feature-on/off switch. Uses [`aoide_storage::node_store::token_bytes_eq`]
/// (length-independent byte compare) rather than `==` on a secret. Pure.
fn classify_token(expected: &str, presented: Option<&str>) -> TokenState {
    match presented {
        None => TokenState::Absent,
        Some(p) if aoide_storage::node_store::token_bytes_eq(expected, p) => TokenState::Valid,
        Some(_) => TokenState::Invalid,
    }
}

/// Is a token-gated action allowed to run? When no token is configured,
/// ALWAYS yes — the off-path is byte-identical to before the token amendment
/// (admission stays rebuild-time-only, exactly CONTRACTS.md §6's original
/// security model). When a token IS configured, only a [`TokenState::Valid`]
/// bearer unlocks it — an absent or wrong token is a clean [`unauthorized`]
/// `-32005` error, not a silent fallback to the old open behavior. Pure —
/// directly testable without a socket or a spawned process.
///
/// It guards the READ commands (`tasks/get`, `aoide/graphSummary`,
/// `tasks/resubscribe`), `message/stream` as a whole, and the full AgentCard
/// (CONTRACTS.md §6, "Read and card gating"). An ungated read is a
/// whole-session-graph leak the moment the door faces a network. The SPAWN arm
/// of `message/send` never consults it: [`spawn_admitted`] is the spawn
/// gate.
fn token_authorized(token_configured: bool, token_state: TokenState) -> bool {
    !token_configured || token_state == TokenState::Valid
}

/// May this caller use a READ arm? The bearer gate ([`token_authorized`]) OR a
/// verified signature whose grant in the request's mesh holds `read`. The
/// bearer exists because UNSIGNED callers, who
/// behind a tunnel or proxy all look loopback; a verified signature plus a
/// grant is a stronger credential than the bearer, so it is governed by the
/// grant alone. Writes never reach this predicate: spawn and inject keep their
/// own signature and grant gates. Pure.
fn read_admitted(bearer_ok: bool, caller: Option<SignedCaller<'_>>) -> bool {
    bearer_ok || caller_grant(caller).holds("read")
}

/// The `-32005` unauthorized error every token-gated arm returns, so the code
/// and message never drift between the read gates.
fn unauthorized() -> (i64, String) {
    (-32005, "unauthorized: a valid A2A token is required".to_string())
}

// ── AgentCard (derived from the command registry, CONTRACTS.md §6) ──────────

/// Build the AgentCard from any iterator of registry commands — factored out
/// of [`agent_card`] so tests can feed a small fake schema instead of a real
/// registry.
pub fn agent_card_from_commands<'a>(
    commands: impl Iterator<Item = &'a Command>,
    bind: &str,
    port: u16,
) -> Value {
    let mut skills: Vec<AgentSkill> = commands
        .filter(|c| c.implemented && c.opens(Door::A2a))
        .map(|c| {
            let dotted = c.dotted();
            let top_level = c.path.first().copied().unwrap_or("");
            AgentSkill {
                id: dotted.clone(),
                name: dotted,
                description: c.summary.to_string(),
                tags: vec![top_level.to_string()],
            }
        })
        .collect();
    // A2A runs no registry command, so no command opens to its door. The two
    // things `message/send` reaches are described here by hand instead: the
    // session inject and the harness spawn, neither through the registry.
    for (name, description) in [
        ("send", "Inject a message into a conducted session. It is held for approval unless the caller is that session's attested parent or sibling."),
        ("spawn", "Start a conducted agent session from the configured harness, with the message as its first prompt."),
    ] {
        skills.push(AgentSkill {
            id: name.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            tags: vec![name.to_string()],
        });
    }

    let card = AgentCard {
        name: Some("aoide".to_string()),
        description: "aoide — a headless conductor for agent sessions, rice \
            generation, and the state/stage tree, exposed as a \
            discoverable A2A remote agent (CONTRACTS.md §6)."
            .to_string(),
        version: Some(aoide_protocol::registry::AOIDE_VERSION.to_string()),
        // Pinned explicitly to the A2A v0.3.x JSON-RPC binding (CONTRACTS.md
        // §6 "Version"): flat "url" below, message/send + tasks/get,
        // lowercase-kebab TaskStates. v1.0's `interfaces`-array + top-level
        // `id` card form is a later, additive follow-on — not this.
        protocol_version: Some("0.3.0".to_string()),
        url: Some(self_url(bind, port)),
        // Phase C: the server now serves `message/stream` + `tasks/resubscribe`
        // over Server-Sent Events, so streaming is advertised true.
        capabilities: Some(AgentCapabilities { streaming: true }),
        default_input_modes: Some(vec!["text/plain".to_string()]),
        default_output_modes: Some(vec!["text/plain".to_string()]),
        skills: Some(skills),
        interfaces: None,
    };
    serde_json::to_value(&card).expect("AgentCard always serializes")
}

/// The real AgentCard, derived from an INJECTED registry rather than a
/// crate-global singleton — see the module doc comment's "DI seam" note.
pub fn agent_card(registry: &Registry, bind: &str, port: u16) -> Value {
    agent_card_from_commands(registry.commands(), bind, port)
}

/// The stripped AgentCard served to an unauthenticated GET once a server
/// token is configured (CONTRACTS.md §6, 2026-08-20 amendment): just enough
/// for a caller to identify and register the agent — `name`,
/// `protocolVersion`, `url` — with no skills inventory, `version`, or
/// `capabilities`. Each field is PICKED OFF the full card `Value` rather than
/// re-derived, so the stripped shape can never drift from what
/// [`agent_card_from_commands`] actually emits. Pure.
fn stripped_card(full: &Value) -> Value {
    let mut out = serde_json::Map::new();
    for key in ["name", "protocolVersion", "url"] {
        if let Some(v) = full.get(key) {
            out.insert(key.to_string(), v.clone());
        }
    }
    Value::Object(out)
}

// ── canonical_state → A2A TaskState mapping (CONTRACTS.md §6) ───────────────

/// `canonical_state` (`aoide_conduct::graph`) → A2A `TaskState`, JSON-RPC/HTTP
/// binding spelling (lowercase-kebab):
///
/// | canonical_state       | TaskState         |
/// | ---------------------- | ----------------- |
/// | `working`              | `working`         |
/// | `stopped`               | `completed` (the TURN ended, not the session — CONTRACTS.md §6) |
/// | `awaiting` (no needsSudo) | `input-required` |
/// | `awaiting` + `needsSudo`  | `auth-required` (precedence: needsSudo is a signal alongside state, not a state of its own) |
/// | `idle`                  | `submitted` (aoide's at-rest/cold state — acknowledged but not actively processing; NOT `working`, which is reserved for the active-turn case above) |
/// | `done`                  | `completed` (MVP simplification — CONTRACTS.md §6 flags the richer terminal vocabulary, CANCELED/REJECTED, as unresolved in v0; FAILED is now produced — see the dead-session row below) |
/// | *(dead session, any of the above)* | `failed` — read-time override (task #33): `aoide_conduct::reap::is_session_dead` resolving true for the session overrides whatever the row above would have said, regardless of its last WRITTEN state; see [`a2a_task_state_checked`] |
pub fn a2a_task_state(canonical: &str, needs_sudo: bool) -> &'static str {
    match canonical {
        "working" => "working",
        "stopped" => "completed",
        "awaiting" => {
            if needs_sudo {
                "auth-required"
            } else {
                "input-required"
            }
        }
        "idle" => "submitted",
        "done" => "completed",
        // canonical_state's own vocabulary is closed to the five states
        // above; a sensible default rather than a panic if it ever grows.
        _ => "working",
    }
}

/// The read-time override task #33 adds beside [`a2a_task_state`]: a session
/// `is_session_dead` (`aoide_conduct::reap` — the reaper's own, sole liveness
/// authority) resolves DEAD reads `failed` regardless of its last WRITTEN
/// state. A session that dies between two polls is not still `submitted`
/// just because nothing has swept a `done` over it yet — the reaper stays
/// the only record MUTATOR; this only changes what a read reports.
///
/// Pure: `dead` is the caller's already-resolved verdict (`task_from_sessions`
/// probes `is_session_dead` at the edge and feeds the bool in here), so this
/// stays a fold, never a second liveness predicate. `dead == false` is
/// BYTE-IDENTICAL to [`a2a_task_state`] alone — this can only override that
/// function's answer, never narrow it.
///
/// Applies uniformly to every resolved session, spawned or not: a human
/// terminal SUPER+Q'd mid-poll is exactly as dead as an A2A-spawned agent
/// whose process died after its ack, and `exempt`/`undying` (which veto
/// REAPING, not truth-telling) never enter into `is_session_dead`'s window-
/// or pid-gone signals, so a dead exempt session reads `failed` too.
pub fn a2a_task_state_checked(dead: bool, canonical: &str, needs_sudo: bool) -> &'static str {
    if dead {
        "failed"
    } else {
        a2a_task_state(canonical, needs_sudo)
    }
}

// ── JSON-RPC 2.0 method routing ──────────────────────────────────────────────

/// `tasks/get id:<sessionId>` — CONTRACTS.md §6 MVP simplification: the A2A
/// Task id and its contextId are BOTH the aoide sessionId (Task=turn vs
/// contextId=session is the real shape; this phase has no multi-task-per-
/// session tracking yet — `tasks/get` always reports the session's CURRENT
/// state, not a specific past turn). TODO(a2a-b3+): splitting Task id from
/// contextId for real per-turn tracking (so a session with several
/// in-flight/completed turns exposes each as its own Task) is still future
/// work — `message/send` (Phase B2) landed the inject/spawn execution
/// semantics but kept this MVP id-collapse.
fn task_from_sessions(sessions: &[SessionRecord], id: &str) -> Result<Value, (i64, String)> {
    Ok(serde_json::to_value(&build_task(sessions, id)?).expect("Task always serializes"))
}

/// The envelope itself, before serialization — [`task_from_sessions`]'s body,
/// split out so the watch-frame arm (`tasks/get` with `aoide/frame`) can set
/// `artifacts` on the SAME status read rather than build a second shape that
/// could drift from it.
fn build_task(sessions: &[SessionRecord], id: &str) -> Result<Task, (i64, String)> {
    let rec = sessions
        .iter()
        .find(|s| s.session_id == id)
        .ok_or_else(|| (-32001_i64, "task not found".to_string()))?;
    let canonical = canonical_state(&rec.state);
    let needs_sudo = rec.needs_sudo.unwrap_or(false);
    // Read-time liveness probe (task #33) — the SAME predicate `reap.rs`
    // sweeps the roster with, fed live here rather than waiting for the
    // next sweep pass to write `done` over a session that already died.
    // No hyprctl round trip on this read path: `live_addresses`/
    // `window_owners` pass `None` (`is_session_dead`'s own "compositor
    // unqueried" degrade — the window signal never fires, never a false
    // dead) and `last_seen` returns `None` for the same reason (absence of
    // staleness evidence is never staleness). That leaves exactly the
    // pid-gone signal live here off a REAL `/proc` probe — which is the one
    // that fires for the case #33 exists to catch: a spawned process that
    // died after its fast ack.
    let now_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let dead = aoide_conduct::reap::is_session_dead(rec, aoide_conduct::reap::proc_exists, now_epoch, |_| None);
    let state = a2a_task_state_checked(dead, canonical, needs_sudo);
    Ok(Task {
        id: rec.session_id.clone(),
        // MVP simplification: task id == sessionId, contextId == sessionId —
        // see the doc comment above.
        context_id: rec.session_id.clone(),
        status: TaskStatus { state: state.to_string(), timestamp: now_iso_utc(), message: opening_turn_message(rec) },
        kind: "task".to_string(),
        artifacts: None,
        history: None,
    })
}

/// How many opening-turn workers may be WAITING at once, across every spawn
/// this door is serving. The wait is the expensive part (up to `READY_BUDGET`
/// plus the socket retry) and it OUTLIVES the RPC that started it, so
/// `MAX_CONN` no longer bounds it — the connection slot is released when the
/// handler returns (branch re-review N2: the comment here used to claim that
/// slot WAS the cap, which stopped being true the moment the work moved off
/// the handler). This is that bound: a small pool, because a spawn-granted
/// peer looping `message/send` against a never-ready `spawnAgent` would
/// otherwise park one 20s thread (plus its real agent process) per request
/// with no backpressure at all. Past the cap the opening turn is reported
/// `busy` — an honest word the caller can retry on, never a silent drop.
const OPENING_TURN_WORKERS_MAX: usize = 8;
static OPENING_TURN_WORKERS: AtomicUsize = AtomicUsize::new(0);

/// One slot in that pool, released when the worker holding it ends — the guard
/// moves into the thread, so a panic or an early return still frees it.
struct OpeningTurnSlot;

impl OpeningTurnSlot {
    fn acquire() -> Option<OpeningTurnSlot> {
        let mut current = OPENING_TURN_WORKERS.load(Ordering::Acquire);
        loop {
            if current >= OPENING_TURN_WORKERS_MAX {
                return None;
            }
            match OPENING_TURN_WORKERS.compare_exchange_weak(
                current,
                current + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Some(OpeningTurnSlot),
                Err(observed) => current = observed,
            }
        }
    }
}

impl Drop for OpeningTurnSlot {
    fn drop(&mut self) {
        OPENING_TURN_WORKERS.fetch_sub(1, Ordering::AcqRel);
    }
}

/// The opening-turn worker's whole life, in ONE function so its write order is
/// a property of the code rather than of a closure's shape: stamp `pending`,
/// hold the turn for a human, stamp the verdict, audit it. `pending` and the
/// verdict therefore come from the SAME thread in that order — nothing can
/// land a verdict first and then be clobbered back to `pending` (branch
/// re-review 2's L was a second writer of `pending`: the door's registration
/// wait stamped it from its own thread, so a verdict that landed first was
/// overwritten, and a verdict written before registration left a stranded
/// `pending` behind). `slot` is held for this whole life, so the pool counts
/// real waits.
fn opening_turn_worker(
    id: String,
    prompt: String,
    audit_log: PathBuf,
    slot: OpeningTurnSlot,
    from: Option<String>,
) {
    let _slot = slot;
    // The door's ack-path stamp can miss a record that registered after it;
    // this one closes that window. A no-op when the value is already `pending`,
    // and — being this thread's first write — always before the verdict below.
    aoide_conduct::graph::stamp_opening_turn(&id, "pending");
    let word = if prompt.is_empty() {
        "skipped-empty"
    } else {
        hold_opening_turn(&id, &prompt, from.as_deref(), &audit_log)
    };
    aoide_conduct::graph::stamp_opening_turn(&id, word);
    let _ = audit(
        &audit_log,
        Door::A2a,
        EventClass::Audit,
        "a2a.message/send",
        word,
        &format!("opening turn for `{id}`: {word}"),
    );
}

/// The opening turn of a spawn this network door started, held rather than
/// typed. The door cannot approve (`Door::can_approve`), so the prompt is queued
/// for a human exactly like a send that did not clear the gate. Nothing is
/// typed into the new session, and the verdict says so: `queue-full` when the
/// node already holds the queue's per-source cap or the file's whole bound,
/// `hold-failed` when the write itself failed or the prompt is over the queue's
/// per-entry byte cap.
fn hold_opening_turn(id: &str, prompt: &str, from: Option<&str>, audit_log: &Path) -> &'static str {
    match hold_pending(id, prompt, true, from, audit_log) {
        Ok(()) => "held-pending",
        Err(HoldRefused::Full | HoldRefused::FileFull) => "queue-full",
        Err(HoldRefused::Stage(_) | HoldRefused::TooLarge(_)) => "hold-failed",
    }
}

/// One `status.message` for an opening-turn verdict — the A2A `Message` shape
/// this binding types the field as, built in one place so the ack
/// (`pending`), the record's own verdict and any future reader cannot drift.
fn opening_turn_status(id: &str, word: &str) -> Message {
    Message {
        role: "agent".to_string(),
        parts: vec![Part {
            kind: "text".to_string(),
            text: Some(format!("opening turn: {word}")),
            extra: Default::default(),
        }],
        message_id: Some(aoide_protocol::wire::gen_message_id()),
        context_id: Some(id.to_string()),
        metadata: None,
    }
}

/// What became of the opening turn, as the task's `status.message`, off the
/// record the worker stamped. `None` for every record that carries no
/// `openingTurn` (a local spawn, an inject into an existing session, a legacy
/// record), so those tasks stay byte-identical.
fn opening_turn_message(rec: &SessionRecord) -> Option<Message> {
    rec.opening_turn
        .as_deref()
        .map(|word| opening_turn_status(&rec.session_id, word))
}

/// Load `sessions.json` off the stage and resolve one task by id.
fn task_get(task_id: &str) -> Result<Value, (i64, String)> {
    let path = sessions_path();
    let sf: SessionsFile =
        load_stage(&path).map_err(|e| (-32603_i64, format!("internal error: {e}")))?;
    task_from_sessions(&sf.sessions, task_id)
}

// ── `tasks/get` + `metadata["aoide/frame"]`: the watch frame ────────────────
//
// CONTRACTS.md §6 (P-RSA S6). The A2A extension that puts a session's watch
// frame — the same frame `aoide session watch --snapshot` prints — on the
// wire as ONE `data` artifact, behind [`output_read_admitted`]. Nothing else
// about `tasks/get` moves: a request that does not ask for a frame is
// answered by [`task_get`] exactly as before, and the two optional fields the
// envelope grew (`artifacts`/`history`) are omitted when absent.

/// The most output lines a frame request may ask for. The floor is 1 (a
/// request for none still gets the last line — a frame with no output at all
/// says nothing), the ceiling is this: a wire bound, not a UI preference.
const FRAME_TAIL_MAX: u64 = 200;
/// `session watch`'s own default window, for a request that names no `tail`.
const FRAME_TAIL_DEFAULT: u64 = 50;
/// How many lines of ONE letter's body a frame may carry — `clean_block`
/// keeps up to `BLOCK_LINES_MAX` (400), which is a LOCAL render bound; the
/// wire's own is this.
const LETTER_BODY_LINES_MAX: usize = 40;
/// The serialized frame's own bound, in bytes.
const FRAME_MAX_BYTES: usize = 256 * 1024;
/// What a refused output read says — ONE text for every refusal, and it says
/// NOTHING about the session asked for: the same message whether the id
/// exists or not, so the read gate is not an existence oracle beyond the
/// status `tasks/get` already reveals. The CODE it answers with is
/// `OUTPUT_READ_REFUSED_CODE`, spelled once in `aoide_protocol::wire::a2a`
/// for the door AND its readers: this arm's own number, never `-32007`, which
/// stays [`verify_signed_request`]'s incomplete-headers/signature-mismatch
/// family — decided BEFORE this arm runs at all — so the code alone tells a
/// refused read from a refused signature without matching prose.
const OUTPUT_READ_REFUSED: &str = "output read refused: reading a session's output needs a signed, \
     verified node whose allows include `read` on this host";

/// What a refused ping-back HISTORY read says (CONTRACTS.md §6, P-RSA S8).
/// The same code as [`OUTPUT_READ_REFUSED`] — §4.4 of the lane brief names no
/// code of its own for this arm, and both refusals are the same family: an
/// output read this caller is not admitted to. The TEXT is its own, because
/// the reason is not the same and an operator deserves to read which one it
/// was: this caller may well hold `read`, and still not be the parent this
/// child was spawned for. Like every refusal here it says NOTHING about the
/// session asked for — the same words whether the id exists or not — so the
/// gate stays no existence oracle.
const HISTORY_READ_REFUSED: &str = "output read refused: a session's ping-back history is readable \
     only with `read` on this host AND the key this node stamped for the parent that spawned the \
     child — on its record, or on the ring entry that outlives it";
/// The caller's capabilities IN ONE MESH — the door's only grant shape, and
/// the thing every policy site reads. Built by [`grant_in_mesh`] and by
/// nothing else: no arm scans a registry itself, so "what may this caller do
/// here" has one answer and one implementation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Grant {
    caps: std::collections::BTreeSet<String>,
}

impl Grant {
    /// Does this grant hold `cap`? The ONE predicate every gated arm asks.
    fn holds(&self, cap: &str) -> bool {
        self.caps.contains(cap)
    }

    /// Nothing granted: the answer for a caller that resolved to no record,
    /// and for a caller whose record holds nothing in this mesh. Not an
    /// error: an unknown caller and a known-but-ungranted one are
    /// indistinguishable from outside by construction (`Grant` carries no
    /// "why"), the same no-existence-oracle shape
    /// [`verify_signed_request`]'s single refusal keeps.
    fn none() -> Self {
        Grant::default()
    }
}

/// **The ONE grant lookup in this door.** The caller's capabilities in the
/// mesh its request acts in, and in no other mesh: `caller_key` is the stored
/// public key the request's signature verified against (#63 P-ID5 — identity
/// is the key, so a name that follows a rename can never widen or lose a
/// grant), and `named` is [`SignedCaller::mesh`], the mesh the request itself
/// named, or `None` when it named none.
///
/// **Two sources, chosen at this ONE place** (P-CHARTER,
/// `docs/architecture/HTTPS-MESH-API.md` "Trust per mesh"):
///
/// ```text
/// the request's mesh = effective_mesh(what it named)
///   (naming no mesh RESOLVES to [pairing] homeMesh — it is not a second
///    case, and it is judged by that mesh's rules exactly like a request
///    that named it)
///        │
///        ▼
///   does a charter GOVERN that mesh at this node?
///     no  ──────> is the mesh CHARTER-SHAPED (a charter was accepted for it)?
///                   no  ──────> that mesh's PAIRED RECORDS (the ordinary pair mesh)
///                   yes ──────> NOTHING (an undecidable operator key fails closed)
///     yes ──────> the charter line for caller_key (by KEY, never by
///                 name), MINUS this box's own `node allow … off --mesh`
///                 refusals — and NOTHING when the key is not on the
///                 line, whatever paired records it may also hold
/// ```
///
/// The subtraction is the design's "Local narrowing only": a node's own `aoide
/// node allow <node> <cap> off --mesh <m>` narrows what its door grants in
/// that mesh and wins over the charter, and nothing local widens a charter
/// grant (a local `on` for a capability the line does not grant is refused at
/// the write — `node_store::AllowError::WidensCharter`). It is read from
/// [`aoide_storage::node_store::Node::refused`] and never from
/// `Node::grants`: in a charter mesh a paired record's grant is INERT — the
/// line is the whole grant — which is what makes a REMOVED line (revocation)
/// refuse on the very next request whatever this box happened to pair with
/// that key before.
///
/// Reads the registry and the charter fresh on every call (never cached:
/// revoking a grant — locally, or by publishing a new charter version — takes
/// effect on the very next request, the same stance `bearerSecret` holds).
/// Twin records sharing one stored pubkey — a hand-edited registry only, and
/// `verify_signed_request` already requires an exact-name header to resolve
/// them — answer in REGISTRY ORDER, the tie-break CONTRACTS.md §7 already
/// documents for `resolve_node`'s ladder.
///
/// **A config that will not load grants NOTHING** (review F6, door-wide): the
/// unnamed case's mesh is the CONFIG's to name ([`effective_mesh`]), and this
/// function takes [`Grant::none`] rather than letting a guessed default mesh
/// answer from its paired records. A NAMED mesh is unaffected — the name is
/// the answer, and no config line decides a pair mesh's rules.
fn grant_in_mesh(named: Option<&str>, caller_key: &str) -> Grant {
    let nodes = aoide_storage::node_store::load_nodes();
    // **One mesh, resolved once** (review N1): a request that named none is
    // judged by the rules of `effective_mesh(None)` — the home mesh — and every
    // read below uses THAT mesh. Before this, the unnamed case skipped both the
    // governing lookup and the shaped test and went straight to the paired
    // records, so a pre-charter peer whose key a charter had since REMOVED was
    // still admitted by its stale `grants[home]` — the pre-charter fallback F2
    // exists to remove, reachable by omitting one header.
    let mesh = match effective_mesh(named) {
        Ok(mesh) => mesh,
        // **An unreadable config grants nothing** (the door-wide half of review
        // F6): `effective_mesh` will not guess a mesh for a request that named
        // none, so there is no mesh whose rules could grant — and the pre-image
        // behaviour (the built-in default, whose paired records then answer) is
        // exactly the stale-`grants[home]` door N1/F2 closed for a CHARTER home,
        // reachable by breaking one file. `Grant::none()` and not an error: a
        // caller and an ungranted caller are indistinguishable from outside by
        // construction (`Grant` carries no "why"), and the arms that OWE the
        // operator a reason build it from the same `Err` in
        // [`mesh_or_refusal`]. A NAMED mesh is untouched: its name is its own
        // answer, and the config plays no part in a pair mesh's rules.
        Err(_) => return Grant::none(),
    };
    let governing = aoide_storage::charter::governing(&mesh);
    // F2: a mesh a charter was accepted for is a charter mesh even while its
    // operator key is undecidable, and a charter mesh never falls back to the
    // pre-charter source — that fallback is how a revoked or unlisted key got
    // back in through a stale pairing.
    let shaped = aoide_storage::charter::charter_shaped(&mesh);
    grant_from(&nodes, governing.as_ref(), shaped, &mesh, caller_key)
}

/// [`grant_in_mesh`]'s pure core — the whole table without the disk reads, so
/// both branches are provable against fixtures. `mesh` is the RESOLVED mesh
/// ([`effective_mesh`], already applied by the caller — the unnamed case never
/// reaches here unresolved). `governing` is the charter that governs it
/// ([`aoide_storage::charter::governing`]), or `None` for a pair mesh and for
/// a charter-shaped mesh whose operator key is undecidable.
///
/// The charter branch answers with the caller's LINE, keyed by `caller_key`
/// (`Charter::grant_for_key`) — never the paired records, whatever this box
/// has paired, which is what "a paired record in a charter mesh is inert"
/// means in code. The refusals come from every same-key record's
/// [`aoide_storage::node_store::Node::refused`] entry for that mesh, because
/// two records with one key are one identity and a local refusal is per
/// identity, exactly as `paired_grant`'s union is.
fn grant_from(
    nodes: &[aoide_storage::node_store::Node],
    governing: Option<&aoide_storage::charter::Charter>,
    shaped: bool,
    mesh: &str,
    caller_key: &str,
) -> Grant {
    let Some(charter) = governing else {
        // **Charter-shaped with an undecidable operator key fails CLOSED**
        // (review F2): the mesh's trust is a charter's, and a charter that
        // cannot be honoured right now grants nothing — never the paired
        // records, which is what a revoked key would come back through. The
        // door tells the operator so in the refusal (`deposit_refusal`) and
        // `aoide mesh` reports the mesh as charter-shaped with `trusted:
        // false`.
        if shaped {
            return Grant::none();
        }
        return paired_grant(nodes, mesh, caller_key);
    };
    let caps = charter.grant_for_key(caller_key).unwrap_or(&[]);
    let refused = nodes
        .iter()
        .filter(|n| n.pubkey.as_deref().is_some_and(|k| !k.is_empty() && k.eq_ignore_ascii_case(caller_key)))
        .flat_map(|n| n.refused(&charter.mesh).iter().cloned())
        .collect::<std::collections::BTreeSet<_>>();
    Grant {
        caps: caps.iter().filter(|cap| !refused.contains(*cap)).cloned().collect(),
    }
}

/// [`grant_in_mesh`]'s pure core — the same answer without the disk read, so
/// the whole table is provable against fixtures.
///
/// **The answer is the UNION across every same-key record in this mesh**
/// (review finding 6; CONTRACTS.md §6, word for word: "a key's effective
/// grant set is the UNION across every record sharing it — revoking a
/// capability from a key means revoking it on EVERY such record, or `node
/// remove`-ing the duplicates"). Twin records exist because
/// `upsert_paired_node` matches by name alone, so one remote instance paired
/// under two names yields two records with one key; `verify_signed_request`
/// then resolves the IDENTITY by exact name, and this resolves the GRANT.
/// Taking the first record in registry order would let a capability the
/// contract grants vanish, and would make a revocation on the non-first twin
/// a silent no-op — so the union is the only answer that keeps the two halves
/// consistent. A revocation therefore has to be applied to every twin: that
/// is the contract's own instruction, not an omission here.
fn paired_grant(nodes: &[aoide_storage::node_store::Node], mesh: &str, caller_key: &str) -> Grant {
    let caps = nodes
        .iter()
        .filter(|n| n.verified)
        .filter(|n| n.pubkey.as_deref().is_some_and(|k| !k.is_empty() && k.eq_ignore_ascii_case(caller_key)))
        .flat_map(|n| n.grant(mesh).iter().cloned())
        .collect();
    Grant { caps }
}

/// The mesh a request acts in: the one it NAMED, or the home mesh
/// ([`aoide_storage::config::home_mesh`], `[pairing] homeMesh`, default
/// `home`) when it named none. The single place the unnamed case becomes a
/// mesh name.
///
/// **It is a RESOLUTION, not a hint** (review N1): whatever this returns is
/// the mesh whose rules decide the request — governing charter, shaped test
/// and paired fallback all read it, and `grant_in_mesh` reads it FIRST so the
/// unnamed case cannot skip any of them. A pre-P-CHARTER peer that names no
/// mesh is therefore judged by the home mesh's rules like anyone else naming
/// it: where home is a pair mesh its migrated grant is read exactly as before,
/// and where home has a CHARTER the charter is the only trust — its key must
/// be on the line, whatever its stale `grants[home]` says. "Evaluated in the
/// home mesh" was always the doctrine; it used to be true of the grants and
/// false of the charter.
///
/// Deliberately takes the NAMED value rather than a [`SignedCaller`]: the
/// mail arm compares a request's mesh against a container's, and a container's
/// mesh can be checked for a caller that resolved through no signature at all.
/// An empty string is the same as absent — a container minted before the mesh
/// was carried (`seal::seal_envelope`'s callers pass the envelope's own
/// `origin_mesh`, still `""` at P-SEAL) says "unnamed", not "a mesh called
/// nothing".
///
/// **`Err` is the unnamed case with an unreadable config** (the door-wide half
/// of review F6). A request that NAMED a mesh is answered from its own name
/// and never reads the config at all; a request that named none is answered by
/// `[pairing] homeMesh` — and when `config.toml` cannot be read, what that
/// mesh IS has no answer. [`aoide_storage::config::home_mesh`] would answer the
/// BUILT-IN DEFAULT there, which is how a charter-governed box gets judged by a
/// pair mesh's rules and how a revoked key comes back through a stale
/// `grants[home]` — the one fallback this signature exists to remove. Callers
/// that read a GRANT take [`Grant::none`] on `Err` ([`grant_in_mesh`]); callers
/// that only need the mesh for a refusal's words name the unreadable config
/// ([`mesh_or_refusal`]).
fn effective_mesh(named: Option<&str>) -> Result<String, aoide_storage::config::LoadError> {
    match named.map(str::trim).filter(|m| !m.is_empty()) {
        Some(m) => Ok(m.to_ascii_lowercase()),
        None => aoide_storage::config::home_mesh_fallible(),
    }
}

/// **The refusal an unreadable config earns at a gated arm**, or the resolved
/// mesh when the config reads — [`effective_mesh`]'s `Err` arm, audited and
/// phrased in ONE place so every mesh-naming refusal answers an unreadable
/// config the same way. The reason names the file and the parse error
/// ([`aoide_storage::config::LoadError`]'s own Display does), because the
/// operator's fix is in that file or in the request.
///
/// `-32010` is the family this belongs to: nothing is granted, exactly as
/// `charter_refusal`'s undecidable-operator arm answers — the difference is
/// only WHICH piece of host state could not be read.
fn mesh_or_refusal(
    what: &str,
    named: Option<&str>,
    label: &str,
    audit_log: &Path,
) -> Result<String, (i64, String)> {
    effective_mesh(named).map_err(|err| {
        let msg = format!(
            "{what}: this request names no mesh, so it acts in this host's home mesh \
             (`[pairing] homeMesh`) — and the config that names it cannot be read ({err}). NOTHING is \
             granted until it loads: the built-in default is a GUESS at a mesh name, and a guess would \
             judge this request by rules nobody named (in the shape that matters, a pair mesh's paired \
             records instead of the charter that governs home). Fix `{}` and retry, or name the mesh \
             in the request",
            err.path().display()
        );
        let _ = audit(audit_log, Door::A2a, EventClass::Audit, label, "unauthorized", &msg);
        (-32010, msg)
    })
}

/// The caller's grant in the mesh its request acts in — the ONE place a call
/// site turns a [`SignedCaller`] into a [`Grant`]. [`Grant::none`] when there
/// is no caller at all (no signature headers, or a weaker rung, which
/// produces no `SignedCaller` by construction); a request that named no mesh
/// resolves through [`effective_mesh`] to the home mesh and is then judged by
/// THAT mesh's rules — charter first, paired records only where no charter is
/// shaped for it (review N1).
fn caller_grant(caller: Option<SignedCaller<'_>>) -> Grant {
    match caller {
        Some(c) => grant_in_mesh(c.mesh, c.key),
        None => Grant::none(),
    }
}

/// The output-read gate (CONTRACTS.md §6, P-RSA S6). True only when both
/// hold:
/// - `read_ok` — the door's read gate ([`read_admitted`]), the same one every
///   other read arm carries. With no token configured it is true for
///   everyone; that is exactly why this predicate exists;
/// - the caller's grant IN THE MESH ITS REQUEST NAMES holds `read` — read
///   through [`grant_in_mesh`], the door's one lookup, and reached only
///   through a [`SignedCaller`], which exists for the SIGNATURE rung and for
///   nothing else (a bare address or token match carries no proof of
///   possession and produces no caller at all). The old name-based "this
///   record is verified with `read` in its `allows`" is the same question
///   with the mesh added: the grant is per mesh now, and this arm reads the
///   request's mesh and no other.
///
/// **Wider than the Inject arm on purpose** (User ruling, 2026-09-25): a
/// signed, verified node holding `read` may read ANY session's frame on this
/// host. The remote-parent key match is NOT required to read — reading is
/// wider than writing, and steering a child without pending (S5) or pulling
/// its ping-back history still needs the key. Pure, so the whole table is
/// provable without a socket, a stage file or a live registry entry.
fn output_read_admitted(read_ok: bool, caller: Option<SignedCaller<'_>>) -> bool {
    read_ok && caller_grant(caller).holds("read")
}

/// The ping-back HISTORY gate (CONTRACTS.md §6, P-RSA S8/S9): the output gate
/// AND the child's own stamped parent key equal to the key that verified the
/// caller's signature.
///
/// History is the one read the 2026-09-25 ruling does NOT widen, because these
/// events belong to a parent: a signed, `read`-holding node may watch any
/// session's FRAME (above), but only the node that spawned this child — the
/// one whose key this door stamped when it admitted the spawn — may read what
/// the child published for it. The comparison is the key, never the stored
/// `node` label: a name follows a rename, a key is the identity.
///
/// **The key is read off the RING first, the record second** ([`stamped_key`]):
/// the ring outlives the record by design — that is the whole reason it exists
/// — so a gate that could only consult `sessions.json` would refuse the parent
/// its own child's last events the moment the record was pruned.
///
/// A `None` on either side is `false`, never a wildcard: an unsigned caller
/// (or a weaker rung, which has no proof and so no key) matches nothing, an
/// unknown id has no key on either side, and a session with no `remoteParent`
/// and no ring — a local session — has nothing to match. Pure, so the whole
/// table is provable without a socket or a stage file.
fn history_admitted(
    read_ok: bool,
    signed: Option<SignedCaller<'_>>,
    target_key: Option<&str>,
) -> bool {
    let Some(caller) = signed else {
        return false;
    };
    output_read_admitted(read_ok, signed)
        && target_key.is_some_and(|k| !k.is_empty() && k == caller.key)
}

/// The key this node stamped for one child: the RING's own entry first (it is
/// the one that survives the record), the roster record's `remoteParent`
/// second. `None` when neither knows the id — which is exactly the id this
/// node does not hold.
fn stamped_key(id: &str) -> Option<String> {
    aoide_storage::pingback_remote::ring_key(id)
        .or_else(|| session_remote_parent(id).map(|rp| rp.key).filter(|k| !k.is_empty()))
}

/// The frame a `tasks/get` request asks for: `Some(tail)` when
/// `params.metadata["aoide/frame"]` is present at all, `None` for a plain
/// status read. Only `params.metadata` counts — never `message.metadata`, the
/// `message/send` fallback `aoide/spawn` also accepts — because this is a
/// request about a session, not a message.
///
/// The value's own `tail` is read if present and is CLAMPED here, at the
/// edge, to `1..=FRAME_TAIL_MAX`: nothing downstream ever sees a window
/// outside the bound. A missing, non-numeric or absent `tail` takes
/// [`FRAME_TAIL_DEFAULT`]; the key's VALUE shape is otherwise not policed
/// (`{"aoide/frame": true}` asks for the default window, which is the
/// tolerant reading of "I want the frame").
fn frame_tail(params: &Value) -> Option<u64> {
    let asked = params.get("metadata")?.get(FRAME_KEY)?;
    let tail = asked.get("tail").and_then(Value::as_u64).unwrap_or(FRAME_TAIL_DEFAULT);
    Some(tail.min(FRAME_TAIL_MAX).max(1))
}

/// The ping-back cursor a `tasks/get` request carries: `Some(seq)` when
/// `params.metadata["aoide/linesAfter"]` is present at all, `None` for a
/// request that asks for no history. Same two rules as [`frame_tail`], for the
/// same reasons: `params.metadata` only, and a value whose shape is not what
/// the writer sends (a missing, null or non-numeric one) reads as the START of
/// the ring — `0` — rather than a refusal. A wrong cursor costs the caller
/// duplicate lines it can recognize by `seq`; refusing it would cost a parent
/// its child's history over one bad integer type.
fn lines_after(params: &Value) -> Option<u64> {
    Some(params.get("metadata")?.get(LINES_AFTER_KEY)?.as_u64().unwrap_or(0))
}

/// The watch frame as the wire artifact, inside the door's own caps.
///
/// Order of shedding, fixed and never mixed: the OLDEST mail letter first,
/// then the OLDEST output line — the newest of each is what a watcher is
/// looking at, and a letter is a whole run's worth of context against one
/// line of output. `truncated: true` is set whenever anything went.
///
/// Two bounds are NOT this function's: the caller's `tail` (applied by
/// [`aoide_conduct::graph::watch_frame`], which never returns more output
/// lines than asked for) and each line's own length (`clean_line`'s
/// `LINE_MAX`, character-counted). The instruction block is bound by that
/// same per-line clip plus the block's line cap and is never dropped: it is
/// the text the frame exists to show, so a frame can exceed
/// [`FRAME_MAX_BYTES`] by at most that block — the door's bound is on what it
/// may DISCARD, not a promise about the sidecar an operator wrote.
fn frame_artifact(frame: aoide_conduct::graph::Frame) -> Artifact {
    let mut frame = frame.for_wire();
    for letter in frame.mail.iter_mut() {
        letter.body.truncate(LETTER_BODY_LINES_MAX);
    }
    let over = |f: &aoide_conduct::graph::Frame| {
        serde_json::to_vec(f).map(|bytes| bytes.len()).unwrap_or(0) > FRAME_MAX_BYTES
    };
    while over(&frame) && (!frame.mail.is_empty() || !frame.output.is_empty()) {
        if !frame.mail.is_empty() {
            frame.mail.remove(0);
        } else {
            frame.output.remove(0);
        }
        frame.truncated = true;
    }
    let mut part = Part { kind: "data".to_string(), text: None, extra: Default::default() };
    part.extra.insert(
        "data".to_string(),
        serde_json::to_value(&frame).expect("a Frame always serializes"),
    );
    Artifact {
        artifact_id: FRAME_ARTIFACT_ID.to_string(),
        name: Some("session watch frame".to_string()),
        parts: vec![part],
    }
}

/// The ping-back history as the wire `message`: the ring read after the
/// caller's cursor, under the part's `data` — `{events, gap, last}`, the ONE
/// shape `aoide_storage::pingback_remote::RingRead` serialises, so the door
/// and the puller that reads it back cannot drift. `history[0]` and nothing
/// else: the A2A envelope's own list carries ONE message here, because one
/// request is one cursor.
fn history_message(task_id: &str, after: u64) -> Message {
    let read = aoide_storage::pingback_remote::events_for(task_id, after);
    let mut part = Part { kind: "data".to_string(), text: None, extra: Default::default() };
    part.extra.insert(
        "data".to_string(),
        serde_json::to_value(&read).expect("a RingRead always serializes"),
    );
    Message {
        role: "agent".to_string(),
        parts: vec![part],
        message_id: Some(HISTORY_MESSAGE_ID.to_string()),
        context_id: None,
        metadata: None,
    }
}

/// `tasks/get` with output asked for: the gate FIRST (so a refusal never
/// depends on whether the id exists), then the SAME status read
/// [`task_get`] answers, plus whichever of the two optional fields the request
/// asked for — the watch frame (`aoide/frame`) and the ping-back history
/// (`aoide/linesAfter`).
///
/// **Two gates, one request.** The frame needs [`output_read_admitted`]; the
/// history needs that AND [`history_admitted`]'s key match. A request that
/// asks for history is judged by the stricter of the two — asking for both is
/// asking for the history, and answering such a request with a frame and a
/// SILENTLY missing ring would be the one thing this door never does. A
/// caller carrying `read` but a foreign key still gets the FRAME it asks for
/// on its own, which is what the 2026-09-25 ruling admits it to.
///
/// A session with no frame to read — an unknown id, a `sub:` card that keeps
/// no PTY of its own, a record that keeps no conduct-owned PTY — answers with
/// the watch's own taught refusal under `-32001`, the same code the unknown-id
/// status read already uses; the message names the reason.
fn task_get_outputs(
    task_id: &str,
    frame: Option<u64>,
    history: Option<u64>,
    read_ok: bool,
    signed_caller: Option<SignedCaller<'_>>,
) -> Result<Value, (i64, String)> {
    if !output_read_admitted(read_ok, signed_caller) {
        return Err((OUTPUT_READ_REFUSED_CODE, OUTPUT_READ_REFUSED.to_string()));
    }
    // The ring's own stamped key is the gate input for history, and the record
    // is the fallback — never the other way round: the ring outlives the
    // record, so a record-first gate would refuse a parent the events the ring
    // was kept for.
    let key = stamped_key(task_id);
    if history.is_some() {
        // An id with NEITHER a ring NOR a record is not a refusal at all: it is
        // the same "task not found" every other `tasks/get` arm answers an
        // unknown id with, and the pull reads it as the permanent answer it is
        // (the child is gone for good). Answering the history-specific refusal
        // here would tell a caller only that the id is not ITS child, and leave
        // the puller retrying an id that can never come back.
        if key.is_none() {
            return Err((TASK_NOT_FOUND_CODE, "task not found".to_string()));
        }
        if !history_admitted(read_ok, signed_caller, key.as_deref()) {
            return Err((OUTPUT_READ_REFUSED_CODE, HISTORY_READ_REFUSED.to_string()));
        }
    }
    let path = sessions_path();
    let sf: SessionsFile =
        load_stage(&path).map_err(|e| (-32603_i64, format!("internal error: {e}")))?;
    let mut task = match build_task(&sf.sessions, task_id) {
        Ok(task) => task,
        // A ring whose child's record is gone: the history read is served from
        // the ring itself (`ring_task`), because that is the read this child's
        // parent is owed and the ring is the only thing that still has it. Any
        // other request for the id keeps the plain not-found answer — a frame
        // needs a record, and there is none.
        Err(not_found) => match (history, key.as_deref()) {
            (Some(_), Some(_)) => ring_task(task_id),
            _ => return Err(not_found),
        },
    };
    if let Some(tail) = frame {
        let frame = aoide_conduct::graph::watch_frame(task_id, tail as usize)
            .map_err(|o| (TASK_NOT_FOUND_CODE, o.message))?;
        task.artifacts = Some(vec![frame_artifact(frame)]);
    }
    if let Some(after) = history {
        task.history = Some(vec![history_message(task_id, after)]);
    }
    Ok(serde_json::to_value(&task).expect("Task always serializes"))
}

/// The envelope a history read gets for a child whose roster record is GONE —
/// the ring's own tail, and nothing invented about the child itself.
///
/// The status is the one thing that must be derived rather than read: a
/// `Task` has one, and there is no record left to fold a state from. It comes
/// from the ring's LAST event, which is the honest answer and the only one
/// this node still holds: an `exited` tail means the child ended (the state
/// A2A calls terminal, `completed`), anything else means no exit was ever
/// published and the child is not known to have stopped. The timestamp is the
/// observation instant, which is what `TaskStatus.timestamp` means on every
/// other arm of this door (`build_task` sets it the same way).
fn ring_task(task_id: &str) -> Task {
    let read = aoide_storage::pingback_remote::events_for(task_id, 0);
    let ended = read.events.last().is_some_and(|e| e.event.get("exited").is_some());
    Task {
        id: task_id.to_string(),
        context_id: task_id.to_string(),
        status: TaskStatus {
            state: if ended { "completed" } else { "working" }.to_string(),
            message: None,
            timestamp: now_iso_utc(),
        },
        kind: "task".to_string(),
        artifacts: None,
        history: None,
    }
}

// ── `message/send`: the inject-or-spawn execution door (Phase B2) ───────────
//
// SECURITY MODEL (CONTRACTS.md §6): a `message/send` SPAWN never runs a
// client-supplied command. The executable comes ONLY from
// `aoide.a2a.spawnAgent` — a nix option, resolved once at `a2a serve` launch
// ([`resolve_spawn_agent`]) — which is rebuild-gated: setting it is the
// user's admission, made once at rebuild time, not per-request. This bounds
// what an external A2A client can do to: (1) task the ALREADY-configured
// agent with a prompt (never a command), or (2) steer an EXISTING conductable
// session the same way `send` would. If `spawnAgent` is unset (the
// default), spawning is simply unavailable — a structured error, not a
// silent no-op. There is deliberately no interactive per-request gate (unlike
// `send`'s pending/--yes/autogate dance): a JSON-RPC request/response
// cannot block on a human clicking "approve" mid-request, so the gate is
// moved entirely to rebuild time, plus the standing loopback bind + the
// Door::A2a audit trail on every inject/spawn/error.

/// What [`decide_send_action`] needs to know about a session named by a
/// `contextId`, decoupled from [`SessionRecord`] so the pure decision stays
/// testable without a stage file on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionRef {
    pub conductable: bool,
    pub has_socket: bool,
}

/// The routing decision `message/send` resolves to — inject into a known
/// session, spawn a fresh conducted one, or a structured JSON-RPC error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendAction {
    Inject { session_id: String },
    Spawn { agent_cmd: String },
    Error { code: i64, msg: String },
}

/// The pure spawn-vs-inject-vs-error decision (CONTRACTS.md §6, "the decided
/// semantics"). No I/O — `session_lookup` is injected so this is unit-testable
/// without a stage file, a socket, or a process.
///
/// - `spawn_asked` (the client set `metadata["aoide/spawn"] == true`) OR a
///   missing `context_id` → **Spawn** the configured agent, or **Error**
///   (`-32004`, "A2A spawn not configured") if `spawn_agent` is empty.
/// - A `context_id` naming a KNOWN, conductable(+socketed) session →
///   **Inject** into it.
/// - A `context_id` naming a known but NOT conductable session → **Error**
///   (`-32004`, "session not conductable").
/// - A `context_id` naming nothing → **Error** (`-32001`, "task not found").
pub fn decide_send_action(
    context_id: Option<&str>,
    spawn_asked: bool,
    spawn_agent: &str,
    session_lookup: impl Fn(&str) -> Option<SessionRef>,
) -> SendAction {
    if spawn_asked || context_id.is_none() {
        return if spawn_agent.is_empty() {
            SendAction::Error {
                code: -32004,
                msg: "A2A spawn not configured".to_string(),
            }
        } else {
            SendAction::Spawn {
                agent_cmd: spawn_agent.to_string(),
            }
        };
    }
    // context_id is Some past this point (the None arm returned above).
    let id = context_id.expect("context_id is Some (checked above)");
    match session_lookup(id) {
        Some(sref) if sref.conductable && sref.has_socket => SendAction::Inject {
            session_id: id.to_string(),
        },
        Some(_) => SendAction::Error {
            code: -32004,
            msg: "session not conductable".to_string(),
        },
        None => SendAction::Error {
            code: -32001,
            msg: "task not found".to_string(),
        },
    }
}

/// Concatenate every text `part`'s `text` field into one prompt — A2A's
/// `Part` union carries `text`/`file`/`data` variants; non-text parts are
/// ignored for this MVP (a richer multi-modal prompt is a later phase). Pure.
fn extract_prompt_text(message: &Value) -> String {
    message
        .get("parts")
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter_map(|p| p.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

/// Resolve `contextId`: prefer `message.contextId`, fall back to the
/// top-level `params.contextId` (both are valid per the A2A JSON-RPC binding;
/// aoide accepts either spot). An empty string is treated as absent. Pure.
fn extract_context_id(message: &Value, params: &Value) -> Option<String> {
    message
        .get("contextId")
        .and_then(Value::as_str)
        .or_else(|| params.get("contextId").and_then(Value::as_str))
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// The explicit-spawn signal: `metadata["aoide/spawn"] == true`, checked on
/// `message.metadata` first, then top-level `params.metadata` (CONTRACTS.md
/// §6 documents this key). Pure.
fn spawn_requested(message: &Value, params: &Value) -> bool {
    let flagged = |v: &Value| {
        v.get("metadata")
            .and_then(|m| m.get("aoide/spawn"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    };
    flagged(message) || flagged(params)
}

/// The task slug a spawning request names: `metadata["aoide/task"]`, read on
/// the SAME two spots [`spawn_requested`] accepts (`message` first, then
/// top-level `params`) — the two keys ride one request shape, and a caller
/// that put `aoide/spawn` at the params level has its slug read there too,
/// rather than quietly getting an unmanaged run. A blank value reads as
/// "names no task", the same non-third state `aoide/from` gives an empty
/// string and the one `spawn --task` gives an empty flag value: the door
/// creates a managed run only for a slug somebody actually wrote. Pure; the
/// slug's own legality is [`spawn_task_slug`]'s question, one layer down,
/// where the refusal text and its audit line live.
fn requested_task(message: &Value, params: &Value) -> Option<String> {
    let named = |v: &Value| {
        v.get("metadata")
            .and_then(|m| m.get(aoide_protocol::wire::TASK_KEY))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    named(message).or_else(|| named(params))
}

/// Hold a caller's task slug to the ONE predicate every task slug and mailbox
/// name takes — `^[a-z0-9][a-z0-9-]*$`
/// (`aoide_storage::node_store::valid_node_name`, the same validator
/// `spawn --task` applies to its flag and [`resolve_bounded_spawn_cwd`]'s
/// sibling `--report-to` applies to a mail role; there is no separate
/// task-slug predicate to reuse, because the slug IS the mailbox name and that
/// one check is what guards the path join). `None` stays `None` — a spawn that
/// named no task is not a malformed one.
///
/// The refusal is the door's `-32602` (invalid params), the same code S3 gives
/// a malformed `aoide/from` claim and for the same reason: the value rode
/// inside the body the caller signed, so a bad one is a client bug worth
/// surfacing rather than a silent downgrade to an unmanaged run. It is applied
/// on the SPAWN side only (`do_spawn`, after `spawn_admitted`) — an Inject
/// request carrying the key builds nothing out of it and is answered exactly
/// as before.
///
/// **The echoed value is cleaned first** (L6 of the S10 review): an illegal slug
/// is illegal precisely because it may hold anything — newlines, ANSI, a bidi
/// override — and this string reaches an RPC error body and whatever local UI
/// prints it, so it goes through `aoide_conduct::graph::clean_line`, the ONE
/// sanitizer every surface that shows a peer's bytes uses (control, Unicode
/// `Cf`, the invisible fillers, whitespace flattened, clipped). Trimmed and
/// capped by that same call: a caller cannot buy a longer echo than a line.
fn spawn_task_slug(task: Option<&str>) -> Result<Option<&str>, String> {
    match task {
        Some(slug) if !aoide_storage::node_store::valid_node_name(slug) => Err(format!(
            "metadata[\"{}\"] `{}` is not a legal task slug (^[a-z0-9][a-z0-9-]*$ — the \
             same name a mailbox and `spawn --task`'s own slug take); send a legal slug, or \
             omit the key for an unmanaged spawn",
            aoide_protocol::wire::TASK_KEY,
            aoide_conduct::graph::clean_line(slug),
        )),
        other => Ok(other),
    }
}

/// Parse one `message/send` `params` object into (prompt text, contextId,
/// spawn_asked, claimed parent session id, named task slug) — pure, so the
/// parsing itself is unit-testable independent of [`decide_send_action`] and
/// the I/O that follows it.
///
/// The fourth field is the caller's own session id, off
/// `message.metadata["aoide/from"]`
/// (`aoide_protocol::wire::FROM_SESSION_KEY`) — ONLY there, never the
/// top-level `params.metadata` fallback [`spawn_requested`] also accepts, as
/// CONTRACTS.md §6 states: this is a claim about WHO is calling, and the
/// client's outbound builder writes it in that one place. An empty or
/// non-string value is not a third state — it reads as "no claim". WHETHER a
/// claim may be honoured is [`claimed_remote_parent`]'s question, one layer
/// down, never this parser's.
///
/// The fifth is the task slug the request names, off `metadata["aoide/task"]`
/// on BOTH the spots [`spawn_requested`] reads ([`requested_task`]) — a
/// directive about what to DO, not an identity claim, so it takes that key's
/// two-spot reading rather than `aoide/from`'s one-spot one. Whether the slug
/// is legal is [`spawn_task_slug`]'s question, and only the Spawn arm consumes
/// it.
fn parse_message_send_params(
    params: &Value,
) -> (String, Option<String>, bool, Option<String>, Option<String>) {
    let message = params.get("message").cloned().unwrap_or(Value::Null);
    let prompt = extract_prompt_text(&message);
    let context_id = extract_context_id(&message, params);
    let spawn_asked = spawn_requested(&message, params);
    let claimed_from = message
        .get("metadata")
        .and_then(|m| m.get(aoide_protocol::wire::FROM_SESSION_KEY))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let task = requested_task(&message, params);
    (prompt, context_id, spawn_asked, claimed_from, task)
}

/// The caller the `aoide/from` claim may be honoured for: the SIGNATURE rung's
/// identity, and nothing else. `resolved` is `message_send`'s own resolution
/// (which also feeds allows off the CURRENT registry) and `signed` the
/// identity [`verify_signed_request`] proved — they agree by construction, the
/// rung literally IS "a signature verified", and this match states that
/// agreement instead of assuming it. A weaker rung (no resolution at all,
/// `NodeRung::Token`, `NodeRung::Addr`) has a `signed` of `None` to pass by
/// then, so nothing to stamp.
fn claimable_caller<'a>(
    resolved: Option<(&aoide_storage::node_store::Node, aoide_storage::node_store::NodeRung)>,
    signed: Option<SignedCaller<'a>>,
) -> Option<SignedCaller<'a>> {
    match resolved {
        Some((_, aoide_storage::node_store::NodeRung::Signature)) => signed,
        _ => None,
    }
}

/// The remote parent to stamp on a spawn, or `Ok(None)` when there is nothing
/// the door may stamp (P-RSA S3, CONTRACTS.md §4/§6).
///
/// Two inputs only — the caller this request PROVED itself to be and the wire
/// claim — because the value must be built from what this door AUTHENTICATED,
/// never from wire bytes: `node` and `key` come off the verified
/// [`SignedCaller`] (the resolved record's name and the stored pubkey that
/// verified), `sessionId` off the claim. A name taken from `X-Aoide-Node` or
/// from the body would be exactly the forgery the key check exists to stop.
///
/// **Honoured on the SIGNATURE rung only** — [`claimable_caller`] is the whole
/// rung table, and it hands over a [`SignedCaller`] only for that rung.
/// Every weaker shape has nothing to pass, so it ignores the claim entirely
/// and returns `Ok(None)`: it can never reach the stamp, and the caller audits
/// the ignore rather than refusing, since an unsigned caller has no claim to
/// make. Pure, so that table is provable without a real spawn.
///
/// A claim that IS honoured is validated
/// ([`aoide_storage::remote_children::valid_claimed_session_id`], the same
/// predicate the client refuses its own unruly claim with) and a bad value is
/// `-32602` — never silently dropped, per CONTRACTS.md §6. Refusing is the
/// CALLER's decision, not this function's: `message_send` applies that error
/// on the spawn side only. The Inject arm never reads the claim, so a malformed
/// one there is not a refusal and not a delivery.
fn claimed_remote_parent(
    caller: Option<SignedCaller<'_>>,
    claim: Option<&str>,
) -> Result<Option<RemoteParent>, (i64, String)> {
    let Some(claim) = claim else {
        return Ok(None);
    };
    let Some(caller) = caller else {
        return Ok(None);
    };
    if !aoide_storage::remote_children::valid_claimed_session_id(claim) {
        return Err((
            -32602,
            format!(
                "invalid params: metadata[\"{}\"] must be 1..={} characters of [A-Za-z0-9._:-] \
                 (a session id) with no `/`",
                aoide_protocol::wire::FROM_SESSION_KEY,
                aoide_storage::remote_children::CLAIMED_SESSION_ID_MAX,
            ),
        ));
    }
    Ok(Some(RemoteParent {
        node: caller.name.to_string(),
        key: caller.key.to_string(),
        session_id: claim.to_string(),
        extra: Default::default(),
    }))
}

/// The `remoteParent` a session record carries, by id: the roster record's
/// fallback inside [`stamped_key`], read off `sessions.json` the way
/// [`session_ref_lookup`] reads its own fields. `None` covers an unknown id, a
/// record with no such field, and an unreadable stage; all three mean "no
/// stamped parent key", never a refusal.
fn session_remote_parent(id: &str) -> Option<RemoteParent> {
    let sf: SessionsFile = load_stage(&sessions_path()).ok()?;
    sf.sessions.iter().find(|s| s.session_id == id).and_then(|s| s.remote_parent.clone())
}

fn unix_ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The session id a spawn mints for its child (`a2a-<pid>-<secs>-<n>`) — the
/// `--id` the wrapper's own `aoide conduct` registers under, and therefore the
/// `contextId`/`sessionId` a later request addresses that child by.
///
/// The trailing counter is NOT decoration: pid + second alone collides for two
/// spawns inside one second (`unix_ts_now` is whole seconds), and two children
/// sharing one id share one `sessions.json` record — `upsert_session`'s update
/// arm keeps a single row, so the two `stamp_spawn_provenance` stamps race on
/// it and the LAST one decides the record's `remoteParent`. That is a parent
/// stamping a run it did not ask for (and losing the one it did), plus one
/// caller-side ledger row for two children. Process-local and monotonic, so
/// back-to-back spawns can never mint the same id; the pid keeps it unique
/// against another `aoided` on the same box, the second keeps it readable.
fn spawn_session_id() -> String {
    static SPAWN_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SPAWN_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("a2a-{}-{}-{}", std::process::id(), unix_ts_now(), seq)
}

/// `session_lookup` for [`decide_send_action`]: read `sessions.json` off the
/// stage and resolve one [`SessionRef`] by id. `has_socket` means the socket
/// path exists ON DISK right now, not merely that the stored string is
/// non-empty — the impure check lives here so `SessionRef`/`decide_send_action`
/// stay disk-free, the same split `conduct::graph::doc`'s `is_conductable_now`
/// draws for the identical bug shape on the `graph` door.
fn session_ref_lookup(id: &str) -> Option<SessionRef> {
    let sf: SessionsFile = load_stage(&sessions_path()).ok()?;
    sf.sessions.iter().find(|s| s.session_id == id).map(|s| SessionRef {
        conductable: s.conductable == Some(true),
        has_socket: s
            .socket
            .as_deref()
            .filter(|p| !p.is_empty())
            .is_some_and(|p| std::path::Path::new(p).exists()),
    })
}

/// Build a `submitted`-state Task keyed on `session_id`/`contextId`
/// (NOT `task_get`, which would report the SESSION's current phase — an
/// unrelated prior turn's state — rather than "this particular message is
/// queued"). Shared by [`do_inject`]'s held-pending arm and `message_send`'s
/// uniform-response guard (CONTRACTS.md §6 amendment, 2026-08-20, #50) so
/// the two "the caller gets an honest immediate `submitted` receipt, the
/// real state shows up later via `tasks/get`/SSE" shapes cannot drift apart.
fn submitted_task(session_id: &str) -> Value {
    let task = Task {
        id: session_id.to_string(),
        context_id: session_id.to_string(),
        status: TaskStatus { state: "submitted".to_string(), timestamp: now_iso_utc(), message: None },
        kind: "task".to_string(),
        artifacts: None,
        history: None,
    };
    serde_json::to_value(&task).expect("Task always serializes")
}

/// Queue an inject from this network door. The door never delivers, so the
/// send is held in `pending.json` for a human to approve from a terminal or
/// the conductor, and the caller gets [`submitted_task`]: the honest immediate
/// answer. `tasks/get` and the SSE stream show the real state once a human has
/// approved.
///
/// The send goes through [`session_send`], as `send` does, so approval and the
/// mailbase filing at delivery (`deliver_local`) belong to the conductor. This
/// invocation never carries `--to`, so `session_send` takes its local branch.
/// `from` is the source [`inject_source`] derived, which `session_send` records
/// on this door as given.
fn do_inject(session_id: &str, prompt: &str, audit_log: &Path, from: &str) -> Result<Value, (i64, String)> {
    let mut flags = std::collections::BTreeMap::new();
    flags.insert("id".to_string(), session_id.to_string());
    flags.insert("submit".to_string(), "true".to_string());
    flags.insert("audit-log".to_string(), audit_log.to_string_lossy().into_owned());
    // The source this door derived ([`inject_source`]): `session_send` reads
    // `--from` as the entry's source on this door only because nothing a remote
    // caller sends can reach a flag. It is never empty, so `resolve_sender`'s
    // `AOIDE_SESSION_ID` fallback, off `a2a serve`'s own long-lived env, never
    // applies.
    flags.insert("from".to_string(), from.to_string());
    let inv = Invocation {
        path: vec!["send".to_string()],
        args: vec![prompt.to_string()],
        flags,
        door: Door::A2a,
    };
    let outcome = session_send(&inv);
    if outcome.status != Status::Ok {
        // Queue state answers uniformly (see `inject_answer`); an oversize
        // prompt is a property of the caller's own text, so it gets its own
        // code and reveals nothing about the queue.
        let reason = outcome.data.as_ref().and_then(|d| d.get("reason")).and_then(Value::as_str);
        let code = match reason {
            Some(PENDING_FULL | PENDING_FILE_FULL) => QUEUE_FULL_CODE,
            Some(PENDING_TOO_LARGE) => INVALID_PARAMS_CODE,
            _ => -32603,
        };
        return Err((code, outcome.message));
    }
    Ok(submitted_task(session_id))
}

/// The source a held inject is recorded under: this door's own account of who
/// sent it, which the queue counts against its per-source cap and shows the
/// approver as the sender. A caller that PROVED a node gets `node:<name>`: a
/// verified signature, or the bearer of a `verified` node (the rungs that name
/// a node only the real one can speak as). Every other caller, the address
/// rung included, gets `addr:<address>`: an address match names a node's URL,
/// not the caller behind it, so it neither borrows the node's name nor fills
/// the node's bucket. `loopback` and `unknown` are one bucket each. Pure.
fn inject_source(
    resolved: Option<(&aoide_storage::node_store::Node, aoide_storage::node_store::NodeRung)>,
    origin: ConnOrigin,
) -> String {
    use aoide_storage::node_store::NodeRung;
    match resolved {
        Some((node, NodeRung::Signature)) => format!("node:{}", node.name),
        Some((node, NodeRung::Token)) if node.verified => format!("node:{}", node.name),
        _ => match origin {
            ConnOrigin::Remote(ip) => format!("addr:{}", ip.to_canonical()),
            ConnOrigin::Loopback => "addr:loopback".to_string(),
            ConnOrigin::Unknown => "addr:unknown".to_string(),
        },
    }
}

/// The refusal a held inject gets when its source already holds the pending
/// queue's per-source cap (`aoide_conduct::graph::PENDING_CAP_PER_SOURCE`).
/// The same `-32000` the pairing park answers when its own queue is full. Only
/// a caller that proved a node identity sees it; [`message_send`] answers
/// anyone else uniformly.
const QUEUE_FULL_CODE: i64 = -32000;
/// JSON-RPC "invalid params": an inject whose text is over the held-entry cap.
const INVALID_PARAMS_CODE: i64 = -32602;

/// Stamp `origin=node:<name>` directly on the just-spawned session's own
/// record — LANE IDENTITY P-ID0 (G16/G5): this DOOR is the record-layer
/// authority for a `node:*` origin, because it is the one place the node
/// name is actually authenticated (`message_send`'s signature/token
/// resolution, above `do_spawn`'s call site). Threading the value through
/// the child's own env (the pre-P-ID0 shape) was unauthenticated — any
/// same-uid process can set `AOIDE_SESSION_ORIGIN=node:X` on itself before
/// invoking `aoide conduct` directly — so `graph/conduct.rs::session_conduct`
/// now REFUSES that shape from its env read entirely, and this function is
/// the only remaining writer of a `node:*` value.
///
/// Retries on the session record landing in `sessions.json` (best-effort, same
/// 300×10ms budget — ~3s): a spawn whose child never registers within that
/// window simply never gets stamped. Disclosed behavior change from the pre-P-ID0
/// shape (a synchronous env write that could never "miss"): a genuinely
/// slow-to-register child can now lose its origin stamp. Never silent about
/// it, though — poll exhaustion with no registration found is eprintln'd by
/// name, so a dropped stamp shows up rather than vanishing quietly. No
/// unbounded retry: a spawn that never registers at all (a failed exec, a
/// missing agent binary) must not spin this thread forever.
///
/// `remote_parent` (P-RSA S3) rides the SAME retry loop — one registration
/// wait, two change-once stamps (`aoide_conduct::graph::stamp_origin` and
/// `stamp_remote_parent`), never a second poll. `None` (no claim, or a claim
/// this door may not honour) stamps nothing, which is the whole pre-S3 shape.
fn stamp_spawn_provenance(id: &str, origin: &str, remote_parent: Option<RemoteParent>) {
    for _ in 0..300 {
        let registered = load_stage(&sessions_path())
            .map(|f: SessionsFile| f.sessions.iter().any(|s| s.session_id == id))
            .unwrap_or(false);
        if registered {
            // NOTE: no `pending` stamp here. The ack path stamps it before the
            // worker is scheduled and the worker stamps it as its own first
            // write, in one thread with its verdict — a third writer from THIS
            // thread could land after a verdict and clobber it back (branch
            // re-review 2's L, fixed).
            aoide_conduct::graph::stamp_origin(id, origin);
            if let Some(parent) = &remote_parent {
                aoide_conduct::graph::stamp_remote_parent(id, parent);
            }
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    eprintln!(
        "aoide a2a: could not stamp node origin for `{id}` within 3s — session registered late or spawn failed"
    );
}

/// How long [`do_spawn`]'s bounded liveness check (task #103) gives the
/// just-launched wrapper process to prove it's still alive before acking
/// `submitted` — 40 × 10ms = 400ms, inside the ~300-500ms window the fix
/// targets. Every legitimate spawn now pays this as fixed RPC latency (an
/// agent meant to run for minutes never notices 400ms); a wrapper that never
/// got past its own exec no longer earns a "submitted" ack for a session id
/// that will never appear in `sessions.json`.
const SPAWN_LIVENESS_ATTEMPTS: u32 = 40;
const SPAWN_LIVENESS_INTERVAL: Duration = Duration::from_millis(10);

/// Poll `try_wait` up to `attempts` times, `interval` apart, returning the
/// exit status the instant one is reported, or `None` once the budget runs
/// out with the child still alive. Pure over an injected poll closure —
/// never a real [`std::process::Child`] — so the bounded-wait SHAPE is
/// unit-testable without spawning a process, the same IO/decision split
/// `daemon::epoch_already_fired` already uses elsewhere in this crate.
fn poll_bounded_exit(
    mut try_wait: impl FnMut() -> std::io::Result<Option<std::process::ExitStatus>>,
    attempts: u32,
    interval: Duration,
) -> Option<std::process::ExitStatus> {
    for i in 0..attempts {
        if let Ok(Some(status)) = try_wait() {
            return Some(status);
        }
        if i + 1 < attempts {
            std::thread::sleep(interval);
        }
    }
    None
}

/// A taught, no-secrets message for [`poll_bounded_exit`]'s failure arm: the
/// WRAPPER process (`aoide conduct`, launched by [`do_spawn`]'s own
/// `cmd.spawn()`) exited before the liveness window closed — virtually
/// always because ITS OWN attempt to exec the configured agent
/// (`aoide-conduct::graph::pty::spawn_on_pty`) failed, since that
/// function's own "spawn FIRST" discipline means a failed exec there returns
/// almost instantly with no session ever registered. Names the PROVEN harness
/// the configured command runs (never the full command line — no flag values,
/// no env, no secrets) and the observed exit status only.
fn spawn_died_immediately_message(harness: &str, status: std::process::ExitStatus) -> String {
    format!(
        "the configured agent (`{harness}`) exited immediately after launch ({status}) — \
         it is likely missing from this unit's PATH, or the configured spawnAgent command line is wrong"
    )
}

/// The door's own argv for one spawn (P-RSA S10): the SAME `conduct` wrapper
/// a local `aoide spawn` builds, through the one builder that shape has
/// (`aoide_conduct::graph::build_conduct_args` — never a second copy of it,
/// which is what the pre-S10 hand-rolled argv was).
///
/// Two of that builder's flags are the door's own decision:
/// `headless = true` always — this door has no terminal to hand a child, its
/// stdio is nulled and it is `setsid`'d, so `--headless` is the honest mode
/// (the log is the child's sink, the pty gets the conventional 80×24 fallback
/// geometry, and `conduct` reads no stdin); and `--task <slug>` exactly when
/// the caller named one, which is what makes the child a MANAGED run (task
/// mailbox, `session watch`'s task view, the exit report, and a record the
/// automatic prune retains). `--spawned` rides unconditionally, as it does for
/// every spawn: it is the registration fact the reaper's abandoned-shell arm
/// reads, and that arm's own shape test (`spawned && restore.is_some() &&
/// idle`) cannot match a non-shell child at all — `restore` is stamped only for
/// a session whose wrapped program captures like a shell, so an ordinary agent
/// `spawnAgent` is never collected by it. A `spawnAgent` that IS a shell never
/// reaches this point at all: `do_spawn` refuses it (`-32004`, audited
/// `shell-spawn-agent`) before any process starts, because that child's first
/// turn is a line typed into a pty and a shell would RUN it (H1).
/// And `--headless` is what makes the child REACHABLE with a keystroke, not
/// just watchable: a headless wrap accepts the ping-back line and a mail-side
/// doorbell write, where a non-headless one with no channel is skipped as
/// `interactive-composer`.
///
/// The other four flags have no source at this door and stay absent, each for
/// a stated reason: `--parent` because `parentSessionId` is a LOCAL id and this
/// door never writes one (CONTRACTS.md §4 — the remote parent is the RECORD
/// field `stamp_spawn_provenance` stamps, not an argv fact); `--instructions-path`
/// because a remote caller names no sidecar and the prompt is injected as the
/// first turn instead; `--timeout` because the door has no deadline to impose
/// and inventing one would kill a long remote run mid-flight; and
/// `--report-to` because the run's own report stays on the child's node,
/// addressed to its own slug (Q5's ruling). Pure — the split of the configured
/// agent command is the same `split_whitespace` the pre-S10 argv did, so what
/// runs is byte-identical.
///
/// `--agent` is the PROVEN `harness` (`spawn_agent_harness`), never a fixed
/// label: the wrap's record carries it, and the ping-back, doorbell and inject
/// readers type into the wrap only when its walked program is that harness.
fn spawn_argv(id: &str, harness: &str, agent_cmd: &str, task: Option<&str>) -> Vec<String> {
    let command: Vec<String> = agent_cmd.split_whitespace().map(str::to_string).collect();
    aoide_conduct::graph::build_conduct_args(true, harness, id, None, task, None, None, None, &command)
}

/// Why a configured `spawnAgent` may not be typed into: a shell (the first turn
/// would RUN as a command) or a command the walk does not prove to be a
/// registered harness. Both refuse before any process starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SpawnAgentRefusal {
    Shell,
    Unproven,
}

/// The one judgement every spawn guard reads: the registered harness a
/// configured `spawnAgent` runs, or why it may not run at all. A shell is refused
/// as `Shell`, and so is everything the walk reads as one without naming a
/// program it proves: a launcher option its table does not list or refuses
/// (`env -S`, `nice --bogus`), `sudo`/`doas`, and `busybox`. Default-deny for the rest:
/// a wrapper with another name, a launcher the walk does not model (`taskset`), or
/// a name with no profile is `Unproven`, because its first turn would be typed into a pty the
/// door cannot prove is a harness. The walk itself is
/// `aoide_conduct::graph::wrapped_harness`, never a copy of it.
fn spawn_agent_harness(agent_cmd: &str) -> Result<&'static str, SpawnAgentRefusal> {
    let configured: Vec<String> = agent_cmd.split_whitespace().map(str::to_string).collect();
    if aoide_conduct::graph::program_is_a_shell(&configured) {
        return Err(SpawnAgentRefusal::Shell);
    }
    aoide_conduct::graph::wrapped_harness(&configured).ok_or(SpawnAgentRefusal::Unproven)
}

/// Build the spawned child's `Command`, env-sanitized, cwd-bound (when
/// `spawn_cwd` resolves), and detached — everything up to but NOT including
/// `.spawn()`. Split out of [`do_spawn`] so the env-clearing shape here is
/// directly unit-testable via `Command::get_envs()`/`Command::get_current_dir()`
/// without an OS-level process spawn (`do_spawn` always launches
/// `std::env::current_exe()`, which under `cargo test` is the TEST binary —
/// see the `Spawn gate table` note above for why no test here drives that spawn).
fn spawn_child_command(
    aoide_bin: &Path,
    argv: &[String],
    audit_log: &Path,
    cwd: Option<&str>,
) -> std::process::Command {
    let mut cmd = std::process::Command::new(aoide_bin);
    cmd.args(argv)
        .env("AOIDE_AUDIT_LOG", audit_log)
        // No `AOIDE_SESSION_ORIGIN` on the child (LANE IDENTITY P-ID0,
        // G16/G5 — reversed from the pre-P-ID0 shape): threading a `node:*`
        // origin through inherited env was unauthenticated, since any
        // same-uid process can set that same var on itself before invoking
        // `aoide conduct` directly. `stamp_spawn_provenance` below stamps the
        // record from THIS door instead, once the child registers. Cleared
        // explicitly in case `a2a serve`'s own env ever carried one.
        .env_remove("AOIDE_SESSION_ORIGIN")
        // No `AOIDE_SESSION_ID` on the child either (S-B, the Osaka
        // wrong-ancestry fix): the `aoide-a2a` systemd unit's own environment
        // can carry the OPERATOR's live terminal session id (set by that
        // terminal's own `aoide conduct` wrap, inherited by every process the
        // unit's shell forks), and a spawned child's tier-3 ambient-parent
        // fallback (`window.rs::resolve_registration_parent`) would otherwise
        // adopt it as `parentSessionId` — a spawned agent parented under a
        // human's unrelated terminal. A real `aoide conduct` launched from an
        // agent's own shell still inherits the id ITS OWN wrap exported
        // (`conduct.rs::session_conduct`'s ordinary local-inheritance path) —
        // only this door, the one place a daemon's ambient env reaches an
        // unrelated freshly-spawned session, clears it.
        .env_remove("AOIDE_SESSION_ID")
        // No `remoteParent` var either, and none to add: the child's remote
        // parent is stamped onto the RECORD by `stamp_spawn_provenance` below,
        // from the resolution this door authenticated — a value the child
        // could read out of its own env is exactly the forgeable shape
        // `AOIDE_SESSION_ORIGIN` above stopped being (P-RSA S3).
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    // ONE seam for the detached-spawn posture: Unix's `setsid(2)` in a
    // `pre_exec` hook, native Windows' `DETACHED_PROCESS |
    // CREATE_NEW_PROCESS_GROUP` flags — `aoide_storage::fs::detach`, which
    // `conduct`'s `spawn_detached` calls too, never a second copy
    // (`crates/AGENTS.md`). After it returns, the child is in its own group,
    // owns no inherited console, and outlives this handler thread.
    aoide_storage::fs::detach(&mut cmd);
    cmd
}

/// Bound `spawn_cwd` (resolved by [`resolve_spawn_cwd`]) against the
/// currently REGISTERED project roots and, on acceptance, return the exact
/// string to hand to `Command::current_dir`. Refuses (returns `None`, having
/// audited exactly once) anything that is not byte-identical to some
/// project's own root — unregistered, a relative path, or a root that no
/// longer exists on disk — since the client never supplies this value and an
/// operator typo must degrade to "inherit", never to an arbitrary directory.
/// An empty `spawn_cwd` (the default — no override configured) is the quiet
/// no-op, not an audited reject.
fn resolve_bounded_spawn_cwd(
    spawn_cwd: &str,
    projects: &[Project],
    audit_log: &Path,
) -> Option<String> {
    if spawn_cwd.is_empty() {
        return None;
    }
    let path = std::path::Path::new(spawn_cwd);
    let registered = projects.iter().any(|p| p.roots().contains(&spawn_cwd));
    if registered && path.is_absolute() && path.is_dir() {
        return Some(spawn_cwd.to_string());
    }
    let reason = if registered {
        "registered project root is not an absolute directory"
    } else {
        "not a registered project root"
    };
    let _ = audit(
        audit_log,
        Door::A2a,
        EventClass::Audit,
        "a2a.message/send",
        "skipped",
        &format!(
            "ignoring configured spawn cwd `{spawn_cwd}` for the spawned child — {reason} — \
             inheriting the daemon's own cwd instead"
        ),
    );
    None
}

/// Spawn a NEW conducted session running the CONFIGURED agent (never a
/// client-supplied command — see the security-model note above `SessionRef`).
/// Detached: launched via the aoide binary's own `conduct` subcommand
/// (`std::env::current_exe()`), `setsid`'d so it survives this handler
/// thread, stdio nulled, and reaped on a parked thread (see below) — it
/// stays parented to the long-lived `a2a serve` daemon for its whole life.
/// `aoide-conduct`'s `spawn` (P2 of the conducted-agents plan) now
/// generalizes exactly this detach/register/reap shape as its own command; a
/// later phase can have this handler ride on it instead of hand-rolling the
/// same mechanics here.
///
/// `node_name` is the resolved, PAIRED, spawn-allowed node `message_send`'s
/// gate already proved before calling this (P-P3, PAIRING.md decision 6) —
/// never optional at this call site, since the gate refuses outright
/// otherwise. Stamped directly onto the spawned record as `origin =
/// "node:<name>"` by [`stamp_spawn_provenance`] below (LANE IDENTITY P-ID0,
/// G16/G5 — this door is the authenticated writer, not the child's env; see
/// that function's doc) and folded into this call's own audit line, so the
/// spawned session's provenance is visible both in the audit log and on the
/// record itself, end to end.
///
/// `remote_parent` (P-RSA S3) is the caller's own claim, already validated and
/// built from THIS resolution by [`claimed_remote_parent`] — `None` for a
/// spawn with no claim. It rides the same stamp, landing as the child's
/// `remoteParent`; `parentSessionId` is never touched by this door, since every
/// reader of that field treats it as a LOCAL id (CONTRACTS.md §4).
///
/// `task` (P-RSA S10) is the slug the caller named under
/// `metadata["aoide/task"]`, already trimmed to "absent or non-empty" by
/// [`requested_task`] and validated HERE by [`spawn_task_slug`] — before
/// `spawn_session_id()` mints an id, before `current_exe()` resolves, before
/// any argv exists, so an illegal slug costs an RPC and nothing else. It is
/// what makes the child a MANAGED run: the argv carries `--task <slug>`
/// through [`spawn_argv`] and the CHILD stamps the fact on its own record at
/// registration, exactly as a local `spawn --task` does. `None` is the whole
/// pre-S10 shape: a plain conducted session, watchable through the log, with
/// no mailbox and no exit report.
///
/// **Bounded liveness check (task #103).** `cmd.spawn()` below only proves
/// the wrapper process itself launched — a caller was previously handed a
/// `submitted` Task the instant that call returned, with no confirmation the
/// wrapper's OWN exec of the configured agent ever succeeded (a missing
/// `spawnAgent` binary on this unit's PATH is the exact defect this closes).
/// [`poll_bounded_exit`] gives the wrapper `SPAWN_LIVENESS_ATTEMPTS ×
/// SPAWN_LIVENESS_INTERVAL` to prove it's still running before the ack goes
/// out; a wrapper that exits inside that window gets
/// [`spawn_died_immediately_message`]'s taught refusal instead of a phantom
/// session id.
fn do_spawn(
    agent_cmd: &str,
    prompt: &str,
    audit_log: &Path,
    node_name: &str,
    spawn_cwd: &str,
    remote_parent: Option<RemoteParent>,
    task: Option<&str>,
) -> Result<Value, (i64, String)> {
    // Refused FIRST and for free, the slug refusal's own shape below: a
    // `spawnAgent` that IS a shell must cost one RPC, never a process and
    // never a line typed into a shell (H1, house rule 4), and never a line typed
    // into a pty the walk cannot prove is a harness. The spawn arm's first turn
    // is held for a human, never typed, and a shell's stdin is a COMMAND LINE:
    // a remote peer's prompt would RUN. The
    // `agent` label is not the question — the configured argv is
    // (`spawn_agent_harness`), so `bash -lc <harness>` is the shell on its face,
    // `env claude` is resolved to its harness, and `taskset -c 0 claude` or a
    // wrapper script is refused as unproven.
    // `-32004` is this door's own "spawn cannot be used as configured" family
    // (the empty-`spawnAgent` arm one layer up): nothing about the CALLER's
    // authority is wrong, so `-32005`/`-32006` would blame the wrong party.
    let harness = match spawn_agent_harness(agent_cmd) {
        Ok(harness) => harness,
        Err(refusal) => {
            let (audit_name, msg) = match refusal {
                SpawnAgentRefusal::Shell => (
                    "shell-spawn-agent",
                    format!(
                        "A2A spawn not configured: `aoide.a2a.spawnAgent` is `{agent_cmd}`, which \
                         reads as a SHELL — a remote prompt typed into it would run as a command. Set \
                         it to the harness to conduct (e.g. `claude`), not to a shell that launches one."
                    ),
                ),
                SpawnAgentRefusal::Unproven => (
                    "unverified-spawn-agent",
                    format!(
                        "A2A spawn not configured: `aoide.a2a.spawnAgent` is `{agent_cmd}`, which the \
                         launcher walk does not prove to be a registered harness ({}). A remote prompt \
                         is typed into the harness's own pty, so only a harness named as its own \
                         program may run: set it to the harness itself (e.g. `claude`), not a wrapper, \
                         a launcher the walk does not model (`taskset`), or a script.",
                        aoide_protocol::agents::known_agents().join(", ")
                    ),
                ),
            };
            let _ = audit(
                audit_log,
                Door::A2a,
                EventClass::Audit,
                "a2a.message/send",
                audit_name,
                &msg,
            );
            return Err((-32004, msg));
        }
    };
    // Refused FIRST and for free: an unusable slug must cost one RPC, never a
    // process — nothing below these lines has run when either fires.
    let task = match spawn_task_slug(task) {        Ok(task) => task,
        Err(msg) => {
            let _ = audit(
                audit_log,
                Door::A2a,
                EventClass::Audit,
                "a2a.message/send",
                "error",
                &msg,
            );
            return Err((-32602, msg));
        }
    };
    // The wrapper's own admission step, shared rather than copied (M2 of the
    // S10 review): a slug a live run already holds is refused HERE, for the
    // same reason `spawn --task` refuses it — two live runs must never share
    // one mailbox, and without this the door is the one spawn path with no
    // admission step at all, so a peer's child could deny the operator their
    // own task name for as long as the peer chooses (this door imposes no
    // deadline by design). `-32602` and not one of the capability codes
    // (`-32004`/`-32006`): nothing is wrong with the CALLER's authority here,
    // the request collides with state this node already holds — the same
    // invalid-params family the malformed-slug refusal above uses, whose key
    // this is. The text is `aoide-conduct`'s ONE refusal sentence, so the
    // peer reads exactly what a local operator would.
    if let Some(slug) = task {
        if let Some((held_by, started_at)) = aoide_conduct::graph::live_run_for(slug) {
            let msg = aoide_conduct::graph::live_run_refusal(slug, &held_by, &started_at);
            let _ = audit(
                audit_log,
                Door::A2a,
                EventClass::Audit,
                "a2a.message/send",
                "error",
                &msg,
            );
            return Err((-32602, msg));
        }
    }
    let id = spawn_session_id();
    let aoide_bin = std::env::current_exe()
        .map_err(|e| (-32603_i64, format!("resolving the aoide binary: {e}")))?;

    let argv = spawn_argv(&id, harness, agent_cmd, task);
    let origin = format!("node:{node_name}");
    // Project roots are already loaded the same way `session_ref_lookup`
    // loads `sessions.json` off the stage — a missing/corrupt file degrades
    // to an empty registry, so a misconfigured `spawn_cwd` never blocks a
    // spawn, only its cwd bound.
    let pf: ProjectsFile = load_stage(&aoide_storage::stage::projects_path()).unwrap_or_default();
    let bounded_cwd = resolve_bounded_spawn_cwd(spawn_cwd, &pf.projects, audit_log);
    let mut cmd = spawn_child_command(&aoide_bin, &argv, audit_log, bounded_cwd.as_deref());

    match cmd.spawn() {
        Ok(mut child) => {
            // Bounded liveness confirmation (task #103): `cmd.spawn()` above
            // only proves the WRAPPER `aoide conduct` process itself
            // launched — it says nothing about whether ITS OWN attempt to
            // exec the configured agent succeeded. That failure is
            // synchronous INSIDE the wrapper (`session_conduct`'s "spawn
            // FIRST" discipline registers no session and the wrapper exits
            // almost instantly), but this door is a separate, detached
            // process with no synchronous view into it — acking
            // unconditionally here is exactly how a caller was handed a
            // `submitted` Task naming a session that had already failed to
            // spawn (the defect this fix closes). Give the wrapper a short
            // window to prove it's still running before acking success.
            if let Some(status) = poll_bounded_exit(
                || child.try_wait(),
                SPAWN_LIVENESS_ATTEMPTS,
                SPAWN_LIVENESS_INTERVAL,
            ) {
                let msg = spawn_died_immediately_message(harness, status);
                let _ = audit(
                    audit_log,
                    Door::A2a,
                    EventClass::Audit,
                    "a2a.message/send",
                    "error",
                    &msg,
                );
                return Err((-32603, msg));
            }

            // `setsid()` above detaches the child into its own session so it
            // survives this handler thread, but a new session does NOT
            // reparent the child — this process is still its parent and
            // still owes it a `wait()`. Skip that and the kernel keeps the
            // exit status around forever once the child dies: a zombie
            // entry in the process table, uncollected for as long as
            // `a2a serve` runs. Park a thread whose only job is to collect
            // it; nothing else here depends on when that happens.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            // Stamp the record's provenance from THIS door, off the handler
            // thread so a slow-to-register child never adds latency to the
            // RPC response — see `stamp_spawn_provenance`'s doc comment.
            {
                let id = id.clone();
                let origin = origin.clone();
                std::thread::spawn(move || stamp_spawn_provenance(&id, &origin, remote_parent));
            }
            // Best-effort first-turn injection — see the doc comment above —
            // on its OWN WORKER, never this handler thread: the readiness wait
            // can run for `READY_BUDGET` (20s) and then the socket retry for
            // ~3s more, and a handler parked that long against `MAX_CONN`
            // hands `503 server busy` to every other RPC — read commands
            // included. The handler's slot is released the moment it returns,
            // so the real bound on these waits is the pool below
            // ([`OPENING_TURN_WORKERS_MAX`]): past it the opening turn is
            // reported `busy`, never silently dropped. Nothing joins the
            // worker, and the record carries the outcome so `tasks/get` can
            // tell the peer what became of its turn.
            aoide_conduct::graph::stamp_opening_turn(&id, "pending");
            let worker_audit = audit_log.to_path_buf();
            let mut ack_word = "pending";
            match OpeningTurnSlot::acquire() {
                None => {
                    // The pool is saturated with other spawns' waits. This is
                    // NOT "the target never reported readiness" — it is "this
                    // door is full" — so it gets its own word, and the peer can
                    // ask again.
                    aoide_conduct::graph::stamp_opening_turn(&id, "busy");
                    ack_word = "busy";
                    let _ = audit(
                        audit_log,
                        Door::A2a,
                        EventClass::Audit,
                        "a2a.message/send",
                        "busy",
                        &format!(
                            "opening turn for `{id}`: busy — {OPENING_TURN_WORKERS_MAX} waits already in flight; the session is spawned, ask again for its opening turn"
                        ),
                    );
                }
                Some(slot) => {
                    let worker_id = id.clone();
                    let prompt = prompt.to_string();
                    let worker_from = Some(format!("node:{node_name}"));
                    let scheduled = std::thread::Builder::new()
                        .name(format!("a2a-opening-turn-{worker_id}"))
                        .spawn(move || {
                            opening_turn_worker(
                                worker_id,
                                prompt,
                                worker_audit,
                                slot,
                                worker_from,
                            )
                        })
                        .is_ok();
                    if !scheduled {
                        // No worker means no turn will ever be typed — and
                        // `not-ready` would be a lie about the TARGET, so this
                        // says what actually happened.
                        aoide_conduct::graph::stamp_opening_turn(&id, "no-worker");
                        ack_word = "no-worker";
                        let _ = audit(
                            audit_log,
                            Door::A2a,
                            EventClass::Audit,
                            "a2a.message/send",
                            "no-worker",
                            &format!(
                                "opening turn for `{id}`: no-worker — the door could not start a worker thread; nothing was typed"
                            ),
                        );
                    }
                }
            }
            let _ = audit(
                audit_log,
                Door::A2a,
                EventClass::Audit,
                "a2a.message/send",
                "ok",
                &format!(
                    "spawned conducted session `{id}` (configured agent, {origin}{}) — opening turn pending; it is held in pending.json for a human, never typed by this door",
                    match task {
                        Some(slug) => format!(", managed run on task `{slug}`"),
                        None => String::new(),
                    }
                ),
            );
            let task = Task {
                id: id.clone(),
                context_id: id.clone(),
                status: TaskStatus {
                    state: "submitted".to_string(),
                    timestamp: now_iso_utc(),
                    message: Some(opening_turn_status(&id, ack_word)),
                },
                kind: "task".to_string(),
                artifacts: None,
                history: None,
            };
            Ok(serde_json::to_value(&task).expect("Task always serializes"))
        }
        Err(e) => {
            let msg = format!("failed to spawn A2A agent: {e} ({origin})");
            let _ = audit(
                audit_log,
                Door::A2a,
                EventClass::Audit,
                "a2a.message/send",
                "error",
                &msg,
            );
            Err((-32603, msg))
        }
    }
}

/// `message/send`: parse params, resolve [`decide_send_action`], execute.
///
/// Every Inject parks in `pending.json` for a human. This door cannot approve a
/// gated send, so no caller is delivered, loopback or signed. What an Inject
/// decides is only whether the caller is admitted to the queue at all.
///
/// - **Inject admission.** With no door-wide token configured, every Inject
///   reaches the queue. Once one is configured, a `contextId` send from a
///   caller that proved neither that token nor a node identity gets the
///   uniform answer: the synthetic `submitted` Task of [`submitted_task`],
///   which names no session and touches neither `sessions.json` nor
///   `pending.json`. An unauthenticated caller therefore cannot tell a real
///   `contextId` from a bogus one. A node identity is a `verified` node's own
///   bearer (the [`aoide_storage::node_store::resolve_node`] Token rung,
///   matched against its `token_file`), or a verified per-request signature
///   whose grant in the signed mesh holds `message` (the Signature rung). An
///   address match alone admits nothing.
/// - **The per-source cap.** The queue holds at most
///   `aoide_conduct::graph::PENDING_CAP_PER_SOURCE` entries per `from`, the
///   source [`inject_source`] derives. A proven node past it is refused
///   `-32000` ([`QUEUE_FULL_CODE`]); any other caller gets the uniform answer.
///   Either way nothing is queued, nothing queued is touched, and
///   `session_send` audits the refusal.
/// - **The source.** A queued Inject carries `node:<name>` in its `from`
///   field when the caller proved the node (a verified signature, or a
///   `verified` node's own bearer), and `addr:<address>` for every other
///   caller, the address rung included. An address match never borrows the
///   node's name, so it neither shows an approver a node that did not send it
///   nor fills that node's bucket. The source is not a gate.
/// - **Spawn.** [`spawn_admitted`]: the caller must resolve by a verified
///   signature and hold `spawn` in the mesh its request names. The door-wide
///   bearer is not consulted, and a bare address or a token match never
///   launches a process. A refusal is `-32006`, naming the missing
///   prerequisite ([`spawn_refusal`]).
/// - **The `aoide/from` claim** is honoured only on the signature rung
///   ([`claimable_caller`], [`claimed_remote_parent`]). A spawn stamps it on
///   the child as `remoteParent`, which the ping-back history gate reads
///   ([`history_admitted`]). No claim changes whether an Inject parks.
/// - **Reads** are gated by [`token_authorized`] or a granted signature, and
///   the door-wide bearer never identifies a node.
fn message_send(
    params: &Value,
    audit_log: &Path,
    spawn_agent: &str,
    spawn_cwd: &str,
    origin: ConnOrigin,
    expected_token: &str,
    presented_token: Option<&str>,
    signed_caller: Option<SignedCaller<'_>>,
) -> Result<Value, (i64, String)> {
    let (prompt, context_id, spawn_asked, claimed_from, task) = parse_message_send_params(params);
    let token_configured = !expected_token.is_empty();
    let token_state = classify_token(expected_token, presented_token);

    let nodes = aoide_storage::node_store::load_nodes();

    // The caller's resolved node IDENTITY, and which rung resolved it. The
    // Inject arm derives the entry's source from it (`inject_source`: the node's
    // name only for a proven rung), and admission is `node_proved`, below. The
    // Spawn arm's `spawn_admitted` requires the SIGNATURE rung, because neither
    // a bare address nor a bare token may authorize launching a process
    // attributed to a node.
    let addr = match origin {
        ConnOrigin::Remote(ip) => Some(ip),
        ConnOrigin::Loopback | ConnOrigin::Unknown => None,
    };
    // `signed_caller` arrives ALREADY VERIFIED: `handle_connection` checked the
    // ed25519 signature, the replay window and the nonce sets
    // (`verify_signed_request`) before this function runs. Its name is the
    // record whose stored pubkey verified the signature, never the wire's
    // `X-Aoide-Node` label, so the lookup below is not a trust decision. When
    // present it is the SOLE resolution: a request that presented signature
    // headers never falls through to the address/token ladder.
    let resolved_node = match signed_caller {
        Some(caller) => nodes
            .iter()
            .find(|p| p.name == caller.name)
            .map(|p| (p, aoide_storage::node_store::NodeRung::Signature)),
        None => aoide_storage::node_store::resolve_node(&nodes, addr, presented_token),
    };

    // The caller's `aoide/from` claim: the spawned child's `remoteParent`. Two
    // questions. FIRST, may this request claim at all: `claimable_caller` hands
    // over an identity for the SIGNATURE rung only. Every weaker rung ignores
    // the claim and gets one audit line. SECOND, is the claim well formed:
    // `claimed_remote_parent`'s `-32602` for a signed caller's malformed one,
    // applied on the SPAWN side only (below), since only a spawn builds a value
    // out of it. The Inject arm never reads the claim. The client refuses an
    // unruly claim before sending it, so this refusal is a backstop.
    let claimed_identity = claimable_caller(resolved_node, signed_caller);
    if claimed_identity.is_none() && claimed_from.is_some() {
        let _ = audit(
            audit_log,
            Door::A2a,
            EventClass::Audit,
            "a2a.message/send",
            "ignored-unsigned-from",
            &format!(
                "ignored metadata[\"{}\"] — a remote parent may only be claimed by a \
                 request this door verified by a node's key (no signature rung on this request)",
                aoide_protocol::wire::FROM_SESSION_KEY,
            ),
        );
    }
    let claim_build = claimed_remote_parent(claimed_identity, claimed_from.as_deref());

    // Admission to the queue, not delivery. Once a door-wide token is
    // configured, a caller must prove a node identity this box trusts with a
    // message, or the uniform guard below answers: a VERIFIED node's own bearer
    // (the Token rung), or a verified signature whose grant in the signed mesh
    // holds `message` (the Signature rung). An address match proves nothing: a
    // tunnel or a shared NAT address looks the same for everyone behind it.
    let node_proved = match resolved_node {
        Some((node, aoide_storage::node_store::NodeRung::Token)) => node.verified,
        Some((_, aoide_storage::node_store::NodeRung::Signature)) => {
            caller_grant(signed_caller).holds("message")
        }
        Some((_, aoide_storage::node_store::NodeRung::Addr)) | None => false,
    };

    // Uniform-response guard. It mirrors `decide_send_action`'s own
    // `spawn_asked`/`context_id` split (`context_id.is_some() && !spawn_asked`
    // is that function's "a lookup, not a spawn" condition), so a request can
    // never classify "spawn" for this gate and "send" for the decision, or the
    // reverse.
    if token_configured && token_state != TokenState::Valid && !node_proved && context_id.is_some() && !spawn_asked {
        let id = context_id.as_deref().expect("context_id.is_some() checked above");
        let _ = audit(
            audit_log,
            Door::A2a,
            EventClass::Audit,
            "a2a.message/send",
            "unauthorized",
            &format!("uniform submitted Task for context `{id}` — no valid token, no node identity (#50)"),
        );
        return Ok(submitted_task(id));
    }

    // The SPAWN side of that same split, and the only place a malformed claim
    // refuses: `decide_send_action`'s own spawn condition (`spawn_asked ||
    // context_id.is_none()`, the exact negation of the guard's inject one), so
    // a request can never classify "inject" for the guard and "spawn" here. An
    // inject-shaped request with a malformed claim proceeds as one with none.
    let spawn_side = spawn_asked || context_id.is_none();
    let remote_parent = match claim_build {
        Ok(parent) => parent,
        Err((code, msg)) if spawn_side => {
            let _ = audit(
                audit_log,
                Door::A2a,
                EventClass::Audit,
                "a2a.message/send",
                "error",
                &msg,
            );
            return Err((code, msg));
        }
        Err(_) => None,
    };

    match decide_send_action(context_id.as_deref(), spawn_asked, spawn_agent, session_ref_lookup) {
        SendAction::Inject { session_id } => {
            // Every inject queues, so a source always rides the pending entry
            // (P-P3 decision 7): the proven node, else the caller's address.
            let from = inject_source(resolved_node, origin);
            match do_inject(&session_id, &prompt, audit_log, &from) {
                // A full source: a proven node is told so. Anyone else gets the
                // uniform answer, as a caller the guard above did not admit
                // would. `session_send` has already audited the refusal.
                Err((QUEUE_FULL_CODE, _)) if !node_proved => Ok(submitted_task(&session_id)),
                other => other,
            }
        }
        SendAction::Spawn { agent_cmd } => {
            if spawn_admitted(resolved_node, &caller_grant(signed_caller)) {
                let node = resolved_node.expect("spawn_admitted only returns true when resolved_node is Some").0;
                do_spawn(&agent_cmd, &prompt, audit_log, &node.name, spawn_cwd, remote_parent, task.as_deref())
            } else {
                let (code, msg) = match mesh_or_refusal(
                    "spawn refused",
                    signed_caller.and_then(|c| c.mesh),
                    "a2a.message/send",
                    audit_log,
                ) {
                    Ok(mesh) => spawn_refusal(resolved_node, &mesh),
                    Err(refusal) => refusal,
                };
                let _ = audit(
                    audit_log,
                    Door::A2a,
                    EventClass::Audit,
                    "a2a.message/send",
                    "unauthorized",
                    &msg,
                );
                Err((code, msg))
            }
        }
        SendAction::Error { code, msg } => Err((code, msg)),
    }
}

/// The node-side half of the Spawn admission check (P-P3, PAIRING.md
/// decision 6; per mesh from P-CHARTER) — the caller's grant
/// ([`grant_in_mesh`], in the mesh its request names) holds `"spawn"`. Pure,
/// split out of [`message_send`] so the gate table (granted / not granted /
/// no resolution at all) is directly unit-testable against a plain [`Grant`],
/// without ever touching [`do_spawn`]'s real OS-level process spawn.
/// Deliberately says nothing about HOW the caller resolved to this node —
/// that question is [`spawn_admitted`]'s job, one layer up.
fn may_spawn(grant: &Grant) -> bool {
    grant.holds("spawn")
}

/// The Spawn arm's FULL admission check (P-P3 decision 6, narrowed
/// 2026-08-25 to the token rung; narrowed AGAIN 2026-08-25/P-P4 to the
/// SIGNATURE rung specifically — see [`spawn_refusal`]'s doc comment for the
/// full grounding). Neither the ADDR rung nor the (now superseded) TOKEN
/// rung reaches [`do_spawn`] any more: a bare source-address match carries
/// no possession proof at all, and a bare shared-secret token is not bound
/// to any one request — replayable, and identical across every request the
/// true node or an impersonator ever sends. Both rungs keep resolving a
/// node identity for every OTHER purpose (Inject's source, [`inject_source`],
/// origin-stamping); this function is the ONE place the narrowing to
/// signature-only lives, rather than re-derived at each call site — and with
/// it the GRANT, read in the request's mesh and no other
/// ([`grant_in_mesh`]). Pure — unit-testable directly against
/// `(&Node, NodeRung, &Grant)` fixtures without touching [`do_spawn`]'s real
/// OS-level process spawn, the same "predicate-level, not through
/// `message_send`" precedent [`may_spawn`]'s own doc comment already
/// established.
fn spawn_admitted(
    resolved: Option<(&aoide_storage::node_store::Node, aoide_storage::node_store::NodeRung)>,
    grant: &Grant,
) -> bool {
    matches!(resolved, Some((_, aoide_storage::node_store::NodeRung::Signature))) && may_spawn(grant)
}

/// The `-32006` refusal every Spawn attempt that fails [`spawn_admitted`]
/// returns (P-P3, message content narrowed P-P4) — a distinct code from
/// `unauthorized()`'s `-32005` (the door-wide bearer gate of the read arms and
/// `message/stream`), since this is a DIFFERENT question: not "do you hold a
/// valid door-wide token" but "do you resolve, via a verified per-request
/// SIGNATURE, to a specific node this operator has paired with and allowed to
/// spawn." The CODE stays `-32006` across every refusal shape (no new code per
/// shape — `message_send`'s callers already match on this one value), but the
/// MESSAGE is now shape-specific (P-P4 requirement: "a taught error telling an
/// unsigned paired caller that its aoide is too old / must sign,"
/// distinguishable from "you were never paired at all"):
/// - **Resolved via the (now-superseded) Token rung, and genuinely
///   `verified`**: this caller IS a real, paired node — it just didn't sign
///   this request. Told to sign, not to re-pair; re-pairing would be
///   nonsensical noise for a caller whose only problem is an old client
///   that predates P-P4.
/// - **Resolved via Signature but the grant in the request's MESH lacks
///   `spawn`**: pairing and signing both succeeded — only the capability
///   grant is missing there. Told the exact `node allow … --mesh <m>` fix
///   (review finding 11), not to re-pair or re-sign: a bare `node allow` is
///   a command that would itself refuse on a two-mesh record, and on a
///   one-mesh record would act in the record's mesh rather than the one the
///   request named.
/// - **Every other shape** (Addr rung, no resolution at all, an
///   unverified Token match): told to pair AND sign, the original P-P3
///   message's grounding, now naming the signature requirement too.
fn spawn_refusal(resolved: Option<(&aoide_storage::node_store::Node, aoide_storage::node_store::NodeRung)>, mesh: &str) -> (i64, String) {
    use aoide_storage::node_store::NodeRung;
    match resolved {
        Some((node, NodeRung::Token)) if node.verified => (
            -32006,
            format!(
                "spawn refused: node `{}` is paired, but this request was not signed — spawn now \
                 requires a per-request ed25519 signature (docs/architecture/PAIRING.md's wire-auth \
                 section), and a bare token no longer admits it. The caller's aoide is too old to \
                 sign requests, or is failing to sign them — upgrade the caller",
                node.name
            ),
        ),
        Some((node, NodeRung::Signature)) => (
            -32006,
            format!(
                "spawn refused: node `{}` is paired and this request is validly signed, but its grant \
                 in mesh `{mesh}` does not include `spawn` — on this (receiving) host run \
                 `aoide node allow {} spawn on --mesh {mesh}`; the caller's own grant is not consulted, \
                 and a grant in another mesh is not a grant here",
                node.name, node.name
            ),
        ),
        Some((_, NodeRung::Addr)) | Some((_, NodeRung::Token)) | None => (
            -32006,
            format!(
                "spawn refused: spawn requires the caller be identified via a verified, per-request \
                 SIGNED request from a paired node (an address match, or an unverified token match, \
                 never admits spawn) — pair first via `aoide pair`, then `node allow <name> spawn on \
                 --mesh {mesh}` (the grant is read in the mesh the request names; on a node trusted in \
                 several, `--mesh` is required)"
            ),
        ),
    }
}

/// `aoide/graphSummary` (CONTRACTS.md §7): wrap the EXISTING resolved
/// `graph.json` v0 document ([`resolve_graph_document`], the exact same
/// function bare `graph` builds its document with) in the
/// federation envelope. No new graph vocabulary — `graph` below is that
/// document verbatim.
fn graph_summary(node_name: &str, self_url: &str) -> Result<Value, (i64, String)> {
    let graph = resolve_graph_document().map_err(|e| (-32603_i64, format!("internal error: {e}")))?;
    Ok(json!({
        "schemaVersion": "0",
        "instance": {
            "name": node_name,
            "url": self_url,
            "emittedAt": now_iso_utc(),
        },
        "graph": graph,
    }))
}

// ── `aoide/mailDeposit` (messaging plan P-M2, CONTRACTS.md §6) ─────────────
//
// The wire for directly-paired nodes: a caller identified via a verified
// PER-REQUEST SIGNATURE (never an address or bare-token match — the same
// signature-only narrowing `spawn_admitted` holds, applied to a new,
// independent capability) deposits one sealed [`aoide_storage::mail::
// Envelope`]. Admission answers ONE question — "is this signed caller a
// paired node holding `message`" — and is entirely separate from the
// envelope's OWN origin signature, which [`aoide_storage::mail::deposit`]
// verifies against `header.from.node`'s own key (spec item 3: the HOP that
// carried the request here and the ORIGIN that minted it are two
// independent lookups, coincident only because P-M2 has no relay yet).

/// The declaration set the two MAIL methods read — **once per request, and only
/// for them** (MAIL.md §Status). A broken zone table must refuse mail rather
/// than serve it with the walls down, while every other method is answered
/// exactly as before; and one read serves the whole request, so the set the
/// caller's `down` is judged by is the set the deposit is filed under.
///
/// A set that will not load is a refused RESULT carrying [`CONFIG_INVALID`] —
/// audited here, once, so this host's own log says why — never an internal
/// error: it is this host's state, and a sender's drain must park its entry for
/// it rather than read it as a dead link.
fn mail_declarations(
    ctx: &RequestCtx,
    label: &str,
) -> Result<Vec<aoide_storage::routing::Loaded>, Value> {
    match aoide_storage::routing::declarations() {
        Ok(set) => Ok(set),
        Err(refusal) => {
            // The load error names this host's own paths and structure, so it
            // stops here, at the audit line; the wire gets fixed text.
            let detail = format!("{CONFIG_INVALID}: {refusal}");
            let _ = audit(ctx.audit_log, Door::A2a, EventClass::Audit, label, "invalid", &detail);
            Err(json!({ "status": "refused", "reason": CONFIG_INVALID, "detail": CONFIG_INVALID_DETAIL }))
        }
    }
}

/// The `down` gate at the door: a request whose own node the mesh declares
/// `down` is refused before anything is filed or hopped — MAIL.md §Status's "a
/// `down` origin's own door requests are refused, and no new letter is accepted
/// from it", with the letters it queued before being kept, never confiscated.
///
/// The name is the declaration's own for the verifying key
/// ([`declared_caller_name`]), never a `nodes.json` nickname, and the word is
/// `charter::STATUS_DOWN` — the transit lane's own refused reason, which is what
/// a sender's drain already classifies as a policy refusal (CONTRACTS.md §6).
fn down_caller_refusal(
    declarations: &[aoide_storage::routing::Loaded],
    mesh: &str,
    caller: SignedCaller<'_>,
    label: &str,
    audit_log: &Path,
) -> Option<Value> {
    let name = declared_caller_name(declarations, mesh, caller);
    if aoide_storage::routing::status_of(declarations, mesh, &name)
        != Some(aoide_storage::charter::STATUS_DOWN)
    {
        return None;
    }
    let detail = format!(
        "mail refused: node `{name}` is declared `down` in mesh `{mesh}` — its own door requests are \
         refused until the declaration changes; what it queued before is kept, never confiscated"
    );
    let _ = audit(audit_log, Door::A2a, EventClass::Audit, label, "unauthorized", &detail);
    Some(json!({
        "status": "refused",
        "reason": aoide_storage::charter::STATUS_DOWN,
        "detail": detail,
    }))
}

/// The node-side half of the Message admission check — the caller's grant
/// ([`grant_in_mesh`], in the mesh its request names) holds `"message"`.
/// Mirrors [`may_spawn`] exactly, one capability over.
fn may_message(grant: &Grant) -> bool {
    grant.holds("message")
}

/// The Message arm's full admission check. Unlike [`spawn_admitted`], there
/// is no now-superseded Token-rung history to migrate off of — `message`
/// is introduced AFTER that narrowing already happened — so resolution
/// here is signature-only from the start: a [`Grant`] is `none()` unless a
/// verified signature resolved a record whose grant in this mesh holds the
/// capability, and there is no Addr/Token fallback rung to even consider.
fn deposit_admitted(grant: &Grant) -> bool {
    may_message(grant)
}

/// The refusal every deposit attempt that fails [`deposit_admitted`]
/// returns — a NEW, distinct code (never `-32006`, which stays `spawn`'s
/// own; never `-32007`, already taken by `verify_signed_request`'s
/// incomplete-headers/signature-mismatch refusals, CONTRACTS.md §6). **Three
/// shapes** (P-CHARTER's third added by review N5, and it PREEMPTS the other
/// two):
///
/// 1. **the mesh is charter-shaped with an undecidable operator key** — no
///    grant in it can be read at all, so the refusal names the mesh, says the
///    key is undecidable, says WHY (the `trusted_operator` reason word only —
///    the detail names both operator keys, so it goes to the audit line and
///    never to an ungated caller), and points at `aoide mesh charter show`;
/// 2. paired-but-not-allowed in THIS
/// MESH, told the exact `node allow --mesh` fix AND where it runs (the
/// RECEIVING host: the gift is the receiver's record of the sender, never the
/// sender's own) plus the `mail outbox retry --refused` that then moves the
/// parked letters;
/// 3. everything else (unpaired, unsigned, no resolution at all)
/// told to pair and allow. `mesh` is the mesh the request acted in, named in
/// both the refusal and its fix: a grant in another mesh is not a grant here,
/// and saying which mesh was read is the difference between a fixable refusal
/// and a mystery.
/// The refusal a **charter-shaped** mesh owes an ungranted caller — shared by
/// both mail arms so they can never teach different fixes for the same state.
/// They did: N5 gave `deposit_refusal` this arm and `poll_refusal` had none
/// until H1's merge review, so the same box in the same state told a poller to
/// run a command that answers `widens-charter` and changes nothing.
///
/// `None` when the mesh is not charter-shaped — the caller's own arm then
/// speaks, and ITS `aoide node allow … on --mesh` teaching is correct there (a
/// pair mesh's grant is editable locally). Two shapes when it is, because the
/// fix differs:
///
/// - **the operator key is UNDECIDABLE** (`governing` is `None`): nothing in
///   the mesh is granted to anybody until it resolves, so the refusal names
///   the mesh, says WHY, and points at the one command that shows it;
/// - **a charter is in force**: the caller's LINE is the whole grant. A
///   missing or removed line is fixed by the charter's OPERATOR signing a new
///   version that lists the key — never by `aoide node allow … on`, and never
///   by anything this host does alone. Local narrowing can only take away
///   (`… off`), so the fix travels with the charter.
///
/// `what` is the arm's own words ("mail deposit refused" / "mail poll
/// refused"); `label` is the audit name the arm self-audits under. In the
/// undecidable shape the DETAIL (both operator keys, and which file holds
/// which) reaches the audit line only: the refusal is returned by an ungated
/// method, so the caller sees the reason word, never the keys.
fn charter_refusal(what: &str, mesh: &str, label: &str, audit_log: &Path) -> Option<(i64, String)> {
    if !aoide_storage::charter::charter_shaped(mesh) {
        return None;
    }
    if aoide_storage::charter::governing(mesh).is_none() {
        let (why, detail) = match aoide_storage::charter::trusted_operator(mesh) {
            Err(refusal) => (refusal.reason.clone(), format!("{}: {}", refusal.reason, refusal.detail)),
            Ok(_) => ("undecidable".to_string(), "the charter's operator key cannot be read".to_string()),
        };
        let _ = audit(
            audit_log,
            Door::A2a,
            EventClass::Audit,
            label,
            "unauthorized",
            &format!("charter-shaped mesh `{mesh}` with an undecidable operator key — {detail}"),
        );
        return Some((
            -32010,
            format!(
                "{what}: mesh `{mesh}` is a CHARTER mesh on this host and its operator key is UNDECIDABLE \
                 right now ({why}). Nothing in that mesh is granted to anybody until it is resolved — the \
                 charter is the only trust there, so there is no paired-record fallback and no local \
                 `node allow … on --mesh {mesh}` that could widen it. See `aoide mesh charter show {mesh}` \
                 for the recorded key, where it is written down (config line vs state record), and \
                 `aoide mesh` for the mesh's own row (this host's own log carries the detail)"
            ),
        ));
    }
    let _ = audit(
        audit_log,
        Door::A2a,
        EventClass::Audit,
        label,
        "unauthorized",
        &format!("charter-governed mesh `{mesh}`: the caller's line carries no `message`"),
    );
    Some((
        -32010,
        format!(
            "{what}: mesh `{mesh}` is a CHARTER mesh on this host and the charter is what grants in it — \
             the caller's line there carries no `message`, and nothing local can supply one: \
             `aoide node allow … on --mesh {mesh}` answers `widens-charter` and changes nothing (narrowing \
             a line with `… off` is the only local move). Re-listing the key is the charter's OPERATOR \
             signing a new version that carries it, delivered to this host as a `mesh charter accept \
             <file>` on the LAN, or as a charter letter over transit. `aoide mesh charter show \
             {mesh}` reads the version in force"
        ),
    ))
}

fn deposit_refusal(signed: Option<SignedCaller<'_>>, mesh: &str, audit_log: &Path) -> (i64, String) {
    if let Some(refusal) = charter_refusal("mail deposit refused", mesh, "a2a.aoide/mailDeposit", audit_log) {
        return refusal;
    }
    match signed {
        Some(caller) => (
            -32010,
            format!(
                "mail deposit refused: node `{}` is paired and this request is validly signed, but its \
                 grant in mesh `{mesh}` does not include `message` — on this (receiving) host run \
                 `aoide node allow {} message on --mesh {mesh}`, then on `{}` run `aoide mail outbox \
                 retry --refused` so its parked letters move; the sender's own grant is not consulted, \
                 and a grant in another mesh is not a grant here",
                caller.name, caller.name, caller.name
            ),
        ),
        None => (
            -32010,
            "mail deposit refused: this method requires the caller be identified via a verified, \
             per-request SIGNED request from a paired node — pair first via `aoide pair`, then \
             `node allow <name> message on` ON THE RECEIVING host (the sender's own grant is not \
             consulted)"
                .to_string(),
        ),
    }
}

/// The answer to one deposit, the same whichever door took it: a plaintext
/// `mailDeposit` and the destination of a sealed container both answer through
/// here. Every refusal names its word and a fixed detail; none names a box.
fn deposit_answer(outcome: &aoide_storage::mail::DepositOutcome, from_node: &str) -> Value {
    use aoide_storage::mail::DepositOutcome;
    match outcome {
        DepositOutcome::Filed { msgid, .. } => json!({ "status": "accepted", "msgid": msgid }),
        DepositOutcome::Duplicate { .. } => json!({ "status": "duplicate" }),
        DepositOutcome::BadMsgid => json!({
            "status": "refused",
            "reason": "bad-msgid",
            "detail": "envelope msgid does not match the recomputed value",
        }),
        DepositOutcome::MalformedAck => json!({
            "status": "refused",
            "reason": "bad-msgid",
            "detail": "receipt text is not exactly one msgid: it names no letter",
        }),
        DepositOutcome::UnverifiedOrigin => json!({
            "status": "refused",
            "reason": "unverified-origin",
            "detail": format!("no key on record for `{from_node}` verifies this envelope's origin signature"),
        }),
        DepositOutcome::NotCorrespondence => json!({
            "status": "refused",
            "reason": aoide_storage::mail::REFUSAL_NOT_CORRESPONDENCE,
            "detail": "only a letter or a receipt is filed; a `charter` is applied from a sealed container",
        }),
        DepositOutcome::UnknownBox => json!({
            "status": "refused",
            "reason": aoide_storage::mail::REFUSAL_UNKNOWN_BOX,
            "detail": aoide_storage::mail::UNKNOWN_BOX_DETAIL,
        }),
        DepositOutcome::TooLarge => json!({
            "status": "refused",
            "reason": aoide_storage::mail::REFUSAL_TOO_LARGE,
            "detail": aoide_storage::mail::TOO_LARGE_DETAIL,
        }),
        DepositOutcome::BoxFull => json!({
            "status": "refused",
            "reason": aoide_storage::mail::REFUSAL_BOX_FULL,
            "detail": aoide_storage::mail::BOX_FULL_DETAIL,
        }),
    }
}

/// `aoide/mailDeposit` (P-M2): `{envelope: <the sealed Envelope, exactly as
/// aoide_storage::mail::Envelope serializes>}`. The declarations are read FIRST
/// (once per request, `config-invalid` on an unloadable set), then the caller's
/// own `down` is refused, then admission (signature-only,
/// [`deposit_admitted`]) — and the envelope's own content is
/// [`aoide_storage::mail::deposit`]'s job: recompute `msgid`, verify the ORIGIN
/// signature, dedup, file (spec item 4's short-circuiting order). The zone check
/// MAIL.md's step 3 describes belongs to the TRANSIT lane, which a plaintext
/// envelope never rides — it carries no mesh at all — so this arm skips it by
/// shape, not by omission; a sealed container takes it in
/// [`deposit_sealed`].
///
/// **Self-audits under its own label, unconditionally** (spec item 11: a
/// deposit never passes `cli/src/dispatch.rs`'s own audit, so this is the
/// one place a flood becomes visible) — mirrors [`pair_request`]'s "audits
/// every call, not only a refusal" shape, once for the admission refusal
/// and once more after `deposit`'s own outcome, never
/// [`message_send`]'s narrower "only the notable branches" one: a flood's
/// signal is volume, and volume must show whether every one of those
/// deposits was accepted, refused, or malformed.
///
/// **Never called from inside `deposit`'s own lock.** `deposit` returns
/// before this function does anything else with the outcome — every
/// `outbox` call below runs AFTER that lock has already released, never
/// nested inside it: `outbox`'s own lock wraps the identical
/// `fs::try_stage_lock` `mail`'s does, and that lock is a plain blocking
/// `flock`, not re-entrant — nesting the two would deadlock a process
/// against its own held lock, not merely contend.
///
/// A filed **letter** mints and spools an ack addressed back to the
/// origin, then best-effort drains that node once, synchronously, reusing
/// the SAME [`aoide_conduct::mail_bridge::drain_node`] the daemon tick
/// calls — one drain implementation, no duplicate dial logic. A filed
/// **receipt** is the opposite leg: [`aoide_storage::outbox::retire_by_ack`]
/// retires the local outbox entry it confirms (spec item 7) — a pure
/// storage-crate lookup keyed on the receipt's own verified `from.node`
/// and `text` (the acked msgid), so a forged or stale ack simply finds no
/// matching entry and retires nothing (see that function's own doc for
/// why the lookup alone proves both of spec item 7's checks). A
/// **duplicate** whose original filing was a letter re-sends the ack
/// (spec item 5: the sender's earlier ack evidently never arrived);
/// every other duplicate is a silent no-op — acking an ack would ping-pong
/// forever, which the vocabulary (`letter`/`receipt` only) has no third
/// shape to end.
fn mail_deposit(params: &Value, ctx: &RequestCtx) -> Result<Value, (i64, String)> {
    // MAIL.md §Status: the declarations are read ONCE per request, for the mail
    // methods only, and an unloadable set refuses here — before anything is
    // parsed, filed or hopped. Every other method never reads them.
    let declarations = match mail_declarations(ctx, "a2a.aoide/mailDeposit") {
        Ok(set) => set,
        Err(refused) => return Ok(refused),
    };
    // **Authorization precedes parsing.** An unauthorized caller gets the one
    // refusal whatever its params say — never a parse-level answer that tells
    // an unsigned prober which fields the door reads. A charter letter's gate
    // is its enclosed charter, not the grant (see [`deposit_sealed`]), so the
    // purpose is peeked from the raw params here; the container is parsed
    // only after.
    let mesh = mesh_or_refusal(
        "mail deposit refused",
        ctx.signed_caller.and_then(|c| c.mesh),
        "a2a.aoide/mailDeposit",
        ctx.audit_log,
    )?;
    // A `down` caller is refused before its grant is even read: `down` is a
    // statement about the node, and the `node allow` fix the grant refusal
    // teaches is not one a `down` node should be sent to run.
    if let Some(caller) = ctx.signed_caller {
        if let Some(refused) = down_caller_refusal(&declarations, &mesh, caller, "a2a.aoide/mailDeposit", ctx.audit_log)
        {
            return Ok(refused);
        }
    }
    let charter_letter = ctx.signed_caller.is_some()
        && params.pointer("/container/purpose").and_then(Value::as_str) == Some(aoide_storage::seal::PURPOSE_CHARTER);
    if !deposit_admitted(&caller_grant(ctx.signed_caller)) && !charter_letter {
        let (code, msg) = deposit_refusal(ctx.signed_caller, &mesh, ctx.audit_log);
        let _ = audit(ctx.audit_log, Door::A2a, EventClass::Audit, "a2a.aoide/mailDeposit", "unauthorized", &msg);
        return Err((code, msg));
    }
    // P-SEAL: a sealed container takes the same admission and then the
    // container's own two halves; a plaintext envelope is the direct-lane
    // per-peer upgrade path, unchanged from P-M2.
    if params.get("container").map(|v| !v.is_null()).unwrap_or(false) {
        return deposit_sealed(params, ctx, &declarations, &mesh);
    }
    // H1: the mail ADAPTER never carries plaintext — "no relay, hub or HTTPS
    // hop ever carries plaintext" (HTTPS-MESH-API.md). A plaintext envelope
    // still reaches a receiver over the direct SSH lane, deliberately (a peer
    // running an older aoide has no binding to publish and must be able to
    // deliver); an HTTPS hop is not that lane, so the upgrade is over before
    // anyone builds a tunnel. Refused as a RESULT carrying the taught word
    // (CONTRACTS.md §6), never a JSON-RPC error, and AUDITED — a downgrade
    // attempt is exactly what an operator wants to see in the log.
    if ctx.sealed_only {
        let detail = "sealed-required: this listener carries sealed containers only — post the \
                      `container` sealed to this node's age binding (`aoide/binding`, learned by a \
                      `aoide mail poll`); a plaintext envelope is accepted only from an admitted peer \
                      over the direct SSH lane, never through a relay or an HTTPS hop";
        let _ = audit(ctx.audit_log, Door::A2a, EventClass::Audit, "a2a.aoide/mailDeposit", "invalid", detail);
        return Ok(json!({ "status": "refused", "reason": SEALED_REQUIRED, "detail": detail }));
    }
    let envelope: aoide_storage::mail::Envelope =
        match serde_json::from_value(params.get("envelope").cloned().unwrap_or(Value::Null)) {
            Ok(e) => e,
            Err(e) => return Err((-32602, format!("invalid params: envelope: {e}"))),
        };

    let hop_name = ctx
        .signed_caller
        .map(|c| c.name)
        .expect("deposit_admitted only returns true when a signed caller resolved");

    // **A plaintext envelope is addressed to THIS box or it is refused.** `to.node`
    // is what makes a letter a letter here, and `mail::deposit` files whatever it
    // verifies — it never asks whether this box was the addressee, because on the
    // direct lane the carrier IS the destination. A depositing hop that hands over
    // an envelope addressed to a third node is asking this door to relay PLAINTEXT
    // (`addressing-mismatch`): the container lane is the one that carries a letter
    // onward, and it carries it sealed.
    let own_key = aoide_storage::identity::load_or_mint().ok().map(|(kp, _)| kp.info().pubkey_hex);
    let declared = own_key
        .as_deref()
        .and_then(|key| aoide_storage::routing::own_name_in(&declarations, &mesh, key));
    let names = [Some(aoide_storage::display::local_node_name()), declared];
    if !names.iter().flatten().any(|mine| mine == &envelope.header.to.node) {
        let detail = format!(
            "envelope is addressed to `{}`, not this node: a plaintext deposit is the direct lane's — \
             a letter in transit travels as a sealed container, never in the clear through a hop",
            envelope.header.to.node
        );
        let _ = audit(ctx.audit_log, Door::A2a, EventClass::Audit, "a2a.aoide/mailDeposit", "invalid", &detail);
        return Ok(json!({ "status": "refused", "reason": "addressing-mismatch", "detail": detail }));
    }

    let outcome = aoide_storage::mail::deposit(envelope.clone(), hop_name).map_err(|e| (-32603_i64, format!("internal error: {e}")))?;

    // Spec item 11: mail self-audits at the door (the one place a deposit
    // never passes `cli/src/dispatch.rs`'s own audit) — ONE line per call,
    // covering every outcome uniformly, mirroring `pair_request`'s own
    // "audits unconditionally, not just on refusal" shape (never
    // `message/send`'s narrower "only the notable branches" one): a flood
    // is a volume signal, and volume must be visible whether every one of
    // those deposits was accepted, refused, or malformed.
    let audit_detail = format!(
        "from {}/{} to {}/{} via {hop_name}: {outcome:?}",
        envelope.header.from.node, envelope.header.from.name, envelope.header.to.node, envelope.header.to.name
    );
    let audit_status = match &outcome {
        aoide_storage::mail::DepositOutcome::BadMsgid
        | aoide_storage::mail::DepositOutcome::MalformedAck
        | aoide_storage::mail::DepositOutcome::UnverifiedOrigin
        | aoide_storage::mail::DepositOutcome::NotCorrespondence
        | aoide_storage::mail::DepositOutcome::UnknownBox
        | aoide_storage::mail::DepositOutcome::TooLarge => "invalid",
        aoide_storage::mail::DepositOutcome::BoxFull => "refused",
        _ => "ok",
    };
    let _ = audit(ctx.audit_log, Door::A2a, EventClass::Audit, "a2a.aoide/mailDeposit", audit_status, &audit_detail);

    // Everything a filed/duplicate envelope does here — mint and spool the
    // ack, retire an entry this receipt confirms — hangs off the outcome
    // alone, so it lives in ONE shared function
    // (`aoide_client::mail_wire::settle_deposit`, through its bridge) that
    // the poll's own hand-over loop calls too. This arm only SHAPES the
    // answer.
    aoide_conduct::mail_bridge::settle_deposit(&envelope, &outcome);

    Ok(deposit_answer(&outcome, &envelope.header.from.node))
}

// ── `aoide/mailPoll` (messaging plan P-M3, CONTRACTS.md §6) ────────────────
//
// The relay-first half of the model (MAIL.md §Wire): a member with no inbound
// address asks a node it CAN reach — its declared relay, today a directly
// paired node over the SSH door — for everything that node spooled toward it.
// A pull, never a push: the caller dials out, and nothing here ever reaches
// back over a connection the caller did not open. HTTPS (a `poll` node's
// `https://` relay) lands on this same method unchanged; only the transport
// under the signed request differs.
//
// Admission is the deposit arm's own question, asked one node narrower:
// `node_may_message`, plus "and you are asking for YOUR OWN node". Handed-over
// entries stay in this box's outbox until the far end's normal ack retires
// them, so the poll is a READ of local state — it writes nothing at all, which
// is what makes a re-poll before the ack hand the same envelopes over again by
// construction rather than by a bookmark.

/// The poll arm's admission check. `caller` is the caller via a verified
/// per-request signature ([`deposit_admitted`]'s own narrowing — signature
/// only, no Addr/Token rung to fall back to); `grant` is that caller's grant
/// in the mesh its request names ([`caller_grant`]); `claimed` is
/// `params.node`, the node whose outbox the caller is asking about. Both
/// halves matter: MAIL.md §Wire's "the caller's verified identity must BE
/// `node` (no polling on another's behalf)" is the `==`, and "hold
/// `message`" is [`may_message`] — exactly "granted `message` in this mesh".
///
/// **"And not be `down`" is [`down_caller_refusal`]'s**, asked of the same
/// per-request declaration set immediately after this predicate and before
/// anything is retired or handed over: `down` is a statement about the node,
/// so it is deliberately not expressible as a grant. `aoide node allow
/// <node> message off --mesh <m>` remains the per-request quarantine that
/// lands on the `message` half.
fn poll_admitted(caller: Option<SignedCaller<'_>>, grant: &Grant, claimed: &str) -> bool {
    matches!(caller, Some(c) if may_message(grant) && c.name == claimed)
}

/// The poll arm's refusal — the SAME `-32010` `mail_deposit` uses (one code
/// for "this caller may not speak to this method", CONTRACTS.md §6), in three
/// shapes: claiming a node this request is not signed as, paired but missing
/// `message` (the same `node allow` fix deposit names), or no verified
/// signature resolution at all.
fn poll_refusal(caller: Option<SignedCaller<'_>>, mesh: &str, claimed: &str, audit_log: &Path) -> (i64, String) {
    // The charter's own refusal comes FIRST, and it is the same function the
    // deposit arm uses (H1's merge review, F1): in a charter-shaped mesh the
    // generic text below teaches `aoide node allow <name> message on --mesh
    // <m>`, which answers `widens-charter` there and changes nothing — and for
    // a charter-RESOLVED caller (no record on this host at all) it names a node
    // with no record to edit.
    if let Some(refusal) = charter_refusal("mail poll refused", mesh, "a2a.aoide/mailPoll", audit_log) {
        return refusal;
    }
    match caller {
        Some(c) if c.name != claimed => (
            -32010,
            format!(
                "mail poll refused: this request is validly signed as node `{}`, which is not the node it \
                 asks for (`{claimed}`) — a poll asks for the CALLER's own outbox, never another node's; \
                 sign as `{claimed}` or ask for `{}`",
                c.name, c.name
            ),
        ),
        Some(c) => (
            -32010,
            format!(
                "mail poll refused: node `{}` is paired and this request is validly signed, but its grant \
                 in mesh `{mesh}` does not include `message` — on this (polled) host run \
                 `aoide node allow {} message on --mesh {mesh}`; the poller's own grant is not consulted",
                c.name, c.name
            ),
        ),
        None => (
            -32010,
            "mail poll refused: this method requires the caller be identified via a verified, per-request \
             SIGNED request from a paired node — pair first via `aoide pair`"
                .to_string(),
        ),
    }
}

/// `aoide/mailPoll` (P-M3): `{ node }` → `{ envelopes: [ <Envelope>, … ] }`.
/// Every outbox entry this box spooled toward `node` that `node` itself may
/// take — all `hold`-flavored ones, and `now` ones whose own attempts have
/// been failing ([`aoide_storage::outbox::poll_payloads`]'s rule, and the ONE
/// place that rule lives). Handed over oldest-first; nothing is marked,
/// moved, or counted, and an entry leaves the spool only when its ack lands
/// (`mail_deposit`'s receipt arm, the same retirement a push would earn).
///
/// **Self-audits under its own `a2a.aoide/mailPoll` label, unconditionally**
/// (MAIL.md item 13's one-door-one-inspection, per method name; the audit
/// name whitelist in the connection handler gains this name so a poll never
/// logs as bare `a2a.rpc`) — one line per call whether it was refused or
/// answered, carrying the caller and how many envelopes it was handed. That
/// count is the flood signal for the pull direction: a relay handing one node
/// hundreds of letters in one answer is visible here, at the node that
/// spooled them.
fn mail_poll(params: &Value, ctx: &RequestCtx) -> Result<Value, (i64, String)> {
    let claimed = params.get("node").and_then(Value::as_str).unwrap_or("").trim().to_string();
    // MAIL.md §Status: the declarations are read ONCE per request, for the two
    // mail methods only, and an unloadable set refuses both — this one before
    // anything is retired or handed over. Authorization precedes shape: an
    // unauthorized caller gets the one refusal whatever its params say.
    let declarations = match mail_declarations(ctx, "a2a.aoide/mailPoll") {
        Ok(set) => set,
        Err(refused) => return Ok(refused),
    };
    // What the poller says it FILED out of its last poll, if it says anything:
    // the acknowledgement that ends this hub's custody of a container it handed
    // over (`outbox::retire_acknowledged` — a `transit` entry, and nothing else).
    // A response can be lost, so the hand-over is not an acknowledgement, and an
    // unacknowledged entry is offered again below.
    let acknowledged: Vec<String> = params
        .get("filed")
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default();

    let mesh = mesh_or_refusal(
        "mail poll refused",
        ctx.signed_caller.and_then(|c| c.mesh),
        "a2a.aoide/mailPoll",
        ctx.audit_log,
    )?;
    let grant = caller_grant(ctx.signed_caller);
    let named = if claimed.is_empty() { ctx.signed_caller.map(|c| c.name.to_string()).unwrap_or_default() } else { claimed.clone() };
    if !poll_admitted(ctx.signed_caller, &grant, &named) {
        let (code, msg) = poll_refusal(ctx.signed_caller, &mesh, &named, ctx.audit_log);
        let _ = audit(ctx.audit_log, Door::A2a, EventClass::Audit, "a2a.aoide/mailPoll", "unauthorized", &msg);
        return Err((code, msg));
    }
    // A `down` poller is refused exactly as a `down` depositor is: its own door
    // requests stop until the declaration changes (MAIL.md §Status).
    if let Some(caller) = ctx.signed_caller {
        if let Some(refused) =
            down_caller_refusal(&declarations, &mesh, caller, "a2a.aoide/mailPoll", ctx.audit_log)
        {
            return Ok(refused);
        }
    }
    if claimed.is_empty() {
        return Err((-32602, "invalid params: node is required".to_string()));
    }
    let poller = ctx.signed_caller.map(|c| c.name.to_string()).unwrap_or_default();

    // What the poller says it was handed and did NOT file (its box was full, or
    // it could not take the bytes yet). Each msgid that is a well-formed msgid
    // defers that entry in THIS poller's own spool, so it is not offered again at
    // the head of every batch; an old hub never reads the key (MAIL.md §Wire).
    let deferred: Vec<String> = params
        .get("deferred")
        .and_then(Value::as_array)
        .map(|list| {
            let mut seen = std::collections::HashSet::new();
            list.iter()
                .filter_map(Value::as_str)
                .filter(|text| aoide_storage::mail::is_msgid(text))
                .filter(|text| seen.insert(*text))
                .take(aoide_storage::outbox::DEFERRED_LIST_CAP)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    aoide_storage::outbox::defer_named(&poller, &deferred)
        .map_err(|e| (-32603_i64, format!("internal error: {e}")))?;
    let payloads = aoide_storage::outbox::poll_payloads(&poller)
        .map_err(|e| (-32603_i64, format!("internal error: {e}")))?;
    // The acknowledgements land FIRST, so what this answer offers is what the
    // poller has NOT yet said it has: an entry whose response was lost is offered
    // again rather than retired into silence.
    let mut retired: Vec<String> = Vec::new();
    for msgid in &acknowledged {
        // **A retirement is a write, and rule 6 puts it in the log.** The
        // poller's own acknowledgement is what ends this hub's custody of a
        // container, so an operator reading the log sees which letters stopped
        // being held here and on whose word.
        if aoide_storage::outbox::retire_acknowledged(&poller, msgid)
            .map_err(|e| (-32603_i64, format!("internal error: {e}")))?
        {
            retired.push(msgid.clone());
        }
    }
    // P-SEAL: one answer, two lists. A sealed entry's container is what a
    // destination holding this node's binding needs; the plaintext envelope
    // rides beside it for one that has published none, which is the per-peer
    // upgrade path on the pull direction. A sealed entry deliberately hands
    // over NO envelope list — the plaintext is inside `ct` and handing it
    // out again would put letter bytes on the wire for a destination that
    // asked to be sealed to.
    //
    // H1 (the branch review): an entry spooled before this node held the
    // poller's binding is upgraded HERE, before it is handed over, and the
    // upgrade is persisted — the same rule the drain applies in the other
    // direction.
    //
    // H2 (the re-review): the decision is made against the LIVE spool, entry
    // by entry, not against the snapshot `poll_payloads` offered. Between the
    // two a drain can retire the entry or seal it concurrently, and handing
    // over the snapshot's envelope then would put a stale plaintext on the
    // wire for a destination that holds a binding. An entry that is gone, or
    // whose destination holds a binding that is not usable now, hands over
    // NOTHING.
    let mut containers: Vec<aoide_storage::seal::Container> = Vec::new();
    let mut envelopes: Vec<aoide_storage::mail::Envelope> = Vec::new();
    // H1 (the branch review): sealed-only binds the PULL direction too. An
    // entry spooled toward a poller that held no binding when it was written
    // hands over as a plaintext envelope, and on the ADAPTER that is the one
    // shape this listener must never carry — "no relay, hub or HTTPS hop ever
    // carries plaintext" is an absolute (HTTPS-MESH-API.md), and the
    // `exchange_bindings` at the top of every poll is deliberately
    // best-effort, so nothing else could enforce it. Withheld, never
    // answered: the entry stays spooled (that path writes nothing — see
    // `hand_over`'s `Reseal::NoBinding` arm), and the answer NAMES it with
    // the taught word, so an operator sees why their letters are not moving.
    let mut withheld: Vec<Value> = Vec::new();
    for (_, offered) in payloads {
        match aoide_storage::outbox::hand_over(&poller, &offered.msgid)
            .map_err(|e| (-32603_i64, format!("internal error: {e}")))?
        {
            aoide_storage::outbox::HandOver::Container(container) => {
                containers.push(*container);
            }
            aoide_storage::outbox::HandOver::Envelope(envelope) => {
                if ctx.sealed_only {
                    withheld.push(json!({
                        "msgid": envelope.msgid,
                        "reason": SEALED_REQUIRED,
                        "detail": "this listener hands over sealed containers only: an entry spooled toward \
                                   you before you published an age binding cannot cross an HTTPS hop in the \
                                   clear. Publish one (a signed `aoide/binding`, which a `aoide mail poll` \
                                   exchanges) and poll again — the entry stays spooled until then",
                    }));
                } else {
                    envelopes.push(*envelope);
                }
            }
            aoide_storage::outbox::HandOver::Nothing => {}
        }
    }

    let _ = audit(
        ctx.audit_log,
        Door::A2a,
        EventClass::Audit,
        "a2a.aoide/mailPoll",
        "ok",
        &format!(
            "node {poller} polled: {} container(s) and {} plaintext envelope(s) handed over{}, {} \
             acknowledged and retired{}",
            containers.len(),
            envelopes.len(),
            if withheld.is_empty() {
                String::new()
            } else {
                format!(", {} withheld ({SEALED_REQUIRED})", withheld.len())
            },
            retired.len(),
            if retired.is_empty() {
                String::new()
            } else {
                format!(" ({})", retired.join(", "))
            }
        ),
    );
    let mut answer = json!({ "containers": containers, "envelopes": envelopes });
    if !withheld.is_empty() {
        // Additive, and only when it bites: the door's own answer (which can
        // never withhold) stays byte-identical.
        answer["withheld"] = json!(withheld);
    }
    Ok(answer)
}

/// The name this door uses for a caller — in policy, in routing and in the audit
/// stamp: the declaration's own name for the verifying key
/// (`aoide_storage::routing::declared_name`), which for a CHARTER mesh is the
/// name on the charter LINE and never the caller's `nodes.json` nickname or the
/// name it wrote in its own header. Only where the mesh names no such key does
/// the resolved record's name stand — a pair mesh's record IS its declaration, so
/// there the two are the same answer anyway. Read over the set the request
/// already loaded, so one request resolves a name once.
fn declared_caller_name(
    set: &[aoide_storage::routing::Loaded],
    mesh: &str,
    caller: SignedCaller<'_>,
) -> String {
    aoide_storage::routing::declared_name(set, mesh, caller.key)
        .unwrap_or_else(|| caller.name.to_string())
}

/// The sealed half of `aoide/mailDeposit` (P-SEAL): the same admission the
/// plaintext arm runs, then `seal::deposit_container`'s shared steps and
/// destination branch. A refusal carries the taught word CONTRACTS.md §6
/// names; an opened container is handed to the SAME `mail::deposit` the
/// plaintext arm uses, so filing, the seen set and the receipt rule have one
/// implementation and not two.
///
/// `declarations` is the ONE set this request read
/// ([`mail_declarations`]), threaded through to
/// [`aoide_storage::seal::deposit_container_over`]: the `down` gate, the name the
/// caller is judged by and the deposit's own zone checks all read the same set.
fn deposit_sealed(
    params: &Value,
    ctx: &RequestCtx,
    declarations: &[aoide_storage::routing::Loaded],
    request_mesh: &str,
) -> Result<Value, (i64, String)> {
    let container: aoide_storage::seal::Container =
        match serde_json::from_value(params.get("container").cloned().unwrap_or(Value::Null)) {
            Ok(c) => c,
            Err(e) => return Err((-32602, format!("invalid params: container: {e}"))),
        };

    // A charter container is admitted on a signed caller alone only for the
    // box it is addressed to. For another node it is a transit hop, which needs
    // the grant — decided HERE, before the container is verified, deduped or
    // routed, so a caller without one learns nothing from what those steps say.
    if container.purpose == aoide_storage::seal::PURPOSE_CHARTER
        && !deposit_admitted(&caller_grant(ctx.signed_caller))
        && !aoide_storage::seal::addressed_here(&container, request_mesh, declarations)
            .map_err(|e| (-32603_i64, format!("internal error: {e}")))?
    {
        let (code, msg) = deposit_refusal(ctx.signed_caller, request_mesh, ctx.audit_log);
        let _ = audit(ctx.audit_log, Door::A2a, EventClass::Audit, "a2a.aoide/mailDeposit", "unauthorized", &msg);
        return Err((code, msg));
    }
    // P-CHARTER (review finding 4, and the design ruling it carries): the
    // mesh a letter rides is checked ONE layer down, inside
    // `seal::deposit_container_over`, AFTER the origin signature and against
    // SIGNED values only (`ctx.origin_mesh` versus the mesh THIS request's
    // per-request signature covers). It is deliberately not checked here
    // against `container.mesh`: that field is hop-mutable by design, so a
    // spooling relay or a TLS edge could turn an accepted deposit into a
    // permanent refusal by flipping one unsigned byte — a cheap mail-delivery
    // DoS. The request's own mesh is what admission reads the grant in.
    // Admission (mesh, `down`, the grant — or a charter letter's own gate) ran
    // in [`mail_deposit`] before this container was parsed.
    let hop_name = {
        let caller = ctx
            .signed_caller
            .expect("deposit_admitted only returns true when a signed caller resolved");
        declared_caller_name(declarations, request_mesh, caller)
    };

    // The chain's last hop is the node that deposited it — the caller this door
    // verified, by the name its mesh gives it. A caller presenting somebody
    // else's hand-over is refused before anything is opened, filed or hopped.
    if let Err(refusal) = aoide_storage::seal::chain_deposited_by(&container, &hop_name) {
        let _ = audit(
            ctx.audit_log,
            Door::A2a,
            EventClass::Audit,
            "a2a.aoide/mailDeposit",
            "invalid",
            &format!("sealed msgid {} via {hop_name}: {}: {}", container.msgid, refusal.reason, refusal.detail),
        );
        return Ok(json!({ "status": "refused", "reason": refusal.reason, "detail": refusal.detail }));
    }

    let outcome = aoide_storage::seal::deposit_container_over(&container, request_mesh, declarations)
        .map_err(|e| (-32603_i64, format!("internal error: {e}")))?;

    let (audit_status, audit_detail) = match &outcome {
        aoide_storage::seal::ContainerOutcome::Opened { envelope, .. } => (
            "ok",
            format!(
                "sealed from {}/{} to {}/{} via {hop_name}: opened",
                envelope.header.from.node,
                envelope.header.from.name,
                envelope.header.to.node,
                envelope.header.to.name
            ),
        ),
        aoide_storage::seal::ContainerOutcome::Duplicate { .. } => {
            ("ok", format!("sealed msgid {} via {hop_name}: duplicate", container.msgid))
        }
        aoide_storage::seal::ContainerOutcome::Applied { mesh, version, rekeyed, .. } => (
            "ok",
            format!(
                "charter `{mesh}` v{version} from `{}` via {hop_name}: applied{}",
                container.origin.node,
                if rekeyed.is_empty() {
                    String::new()
                } else {
                    format!(
                        " (RE-KEYED: {})",
                        rekeyed.iter().map(|r| r.node.as_str()).collect::<Vec<_>>().join(", ")
                    )
                }
            ),
        ),
        aoide_storage::seal::ContainerOutcome::Refused { reason, detail } => {
            ("invalid", format!("sealed msgid {} via {hop_name}: {reason}: {detail}", container.msgid))
        }
        // A hop: this box is not the destination, so the letter is carried on
        // rather than opened. Its own audit line is written by the arm below,
        // AFTER `file_transit_hop` — the write is what the line is about, so a
        // filing that failed must not read as one that happened.
        aoide_storage::seal::ContainerOutcome::Hopped(_) => ("", String::new()),
    };
    if !audit_status.is_empty() {
        let _ = audit(ctx.audit_log, Door::A2a, EventClass::Audit, "a2a.aoide/mailDeposit", audit_status, &audit_detail);
    }

    match outcome {
        aoide_storage::seal::ContainerOutcome::Refused { reason, detail } => {
            Ok(json!({ "status": "refused", "reason": reason, "detail": detail }))
        }
        // A hop: nothing is filed as correspondence, no reader is rung and NO
        // ACK is minted — the letter is not here, it is on its way, and only the
        // destination's own receipt may tell an origin otherwise. The container
        // is filed as `transit` and spooled toward `next`; the answer is the
        // hop taking custody, which is exactly what `accepted` means to a drain
        // (the entry stays spooled until a real receipt retires it).
        aoide_storage::seal::ContainerOutcome::Hopped(hop) => {
            // A charter letter for THIS box is admitted on its enclosed charter
            // alone (above); one for another node is this box's custody, and
            // a relay carries on what its grants admit and nothing else.
            if container.purpose == aoide_storage::seal::PURPOSE_CHARTER && !deposit_admitted(&caller_grant(ctx.signed_caller)) {
                let (code, msg) = deposit_refusal(ctx.signed_caller, request_mesh, ctx.audit_log);
                let _ = audit(ctx.audit_log, Door::A2a, EventClass::Audit, "a2a.aoide/mailDeposit", "unauthorized", &msg);
                return Err((code, msg));
            }
            // The write comes FIRST, and the audit line follows it: a `transit`
            // entry the hub could not write is not a hop, and an audit that said
            // "next `chiyo`" about it would be the one record of a letter that is
            // nowhere.
            match aoide_storage::seal::file_transit_hop(&hop) {
                Ok(()) => {
                    let _ = audit(
                        ctx.audit_log,
                        Door::A2a,
                        EventClass::Audit,
                        "a2a.aoide/mailDeposit",
                        "ok",
                        &format!(
                            "sealed transit msgid {} via {hop_name}: next `{}` in mesh `{}`{}",
                            hop.container.msgid,
                            hop.next,
                            hop.mesh,
                            if hop.held { ", HELD for its own poll" } else { "" }
                        ),
                    );
                }
                Err(e) => {
                    let _ = audit(
                        ctx.audit_log,
                        Door::A2a,
                        EventClass::Audit,
                        "a2a.aoide/mailDeposit",
                        "invalid",
                        &format!("sealed transit msgid {} via {hop_name}: NOT FILED: {e}", hop.container.msgid),
                    );
                    return Err((-32603_i64, format!("internal error: {e}")));
                }
            }
            if !hop.held {
                // Best-effort, exactly like the destination's own post-filing
                // drain. The container is on this box's disk now, so a dial that
                // fails costs a later pass (the daemon's sweep) and nothing else.
                let _ = aoide_conduct::mail_bridge::drain_node(&hop.next);
            }
            Ok(json!({
                "status": "accepted",
                "msgid": hop.container.msgid,
                "transit": { "next": hop.next, "mesh": hop.mesh, "held": hop.held },
            }))
        }
        aoide_storage::seal::ContainerOutcome::Duplicate { filed_letter } => {
            // **A hub that is holding this container but cannot move it says so.**
            // Its own custody being parked — its next hop refused, an older relay
            // — is information the DEPOSITING hop needs: answering `duplicate`
            // forever would leave the origin retrying against a letter that is
            // going nowhere, with nothing anywhere saying why (the hub's own
            // `mail outbox` is the only place that knew). The word is the one the
            // hub's next hop gave, so the chain of custody reads the same refusal.
            if let Some(word) = aoide_storage::outbox::parked_transit_refusal(&container.msgid) {
                let detail = format!(
                    "this hop holds the container and cannot move it on: its own next hop answered \
                     `{word}`. The letter is parked here (`aoide mail outbox retry --refused` on this \
                     node) and nothing about the letter itself is wrong"
                );
                let _ = audit(
                    ctx.audit_log,
                    Door::A2a,
                    EventClass::Audit,
                    "a2a.aoide/mailDeposit",
                    "invalid",
                    &format!("sealed transit msgid {} via {hop_name}: parked: {word}", container.msgid),
                );
                return Ok(json!({ "status": "refused", "reason": word, "detail": detail }));
            }
            // Nothing is opened here — that is the point of deciding dedup
            // before the open. The receipt the first delivery never got is
            // recovered from the FILED record instead, which is where the
            // opened envelope already lives, so a lost ack costs no second
            // open and no second filing.
            if filed_letter {
                if let Ok(Some(entry)) = aoide_storage::mail::show(&container.msgid) {
                    aoide_conduct::mail_bridge::settle_deposit(
                        &entry.envelope,
                        &aoide_storage::mail::DepositOutcome::Duplicate { filed_letter: true },
                    );
                }
            }
            Ok(json!({ "status": "duplicate" }))
        }
        aoide_storage::seal::ContainerOutcome::Applied { mesh, version, rekeyed, digest } => {
            // P-CHARTER: the enclosed charter verified and is in force —
            // `deposit_container` wrote it before returning. Nothing is filed
            // and nothing is acked; the container is recorded once so a
            // re-offered copy is answered `duplicate` rather than applied
            // twice, exactly as an opened letter is.
            //
            // The answer is `"accepted"` — MAIL.md §Wire's closed three-word
            // vocabulary, and the ONLY word a sender retires an entry on. The
            // charter detail rides in `data` beside it, where a word a sender's
            // classifier has never heard of can never make the far end read a
            // landed charter as a refusal (and leave its own outbox saying so
            // forever).
            if let Err(e) = aoide_storage::seal::record_admitted(&container, &digest) {
                let _ = audit(
                    ctx.audit_log,
                    Door::A2a,
                    EventClass::Audit,
                    "a2a.aoide/mailDeposit",
                    "invalid",
                    &format!("dedup record write failed for {}: {e}", container.msgid),
                );
            }
            for rekeyed in &rekeyed {
                // A new key on an old name is what a stolen operator key would
                // sign: every one is audited where the charter landed.
                let _ = audit(
                    ctx.audit_log,
                    Door::A2a,
                    EventClass::Audit,
                    "a2a.aoide/mailDeposit",
                    "rekeyed",
                    &format!(
                        "charter `{mesh}` v{version} re-keyed `{}`: {} -> {}",
                        rekeyed.node, rekeyed.from, rekeyed.to
                    ),
                );
            }
            Ok(json!({
                "status": "accepted",
                "msgid": container.msgid,
                "charter": { "mesh": mesh, "version": version, "rekeyed": rekeyed },
            }))
        }
        aoide_storage::seal::ContainerOutcome::Opened { envelope, .. } => {
            // The destination files into its own box and dedups there. It never
            // writes the pre-open gate: that record belongs to a hub's hop.
            let filed = aoide_storage::mail::deposit((*envelope).clone(), &hop_name)
                .map_err(|e| (-32603_i64, format!("internal error: {e}")))?;
            aoide_conduct::mail_bridge::settle_deposit(&envelope, &filed);
            Ok(deposit_answer(&filed, &envelope.header.from.node))
        }
    }
}

/// `aoide/binding` (P-SEAL, CONTRACTS.md §6): `{ binding? }` →
/// `{ binding }`. A signed read of THIS node's own current binding, and —
/// when the caller includes its own — the moment this node stores the
/// caller's, so one authenticated round trip upgrades both ends.
///
/// Admission is `mailDeposit`'s own question (a verified caller holding
/// `message`): the binding is what a peer needs before it can seal anything
/// TO this node, so refusing it to a node that may already deposit mail
/// would leave the two halves of the same exchange gated differently.
fn node_binding(params: &Value, ctx: &RequestCtx) -> Result<Value, (i64, String)> {
    let mesh = mesh_or_refusal(
        "binding refused",
        ctx.signed_caller.and_then(|c| c.mesh),
        "a2a.aoide/binding",
        ctx.audit_log,
    )?;
    let grant = caller_grant(ctx.signed_caller);
    if !deposit_admitted(&grant) {
        let (code, msg) = deposit_refusal(ctx.signed_caller, &mesh, ctx.audit_log);
        let _ = audit(ctx.audit_log, Door::A2a, EventClass::Audit, "a2a.aoide/binding", "unauthorized", &msg);
        return Err((code, msg));
    }
    let caller = ctx.signed_caller.map(|c| c.name.to_string()).unwrap_or_default();

    if let Some(raw) = params.get("binding").filter(|v| !v.is_null()) {
        match serde_json::from_value::<aoide_storage::seal::Binding>(raw.clone()) {
            Ok(binding) => {
                // `stale-binding` and `binding-mismatch` are refusals the
                // CALLER must see — its own binding was not accepted as
                // current, and it has to know before it starts sealing.
                //
                // L4 (the branch review): the audit line comes FIRST. The
                // contract is that this method audits unconditionally, and a
                // refusal that returns before the audit is the one line an
                // operator most needs — a peer repeatedly publishing a
                // superseded binding is exactly what a downgrade attempt
                // looks like from here.
                if let Err(e) = aoide_storage::seal::learn_binding(&caller, &binding) {
                    let _ = audit(
                        ctx.audit_log,
                        Door::A2a,
                        EventClass::Audit,
                        "a2a.aoide/binding",
                        "invalid",
                        &format!("binding REFUSED for {caller}: {e}"),
                    );
                    return Err((-32010_i64, format!("binding refused: {e}")));
                }
            }
            Err(e) => {
                let _ = audit(
                    ctx.audit_log,
                    Door::A2a,
                    EventClass::Audit,
                    "a2a.aoide/binding",
                    "invalid",
                    &format!("malformed binding from {caller}: {e}"),
                );
                return Err((-32602, format!("invalid params: binding: {e}")));
            }
        }
    }

    let mine = aoide_storage::seal::publish_binding()
        .map_err(|e| (-32603_i64, format!("internal error: {e}")))?;
    let _ = audit(
        ctx.audit_log,
        Door::A2a,
        EventClass::Audit,
        "a2a.aoide/binding",
        "ok",
        &format!(
            "binding generation {} to {caller} (age fingerprint {})",
            mine.generation, mine.age_fingerprint
        ),
    );
    Ok(json!({ "binding": mine }))
}

/// The mail family's whole method table — the three methods the H1 mail
/// adapter serves, and the door's own three mail arms, as ONE function.
///
/// `None` is not a refusal: it means "this name is not a mail method at all".
/// The door falls through to its own table below; the adapter
/// ([`serve_mail`]) answers `-32601`. Extracting it is what keeps the two
/// listeners from drifting on what a mail call MEANS — the adapter's dispatch
/// table *is* this function, never a second copy of the three arms.
fn mail_rpc(method: &str, params: &Value, ctx: &RequestCtx) -> Option<Result<Value, (i64, String)>> {
    match method {
        "aoide/mailDeposit" => Some(mail_deposit(params, ctx)),
        "aoide/mailPoll" => Some(mail_poll(params, ctx)),
        // P-SEAL: the binding exchange. A signed READ of this node's own
        // current binding, and — when the caller includes its own — the
        // moment this node stores the caller's. One round trip both ways,
        // which is what a direct lane wants and what the per-peer upgrade
        // needs before the first sealed letter can go anywhere.
        "aoide/binding" => Some(node_binding(params, ctx)),
        _ => None,
    }
}

// ── The pairing ceremony wire (CONTRACTS.md §6, P-P2,
// ── `docs/architecture/PAIRING.md`) ─────────────────────────────────────────
//
// Four JSON-RPC methods, all deliberately UNAUTHENTICATED at the DOOR level
// (no token/origin gate) — this IS the bootstrap: there is no established
// pairing yet to gate `read_ok` against, and a parked/relayed request grants
// NOTHING by itself (PAIRING.md: "a parked request grants NOTHING until
// approved"). P-P4 (signed wire requests) is the later phase that adds real
// authentication to paired callers; unpaired bootstrap traffic like this
// stays exactly as open as `message/send`'s own unauthenticated read arms
// were before any token was ever configured. `aoide/pairPoll` (below) is
// "unauthenticated" in that same door-level sense only — it carries and
// verifies its OWN signature inline, unlike the other three.
//
// `aoide/pairRequest` is the REQUESTER -> APPROVER direction: box A asks box
// B to park a pairing request, carrying a COMMITMENT to A's own nonce, not
// the nonce itself (review-bounce Finding 1 — the original shape let an
// active on-path attacker choose four of the SAS transcript's six fields
// after observing the real ones; see `aoide_storage::pairing`'s module doc
// for the full commit-then-reveal reasoning, the Bluetooth SSP idiom this
// borrows). `aoide/pairReveal` is A's immediate follow-up (same `pair`
// invocation, two sequential POSTs) that hands over the nonce the
// commitment already fixed; B verifies it and only THEN has a SAS to show.
//
// `aoide/pairPoll` (Design A, task #119 — REPLACES the old `aoide/pairApprove`
// reverse callback) is A's own follow-up, POSTed to B's door over the SAME
// forward dial `aoide/pairRequest`/`aoide/pairReveal` already used — never a
// callback B initiates back to A. B's own operator approving
// (`pair <id>` on the inbound entry) is now PURELY LOCAL: it commits
// B's own node record for A and marks B's parked entry `approved`
// ([`aoide_storage::pairing::mark_inbound_approved`]) but dials nobody. A's
// `pair <id>` then POLLS this method until it sees `approved`,
// verifies the release is bound to the SAME transcript A already committed
// to, and only THEN runs its own confirm-then-commit. This is the whole
// point: a REQUESTER whose own A2A door is loopback-only ([[doors-loopback-only]])
// can now complete pairing, because nothing ever needs to dial IN to it —
// see [`pair_poll`]'s own doc for the auth mechanism and the existence-oracle
// discipline it holds.

/// [`ConnOrigin`] rendered for DISPLAY only — [`InboundPairingRequest`]'s
/// `originAddr` field (the bare `pair` pending listing's own column, PAIRING.md: "parks
/// pending (id, ... origin addr)"). Never a security decision in this
/// phase — there is no pairing yet to gate origin against.
fn origin_display(origin: ConnOrigin) -> String {
    match origin {
        ConnOrigin::Loopback => "loopback".to_string(),
        ConnOrigin::Remote(ip) => ip.to_string(),
        ConnOrigin::Unknown => "unknown".to_string(),
    }
}

/// A lowercase-hex string of exactly `len` characters. The one shape check
/// [`valid_pubkey_hex`]/[`valid_nonce_hex`]/[`valid_commit_hex`] each pin to
/// a different fixed length — pubkeys and commitments are both full
/// SHA-256/ed25519-key-shaped (64 hex chars), nonces are half that (32 hex
/// chars, 16 bytes) — factored once so the three validators can't drift out
/// of sync with each other's character-class check. Pure.
fn valid_hex(s: &str, len: usize) -> bool {
    s.len() == len && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// A 64-lowercase-hex-char ed25519 public key, exactly as
/// [`aoide_storage::identity::IdentityInfo::pubkey_hex`] renders one. Pure.
fn valid_pubkey_hex(s: &str) -> bool {
    valid_hex(s, 64)
}

/// A 32-lowercase-hex-char (16-byte) nonce, exactly as
/// [`aoide_storage::pairing::random_hex(16)`] renders one — both this door
/// and the client crate mint nonces the same way, so this length is a fixed
/// contract, not a range. Pure.
fn valid_nonce_hex(s: &str) -> bool {
    valid_hex(s, 32)
}

/// A 64-lowercase-hex-char commitment, exactly as
/// [`aoide_storage::pairing::derive_commit`] renders one (a full SHA-256
/// digest, hex-encoded — the same length as a pubkey, a coincidence of both
/// being 32-byte digests, not a shared meaning). Pure.
fn valid_commit_hex(s: &str) -> bool {
    valid_hex(s, 64)
}

/// A URL sane enough to remember for the resulting node record's stored
/// address (FUTURE non-ceremony calls — `message/send`, `aoide/graphSummary`
/// pulls, spawn) — no scheme/host validation beyond "looks like a URL and
/// isn't absurdly long" (`post_json`/curl, `aoide-client`'s job, will fail
/// loudly on anything genuinely malformed when a real call is dialed).
/// Design A (task #119): the ceremony's OWN completion no longer dials this
/// URL at all (`aoide/pairPoll` reverses the direction — see the section doc
/// above), so this validation exists purely for the node record's own future
/// use, not for anything the ceremony itself does synchronously. Pure.
fn valid_node_url(s: &str) -> bool {
    !s.is_empty() && s.len() <= 2048 && s.contains("://")
}

/// The door URL THIS `a2a serve` process is actually answering on, derived
/// from `bind`/`port` — [`route`]'s own `aoide/graphSummary` handling
/// builds its `self_url` from this one formula. (The discovery
/// advertisement carries NO door URL at all — task #120's rendezvous-not-
/// authentication stance, `aoide_storage::advertise`'s module doc.)
fn self_url(bind: &str, port: u16) -> String {
    format!("http://{bind}:{port}/")
}

/// Best-effort emit onto aoided's own events feed for a pairing-ceremony
/// milestone (P-P5, CONTRACTS.md §6's "Pairing events feed" subsection):
/// `pair-parked` and `pair-revealed` are the two live kinds. The third,
/// `pair-awaiting-confirm`, is RETIRED with the callback that emitted it
/// (task #119 — approval is learned by the requester's own poll, which
/// runs client-side where this door-side feed writer never sees it);
/// `parse_pair_line` still reads it for old feed lines. Never `?`,
/// never panics — the posture
/// `aoide_secrets::broker::emit_notify` already holds, because a
/// notification write must never fail or block the ceremony itself
/// ([`FeedWriter::append`] is already best-effort internally; this
/// wrapper's own job is only to resolve the path and shape the record).
///
/// `a2a serve` is a SEPARATE process from `aoided` (module doc's DI-seam
/// note — this crate has no dependency on the resident daemon's runtime,
/// only its path/cap resolvers), so it opens its OWN [`FeedWriter`] onto
/// the SAME `$XDG_RUNTIME_DIR/aoide/events.jsonl` `aoided` already writes
/// through (`crate::daemon::events_path`) rather than routing through the
/// daemon process. Two independent writers sharing one capped,
/// truncate-in-place file means a cap-truncate race at the 1 MiB boundary
/// can lose a line — accepted, because the feed is ephemeral cues, not
/// the durable record (that stays the audit log, already written at every
/// one of these three call sites); the watcher's actual authority is
/// `aoide_storage::pairing::list_inbound`/`list_outbound`, and this feed
/// line is only ever a trigger to re-check them, never itself trusted
/// data.
///
/// `payload` carries fields BY NAME ONLY (`id`, `name`, `originAddr`,
/// `url`, `direction`) — never a SAS, pubkey, nonce, or commitment; the
/// watcher re-derives the SAS locally from its own identity plus
/// `list_inbound`/`list_outbound`, so nothing secret-shaped ever needs to
/// ride this line.
fn emit_pairing_event(kind: &str, payload: Value) {
    let events_path = crate::daemon::events_path(&crate::daemon::socket_path());
    let feed = aoide_protocol::feed::FeedWriter::new(events_path, crate::daemon::EVENTS_CAP_BYTES, 0o600);
    feed.append(&json!({
        "v": 0,
        "ts": aoide_protocol::audit::now_secs(),
        "class": serde_json::to_value(EventClass::Gate).unwrap_or_else(|_| json!("gate")),
        "kind": kind,
        "source": "a2a-door",
        "payload": payload,
    }));
}

/// `aoide/charterFetch` (P-CHARTER): **the LAN join's one read.** A machine
/// that trusts nothing yet asks a machine it can reach on the local network
/// for a mesh's operator key and the charter in force, so it can enter the
/// mesh in one ceremony instead of hand-copying a key and a file.
///
/// What it returns is public material — an operator PUBLIC key and a charter
/// document that is already operator-signed — so it carries no bearer and no
/// per-request signature, exactly like `aoide/pairRequest`'s own bootstrap
/// answer. Its AUTHORITY is not this method at all: the caller verifies the
/// charter's signature under the key it just received (which proves only that
/// that key signed that charter — the trust step is the fingerprint the
/// operator compares out of band, the same class as pairing's typed code) and
/// then records the key in STATE through `aoide_storage::charter::
/// trust_operator`.
///
/// **The local-network guard is the whole access rule**, and it is the
/// approved portable one (`aoide_storage::charter::is_local_network`): the
/// observed peer must be a private or link-local address, and loopback is
/// REFUSED, because a relayed forward and an ssh tunnel both arrive as
/// loopback here — admitting loopback would admit every relay. A refused peer
/// answers `-32007` with a taught message naming the non-LAN paths, never a
/// silent empty result.
///
/// A mesh this node itself has no charter for is refused with a taught
/// message (the caller reached the wrong machine, or the name is a typo) —
/// this method never mints, never signs and never writes: a LAN join must not
/// be able to change the operator's machine.
fn charter_fetch(params: &Value, origin: ConnOrigin, audit_log: &Path) -> Result<Value, (i64, String)> {
    let mesh = params.get("mesh").and_then(Value::as_str).unwrap_or("");
    if !aoide_storage::node_store::valid_node_name(mesh) {
        return Err((-32602, format!("invalid params: mesh `{mesh}` is not a mesh name")));
    }
    let non_lan = match origin {
        ConnOrigin::Remote(ip) if aoide_storage::charter::is_local_network(&ip.to_string()) => None,
        ConnOrigin::Remote(ip) => Some(ip.to_string()),
        ConnOrigin::Loopback => Some("127.0.0.1".to_string()),
        ConnOrigin::Unknown => Some("an undetermined address".to_string()),
    };
    if let Some(peer) = non_lan {
        let detail = format!(
            "a LAN join from {peer}: `aoide/charterFetch` is admitted only over a local network \
             (private or link-local addresses, never loopback — a relayed or tunneled request \
             arrives from loopback). Take the mesh without a LAN: `aoide mesh join {mesh} \
             --operator <key>` with the operator's own operator line, or `aoide mesh charter \
             accept <file>` with the signed file they hand you"
        );
        let _ = audit(audit_log, Door::A2a, EventClass::Audit, "a2a.aoide/charterFetch", "unauthorized", &detail);
        return Err((-32007, detail));
    }
    let Some(charter) = aoide_storage::charter::in_force_charter(mesh) else {
        return Err((
            -32000,
            format!(
                "mesh `{mesh}` has no charter in force on this machine — the operator must run \
                 `aoide mesh charter init {mesh}` and `aoide mesh charter sign {mesh}` there first"
            ),
        ));
    };
    let operator = aoide_storage::charter::trusted_operator(mesh)
        .map_err(|r| (-32000, format!("this machine cannot decide which key signs `{mesh}`: {r}")))?;
    let bytes = std::fs::read(aoide_storage::charter::in_force_path(mesh))
        .map_err(|e| (-32603, format!("reading the charter in force: {e}")))?;
    let sig = std::fs::read(aoide_storage::charter::in_force_sig_path(mesh))
        .map_err(|e| (-32603, format!("reading the charter's signature: {e}")))?;
    let _ = audit(
        audit_log,
        Door::A2a,
        EventClass::Audit,
        "a2a.aoide/charterFetch",
        "ok",
        &format!("charter `{mesh}` v{} released to a local-network join", charter.version),
    );
    Ok(json!({
        "mesh": charter.mesh,
        "version": charter.version,
        "operator": operator,
        "charter": base64_bytes(&bytes),
        "sig": base64_bytes(&sig),
    }))
}

/// The tiny encoder this door needs for handing two blobs to a join: the
/// charter document and its signature, the latter raw bytes that are not
/// UTF-8-safe to embed in JSON as text. Standard base64 with padding, written
/// here rather than pulling a dependency for fourteen lines, and it has
/// exactly one caller — nothing else in this door emits a blob.
fn base64_bytes(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(ALPHABET[((n >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 { ALPHABET[((n >> 6) & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { ALPHABET[(n & 63) as usize] as char } else { '=' });
    }
    out
}

/// `aoide/pairRequest` (CONTRACTS.md §6, P-P2): the pairing ceremony's
/// bootstrap request. Box A POSTs `{pubkeyHex, name, commitHex, url}` — its
/// own public key, its own SELF-CLAIMED instance name (A's
/// `local_host_name` chain — the name THIS instance will record A under), a
/// COMMITMENT to its own nonce (`aoide_storage::pairing::derive_commit`,
/// never the nonce itself — module doc on `aoide_storage::pairing`, the
/// commit-then-reveal fix), and its own advertised A2A door URL (recorded
/// for the resulting node record's own future non-ceremony calls — Design A,
/// task #119: the ceremony's own completion no longer dials this URL) —
/// PLUS an OPTIONAL `selfVia` (P-PV1, task #131): A's own self-asserted
/// `ssh://[user@]host` reach-back hop claim, carried alongside `url` for
/// exactly the case where A's door is loopback-only and reached through a
/// tunnel — B, dialed over that tunnel, can only ever OBSERVE the
/// connection arriving from loopback, so nothing about the connection
/// itself can answer "how do I dial A back"; `selfVia` is A's own claim of
/// that answer (same trust class as `url` — self-asserted data, a
/// transport marker only; trust stays in pubkeys + SAS). Absent on an old
/// requester, or when A has no such claim to make. THIS instance (box B)
/// parks it whole, `selfVia` included ([`aoide_storage::pairing::
/// park_inbound`], cap-checked — a full queue is `-32000`, review-bounce
/// Finding 3; a SAME-pubkey retry supersedes whatever this identity had
/// parked already, audited as a supersede but never named as one on the
/// wire — R3, `aoide_storage::pairing`'s own module doc), for its own
/// LATER `pair <id>` commit to read
/// (`aoide-client::commands::approve_inbound`'s own doc), and answers
/// SYNCHRONOUSLY with its OWN public key and a freshly-minted nonce —
/// public material, same "freely shown" stance `docs/architecture/
/// PAIRING.md`'s "Identity" section already states for `aoide identity`,
/// and safe to reveal immediately since B moves SECOND (nothing of B's is
/// fixed by a commitment A could exploit the way the reverse would be).
///
/// **`mesh` (P-CHARTER) is the field that makes the two ends agree.** A mesh
/// is a trust scope and each side used to resolve it alone, so two operators
/// naming different meshes landed an asymmetric pair with the grant in a mesh
/// the other end never reads. A's chosen mesh now rides the request and B's
/// commit takes it as the mesh to write (`pairing_mesh` puts it above every
/// local source). It is validated HERE as a mesh NAME
/// ([`aoide_storage::node_store::valid_node_name`], the same grammar `--mesh`
/// and `[mesh.<name>]` take) and refused `-32602` otherwise, because it names
/// a trust scope rather than merely enriching a record; it is otherwise
/// self-asserted DATA of the same class as `url`/`selfVia`, and it is shown
/// beside the code at `pair <id>` so the operator's own typed confirmation is
/// what actually authorizes it.
///
/// `name` is validated against [`aoide_storage::node_store::valid_node_name`]
/// HERE, at park time — not merely at `node add`'s door the way a
/// legacy-path name is — because `pair <id>` reuses this
/// self-claimed name VERBATIM as the approver's own local nickname (no
/// separate `--name` flag on `approve`), and that nickname later joins a
/// `state/node-cache/<name>.json` path; a traversal-shaped name must never
/// reach that far. A malformed request (bad pubkey/name/commit/url shape) is
/// refused with `-32602` before anything is parked.
fn pair_request(params: &Value, origin: ConnOrigin, audit_log: &Path) -> Result<Value, (i64, String)> {
    let pubkey_hex = params.get("pubkeyHex").and_then(Value::as_str).unwrap_or("");
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let commit_hex = params.get("commitHex").and_then(Value::as_str).unwrap_or("");
    let url = params.get("url").and_then(Value::as_str).unwrap_or("");
    // OPTIONAL (P-PV1, task #131) — an old requester's body carries no
    // `selfVia` key at all, and any shape that isn't a non-empty string
    // (absent, wrong type, empty) collapses to `None` the same way: this
    // field is never load-bearing enough to refuse a pairing request over,
    // only to enrich the approver's eventual commit when present.
    let self_via = params.get("selfVia").and_then(Value::as_str).filter(|s| !s.is_empty());
    // OPTIONAL (P-CHARTER), and unlike `selfVia` it IS validated: a mesh is a
    // trust scope the approver's commit writes into, not a transport marker,
    // so a shape that cannot be a mesh name is refused by name rather than
    // silently dropped — absent (an old requester) still means `None`.
    let mesh = match params.get("mesh").and_then(Value::as_str).filter(|s| !s.is_empty()) {
        Some(m) => {
            if !aoide_storage::node_store::valid_node_name(m) {
                return Err((
                    -32602,
                    format!("invalid params: mesh `{m}` is not a mesh name (`^[a-z0-9][a-z0-9-]*$`)"),
                ));
            }
            Some(m)
        }
        None => None,
    };

    if !valid_pubkey_hex(pubkey_hex) {
        return Err((-32602, "invalid params: pubkeyHex must be 64 hex characters".to_string()));
    }

    // P-SEAL: the requester's own self-signed age binding, when it has one.
    // A request that carries one is checked against the key it claims BEFORE
    // anything is parked: a binding whose signer is not `pubkeyHex` is a
    // malformed request, not an older-peer request, and parking it would
    // only defer the same refusal to the commit.
    let binding: Option<aoide_storage::seal::Binding> = match params.get("binding") {
        None | Some(Value::Null) => None,
        Some(raw) => match serde_json::from_value::<aoide_storage::seal::Binding>(raw.clone()) {
            Ok(binding) => {
                if !aoide_storage::seal::verify_binding(&binding, Some(pubkey_hex)) {
                    return Err((
                        -32602,
                        "invalid params: binding does not verify under the pubkeyHex it claims".to_string(),
                    ));
                }
                Some(binding)
            }
            Err(_) => return Err((-32602, "invalid params: binding is malformed".to_string())),
        },
    };
    if !aoide_storage::node_store::valid_node_name(name) {
        return Err((
            -32602,
            "invalid params: name must match `^[a-z0-9][a-z0-9-]*$`".to_string(),
        ));
    }
    if !valid_commit_hex(commit_hex) {
        return Err((-32602, "invalid params: commitHex must be 64 hex characters".to_string()));
    }
    if !valid_node_url(url) {
        return Err((-32602, "invalid params: url must be a non-empty URL, at most 2048 characters".to_string()));
    }

    let (kp, _) =
        aoide_storage::identity::load_or_mint().map_err(|e| (-32603_i64, format!("loading this instance's identity: {e}")))?;
    let info = kp.info();

    let requested_at = now_iso_utc();
    let now_epoch = aoide_storage::time::parse_iso_utc(&requested_at).unwrap_or_else(|| unix_ts_now() as i64);
    let expires_at = aoide_storage::pairing::expires_at_from(now_epoch);

    let (entry, evicted_id) = aoide_storage::pairing::park_inbound_with_mesh(
        pubkey_hex,
        name,
        &origin_display(origin),
        url,
        commit_hex,
        &requested_at,
        &expires_at,
        self_via,
        // P-CHARTER: the mesh the requester named rides the SAME write as the
        // entry, so there is no second, best-effort write that could silently
        // fail and leave a meshless park behind (review F6) — a meshless park
        // makes the approver resolve the mesh LOCALLY at commit, which is the
        // asymmetry the field exists to make impossible.
        mesh,
    )
    .map_err(|e| (-32000_i64, e))?;

    // P-SEAL: attach the requester's binding after parking, never as part of
    // it — a request that carries none (an older aoide) parks and pairs
    // exactly as before. The binding was already verified under the claimed
    // `pubkeyHex` above, so a failure here is a local write failure, not a
    // refusal of the request: the ceremony is what the caller asked for and
    // the binding is what it offered.
    if let Some(binding) = &binding {
        let _ = aoide_storage::pairing::set_inbound_binding(&entry.id, binding);
    }

    // R3 (one live parked request per requester identity,
    // `aoide_storage::pairing`'s own module doc): a same-pubkey retry
    // superseded whatever was parked before it. The audit STATUS names the
    // supersede and its evicted id; the WIRE response below never does —
    // an ordinary fresh-id response either way (CONTRACTS.md §6).
    let status = match &evicted_id {
        Some(old_id) => format!("parked (superseding {old_id})"),
        None => "parked".to_string(),
    };
    let _ = audit(
        audit_log,
        Door::A2a,
        EventClass::Audit,
        "a2a.pairRequest",
        &status,
        &format!(
            "pairing request `{}` parked (claimed name `{name}`, origin {})",
            entry.id,
            origin_display(origin)
        ),
    );

    emit_pairing_event(
        "pair-parked",
        json!({
            "id": entry.id,
            "name": entry.name,
            "originAddr": entry.origin_addr,
            "url": entry.url,
            "direction": "inbound",
        }),
    );

    Ok(json!({
        "id": entry.id,
        "pubkeyHex": info.pubkey_hex,
        "nonceHex": entry.approver_nonce_hex,
        "expiresAt": entry.expires_at,
    }))
}

/// `aoide/pairReveal` (CONTRACTS.md §6, P-P2, review-bounce Finding 1): box
/// A's immediate follow-up to `aoide/pairRequest` (same `pair`
/// invocation, two sequential POSTs), handing over the nonce its earlier
/// `commitHex` already fixed. `{id, nonceHex}` — `id` is the SAME id
/// [`pair_request`] handed back synchronously. THIS instance (box B) checks
/// `derive_commit(entry.pubkeyHex, nonceHex) == entry.commitHex`
/// ([`aoide_storage::pairing::reveal_inbound`]); a match stores the nonce
/// (so bare `pair`/`pair <id>` can finally derive a SAS for this
/// entry) and a MISMATCH drops the parked entry outright — there is nothing
/// left worth keeping once the commitment fails to check out (a genuine
/// tamper, or a bug; either way the honest path is to start over, not to
/// leave a broken entry sitting in the queue).
fn pair_reveal(params: &Value, audit_log: &Path) -> Result<Value, (i64, String)> {
    let id = params.get("id").and_then(Value::as_str).unwrap_or("");
    let nonce_hex = params.get("nonceHex").and_then(Value::as_str).unwrap_or("");
    if id.is_empty() {
        return Err((-32602, "invalid params: id is required".to_string()));
    }
    if !valid_nonce_hex(nonce_hex) {
        return Err((-32602, "invalid params: nonceHex must be 32 hex characters".to_string()));
    }

    let now_epoch = aoide_storage::time::parse_iso_utc(&now_iso_utc()).unwrap_or_else(|| unix_ts_now() as i64);
    match aoide_storage::pairing::reveal_inbound(id, nonce_hex, now_epoch) {
        Ok(entry) => {
            let _ = audit(
                audit_log,
                Door::A2a,
                EventClass::Audit,
                "a2a.pairReveal",
                "ok",
                &format!("pairing request `{id}` revealed — commitment verified (claimed name `{}`)", entry.name),
            );
            emit_pairing_event(
                "pair-revealed",
                json!({
                    "id": entry.id,
                    "name": entry.name,
                    "originAddr": entry.origin_addr,
                    "url": entry.url,
                    "direction": "inbound",
                }),
            );
            Ok(json!({ "ok": true }))
        }
        Err(aoide_storage::pairing::RevealError::Unknown) => Err((
            -32001,
            "no pending inbound pairing request with that id (unknown, already resolved, or expired)".to_string(),
        )),
        Err(aoide_storage::pairing::RevealError::Mismatch) => {
            let _ = audit(
                audit_log,
                Door::A2a,
                EventClass::Audit,
                "a2a.pairReveal",
                "mismatch",
                &format!("pairing request `{id}` dropped — the revealed nonce did not match its commitment"),
            );
            Err((
                -32002,
                "commitment mismatch: the revealed nonce does not match the pubkey's earlier commitment — the parked request has been dropped".to_string(),
            ))
        }
        Err(aoide_storage::pairing::RevealError::Io(e)) => Err((-32603, format!("resolving the inbound pairing request: {e}"))),
    }
}

/// `aoide/pairPoll` (Design A, task #119, CONTRACTS.md §6 — REPLACES the old
/// `aoide/pairApprove` reverse callback): the REQUESTER's `pair
/// <id>` POSTs this to the APPROVER's door, over the SAME forward dial
/// `aoide/pairRequest`/`aoide/pairReveal` already used, asking "has the
/// entry I parked with you been approved yet?" `{id, timestampIso, nonceHex,
/// signatureHex}` — `id` is the SAME id [`pair_request`] handed back
/// synchronously; the other three are a self-contained signature (never
/// P-P4's header-based scheme, which needs a VERIFIED node record to check
/// against — one doesn't exist on the approver's side until the id this
/// poll is asking about is ITSELF approved, a bootstrapping problem P-P4
/// can't solve here) proving the poller holds the private key matching
/// [`InboundPairingRequest::pubkey_hex`] — the REQUESTER's own pubkey,
/// captured at `aoide/pairRequest` time, long before any node record
/// exists. The signed message is
/// `aoide_storage::wire_auth::canonical_string("PAIRPOLL", id, timestampIso,
/// nonceHex, &[])` (the SAME canonical-string primitive P-P4 signs, reused
/// rather than reinvented, with an empty body — a poll carries no body of
/// its own to bind).
///
/// The nonce is signed but NOT replay-checked (no `consume_nonce` here,
/// unlike P-P4's request path): a captured poll replays cleanly inside the
/// skew window, and that is accepted deliberately — the response is
/// idempotent and releases only B's own pubkey, which `aoide/pairRequest`
/// already hands to any caller; there is nothing a replay gains. Timing is
/// likewise not uniform across the pending cases (an unknown id refuses
/// before the signature verify, a known one pays it) — the oracle
/// discipline below is byte-level, not timing-level, stated so nobody
/// reads a stronger claim into it.
///
/// **Existence-oracle discipline (mirrors the door's own Phase G/2026-08-20
/// amendment for `message/send`'s `contextId` lookup, CONTRACTS.md §6): an
/// unauthenticated or wrongly-signed poller must never learn anything an
/// authenticated one couldn't.** Three cases — the id doesn't exist (never
/// parked, already expired), the signature doesn't verify against the
/// entry's own stored `pubkey_hex`, or the entry exists and verifies but
/// isn't approved YET — all answer with the IDENTICAL `{"status":"pending"}`,
/// never a distinct error code that would let a caller tell "wrong id" apart
/// from "right id, wrong key" apart from "right id and key, just not
/// approved yet." Only a poll that BOTH verifies AND finds
/// [`InboundPairingRequest::approved`] `true` gets the release:
/// `{"status":"approved", "pubkeyHex": "<B's own pubkey>"}` — B's own
/// identity, re-loaded fresh (never stored on the parked entry; it's B's
/// OWN persistent key, always re-derivable) so the requester can bind this
/// release to the SAME transcript it already holds
/// (`aoide_storage::pairing::mark_outbound_awaiting_confirm`'s own pubkey
/// check on the requester's side is what actually enforces this — a
/// mismatch there rejects a substituted reveal without ever committing).
/// Malformed params (missing id, non-hex nonce/signature) refuse `-32602`
/// BEFORE any lookup — a shape error reveals nothing about any id's
/// existence, so it stays distinct from the uniform "pending" response.
fn pair_poll(params: &Value, audit_log: &Path) -> Result<Value, (i64, String)> {
    let id = params.get("id").and_then(Value::as_str).unwrap_or("");
    let timestamp = params.get("timestampIso").and_then(Value::as_str).unwrap_or("");
    let nonce_hex = params.get("nonceHex").and_then(Value::as_str).unwrap_or("");
    let signature_hex = params.get("signatureHex").and_then(Value::as_str).unwrap_or("");
    if id.is_empty() {
        return Err((-32602, "invalid params: id is required".to_string()));
    }
    if timestamp.is_empty() {
        return Err((-32602, "invalid params: timestampIso is required".to_string()));
    }
    if !valid_nonce_hex(nonce_hex) {
        return Err((-32602, "invalid params: nonceHex must be 32 hex characters".to_string()));
    }
    if signature_hex.is_empty() {
        return Err((-32602, "invalid params: signatureHex is required".to_string()));
    }

    let now_epoch = aoide_storage::time::parse_iso_utc(&now_iso_utc()).unwrap_or_else(|| unix_ts_now() as i64);
    let pending = json!({ "status": "pending" });

    let Some(entry) = aoide_storage::pairing::list_inbound(now_epoch).into_iter().find(|e| e.id == id) else {
        // Unknown or expired — the SAME response an authenticated-but-not-
        // yet-approved poll gets (module doc: existence-oracle discipline).
        return Ok(pending);
    };

    let ts_epoch = match aoide_storage::time::parse_iso_utc(timestamp) {
        Some(t) => t,
        None => return Ok(pending),
    };
    if !aoide_storage::wire_auth::within_skew(now_epoch, ts_epoch, aoide_storage::wire_auth::signature_skew_secs()) {
        return Ok(pending);
    }

    let canonical = aoide_storage::wire_auth::canonical_string("PAIRPOLL", id, timestamp, nonce_hex, &[], None);
    if !aoide_storage::wire_auth::verify_signature_hex(&entry.pubkey_hex, canonical.as_bytes(), signature_hex) {
        // A bad signature against a REAL entry answers exactly like an
        // unknown one — no distinct code, nothing for an outsider to learn.
        return Ok(pending);
    }

    if !entry.approved {
        return Ok(pending);
    }

    let (kp, _) = aoide_storage::identity::load_or_mint().map_err(|e| (-32603_i64, format!("loading this instance's identity: {e}")))?;
    let info = kp.info();

    // P-SEAL: release this node's own age binding alongside the pubkey, so
    // one ceremony leaves both ends able to seal to each other. Best-effort
    // by construction: an instance whose age key cannot be minted still
    // pairs, it simply seals nothing until it can.
    let released_binding = aoide_storage::seal::publish_binding().ok();

    let _ = audit(
        audit_log,
        Door::A2a,
        EventClass::Audit,
        "a2a.pairPoll",
        "released",
        &format!("pairing request `{id}` released to `{}`'s own verified poll", entry.name),
    );

    Ok(json!({ "status": "approved", "pubkeyHex": info.pubkey_hex, "binding": released_binding }))
}

/// Per-request context [`handle_jsonrpc`]/[`handle_jsonrpc_bytes`] thread
/// through to whichever method needs it: `message/send` needs
/// `audit_log`/`spawn_agent`/`origin`; `aoide/graphSummary` (CONTRACTS.md §7)
/// needs `node_name`/`self_url`. Bundled into one struct once a second method
/// needed request-scoped dependencies, rather than growing `handle_jsonrpc`'s
/// positional-arg list again.
struct RequestCtx<'a> {
    audit_log: &'a Path,
    spawn_agent: &'a str,
    spawn_cwd: &'a str,
    origin: ConnOrigin,
    node_name: &'a str,
    self_url: &'a str,
    /// The server's own expected A2A token (CONTRACTS.md §6 amendment,
    /// 2026-08-18) — empty means none configured (feature off). Resolved
    /// once at `a2a serve` launch, same as `spawn_agent`.
    expected_token: &'a str,
    /// This request's `Authorization: Bearer <token>`, if any.
    presented_token: Option<&'a str>,
    /// P-P4: `Some(caller)` when [`verify_signed_request`] already verified
    /// this request's signature headers against a paired node — computed
    /// exactly once, in `handle_connection`, before either dispatch path,
    /// never re-verified here. `None` covers both "no signature headers at
    /// all" and "this ctx predates P-P4 in a test fixture."
    signed_caller: Option<SignedCaller<'a>>,
    /// H1: is this request being answered by the mail ADAPTER rather than the
    /// door? `false` on the door — byte-identical behavior, including the
    /// plaintext envelope a destination with no binding still receives over
    /// the direct SSH lane. `true` only in
    /// [`RequestCtx::mail_adapter`], where `mail_deposit` refuses an
    /// unsealed envelope with `sealed-required`: no relay, hub or HTTPS hop
    /// ever carries plaintext.
    sealed_only: bool,
}

impl<'a> RequestCtx<'a> {
    /// The mail adapter's ctx (H1): every door-only field a mail method never
    /// reads is empty by construction — no spawn command, no spawn cwd, no
    /// instance name, no door token, no presented bearer (the adapter has no
    /// token concept at all). What is left is exactly what `mail_rpc`'s three
    /// methods consume: the audit log, the connection's origin, and the
    /// verified caller.
    fn mail_adapter(
        audit_log: &'a Path,
        origin: ConnOrigin,
        signed_caller: Option<SignedCaller<'a>>,
    ) -> RequestCtx<'a> {
        RequestCtx {
            audit_log,
            spawn_agent: "",
            spawn_cwd: "",
            origin,
            node_name: "",
            self_url: "",
            expected_token: "",
            presented_token: None,
            signed_caller,
            sealed_only: true,
        }
    }
}

/// Handle one parsed JSON-RPC 2.0 request `Value`, returning the response
/// `Value` (always — unlike `mcp.rs`'s stdio notifications, an HTTP POST
/// always gets a reply body).
fn handle_jsonrpc(req: &Value, ctx: &RequestCtx) -> Value {
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let method = req.get("method").and_then(Value::as_str).unwrap_or("");
    let params = req.get("params").cloned().unwrap_or(Value::Null);

    // Phase G (CONTRACTS.md §6 amendment, 2026-08-20): the read commands are
    // token-gated by the SAME rule as spawn. Computed once; only bites when a
    // token is configured (off-path unchanged). `message/send` runs its own
    // classify internally (it needs the full TokenState for the bearer gate),
    // so it is not re-gated here.
    let read_ok = read_admitted(
        token_authorized(!ctx.expected_token.is_empty(), classify_token(ctx.expected_token, ctx.presented_token)),
        ctx.signed_caller,
    );

    let result: Result<Value, (i64, String)> = match mail_rpc(method, &params, ctx) {
        Some(mail) => mail,
        None => match method {
            "tasks/get" if !read_ok => Err(unauthorized()),
            "tasks/get" => {
                let task_id = params.get("id").and_then(Value::as_str).unwrap_or("");
                // `metadata["aoide/frame"]` asks for the watch frame (CONTRACTS.md
                // §6, P-RSA S6); a request without it is the untouched status read.
                match (frame_tail(&params), lines_after(&params)) {
                    (None, None) => task_get(task_id),
                    (frame, history) => task_get_outputs(task_id, frame, history, read_ok, ctx.signed_caller),
                }
            }
            "message/send" => message_send(
                &params,
                ctx.audit_log,
                ctx.spawn_agent,
                ctx.spawn_cwd,
                ctx.origin,
                ctx.expected_token,
                ctx.presented_token,
                ctx.signed_caller,
            ),
            "aoide/graphSummary" if !read_ok => Err(unauthorized()),
            "aoide/graphSummary" => graph_summary(ctx.node_name, ctx.self_url),
            // Deliberately UNGATED by `read_ok` — the pairing bootstrap has no
            // established credential to check yet (see the section doc above
            // `pair_request`).
            "aoide/pairRequest" => pair_request(&params, ctx.origin, ctx.audit_log),
            "aoide/pairReveal" => pair_reveal(&params, ctx.audit_log),
            "aoide/pairPoll" => pair_poll(&params, ctx.audit_log),
            // P-CHARTER: the LAN join's one read — the mesh's operator key and
            // the charter in force, over a local-network connection only.
            // Deliberately NOT in `mail_rpc`: the mail adapter is not a join
            // surface, and a node asking it for a charter is a stranger asking
            // the wrong listener (`-32601` there, by construction).
            "aoide/charterFetch" => charter_fetch(&params, ctx.origin, ctx.audit_log),
            "" => Err((-32600, "invalid request: missing method".to_string())),
            other => Err((-32601, format!("method not found: {other}"))),
        },
    };

    let resp = match result {
        Ok(value) => JsonRpcResponse::ok(id, value),
        Err((code, message)) => JsonRpcResponse::err(id, code, message),
    };
    serde_json::to_value(&resp).expect("JsonRpcResponse always serializes")
}

/// The JSON-RPC `id` a request body carries, read once for the one answer the
/// door writes before dispatch. `null` for a body that does not parse — the same
/// value [`JsonRpcResponse::ok`] takes for an id-less call.
fn request_id(body: &[u8]) -> Value {
    serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|v| v.get("id").cloned())
        .unwrap_or(Value::Null)
}

/// The JSON-RPC method a request body names, where it is one of the two MAIL
/// methods — the only two the door answers from the declaration set.
/// `req.method` is the HTTP verb, so the name comes from the body, read once
/// here and once more by the dispatcher that would have run.
fn mail_method_of(body: &[u8]) -> Option<&'static str> {
    let named = serde_json::from_slice::<Value>(body).ok()?;
    match named.get("method").and_then(Value::as_str)? {
        "aoide/mailDeposit" => Some("aoide/mailDeposit"),
        "aoide/mailPoll" => Some("aoide/mailPoll"),
        _ => None,
    }
}

/// What the door answers a mail method with BEFORE dispatch, when this host's
/// declarations will not load — or that it should keep the refusal it has.
enum PreDispatch {
    /// The refused `config-invalid` result, which is never signed. It is reached
    /// only when `config.toml` will not load, and the charter this node governs
    /// is decided through that same file
    /// ([`aoide_storage::charter::governing`] reads its operator line), so the
    /// raw in-force read below is the only key it can be checked against, and
    /// the door signs nothing under a charter it does not govern.
    Unloadable { detail: String },
    /// A JSON-RPC error instead: the refusal the frame took (its clock, its
    /// audience, a reused nonce, or a full key).
    Error(i64, String),
    /// Leave the door's own `-32007` standing.
    Keep,
}

/// **The cryptographic half of the door's signature ladder, without the half
/// that needs the declaration set**: `Unloadable` when the set
/// will not load, the request names a mesh and a node, and its signature verifies
/// under the key the charter IN FORCE for that mesh gives THAT node. `Keep` in
/// every other case — a genuinely bad signature, a name the charter does not
/// carry, a pair mesh (its keys need no declaration and would have resolved) —
/// which leaves the door's own `-32007` standing.
///
/// **A verified frame is judged as every other verified frame is.** The clock
/// checks ([`time_refusal`]) run before the signature, and the audience and nonce
/// are taken by [`admit_verified`] before anything is answered. The nonce is
/// recorded here, so bytes this door already answered `config-invalid` cannot be
/// replayed into a method once the config is back: the replay is refused
/// (`-32009`, or `-32013` for a full key). A frame refused for its audience
/// records nothing. The audit line is the method's own; nothing about the load
/// error goes on the wire.
fn mail_unloadable_declaration(
    req: &HttpRequest,
    method: &str,
    listener: Listener,
    audit_log: &Path,
    now_epoch: i64,
    started_epoch: i64,
) -> PreDispatch {
    // Only POST is a signed request (`verify_signed_request`). This arm is
    // reached for every -32007 refusal, so it repeats that gate: a method
    // spelled `ANSWER` must never reach the canonical string below.
    if req.method != "POST" {
        return PreDispatch::Keep;
    }
    let Some(refusal) = aoide_storage::routing::declarations().err() else {
        return PreDispatch::Keep;
    };
    let Some(mesh) = req.signed_mesh.as_deref() else { return PreDispatch::Keep };
    // A name that is not a mesh name is never a path component: nothing below
    // this line builds one from it.
    if !aoide_storage::node_store::valid_node_name(mesh) {
        return PreDispatch::Keep;
    }
    let Some(node) = req.signed_node.as_deref() else { return PreDispatch::Keep };
    let (Some(timestamp), Some(nonce), Some(signature)) = (
        req.signed_timestamp.as_deref(),
        req.signed_nonce.as_deref(),
        req.signed_signature.as_deref(),
    ) else {
        return PreDispatch::Keep;
    };
    let Ok(text) = std::fs::read_to_string(aoide_storage::charter::in_force_path(mesh)) else {
        return PreDispatch::Keep;
    };
    let Ok(charter) = aoide_storage::charter::parse(&text) else { return PreDispatch::Keep };
    if charter.mesh != mesh {
        return PreDispatch::Keep;
    }
    let Some(line) = charter.nodes.get(node) else { return PreDispatch::Keep };
    // Only a timestamp this door can read is judged. One it cannot read returns
    // `Keep`, and the door's own `-32007` stands for it.
    let Some(ts_epoch) = aoide_storage::time::parse_iso_utc(timestamp) else { return PreDispatch::Keep };
    if let Some((code, message)) = time_refusal(timestamp, ts_epoch, now_epoch, started_epoch) {
        let _ = audit(audit_log, Door::A2a, EventClass::Audit, "a2a.signed-request", "unauthorized", &message);
        return PreDispatch::Error(code, message);
    }
    let canonical = aoide_storage::wire_auth::canonical_string(
        &req.method,
        &req.path,
        timestamp,
        nonce,
        &req.body,
        Some(mesh),
    );
    if !aoide_storage::wire_auth::verify_signature_hex(&line.key, canonical.as_bytes(), signature) {
        return PreDispatch::Keep;
    }
    if let Err((code, message)) = admit_verified(listener, req, &format!("charter node `{node}`"), &line.key, now_epoch) {
        let _ = audit(audit_log, Door::A2a, EventClass::Audit, "a2a.signed-request", "unauthorized", &message);
        return PreDispatch::Error(code, message);
    }
    let detail = format!("{CONFIG_INVALID}: {refusal}");
    let _ = audit(audit_log, Door::A2a, EventClass::Audit, method, "invalid", &detail);
    PreDispatch::Unloadable { detail: CONFIG_INVALID_DETAIL.to_string() }
}

fn jsonrpc_error_value(code: i64, message: impl Into<String>) -> Value {
    serde_json::to_value(JsonRpcResponse::err(Value::Null, code, message))
        .expect("JsonRpcResponse always serializes")
}

fn handle_jsonrpc_bytes(body: &[u8], ctx: &RequestCtx) -> Value {
    match serde_json::from_slice::<Value>(body) {
        Ok(req) => handle_jsonrpc(&req, ctx),
        Err(e) => jsonrpc_error_value(-32700, format!("parse error: {e}")),
    }
}

/// The mail adapter's dispatch (H1): [`mail_rpc`], or `-32601`. That is the
/// whole table — `message/send`, `tasks/get`, the `pair*` ceremony and
/// `message/stream` are not merely refused here, they are unreachable: the
/// functions behind them are never called from this process.
///
/// A missing `method` is `-32601` here rather than the door's `-32600`: on a
/// listener whose table is three names, "not one of them" is the whole answer,
/// and there is no request-shape advice to give.
fn handle_mail_jsonrpc(req: &Value, ctx: &RequestCtx) -> Value {
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let method = req.get("method").and_then(Value::as_str).unwrap_or("");
    let params = req.get("params").cloned().unwrap_or(Value::Null);

    let result = mail_rpc(method, &params, ctx)
        .unwrap_or_else(|| Err((-32601, format!("method not found: {method}"))));
    let resp = match result {
        Ok(value) => JsonRpcResponse::ok(id, value),
        Err((code, message)) => JsonRpcResponse::err(id, code, message),
    };
    serde_json::to_value(&resp).expect("JsonRpcResponse always serializes")
}

fn handle_mail_jsonrpc_bytes(body: &[u8], ctx: &RequestCtx) -> Value {
    match serde_json::from_slice::<Value>(body) {
        Ok(req) => handle_mail_jsonrpc(&req, ctx),
        Err(e) => jsonrpc_error_value(-32700, format!("parse error: {e}")),
    }
}

// ── SSE streaming: `message/stream` + `tasks/resubscribe` (Phase C) ──────────
//
// These two methods do NOT take the one-shot `route()`→`write_http_response`
// path: they keep the socket open, write `text/event-stream` headers ONCE,
// and emit a `data:` event whenever the target task's status changes, until
// it reaches a TERMINAL state, the client disconnects, or [`MAX_STREAM`]
// elapses. `message/stream` FIRST runs the send (inject/spawn, reusing
// [`message_send`]) and then streams the resulting task; `tasks/resubscribe`
// streams an already-existing task named by `params.id`.

/// Format one Server-Sent-Events data frame: `data: <json>\n\n`. Pure.
fn sse_event(value: &Value) -> String {
    format!("data: {}\n\n", serde_json::to_string(value).unwrap_or_default())
}

/// A2A terminal `TaskState`s (JSON-RPC binding spelling): once a task reaches
/// one of these it will not change again, so the stream emits its final event
/// and closes. For aoide today only `completed` is actually reachable
/// (canonical `done`/`stopped` → `completed`); `failed`/`canceled`/`rejected`
/// have no canonical_state producer yet (CONTRACTS.md §6) but are recognised
/// as terminal here so a future producer streams correctly with no change. Pure.
fn is_terminal_state(state: &str) -> bool {
    matches!(state, "completed" | "failed" | "canceled" | "rejected")
}

/// Emit-on-change: emit only when this is the first observation (`last` is
/// `None`) or the state differs from the last emitted one. Pure. (The stream
/// loop additionally forces the FINAL event even when the state is unchanged,
/// so a terminal/timeout close is never swallowed.)
fn should_emit(last: Option<&str>, current: &str) -> bool {
    last != Some(current)
}

/// Build the JSON-RPC result envelope for one SSE stream event. A non-final
/// event carries the Task itself as `result`; the FINAL event's `result` is
/// shaped as A2A's `TaskStatusUpdateEvent` (`{taskId, contextId, status,
/// final:true, kind:"status-update"}`) so the client knows it is the last one.
/// Pure — the `task` argument is whatever [`task_get`] produced.
fn build_stream_event(rpc_id: &Value, task: &Value, is_final: bool) -> Value {
    let result = if is_final {
        let event = TaskStatusUpdateEvent {
            task_id: task.get("id").cloned().unwrap_or(Value::Null),
            context_id: task.get("contextId").cloned().unwrap_or(Value::Null),
            status: task.get("status").cloned().unwrap_or(Value::Null),
            is_final: true,
            kind: "status-update".to_string(),
        };
        serde_json::to_value(&event).expect("TaskStatusUpdateEvent always serializes")
    } else {
        task.clone()
    };
    serde_json::to_value(JsonRpcResponse::ok(rpc_id.clone(), result))
        .expect("JsonRpcResponse always serializes")
}

/// Peek a parsed request: if it's a `POST /` whose JSON-RPC body names a
/// streaming method (`message/stream` / `tasks/resubscribe`), return that
/// method so [`handle_connection`] can hand the socket to [`stream_task`].
/// Everything else (GET card, `tasks/get`, `message/send`, errors) returns
/// `None` and keeps the existing one-shot path. Pure.
fn streaming_method(req: &HttpRequest) -> Option<String> {
    if req.method != "POST" || req.path != "/" {
        return None;
    }
    let v: Value = serde_json::from_slice(&req.body).ok()?;
    match v.get("method").and_then(Value::as_str)? {
        m @ ("message/stream" | "tasks/resubscribe") => Some(m.to_string()),
        _ => None,
    }
}

/// Read the `status.state` string out of a Task JSON (`task_get`'s shape).
fn task_state_of(task: &Value) -> String {
    task.get("status")
        .and_then(|s| s.get("state"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// Take over the socket and stream the target task's status as SSE, until it
/// reaches a terminal state, the client disconnects, or [`MAX_STREAM`] elapses.
/// The request was already fully parsed by [`handle_connection`], so this only
/// ever WRITES the socket (never reads it again) — the 10s read-timeout set on
/// the stream by `handle_connection` therefore cannot interrupt this loop.
fn stream_task<W: Write>(
    writer: &mut W,
    req: &HttpRequest,
    method: &str,
    audit_log: &Path,
    spawn_agent: &str,
    spawn_cwd: &str,
    origin: ConnOrigin,
    expected_token: &str,
    presented_token: Option<&str>,
    signed_caller: Option<SignedCaller<'_>>,
) -> std::io::Result<()> {
    let rpc: Value = serde_json::from_slice(&req.body).unwrap_or(Value::Null);
    let rpc_id = rpc.get("id").cloned().unwrap_or(Value::Null);
    let params = rpc.get("params").cloned().unwrap_or(Value::Null);

    // Resolve the target task + its initial state. `message/stream` runs the
    // send FIRST (inject/spawn) and streams the task it produced;
    // `tasks/resubscribe` streams an existing task by id.
    //
    // Phase G (CONTRACTS.md §6 amendment, 2026-08-20): gate BOTH streaming
    // reads by the same token rule as the one-shot commands. When a token is
    // configured and the caller lacks a valid one, resolution short-circuits
    // to `unauthorized()` BEFORE `message_send` runs — so an unauthenticated
    // `message/stream` neither injects nor spawns, it only receives the
    // `-32005` SSE error event below. `tasks/resubscribe`'s own `task_get`
    // would otherwise be an ungated session-state read (the SSE sibling of
    // `tasks/get`). Off-path (no token) is byte-identical to before.
    //
    // Only the READ (`tasks/resubscribe`) also admits a signed caller holding
    // `read` ([`read_admitted`]); `message/stream` injects or spawns, so it
    // stays bearer-gated and `message_send` applies its own grant gates.
    let bearer_ok = token_authorized(
        !expected_token.is_empty(),
        classify_token(expected_token, presented_token),
    );
    let resolved: Result<Value, (i64, String)> = match method {
        "message/stream" if !bearer_ok => Err(unauthorized()),
        "message/stream" => {
            message_send(&params, audit_log, spawn_agent, spawn_cwd, origin, expected_token, presented_token, signed_caller)
        }
        _ /* tasks/resubscribe */ if !read_admitted(bearer_ok, signed_caller) => Err(unauthorized()),
        _ => match params.get("id").and_then(Value::as_str).filter(|s| !s.is_empty()) {
            Some(id) => task_get(id),
            None => Err((-32001, "task not found".to_string())),
        },
    };

    // SSE response headers — written exactly once, before any event. Never
    // signed: a stream has no single body for an answer signature to cover.
    write!(
        writer,
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n",
    )?;
    writer.flush()?;

    // A resolution error (bad send, unknown resubscribe id) → one SSE event
    // carrying the JSON-RPC error, then close.
    let mut task = match resolved {
        Ok(task) => task,
        Err((code, message)) => {
            let err = serde_json::to_value(JsonRpcResponse::err(rpc_id, code, message))
                .expect("JsonRpcResponse always serializes");
            // Never signed: an SSE event, part of a stream that is never signed.
            let _ = writer.write_all(sse_event(&err).as_bytes());
            let _ = writer.flush();
            let _ = audit(
                audit_log,
                Door::A2a,
                EventClass::Audit,
                &format!("a2a.{method}"),
                "error",
                &format!("stream open error {code}"),
            );
            return Ok(());
        }
    };
    // The task id we re-poll each tick — the sessionId (Task id == contextId,
    // MVP id-collapse — see `task_from_sessions`).
    let task_id = task.get("id").and_then(Value::as_str).unwrap_or("").to_string();

    let stream_start = Instant::now();
    let mut last_state: Option<String> = None;
    loop {
        let state = task_state_of(&task);
        // Terminal state OR the absolute duration cap → this is the final event.
        let is_final = is_terminal_state(&state) || stream_start.elapsed() >= MAX_STREAM;

        if should_emit(last_state.as_deref(), &state) || is_final {
            let event = build_stream_event(&rpc_id, &task, is_final);
            // A write/flush failure means the client hung up — best-effort,
            // just stop. Never signed: an SSE event, as above.
            if writer.write_all(sse_event(&event).as_bytes()).is_err() || writer.flush().is_err() {
                break;
            }
            last_state = Some(state);
        }

        if is_final {
            break;
        }
        std::thread::sleep(STREAM_POLL);

        // Refresh the task off the stage for the next tick. A transient
        // not-found (e.g. a just-spawned session not yet written to
        // sessions.json) keeps the last-known task rather than aborting — the
        // MAX_STREAM cap still bounds the wait, and a real terminal state will
        // be observed as soon as the record settles.
        if let Ok(fresh) = task_get(&task_id) {
            task = fresh;
        }
    }

    let _ = audit(
        audit_log,
        Door::A2a,
        EventClass::Audit,
        &format!("a2a.{method}"),
        "ok",
        "stream closed",
    );
    Ok(())
}

// ── Minimal HTTP/1.1 layer (hand-rolled, zero deps) ──────────────────────────

/// One parsed HTTP request: the request line + the body (read exactly
/// `Content-Length` bytes). Headers beyond `Content-Length` are read and
/// discarded — this door doesn't need cookies/auth headers for the MVP.
#[derive(Debug, PartialEq, Eq)]
struct HttpRequest {
    method: String,
    path: String,
    body: Vec<u8>,
    /// The `Authorization` header's bearer token, if present and well-formed
    /// (CONTRACTS.md §6 amendment, 2026-08-18) — `Some(<token>)` for
    /// `Authorization: Bearer <token>`, `None` for a missing header or any
    /// other scheme. Every other header this door doesn't need is still read
    /// and discarded, same as before this field existed.
    bearer: Option<String>,
    /// The four P-P4 signed-request headers (`aoide_storage::wire_auth`'s
    /// `HEADER_*` constants), raw header VALUES, unvalidated — captured
    /// together so `verify_signed_request` can tell "no signature headers
    /// at all" (every field `None`, the untouched path) from "a malformed
    /// signed request" (some but not all four present, refused outright)
    /// without re-parsing headers a second time. Each is `Some` the instant
    /// its own header line is seen, however many times — a request that
    /// repeats one of these headers keeps only the LAST value, same
    /// last-wins behavior `content_length`/`bearer` already had before this
    /// field existed.
    signed_node: Option<String>,
    signed_timestamp: Option<String>,
    signed_nonce: Option<String>,
    signed_signature: Option<String>,
    /// The mesh this request acts in (`wire_auth::HEADER_MESH`, P-CHARTER),
    /// raw header VALUE, unvalidated — `None` for a request that names none,
    /// which is a pre-P-CHARTER peer and NOT an error (see
    /// `wire_auth::canonical_string`'s doc: the five-field signature is what
    /// such a peer can produce). Never a member of the "all present together
    /// or not at all" quartet above: the four `X-Aoide-*` headers are the
    /// signature, and this one is a field the signature COVERS.
    signed_mesh: Option<String>,
}

/// Extract the bearer token from a raw `Authorization` header VALUE (the
/// part after `Authorization:`), case-insensitive on the `Bearer` scheme
/// name (RFC 7235 §2.1 treats auth-scheme as case-insensitive), trimmed.
/// `None` for any other scheme, an empty token, or a malformed header. Pure.
fn extract_bearer(header_value: &str) -> Option<String> {
    let rest = header_value.trim();
    let (scheme, token) = rest.split_once(char::is_whitespace)?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = token.trim();
    (!token.is_empty()).then(|| token.to_string())
}

/// A parse failure, carrying the HTTP status it should become — a plain
/// `String` error can't distinguish "malformed" (400) from "you asked for
/// too much" (413), and `handle_connection` needs that distinction to
/// respond correctly instead of hardcoding 400 for everything.
#[derive(Debug)]
struct ParseError {
    status: u16,
    message: String,
    /// The peer connected and closed without sending a byte (a reachability
    /// probe): nothing to answer and nothing to audit.
    idle: bool,
}

impl ParseError {
    fn new(status: u16, message: impl Into<String>) -> Self {
        Self { status, message: message.into(), idle: false }
    }
}

/// Read one line (the request line, or one header line), bounded to
/// `MAX_LINE` bytes so a newline-less hostile stream can't grow the buffer
/// without limit. `what` names the line for error messages.
///
/// Returns `Ok(None)` on a clean EOF before any bytes of this line arrived
/// (an idle/closed connection — tolerated, same as upstream HTTP servers).
/// Returns `Err` if `MAX_LINE` bytes were read without finding `\n`, or if
/// the connection closed mid-line.
fn read_bounded_line<R: BufRead>(r: &mut R, what: &str) -> Result<Option<String>, ParseError> {
    let mut buf = Vec::new();
    r.by_ref()
        .take(MAX_LINE as u64)
        .read_until(b'\n', &mut buf)
        .map_err(|e| ParseError::new(400, format!("reading {what}: {e}")))?;
    if buf.is_empty() {
        return Ok(None);
    }
    if !buf.ends_with(b"\n") {
        return Err(if buf.len() >= MAX_LINE {
            ParseError::new(400, format!("{what} too long (max {MAX_LINE} bytes)"))
        } else {
            ParseError::new(400, format!("connection closed while reading {what}"))
        });
    }
    Ok(Some(
        String::from_utf8_lossy(&buf).trim_end_matches(['\r', '\n']).to_string(),
    ))
}

/// Parse one HTTP/1.1 request off a `BufRead` — the request line, headers up
/// to the blank line (only `Content-Length` is consulted), then exactly that
/// many body bytes. Pure enough to unit-test against an in-memory buffer
/// (`Cursor`) as well as a real `TcpStream`.
///
/// Every untrusted-length quantity here (request line, each header line,
/// header count, body length) is capped BEFORE it drives an allocation or an
/// unbounded read — see the `MAX_*` constants above — and `start` (the
/// connection's accept time) enforces an absolute wall-clock budget
/// (`MAX_REQUEST`) across the whole parse, on top of the per-read socket
/// timeout `handle_connection` sets.
fn parse_http_request<R: BufRead>(r: &mut R, start: Instant) -> Result<HttpRequest, ParseError> {
    let check_deadline = |what: &str| -> Result<(), ParseError> {
        if start.elapsed() > MAX_REQUEST {
            Err(ParseError::new(400, format!("request deadline exceeded ({what})")))
        } else {
            Ok(())
        }
    };

    let request_line = match read_bounded_line(r, "request line")? {
        Some(l) => l,
        None => {
            return Err(ParseError {
                idle: true,
                ..ParseError::new(400, "connection closed before a request line arrived")
            })
        }
    };
    check_deadline("after request line")?;

    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| ParseError::new(400, "missing HTTP method"))?
        .to_string();
    let path = parts
        .next()
        .ok_or_else(|| ParseError::new(400, "missing HTTP request-target"))?
        .to_string();
    // The HTTP-version token (3rd word) is read but not otherwise checked —
    // this door only ever needs to understand HTTP/1.1 requests to itself.

    let mut content_length: usize = 0;
    let mut bearer: Option<String> = None;
    let mut signed_node: Option<String> = None;
    let mut signed_timestamp: Option<String> = None;
    let mut signed_nonce: Option<String> = None;
    let mut signed_signature: Option<String> = None;
    let mut signed_mesh: Option<String> = None;
    let mut header_count: usize = 0;
    loop {
        if header_count >= MAX_HEADERS {
            return Err(ParseError::new(400, format!("too many headers (max {MAX_HEADERS})")));
        }
        let header_line = match read_bounded_line(r, "header line")? {
            Some(l) => l,
            None => break, // connection closed mid-headers; tolerate (no body to read)
        };
        header_count += 1;
        check_deadline("reading headers")?;
        if header_line.is_empty() {
            break; // the blank line ending the header block
        }
        if let Some((name, value)) = header_line.split_once(':') {
            let name = name.trim();
            let value = value.trim();
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.parse().unwrap_or(0);
            } else if name.eq_ignore_ascii_case("authorization") {
                bearer = extract_bearer(value);
            } else if name.eq_ignore_ascii_case(aoide_storage::wire_auth::HEADER_NODE) {
                signed_node = (!value.is_empty()).then(|| value.to_string());
            } else if name.eq_ignore_ascii_case(aoide_storage::wire_auth::HEADER_TIMESTAMP) {
                signed_timestamp = (!value.is_empty()).then(|| value.to_string());
            } else if name.eq_ignore_ascii_case(aoide_storage::wire_auth::HEADER_NONCE) {
                signed_nonce = (!value.is_empty()).then(|| value.to_string());
            } else if name.eq_ignore_ascii_case(aoide_storage::wire_auth::HEADER_SIGNATURE) {
                signed_signature = (!value.is_empty()).then(|| value.to_string());
            } else if name.eq_ignore_ascii_case(aoide_storage::wire_auth::HEADER_MESH) {
                signed_mesh = (!value.is_empty()).then(|| value.to_string());
            }
        }
    }

    // Reject an oversize body BEFORE allocating anything for it — an
    // untrusted `Content-Length: 999999999999` must never reach a `Vec`
    // allocation (that aborts the whole process on failure, not just this
    // connection).
    if content_length > MAX_BODY {
        return Err(ParseError::new(
            413,
            format!("request body too large: {content_length} bytes (max {MAX_BODY})"),
        ));
    }
    check_deadline("before reading body")?;

    let mut body = Vec::new();
    if content_length > 0 {
        r.by_ref()
            .take(content_length as u64)
            .read_to_end(&mut body)
            .map_err(|e| ParseError::new(400, format!("reading body ({content_length} bytes): {e}")))?;
        if body.len() != content_length {
            return Err(ParseError::new(
                400,
                format!(
                    "connection closed before full body arrived ({} of {content_length} bytes)",
                    body.len()
                ),
            ));
        }
    }

    Ok(HttpRequest {
        method,
        path,
        body,
        bearer,
        signed_node,
        signed_timestamp,
        signed_nonce,
        signed_signature,
        signed_mesh,
    })
}

fn status_reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        503 => "Service Unavailable",
        _ => "Internal Server Error",
    }
}

/// Write one HTTP/1.1 response with a JSON body.
fn write_http_response<W: Write>(w: &mut W, status: u16, body: &[u8]) -> std::io::Result<()> {
    write_http_response_with(w, status, body, &[])
}

/// [`write_http_response`] with `extra` header lines after `Connection: close`.
fn write_http_response_with<W: Write>(w: &mut W, status: u16, body: &[u8], extra: &[(&str, String)]) -> std::io::Result<()> {
    let mut head = format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
        status_reason(status),
        body.len(),
    );
    for (name, value) in extra {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    w.write_all(head.as_bytes())?;
    w.write_all(body)?;
    w.flush()
}

/// The answer-signature header for one response (relay-proof), or nothing.
/// Only a JSON-RPC POST to `/` is ever signed: the card GET and every other
/// path stay unsigned even when their signature verified, so the door's key
/// signs nothing a client does not check. The signature covers the request's
/// own signature and nonce, the status and the exact body bytes; a key that
/// will not load, or is not on disk, leaves the answer unsigned and audited,
/// and the client refuses an unsigned answer.
fn answer_headers(req: &HttpRequest, status: u16, body: &[u8], audit_log: &Path) -> Vec<(&'static str, String)> {
    if req.method != "POST" || req.path != "/" {
        return Vec::new();
    }
    let (Some(request_signature), Some(request_nonce)) = (req.signed_signature.as_deref(), req.signed_nonce.as_deref()) else {
        return Vec::new();
    };
    // Load only: answering a request must never mint this box's identity. A
    // missing key is an unsigned answer, audited, like a key that will not load.
    match aoide_storage::identity::load_existing() {
        Ok(Some(kp)) => {
            let signature = aoide_storage::wire_auth::sign_answer(&kp, request_signature, request_nonce, status, body);
            vec![(aoide_storage::wire_auth::HEADER_ANSWER_SIGNATURE, signature)]
        }
        Ok(None) => {
            let _ = audit(
                audit_log,
                Door::A2a,
                EventClass::Audit,
                "a2a.answer-unsigned",
                "absent",
                "no identity key is on disk, so this answer goes out unsigned",
            );
            Vec::new()
        }
        Err(e) => {
            let _ = audit(
                audit_log,
                Door::A2a,
                EventClass::Audit,
                "a2a.answer-unsigned",
                "error",
                &format!("the identity key would not load, so this answer goes out unsigned: {e}"),
            );
            Vec::new()
        }
    }
}

// ── Routing ──────────────────────────────────────────────────────────────────

/// The audit-log command label for one POST `/` body's method: the JSON-RPC
/// method name *whitelisted* against the door's known set, never interpolated
/// verbatim out of the untrusted body (a hostile body could otherwise bloat
/// the audit log or plant a misleading label — e.g. `a2a.graph.session
/// delete`, or a multi-KB string). Shared by the door's [`route`] and the
/// mail adapter's [`route_mail`], so the two listeners agree on every label
/// they emit. `history_asked` wins the tie over `frame_asked` (the stricter
/// read is the one worth naming). Pure.
fn rpc_method_label(parsed_method: Option<&str>, history_asked: bool, frame_asked: bool) -> &'static str {
    match parsed_method {
        Some("tasks/get") if history_asked => "tasks/get.history",
        Some("tasks/get") if frame_asked => "tasks/get.frame",
        Some("tasks/get") => "tasks/get",
        Some("message/send") => "message/send",
        Some("message/stream") => "message/stream",
        Some("tasks/resubscribe") => "tasks/resubscribe",
        Some("tasks/cancel") => "tasks/cancel",
        Some("aoide/graphSummary") => "aoide/graphSummary",
        Some("aoide/pairRequest") => "aoide/pairRequest",
        Some("aoide/pairReveal") => "aoide/pairReveal",
        Some("aoide/pairPoll") => "aoide/pairPoll",
        Some("aoide/mailDeposit") => "aoide/mailDeposit",
        Some("aoide/mailPoll") => "aoide/mailPoll",
        // P-CHARTER: the door's own join read, named here so a `charterFetch`
        // at the door does not audit as bare `a2a.rpc` (the adapter labels the
        // same attempt `mail-adapter.refused aoide/charterFetch`, since it
        // cannot serve it — the two listeners now disagree for a reason).
        Some("aoide/charterFetch") => "aoide/charterFetch",
        Some("aoide/binding") => "aoide/binding",
        _ => "rpc",
    }
}

fn not_found(method: &str, path: &str) -> (u16, Vec<u8>, String) {
    let body = jsonrpc_error_value(-32601, format!("not found: {method} {path}"));
    (
        404,
        serde_json::to_vec(&body).unwrap_or_default(),
        "a2a.not-found".to_string(),
    )
}

fn method_not_allowed(method: &str, path: &str) -> (u16, Vec<u8>, String) {
    let body = jsonrpc_error_value(-32600, format!("method not allowed: {method} {path}"));
    (
        405,
        serde_json::to_vec(&body).unwrap_or_default(),
        "a2a.method-not-allowed".to_string(),
    )
}

// ── P-P4: per-request signature verification for paired nodes ───────────────
//
// `docs/architecture/PAIRING.md`'s "Wire authentication (paired nodes)"
// section: a request carrying all four `aoide_storage::wire_auth::HEADER_*`
// headers claims to come from a specific, already-PAIRED node, proven by an
// ed25519 signature over that ONE request's own method/path/timestamp/
// nonce/body-digest. Verification runs ONCE, in [`handle_connection`],
// BEFORE either dispatch path (the one-shot `route`/`handle_jsonrpc_bytes`
// path AND the SSE `stream_task` path) — a request that claims a paired
// node and FAILS verification in any way is refused OUTRIGHT, fail-closed,
// with no fallthrough to the weaker addr/token ladder `resolve_node`
// already offers (the #84 "sentinel on resolve failure" discipline,
// applied here as an outright request refusal rather than an unmatchable
// token). A request carrying NONE of the four headers is untouched by any
// of this — [`SignedRequestOutcome::Unsigned`] — and flows through exactly
// as it did before this phase.

/// One replay memory per verifying key: `nonce -> the epoch its frame stops
/// being accepted`, which is the frame's own timestamp plus
/// `signature_skew_secs()`. PROCESS-LOCAL and never disk-persisted
/// (`aoide-storage`'s `wire_auth` module doc gives the placement's reasoning).
/// A restart clears it, and the start check in [`verify_signed_request`] closes
/// that gap: a frame stamped at or before this process started plus five seconds
/// is refused, so nothing an earlier process accepted can be accepted again here.
///
/// Keyed on the PUBKEY that verified the request, not any node name (#63
/// P-ID5): `X-Aoide-Node` is outside the canonical string, so a captured request
/// replayed under another claimed name lands in the same set, exactly because it
/// is one signer. Only a frame that verified and passed its audience check ever
/// reaches a set, so the map holds at most one entry per signer admitted here.
static NONCE_SETS: Mutex<BTreeMap<String, BTreeMap<String, i64>>> = Mutex::new(BTreeMap::new());

/// The default for [`NONCE_CAP_ENV`]: unexpired nonces one key may hold.
const NONCE_CAP_DEFAULT: usize = 512;

/// The knob for [`NONCE_CAP_DEFAULT`]. Unset, blank, non-numeric or zero
/// all give the default, as the door's other knobs do.
const NONCE_CAP_ENV: &str = "AOIDE_NONCE_CAP";

/// The code for a key whose set is full of unexpired nonces. It is answered
/// over HTTP 503 ([`refusal_status`]), which a caller's transport path backs off
/// on, so its letter stays live.
const NONCE_BUSY_CODE: i64 = -32013;

/// What [`nonce_check`] decided for one `(key, nonce)` pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NonceCheck {
    /// Not recorded under this key. It is recorded now, until its window closes.
    Fresh,
    /// Recorded under this key, and its window is still open.
    Replay,
    /// This key's set is full of unexpired nonces. Nothing is recorded.
    Busy,
}

/// Check-and-record one verified request's nonce under its signer's key, with at
/// most `cap` unexpired entries per key.
///
/// Only THIS key's expired entries are dropped, and only here. A set never evicts
/// an unexpired entry to make room: a full set refuses its next new nonce as
/// [`NonceCheck::Busy`] and records nothing. The recorded-nonce check comes before
/// the cap, so a replay at the cap is still a replay. The key and the nonce are
/// both trimmed and lowercased (`wire_auth::canonical_string` does the same to
/// the nonce), so a case-changed copy of a captured nonce is the same nonce, and
/// twin records whose stored keys differ only in hex case share one set.
/// Poison-recovering like every other process-local lock in this
/// workspace (`pairing.rs`'s `PARK_LOCK` precedent): a panic inside one caller
/// must never wedge every other signed request.
fn nonce_check(key: &str, nonce: &str, ts_epoch: i64, now_epoch: i64, cap: usize) -> NonceCheck {
    let key = key.trim().to_ascii_lowercase();
    let nonce = nonce.trim().to_ascii_lowercase();
    let expires = ts_epoch + aoide_storage::wire_auth::signature_skew_secs();
    let mut sets = NONCE_SETS.lock().unwrap_or_else(|e| e.into_inner());
    let seen = sets.entry(key).or_default();
    seen.retain(|_, until| *until >= now_epoch);
    if seen.contains_key(&nonce) {
        NonceCheck::Replay
    } else if seen.len() >= cap {
        NonceCheck::Busy
    } else {
        seen.insert(nonce, expires);
        NonceCheck::Fresh
    }
}

/// [`nonce_check`]'s cap: [`NONCE_CAP_ENV`] when it parses, else the default.
fn nonce_cap() -> usize {
    parse_nonce_cap(std::env::var(NONCE_CAP_ENV).ok().as_deref())
}

/// The parse behind [`nonce_cap`], split out so the knob's fallbacks are tested
/// without touching the process environment.
fn parse_nonce_cap(raw: Option<&str>) -> usize {
    raw.map(str::trim)
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(NONCE_CAP_DEFAULT)
}

/// The one replay step, taken by [`admit_verified`] for each verified frame that
/// passes its audience check, so every listener words and codes a replay alike.
/// `who` names the signer in the text.
fn consume_nonce(who: &str, key: &str, nonce: &str, ts_epoch: i64, now_epoch: i64) -> Result<(), (i64, String)> {
    let cap = nonce_cap();
    match nonce_check(key, nonce, ts_epoch, now_epoch, cap) {
        NonceCheck::Fresh => Ok(()),
        NonceCheck::Replay => Err((
            -32009,
            format!(
                "nonce replay: {who} reused a `{}` value already seen within the current replay window",
                aoide_storage::wire_auth::HEADER_NONCE
            ),
        )),
        NonceCheck::Busy => Err((
            NONCE_BUSY_CODE,
            format!(
                "busy: {who} already has {cap} signed requests recorded in the current replay window, so this one is \
                 refused and not recorded; retry once the oldest of them expires (`{NONCE_CAP_ENV}` raises the cap)"
            ),
        )),
    }
}

/// The HTTP status a refusal is answered with. A busy key is 503, so the caller
/// backs off on its transport path and the letter stays live; every other
/// refusal is 200 with the code in the body.
fn refusal_status(code: i64) -> u16 {
    if code == NONCE_BUSY_CODE {
        503
    } else {
        200
    }
}

/// The caller identity [`verify_signed_request`] PROVED for one request: the
/// RESOLVED record's name (#63 P-ID5 — the record whose stored pubkey verified,
/// never the `X-Aoide-Node` label) plus the stored key that did the verifying.
/// ONE value, because they are one fact: `message_send` loads the registry
/// again for its own allows questions, and a consumer that took the
/// name from this outcome and the key from that second read could pair this
/// request's name with a key that never verified anything (a hand-edited
/// registry with two records sharing a name, or a same-uid edit of
/// `nodes.json` between the two loads). The claim's `remoteParent.key` is
/// built from `key` here, so the stamped key always IS the key that verified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SignedCaller<'a> {
    /// The resolved record's name — what every downstream consumer addresses
    /// the caller by (allows lookup, `node:<name>` origin stamp).
    name: &'a str,
    /// The resolved record's stored public key, the one the signature was
    /// verified against. Never empty: `verify_signed_request` resolves only a
    /// record whose non-empty stored key verified. This is also the key
    /// [`grant_in_mesh`] reads the caller's grant by (#63 P-ID5: identity is
    /// the key).
    key: &'a str,
    /// The mesh this request NAMED (`X-Aoide-Mesh`, P-CHARTER), or `None`
    /// when it named none — a pre-P-CHARTER peer, NOT an error
    /// (`wire_auth::canonical_string`'s five-field form is what such a peer
    /// can sign). [`effective_mesh`] is the one place the unnamed case
    /// becomes the home mesh, and the door RESOLVES with it before any read:
    /// the request's grant is then judged by that mesh's rules like anyone
    /// else naming it — its governing charter first, its paired records only
    /// where no charter is shaped for it (review N1, the user's ruling; the
    /// old "never matches a charter mesh" rule was overruled and is gone).
    ///
    /// It rides with the caller rather than beside it because only a VERIFIED
    /// signature proves it: the four `X-Aoide-*` headers and this one are one
    /// signed fact. A request with no caller has no mesh to act in, which is
    /// exactly what [`caller_grant`] answers with `Grant::none()`.
    mesh: Option<&'a str>,
}

/// [`verify_signed_request`]'s result — three shapes, not a `Result`,
/// because "no signature headers at all" is a THIRD outcome distinct from
/// both success and refusal (the untouched, pre-P-P4 path), and collapsing
/// it into `Ok(None)`/`Err(())` would blur that distinction at every call
/// site.
#[derive(Debug, Clone, PartialEq)]
enum SignedRequestOutcome {
    /// None of the four `HEADER_*` values were present — the existing
    /// addr/token resolution ladder applies exactly as before this phase.
    Unsigned,
    /// All four headers were present and verification succeeded. `resolved`
    /// is the name of the node RECORD whose stored pubkey verified the
    /// signature (#63 P-ID5: identity is the key) and `key` the stored pubkey
    /// that verified — together the [`SignedCaller`] the door threads
    /// downstream. `claimed` is what the `X-Aoide-Node` header said —
    /// display/attribution only, carried so the caller can audit a
    /// claimed-vs-resolved mismatch as attribution drift; it is never
    /// trusted and never wins over `resolved` anywhere. `mesh` is the mesh
    /// the request NAMED (`X-Aoide-Mesh`, P-CHARTER), `None` when it named
    /// none.
    Verified { resolved: String, key: String, claimed: String, mesh: Option<String> },
    /// Signature headers were present but verification failed somewhere —
    /// the JSON-RPC `(code, message)` the WHOLE request refuses with, no
    /// matter which method it named.
    Refused(i64, String),
}

/// The clock checks every signed frame passes before its signature is looked at.
/// A frame more than the knob's window behind this instance's own
/// "now", or more than [`SKEW_AHEAD_SECS`] ahead of it, is clock skew. A frame
/// stamped at or before this door's start plus [`SKEW_AHEAD_SECS`] is refused too:
/// the replay memory is per process and a restart clears it, so a frame an earlier
/// process could have accepted (stamped at most that far past its own start) must
/// not be accepted here. Both refusals are `-32008` and name both times.
fn time_refusal(timestamp: &str, ts_epoch: i64, now_epoch: i64, started_epoch: i64) -> Option<(i64, String)> {
    use aoide_storage::wire_auth::{HEADER_TIMESTAMP, SIGNATURE_SKEW_ENV, SKEW_AHEAD_SECS};

    let window = aoide_storage::wire_auth::signature_skew_secs();
    let now_text = aoide_storage::time::iso_utc_from_epoch(now_epoch);
    if !aoide_storage::wire_auth::within_skew(now_epoch, ts_epoch, window) {
        let detail = if ts_epoch > now_epoch {
            format!(
                "{} seconds ahead of this instance's own now (`{now_text}`); only {SKEW_AHEAD_SECS}s ahead is \
                 accepted, so the caller's clock is fast",
                ts_epoch - now_epoch
            )
        } else {
            format!(
                "{} seconds behind this instance's own now (`{now_text}`); the allowed window is {window}s behind \
                 (`{SIGNATURE_SKEW_ENV}` raises it)",
                now_epoch - ts_epoch
            )
        };
        return Some((-32008, format!("clock skew too large: this request's `{HEADER_TIMESTAMP}` (`{timestamp}`) is {detail}")));
    }
    if ts_epoch <= started_epoch + SKEW_AHEAD_SECS {
        return Some((
            -32008,
            format!(
                "this request's `{HEADER_TIMESTAMP}` (`{timestamp}`) is not later than this door's start (`{}`) plus \
                 {SKEW_AHEAD_SECS}s: the replay memory is cleared at every start, and a frame stamped up to \
                 {SKEW_AHEAD_SECS}s after a start may be one an earlier process accepted, so it is refused; retry \
                 with a fresh timestamp",
                aoide_storage::time::iso_utc_from_epoch(started_epoch),
            ),
        ));
    }
    None
}

/// Verify a request's P-P4 signature headers, if it carries any (module doc
/// above). Pure aside from one read: the node registry
/// (`aoide_storage::node_store::load_nodes`). `now_epoch` is the caller's own
/// "now" so this stays unit-testable against a fixed clock, the same shape
/// `aoide_storage::pairing::park_inbound`'s own `now_epoch` parameter
/// holds.
///
/// **Resolution is BY KEY, not by name (#63 P-ID5).** The signature proves
/// possession of a private key; the caller's identity is the node record
/// whose stored `pubkey` verifies that signature — found by trying the
/// signature against every VERIFIED node's stored pubkey (operator-curated
/// small N; one ed25519 verify is microseconds, and the skew check below
/// runs first so a stale request never costs any). The `X-Aoide-Node`
/// header does no identity work: it is display/attribution, checked only
/// for wire-format validity, and consulted for exactly one thing — the
/// exact-name tiebreak when MULTIPLE verified records share the verifying
/// pubkey (the same remote instance paired under two names; both records
/// hold the same proven key, so the tiebreak picks among equal-security
/// records, it never elevates a name to identity). Shared-key records with
/// no exact-name match refuse as ambiguous rather than picking one — their
/// `allows` may differ, and guessing would grant one record's
/// grants on the other's behalf.
///
/// A signature no verified node's key verifies refuses with the SAME code
/// and message as a bad signature — they are literally the same code path,
/// so the refusal is never an existence oracle over the registry (unknown
/// key, unverified node, keyless record, and tampered request are
/// indistinguishable from outside).
///
/// Checks run in an order that never spends work verifying a signature for
/// a request that's already disqualified for a cheaper reason (malformed
/// headers, unparsable timestamp, clock skew). It records nothing: a verified
/// frame is admitted (its audience, then its nonce) by [`admit_verified`], so a
/// frame refused on the way leaves no nonce-set entry.
fn verify_signed_request(req: &HttpRequest, now_epoch: i64, started_epoch: i64) -> SignedRequestOutcome {
    use aoide_storage::wire_auth::{HEADER_NONCE, HEADER_NODE, HEADER_SIGNATURE, HEADER_TIMESTAMP};

    let claims_signed =
        req.signed_node.is_some() || req.signed_timestamp.is_some() || req.signed_nonce.is_some() || req.signed_signature.is_some();
    if !claims_signed {
        return SignedRequestOutcome::Unsigned;
    }
    let (Some(node_name), Some(timestamp), Some(nonce), Some(signature)) = (
        req.signed_node.as_deref(),
        req.signed_timestamp.as_deref(),
        req.signed_nonce.as_deref(),
        req.signed_signature.as_deref(),
    ) else {
        return SignedRequestOutcome::Refused(
            -32007,
            format!(
                "incomplete signed-request headers: `{HEADER_NODE}`/`{HEADER_TIMESTAMP}`/\
                 `{HEADER_NONCE}`/`{HEADER_SIGNATURE}` must all be present together, or not at all"
            ),
        );
    };

    // Domain separation (relay-proof): an answer's signature covers the string
    // `answer\0…`, and a request's covers its own method first. Canonical
    // strings fold the method's case, so any token spelling `ANSWER` would
    // otherwise build the answer's tag. Only POST is a signed request, so
    // every other method is refused before a signature is looked at. An
    // unsigned GET of the card returns `Unsigned` above, untouched.
    if req.method != "POST" {
        return SignedRequestOutcome::Refused(-32007, "a signed request must be a POST".to_string());
    }

    // Wire-format validity only — the VALUE never selects a record below.
    if !aoide_storage::node_store::valid_node_name(node_name) {
        return SignedRequestOutcome::Refused(-32007, format!("`{HEADER_NODE}` is not a well-formed node name: `{node_name}`"));
    }
    // P-CHARTER: the mesh is validated the same wire-format way, BEFORE the
    // signature is built over it, and never selects a record either — it
    // selects which GRANT of the resolved record applies
    // ([`grant_in_mesh`]). A malformed name is refused outright rather than
    // looked up, so a typo is a taught error and not a silent "no grants".
    if let Some(mesh) = req.signed_mesh.as_deref() {
        if !aoide_storage::node_store::valid_node_name(mesh) {
            return SignedRequestOutcome::Refused(
                -32007,
                format!("`{}` is not a well-formed mesh name: `{mesh}`", aoide_storage::wire_auth::HEADER_MESH),
            );
        }
    }

    let Some(ts_epoch) = aoide_storage::time::parse_iso_utc(timestamp) else {
        return SignedRequestOutcome::Refused(-32007, format!("`{HEADER_TIMESTAMP}` is not a valid ISO-8601 timestamp: `{timestamp}`"));
    };
    if let Some((code, message)) = time_refusal(timestamp, ts_epoch, now_epoch, started_epoch) {
        return SignedRequestOutcome::Refused(code, message);
    }

    // `&req.method` (P-P4 review finding 2), not a hardcoded `"POST"`
    // literal: the OBSERVED method of THIS request, so the canonical
    // string is genuinely bound to what arrived, not a re-typed assumption
    // that happens to agree with it. Every real signed request today IS a
    // POST (the client's own `HTTP_METHOD` constant — `aoide-client::
    // commands` — never builds anything else), so this changes no byte of
    // any existing signature's canonical string or the pinned vectors
    // (CONTRACTS.md §6) — it only makes the module doc's "binds method"
    // claim structurally true instead of coincidentally true.
    let canonical = aoide_storage::wire_auth::canonical_string(&req.method, &req.path, timestamp, nonce, &req.body, req.signed_mesh.as_deref());
    let nodes = aoide_storage::node_store::load_nodes();
    // By-key resolution (fn doc): every verified record whose stored pubkey
    // verifies this signature. An unverified or keyless record never enters
    // the trial set — an unverified node's key can never resolve.
    let candidates: Vec<&aoide_storage::node_store::Node> = nodes
        .iter()
        .filter(|p| p.verified)
        .filter(|p| {
            p.pubkey
                .as_deref()
                .filter(|k| !k.is_empty())
                .is_some_and(|k| aoide_storage::wire_auth::verify_signature_hex(k, canonical.as_bytes(), signature))
        })
        .collect();
    // ONE refusal for unknown key / unverified node / keyless record / bad
    // signature alike — never an existence oracle over the registry.
    let resolved = match candidates.as_slice() {
        [] => {
            // **Third rung (P-CHARTER, review F3): a TRUSTED, IN-FORCE
            // charter's own node list.** A charter mesh's node list IS its
            // trust, and a machine that took the mesh by `mesh join` or
            // `mesh charter accept` holds no `nodes.json` record for anyone —
            // so without this rung a charter letter could never reach the
            // machines the non-LAN paths create, and "later versions arrive as
            // letters" was unreachable through the door.
            //
            // It is deliberately NARROW: only the mesh the request SIGNED
            // (`req.signed_mesh`, never `container.mesh`), only a mesh this
            // node can HONOUR (`charter::governing` — an unverified,
            // superseded or undecidable charter resolves nobody), and the key
            // must be one the charter's line carries. The grant that follows is
            // still the charter LINE's (`grant_in_mesh`), so this rung decides
            // IDENTITY only, exactly as the registry rung does.
            let charter_signer = req
                .signed_mesh
                .as_deref()
                .and_then(aoide_storage::charter::governing)
                .and_then(|charter| {
                    charter.nodes.iter().find(|(_, line)| {
                        aoide_storage::wire_auth::verify_signature_hex(&line.key, canonical.as_bytes(), signature)
                    })
                    .map(|(name, line)| (name.clone(), line.key.clone()))
                });
            let Some((name, key)) = charter_signer else {
                return SignedRequestOutcome::Refused(-32007, "signature verification failed".to_string());
            };
            return SignedRequestOutcome::Verified {
                resolved: name,
                key,
                claimed: node_name.to_string(),
                mesh: req.signed_mesh.clone(),
            };
        }
        [one] => *one,
        several => {
            let Some(exact) = several.iter().find(|p| p.name == node_name) else {
                return SignedRequestOutcome::Refused(
                    -32007,
                    format!(
                        "ambiguous signer: {} verified node records share the public key that verifies this \
                         signature and none is named `{node_name}` — send `{HEADER_NODE}` naming one of them, \
                         or remove the duplicate record (`node remove`)",
                        several.len()
                    ),
                );
            };
            *exact
        }
    };
    SignedRequestOutcome::Verified {
        resolved: resolved.name.clone(),
        // The stored key that verified, verbatim from the SAME record
        // `resolved` names — threaded out so the claim's `remoteParent.key` is
        // this value and never a re-read of the registry (`SignedCaller`'s own
        // doc says why). Non-empty by construction: the trial set above admits
        // only records with a non-empty stored pubkey.
        key: resolved.pubkey.clone().unwrap_or_default(),
        claimed: node_name.to_string(),
        mesh: req.signed_mesh.clone(),
    }
}

/// The audit detail `handle_connection` logs when a verified request's
/// claimed `X-Aoide-Node` name and its key-resolved record disagree
/// (#63 P-ID5) — `None` when they agree (the overwhelmingly common case,
/// nothing logged). Drift is ATTRIBUTION news, never a gate: the request
/// already proved possession of the resolved record's key, so it proceeds
/// as the resolved node everywhere; this line only keeps the operator's
/// audit trail honest about what the wire claimed. Pure, extracted from
/// `handle_connection` the same way [`spawn_admitted`] is — testable
/// without a socket.
fn attribution_drift_detail(claimed: &str, resolved: &str) -> Option<String> {
    (claimed != resolved).then(|| {
        format!(
            "attribution drift: `{}` claimed `{claimed}` but the signature verifies against the stored \
             public key of node `{resolved}` — proceeding as `{resolved}`; the claimed name is a label, \
             never an identity",
            aoide_storage::wire_auth::HEADER_NODE
        )
    })
}

/// The signed methods an old client may still call without `params.to`: the
/// two mail methods, the binding read and the LAN join's charter read. Any
/// other signed method needs `to`, and a method added later is closed until
/// it is listed here.
const UNTAGGED_METHODS: &[&str] = &["aoide/mailDeposit", "aoide/mailPoll", "aoide/binding", "aoide/charterFetch"];

/// What the audience step admits one VERIFIED frame as. `params.to` is the
/// callee's identity key (lowercase hex), so a frame signed for another door
/// is refused even though this door trusts its signer.
#[derive(Debug, PartialEq, Eq)]
enum Audience {
    /// `to` names this door's key.
    Admit,
    /// No `to` on an allow-listed method: an old client, admitted and audited.
    Untagged { method: String },
}

/// The audience of one verified body. A body that is not JSON, or names no
/// method, is refused with the answer its listener has always given it: `-32700`
/// for bytes that do not parse, then the listener's own missing-method code
/// (`-32600` on the A2A door, `-32601` on the mail adapter). A named method needs
/// `params.to` to be this door's own key, or to be on [`UNTAGGED_METHODS`].
fn audience(listener: Listener, body: &[u8], own_key: Option<&str>) -> Result<Audience, (i64, String)> {
    let request = serde_json::from_slice::<Value>(body).map_err(|e| (-32700, format!("parse error: {e}")))?;
    let Some(method) = request.get("method").and_then(Value::as_str).filter(|m| !m.is_empty()) else {
        return Err(match listener {
            Listener::A2a => (-32600, "invalid request: missing method".to_string()),
            Listener::Mail => (-32601, "method not found: ".to_string()),
        });
    };
    match request.get("params").and_then(|p| p.get("to")) {
        Some(to) => {
            let named = to.as_str().map(|t| t.trim().to_ascii_lowercase());
            if named.is_some() && named.as_deref() == own_key {
                Ok(Audience::Admit)
            } else {
                Err((
                    -32012,
                    "wrong-audience: `params.to` is not this door's identity key, so this request was signed \
                     for another door; address it to the key of the node it is sent to"
                        .to_string(),
                ))
            }
        }
        None if UNTAGGED_METHODS.contains(&method) => Ok(Audience::Untagged { method: method.to_string() }),
        None => Err((
            -32012,
            format!(
                "`{method}` needs `params.to`, the callee's identity key, and this request has none: this is a client \
                 that predates `to`, so upgrade aoide on the caller. Without `to` only {} are open",
                UNTAGGED_METHODS.join(", ")
            ),
        )),
    }
}

/// The steps a VERIFIED frame takes before its method runs or any answer is
/// written, in the one order every listener shares: its audience and shape
/// ([`audience`]), then its nonce. A step that refuses ends the frame and nothing
/// is recorded for it, so a frame refused for its audience leaves its nonce free
/// for the right call. `who` names the signer in a replay refusal, and `key` is
/// the stored key that verified the signature.
fn admit_verified(
    listener: Listener,
    req: &HttpRequest,
    who: &str,
    key: &str,
    now_epoch: i64,
) -> Result<Audience, (i64, String)> {
    use aoide_storage::wire_auth::{HEADER_NONCE, HEADER_TIMESTAMP};

    let admitted = audience(listener, &req.body, own_identity_key().as_deref())?;
    let (Some(timestamp), Some(nonce)) = (req.signed_timestamp.as_deref(), req.signed_nonce.as_deref()) else {
        return Err((-32007, format!("incomplete signed-request headers: `{HEADER_TIMESTAMP}` and `{HEADER_NONCE}` are required")));
    };
    let Some(ts_epoch) = aoide_storage::time::parse_iso_utc(timestamp) else {
        return Err((-32007, format!("`{HEADER_TIMESTAMP}` is not a valid ISO-8601 timestamp: `{timestamp}`")));
    };
    consume_nonce(who, key, nonce, ts_epoch, now_epoch)?;
    Ok(admitted)
}

/// This box's own identity key, read WITHOUT minting one: answering a request
/// never creates the identity (the answer path's own rule, `answer_headers`).
/// `None` when no key is on disk, which makes every `to` wrong.
fn own_identity_key() -> Option<String> {
    aoide_storage::identity::load_existing()
        .ok()
        .flatten()
        .map(|kp| kp.info().pubkey_hex.trim().to_ascii_lowercase())
}

/// Route one parsed request to (HTTP status, response body, audit-log
/// command label). `audit_log`/`spawn_agent`/`origin` are only consulted by a
/// POST `/` whose body parses as `message/send`; `node_name` (plus the
/// `self_url` this function derives from `bind`/`port`, the same way
/// [`agent_card`]'s own `url` field does) is only consulted by
/// `aoide/graphSummary`; `registry` and `expected_token` (compared against
/// `req.bearer`, CONTRACTS.md §6 2026-08-20 amendment) are only consulted by
/// the AgentCard GET, which strips the card down to `name`/`protocolVersion`/
/// `url` when a token is configured and the bearer doesn't classify `Valid` —
/// every other route is pure I/O-free routing over what's already in `req`,
/// so it still unit-tests without a real socket, spawn, or audit-log write.
/// `signed_caller` is P-P4's own addition: `Some(caller)` when
/// [`verify_signed_request`] already verified this request's signature
/// headers against a paired node — the KEY-resolved record's name and the
/// stored key that verified (#63 P-ID5), never the wire-claimed label (never
/// re-verified here — `handle_connection` runs that check exactly once, before
/// EITHER dispatch path), threaded straight into [`RequestCtx`] for
/// `message/send` to consume.
fn route(
    req: &HttpRequest,
    bind: &str,
    port: u16,
    audit_log: &Path,
    spawn_agent: &str,
    spawn_cwd: &str,
    node_name: &str,
    origin: ConnOrigin,
    expected_token: &str,
    registry: &Registry,
    signed_caller: Option<SignedCaller<'_>>,
) -> (u16, Vec<u8>, String) {
    match req.path.as_str() {
        "/.well-known/agent-card.json" => {
            if req.method == "GET" {
                let full = agent_card(registry, bind, port);
                let token_configured = !expected_token.is_empty();
                let token_state = classify_token(expected_token, req.bearer.as_deref());
                // A GET here returns a card, never JSON-RPC — no -32005 on
                // this path (CONTRACTS.md §6, 2026-08-20 amendment): an
                // unauthorized caller still gets 200 and a card, just the
                // stripped one, so discovery keeps working without leaking
                // the skills inventory/version/capabilities.
                let card = if token_authorized(token_configured, token_state) {
                    full
                } else {
                    stripped_card(&full)
                };
                (
                    200,
                    serde_json::to_vec(&card).unwrap_or_default(),
                    "a2a.agent-card".to_string(),
                )
            } else {
                method_not_allowed(&req.method, &req.path)
            }
        }
        "/" => {
            if req.method == "POST" {
                // Label the audit record with the JSON-RPC method when we can
                // parse enough of the body to see it, even if `handle_jsonrpc`
                // later rejects the request. `method` is attacker-controlled
                // (an unparsed string straight out of the untrusted body) —
                // whitelist it against the known method set rather than
                // interpolating it verbatim, so a hostile body can't bloat
                // the audit log or plant a misleading label (e.g.
                // `a2a.graph.session delete`, or a multi-KB string).
                let parsed = serde_json::from_slice::<Value>(&req.body).ok();
                let parsed_method = parsed
                    .as_ref()
                    .and_then(|v| v.get("method").and_then(Value::as_str).map(str::to_string));
                // A frame read gets its OWN label, off the same already-parsed
                // body: an operator scanning the audit log sees which
                // `tasks/get` calls read a session's output, and which were
                // only status polls (CONTRACTS.md §6, P-RSA S6). A history
                // read gets its own too (S8), and it wins the tie when a
                // request asks for both — the stricter read is the one worth
                // naming.
                let parsed_params = parsed.as_ref().and_then(|v| v.get("params"));
                let history_asked = parsed_params.and_then(lines_after).is_some();
                let frame_asked = parsed_params.and_then(frame_tail).is_some();
                let label = rpc_method_label(parsed_method.as_deref(), history_asked, frame_asked);
                let self_url = self_url(bind, port);
                let ctx = RequestCtx {
                    audit_log,
                    spawn_agent,
                    spawn_cwd,
                    origin,
                    node_name,
                    self_url: &self_url,
                    expected_token,
                    presented_token: req.bearer.as_deref(),
                    signed_caller,
                    sealed_only: false,
                };
                let resp = handle_jsonrpc_bytes(&req.body, &ctx);
                let body = serde_json::to_vec(&resp).unwrap_or_default();
                (200, body, format!("a2a.{label}"))
            } else {
                method_not_allowed(&req.method, &req.path)
            }
        }
        _ => not_found(&req.method, &req.path),
    }
}

// ── The mail adapter's route (H1) ───────────────────────────────────────────

/// Route one parsed request as the MAIL ADAPTER sees it. The same two paths
/// [`route`] serves — `GET /.well-known/agent-card.json` and `POST /` — over a
/// method table that holds three names and nothing else: every other method is
/// `-32601`, because [`handle_mail_jsonrpc`] has no second table to fall
/// through to.
///
/// The card is ALWAYS the stripped three-key shape ([`stripped_card`]), never
/// the door's authorize-or-strip choice: the mail profile has no door-wide
/// token concept, so there is no credentialed caller to hand the full card to,
/// and the relay's skills inventory must not reach the open internet.
///
/// The Host header is never consulted — through a tunnel it carries the
/// front's own hostname, which is not this process's business.
fn route_mail(
    req: &HttpRequest,
    bind: &str,
    port: u16,
    audit_log: &Path,
    origin: ConnOrigin,
    registry: &Registry,
    signed_caller: Option<SignedCaller<'_>>,
) -> (u16, Vec<u8>, String) {
    match req.path.as_str() {
        "/.well-known/agent-card.json" => {
            if req.method != "GET" {
                return method_not_allowed(&req.method, &req.path);
            }
            let card = stripped_card(&agent_card(registry, bind, port));
            (
                200,
                serde_json::to_vec(&card).unwrap_or_default(),
                "a2a.agent-card".to_string(),
            )
        }
        "/" => {
            if req.method != "POST" {
                return method_not_allowed(&req.method, &req.path);
            }
            let parsed = serde_json::from_slice::<Value>(&req.body).ok();
            let parsed_method = parsed
                .as_ref()
                .and_then(|v| v.get("method").and_then(Value::as_str).map(str::to_string));
            let label = mail_method_label(parsed_method.as_deref());
            let ctx = RequestCtx::mail_adapter(audit_log, origin, signed_caller);
            let resp = handle_mail_jsonrpc_bytes(&req.body, &ctx);
            let body = serde_json::to_vec(&resp).unwrap_or_default();
            (200, body, format!("a2a.{label}"))
        }
        _ => not_found(&req.method, &req.path),
    }
}

/// The audit label for one adapter `POST /` body (H1). A mail method keeps the
/// name the door emits for it, so a mail call reads identically wherever it
/// landed. Anything else is a request this listener cannot serve, and it is
/// labelled as exactly that — `mail-adapter.refused`, with the DOOR method
/// name it tried to look like taken from the same closed whitelist the door's
/// own label uses (never interpolated raw: a hostile body must not bloat the
/// log or plant a label). A log reader scanning for `message/send` still finds
/// the attempt, and sees at a glance that the adapter — a process with no such
/// method — turned it away, rather than a door method name emitted by a
/// listener that cannot serve it (the branch review's F5). Pure.
fn mail_method_label(parsed_method: Option<&str>) -> String {
    match parsed_method {
        Some("aoide/mailDeposit") => "aoide/mailDeposit".to_string(),
        Some("aoide/mailPoll") => "aoide/mailPoll".to_string(),
        Some("aoide/binding") => "aoide/binding".to_string(),
        Some(m) => match rpc_method_label(Some(m), false, false) {
            "rpc" => "mail-adapter.refused".to_string(),
            named => format!("mail-adapter.refused {named}"),
        },
        None => "mail-adapter.refused".to_string(),
    }
}

// ── Inbound bearer resolved via the secrets broker (task #84) ───────────────
// The token-FILE mechanism above (`resolve_token_file`/`read_expected_token`)
// reads its value ONCE, at `a2a serve` launch, and holds it in memory for the
// server's whole lifetime — fine for a file, since revoking it means editing
// the file and restarting the daemon anyway. A secrets-broker-resolved
// bearer must NOT work that way: the whole point of routing it through the
// broker is that `secrets rm`/a policy edit takes effect immediately, with
// no daemon restart — so this door must resolve it FRESH, every connection,
// never once and cached. That is the one deliberate architectural
// difference from the file mechanism below, and it is why
// [`InboundBearerConfig`] carries the INPUTS to a resolve (a secret name, a
// socket path) rather than a resolved value.

/// The self-asserted consumer name this door presents to the secrets broker
/// when resolving its own inbound bearer — see `crates/secrets/AGENTS.md`'s
/// honesty note (consumer identity is self-asserted; #63's seal
/// authenticates the session and its origin class, never this string):
/// nothing on the wire authenticates this string, it is simply the name an operator's
/// `policy.json` `consumers[]`/`automation.consumers` lists to grant
/// `a2a serve` access to the named secret.
const BEARER_CONSUMER_DOOR: &str = "a2a-door";

/// Bound on the inbound bearer resolve's socket READ (the PARKING HAZARD,
/// task #84): a misconfigured `requireTotp`-gated bearer secret with no
/// `automation`-open exemption for [`BEARER_CONSUMER_DOOR`] would otherwise
/// let the broker hold this call's read open for up to
/// `AOIDE_SECRETS_PARK_TIMEOUT` (default 300s) — an HTTP door has no human
/// to type a code into. `aoide_secrets::client::resolve_bounded`'s own
/// `wait:false` on the wire means the deployed, automation-open happy path
/// never even reaches this timeout; it exists purely as the second,
/// independent bound for a misconfigured deployment (that function's own
/// doc comment).
const BEARER_RESOLVE_TIMEOUT: Duration = Duration::from_secs(2);

/// The inbound-bearer MECHANISM `a2a serve` resolves once at launch (task
/// #84) — everything [`resolve_inbound_bearer`] needs to compute one
/// request's expected token fresh, never the value itself. `Clone` so
/// [`serve`]'s accept loop can hand each spawned connection-handler thread
/// its own copy, the same shape every other per-connection input below
/// already takes.
#[derive(Clone)]
struct InboundBearerConfig {
    /// [`resolve_bearer_secret`]'s result — a secrets-broker secret NAME,
    /// resolved fresh on every connection when non-empty. Takes precedence
    /// over `file_token` below.
    bearer_secret: String,
    /// The secrets broker's socket path (`aoide_secrets::socket::
    /// socket_path`), resolved once at launch — reused for every
    /// connection's resolve, never re-derived per request.
    secrets_socket: std::path::PathBuf,
    /// [`read_expected_token`]'s result — the pre-existing token-FILE
    /// mechanism's value, read once at launch. Consulted only when
    /// `bearer_secret` is empty; unchanged from before this task.
    file_token: String,
}

/// Resolve THIS connection's effective expected inbound bearer token (task
/// #84). `cfg.bearer_secret`, when set, takes precedence over
/// `cfg.file_token` and is resolved FRESH from the secrets broker via
/// [`aoide_secrets::client::resolve_bounded`] — nothing this function
/// returns is ever cached: `handle_connection` calls it once per accepted
/// connection and drops it once that connection's response has been
/// written, the "value lives only in the request path" discipline task
/// #84's brief calls for (`crates/secrets/AGENTS.md`'s "NO CACHE, EVER"
/// invariant, extended here to this door's consumption of the broker).
///
/// **Fails CLOSED without a second `token_configured` boolean rippling
/// through every downstream function (and its tests) in this file.** A
/// broker resolve failure (unreachable, denied, or
/// [`BEARER_RESOLVE_TIMEOUT`] elapsing) returns [`resolve_failure_sentinel`]
/// instead of an empty string. Unlike the true "no bearer mechanism
/// configured at all" case (empty string), this is a FRESH,
/// practically-unguessable-per-call value — so every downstream call site's
/// existing `!expected_token.is_empty()` "is a token configured" check
/// still reads `true` (every bearer check this request denies), while no
/// presented `Authorization: Bearer <token>` can ever happen to equal it
/// ([`resolve_failure_sentinel`]'s own doc). This reuses the EXACT
/// [`token_authorized`]/[`classify_token`] machinery every other bearer
/// check in this file already runs — `route`/`message_send`/`handle_jsonrpc`/
/// `stream_task` are UNCHANGED by this task, only `handle_connection`/
/// [`serve`] resolve the value differently now.
fn resolve_inbound_bearer(cfg: &InboundBearerConfig) -> String {
    if cfg.bearer_secret.is_empty() {
        return cfg.file_token.clone();
    }
    match aoide_secrets::client::resolve_bounded(
        &cfg.secrets_socket,
        &cfg.bearer_secret,
        BEARER_CONSUMER_DOOR,
        BEARER_RESOLVE_TIMEOUT,
    ) {
        Ok(value) if value.trim().is_empty() => {
            eprintln!(
                "aoide a2a: inbound bearer secret `{}` resolved to an empty value — \
                 refusing every bearer check on this connection (fail closed)",
                cfg.bearer_secret,
            );
            resolve_failure_sentinel()
        }
        Ok(value) => value,
        Err(e) => {
            // `e` is one of the resolve wire's own error strings
            // (CONTRACTS.md's "Secrets wire" catalog) — never the secret's
            // VALUE (this crate's own invariant, restated in
            // `resolve_bounded`'s doc); safe to log the secret's NAME and
            // this reason for an operator debugging a misconfiguration.
            eprintln!(
                "aoide a2a: inbound bearer resolve failed for secret `{}` via {}: {e} — \
                 refusing every bearer check on this connection (fail closed)",
                cfg.bearer_secret,
                cfg.secrets_socket.display(),
            );
            resolve_failure_sentinel()
        }
    }
}

/// A fresh, non-empty value no remote caller could predict or observe —
/// [`resolve_inbound_bearer`]'s fail-closed return on a broker resolve
/// failure. Built ONLY from this process's own pid, the current instant to
/// nanosecond precision, and a per-process atomic counter — never printed,
/// logged, or compared against anything but a presented bearer (and even
/// then, only ever on the LOSING side of that comparison: this value exists
/// solely to keep `!expected_token.is_empty()` true so every check fails
/// closed, not to BE a real credential).
fn resolve_failure_sentinel() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("unresolvable-a2a-bearer-secret::{}::{nanos}::{n}", std::process::id())
}

// ── The blocking accept loop ─────────────────────────────────────────────────

/// RAII in-flight-connection-slot guard: decrements [`IN_FLIGHT`] on drop,
/// including on a handler-thread panic, so a slot is always released — a
/// plain `fetch_add`/`fetch_sub` pair around the handler body would leak a
/// slot forever if the handler ever panicked instead of returning `Err`.
struct ConnGuard;

impl Drop for ConnGuard {
    fn drop(&mut self) {
        IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Serve the A2A door: bind `bind:port` and block, handling connections
/// thread-per-connection. Returns on a bind failure (a clean `Err`, never a
/// panic) — the caller (root `lib.rs::run_cli`) renders that as the process
/// exit code, same as `mcp serve --stdio`'s failure path.
///
/// `registry` is injected (see the module doc comment's "DI seam" note) and
/// must be `'static` — it is moved into each spawned connection-handler
/// thread, same as every other per-connection state below. Root `lib.rs`
/// passes `dispatch::registry()`, whose `&'static Registry` already satisfies
/// this.
///
/// A connection flood is bounded by [`MAX_CONN`]: past that many in-flight
/// handler threads, a new connection gets a fast `503` written directly
/// (no handler thread spawned, no `BufReader`/parse work done) rather than
/// growing the thread count without limit.
///
/// The advertise thread (`discovery::spawn_advertiser`) is the ONE thing
/// this function starts besides the accept loop itself — always spawned,
/// so the runtime switch (`aoide node advertise on|off`, read fresh every
/// tick) can take effect without a restart. `discovery_advertise` (P-P6,
/// [`resolve_discovery_advertise`]) is the launch-time FORCE-ON half,
/// OR'd with that switch per tick; with both off (the default) the thread
/// ticks silently and sends nothing. No identity file is touched either
/// way — the advertisement carries name + ssh hop info only, never a
/// fingerprint (rendezvous, not authentication; `aoide_storage::
/// advertise`'s module doc). A refused thread spawn is logged and costs
/// discovery only, never the door itself.
//
// TODO(a2a-hardening): chunked Transfer-Encoding and extra systemd
// sandboxing (aoide-a2a.service) are deliberately out of scope for this
// pass — see the security-review notes that produced this hardening.
pub fn serve(
    bind: &str,
    port: u16,
    audit_log: &Path,
    spawn_agent: &str,
    spawn_cwd: &str,
    node_name: &str,
    expected_token: &str,
    bearer_secret: &str,
    secrets_socket: &Path,
    discovery_advertise: bool,
    registry: &'static Registry,
) -> std::io::Result<()> {
    let started_epoch = unix_ts_now() as i64;
    let listener = TcpListener::bind((bind, port))?;
    eprintln!("aoide a2a: listening on http://{bind}:{port}/");
    let bearer_cfg = InboundBearerConfig {
        bearer_secret: bearer_secret.to_string(),
        secrets_socket: secrets_socket.to_path_buf(),
        file_token: expected_token.to_string(),
    };

    // Discovery advertising (P-P6 + task #120) — the thread always spawns
    // so the runtime switch (`aoide node advertise on|off`) works without
    // a restart; whether a tick SENDS is `discovery_advertise ||
    // aoide_storage::advertise::enabled()`, checked inside the thread.
    // `user` is this process's own login ($USER → $LOGNAME, the same chain
    // `aoide-client::tunnel::resolve_login` walks) — with neither set
    // there is no ssh hop to advertise, so advertising is skipped with a
    // taught line rather than emitting a line every listener would drop as
    // invalid. The join handle is deliberately dropped: dropping a
    // `JoinHandle` detaches nothing extra (the thread already runs
    // independent of it), and this function itself never returns until the
    // process exits, so there is no later point to join it against anyway
    // (module doc's "clean shutdown needs no signal" note).
    {
        let host = aoide_storage::display::local_host_name();
        let user = std::env::var("USER")
            .ok()
            .filter(|u| !u.trim().is_empty())
            .or_else(|| std::env::var("LOGNAME").ok().filter(|u| !u.trim().is_empty()))
            .unwrap_or_default();
        if !aoide_storage::advertise::valid_user(&user) {
            eprintln!(
                "aoide a2a discovery: no usable ssh login for an advertisement (neither $USER \
                 nor $LOGNAME holds one) — continuing without discovery advertising"
            );
        } else if crate::discovery::spawn_advertiser(node_name, &host, &user, discovery_advertise)
            .is_some()
            && discovery_advertise
        {
            // The "advertising" claim is only printed when this process is
            // FORCED on — the runtime switch's state can change under a
            // long-lived process, so its ticks speak for themselves.
            eprintln!(
                "aoide a2a discovery: advertising {node_name} ({user}@{host}) by broadcast \
                 {}:{}",
                aoide_storage::advertise::BROADCAST_ADDR,
                aoide_storage::advertise::PORT
            );
        }
    }

    for incoming in listener.incoming() {
        let mut stream = match incoming {
            Ok(s) => s,
            Err(e) => {
                eprintln!("aoide a2a: accept error: {e}");
                continue;
            }
        };

        let prior = IN_FLIGHT.fetch_add(1, Ordering::SeqCst);
        if prior >= MAX_CONN {
            IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
            let body = jsonrpc_error_value(
                -32000,
                "server busy: too many in-flight connections, try again shortly",
            );
            let body = serde_json::to_vec(&body).unwrap_or_default();
            // Never signed: no request was read, so there is nothing to answer.
            if let Err(e) = write_http_response(&mut stream, 503, &body) {
                eprintln!("aoide a2a: writing 503 (busy) response: {e}");
            }
            continue;
        }

        let bind = bind.to_string();
        let audit_log = audit_log.to_path_buf();
        let spawn_agent = spawn_agent.to_string();
        let spawn_cwd = spawn_cwd.to_string();
        let node_name = node_name.to_string();
        let bearer_cfg = bearer_cfg.clone();
        std::thread::spawn(move || {
            let _guard = ConnGuard; // released on every exit path, incl. panic
            if let Err(e) = handle_connection(
                stream,
                &bind,
                port,
                &audit_log,
                &spawn_agent,
                &spawn_cwd,
                &node_name,
                Some(&bearer_cfg),
                Listener::A2a,
                registry,
                started_epoch,
            ) {
                eprintln!("aoide a2a: connection error: {e}");
            }
        });
    }
    Ok(())
}

/// Serve the mail adapter (H1) — a second listener that is NOT the door.
///
/// It binds [`MAIL_ADAPTER_BIND`] and nothing else: there is no bind option,
/// flag or env var that could move it off loopback, because the thing facing
/// the mesh is a TLS-terminating front (cloudflared, a VPS, a tailnet) and the
/// A2A door is never fronted by one. Everything it serves is [`route_mail`]'s
/// table — `aoide/mailDeposit`, `aoide/mailPoll`, `aoide/binding`, and the
/// stripped card — so the conduct path (`message/send`'s inject and spawn arms,
/// the SSE takeover, the pairing ceremony, `tasks/get`) is unreachable from
/// this process by construction, not by refusal.
///
/// Shares the door's hostile-input hardening ([`MAX_BODY`], [`MAX_LINE`],
/// [`MAX_HEADERS`], [`MAX_REQUEST`], [`MAX_CONN`]/[`IN_FLIGHT`]) and its
/// per-request signature verification (and so its process-local nonce sets
/// and the single audit log) — those are properties of the transport, not of
/// the method table. Deliberately NOT shared: the inbound bearer (no token
/// concept here), discovery advertising (an adapter is not a discoverable
/// agent), and `message/stream`.
pub fn serve_mail(port: u16, audit_log: &Path, registry: &'static Registry) -> std::io::Result<()> {
    serve_mail_from(port, audit_log, registry, unix_ts_now() as i64)
}

/// [`serve_mail`] with the start it records passed in. The caller stamps it before
/// the bind, as `serve_mail` does. An in-process test door passes a start in the
/// past, so a request signed at once is admitted.
fn serve_mail_from(port: u16, audit_log: &Path, registry: &'static Registry, started_epoch: i64) -> std::io::Result<()> {
    let listener = TcpListener::bind((MAIL_ADAPTER_BIND, port))?;
    eprintln!(
        "aoide mail serve: listening on http://{MAIL_ADAPTER_BIND}:{port}/ (loopback only; TLS \
         terminates at the front)"
    );

    for incoming in listener.incoming() {
        let mut stream = match incoming {
            Ok(s) => s,
            Err(e) => {
                eprintln!("aoide mail serve: accept error: {e}");
                continue;
            }
        };

        let prior = IN_FLIGHT.fetch_add(1, Ordering::SeqCst);
        if prior >= MAX_CONN {
            IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
            let body = jsonrpc_error_value(
                -32000,
                "server busy: too many in-flight connections, try again shortly",
            );
            let body = serde_json::to_vec(&body).unwrap_or_default();
            // Never signed: no request was read, so there is nothing to answer.
            if let Err(e) = write_http_response(&mut stream, 503, &body) {
                eprintln!("aoide mail serve: writing 503 (busy) response: {e}");
            }
            continue;
        }

        let audit_log = audit_log.to_path_buf();
        std::thread::spawn(move || {
            let _guard = ConnGuard; // released on every exit path, incl. panic
            // `""` throughout the door-only string parameters: no method this
            // listener serves reads a spawn command, a spawn cwd or an
            // instance name (see `RequestCtx::mail_adapter`). `None` for the
            // bearer config, for the same reason.
            if let Err(e) = handle_connection(
                stream,
                MAIL_ADAPTER_BIND,
                port,
                &audit_log,
                "",
                "",
                "",
                None,
                Listener::Mail,
                registry,
                started_epoch,
            ) {
                eprintln!("aoide mail serve: connection error: {e}");
            }
        });
    }
    Ok(())
}

/// Handle one connection: parse exactly one request, route it, audit it,
/// write the response. A malformed request never panics — it becomes a 4xx
/// with a JSON-RPC-style error body, same as every other routing failure.
fn handle_connection(
    stream: TcpStream,
    bind: &str,
    port: u16,
    audit_log: &Path,
    spawn_agent: &str,
    spawn_cwd: &str,
    node_name: &str,
    bearer_cfg: Option<&InboundBearerConfig>,
    listener: Listener,
    registry: &Registry,
    started_epoch: i64,
) -> std::io::Result<()> {
    // The connection's ORIGIN (CONTRACTS.md §6 amendment, 2026-08-14): TCP
    // `peer_addr()`, not any client-supplied field — a hostile client cannot
    // spoof this. Resolved once, BEFORE the read-timeout/BufReader wrapping
    // below (which only affect reading, not this), and threaded to every
    // path that can reach `message/send` (the one-shot route below AND the
    // `message/stream` SSE path).
    let origin = classify_origin(stream.peer_addr().ok().map(|sa| sa.ip()));

    // Never let one slow/hostile client wedge a server thread forever: the
    // per-read timeout catches a fully-idle client, and the absolute
    // `MAX_REQUEST` deadline (checked inside `parse_http_request`) catches
    // one that dribbles a byte at a time just inside that timeout.
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;

    let start = Instant::now();
    let req = match parse_http_request(&mut reader, start) {
        Ok(req) => req,
        // A peer that connected and closed without a byte (the client's
        // tunnel-reuse probe) asked nothing: no response, no audit line.
        Err(e) if e.idle => return Ok(()),
        Err(e) => {
            let b = jsonrpc_error_value(-32700, format!("bad request: {}", e.message));
            let body = serde_json::to_vec(&b).unwrap_or_default();
            let _ = audit(
                audit_log,
                Door::A2a,
                EventClass::Audit,
                "a2a.bad-request",
                "error",
                &listener.audit_detail(e.status, origin),
            );
            // Never signed: the request did not parse, so nothing was verified.
            return write_http_response(&mut writer, e.status, &body);
        }
    };

    let expected_token = match (listener, bearer_cfg) {
        // task #84: resolve THIS connection's effective expected bearer fresh —
        // AFTER a successful parse (a malformed request never touches the
        // broker at all), never once at `a2a serve` launch when a broker
        // secret is configured — see `resolve_inbound_bearer`'s own doc for the
        // fail-closed/no-cache reasoning. `expected_token` is a local `String`
        // that lives only for the rest of this one connection's handling.
        (Listener::A2a, Some(cfg)) => resolve_inbound_bearer(cfg),
        // The mail adapter has no door token concept at all (H1): nothing is
        // resolved, `expected_token` stays empty and `unauthorized()` never
        // fires here — the three mail methods are signature-gated instead.
        _ => String::new(),
    };

    // P-P4 (`docs/architecture/PAIRING.md`'s "Wire authentication" section):
    // verify this request's signature headers, if any, EXACTLY ONCE here —
    // before EITHER dispatch path below — so a request that claims a paired
    // node and fails verification is refused fail-closed regardless of
    // which JSON-RPC method it named, never reaching `stream_task` OR
    // `route`/`handle_jsonrpc_bytes`. A request carrying no signature
    // headers at all (`SignedRequestOutcome::Unsigned`) is completely
    // untouched by this — `signed_caller` stays `None`, and everything
    // below behaves exactly as it did before this phase.
    let now_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    // `resolved` AND `key` are threaded together (`SignedCaller`), both taken
    // off the ONE record this verification produced: the name is what flows
    // downstream as the caller's identity (allows lookup, `node:<name>` origin
    // stamp), and the key is what the claim's `remoteParent.key` is
    // built from — never a re-read of the registry, which `message_send` loads
    // again for its own purposes. The claimed header name is attribution only;
    // when it disagrees, the drift is audited and the resolved name still wins.
    // Owned here, borrowed below: the outcome owns its two strings, and the
    // borrowed `SignedCaller` is what this door threads through `route` and
    // `stream_task`.
    let verified: Option<(String, String, Option<String>)> = match verify_signed_request(&req, now_epoch, started_epoch) {
        SignedRequestOutcome::Unsigned => None,
        SignedRequestOutcome::Verified { resolved, key, claimed, mesh } => {
            if let Some(detail) = attribution_drift_detail(&claimed, &resolved) {
                let _ = audit(
                    audit_log,
                    Door::A2a,
                    EventClass::Audit,
                    "a2a.signed-request",
                    "attribution-drift",
                    &listener.audit_detail_with(&detail, origin),
                );
            }
            Some((resolved, key, mesh))
        }
        SignedRequestOutcome::Refused(code, message) => {
            // **The two mail methods read `config-invalid`, not a lie about their
            // caller**. When this host's declaration set will not
            // load, what failed is RESOLUTION — the declarations that name people
            // are the ones this host cannot read — so `-32007 signature
            // verification failed` both blames a caller who may be perfectly
            // honest and hides the host's own broken state. The signature is
            // checked CRYPTOGRAPHICALLY here, against the key the node the request
            // itself names holds on the charter in force; only then is
            // `config-invalid` the answer, and the method is never dispatched with
            // an unresolved caller. Every other case keeps `-32007`.
            if code == -32007 {
                if let Some(method) = mail_method_of(&req.body) {
                    match mail_unloadable_declaration(&req, method, listener, audit_log, now_epoch, started_epoch) {
                        PreDispatch::Unloadable { detail } => {
                            let body_val = JsonRpcResponse::ok(
                                request_id(&req.body),
                                json!({ "status": "refused", "reason": CONFIG_INVALID, "detail": detail }),
                            );
                            let body = serde_json::to_vec(&body_val).unwrap_or_default();
                            // Never signed: the request was checked only against the raw
                            // in-force charter, which this node does not govern while its
                            // config will not load (`PreDispatch::Unloadable`).
                            return write_http_response(&mut writer, 200, &body);
                        }
                        PreDispatch::Error(err, message) => {
                            let body_val = jsonrpc_error_value(err, message);
                            let body = serde_json::to_vec(&body_val).unwrap_or_default();
                            // Never signed: the frame was refused before any answer is signed. A busy
                            // key answers 503 through `refusal_status`, still unsigned.
                            return write_http_response(&mut writer, refusal_status(err), &body);
                        }
                        PreDispatch::Keep => {}
                    }
                }
            }
            let body_val = jsonrpc_error_value(code, message.clone());
            let body = serde_json::to_vec(&body_val).unwrap_or_default();
            let _ = audit(
                audit_log,
                Door::A2a,
                EventClass::Audit,
                "a2a.signed-request",
                "unauthorized",
                &listener.audit_detail_with(&message, origin),
            );
            // Never signed: a refused request was not verified, and the door never signs
            // bytes an unverified sender chose.
            return write_http_response(&mut writer, refusal_status(code), &body);
        }
    };
    // The admission step, once, before either dispatch path: the frame's audience
    // and shape, then its nonce (`admit_verified`). A verified frame that either
    // refuses is answered unsigned, and records nothing.
    if let Some((resolved, key, _)) = verified.as_ref() {
        match admit_verified(listener, &req, &format!("node `{resolved}`"), key, now_epoch) {
            Ok(Audience::Admit) => {}
            Ok(Audience::Untagged { method }) => {
                let _ = audit(
                    audit_log,
                    Door::A2a,
                    EventClass::Audit,
                    "a2a.signed-request",
                    "untagged",
                    &listener.audit_detail_with(&format!("{method} sent without params.to"), origin),
                );
            }
            Err((code, message)) => {
                let _ = audit(
                    audit_log,
                    Door::A2a,
                    EventClass::Audit,
                    "a2a.signed-request",
                    "unauthorized",
                    &listener.audit_detail_with(&message, origin),
                );
                let body = serde_json::to_vec(&jsonrpc_error_value(code, message)).unwrap_or_default();
                return write_http_response(&mut writer, refusal_status(code), &body);
            }
        }
    }
    let signed_caller = verified
        .as_ref()
        .map(|(name, key, mesh)| SignedCaller { name: name.as_str(), key: key.as_str(), mesh: mesh.as_deref() });

    // Phase C: a `message/stream` / `tasks/resubscribe` POST takes over the
    // socket — headers-once + an SSE event loop in `stream_task` — instead of
    // the one-shot `route()`→`write_http_response` path below (which every
    // other request, incl. `tasks/get`/`message/send`, keeps unchanged). The
    // stream open is audited as `Door::A2a` at start; `stream_task` audits its
    // close. (The 10s read-timeout set above is harmless here: `stream_task`
    // only writes the socket, never reads it again.)
    if listener == Listener::A2a {
        if let Some(method) = streaming_method(&req) {
            let _ = audit(
                audit_log,
                Door::A2a,
                EventClass::Audit,
                &format!("a2a.{method}"),
                "open",
                "SSE stream open",
            );
            return stream_task(
                &mut writer,
                &req,
                &method,
                audit_log,
                spawn_agent,
                spawn_cwd,
                origin,
                &expected_token,
                req.bearer.as_deref(),
                signed_caller,
            );
        }
    }

    let (status, body, audit_cmd) = match listener {
        Listener::A2a => route(
            &req,
            bind,
            port,
            audit_log,
            spawn_agent,
            spawn_cwd,
            node_name,
            origin,
            &expected_token,
            registry,
            signed_caller,
        ),
        Listener::Mail => route_mail(&req, bind, port, audit_log, origin, registry, signed_caller),
    };

    // Security/audit (CONTRACTS.md §6): every handled request routes through
    // the single audit log, same discipline as the CLI/MCP doors. The
    // forwarded JSON-RPC body is DATA — it is never executed, only routed
    // through `handle_jsonrpc`'s method dispatch above and logged here.
    // "error" reflects the LOGICAL outcome (an HTTP-200 JSON-RPC error, e.g.
    // `tasks/get` on an unknown id, still audits as an error), not just the
    // HTTP status line — same as `dispatch::dispatch`'s audit, which keys off
    // `Outcome::status` rather than the exit code.
    let is_error = status >= 400
        || serde_json::from_slice::<Value>(&body)
            .ok()
            .is_some_and(|v| v.get("error").is_some());
    let status_word = if is_error { "error" } else { "ok" };
    let _ = audit(
        audit_log,
        Door::A2a,
        EventClass::Audit,
        &audit_cmd,
        status_word,
        &listener.audit_detail(status, origin),
    );

    // Signed only when the request's own signature verified (see `answer_headers`).
    let extra = if verified.is_some() { answer_headers(&req, status, &body, audit_log) } else { Vec::new() };
    write_http_response_with(&mut writer, status, &body, &extra)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;
    use serde_json::json;

    fn fake_handler(_inv: &Invocation) -> aoide_protocol::output::Outcome {
        aoide_protocol::output::Outcome::ok("fake", "fake")
    }

    /// A loopback `RequestCtx` — every pre-existing test below predates the
    /// non-loopback pending-gate amendment and exercises the historical
    /// "trusted, loopback caller" behavior, so this preserves that intent
    /// exactly (none of them touch the Inject-delivery branch anyway — they
    /// all resolve to `SendAction::Error`/`tasks/get`, which never consult
    /// `origin`).
    fn test_ctx<'a>(audit_log: &'a Path, spawn_agent: &'a str) -> RequestCtx<'a> {
        RequestCtx {
            audit_log,
            spawn_agent,
            spawn_cwd: "",
            origin: ConnOrigin::Loopback,
            node_name: "aoide",
            self_url: "http://127.0.0.1:8710/",
            expected_token: "",
            presented_token: None,
            signed_caller: None,
            sealed_only: false,
        }
    }

    /// The caller identity a `message_send` test threads when it only cares
    /// WHICH record resolved: those tests reach no stamp (no claim on their
    /// bodies), so the key is never read off it. The tests that DO check the
    /// stamped key take it from `verify_signed_request`'s own outcome — the
    /// only thing that can produce a real one.
    fn caller(name: &str) -> SignedCaller<'_> {
        // The stored key that VERIFIED — the fact a real `SignedCaller`
        // always carries. P-CHARTER reads the grant BY KEY
        // (`grant_in_mesh`), so a hand-built caller with an empty or stale key
        // holds nothing in every mesh and would silently change what the arm
        // under test does; resolving it here keeps every fixture honest about
        // the identity the door would have proved. `""` (no record, or a
        // record with no key) is exactly what an unpaired caller resolves to.
        let key = aoide_storage::node_store::load_nodes()
            .into_iter()
            .find(|p| p.name == name)
            .and_then(|p| p.pubkey)
            .unwrap_or_default();
        SignedCaller { name, key: Box::leak(key.into_boxed_str()), mesh: None }
    }

    /// A `Grant` fixture built straight from a capability list — the shape
    /// every gate test wants (`Grant`'s own field is private to the module,
    /// and `paired_grant` is the real lookup these tests exercise
    /// separately).
    fn grant_of(_mesh: &str, caps: &[&str]) -> Grant {
        Grant { caps: caps.iter().map(|c| c.to_string()).collect() }
    }

    /// The grant a fixture RECORD would produce in `mesh` — the same answer
    /// [`paired_grant`] gives for that record and mesh, without needing the
    /// registry.
    fn grant_for(node: &aoide_storage::node_store::Node, mesh: &str) -> Grant {
        Grant { caps: node.grant(mesh).iter().cloned().collect() }
    }

    /// A minimal `Node` fixture (P-P3) — unpaired/no-token by
    /// default, the same "mostly default, caller sets what it needs" shape
    /// `aoide_storage::node_store::tests::fixture_node` uses in its own
    /// crate; kept as a separate small copy here (this crate's tests build
    /// several ad hoc `Node { .. }` literals of their own already, and this
    /// one intentionally matches that local style rather than reaching for
    /// a cross-crate test helper that doesn't exist).
    fn fixture_node(name: &str, url: &str) -> aoide_storage::node_store::Node {
        aoide_storage::node_store::Node {
            name: name.to_string(),
            url: url.to_string(),
            token_file: None,
            bearer_secret: None,
            hub: false,
            pubkey: None,
            verified: false,
            grants: aoide_storage::node_store::Grants::new(),
            narrowed: aoide_storage::node_store::Grants::new(),
            via: None,
            added_at: "2026-08-25T00:00:00Z".to_string(),
        }
    }

    // (a) AgentCard generation from a small fake schema.
    #[test]
    fn agent_card_only_advertises_implemented_commands_that_open_to_a2a() {
        let mut r = Registry::new();
        r.insert(Command {
            path: &["foo", "bar"],
            summary: "does a thing",
            args: &[],
            flags: &[],
            gated: false,
            implemented: true,
            internal: false,
            exit_codes: (),
            examples: &[],
            handler: fake_handler,
            available: || true,
            doors: &[Door::Cli, Door::Daemon, Door::A2a],
            ..aoide_protocol::registry::Command::BLANK
        });
        r.insert(Command {
            path: &["foo", "closed"],
            summary: "implemented, but closed to a2a",
            args: &[],
            flags: &[],
            gated: false,
            implemented: true,
            internal: false,
            exit_codes: (),
            examples: &[],
            handler: fake_handler,
            available: || true,
            ..aoide_protocol::registry::Command::BLANK
        });
        r.insert(Command {
            path: &["foo", "stub"],
            summary: "not yet",
            args: &[],
            flags: &[],
            gated: false,
            implemented: false,
            internal: false,
            exit_codes: (),
            examples: &[],
            handler: fake_handler,
            available: || true,
            ..aoide_protocol::registry::Command::BLANK
        });

        let card = agent_card_from_commands(r.commands(), "127.0.0.1", 8710);
        assert_eq!(card["name"], "aoide");
        assert_eq!(card["version"], aoide_protocol::registry::AOIDE_VERSION);
        assert_eq!(card["url"], "http://127.0.0.1:8710/");
        assert_eq!(card["capabilities"]["streaming"], true);

        // The registry's open commands first (only `foo.bar`: `foo.closed` is
        // closed to A2A and `foo.stub` is not implemented), then the two the
        // card describes itself, since A2A opens no registry door.
        let skills = card["skills"].as_array().unwrap();
        let ids: Vec<&str> = skills.iter().map(|s| s["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["foo.bar", "send", "spawn"], "{ids:?}");
        assert_eq!(skills[0]["name"], "foo.bar");
        assert_eq!(skills[0]["description"], "does a thing");
        assert_eq!(skills[0]["tags"][0], "foo");

        // `agent_card` (the injected-registry wrapper) produces the same
        // shape as the pure `agent_card_from_commands` it wraps.
        let card2 = agent_card(&r, "127.0.0.1", 8710);
        assert_eq!(card2, card);
    }

    /// A2A runs no registry command and starts no spawn through one: its
    /// `message/send` routes straight to `session_send` and the harness spawn
    /// (CONTRACTS.md §6). This is a tripwire over the production half of this
    /// file: a call into the registry gate, or into the conduct spawn that a
    /// registry command would reach, means a path has started running commands
    /// the A2A door never declared.
    #[test]
    fn a2a_runs_no_registry_command() {
        let production = include_str!("a2a.rs").split("#[cfg(test)]").next().unwrap();
        for reach in [".invoke(", "dispatch(", "handler)(", "session_spawn", "launch_session"] {
            assert!(!production.contains(reach), "a2a.rs must not reach `{reach}`: A2A runs no registry command");
        }
    }

    // (b) canonical_state -> TaskState mapping, incl. needs_sudo precedence.
    #[test]
    fn task_state_mapping_matches_contracts_section_6() {
        assert_eq!(a2a_task_state("working", false), "working");
        assert_eq!(a2a_task_state("stopped", false), "completed");
        assert_eq!(a2a_task_state("awaiting", false), "input-required");
        // needsSudo takes precedence over the plain awaiting->input-required.
        assert_eq!(a2a_task_state("awaiting", true), "auth-required");
        assert_eq!(a2a_task_state("idle", false), "submitted");
        assert_eq!(a2a_task_state("done", false), "completed");
    }

    // (b2) task #33 — the dead-session override, pure: dead=true -> `failed`
    // for every canonical state; dead=false is byte-identical to the plain
    // fold `task_state_mapping_matches_contracts_section_6` already pins.
    #[test]
    fn task_state_checked_dead_overrides_every_canonical_state_to_failed() {
        for (canonical, needs_sudo) in [
            ("working", false),
            ("stopped", false),
            ("awaiting", false),
            ("awaiting", true),
            ("idle", false),
            ("done", false),
        ] {
            assert_eq!(a2a_task_state_checked(true, canonical, needs_sudo), "failed");
        }
    }

    #[test]
    fn task_state_checked_alive_matches_the_plain_fold_exactly() {
        for (canonical, needs_sudo) in [
            ("working", false),
            ("stopped", false),
            ("awaiting", false),
            ("awaiting", true),
            ("idle", false),
            ("done", false),
        ] {
            assert_eq!(
                a2a_task_state_checked(false, canonical, needs_sudo),
                a2a_task_state(canonical, needs_sudo),
            );
        }
    }

    fn fixture_session(id: &str, state: &str, needs_sudo: Option<bool>) -> SessionRecord {
        SessionRecord {
            session_id: id.to_string(),
            state: state.to_string(),
            needs_sudo,
            ..Default::default()
        }
    }

    // (c) JSON-RPC request parse + method routing.
    #[test]
    fn tasks_get_resolves_a_known_session_and_maps_its_state() {
        let sessions = vec![fixture_session("sess-1", "awaiting", Some(true))];
        let task = task_from_sessions(&sessions, "sess-1").unwrap();
        assert_eq!(task["id"], "sess-1");
        assert_eq!(task["contextId"], "sess-1");
        assert_eq!(task["status"]["state"], "auth-required");
        assert_eq!(task["kind"], "task");
        assert!(task["status"]["timestamp"].as_str().unwrap().ends_with('Z'));
    }

    #[test]
    fn tasks_get_unknown_id_is_a_structured_error() {
        let sessions = vec![fixture_session("sess-1", "working", None)];
        let err = task_from_sessions(&sessions, "nope").unwrap_err();
        assert_eq!(err.0, -32001);
        assert_eq!(err.1, "task not found");
    }

    // ── `decide_send_action` — every branch (pure, no I/O) ───────────────────

    #[test]
    fn decide_send_action_spawn_asked_wins_even_with_a_valid_contextid() {
        // spawn_asked == true short-circuits the contextId lookup entirely —
        // the closure below panics if it's ever consulted.
        let action = decide_send_action(Some("sess-1"), true, "claude", |_| {
            panic!("session_lookup must not be consulted when spawn is explicitly asked")
        });
        assert_eq!(action, SendAction::Spawn { agent_cmd: "claude".to_string() });
    }

    #[test]
    fn decide_send_action_no_context_id_spawns() {
        let action = decide_send_action(None, false, "claude", |_| {
            panic!("session_lookup must not be consulted with no contextId")
        });
        assert_eq!(action, SendAction::Spawn { agent_cmd: "claude".to_string() });
    }

    /// H1: a `spawnAgent` that IS a shell is refused at the ACT boundary, in
    /// `do_spawn`'s own prologue — the shape the illegal-slug refusal beside it
    /// already holds — so the refusal is audited by name and no process, no
    /// session and no first-turn keystroke ever happens. Refused: the exact shell,
    /// a shell reached through a launcher, `bash -lc <harness>` (a shell that runs
    /// the harness, so a line typed into it runs as a command), and `env -S
    /// <harness>` (a command line whose program the walk cannot name). Which
    /// harnesses DO pass is `spawn_agent_harness`'s own test; the spawn itself
    /// is never driven here (`do_spawn` launches the test binary).
    #[test]
    fn do_spawn_refuses_a_shell_spawn_agent_before_anything_starts() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-shell-spawn-agent-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(root.join("stage")).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", root.join("stage"));
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        let audit_log = root.join("log");

        for configured in ["bash", "bash -lc claude", "env bash", "dash", "nix develop -c bash", "env -S claude"] {
            let err = do_spawn(configured, "remote prompt", &audit_log, "nodeb", "", None, None)
                .expect_err("a shell spawnAgent must be refused, not run");
            assert_eq!(err.0, -32004, "`{configured}`: this door's own config code");
            assert!(err.1.contains("aoide.a2a.spawnAgent"), "the refusal names the option: {}", err.1);
            assert!(err.1.contains(configured), "and the value it saw: {}", err.1);
            let log = std::fs::read_to_string(&audit_log).unwrap_or_default();
            assert!(log.contains("shell-spawn-agent"), "the refusal is audited by name: {log}");
            assert!(log.contains(configured), "{log}");
        }
        // Nothing was spawned: no record, no log, no session file.
        let sessions: Vec<aoide_storage::records::SessionRecord> =
            load_stage::<SessionsFile>(&sessions_path())
                .map(|f| f.sessions)
                .unwrap_or_default();
        assert!(sessions.is_empty(), "no session may exist after a refused spawn: {sessions:?}");

        let _ = std::fs::remove_dir_all(&root);
        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    /// Default-deny, pure: a configured command is accepted only when the launcher
    /// walk proves it runs a registered harness, and it is accepted AS that
    /// harness (the name readiness and the submit key use). A shell is refused as
    /// a shell; anything the walk cannot prove is unproven.
    #[test]
    fn spawn_agent_harness_accepts_only_a_registered_harness_the_walk_proves() {
        let accepted: &[(&str, &str)] = &[
            ("claude", "claude"),
            ("claude --dangerously-skip-permissions", "claude"),
            ("/usr/bin/claude", "claude"),
            ("env claude", "claude"),
            ("nix develop -c claude", "claude"),
        ];
        for (configured, harness) in accepted {
            assert_eq!(spawn_agent_harness(configured), Ok(*harness), "`{configured}` runs {harness}");
        }
        let refused: &[(&str, SpawnAgentRefusal)] = &[
            ("bash", SpawnAgentRefusal::Shell),
            ("bash -lc claude", SpawnAgentRefusal::Shell),
            ("env bash", SpawnAgentRefusal::Shell),
            ("env -S claude", SpawnAgentRefusal::Shell),
            ("env --split-string=claude", SpawnAgentRefusal::Shell),
            ("taskset -c 0 claude", SpawnAgentRefusal::Unproven),
            ("/opt/x/my-wrapper", SpawnAgentRefusal::Unproven),
            ("no-such-agent", SpawnAgentRefusal::Unproven),
        ];
        for (configured, why) in refused {
            assert_eq!(spawn_agent_harness(configured), Err(*why), "`{configured}` is refused");
        }
    }

    /// The default-deny refusal in `do_spawn`'s prologue, for a configured command
    /// the walk cannot prove: a launcher it does not model, a wrapper script, a
    /// name with no profile. Audited as `unverified-spawn-agent`, and no process,
    /// session or first-turn keystroke ever happens.
    #[test]
    fn do_spawn_refuses_an_unproven_spawn_agent_before_anything_starts() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-unverified-spawn-agent-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(root.join("stage")).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", root.join("stage"));
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        let audit_log = root.join("log");

        for configured in ["taskset -c 0 claude", "/opt/x/my-wrapper", "no-such-agent"] {
            let err = do_spawn(configured, "remote prompt", &audit_log, "nodeb", "", None, None)
                .expect_err("an unproven spawnAgent must be refused, not run");
            assert_eq!(err.0, -32004, "`{configured}`: this door's own config code");
            assert!(err.1.contains("aoide.a2a.spawnAgent"), "the refusal names the option: {}", err.1);
            assert!(err.1.contains(configured), "and the value it saw: {}", err.1);
            let log = std::fs::read_to_string(&audit_log).unwrap_or_default();
            assert!(log.contains("unverified-spawn-agent"), "the refusal is audited by name: {log}");
        }
        let sessions: Vec<aoide_storage::records::SessionRecord> =
            load_stage::<SessionsFile>(&sessions_path())
                .map(|f| f.sessions)
                .unwrap_or_default();
        assert!(sessions.is_empty(), "no session may exist after a refused spawn: {sessions:?}");

        let _ = std::fs::remove_dir_all(&root);
        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    #[test]
    fn decide_send_action_spawn_disabled_is_a_structured_error() {
        // No contextId AND an empty spawn_agent (the default,
        // `aoide.a2a.spawnAgent = ""`) → a structured error, not a silent
        // no-op and not a fallback to something else.
        let action = decide_send_action(None, false, "", |_| {
            panic!("session_lookup must not be consulted with no contextId")
        });
        assert_eq!(
            action,
            SendAction::Error { code: -32004, msg: "A2A spawn not configured".to_string() }
        );
        // Same error when spawn IS explicitly asked but nothing is configured.
        let action2 = decide_send_action(Some("sess-1"), true, "", |_| None);
        assert_eq!(
            action2,
            SendAction::Error { code: -32004, msg: "A2A spawn not configured".to_string() }
        );
    }

    #[test]
    fn decide_send_action_known_conductable_session_injects() {
        let action = decide_send_action(Some("sess-1"), false, "claude", |id| {
            assert_eq!(id, "sess-1");
            Some(SessionRef { conductable: true, has_socket: true })
        });
        assert_eq!(action, SendAction::Inject { session_id: "sess-1".to_string() });
    }

    #[test]
    fn decide_send_action_known_but_not_conductable_is_an_error() {
        // Registered but not conductable (no control socket) — same shape as
        // `send`'s own `not-conductable` rejection.
        let action = decide_send_action(Some("plain"), false, "claude", |_| {
            Some(SessionRef { conductable: false, has_socket: false })
        });
        assert_eq!(
            action,
            SendAction::Error { code: -32004, msg: "session not conductable".to_string() }
        );
        // Conductable but socket-less (a bind failure at conduct time) is the
        // same rejection — `has_socket` gates it too.
        let action2 = decide_send_action(Some("nosock"), false, "claude", |_| {
            Some(SessionRef { conductable: true, has_socket: false })
        });
        assert_eq!(
            action2,
            SendAction::Error { code: -32004, msg: "session not conductable".to_string() }
        );
    }

    #[test]
    fn decide_send_action_unknown_context_id_is_task_not_found() {
        let action = decide_send_action(Some("ghost"), false, "claude", |_| None);
        assert_eq!(
            action,
            SendAction::Error { code: -32001, msg: "task not found".to_string() }
        );
    }

    // ── `session_ref_lookup` — `has_socket` is disk-derived (the identical
    // bug shape `e2758f7` fixed on the `graph` door,
    // `conduct::graph::doc::is_conductable_now`) ─────────────────────────────

    #[test]
    fn session_ref_lookup_has_socket_tracks_the_file_on_disk_without_touching_the_stored_record() {
        // `shellbridge.service` owns `$XDG_RUNTIME_DIR/aoide` with
        // `RuntimeDirectoryPreserve=no`, so a rebuild deletes a live
        // session's socket file without ever touching `sessions.json` — a
        // stored socket STRING can long outlive the file it names.
        // `has_socket` must track the file, not just non-emptiness.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = std::env::temp_dir().join(format!(
            "aoide-server-a2a-sessionref-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        let socket = stage.join("session-a.sock");
        std::fs::write(&socket, b"").unwrap();

        let sf = SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![conductable_session("a", &socket)],
        };
        write_stage(&sessions_path(), &sf).unwrap();

        let sref = session_ref_lookup("a").unwrap();
        assert!(sref.conductable);
        assert!(sref.has_socket, "an existing socket file reports has_socket");

        // The socket vanishes (a rebuild tearing down the runtime dir) — the
        // ORIGINAL record is never touched; only the REPORTED value changes.
        std::fs::remove_file(&socket).unwrap();
        let sref2 = session_ref_lookup("a").unwrap();
        assert!(sref2.conductable, "conductable is untouched by the missing socket");
        assert!(!sref2.has_socket, "a removed socket file reports NOT has_socket");

        let after: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec = after.sessions.iter().find(|s| s.session_id == "a").unwrap();
        assert_eq!(rec.conductable, Some(true), "the stored flag is never modified by a read");
        assert_eq!(
            rec.socket,
            Some(socket.to_string_lossy().into_owned()),
            "the stored socket path is never cleared or migrated by a read"
        );

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    // ── `message/send` param parsing (pure, no I/O) ──────────────────────────

    #[test]
    fn extract_prompt_text_concatenates_text_parts_and_ignores_others() {
        let message = json!({
            "parts": [
                { "kind": "text", "text": "hello" },
                { "kind": "file", "uri": "ignored://non-text-part" },
                { "kind": "text", "text": "world" },
            ]
        });
        assert_eq!(extract_prompt_text(&message), "hello\nworld");
        // No parts at all → empty prompt, not a panic.
        assert_eq!(extract_prompt_text(&json!({})), "");
    }

    #[test]
    fn extract_context_id_prefers_message_then_falls_back_to_params() {
        // message.contextId wins over params.contextId when both are present.
        let message = json!({ "contextId": "from-message" });
        let params = json!({ "contextId": "from-params" });
        assert_eq!(extract_context_id(&message, &params), Some("from-message".to_string()));
        // Falls back to params.contextId when the message carries none.
        assert_eq!(
            extract_context_id(&json!({}), &params),
            Some("from-params".to_string())
        );
        // Neither present, or an empty string, is treated as absent.
        assert_eq!(extract_context_id(&json!({}), &json!({})), None);
        assert_eq!(
            extract_context_id(&json!({ "contextId": "" }), &json!({})),
            None
        );
    }

    #[test]
    fn spawn_requested_reads_the_aoide_spawn_metadata_key() {
        // The documented key, on the message object.
        let message = json!({ "metadata": { "aoide/spawn": true } });
        assert!(spawn_requested(&message, &json!({})));
        // Or on the top-level params object.
        let params = json!({ "metadata": { "aoide/spawn": true } });
        assert!(spawn_requested(&json!({}), &params));
        // Absent, false, or a non-boolean value → not requested.
        assert!(!spawn_requested(&json!({}), &json!({})));
        assert!(!spawn_requested(
            &json!({ "metadata": { "aoide/spawn": false } }),
            &json!({})
        ));
        assert!(!spawn_requested(
            &json!({ "metadata": { "aoide/spawn": "true" } }),
            &json!({})
        ));
    }

    #[test]
    fn parse_message_send_params_extracts_all_five_fields_together() {
        let params = json!({
            "message": {
                "role": "user",
                "parts": [{ "kind": "text", "text": "do the thing" }],
                "contextId": "sess-9",
                "metadata": {
                    "aoide/spawn": true,
                    "aoide/from": "conduct-1-2",
                    "aoide/task": "fix-flaky"
                },
            }
        });
        let (prompt, context_id, spawn_asked, claimed_from, task) =
            parse_message_send_params(&params);
        assert_eq!(prompt, "do the thing");
        assert_eq!(context_id.as_deref(), Some("sess-9"));
        assert!(spawn_asked);
        assert_eq!(claimed_from.as_deref(), Some("conduct-1-2"));
        assert_eq!(task.as_deref(), Some("fix-flaky"));

        // A minimal params with no `message` at all is tolerated, not a panic.
        let (prompt2, context_id2, spawn_asked2, claimed_from2, task2) =
            parse_message_send_params(&json!({}));
        assert_eq!(prompt2, "");
        assert_eq!(context_id2, None);
        assert!(!spawn_asked2);
        assert_eq!(claimed_from2, None);
        assert_eq!(task2, None);
    }

    /// `aoide/task` takes the SAME two-spot reading `aoide/spawn` does (a
    /// directive about what to do, unlike `aoide/from`'s identity claim), and a
    /// blank or non-string value names no task at all — the non-third state
    /// `spawn --task`'s own empty-flag read takes.
    #[test]
    fn requested_task_reads_the_aoide_task_key_where_spawn_is_read() {
        assert_eq!(
            requested_task(
                &json!({ "metadata": { "aoide/task": "fix-flaky" } }),
                &json!({}),
            )
            .as_deref(),
            Some("fix-flaky")
        );
        assert_eq!(
            requested_task(&json!({}), &json!({ "metadata": { "aoide/task": "from-params" } }))
                .as_deref(),
            Some("from-params")
        );
        // The message object wins when both carry one, the same precedence
        // `extract_context_id` gives.
        assert_eq!(
            requested_task(
                &json!({ "metadata": { "aoide/task": "from-message" } }),
                &json!({ "metadata": { "aoide/task": "from-params" } }),
            )
            .as_deref(),
            Some("from-message")
        );
        assert_eq!(
            requested_task(&json!({ "metadata": { "aoide/task": "  fix-flaky  " } }), &json!({}))
                .as_deref(),
            Some("fix-flaky"),
            "the same trim `spawn --task` applies to its flag value"
        );
        for empty_or_not_a_string in [json!(""), json!("   "), json!(null), json!(7)] {
            let message = json!({ "metadata": { "aoide/task": empty_or_not_a_string } });
            assert_eq!(requested_task(&message, &json!({})), None, "{empty_or_not_a_string}");
        }
    }

    /// The slug is held to the ONE predicate every mailbox name takes, and the
    /// refusal is taught: it quotes the value, states the shape and names the
    /// way out (`-32602`, applied by `do_spawn` — the same spawn-side-only
    /// discipline S3's malformed `aoide/from` claim holds). The quoted value is
    /// the CLEANED one (L6 of the S10 review): a slug may be illegal precisely
    /// because it carries a newline, an escape or a bidi override, and this
    /// string is printed by a caller's UI.
    #[test]
    fn a_task_slug_off_the_shape_is_refused_with_a_taught_message() {
        assert_eq!(spawn_task_slug(None), Ok(None), "naming no task is not a malformed one");
        assert_eq!(spawn_task_slug(Some("fix-flaky")), Ok(Some("fix-flaky")));
        assert_eq!(spawn_task_slug(Some("a1-b2")), Ok(Some("a1-b2")));

        for bad in ["Upper", "-leading", "under_score", "a/b", "../evil", "a b"] {
            let refusal = spawn_task_slug(Some(bad))
                .expect_err(&format!("`{bad}` must not be accepted as a task slug"));
            assert!(refusal.contains(bad), "quotes the offending value: {refusal}");
            assert!(
                refusal.contains("^[a-z0-9][a-z0-9-]*$"),
                "states the predicate, so the caller can fix it: {refusal}"
            );
            assert!(
                refusal.contains(aoide_protocol::wire::TASK_KEY),
                "names the key it read: {refusal}"
            );
        }

        // The hostile shapes: what reaches the error body is the SANITIZED
        // value, and the raw bytes never do. The expectation is the shared
        // sanitizer's OWN output (never a hand-guessed literal: `strip_unsafe`
        // drops a whole ESC sequence, not just the `\u{1b}`).
        for hostile in ["evil\nslug", "evil\u{1b}[31mslug", "evil\u{202e}slug", "evil\u{200b}slug"] {
            let refusal = spawn_task_slug(Some(hostile))
                .expect_err(&format!("`{hostile}` must not be accepted as a task slug"));
            assert!(
                !refusal.contains('\n') && !refusal.contains('\u{1b}') && !refusal.contains('\u{202e}'),
                "no raw control/format byte survives into the refusal: {refusal:?}"
            );
            assert!(
                refusal.contains(&aoide_conduct::graph::clean_line(hostile)),
                "the cleaned value is what is taught: {refusal:?}"
            );
            assert!(refusal.contains("evil"), "and the useful part is still legible: {refusal:?}");
        }
        // And the echo is bounded: a long ILLEGAL value comes back as one
        // clipped line (the sanitizer's own 200-character cap), never whole.
        let refusal = spawn_task_slug(Some(&"X".repeat(4096))).unwrap_err();
        assert!(
            refusal.chars().count() < 1024,
            "the echo is capped, not echoed whole: {} chars",
            refusal.chars().count()
        );
    }

    /// The claim is read off `message.metadata` ONLY (CONTRACTS.md §6): a
    /// top-level `params.metadata` copy — which `aoide/spawn` does accept —
    /// is not a second spelling for WHO is calling, and an empty or
    /// non-string value is not a third state.
    #[test]
    fn the_from_claim_is_read_only_off_message_metadata() {
        let top_level_only = json!({ "metadata": { "aoide/from": "conduct-1-2" } });
        assert_eq!(parse_message_send_params(&top_level_only).3, None);

        let on_message = json!({ "message": { "metadata": { "aoide/from": "conduct-1-2" } } });
        assert_eq!(parse_message_send_params(&on_message).3.as_deref(), Some("conduct-1-2"));

        // Both present: the message's own value wins, the params copy is
        // never read.
        let both = json!({
            "message": { "metadata": { "aoide/from": "from-message" } },
            "metadata": { "aoide/from": "from-params" },
        });
        assert_eq!(parse_message_send_params(&both).3.as_deref(), Some("from-message"));

        for empty_or_not_a_string in [json!(""), json!(7), json!(null), json!(true)] {
            let params = json!({ "message": { "metadata": { "aoide/from": empty_or_not_a_string } } });
            assert_eq!(parse_message_send_params(&params).3, None, "no third state: {empty_or_not_a_string}");
        }
    }

    /// P-RSA S3: the claim is honoured on the SIGNATURE rung ONLY.
    /// [`claimable_caller`] is the entire rung table — it hands a caller over
    /// for that rung and for no other — and [`claimed_remote_parent`] is
    /// provable on top of it on fixtures alone: no disk, no wire, no spawn.
    #[test]
    fn claimed_remote_parent_is_honoured_on_the_signature_rung_only() {
        let node = fixture_node("yomi-strix", "http://10.0.0.9:8710/");
        let key = "ab".repeat(32);
        let signed = SignedCaller { name: "yomi-strix", key: &key, mesh: None };

        for rung in [
            aoide_storage::node_store::NodeRung::Token,
            aoide_storage::node_store::NodeRung::Addr,
        ] {
            assert_eq!(
                claimable_caller(Some((&node, rung)), Some(signed)),
                None,
                "a weaker rung hands over nothing — it can never reach a stamp"
            );
        }
        assert_eq!(
            claimable_caller(None, Some(signed)),
            None,
            "no resolution at all: nothing to attribute the claim to"
        );
        assert_eq!(
            claimable_caller(Some((&node, aoide_storage::node_store::NodeRung::Signature)), None),
            None,
            "the rung alone is nothing: the identity comes off the verified signature or not at all"
        );

        // The one honoured shape: the rung that verified, the name and key it
        // verified, the claim for the session id, nothing else. The node
        // fixture carries no key of its own — the stamp takes its key from the
        // caller, which is the whole of LOW-1.
        assert_eq!(
            claimable_caller(Some((&node, aoide_storage::node_store::NodeRung::Signature)), Some(signed)),
            Some(signed)
        );
        assert_eq!(
            claimed_remote_parent(Some(signed), Some("conduct-1-2")),
            Ok(Some(RemoteParent {
                node: "yomi-strix".to_string(),
                key: key.clone(),
                session_id: "conduct-1-2".to_string(),
                extra: Default::default(),
            }))
        );
        assert_eq!(
            claimed_remote_parent(Some(signed), None),
            Ok(None),
            "no claim on the wire: nothing stamped, the whole pre-S3 shape"
        );

        // A malformed claim from a signed caller is an error for the CALLER to
        // apply, never a silent drop (CONTRACTS.md §6) — and the caller applies
        // it on the spawn side only, so this table stops at the refusal value.
        for bad in ["", "a/b", "spaces not allowed", "😀"] {
            let err = claimed_remote_parent(Some(signed), Some(bad)).unwrap_err();
            assert_eq!(err.0, -32602, "malformed claim `{bad}`");
            assert!(err.1.contains("aoide/from"), "the refusal names the key: {}", err.1);
        }

        // ...but a weaker rung never gets that far: it has no caller to
        // validate the claim against, so the same bad claim is silence there,
        // not -32602.
        assert_eq!(
            claimed_remote_parent(
                claimable_caller(Some((&node, aoide_storage::node_store::NodeRung::Token)), Some(signed)),
                Some("a/b")
            ),
            Ok(None),
            "an unsigned caller's bad claim is silence, not -32602 — there is no claim to validate"
        );
    }

    /// P-RSA S3 (MED-2): pid + second alone collides for two spawns inside one
    /// second (`unix_ts_now` is whole seconds), and two children sharing one id
    /// share one `sessions.json` record — so the LAST `stamp_spawn_provenance`
    /// decides whose run it is, and one caller's ledger holds one row for two
    /// children. Two ids in a tight loop IS that collision, made deterministic.
    #[test]
    fn spawn_ids_never_collide_within_a_second_and_stay_legal_session_ids() {
        let a = spawn_session_id();
        let b = spawn_session_id();
        assert_ne!(a, b, "two spawns inside one second must not mint one id");
        for id in [&a, &b] {
            assert!(id.starts_with(&format!("a2a-{}-", std::process::id())), "{id}");
            assert_eq!(id.split('-').count(), 4, "a2a-<pid>-<secs>-<n>: {id}");
            assert!(
                aoide_storage::remote_children::valid_claimed_session_id(id),
                "a spawned child's id is what a later claim addresses it by: {id}"
            );
        }
    }

    /// P-RSA S3: the value stamped is the RESOLVED record's, key-authenticated
    /// by `verify_signed_request` — never the `X-Aoide-Node` label the request
    /// carried. Same real-crypto round trip as the P-P4/P-P5b tests, driven
    /// with a header naming a node that does not exist while the signature
    /// belongs to `yomi-strix`'s key.
    #[test]
    fn the_remote_parent_is_built_from_the_resolved_node_not_the_header_name() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-remote-parent-resolved-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::env::set_var("AOIDE_STATE_DIR", &root);

        let kp = setup_signed_node_with_allows("yomi-strix", &["read", "spawn"]);
        let body =
            aoide_client::wire::build_message_send_body("status check please", "mid-rp-1", None, Some("conduct-1-2"), None);
        let body_bytes = serde_json::to_vec(&body).unwrap();
        let now = 1_800_000_000_i64;
        // The header names a node this box has never paired with; the
        // signature is the one registered key's.
        let req = signed_request(&kp, "sakaki-impostor", "/", &body_bytes, now, &unique_nonce("rp-resolved"));

        let (signed_caller, signed_key) = match verify_signed_request(&req, now, 0) {
            SignedRequestOutcome::Verified { resolved, key, .. } => (resolved, key),
            other => panic!("expected a REAL genuine signature to verify, got {other:?}"),
        };
        assert_eq!(
            signed_caller, "yomi-strix",
            "the KEY selects the record; the header label is a tie-break among records sharing a key, never a name"
        );
        assert_eq!(
            signed_key,
            kp.info().pubkey_hex,
            "the outcome carries the key that verified, not just the name"
        );

        // `message_send`'s own resolution + claim, verbatim: find by the
        // verified identity's NAME (current registry, for allows),
        // gate on the rung, stamp from the verified identity.
        let nodes = aoide_storage::node_store::load_nodes();
        let resolved = nodes
            .iter()
            .find(|p| p.name == signed_caller)
            .map(|p| (p, aoide_storage::node_store::NodeRung::Signature));
        let caller = claimable_caller(
            resolved,
            Some(SignedCaller { name: &signed_caller, key: &signed_key, mesh: None }),
        );
        let stamped = claimed_remote_parent(caller, parse_message_send_params(&body["params"]).3.as_deref())
            .unwrap()
            .expect("a verified signature plus a valid claim stamps a parent");
        assert_eq!(stamped.node, "yomi-strix", "never the header's `sakaki-impostor`");
        assert_eq!(stamped.key, kp.info().pubkey_hex, "the verifying key, not a wire string");
        assert_eq!(stamped.session_id, "conduct-1-2");

        let _ = std::fs::remove_dir_all(&root);
        match saved {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    /// P-RSA S10: `aoide node spawn --task <slug>` and the server's own
    /// `metadata["aoide/task"]` reader are ONE wire key, proven end to end
    /// through the real body builder — the same discipline
    /// `the_clients_from_claim_round_trips_through_the_inbound_parser` holds
    /// for `aoide/from`. `None` stays byte-identical to the pre-S10 body.
    #[test]
    fn the_clients_task_slug_round_trips_through_the_inbound_parser() {
        let body = aoide_client::wire::build_message_send_body(
            "do the thing",
            "mid-task-1",
            None,
            None,
            Some("fix-flaky"),
        );
        let (_, _, _, _, task) = parse_message_send_params(&body["params"]);
        assert_eq!(task.as_deref(), Some("fix-flaky"), "{body}");

        let plain = aoide_client::wire::build_message_send_body("hi", "mid-task-2", None, None, None);
        assert_eq!(parse_message_send_params(&plain["params"]).4, None);
    }

    /// The outbound builder (`aoide-client`) round-tripped through the
    /// inbound parser here — a dev-dependency-only edge (see `Cargo.toml`):
    /// production code never lets `aoide-server` reach `aoide-client`.
    #[test]
    fn build_message_send_body_round_trips_through_the_inbound_parser() {
        let body = aoide_client::wire::build_message_send_body("hello there", "mid-123", None, None, None);
        let (prompt, ctx, spawn, claimed_from, task) = parse_message_send_params(&body["params"]);
        assert_eq!(prompt, "hello there");
        assert_eq!(ctx, None);
        assert!(!spawn);
        assert_eq!(claimed_from, None, "no `--parent` claim on this body");
        assert_eq!(task, None, "no `aoide/task` on a body the client builds today");
    }

    /// P-RSA S3: the claim the CLIENT signs (`aoide/from`) is the one the
    /// server's parser reads, through the same real body builder —
    /// `aoide_client::wire::build_message_send_body`'s fourth parameter and
    /// `parse_message_send_params`' fourth field are one wire key, proven end
    /// to end rather than assumed.
    #[test]
    fn the_clients_from_claim_round_trips_through_the_inbound_parser() {
        let body = aoide_client::wire::build_message_send_body("hello there", "mid-789", None, Some("conduct-1-2"), None);
        let (_, _, _, claimed_from, _) = parse_message_send_params(&body["params"]);
        assert_eq!(claimed_from.as_deref(), Some("conduct-1-2"));
    }

    /// P-P5b (`node spawn`): the exact body `handle_node_spawn`
    /// (`aoide-client::commands`) posts is
    /// `aoide_client::wire::build_message_send_body(text, id, None, None, None)` — this
    /// proves that shape routes all the way to `SendAction::Spawn`, carrying
    /// the client's prompt text verbatim as the argument `do_spawn` would
    /// type as the newly spawned session's first turn, against the SERVER's
    /// own `parse_message_send_params`/`decide_send_action`, not a guessed
    /// shape.
    #[test]
    fn build_message_send_body_routes_to_the_spawn_arm_exactly_as_do_spawn_expects() {
        let body = aoide_client::wire::build_message_send_body("status check please", "mid-456", None, None, None);
        let (prompt, ctx, spawn_asked, _, _) = parse_message_send_params(&body["params"]);
        assert_eq!(prompt, "status check please");
        assert_eq!(ctx, None, "no contextId — the Spawn signal `decide_send_action` reads");
        assert!(!spawn_asked, "spawn is signaled by the ABSENT contextId, not the metadata flag — `node spawn` never sets it");

        let action = decide_send_action(ctx.as_deref(), spawn_asked, "claude", session_ref_lookup);
        assert_eq!(
            action,
            SendAction::Spawn { agent_cmd: "claude".to_string() },
            "routes to Spawn with `do_spawn`'s prompt arg equal to `prompt` above (\"status check please\")"
        );
    }

    /// **A refused far spawn creates no box.** A spawn the charter allows makes
    /// its child's box as a local act (MAIL.md, "Store"); a spawn refused before
    /// anything starts names no box and writes no box file for its task slug.
    #[test]
    fn a_refused_far_spawn_creates_no_box_file() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("spawn-refused-box");
        act_as(&root, "here");

        let audit_log = root.join("log");
        let refused = do_spawn("bash -lc claude", "hi", &audit_log, "peer", "", None, Some("far-task"));
        assert!(refused.is_err(), "a shell spawn agent is refused before anything starts");
        assert!(!aoide_storage::mail::box_exists("far-task"), "no box for the refused spawn's slug");
        assert!(!aoide_storage::mail::box_path("far-task").exists(), "and no box file");

        mail_deposit_cleanup(&root, saved_state, saved_stage);
    }

    /// P-RSA S10 review, L10 + M2's door half: the two refusals that must fire
    /// BEFORE an id is minted, before an argv exists and before `current_exe()`
    /// is consulted — driven through `do_spawn` ITSELF, which is safe here for
    /// exactly that reason: both return early, so this never reaches the real
    /// process spawn the rest of this suite deliberately stops short of. That
    /// early-return is the invariant; if either check ever moves below
    /// `cmd.spawn()`, this test will try to spawn the test binary and fail
    /// loudly rather than quietly.
    #[test]
    fn do_spawn_refuses_an_illegal_and_a_live_held_slug_through_its_own_boundary() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = std::env::temp_dir().join(format!(
            "aoide-a2a-dospawn-refusal-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        let audit_log = stage.join("audit.log");

        // (1) An illegal slug: `-32602`, taught, audited — and NOTHING spawned
        // (no record exists, so no child ever registered).
        let (code, msg) = do_spawn("claude", "hi", &audit_log, "peer", "", None, Some("Upper"))
            .expect_err("an illegal slug must be refused");
        assert_eq!(code, -32602);
        assert!(msg.contains("Upper") && msg.contains("^[a-z0-9][a-z0-9-]*$"), "{msg}");
        assert!(!sessions_path().exists(), "a refused spawn writes no record");
        let audited = std::fs::read_to_string(&audit_log).unwrap_or_default();
        assert!(audited.contains("not a legal task slug"), "audited: {audited}");

        // (2) A slug a live run already holds — the shape a peer's door child
        // leaves behind — refused with `aoide-conduct`'s OWN sentence.
        let mut held = fixture_session("a2a-held", "working", None);
        held.task = Some("build-reports".to_string());
        held.origin = Some("node:peer".to_string());
        let sf = SessionsFile { schema_version: "0".to_string(), sessions: vec![held] };
        write_stage(&sessions_path(), &sf).unwrap();
        let (held_by, started_at) = aoide_conduct::graph::live_run_for("build-reports")
            .expect("the held run is visible to the door");

        let (code, msg) =
            do_spawn("claude", "hi", &audit_log, "peer", "", None, Some("build-reports"))
                .expect_err("a live-held slug must be refused");
        assert_eq!(code, -32602, "state this node holds, not a capability the caller lacks");
        assert_eq!(
            msg,
            aoide_conduct::graph::live_run_refusal("build-reports", &held_by, &started_at),
            "the shared refusal text, byte for byte"
        );
        let after: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert_eq!(after.sessions.len(), 1, "nothing was spawned, nothing was written");
        assert!(std::fs::read_to_string(&audit_log).unwrap().contains("already has a live run"));

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    // ── P-RSA S10: a remote spawn is a headless managed run ──────────────
    //
    // `spawn_argv` is `do_spawn`'s own argv half, factored out for exactly the
    // reason `spawn_child_command` was: a `cargo test` binary's `current_exe()`
    // is the harness, so no test here drives the real spawn — the SHAPE is what
    // is provable, off a real `Command`'s `get_args()`/`get_envs()`.

    /// S10: the door's argv IS the wrapper a local `spawn` builds —
    /// `--spawned --headless` always (this door has no terminal to hand a
    /// child), `--task <slug>` exactly when the caller named one — and an
    /// ILLEGAL slug never reaches an argv at all, because `do_spawn` refuses it
    /// before the id is minted or `current_exe()` is resolved (the refusal's
    /// own test is `a_task_slug_off_the_shape_is_refused_with_a_taught_message`).
    #[test]
    fn a2a_spawn_argv_is_a_headless_managed_run_with_the_task_only_when_named() {
        let audit = Path::new("/tmp/aoide-a2a-test-audit-does-not-need-to-exist.log");
        let args_of = |argv: &[String]| -> Vec<String> {
            spawn_child_command(Path::new("/bin/true"), argv, audit, None)
                .get_args()
                .map(|a| a.to_string_lossy().into_owned())
                .collect()
        };

        assert_eq!(
            args_of(&spawn_argv("a2a-11-1790", "claude", "claude --dangerously-skip-permissions", None)),
            vec![
                "conduct",
                "--spawned",
                "--headless",
                "--agent",
                "claude",
                "--id=a2a-11-1790",
                "--",
                "claude",
                "--dangerously-skip-permissions",
            ],
            "a no-task spawn is the plain headless wrapper, the configured agent's own argv \
             untouched (still `split_whitespace`, never a shell)"
        );
        assert_eq!(
            args_of(&spawn_argv("a2a-11-1791", "claude", "claude", Some("fix-flaky"))),
            vec![
                "conduct",
                "--spawned",
                "--headless",
                "--agent",
                "claude",
                "--id=a2a-11-1791",
                "--task",
                "fix-flaky",
                "--",
                "claude",
            ],
            "a named task rides the SAME argv a local `spawn --task` builds, before the `--` \
             that ends the wrapper's own flags"
        );

        assert!(spawn_task_slug(Some("Upper")).is_err(), "and the illegal slug has no argv to read");
    }

    /// S10's no-regression pin: `--headless` and `--task` are ARGV facts, not
    /// new environment. The child's `Command` carries exactly the three entries
    /// the pre-S10 spawn carried — `AOIDE_AUDIT_LOG` set, `AOIDE_SESSION_ORIGIN`
    /// and `AOIDE_SESSION_ID` explicitly REMOVED — with a task named or not.
    /// Nothing else appears: `AOIDE_TASK`/`AOIDE_TASK_INSTRUCTIONS` are exported
    /// by the child's own `conduct` to the AGENT it wraps (`spawn_on_pty`),
    /// never by this door to the wrapper.
    #[test]
    fn a2a_spawn_env_is_unchanged_by_headless_and_task() {
        let audit = Path::new("/tmp/aoide-a2a-test-audit-does-not-need-to-exist.log");
        let env_of = |argv: &[String]| -> std::collections::BTreeMap<String, Option<String>> {
            spawn_child_command(Path::new("/bin/true"), argv, audit, None)
                .get_envs()
                .map(|(k, v)| {
                    (
                        k.to_string_lossy().into_owned(),
                        v.map(|v| v.to_string_lossy().into_owned()),
                    )
                })
                .collect()
        };
        let expected: std::collections::BTreeMap<String, Option<String>> = [
            (
                "AOIDE_AUDIT_LOG".to_string(),
                Some(audit.to_string_lossy().into_owned()),
            ),
            ("AOIDE_SESSION_ORIGIN".to_string(), None),
            ("AOIDE_SESSION_ID".to_string(), None),
        ]
        .into_iter()
        .collect();

        assert_eq!(env_of(&spawn_argv("a2a-11-1", "claude", "claude", None)), expected);
        assert_eq!(
            env_of(&spawn_argv("a2a-11-2", "claude", "claude", Some("fix-flaky"))),
            expected,
            "S10 adds an argv flag and no environment at all"
        );
    }

    /// S10 on the S3 side: a MANAGED remote child — a record that already
    /// carries the task slug its own `conduct` stamped at registration — takes
    /// the door's `origin`+`remoteParent` stamp exactly as a plain one does, and
    /// keeps the slug. Two writers, two stage-lock sections, so the second
    /// never clobbers the first's field.
    #[test]
    fn the_stamp_lands_on_a_managed_run_and_leaves_its_task_slug_alone() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = std::env::temp_dir().join(format!(
            "aoide-server-a2a-stamp-task-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        let mut rec = fixture_session("a2a-spawned-task", "working", None);
        rec.task = Some("fix-flaky".to_string());
        let sf = SessionsFile { schema_version: "0".to_string(), sessions: vec![rec] };
        write_stage(&sessions_path(), &sf).unwrap();

        stamp_spawn_provenance(
            "a2a-spawned-task",
            "node:yomi-strix",
            Some(RemoteParent {
                node: "yomi-strix".to_string(),
                key: "cd".repeat(32),
                session_id: "conduct-17991-1790312541".to_string(),
                extra: Default::default(),
            }),
        );

        let after: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec = after.sessions.iter().find(|s| s.session_id == "a2a-spawned-task").unwrap();
        assert_eq!(rec.origin.as_deref(), Some("node:yomi-strix"), "the origin still lands");
        let rp = rec.remote_parent.as_ref().expect("the remoteParent stamp still lands");
        assert_eq!(rp.node, "yomi-strix");
        assert_eq!(rp.session_id, "conduct-17991-1790312541");
        assert_eq!(
            rec.task.as_deref(),
            Some("fix-flaky"),
            "the child's own task slug survives both stamps — the managed shape changes nothing \
             about S3's two register-wait stamps"
        );
        assert_eq!(rec.parent_session_id, None, "still no foreign id in the LOCAL parent edge");

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    // ── `message/send` end-to-end via `handle_jsonrpc` — ERROR branches only.
    // Every one of these resolves to `SendAction::Error` before touching a
    // socket or a process, so none of them spawn or bind (house rule: no
    // real spawn/socket/bind in this suite).

    #[test]
    fn message_send_with_no_context_and_spawning_disabled_is_a2a_dash_32004() {
        let req = json!({
            "jsonrpc": "2.0", "id": 1, "method": "message/send",
            "params": { "message": { "parts": [{ "kind": "text", "text": "hi" }] } }
        });
        // spawn_agent == "" (the default) → decide_send_action errors out
        // before any process would be spawned.
        let resp = handle_jsonrpc(&req, &test_ctx(Path::new("/dev/null"), ""));
        assert_eq!(resp["error"]["code"], -32004);
        assert_eq!(resp["error"]["message"], "A2A spawn not configured");
        assert_eq!(resp["id"], 1);
    }

    #[test]
    fn message_send_end_to_end_unknown_and_unconductable_contexts_are_clean_errors() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = std::env::temp_dir().join(format!(
            "aoide-server-a2a-send-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        // A registered session with no control socket (not conductable).
        let sf = SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![fixture_session("plain", "working", None)],
        };
        write_stage(&sessions_path(), &sf).unwrap();

        // Unknown contextId → -32001 (never reaches the socket/spawn layer).
        let req = json!({
            "jsonrpc": "2.0", "id": 1, "method": "message/send",
            "params": { "message": {
                "parts": [{ "kind": "text", "text": "hi" }],
                "contextId": "ghost",
            } }
        });
        let resp = handle_jsonrpc(&req, &test_ctx(Path::new("/dev/null"), "claude"));
        assert_eq!(resp["error"]["code"], -32001);

        // Known but not conductable → -32004 "session not conductable".
        let req2 = json!({
            "jsonrpc": "2.0", "id": 2, "method": "message/send",
            "params": { "message": {
                "parts": [{ "kind": "text", "text": "hi" }],
                "contextId": "plain",
            } }
        });
        let resp2 = handle_jsonrpc(&req2, &test_ctx(Path::new("/dev/null"), "claude"));
        assert_eq!(resp2["error"]["code"], -32004);
        assert_eq!(resp2["error"]["message"], "session not conductable");

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// LANE IDENTITY P-ID0 (G16/G5): `stamp_spawn_provenance` is the record-layer
    /// authority `do_spawn` now calls directly, rather than threading a
    /// `node:*` value through the child's own env — proven up to, but never
    /// through, `do_spawn`'s real process spawn, same documented boundary
    /// `node_spawn_signed_and_allowed_is_admitted_up_to_the_do_spawn_
    /// boundary` draws above: the record here is written directly (as
    /// `session_conduct`'s own registration would, once the child comes up),
    /// so the retry loop finds it on its very first poll.
    ///
    /// P-RSA S3 extends it to the second of the two stamps the one retry loop
    /// carries: `None` leaves the record's `remoteParent` absent (the whole
    /// pre-S3 shape), and `Some` lands it on the SAME pass as the origin —
    /// never a second poll, never a second thread.
    #[test]
    fn stamp_spawn_provenance_lands_the_origin_and_the_remote_parent_on_a_registered_record() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = std::env::temp_dir().join(format!(
            "aoide-server-a2a-stamp-origin-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        let sf = SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![
                fixture_session("a2a-spawned-1", "working", None),
                fixture_session("a2a-spawned-2", "working", None),
            ],
        };
        write_stage(&sessions_path(), &sf).unwrap();

        stamp_spawn_provenance("a2a-spawned-1", "node:yomi-strix", None);
        stamp_spawn_provenance(
            "a2a-spawned-2",
            "node:yomi-strix",
            Some(RemoteParent {
                node: "yomi-strix".to_string(),
                key: "ab".repeat(32),
                session_id: "conduct-17991-1790312541".to_string(),
                extra: Default::default(),
            }),
        );

        let after: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec = after.sessions.iter().find(|s| s.session_id == "a2a-spawned-1").unwrap();
        assert_eq!(rec.origin.as_deref(), Some("node:yomi-strix"));
        assert!(rec.remote_parent.is_none(), "no claim → nothing stamped, byte-identical to the pre-S3 shape");

        let rec2 = after.sessions.iter().find(|s| s.session_id == "a2a-spawned-2").unwrap();
        assert_eq!(rec2.origin.as_deref(), Some("node:yomi-strix"));
        let rp = rec2.remote_parent.as_ref().expect("the claim landed on the same registration pass");
        assert_eq!(rp.node, "yomi-strix");
        assert_eq!(rp.key, "ab".repeat(32));
        assert_eq!(rp.session_id, "conduct-17991-1790312541");
        assert_eq!(
            rec2.parent_session_id, None,
            "the door never writes a foreign id into the LOCAL parentSessionId (CONTRACTS.md §4)"
        );

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn unknown_method_is_minus_32601() {
        let req = json!({ "jsonrpc": "2.0", "id": 2, "method": "bogus/method", "params": {} });
        let resp = handle_jsonrpc(&req, &test_ctx(Path::new("/dev/null"), ""));
        assert_eq!(resp["error"]["code"], -32601);
    }

    #[test]
    fn tasks_get_end_to_end_reads_the_stage_sessions_file() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = std::env::temp_dir().join(format!(
            "aoide-server-a2a-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        let sf = SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![fixture_session("s1", "stopped", None)],
        };
        write_stage(&sessions_path(), &sf).unwrap();

        let req = json!({ "jsonrpc": "2.0", "id": 7, "method": "tasks/get", "params": { "id": "s1" } });
        let resp = handle_jsonrpc(&req, &test_ctx(Path::new("/dev/null"), ""));
        assert_eq!(resp["result"]["status"]["state"], "completed");

        let req = json!({ "jsonrpc": "2.0", "id": 8, "method": "tasks/get", "params": { "id": "ghost" } });
        let resp = handle_jsonrpc(&req, &test_ctx(Path::new("/dev/null"), ""));
        assert_eq!(resp["error"]["code"], -32001);

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// Task #33, end to end: a session whose recorded `pid` is
    /// guaranteed-absent — `999_999_999`, the same "no real process anywhere
    /// near this pid" fixture value `graph_residency_p_d6.rs`'s reap-over-
    /// the-socket test uses, since `/proc/999999999` does not exist on any
    /// Linux box — reads `failed` over `tasks/get`, not stale `submitted`/
    /// `working`. `state: "working"` (mid-turn) on purpose: `is_session_dead`'s
    /// `pid_signal` fires unconditionally off the real `/proc` probe,
    /// independent of state/staleness timers, so this is deterministic
    /// without a wall-clock wait or a live compositor.
    ///
    /// The inverse guard sits in the SAME test, against the SAME stage: a
    /// pid-less, windowless "hook-only" record (`fixture_session`'s default
    /// shape) must NOT read dead. `is_session_dead`'s other arms that could
    /// otherwise catch it — window-gone (no `windowAddress` to be gone),
    /// pre-boot-ghost and orphaned-subagent (both sweep-level checks outside
    /// `is_session_dead` itself, never consulted here) — are structurally
    /// out of reach; the one arm that IS in reach, `stale_abandoned`, cannot
    /// fire either, because `task_from_sessions` feeds `is_session_dead` a
    /// `last_seen` that always answers `None` on this read path (no hyprctl
    /// round trip, no reap-style evidence gathering) — and absence of
    /// evidence is never evidence of staleness (`is_session_dead`'s own
    /// "never-false-reap" guard).
    #[test]
    fn tasks_get_end_to_end_a_dead_pid_reads_failed_and_a_hook_only_record_does_not() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = std::env::temp_dir().join(format!(
            "aoide-server-a2a-dead-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        let dead_pid_session = SessionRecord {
            session_id: "s-dead".to_string(),
            state: "working".to_string(),
            // No real process anywhere near this pid — see the doc comment.
            pid: Some(999_999_999),
            ..Default::default()
        };
        let sf = SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![
                dead_pid_session,
                fixture_session("s-hook-only", "idle", None),
            ],
        };
        write_stage(&sessions_path(), &sf).unwrap();

        let req = json!({ "jsonrpc": "2.0", "id": 9, "method": "tasks/get", "params": { "id": "s-dead" } });
        let resp = handle_jsonrpc(&req, &test_ctx(Path::new("/dev/null"), ""));
        assert_eq!(resp["result"]["status"]["state"], "failed");

        let req = json!({ "jsonrpc": "2.0", "id": 10, "method": "tasks/get", "params": { "id": "s-hook-only" } });
        let resp = handle_jsonrpc(&req, &test_ctx(Path::new("/dev/null"), ""));
        assert_eq!(
            resp["result"]["status"]["state"], "submitted",
            "a pid-less hook-only record must read its plain idle->submitted mapping, never failed"
        );

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// Phase G: the read commands (`tasks/get`, `aoide/graphSummary`) are
    /// token-gated by the same rule as spawn. When a token IS configured, an
    /// absent or wrong bearer is a clean `-32005` BEFORE the read runs; a
    /// valid bearer passes through to the normal handler.
    #[test]
    fn read_commands_are_token_gated_when_a_token_is_configured() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = std::env::temp_dir().join(format!(
            "aoide-server-a2a-gate-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        let sf = SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![fixture_session("s1", "stopped", None)],
        };
        write_stage(&sessions_path(), &sf).unwrap();

        let ctx = |presented: Option<&'static str>| RequestCtx {
            audit_log: Path::new("/dev/null"),
            spawn_agent: "",
            spawn_cwd: "",
            origin: ConnOrigin::Loopback,
            node_name: "aoide",
            self_url: "http://127.0.0.1:8710/",
            expected_token: "s3cr3t",
            presented_token: presented,
            signed_caller: None,
            sealed_only: false,
        };
        let get = json!({ "jsonrpc": "2.0", "id": 1, "method": "tasks/get", "params": { "id": "s1" } });
        let sum = json!({ "jsonrpc": "2.0", "id": 2, "method": "aoide/graphSummary" });

        // Absent bearer — both reads denied, and denied BEFORE the read runs
        // (a real session id still returns -32005, never its state).
        assert_eq!(handle_jsonrpc(&get, &ctx(None))["error"]["code"], -32005);
        assert_eq!(handle_jsonrpc(&sum, &ctx(None))["error"]["code"], -32005);
        // Wrong bearer — same.
        assert_eq!(handle_jsonrpc(&get, &ctx(Some("wrong")))["error"]["code"], -32005);
        assert_eq!(handle_jsonrpc(&sum, &ctx(Some("wrong")))["error"]["code"], -32005);
        // Valid bearer — passes the gate; tasks/get reaches its real handler
        // and resolves the known session's state (never -32005).
        let ok = handle_jsonrpc(&get, &ctx(Some("s3cr3t")));
        assert_eq!(ok["result"]["status"]["state"], "completed");
        // graphSummary with a valid bearer is past the gate too — and it
        // must be a REAL read, not just "any non-auth response" (a bare
        // `assert_ne!` here would still pass if the read arm silently broke
        // and started returning some other error): the wrapped graph
        // document and the instance envelope both come through.
        let sum_ok = handle_jsonrpc(&sum, &ctx(Some("s3cr3t")));
        assert_eq!(sum_ok["result"]["schemaVersion"], "0");
        assert_eq!(sum_ok["result"]["instance"]["name"], "aoide");
        assert!(sum_ok["result"]["graph"]["nodes"].is_array());

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// Phase G off-path: with NO token configured (today's default), the read
    /// commands stay open exactly as before — the gate only bites when armed.
    #[test]
    fn read_commands_stay_open_when_no_token_is_configured() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = std::env::temp_dir().join(format!(
            "aoide-server-a2a-nogate-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        let sf = SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![fixture_session("s1", "stopped", None)],
        };
        write_stage(&sessions_path(), &sf).unwrap();

        let req = json!({ "jsonrpc": "2.0", "id": 1, "method": "tasks/get", "params": { "id": "s1" } });
        // Empty expected_token = feature off; no bearer presented; still works.
        let resp = handle_jsonrpc(&req, &test_ctx(Path::new("/dev/null"), ""));
        assert_eq!(resp["result"]["status"]["state"], "completed");

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// A bearer-gated door's two reads, a stage holding one session, and the
    /// registry a signed caller resolves through. `allows` is what the paired
    /// node `box-b` holds in the home mesh; `bearer` is what the request
    /// presents; `signed` is whether the request carries a verified signature.
    fn bearer_door_reads(allows: &[&str], signed: bool, bearer: Option<&'static str>, expected: &'static str) -> (Value, Value) {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("door-read");
        act_as(&root, "here");
        setup_signed_node_with_allows("box-b", allows);
        let sf = SessionsFile { schema_version: "0".to_string(), sessions: vec![fixture_session("s1", "stopped", None)] };
        write_stage(&sessions_path(), &sf).unwrap();

        let audit_log = root.join("log");
        let mut ctx = mail_deposit_ctx(&audit_log, signed.then_some("box-b"));
        ctx.expected_token = expected;
        ctx.presented_token = bearer;
        let get = handle_jsonrpc(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tasks/get", "params": { "id": "s1" } }), &ctx);
        let sum = handle_jsonrpc(&json!({ "jsonrpc": "2.0", "id": 2, "method": "aoide/graphSummary" }), &ctx);
        mail_deposit_cleanup(&root, saved_state, saved_stage);
        (get, sum)
    }

    #[test]
    fn a_signed_caller_holding_read_reads_through_a_bearer_gated_door() {
        let (get, sum) = bearer_door_reads(&["read"], true, None, "s3cr3t");
        assert_eq!(get["result"]["status"]["state"], "completed", "{get}");
        assert!(sum["result"]["graph"]["nodes"].is_array(), "{sum}");
    }

    #[test]
    fn a_signed_caller_without_read_is_refused_by_a_bearer_gated_door() {
        let (get, sum) = bearer_door_reads(&["message"], true, None, "s3cr3t");
        assert_eq!(get["error"]["code"], -32005, "{get}");
        assert_eq!(sum["error"]["code"], -32005, "{sum}");
    }

    #[test]
    fn an_unsigned_loopback_caller_without_the_bearer_is_refused_by_a_bearer_gated_door() {
        let (get, sum) = bearer_door_reads(&["read"], false, None, "s3cr3t");
        assert_eq!(get["error"]["code"], -32005, "{get}");
        assert_eq!(sum["error"]["code"], -32005, "{sum}");
    }

    #[test]
    fn an_unsigned_caller_with_the_bearer_reads_through_a_bearer_gated_door() {
        let (get, sum) = bearer_door_reads(&[], false, Some("s3cr3t"), "s3cr3t");
        assert_eq!(get["result"]["status"]["state"], "completed", "{get}");
        assert!(sum["result"]["graph"]["nodes"].is_array(), "{sum}");
    }

    #[test]
    fn an_unsigned_loopback_caller_reads_a_door_with_no_bearer_configured() {
        let (get, sum) = bearer_door_reads(&[], false, None, "");
        assert_eq!(get["result"]["status"]["state"], "completed", "{get}");
        assert!(sum["result"]["graph"]["nodes"].is_array(), "{sum}");
    }

    #[test]
    fn a_signed_caller_holding_read_resubscribes_through_a_bearer_gated_door() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("door-resubscribe");
        act_as(&root, "here");
        setup_signed_node_with_allows("box-b", &["read"]);
        let sf = SessionsFile { schema_version: "0".to_string(), sessions: vec![fixture_session("s1", "stopped", None)] };
        write_stage(&sessions_path(), &sf).unwrap();

        let audit_log = root.join("log");
        let body = serde_json::to_vec(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tasks/resubscribe", "params": { "id": "s1" } })).unwrap();
        let req = HttpRequest {
            method: "POST".to_string(),
            path: "/".to_string(),
            body,
            bearer: None,
            signed_node: None,
            signed_timestamp: None,
            signed_nonce: None,
            signed_signature: None,
            signed_mesh: None,
        };
        let ctx = mail_deposit_ctx(&audit_log, Some("box-b"));
        let stream = |signed_caller| {
            let mut out = Vec::new();
            stream_task(&mut out, &req, "tasks/resubscribe", &audit_log, "", "", ConnOrigin::Loopback, "s3cr3t", None, signed_caller).unwrap();
            first_sse_data_json(&out)
        };
        let signed = stream(ctx.signed_caller);
        let unsigned = stream(None);
        mail_deposit_cleanup(&root, saved_state, saved_stage);

        assert_eq!(signed["result"]["status"]["state"], "completed", "{signed}");
        assert_eq!(unsigned["error"]["code"], -32005, "{unsigned}");
    }

    /// Pull the JSON payload out of the FIRST `data: <json>\n\n` SSE frame in
    /// `stream_task`'s written output (`sse_event`'s exact format). Every
    /// `stream_task` test below writes at most one event before returning
    /// (a denial closes immediately; a terminal initial state closes on its
    /// first tick), so "first" is also "only" in practice.
    fn first_sse_data_json(out: &[u8]) -> Value {
        let text = String::from_utf8_lossy(out);
        let frame = text.split("data: ").nth(1).expect("at least one SSE data event was written");
        let line = frame.split('\n').next().unwrap();
        serde_json::from_str(line).expect("SSE data line is valid JSON")
    }

    /// Phase G, SSE half: `tasks/resubscribe` is gated by the SAME rule as
    /// the one-shot `tasks/get` — proven here at the `stream_task` level
    /// (not just `handle_jsonrpc`), driving the function with a real
    /// `Vec<u8>` writer exactly as `handle_connection` would. A REAL staged
    /// session id is used so a passing gate would leak real state; instead
    /// the SSE headers open (the socket contract doesn't change) and the
    /// stream's one and only event is the `-32005` denial — the session's
    /// state is never read.
    #[test]
    fn stream_task_tasks_resubscribe_denies_before_reading_a_real_session_when_a_token_is_configured() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = std::env::temp_dir().join(format!(
            "aoide-server-a2a-sse-gate-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        let sf = SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![fixture_session("s1", "working", None)],
        };
        write_stage(&sessions_path(), &sf).unwrap();

        let body = serde_json::to_vec(&json!({
            "jsonrpc": "2.0", "id": 1, "method": "tasks/resubscribe", "params": { "id": "s1" }
        }))
        .unwrap();
        let req = HttpRequest {
            method: "POST".to_string(),
            path: "/".to_string(),
            body,
            bearer: None,
            signed_node: None,
            signed_timestamp: None,
            signed_nonce: None,
            signed_signature: None,
            signed_mesh: None,
        };

        let mut out: Vec<u8> = Vec::new();
        let result = stream_task(
            &mut out,
            &req,
            "tasks/resubscribe",
            Path::new("/dev/null"),
            "",
            "",
            ConnOrigin::Loopback,
            "s3cr3t",
            None,
            None,
        );
        assert!(result.is_ok(), "a denied stream still returns Ok — it closed cleanly, not by erroring out");

        let text = String::from_utf8_lossy(&out);
        assert!(text.contains("text/event-stream"), "SSE headers are written before the gate is even consulted: {text}");
        let event = first_sse_data_json(&out);
        assert_eq!(event["error"]["code"], -32005);

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// Phase G, SSE half: `message/stream` is gated BEFORE `message_send`
    /// runs, so an unauthenticated caller can neither inject into nor spawn
    /// off an existing session through the streaming path. The
    /// security-load-bearing assertion isn't the `-32005` alone (a broken
    /// gate that still happened to error out some OTHER way would pass
    /// that) — it's that `message_send` never ran at ALL: a conductable
    /// session's control socket is stood in with a real `UnixListener` (a
    /// wrongly-attempted Inject would connect to it) AND the pending-queue
    /// file (`do_inject`'s fallback when delivery isn't immediate) never
    /// gets created, proving `do_inject`/`session_send` were never reached
    /// rather than merely "delivery was skipped".
    #[test]
    fn stream_task_message_stream_denies_before_message_send_runs_when_a_token_is_configured() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-sse-msend-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::remove_var("AOIDE_CONDUCT_AUTOGATE");
        std::env::remove_var("AOIDE_SESSION_ID");

        let id = "sse-tgt";
        let socket = aoide_conduct::graph::conduct_socket_path(id);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        // If message_send WRONGLY ran (the gate failed to short-circuit
        // before it), a successful Inject would connect here.
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();

        let sf = SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![conductable_session(id, &socket)],
        };
        write_stage(&sessions_path(), &sf).unwrap();

        let body = serde_json::to_vec(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "message/stream",
            "params": { "message": { "parts": [{ "kind": "text", "text": "hi" }], "contextId": id } }
        }))
        .unwrap();
        let req = HttpRequest {
            method: "POST".to_string(),
            path: "/".to_string(),
            body,
            bearer: None,
            signed_node: None,
            signed_timestamp: None,
            signed_nonce: None,
            signed_signature: None,
            signed_mesh: None,
        };

        let audit_log = root.join("log");
        let mut out: Vec<u8> = Vec::new();
        let result = stream_task(
            &mut out,
            &req,
            "message/stream",
            &audit_log,
            "",
            "",
            ConnOrigin::Loopback,
            "s3cr3t",
            None,
            None,
        );
        assert!(result.is_ok());

        let event = first_sse_data_json(&out);
        assert_eq!(event["error"]["code"], -32005);

        assert!(
            !stage.join("pending.json").exists(),
            "message_send must never run once the SSE gate denies — a held-pending send would still \
             have written pending.json, so its absence proves do_inject was never reached at all"
        );
        assert!(
            listener.accept().is_err(),
            "the conducted session's socket must never be touched by a denied message/stream"
        );

        let _ = std::fs::remove_dir_all(&root);
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    /// Phase G off-path, SSE half: with NO token configured, `stream_task`
    /// is byte-identical to before — `tasks/resubscribe` reaches the real
    /// session and streams its actual terminal state, proving the gate
    /// doesn't bite when off (not just that it returns SOME non-error
    /// event). The fixture session's canonical state ("stopped" ->
    /// "completed", `a2a_task_state`) is already terminal, so
    /// `stream_task`'s loop emits the final event on its very first tick
    /// and returns — no `STREAM_POLL` sleep, nowhere near `MAX_STREAM`.
    #[test]
    fn stream_task_tasks_resubscribe_reaches_the_real_terminal_state_when_no_token_is_configured() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = std::env::temp_dir().join(format!(
            "aoide-server-a2a-sse-nogate-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        let sf = SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![fixture_session("s1", "stopped", None)],
        };
        write_stage(&sessions_path(), &sf).unwrap();

        let body = serde_json::to_vec(&json!({
            "jsonrpc": "2.0", "id": 9, "method": "tasks/resubscribe", "params": { "id": "s1" }
        }))
        .unwrap();
        let req = HttpRequest {
            method: "POST".to_string(),
            path: "/".to_string(),
            body,
            bearer: None,
            signed_node: None,
            signed_timestamp: None,
            signed_nonce: None,
            signed_signature: None,
            signed_mesh: None,
        };

        let mut out: Vec<u8> = Vec::new();
        let result = stream_task(
            &mut out,
            &req,
            "tasks/resubscribe",
            Path::new("/dev/null"),
            "",
            "",
            ConnOrigin::Loopback,
            "",
            None,
            None,
        );
        assert!(result.is_ok());

        let event = first_sse_data_json(&out);
        assert_eq!(event["result"]["status"]["state"], "completed");
        assert_eq!(event["result"]["final"], true, "the terminal state closes the stream on its first tick");

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    // ── The non-loopback pending-gate amendment (CONTRACTS.md §6, 2026-08-14) ──
    //
    // The one must-fix security gap: `do_inject` used to force `--yes`
    // UNCONDITIONALLY, so any reachable node could inject text into any
    // local conductable session with zero approval the moment the door binds
    // somewhere other than loopback. These tests drive `message_send`
    // directly (the same function `handle_jsonrpc`'s `message/send` arm
    // calls) with a real `UnixListener` standing in for the target session's
    // control socket, exactly like `conduct::graph::send`'s own gate tests.

    /// What a network door does with an inject: the session's socket sees
    /// nothing, and the send queues on `pending.json` (`queued` is the running
    /// total for the test). Puts `listener` nonblocking to make that check.
    fn assert_held_at_the_door(listener: &UnixListener, stage: &std::path::Path, queued: usize) {
        listener.set_nonblocking(true).unwrap();
        assert!(listener.accept().is_err(), "a network door never writes to the session's socket");
        let queue: Value = serde_json::from_str(&std::fs::read_to_string(stage.join("pending.json")).expect("the send is queued")).unwrap();
        assert_eq!(queue["pending"].as_array().map(Vec::len), Some(queued), "{queue}");
    }

    /// A conducted harness wrap: labelled `claude`, and the launcher walk resolved
    /// `claude` (`wrapped_program`), so it carries the proof every pty writer asks.
    /// A test that needs another shape sets it after this.
    fn conductable_session(id: &str, socket: &std::path::Path) -> SessionRecord {
        let mut rec = fixture_session(id, "working", None);
        rec.conductable = Some(true);
        rec.socket = Some(socket.to_string_lossy().into_owned());
        rec.agent = "claude".to_string();
        rec.wrapped_program = Some("claude".to_string());
        rec
    }

    #[test]
    fn classify_origin_maps_loopback_remote_and_unknown() {
        assert_eq!(classify_origin(Some("127.0.0.1".parse().unwrap())), ConnOrigin::Loopback);
        assert_eq!(classify_origin(Some("::1".parse().unwrap())), ConnOrigin::Loopback);
        assert_eq!(
            classify_origin(Some("10.0.0.5".parse().unwrap())),
            ConnOrigin::Remote("10.0.0.5".parse().unwrap())
        );
        assert_eq!(classify_origin(None), ConnOrigin::Unknown);
    }

    #[test]
    fn classify_origin_maps_cgnat_tailnet_and_lan_addresses_to_remote() {
        // Table test (M3, task #16): a Melete-triggered job dials in from a
        // real box's LAN or tailnet address, never loopback — this is the
        // shape `classify_origin` must fold to `Remote`, not the shape that
        // (incorrectly) free-passes it. No
        // CIDR/tailnet special-casing exists or is added here: every one of
        // these is just "some non-loopback `Some(ip)`", the same arm
        // `10.0.0.5` already exercises above — this table only widens the
        // address SHAPES covered (CGNAT/tailnet 100.64.0.0/10, ordinary LAN),
        // it does not add a new code path.
        let cases: &[(&str, ConnOrigin)] = &[
            // sakaki's tailscale0 (100.82.117.51, brief's verified recon) —
            // CGNAT-range tailnet address.
            ("100.82.117.51", ConnOrigin::Remote("100.82.117.51".parse().unwrap())),
            // sakaki's LAN address (192.168.1.202, brief's verified recon).
            ("192.168.1.202", ConnOrigin::Remote("192.168.1.202".parse().unwrap())),
            // Loopback stays loopback regardless of address family —
            // unchanged by this table, restated here so the two families
            // sit side by side in one place.
            ("127.0.0.1", ConnOrigin::Loopback),
            ("::1", ConnOrigin::Loopback),
        ];
        for (addr, expected) in cases {
            let ip: IpAddr = addr.parse().unwrap();
            assert_eq!(classify_origin(Some(ip)), *expected, "address {addr}");
        }
    }

    #[test]
    fn classify_token_is_absent_valid_or_invalid() {
        assert_eq!(classify_token("s3cr3t", None), TokenState::Absent);
        assert_eq!(classify_token("s3cr3t", Some("s3cr3t")), TokenState::Valid);
        assert_eq!(classify_token("s3cr3t", Some("wrong")), TokenState::Invalid);
        // A presented token of a DIFFERENT length than expected is still a
        // clean Invalid, not a panic or an early-return short-circuit.
        assert_eq!(classify_token("s3cr3t", Some("s3cr3tt")), TokenState::Invalid);
        assert_eq!(classify_token("s3cr3t", Some("")), TokenState::Invalid);
    }

    #[test]
    fn token_authorized_always_allows_when_no_token_is_configured() {
        // THE regression pin: `spawn_agent` alone (rebuild-time admission)
        // still fully gates spawn, and the read commands stay open, when no
        // token is set — Phase G adds a gate, it doesn't tighten the existing
        // off-path.
        for token_state in [TokenState::Absent, TokenState::Invalid, TokenState::Valid] {
            assert!(token_authorized(false, token_state), "token_configured=false must always allow, got {token_state:?}");
        }
    }

    #[test]
    fn token_authorized_requires_a_valid_token_once_one_is_configured() {
        assert!(!token_authorized(true, TokenState::Absent));
        assert!(!token_authorized(true, TokenState::Invalid));
        assert!(token_authorized(true, TokenState::Valid));
    }

    #[test]
    fn extract_bearer_parses_the_authorization_header_value() {
        assert_eq!(extract_bearer("Bearer abc123").as_deref(), Some("abc123"));
        // The scheme name is case-insensitive (RFC 7235 §2.1); extra
        // whitespace around the token is trimmed.
        assert_eq!(extract_bearer("bearer   abc123  ").as_deref(), Some("abc123"));
        assert_eq!(extract_bearer("BEARER abc123").as_deref(), Some("abc123"));
        // Any other scheme, a missing token, or a malformed header → None.
        assert_eq!(extract_bearer("Basic dXNlcjpwYXNz"), None);
        assert_eq!(extract_bearer("Bearer"), None);
        assert_eq!(extract_bearer("Bearer   "), None);
        assert_eq!(extract_bearer(""), None);
    }

    #[test]
    fn parse_http_request_captures_the_authorization_bearer_header() {
        let raw = b"POST / HTTP/1.1\r\nAuthorization: Bearer my-token\r\nContent-Length: 2\r\n\r\n{}";
        let mut r = BufReader::new(std::io::Cursor::new(&raw[..]));
        let req = parse_http_request(&mut r, Instant::now()).unwrap();
        assert_eq!(req.bearer.as_deref(), Some("my-token"));

        // No Authorization header at all → None, same as before this field
        // existed.
        let raw2 = b"POST / HTTP/1.1\r\nContent-Length: 2\r\n\r\n{}";
        let mut r2 = BufReader::new(std::io::Cursor::new(&raw2[..]));
        let req2 = parse_http_request(&mut r2, Instant::now()).unwrap();
        assert_eq!(req2.bearer, None);
    }

    #[test]
    fn resolve_token_file_prefers_flag_then_env_then_defaults_empty() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_A2A_TOKEN_FILE").ok();

        let mut flags = std::collections::BTreeMap::new();
        flags.insert("token-file".to_string(), "/flag/path".to_string());
        let inv = Invocation { path: vec![], args: vec![], flags, door: Door::Cli };
        std::env::set_var("AOIDE_A2A_TOKEN_FILE", "/env/path");
        assert_eq!(resolve_token_file(&inv), "/flag/path", "an explicit flag wins outright");

        let inv_no_flag = Invocation {
            path: vec![],
            args: vec![],
            flags: std::collections::BTreeMap::new(),
            door: Door::Cli,
        };
        assert_eq!(resolve_token_file(&inv_no_flag), "/env/path", "falls back to the env var");

        std::env::remove_var("AOIDE_A2A_TOKEN_FILE");
        assert_eq!(resolve_token_file(&inv_no_flag), "", "defaults to empty (no token required)");

        match saved {
            Some(v) => std::env::set_var("AOIDE_A2A_TOKEN_FILE", v),
            None => std::env::remove_var("AOIDE_A2A_TOKEN_FILE"),
        }
    }

    #[test]
    fn read_expected_token_trims_and_tolerates_absence() {
        assert_eq!(read_expected_token(""), None, "an empty path is feature-off — no disk read");
        assert_eq!(
            read_expected_token("/nonexistent/aoide-a2a-token-file-does-not-exist"),
            None,
            "an unreadable path is tolerated as unconfigured, not a hard failure"
        );

        let dir = std::env::temp_dir().join(format!("aoide-a2a-token-file-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("token");
        std::fs::write(&path, "  s3cr3t-value\n").unwrap();
        assert_eq!(
            read_expected_token(path.to_str().unwrap()),
            Some("s3cr3t-value".to_string()),
            "trims surrounding whitespace/newline"
        );

        std::fs::write(&path, "   \n").unwrap();
        assert_eq!(
            read_expected_token(path.to_str().unwrap()),
            None,
            "a whitespace-only file is treated as unconfigured"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn message_send_spawn_rejects_an_unpaired_caller_with_the_p_p3_taught_error() {
        // No contextId → the Spawn arm — spawn_agent is non-empty so
        // `decide_send_action` resolves to Spawn, and the P-P3 gate must
        // reject it BEFORE `do_spawn` ever runs (so this never actually
        // spawns a process — the house rule every other error-branch test in
        // this suite already follows). No node is registered at all, so
        // NEITHER the old door-wide-token gate NOR the new pairing gate can
        // possibly pass — -32006, not the old -32005.
        //
        // The spawn gate reads the process-global config and node registry, so
        // this holds the env lock like every test that sets either: unlocked,
        // it raced the bad-config tests and read their unparseable file.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let params = json!({
            "message": { "parts": [{ "kind": "text", "text": "hi" }] }
        });
        let err = message_send(
            &params,
            Path::new("/dev/null"),
            "claude",
            "",
            ConnOrigin::Loopback,
            "expected-secret",
            None, None
        )
        .unwrap_err();
        assert_eq!(err.0, -32006);

        let err2 = message_send(
            &params,
            Path::new("/dev/null"),
            "claude",
            "",
            ConnOrigin::Loopback,
            "expected-secret",
            Some("wrong-secret"), None
        )
        .unwrap_err();
        assert_eq!(err2.0, -32006);
    }

    // ── Spawn liveness check (task #103) ──────────────────────────────────
    //
    // `poll_bounded_exit` and `spawn_died_immediately_message` are the pure
    // halves of `do_spawn`'s bounded liveness check, factored out exactly so
    // they're testable without a real spawn — same "never through `do_spawn`
    // itself" precedent the table below states for the gate predicates.

    /// A REAL `exit 1` status from this host's own shell. `std::process::
    /// ExitStatus` has no portable constructor — `ExitStatusExt::from_raw` is
    /// POSIX-only — and a status FABRICATED for the fixture is exactly what
    /// these two tests must not come to depend on, so the fixture asks the
    /// host for one. Asserts the code on the way out, so a shell that answered
    /// differently could never silently become the fixture.
    fn exit_one_status() -> std::process::ExitStatus {
        #[cfg(unix)]
        let mut cmd = {
            let mut c = std::process::Command::new("sh");
            c.args(["-c", "exit 1"]);
            c
        };
        #[cfg(windows)]
        let mut cmd = {
            let mut c = std::process::Command::new("cmd");
            c.args(["/C", "exit 1"]);
            c
        };
        cmd.stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
        let status = cmd.status().expect("this host's shell runs");
        assert_eq!(status.code(), Some(1), "the fixture must be exit 1");
        status
    }

    #[test]
    fn poll_bounded_exit_returns_the_status_the_moment_try_wait_reports_one() {
        let mut calls = 0u32;
        let status = poll_bounded_exit(
            || {
                calls += 1;
                if calls == 3 {
                    Ok(Some(exit_one_status())) // exit code 1
                } else {
                    Ok(None)
                }
            },
            10,
            Duration::ZERO,
        );
        assert_eq!(status.and_then(|s| s.code()), Some(1));
        assert_eq!(calls, 3, "must stop polling the instant an exit is reported");
    }

    #[test]
    fn poll_bounded_exit_gives_up_after_the_full_budget_with_the_child_still_alive() {
        let mut calls = 0u32;
        let status = poll_bounded_exit(
            || {
                calls += 1;
                Ok(None)
            },
            5,
            Duration::ZERO,
        );
        assert_eq!(status, None, "still alive past the budget — never a false failure");
        assert_eq!(calls, 5, "the full attempt budget must be spent, no early giving-up");
    }

    #[test]
    fn spawn_died_immediately_message_names_the_proven_harness_never_the_command_line() {
        let status = exit_one_status(); // exit code 1
        let harness = spawn_agent_harness("claude --dangerously-skip-permissions").unwrap();
        let msg = spawn_died_immediately_message(harness, status);
        assert!(msg.contains("`claude`"), "names the proven harness: {msg}");
        assert!(
            !msg.contains("--dangerously-skip-permissions"),
            "never echoes flag values back — taught, not a raw command dump: {msg}"
        );
        assert!(!msg.contains("PATH="), "no env leakage");
    }

    // ── S-B: spawn env sanitize + bounded spawn cwd ──────────────────────────
    //
    // `spawn_child_command` and `resolve_bounded_spawn_cwd` are `do_spawn`'s
    // own pure-ish halves, factored out exactly so they're testable without a
    // real OS-level spawn — same "never through `do_spawn` itself" precedent:
    // `current_exe()` resolves to the TEST binary under `cargo test`, so no
    // test drives that spawn.

    /// GATED on native Windows with its PROVEN reason: the fixture proves the
    /// child's environment by spawning `/bin/sh` and comparing `printf`'s exact
    /// bytes (`sibling-ok|unset`, and no trailing newline). This host has neither
    /// `/bin/sh` nor `printf`, and `cmd` neither expands an undefined `%VAR%`
    /// the way `${VAR:-unset}` does nor prints without a trailing CRLF — so the
    /// PROBE is a POSIX fact. The builder-state assertions above the spawn are
    /// consequently not run there either, and that gap is named rather than
    /// implied.
    #[cfg_attr(windows, ignore = "the probe spawns `/bin/sh` and compares `printf`'s exact bytes; this host has no /bin/sh or printf")]
    #[test]
    fn a2a_spawn_clears_the_daemons_own_session_id_from_the_child() {
        // The Osaka wrong-ancestry bug: the `aoide-a2a` unit's own
        // environment can carry the operator's live `AOIDE_SESSION_ID`
        // (inherited from whatever terminal the unit itself descends from),
        // and a spawned child must never see it. First proven via
        // `Command::get_envs()` (stable since Rust 1.57): it enumerates only
        // the EXPLICIT `.env()`/`.env_remove()` calls a `Command` carries — a
        // removed var reports `Some(None)`, and a var the `Command` never
        // mentions is simply ABSENT from the map, meaning ordinary fork/exec
        // inheritance still applies to it. Then proven for real: with a
        // sibling var and a synthetic session id actually set in THIS
        // process's own environment, the child is actually spawned (stdio
        // re-piped over `spawn_child_command`'s null default — `Command`'s
        // builder setters are last-call-wins, so re-configuring after the
        // fact is safe) and its own stdout is read back, showing the sibling
        // var passed through by ordinary inheritance while
        // `AOIDE_SESSION_ID` did not — the removal targets
        // `AOIDE_SESSION_ORIGIN`/`AOIDE_SESSION_ID` by name, nothing else.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_sibling = std::env::var("AOIDE_A2A_SIBLING_TEST_VAR").ok();
        let saved_session_id = std::env::var("AOIDE_SESSION_ID").ok();
        std::env::set_var("AOIDE_A2A_SIBLING_TEST_VAR", "sibling-ok");
        std::env::set_var("AOIDE_SESSION_ID", "a2a-test-synthetic-session-id");

        let mut cmd = spawn_child_command(
            Path::new("/bin/sh"),
            &[
                "-c".to_string(),
                "printf '%s|%s' \"$AOIDE_A2A_SIBLING_TEST_VAR\" \"${AOIDE_SESSION_ID:-unset}\""
                    .to_string(),
            ],
            Path::new("/tmp/aoide-a2a-test-audit-does-not-need-to-exist.log"),
            None,
        );
        let envs: std::collections::HashMap<&std::ffi::OsStr, Option<&std::ffi::OsStr>> =
            cmd.get_envs().collect();
        assert_eq!(
            envs.get(std::ffi::OsStr::new("AOIDE_SESSION_ID")),
            Some(&None),
            "AOIDE_SESSION_ID must be explicitly removed from the child, not merely absent: {envs:?}"
        );
        assert_eq!(
            envs.get(std::ffi::OsStr::new("AOIDE_SESSION_ORIGIN")),
            Some(&None),
            "the pre-existing AOIDE_SESSION_ORIGIN removal must still be present, unreplaced: {envs:?}"
        );

        let output = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .expect("spawning /bin/sh must succeed in the test sandbox");
        assert!(
            output.status.success(),
            "the child must exit cleanly: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "sibling-ok|unset",
            "the sibling var passes through untouched and AOIDE_SESSION_ID is gone, proven by \
             an actually spawned child reading its own environment back — not just the \
             Command's builder state"
        );

        match saved_sibling {
            Some(v) => std::env::set_var("AOIDE_A2A_SIBLING_TEST_VAR", v),
            None => std::env::remove_var("AOIDE_A2A_SIBLING_TEST_VAR"),
        }
        match saved_session_id {
            Some(v) => std::env::set_var("AOIDE_SESSION_ID", v),
            None => std::env::remove_var("AOIDE_SESSION_ID"),
        }
    }

    #[test]
    fn a2a_spawn_uses_a_registered_project_root_as_the_child_cwd() {
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-spawncwd-ok-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let root_str = root.to_string_lossy().into_owned();
        let projects = vec![Project {
            name: "aoide".into(),
            path: root_str.clone(),
            ..Default::default()
        }];
        let audit_log = root.join("audit.log");

        let resolved = resolve_bounded_spawn_cwd(&root_str, &projects, &audit_log);
        assert_eq!(
            resolved,
            Some(root_str),
            "a byte-identical registered, existing root is accepted"
        );
        assert!(
            std::fs::read_to_string(&audit_log)
                .unwrap_or_default()
                .is_empty(),
            "the accept path never audits — only the reject path does"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a2a_spawn_ignores_an_unregistered_spawn_cwd_and_audits_it() {
        let registered_root = std::env::temp_dir().join(format!(
            "aoide-a2a-spawncwd-registered-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let unregistered_dir = std::env::temp_dir().join(format!(
            "aoide-a2a-spawncwd-unregistered-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&registered_root).unwrap();
        std::fs::create_dir_all(&unregistered_dir).unwrap();
        let projects = vec![Project {
            name: "aoide".into(),
            path: registered_root.to_string_lossy().into_owned(),
            ..Default::default()
        }];
        let audit_log = registered_root.join("audit.log");

        let unregistered_str = unregistered_dir.to_string_lossy().into_owned();
        let resolved = resolve_bounded_spawn_cwd(&unregistered_str, &projects, &audit_log);
        assert_eq!(
            resolved, None,
            "a real, existing directory that is simply not a registered root is still refused"
        );

        let log = std::fs::read_to_string(&audit_log).unwrap_or_default();
        assert!(
            log.contains("\"door\":\"a2a\""),
            "audited through Door::A2a: {log}"
        );
        assert!(
            log.contains("\"status\":\"skipped\""),
            "the reject path audits exactly once, as \"skipped\": {log}"
        );
        // The audit line is JSON, so the rejected path appears ESCAPED there
        // (`\\` on native Windows) — accept either spelling rather than
        // comparing raw text, which a backslash path can never match.
        let escaped = unregistered_str.replace('\\', "\\\\");
        assert!(
            log.contains(&unregistered_str) || log.contains(&escaped),
            "names the rejected path, never silently: {log}"
        );

        let _ = std::fs::remove_dir_all(&registered_root);
        let _ = std::fs::remove_dir_all(&unregistered_dir);
    }

    // ── Spawn gate table (P-P3, PAIRING.md decision 6) ───────────────────────
    //
    // `node_may_spawn` (the pure predicate) covers paired+allowed / paired+
    // denied / unpaired, and `spawn_admitted` (the full check, folding in
    // WHICH rung resolved the caller) covers token-rung-admitted /
    // addr-rung-refused, both directly against fixtures — never through
    // `message_send`/`do_spawn`, which would actually launch a process (see
    // no test in this file drives `do_spawn`'s real OS-level spawn, because
    // `current_exe()` under `cargo test` is the test binary). The integration
    // tests below drive `message_send`
    // itself for the REFUSAL branches, which never reach `do_spawn` at all.

    #[test]
    fn may_spawn_reads_the_grant_and_nothing_else() {
        // P-CHARTER: the capability question is `Grant::holds("spawn")` and
        // nothing more — the grant ALREADY carries "a verified record said
        // so in this mesh" (`paired_grant` builds it that way), so there is
        // no second `verified` check to forget here.
        assert!(may_spawn(&grant_of("home", &["read", "spawn"])), "spawn in the grant");
        assert!(!may_spawn(&grant_of("home", &["read"])), "spawn revoked — not in the grant");
        assert!(!may_spawn(&Grant::none()), "no record, no resolution, nothing: still refused");
    }

    #[test]
    fn spawn_admitted_requires_the_signature_rung_specifically() {
        // P-P4's narrowing (superseding the 2026-08-25/P-P3 narrowing this
        // test used to pin): the grant alone says nothing about HOW the
        // caller resolved to this node — `spawn_admitted` is the full check,
        // and it must refuse the SAME spawn-holding caller whenever anything
        // less than a verified per-request SIGNATURE resolved it, including
        // the Token rung that P-P3 itself admitted. Proven at the
        // predicate/wiring level directly against `(Node, NodeRung)` +
        // `Grant` fixtures, the same "never through `message_send`/
        // `do_spawn`" precedent this suite already holds for the positive
        // case.
        let mut paired_allowed = fixture_node("box-b", "http://10.0.0.5:8710/");
        paired_allowed.verified = true;
        paired_allowed.grants = aoide_storage::node_store::grants_in("home", &["read", "spawn"]);
        let grant = grant_for(&paired_allowed, "home");

        assert!(
            spawn_admitted(Some((&paired_allowed, aoide_storage::node_store::NodeRung::Signature)), &grant),
            "paired + spawn in the grant + resolved via a VERIFIED SIGNATURE — admitted"
        );
        assert!(
            !spawn_admitted(Some((&paired_allowed, aoide_storage::node_store::NodeRung::Token)), &grant),
            "the SAME grant holder, resolved only via its bare token — refused as of P-P4: \
             a shared secret is no longer sufficient, only a per-request signature is"
        );
        assert!(
            !spawn_admitted(Some((&paired_allowed, aoide_storage::node_store::NodeRung::Addr)), &grant),
            "the SAME grant holder, resolved only by address — refused: address alone never admits spawn"
        );
        assert!(!spawn_admitted(None, &grant), "no resolution at all — refused");
        assert!(
            !spawn_admitted(Some((&paired_allowed, aoide_storage::node_store::NodeRung::Signature)), &Grant::none()),
            "a signed caller with no grant in THIS mesh — refused: the mesh is part of the question"
        );
    }

    #[test]
    fn spawn_refusal_names_the_right_reason_for_each_shape() {
        // P-P4: the taught error MESSAGE (not just the code, still `-32006`
        // uniformly) now distinguishes "paired but unsigned" from "never
        // paired" from "signed but not allowed" — the brief's own
        // requirement ("a taught error telling an unsigned paired caller
        // that its aoide is too old / must sign").
        use aoide_storage::node_store::NodeRung;
        let mut paired_allowed = fixture_node("box-b", "http://10.0.0.5:8710/");
        paired_allowed.verified = true;
        paired_allowed.grants = aoide_storage::node_store::grants_in("home", &["spawn"]);

        let (code, msg) = spawn_refusal(Some((&paired_allowed, NodeRung::Token)), "home");
        assert_eq!(code, -32006);
        assert!(msg.contains("was not signed"), "Token-rung-but-verified must name signing specifically: {msg:?}");

        let mut paired_denied = paired_allowed.clone();
        paired_denied.grants = aoide_storage::node_store::grants_in("home", &["read"]);
        let (code, msg) = spawn_refusal(Some((&paired_denied, NodeRung::Signature)), "home");
        assert_eq!(code, -32006);
        assert!(msg.contains("node allow"), "Signature-rung-but-not-allowed must name the `node allow` fix: {msg:?}");
        assert!(
            msg.contains("--mesh home"),
            "review finding 11: the taught fix must be a command the operator can actually type on a \
             two-mesh record — it names the mesh the request acted in: {msg:?}"
        );

        let (code, msg) = spawn_refusal(None, "home");
        assert_eq!(code, -32006);
        assert!(msg.contains("aoide pair"), "no resolution at all must point at the pairing ceremony: {msg:?}");

        let mut unverified = fixture_node("box-c", "http://10.0.0.6:8710/");
        unverified.grants = aoide_storage::node_store::grants_in("home", &["spawn"]); // a granted set that was never actually paired.
        let (code, msg) = spawn_refusal(Some((&unverified, NodeRung::Token)), "home");
        assert_eq!(code, -32006);
        assert!(
            msg.contains("aoide pair"),
            "Token rung but NOT verified is the generic 'never paired' message, not the 'must sign' one: {msg:?}"
        );
    }

    // ── P-P4: `verify_signed_request` (docs/architecture/PAIRING.md's
    // "Wire authentication (paired nodes)" section) ─────────────────────────

    /// Register `node_name` as a VERIFIED node holding THIS test process's
    /// own P-P1 identity's pubkey, and return that keypair to sign with.
    /// A test-only shortcut: in production a node's stored pubkey is always
    /// the OTHER instance's, never this process's own, but a single-process
    /// test has no second identity to mint — signing "as itself" and
    /// registering itself as its own trusted node exercises the
    /// canonical-string/verify plumbing in isolation with no second process
    /// involved, exactly the same "one process plays both roles" shortcut
    /// `identity.rs`'s own sign/verify round-trip test already takes.
    fn setup_signed_node(node_name: &str) -> aoide_storage::identity::Keypair {
        let (kp, _) = aoide_storage::identity::load_or_mint().unwrap();
        let mut node = fixture_node(node_name, "http://node/");
        node.verified = true;
        node.pubkey = Some(kp.info().pubkey_hex);
        aoide_storage::node_store::save_nodes(&[node]).unwrap();
        kp
    }

    /// [`setup_signed_node`]'s sibling with a controllable grant set — the
    /// admission/revocation round-trip tests need a genuinely paired+verified
    /// node whose grants they choose, in the mesh they choose (P-CHARTER).
    fn setup_signed_node_with_allows(node_name: &str, allows: &[&str]) -> aoide_storage::identity::Keypair {
        setup_signed_node_with_grants(node_name, "home", allows)
    }

    /// [`setup_signed_node_with_allows`] with the mesh spelled out — the
    /// fixture a named-mesh test needs (review N5).
    fn setup_signed_node_with_grants(node_name: &str, mesh: &str, allows: &[&str]) -> aoide_storage::identity::Keypair {
        let (kp, _) = aoide_storage::identity::load_or_mint().unwrap();
        let mut node = fixture_node(node_name, "http://node/");
        node.verified = true;
        node.pubkey = Some(kp.info().pubkey_hex);
        node.grants = aoide_storage::node_store::grants_in(mesh, allows);
        aoide_storage::node_store::save_nodes(&[node]).unwrap();
        kp
    }

    /// Build a signed [`HttpRequest`] for `path`/`body`, timestamped
    /// `ts_epoch` seconds since epoch, nonce `nonce` — the test-side mirror
    /// of `aoide-client`'s real wire builder, built directly against
    /// `aoide_storage::wire_auth` rather than through a real curl call.
    fn signed_request(kp: &aoide_storage::identity::Keypair, node_name: &str, path: &str, body: &[u8], ts_epoch: i64, nonce: &str) -> HttpRequest {
        signed_request_in_mesh(kp, node_name, path, body, ts_epoch, nonce, None)
    }

    /// [`signed_request`] with the `X-Aoide-Mesh` header (P-CHARTER) — the
    /// pre-charter shape is the `None` arm above, so every test written
    /// before per-mesh trust keeps signing exactly what it signed before.
    fn signed_request_in_mesh(
        kp: &aoide_storage::identity::Keypair,
        node_name: &str,
        path: &str,
        body: &[u8],
        ts_epoch: i64,
        nonce: &str,
        mesh: Option<&str>,
    ) -> HttpRequest {
        let timestamp = aoide_storage::time::iso_utc_from_epoch(ts_epoch);
        let canonical = aoide_storage::wire_auth::canonical_string("POST", path, &timestamp, nonce, body, mesh);
        let signature = aoide_storage::wire_auth::sign_hex(kp, canonical.as_bytes());
        HttpRequest {
            method: "POST".to_string(),
            path: path.to_string(),
            body: body.to_vec(),
            bearer: None,
            signed_node: Some(node_name.to_string()),
            signed_timestamp: Some(timestamp),
            signed_nonce: Some(nonce.to_string()),
            signed_signature: Some(signature),
            signed_mesh: mesh.map(str::to_string),
        }
    }

    /// An untagged call an old client may send: admitted without `to`, so a test can
    /// reach the nonce step with nothing on disk beyond its own signer.
    const UNTAGGED_POLL: &[u8] = br#"{"jsonrpc":"2.0","id":1,"method":"aoide/mailPoll","params":{"node":"box-b"}}"#;

    /// One signed frame through the door's admission, in the order
    /// `handle_connection` takes it: the clock and signature checks
    /// ([`verify_signed_request`]), then [`admit_verified`]. `Ok` is the frame
    /// admitted, `Err` the refusal it was answered with.
    fn admit_signed(req: &HttpRequest, now: i64, started: i64) -> Result<Audience, (i64, String)> {
        match verify_signed_request(req, now, started) {
            SignedRequestOutcome::Verified { resolved, key, .. } => {
                admit_verified(Listener::A2a, req, &format!("node `{resolved}`"), &key, now)
            }
            SignedRequestOutcome::Refused(code, message) => Err((code, message)),
            SignedRequestOutcome::Unsigned => panic!("the test frame carries its signature headers"),
        }
    }

    /// A unique nonce per test call — `NONCE_SETS` is a single process-wide
    /// static shared across every test in this binary (module doc), so two
    /// tests reusing the same literal nonce string could spuriously see
    /// each other's entries; a wall-clock-derived suffix keeps every test's
    /// nonces disjoint, the same "derive a unique string from pid+nanos"
    /// idiom this file's temp-dir helpers already use.
    fn unique_nonce(tag: &str) -> String {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        format!("{tag}-{}-{nanos}", std::process::id())
    }

    #[test]
    fn verify_signed_request_with_no_headers_at_all_is_unsigned() {
        let req = HttpRequest {
            method: "POST".into(),
            path: "/".into(),
            body: b"{}".to_vec(),
            bearer: None,
            signed_node: None,
            signed_timestamp: None,
            signed_nonce: None,
            signed_signature: None,
            signed_mesh: None,
        };
        assert_eq!(
            verify_signed_request(&req, 0, 0),
            SignedRequestOutcome::Unsigned,
            "no signature headers at all — the untouched, pre-P-P4 path"
        );
    }

    #[test]
    fn verify_signed_request_refuses_incomplete_headers_without_touching_the_node_registry() {
        // Claims a node (the `X-Aoide-Node` header present) but is missing
        // the other three — refused OUTRIGHT, never treated as "unsigned"
        // (the brief's "unsigned-but-claiming-paired refusal" case).
        let req = HttpRequest {
            method: "POST".into(),
            path: "/".into(),
            body: b"{}".to_vec(),
            bearer: None,
            signed_node: Some("box-b".to_string()),
            signed_timestamp: None,
            signed_nonce: None,
            signed_signature: None,
            signed_mesh: None,
        };
        match verify_signed_request(&req, 0, 0) {
            SignedRequestOutcome::Refused(code, msg) => {
                assert_eq!(code, -32007);
                assert!(msg.contains("must all be present together"));
            }
            other => panic!("expected Refused, got {other:?}"),
        }
    }

    #[test]
    fn verify_signed_request_round_trips_a_genuine_signature() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-verify-sig-ok-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::env::set_var("AOIDE_STATE_DIR", &root);

        let kp = setup_signed_node("box-b");
        let now = 1_800_000_000_i64;
        let req = signed_request(&kp, "box-b", "/", b"{\"a\":1}", now, &unique_nonce("ok"));
        assert_eq!(
            verify_signed_request(&req, now, 0),
            SignedRequestOutcome::Verified {
                resolved: "box-b".to_string(),
                key: kp.info().pubkey_hex.clone(),
                claimed: "box-b".to_string(),
                mesh: None,
            }
        );

        let _ = std::fs::remove_dir_all(&root);
        match saved {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    #[test]
    fn an_unknown_or_unverified_signer_is_refused_identically_to_a_bad_signature() {
        // #63 P-ID5's no-existence-oracle pin: a signature matching NO
        // verified node's stored key (empty registry, or a node registered
        // but never verified) refuses with the EXACT code+message a merely
        // tampered/bad signature earns — an outsider can never distinguish
        // "your key isn't registered here" from "your signature is wrong".
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-verify-sig-unknown-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::env::set_var("AOIDE_STATE_DIR", &root);

        let (kp, _) = aoide_storage::identity::load_or_mint().unwrap();
        let now = 1_800_000_000_i64;

        // Unknown key: nothing registered at all — the claimed name resolves
        // nothing because names resolve nothing; no stored key verifies.
        let req = signed_request(&kp, "nobody", "/", b"{}", now, &unique_nonce("unknown"));
        let unknown_key_refusal = match verify_signed_request(&req, now, 0) {
            SignedRequestOutcome::Refused(code, msg) => {
                assert_eq!(code, -32007);
                (code, msg)
            }
            other => panic!("expected Refused, got {other:?}"),
        };

        // Registered, correct pubkey, but never actually paired
        // (`verified: false`) — its key never enters the trial set.
        let mut unverified_node = fixture_node("box-c", "http://node/");
        unverified_node.pubkey = Some(kp.info().pubkey_hex);
        aoide_storage::node_store::save_nodes(&[unverified_node]).unwrap();
        let req2 = signed_request(&kp, "box-c", "/", b"{}", now, &unique_nonce("unverified"));
        let unverified_refusal = match verify_signed_request(&req2, now, 0) {
            SignedRequestOutcome::Refused(code, msg) => (code, msg),
            other => panic!("expected Refused, got {other:?}"),
        };

        // A genuinely VERIFIED node, but a tampered body — the plain
        // bad-signature refusal every case above must be indistinguishable
        // from.
        let mut verified_node = fixture_node("box-c", "http://node/");
        verified_node.verified = true;
        verified_node.pubkey = Some(kp.info().pubkey_hex);
        aoide_storage::node_store::save_nodes(&[verified_node]).unwrap();
        let mut req3 = signed_request(&kp, "box-c", "/", b"{\"real\":true}", now, &unique_nonce("bad-sig"));
        req3.body = b"{\"real\":false}".to_vec();
        let bad_sig_refusal = match verify_signed_request(&req3, now, 0) {
            SignedRequestOutcome::Refused(code, msg) => (code, msg),
            other => panic!("expected Refused, got {other:?}"),
        };

        assert_eq!(unknown_key_refusal, bad_sig_refusal, "unknown key vs bad signature must be indistinguishable");
        assert_eq!(unverified_refusal, bad_sig_refusal, "unverified node's key vs bad signature must be indistinguishable");

        let _ = std::fs::remove_dir_all(&root);
        match saved {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    #[test]
    fn verify_signed_request_refuses_a_tampered_body_or_path() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-verify-sig-tamper-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::env::set_var("AOIDE_STATE_DIR", &root);

        let kp = setup_signed_node("box-b");
        let now = 1_800_000_000_i64;

        // Signed over one body; the ARRIVING body is different — the digest
        // (and so the canonical string, and so the signature) no longer
        // matches.
        let mut req = signed_request(&kp, "box-b", "/", b"{\"real\":true}", now, &unique_nonce("tamper-body"));
        req.body = b"{\"real\":false}".to_vec();
        match verify_signed_request(&req, now, 0) {
            SignedRequestOutcome::Refused(code, msg) => {
                assert_eq!(code, -32007);
                assert!(msg.contains("signature verification failed"));
            }
            other => panic!("a tampered body must refuse, got {other:?}"),
        }

        // Signed for path "/"; the ARRIVING request line names a different
        // path — same signature, different canonical string.
        let mut req2 = signed_request(&kp, "box-b", "/", b"{}", now, &unique_nonce("tamper-path"));
        req2.path = "/other".to_string();
        match verify_signed_request(&req2, now, 0) {
            SignedRequestOutcome::Refused(code, _) => assert_eq!(code, -32007),
            other => panic!("a tampered path must refuse, got {other:?}"),
        }

        let _ = std::fs::remove_dir_all(&root);
        match saved {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    #[test]
    fn verify_signed_request_refuses_clock_skew_beyond_the_window_naming_both_timestamps() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-verify-sig-skew-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::env::set_var("AOIDE_STATE_DIR", &root);

        let kp = setup_signed_node("box-b");
        let signed_at = 1_800_000_000_i64;
        // The verifier's own "now" is 10 minutes later — well past the 120s
        // window behind it.
        let verifier_now = signed_at + 600;
        let req = signed_request(&kp, "box-b", "/", b"{}", signed_at, &unique_nonce("skew"));
        match verify_signed_request(&req, verifier_now, 0) {
            SignedRequestOutcome::Refused(code, msg) => {
                assert_eq!(code, -32008);
                assert!(msg.contains(&aoide_storage::time::iso_utc_from_epoch(signed_at)), "must name the request's OWN timestamp: {msg:?}");
                assert!(
                    msg.contains(&aoide_storage::time::iso_utc_from_epoch(verifier_now)),
                    "must name the verifier's OWN now too (both timestamps): {msg:?}"
                );
            }
            other => panic!("expected Refused, got {other:?}"),
        }

        let _ = std::fs::remove_dir_all(&root);
        match saved {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    #[test]
    fn a_replayed_nonce_inside_the_window_is_refused_on_its_second_admission() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-verify-sig-replay-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::env::set_var("AOIDE_STATE_DIR", &root);

        let kp = setup_signed_node("box-b");
        let now = 1_800_000_000_i64;
        let nonce = unique_nonce("replay");
        let req = signed_request(&kp, "box-b", "/", UNTAGGED_POLL, now, &nonce);

        assert_eq!(
            verify_signed_request(&req, now, 0),
            SignedRequestOutcome::Verified {
                resolved: "box-b".to_string(),
                key: kp.info().pubkey_hex.clone(),
                claimed: "box-b".to_string(),
                mesh: None,
            },
            "verification records nothing, so the same frame verifies again"
        );
        assert!(admit_signed(&req, now, 0).is_ok(), "the FIRST admission takes the nonce");

        // The same request, replayed: same nonce, still inside the window. Its
        // signature is still perfectly valid, but its nonce is already taken.
        match admit_signed(&req, now, 0) {
            Err((code, msg)) => {
                assert_eq!(code, -32009);
                assert!(msg.contains("replay"));
            }
            other => panic!("a replayed nonce must refuse on its SECOND admission, got {other:?}"),
        }

        let _ = std::fs::remove_dir_all(&root);
        match saved {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    #[test]
    fn a_signature_resolves_the_node_whose_key_signed_even_when_the_name_header_claims_another() {
        // #63 P-ID5's core inversion: `box-a` holds the signing key, the
        // wire claims `box-b` (a different, genuinely registered node) —
        // resolution follows the KEY, the claimed name survives only as
        // attribution, and the mismatch produces a drift audit line.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-verify-by-key-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::env::set_var("AOIDE_STATE_DIR", &root);

        let (kp, _) = aoide_storage::identity::load_or_mint().unwrap();
        let mut box_a = fixture_node("box-a", "http://node-a/");
        box_a.verified = true;
        box_a.pubkey = Some(kp.info().pubkey_hex);
        let mut box_b = fixture_node("box-b", "http://node-b/");
        box_b.verified = true;
        // Well-formed but unrelated key material — never verifies anything.
        box_b.pubkey = Some("aa".repeat(32));
        aoide_storage::node_store::save_nodes(&[box_a, box_b]).unwrap();

        let now = 1_800_000_000_i64;
        let req = signed_request(&kp, "box-b", "/", b"{}", now, &unique_nonce("by-key"));
        match verify_signed_request(&req, now, 0) {
            SignedRequestOutcome::Verified { resolved, claimed, .. } => {
                assert_eq!(resolved, "box-a", "the record whose stored key verifies IS the caller");
                assert_eq!(claimed, "box-b", "the wire's claim rides along for attribution");
                let detail = attribution_drift_detail(&claimed, &resolved).expect("a claimed-vs-resolved mismatch must produce a drift audit line");
                assert!(detail.contains("box-a") && detail.contains("box-b"), "the drift line names both: {detail:?}");
            }
            other => panic!("expected Verified resolving box-a, got {other:?}"),
        }

        let _ = std::fs::remove_dir_all(&root);
        match saved {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    #[test]
    fn a_locally_renamed_node_still_authenticates_by_its_key() {
        // The defect that motivated this phase (PAIRING.md's former
        // known-limitation note): the operator renamed the record, the far
        // end still claims its old self name — the key hasn't changed, so
        // authentication must not break.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-verify-renamed-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::env::set_var("AOIDE_STATE_DIR", &root);

        let kp = setup_signed_node("renamed-node");
        let now = 1_800_000_000_i64;
        // The wire still claims the name from before the local rename —
        // registered nowhere.
        let req = signed_request(&kp, "old-name", "/", b"{}", now, &unique_nonce("renamed"));
        match verify_signed_request(&req, now, 0) {
            SignedRequestOutcome::Verified { resolved, claimed, .. } => {
                assert_eq!(resolved, "renamed-node");
                assert_eq!(claimed, "old-name");
            }
            other => panic!("a renamed node's signature must still resolve it, got {other:?}"),
        }

        let _ = std::fs::remove_dir_all(&root);
        match saved {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    #[test]
    fn shared_key_records_take_the_exact_name_tiebreak_or_refuse_ambiguous() {
        // Collision semantics (#63 P-ID5, pinned in CONTRACTS §6): two
        // verified records CAN share a pubkey (`upsert_paired_node` matches
        // by name only — the same remote instance paired under two names).
        // Both hold the same PROVEN key, so the claimed name may pick among
        // them (equal security, possibly different allows); with no
        // exact-name match, refusing beats guessing which grants apply.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-verify-shared-key-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::env::set_var("AOIDE_STATE_DIR", &root);

        let (kp, _) = aoide_storage::identity::load_or_mint().unwrap();
        let pubkey = kp.info().pubkey_hex;
        let mut twin_a = fixture_node("twin-a", "http://node-a/");
        twin_a.verified = true;
        twin_a.pubkey = Some(pubkey.clone());
        let mut twin_b = fixture_node("twin-b", "http://node-b/");
        twin_b.verified = true;
        twin_b.pubkey = Some(pubkey);
        aoide_storage::node_store::save_nodes(&[twin_a, twin_b]).unwrap();

        let now = 1_800_000_000_i64;
        // Claimed name matches one twin exactly — that one wins.
        let req = signed_request(&kp, "twin-b", "/", b"{}", now, &unique_nonce("twin-exact"));
        match verify_signed_request(&req, now, 0) {
            SignedRequestOutcome::Verified { resolved, claimed, .. } => {
                assert_eq!(resolved, "twin-b");
                assert_eq!(claimed, "twin-b");
            }
            other => panic!("an exact-name match among shared-key records must resolve it, got {other:?}"),
        }

        // Claimed name matches neither — refused as ambiguous, taught.
        let req2 = signed_request(&kp, "twin-c", "/", b"{}", now, &unique_nonce("twin-none"));
        match verify_signed_request(&req2, now, 0) {
            SignedRequestOutcome::Refused(code, msg) => {
                assert_eq!(code, -32007);
                assert!(msg.contains("ambiguous"), "must refuse as ambiguous, never pick a record arbitrarily: {msg:?}");
            }
            other => panic!("shared-key records with no exact-name match must refuse, got {other:?}"),
        }

        let _ = std::fs::remove_dir_all(&root);
        match saved {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    #[test]
    fn a_nonce_replay_is_caught_across_shared_key_records() {
        // The nonce sets key on the verifying PUBKEY, not any name
        // (`NONCE_SETS`'s doc): `X-Aoide-Node` is outside the canonical
        // string, so a captured request replayed under a shared-key twin's
        // name still lands on the same cache key and refuses.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-verify-twin-replay-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::env::set_var("AOIDE_STATE_DIR", &root);

        let (kp, _) = aoide_storage::identity::load_or_mint().unwrap();
        let pubkey = kp.info().pubkey_hex;
        let mut twin_a = fixture_node("twin-a", "http://node-a/");
        twin_a.verified = true;
        twin_a.pubkey = Some(pubkey.clone());
        let mut twin_b = fixture_node("twin-b", "http://node-b/");
        twin_b.verified = true;
        twin_b.pubkey = Some(pubkey);
        aoide_storage::node_store::save_nodes(&[twin_a, twin_b]).unwrap();

        let now = 1_800_000_000_i64;
        let nonce = unique_nonce("twin-replay");
        let req = signed_request(&kp, "twin-a", "/", UNTAGGED_POLL, now, &nonce);
        assert!(admit_signed(&req, now, 0).is_ok(), "first use must be admitted");

        // The same request re-sent claiming the twin: the name header is
        // OUTSIDE the canonical string, so the identical method/path/
        // timestamp/nonce/body yields the byte-identical (deterministic
        // ed25519) signature, exactly what a captured-and-relabeled replay
        // carries. Same key, same nonce: still a replay.
        let replayed = signed_request(&kp, "twin-b", "/", UNTAGGED_POLL, now, &nonce);
        match admit_signed(&replayed, now, 0) {
            Err((code, _)) => assert_eq!(code, -32009),
            other => panic!("a replay under the twin's name must still be caught, got {other:?}"),
        }

        let _ = std::fs::remove_dir_all(&root);
        match saved {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    #[test]
    fn attribution_drift_detail_fires_only_on_a_mismatch_and_names_both() {
        assert_eq!(attribution_drift_detail("box-a", "box-a"), None, "agreement logs nothing");
        let detail = attribution_drift_detail("claimed-name", "resolved-name").expect("a mismatch must produce the audit detail");
        assert!(detail.contains("claimed-name") && detail.contains("resolved-name"), "both names in the line: {detail:?}");
        assert!(detail.contains("X-Aoide-Node"), "names the header the claim rode in on: {detail:?}");
    }

    /// Each key's nonces live in its own set. A key at its cap is refused Busy,
    /// and that refusal never evicts another key's recorded nonce, so J's nonce
    /// stays a replay however many nonces K records.
    #[test]
    fn a_flood_on_one_key_never_makes_another_keys_recorded_nonce_replayable() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let j = unique_nonce("flood-j");
        let k = unique_nonce("flood-k");
        let now = 1_800_000_000_i64;
        let cap = NONCE_CAP_DEFAULT;
        assert_eq!(nonce_check(&j, "j-once", now, now, cap), NonceCheck::Fresh, "J's first nonce is recorded");
        for i in 0..cap {
            assert_eq!(nonce_check(&k, &format!("k-{i}"), now, now, cap), NonceCheck::Fresh);
        }
        assert_eq!(nonce_check(&k, "k-over", now, now, cap), NonceCheck::Busy, "K is at its cap");
        assert_eq!(nonce_check(&j, "j-once", now, now, cap), NonceCheck::Replay, "K's flood never evicted J's recorded nonce");
    }

    /// A nonce is remembered only for its own window. Once the window of the
    /// request that used it has passed, a freshly signed request reusing that
    /// nonce value is not a replay, and the OLD bytes are refused by the clock.
    #[test]
    fn a_nonce_whose_window_has_passed_is_forgotten_and_the_old_bytes_are_refused_by_the_clock() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, saved) = adapter_root("nonce-expiry");
        let kp = setup_signed_node("box-b");
        let then = 1_800_000_000_i64;
        let skew = aoide_storage::wire_auth::signature_skew_secs();
        let nonce = unique_nonce("expiry");
        let first = signed_request(&kp, "box-b", "/", UNTAGGED_POLL, then, &nonce);
        assert!(admit_signed(&first, then, 0).is_ok(), "the first use is admitted");

        let later = then + skew + 1;
        let fresh = signed_request(&kp, "box-b", "/", UNTAGGED_POLL, later, &nonce);
        assert!(admit_signed(&fresh, later, 0).is_ok(), "the window has passed, so the nonce no longer blocks a new frame");
        assert!(
            matches!(verify_signed_request(&first, later, 0), SignedRequestOutcome::Refused(-32008, _)),
            "the old bytes are refused by the timestamp, whatever the cache remembers"
        );
        adapter_cleanup(&root, saved);
    }

    /// A frame minted before this door started is refused though the skew window
    /// would admit it: the door holds no replay memory from before its own start,
    /// so a frame from before a restart cannot be replayed into the new process.
    /// Frames stamped up to the start bound (start + 5s) are refused, and a frame
    /// stamped after it is accepted.
    #[test]
    fn a_frame_minted_before_the_door_started_is_refused_and_one_after_the_bound_is_accepted() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, saved) = adapter_root("start-gate");
        let kp = setup_signed_node("box-b");
        let started = 1_800_000_000_i64;
        let early = signed_request(&kp, "box-b", "/", b"{}", started - 1, &unique_nonce("start-early"));
        match verify_signed_request(&early, started, started) {
            SignedRequestOutcome::Refused(-32008, message) => {
                assert!(message.contains(&aoide_storage::time::iso_utc_from_epoch(started - 1)), "{message}");
                assert!(message.contains(&aoide_storage::time::iso_utc_from_epoch(started)), "{message}");
            }
            other => panic!("a frame minted before the door started is refused: {other:?}"),
        }
        let same_second = signed_request(&kp, "box-b", "/", b"{}", started, &unique_nonce("start-same"));
        assert!(
            matches!(verify_signed_request(&same_second, started, started), SignedRequestOutcome::Refused(-32008, _)),
            "a frame stamped in the start second is refused too: the start is whole seconds"
        );
        let at_bound = signed_request(&kp, "box-b", "/", b"{}", started + 5, &unique_nonce("start-bound"));
        assert!(
            matches!(verify_signed_request(&at_bound, started + 5, started), SignedRequestOutcome::Refused(-32008, _)),
            "a frame stamped at start + 5s is refused: the bound is inclusive"
        );
        let late = signed_request(&kp, "box-b", "/", b"{}", started + 6, &unique_nonce("start-late"));
        assert!(
            matches!(verify_signed_request(&late, started + 6, started), SignedRequestOutcome::Verified { .. }),
            "a frame stamped after start + 5s is accepted"
        );
        adapter_cleanup(&root, saved);
    }

    /// The ahead bound is fixed at five seconds whatever the knob says. A frame five
    /// seconds ahead of this door's clock is accepted; one six seconds ahead is
    /// refused, naming both times, the bound, and that the caller's clock is fast.
    #[test]
    fn a_frame_five_seconds_ahead_is_accepted_and_one_six_seconds_ahead_is_refused_as_a_fast_clock() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, saved) = adapter_root("ahead-bound");
        let kp = setup_signed_node("box-b");
        let now = 1_800_000_000_i64;
        let five = signed_request(&kp, "box-b", "/", b"{}", now + 5, &unique_nonce("ahead-five"));
        assert!(
            matches!(verify_signed_request(&five, now, 0), SignedRequestOutcome::Verified { .. }),
            "a frame five seconds ahead is inside the bound"
        );
        let six = signed_request(&kp, "box-b", "/", b"{}", now + 6, &unique_nonce("ahead-six"));
        match verify_signed_request(&six, now, 0) {
            SignedRequestOutcome::Refused(-32008, message) => {
                assert!(message.contains("fast"), "the caller's clock is fast: {message}");
                assert!(message.contains("5s"), "names the bound: {message}");
                assert!(message.contains(&aoide_storage::time::iso_utc_from_epoch(now + 6)), "names the frame's time: {message}");
                assert!(message.contains(&aoide_storage::time::iso_utc_from_epoch(now)), "names this door's now: {message}");
            }
            other => panic!("a frame six seconds ahead is refused, got {other:?}"),
        }
        adapter_cleanup(&root, saved);
    }

    /// Case A: a frame an earlier process accepted, stamped as far ahead as the
    /// bound allows, is refused by the restarted door. Accepted at `accepted`, it
    /// is stamped at most `accepted + 5`; a restart at `restart >= accepted` refuses
    /// every stamp up to `restart + 5`, so the same bytes stay refused after it.
    #[test]
    fn a_frame_accepted_before_a_restart_is_refused_after_it_at_the_full_ahead_bound() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, saved) = adapter_root("restart-case-a");
        let kp = setup_signed_node("box-b");
        let accepted = 1_800_000_000_i64;
        let frame = signed_request(&kp, "box-b", "/", b"{}", accepted + 5, &unique_nonce("restart-a"));
        assert!(
            matches!(verify_signed_request(&frame, accepted, 0), SignedRequestOutcome::Verified { .. }),
            "the earlier process accepts it, five seconds ahead"
        );
        let restart = accepted;
        assert!(
            matches!(verify_signed_request(&frame, restart, restart), SignedRequestOutcome::Refused(-32008, _)),
            "the restarted door refuses the same bytes in the restart second"
        );
        assert!(
            matches!(verify_signed_request(&frame, restart + 3, restart), SignedRequestOutcome::Refused(-32008, _)),
            "and still refuses them a few seconds into the new process"
        );
        adapter_cleanup(&root, saved);
    }

    /// Case B: a frame minted before a door's start and never delivered anywhere is
    /// refused once its stamp is within the start bound, whatever its signer meant.
    #[test]
    fn a_frame_minted_before_a_start_and_never_delivered_is_refused_at_the_bound() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, saved) = adapter_root("minted-case-b");
        let kp = setup_signed_node("box-b");
        let started = 1_800_000_000_i64;
        let minted_before = signed_request(&kp, "box-b", "/", b"{}", started - 1 + 5, &unique_nonce("minted-b"));
        assert!(
            matches!(verify_signed_request(&minted_before, started + 1, started), SignedRequestOutcome::Refused(-32008, _)),
            "a frame stamped at most start + 5s is refused, though it was never delivered before the start"
        );
        adapter_cleanup(&root, saved);
    }

    /// A key whose set is full is refused busy over HTTP 503, unsigned, and
    /// records nothing. The set is filled to the default cap through
    /// `nonce_check`, not the knob, so this test never changes the cap for a test
    /// running beside it.
    #[test]
    fn a_key_at_its_cap_answers_busy_over_http_503_and_the_answer_is_unsigned() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, saved) = adapter_root("nonce-busy-socket");
        let audit = root.join("log");
        let port = free_loopback_port();
        raise_a2a_door(port, &audit);

        let kp = setup_signed_node_with_allows("box-b", &["message"]);
        let key = kp.info().pubkey_hex.clone();
        let now = epoch_now();
        for i in 0..NONCE_CAP_DEFAULT {
            assert_eq!(nonce_check(&key, &format!("fill-{i}"), now, now, NONCE_CAP_DEFAULT), NonceCheck::Fresh);
        }

        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "aoide/graphSummary", "params": { "to": key } }).to_string();
        let nonce = unique_nonce("busy");
        let answer = send_raw(
            port,
            &raw_request("127.0.0.1", "POST", "/", body.as_bytes(), Some((&kp, "box-b", now, nonce.as_str())), None),
        );
        assert!(answer.starts_with("HTTP/1.1 503"), "a key at its cap is refused busy: {answer}");
        let answer_body: Value = serde_json::from_str(answer.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(answer_body["error"]["code"], -32013, "{answer_body}");
        assert!(
            aoide_test_support::raw_header(&answer, aoide_storage::wire_auth::HEADER_ANSWER_SIGNATURE).is_none(),
            "a busy refusal is unsigned"
        );
        adapter_cleanup(&root, saved);
    }

    /// A full key refuses its next new nonce as busy and records nothing, and
    /// that nonce goes through once an entry's window has closed. A replay at the
    /// cap is still a replay: the recorded-nonce check comes before the cap.
    #[test]
    fn a_full_key_refuses_busy_until_an_entry_expires_and_a_replay_at_the_cap_is_still_a_replay() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let tag = unique_nonce("busy-unit");
        let skew = aoide_storage::wire_auth::signature_skew_secs();
        let now = 1_800_000_000_i64;
        assert_eq!(nonce_check(&tag, "n1", now, now, 2), NonceCheck::Fresh);
        assert_eq!(nonce_check(&tag, "n2", now + 1, now + 1, 2), NonceCheck::Fresh);
        assert_eq!(nonce_check(&tag, "n3", now + 2, now + 2, 2), NonceCheck::Busy, "a full key refuses a new nonce");
        assert_eq!(nonce_check(&tag, "n3", now + 2, now + 2, 2), NonceCheck::Busy, "and records nothing, so it stays new");
        assert_eq!(nonce_check(&tag, "n2", now + 2, now + 2, 2), NonceCheck::Replay, "a recorded nonce is a replay at the cap");
        assert_eq!(
            nonce_check(&tag, "n3", now + skew + 1, now + skew + 1, 2),
            NonceCheck::Fresh,
            "n1's window has closed, so its slot is free"
        );
    }

    #[test]
    fn the_per_key_cap_is_the_knob_when_it_parses_to_a_positive_integer_and_the_default_otherwise() {
        assert_eq!(parse_nonce_cap(None), NONCE_CAP_DEFAULT);
        assert_eq!(parse_nonce_cap(Some("")), NONCE_CAP_DEFAULT);
        assert_eq!(parse_nonce_cap(Some("   ")), NONCE_CAP_DEFAULT);
        assert_eq!(parse_nonce_cap(Some("lots")), NONCE_CAP_DEFAULT);
        assert_eq!(parse_nonce_cap(Some("0")), NONCE_CAP_DEFAULT);
        assert_eq!(parse_nonce_cap(Some("-3")), NONCE_CAP_DEFAULT);
        assert_eq!(parse_nonce_cap(Some(" 7 ")), 7);
    }

    /// The knob is read from `AOIDE_NONCE_CAP`, and falls back to the default for every
    /// value it cannot use: unset, blank, non-numeric and zero.
    #[test]
    fn nonce_cap_reads_the_knob_and_falls_back_to_the_default_for_unusable_values() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_NONCE_CAP").ok();
        let cases: [(Option<&str>, usize); 6] = [
            (None, NONCE_CAP_DEFAULT),
            (Some(""), NONCE_CAP_DEFAULT),
            (Some("   "), NONCE_CAP_DEFAULT),
            (Some("lots"), NONCE_CAP_DEFAULT),
            (Some("0"), NONCE_CAP_DEFAULT),
            (Some(" 7 "), 7),
        ];
        for (raw, expected) in cases {
            match raw {
                Some(value) => std::env::set_var("AOIDE_NONCE_CAP", value),
                None => std::env::remove_var("AOIDE_NONCE_CAP"),
            }
            assert_eq!(nonce_cap(), expected, "AOIDE_NONCE_CAP = {raw:?}");
        }
        match saved {
            Some(value) => std::env::set_var("AOIDE_NONCE_CAP", value),
            None => std::env::remove_var("AOIDE_NONCE_CAP"),
        }
    }

    #[test]
    fn nonce_check_is_blind_to_letter_case_and_surrounding_space() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        // The signature folds the nonce (`canonical_string`), so the sets must
        // fold it too: every spelling of one nonce is one nonce, and so is every
        // spelling of one key.
        let tag = unique_nonce("case-node");
        let now = 1_800_000_000_i64;
        let cap = NONCE_CAP_DEFAULT;
        assert_eq!(nonce_check(&tag, "AbC123", now, now, cap), NonceCheck::Fresh, "the first spelling is recorded");
        assert_eq!(nonce_check(&tag, "abc123", now, now, cap), NonceCheck::Replay, "the lower-case spelling is the same nonce");
        assert_eq!(nonce_check(&tag, " ABC123 ", now, now, cap), NonceCheck::Replay, "and surrounding space is not a new nonce");
        assert_eq!(
            nonce_check(&tag.to_uppercase(), "abc123", now, now, cap),
            NonceCheck::Replay,
            "and the key's case does not split the namespace"
        );
    }

    // ── P-P5b (`node spawn`) — the real signed wire round trip ──────────────
    //
    // Both tests below build the SPAWN-SHAPED body via `aoide_client::wire::
    // build_message_send_body(text, id, None, None)` — the exact function
    // `aoide-client::commands::handle_node_spawn` calls — and a REAL ed25519
    // signature over it (`signed_request`, the same helper the P-P4 tests
    // above use), so this is a genuine client-body + real-crypto round trip,
    // not a hand-typed guess at either shape.

    /// "A signed node spawn is ADMITTED" (PAIRING.md's own live-gate
    /// wording) — proven up to, but never through, `do_spawn`'s real
    /// OS-level process spawn: `verify_signed_request` really verifies the
    /// signature, and `spawn_admitted` — fed the EXACT resolution
    /// `message_send` itself performs when `signed_caller` is `Some`
    /// (the two-line `nodes.iter().find(name).map(|p| (p,
    /// NodeRung::Signature))`) — really admits it. This file's own
    /// established discipline (see the doc comment atop the "Spawn gate
    /// table" section above) is that NO test here drives
    /// `do_spawn`'s real process spawn, because `std::env::current_exe()`
    /// inside a `cargo test` binary is the TEST binary, not a real `aoide`
    /// — calling `message_send`'s Spawn arm all the way through on the
    /// ADMITTED path would do exactly that. `spawn_admitted` returning
    /// `true` from a REAL verified signature is precisely the boundary
    /// `do_spawn` would be invoked from (`message_send`'s own `if
    /// spawn_admitted(resolved_node) { do_spawn(...) }`); proving up to
    /// it is this suite's documented choice, not a gap.
    #[test]
    fn node_spawn_signed_and_allowed_is_admitted_up_to_the_do_spawn_boundary() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-node-spawn-admitted-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::env::set_var("AOIDE_STATE_DIR", &root);

        let kp = setup_signed_node_with_allows("yomi-strix", &["read", "spawn"]);
        let body = aoide_client::wire::build_message_send_body("status check please", "mid-spawn-1", None, None, None);
        let body_bytes = serde_json::to_vec(&body).unwrap();
        let now = 1_800_000_000_i64;
        let req = signed_request(&kp, "yomi-strix", "/", &body_bytes, now, &unique_nonce("admit"));

        let signed_caller = match verify_signed_request(&req, now, 0) {
            SignedRequestOutcome::Verified { resolved, .. } => resolved,
            other => panic!("expected a REAL genuine signature to verify, got {other:?}"),
        };
        assert_eq!(signed_caller, "yomi-strix");

        // The exact resolution `message_send` performs when `signed_caller`
        // is `Some` — the SOLE resolution, no fallthrough to addr/token (P-P4).
        let nodes = aoide_storage::node_store::load_nodes();
        let resolved = nodes
            .iter()
            .find(|p| p.name == signed_caller)
            .map(|p| (p, aoide_storage::node_store::NodeRung::Signature));
        assert!(spawn_admitted(resolved, &grant_for(&nodes[0], "home")), "a genuinely signed, paired, spawn-allowed node must be ADMITTED");
        // The exact name that would flow into `do_spawn`'s `node_name` arg,
        // and hence into `origin: format!("node:{name}")` — the wire-verified
        // name, not a guess.
        assert_eq!(resolved.unwrap().0.name, "yomi-strix");

        let _ = std::fs::remove_dir_all(&root);
        match saved {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    /// The live-gate's other half (PAIRING.md: "a spawn refused for a node
    /// with spawn revoked") — same real signature round trip as above, but
    /// SAFE to drive all the way through the REAL `message_send` (not just
    /// `spawn_admitted`): a refusal never reaches `do_spawn`, matching this
    /// file's "REFUSAL branches only" precedent for calling `message_send`
    /// directly.
    #[test]
    fn node_spawn_revoked_is_refused_through_the_real_message_send_wire_path() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-node-spawn-revoked-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::set_var("AOIDE_STAGE_DIR", root.join("stage"));

        // Paired, verified, spawn REVOKED — `allows` carries only "read".
        let kp = setup_signed_node_with_allows("yomi-strix", &["read"]);
        let body = aoide_client::wire::build_message_send_body("status check please", "mid-spawn-2", None, None, None);
        let body_bytes = serde_json::to_vec(&body).unwrap();
        let now = 1_800_000_000_i64;
        let req = signed_request(&kp, "yomi-strix", "/", &body_bytes, now, &unique_nonce("revoked"));
        let (signed_name, signed_key) = match verify_signed_request(&req, now, 0) {
            SignedRequestOutcome::Verified { resolved, key, .. } => (resolved, key),
            other => panic!("expected a REAL genuine signature to verify, got {other:?}"),
        };
        let caller = SignedCaller { name: &signed_name, key: &signed_key, mesh: None };

        let err = message_send(
            &body["params"],
            &root.join("log"),
            "claude",
            "",
            ConnOrigin::Loopback,
            "",
            None,
            Some(caller),
        )
        .unwrap_err();
        assert_eq!(err.0, -32006, "genuinely signed and paired, but `spawn` was revoked from allows");
        assert!(err.1.contains("node allow"), "taught error must name the exact fix: {}", err.1);

        let _ = std::fs::remove_dir_all(&root);
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
    }

    /// P-RSA S3: a signed caller's MALFORMED claim is `-32602`, refused before
    /// any spawn admission is even consulted — the claim is inside the body
    /// digest, so a caller that signed it meant it, and a silent drop would
    /// hide a client bug. Driven through the REAL `message_send` on a refusal
    /// branch only (this file's documented discipline: no test here may reach
    /// `do_spawn`'s real process spawn, which on the admitted path would
    /// exec the TEST binary as an agent).
    #[test]
    fn a_signed_callers_malformed_from_claim_is_refused_with_minus_32602() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-from-claim-bad-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::set_var("AOIDE_STAGE_DIR", root.join("stage"));

        // Genuinely signed, verified, spawn-allowed: nothing but the claim is
        // wrong, so the -32602 below is the claim's own doing.
        let kp = setup_signed_node_with_allows("yomi-strix", &["read", "spawn"]);
        let body =
            aoide_client::wire::build_message_send_body("status check please", "mid-bad-1", None, Some("not/a/session"), None);
        let body_bytes = serde_json::to_vec(&body).unwrap();
        let now = 1_800_000_000_i64;
        let req = signed_request(&kp, "yomi-strix", "/", &body_bytes, now, &unique_nonce("bad-from"));
        let (signed_name, signed_key) = match verify_signed_request(&req, now, 0) {
            SignedRequestOutcome::Verified { resolved, key, .. } => (resolved, key),
            other => panic!("expected a REAL genuine signature to verify, got {other:?}"),
        };

        let audit_log = root.join("log");
        let err = message_send(
            &body["params"],
            &audit_log,
            "claude",
            "",
            ConnOrigin::Loopback,
            "",
            None,
            Some(SignedCaller { name: &signed_name, key: &signed_key, mesh: None }),
        )
        .unwrap_err();
        assert_eq!(err.0, -32602, "a signed caller's malformed claim is invalid params, never a silent drop");
        assert!(err.1.contains("aoide/from"), "the refusal names the key: {}", err.1);
        // ...and the same malformed value UNSIGNED is not an error at all —
        // there is no claim to validate off a weaker rung. Loopback with no
        // token resolves no node, so the spawn arm refuses: the refusal is
        // -32006, provably not -32602.
        let unsigned = message_send(
            &body["params"],
            &audit_log,
            "claude",
            "",
            ConnOrigin::Loopback,
            "",
            None,
            None,
        )
        .unwrap_err();
        assert_eq!(unsigned.0, -32006, "an unsigned caller's bad claim is ignored, not validated: {}", unsigned.1);

        let log = std::fs::read_to_string(&audit_log).unwrap_or_default();
        assert!(
            log.contains("\"status\":\"ignored-unsigned-from\""),
            "the ignore is audited, one line, by name: {log}"
        );
        assert!(
            !log.contains("\"status\":\"ok\""),
            "neither call ever spawned: {log}"
        );

        let _ = std::fs::remove_dir_all(&root);
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
    }

    /// P-RSA S3 / MED-3: the claim belongs to the SPAWN side. The Inject arm
    /// consumes it nowhere — S5 is what threads an attested sender into an
    /// inject — so a malformed claim on an inject-shaped request must take
    /// exactly the path an ABSENT claim takes (the send is queued and answered
    /// with a `submitted` Task), never the door's `-32602`. Driven through the
    /// REAL `message_send` with a genuinely signed caller and a session that is
    /// conductable right now, because that is where the ordering bug lived: the
    /// refusal used to fire between node resolution and the uniform-response
    /// guard, ahead of the arm that never reads the value.
    #[test]
    fn a_malformed_claim_on_an_inject_shaped_request_is_ignored_not_refused() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-inject-bad-from-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        std::env::remove_var("AOIDE_CONDUCT_AUTOGATE");
        std::env::remove_var("AOIDE_SESSION_ID");

        let kp = setup_signed_node_with_allows("yomi-strix", &["read"]);

        // A session that IS conductable NOW (registered + its socket on disk),
        // so `decide_send_action` classifies this request Inject (contextId
        // present, spawn not asked) instead of Spawn or Error.
        let id = "inject-tgt";
        let socket = aoide_conduct::graph::conduct_socket_path(id);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        let sf = SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![conductable_session(id, &socket)],
        };
        write_stage(&sessions_path(), &sf).unwrap();

        let body = aoide_client::wire::build_message_send_body(
            "status check please",
            "mid-inject-bad",
            Some(id),
            Some("not/a/session"),
            None,
        );
        let body_bytes = serde_json::to_vec(&body).unwrap();
        let now = 1_800_000_000_i64;
        let req = signed_request(&kp, "yomi-strix", "/", &body_bytes, now, &unique_nonce("inject-bad-from"));
        let (signed_name, signed_key) = match verify_signed_request(&req, now, 0) {
            SignedRequestOutcome::Verified { resolved, key, .. } => (resolved, key),
            other => panic!("expected a REAL genuine signature to verify, got {other:?}"),
        };

        let audit_log = root.join("log");
        let result = message_send(
            &body["params"],
            &audit_log,
            "",
            "",
            ConnOrigin::Loopback,
            "",
            None,
            Some(SignedCaller { name: &signed_name, key: &signed_key, mesh: None }),
        );
        let task = result.unwrap_or_else(|e| {
            panic!("a malformed claim on the Inject arm must not be refused: {e:?}")
        });
        assert_eq!(task["id"], id, "the Inject arm's Task for the session it targeted");

        // The send took the queued path — a signed caller is never delivered
        // straight through — attributed to the NODE that verified,
        // never to the malformed claim, which nothing on this arm read.
        let pending: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(stage.join("pending.json")).unwrap()).unwrap();
        let entries = pending["pending"].as_array().unwrap();
        assert_eq!(entries.len(), 1, "{pending}");
        assert_eq!(entries[0]["from"], "node:yomi-strix");

        // The weaker-rung ignore line did not fire: this request DID verify by
        // a node's key, so the claim was not ignored for its rung — it simply
        // is not consumed on this arm yet.
        let log = std::fs::read_to_string(&audit_log).unwrap_or_default();
        assert!(
            !log.contains("ignored-unsigned-from"),
            "a signed caller is not an unsigned ignore: {log}"
        );

        let _ = std::fs::remove_dir_all(&root);
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
    }

    /// P-RSA S3 / LOW-1: the stamp's `key` is the key that VERIFIED, not a
    /// second read of the registry by name. Hand-written `nodes.json` only —
    /// `insert_node` refuses duplicate names — but the decoupling is real: two
    /// records sharing a name, the IMPOSTOR listed FIRST with a different
    /// stored key. `verify_signed_request` resolves the signer by KEY, so it
    /// returns the second record; a stamp that re-found the record by name
    /// would take the first record's pubkey and stamp a `remoteParent.key`
    /// that never verified anything.
    #[test]
    fn the_remote_parent_keys_on_the_verifying_key_not_a_second_name_lookup() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-remote-parent-twin-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::env::set_var("AOIDE_STATE_DIR", &root);

        let kp = setup_signed_node_with_allows("yomi-strix", &["read", "spawn"]);
        let real = aoide_storage::node_store::load_nodes().remove(0);
        // The twin: same name, a different (never-verifying) stored key, placed
        // FIRST in the file so a find-by-name lands on it.
        let mut impostor = real.clone();
        impostor.url = "http://impostor/".to_string();
        impostor.pubkey = Some("cd".repeat(32));
        aoide_storage::node_store::save_nodes(&[impostor, real]).unwrap();

        let body = aoide_client::wire::build_message_send_body(
            "status check please",
            "mid-twin",
            None,
            Some("conduct-1-2"),
            None,
        );
        let body_bytes = serde_json::to_vec(&body).unwrap();
        let now = 1_800_000_000_i64;
        let req = signed_request(&kp, "yomi-strix", "/", &body_bytes, now, &unique_nonce("rp-twin"));
        let (signed_name, signed_key) = match verify_signed_request(&req, now, 0) {
            SignedRequestOutcome::Verified { resolved, key, .. } => (resolved, key),
            other => panic!("expected a REAL genuine signature to verify, got {other:?}"),
        };
        assert_eq!(signed_key, kp.info().pubkey_hex, "the key trial resolved the REAL twin");

        // `message_send`'s own resolution + claim, verbatim: the name lookup
        // (which DOES land on the impostor, since names are not identity) and
        // the rung gate, then the stamp — off the verified identity.
        let nodes = aoide_storage::node_store::load_nodes();
        let resolved = nodes
            .iter()
            .find(|p| p.name == signed_name)
            .map(|p| (p, aoide_storage::node_store::NodeRung::Signature));
        assert_eq!(
            resolved.expect("the name exists").0.pubkey.as_deref(),
            Some("cd".repeat(32).as_str()),
            "the find-by-name really does land on the impostor — the point of this test",
        );
        let stamped = claimed_remote_parent(
            claimable_caller(resolved, Some(SignedCaller { name: &signed_name, key: &signed_key, mesh: None })),
            Some("conduct-1-2"),
        )
        .unwrap()
        .expect("a verified signature plus a valid claim stamps a parent");
        assert_eq!(
            stamped.key,
            kp.info().pubkey_hex,
            "the stamped key is the one that verified, never the name-twin's stored key"
        );

        let _ = std::fs::remove_dir_all(&root);
        match saved {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    #[test]
    fn message_send_spawn_refuses_a_paired_node_whose_allows_lacks_spawn() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-spawn-denied-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", root.join("stage"));
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));

        let mut node = fixture_node("denied-node", "http://10.0.0.5:8710/");
        node.verified = true;
        node.grants = aoide_storage::node_store::grants_in("home", &["read"]); // spawn explicitly absent (revoked or never granted).
        aoide_storage::node_store::save_nodes(&[node]).unwrap();

        let params = json!({ "message": { "parts": [{ "kind": "text", "text": "hi" }] } });
        let remote_origin = ConnOrigin::Remote("10.0.0.5".parse().unwrap());
        let err = message_send(
            &params,
            &root.join("log"),
            "claude",
            "",
            remote_origin,
            "",
            None,
            None,
        )
        .unwrap_err();
        assert_eq!(err.0, -32006, "resolved to a REAL node, but `spawn` is not in its allows");

        let _ = std::fs::remove_dir_all(&root);
        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
    }

    #[test]
    fn message_send_spawn_refuses_an_addr_resolved_paired_and_allowed_node_with_no_token_file_configured() {
        // The gap the 2026-08-25 review finding closed: a node that IS
        // paired AND has `spawn` in `allows` — everything decision 6
        // originally asked for — but has no `token_file` set, so it can
        // ONLY resolve via the address rung. Before the narrowing this
        // would have reached `do_spawn`; behind any NAT/reverse-proxy
        // deployment a shared source address is exactly the unsigned
        // signal that must never itself authorize launching a process
        // attributed to this node.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-spawn-addr-only-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", root.join("stage"));
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));

        let mut node = fixture_node("addr-only-node", "http://10.0.0.5:8710/");
        node.verified = true;
        node.grants = aoide_storage::node_store::grants_in("home", &["read", "spawn"]);
        // `token_file` deliberately left `None` — this node can only ever
        // resolve via the address rung.
        aoide_storage::node_store::save_nodes(&[node]).unwrap();

        let params = json!({ "message": { "parts": [{ "kind": "text", "text": "hi" }] } });
        let remote_origin = ConnOrigin::Remote("10.0.0.5".parse().unwrap());
        let err = message_send(
            &params,
            &root.join("log"),
            "claude",
            "",
            remote_origin,
            "",
            None,
            None,
        )
        .unwrap_err();
        assert_eq!(
            err.0, -32006,
            "paired AND `spawn` in allows, but resolved ONLY via address — still refused"
        );

        let _ = std::fs::remove_dir_all(&root);
        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
    }

    #[test]
    fn message_send_spawn_refuses_the_door_wide_bearer_alone_with_no_node_identity() {
        // The exact scenario decision 6 names explicitly: a caller presenting
        // a VALID door-wide bearer (the OLD gate this amendment replaces)
        // but resolving to no specific registered node at all — no node's
        // own `token_file` matches this token, and the registry is empty so
        // no address can match either. Under the pre-P-P3 gate this would
        // have passed (`token_authorized` was the whole gate); now it must
        // still refuse.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-spawn-doorwide-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", root.join("stage"));
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));

        let params = json!({ "message": { "parts": [{ "kind": "text", "text": "hi" }] } });
        let err = message_send(
            &params,
            &root.join("log"),
            "claude",
            "",
            ConnOrigin::Loopback,
            "the-door-wide-secret",
            Some("the-door-wide-secret"), // matches expected_token exactly.
            None,
        )
        .unwrap_err();
        assert_eq!(err.0, -32006, "a valid DOOR-WIDE bearer alone no longer reaches the spawn arm");

        let _ = std::fs::remove_dir_all(&root);
        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
    }

    #[test]
    fn non_loopback_message_send_is_held_pending_not_delivered() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-nonloopback-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::remove_var("AOIDE_CONDUCT_AUTOGATE");
        std::env::remove_var("AOIDE_SESSION_ID");

        let id = "remote-target";
        let socket = aoide_conduct::graph::conduct_socket_path(id);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        // A listener stands in for the conducted process — if a delivery
        // were WRONGLY attempted, connecting to it would succeed; the
        // assertions below prove the connect never happens at all.
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();

        let sf = SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![conductable_session(id, &socket)],
        };
        write_stage(&sessions_path(), &sf).unwrap();

        let audit_log = root.join("log");
        let params = serde_json::json!({
            "message": { "parts": [{ "kind": "text", "text": "inject me" }], "contextId": id }
        });
        let remote_origin = ConnOrigin::Remote("10.0.0.9".parse().unwrap());
        let result = message_send(&params, &audit_log, "", "", remote_origin, "", None, None);
        let task = result.expect("a pending send is still an Ok Task, not a JSON-RPC error");
        assert_eq!(task["id"], id);
        assert_eq!(
            task["status"]["state"], "submitted",
            "the synchronous response reports `submitted`, not the session's unrelated state"
        );

        // Nothing connected to the socket — no delivery was attempted.
        assert!(listener.accept().is_err(), "a non-loopback send must never touch the socket");

        // `pending.json` carries the queued entry.
        let pending: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(stage.join("pending.json")).unwrap(),
        )
        .unwrap();
        let entries = pending["pending"].as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["sessionId"], id);
        assert_eq!(entries[0]["text"], "inject me");

        // Audited through the single Door::A2a log, "pending" status — every
        // outcome (queued/delivered/error) routes through the SAME
        // audit path `send` already uses (`conduct::graph::send::
        // audit_send`), never a second logging path.
        let log = std::fs::read_to_string(&audit_log).unwrap_or_default();
        assert!(log.contains("\"door\":\"a2a\""), "audited through Door::A2a: {log}");
        assert!(log.contains("\"status\":\"pending\""), "audited as pending: {log}");
        assert!(log.contains("send"), "reuses send's own audit command label: {log}");

        let _ = std::fs::remove_dir_all(&root);
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    #[test]
    fn an_address_matched_inject_is_recorded_under_its_address_not_the_nodes_name() {
        // No door token is configured, so a caller whose address matches a
        // registered node's URL reaches the queue with no credential at all:
        // the address rung is the case that can actually happen. The entry is
        // recorded under the address it came from, never `node:<name>`, so an
        // approver is not shown a node that did not send it and the node's own
        // bucket is not the one it counts against.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-pending-origin-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::remove_var("AOIDE_CONDUCT_AUTOGATE");
        std::env::remove_var("AOIDE_SESSION_ID");

        // Registered and resolvable by address. The send still queues.
        aoide_storage::node_store::save_nodes(&[fixture_node("watching-node", "http://10.0.0.9:8710/")]).unwrap();

        let id = "attributed-target";
        let socket = aoide_conduct::graph::conduct_socket_path(id);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();

        let sf = SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![conductable_session(id, &socket)],
        };
        write_stage(&sessions_path(), &sf).unwrap();

        let audit_log = root.join("log");
        let params = serde_json::json!({
            "message": { "parts": [{ "kind": "text", "text": "who sent this" }], "contextId": id }
        });
        let remote_origin = ConnOrigin::Remote("10.0.0.9".parse().unwrap());
        let result = message_send(&params, &audit_log, "", "", remote_origin, "", None, None);
        assert!(result.is_ok(), "still a submitted Task, never a JSON-RPC error");

        let pending: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(stage.join("pending.json")).unwrap()).unwrap();
        let entries = pending["pending"].as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["from"], "addr:10.0.0.9", "an address match is recorded as the address, not as the node");

        let _ = std::fs::remove_dir_all(&root);
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    #[test]
    fn message_send_resolves_via_signed_caller_producing_signature_rung_attribution() {
        // P-P4: `message_send`'s `signed_caller` param (already verified
        // by `verify_signed_request`, one layer up) is the SOLE resolution
        // when present — this test drives that resolution directly (the
        // signature verification itself is `verify_signed_request`'s own
        // test suite above), proving the resulting `NodeRung::Signature`
        // attribution reaches `pending.json` the same way Token/Addr
        // already did before this phase.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-sig-attr-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::remove_var("AOIDE_CONDUCT_AUTOGATE");
        std::env::remove_var("AOIDE_SESSION_ID");

        let mut signed_node = fixture_node("signed-node", "http://10.0.0.9:8710/");
        signed_node.verified = true;
        aoide_storage::node_store::save_nodes(&[signed_node]).unwrap();

        let id = "sig-attr-tgt";
        let socket = aoide_conduct::graph::conduct_socket_path(id);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();

        let sf = SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![conductable_session(id, &socket)],
        };
        write_stage(&sessions_path(), &sf).unwrap();

        let audit_log = root.join("log");
        let params = serde_json::json!({
            "message": { "parts": [{ "kind": "text", "text": "signed hello" }], "contextId": id }
        });
        // A REMOTE origin with NO token presented at all —
        // under the pre-P-P4 ladder this would resolve to nothing
        // (`resolve_node` needs a token or a matching address); it resolves
        // here purely because `signed_caller` is `Some`.
        let remote_origin = ConnOrigin::Remote("203.0.113.1".parse().unwrap());
        let result = message_send(
            &params,
            &audit_log,
            "",
            "",
            remote_origin,
            "",
            None,
            Some(caller("signed-node")),
        );
        assert!(result.is_ok());

        let pending: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(stage.join("pending.json")).unwrap()).unwrap();
        let entries = pending["pending"].as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["from"], "node:signed-node", "resolution via signed_caller attributes correctly");

        let _ = std::fs::remove_dir_all(&root);
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    #[test]
    fn message_send_signed_caller_never_falls_through_to_the_addr_token_ladder() {
        // Fail-closed discipline (#84 precedent, brief point 2): a
        // `signed_caller` that names a node NOT actually present in the
        // (freshly reloaded) registry — an edge case `verify_signed_request`
        // itself already prevents in practice, since it only ever hands
        // back a name it just confirmed is registered+verified — must
        // resolve to NOTHING, never silently fall back to whatever the
        // presented token/address WOULD otherwise have resolved to.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-sig-nofall-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::remove_var("AOIDE_CONDUCT_AUTOGATE");
        std::env::remove_var("AOIDE_SESSION_ID");

        // A REAL registered node that WOULD resolve via its own token, if
        // the addr/token ladder ever ran.
        let token_file = root.join("real-node.token");
        std::fs::write(&token_file, "real-secret").unwrap();
        let mut real_node = fixture_node("real-node", "http://10.0.0.9:8710/");
        real_node.token_file = Some(token_file.to_string_lossy().into_owned());
        aoide_storage::node_store::save_nodes(&[real_node]).unwrap();

        let id = "nofall-tgt";
        let socket = aoide_conduct::graph::conduct_socket_path(id);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();

        let sf = SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![conductable_session(id, &socket)],
        };
        write_stage(&sessions_path(), &sf).unwrap();

        let audit_log = root.join("log");
        let params = serde_json::json!({
            "message": { "parts": [{ "kind": "text", "text": "hi" }], "contextId": id }
        });
        let remote_origin = ConnOrigin::Remote("10.0.0.9".parse().unwrap());
        // Presents the REAL node's own valid token AND claims a signed node
        // name ("ghost") that doesn't exist in the registry at all.
        let result = message_send(
            &params,
            &audit_log,
            "",
            "",
            remote_origin,
            "",
            Some("real-secret"),
            Some(caller("ghost")),
        );
        assert!(result.is_ok());

        let pending: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(stage.join("pending.json")).unwrap()).unwrap();
        let entries = pending["pending"].as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0]["from"], "addr:10.0.0.9",
            "a signed_caller resolving to nothing must NOT fall back to the token-resolved `real-node`: it is recorded by address"
        );

        let _ = std::fs::remove_dir_all(&root);
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    /// LANE IDENTITY P-ID3 (G9): the unresolvable `signed_caller` of the test
    /// above, but with `AOIDE_SESSION_ID` set in THIS PROCESS's own env first
    /// — standing in for whatever `aoide a2a serve` might have inherited at
    /// launch. The pending entry's `from` is the door's own `addr:<address>`
    /// and never that variable: the door stamps `--from` on every inject, so
    /// `session_send`'s `resolve_sender` env fallback has nothing to fall back
    /// from.
    #[test]
    fn an_unproven_inject_never_falls_back_to_this_processs_own_ambient_session_id() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_session_id = std::env::var("AOIDE_SESSION_ID").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-g9-no-env-leak-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::remove_var("AOIDE_CONDUCT_AUTOGATE");
        // The one line that matters: this stands in for a stray
        // `AOIDE_SESSION_ID` in `aoide a2a serve`'s OWN launch environment —
        // never the remote caller's, which has no channel to set it at all.
        std::env::set_var("AOIDE_SESSION_ID", "daemons-own-stray-session");

        let token_file = root.join("real-node.token");
        std::fs::write(&token_file, "real-secret").unwrap();
        let mut real_node = fixture_node("real-node", "http://10.0.0.9:8710/");
        real_node.token_file = Some(token_file.to_string_lossy().into_owned());
        aoide_storage::node_store::save_nodes(&[real_node]).unwrap();

        let id = "g9-no-env-leak-tgt";
        let socket = aoide_conduct::graph::conduct_socket_path(id);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();

        let sf = SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![conductable_session(id, &socket)],
        };
        write_stage(&sessions_path(), &sf).unwrap();

        let audit_log = root.join("log");
        let params = serde_json::json!({
            "message": { "parts": [{ "kind": "text", "text": "hi" }], "contextId": id }
        });
        let remote_origin = ConnOrigin::Remote("10.0.0.9".parse().unwrap());
        // An unresolvable `signed_caller` ("ghost"): the caller proved
        // nothing, the shape that used to fall through to the env.
        let result = message_send(
            &params,
            &audit_log,
            "",
            "",
            remote_origin,
            "",
            Some("real-secret"),
            Some(caller("ghost")),
        );
        assert!(result.is_ok(), "{:?}", result.err());
        assert!(listener.accept().is_err(), "unattributed sends must never touch the socket");

        let pending: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(stage.join("pending.json")).unwrap()).unwrap();
        let entries = pending["pending"].as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0]["from"], "addr:10.0.0.9",
            "an unproven inject must NEVER pick up this process's own ambient AOIDE_SESSION_ID"
        );

        let _ = std::fs::remove_dir_all(&root);
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
        match saved_session_id {
            Some(v) => std::env::set_var("AOIDE_SESSION_ID", v),
            None => std::env::remove_var("AOIDE_SESSION_ID"),
        }
    }

    #[test]
    fn loopback_message_send_is_held_at_the_door_like_any_other() {
        // A loopback origin is no exception at a network door: it is held like
        // any caller. A tunnel terminating on loopback looks exactly like this,
        // so loopback cannot be the trusted rung here.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-loopback-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::remove_var("AOIDE_CONDUCT_AUTOGATE");
        std::env::remove_var("AOIDE_SESSION_ID");

        let id = "local-target";
        let socket = aoide_conduct::graph::conduct_socket_path(id);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let listener = UnixListener::bind(&socket).unwrap();

        let sf = SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![conductable_session(id, &socket)],
        };
        write_stage(&sessions_path(), &sf).unwrap();


        let audit_log = root.join("log");
        let params = serde_json::json!({
            "message": { "parts": [{ "kind": "text", "text": "hello loopback" }], "contextId": id }
        });
        let result = message_send(
            &params,
            &audit_log,
            "",
            "",
            ConnOrigin::Loopback,
            "",
            None,
            None,
        );
        let task = result.unwrap();
        assert_eq!(task["status"]["state"], "submitted", "{task}");
        assert_held_at_the_door(&listener, &stage, 1);

        let _ = std::fs::remove_dir_all(&root);
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    #[test]
    fn message_send_to_a_live_socket_is_held_and_a_removed_one_is_refused() {
        // The A2A-door half of the identical bug `e2758f7` fixed on `graph`:
        // a stored `conductable` session whose control socket has since been
        // deleted (`shellbridge.service` owns `$XDG_RUNTIME_DIR/aoide` with
        // `RuntimeDirectoryPreserve=no`, so a rebuild deletes it out from
        // under every live session) must take the EXISTING -32004 "session
        // not conductable" arm — never `SendAction::Inject` reaching an
        // address nothing can reach. Matters more here than on `graph`: this
        // is the cross-node path, so a remote node would be told delivery is
        // happening when it cannot be.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-socket-gone-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::remove_var("AOIDE_CONDUCT_AUTOGATE");
        std::env::remove_var("AOIDE_SESSION_ID");

        let id = "socket-gone-target";
        let socket = aoide_conduct::graph::conduct_socket_path(id);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let listener = UnixListener::bind(&socket).unwrap();

        let sf = SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![conductable_session(id, &socket)],
        };
        write_stage(&sessions_path(), &sf).unwrap();

        let audit_log = root.join("log");

        // While the socket still exists the send is held like any other at this
        // door: nothing is written, and the socket's presence changes nothing.
        let params = serde_json::json!({
            "message": { "parts": [{ "kind": "text", "text": "still here" }], "contextId": id }
        });
        let result = message_send(
            &params,
            &audit_log,
            "",
            "",
            ConnOrigin::Loopback,
            "",
            None,
            None,
        );
        assert_eq!(result.unwrap()["status"]["state"], "submitted");
        assert_held_at_the_door(&listener, &stage, 1);

        // The socket file is deleted (a rebuild tearing down the runtime
        // dir) — the STORED record is untouched, but the SAME contextId
        // must now refuse rather than inject into a dead address.
        std::fs::remove_file(&socket).unwrap();
        let params2 = serde_json::json!({
            "message": { "parts": [{ "kind": "text", "text": "too late" }], "contextId": id }
        });
        let err = message_send(
            &params2,
            &audit_log,
            "",
            "",
            ConnOrigin::Loopback,
            "",
            None,
            None,
        )
        .expect_err("a missing socket must be a structured error, not a failed connect");
        assert_eq!(err.0, -32004);
        assert_eq!(err.1, "session not conductable");

        let _ = std::fs::remove_dir_all(&root);
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    /// A shell child takes no steer from a network door (house rule 4). Every
    /// line is held PENDING, and a human's own approval is what lets it reach a
    /// shell. The P-C5 capture on the shell record is why that approval matters
    /// for it; this door never supplies one.
    #[test]
    fn a_shell_wrapped_target_takes_no_auto_delivered_inject() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-shell-target-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::remove_var("AOIDE_CONDUCT_AUTOGATE");
        std::env::remove_var("AOIDE_SESSION_ID");

        let mut records = Vec::new();
        let mut listeners = Vec::new();
        for id in ["plain-target", "shell-target"] {
            let socket = aoide_conduct::graph::conduct_socket_path(id);
            std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
            listeners.push((id, UnixListener::bind(&socket).unwrap()));
            let mut rec = conductable_session(id, &socket);
            // The shell arm is the SAME record with the P-C5 capture on it —
            // a `--agent <harness> -- bash` session a moment after it started.
            if id == "shell-target" {
                rec.restore = Some(aoide_storage::records::RestoreSnapshot::default());
            }
            records.push(rec);
        }
        write_stage(&sessions_path(), &SessionsFile { schema_version: "0".to_string(), sessions: records }).unwrap();
        let audit_log = root.join("log");

        // Both arms are held: a network door writes to no session, and the
        // shell rule is one more reason it never delivers.
        let plain = listeners.iter().find(|(id, _)| *id == "plain-target").unwrap().1.try_clone().unwrap();
        let result = message_send(
            &json!({ "message": { "parts": [{ "kind": "text", "text": "plain hello" }], "contextId": "plain-target" } }),
            &audit_log,
            "",
            "",
            ConnOrigin::Loopback,
            "",
            None,
            None,
        );
        assert_eq!(result.unwrap()["status"]["state"], "submitted");
        assert_held_at_the_door(&plain, &stage, 1);

        let shell = listeners.iter().find(|(id, _)| *id == "shell-target").unwrap().1.try_clone().unwrap();
        let held = message_send(
            &json!({ "message": { "parts": [{ "kind": "text", "text": "would run" }], "contextId": "shell-target" } }),
            &audit_log,
            "",
            "",
            ConnOrigin::Loopback,
            "",
            None,
            None,
        )
        .unwrap();
        assert_eq!(held["status"]["state"], "submitted", "{held}");
        assert_held_at_the_door(&shell, &stage, 2);

        let _ = std::fs::remove_dir_all(&root);
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    /// A harness target is held at a network door whether or not the launcher
    /// walk proves its harness: a label of `claude` with no proof, or the wrong
    /// proof, and a proven wrap alike. Nothing is written to any of them.
    #[test]
    fn a_harness_target_is_held_at_a_network_door_proven_or_not() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-unverified-target-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::remove_var("AOIDE_CONDUCT_AUTOGATE");
        std::env::remove_var("AOIDE_SESSION_ID");

        // Every label is `claude`; only the walked program differs.
        let walks = [("proven-target", Some("claude")), ("none-target", None), ("taskset-target", Some("taskset"))];
        let mut records = Vec::new();
        let mut listeners = Vec::new();
        for (id, walked) in walks {
            let socket = aoide_conduct::graph::conduct_socket_path(id);
            std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
            listeners.push((id, UnixListener::bind(&socket).unwrap()));
            let mut rec = conductable_session(id, &socket);
            rec.agent = "claude".to_string();
            rec.wrapped_program = walked.map(str::to_string);
            records.push(rec);
        }
        write_stage(&sessions_path(), &SessionsFile { schema_version: "0".to_string(), sessions: records }).unwrap();
        let audit_log = root.join("log");
        let listener_for = |id: &str| listeners.iter().find(|(i, _)| *i == id).unwrap().1.try_clone().unwrap();

        let proven = listener_for("proven-target");
        let result = message_send(
            &json!({ "message": { "parts": [{ "kind": "text", "text": "proven hello" }], "contextId": "proven-target" } }),
            &audit_log,
            "",
            "",
            ConnOrigin::Loopback,
            "",
            None,
            None,
        );
        assert_eq!(result.unwrap()["status"]["state"], "submitted");
        assert_held_at_the_door(&proven, &stage, 1);

        for (n, id) in ["none-target", "taskset-target"].into_iter().enumerate() {
            let listener = listener_for(id);
            let held = message_send(
                &json!({ "message": { "parts": [{ "kind": "text", "text": "held" }], "contextId": id } }),
                &audit_log,
                "",
                "",
                ConnOrigin::Loopback,
                "",
                None,
                None,
            )
            .unwrap();
            assert_eq!(held["status"]["state"], "submitted", "{held}");
            assert_held_at_the_door(&listener, &stage, n + 2);
        }

        let _ = std::fs::remove_dir_all(&root);
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    #[test]
    fn a_message_held_at_the_network_door_files_nothing_into_the_mailbase() {
        // Messaging plan P-M1: the mailbase entry is filed at delivery through the
        // shared seam. A network door holds the message instead, so nothing is
        // filed until a local approval delivers it.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-mail-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::remove_var("AOIDE_CONDUCT_AUTOGATE");
        std::env::remove_var("AOIDE_SESSION_ID");

        let id = "mail-a2a-target";
        let socket = aoide_conduct::graph::conduct_socket_path(id);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let listener = UnixListener::bind(&socket).unwrap();

        let sf = SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![conductable_session(id, &socket)],
        };
        write_stage(&sessions_path(), &sf).unwrap();

        let audit_log = root.join("log");
        let params = serde_json::json!({
            "message": { "parts": [{ "kind": "text", "text": "hello from a node" }], "contextId": id }
        });
        let result = message_send(&params, &audit_log, "", "", ConnOrigin::Loopback, "", None, None);
        assert_eq!(result.unwrap()["status"]["state"], "submitted");
        assert_held_at_the_door(&listener, &stage, 1);

        // Filing happens at delivery, and this door never delivers: the message
        // waits in `pending.json` and is not yet in the mailbase.
        let entries = aoide_storage::mail::read_base().unwrap_or_default();
        assert!(entries.is_empty(), "held, so nothing is filed yet: {entries:?}");

        let _ = std::fs::remove_dir_all(&root);
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    /// M3/M4, at the read end: what the worker stamped is what `tasks/get`
    /// shows. A record whose opening turn never ran carries `not-ready`, and
    /// that reaches the peer as the task's `status.message` — never a bare
    /// `submitted` over a session no turn ever reached.
    #[test]
    fn the_opening_turns_outcome_is_what_tasks_get_shows() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-openingturn-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(root.join("stage")).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", root.join("stage"));

        let mut rec = fixture_session("spawned-remote", "idle", None);
        rec.parent_session_id = Some("node-parent".to_string());
        write_stage(
            &sessions_path(),
            &SessionsFile { schema_version: "0".to_string(), sessions: vec![rec] },
        )
        .unwrap();

        // Before any stamp: the task carries no opening-turn message at all
        // (byte-identical to the pre-amendment shape).
        let task = task_get("spawned-remote").unwrap();
        assert!(task["status"].get("message").is_none(), "task: {task}");

        // The door's own pending stamp, then the worker's verdict.
        aoide_conduct::graph::stamp_opening_turn("spawned-remote", "pending");
        let task = task_get("spawned-remote").unwrap();
        assert_eq!(
            task["status"]["message"]["parts"][0]["text"], "opening turn: pending",
            "the status message is an A2A Message object, not a string: {task}"
        );
        assert_eq!(task["status"]["message"]["role"], "agent", "task: {task}");

        aoide_conduct::graph::stamp_opening_turn("spawned-remote", "not-ready");
        let task = task_get("spawned-remote").unwrap();
        assert_eq!(
            task["status"]["message"]["parts"][0]["text"], "opening turn: not-ready",
            "a peer whose opening turn never ran is TOLD so: {task}"
        );

        let _ = std::fs::remove_dir_all(&root);
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
    }

    /// An empty opening turn is not a send: the worker records `skipped-empty`
    /// and nothing reaches the queue.
    #[test]
    fn an_empty_opening_turn_is_skipped_and_queues_nothing() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-empty-turn-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(root.join("stage")).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", root.join("stage"));

        let id = "empty-turn";
        write_stage(
            &sessions_path(),
            &SessionsFile {
                schema_version: "0".to_string(),
                sessions: vec![fixture_session(id, "idle", None)],
            },
        )
        .unwrap();

        opening_turn_worker(
            id.to_string(),
            String::new(),
            root.join("log"),
            OpeningTurnSlot::acquire().expect("a slot"),
            None,
        );
        let f: SessionsFile = load_stage(&sessions_path()).unwrap();
        let word = f.sessions.iter().find(|s| s.session_id == id).and_then(|s| s.opening_turn.clone());
        assert_eq!(word.as_deref(), Some("skipped-empty"));
        assert!(!root.join("stage").join("pending.json").exists(), "an empty turn is not a send");

        let _ = std::fs::remove_dir_all(&root);
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
    }

    /// A failed queue write is `hold-failed`, end to end: the worker stamps it
    /// on the record, nothing is typed, and the queue file the failure hit is
    /// left as it was. The write fails inside the queue's own read, because
    /// `pending.json` does not parse; that is the error branch a failed write
    /// takes as well, and it is the only way this test can make the write fail
    /// without depending on whether the suite runs as root.
    #[test]
    fn a_failed_queue_write_is_hold_failed_and_overwrites_nothing() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-hold-failed-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(root.join("stage")).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", root.join("stage"));
        let queue = root.join("stage").join("pending.json");
        let corrupt = "{ not json";
        std::fs::write(&queue, corrupt).unwrap();

        let id = "hold-failed";
        write_stage(
            &sessions_path(),
            &SessionsFile {
                schema_version: "0".to_string(),
                sessions: vec![fixture_session(id, "idle", None)],
            },
        )
        .unwrap();

        opening_turn_worker(
            id.to_string(),
            "hi".to_string(),
            root.join("log"),
            OpeningTurnSlot::acquire().expect("a slot"),
            None,
        );
        let f: SessionsFile = load_stage(&sessions_path()).unwrap();
        let word = f.sessions.iter().find(|s| s.session_id == id).and_then(|s| s.opening_turn.clone());
        assert_eq!(word.as_deref(), Some("hold-failed"), "the verdict names the failure");
        assert_eq!(std::fs::read_to_string(&queue).unwrap(), corrupt, "the queue file was overwritten");

        let _ = std::fs::remove_dir_all(&root);
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
    }

    /// A node that already holds the queue's per-source cap gets `queue-full`
    /// for its spawn's opening turn: the worker stamps it on the record, and
    /// nothing is queued or moved.
    #[test]
    fn an_opening_turn_from_a_full_source_is_queue_full_and_queues_nothing() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-queue-full-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(root.join("stage")).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", root.join("stage"));
        let id = "queue-full";
        write_stage(
            &sessions_path(),
            &SessionsFile {
                schema_version: "0".to_string(),
                sessions: vec![fixture_session(id, "idle", None)],
            },
        )
        .unwrap();
        fill_source(id, "node:busy-node");
        let before = std::fs::read_to_string(root.join("stage").join("pending.json")).unwrap();

        opening_turn_worker(
            id.to_string(),
            "hi".to_string(),
            root.join("log"),
            OpeningTurnSlot::acquire().expect("a slot"),
            Some("node:busy-node".to_string()),
        );
        let f: SessionsFile = load_stage(&sessions_path()).unwrap();
        let word = f.sessions.iter().find(|s| s.session_id == id).and_then(|s| s.opening_turn.clone());
        assert_eq!(word.as_deref(), Some("queue-full"), "the verdict names the cap");
        let after = std::fs::read_to_string(root.join("stage").join("pending.json")).unwrap();
        assert_eq!(after, before, "nothing queued, nothing moved");

        let _ = std::fs::remove_dir_all(&root);
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
    }

    /// A prompt past the queue's per-entry byte cap is never queued: the
    /// worker stamps `hold-failed`, the one word for "held nothing", and the
    /// queue is exactly as it was.
    #[test]
    fn an_oversize_opening_turn_is_hold_failed_and_queues_nothing() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-too-large-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(root.join("stage")).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", root.join("stage"));
        let id = "too-large";
        write_stage(
            &sessions_path(),
            &SessionsFile {
                schema_version: "0".to_string(),
                sessions: vec![fixture_session(id, "idle", None)],
            },
        )
        .unwrap();

        opening_turn_worker(
            id.to_string(),
            "x".repeat(aoide_conduct::graph::PENDING_ENTRY_MAX_BYTES),
            root.join("log"),
            OpeningTurnSlot::acquire().expect("a slot"),
            Some("node:chatty".to_string()),
        );
        let f: SessionsFile = load_stage(&sessions_path()).unwrap();
        let word = f.sessions.iter().find(|s| s.session_id == id).and_then(|s| s.opening_turn.clone());
        assert_eq!(word.as_deref(), Some("hold-failed"), "an oversize prompt holds nothing");
        assert!(!root.join("stage").join("pending.json").exists(), "and queues nothing");

        let _ = std::fs::remove_dir_all(&root);
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
    }

    /// N2: the opening-turn worker pool is a REAL bound — the eighth waiter
    /// fits, the ninth is refused (`busy`), and a released slot is reusable.
    /// Without it the wait the M4 fix moved off the handler is bounded by
    /// nothing at all.
    /// The L of the second re-review, at the ordering itself: the worker
    /// stamps `pending` and then its verdict, from one thread, so the record
    /// can never be left reading `pending` after the worker is done — not even
    /// when a verdict was already there.
    #[test]
    fn the_opening_turn_worker_ends_on_its_verdict_never_on_pending() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-worker-order-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(root.join("stage")).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", root.join("stage"));
        let audit_log = root.join("log");

        let id = "worker-order";
        write_stage(
            &sessions_path(),
            &SessionsFile {
                schema_version: "0".to_string(),
                sessions: vec![fixture_session(id, "idle", None)],
            },
        )
        .unwrap();

        let read_word = || -> Option<String> {
            let f: SessionsFile = load_stage(&sessions_path()).unwrap();
            f.sessions.iter().find(|s| s.session_id == id).and_then(|s| s.opening_turn.clone())
        };

        // A network door cannot approve, so the opening turn is held for a
        // human, not typed: the verdict is `held-pending`, never `pending`.
        opening_turn_worker(
            id.to_string(),
            "hi".to_string(),
            audit_log.clone(),
            OpeningTurnSlot::acquire().expect("a slot"),
            None,
        );
        assert_eq!(read_word().as_deref(), Some("held-pending"), "the verdict, never `pending`");

        // And a verdict already on the record is replaced by THIS worker's own
        // — the pending write in between is the same thread's, so nothing can
        // strand it.
        aoide_conduct::graph::stamp_opening_turn(id, "delivered");
        opening_turn_worker(
            id.to_string(),
            "hi".to_string(),
            audit_log,
            OpeningTurnSlot::acquire().expect("a slot"),
            None,
        );
        assert_eq!(read_word().as_deref(), Some("held-pending"), "still ends on its own verdict");

        let _ = std::fs::remove_dir_all(&root);
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
    }

    /// The exact regression the second re-review found: the door's
    /// registration wait must NOT write `pending` from its own thread any more,
    /// because a verdict that landed first would be clobbered back to it.
    #[test]
    fn the_registration_wait_never_writes_pending_over_a_verdict() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-prov-order-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(root.join("stage")).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", root.join("stage"));

        let id = "prov-order";
        let mut rec = fixture_session(id, "idle", None);
        rec.opening_turn = Some("not-ready".to_string());
        write_stage(
            &sessions_path(),
            &SessionsFile { schema_version: "0".to_string(), sessions: vec![rec] },
        )
        .unwrap();

        stamp_spawn_provenance(id, "node:remote-1", None);

        let f: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec = f.sessions.iter().find(|s| s.session_id == id).unwrap();
        assert_eq!(
            rec.opening_turn.as_deref(),
            Some("not-ready"),
            "the registration wait must not touch the verdict"
        );
        assert_eq!(rec.origin.as_deref(), Some("node:remote-1"), "it still stamps the origin");

        let _ = std::fs::remove_dir_all(&root);
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
    }

    #[test]
    fn the_opening_turn_worker_pool_refuses_past_its_cap() {
        let mut held: Vec<OpeningTurnSlot> = Vec::new();
        for _ in 0..OPENING_TURN_WORKERS_MAX {
            held.push(OpeningTurnSlot::acquire().expect("a slot inside the cap"));
        }
        assert!(
            OpeningTurnSlot::acquire().is_none(),
            "the pool must refuse the {}-th concurrent wait",
            OPENING_TURN_WORKERS_MAX + 1
        );
        held.pop();
        assert!(
            OpeningTurnSlot::acquire().is_some(),
            "a released slot is reusable"
        );
    }

    #[test]
    fn loopback_message_send_gets_the_uniform_answer_once_a_token_is_configured_and_absent() {
        // The actual gap the 2026-08-19 amendment closed: a proxy/tunnel
        // makes an outside caller LOOK loopback to `peer_addr()`. Once the
        // operator configures a token, an unauthenticated "loopback" caller
        // must no longer get the automatic pass it used to.
        //
        // Superseded by the 2026-08-20 (#50) uniform-response amendment: this
        // scenario now hits the uniform guard BEFORE `decide_send_action`
        // even runs, so it no longer queues into `pending.json` at all — it
        // used to (a hold-pending Task, one queued entry); now it's a
        // synthetic submitted Task and the queue stays untouched, closing
        // the unauthenticated-queue-write half of #50.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            // Kept SHORT deliberately: XDG_RUNTIME_DIR is set to this root, so
            // conduct_socket_path() hangs `/aoide/session-<id>.sock` off it and
            // the whole thing must fit SUN_LEN (107 bytes + NUL).
            "aoide-a2a-tok-abs-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::remove_var("AOIDE_CONDUCT_AUTOGATE");
        std::env::remove_var("AOIDE_SESSION_ID");

        let id = "tok-abs-tgt";
        let socket = aoide_conduct::graph::conduct_socket_path(id);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        // If a delivery were WRONGLY attempted, connecting would succeed;
        // the assertions below prove it never happens.
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();

        let sf = SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![conductable_session(id, &socket)],
        };
        write_stage(&sessions_path(), &sf).unwrap();

        let audit_log = root.join("log");
        let params = serde_json::json!({
            "message": { "parts": [{ "kind": "text", "text": "spoofed loopback" }], "contextId": id }
        });
        let result = message_send(
            &params,
            &audit_log,
            "",
            "",
            ConnOrigin::Loopback,
            "the-real-token",
            None,
            None,
        );
        let task = result.expect("the uniform arm always answers Ok, never a JSON-RPC error");
        assert_eq!(
            task["status"]["state"], "submitted",
            "the uniform synthetic Task, not the session's unrelated state"
        );
        assert!(listener.accept().is_err(), "an unauthenticated 'loopback' send must never touch the socket once a token is configured");

        // #50: the uniform arm never resolves the id, so it never queues —
        // no `pending.json` entry, unlike this scenario's pre-#50 behavior.
        let pending_path = stage.join("pending.json");
        if pending_path.exists() {
            let pending: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&pending_path).unwrap()).unwrap();
            assert!(
                pending["pending"].as_array().map(Vec::is_empty).unwrap_or(true),
                "an unauthenticated 'loopback' send must never feed the approval queue"
            );
        }

        let _ = std::fs::remove_dir_all(&root);
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    #[test]
    fn loopback_message_send_with_a_valid_token_is_held_too() {
        // The other half of the same coupling: presenting the CORRECT token
        // restores exactly the original loopback behavior — this amendment
        // narrows trust, it doesn't remove the ability to be trusted.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            // Kept SHORT deliberately — see the sibling test above for why.
            "aoide-a2a-tok-ok-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::remove_var("AOIDE_CONDUCT_AUTOGATE");
        std::env::remove_var("AOIDE_SESSION_ID");

        let id = "tok-ok-tgt";
        let socket = aoide_conduct::graph::conduct_socket_path(id);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let listener = UnixListener::bind(&socket).unwrap();

        let sf = SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![conductable_session(id, &socket)],
        };
        write_stage(&sessions_path(), &sf).unwrap();


        let audit_log = root.join("log");
        let params = serde_json::json!({
            "message": { "parts": [{ "kind": "text", "text": "authenticated loopback" }], "contextId": id }
        });
        let result = message_send(
            &params,
            &audit_log,
            "",
            "",
            ConnOrigin::Loopback,
            "the-real-token",
            Some("the-real-token"), None
        );
        assert_eq!(result.unwrap()["status"]["state"], "submitted");
        assert_held_at_the_door(&listener, &stage, 1);

        let _ = std::fs::remove_dir_all(&root);
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    #[test]
    fn a_registered_address_send_is_held_and_never_touches_the_socket() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-addr-held-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let stage = root.join("stage");
        let state = root.join("state");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        std::env::set_var("AOIDE_STATE_DIR", &state);
        std::env::remove_var("AOIDE_CONDUCT_AUTOGATE");
        std::env::remove_var("AOIDE_SESSION_ID");

        // A registered node whose url resolves (as an IP literal — no real
        // DNS) to the connecting address.
        aoide_storage::node_store::save_nodes(&[aoide_storage::node_store::Node {
            name: "trusted-node".into(),
            url: "http://10.0.0.9:8710/".into(),
            token_file: None,
            bearer_secret: None,
            hub: false,
            pubkey: None,
            verified: false,
            grants: aoide_storage::node_store::Grants::new(),
            narrowed: aoide_storage::node_store::Grants::new(),
            via: None,
            added_at: "2026-08-14T00:00:00Z".into(),
        }])
        .unwrap();

        let id = "addr-held-target";
        let socket = aoide_conduct::graph::conduct_socket_path(id);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let listener = UnixListener::bind(&socket).unwrap();

        let sf = SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![conductable_session(id, &socket)],
        };
        write_stage(&sessions_path(), &sf).unwrap();


        let audit_log = root.join("log");
        let params = serde_json::json!({
            "message": { "parts": [{ "kind": "text", "text": "trusted send" }], "contextId": id }
        });
        let remote_origin = ConnOrigin::Remote("10.0.0.9".parse().unwrap());
        let result = message_send(&params, &audit_log, "", "", remote_origin, "", None, None);
        assert!(result.is_ok(), "{:?}", result.err());
        assert_held_at_the_door(&listener, &stage, 1);

        let _ = std::fs::remove_dir_all(&root);
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    // ── Signed requests outrank loopback for Inject (P-S6, CONTRACTS.md §6 ──
    // ── amendment 2026-08-26) ─────────────────────────────────────────────
    //
    // An ssh `-L` tunnel makes a remote node's request arrive at
    // `peer_addr()` looking exactly like a genuinely local caller — the top
    // risk the ssh-transport lane's plan names explicitly. These two tests
    // are the pin: a signed node over a LOOPBACK connection is still a remote
    // node. An ssh `-L` tunnel makes it look local, and it must park like any
    // other remote send.

    #[test]
    fn signed_inject_from_a_node_on_a_loopback_connection_is_held_pending() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-sig-loop-pending-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::remove_var("AOIDE_CONDUCT_AUTOGATE");
        std::env::remove_var("AOIDE_SESSION_ID");

        // Paired and verified. A signed send from this node must queue,
        // whatever the connection's own origin looks like.
        let mut signed_node = fixture_node("tunneled-node", "http://10.0.0.9:8710/");
        signed_node.verified = true;
        aoide_storage::node_store::save_nodes(&[signed_node]).unwrap();

        let id = "sig-loop-pending-tgt";
        let socket = aoide_conduct::graph::conduct_socket_path(id);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        // Nothing must ever connect here — a wrongly-delivered send would.
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();

        let sf = SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![conductable_session(id, &socket)],
        };
        write_stage(&sessions_path(), &sf).unwrap();

        let audit_log = root.join("log");
        let params = serde_json::json!({
            "message": { "parts": [{ "kind": "text", "text": "tunneled send" }], "contextId": id }
        });
        // The defect this pin closes: an ssh `-L` forward makes a tunneled
        // node's connection classify as `ConnOrigin::Loopback`
        // (`classify_origin`, `peer_addr()`) exactly like this. Before the
        // P-S6 narrowing, a loopback origin once earned delivery, so this
        // would have auto-delivered.
        let result = message_send(
            &params,
            &audit_log,
            "",
            "",
            ConnOrigin::Loopback,
            "",
            None,
            Some(caller("tunneled-node")),
        );
        assert!(result.is_ok(), "{:?}", result.err());
        assert!(listener.accept().is_err(), "a signed node's send must never touch the socket, loopback or not");

        let pending: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(stage.join("pending.json")).unwrap()).unwrap();
        let entries = pending["pending"].as_array().unwrap();
        assert_eq!(entries.len(), 1, "the send is held pending, not dropped");
        assert_eq!(entries[0]["from"], "node:tunneled-node");

        let _ = std::fs::remove_dir_all(&root);
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    // ── P-RSA S5: a remote parent's inject parks like any other ─────────
    //
    // The claim `node spawn` stamped on the child (S3) is stamped and read by
    // the history gate only. An Inject never reads it, so a remote parent's
    // inject parks in `pending.json` like every other network inject.

    const PARENT_KEY: &str = "aa11cc22dd33ff44";
    const PARENT_SESSION: &str = "par-1";
    const REMOTE_CHILD: &str = "remote-child-1";

    /// Write the S5 fixture into an ALREADY-prepared `root`: one registered,
    /// verified node (`node_name`, whose stored key is `key`), and one
    /// conductable session ([`REMOTE_CHILD`]) whose `remoteParent` names that
    /// node's key and `parent_session` — the exact shape S3's
    /// `stamp_remote_parent` leaves behind. Points `AOIDE_STAGE_DIR`/
    /// `AOIDE_STATE_DIR`/`XDG_RUNTIME_DIR` at the box and clears the ambient
    /// session vars, so the caller must already hold `env_lock`.
    ///
    /// Returns `(stage, audit_log, listener)`: the listener sits at the child's
    /// own control socket, so a delivery is observable as bytes arriving and a
    /// held-pending send as nothing arriving (bound NONBLOCKING by the callers
    /// that expect silence, like every other pending-path test here).
    fn write_remote_parent_box(
        root: &std::path::Path,
        node_name: &str,
        key: &str,
        parent_session: &str,
    ) -> (std::path::PathBuf, std::path::PathBuf, UnixListener) {
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("XDG_RUNTIME_DIR", root);
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::remove_var("AOIDE_CONDUCT_AUTOGATE");
        std::env::remove_var("AOIDE_SESSION_ID");

        let mut node = fixture_node(node_name, "http://192.0.2.51:8710/");
        node.verified = true;
        node.pubkey = Some(key.to_string());
        // P-CHARTER (review finding 5): delivering a `message/send` needs the
        // caller to hold `message` IN THE MESH ITS REQUEST NAMES — the same
        // capability the delivery itself needs. A fixture without it would
        // exercise the pending queue, not the arm under test, and would leave
        // the test's own accept-thread waiting on a connection this door
        // never dials.
        node.grants = aoide_storage::node_store::grants_in("home", &["message"]);
        aoide_storage::node_store::save_nodes(&[node]).unwrap();

        let socket = aoide_conduct::graph::conduct_socket_path(REMOTE_CHILD);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let listener = UnixListener::bind(&socket).unwrap();
        let mut child = conductable_session(REMOTE_CHILD, &socket);
        child.remote_parent = Some(RemoteParent {
            node: node_name.to_string(),
            key: key.to_string(),
            session_id: parent_session.to_string(),
            extra: Default::default(),
        });
        write_stage(
            &sessions_path(),
            &SessionsFile { schema_version: "0".to_string(), sessions: vec![child] },
        )
        .unwrap();
        (stage, root.join("log"), listener)
    }

    /// One inject-shaped `message/send` body for the S5 table, naming
    /// [`REMOTE_CHILD`] and carrying `claim` as `metadata["aoide/from"]` when
    /// given — the one place `parse_message_send_params` reads it.
    fn remote_parent_body(claim: Option<&str>) -> Value {
        let mut message = json!({
            "parts": [{ "kind": "text", "text": "steer it" }],
            "contextId": REMOTE_CHILD,
        });
        if let Some(claim) = claim {
            message["metadata"] = json!({ aoide_protocol::wire::FROM_SESSION_KEY: claim });
        }
        json!({ "message": message })
    }

    fn remote_parent_box_root(tag: &str) -> std::path::PathBuf {
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    /// Restore the two stage/state env vars an S5 test saved on entry.
    fn restore_stage_state(saved_stage: Option<String>, saved_state: Option<String>) {
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    /// End to end through the REAL signature (not the [`caller`] shorthand): the
    /// A genuinely signed node whose own key is the child's `remoteParent.key`
    /// sends an inject naming the child. The inject parks, and the door
    /// touches neither the child's socket nor anything else.
    #[test]
    fn a_genuinely_signed_remote_parent_is_held_at_the_door() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let root = remote_parent_box_root("s5-signed");
        // The record's `remoteParent.key` must be the key that will VERIFY —
        // `verify_signed_request` resolves by pubkey, so the fixture node takes
        // the minted identity's own key before the request is built.
        let (kp, _) = aoide_storage::identity::load_or_mint().unwrap();
        let pubkey = kp.info().pubkey_hex.clone();
        let (stage, audit_log, listener) =
            write_remote_parent_box(&root, "yomi-strix", &pubkey, PARENT_SESSION);

        let body = remote_parent_body(Some(PARENT_SESSION));
        let body_bytes = serde_json::to_vec(&body).unwrap();
        let now = 1_800_000_000_i64;
        let req = signed_request(&kp, "yomi-strix", "/", &body_bytes, now, &unique_nonce("s5-signed"));
        let (signed_name, signed_key) = match verify_signed_request(&req, now, 0) {
            SignedRequestOutcome::Verified { resolved, key, .. } => (resolved, key),
            other => panic!("expected a REAL genuine signature to verify, got {other:?}"),
        };

        let result = message_send(
            &body,
            &audit_log,
            "",
            "",
            ConnOrigin::Remote("192.0.2.51".parse().unwrap()),
            "",
            None,
            Some(SignedCaller { name: &signed_name, key: &signed_key, mesh: None }),
        );
        assert!(result.is_ok(), "{:?}", result.err());
        assert_held_at_the_door(&listener, &stage, 1);

        let _ = std::fs::remove_dir_all(&root);
        restore_stage_state(saved_stage, saved_state);
    }

    /// With a door token configured and no bearer presented, an UNSIGNED
    /// caller claiming a remote parent gets exactly the uniform `submitted`
    /// Task every unauthenticated inject gets (#50's guard). The `aoide/from`
    /// claim is honoured only on the signature rung, so it earns nothing here,
    /// and nothing queues.
    #[test]
    fn a_door_token_refuses_an_unauthenticated_remote_parent_uniformly() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let root = remote_parent_box_root("s5-parent-token");
        let (stage, audit_log, listener) =
            write_remote_parent_box(&root, "parent-node", PARENT_KEY, PARENT_SESSION);
        listener.set_nonblocking(true).unwrap();

        let task = message_send(
            &remote_parent_body(Some(PARENT_SESSION)),
            &audit_log,
            "",
            "",
            ConnOrigin::Loopback,
            "door-secret",
            None,
            None,
        )
        .expect("the uniform response is an ANSWER, not a refusal");

        assert_eq!(task["status"]["state"], "submitted", "the uniform Task, verbatim: {task}");
        assert_eq!(task["id"], REMOTE_CHILD);
        assert!(task.get("artifacts").is_none(), "{task}");
        assert!(listener.accept().is_err(), "a tokenless caller's send is never delivered");
        assert!(!stage.join("pending.json").exists(), "the uniform path never queues either");

        let log = std::fs::read_to_string(&audit_log).unwrap();
        assert!(log.contains("no valid token, no node identity"), "the guard's own audit: {log}");

        let _ = std::fs::remove_dir_all(&root);
        restore_stage_state(saved_stage, saved_state);
    }

    // ── Uniform-response guard, #50 (CONTRACTS.md §6 amendment, 2026-08-20) ──
    //
    // Once a token is configured, an unauthenticated `message/send` naming a
    // contextId must be impossible to distinguish from the outside whether
    // that id names a real conductable session, a known-but-not-conductable
    // one, or nothing at all — and must never touch `pending.json`. These
    // tests drive `message_send` directly, same house style as the
    // non-loopback pending-gate tests above.

    fn non_conductable_session(id: &str) -> SessionRecord {
        fixture_session(id, "working", None)
    }

    #[test]
    fn uniform_response_hides_existence_and_never_queues_when_unauthenticated() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-uniform-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::remove_var("AOIDE_CONDUCT_AUTOGATE");
        std::env::remove_var("AOIDE_SESSION_ID");

        let real_id = "real-target";
        let noncond_id = "noncond-target";
        let bogus_id = "bogus-target"; // never written to sessions.json at all

        let real_socket = aoide_conduct::graph::conduct_socket_path(real_id);
        std::fs::create_dir_all(real_socket.parent().unwrap()).unwrap();
        // If the guard wrongly fell through to do_inject, connecting here
        // would succeed — proving it never happens is the point.
        let real_listener = UnixListener::bind(&real_socket).unwrap();
        real_listener.set_nonblocking(true).unwrap();

        let sf = SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![conductable_session(real_id, &real_socket), non_conductable_session(noncond_id)],
        };
        write_stage(&sessions_path(), &sf).unwrap();

        let audit_log = root.join("log");

        // Both "no bearer at all" and "a wrong bearer" must land on the same
        // uniform answer — this is TokenState::Absent vs TokenState::Invalid,
        // both non-Valid.
        for presented in [None, Some("wrong-tok")] {
            let mut shapes = Vec::new();
            for id in [real_id, bogus_id, noncond_id] {
                let params = serde_json::json!({
                    "message": { "parts": [{ "kind": "text", "text": "probe" }], "contextId": id }
                });
                // Loopback origin too — the uniform answer holds even for the
                // origin that would otherwise get the automatic trust pass.
                let result = message_send(
                    &params,
                    &audit_log,
                    "",
                    "",
                    ConnOrigin::Loopback,
                    "s3cr3t",
                    presented,
                    None,
                );
                let task = result.expect("uniform arm always answers Ok, never a JSON-RPC error");
                assert_eq!(task["id"], id);
                assert_eq!(task["contextId"], id);
                assert_eq!(task["status"]["state"], "submitted");
                assert_eq!(task["kind"], "task");
                assert!(task["status"]["timestamp"].as_str().unwrap().ends_with('Z'));

                let mut normalized = task.clone();
                normalized["id"] = serde_json::Value::Null;
                normalized["contextId"] = serde_json::Value::Null;
                normalized["status"]["timestamp"] = serde_json::Value::Null;
                shapes.push(normalized);
            }
            assert_eq!(shapes[0], shapes[1], "real vs bogus id: byte-identical shape modulo id/timestamp");
            assert_eq!(shapes[0], shapes[2], "real vs non-conductable id: byte-identical shape modulo id/timestamp");
        }

        // Never touched the real session's control socket.
        assert!(real_listener.accept().is_err(), "the uniform arm must never attempt delivery");

        // Never wrote pending.json — no queue write for any of the three.
        let pending_path = stage.join("pending.json");
        if pending_path.exists() {
            let pending: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&pending_path).unwrap()).unwrap();
            assert!(
                pending["pending"].as_array().map(Vec::is_empty).unwrap_or(true),
                "unauthenticated sends must never feed the approval queue"
            );
        }

        let log = std::fs::read_to_string(&audit_log).unwrap_or_default();
        assert!(log.contains("\"door\":\"a2a\""), "audited through Door::A2a: {log}");
        assert!(log.contains("\"status\":\"unauthorized\""), "audited as unauthorized: {log}");
        assert!(log.contains("a2a.message/send"), "reuses message/send's own audit label: {log}");

        let _ = std::fs::remove_dir_all(&root);
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    #[test]
    fn uniform_response_guard_lets_a_valid_bearer_reach_the_real_decision() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-uniform-valid-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::remove_var("AOIDE_CONDUCT_AUTOGATE");
        std::env::remove_var("AOIDE_SESSION_ID");

        let real_id = "valid-real-target";
        let noncond_id = "valid-noncond-target";
        let bogus_id = "valid-bogus-target";

        let real_socket = aoide_conduct::graph::conduct_socket_path(real_id);
        std::fs::create_dir_all(real_socket.parent().unwrap()).unwrap();
        let real_listener = UnixListener::bind(&real_socket).unwrap();
        real_listener.set_nonblocking(true).unwrap();

        let sf = SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![conductable_session(real_id, &real_socket), non_conductable_session(noncond_id)],
        };
        write_stage(&sessions_path(), &sf).unwrap();

        let audit_log = root.join("log");
        // Remote + no registered node, so a real send is held
        // pending rather than delivered — same as the pre-#50 Inject arm,
        // and it proves `do_inject` (not the uniform guard) ran: only that
        // path writes `pending.json`.
        let remote_origin = ConnOrigin::Remote("10.0.0.9".parse().unwrap());

        let params = serde_json::json!({
            "message": { "parts": [{ "kind": "text", "text": "authed send" }], "contextId": real_id }
        });
        let result = message_send(
            &params,
            &audit_log,
            "",
            "",
            remote_origin,
            "s3cr3t",
            Some("s3cr3t"),
            None,
        );
        let task = result.expect("a valid bearer still resolves the real Inject decision");
        assert_eq!(task["id"], real_id);
        assert_eq!(task["status"]["state"], "submitted");
        assert!(real_listener.accept().is_err(), "not delivered — held pending, same as before #50");

        let pending: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(stage.join("pending.json")).unwrap()).unwrap();
        let entries = pending["pending"].as_array().unwrap();
        assert_eq!(entries.len(), 1, "a valid bearer's Inject still queues, unlike the uniform guard");
        assert_eq!(entries[0]["sessionId"], real_id);

        let bogus_params = serde_json::json!({
            "message": { "parts": [{ "kind": "text", "text": "x" }], "contextId": bogus_id }
        });
        let bogus_err = message_send(
            &bogus_params,
            &audit_log,
            "",
            "",
            remote_origin,
            "s3cr3t",
            Some("s3cr3t"),
            None,
        )
        .unwrap_err();
        assert_eq!(bogus_err.0, -32001);

        let noncond_params = serde_json::json!({
            "message": { "parts": [{ "kind": "text", "text": "x" }], "contextId": noncond_id }
        });
        let noncond_err = message_send(
            &noncond_params,
            &audit_log,
            "",
            "",
            remote_origin,
            "s3cr3t",
            Some("s3cr3t"),
            None,
        )
        .unwrap_err();
        assert_eq!(noncond_err.0, -32004);

        let _ = std::fs::remove_dir_all(&root);
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    #[test]
    fn uniform_response_guard_is_a_no_op_when_no_token_is_configured() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-uniform-off-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::remove_var("AOIDE_CONDUCT_AUTOGATE");
        std::env::remove_var("AOIDE_SESSION_ID");

        let real_id = "off-real-target";
        let noncond_id = "off-noncond-target";
        let bogus_id = "off-bogus-target";

        let real_socket = aoide_conduct::graph::conduct_socket_path(real_id);
        std::fs::create_dir_all(real_socket.parent().unwrap()).unwrap();
        let real_listener = UnixListener::bind(&real_socket).unwrap();

        let sf = SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![conductable_session(real_id, &real_socket), non_conductable_session(noncond_id)],
        };
        write_stage(&sessions_path(), &sf).unwrap();


        let audit_log = root.join("log");
        // Loopback + no token configured: off-path, held at the door like any
        // other inject. The #50 guard is not involved.
        let params = serde_json::json!({
            "message": { "parts": [{ "kind": "text", "text": "off-path send" }], "contextId": real_id }
        });
        let result = message_send(
            &params,
            &audit_log,
            "",
            "",
            ConnOrigin::Loopback,
            "",
            None,
            None,
        );
        assert!(result.is_ok(), "{:?}", result.as_ref().err());
        assert_held_at_the_door(&real_listener, &stage, 1);

        let bogus_params = serde_json::json!({
            "message": { "parts": [{ "kind": "text", "text": "x" }], "contextId": bogus_id }
        });
        let bogus_err = message_send(
            &bogus_params,
            &audit_log,
            "",
            "",
            ConnOrigin::Loopback,
            "",
            None,
            None,
        )
        .unwrap_err();
        assert_eq!(bogus_err.0, -32001);

        let noncond_params = serde_json::json!({
            "message": { "parts": [{ "kind": "text", "text": "x" }], "contextId": noncond_id }
        });
        let noncond_err = message_send(
            &noncond_params,
            &audit_log,
            "",
            "",
            ConnOrigin::Loopback,
            "",
            None,
            None,
        )
        .unwrap_err();
        assert_eq!(noncond_err.0, -32004);

        let _ = std::fs::remove_dir_all(&root);
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    #[test]
    fn uniform_response_guard_never_fires_for_a_per_node_token() {
        // A server-wide token IS configured (and the presented bearer does
        // NOT match it), but the presented token DOES match a VERIFIED
        // node's own `token_file`. A verified node's own bearer is its
        // identity, never checked against the server-wide token, so the
        // uniform guard must not swallow this send. The send parks: the
        // proof it cleared the guard is that it reaches
        // `session_ref_lookup`/`do_inject` and queues into `pending.json`,
        // which the guard's own synthetic path never does. The unverified
        // case is `an_unverified_node_bearer_gets_the_uniform_answer_behind_a_door_token`.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-uniform-ptok-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let stage = root.join("stage");
        let state = root.join("state");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        std::env::set_var("AOIDE_STATE_DIR", &state);
        std::env::remove_var("AOIDE_CONDUCT_AUTOGATE");
        std::env::remove_var("AOIDE_SESSION_ID");

        let token_path = root.join("node.token");
        std::fs::write(&token_path, "node-secret\n").unwrap();
        aoide_storage::node_store::save_nodes(&[aoide_storage::node_store::Node {
            name: "enrolled-node".into(),
            url: "http://192.0.2.99:8710/".into(),
            token_file: Some(token_path.to_string_lossy().into_owned()),
            bearer_secret: None,
            hub: false,
            pubkey: None,
            verified: true,
            grants: aoide_storage::node_store::Grants::new(),
            narrowed: aoide_storage::node_store::Grants::new(),
            via: None,
            added_at: "2026-08-20T00:00:00Z".into(),
        }])
        .unwrap();

        let id = "ptok-still-injects";
        let socket = aoide_conduct::graph::conduct_socket_path(id);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        // Nonblocking + no acceptor thread: this send is held pending (see
        // above), so a connection must never actually land here — a blocking
        // `accept()` would hang forever waiting for a delivery that never
        // comes, exactly the trap the ORIGINAL (wrong) version of this test
        // fell into.
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();

        let sf = SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![conductable_session(id, &socket)],
        };
        write_stage(&sessions_path(), &sf).unwrap();

        let audit_log = root.join("log");
        let params = serde_json::json!({
            "message": { "parts": [{ "kind": "text", "text": "per-node authed" }], "contextId": id }
        });
        let remote_origin = ConnOrigin::Remote("10.0.0.9".parse().unwrap());
        // "server-secret" is configured server-wide; "node-secret" (what's
        // presented) does NOT match it — only the per-node token match
        // saves this from the #50 uniform guard.
        let result = message_send(
            &params,
            &audit_log,
            "",
            "",
            remote_origin,
            "server-secret",
            Some("node-secret"),
            None,
        );
        let task = result.expect("the per-node token clears the #50 guard, so it's still an Ok Task");
        assert_eq!(task["id"], id);
        assert_eq!(task["status"]["state"], "submitted");
        assert!(listener.accept().is_err(), "held pending, not delivered — the non-Valid server bearer still coerces Unknown");

        // The proof this reached REAL Inject machinery (not the #50 guard):
        // `pending.json` carries the queued entry, exactly like an
        // authenticated-but-not-delivered send does.
        let pending: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(stage.join("pending.json")).unwrap()).unwrap();
        let entries = pending["pending"].as_array().unwrap();
        assert_eq!(entries.len(), 1, "the per-node token reaches do_inject/session_ref_lookup, unlike the #50 guard");
        assert_eq!(entries[0]["sessionId"], id);

        let _ = std::fs::remove_dir_all(&root);
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    /// Shared setup for the two uniform-guard admission tests below: a door
    /// token is configured, the stage and state point at one box, and the
    /// session `id` is conductable with a nonblocking listener at its socket.
    fn uniform_guard_box(tag: &str, id: &str) -> (std::path::PathBuf, std::path::PathBuf, UnixListener) {
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::remove_var("AOIDE_CONDUCT_AUTOGATE");
        std::env::remove_var("AOIDE_SESSION_ID");
        let socket = aoide_conduct::graph::conduct_socket_path(id);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        let sf = SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![conductable_session(id, &socket)],
        };
        write_stage(&sessions_path(), &sf).unwrap();
        (root, stage, listener)
    }

    #[test]
    fn a_verified_signed_node_clears_the_uniform_guard_behind_a_door_token() {
        // A paired node's verified signature, with `message` granted in the
        // mesh it signed for, is a node identity: its send reaches the queue.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let id = "sig-guard-target";
        let (root, stage, listener) = uniform_guard_box("sig-guard", id);

        let mut paired = fixture_node("paired-node", "http://192.0.2.88:8710/");
        paired.verified = true;
        paired.pubkey = Some("ab12".to_string());
        paired.grants = aoide_storage::node_store::grants_in("home", &["message"]);
        aoide_storage::node_store::save_nodes(&[paired]).unwrap();

        let params = serde_json::json!({
            "message": { "parts": [{ "kind": "text", "text": "signed hello" }], "contextId": id }
        });
        let signed = SignedCaller { name: "paired-node", key: "ab12", mesh: Some("home") };
        let result = message_send(
            &params,
            &root.join("log"),
            "",
            "",
            ConnOrigin::Remote("192.0.2.88".parse().unwrap()),
            "server-secret",
            None,
            Some(signed),
        );
        let task = result.expect("a verified node is never answered by the guard's error path");
        assert_eq!(task["id"], id);
        assert_held_at_the_door(&listener, &stage, 1);

        let _ = std::fs::remove_dir_all(&root);
        restore_stage_state(saved_stage, saved_state);
    }

    #[test]
    fn a_signed_node_without_message_in_the_signed_mesh_gets_the_uniform_answer() {
        // A valid signature proves who is calling, not that this box takes its
        // messages. Without `message` in the mesh it signed for (here it holds
        // `read` there, and `message` only in another mesh), the send is
        // answered like an unauthenticated one and nothing queues.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let id = "sig-nogrant-target";
        let (root, stage, listener) = uniform_guard_box("sig-nogrant", id);

        let mut paired = fixture_node("paired-node", "http://192.0.2.88:8710/");
        paired.verified = true;
        paired.pubkey = Some("ab12".to_string());
        paired.grants = aoide_storage::node_store::grants_in("home", &["read"]);
        paired.grants.insert("work".to_string(), vec!["message".to_string()]);
        aoide_storage::node_store::save_nodes(&[paired]).unwrap();

        let params = serde_json::json!({
            "message": { "parts": [{ "kind": "text", "text": "signed, ungranted" }], "contextId": id }
        });
        let audit_log = root.join("log");
        let result = message_send(
            &params,
            &audit_log,
            "",
            "",
            ConnOrigin::Remote("192.0.2.88".parse().unwrap()),
            "server-secret",
            None,
            Some(SignedCaller { name: "paired-node", key: "ab12", mesh: Some("home") }),
        );
        let task = result.expect("the uniform arm answers Ok, never a JSON-RPC error");
        assert_eq!(task["status"]["state"], "submitted");
        assert!(!stage.join("pending.json").exists(), "the uniform path never queues");
        assert!(listener.accept().is_err(), "and never touches the socket");
        let log = std::fs::read_to_string(&audit_log).unwrap_or_default();
        assert!(log.contains("no node identity"), "the guard's own audit: {log}");

        let _ = std::fs::remove_dir_all(&root);
        restore_stage_state(saved_stage, saved_state);
    }

    #[test]
    fn an_unverified_node_bearer_gets_the_uniform_answer_behind_a_door_token() {
        // A node's own `tokenFile` bearer names it, but a record that was never
        // verified by pairing is no identity this box trusts: the send is
        // answered like an unauthenticated one and nothing queues.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let id = "unverified-tok-target";
        let (root, stage, listener) = uniform_guard_box("unverified-tok", id);

        let token_path = root.join("node.token");
        std::fs::write(&token_path, "node-secret\n").unwrap();
        let mut enrolled = fixture_node("enrolled-node", "http://192.0.2.99:8710/");
        enrolled.token_file = Some(token_path.to_string_lossy().into_owned());
        aoide_storage::node_store::save_nodes(&[enrolled]).unwrap();

        let params = serde_json::json!({
            "message": { "parts": [{ "kind": "text", "text": "unverified bearer" }], "contextId": id }
        });
        let audit_log = root.join("log");
        let result = message_send(
            &params,
            &audit_log,
            "",
            "",
            ConnOrigin::Remote("10.0.0.9".parse().unwrap()),
            "server-secret",
            Some("node-secret"),
            None,
        );
        let task = result.expect("the uniform arm answers Ok, never a JSON-RPC error");
        assert_eq!(task["status"]["state"], "submitted");
        assert!(!stage.join("pending.json").exists(), "the uniform path never queues");
        assert!(listener.accept().is_err(), "and never touches the socket");
        let log = std::fs::read_to_string(&audit_log).unwrap_or_default();
        assert!(log.contains("no node identity"), "the guard's own audit: {log}");

        let _ = std::fs::remove_dir_all(&root);
        restore_stage_state(saved_stage, saved_state);
    }

    /// Fill `from`'s share of the pending queue to the cap, the way the door
    /// itself queues: through `hold_pending`.
    fn fill_source(id: &str, from: &str) {
        for n in 0..aoide_conduct::graph::PENDING_CAP_PER_SOURCE {
            let log = aoide_protocol::default_audit_log();
            aoide_conduct::graph::hold_pending(id, &format!("{from} #{n}"), true, Some(from), &log).unwrap();
        }
    }

    fn queued_from(stage: &std::path::Path, from: &str) -> Vec<String> {
        let queue: Value =
            serde_json::from_str(&std::fs::read_to_string(stage.join("pending.json")).unwrap()).unwrap();
        queue["pending"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["from"] == from)
            .map(|e| e["text"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn an_oversize_inject_answers_its_own_code_and_queues_nothing() {
        // A prompt over the held-entry cap is refused `-32602` (invalid params),
        // not the uniform `-32603` of a failed write, and the queue is untouched.
        // Before the change the same refusal answered `-32603`.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let id = "oversize-target";
        let (root, stage, listener) = uniform_guard_box("oversize", id);

        let path = root.join("node.token");
        std::fs::write(&path, "node-secret\n").unwrap();
        let mut node = fixture_node("big-node", "http://192.0.2.99:8710/");
        node.token_file = Some(path.to_string_lossy().into_owned());
        node.verified = true;
        aoide_storage::node_store::save_nodes(&[node]).unwrap();

        let big = "x".repeat(aoide_conduct::graph::PENDING_ENTRY_MAX_BYTES + 1);
        let params = serde_json::json!({
            "message": { "parts": [{ "kind": "text", "text": big }], "contextId": id }
        });
        let audit_log = root.join("log");
        let (code, msg) = message_send(
            &params,
            &audit_log,
            "",
            "",
            ConnOrigin::Remote("10.0.0.9".parse().unwrap()),
            "server-secret",
            Some("node-secret"),
            None,
        )
        .expect_err("an oversize prompt is refused");
        assert_eq!(code, INVALID_PARAMS_CODE, "{msg}");
        assert!(!stage.join("pending.json").exists(), "nothing queued");
        assert!(listener.accept().is_err(), "nothing is delivered");

        let _ = std::fs::remove_dir_all(&root);
        restore_stage_state(saved_stage, saved_state);
    }

    #[test]
    fn a_full_source_refuses_a_proven_node_and_leaves_another_source_alone() {
        // A proven node (a verified node's own bearer) whose source already
        // holds the cap is refused `-32000`, audited, and nothing queued moves.
        // Another node's send still queues.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let id = "cap-target";
        let (root, stage, listener) = uniform_guard_box("cap-proven", id);

        let mut nodes = Vec::new();
        for (name, secret) in [("full-node", "full-secret"), ("other-node", "other-secret")] {
            let path = root.join(format!("{name}.token"));
            std::fs::write(&path, format!("{secret}\n")).unwrap();
            let mut node = fixture_node(name, "http://192.0.2.99:8710/");
            node.token_file = Some(path.to_string_lossy().into_owned());
            node.verified = true;
            nodes.push(node);
        }
        aoide_storage::node_store::save_nodes(&nodes).unwrap();
        fill_source(id, "node:full-node");
        let before = queued_from(&stage, "node:full-node");
        assert_eq!(before.len(), aoide_conduct::graph::PENDING_CAP_PER_SOURCE);

        let params = serde_json::json!({
            "message": { "parts": [{ "kind": "text", "text": "one too many" }], "contextId": id }
        });
        let audit_log = root.join("log");
        let send = |bearer: &str| {
            message_send(
                &params,
                &audit_log,
                "",
                "",
                ConnOrigin::Remote("10.0.0.9".parse().unwrap()),
                "server-secret",
                Some(bearer),
                None,
            )
        };

        let (code, msg) = send("full-secret").expect_err("a proven node past the cap is told");
        assert_eq!(code, QUEUE_FULL_CODE, "{msg}");
        assert!(msg.contains("node:full-node") && msg.contains("pending sends"), "{msg}");
        assert_eq!(queued_from(&stage, "node:full-node"), before, "the first N are intact, in order");
        let log = std::fs::read_to_string(&audit_log).unwrap_or_default();
        assert!(
            log.lines().any(|l| l.contains("\"status\":\"refused\"") && l.contains("one too many")),
            "the refusal is audited: {log}"
        );

        send("other-secret").expect("another source still queues");
        assert_eq!(queued_from(&stage, "node:other-node"), ["one too many"]);
        assert!(listener.accept().is_err(), "nothing is delivered either way");

        let _ = std::fs::remove_dir_all(&root);
        restore_stage_state(saved_stage, saved_state);
    }

    #[test]
    fn a_full_source_answers_a_caller_that_is_not_proven_uniformly() {
        // With no door token every caller reaches the queue, so the cap is the
        // only bound. A caller named by address alone proves nothing, so when
        // its own address already holds the cap the answer is the same
        // synthetic `submitted` Task the guard gives an unauthenticated send,
        // and nothing is queued.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let id = "cap-unproven-target";
        let (root, stage, listener) = uniform_guard_box("cap-unproven", id);

        aoide_storage::node_store::save_nodes(&[fixture_node("addr-node", "http://192.0.2.89:8710/")]).unwrap();
        fill_source(id, "addr:192.0.2.89");
        let before = queued_from(&stage, "addr:192.0.2.89");

        let params = serde_json::json!({
            "message": { "parts": [{ "kind": "text", "text": "address only, one too many" }], "contextId": id }
        });
        let audit_log = root.join("log");
        let task = message_send(
            &params,
            &audit_log,
            "",
            "",
            ConnOrigin::Remote("192.0.2.89".parse().unwrap()),
            "",
            None,
            None,
        )
        .expect("an unproven caller is never told the queue is full");
        assert_eq!(task, {
            let mut uniform = submitted_task(id);
            uniform["status"]["timestamp"] = task["status"]["timestamp"].clone();
            uniform
        });
        assert_eq!(queued_from(&stage, "addr:192.0.2.89"), before, "nothing queued, nothing moved");
        let log = std::fs::read_to_string(&audit_log).unwrap_or_default();
        assert!(log.contains("\"status\":\"refused\""), "the refusal is still audited: {log}");
        assert!(listener.accept().is_err());

        let _ = std::fs::remove_dir_all(&root);
        restore_stage_state(saved_stage, saved_state);
    }

    /// One registered node at `192.0.2.89` that can prove itself with its own
    /// bearer (`node-secret`), for the tests that put an address-only caller
    /// at the same address.
    fn bearer_node_at_the_callers_address(root: &std::path::Path) {
        let path = root.join("addr-node.token");
        std::fs::write(&path, "node-secret\n").unwrap();
        let mut node = fixture_node("addr-node", "http://192.0.2.89:8710/");
        node.token_file = Some(path.to_string_lossy().into_owned());
        node.verified = true;
        aoide_storage::node_store::save_nodes(&[node]).unwrap();
    }

    #[test]
    fn an_address_only_caller_fills_its_own_bucket_and_never_the_nodes() {
        // With no door token an address-only caller at a registered node's
        // address parks. It counts against `addr:<address>`: 32 entries fill
        // that bucket, the next is answered uniformly and queues nothing, no
        // entry ever carries the node's name, and the real node, proving itself
        // with its own bearer from the same address, still queues.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let id = "addr-bucket-target";
        let (root, stage, listener) = uniform_guard_box("addr-bucket", id);
        bearer_node_at_the_callers_address(&root);

        let audit_log = root.join("log");
        let send = |text: &str, bearer: Option<&str>| {
            let params = serde_json::json!({
                "message": { "parts": [{ "kind": "text", "text": text }], "contextId": id }
            });
            message_send(
                &params,
                &audit_log,
                "",
                "",
                ConnOrigin::Remote("192.0.2.89".parse().unwrap()),
                "",
                bearer,
                None,
            )
        };
        let cap = aoide_conduct::graph::PENDING_CAP_PER_SOURCE;
        for n in 0..cap + 1 {
            send(&format!("address only {n}"), None).expect("an unproven caller always gets a task");
        }
        assert_eq!(queued_from(&stage, "addr:192.0.2.89").len(), cap, "the address's bucket stops at the cap");
        assert!(
            queued_from(&stage, "node:addr-node").is_empty(),
            "an address match never lands in the node's bucket"
        );

        send("the node itself", Some("node-secret")).expect("the proven node is not told its bucket is full");
        assert_eq!(queued_from(&stage, "node:addr-node"), ["the node itself"]);
        assert_eq!(queued_from(&stage, "addr:192.0.2.89").len(), cap, "and the address's bucket did not move");
        assert!(listener.accept().is_err(), "nothing is delivered either way");

        let _ = std::fs::remove_dir_all(&root);
        restore_stage_state(saved_stage, saved_state);
    }

    #[test]
    fn a_full_node_bucket_does_not_turn_an_address_only_caller_away() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let id = "node-full-target";
        let (root, stage, _listener) = uniform_guard_box("node-full", id);
        bearer_node_at_the_callers_address(&root);
        fill_source(id, "node:addr-node");

        let params = serde_json::json!({
            "message": { "parts": [{ "kind": "text", "text": "from the address" }], "contextId": id }
        });
        message_send(
            &params,
            &root.join("log"),
            "",
            "",
            ConnOrigin::Remote("192.0.2.89".parse().unwrap()),
            "",
            None,
            None,
        )
        .expect("a task");
        assert_eq!(queued_from(&stage, "addr:192.0.2.89"), ["from the address"]);
        assert_eq!(
            queued_from(&stage, "node:addr-node").len(),
            aoide_conduct::graph::PENDING_CAP_PER_SOURCE,
            "the node's own bucket is untouched"
        );

        let _ = std::fs::remove_dir_all(&root);
        restore_stage_state(saved_stage, saved_state);
    }

    #[test]
    fn a_door_token_holder_at_a_nodes_address_is_recorded_by_address() {
        // The other way an address-matched caller can park: it presents the
        // door-wide token. That token names no node, so the address rung still
        // resolves and still proves nothing.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let id = "door-token-target";
        let (root, stage, _listener) = uniform_guard_box("door-token", id);
        bearer_node_at_the_callers_address(&root);

        let params = serde_json::json!({
            "message": { "parts": [{ "kind": "text", "text": "door token" }], "contextId": id }
        });
        message_send(
            &params,
            &root.join("log"),
            "",
            "",
            ConnOrigin::Remote("192.0.2.89".parse().unwrap()),
            "server-secret",
            Some("server-secret"),
            None,
        )
        .expect("a task");
        assert_eq!(queued_from(&stage, "addr:192.0.2.89"), ["door token"]);
        assert!(queued_from(&stage, "node:addr-node").is_empty());

        let _ = std::fs::remove_dir_all(&root);
        restore_stage_state(saved_stage, saved_state);
    }

    #[test]
    fn an_injects_source_is_the_node_only_for_a_proven_rung() {
        use aoide_storage::node_store::NodeRung::{Addr, Signature, Token};
        let mut node = fixture_node("peer", "http://192.0.2.7:8710/");
        let remote = ConnOrigin::Remote("192.0.2.7".parse().unwrap());
        assert_eq!(inject_source(Some((&node, Signature)), remote), "node:peer");
        assert_eq!(
            inject_source(Some((&node, Token)), remote),
            "addr:192.0.2.7",
            "the bearer of an unverified node proves nothing"
        );
        node.verified = true;
        assert_eq!(inject_source(Some((&node, Token)), remote), "node:peer");
        assert_eq!(inject_source(Some((&node, Addr)), remote), "addr:192.0.2.7");
        assert_eq!(inject_source(None, remote), "addr:192.0.2.7");
        assert_eq!(inject_source(None, ConnOrigin::Loopback), "addr:loopback");
        assert_eq!(inject_source(None, ConnOrigin::Unknown), "addr:unknown");
        assert_eq!(
            inject_source(None, ConnOrigin::Remote("::ffff:192.0.2.7".parse().unwrap())),
            "addr:192.0.2.7",
            "an IPv4 caller on a dual-stack socket is one source, not two"
        );
    }

    #[test]
    fn an_address_match_alone_gets_the_uniform_answer_behind_a_door_token() {
        // An address proves no identity. A registered node's host that presents
        // neither its own bearer nor a signature gets the uniform answer, and
        // nothing reaches the queue.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let id = "addr-guard-target";
        let (root, stage, listener) = uniform_guard_box("addr-guard", id);

        aoide_storage::node_store::save_nodes(&[fixture_node("addr-node", "http://192.0.2.89:8710/")]).unwrap();

        let params = serde_json::json!({
            "message": { "parts": [{ "kind": "text", "text": "address only" }], "contextId": id }
        });
        let audit_log = root.join("log");
        let result = message_send(
            &params,
            &audit_log,
            "",
            "",
            ConnOrigin::Remote("192.0.2.89".parse().unwrap()),
            "server-secret",
            None,
            None,
        );
        let task = result.expect("the uniform arm answers Ok, never a JSON-RPC error");
        assert_eq!(task["status"]["state"], "submitted");
        assert!(!stage.join("pending.json").exists(), "the uniform path never queues");
        assert!(listener.accept().is_err(), "and never touches the socket");
        let log = std::fs::read_to_string(&audit_log).unwrap_or_default();
        assert!(log.contains("no node identity"), "the guard's own audit: {log}");

        let _ = std::fs::remove_dir_all(&root);
        restore_stage_state(saved_stage, saved_state);
    }

    // ── `resolve_node_name` precedence (flag → env → hostname → default) ────

    #[test]
    fn resolve_node_name_prefers_flag_then_env_then_falls_back() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_A2A_NODE_NAME").ok();

        let mut flags = std::collections::BTreeMap::new();
        flags.insert("node-name".to_string(), "flag-name".to_string());
        let inv = Invocation { path: vec![], args: vec![], flags, door: Door::Cli };
        std::env::set_var("AOIDE_A2A_NODE_NAME", "env-name");
        assert_eq!(resolve_node_name(&inv), "flag-name", "an explicit flag wins outright");

        let inv_no_flag = Invocation {
            path: vec![],
            args: vec![],
            flags: std::collections::BTreeMap::new(),
            door: Door::Cli,
        };
        assert_eq!(resolve_node_name(&inv_no_flag), "env-name", "falls back to the env var");

        std::env::remove_var("AOIDE_A2A_NODE_NAME");
        // Falls back to the OS hostname (or, failing that, "aoide") — either
        // way, never empty.
        assert!(!resolve_node_name(&inv_no_flag).is_empty());

        match saved {
            Some(v) => std::env::set_var("AOIDE_A2A_NODE_NAME", v),
            None => std::env::remove_var("AOIDE_A2A_NODE_NAME"),
        }
    }

    // ── `resolve_discovery_advertise` (P-P6) — off unless a flag or a
    // ── truthy env explicitly opts in. This is the launch-time FORCE-ON
    // ── half `serve` hands `discovery::spawn_advertiser`; the runtime
    // ── half is `aoide_storage::advertise::enabled()` (its own crate's
    // ── tests), OR'd in per tick. `false` here + switch off (the
    // ── default) = a tick that sends nothing — proven with no real
    // ── thread, socket, or sleep involved. ──────────────────────────────

    #[test]
    fn resolve_discovery_advertise_is_off_by_default() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_DISCOVERY_ADVERTISE").ok();
        std::env::remove_var("AOIDE_DISCOVERY_ADVERTISE");

        let inv = Invocation { path: vec![], args: vec![], flags: std::collections::BTreeMap::new(), door: Door::Cli };
        assert!(!resolve_discovery_advertise(&inv), "no flag, no env — discovery stays off");

        match saved {
            Some(v) => std::env::set_var("AOIDE_DISCOVERY_ADVERTISE", v),
            None => std::env::remove_var("AOIDE_DISCOVERY_ADVERTISE"),
        }
    }

    #[test]
    fn resolve_discovery_advertise_honors_the_flag_and_the_truthy_env_vocabulary() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_DISCOVERY_ADVERTISE").ok();

        let mut flags = std::collections::BTreeMap::new();
        flags.insert("discovery-advertise".to_string(), String::new());
        let flag_inv = Invocation { path: vec![], args: vec![], flags, door: Door::Cli };
        std::env::remove_var("AOIDE_DISCOVERY_ADVERTISE");
        assert!(resolve_discovery_advertise(&flag_inv), "bare flag presence turns it on");

        let no_flag_inv = Invocation { path: vec![], args: vec![], flags: std::collections::BTreeMap::new(), door: Door::Cli };
        for truthy in ["1", "true", "yes", "all"] {
            std::env::set_var("AOIDE_DISCOVERY_ADVERTISE", truthy);
            assert!(resolve_discovery_advertise(&no_flag_inv), "`{truthy}` must be truthy");
        }
        for not_truthy in ["0", "false", "no", "", "TRUE", "garbage"] {
            std::env::set_var("AOIDE_DISCOVERY_ADVERTISE", not_truthy);
            assert!(!resolve_discovery_advertise(&no_flag_inv), "`{not_truthy}` must NOT be truthy");
        }

        match saved {
            Some(v) => std::env::set_var("AOIDE_DISCOVERY_ADVERTISE", v),
            None => std::env::remove_var("AOIDE_DISCOVERY_ADVERTISE"),
        }
    }

    // ── `aoide/graphSummary` (CONTRACTS.md §7) ───────────────────────────────

    #[test]
    fn graph_summary_wraps_the_resolved_graph_document_verbatim() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = std::env::temp_dir().join(format!(
            "aoide-a2a-graphsummary-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        let resp = graph_summary("test-instance", "http://127.0.0.1:8710/").unwrap();
        assert_eq!(resp["schemaVersion"], "0");
        assert_eq!(resp["instance"]["name"], "test-instance");
        assert_eq!(resp["instance"]["url"], "http://127.0.0.1:8710/");
        assert!(resp["instance"]["emittedAt"].as_str().unwrap().ends_with('Z'));
        // `graph` is EXACTLY what `resolve_graph_document` (the same function
        // bare `graph` uses) produces — no second vocabulary.
        assert_eq!(resp["graph"], resolve_graph_document().unwrap());
        assert_eq!(resp["graph"]["schemaVersion"], "0");
        assert!(resp["graph"]["nodes"].is_array());
        assert!(resp["graph"]["edges"].is_array());

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn handle_jsonrpc_routes_aoide_graph_summary_and_still_32601s_everything_else() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = std::env::temp_dir().join(format!(
            "aoide-a2a-graphsummary-rpc-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        let req = json!({ "jsonrpc": "2.0", "id": 1, "method": "aoide/graphSummary", "params": {} });
        let resp = handle_jsonrpc(&req, &test_ctx(Path::new("/dev/null"), ""));
        assert_eq!(resp["result"]["schemaVersion"], "0");
        assert_eq!(resp["result"]["instance"]["name"], "aoide");
        assert!(resp["result"]["graph"]["nodes"].is_array());

        // An unrelated unknown method is still a clean -32601, unaffected by
        // the new method joining the dispatch table (JSON-RPC spec).
        let req2 = json!({ "jsonrpc": "2.0", "id": 2, "method": "aoide/notARealMethod", "params": {} });
        let resp2 = handle_jsonrpc(&req2, &test_ctx(Path::new("/dev/null"), ""));
        assert_eq!(resp2["error"]["code"], -32601);

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    // ── The pairing ceremony wire (CONTRACTS.md §6, P-P2) ────────────────────

    /// Set `AOIDE_STATE_DIR`/`AOIDE_STAGE_DIR` to a fresh tempdir under
    /// `root` — used to stand in for "acting as one box" in the sequential
    /// ceremony test below, which plays BOTH box A and box B in the same
    /// process by swapping this env between steps (never concurrently —
    /// `aoide-storage`'s state resolution is process-global, so two REAL
    /// concurrent identities cannot coexist in one test binary; the
    /// REAL two-instance HTTP proof for the pre-existing `node
    /// add`/`pull` surface, `cli/tests/node_connectivity.rs`, is the
    /// pattern this test cannot fully match for THIS feature — the ceremony
    /// writes DISTINCT per-side state (each box's own `nodes.json`/
    /// identity), unlike a read-only `graphSummary` pull, which that
    /// existing test's two threads can share one state dir for).
    fn act_as(root: &std::path::Path, who: &str) {
        let dir = root.join(who);
        std::fs::create_dir_all(dir.join("stage")).unwrap();
        std::env::set_var("AOIDE_STATE_DIR", dir.join("state"));
        std::env::set_var("AOIDE_STAGE_DIR", dir.join("stage"));
    }

    #[test]
    fn pair_request_parks_and_returns_the_approvers_public_identity_and_nonce() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-pairrequest-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        act_as(&root, "b");

        let audit_log = root.join("log");
        let commit = aoide_storage::pairing::derive_commit(&"a".repeat(64), &"c".repeat(16));
        let params = json!({
            "pubkeyHex": "a".repeat(64),
            "name": "box-a",
            "commitHex": commit,
            "url": "http://box-a:8710/",
        });
        let resp = pair_request(&params, ConnOrigin::Remote("10.0.0.5".parse().unwrap()), &audit_log).unwrap();
        assert!(resp["id"].as_str().unwrap().len() == 8);
        assert!(valid_pubkey_hex(resp["pubkeyHex"].as_str().unwrap()), "B's own real pubkey, not A's");
        assert_ne!(resp["pubkeyHex"], "a".repeat(64), "B answers with its OWN key, not an echo of A's");
        assert!(valid_nonce_hex(resp["nonceHex"].as_str().unwrap()));
        assert!(resp["expiresAt"].as_str().unwrap().ends_with('Z'));

        // Parked on disk, origin recorded for display, NOT YET revealed —
        // no nonce, so no SAS to show, until `aoide/pairReveal` runs.
        let now_epoch = aoide_storage::time::parse_iso_utc(&now_iso_utc()).unwrap();
        let pending = aoide_storage::pairing::list_inbound(now_epoch);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].name, "box-a");
        assert_eq!(pending[0].origin_addr, "10.0.0.5");
        assert_eq!(pending[0].url, "http://box-a:8710/");
        assert_eq!(pending[0].commit_hex, commit);
        assert!(pending[0].requester_nonce_hex.is_none(), "unrevealed at request time");

        // Audited.
        let log = std::fs::read_to_string(&audit_log).unwrap_or_default();
        assert!(log.contains("a2a.pairRequest"), "{log}");

        let _ = std::fs::remove_dir_all(&root);
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
    }

    #[test]
    fn pair_request_rejects_malformed_pubkey_name_commit_or_url_before_parking_anything() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-pairrequest-invalid-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        act_as(&root, "b");
        let audit_log = root.join("log");

        let commit = aoide_storage::pairing::derive_commit(&"a".repeat(64), &"c".repeat(16));
        let good = json!({ "pubkeyHex": "a".repeat(64), "name": "box-a", "commitHex": commit, "url": "http://a/" });

        let bad_pubkey = { let mut p = good.clone(); p["pubkeyHex"] = json!("too-short"); p };
        assert_eq!(pair_request(&bad_pubkey, ConnOrigin::Loopback, &audit_log).unwrap_err().0, -32602);

        let bad_name = { let mut p = good.clone(); p["name"] = json!("../../evil"); p };
        assert_eq!(pair_request(&bad_name, ConnOrigin::Loopback, &audit_log).unwrap_err().0, -32602);

        let bad_commit = { let mut p = good.clone(); p["commitHex"] = json!("zz"); p };
        assert_eq!(pair_request(&bad_commit, ConnOrigin::Loopback, &audit_log).unwrap_err().0, -32602);

        let bad_url = { let mut p = good.clone(); p["url"] = json!(""); p };
        assert_eq!(pair_request(&bad_url, ConnOrigin::Loopback, &audit_log).unwrap_err().0, -32602);

        let now_epoch = aoide_storage::time::parse_iso_utc(&now_iso_utc()).unwrap();
        assert!(
            aoide_storage::pairing::list_inbound(now_epoch).is_empty(),
            "nothing was parked — every malformed request was refused first"
        );

        let _ = std::fs::remove_dir_all(&root);
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
    }

    #[test]
    fn pair_request_refuses_beyond_the_park_cap() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_cap = std::env::var(aoide_storage::pairing::PAIRING_PARK_CAP_ENV).ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-pairrequest-cap-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        act_as(&root, "b");
        std::env::set_var(aoide_storage::pairing::PAIRING_PARK_CAP_ENV, "1");
        let audit_log = root.join("log");

        // DISTINCT pubkeys (R3, `aoide_storage::pairing`'s own module doc) —
        // a SAME-pubkey retry now supersedes rather than refusing, so this
        // cap-refusal test needs two genuinely different identities to
        // still exercise the cap itself.
        let request = |pubkey_byte: char, name: &str| {
            let pubkey = pubkey_byte.to_string().repeat(64);
            let commit = aoide_storage::pairing::derive_commit(&pubkey, &"c".repeat(16));
            json!({ "pubkeyHex": pubkey, "name": name, "commitHex": commit, "url": "http://a/" })
        };
        pair_request(&request('a', "box-a"), ConnOrigin::Loopback, &audit_log).expect("first request is under the cap");
        let err = pair_request(&request('b', "box-c"), ConnOrigin::Loopback, &audit_log).unwrap_err();
        assert_eq!(err.0, -32000, "a distinct code from ordinary invalid-params -32602");

        let now_epoch = aoide_storage::time::parse_iso_utc(&now_iso_utc()).unwrap();
        assert_eq!(aoide_storage::pairing::list_inbound(now_epoch).len(), 1, "the refused request wrote nothing");

        let _ = std::fs::remove_dir_all(&root);
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        match saved_cap {
            Some(v) => std::env::set_var(aoide_storage::pairing::PAIRING_PARK_CAP_ENV, v),
            None => std::env::remove_var(aoide_storage::pairing::PAIRING_PARK_CAP_ENV),
        }
    }

    /// R3, door level: a second `aoide/pairRequest` from the SAME pubkey
    /// evicts the first parked entry rather than coexisting with it — the
    /// audit line names the evicted id, but the WIRE response stays the
    /// ordinary fresh-id shape (CONTRACTS.md §6 — the evicted id never
    /// rides the wire).
    #[test]
    fn pair_request_from_the_same_pubkey_supersedes_the_prior_parked_request_and_audits_it() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-pairrequest-supersede-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        act_as(&root, "b");
        let audit_log = root.join("log");

        let pubkey = "a".repeat(64);
        let request = |name: &str, nonce_byte: char| {
            let commit = aoide_storage::pairing::derive_commit(&pubkey, &nonce_byte.to_string().repeat(16));
            json!({ "pubkeyHex": pubkey, "name": name, "commitHex": commit, "url": "http://a/" })
        };

        let resp1 = pair_request(&request("box-a", 'c'), ConnOrigin::Loopback, &audit_log).unwrap();
        let id1 = resp1["id"].as_str().unwrap().to_string();

        let resp2 = pair_request(&request("box-a-retry", 'd'), ConnOrigin::Loopback, &audit_log).unwrap();
        let id2 = resp2["id"].as_str().unwrap().to_string();
        assert_ne!(id1, id2, "the superseding request gets its own fresh id");

        // Exactly one parked entry survives — the newer one.
        let now_epoch = aoide_storage::time::parse_iso_utc(&now_iso_utc()).unwrap();
        let pending = aoide_storage::pairing::list_inbound(now_epoch);
        assert_eq!(pending.len(), 1, "the first entry is superseded, not left coexisting");
        assert_eq!(pending[0].id, id2);
        assert_eq!(pending[0].name, "box-a-retry");

        // The wire response stays exactly the ordinary four keys.
        let keys: std::collections::BTreeSet<String> = resp2.as_object().unwrap().keys().cloned().collect();
        assert_eq!(
            keys,
            ["expiresAt", "id", "nonceHex", "pubkeyHex"].iter().map(|s| s.to_string()).collect::<std::collections::BTreeSet<_>>(),
            "the wire response never grows an evicted-id field"
        );

        // The audit log names the supersede and the evicted id.
        let log = std::fs::read_to_string(&audit_log).unwrap_or_default();
        assert!(log.contains(&format!("superseding {id1}")), "{log}");

        let _ = std::fs::remove_dir_all(&root);
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
    }

    #[test]
    fn pair_request_from_a_case_varied_pubkey_still_supersedes_the_prior_parked_request() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-pairrequest-supersede-case-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        act_as(&root, "b");
        let audit_log = root.join("log");

        let pubkey_lower = "a".repeat(64);
        let pubkey_case_varied = format!("{}A", "a".repeat(63));
        let request = |pubkey: &str, name: &str, nonce_byte: char| {
            let commit = aoide_storage::pairing::derive_commit(pubkey, &nonce_byte.to_string().repeat(16));
            json!({ "pubkeyHex": pubkey, "name": name, "commitHex": commit, "url": "http://a/" })
        };

        let resp1 = pair_request(&request(&pubkey_lower, "box-a", 'c'), ConnOrigin::Loopback, &audit_log).unwrap();
        let id1 = resp1["id"].as_str().unwrap().to_string();

        // Same key, one hex character uppercased on the retry — the wire's
        // own `valid_pubkey_hex` already accepts either case.
        let resp2 = pair_request(&request(&pubkey_case_varied, "box-a-retry", 'd'), ConnOrigin::Loopback, &audit_log).unwrap();
        let id2 = resp2["id"].as_str().unwrap().to_string();
        assert_ne!(id1, id2, "the superseding request gets its own fresh id");

        let now_epoch = aoide_storage::time::parse_iso_utc(&now_iso_utc()).unwrap();
        let pending = aoide_storage::pairing::list_inbound(now_epoch);
        assert_eq!(pending.len(), 1, "a hex-case-varied pubkey is still the same requester — one survivor, not two");
        assert_eq!(pending[0].id, id2);

        let log = std::fs::read_to_string(&audit_log).unwrap_or_default();
        assert!(log.contains(&format!("superseding {id1}")), "{log}");

        let _ = std::fs::remove_dir_all(&root);
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
    }

    #[test]
    fn pair_reveal_completes_the_commitment_and_the_entry_gains_a_sas() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-pairreveal-ok-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        act_as(&root, "b");
        let audit_log = root.join("log");

        let nonce_a = "c".repeat(32);
        let commit = aoide_storage::pairing::derive_commit(&"a".repeat(64), &nonce_a);
        let request_params = json!({ "pubkeyHex": "a".repeat(64), "name": "box-a", "commitHex": commit, "url": "http://a/" });
        let resp = pair_request(&request_params, ConnOrigin::Loopback, &audit_log).unwrap();
        let id = resp["id"].as_str().unwrap().to_string();

        let reveal_resp = pair_reveal(&json!({ "id": id, "nonceHex": nonce_a }), &audit_log).unwrap();
        assert_eq!(reveal_resp["ok"], true);

        let now_epoch = aoide_storage::time::parse_iso_utc(&now_iso_utc()).unwrap();
        let pending = aoide_storage::pairing::list_inbound(now_epoch);
        assert_eq!(pending.len(), 1, "revealing never removes the entry");
        assert_eq!(pending[0].requester_nonce_hex.as_deref(), Some(nonce_a.as_str()));

        let log = std::fs::read_to_string(&audit_log).unwrap_or_default();
        assert!(log.contains("a2a.pairReveal"), "{log}");

        let _ = std::fs::remove_dir_all(&root);
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
    }

    #[test]
    fn pair_reveal_rejects_a_wrong_nonce_and_drops_the_parked_entry() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-pairreveal-mismatch-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        act_as(&root, "b");
        let audit_log = root.join("log");

        let commit = aoide_storage::pairing::derive_commit(&"a".repeat(64), &"c".repeat(32));
        let request_params = json!({ "pubkeyHex": "a".repeat(64), "name": "box-a", "commitHex": commit, "url": "http://a/" });
        let resp = pair_request(&request_params, ConnOrigin::Loopback, &audit_log).unwrap();
        let id = resp["id"].as_str().unwrap().to_string();

        let err = pair_reveal(&json!({ "id": id, "nonceHex": "d".repeat(32) }), &audit_log).unwrap_err();
        assert_eq!(err.0, -32002, "a distinct code from an unknown id or ordinary invalid params");

        let now_epoch = aoide_storage::time::parse_iso_utc(&now_iso_utc()).unwrap();
        assert!(aoide_storage::pairing::list_inbound(now_epoch).is_empty(), "the mismatched reveal dropped the parked entry outright");

        let _ = std::fs::remove_dir_all(&root);
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
    }

    #[test]
    fn pair_reveal_rejects_an_unknown_id() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-pairreveal-unknown-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        act_as(&root, "b");
        let audit_log = root.join("log");

        let err = pair_reveal(&json!({ "id": "nosuchid", "nonceHex": "c".repeat(32) }), &audit_log).unwrap_err();
        assert_eq!(err.0, -32001);

        let _ = std::fs::remove_dir_all(&root);
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
    }

    /// Sign a `PAIRPOLL` poll of `id` with `kp` — the exact canonical string
    /// [`pair_poll`] itself verifies against, built once here so every
    /// `pair_poll_*` test below signs identically to how a real requester
    /// would (`aoide-client`'s own poll body-builder, unit-tested separately
    /// against this same shape).
    fn sign_poll(kp: &aoide_storage::identity::Keypair, id: &str, timestamp: &str, nonce: &str) -> String {
        let canonical = aoide_storage::wire_auth::canonical_string("PAIRPOLL", id, timestamp, nonce, &[], None);
        aoide_storage::wire_auth::sign_hex(kp, canonical.as_bytes())
    }

    /// Existence-oracle discipline (module doc on [`pair_poll`]): an unknown
    /// id, a known id polled with a signature from the WRONG key (a
    /// non-original-requester — exactly the "arbitrary poller cannot
    /// harvest/complete someone else's pairing" security invariant), and a
    /// known id polled correctly but not yet approved all answer with the
    /// IDENTICAL `{"status":"pending"}` — nothing distinguishes them to an
    /// unauthenticated or wrongly-authenticated caller.
    #[test]
    fn pair_poll_returns_pending_uniformly_for_unknown_id_wrong_signer_and_not_yet_approved() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-pairpoll-uniform-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        act_as(&root, "b");
        let (kp_requester, _) = aoide_storage::identity::load_or_mint().unwrap();
        let requester_pubkey = kp_requester.info().pubkey_hex;
        let (kp_impostor, _) = {
            // A second, DIFFERENT identity under its own state dir, minted
            // then torn straight back down to "b"'s — this test only needs
            // its keypair, never its files.
            act_as(&root, "impostor");
            let kp = aoide_storage::identity::load_or_mint().unwrap();
            act_as(&root, "b");
            kp
        };

        let now = now_iso_utc();
        let now_epoch = aoide_storage::time::parse_iso_utc(&now).unwrap();
        let commit = aoide_storage::pairing::derive_commit(&requester_pubkey, &"c".repeat(32));
        let (entry, _evicted) = aoide_storage::pairing::park_inbound(
            &requester_pubkey, "box-a", "10.0.0.5", "http://box-a:8710/", &commit, &now,
            &aoide_storage::pairing::expires_at_from(now_epoch),
            None,
        )
        .unwrap();
        aoide_storage::pairing::reveal_inbound(&entry.id, &"c".repeat(32), now_epoch).unwrap();

        let audit_log = root.join("log");
        let nonce = "9".repeat(32);

        // Unknown id entirely.
        let sig_unknown = sign_poll(&kp_requester, "nosuchid", &now, &nonce);
        let resp = pair_poll(&json!({ "id": "nosuchid", "timestampIso": now, "nonceHex": nonce, "signatureHex": sig_unknown }), &audit_log).unwrap();
        assert_eq!(resp["status"], "pending");

        // Real id, but signed by a DIFFERENT key than the original requester's.
        let sig_impostor = sign_poll(&kp_impostor, &entry.id, &now, &nonce);
        let resp = pair_poll(&json!({ "id": entry.id, "timestampIso": now, "nonceHex": nonce, "signatureHex": sig_impostor }), &audit_log).unwrap();
        assert_eq!(resp["status"], "pending", "a non-original-requester's signature must never release anything");

        // Real id, correctly signed by the ORIGINAL requester — still
        // pending, because nobody has approved it yet.
        let sig_real = sign_poll(&kp_requester, &entry.id, &now, &nonce);
        let resp = pair_poll(&json!({ "id": entry.id, "timestampIso": now, "nonceHex": nonce, "signatureHex": sig_real }), &audit_log).unwrap();
        assert_eq!(resp["status"], "pending", "not yet approved");

        // All three responses are byte-identical shapes — an outsider
        // learns nothing about which case they hit.
        assert_eq!(resp.to_string(), json!({ "status": "pending" }).to_string());

        let _ = std::fs::remove_dir_all(&root);
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
    }

    /// Once the approver's own operator has locally approved (module doc on
    /// [`pair_poll`]: no network callback — `mark_inbound_approved` is the
    /// ONLY thing that flips this), a poll correctly signed by the ORIGINAL
    /// requester gets the release: `{"status":"approved","pubkeyHex":<B's
    /// own pubkey>}` — B's identity re-derived fresh, never stored on the
    /// parked entry. One audit line records the release.
    #[test]
    fn pair_poll_releases_the_approvers_pubkey_only_once_approved_and_correctly_signed() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-pairpoll-released-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        act_as(&root, "b");
        let (kp_b, _) = aoide_storage::identity::load_or_mint().unwrap();
        let (kp_requester, _) = {
            act_as(&root, "a");
            let kp = aoide_storage::identity::load_or_mint().unwrap();
            act_as(&root, "b");
            kp
        };
        let requester_pubkey = kp_requester.info().pubkey_hex;

        let now = now_iso_utc();
        let now_epoch = aoide_storage::time::parse_iso_utc(&now).unwrap();
        let commit = aoide_storage::pairing::derive_commit(&requester_pubkey, &"c".repeat(32));
        let (entry, _evicted) = aoide_storage::pairing::park_inbound(
            &requester_pubkey, "box-a", "10.0.0.5", "http://box-a:8710/", &commit, &now,
            &aoide_storage::pairing::expires_at_from(now_epoch),
            None,
        )
        .unwrap();
        aoide_storage::pairing::reveal_inbound(&entry.id, &"c".repeat(32), now_epoch).unwrap();

        let audit_log = root.join("log");
        let nonce = "9".repeat(32);
        let sig = sign_poll(&kp_requester, &entry.id, &now, &nonce);
        let params = json!({ "id": entry.id, "timestampIso": now, "nonceHex": nonce, "signatureHex": sig });

        // Not yet approved — pending.
        assert_eq!(pair_poll(&params, &audit_log).unwrap()["status"], "pending");

        // The approver's own operator approves — PURELY LOCAL, no network
        // call anywhere in this line (module doc: this is the whole point).
        aoide_storage::pairing::mark_inbound_approved(&entry.id, now_epoch).unwrap();

        let resp = pair_poll(&params, &audit_log).unwrap();
        assert_eq!(resp["status"], "approved");
        assert_eq!(resp["pubkeyHex"], kp_b.info().pubkey_hex, "the release carries B's OWN identity, re-derived fresh");

        let log = std::fs::read_to_string(&audit_log).unwrap();
        assert!(log.contains("a2a.pairPoll"), "{log}");

        // Approving never removed the parked entry — a repeated poll (e.g.
        // the requester's connection dropped after the first release) still
        // finds it.
        assert_eq!(aoide_storage::pairing::list_inbound(now_epoch).len(), 1);

        let _ = std::fs::remove_dir_all(&root);
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
    }

    #[test]
    fn pair_poll_rejects_malformed_params_before_any_lookup() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-pairpoll-malformed-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        act_as(&root, "b");
        let audit_log = root.join("log");

        assert_eq!(pair_poll(&json!({}), &audit_log).unwrap_err().0, -32602, "missing id");
        assert_eq!(
            pair_poll(&json!({ "id": "x", "timestampIso": "2026-08-28T00:00:00Z", "nonceHex": "short", "signatureHex": "s" }), &audit_log)
                .unwrap_err()
                .0,
            -32602,
            "malformed nonceHex"
        );

        let _ = std::fs::remove_dir_all(&root);
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
    }

    // ── P-P5: the pairing events feed (`emit_pairing_event`) ────────────

    /// Save/restore `AOIDE_DAEMON_EVENTS` alongside the existing
    /// `AOIDE_STATE_DIR`/`AOIDE_STAGE_DIR` pair, mirroring `act_as`'s own
    /// save/restore shape one level up — this env var is what redirects
    /// [`emit_pairing_event`]'s `crate::daemon::events_path` resolution
    /// onto a tempfile instead of the real runtime dir, under the same
    /// `env_lock` every test in this module already holds.
    fn set_events_path(p: &std::path::Path) -> Option<String> {
        let saved = std::env::var("AOIDE_DAEMON_EVENTS").ok();
        std::env::set_var("AOIDE_DAEMON_EVENTS", p);
        saved
    }
    fn restore_events_path(saved: Option<String>) {
        match saved {
            Some(v) => std::env::set_var("AOIDE_DAEMON_EVENTS", v),
            None => std::env::remove_var("AOIDE_DAEMON_EVENTS"),
        }
    }

    #[test]
    fn pair_request_emits_one_gate_classed_parked_line() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-pairevent-parked-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        act_as(&root, "b");
        let events_path = root.join("events.jsonl");
        let saved_events = set_events_path(&events_path);
        let audit_log = root.join("log");

        let commit = aoide_storage::pairing::derive_commit(&"a".repeat(64), &"c".repeat(32));
        let params = json!({ "pubkeyHex": "a".repeat(64), "name": "box-a", "commitHex": commit, "url": "http://box-a:8710/" });
        pair_request(&params, ConnOrigin::Remote("10.0.0.5".parse().unwrap()), &audit_log).unwrap();

        let feed = std::fs::read_to_string(&events_path).unwrap();
        let lines: Vec<&str> = feed.lines().collect();
        assert_eq!(lines.len(), 1, "exactly one line: {feed}");
        let rec: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(rec["class"], "gate");
        assert_eq!(rec["kind"], "pair-parked");
        assert_eq!(rec["source"], "a2a-door");
        assert_eq!(rec["payload"]["name"], "box-a");
        assert_eq!(rec["payload"]["originAddr"], "10.0.0.5");
        assert_eq!(rec["payload"]["url"], "http://box-a:8710/");
        assert_eq!(rec["payload"]["direction"], "inbound");

        restore_events_path(saved_events);
        let _ = std::fs::remove_dir_all(&root);
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
    }

    #[test]
    fn pairing_feed_lines_never_carry_a_sas_pubkey_nonce_or_commitment() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-pairevent-nosecrets-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        act_as(&root, "b");
        let events_path = root.join("events.jsonl");
        let saved_events = set_events_path(&events_path);
        let audit_log = root.join("log");

        let requester_pubkey = "a".repeat(64);
        let requester_nonce = "c".repeat(32);
        let commit = aoide_storage::pairing::derive_commit(&requester_pubkey, &requester_nonce);
        let params = json!({ "pubkeyHex": requester_pubkey, "name": "box-a", "commitHex": commit, "url": "http://box-a:8710/" });
        let resp = pair_request(&params, ConnOrigin::Loopback, &audit_log).unwrap();
        let id = resp["id"].as_str().unwrap().to_string();
        let approver_nonce = resp["nonceHex"].as_str().unwrap().to_string();
        pair_reveal(&json!({ "id": id, "nonceHex": requester_nonce }), &audit_log).unwrap();

        let feed = std::fs::read_to_string(&events_path).unwrap();
        assert!(!feed.is_empty());
        // No field named sas/pubkey/pubkeyHex/nonce/nonceHex/commit/commitHex
        // anywhere on the feed, AND the actual hex values never ride it —
        // both checks, per the plan (a field-name check alone would miss a
        // renamed-but-still-secret field slipping through).
        for banned_field in ["sas", "pubkey", "pubkeyHex", "nonce", "nonceHex", "commit", "commitHex"] {
            assert!(!feed.contains(banned_field), "feed line named a forbidden field `{banned_field}`: {feed}");
        }
        for secret_value in [&requester_pubkey, &requester_nonce, &approver_nonce, &commit] {
            assert!(!feed.contains(secret_value.as_str()), "feed line carried a secret hex value: {feed}");
        }

        restore_events_path(saved_events);
        let _ = std::fs::remove_dir_all(&root);
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
    }

    #[test]
    fn pair_reveal_emits_revealed_on_ok_and_nothing_on_a_mismatch() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-pairevent-reveal-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        act_as(&root, "b");
        let events_path = root.join("events.jsonl");
        let saved_events = set_events_path(&events_path);
        let audit_log = root.join("log");

        // `pair_request` itself already emits `pair-parked`, so "nothing on
        // a mismatch" is checked as "no NEW line", not "the feed stays
        // empty" — the feed already carries that one line by this point.
        let commit = aoide_storage::pairing::derive_commit(&"a".repeat(64), &"c".repeat(32));
        let params = json!({ "pubkeyHex": "a".repeat(64), "name": "box-a", "commitHex": commit, "url": "http://a/" });
        let id = pair_request(&params, ConnOrigin::Loopback, &audit_log).unwrap()["id"].as_str().unwrap().to_string();
        let lines_before_mismatch = std::fs::read_to_string(&events_path).unwrap().lines().count();
        assert_eq!(lines_before_mismatch, 1, "pair_request's own pair-parked line");
        let _ = pair_reveal(&json!({ "id": id, "nonceHex": "d".repeat(32) }), &audit_log).unwrap_err();
        let lines_after_mismatch = std::fs::read_to_string(&events_path).unwrap().lines().count();
        assert_eq!(lines_after_mismatch, lines_before_mismatch, "a reveal mismatch must emit nothing");

        // Now a genuine ok reveal: exactly one `pair-revealed` line.
        let commit2 = aoide_storage::pairing::derive_commit(&"b".repeat(64), &"e".repeat(32));
        let params2 = json!({ "pubkeyHex": "b".repeat(64), "name": "box-c", "commitHex": commit2, "url": "http://c/" });
        let id2 = pair_request(&params2, ConnOrigin::Loopback, &audit_log).unwrap()["id"].as_str().unwrap().to_string();
        pair_reveal(&json!({ "id": id2, "nonceHex": "e".repeat(32) }), &audit_log).unwrap();

        let feed = std::fs::read_to_string(&events_path).unwrap();
        let kinds: Vec<String> = feed.lines().map(|l| serde_json::from_str::<Value>(l).unwrap()["kind"].as_str().unwrap().to_string()).collect();
        assert_eq!(
            kinds,
            vec!["pair-parked".to_string(), "pair-parked".to_string(), "pair-revealed".to_string()],
            "the second `pair_request` parks its own line before its `pair_reveal` adds the third"
        );

        restore_events_path(saved_events);
        let _ = std::fs::remove_dir_all(&root);
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
    }

    /// P-P5: an unwritable events path must never fail the ceremony itself
    /// — same precedent as `aoide_secrets::broker`'s
    /// `resolve_still_succeeds_when_the_events_feed_path_is_unwritable`.
    /// Root ignores directory permissions too, so this skips under a root
    /// test runner, same precedent.
    /// GATED on Unix with its reason: the fixture makes a directory UNWRITABLE
    /// by mode (`0o500`), root ignores modes so the test skips under a root
    /// runner, and `effective_uid` is this host's uid lookup — all three are
    /// POSIX facts. The ceremony's "an unwritable events path never fails the
    /// pair request" contract is therefore not asserted on native Windows,
    /// where a directory's policy is a DACL this fixture cannot spell in a
    /// mode. Said plainly: no native twin exists for this one.
    #[cfg(unix)]
    #[test]
    fn pair_request_still_succeeds_when_the_events_path_is_unwritable() {
        if aoide_secrets::home::effective_uid() == 0 {
            return;
        }
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-pairevent-unwritable-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        act_as(&root, "b");

        let ro_dir = root.join("events-ro-dir");
        std::fs::create_dir_all(&ro_dir).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&ro_dir, std::fs::Permissions::from_mode(0o500)).unwrap();
        let events_path = ro_dir.join("events.jsonl");
        let saved_events = set_events_path(&events_path);
        let audit_log = root.join("log");

        let commit = aoide_storage::pairing::derive_commit(&"a".repeat(64), &"c".repeat(32));
        let params = json!({ "pubkeyHex": "a".repeat(64), "name": "box-a", "commitHex": commit, "url": "http://a/" });
        let resp = pair_request(&params, ConnOrigin::Loopback, &audit_log);

        std::fs::set_permissions(&ro_dir, std::fs::Permissions::from_mode(0o700)).unwrap();

        assert!(resp.is_ok(), "the ceremony must succeed even when the events feed is unwritable: {resp:?}");
        assert!(!events_path.exists(), "the feed file must never have been created under a read-only parent");

        restore_events_path(saved_events);
        let _ = std::fs::remove_dir_all(&root);
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
    }

    /// The full ceremony, end to end, over BOTH its typed codes (the
    /// mutual-code redesign, R1): request -> reveal -> pending -> B's own
    /// gate code shown (both sides derive the SAME `derive_sas` value
    /// independently) -> B approves PURELY LOCALLY (commits B's own
    /// record, marks its parked entry approved — NO network call to A —
    /// and derives its OWN reply code, `derive_reply_sas`, the code A's
    /// operator will need) -> A polls B (`aoide/pairPoll`, signed with A's
    /// OWN identity, over the SAME forward dial the request/reveal already
    /// used) -> A's own operator confirms against B's reply code (commits
    /// A's own record only once its own independently-derived
    /// `derive_reply_sas` matches B's) — PAIRING.md's own "The ceremony"
    /// diagram under Design A (task #119) and decision 4's mutual
    /// confirmation, review-bounce Findings 1 and 2 both still exercised
    /// end to end, driven through the real handler functions
    /// (`pair_request`/`pair_reveal`/`pair_poll`) and the real
    /// `aoide_storage::node_store`/`pairing` state, with
    /// `AOIDE_STATE_DIR`/`AOIDE_STAGE_DIR` swapped between steps to play box
    /// A then box B then box A again (see [`act_as`]'s own doc for why this
    /// test cannot be a genuine two-thread two-identity proof the way
    /// `cli/tests/node_connectivity.rs` is for the read-only `graphSummary`
    /// pull). B's own approve gate (`aoide-client::commands::
    /// approve_inbound`) and A's OWN final confirm-then-commit step
    /// (`pair <id>` on a polled-approved outbound entry,
    /// `aoide-client::commands::commit_outbound`) both live in
    /// `aoide-client` — simulated here by calling the same library
    /// functions those handlers call
    /// (`mark_outbound_awaiting_confirm`/`upsert_paired_node`/
    /// `take_outbound`) plus the SAME two derivations they gate on
    /// (`derive_sas`/`derive_reply_sas`), since this crate cannot depend on
    /// `aoide-client` (wrong DAG direction) — asserting both sides
    /// independently reach the IDENTICAL value for each of the two codes
    /// is what proves this end-to-end, not merely that a function of that
    /// name was called. A's own advertised `url` is deliberately a bogus,
    /// undialable address (`http://box-a-is-loopback-only.invalid/`) —
    /// under the OLD callback design B would have had to dial it to
    /// deliver the approval and the ceremony could never have completed;
    /// under Design A nothing ever dials it, so the ceremony completing
    /// anyway is itself the proof that no approver->requester network
    /// callback exists.
    #[test]
    fn full_pairing_ceremony_request_reveal_pending_approve_poll_confirm_writes_records_on_both_ends() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-ceremony-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let audit_log = root.join("log");

        // ── Step 1: box A mints its identity, picks a nonce, and commits to
        // it (`aoide-client`'s own body-builder is unit-tested separately
        // against this exact shape; here we construct the wire params
        // directly, the same way `pair_request`/`pair_reveal` will receive
        // them). A's `url` is undialable ON PURPOSE — module doc above.
        act_as(&root, "a");
        let (kp_a, _) = aoide_storage::identity::load_or_mint().unwrap();
        let pubkey_a = kp_a.info().pubkey_hex;
        let nonce_a = aoide_storage::pairing::random_hex(16);
        let commit_a = aoide_storage::pairing::derive_commit(&pubkey_a, &nonce_a);
        let request_params = json!({
            "pubkeyHex": pubkey_a, "name": "box-a", "commitHex": commit_a, "url": "http://box-a-is-loopback-only.invalid/",
        });

        // ── Step 2: box B receives it — parks pending (no SAS yet, unrevealed),
        // answers with its own pubkey + nonce.
        act_as(&root, "b");
        let resp = pair_request(&request_params, ConnOrigin::Remote("10.0.0.9".parse().unwrap()), &audit_log).unwrap();
        let id = resp["id"].as_str().unwrap().to_string();
        let pubkey_b = resp["pubkeyHex"].as_str().unwrap().to_string();
        let nonce_b = resp["nonceHex"].as_str().unwrap().to_string();

        // ── Step 3: back on box A — reveal the nonce the commitment already
        // fixed (review-bounce Finding 1's own second POST), then derive its
        // OWN SAS (now that it has both nonces) and remember the outbound
        // request in `awaiting-approval`.
        let reveal_params = json!({ "id": id, "nonceHex": nonce_a });
        act_as(&root, "b");
        let reveal_resp = pair_reveal(&reveal_params, &audit_log).unwrap();
        assert_eq!(reveal_resp["ok"], true);
        act_as(&root, "a");
        let sas_a = aoide_storage::pairing::derive_sas(&pubkey_a, &pubkey_b, &nonce_a, &nonce_b);
        let now_epoch = aoide_storage::time::parse_iso_utc(&now_iso_utc()).unwrap();
        aoide_storage::pairing::park_outbound(aoide_storage::pairing::OutboundPairingRequest {
        binding: None,
            id: id.clone(),
            url: "http://box-b:9-a2a/".to_string(),
            name: "box-b".to_string(),
            pubkey_hex: pubkey_b.clone(),
            requester_nonce_hex: nonce_a.clone(),
            approver_nonce_hex: nonce_b.clone(),
            requested_at: now_iso_utc(),
            expires_at: aoide_storage::pairing::expires_at_from(now_epoch),
            state: aoide_storage::pairing::OutboundState::AwaitingApproval,
            via: None,
            mesh: None,
            tries: 0,
        })
        .unwrap();

        // ── Step 4: box B's operator lists pending, derives the SAME gate
        // code independently from its own stored (now-revealed) copy of
        // the transcript, and approves — committing B's OWN node record
        // for A. B also derives its OWN reply code here (`derive_reply_sas`
        // — the mutual-code redesign, R1): the SAME transcript plus a
        // leading domain tag, never the code just used above, since B's
        // own approve is what `commands::approve_inbound` computes and
        // relays to A's operator the moment it commits.
        act_as(&root, "b");
        let pending = aoide_storage::pairing::list_inbound(now_epoch);
        assert_eq!(pending.len(), 1);
        let entry = pending.into_iter().find(|e| e.id == id).unwrap();
        let requester_nonce = entry.requester_nonce_hex.clone().expect("revealed by step 3");
        let (kp_b, _) = aoide_storage::identity::load_or_mint().unwrap();
        let sas_b = aoide_storage::pairing::derive_sas(
            &entry.pubkey_hex,
            &kp_b.info().pubkey_hex,
            &requester_nonce,
            &entry.approver_nonce_hex,
        );
        assert_eq!(sas_a, sas_b, "both sides must derive the IDENTICAL gate code from the same transcript");
        let reply_sas_b = aoide_storage::pairing::derive_reply_sas(
            &entry.pubkey_hex,
            &kp_b.info().pubkey_hex,
            &requester_nonce,
            &entry.approver_nonce_hex,
        );
        assert_ne!(sas_b, reply_sas_b, "the gate code and the reply code must never coincide");

        // A poll BEFORE approval must answer `pending` — never leak that the
        // id exists as anything more (module doc on `pair_poll`).
        let poll_nonce = "9".repeat(32);
        let poll_ts = now_iso_utc();
        let poll_sig_pre = sign_poll(&kp_a, &id, &poll_ts, &poll_nonce);
        let poll_params = json!({ "id": id, "timestampIso": poll_ts, "nonceHex": poll_nonce, "signatureHex": poll_sig_pre });
        let poll_before = pair_poll(&poll_params, &audit_log).unwrap();
        assert_eq!(poll_before["status"], "pending", "not approved yet");

        // Design A (task #119): B's own node record commits, exactly as
        // before — but approving is now PURELY LOCAL. `mark_inbound_approved`
        // (not `take_inbound`) leaves the entry PARKED so A's later poll can
        // still find it; nothing here dials A's `url` at all.
        aoide_storage::pairing::mark_inbound_approved(&id, now_epoch).unwrap();
        let mut nodes_b = aoide_storage::node_store::load_nodes();
        aoide_storage::node_store::upsert_paired_node(&mut nodes_b, &entry.name, &entry.url, &entry.pubkey_hex, &now_iso_utc(), &["read".to_string()], "home");
        aoide_storage::node_store::save_nodes(&nodes_b).unwrap();

        // B's own record for A: pubkey = A's real key, verified, name =
        // A's claimed name, url = what A self-reported (the UNDIALABLE
        // address — B's own commit above never touched it as a network
        // target, only as a stored string).
        let nodes_b_final = aoide_storage::node_store::load_nodes();
        assert_eq!(nodes_b_final.len(), 1);
        assert_eq!(nodes_b_final[0].name, "box-a");
        assert_eq!(nodes_b_final[0].pubkey.as_deref(), Some(pubkey_a.as_str()));
        assert!(nodes_b_final[0].verified);
        assert_eq!(nodes_b_final[0].url, "http://box-a-is-loopback-only.invalid/");
        // P-P3's lane, untouched this phase.
        assert!(!nodes_b_final[0].hub);

        // ── Step 5: A polls B's door — `aoide/pairPoll`, signed with A's OWN
        // identity, over the SAME forward dial the request/reveal already
        // used. This REPLACES the old reverse callback outright: nothing
        // dials A's (undialable) `url` anywhere in this test, and the
        // ceremony completes anyway — that IS the "no callback" proof.
        let poll_sig = sign_poll(&kp_a, &id, &poll_ts, &poll_nonce);
        let poll_params_post_approve = json!({ "id": id, "timestampIso": poll_ts, "nonceHex": poll_nonce, "signatureHex": poll_sig });
        let poll_resp = pair_poll(&poll_params_post_approve, &audit_log).unwrap();
        assert_eq!(poll_resp["status"], "approved");
        assert_eq!(poll_resp["pubkeyHex"], kp_b.info().pubkey_hex);
        assert!(aoide_storage::node_store::load_nodes().len() == 1, "still only B's own record — the poll commits nothing on B's side");

        // ── Step 6: back on A — the poll response transitions A's outbound
        // entry (`mark_outbound_awaiting_confirm`, the SAME function the old
        // callback handler used to call — only the TRIGGER moved), rejecting
        // a mismatched pubkey the same way a substituted reveal would be
        // rejected (review-bounce Finding 2, preserved). A's OWN operator
        // then confirms — gating on B's REPLY code, never the gate code A's
        // own screen already showed (`pair <id>` a second time,
        // requester-side — `aoide-client::commands::commit_outbound`'s own
        // confirm branch; simulated here via the same library calls that
        // handler makes, since this crate cannot depend on `aoide-client`).
        act_as(&root, "a");
        let polled_pubkey = poll_resp["pubkeyHex"].as_str().unwrap();
        let marked = aoide_storage::pairing::mark_outbound_awaiting_confirm(&id, polled_pubkey, now_epoch).unwrap();
        assert_eq!(marked.state, aoide_storage::pairing::OutboundState::AwaitingConfirm);
        let reply_sas_a = aoide_storage::pairing::derive_reply_sas(&pubkey_a, &marked.pubkey_hex, &marked.requester_nonce_hex, &marked.approver_nonce_hex);
        assert_eq!(reply_sas_a, reply_sas_b, "A independently re-derives the IDENTICAL reply code B already computed at approve time");
        assert_ne!(reply_sas_a, sas_a, "A's own confirm gates on the reply code, never the gate code its own screen already showed");
        let mut nodes_a = aoide_storage::node_store::load_nodes();
        aoide_storage::node_store::upsert_paired_node(&mut nodes_a, &marked.name, &marked.url, &marked.pubkey_hex, &now_iso_utc(), &["read".to_string()], "home");
        aoide_storage::node_store::save_nodes(&nodes_a).unwrap();
        aoide_storage::pairing::take_outbound(&id, now_epoch).unwrap();

        // A's own record for B: pubkey = B's real key, verified, name = the
        // nickname A itself chose at request time, url = what A dialed.
        let nodes_a_final = aoide_storage::node_store::load_nodes();
        assert_eq!(nodes_a_final.len(), 1);
        assert_eq!(nodes_a_final[0].name, "box-b");
        assert_eq!(nodes_a_final[0].pubkey.as_deref(), Some(pubkey_b.as_str()));
        assert!(nodes_a_final[0].verified);
        assert_eq!(nodes_a_final[0].url, "http://box-b:9-a2a/");

        // The outbound entry is consumed — a second confirm with the same
        // id now finds nothing.
        assert!(aoide_storage::pairing::list_outbound(now_epoch).is_empty());

        // B's own inbound entry, meanwhile, stays parked (approved, never
        // taken) until it expires — the poll never removes it either, so a
        // repeated/duplicate poll from A would still find the SAME release.
        act_as(&root, "b");
        assert_eq!(aoide_storage::pairing::list_inbound(now_epoch).len(), 1);

        // The private key never rode any wire body this test constructed —
        // grep every JSON value exchanged for anything key-shaped beyond the
        // public hex fields already asserted above.
        for v in [&request_params, &resp, &reveal_params, &reveal_resp, &poll_params, &poll_before, &poll_params_post_approve, &poll_resp] {
            let dumped = v.to_string().to_lowercase();
            assert!(!dumped.contains("signing"), "no private material anywhere on the wire: {dumped}");
        }

        let _ = std::fs::remove_dir_all(&root);
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
    }

    // (d) the HTTP request parse (request line + Content-Length body).
    #[test]
    fn parse_http_request_reads_request_line_and_body() {
        let raw = b"POST / HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: 12\r\n\r\n{\"a\":\"body\"}";
        let mut r = BufReader::new(std::io::Cursor::new(&raw[..]));
        let req = parse_http_request(&mut r, Instant::now()).unwrap();
        assert_eq!(req.method, "POST");
        assert_eq!(req.path, "/");
        assert_eq!(req.body, b"{\"a\":\"body\"}");
    }

    #[test]
    fn parse_http_request_handles_a_get_with_no_body() {
        let raw = b"GET /.well-known/agent-card.json HTTP/1.1\r\nHost: x\r\n\r\n";
        let mut r = BufReader::new(std::io::Cursor::new(&raw[..]));
        let req = parse_http_request(&mut r, Instant::now()).unwrap();
        assert_eq!(req.method, "GET");
        assert_eq!(req.path, "/.well-known/agent-card.json");
        assert!(req.body.is_empty());
    }

    // ── Hostile-input regressions (security review, pre-commit) ─────────────
    //
    // All of these feed crafted bytes straight to `parse_http_request` via an
    // in-memory `Cursor` — no real socket, no sleeping, so they can't hang or
    // flake. Each one exercises a cap that, before this pass, was either
    // absent (unbounded alloc/read) or untested.

    #[test]
    fn content_length_over_max_body_is_rejected_before_any_large_allocation() {
        // A `Content-Length` far past MAX_BODY must error out of the header
        // loop WITHOUT ever reaching the body-allocation step below it — if
        // this test hangs or OOMs instead of returning quickly, the cap
        // isn't being enforced before the allocation.
        let raw = format!(
            "POST / HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            MAX_BODY + 1
        );
        let mut r = BufReader::new(std::io::Cursor::new(raw.as_bytes()));
        let err = parse_http_request(&mut r, Instant::now()).unwrap_err();
        assert_eq!(err.status, 413);
        assert!(err.message.contains("too large"), "message: {}", err.message);
    }

    #[test]
    fn a_content_length_of_exactly_max_body_is_not_rejected_by_the_cap() {
        // Boundary check: MAX_BODY itself is still allowed by the cap (it's
        // an inclusive limit) — it should fail later, on the short read
        // (Cursor has no body bytes), not on the size check.
        let raw = format!("POST / HTTP/1.1\r\nContent-Length: {MAX_BODY}\r\n\r\n");
        let mut r = BufReader::new(std::io::Cursor::new(raw.as_bytes()));
        let err = parse_http_request(&mut r, Instant::now()).unwrap_err();
        assert!(!err.message.contains("too large"), "message: {}", err.message);
    }

    #[test]
    fn a_request_line_with_no_newline_past_max_line_is_rejected() {
        // No `\n` anywhere — a hostile stream that would otherwise grow the
        // line buffer without bound. Longer than MAX_LINE so the cap (not
        // Cursor EOF) is what triggers the error.
        let raw = vec![b'A'; MAX_LINE + 1];
        let mut r = BufReader::new(std::io::Cursor::new(raw));
        let err = parse_http_request(&mut r, Instant::now()).unwrap_err();
        assert_eq!(err.status, 400);
        assert!(err.message.contains("too long"), "message: {}", err.message);
    }

    #[test]
    fn a_header_line_with_no_newline_past_max_line_is_rejected() {
        let mut raw = b"GET / HTTP/1.1\r\n".to_vec();
        raw.extend(std::iter::repeat(b'A').take(MAX_LINE + 1));
        let mut r = BufReader::new(std::io::Cursor::new(raw));
        let err = parse_http_request(&mut r, Instant::now()).unwrap_err();
        assert_eq!(err.status, 400);
        assert!(err.message.contains("too long"), "message: {}", err.message);
    }

    #[test]
    fn more_than_max_headers_is_rejected() {
        let mut raw = b"GET / HTTP/1.1\r\n".to_vec();
        for i in 0..=MAX_HEADERS {
            raw.extend_from_slice(format!("X-Filler-{i}: x\r\n").as_bytes());
        }
        raw.extend_from_slice(b"\r\n");
        let mut r = BufReader::new(std::io::Cursor::new(raw));
        let err = parse_http_request(&mut r, Instant::now()).unwrap_err();
        assert_eq!(err.status, 400);
        assert!(err.message.contains("too many headers"), "message: {}", err.message);
    }

    #[test]
    fn a_valid_small_request_still_parses_under_the_new_caps() {
        // Regression: none of the new caps should reject an ordinary,
        // well-formed request.
        let raw = b"POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2\r\n\r\n{}";
        let mut r = BufReader::new(std::io::Cursor::new(&raw[..]));
        let req = parse_http_request(&mut r, Instant::now()).unwrap();
        assert_eq!(req.method, "POST");
        assert_eq!(req.path, "/");
        assert_eq!(req.body, b"{}");
    }

    // ── SSE streaming helpers (Phase C — pure, no socket, no sleep) ─────────

    #[test]
    fn sse_event_frames_json_as_a_data_line() {
        let v = json!({ "a": 1 });
        assert_eq!(sse_event(&v), "data: {\"a\":1}\n\n");
    }

    #[test]
    fn is_terminal_state_covers_the_four_a2a_terminal_states_only() {
        for terminal in ["completed", "failed", "canceled", "rejected"] {
            assert!(is_terminal_state(terminal), "{terminal} should be terminal");
        }
        for live in ["working", "submitted", "input-required", "auth-required", ""] {
            assert!(!is_terminal_state(live), "{live} should NOT be terminal");
        }
    }

    #[test]
    fn should_emit_fires_on_first_observation_and_on_change_but_not_on_repeat() {
        // First observation (nothing emitted yet) always emits.
        assert!(should_emit(None, "working"));
        // A changed state emits.
        assert!(should_emit(Some("working"), "completed"));
        // The same state again does NOT emit (the loop's `|| is_final` still
        // forces the terminal/timeout event separately).
        assert!(!should_emit(Some("working"), "working"));
    }

    #[test]
    fn build_stream_event_non_final_carries_the_task_and_final_is_a_status_update() {
        let task = json!({
            "id": "sess-1",
            "contextId": "sess-1",
            "status": { "state": "working", "timestamp": "2026-01-01T00:00:00Z" },
            "kind": "task",
        });
        // Non-final: the result IS the task, no `final` marker.
        let ev = build_stream_event(&json!(7), &task, false);
        assert_eq!(ev["jsonrpc"], "2.0");
        assert_eq!(ev["id"], 7);
        assert_eq!(ev["result"]["kind"], "task");
        assert_eq!(ev["result"]["status"]["state"], "working");
        assert!(ev["result"].get("final").is_none());

        // Final: a TaskStatusUpdateEvent with `final: true`.
        let done = json!({
            "id": "sess-1",
            "contextId": "sess-1",
            "status": { "state": "completed", "timestamp": "2026-01-01T00:00:01Z" },
            "kind": "task",
        });
        let fev = build_stream_event(&json!(7), &done, true);
        assert_eq!(fev["result"]["kind"], "status-update");
        assert_eq!(fev["result"]["final"], true);
        assert_eq!(fev["result"]["taskId"], "sess-1");
        assert_eq!(fev["result"]["contextId"], "sess-1");
        assert_eq!(fev["result"]["status"]["state"], "completed");
    }

    #[test]
    fn streaming_method_only_matches_the_two_sse_methods_on_post_root() {
        let mk = |method: &str, path: &str, body: &str| HttpRequest {
            method: method.into(),
            path: path.into(),
            body: body.as_bytes().to_vec(),
            bearer: None,
            signed_node: None,
            signed_timestamp: None,
            signed_nonce: None,
            signed_signature: None,
            signed_mesh: None,
        };
        assert_eq!(
            streaming_method(&mk("POST", "/", r#"{"method":"message/stream"}"#)).as_deref(),
            Some("message/stream")
        );
        assert_eq!(
            streaming_method(&mk("POST", "/", r#"{"method":"tasks/resubscribe"}"#)).as_deref(),
            Some("tasks/resubscribe")
        );
        // A one-shot method is NOT a streaming method.
        assert_eq!(streaming_method(&mk("POST", "/", r#"{"method":"message/send"}"#)), None);
        assert_eq!(streaming_method(&mk("POST", "/", r#"{"method":"tasks/get"}"#)), None);
        // Wrong method / path / unparseable body → not a stream.
        assert_eq!(streaming_method(&mk("GET", "/", r#"{"method":"message/stream"}"#)), None);
        assert_eq!(streaming_method(&mk("POST", "/other", r#"{"method":"message/stream"}"#)), None);
        assert_eq!(streaming_method(&mk("POST", "/", "not json")), None);
    }

    // Routing: non-matching path/method -> 404/405 with a JSON-RPC-style body.
    #[test]
    fn unknown_path_is_404_and_wrong_method_on_a_known_path_is_405() {
        let registry = Registry::new();
        let (status, body, _) = route(
            &HttpRequest {
                method: "GET".into(),
                path: "/nope".into(),
                body: vec![],
                bearer: None,
                signed_node: None,
                signed_timestamp: None,
                signed_nonce: None,
                signed_signature: None,
                signed_mesh: None,
            },
            "127.0.0.1",
            8710,
            Path::new("/dev/null"),
            "",
            "",
            "aoide",
            ConnOrigin::Loopback,
            "",
            &registry,
            None,
        );
        assert_eq!(status, 404);
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert!(v["error"]["code"].is_i64());

        let (status, body, _) = route(
            &HttpRequest {
                method: "GET".into(),
                path: "/".into(),
                body: vec![],
                bearer: None,
                signed_node: None,
                signed_timestamp: None,
                signed_nonce: None,
                signed_signature: None,
                signed_mesh: None,
            },
            "127.0.0.1",
            8710,
            Path::new("/dev/null"),
            "",
            "",
            "aoide",
            ConnOrigin::Loopback,
            "",
            &registry,
            None,
        );
        assert_eq!(status, 405);
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert!(v["error"]["code"].is_i64());
    }

    // ── P4: unauthenticated AgentCard GET is stripped, not gated ────────────
    // (CONTRACTS.md §6, 2026-08-20 amendment)

    /// Off-path pin: with NO token configured, the served card is
    /// byte-identical to the pre-amendment behavior — the FULL card,
    /// field-for-field against `agent_card_from_commands` directly — with or
    /// without a bearer presented (there's nothing configured to compare it
    /// against).
    #[test]
    fn agent_card_get_with_no_token_configured_serves_the_full_card_unchanged() {
        let registry = Registry::new();
        let expected = agent_card_from_commands(registry.commands(), "127.0.0.1", 8710);

        for bearer in [None, Some("anything".to_string())] {
            let req = HttpRequest {
                method: "GET".into(),
                path: "/.well-known/agent-card.json".into(),
                body: vec![],
                bearer,
                signed_node: None,
                signed_timestamp: None,
                signed_nonce: None,
                signed_signature: None,
                signed_mesh: None,
            };
            let (status, body, label) = route(
                &req,
                "127.0.0.1",
                8710,
                Path::new("/dev/null"),
                "",
                "",
                "aoide",
                ConnOrigin::Loopback,
                "",
                &registry,
                None,
            );
            assert_eq!(status, 200);
            assert_eq!(label, "a2a.agent-card");
            let served: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(served, expected);
        }
    }

    /// Token configured, no bearer presented: the served card is stripped to
    /// EXACTLY three keys — assert the key COUNT, not just presence, so a
    /// future field added to the full card can't silently leak through the
    /// strip. Status stays 200 (a card GET never becomes `-32005`).
    #[test]
    fn agent_card_get_with_token_and_no_bearer_is_stripped_to_exactly_three_keys() {
        let registry = Registry::new();
        let req = HttpRequest {
            method: "GET".into(),
            path: "/.well-known/agent-card.json".into(),
            body: vec![],
            bearer: None,
            signed_node: None,
            signed_timestamp: None,
            signed_nonce: None,
            signed_signature: None,
            signed_mesh: None,
        };
        let (status, body, label) = route(
            &req,
            "127.0.0.1",
            8710,
            Path::new("/dev/null"),
            "",
            "",
            "aoide",
            ConnOrigin::Loopback,
            "s3cr3t",
            &registry,
            None,
        );
        assert_eq!(status, 200, "a card GET never becomes -32005, even unauthorized");
        assert_eq!(label, "a2a.agent-card");
        let served: Value = serde_json::from_slice(&body).unwrap();
        let obj = served.as_object().expect("stripped card is still a JSON object");
        assert_eq!(obj.len(), 3, "stripped card must carry exactly name/protocolVersion/url, got {obj:?}");
        assert_eq!(served["name"], "aoide");
        assert_eq!(served["protocolVersion"], "0.3.0");
        assert_eq!(served["url"], "http://127.0.0.1:8710/");
        assert!(!obj.contains_key("skills"), "skills inventory must not leak unauthenticated");
        assert!(!obj.contains_key("version"), "version must not leak unauthenticated");
        assert!(!obj.contains_key("capabilities"), "capabilities must not leak unauthenticated");
        assert!(!obj.contains_key("defaultInputModes"));
        assert!(!obj.contains_key("defaultOutputModes"));
    }

    /// Token configured, WRONG bearer presented: the same stripped card as
    /// no bearer at all — `token_authorized` treats `TokenState::Invalid`
    /// identically to `Absent`.
    #[test]
    fn agent_card_get_with_wrong_bearer_is_the_same_stripped_card() {
        let registry = Registry::new();
        let req = HttpRequest {
            method: "GET".into(),
            path: "/.well-known/agent-card.json".into(),
            body: vec![],
            bearer: Some("nope".to_string()),
            signed_node: None,
            signed_timestamp: None,
            signed_nonce: None,
            signed_signature: None,
            signed_mesh: None,
        };
        let (status, body, _) = route(
            &req,
            "127.0.0.1",
            8710,
            Path::new("/dev/null"),
            "",
            "",
            "aoide",
            ConnOrigin::Loopback,
            "s3cr3t",
            &registry,
            None,
        );
        assert_eq!(status, 200);
        let served: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(served.as_object().unwrap().len(), 3);
        assert_eq!(served["name"], "aoide");
    }

    /// Token configured, VALID bearer presented: the full card, unchanged.
    #[test]
    fn agent_card_get_with_valid_bearer_serves_the_full_card() {
        let registry = Registry::new();
        let expected = agent_card_from_commands(registry.commands(), "127.0.0.1", 8710);
        let req = HttpRequest {
            method: "GET".into(),
            path: "/.well-known/agent-card.json".into(),
            body: vec![],
            bearer: Some("s3cr3t".to_string()),
            signed_node: None,
            signed_timestamp: None,
            signed_nonce: None,
            signed_signature: None,
            signed_mesh: None,
        };
        let (status, body, _) = route(
            &req,
            "127.0.0.1",
            8710,
            Path::new("/dev/null"),
            "",
            "",
            "aoide",
            ConnOrigin::Loopback,
            "s3cr3t",
            &registry,
            None,
        );
        assert_eq!(status, 200);
        let served: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(served, expected);
    }

    // ── task #84: inbound bearer resolved via the secrets broker ────────────

    fn bearer_cfg(bearer_secret: &str, secrets_socket: &Path, file_token: &str) -> InboundBearerConfig {
        InboundBearerConfig {
            bearer_secret: bearer_secret.to_string(),
            secrets_socket: secrets_socket.to_path_buf(),
            file_token: file_token.to_string(),
        }
    }

    #[test]
    fn resolve_inbound_bearer_falls_through_to_the_file_token_when_bearer_secret_is_unset() {
        let cfg = bearer_cfg("", Path::new("/tmp/aoide-a2a-unused.sock"), "file-token-value");
        assert_eq!(resolve_inbound_bearer(&cfg), "file-token-value");
    }

    #[test]
    fn resolve_inbound_bearer_with_neither_configured_is_the_empty_off_path() {
        let cfg = bearer_cfg("", Path::new("/tmp/aoide-a2a-unused.sock"), "");
        assert_eq!(resolve_inbound_bearer(&cfg), "");
        // Off-path is byte-identical to before this task: `token_authorized`
        // never gates anything when `expected_token` is empty.
        assert!(token_authorized(false, classify_token("", None)));
    }

    /// A dead broker socket (unreachable — the same shape as a stopped
    /// broker) FAILS CLOSED: [`resolve_inbound_bearer`] returns a non-empty
    /// sentinel, never the file token (a resolve failure must not silently
    /// fall back to the weaker mechanism) and never empty (which would read
    /// as "not configured" and open the door wide).
    #[test]
    fn resolve_inbound_bearer_fails_closed_on_an_unreachable_broker() {
        let dead = Path::new("/tmp/aoide-a2a-bearer-nonexistent-test.sock");
        let cfg = bearer_cfg("some-secret", dead, "file-token-should-be-ignored");
        let sentinel = resolve_inbound_bearer(&cfg);
        assert!(!sentinel.is_empty(), "a resolve failure must never read as 'not configured'");
        assert_ne!(sentinel, "file-token-should-be-ignored", "must not silently fall back to the file token");

        // Feed it through the EXACT machinery every other bearer check in
        // this file runs: nothing a caller could plausibly present matches,
        // and even the sentinel value ITSELF is never handed to a caller —
        // it only ever exists on this side of the comparison.
        assert!(!token_authorized(true, classify_token(&sentinel, Some("wrong"))));
        assert!(!token_authorized(true, classify_token(&sentinel, None)));
    }

    /// Two consecutive resolve failures never produce the same sentinel —
    /// pinning that it is fresh per call, not a fixed placeholder string an
    /// attacker could learn once and replay.
    #[test]
    fn resolve_inbound_bearer_sentinel_is_fresh_every_call() {
        let dead = Path::new("/tmp/aoide-a2a-bearer-nonexistent-test-2.sock");
        let cfg = bearer_cfg("some-secret", dead, "");
        let a = resolve_inbound_bearer(&cfg);
        let b = resolve_inbound_bearer(&cfg);
        assert_ne!(a, b);
    }

    /// A real broker + socket round trip: `bearer_secret` set AND a
    /// (deliberately wrong) `file_token` also set — the broker-resolved
    /// value wins outright, proving the precedence [`resolve_inbound_bearer`]'s
    /// own doc states.
    /// GATED on native Windows with its PROVEN reason, measured from the
    /// refusal itself: the fixture's broker stores through the built-in `file`
    /// backend, whose template is the POSIX preset `cat {home}/store/{name}` —
    /// and this tree REFUSES a POSIX-preset template BY NAME on native Windows
    /// (`CORE-POSIX.md`'s "shell interpreter for a stored command line" row:
    /// "never run under `cmd`, never silently stubbed"), which `put` returns as
    /// `Err(Other("backend `file` (set) is a built-in POSIX-shell preset …"))`.
    ///
    /// The contract this test covers is NOT left uncovered there: a
    /// broker-over-a-real-socket round trip with a HOST-SHAPED template is
    /// exercised natively by `aoide-secrets`' own
    /// `backend::tests::a_native_windows_template_*` (a real `cmd /C` template,
    /// end to end) and by the broker's own native suite, and the PRECEDENCE
    /// half stays covered on both hosts by the pure `bearer_cfg` asserts around
    /// this test.
    #[cfg_attr(windows, ignore = "the fixture stores through the built-in `file` backend, whose template is the POSIX preset `cat ...` — refused by name on native Windows (CORE-POSIX.md's shell-interpreter row); native cover: secrets::backend::tests::a_native_windows_template_*")]
    #[test]
    fn resolve_inbound_bearer_prefers_a_resolved_broker_secret_over_the_file_token() {
        let home = std::env::temp_dir().join(format!(
            "aoide-a2a-bearer-precedence-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&home).unwrap();
        aoide_secrets::store::save_policies(
            &home,
            &[aoide_secrets::policy::Policy::new("melete-door-token", "file", "k")],
        )
        .unwrap();

        // `/tmp` is not a path native Windows has, and a long socket name does
        // not fit `sun_path` there either — the one short-path seam answers both.
        let socket_path = aoide_test_support::short_tmp(&format!(
            "bearer-precedence-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ))
        .join("b.sock");
        let home_for_thread = home.clone();
        let sock_for_thread = socket_path.clone();
        let broker_thread = std::thread::spawn(move || {
            let _ = aoide_secrets::broker::serve(&home_for_thread, &sock_for_thread);
        });
        let mut connected = false;
        for _ in 0..50 {
            if UnixStream::connect(&socket_path).is_ok() {
                connected = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(connected, "broker did not bind {} in time", socket_path.display());

        let stored =
            aoide_secrets::client::put(&socket_path, "melete-door-token", "the-broker-value", false);
        assert!(matches!(stored, Ok(false)), "put must store a FRESH value: {stored:?}");

        let cfg = bearer_cfg("melete-door-token", &socket_path, "the-file-value-must-lose");
        assert_eq!(resolve_inbound_bearer(&cfg), "the-broker-value");

        drop(broker_thread);
        std::fs::remove_file(&socket_path).ok();
        std::fs::remove_dir_all(&home).ok();
    }

    /// A broker secret whose stored value is EMPTY must read as a resolve
    /// failure: an empty expected bearer is the "no token configured" signal,
    /// and treating it so would restore loopback's standing.
    #[cfg_attr(windows, ignore = "the fixture stores through the built-in `file` backend, whose template is the POSIX preset `cat ...`")]
    #[test]
    fn resolve_inbound_bearer_fails_closed_on_an_empty_resolved_secret() {
        let stamp = format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        );
        let home = std::env::temp_dir().join(format!("aoide-a2a-bearer-empty-{stamp}"));
        std::fs::create_dir_all(&home).unwrap();
        aoide_secrets::store::save_policies(
            &home,
            &[aoide_secrets::policy::Policy::new("empty-door-token", "file", "k")],
        )
        .unwrap();
        let socket_path = aoide_test_support::short_tmp(&format!("bearer-empty-{stamp}")).join("b.sock");
        let home_for_thread = home.clone();
        let sock_for_thread = socket_path.clone();
        let broker_thread = std::thread::spawn(move || {
            let _ = aoide_secrets::broker::serve(&home_for_thread, &sock_for_thread);
        });
        let mut connected = false;
        for _ in 0..50 {
            if UnixStream::connect(&socket_path).is_ok() {
                connected = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(connected, "broker did not bind {} in time", socket_path.display());
        // The broker's put refuses an empty value, so the fixture writes the
        // `file` backend's store directly, the way a stale empty value got there.
        std::fs::create_dir_all(home.join("store")).unwrap();
        std::fs::write(home.join("store").join("k"), "").unwrap();

        let cfg = bearer_cfg("empty-door-token", &socket_path, "");
        let expected = resolve_inbound_bearer(&cfg);
        assert!(!expected.is_empty(), "an empty resolved bearer must never read as 'not configured'");
        assert!(!token_authorized(!expected.is_empty(), classify_token(&expected, None)));

        drop(broker_thread);
        std::fs::remove_file(&socket_path).ok();
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn launch_token_fails_closed_on_a_configured_file_that_yields_no_token() {
        assert_eq!(launch_token(""), "", "no file configured is the off path");
        let dir = std::env::temp_dir().join(format!("aoide-a2a-launch-token-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("token");
        std::fs::write(&path, "\n").unwrap();
        for file in [path.to_str().unwrap(), "/nonexistent/aoide-a2a-token-file"] {
            let expected = launch_token(file);
            assert!(!expected.is_empty(), "a configured token file with no token must not read as 'not configured': {file}");
            assert!(!token_authorized(true, classify_token(&expected, None)));
        }
        std::fs::write(&path, "real\n").unwrap();
        assert_eq!(launch_token(path.to_str().unwrap()), "real");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── NO-CACHE / no-log grep gate (task #84 PINNED CONSTRAINT) ────────────

    /// A resolved/expected bearer value must NEVER reach an `audit(...)`
    /// call — grep this crate's own source for every `audit(` call site and
    /// forbid the identifiers that hold token bytes (`expected_token`,
    /// `presented_token`) from appearing inside its argument list. A future
    /// edit that accidentally threads either one into an audit line fails
    /// this test loudly instead of silently leaking a bearer into
    /// `~/Aoide/log`.
    /// Only the PRODUCTION half of this file (everything before `mod
    /// tests {`) — scanning the test module itself would trip over this
    /// very grep gate's own source text (its doc comments and string
    /// literals mention "audit(`"/`expected_token` by name to describe
    /// what it checks), which is noise, not a real call site.
    fn production_source() -> &'static str {
        let src = include_str!("a2a.rs");
        // The marker is searched LINE-ENDING AGNOSTICALLY: a Windows checkout
        // legitimately carries CRLF (`core.autocrlf`), and `include_str!` hands
        // back the file's bytes verbatim — so a `\n`-spelled needle finds
        // nothing there and this gate failed for a reason that has nothing to do
        // with what it guards. Same file, same scan, either checkout.
        let test_mod_start = src
            .find("#[cfg(test)]\r\nmod tests {")
            .or_else(|| src.find("#[cfg(test)]\nmod tests {"))
            .expect("this file has a `mod tests` block");
        &src[..test_mod_start]
    }

    /// Every balanced-paren call site in `production_source()` whose callee
    /// name is `name` (e.g. `"audit"`, `"eprintln!"` including its `!`) —
    /// paren-depth tracked so a call whose OWN arguments contain a nested
    /// `(...)` (a `format!(...)` argument, a `Door::A2a` path — none
    /// actually parenthesized, but future-proofed anyway) is captured
    /// whole, not truncated at the first inner `)`.
    fn call_sites<'a>(src: &'a str, name: &str) -> Vec<&'a str> {
        let needle = format!("{name}(");
        let mut sites = Vec::new();
        let mut idx = 0;
        while let Some(rel) = src[idx..].find(&needle) {
            let start = idx + rel;
            let mut depth = 0i32;
            let mut end = None;
            for (offset, ch) in src[start..].char_indices() {
                match ch {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            end = Some(start + offset + 1);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let Some(end) = end else { break };
            sites.push(&src[start..end]);
            idx = end;
        }
        sites
    }

    /// A resolved/expected bearer value must NEVER reach an `audit(...)`
    /// call — grep this crate's own PRODUCTION source (never the test
    /// module — see `production_source`'s doc) for every `audit(` call
    /// site and forbid the identifiers that hold token bytes
    /// (`expected_token`, `presented_token`) from appearing inside its
    /// argument list. A future edit that accidentally threads either one
    /// into an audit line fails this test loudly instead of silently
    /// leaking a bearer into `~/Aoide/log`.
    #[test]
    fn bearer_identifiers_never_reach_an_audit_call_grep_gate() {
        let src = production_source();
        let forbidden = ["expected_token", "presented_token"];
        let sites = call_sites(src, "audit");
        for call in &sites {
            for name in forbidden {
                assert!(
                    !call.contains(name),
                    "an audit(...) call mentions `{name}` — a resolved/expected bearer value must \
                     never reach the audit log:\n{call}"
                );
            }
        }
        assert!(sites.len() > 5, "sanity: this file should have several audit( call sites to check");
    }

    /// Same discipline, `eprintln!` (`resolve_inbound_bearer`'s own
    /// diagnostic on a resolve failure logs the SECRET'S NAME and the
    /// broker's error reason — never a resolved value).
    #[test]
    fn bearer_identifiers_never_reach_an_eprintln_call_grep_gate() {
        let src = production_source();
        let forbidden = ["expected_token", "presented_token"];
        for call in call_sites(src, "eprintln!") {
            for name in forbidden {
                assert!(
                    !call.contains(name),
                    "an eprintln!(...) call mentions `{name}` — a resolved/expected bearer value must \
                     never reach a log line:\n{call}"
                );
            }
        }
    }

    // ── `aoide/mailDeposit` (messaging plan P-M2, CONTRACTS.md §6) ──────────

    fn mail_deposit_root(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "aoide-a2a-maildeposit-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ))
    }

    /// Restores `AOIDE_STATE_DIR`/`AOIDE_STAGE_DIR` and removes `root` — the
    /// closing half of every test below, matching `a_successfully_
    /// delivered_message_send_files_into_the_mailbase`'s own inline shape
    /// rather than introducing a new fixture struct for eight call sites.
    fn mail_deposit_cleanup(root: &std::path::Path, saved_state: Option<String>, saved_stage: Option<String>) {
        let _ = std::fs::remove_dir_all(root);
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
    }

    /// A [`RequestCtx`] for the mail-deposit tests: the caller identity they
    /// need is a NAME (`deposit_admitted` reads `allows`, and `hop_name` is
    /// the name), so the key here is a fixture literal — no deposit path reads
    /// it, and the stamp that does is `message/send`'s.
    /// **An unloadable declaration set refuses BOTH mail methods, and only
    /// them.** MAIL.md §Status: a broken zone table means no zone check can run,
    /// and no zone check means no mail — never mail with the walls down. The
    /// refusal is a RESULT carrying the closed word `config-invalid`, audited
    /// once under the method's own label, so a sender's drain parks the entry
    /// rather than reading a dead link.
    #[test]
    fn an_unloadable_declaration_refuses_both_mail_methods() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let root = aoide_test_support::unique_tmp("door-config-invalid");
        charter_machine(&root, "receiver", "receiverbox");
        // A `[status]` for a node the mesh does not have: the section is refused
        // as a whole, which is what "a status for a line that is not there" is
        // for — and the refusal is what `declarations()` fails closed on.
        std::fs::write(root.join("receiver").join("config.toml"), "[mesh.home.status]\nnobody = \"down\"\n").unwrap();
        let audit_log = root.join("receiver").join("log");
        let ctx = mail_deposit_ctx(&audit_log, None);

        let deposit = mail_deposit(&json!({}), &ctx).expect("a refusal is an answer here, never an error");
        assert_eq!(deposit["status"], json!("refused"), "{deposit}");
        assert_eq!(deposit["reason"], json!("config-invalid"), "the closed word: {deposit}");

        let poll = mail_poll(&json!({ "node": "receiverbox" }), &ctx).expect("a refusal is an answer here, never an error");
        assert_eq!(poll["status"], json!("refused"), "{poll}");
        assert_eq!(poll["reason"], json!("config-invalid"), "{poll}");

        let log = std::fs::read_to_string(&audit_log).unwrap_or_default();
        assert!(log.contains("config-invalid"), "the host's own log says why: {log}");
        assert!(
            log.contains("a2a.aoide/mailDeposit") && log.contains("a2a.aoide/mailPoll"),
            "under the method's own label, so a flood is attributable: {log}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    fn mail_deposit_ctx<'a>(audit_log: &'a std::path::Path, name: Option<&'a str>) -> RequestCtx<'a> {
        RequestCtx {
            audit_log,
            spawn_agent: "",
            spawn_cwd: "",
            origin: ConnOrigin::Loopback,
            node_name: "",
            self_url: "",
            expected_token: "",
            presented_token: None,
            // P-CHARTER: the grant is read by the stored KEY, so this fixture
            // must carry the key the record actually holds — the same value
            // `verify_signed_request` would have resolved for a real signed
            // request — rather than a literal no record ever carries. The key
            // is leaked because the ctx BORROWS it for the test's lifetime
            // and the registry's own copy is dropped with its `Vec`; one
            // short string per test process, never a production path.
            signed_caller: name.map(|name| SignedCaller {
                name,
                key: Box::leak(
                    aoide_storage::node_store::load_nodes()
                        .into_iter()
                        .find(|p| p.name == name)
                        .and_then(|p| p.pubkey)
                        .unwrap_or_default()
                        .into_boxed_str(),
                ),
                mesh: None,
            }),
            sealed_only: false,
        }
    }

    #[test]
    fn may_message_reads_the_grant_so_a_revoked_capability_refuses() {
        // P-CHARTER shape: the capability question over a `Grant`, which is
        // `none()` for a caller that resolved to nothing at all (no
        // signature, no record, or a record granted nothing in this mesh) —
        // so a revoked capability and an unknown caller refuse through the
        // SAME answer, by construction.
        assert!(may_message(&grant_of("home", &["read", "message"])), "message in the grant");
        assert!(!may_message(&grant_of("home", &["read"])), "message revoked, read kept — refused");
        assert!(!may_message(&Grant::none()), "no record, no resolution, nothing granted — refused");
    }

    #[test]
    fn deposit_admitted_needs_a_grant_the_caller_cannot_fake() {
        // `deposit_admitted` is `may_message` and nothing else, and a
        // `Grant` exists only for a caller a verified signature resolved to
        // a record holding the capability IN THE MESH ITS REQUEST NAMES
        // (`grant_in_mesh`/`paired_grant`, exercised above against a real
        // registry). `Grant::none()` is what every other shape collapses to
        // — unsigned, addr-only, bare token, unknown key, or a grant in
        // another mesh — and this pins that it refuses.
        let mut paired_allowed = fixture_node("box-b", "http://10.0.0.5:8710/");
        paired_allowed.verified = true;
        paired_allowed.grants = aoide_storage::node_store::grants_in("home", &["message"]);

        assert!(deposit_admitted(&grant_for(&paired_allowed, "home")), "granted `message` in this mesh: admitted");
        assert!(!deposit_admitted(&Grant::none()), "no signature resolution at all — refused, with no fallback rung to try instead");
        assert!(
            !deposit_admitted(&grant_for(&paired_allowed, "away")),
            "the same record, a request naming another mesh it is not in — refused"
        );
    }

    #[test]
    fn an_unsigned_caller_is_refused_by_mail_deposit() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("unsigned");
        act_as(&root, "here");

        let audit_log = root.join("log");
        let envelope = aoide_storage::mail::mint_outbound_letter("alice", "there", "bob", "hi").unwrap();
        let ctx = mail_deposit_ctx(&audit_log, None);
        let params = json!({ "envelope": envelope });
        let err = mail_deposit(&params, &ctx).expect_err("no signature headers — must refuse, never file");
        assert_eq!(err.0, -32010);

        assert!(aoide_storage::mail::read_base().unwrap().is_empty(), "an unsigned caller's envelope is never filed");

        mail_deposit_cleanup(&root, saved_state, saved_stage);
    }

    #[test]
    fn a_paired_node_without_message_is_refused_with_a_taught_error() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("noallow");
        act_as(&root, "here");

        setup_signed_node_with_allows("box-b", &["read"]); // verified, paired, but no "message"

        let audit_log = root.join("log");
        let envelope = aoide_storage::mail::mint_outbound_letter("alice", "there", "bob", "hi").unwrap();
        let ctx = mail_deposit_ctx(&audit_log, Some("box-b"));
        let params = json!({ "envelope": envelope });
        let (code, msg) = mail_deposit(&params, &ctx).expect_err("paired but message not in allows — must refuse");
        assert_eq!(code, -32010);
        assert!(msg.contains("node allow box-b message on"), "names the exact fix: {msg}");

        mail_deposit_cleanup(&root, saved_state, saved_stage);
    }

    /// **The LAN guard on the door's join read** (`aoide/charterFetch`): a
    /// private peer is answered, and loopback, an undetermined address and a
    /// public address are all refused `-32007` with a taught message naming
    /// the non-LAN paths. Loopback is the case that matters most — a relayed
    /// forward and an ssh tunnel both arrive as loopback here, so admitting it
    /// would admit every relay the rule exists to refuse.
    #[test]
    fn charter_fetch_admits_the_local_network_and_refuses_everything_else() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_root = std::env::var("AOIDE_ROOT").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let root = mail_deposit_root("charter-fetch-guard");

        // A machine that has rooted and signed `home`, so there IS something
        // to fetch — the guard is what is under test, not an empty mesh.
        charter_machine(&root, "operator", "opbox");
        let init = aoide_storage::charter::init("home").unwrap();
        let line = aoide_storage::charter::node_line().unwrap();
        let src = format!("mesh = \"home\"\nversion = 0\nrelays = []\n\n[nodes]\n{line}\n");
        std::fs::write(aoide_storage::charter::source_path("home"), &src).unwrap();
        aoide_storage::charter::sign("home", None).unwrap();
        let audit_log = root.join("log");

        for (origin, label) in [
            (ConnOrigin::Remote("192.168.1.20".parse().unwrap()), "private"),
            (ConnOrigin::Remote("fd00::9".parse().unwrap()), "unique-local"),
            // A dual-stack door sees every v4 caller as v4-mapped; the guard
            // unwraps it and classifies by the v4 it maps to (review F4).
            (ConnOrigin::Remote("::ffff:192.168.1.20".parse().unwrap()), "a v4-mapped private peer"),
        ] {
            let out = charter_fetch(&json!({ "mesh": "home" }), origin, &audit_log)
                .unwrap_or_else(|e| panic!("a {label} peer is admitted: {e:?}"));
            assert_eq!(out["mesh"], "home");
            assert_eq!(out["operator"], init.operator);
            assert_eq!(out["version"], 1);
            assert!(out["charter"].as_str().is_some_and(|s| !s.is_empty()), "the document rides it");
            assert!(out["sig"].as_str().is_some_and(|s| !s.is_empty()), "and its signature");
        }

        for (origin, label) in [
            (ConnOrigin::Loopback, "loopback"),
            (ConnOrigin::Unknown, "an undetermined address"),
            (ConnOrigin::Remote("203.0.113.9".parse().unwrap()), "a public address"),
            (ConnOrigin::Remote("8.8.8.8".parse().unwrap()), "another public address"),
        ] {
            let err = charter_fetch(&json!({ "mesh": "home" }), origin, &audit_log)
                .unwrap_err();
            assert_eq!(err.0, -32007, "{label} is refused with the unauthorized code: {err:?}");
            assert!(err.1.contains("local network"), "and says why: {}", err.1);
            assert!(err.1.contains("--operator"), "and names the non-LAN path: {}", err.1);
        }

        // A mesh this machine has no charter for is its own taught answer, not
        // a guard refusal.
        let err = charter_fetch(&json!({ "mesh": "away" }), ConnOrigin::Remote("192.168.1.20".parse().unwrap()), &audit_log)
            .unwrap_err();
        assert!(err.1.contains("no charter in force"), "{err:?}");

        let _ = std::fs::remove_dir_all(&root);
        match saved_root {
            Some(v) => std::env::set_var("AOIDE_ROOT", v),
            None => std::env::remove_var("AOIDE_ROOT"),
        }
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    /// **F6: the ceremony's mesh rides the park's own write, never a second
    /// one.** `aoide/pairRequest` with a `mesh` leaves the parked entry
    /// carrying it — there is no best-effort follow-up write that could fail
    /// silently and leave the approver resolving the mesh locally (which is
    /// the asymmetry the field exists to prevent).
    #[test]
    fn pair_request_parks_the_mesh_it_was_sent() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp("a2a-pairrequest-mesh");
        std::env::set_var("AOIDE_STATE_DIR", &root);
        std::env::set_var("AOIDE_ROOT", &root);
        std::env::set_var("STRILLIX_ROOT", &root);

        let pubkey = "a".repeat(64);
        let commit = aoide_storage::pairing::derive_commit(&pubkey, &"c".repeat(32));
        let resp = pair_request(
            &json!({
                "pubkeyHex": pubkey,
                "name": "box-a",
                "commitHex": commit,
                "url": "http://box-a:8710/",
                "mesh": "away",
            }),
            ConnOrigin::Loopback,
            &root.join("log"),
        )
        .expect("a well-formed request with a mesh parks");
        assert!(resp["id"].is_string());

        let parked = aoide_storage::pairing::list_inbound(
            aoide_storage::time::parse_iso_utc(&now_iso_utc()).unwrap(),
        );
        assert_eq!(parked.len(), 1);
        assert_eq!(
            parked[0].mesh.as_deref(),
            Some("away"),
            "the mesh is on the entry the SAME write created — nothing was ignored"
        );

        // And a malformed mesh name is refused before anything is parked.
        let bad = pair_request(
            &json!({
                "pubkeyHex": "b".repeat(64),
                "name": "box-b",
                "commitHex": aoide_storage::pairing::derive_commit(&"b".repeat(64), &"d".repeat(32)),
                "url": "http://box-b:8710/",
                "mesh": "Not A Mesh",
            }),
            ConnOrigin::Loopback,
            &root.join("log"),
        )
        .unwrap_err();
        assert_eq!(bad.0, -32602);
        assert!(bad.1.contains("mesh name"), "{}", bad.1);

        let _ = std::fs::remove_dir_all(&root);
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    /// **Both mail arms refuse a charter-shaped mesh with the SAME text**
    /// (H1's merge review, F1). They did not: `deposit_refusal` had review
    /// N5's arm and `poll_refusal` had none, so the same box in the same state
    /// told a poller to run a command that answers `widens-charter`. The two
    /// now come off one helper (`charter_refusal`), and this pins the equality
    /// rather than either text — a copy would break the moment one side is
    /// edited.
    #[test]
    fn the_two_mail_arms_share_one_charter_refusal() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_root = std::env::var("AOIDE_ROOT").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let root = mail_deposit_root("shared-charter-refusal");

        // Shaped, and the operator key undecidable: the config line contradicts
        // the state record (`operator-mismatch`).
        charter_machine(&root, "operator", "opbox");
        let init = aoide_storage::charter::init("home").unwrap();
        let line = aoide_storage::charter::node_line().unwrap();
        let src = format!("mesh = \"home\"\nversion = 0\nrelays = []\n\n[nodes]\n{line}\n");
        std::fs::write(aoide_storage::charter::source_path("home"), &src).unwrap();
        aoide_storage::charter::sign("home", None).unwrap();
        let listed_key = aoide_storage::charter::parse(&src).unwrap().nodes["opbox"].key.clone();

        charter_machine(&root, "receiver", "receiverbox");
        aoide_storage::charter::trust_operator("home", &format!("ed25519:{}", "ab".repeat(32))).unwrap();
        std::fs::write(
            root.join("receiver").join("config.toml"),
            format!("[mesh.home]\noperator = \"ed25519:{}\"\n", "cd".repeat(32)),
        )
        .unwrap();
        assert!(aoide_storage::charter::charter_shaped("home") && aoide_storage::charter::governing("home").is_none());

        let audit_log = root.join("log");
        let caller = SignedCaller { name: "box-a", key: &listed_key, mesh: Some("home") };

        let (deposit_code, deposit) = deposit_refusal(Some(caller), "home", &audit_log);
        let (poll_code, poll) = poll_refusal(Some(caller), "home", "box-a", &audit_log);
        assert_eq!(deposit_code, -32010);
        assert_eq!(poll_code, -32010);
        let body_of = |m: &str| m.replacen("mail deposit refused: ", "", 1).replacen("mail poll refused: ", "", 1);
        assert_eq!(
            body_of(&deposit),
            body_of(&poll),
            "one helper, one teaching — the arms differ only in their own words:\n{deposit}\n{poll}"
        );
        assert!(poll.contains("UNDECIDABLE") && poll.contains("mesh charter show home"), "{poll}");
        assert!(!poll.contains("node allow box-a message on"), "and never the local `on` that cannot widen a charter: {poll}");

        let _ = init;
        let _ = std::fs::remove_dir_all(&root);
        match saved_root {
            Some(v) => std::env::set_var("AOIDE_ROOT", v),
            None => std::env::remove_var("AOIDE_ROOT"),
        }
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    /// **The door's refusal in a shaped mesh with an undecidable key is TAUGHT**
    /// (re-review N5): it names the mesh, says the operator key is undecidable
    /// and WHY (the `trusted_operator` reason), says there is no paired-record
    /// fallback and no local `on` that could widen it, and points at the one
    /// command that shows the recorded key — never the generic "run `node allow
    /// <name> message on`", which in that window answers `widens-charter` and
    /// which may name a caller that has no record here at all.
    #[test]
    fn a_shaped_mesh_with_an_undecidable_key_refuses_with_a_taught_reason() {
        use aoide_storage::node_store::AllowError;

        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_root = std::env::var("AOIDE_ROOT").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let root = mail_deposit_root("shaped-taught-refusal");

        // A charter in force, whose operator key the config line then
        // contradicts: shaped, undecidable.
        charter_machine(&root, "operator", "opbox");
        let init = aoide_storage::charter::init("home").unwrap();
        let line = aoide_storage::charter::node_line().unwrap();
        let src = format!("mesh = \"home\"\nversion = 0\nrelays = []\n\n[nodes]\n{line}\n");
        std::fs::write(aoide_storage::charter::source_path("home"), &src).unwrap();
        aoide_storage::charter::sign("home", None).unwrap();
        let listed_key = aoide_storage::charter::parse(&src).unwrap().nodes["opbox"].key.clone();

        charter_machine(&root, "receiver", "receiverbox");
        // The receiver records ONE operator key in state (a `mesh join`) and
        // its config line names a DIFFERENT one: shaped, and `operator-mismatch`.
        let state_key = "ab".repeat(32);
        let config_key = "cd".repeat(32);
        aoide_storage::charter::trust_operator("home", &format!("ed25519:{state_key}")).unwrap();
        std::fs::write(
            root.join("receiver").join("config.toml"),
            format!("[mesh.home]\noperator = \"ed25519:{config_key}\"\n"),
        )
        .unwrap();
        assert!(aoide_storage::charter::charter_shaped("home"), "charter-shaped");
        assert!(
            aoide_storage::charter::governing("home").is_none(),
            "and the key is undecidable — that is the window"
        );

        let (code, message) = deposit_refusal(
            Some(SignedCaller { name: "box-a", key: &listed_key, mesh: Some("home") }),
            "home",
            &root.join("log"),
        );
        assert_eq!(code, -32010);
        assert!(message.contains("CHARTER mesh"), "{message}");
        assert!(message.contains("UNDECIDABLE"), "{message}");
        assert!(message.contains("operator-mismatch"), "and says why: {message}");
        assert!(
            !message.contains(&state_key) && !message.contains(&config_key),
            "and the two operator KEYS stay out of an ungated caller's refusal — they name this host's config and state: {message}"
        );
        assert!(message.contains("mesh charter show home"), "and points at the one command that shows it: {message}");
        assert!(
            !message.contains("node allow box-a message on"),
            "this window is not a grant to fix with `node allow`: {message}"
        );

        // And the local widening refusal tells the same truth.
        let mut nodes = aoide_storage::node_store::load_nodes();
        let mut record = fixture_node("box-a", "http://10.0.0.9:8710/");
        record.verified = true;
        record.pubkey = Some("bb".repeat(32));
        nodes.push(record);
        assert_eq!(
            aoide_storage::node_store::set_node_allow(&mut nodes, "box-a", "read", true, "home").unwrap_err(),
            AllowError::WidensCharter
        );
        let _ = init;

        let _ = std::fs::remove_dir_all(&root);
        match saved_root {
            Some(v) => std::env::set_var("AOIDE_ROOT", v),
            None => std::env::remove_var("AOIDE_ROOT"),
        }
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    /// **F1's door half.** A charter letter the door admits and applies is
    /// answered with MAIL.md §Wire's own word — `"accepted"`, the only word a
    /// sender retires an entry on — with the charter detail in `data` beside
    /// it, never as the status itself.
    ///
    /// `aoide-client` cannot be reached from here (it sits beneath this
    /// crate), so the other half of the wire lives in that crate's
    /// `a_landed_charter_retires_the_senders_entry`, which reads this same
    /// reply shape through the sender's own classifier. This test is the one
    /// that fails if the door ever answers a word the sender was never taught.
    #[test]
    fn a_landed_charter_is_answered_accepted_with_its_detail_in_data() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_root = std::env::var("AOIDE_ROOT").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_config = std::env::var("AOIDE_CONFIG").ok();
        let saved_node = std::env::var("AOIDE_A2A_NODE_NAME").ok();
        let root = mail_deposit_root("charter-applied");

        // The receiving node prints its node line first — that is all the
        // operator needs from it.
        charter_machine(&root, "receiver", "receiverbox");
        let receiver_line = aoide_storage::charter::node_line().unwrap();

        // The operator's box: root the mesh, paste both lines under [nodes],
        // sign v1. Its identity keypair is kept — it is the key the charter's
        // `opbox` line carries, and the carriage assertions below sign as it.
        charter_machine(&root, "operator", "opbox");
        let init = aoide_storage::charter::init("home").unwrap();
        let (op_kp, _) = aoide_storage::identity::load_or_mint().unwrap();
        let op_line = aoide_storage::charter::node_line().unwrap();
        let src = format!(
            "mesh = \"home\"\nversion = 0\nrelays = []\n\n[nodes]\n{op_line}\n{receiver_line}\n"
        );
        std::fs::write(aoide_storage::charter::source_path("home"), &src).unwrap();
        aoide_storage::charter::sign("home", None).unwrap();
        let bytes = std::fs::read(aoide_storage::charter::source_path("home")).unwrap();
        let sig = std::fs::read(aoide_storage::charter::source_sig_path("home")).unwrap();
        let binding = aoide_storage::charter::parse(&src).unwrap().nodes["receiverbox"].age.clone();
        let container = aoide_storage::seal::seal_charter(
            &bytes,
            &sig,
            &binding,
            "home",
            1,
            "receiverbox",
            "receiverbox",
            &aoide_storage::time::now_iso_utc(),
        )
        .unwrap();

        // The receiving node trusts ONLY the operator line — **no pairing, and
        // no `nodes.json` record for the origin** (review F3: the first cut of
        // this test said exactly that and then INSTALLED the record at
        // `setup_signed_node_with_allows`, which made the test prove the
        // registry rung instead of the charter rung). What admits the deposit
        // now is the charter the receiver just accepted: the origin is a node
        // ON it, and its key verifies the request's signature
        // (`verify_signed_request`'s charter rung).
        charter_machine(&root, "receiver", "receiverbox");
        std::fs::write(
            root.join("receiver").join("config.toml"),
            format!("[mesh.home]\noperator = \"ed25519:{}\"\n", init.operator),
        )
        .unwrap();
        assert!(
            aoide_storage::node_store::load_nodes().is_empty(),
            "the premise, asserted where the old test merely claimed it: the receiver knows no node"
        );

        // The request must name the mesh it acts in for the charter rung to
        // apply — the mesh is inside the signature (`X-Aoide-Mesh`), and the
        // rung reads it, never the container's hop-mutable field. The signer is
        // a charter NODE's key, so the request is built from a keypair holding
        // it rather than from this process's own identity.
        let body = serde_json::to_vec(&json!({ "container": container })).unwrap();
        let now = 1_800_000_000_i64;
        let op_key = aoide_storage::charter::parse(&src).unwrap().nodes["opbox"].key.clone();
        assert_eq!(op_key, op_kp.info().pubkey_hex, "the charter's line carries the operator machine's own identity key");

        let audit_log = root.join("log");
        let ctx = RequestCtx { signed_caller: Some(SignedCaller { name: "opbox", key: op_key.as_str(), mesh: Some("home") }), ..mail_deposit_ctx(&audit_log, None) };

        let reply = mail_deposit(&json!({ "container": container }), &ctx)
            .expect("an admitted charter deposit is answered, never refused");
        assert_eq!(
            reply["status"],
            json!("accepted"),
            "the sender's own vocabulary, or its outbox parks a charter that landed: {reply}"
        );
        assert_eq!(reply["charter"]["mesh"], json!("home"));
        assert_eq!(reply["charter"]["version"], json!(1));
        assert!(reply["msgid"].is_string(), "and the entry it retires is named: {reply}");
        assert_eq!(
            aoide_storage::charter::in_force_charter("home").unwrap().version,
            1,
            "the charter really is in force on this node"
        );
        assert_eq!(
            aoide_storage::charter::load_trust("home").unwrap().unwrap().operator,
            init.operator,
            "and recorded under the operator key the config line named"
        );

        // **The rung that makes every LATER letter deliverable** (review F3).
        // The charter is in force now, and this box still holds no record for
        // `opbox`: a signed request naming `home` resolves through the
        // charter's own node list — identity only, the grant still the line's
        // (`grant_in_mesh`) — while a key the charter does not list resolves to
        // nothing, and a mesh that charter does not govern resolves to nothing.
        let req = signed_request_in_mesh(&op_kp, "opbox", "/", &body, now, &unique_nonce("carriage"), Some("home"));
        match verify_signed_request(&req, now, 0) {
            SignedRequestOutcome::Verified { resolved, key, .. } => {
                assert_eq!(resolved, "opbox", "the charter's line names the signer");
                assert_eq!(key, op_key, "and its key is the one that verified — identity only, no grant");
            }
            other => panic!("a charter-listed signer resolves through the charter rung, got {other:?}"),
        }
        assert!(
            aoide_storage::node_store::load_nodes().iter().all(|n| n.name != "opbox"),
            "and it resolved with no registry record at all"
        );
        let stranger_kp = aoide_storage::identity::mint_ephemeral().unwrap();
        let stranger_req =
            signed_request_in_mesh(&stranger_kp, "opbox", "/", &body, now, &unique_nonce("carriage-stranger"), Some("home"));
        assert!(matches!(
            verify_signed_request(&stranger_req, now, 0),
            SignedRequestOutcome::Refused(_, _)
        ), "a key the charter does not list resolves to nothing");
        let away_req = signed_request_in_mesh(&op_kp, "opbox", "/", &body, now, &unique_nonce("carriage-away"), Some("away"));
        assert!(matches!(
            verify_signed_request(&away_req, now, 0),
            SignedRequestOutcome::Refused(_, _)
        ), "and the rung never crosses meshes");

        let _ = std::fs::remove_dir_all(&root);
        for (key, value) in [
            ("AOIDE_ROOT", saved_root),
            ("AOIDE_STATE_DIR", saved_state),
            ("AOIDE_STAGE_DIR", saved_stage),
            ("AOIDE_CONFIG", saved_config),
            ("AOIDE_A2A_NODE_NAME", saved_node),
        ] {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }

    /// A machine for the charter-carriage test: its own `AOIDE_ROOT`,
    /// `AOIDE_STATE_DIR` and node name, so ONE process can be the operator's
    /// box and the receiving node in turn. `act_as` deliberately does not set
    /// `AOIDE_ROOT` (nothing else in this module reads a charter source or a
    /// config file), and the charter path reads both.
    fn charter_machine(root: &std::path::Path, who: &str, node_name: &str) {
        let dir = root.join(who);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("AOIDE_ROOT", &dir);
        std::env::set_var("STRILLIX_ROOT", &dir);
        std::env::set_var("AOIDE_STATE_DIR", dir.join("state"));
        std::env::set_var("AOIDE_STAGE_DIR", dir.join("stage"));
        std::env::remove_var("AOIDE_CONFIG");
        std::env::set_var("AOIDE_A2A_NODE_NAME", node_name);
    }

    /// Registers a node under `display::local_node_name()` — the name this box
/// presents as a NODE (the folded form; every registered node's own name and
/// every mesh key is `valid_node_name`-shaped, so the raw, case-preserving host
/// name would never match one — see `MeshSection::self_declared` in `client`).
    /// [`aoide_storage::mail::mint_outbound_letter`] will ever stamp as
    /// `header.from.node` (P-M1 ruling: self never crosses the wire) — so an
    /// envelope this test mints has a genuinely verifiable origin, using
    /// this test process's own identity as BOTH the origin's and the hop's
    /// key (the same "one process plays both roles" shortcut
    /// [`setup_signed_node`] already documents). Origin and hop coincide in
    /// P-M2, so this ALSO doubles as the connection's signed hop.
    fn setup_verifiable_origin(allows: &[&str]) -> String {
        let origin_name = aoide_storage::display::local_node_name();
        setup_signed_node_with_allows(&origin_name, allows);
        origin_name
    }

    #[test]
    fn a_plaintext_envelope_addressed_to_a_third_node_is_refused_and_audited() {
        // The plaintext lane is the DIRECT lane's: a hop that hands this door an
        // envelope addressed to someone else is asking it to relay plaintext, and
        // `mail::deposit` (which verifies and files — it never asks who the
        // addressee is) would happily file it. The door refuses it with the word
        // it teaches, and says so in the audit log.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("plaintext-third-party");
        act_as(&root, "here");

        let origin_name = setup_verifiable_origin(&["message"]);
        // Addressed somewhere else — and, since `to.name` is free attribution,
        // the letter is well-formed in every way except the one that matters.
        let envelope = aoide_storage::mail::mint_outbound_letter("alice", "away-node", "conductor", "in the clear").unwrap();
        assert_eq!(envelope.header.from.node, origin_name);

        let audit_log = root.join("log");
        let ctx = mail_deposit_ctx(&audit_log, Some(&origin_name));
        let req = json!({ "jsonrpc": "2.0", "id": 1, "method": "aoide/mailDeposit", "params": { "envelope": envelope } });
        let resp = handle_jsonrpc(&req, &ctx);
        assert_eq!(resp["result"]["status"], "refused", "{resp}");
        assert_eq!(resp["result"]["reason"], aoide_storage::seal::ADDRESSING_MISMATCH, "{resp}");
        assert!(
            aoide_storage::mail::read_base().unwrap().is_empty(),
            "nothing is filed for a node this box is not"
        );
        let log = std::fs::read_to_string(&audit_log).unwrap_or_default();
        assert!(log.contains("away-node") && log.contains("invalid"), "the refusal is audited: {log}");

        mail_deposit_cleanup(&root, saved_state, saved_stage);
    }

    #[test]
    fn a_deposit_from_a_verified_message_holding_node_files_a_letter() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("files");
        act_as(&root, "here");

        let origin_name = setup_verifiable_origin(&["message"]);
        aoide_storage::mail::box_add("conductor").unwrap();
        let envelope = aoide_storage::mail::mint_outbound_letter("alice", &aoide_storage::display::local_node_name(), "conductor", "hello from the wire").unwrap();
        assert_eq!(envelope.header.from.node, origin_name);

        let audit_log = root.join("log");
        let ctx = mail_deposit_ctx(&audit_log, Some(&origin_name));
        let req = json!({ "jsonrpc": "2.0", "id": 1, "method": "aoide/mailDeposit", "params": { "envelope": envelope } });
        let resp = handle_jsonrpc(&req, &ctx);
        assert_eq!(resp["result"]["status"], "accepted", "{resp}");

        let entries = aoide_storage::mail::read_base().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].envelope.header.to.name, "conductor");
        assert_eq!(entries[0].via, origin_name, "via is the HOP's resolved name");

        mail_deposit_cleanup(&root, saved_state, saved_stage);
    }

    #[test]
    fn a_filed_remote_letter_does_not_ring_in_the_door_process() {
        // P-M5a-2c: the architecture owner's ruling on b8af466 withdrew
        // `mail_deposit`'s in-process `aoide_conduct::graph::ring` call — a
        // ring now executes only inside the resident daemon. The fixture is
        // unchanged from the slice this corrects (a REAL headless wrap +
        // hook-fed child pair, armed for `conductor`, built the identical
        // way `aoide-conduct`'s own `graph::doorbell` tests build it) so the
        // ONLY thing that changed is the assertion: the deposit still files
        // and acks, but the armed reader's socket must never see a
        // connection, and the letter's own latch must stay armed for the
        // next daemon-side trigger (P-M5b-2 gives this door a forward path
        // of its own).
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_runtime = std::env::var("XDG_RUNTIME_DIR").ok();
        let root = mail_deposit_root("no-ring");
        act_as(&root, "here");
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        std::env::set_var("XDG_RUNTIME_DIR", root.join("runtime"));

        let origin_name = setup_verifiable_origin(&["message"]);

        let wrap_id = "wrap-1";
        let child_id = "wrap-1-child";
        let socket = root.join("wrap-1.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let wrap = SessionRecord {
            session_id: wrap_id.to_string(),
            state: "idle".to_string(),
            agent: "claude".to_string(),
            conductable: Some(true),
            socket: Some(socket.to_string_lossy().into_owned()),
            headless: true,
            ..Default::default()
        };
        let child = SessionRecord {
            session_id: child_id.to_string(),
            state: "stopped".to_string(),
            agent: "claude".to_string(),
            parent_session_id: Some(wrap_id.to_string()),
            ..Default::default()
        };
        write_stage(&sessions_path(), &SessionsFile { schema_version: String::new(), sessions: vec![wrap, child] }).unwrap();
        aoide_storage::mail::enrol_reader("conductor", wrap_id).unwrap();

        let envelope = aoide_storage::mail::mint_outbound_letter("alice", &aoide_storage::display::local_node_name(), "conductor", "hello from the wire").unwrap();
        let audit_log = root.join("log");
        let ctx = mail_deposit_ctx(&audit_log, Some(&origin_name));
        let req = json!({ "jsonrpc": "2.0", "id": 1, "method": "aoide/mailDeposit", "params": { "envelope": envelope } });

        let resp = handle_jsonrpc(&req, &ctx);
        assert_eq!(resp["result"]["status"], "accepted", "{resp}");

        // No bytes ever arrive — a short bounded poll, not a blocking
        // `accept`: there is no ring left in this process to connect, so
        // nothing here is ever supposed to become readable.
        listener.set_nonblocking(true).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(200);
        loop {
            match listener.accept() {
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                other => panic!("the door process must never ring the target itself: {other:?}"),
            }
            if std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let targets = aoide_storage::mail::ring_targets("conductor").unwrap();
        assert_eq!(targets.armed.len(), 1, "still armed for the next daemon-side trigger — a deposit files and acks, it does not ring");

        // Structural, same discipline the slice this corrects held: no arm
        // of `mail_deposit` may call `graph::ring(` anymore (flipped from
        // that slice's own "the letter arm must call it" assertion).
        let src = production_source();
        assert!(call_sites(src, "graph::ring").is_empty(), "aoide-server must never call graph::ring( directly anymore");

        mail_deposit_cleanup(&root, saved_state, saved_stage);
        match saved_runtime {
            Some(v) => std::env::set_var("XDG_RUNTIME_DIR", v),
            None => std::env::remove_var("XDG_RUNTIME_DIR"),
        }
    }

    #[test]
    fn a_duplicate_deposit_returns_duplicate_and_files_nothing_twice() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("dup");
        act_as(&root, "here");

        // `settle_deposit`'s best-effort drain must never hang or
        // block this test — a dead loopback port refuses instantly, unlike
        // the file's own `"http://node/"` placeholder (unresolvable
        // hostname, fine for the pure-predicate tests above that never
        // actually dial it, wrong for one that does).
        let origin_name = setup_verifiable_origin(&["message"]);
        let mut nodes = aoide_storage::node_store::load_nodes();
        nodes[0].url = "http://127.0.0.1:1/".to_string();
        aoide_storage::node_store::save_nodes(&nodes).unwrap();

        aoide_storage::mail::box_add("conductor").unwrap();
        let envelope = aoide_storage::mail::mint_outbound_letter("alice", &aoide_storage::display::local_node_name(), "conductor", "hello twice").unwrap();
        let audit_log = root.join("log");
        let ctx = mail_deposit_ctx(&audit_log, Some(&origin_name));
        let params = json!({ "envelope": envelope });

        let first = mail_deposit(&params, &ctx).unwrap();
        assert_eq!(first["status"], "accepted");

        let second = mail_deposit(&params, &ctx).unwrap();
        assert_eq!(second["status"], "duplicate");

        let entries = aoide_storage::mail::read_base().unwrap();
        assert_eq!(entries.len(), 1, "the duplicate deposit files nothing a second time");

        mail_deposit_cleanup(&root, saved_state, saved_stage);
    }

    /// The guard for the residual the nonce sets leave: each listener process keeps
    /// its own sets, so a deposit one listener files can be sent again as the same
    /// signed bytes to the other inside the window. That second frame is answered
    /// `duplicate`, and nothing is filed or spooled a second time. The A2A door's
    /// set for this key is emptied first, the state a second process starts with, so
    /// the msgid is the only thing that stops the second filing.
    #[test]
    fn a_deposit_one_listener_files_is_a_duplicate_at_the_other_and_is_filed_once() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("cross-listener");
        act_as(&root, "here");

        let origin_name = aoide_storage::display::local_node_name();
        let kp = setup_signed_node_with_grants(&origin_name, "home", &["message"]);
        let mut nodes = aoide_storage::node_store::load_nodes();
        nodes[0].url = "http://127.0.0.1:1/".to_string();
        aoide_storage::node_store::save_nodes(&nodes).unwrap();
        aoide_storage::mail::box_add("conductor").unwrap();

        // The mail adapter takes sealed containers only, so the letter goes as a
        // container sealed to this box's binding and handed over by the signer.
        let local = aoide_storage::display::local_node_name();
        let envelope = aoide_storage::mail::mint_outbound_letter("alice", &local, "conductor", "hello across listeners").unwrap();
        let binding = aoide_storage::seal::publish_binding().unwrap();
        let now = aoide_storage::time::now_iso_utc();
        let container = aoide_storage::seal::seal_envelope(&envelope, &binding, "home", "home", &local, &origin_name, &now).unwrap();
        let own = kp.info().pubkey_hex.clone();
        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "aoide/mailDeposit", "params": { "container": container, "to": own } });
        let nonce = unique_nonce("cross-listener");
        let request = raw_request(
            "127.0.0.1",
            "POST",
            "/",
            body.to_string().as_bytes(),
            Some((&kp, &origin_name, epoch_now(), &nonce)),
            Some("home"),
        );
        let answer = |raw: String| -> Value { serde_json::from_str(raw.split_once("\r\n\r\n").unwrap().1).unwrap() };

        let audit = root.join("log");
        let mail_port = free_loopback_port();
        let a2a_port = free_loopback_port();
        raise_mail_adapter(mail_port, &audit);
        raise_a2a_door(a2a_port, &audit);

        let filed = answer(send_raw(mail_port, &request));
        assert_eq!(filed["result"]["status"], "accepted", "the mail adapter files the deposit: {filed}");
        let spooled = aoide_storage::outbox::list_entries(&origin_name).unwrap().len();

        NONCE_SETS.lock().unwrap_or_else(|e| e.into_inner()).remove(&own.to_ascii_lowercase());
        assert!(
            !NONCE_SETS.lock().unwrap_or_else(|e| e.into_inner()).contains_key(&own.to_ascii_lowercase()),
            "the first listener's nonce set is gone, so the replay below reaches the msgid check: {own}"
        );
        let again = answer(send_raw(a2a_port, &request));
        assert_eq!(again["result"]["status"], "duplicate", "the same signed bytes at the other listener: {again}");

        assert_eq!(aoide_storage::mail::read_base().unwrap().len(), 1, "filed once, across both listeners");
        assert_eq!(
            aoide_storage::outbox::list_entries(&origin_name).unwrap().len(),
            spooled,
            "and spooled no second time"
        );

        mail_deposit_cleanup(&root, saved_state, saved_stage);
    }

    #[test]
    fn a_duplicate_of_a_filed_letter_inside_the_hour_holds_its_reack_back() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("dup-respool");
        act_as(&root, "here");

        let origin_name = setup_verifiable_origin(&["message"]);
        let mut nodes = aoide_storage::node_store::load_nodes();
        nodes[0].url = "http://127.0.0.1:1/".to_string();
        aoide_storage::node_store::save_nodes(&nodes).unwrap();

        aoide_storage::mail::box_add("conductor").unwrap();
        let envelope = aoide_storage::mail::mint_outbound_letter("alice", &aoide_storage::display::local_node_name(), "conductor", "hi").unwrap();
        let audit_log = root.join("log");
        let ctx = mail_deposit_ctx(&audit_log, Some(&origin_name));
        let params = json!({ "envelope": envelope });

        mail_deposit(&params, &ctx).unwrap();
        let before = aoide_storage::outbox::list_entries(&origin_name).unwrap();
        assert_eq!(before.len(), 1, "filing a letter spools an ack toward the origin");
        aoide_storage::outbox::remove_entry(&origin_name, &before[0].envelope.msgid).unwrap();
        assert!(aoide_storage::outbox::list_entries(&origin_name).unwrap().is_empty(), "ack removed, simulating an earlier successful drain");

        // The SAME envelope again, inside the hour: a duplicate re-acks at most
        // once per key per hour (`mail::reack_due`), and the first ack already
        // went out. The re-ack after the hour is proved in storage.
        mail_deposit(&params, &ctx).unwrap();
        let after = aoide_storage::outbox::list_entries(&origin_name).unwrap();
        assert!(after.is_empty(), "a duplicate inside the hour spools no second ack: {after:?}");

        mail_deposit_cleanup(&root, saved_state, saved_stage);
    }

    #[test]
    fn repeated_duplicate_redeliveries_spool_exactly_one_ack() {
        // The outbox investigation's own root cause: a sender that never
        // sees its ack redelivers the SAME letter, `mail::deposit`
        // correctly classifies each redelivery as `Duplicate{filed_letter:
        // true}`, and the old `spool_and_drain_ack` used to mint a BRAND-NEW ack
        // envelope — new msgid, new file — on every single one, with the
        // ack still sitting undelivered in the spool the whole time (never
        // removed, so this never depends on `mail_deposit`'s own
        // best-effort drain succeeding or failing). This pins the ledger
        // fix: N redeliveries of the same letter must leave exactly ONE
        // spooled ack for that (reader, msgid), not N.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("dup-flood-one-ack");
        act_as(&root, "here");

        let origin_name = setup_verifiable_origin(&["message"]);
        let mut nodes = aoide_storage::node_store::load_nodes();
        nodes[0].url = "http://127.0.0.1:1/".to_string();
        aoide_storage::node_store::save_nodes(&nodes).unwrap();

        aoide_storage::mail::box_add("conductor").unwrap();
        let envelope = aoide_storage::mail::mint_outbound_letter("alice", &aoide_storage::display::local_node_name(), "conductor", "hi").unwrap();
        let audit_log = root.join("log");
        let ctx = mail_deposit_ctx(&audit_log, Some(&origin_name));
        let params = json!({ "envelope": envelope });

        let first = mail_deposit(&params, &ctx).unwrap();
        assert_eq!(first["status"], "accepted");
        let first_spool = aoide_storage::outbox::list_entries(&origin_name).unwrap();
        assert_eq!(first_spool.len(), 1, "the first filing spools exactly one ack");
        let ack_msgid = first_spool[0].envelope.msgid.clone();

        // The ack is deliberately left in the spool (undelivered) across
        // every redelivery below — exactly the "dead link" condition that
        // produced 16.5k duplicates.
        for _ in 0..10 {
            let redelivered = mail_deposit(&params, &ctx).unwrap();
            assert_eq!(redelivered["status"], "duplicate");
        }

        let spool = aoide_storage::outbox::list_entries(&origin_name).unwrap();
        assert_eq!(spool.len(), 1, "ten redeliveries must still leave exactly one spooled ack");
        assert_eq!(spool[0].envelope.msgid, ack_msgid, "the surviving ack is the ORIGINAL one, never re-minted");

        mail_deposit_cleanup(&root, saved_state, saved_stage);
    }

    #[test]
    fn filing_a_letter_spools_an_ack_signed_by_this_box() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("ack-shape");
        act_as(&root, "here");

        let origin_name = setup_verifiable_origin(&["message"]);
        let mut nodes = aoide_storage::node_store::load_nodes();
        nodes[0].url = "http://127.0.0.1:1/".to_string();
        aoide_storage::node_store::save_nodes(&nodes).unwrap();

        aoide_storage::mail::box_add("conductor").unwrap();
        let envelope = aoide_storage::mail::mint_outbound_letter("alice", &aoide_storage::display::local_node_name(), "conductor", "hi").unwrap();
        let audit_log = root.join("log");
        let ctx = mail_deposit_ctx(&audit_log, Some(&origin_name));
        let params = json!({ "envelope": envelope });
        let result = mail_deposit(&params, &ctx).unwrap();
        let msgid = result["msgid"].as_str().unwrap().to_string();

        let spooled = aoide_storage::outbox::list_entries(&origin_name).unwrap();
        assert_eq!(spooled.len(), 1);
        let ack = &spooled[0].envelope;
        assert_eq!(ack.header.kind, aoide_storage::mail::ENTRY_TYPE_RECEIPT);
        assert_eq!(ack.header.to.node, origin_name, "the ack's `to` is the origin");
        assert_eq!(ack.text, msgid, "the ack's text is the acked msgid");
        assert_eq!(ack.header.from.node, aoide_storage::display::local_node_name(), "signed by this box");

        mail_deposit_cleanup(&root, saved_state, saved_stage);
    }

    #[test]
    fn a_deposit_audits_under_its_own_method_label() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("audit-label");
        act_as(&root, "here");

        let origin_name = setup_verifiable_origin(&["message"]);
        let mut nodes = aoide_storage::node_store::load_nodes();
        nodes[0].url = "http://127.0.0.1:1/".to_string();
        aoide_storage::node_store::save_nodes(&nodes).unwrap();

        let envelope = aoide_storage::mail::mint_outbound_letter("alice", &aoide_storage::display::local_node_name(), "conductor", "hi").unwrap();
        let audit_log = root.join("log");
        let ctx = mail_deposit_ctx(&audit_log, Some(&origin_name));
        let params = json!({ "envelope": envelope });
        mail_deposit(&params, &ctx).unwrap();

        let log = std::fs::read_to_string(&audit_log).unwrap();
        assert!(log.contains("a2a.aoide/mailDeposit"), "{log}");
        assert!(!log.contains("a2a.rpc"), "never falls back to the generic label: {log}");

        mail_deposit_cleanup(&root, saved_state, saved_stage);
    }

    #[test]
    fn an_envelope_whose_origin_key_is_unknown_is_unverified_origin() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("unverified-origin");
        act_as(&root, "here");

        // The HOP is a genuinely admitted, message-holding node — but
        // registered under a DIFFERENT name than the envelope's own origin
        // (`display::local_host_name()`, which `mint_outbound_letter`
        // always stamps and which this test never registers), so admission
        // succeeds while the origin lookup still has no key to try. This is
        // the two-lookup split itself (spec item 3): the hop and the origin
        // are read from two different places, and P-M2 having them usually
        // coincide is not the same as them being the same read.
        setup_signed_node_with_allows("box-hop", &["message"]);

        let envelope = aoide_storage::mail::mint_outbound_letter("alice", &aoide_storage::display::local_node_name(), "conductor", "hi").unwrap();
        let origin_name = envelope.header.from.node.clone();
        assert!(
            aoide_storage::node_store::load_nodes().iter().all(|n| n.name != origin_name),
            "sanity: nothing is registered under the envelope's own origin name"
        );

        let audit_log = root.join("log");
        let ctx = mail_deposit_ctx(&audit_log, Some("box-hop"));
        let params = json!({ "envelope": envelope });
        // MAIL.md §Wire: admission (step 1) is a JSON-RPC error; what
        // becomes of a well-formed envelope (steps 2 onward, this one) is a
        // RESULT — a refused outcome is not the same answer as "you may
        // not speak to this method at all."
        let result = mail_deposit(&params, &ctx).expect("no key on record for the origin is an OUTCOME, not a protocol error");
        assert_eq!(result["status"], "refused");
        assert_eq!(result["reason"], "unverified-origin");
        assert!(result["detail"].as_str().unwrap().contains(&origin_name), "{result}");

        let log = std::fs::read_to_string(&audit_log).unwrap();
        assert!(log.contains("\"status\":\"invalid\""), "a refused RESULT still audits as invalid, unconditionally: {log}");

        assert!(aoide_storage::mail::read_base().unwrap().is_empty(), "an unverified origin is never filed");

        mail_deposit_cleanup(&root, saved_state, saved_stage);
    }

    #[test]
    fn a_deposit_with_a_mismatched_msgid_is_a_refused_result_not_a_protocol_error() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("bad-msgid");
        act_as(&root, "here");

        // A genuinely verifiable origin (unlike the sibling test above) —
        // this proves the msgid check is what refuses, not a side effect of
        // an origin this test never bothered to register.
        let origin_name = setup_verifiable_origin(&["message"]);
        let mut envelope = aoide_storage::mail::mint_outbound_letter("alice", &aoide_storage::display::local_node_name(), "conductor", "hi").unwrap();
        envelope.msgid = "0".repeat(64); // well-formed hex, does not recompute

        let audit_log = root.join("log");
        let ctx = mail_deposit_ctx(&audit_log, Some(&origin_name));
        let params = json!({ "envelope": envelope });
        let result = mail_deposit(&params, &ctx).expect("a tampered msgid is an OUTCOME, not a protocol error");
        assert_eq!(result["status"], "refused");
        assert_eq!(result["reason"], "bad-msgid");
        assert!(result["detail"].as_str().unwrap().contains("recomputed"), "{result}");

        let log = std::fs::read_to_string(&audit_log).unwrap();
        assert!(log.contains("\"status\":\"invalid\""), "a refused RESULT still audits as invalid, unconditionally: {log}");

        assert!(aoide_storage::mail::read_base().unwrap().is_empty(), "a bad msgid is never filed");

        mail_deposit_cleanup(&root, saved_state, saved_stage);
    }

    // ── `aoide/mailPoll` (messaging plan P-M3, CONTRACTS.md §6) ────────────

    /// Two paired, `message`-holding nodes with a held letter spooled toward
    /// each — the fixture every poll test below starts from. Returns the
    /// (box-b msgid, box-c msgid) pair.
    fn two_pollers_fixture() -> (String, String) {
        setup_signed_node_with_allows("box-b", &["message"]);
        let mut nodes = aoide_storage::node_store::load_nodes();
        let mut box_c = fixture_node("box-c", "http://node/");
        box_c.verified = true;
        box_c.grants = aoide_storage::node_store::grants_in("home", &["message"]);
        nodes.push(box_c);
        aoide_storage::node_store::save_nodes(&nodes).unwrap();

        let for_b = aoide_storage::mail::mint_outbound_letter("alice", "box-b", "bob", "held for b").unwrap();
        let for_c = aoide_storage::mail::mint_outbound_letter("alice", "box-c", "carol", "held for c").unwrap();
        let (b, c) = (for_b.msgid.clone(), for_c.msgid.clone());
        aoide_storage::outbox::write_entry("box-b", &aoide_storage::outbox::OutboxEntry::held(for_b)).unwrap();
        aoide_storage::outbox::write_entry("box-c", &aoide_storage::outbox::OutboxEntry::held(for_c)).unwrap();
        (b, c)
    }

    #[test]
    fn poll_admitted_requires_a_signed_message_holder_asking_for_its_own_node() {
        let mut paired = fixture_node("box-b", "http://10.0.0.5:8710/");
        paired.verified = true;
        paired.grants = aoide_storage::node_store::grants_in("home", &["read", "message"]);
        let grant = grant_for(&paired, "home");
        let sig = |name: &'static str| Some(SignedCaller { name, key: "aa11", mesh: None });

        assert!(poll_admitted(sig("box-b"), &grant, "box-b"), "granted `message` + asking for itself");

        assert!(!poll_admitted(sig("box-b"), &grant, "box-c"), "no polling on another's behalf");
        assert!(!poll_admitted(sig("box-c"), &grant, "box-b"), "nor the other way round — the CLAIM must be the caller's own name");

        let mut denied = paired.clone();
        denied.grants = aoide_storage::node_store::grants_in("home", &["read"]);
        assert!(!poll_admitted(sig("box-b"), &grant_for(&denied, "home"), "box-b"), "message revoked — refused");
        assert!(
            !poll_admitted(sig("box-b"), &grant_for(&paired, "away"), "box-b"),
            "the same record, a request naming a mesh it holds nothing in — refused"
        );
        assert!(!poll_admitted(None, &grant, "box-b"), "no verified signature resolution at all");
        assert!(!poll_admitted(sig("box-b"), &Grant::none(), "box-b"), "and no grant to speak with either");
    }

    /// **A poller's `deferred` list defers the entries it names, in its own
    /// spool and nowhere else.** The poller was handed a container and did not
    /// file it (its box is full), so it names it; the hub defers that entry for
    /// this poller only, and another node's entry for the same msgid is untouched.
    #[test]
    fn a_hub_defers_only_the_callers_own_entries() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("poll-defer-own");
        act_as(&root, "here");

        setup_signed_node_with_allows("box-b", &["message"]);
        let binding = aoide_storage::seal::publish_binding().unwrap();
        let envelope = aoide_storage::mail::mint_outbound_letter("alice", "box-b", "bob", "for b").unwrap();
        let container = aoide_storage::seal::seal_envelope(
            &envelope,
            &binding,
            "",
            "",
            "box-b",
            "box-b",
            &aoide_storage::time::now_iso_utc(),
        )
        .unwrap();
        let msgid = container.msgid.clone();
        aoide_storage::outbox::write_entry("box-b", &aoide_storage::outbox::OutboxEntry::transit_held(container.clone()))
            .unwrap();
        aoide_storage::outbox::write_entry("box-c", &aoide_storage::outbox::OutboxEntry::transit_held(container))
            .unwrap();

        let audit_log = root.join("log");
        let ctx = mail_deposit_ctx(&audit_log, Some("box-b"));
        mail_poll(&json!({ "node": "box-b", "deferred": [msgid] }), &ctx).unwrap();

        let own = aoide_storage::outbox::list_entries("box-b").unwrap();
        assert_eq!(own.len(), 1, "{own:?}");
        assert_eq!(own[0].tries, 1, "named: the hub defers it for this poller");
        assert!(own[0].last_outcome.starts_with("deferred:"), "{}", own[0].last_outcome);
        let other = aoide_storage::outbox::list_entries("box-c").unwrap();
        assert_eq!(other[0].tries, 0, "another node's entry for the same msgid is not deferred");

        mail_deposit_cleanup(&root, saved_state, saved_stage);
    }

    /// **Fifty deferred entries never take a poll's batch.** The poller names
    /// the fifty it could not file; they wait their window, so the one fresh
    /// letter behind them is the whole answer. Without the deferral the first
    /// fifty fill the batch and the fresh letter waits for the next poll, and
    /// the next, for as long as the wall stands.
    #[test]
    fn a_wall_of_deferred_poll_entries_never_starves_a_fresh_letter() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("poll-defer-wall");
        act_as(&root, "here");

        setup_signed_node_with_allows("box-b", &["message"]);
        let binding = aoide_storage::seal::publish_binding().unwrap();
        let now = aoide_storage::time::now_iso_utc();
        let seal = |text: &str| {
            let envelope = aoide_storage::mail::mint_outbound_letter("alice", "box-b", "bob", text).unwrap();
            aoide_storage::seal::seal_envelope(&envelope, &binding, "", "", "box-b", "box-b", &now).unwrap()
        };
        let mut wall = Vec::new();
        for i in 0..50 {
            let container = seal(&format!("wall {i}"));
            wall.push(container.msgid.clone());
            aoide_storage::outbox::write_entry("box-b", &aoide_storage::outbox::OutboxEntry::transit_held(container))
                .unwrap();
        }
        let fresh = seal("the fresh one");
        let fresh_msgid = fresh.msgid.clone();
        aoide_storage::outbox::write_entry("box-b", &aoide_storage::outbox::OutboxEntry::transit_held(fresh)).unwrap();

        let audit_log = root.join("log");
        let ctx = mail_deposit_ctx(&audit_log, Some("box-b"));
        let answer = mail_poll(&json!({ "node": "box-b", "deferred": wall }), &ctx).unwrap();
        let offered: Vec<String> = answer["containers"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|c| c["msgid"].as_str().map(str::to_string))
            .collect();
        assert_eq!(offered, vec![fresh_msgid], "the deferred fifty take no slot; the fresh letter is offered");

        mail_deposit_cleanup(&root, saved_state, saved_stage);
    }

    /// **The `deferred` key is additive.** An old hub reads its params with
    /// `get` and never looks at the key, so a poll that carries it must answer
    /// exactly what the same poll without it answers. Names a msgid this hub
    /// does not hold, so the list changes nothing it could not already see.
    /// (This guards a compatibility property, so it passes before the fix too.)
    #[test]
    fn an_old_hub_ignores_the_deferred_list() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("poll-defer-old-hub");
        act_as(&root, "here");

        setup_signed_node_with_allows("box-b", &["message"]);
        let binding = aoide_storage::seal::publish_binding().unwrap();
        let envelope = aoide_storage::mail::mint_outbound_letter("alice", "box-b", "bob", "for b").unwrap();
        let container = aoide_storage::seal::seal_envelope(
            &envelope,
            &binding,
            "",
            "",
            "box-b",
            "box-b",
            &aoide_storage::time::now_iso_utc(),
        )
        .unwrap();
        let msgid = container.msgid.clone();
        aoide_storage::outbox::write_entry("box-b", &aoide_storage::outbox::OutboxEntry::transit_held(container)).unwrap();

        let audit_log = root.join("log");
        let ctx = mail_deposit_ctx(&audit_log, Some("box-b"));
        let plain = mail_poll(&json!({ "node": "box-b" }), &ctx).unwrap();
        let with_key = mail_poll(&json!({ "node": "box-b", "deferred": ["0".repeat(64)] }), &ctx).unwrap();
        assert_eq!(plain["containers"], with_key["containers"], "the key changes nothing unnamed");
        assert_eq!(with_key["containers"][0]["msgid"], json!(msgid));
        assert_eq!(aoide_storage::outbox::list_entries("box-b").unwrap()[0].tries, 0);

        mail_deposit_cleanup(&root, saved_state, saved_stage);
    }

    /// **Junk in the `deferred` list is dropped, and a valid msgid beside it
    /// still counts.** A non-hex text never names a spool file, so a `../` or a
    /// capital is not a path and not a msgid; the lowercase hex msgid next to it
    /// is deferred as usual.
    #[test]
    fn a_deferred_list_with_a_non_hex_msgid_is_ignored() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("poll-defer-junk");
        act_as(&root, "here");

        setup_signed_node_with_allows("box-b", &["message"]);
        let binding = aoide_storage::seal::publish_binding().unwrap();
        let envelope = aoide_storage::mail::mint_outbound_letter("alice", "box-b", "bob", "for b").unwrap();
        let container = aoide_storage::seal::seal_envelope(
            &envelope,
            &binding,
            "",
            "",
            "box-b",
            "box-b",
            &aoide_storage::time::now_iso_utc(),
        )
        .unwrap();
        let msgid = container.msgid.clone();
        aoide_storage::outbox::write_entry("box-b", &aoide_storage::outbox::OutboxEntry::transit_held(container)).unwrap();

        let audit_log = root.join("log");
        let ctx = mail_deposit_ctx(&audit_log, Some("box-b"));
        let junk = json!(["../escape", msgid.to_uppercase(), 7, msgid]);
        mail_poll(&json!({ "node": "box-b", "deferred": junk }), &ctx).unwrap();

        let own = aoide_storage::outbox::list_entries("box-b").unwrap();
        assert_eq!(own.len(), 1, "{own:?}");
        assert_eq!(own[0].tries, 1, "the valid msgid is deferred; the junk names nothing");

        mail_deposit_cleanup(&root, saved_state, saved_stage);
    }

    /// **One msgid named fifty times is deferred once.** The list is deduplicated
    /// before the cap, so a repeated name neither takes fifty batch-sized steps
    /// nor counts as fifty tries.
    #[test]
    fn one_msgid_repeated_fifty_times_is_deferred_once() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("poll-defer-repeat");
        act_as(&root, "here");

        setup_signed_node_with_allows("box-b", &["message"]);
        let binding = aoide_storage::seal::publish_binding().unwrap();
        let envelope = aoide_storage::mail::mint_outbound_letter("alice", "box-b", "bob", "for b").unwrap();
        let container = aoide_storage::seal::seal_envelope(
            &envelope,
            &binding,
            "",
            "",
            "box-b",
            "box-b",
            &aoide_storage::time::now_iso_utc(),
        )
        .unwrap();
        let msgid = container.msgid.clone();
        aoide_storage::outbox::write_entry("box-b", &aoide_storage::outbox::OutboxEntry::transit_held(container)).unwrap();

        let audit_log = root.join("log");
        let ctx = mail_deposit_ctx(&audit_log, Some("box-b"));
        mail_poll(&json!({ "node": "box-b", "deferred": vec![msgid; 50] }), &ctx).unwrap();

        let own = aoide_storage::outbox::list_entries("box-b").unwrap();
        assert_eq!(own.len(), 1, "{own:?}");
        assert_eq!(own[0].tries, 1, "fifty copies of one msgid are one deferral");

        mail_deposit_cleanup(&root, saved_state, saved_stage);
    }

    /// The poll's own acknowledgement, at the door: a `transit` entry held for
    /// the poller is offered until the poller NAMES it in `filed`, and then it is
    /// retired — the hand-over itself retires nothing (a response can be lost).
    #[test]
    fn a_poll_retires_a_held_container_only_when_the_poller_names_it() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("poll-ack");
        act_as(&root, "here");

        setup_signed_node_with_allows("box-b", &["message"]);
        let envelope = aoide_storage::mail::mint_outbound_letter("alice", "box-b", "bob", "for b").unwrap();
        let container = {
            let binding = aoide_storage::seal::publish_binding().unwrap();
            aoide_storage::seal::seal_envelope(
                &envelope,
                &binding,
                "",
                "",
                "box-b",
                "box-b",
                &aoide_storage::time::now_iso_utc(),
            )
            .unwrap()
        };
        let msgid = container.msgid.clone();
        aoide_storage::outbox::write_entry("box-b", &aoide_storage::outbox::OutboxEntry::transit_held(container)).unwrap();

        let audit_log = root.join("log");
        let ctx = mail_deposit_ctx(&audit_log, Some("box-b"));

        let first = mail_poll(&json!({ "node": "box-b" }), &ctx).unwrap();
        assert_eq!(first["containers"].as_array().unwrap().len(), 1, "{first}");
        assert_eq!(
            aoide_storage::outbox::list_entries("box-b").unwrap().len(),
            1,
            "the hand-over retires nothing: only the poller's word does"
        );

        let second = mail_poll(&json!({ "node": "box-b", "filed": [msgid] }), &ctx).unwrap();
        assert_eq!(second["containers"].as_array().unwrap().len(), 0, "named: retired, not offered");
        assert!(aoide_storage::outbox::list_entries("box-b").unwrap().is_empty());
        // The retirement is a write, and it is in the log: the count, the msgids,
        // and the poller whose word ended this hub's custody.
        let log = std::fs::read_to_string(&audit_log).unwrap_or_default();
        assert!(
            log.contains("1 acknowledged and retired") && log.contains(&msgid),
            "the ok line names what it retired: {log}"
        );

        mail_deposit_cleanup(&root, saved_state, saved_stage);
    }

    /// **A hub that holds a container it cannot move says so.** The origin's
    /// retry at such a hub dedups; answering `duplicate` forever would leave the
    /// origin retrying a letter that is going nowhere with nothing saying why. The
    /// door answers the word the hub's own next hop gave, audited, and the letter
    /// stays where it is.
    #[test]
    fn a_hub_whose_next_hop_refused_answers_the_retry_with_that_word() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_root = std::env::var("AOIDE_ROOT").ok();
        let root = mail_deposit_root("hub-parked");
        act_as(&root, "here");
        std::env::set_var("AOIDE_ROOT", &root);
        std::env::set_var("STRILLIX_ROOT", &root);
        let far_key = hop_fixture();
        let container = hop_container(&far_key);

        // A hop: the container is for another node, the hub carries it on to
        // `dave`, and the gate records the hop once it is on disk.
        let audit_log = root.join("log");
        let ctx = mail_deposit_ctx(&audit_log, Some("far"));
        let params = json!({ "container": container });
        let first = mail_deposit(&params, &ctx).unwrap();
        assert_eq!(first["status"], "accepted", "{first}");

        // The hub's own custody of that letter is parked: its next hop answered
        // `no-route`, which is what the origin must be told.
        let mut parked = aoide_storage::outbox::OutboxEntry::transit(container.clone());
        parked.refused = true;
        parked.last_try_at = aoide_storage::time::now_iso_utc();
        parked.last_outcome = "refused: no-route: `dave` is not a node of mesh `home`".to_string();
        aoide_storage::outbox::write_entry("dave", &parked).unwrap();

        let again = mail_deposit(&params, &ctx).unwrap();
        assert_eq!(again["status"], "refused", "{again}");
        assert_eq!(again["reason"], "no-route", "{again}");
        assert!(
            again["detail"].as_str().unwrap().contains("retry --refused"),
            "and it says where the hand is: {again}"
        );

        let log = std::fs::read_to_string(&audit_log).unwrap_or_default();
        assert!(log.contains("parked: no-route"), "the answer is audited: {log}");

        mail_deposit_cleanup(&root, saved_state, saved_stage);
        match saved_root {
            Some(v) => std::env::set_var("AOIDE_ROOT", v),
            None => std::env::remove_var("AOIDE_ROOT"),
        }
    }

    /// The fixture the door's transit tests need: a mesh whose only member that
    /// matters is THIS box under a name the chain can spell (`far`, holding this
    /// process's own identity key, so a container this test seals is a chain a hop
    /// may carry), plus a destination (`dave`) and a relay (`relay`).
    ///
    /// Nothing here is a charter: a pair mesh's records ARE its declaration, which
    /// keeps these tests about the door's arms and not about signing files.
    fn hop_fixture() -> String {
        let (kp, _) = aoide_storage::identity::load_or_mint().unwrap();
        let key = |tag: &str| format!("{tag}{tag}{tag}{tag}").repeat(4);
        let node = |name: &str, url: &str, key: String| aoide_storage::node_store::Node {
            name: name.to_string(),
            url: url.to_string(),
            token_file: None,
            bearer_secret: None,
            hub: false,
            pubkey: Some(key),
            verified: true,
            grants: aoide_storage::node_store::grants_in("home", &["message"]),
            narrowed: aoide_storage::node_store::Grants::new(),
            via: None,
            added_at: "2026-09-07T00:00:00Z".to_string(),
        };
        std::fs::write(
            aoide_storage::config::source().path,
            "[pairing]\nhomeMesh = \"home\"\n\n\
             [mesh.home]\n[mesh.home.nodes]\nfar = \"ssh://far\"\ndave = \"ssh://dave\"\nrelay = \"ssh://relay\"\n",
        )
        .unwrap();
        aoide_storage::node_store::save_nodes(&[
            node("far", "ssh://far", kp.info().pubkey_hex),
            node("dave", "ssh://dave", key("d4")),
            node("relay", "ssh://relay", key("e5")),
        ])
        .unwrap();
        kp.info().pubkey_hex
    }

    /// One container this test seals as `far`, addressed to `dave` and handed to
    /// `far` (this box) — the shape a relay deposits after it carried the letter.
    fn hop_container(far_key: &str) -> aoide_storage::seal::Container {
        let binding = aoide_storage::seal::publish_binding().unwrap();
        let envelope = aoide_storage::mail::mint_outbound_letter_from(
            "far",
            "alice",
            "dave",
            "bob",
            "a letter in transit",
            "home",
        )
        .unwrap();
        let container = aoide_storage::seal::seal_envelope(
            &envelope,
            &binding,
            "home",
            "home",
            "dave",
            "far",
            &aoide_storage::time::now_iso_utc(),
        )
        .unwrap();
        assert_eq!(container.origin.key, far_key);
        container
    }

    /// Append one hop entry to a container, as a hop would — the chain a test
    /// hands the door when it wants to be the hop BEFORE it.
    fn append_hop(
        container: &aoide_storage::seal::Container,
        node: &str,
        next: &str,
        mesh: &str,
        kp: &aoide_storage::identity::Keypair,
    ) -> aoide_storage::seal::Container {
        let (msgid, prev) = aoide_storage::seal::chain_tail(container).unwrap();
        let at = aoide_storage::time::now_iso_utc();
        let mut forwarded = container.clone();
        forwarded.transit.push(aoide_storage::seal::TransitEntry {
            node: node.to_string(),
            next: next.to_string(),
            at: at.clone(),
            mesh: mesh.to_string(),
            sig: aoide_storage::wire_auth::sign_hex(
                kp,
                &aoide_storage::seal::hop_bytes(&msgid, &prev, node, next, &at, mesh),
            ),
        });
        forwarded
    }

    /// **A hop is carried, never acked.** A sealed container addressed to another
    /// node is filed as a `transit` entry, spooled toward the `next` the four
    /// steps pick, and answered `accepted` with the hop named — no letter filed,
    /// no reader rung, and NO receipt minted back to the depositing hop (a hub
    /// that acked would tell the origin its letter had landed). The hub's own
    /// records hold the routing metadata and the digest, never the letter.
    #[test]
    fn a_container_addressed_elsewhere_is_hopped_and_never_acked() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_root = std::env::var("AOIDE_ROOT").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("hop-never-acks");
        act_as(&root, "here");
        // The fixture writes this box's config.toml, which lives under the root.
        std::env::set_var("AOIDE_ROOT", &root);
        std::env::set_var("STRILLIX_ROOT", &root);
        let far_key = hop_fixture();
        let container = hop_container(&far_key);

        let audit_log = root.join("log");
        let ctx = mail_deposit_ctx(&audit_log, Some("far"));
        let answer = mail_deposit(&json!({ "container": container }), &ctx).unwrap();
        assert_eq!(answer["status"], "accepted", "{answer}");
        assert_eq!(answer["transit"]["next"], "dave", "{answer}");
        assert_eq!(answer["transit"]["held"], false);

        assert!(
            aoide_storage::mail::read_base().unwrap().is_empty(),
            "nothing is filed as correspondence at a hub"
        );
        assert!(
            aoide_storage::outbox::list_entries("far").unwrap().is_empty(),
            "and no receipt is minted back to the depositing hop"
        );
        assert_eq!(aoide_storage::outbox::list_entries("dave").unwrap().len(), 1, "spooled toward `dave`");
        let log = std::fs::read_to_string(&audit_log).unwrap_or_default();
        assert!(log.contains("sealed transit") && log.contains("dave"), "audited after the write: {log}");

        mail_deposit_cleanup(&root, saved_state, saved_stage);
        match saved_root {
            Some(v) => std::env::set_var("AOIDE_ROOT", v),
            None => std::env::remove_var("AOIDE_ROOT"),
        }
    }

    /// **A retry over another route is a duplicate, not a second hop.** The same
    /// immutable container deposited again at the same hub is answered `duplicate`
    /// (the dedup gate's cheap path, before anything is opened or filed) and the
    /// hub does not carry it twice.
    #[test]
    fn the_same_container_offered_again_is_a_duplicate_not_a_second_hop() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_root = std::env::var("AOIDE_ROOT").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("hop-duplicate");
        act_as(&root, "here");
        // The fixture writes this box's config.toml, which lives under the root.
        std::env::set_var("AOIDE_ROOT", &root);
        std::env::set_var("STRILLIX_ROOT", &root);
        let far_key = hop_fixture();
        let container = hop_container(&far_key);

        let audit_log = root.join("log");
        let ctx = mail_deposit_ctx(&audit_log, Some("far"));
        let first = mail_deposit(&json!({ "container": container.clone() }), &ctx).unwrap();
        assert_eq!(first["status"], "accepted", "{first}");
        let again = mail_deposit(&json!({ "container": container }), &ctx).unwrap();
        assert_eq!(again["status"], "duplicate", "{again}");
        assert_eq!(aoide_storage::outbox::list_entries("dave").unwrap().len(), 1, "one spooled copy");

        mail_deposit_cleanup(&root, saved_state, saved_stage);
        match saved_root {
            Some(v) => std::env::set_var("AOIDE_ROOT", v),
            None => std::env::remove_var("AOIDE_ROOT"),
        }
    }

    /// **A chain that already names this box is a loop, dropped once.** Two hops
    /// through the same machine is one machine too many: the door refuses `loop`,
    /// audited, and nothing is filed or spooled onward — the letter dies where it
    /// came back to.
    #[test]
    fn a_chain_that_already_names_this_box_is_a_loop() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_root = std::env::var("AOIDE_ROOT").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("hop-loop");
        act_as(&root, "here");
        // The fixture writes this box's config.toml, which lives under the root.
        std::env::set_var("AOIDE_ROOT", &root);
        std::env::set_var("STRILLIX_ROOT", &root);
        let far_key = hop_fixture();
        let (kp, _) = aoide_storage::identity::load_or_mint().unwrap();
        let container = hop_container(&far_key);
        // The chain already names `far` once (entry 1) and again as the hop that
        // handed it over.
        let looped = append_hop(&container, "far", "far", "home", &kp);

        let audit_log = root.join("log");
        let ctx = mail_deposit_ctx(&audit_log, Some("far"));
        let answer = mail_deposit(&json!({ "container": looped }), &ctx).unwrap();
        assert_eq!(answer["status"], "refused", "{answer}");
        assert_eq!(answer["reason"], aoide_storage::seal::CHAIN_LOOP, "{answer}");
        assert!(aoide_storage::outbox::list_entries("dave").unwrap().is_empty(), "nothing spooled onward");
        let log = std::fs::read_to_string(&audit_log).unwrap_or_default();
        assert!(log.contains(aoide_storage::seal::CHAIN_LOOP), "the drop is audited: {log}");

        mail_deposit_cleanup(&root, saved_state, saved_stage);
        match saved_root {
            Some(v) => std::env::set_var("AOIDE_ROOT", v),
            None => std::env::remove_var("AOIDE_ROOT"),
        }
    }

    /// **A tampered hop is refused at the hop and audited.** An entry's `mesh` is
    /// inside its signature, so flipping it breaks the entry — and the hop's own
    /// zone check reads that zone FIRST, so what the depositing hop is told is the
    /// wall (`zone-violation`) rather than a signature failure. Either way the
    /// carrier refuses before anything is filed or spooled onward, and the refusal
    /// is audited.
    #[test]
    fn a_tampered_hop_zone_is_refused_at_the_hop() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_root = std::env::var("AOIDE_ROOT").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("hop-tampered");
        act_as(&root, "here");
        // The fixture writes this box's config.toml, which lives under the root.
        std::env::set_var("AOIDE_ROOT", &root);
        std::env::set_var("STRILLIX_ROOT", &root);
        let far_key = hop_fixture();
        let mut container = hop_container(&far_key);
        // One unsigned byte: entry 1's own zone, which the origin signed.
        container.transit[0].mesh = "somewhere-else".to_string();

        let audit_log = root.join("log");
        let ctx = mail_deposit_ctx(&audit_log, Some("far"));
        let answer = mail_deposit(&json!({ "container": container }), &ctx).unwrap();
        assert_eq!(answer["status"], "refused", "{answer}");
        assert_eq!(answer["reason"], aoide_storage::seal::ZONE_VIOLATION, "{answer}");
        assert!(aoide_storage::outbox::list_entries("dave").unwrap().is_empty());
        let log = std::fs::read_to_string(&audit_log).unwrap_or_default();
        assert!(log.contains(aoide_storage::seal::ZONE_VIOLATION), "audited: {log}");

        mail_deposit_cleanup(&root, saved_state, saved_stage);
        match saved_root {
            Some(v) => std::env::set_var("AOIDE_ROOT", v),
            None => std::env::remove_var("AOIDE_ROOT"),
        }
    }

    /// Spec, P-M3: **a poller receives only its own entries.** box-b asks for
    /// box-b and gets exactly what was spooled toward box-b — box-c's held
    /// letter is not in the answer, and is still sitting in box-c's spool
    /// afterwards.
    #[test]
    fn a_poller_receives_only_its_own_entries() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("poll-own-entries");
        act_as(&root, "here");

        let (for_b, for_c) = two_pollers_fixture();
        let audit_log = root.join("log");
        let ctx = mail_deposit_ctx(&audit_log, Some("box-b"));

        let answer = mail_poll(&json!({ "node": "box-b" }), &ctx).unwrap();
        let handed: Vec<String> = answer["envelopes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["msgid"].as_str().unwrap_or("").to_string())
            .collect();
        assert_eq!(handed, vec![for_b.clone()], "exactly this node's own entry: {answer}");
        assert!(!handed.contains(&for_c), "another node's held letter is never handed over: {answer}");
        assert_eq!(aoide_storage::outbox::list_entries("box-c").unwrap().len(), 1, "and it is untouched in its own spool");

        mail_deposit_cleanup(&root, saved_state, saved_stage);
    }

    /// Spec, P-M3: **a non-`message` poller is refused.** A paired,
    /// validly-signed caller whose `allows` lacks `message` is refused as a
    /// JSON-RPC ERROR (admission, MAIL.md §Wire) — never a 200 result with an
    /// empty envelope list, which would read as "nothing waiting for you."
    ///
    /// This is the reachable half of the spec's "non-`message` or `down`"
    /// pair. The `down` half is the door's own clause — a node the mesh
    /// declares `down` is refused a RESULT carrying that word, from the same
    /// per-request declaration set ([`down_caller_refusal`]), beside this
    /// predicate and before anything is handed over — while `aoide node allow
    /// <node> message off` remains the per-request quarantine that lands on
    /// this same `message` half.
    #[test]
    fn a_non_message_poller_is_refused() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("poll-noallow");
        act_as(&root, "here");

        setup_signed_node_with_allows("box-b", &["read"]);
        let held = aoide_storage::mail::mint_outbound_letter("alice", "box-b", "bob", "waiting").unwrap();
        aoide_storage::outbox::write_entry("box-b", &aoide_storage::outbox::OutboxEntry::held(held)).unwrap();

        let audit_log = root.join("log");
        let ctx = mail_deposit_ctx(&audit_log, Some("box-b"));
        let (code, msg) = mail_poll(&json!({ "node": "box-b" }), &ctx).expect_err("message not in allows — must refuse");
        assert_eq!(code, -32010);
        assert!(msg.contains("node allow box-b message on"), "names the exact fix: {msg}");

        let log = std::fs::read_to_string(&audit_log).unwrap();
        assert!(log.contains("a2a.aoide/mailPoll"), "{log}");
        assert!(!log.contains("a2a.rpc"), "never falls back to the generic label: {log}");

        mail_deposit_cleanup(&root, saved_state, saved_stage);
    }

    /// A signed caller asking for a mailbox it is not: refused, and told
    /// which name it is actually signed as. Nothing is handed over — the
    /// claim buys no reach into another node's spool.
    #[test]
    fn a_poller_claiming_another_nodes_mailbox_is_refused_and_hands_nothing_over() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("poll-other-node");
        act_as(&root, "here");

        let (for_b, _for_c) = two_pollers_fixture();
        let audit_log = root.join("log");
        let ctx = mail_deposit_ctx(&audit_log, Some("box-b"));

        let (code, msg) = mail_poll(&json!({ "node": "box-c" }), &ctx).expect_err("no polling on another's behalf");
        assert_eq!(code, -32010);
        assert!(msg.contains("box-c") && msg.contains("box-b"), "names both the claim and the signer: {msg}");

        assert!(
            aoide_storage::outbox::list_entries("box-b").unwrap().iter().any(|e| e.envelope.msgid == for_b),
            "box-b's own entry is untouched — the refusal handed nothing over"
        );

        mail_deposit_cleanup(&root, saved_state, saved_stage);
    }

    #[test]
    fn an_unsigned_poller_and_a_missing_node_param_are_both_refused() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("poll-unsigned");
        act_as(&root, "here");

        setup_signed_node_with_allows("box-b", &["message"]);
        let audit_log = root.join("log");

        let ctx = mail_deposit_ctx(&audit_log, None);
        let err = mail_poll(&json!({ "node": "box-b" }), &ctx).expect_err("no signature headers — must refuse");
        assert_eq!(err.0, -32010);

        // A missing `node` is a SHAPE error, refused before any lookup
        // (`pair_poll`'s own precedence: malformed params reveal nothing).
        let signed = mail_deposit_ctx(&audit_log, Some("box-b"));
        let shape = mail_poll(&json!({}), &signed).expect_err("node is required");
        assert_eq!(shape.0, -32602);

        mail_deposit_cleanup(&root, saved_state, saved_stage);
    }

    /// **Authorization precedes parsing** on both mail methods: an unsigned
    /// caller, and a signed one with no `message` grant, get the one refusal
    /// whatever their params say — never `invalid params: container: missing
    /// field v`, which would tell a prober what the door reads.
    #[test]
    fn an_unauthorized_caller_is_refused_whatever_its_params_say() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("auth-before-parse");
        act_as(&root, "here");
        setup_signed_node_with_allows("box-b", &[]);
        let audit_log = root.join("log");

        for who in [None, Some("box-b")] {
            let ctx = mail_deposit_ctx(&audit_log, who);
            for params in [json!({}), json!({ "container": {} }), json!({ "envelope": {} }), json!({ "container": { "purpose": "letter" } })] {
                let err = mail_deposit(&params, &ctx).expect_err("unauthorized");
                assert_eq!(err.0, -32010, "{who:?} {params}: {}", err.1);
                assert!(err.1.starts_with("mail deposit refused"), "{who:?} {params}: {}", err.1);
            }
            for params in [json!({}), json!({ "node": "" }), json!({ "node": "box-b" })] {
                let err = mail_poll(&params, &ctx).expect_err("unauthorized");
                assert_eq!(err.0, -32010, "{who:?} {params}: {}", err.1);
                assert!(err.1.starts_with("mail poll refused"), "{who:?} {params}: {}", err.1);
            }
        }

        mail_deposit_cleanup(&root, saved_state, saved_stage);
    }

    /// A signed caller with no `message` grant gets the refusal for a charter
    /// container addressed ELSEWHERE before anything about it is verified,
    /// deduped or routed (no oracle on this box's declarations or seen set) —
    /// while one addressed here, by name or by this box's own age key, is still
    /// admitted on the signature alone and reaches the charter's own checks.
    #[test]
    fn a_grantless_caller_cannot_probe_a_charter_container_for_another_node() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("charter-probe");
        act_as(&root, "here");
        setup_signed_node_with_allows("box-b", &[]);
        let audit_log = root.join("log");
        let ctx = mail_deposit_ctx(&audit_log, Some("box-b"));

        let binding = aoide_storage::seal::publish_binding().unwrap();
        let mut container = aoide_storage::seal::seal_charter(
            b"not a charter", b"not a sig", &binding, "home", 1, "elsewhere", "elsewhere",
            &aoide_storage::time::now_iso_utc(),
        )
        .unwrap();

        // Another node's key and another node's name: refused before any check.
        container.to.age = "age1someoneelse".to_string();
        let err = mail_deposit(&json!({ "container": container }), &ctx).expect_err("grantless and not for here");
        assert_eq!(err.0, -32010, "{}", err.1);
        assert!(err.1.starts_with("mail deposit refused"), "{}", err.1);

        // Addressed here by this box's own age key, whatever name the charter
        // line spells: admitted to the charter's own verification, which says
        // what is wrong with the (deliberately bogus) payload as a result.
        container.to.age = binding.age_pubkey.clone();
        let reply = mail_deposit(&json!({ "container": container }), &ctx).expect("an answer, not a -32010");
        assert_eq!(reply["status"], json!("refused"), "{reply}");

        mail_deposit_cleanup(&root, saved_state, saved_stage);
    }

    /// Spec, P-M3: **a hold entry drains only via poll** — the door half.
    /// The held letter is handed over by the node's own ask, and the entry
    /// stays exactly where it was: a poll is a READ, and an entry leaves the
    /// spool only when its ack retires it (or `mail outbox rm`).
    #[test]
    fn a_hold_entry_is_handed_over_by_the_poll_and_stays_spooled_until_acked() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("poll-hold");
        act_as(&root, "here");

        setup_signed_node_with_allows("box-b", &["message"]);
        let held = aoide_storage::mail::mint_outbound_letter("alice", "box-b", "bob", "waiting to be asked for").unwrap();
        let msgid = held.msgid.clone();
        aoide_storage::outbox::write_entry("box-b", &aoide_storage::outbox::OutboxEntry::held(held)).unwrap();

        let audit_log = root.join("log");
        let ctx = mail_deposit_ctx(&audit_log, Some("box-b"));
        let answer = mail_poll(&json!({ "node": "box-b" }), &ctx).unwrap();
        // P-SEAL: the answer carries both lists. This entry is unsealed (it
        // was spooled with no container), so it rides `envelopes` and
        // `containers` is empty — a sealed entry would be the other way
        // round, with no plaintext handed over at all.
        assert_eq!(answer["envelopes"][0]["msgid"], Value::String(msgid.clone()), "{answer}");
        assert_eq!(answer["containers"].as_array().unwrap().len(), 0, "{answer}");
        assert_eq!(
            answer.as_object().unwrap().len(),
            2,
            "the result is exactly `containers` and `envelopes`, per the wire shape: {answer}"
        );

        let rows = aoide_storage::outbox::list_entries("box-b").unwrap();
        assert_eq!(rows.len(), 1, "handing over retires nothing");
        assert_eq!(rows[0].tries, 0, "and records no attempt");

        mail_deposit_cleanup(&root, saved_state, saved_stage);
    }

    /// Spec, P-M3: **re-poll before ack is idempotent.** The same held entry
    /// comes back on the next ask, byte-identically — hand-over writes no
    /// bookmark, so the predicate that offered it the first time still holds.
    /// (What the POLLER does with the second copy — dedup, one ack — is
    /// `aoide-client`'s `a_repoll_before_the_ack_files_nothing_twice`.)
    #[test]
    fn a_repoll_before_the_ack_hands_the_same_envelope_over_again() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("poll-repoll");
        act_as(&root, "here");

        setup_signed_node_with_allows("box-b", &["message"]);
        let held = aoide_storage::mail::mint_outbound_letter("alice", "box-b", "bob", "twice").unwrap();
        let msgid = held.msgid.clone();
        aoide_storage::outbox::write_entry("box-b", &aoide_storage::outbox::OutboxEntry::held(held)).unwrap();

        let audit_log = root.join("log");
        let ctx = mail_deposit_ctx(&audit_log, Some("box-b"));
        let first = mail_poll(&json!({ "node": "box-b" }), &ctx).unwrap();
        let second = mail_poll(&json!({ "node": "box-b" }), &ctx).unwrap();
        assert_eq!(first, second, "the same ask, the same answer, byte for byte");
        assert_eq!(second["envelopes"][0]["msgid"], Value::String(msgid));

        assert_eq!(aoide_storage::outbox::list_entries("box-b").unwrap().len(), 1, "still exactly one entry, still unretired");

        mail_deposit_cleanup(&root, saved_state, saved_stage);
    }

    /// A `now` entry whose own attempts have been failing is a poll's
    /// business too (MAIL.md §Wire): the peer asking for its own mail is the
    /// way a stuck spool clears when the drain's own dials cannot.
    #[test]
    fn a_poll_offers_a_failing_now_entry_beside_the_held_ones() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("poll-failing-now");
        act_as(&root, "here");

        setup_signed_node_with_allows("box-b", &["message"]);
        let mut stuck = aoide_storage::outbox::OutboxEntry::fresh(
            aoide_storage::mail::mint_outbound_letter("alice", "box-b", "bob", "stuck").unwrap(),
        );
        stuck.tries = 4;
        stuck.last_outcome = "transport: connection refused".to_string();
        let stuck_msgid = stuck.envelope.msgid.clone();
        aoide_storage::outbox::write_entry("box-b", &stuck).unwrap();
        let untried = aoide_storage::mail::mint_outbound_letter("alice", "box-b", "bob", "untried").unwrap();
        aoide_storage::outbox::write_entry("box-b", &aoide_storage::outbox::OutboxEntry::fresh(untried)).unwrap();

        let audit_log = root.join("log");
        let ctx = mail_deposit_ctx(&audit_log, Some("box-b"));
        let answer = mail_poll(&json!({ "node": "box-b" }), &ctx).unwrap();
        let handed: Vec<String> = answer["envelopes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["msgid"].as_str().unwrap_or("").to_string())
            .collect();
        assert_eq!(handed, vec![stuck_msgid], "the failing entry, never the never-attempted one: {answer}");

        mail_deposit_cleanup(&root, saved_state, saved_stage);
    }

    /// The poll's own audit line, unconditionally, under its own label, with
    /// the handed-over count — the pull direction's flood signal (a relay
    /// answering one node with hundreds of letters is visible HERE, at the
    /// node that spooled them).
    #[test]
    fn every_poll_self_audits_under_its_own_label_with_the_count() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("poll-audit");
        act_as(&root, "here");

        let (_for_b, _for_c) = two_pollers_fixture();
        let audit_log = root.join("log");
        let ctx = mail_deposit_ctx(&audit_log, Some("box-b"));
        mail_poll(&json!({ "node": "box-b" }), &ctx).unwrap();

        let log = std::fs::read_to_string(&audit_log).unwrap();
        assert!(log.contains("a2a.aoide/mailPoll"), "{log}");
        assert!(
            log.contains("0 container(s) and 1 plaintext envelope(s) handed over"),
            "the counts ride the line: {log}"
        );
        assert!(!log.contains("a2a.rpc"), "never falls back to the generic label: {log}");

        mail_deposit_cleanup(&root, saved_state, saved_stage);
    }

    // ── P-RSA S6: the watch frame on `tasks/get` (CONTRACTS.md §6) ───────────

    /// A stage holding ONE session with a real transcript, under temp
    /// `AOIDE_STAGE_DIR`/`AOIDE_STATE_DIR` roots — BOTH, because the frame
    /// path reads `sessions.json` off the stage and the mailbase and the
    /// report cursor off the state root, and no test here may read the
    /// operator's own tree. The returned guard restores both on drop, so a
    /// temp root never leaks into the next test in this binary (the same
    /// save/restore the file's older tests spell out inline).
    struct Roots {
        stage: Option<String>,
        state: Option<String>,
    }

    impl Drop for Roots {
        fn drop(&mut self) {
            match &self.stage {
                Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
                None => std::env::remove_var("AOIDE_STAGE_DIR"),
            }
            match &self.state {
                Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
                None => std::env::remove_var("AOIDE_STATE_DIR"),
            }
        }
    }

    fn frame_stage(tag: &str, id: &str, state: &str) -> Roots {
        let roots = Roots {
            stage: std::env::var("AOIDE_STAGE_DIR").ok(),
            state: std::env::var("AOIDE_STATE_DIR").ok(),
        };
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-server-a2a-frame-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::create_dir_all(root.join("state/mail")).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        let log = stage.join(format!("{id}.log"));
        std::fs::write(&log, "line one\nline two\n").unwrap();
        let mut rec = fixture_session(id, state, None);
        rec.log_path = Some(log.to_string_lossy().into_owned());
        write_stage(
            &sessions_path(),
            &SessionsFile { schema_version: "0".to_string(), sessions: vec![rec] },
        )
        .unwrap();
        roots
    }

    /// A `tasks/get` body asking for the frame, `tail` included when given.
    fn frame_request(id: &str, tail: Option<u64>) -> Value {
        let mut params = json!({ "id": id });
        // The wire key is spelled out here, never taken from the const: a
        // renamed const must fail this test, not silently move the wire.
        params["metadata"] = match tail {
            Some(t) => json!({ "aoide/frame": { "tail": t } }),
            None => json!({ "aoide/frame": {} }),
        };
        json!({ "jsonrpc": "2.0", "id": 1, "method": "tasks/get", "params": params })
    }

    /// The `(resolved name, verified key)` `handle_connection` threads into
    /// [`SignedCaller`] after [`verify_signed_request`] verified a real
    /// signature — the same round trip, so the gate sees exactly what
    /// production sees.
    fn verified_caller(kp: &aoide_storage::identity::Keypair, body: &[u8]) -> (String, String) {
        let now = 1_800_000_000_i64;
        let req = signed_request(kp, "yomi-strix", "/", body, now, &unique_nonce("frame"));
        match verify_signed_request(&req, now, 0) {
            SignedRequestOutcome::Verified { resolved, key, .. } => (resolved, key),
            other => panic!("expected a real signature to verify, got {other:?}"),
        }
    }

    /// The whole output-read table at the predicate (P-RSA S6, with the
    /// 2026-09-25 ruling folded in): unsigned and the bearer/address rungs are
    /// refused, a signature-rung node is admitted on `verified` + `read` and
    /// on nothing else — the target's own `remoteParent` never enters into it.
    #[test]
    fn output_read_admitted_is_a_signed_caller_holding_read_in_the_mesh_it_names() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp("a2a-read-gate-mesh");
        std::env::set_var("AOIDE_STATE_DIR", &root);

        let mut reader = fixture_node("box-b", "http://10.0.0.5:8710/");
        reader.verified = true;
        reader.pubkey = Some("aa11".to_string());
        reader.grants = aoide_storage::node_store::grants_in("home", &["read"]);
        aoide_storage::node_store::save_nodes(&[reader.clone()]).unwrap();

        // The caller signature verification would produce for this record:
        // its own stored key, and the mesh its request names.
        let signed = |mesh: Option<&'static str>| Some(SignedCaller { name: "box-b", key: "aa11", mesh });

        assert!(
            output_read_admitted(true, signed(None)),
            "signed, granted `read` in the home mesh (and the request names none): admitted"
        );
        assert!(output_read_admitted(true, signed(Some("home"))), "naming the home mesh explicitly is the same grant");
        assert!(!output_read_admitted(true, None), "unsigned with no door token (read_ok true): refused");
        assert!(!output_read_admitted(false, signed(None)), "the bearer gate still runs first");
        assert!(
            !output_read_admitted(true, signed(Some("away"))),
            "the SAME caller acting in another mesh holds nothing there: a grant is given in one mesh and holds only there"
        );

        let mut no_read = reader.clone();
        no_read.grants = aoide_storage::node_store::grants_in("home", &["spawn"]);
        aoide_storage::node_store::save_nodes(&[no_read.clone()]).unwrap();
        assert!(!output_read_admitted(true, signed(None)), "signed but `read` was never granted");

        let mut unverified = reader.clone();
        unverified.verified = false;
        aoide_storage::node_store::save_nodes(&[unverified]).unwrap();
        assert!(!output_read_admitted(true, signed(None)), "a grant map without a pairing grants nothing");

        match saved {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    /// **The mesh decides which grant answers** — the whole point of
    /// per-mesh trust: the same caller, the same signature, a different mesh
    /// named, and a grant it does not hold there is not a grant. Two meshes
    /// sharing a third node is the case the design calls out by name.
    ///
    /// Review finding 6 lives at the end: twin records with ONE key answer by
    /// UNION, per CONTRACTS.md §6.
    #[test]
    fn a_grant_in_one_mesh_never_answers_for_another_mesh() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp("a2a-mesh-scope");
        std::env::set_var("AOIDE_STATE_DIR", &root);

        let mut node = fixture_node("box-b", "http://10.0.0.5:8710/");
        node.verified = true;
        node.pubkey = Some("aa11".to_string());
        node.grants = std::collections::BTreeMap::from([
            ("away".to_string(), vec!["read".to_string()]),
            ("home".to_string(), vec!["spawn".to_string()]),
        ]);
        aoide_storage::node_store::save_nodes(&[node.clone()]).unwrap();

        // The pure core, so the table is provable without the disk at all.
        assert!(paired_grant(std::slice::from_ref(&node), "away", "aa11").holds("read"));
        assert!(
            !paired_grant(std::slice::from_ref(&node), "home", "aa11").holds("read"),
            "`read` was granted in `away` and nowhere else"
        );
        assert!(paired_grant(std::slice::from_ref(&node), "home", "aa11").holds("spawn"));
        assert!(!paired_grant(std::slice::from_ref(&node), "away", "aa11").holds("spawn"));

        // And through the real lookup the door uses.
        assert!(grant_in_mesh(Some("away"), "aa11").holds("read"));
        assert!(!grant_in_mesh(Some("home"), "aa11").holds("read"));
        assert!(grant_in_mesh(Some("home"), "aa11").holds("spawn"));
        assert!(
            grant_in_mesh(None, "aa11").holds("spawn"),
            "an unnamed request reads the HOME mesh's grant, and only that one"
        );
        assert!(grant_in_mesh(Some("never-declared"), "aa11") == Grant::none(), "a mesh the record is not in grants nothing");
        assert!(grant_in_mesh(Some("home"), "ff99") == Grant::none(), "a key no record holds grants nothing");

        // Review finding 6 / CONTRACTS.md §6: twin records sharing ONE key
        // answer by UNION, so a capability either twin grants is held, and a
        // revocation is only real once it lands on BOTH.
        let mut twin_a = fixture_node("twin-a", "http://a/");
        twin_a.verified = true;
        twin_a.pubkey = Some("aa11".to_string());
        twin_a.grants = aoide_storage::node_store::grants_in("home", &["spawn"]);
        let mut twin_b = fixture_node("twin-b", "http://b/");
        twin_b.verified = true;
        twin_b.pubkey = Some("aa11".to_string());
        twin_b.grants = std::collections::BTreeMap::from([("home".to_string(), vec!["read".to_string()])]);
        let twins = vec![twin_a.clone(), twin_b.clone()];

        let union = paired_grant(&twins, "home", "aa11");
        assert!(union.holds("spawn"), "the first twin's capability survives: {union:?}");
        assert!(union.holds("read"), "and so does the second's — the union, never the first match");
        assert!(!union.holds("message"), "a capability NEITHER twin grants is not there");

        let mut revoked = twins.clone();
        revoked[0].grants = aoide_storage::node_store::grants_in("home", &["read"]);
        assert!(
            !paired_grant(&revoked, "home", "aa11").holds("spawn"),
            "revoking on EVERY twin is what actually revokes (the contract's own instruction)"
        );

        match saved {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    /// Review finding 9: the deleted helper's guard is the COMPILER's own
    /// `never used` warning — the tree builds warning-free, and a helper
    /// nothing calls would show up there. No source-scanning test sits here
    /// (a first cut spelled the name it searched for, which the search found
    /// in itself); the live answer to "which record is this caller" is
    /// `grant_in_mesh`, exercised by the gate tests above.

    /// **A config that will not load grants nothing** (review F6, door-wide).
    /// An unnamed request's mesh is the CONFIG's to name, so an unreadable
    /// config leaves nothing to judge it by — and the pre-image fallback (the
    /// built-in default, whose PAIRED RECORDS then answer) is the same
    /// stale-`grants[home]` door N1/F2 closed for a charter home, reachable by
    /// breaking one file. A NAMED mesh is untouched: the name is its own
    /// answer, and no config line decides a pair mesh's rules.
    #[test]
    fn an_unreadable_config_grants_nothing_to_an_unnamed_request() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_root = std::env::var("AOIDE_ROOT").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_config = std::env::var("AOIDE_CONFIG").ok();
        let root = aoide_test_support::short_tmp("a2a-cfg-failclosed");
        std::env::set_var("AOIDE_ROOT", &root);
        std::env::set_var("STRILLIX_ROOT", &root);
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::remove_var("AOIDE_CONFIG");

        let config = root.join("config.toml");
        std::fs::write(&config, "[pairing]\nhomeMesh = \"home\"\n").unwrap();

        let mut node = fixture_node("box-b", "http://10.0.0.5:8710/");
        node.verified = true;
        node.pubkey = Some("aa11".to_string());
        node.grants = std::collections::BTreeMap::from([
            ("home".to_string(), vec!["message".to_string()]),
            ("away".to_string(), vec!["read".to_string()]),
        ]);
        aoide_storage::node_store::save_nodes(&[node]).unwrap();

        // 1. The config reads: N1's behaviour, unchanged — the unnamed request
        //    resolves to `home`, and home's PAIRED RECORDS are what answer.
        assert_eq!(effective_mesh(None).ok().as_deref(), Some("home"), "`[pairing] homeMesh`, read");
        assert!(
            grant_in_mesh(None, "aa11").holds("message"),
            "while the config reads, home is a pair mesh and its record is the grant"
        );
        assert!(grant_in_mesh(Some("away"), "aa11").holds("read"));

        // 2. The config stops loading. Home has no answer, and the built-in
        //    default is a guess, not an answer.
        std::fs::write(&config, "this is not a config at all ][\n").unwrap();
        assert!(aoide_storage::config::load().is_err(), "the premise: the config no longer loads");
        assert!(effective_mesh(None).is_err(), "`[pairing] homeMesh` cannot be read, so home cannot be resolved");
        assert!(
            grant_in_mesh(None, "aa11") == Grant::none(),
            "an unnamed request is granted NOTHING while the config will not load — never a guessed mesh's paired records"
        );

        // 3. A request that NAMED its mesh is unaffected: the name is the
        //    answer, and the config plays no part in a pair mesh's rules.
        assert_eq!(effective_mesh(Some("away")).ok().as_deref(), Some("away"), "a named mesh never reads the config");
        assert_eq!(effective_mesh(Some("  AWAY ")).ok().as_deref(), Some("away"), "trimmed and folded, as before");
        assert!(
            grant_in_mesh(Some("away"), "aa11").holds("read"),
            "and a named mesh's own rules still answer: the fence is exactly the unnamed case"
        );

        match saved_root {
            Some(v) => std::env::set_var("AOIDE_ROOT", v),
            None => std::env::remove_var("AOIDE_ROOT"),
        }
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
        match saved_config {
            Some(v) => std::env::set_var("AOIDE_CONFIG", v),
            None => std::env::remove_var("AOIDE_CONFIG"),
        }
    }

    /// **The refusal NAMES the unreadable config.** The arms that owe the
    /// operator a reason build it from the same `Err`, so the reason is the
    /// real one — not "you hold nothing in mesh `home`", which is the reason a
    /// GUESSED mesh would produce.
    #[test]
    fn an_unreadable_config_refuses_a_gated_arm_and_names_the_config() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_root = std::env::var("AOIDE_ROOT").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_config = std::env::var("AOIDE_CONFIG").ok();
        let root = aoide_test_support::short_tmp("a2a-cfg-refusal");
        std::env::set_var("AOIDE_ROOT", &root);
        std::env::set_var("STRILLIX_ROOT", &root);
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::remove_var("AOIDE_CONFIG");

        let config = root.join("config.toml");
        std::fs::write(&config, "[pairing]\nhomeMesh = \"home\"\n").unwrap();

        let mut node = fixture_node("box-b", "http://10.0.0.5:8710/");
        node.verified = true;
        node.pubkey = Some("aa11".to_string());
        node.grants = aoide_storage::node_store::grants_in("home", &["message"]);
        aoide_storage::node_store::save_nodes(&[node]).unwrap();

        let audit_log = root.join("log");
        let ctx = mail_deposit_ctx(&audit_log, Some("box-b"));
        assert!(
            node_binding(&json!({}), &ctx).is_ok(),
            "with a readable config this caller is admitted — the fixture is otherwise fine"
        );

        std::fs::write(&config, "this is not a config at all ][\n").unwrap();
        let (code, msg) = node_binding(&json!({}), &ctx).expect_err("an unreadable config grants nothing");
        assert_eq!(code, -32010, "the capability family's own code, like the undecidable-operator arm");
        assert!(msg.contains("config.toml"), "the reason names the FILE: {msg}");
        assert!(msg.contains("names no mesh"), "and the case it is about, so the operator knows the fix: {msg}");
        assert!(
            !msg.contains("does not include `message`"),
            "never the guessed mesh's reason — that would be a lie about why: {msg}"
        );
        let log = std::fs::read_to_string(&audit_log).unwrap_or_default();
        assert!(log.contains("a2a.aoide/binding"), "audited under the arm's own label: {log}");

        // The SAME request, naming its mesh, is judged by that mesh — the
        // fence is the unnamed case and nothing else.
        let mut named = mail_deposit_ctx(&audit_log, Some("box-b"));
        named.signed_caller = Some(SignedCaller { name: "box-b", key: "aa11", mesh: Some("home") });
        assert!(
            node_binding(&json!({}), &named).is_ok(),
            "a named mesh's own rules answer whatever the config says: the name IS the answer"
        );

        match saved_root {
            Some(v) => std::env::set_var("AOIDE_ROOT", v),
            None => std::env::remove_var("AOIDE_ROOT"),
        }
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
        match saved_config {
            Some(v) => std::env::set_var("AOIDE_CONFIG", v),
            None => std::env::remove_var("AOIDE_CONFIG"),
        }
    }

    /// **The charter as the second source behind the one lookup** — every
    /// P-CHARTER "Trust per mesh" bullet this slice owns, at the door:
    ///
    /// 1. a key listed on an ACCEPTED charter holds that line's grant in that
    ///    mesh (`message` only, the line's default), with no paired record;
    /// 2. an UNNAMED request resolves to `effective_mesh(None)` — the home
    ///    mesh — and is judged by THAT mesh's rules: the charter's line where
    ///    one governs, and NOTHING when the mesh is charter-shaped with an
    ///    undecidable operator key. The paired records answer only where no
    ///    charter is shaped for the mesh (review N1; the assertion below runs
    ///    with a dangerous `grants[home]` record installed for exactly this);
    /// 3. a paired record in a charter mesh is INERT: for a listed key it
    ///    neither widens nor narrows the line, and for a key the charter does
    ///    not list it grants nothing there;
    /// 4. local `node allow … off --mesh` NARROWS a charter grant and wins over
    ///    it, and nothing local widens one (`on` is refused at the write);
    /// 5. a REMOVED line (revocation) refuses on the first request after the
    ///    new version is received, whatever this box paired with that key;
    /// 6. a mesh whose config operator line and state record disagree leaves NO
    ///    charter in force (`operator-mismatch`), so the charter stops granting
    ///    anything at that mesh.
    #[test]
    fn the_charter_is_the_second_source_behind_the_one_lookup() {
        use aoide_storage::node_store::{AllowChange, AllowError};

        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_root = std::env::var("AOIDE_ROOT").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_config = std::env::var("AOIDE_CONFIG").ok();
        let saved_node = std::env::var("AOIDE_A2A_NODE_NAME").ok();
        let root = mail_deposit_root("charter-source");

        // The operator's box roots `home` and pastes the one other machine's
        // node line under `[nodes]` — no `grant` key, so the line's default.
        charter_machine(&root, "operator", "opbox");
        let init = aoide_storage::charter::init("home").unwrap();
        let op_line = aoide_storage::charter::node_line().unwrap();
        charter_machine(&root, "receiver", "receiverbox");
        let receiver_line = aoide_storage::charter::node_line().unwrap();
        // The keys the door will see are the ones the SIGNED charter carries.
        let listed = aoide_storage::charter::parse(&format!(
            "mesh = \"home\"\nversion = 1\nrelays = []\n\n[nodes]\n{op_line}\n{receiver_line}\n"
        ))
        .unwrap();
        let op_key = listed.nodes["opbox"].key.clone();
        let receiver_key = listed.nodes["receiverbox"].key.clone();

        charter_machine(&root, "operator", "opbox");
        let v1 = format!("mesh = \"home\"\nversion = 0\nrelays = []\n\n[nodes]\n{op_line}\n{receiver_line}\n");
        std::fs::write(aoide_storage::charter::source_path("home"), &v1).unwrap();
        aoide_storage::charter::sign("home", None).unwrap();
        let v1_bytes = std::fs::read(aoide_storage::charter::source_path("home")).unwrap();
        let v1_sig = std::fs::read(aoide_storage::charter::source_sig_path("home")).unwrap();

        // The receiving machine trusts ONLY the operator key, by its config
        // line, and accepts v1 by file — no pairing, no registry entry.
        charter_machine(&root, "receiver", "receiverbox");
        std::fs::write(
            root.join("receiver").join("config.toml"),
            format!("[mesh.home]\noperator = \"ed25519:{}\"\n", init.operator),
        )
        .unwrap();
        assert_eq!(aoide_storage::charter::accept(&v1_bytes, &v1_sig).unwrap().charter.version, 1);

        // 1. The line IS the grant, and only in its own mesh.
        assert!(aoide_storage::node_store::load_nodes().is_empty(), "nothing is paired on this box");
        let grant = grant_in_mesh(Some("home"), &receiver_key);
        assert!(grant.holds("message"), "a listed key holds its line's grant: {grant:?}");
        assert!(!grant.holds("read"), "and nothing else — the line's only capability");
        assert!(!grant.holds("spawn"));
        assert!(grant_in_mesh(Some("home"), &op_key).holds("message"), "the operator's own machine is a node like any other");
        assert!(
            grant_in_mesh(Some("away"), &receiver_key) == Grant::none(),
            "a mesh no charter governs and no record holds grants nothing"
        );

        // 2. **A request naming no mesh is judged by the HOME mesh's rules**
        // (review N1, the user's ruling): it resolves to `effective_mesh(None)`
        // = `home`, and EVERY read — the governing charter, the shaped test,
        // the paired fallback — is that mesh's. Asserted here with the
        // dangerous record NOT YET installed; the fixture that carries a stale
        // `grants[home]` for a key the charter does not list is asserted below,
        // where it can catch the loophole this test used to pass trivially.
        assert!(
            grant_in_mesh(None, &receiver_key).holds("message"),
            "unnamed resolves to home, and home's charter line answers — not the paired records"
        );
        assert_eq!(
            grant_in_mesh(None, &receiver_key),
            grant_in_mesh(Some("home"), &receiver_key),
            "and it is indistinguishable from naming `home` — one rule, not two"
        );

        // 3. A paired record in a charter mesh is INERT.
        let mut listed = fixture_node("receiverbox", "http://10.0.0.5:8710/");
        listed.verified = true;
        listed.pubkey = Some(receiver_key.clone());
        listed.grants = std::collections::BTreeMap::from([
            ("home".to_string(), vec!["read".to_string(), "spawn".to_string()]),
            ("away".to_string(), vec!["read".to_string()]),
        ]);
        let mut unlisted = fixture_node("stranger", "http://10.0.0.6:8710/");
        unlisted.verified = true;
        unlisted.pubkey = Some("bb22".to_string());
        unlisted.grants = std::collections::BTreeMap::from([
            (
                "home".to_string(),
                vec!["read".to_string(), "spawn".to_string(), "message".to_string()],
            ),
            ("away".to_string(), vec!["read".to_string()]),
        ]);
        aoide_storage::node_store::save_nodes(&[listed.clone(), unlisted.clone()]).unwrap();
        let grant = grant_in_mesh(Some("home"), &receiver_key);
        assert!(grant.holds("message") && !grant.holds("read") && !grant.holds("spawn"), "the record is inert: {grant:?}");
        assert!(
            grant_in_mesh(Some("home"), "bb22") == Grant::none(),
            "a key the charter does not list holds nothing in its mesh, paired record or not"
        );
        assert!(
            grant_in_mesh(Some("away"), "bb22").holds("read"),
            "while the SAME record's grant in a mesh no charter governs answers exactly as before"
        );
        assert!(
            grant_in_mesh(Some("away"), &receiver_key).holds("read"),
            "and the paired source still answers per mesh for a listed key, in the mesh the charter does not govern"
        );
        // **The unnamed path, with the dangerous record INSTALLED** (review
        // N1): `stranger` holds `[read, spawn, message]` in `home` — the
        // migrated shape of every machine that became a charter mesh after it
        // was paired — and `home` is charter-shaped. Naming `home` and naming
        // nothing must give the same answer, and that answer is the charter's
        // (nothing), never the stale record's.
        assert!(
            grant_in_mesh(Some("home"), "bb22") == Grant::none(),
            "the charter's rule refuses it"
        );
        assert!(
            grant_in_mesh(None, "bb22") == Grant::none(),
            "and OMITTING the mesh header is not a way around that — the loophole this test used to pass with an empty registry"
        );
        assert_eq!(
            grant_in_mesh(None, "bb22"),
            grant_in_mesh(Some("home"), "bb22"),
            "one rule for the unnamed request: home's"
        );

        // 4. Local narrowing wins over the charter; nothing local widens one.
        let mut nodes = aoide_storage::node_store::load_nodes();
        assert_eq!(
            aoide_storage::node_store::set_node_allow(&mut nodes, "receiverbox", "message", false, "home").unwrap(),
            AllowChange::Disabled
        );
        aoide_storage::node_store::save_nodes(&nodes).unwrap();
        assert!(
            grant_in_mesh(Some("home"), &receiver_key) == Grant::none(),
            "`off` narrows the charter's grant to nothing"
        );
        let mut nodes = aoide_storage::node_store::load_nodes();
        assert_eq!(
            aoide_storage::node_store::set_node_allow(&mut nodes, "receiverbox", "read", true, "home").unwrap_err(),
            AllowError::WidensCharter,
            "the charter's line grants no `read`, and no local flag may add one"
        );
        assert_eq!(
            aoide_storage::node_store::set_node_allow(&mut nodes, "receiverbox", "message", true, "home").unwrap(),
            AllowChange::Enabled,
            "while `on` clears the refusal this box's own `off` recorded — it never exceeds the charter"
        );
        aoide_storage::node_store::save_nodes(&nodes).unwrap();
        assert!(grant_in_mesh(Some("home"), &receiver_key).holds("message"), "the charter is the grant again");

        // 6a. An operator disagreement leaves the mesh CHARTER-SHAPED and its
        // operator key UNDECIDABLE — and a shaped mesh fails CLOSED (review
        // F2): every request in it is refused, including one whose key has a
        // paired record with a grant in this mesh. There is no fallback to the
        // pre-charter source while a charter is shaped for the mesh, because
        // that fallback is what let a revoked or unlisted key back in.
        std::fs::write(
            root.join("receiver").join("config.toml"),
            format!("[mesh.home]\noperator = \"ed25519:{receiver_key}\"\n"),
        )
        .unwrap();
        assert!(
            aoide_storage::charter::charter_shaped("home"),
            "the mesh is charter-shaped: a charter was accepted for it, whatever the config line now says"
        );
        assert!(
            grant_in_mesh(Some("home"), &op_key) == Grant::none(),
            "a listed key with no record holds nothing once the config line and the record disagree"
        );
        assert!(
            grant_in_mesh(Some("home"), &receiver_key) == Grant::none(),
            "and a key WITH a paired record does not fall back to it either: {grant:?}"
        );
        // And nothing local may widen while the key is undecidable: `on` still
        // answers WidensCharter rather than pushing a capability into grants.
        let mut nodes = aoide_storage::node_store::load_nodes();
        let before = nodes.iter().find(|n| n.name == "receiverbox").unwrap().grants.clone();
        assert_eq!(
            aoide_storage::node_store::set_node_allow(&mut nodes, "receiverbox", "spawn", true, "home").unwrap_err(),
            AllowError::WidensCharter,
            "a charter-shaped mesh whose operator key is undecidable is not a licence to widen locally"
        );
        assert_eq!(
            nodes.iter().find(|n| n.name == "receiverbox").unwrap().grants,
            before,
            "and the refused call wrote nothing into the paired record"
        );
        std::fs::write(
            root.join("receiver").join("config.toml"),
            format!("[mesh.home]\noperator = \"ed25519:{}\"\n", init.operator),
        )
        .unwrap();
        assert!(grant_in_mesh(Some("home"), &receiver_key).holds("message"), "and the charter answers again once resolved");

        // 5. Revocation: v2 re-signed with the line DELETED.
        charter_machine(&root, "operator", "opbox");
        let v2 = format!("mesh = \"home\"\nversion = 1\nrelays = []\n\n[nodes]\n{op_line}\n");
        std::fs::write(aoide_storage::charter::source_path("home"), &v2).unwrap();
        aoide_storage::charter::sign("home", None).unwrap();
        let v2_bytes = std::fs::read(aoide_storage::charter::source_path("home")).unwrap();
        let v2_sig = std::fs::read(aoide_storage::charter::source_sig_path("home")).unwrap();

        charter_machine(&root, "receiver", "receiverbox");
        assert_eq!(aoide_storage::charter::accept(&v2_bytes, &v2_sig).unwrap().charter.version, 2);
        assert!(
            grant_in_mesh(Some("home"), &receiver_key) == Grant::none(),
            "a removed line refuses on the FIRST request after the new version is received"
        );
        assert!(aoide_storage::charter::in_force_charter("home").unwrap().nodes.get("receiverbox").is_none());

        let _ = std::fs::remove_dir_all(&root);
        for (key, value) in [
            ("AOIDE_ROOT", saved_root),
            ("AOIDE_STATE_DIR", saved_state),
            ("AOIDE_STAGE_DIR", saved_stage),
            ("AOIDE_CONFIG", saved_config),
            ("AOIDE_A2A_NODE_NAME", saved_node),
        ] {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }

    /// `tail` is read off `params.metadata` only, clamped to `1..=200`, and
    /// its absence is a plain status read.
    #[test]
    fn frame_tail_clamps_and_reads_only_params_metadata() {
        assert_eq!(frame_tail(&json!({})), None, "no params at all");
        assert_eq!(frame_tail(&json!({ "metadata": {} })), None, "no frame key: a status read");
        assert_eq!(frame_tail(&frame_request("s", None)["params"]), Some(50), "absent tail takes the watch default");
        assert_eq!(frame_tail(&frame_request("s", Some(5))["params"]), Some(5));
        assert_eq!(frame_tail(&frame_request("s", Some(0))["params"]), Some(1), "the floor is one line");
        assert_eq!(frame_tail(&frame_request("s", Some(10_000))["params"]), Some(200), "the ceiling is 200");
        assert_eq!(frame_tail(&json!({ "metadata": { "aoide/frame": { "tail": "many" } } })), Some(50));
        assert_eq!(frame_tail(&json!({ "metadata": { "aoide/frame": true } })), Some(50));
        assert_eq!(
            frame_tail(&json!({ "message": { "metadata": { "aoide/frame": { "tail": 5 } } } })),
            None,
            "`message.metadata` is message/send's own fallback, never tasks/get's"
        );
    }

    /// A frame read with no signature at all — and no door token configured,
    /// which is the shape that matters (`read_ok` is true for everyone) — is
    /// refused, and the refusal says the SAME thing whether the session
    /// exists or not.
    #[test]
    fn an_unsigned_frame_read_is_refused_and_is_no_existence_oracle() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _roots = frame_stage("unsigned", "sess-1", "working");
        let ctx = test_ctx(Path::new("/dev/null"), "");

        let known = handle_jsonrpc(&frame_request("sess-1", Some(5)), &ctx);
        let unknown = handle_jsonrpc(&frame_request("ghost", Some(5)), &ctx);
        assert_eq!(known["error"]["code"], OUTPUT_READ_REFUSED_CODE);
        assert_eq!(unknown["error"]["code"], OUTPUT_READ_REFUSED_CODE);
        assert_eq!(
            OUTPUT_READ_REFUSED_CODE, -32011,
            "the read refusal has its own code, never verify_signed_request's -32007"
        );
        assert_eq!(
            known["error"]["message"], unknown["error"]["message"],
            "one text for every refusal, or the gate is an existence oracle"
        );
        assert_eq!(known["error"]["message"], OUTPUT_READ_REFUSED);

        // The SAME request without the frame key is the untouched status read
        // — answered, ungated, and carrying neither optional field.
        let status = handle_jsonrpc(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tasks/get", "params": { "id": "sess-1" } }), &ctx);
        assert_eq!(status["result"]["status"]["state"], "working");
        assert_eq!(status["result"]["kind"], "task");
        assert!(status["result"].get("artifacts").is_none(), "{status}");
        assert!(status["result"].get("history").is_none(), "{status}");

        // And an unknown id on the STATUS path is still the same -32001 it
        // always was.
        let missing = handle_jsonrpc(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tasks/get", "params": { "id": "ghost" } }), &ctx);
        assert_eq!(missing["error"]["code"], -32001);
        assert_eq!(missing["error"]["message"], "task not found");

    }

    /// A genuinely signed caller whose node was never granted `read` is
    /// refused with the same `-32011`.
    #[test]
    fn a_signed_node_without_read_is_refused() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _roots = frame_stage("no-read", "sess-1", "working");
        let kp = setup_signed_node_with_allows("yomi-strix", &["spawn"]);
        let body = serde_json::to_vec(&frame_request("sess-1", Some(5))).unwrap();
        let (name, key) = verified_caller(&kp, &body);
        let ctx = RequestCtx { signed_caller: Some(SignedCaller { name: &name, key: &key, mesh: None }), ..test_ctx(Path::new("/dev/null"), "") };
        let parsed = serde_json::from_slice::<Value>(&body).unwrap();

        let resp = handle_jsonrpc(&parsed, &ctx);
        assert_eq!(resp["error"]["code"], OUTPUT_READ_REFUSED_CODE);
        assert_eq!(resp["error"]["message"], OUTPUT_READ_REFUSED);

    }

    /// A signed, verified node holding `read` reads ANY session's frame on
    /// this host — including one whose record carries no `remoteParent` at
    /// all, which is the whole point of the 2026-09-25 ruling (reading is
    /// wider than writing). The frame is the local one, minus the fields that
    /// name this box.
    #[test]
    fn a_signed_reader_reads_any_sessions_frame() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _roots = frame_stage("admitted", "sess-1", "working");
        let kp = setup_signed_node_with_allows("yomi-strix", &["read"]);
        let body = serde_json::to_vec(&frame_request("sess-1", Some(5))).unwrap();
        let (name, key) = verified_caller(&kp, &body);
        let ctx = RequestCtx { signed_caller: Some(SignedCaller { name: &name, key: &key, mesh: None }), ..test_ctx(Path::new("/dev/null"), "") };
        let parsed = serde_json::from_slice::<Value>(&body).unwrap();

        let resp = handle_jsonrpc(&parsed, &ctx);
        assert!(resp.get("error").is_none(), "admitted: {resp}");
        let task = &resp["result"];
        assert_eq!(task["id"], "sess-1");
        assert_eq!(task["status"]["state"], "working", "the status read is the same one");
        assert_eq!(task["artifacts"][0]["artifactId"], "frame");
        assert_eq!(task["artifacts"][0]["parts"][0]["kind"], "data");
        let frame = &task["artifacts"][0]["parts"][0]["data"];
        assert_eq!(frame["sessionId"], "sess-1");
        assert_eq!(frame["output"], json!(["line one", "line two"]));
        assert_eq!(frame["truncated"], Value::Null, "nothing was cut: the flag is absent");
        assert!(frame["logPath"].is_null(), "a path on this box never rides the wire: {frame}");
        assert!(frame["instructionsPath"].is_null(), "{frame}");
        assert!(frame["socket"].is_null(), "{frame}");
        assert!(task.get("history").is_none(), "{task}");

    }

    /// A signed reader asking for a session that keeps no frame gets the
    /// watch's own taught refusal, under the unknown-id code — the gate came
    /// first, so this is a per-session answer, not a refusal of the caller.
    #[test]
    fn a_frame_read_of_an_unknown_session_is_taught_after_the_gate() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _roots = frame_stage("unknown-after-gate", "sess-1", "working");
        let kp = setup_signed_node_with_allows("yomi-strix", &["read"]);
        let body = serde_json::to_vec(&frame_request("ghost", Some(5))).unwrap();
        let (name, key) = verified_caller(&kp, &body);
        let ctx = RequestCtx { signed_caller: Some(SignedCaller { name: &name, key: &key, mesh: None }), ..test_ctx(Path::new("/dev/null"), "") };
        let parsed = serde_json::from_slice::<Value>(&body).unwrap();

        let resp = handle_jsonrpc(&parsed, &ctx);
        assert_eq!(resp["error"]["code"], -32001);
        assert_eq!(resp["error"]["message"], "task not found", "{resp}");

    }

    /// A frame whose letters alone blow the byte cap sheds the OLDEST letters
    /// first — the newest is what a watcher is looking at — keeps the output
    /// lines (they come second), and says `truncated`.
    #[test]
    fn the_frame_byte_cap_sheds_the_oldest_letters_first() {
        let line = "x".repeat(600);
        let letter = |seq: u64| aoide_conduct::graph::MailLine {
            seq,
            received_at: "2026-01-01T00:00:00Z".to_string(),
            from: "child".to_string(),
            subject: "s".to_string(),
            run: "this run".to_string(),
            body: vec![line.clone(); 40],
        };
        let frame = aoide_conduct::graph::Frame {
            session_id: "sess-1".to_string(),
            label: "run".to_string(),
            agent: "claude".to_string(),
            task: Some("fix-flaky".to_string()),
            parent: None,
            started_at: "2026-01-01T00:00:00Z".to_string(),
            state: "running".to_string(),
            presence: "running".to_string(),
            status: "running".to_string(),
            exit_code: None,
            ended_at: None,
            outcome: None,
            socket: Some("/run/sess-1.sock".to_string()),
            conductable: true,
            log_path: Some("/state/sess-1.log".to_string()),
            instructions_path: Some("/state/sess-1.md".to_string()),
            instructions: None,
            output: (0..200).map(|i| format!("{i:03}{}", "y".repeat(100))).collect(),
            mail: (0..20).map(letter).collect(),
            report: "not filed yet".to_string(),
            wake: None,
            suggested: Some("aoide send --id sess-1 --submit -- \"<text>\"".to_string()),
            truncated: false,
        };
        let before = serde_json::to_vec(&frame).unwrap().len();
        assert!(before > FRAME_MAX_BYTES, "the fixture must really be over the cap: {before}");

        let artifact = frame_artifact(frame);
        let data = &artifact.parts[0].extra["data"];
        assert_eq!(data["truncated"], true, "the cap says so: {data}");
        assert!(
            serde_json::to_vec(data).unwrap().len() <= FRAME_MAX_BYTES,
            "the frame is inside the cap once sheddable content went"
        );
        let mail = data["mail"].as_array().unwrap();
        assert!(!mail.is_empty(), "the NEWEST letters survive");
        assert_eq!(mail.last().unwrap()["seq"], 19, "the newest letter is the one kept");
        assert!(mail[0]["seq"].as_u64().unwrap() > 0, "and the oldest are the ones gone: {mail:?}");
        assert_eq!(
            mail[0]["body"].as_array().unwrap().len(),
            LETTER_BODY_LINES_MAX,
            "every surviving letter's body is still clamped to 40 lines"
        );
        assert_eq!(data["output"].as_array().unwrap().len(), 200, "output is shed SECOND, so it is intact here");
        assert!(data["logPath"].is_null(), "and the wire strike still happened: {data}");
    }

    /// A letter body over the per-letter cap is cut to 40 lines — a
    /// per-letter bound, not the byte cap, so it does NOT set `truncated`.
    #[test]
    fn a_letter_body_is_capped_at_forty_lines_without_flagging_the_frame() {
        let frame = aoide_conduct::graph::Frame {
            session_id: "sess-1".to_string(),
            label: "run".to_string(),
            agent: "claude".to_string(),
            task: Some("fix-flaky".to_string()),
            parent: None,
            started_at: "2026-01-01T00:00:00Z".to_string(),
            state: "running".to_string(),
            presence: "running".to_string(),
            status: "running".to_string(),
            exit_code: None,
            ended_at: None,
            outcome: None,
            socket: None,
            conductable: false,
            log_path: Some("/state/sess-1.log".to_string()),
            instructions_path: None,
            instructions: None,
            output: vec!["one line".to_string()],
            mail: vec![aoide_conduct::graph::MailLine {
                seq: 3,
                received_at: "2026-01-01T00:00:00Z".to_string(),
                from: "child".to_string(),
                subject: "s".to_string(),
                run: "this run".to_string(),
                body: (0..100).map(|i| format!("line {i}")).collect(),
            }],
            report: "not filed yet".to_string(),
            wake: None,
            suggested: None,
            truncated: false,
        };
        let artifact = frame_artifact(frame);
        let data = &artifact.parts[0].extra["data"];
        assert_eq!(data["mail"][0]["body"].as_array().unwrap().len(), 40);
        assert_eq!(data["mail"][0]["body"][39], "line 39", "the FIRST 40 lines are the ones kept");
        assert!(data.get("truncated").is_none(), "the byte cap is what raises the flag: {data}");
    }

    // ── P-RSA S8: the ping-back history on `tasks/get` (CONTRACTS.md §6) ─────

    /// The frame's own stage, plus the two things history adds: a
    /// `remoteParent` stamp naming `key` as the parent's verifying key, and a
    /// ring holding `n` events for that child. Called AFTER the signed node is
    /// set up: `setup_signed_node_with_allows` writes `nodes.json` into
    /// whatever state root is current, and the door resolves the caller there.
    fn stamp_history_parent(id: &str, key: &str, n: u64) {
        let mut file: SessionsFile = load_stage(&sessions_path()).unwrap();
        file.sessions.iter_mut().find(|s| s.session_id == id).unwrap().remote_parent =
            Some(RemoteParent {
                node: "sakaki".to_string(),
                key: key.to_string(),
                session_id: "parent-1".to_string(),
                extra: Default::default(),
            });
        write_stage(&sessions_path(), &file).unwrap();
        for i in 0..n {
            aoide_storage::pingback_remote::spool_event(id, key, 1_790_313_000, json!({ "exited": { "code": i } }));
        }
    }

    /// A `tasks/get` body asking for ping-back history. The wire key is
    /// spelled out here, never taken from the const: a renamed const must fail
    /// this test, not silently move the wire (the same rule [`frame_request`]
    /// holds).
    fn history_request(id: &str, after: u64) -> Value {
        json!({
            "jsonrpc": "2.0", "id": 1, "method": "tasks/get",
            "params": { "id": id, "metadata": { "aoide/linesAfter": after } },
        })
    }

    /// Both output keys in ONE request — the shape that has to fail closed.
    fn both_request(id: &str) -> Value {
        json!({
            "jsonrpc": "2.0", "id": 1, "method": "tasks/get",
            "params": { "id": id, "metadata": { "aoide/frame": { "tail": 5 }, "aoide/linesAfter": 0 } },
        })
    }

    /// A signed caller's context, from a body signed by `kp`.
    fn signed_ctx(kp: &aoide_storage::identity::Keypair, body: &[u8]) -> (String, String) {
        verified_caller(kp, body)
    }

    /// `linesAfter` is read off `params.metadata` only, and a value that is not
    /// a number reads as the start of the ring — a wrong cursor costs
    /// duplicates, never a refusal.
    #[test]
    fn lines_after_reads_only_params_metadata_and_tolerates_any_value() {
        assert_eq!(lines_after(&json!({})), None, "no params at all");
        assert_eq!(lines_after(&json!({ "metadata": {} })), None, "no linesAfter key: a status read");
        assert_eq!(lines_after(&history_request("s", 0)["params"]), Some(0));
        assert_eq!(lines_after(&history_request("s", 7)["params"]), Some(7));
        assert_eq!(
            lines_after(&json!({ "metadata": { "aoide/linesAfter": null } })),
            Some(0),
            "a cursor nobody wrote a number for starts at the beginning"
        );
        assert_eq!(lines_after(&json!({ "metadata": { "aoide/linesAfter": "many" } })), Some(0));
        assert_eq!(
            lines_after(&json!({ "message": { "metadata": { "aoide/linesAfter": 5 } } })),
            None,
            "`message.metadata` is message/send's own fallback, never tasks/get's"
        );
    }

    /// The history gate's whole table at the predicate (P-RSA S8): the output
    /// gate AND the target's stamped key equal to the caller's verifying key.
    /// Nothing else — not the stored `node` label, and never a wildcard.
    #[test]
    fn history_is_admitted_only_for_the_key_this_door_stamped() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STATE_DIR").ok();
        let root = aoide_test_support::short_tmp("a2a-history-gate");
        std::env::set_var("AOIDE_STATE_DIR", &root);

        let mut reader = fixture_node("box-b", "http://10.0.0.5:8710/");
        reader.verified = true;
        reader.pubkey = Some("key-of-mine".to_string());
        reader.grants = aoide_storage::node_store::grants_in("home", &["read"]);
        aoide_storage::node_store::save_nodes(&[reader.clone()]).unwrap();

        let sig = |key: &'static str| Some(SignedCaller { name: "box-b", key, mesh: None });
        let stamp = |key: &str| RemoteParent {
            node: "sakaki".to_string(),
            key: key.to_string(),
            session_id: "parent-1".to_string(),
            extra: Default::default(),
        };

        let _mine = stamp("key-of-mine");
        assert!(
            history_admitted(true, sig("key-of-mine"), Some("key-of-mine")),
            "the key this door stamped for the child reads it"
        );
        assert!(
            !history_admitted(true, sig("key-of-someone-else"), Some("key-of-mine")),
            "a foreign key does not (and holds no grant of its own either)"
        );
        assert!(
            !history_admitted(true, sig("key-of-mine"), None),
            "an id with neither a record nor a ring has no key to match"
        );

        // Not a wildcard in either direction: an EMPTY stamped key (a
        // hand-written record) matches nobody, and an empty caller key is not
        // a `SignedCaller` at all (the door resolves one only from a key that
        // verified).
        assert!(!history_admitted(true, sig("key-of-mine"), Some("")));

        // Unsigned, a weaker rung, no `read`, unverified: all refused, and all
        // by the output gate that runs first.
        assert!(!history_admitted(true, None, Some("key-of-mine")), "no proof, no key to match");
        assert!(
            !history_admitted(false, sig("key-of-mine"), Some("key-of-mine")),
            "the bearer gate still runs first"
        );
        let mut no_read = reader.clone();
        no_read.grants = aoide_storage::node_store::grants_in("home", &["spawn"]);
        aoide_storage::node_store::save_nodes(&[no_read]).unwrap();
        assert!(!history_admitted(true, sig("key-of-mine"), Some("key-of-mine")), "`read` is still required");
        let mut unverified = reader.clone();
        unverified.verified = false;
        aoide_storage::node_store::save_nodes(&[unverified]).unwrap();
        assert!(!history_admitted(true, sig("key-of-mine"), Some("key-of-mine")), "a pairing is still required");

        match saved {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    /// The matching key reads the ring: the events after the cursor, the
    /// newest `seq`, and no gap — as ONE `data` message under `history`,
    /// leaving the status read and the frame artifact exactly as they were.
    #[test]
    fn a_signed_parent_reads_its_childs_ping_back_history() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _roots = frame_stage("history-admitted", "sess-1", "working");
        let kp = setup_signed_node_with_allows("yomi-strix", &["read"]);
        stamp_history_parent("sess-1", &kp.info().pubkey_hex, 3);
        let body = serde_json::to_vec(&history_request("sess-1", 0)).unwrap();
        let (name, key_used) = signed_ctx(&kp, &body);
        let ctx = RequestCtx {
            signed_caller: Some(SignedCaller { name: &name, key: &key_used, mesh: None }),
            ..test_ctx(Path::new("/dev/null"), "")
        };
        let parsed = serde_json::from_slice::<Value>(&body).unwrap();

        let resp = handle_jsonrpc(&parsed, &ctx);
        assert!(resp.get("error").is_none(), "admitted: {resp}");
        let task = &resp["result"];
        assert_eq!(task["id"], "sess-1");
        assert_eq!(task["status"]["state"], "working", "the status read is the same one");
        assert!(task.get("artifacts").is_none(), "history is not a frame: {task}");

        let message = &task["history"][0];
        assert_eq!(message["messageId"], "pingback", "found by identity, not by position");
        assert_eq!(message["parts"][0]["kind"], "data");
        let ring = &message["parts"][0]["data"];
        assert_eq!(ring["events"].as_array().unwrap().len(), 3, "{ring}");
        assert_eq!(ring["events"][0]["seq"], 1);
        assert_eq!(ring["events"][0]["event"], json!({ "exited": { "code": 0 } }));
        assert_eq!(ring["gap"], false);
        assert_eq!(ring["last"], 3);

        // The cursor is the request's own input, and the ring's own seq is what
        // a parent advances to.
        let body = serde_json::to_vec(&history_request("sess-1", 2)).unwrap();
        let (name, key_used) = signed_ctx(&kp, &body);
        let ctx = RequestCtx {
            signed_caller: Some(SignedCaller { name: &name, key: &key_used, mesh: None }),
            ..test_ctx(Path::new("/dev/null"), "")
        };
        let resp = handle_jsonrpc(&serde_json::from_slice::<Value>(&body).unwrap(), &ctx);
        let ring = &resp["result"]["history"][0]["parts"][0]["data"];
        assert_eq!(ring["events"].as_array().unwrap().len(), 1, "only what is past the cursor: {ring}");
        assert_eq!(ring["events"][0]["seq"], 3);
        assert_eq!(ring["last"], 3);
    }

    /// A caller holding `read` but NOT this child's stamped key: refused for
    /// history with `-32011` and the history's own text — while the frame it
    /// asks for on its own is still admitted (the 2026-09-25 ruling: reading
    /// is wider than reading a child's ping-back), and a request carrying BOTH
    /// keys fails closed.
    #[test]
    fn a_foreign_key_is_refused_for_history_while_its_frame_is_admitted() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _roots = frame_stage("history-foreign", "sess-1", "working");
        let kp = setup_signed_node_with_allows("yomi-strix", &["read"]);
        stamp_history_parent("sess-1", &"cc".repeat(32), 2);

        let refused = |request: Value| -> Value {
            let body = serde_json::to_vec(&request).unwrap();
            let (name, key) = signed_ctx(&kp, &body);
            let ctx = RequestCtx {
                signed_caller: Some(SignedCaller { name: &name, key: &key, mesh: None }),
                ..test_ctx(Path::new("/dev/null"), "")
            };
            handle_jsonrpc(&serde_json::from_slice::<Value>(&body).unwrap(), &ctx)
        };

        let resp = refused(history_request("sess-1", 0));
        assert_eq!(resp["error"]["code"], OUTPUT_READ_REFUSED_CODE);
        assert_eq!(resp["error"]["message"], HISTORY_READ_REFUSED);
        assert_ne!(
            HISTORY_READ_REFUSED, OUTPUT_READ_REFUSED,
            "the reason a caller reads must be the reason it was refused"
        );

        // The same caller, the same session, asked for as a FRAME: admitted.
        let resp = refused(frame_request("sess-1", Some(5)));
        assert!(resp.get("error").is_none(), "a reader may still watch the frame: {resp}");
        assert_eq!(resp["result"]["artifacts"][0]["artifactId"], "frame");
        assert!(resp["result"].get("history").is_none(), "{resp}");

        // BOTH keys in one request: asking for the history is asking for the
        // history, so the stricter gate decides the whole request — the
        // alternative is a frame answered around a silently missing ring.
        let resp = refused(both_request("sess-1"));
        assert_eq!(resp["error"]["code"], OUTPUT_READ_REFUSED_CODE);
        assert_eq!(resp["error"]["message"], HISTORY_READ_REFUSED);

        // And an id this node holds NEITHER a record NOR a ring for is the
        // plain not-found every other `tasks/get` arm answers an unknown id
        // with — never the history-specific refusal, which would tell the
        // caller only "not your child" about an id that will never come back
        // (the pull latches a row on exactly this code, H2 of the S8/S9
        // review). It stays no oracle about a RING: an id that has one answers
        // only its own parent's key, as above.
        let resp = refused(history_request("ghost", 0));
        assert_eq!(resp["error"]["code"], TASK_NOT_FOUND_CODE);
        assert_eq!(resp["error"]["message"], "task not found");
    }

    /// H2 of the S8/S9 review: the ring is DESIGNED to outlive the child's
    /// roster record, and the read the whole lane exists for is the one a
    /// pruned child's parent still owes itself — so the door serves the ring
    /// without the record, gated on the key the RING was stamped with.
    #[test]
    fn a_pruned_childs_ring_is_still_readable_by_its_own_parent_key() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _roots = frame_stage("history-ring-outlives", "sess-1", "working");
        let kp = setup_signed_node_with_allows("yomi-strix", &["read"]);
        let key = kp.info().pubkey_hex;
        stamp_history_parent("sess-1", &key, 2);

        // An unrelated reap prunes the child: the record is gone, the ring is
        // not (nothing prunes a ring while its parent may still read it).
        let mut file: SessionsFile = load_stage(&sessions_path()).unwrap();
        file.sessions.retain(|s| s.session_id != "sess-1");
        write_stage(&sessions_path(), &file).unwrap();

        let ask = |request: Value, caller_key: &str| -> Value {
            let body = serde_json::to_vec(&request).unwrap();
            let ctx = RequestCtx {
                signed_caller: Some(SignedCaller { name: "yomi-strix", key: caller_key, mesh: None }),
                ..test_ctx(Path::new("/dev/null"), "")
            };
            handle_jsonrpc(&serde_json::from_slice::<Value>(&body).unwrap(), &ctx)
        };

        // The parent that was stamped reads it, and the envelope is the ring's
        // own tail: the last event is the child's exit, so the child is
        // `completed` — the one state this node can still honestly derive.
        let resp = ask(history_request("sess-1", 0), &key);
        assert!(resp.get("error").is_none(), "the ring outlives the record: {resp}");
        assert_eq!(resp["result"]["id"], "sess-1");
        assert_eq!(resp["result"]["contextId"], "sess-1");
        assert_eq!(resp["result"]["status"]["state"], "completed", "{resp}");
        assert!(resp["result"].get("artifacts").is_none(), "a ring is not a frame: {resp}");
        let ring = &resp["result"]["history"][0]["parts"][0]["data"];
        assert_eq!(ring["events"].as_array().unwrap().len(), 2, "{ring}");
        assert_eq!(ring["last"], 2);

        // Any other key is refused, with the history's own words — a ring does
        // not become public just because its record is gone. The stranger is
        // a real, verified node holding `read` in this mesh (a caller whose
        // key verifies NOTHING would never reach this gate at all: it has no
        // grant, so the output gate refuses it first — pinned by
        // `output_read_admitted_is_a_signed_caller_holding_read_in_the_mesh_it_names`).
        let stranger = aoide_storage::identity::mint_ephemeral().unwrap();
        let stranger_key = stranger.info().pubkey_hex;
        let mut nodes = aoide_storage::node_store::load_nodes();
        let mut other = fixture_node("other-box", "http://other/");
        other.verified = true;
        other.pubkey = Some(stranger_key.clone());
        other.grants = aoide_storage::node_store::grants_in("home", &["read"]);
        nodes.push(other);
        aoide_storage::node_store::save_nodes(&nodes).unwrap();

        let resp = ask(history_request("sess-1", 0), &stranger_key);
        assert_eq!(resp["error"]["code"], OUTPUT_READ_REFUSED_CODE);
        assert_eq!(resp["error"]["message"], HISTORY_READ_REFUSED);

        // A FRAME for the same pruned child is still the not-found it always
        // was: a frame needs a record, and there is none.
        let resp = ask(frame_request("sess-1", Some(5)), &key);
        assert_eq!(resp["error"]["code"], TASK_NOT_FOUND_CODE);
        assert_eq!(resp["error"]["message"], "task not found");
    }

    /// No signature at all, and no door token configured (`read_ok` true for
    /// everyone — the shape that matters): the output gate refuses first, so
    /// the caller reads the grant it is missing and not the parent it is not.
    #[test]
    fn an_unsigned_history_read_is_refused() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _roots = frame_stage("history-unsigned", "sess-1", "working");
        stamp_history_parent("sess-1", &"cc".repeat(32), 2);
        let ctx = test_ctx(Path::new("/dev/null"), "");

        let resp = handle_jsonrpc(&history_request("sess-1", 0), &ctx);
        assert_eq!(resp["error"]["code"], OUTPUT_READ_REFUSED_CODE);
        assert_eq!(resp["error"]["message"], OUTPUT_READ_REFUSED, "{resp}");

        // The same request without the key is the untouched status read.
        let status = handle_jsonrpc(
            &json!({ "jsonrpc": "2.0", "id": 1, "method": "tasks/get", "params": { "id": "sess-1" } }),
            &ctx,
        );
        assert_eq!(status["result"]["status"]["state"], "working");
        assert!(status["result"].get("history").is_none(), "{status}");
    }

    // ── H1: the mail adapter (`aoide mail serve`) ───────────────────────────
    //
    // The adapter is a second LISTENER, not a second door: these tests pin
    // that its table is the three mail methods plus the stripped card, that
    // every other method name is unreachable from it, that the card is
    // stripped unconditionally, that `handle_connection`'s shared signed-request
    // path still refuses an unsigned caller and a replayed nonce there, that it
    // never looks at the Host header, and that it binds loopback.

    /// A tiny registry standing in for the assembled one: enough implemented
    /// commands that a NON-stripped card would leak a skills inventory.
    fn full_card_registry() -> Registry {
        let mut r = Registry::new();
        let paths: [&'static [&'static str]; 3] = [&["message", "send"], &["tasks", "get"], &["mail", "deposit"]];
        for path in paths {
            r.insert(Command {
                path,
                summary: "a skill that must never reach the adapter's card",
                args: &[],
                flags: &[],
                gated: false,
                implemented: true,
                internal: false,
                exit_codes: (),
                examples: &[],
                available: || true,
                handler: fake_handler,
                doors: &[Door::Cli, Door::Daemon, Door::Mcp, Door::A2a],
                ..aoide_protocol::registry::Command::BLANK
            });
        }
        r
    }

    /// Isolate `AOIDE_STATE_DIR` for one adapter test (the pattern every
    /// signed-request test above already uses).
    fn adapter_root(tag: &str) -> (std::path::PathBuf, Option<String>) {
        let root = aoide_test_support::short_tmp(&format!(
            "aoide-a2a-mail-adapter-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let saved = std::env::var("AOIDE_STATE_DIR").ok();
        std::env::set_var("AOIDE_STATE_DIR", &root);
        (root, saved)
    }

    fn adapter_cleanup(root: &std::path::Path, saved: Option<String>) {
        let _ = std::fs::remove_dir_all(root);
        match saved {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }

    /// One `HttpRequest` as the adapter's route sees it.
    fn mail_req(method: &str, path: &str, body: &[u8]) -> HttpRequest {
        HttpRequest {
            method: method.to_string(),
            path: path.to_string(),
            body: body.to_vec(),
            bearer: None,
            signed_node: None,
            signed_timestamp: None,
            signed_nonce: None,
            signed_signature: None,
            signed_mesh: None,
        }
    }

    #[test]
    fn the_mail_route_serves_the_card_and_the_three_methods_and_nothing_else() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, saved) = adapter_root("route");
        let audit_log = root.join("log");
        let registry = full_card_registry();
        let card = route_mail(
            &mail_req("GET", "/.well-known/agent-card.json", b""),
            MAIL_ADAPTER_BIND,
            MAIL_ADAPTER_PORT_DEFAULT,
            &audit_log,
            ConnOrigin::Loopback,
            &registry,
            None,
        );
        assert_eq!(card.0, 200, "the card GET answers");

        let unknown = route_mail(
            &mail_req("GET", "/anything/else", b""),
            MAIL_ADAPTER_BIND,
            MAIL_ADAPTER_PORT_DEFAULT,
            &audit_log,
            ConnOrigin::Loopback,
            &registry,
            None,
        );
        assert_eq!(unknown.0, 404, "any other path is not found");
        assert_eq!(unknown.2, "a2a.not-found");

        let not_allowed = route_mail(
            &mail_req("POST", "/.well-known/agent-card.json", b"{}"),
            MAIL_ADAPTER_BIND,
            MAIL_ADAPTER_PORT_DEFAULT,
            &audit_log,
            ConnOrigin::Loopback,
            &registry,
            None,
        );
        assert_eq!(not_allowed.0, 405, "the card is a GET");

        // A mail method on `POST /` reaches the shared dispatcher and is
        // labelled exactly as the door labels it (the same whitelist).
        let poll = route_mail(
            &mail_req(
                "POST",
                "/",
                br#"{"jsonrpc":"2.0","id":1,"method":"aoide/mailPoll","params":{"node":"box-b"}}"#,
            ),
            MAIL_ADAPTER_BIND,
            MAIL_ADAPTER_PORT_DEFAULT,
            &audit_log,
            ConnOrigin::Loopback,
            &registry,
            None,
        );
        assert_eq!(poll.0, 200);
        assert_eq!(poll.2, "a2a.aoide/mailPoll", "the adapter reuses the door's label whitelist");
        let body: Value = serde_json::from_slice(&poll.1).unwrap();
        assert_eq!(body["error"]["code"], -32010, "unsigned → the mail admission refusal: {body}");

        adapter_cleanup(&root, saved);
    }

    #[test]
    fn the_adapter_serves_a_stripped_card_even_with_a_full_registry() {
        let registry = full_card_registry();
        let full = agent_card(&registry, "127.0.0.1", MAIL_ADAPTER_PORT_DEFAULT);
        assert!(full["skills"].as_array().unwrap().len() >= 3, "the fixture registry is not empty");

        let (status, body, label) = route_mail(
            &mail_req("GET", "/.well-known/agent-card.json", b""),
            MAIL_ADAPTER_BIND,
            MAIL_ADAPTER_PORT_DEFAULT,
            Path::new("/dev/null"),
            ConnOrigin::Loopback,
            &registry,
            None,
        );
        assert_eq!(status, 200);
        assert_eq!(label, "a2a.agent-card");
        let card: Value = serde_json::from_slice(&body).unwrap();
        let keys: Vec<&str> = card.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            vec!["name", "protocolVersion", "url"],
            "the adapter's card is the three-key shape, unconditionally: {card}"
        );
        assert_eq!(card["url"], format!("http://127.0.0.1:{MAIL_ADAPTER_PORT_DEFAULT}/"));
    }

    #[test]
    fn a_plaintext_envelope_is_refused_sealed_required_on_the_adapter_and_still_accepted_on_the_door() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("sealed-required");
        act_as(&root, "here");
        // The envelope's own ORIGIN is this box (a letter minted here), so the
        // record that makes it verifiable is the local node's — the fixture
        // the other plaintext-lane tests use.
        let origin_name = setup_verifiable_origin(&["message"]);

        let audit_log = root.join("log");
        aoide_storage::mail::box_add("bob").unwrap();
        let envelope = aoide_storage::mail::mint_outbound_letter("alice", &aoide_storage::display::local_node_name(), "bob", "hi").unwrap();
        let params = json!({ "envelope": envelope });

        // The adapter: refused as a RESULT carrying the taught word, audited,
        // and nothing filed.
        let adapter = RequestCtx::mail_adapter(&audit_log, ConnOrigin::Loopback, Some(caller(&origin_name)));
        let refused = mail_deposit(&params, &adapter).expect("a refusal here is a result, not a JSON-RPC error");
        assert_eq!(refused["status"], "refused", "{refused}");
        assert_eq!(refused["reason"], "sealed-required", "{refused}");
        assert!(refused["detail"].as_str().unwrap().contains("sealed-required"), "{refused}");
        assert!(
            aoide_storage::mail::read_base().unwrap().is_empty(),
            "a refused plaintext envelope is never filed"
        );
        let log = std::fs::read_to_string(&audit_log).unwrap_or_default();
        assert!(log.contains("a2a.aoide/mailDeposit") && log.contains("sealed-required"), "{log}");

        // The door — the SSH direct lane — is untouched: the same plaintext
        // envelope from the same admitted peer still files, which is the
        // per-peer upgrade path a destination with no binding depends on.
        let door = mail_deposit_ctx(&audit_log, Some(&origin_name));
        let filed = mail_deposit(&params, &door).expect("the direct lane still accepts a plaintext envelope");
        assert_eq!(filed["status"], "accepted", "{filed}");
        assert_eq!(aoide_storage::mail::read_base().unwrap().len(), 1, "the direct lane filed it");

        mail_deposit_cleanup(&root, saved_state, saved_stage);
    }

    /// A letter to a name with no box answers `unknown-box` with the fixed
    /// detail, which names no box; nothing is filed and no box is created.
    #[test]
    fn an_unknown_box_answers_unknown_box_and_names_no_box() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("unknown-box");
        act_as(&root, "here");
        let origin_name = setup_verifiable_origin(&["message"]);

        let audit_log = root.join("log");
        let envelope = aoide_storage::mail::mint_outbound_letter("alice", &aoide_storage::display::local_node_name(), "carol", "hi").unwrap();
        let params = json!({ "envelope": envelope });
        let door = mail_deposit_ctx(&audit_log, Some(&origin_name));
        let refused = mail_deposit(&params, &door).expect("a refusal is a result, not a JSON-RPC error");

        assert_eq!(refused["status"], "refused", "{refused}");
        assert_eq!(refused["reason"], "unknown-box", "{refused}");
        assert_eq!(refused["detail"], aoide_storage::mail::UNKNOWN_BOX_DETAIL, "{refused}");
        assert!(!refused.to_string().contains("carol"), "the answer names no box: {refused}");
        assert!(aoide_storage::mail::read_base().unwrap().is_empty(), "nothing is filed");
        assert!(!aoide_storage::mail::box_path("carol").exists(), "a far deposit created a box");

        mail_deposit_cleanup(&root, saved_state, saved_stage);
    }

    #[test]
    fn the_adapter_poll_withholds_a_plaintext_entry_while_the_door_hands_it_over() {
        // F1: sealed-only binds the pull direction. An entry spooled toward a
        // poller that held no binding is the ONE shape a poll could put on an
        // HTTPS hop in the clear, and `exchange_bindings` (best-effort, at the
        // top of every poll) is not what enforces it — this is.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let root = mail_deposit_root("poll-withholds");
        act_as(&root, "here");
        setup_signed_node_with_allows("box-b", &["message"]);
        let audit_log = root.join("log");

        // A `hold` entry (offered to every poll) toward a node this box holds
        // NO binding for → `hand_over` can only answer with the plaintext.
        let envelope =
            aoide_storage::mail::mint_outbound_letter("alice", "box-b", "bob", "letter-bytes-must-not-cross").unwrap();
        let msgid = envelope.msgid.clone();
        aoide_storage::outbox::write_entry("box-b", &aoide_storage::outbox::OutboxEntry::held(envelope)).unwrap();

        let params = json!({ "node": "box-b" });

        // The ADAPTER: withheld, named, and nothing left this process.
        let adapter = RequestCtx::mail_adapter(&audit_log, ConnOrigin::Loopback, Some(caller("box-b")));
        let withheld = mail_poll(&params, &adapter).expect("a poll answer, not an error");
        assert_eq!(withheld["envelopes"], json!([]), "no plaintext envelope is handed over: {withheld}");
        assert_eq!(withheld["containers"], json!([]), "{withheld}");
        assert_eq!(withheld["withheld"][0]["msgid"], msgid, "{withheld}");
        assert_eq!(withheld["withheld"][0]["reason"], "sealed-required", "{withheld}");
        assert!(
            withheld["withheld"][0]["detail"].as_str().unwrap().contains("sealed-required")
                || withheld["withheld"][0]["detail"].as_str().unwrap().contains("binding"),
            "the withheld entry says what to do about it: {withheld}"
        );
        assert!(
            !serde_json::to_string(&withheld).unwrap().contains("letter-bytes-must-not-cross"),
            "the letter's own bytes never ride an adapter answer: {withheld}"
        );
        let still_spooled = aoide_storage::outbox::list_entries("box-b").unwrap();
        assert_eq!(still_spooled.len(), 1, "a withheld entry STAYS spooled: {still_spooled:?}");
        let log = std::fs::read_to_string(&audit_log).unwrap_or_default();
        assert!(
            log.contains("1 withheld (sealed-required)"),
            "the poll's own audit line counts what it refused to hand over: {log}"
        );

        // The DOOR — not an HTTPS hop — is unchanged: the same entry is handed
        // over, with no `withheld` key at all.
        let door = mail_deposit_ctx(&audit_log, Some("box-b"));
        let handed = mail_poll(&params, &door).expect("a poll answer");
        assert_eq!(handed["envelopes"][0]["text"], "letter-bytes-must-not-cross", "{handed}");
        assert!(handed.get("withheld").is_none(), "the door's answer shape is untouched: {handed}");

        mail_deposit_cleanup(&root, saved_state, saved_stage);
    }

    /// **The charter holds THROUGH the adapter, not just at the door**
    /// (P-CHARTER × H1): the adapter lane routes `mailPoll` into the same
    /// `mail_poll` → `effective_mesh`/`caller_grant`/`poll_admitted` the door
    /// uses, and `handle_connection`'s one `verify_signed_request` call is the
    /// same one too — so a key a charter listed, and then stopped listing, is
    /// refused on the adapter exactly as it would be on the door. Nothing here
    /// is adapter-specific: that is the point.
    #[test]
    fn a_charter_revoked_key_is_refused_through_the_mail_adapter() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_root = std::env::var("AOIDE_ROOT").ok();
        let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_config = std::env::var("AOIDE_CONFIG").ok();
        let saved_node = std::env::var("AOIDE_A2A_NODE_NAME").ok();
        let root = mail_deposit_root("adapter-charter-revoked");

        // The operator box roots `home`; the receiving box takes the charter by
        // file, trusting only the operator key from its own config.
        charter_machine(&root, "operator", "opbox");
        let init = aoide_storage::charter::init("home").unwrap();
        let op_line = aoide_storage::charter::node_line().unwrap();
        charter_machine(&root, "receiver", "receiverbox");
        let receiver_line = aoide_storage::charter::node_line().unwrap();
        let receiver_key = aoide_storage::charter::parse(&format!(
            "mesh = \"home\"\nversion = 1\nrelays = []\n\n[nodes]\n{op_line}\n{receiver_line}\n"
        ))
        .unwrap()
        .nodes["receiverbox"]
            .key
            .clone();

        charter_machine(&root, "operator", "opbox");
        let v1 = format!("mesh = \"home\"\nversion = 0\nrelays = []\n\n[nodes]\n{op_line}\n{receiver_line}\n");
        std::fs::write(aoide_storage::charter::source_path("home"), &v1).unwrap();
        aoide_storage::charter::sign("home", None).unwrap();
        let v1_bytes = std::fs::read(aoide_storage::charter::source_path("home")).unwrap();
        let v1_sig = std::fs::read(aoide_storage::charter::source_sig_path("home")).unwrap();

        charter_machine(&root, "receiver", "receiverbox");
        std::fs::write(
            root.join("receiver").join("config.toml"),
            format!("[mesh.home]\noperator = \"ed25519:{}\"\n", init.operator),
        )
        .unwrap();
        assert_eq!(aoide_storage::charter::accept(&v1_bytes, &v1_sig).unwrap().charter.version, 1);

        // The dangerous record the N1 review closed the loophole for: this box
        // ALSO holds a verified paired record granting `message` in `home`. In
        // a charter mesh a paired record is inert — the charter answers — and
        // this test asserts that THROUGH the adapter.
        let mut stale = fixture_node("receiverbox", "http://10.0.0.5:8710/");
        stale.verified = true;
        stale.pubkey = Some(receiver_key.clone());
        stale.grants = aoide_storage::node_store::grants_in("home", &["message"]);
        aoide_storage::node_store::save_nodes(&[stale]).unwrap();

        let audit_log = root.join("log");
        let port = free_loopback_port();
        raise_mail_adapter(port, &audit_log);
        let (kp, _) = aoide_storage::identity::load_or_mint().unwrap();
        let now = || {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0)
        };
        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "aoide/mailPoll", "params": { "node": "receiverbox", "to": kp.info().pubkey_hex } })
            .to_string();
        let poll = |nonce: &str| {
            send_raw(
                port,
                &raw_request(
                    "aoide.necoconeco.net",
                    "POST",
                    "/",
                    body.as_bytes(),
                    Some((&kp, "receiverbox", now(), nonce)),
                    Some("home"),
                ),
            )
        };
        let answer = |raw: String| -> Value {
            serde_json::from_str(raw.split("\r\n\r\n").nth(1).unwrap()).expect("a JSON body")
        };

        // ADMITTED while the charter lists this key: the line IS the grant.
        let listed = answer(poll(&unique_nonce("adapter-charter-listed")));
        assert!(listed.get("error").is_none(), "a charter-listed key polls through the adapter: {listed}");
        assert_eq!(listed["result"]["envelopes"], json!([]), "{listed}");

        // ADMITTED with NO RECORD AT ALL — the charter's IDENTITY rung
        // (review F3): v1's line lists this key, so `verify_signed_request`
        // resolves the caller from the charter itself, exactly as it would at
        // the door. Nothing in this assertion touches the registry.
        aoide_storage::node_store::save_nodes(&[]).unwrap();
        let charter_resolved = answer(poll(&unique_nonce("adapter-charter-identity")));
        assert!(
            charter_resolved.get("error").is_none(),
            "a charter-listed key resolves with no paired record, through the adapter: {charter_resolved}"
        );
        assert_eq!(charter_resolved["result"]["envelopes"], json!([]), "{charter_resolved}");

        // Put the stale record back for the revocation half: the review N1
        // loophole is a PAIRED grant that must not come back in once the
        // charter stops listing the key.
        let mut stale = fixture_node("receiverbox", "http://10.0.0.5:8710/");
        stale.verified = true;
        stale.pubkey = Some(receiver_key.clone());
        stale.grants = aoide_storage::node_store::grants_in("home", &["message"]);
        aoide_storage::node_store::save_nodes(&[stale]).unwrap();

        // REVOKED: the operator signs v2 without this line, and the receiving
        // box accepts it. Same key, same stale paired grant, same listener.
        charter_machine(&root, "operator", "opbox");
        let v2 = format!("mesh = \"home\"\nversion = 1\nrelays = []\n\n[nodes]\n{op_line}\n");
        std::fs::write(aoide_storage::charter::source_path("home"), &v2).unwrap();
        aoide_storage::charter::sign("home", None).unwrap();
        let v2_bytes = std::fs::read(aoide_storage::charter::source_path("home")).unwrap();
        let v2_sig = std::fs::read(aoide_storage::charter::source_sig_path("home")).unwrap();
        charter_machine(&root, "receiver", "receiverbox");
        assert_eq!(
            aoide_storage::charter::accept(&v2_bytes, &v2_sig).unwrap().charter.version,
            2,
            "the revocation is the version now in force"
        );

        let revoked = answer(poll(&unique_nonce("adapter-charter-revoked")));
        assert_eq!(
            revoked["error"]["code"], -32010,
            "a removed line refuses on the next request, whatever paired grant the record still holds: {revoked}"
        );
        let taught = revoked["error"]["message"].as_str().unwrap().to_string();
        assert!(taught.contains("home"), "and names the mesh it was judged in: {revoked}");
        // F1: the teaching is the CHARTER's, not the local `node allow … on`
        // the generic arm emits — in a governed mesh that command answers
        // `widens-charter` and changes nothing, so it would send the operator
        // to a fix that cannot work.
        assert!(
            taught.contains("OPERATOR"),
            "the fix travels with the charter's operator, not this host: {revoked}"
        );
        assert!(
            !taught.contains("on this (polled) host run"),
            "and it does NOT teach the generic arm's `node allow … on` as the fix: {revoked}"
        );
        assert!(
            taught.contains("widens-charter"),
            "naming that local command only to say it cannot work here: {revoked}"
        );
        assert!(taught.contains("charter show"), "pointing at the read that shows the version: {revoked}");

        // With the stale record gone too, the key resolves NOWHERE at all —
        // fail-closed at the identity rung, still through the same listener.
        aoide_storage::node_store::save_nodes(&[]).unwrap();
        let unknown = answer(poll(&unique_nonce("adapter-charter-unknown")));
        assert_eq!(unknown["error"]["code"], -32007, "an unlisted, unpaired key resolves nobody: {unknown}");

        // The audit trail, DISCRIMINATINGLY (review F2 — the bare `contains`
        // pair this replaces was satisfied by the FIRST, ADMITTED poll alone,
        // so deleting the refusal's own audit line would not have failed it).
        let log = std::fs::read_to_string(&audit_log).unwrap_or_default();
        let lines: Vec<Value> = log
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str::<Value>(l).expect("an audit line is JSON"))
            .collect();
        let with = |command: &str| -> Vec<&Value> {
            lines.iter().filter(|v| v["command"] == command).collect()
        };
        // 1. The -32010 refusal is audited by `mail_poll` ITSELF, under its own
        //    label with the `unauthorized` status — the line that would vanish
        //    if the function stopped auditing.
        let poll_refusals: Vec<&Value> = with("a2a.aoide/mailPoll")
            .into_iter()
            .filter(|v| v["status"] == "unauthorized")
            .collect();
        assert_eq!(
            poll_refusals.len(),
            2,
            "TWO lines: the shared helper's own charter detail (which never reaches the caller) and \
             `mail_poll`'s refusal line (the taught text the caller read): {log}"
        );
        assert!(
            poll_refusals
                .iter()
                .any(|v| v["message"].as_str().is_some_and(|m| m.contains("charter-governed mesh"))),
            "the helper records which mesh state refused, and why: {log}"
        );
        assert!(
            poll_refusals
                .iter()
                .any(|v| v["message"].as_str().is_some_and(|m| m.contains("CHARTER mesh"))),
            "and the refusal itself is audited under the arm's own label: {log}"
        );
        // 2. Every ROUTED adapter line names its listener — the three answered
        //    polls (listed, charter-resolved, then refused).
        let routed: Vec<&Value> = with("a2a.aoide/mailPoll")
            .into_iter()
            .filter(|v| v["message"].as_str().is_some_and(|m| m.contains("via mail-adapter")))
            .collect();
        assert_eq!(
            routed.len(),
            3,
            "each ROUTED mailPoll on the adapter is tagged with its listener: {log}"
        );
        // 3. The identity-rung refusal (-32007) happens BEFORE routing, so it
        //    audits under `a2a.signed-request` — tagged by `audit_detail_with`,
        //    which is the other half of the review's finding.
        let pre_route: Vec<&Value> = with("a2a.signed-request")
            .into_iter()
            .filter(|v| v["message"].as_str().is_some_and(|m| m.contains("via mail-adapter")))
            .collect();
        assert_eq!(
            pre_route.len(),
            1,
            "the pre-route refusal names its listener too: {log}"
        );

        match saved_root {
            Some(v) => std::env::set_var("AOIDE_ROOT", v),
            None => std::env::remove_var("AOIDE_ROOT"),
        }
        match saved_state {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        match saved_config {
            Some(v) => std::env::set_var("AOIDE_CONFIG", v),
            None => std::env::remove_var("AOIDE_CONFIG"),
        }
        match saved_node {
            Some(v) => std::env::set_var("AOIDE_A2A_NODE_NAME", v),
            None => std::env::remove_var("AOIDE_A2A_NODE_NAME"),
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn every_non_mail_method_is_32601_on_the_adapter() {
        let ctx = RequestCtx::mail_adapter(Path::new("/dev/null"), ConnOrigin::Loopback, None);
        for method in [
            "message/send",
            "message/stream",
            "tasks/get",
            "tasks/resubscribe",
            "aoide/graphSummary",
            "aoide/pairRequest",
            "aoide/pairReveal",
            "aoide/pairPoll",
            "state",
            "events",
            "control",
            "credentials",
        ] {
            let resp = handle_mail_jsonrpc(&json!({ "jsonrpc": "2.0", "id": 7, "method": method }), &ctx);
            assert_eq!(resp["error"]["code"], -32601, "{method} must not exist on the adapter: {resp}");
            assert_eq!(resp["error"]["message"], format!("method not found: {method}"), "{resp}");
            assert_eq!(resp["id"], 7, "the id is echoed, error or not");
        }
    }

    #[test]
    fn the_adapter_shares_the_signed_request_path_so_an_unsigned_poll_is_refused_and_audited() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, saved) = adapter_root("unsigned-poll");
        let audit_log = root.join("log");

        let resp = handle_mail_jsonrpc(
            &json!({ "jsonrpc": "2.0", "id": 1, "method": "aoide/mailPoll", "params": { "node": "box-b" } }),
            &RequestCtx::mail_adapter(&audit_log, ConnOrigin::Loopback, None),
        );
        assert_eq!(resp["error"]["code"], -32010, "{resp}");
        let log = std::fs::read_to_string(&audit_log).unwrap_or_default();
        assert!(log.contains("a2a.aoide/mailPoll"), "the refusal is audited: {log}");
        assert!(log.contains("unauthorized"), "{log}");

        adapter_cleanup(&root, saved);
    }

    #[test]
    fn the_adapters_audit_detail_names_the_listener_and_the_origin_and_the_doors_does_not_change() {
        assert_eq!(Listener::A2a.audit_detail(200, ConnOrigin::Loopback), "HTTP 200");
        assert_eq!(
            Listener::Mail.audit_detail(200, ConnOrigin::Loopback),
            "HTTP 200 from loopback via mail-adapter"
        );
        assert_eq!(
            Listener::Mail.audit_detail(200, ConnOrigin::Remote("10.0.0.5".parse().unwrap())),
            "HTTP 200 from remote 10.0.0.5 via mail-adapter"
        );
        assert_eq!(Listener::Mail.audit_detail(200, ConnOrigin::Unknown), "HTTP 200 from unknown via mail-adapter");
    }

    /// A raw HTTP/1.1 request as bytes, with the four signed headers when a
    /// key is given — the adapter's socket tests drive the REAL parse +
    /// verify + route path rather than calling the pieces. `mesh` names the
    /// mesh in the signature (P-CHARTER); `None` is a request that names none,
    /// which is judged in this box's home mesh.
    fn raw_request(
        host: &str,
        method: &str,
        path: &str,
        body: &[u8],
        signed: Option<(&aoide_storage::identity::Keypair, &str, i64, &str)>,
        mesh: Option<&str>,
    ) -> Vec<u8> {
        let mut head = format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\n");
        if let Some((kp, node, ts, nonce)) = signed {
            let timestamp = aoide_storage::time::iso_utc_from_epoch(ts);
            let canonical = aoide_storage::wire_auth::canonical_string(method, path, &timestamp, nonce, body, mesh);
            let signature = aoide_storage::wire_auth::sign_hex(kp, canonical.as_bytes());
            for (name, value) in [
                (aoide_storage::wire_auth::HEADER_NODE, node.to_string()),
                (aoide_storage::wire_auth::HEADER_TIMESTAMP, timestamp),
                (aoide_storage::wire_auth::HEADER_NONCE, nonce.to_string()),
                (aoide_storage::wire_auth::HEADER_SIGNATURE, signature),
            ] {
                head.push_str(&format!("{name}: {value}\r\n"));
            }
            // P-CHARTER: the mesh rides its own header, and the canonical
            // string above was built WITH it — a signature that names a mesh
            // the headers do not carry verifies as tampered.
            if let Some(mesh) = mesh {
                head.push_str(&format!("{}: {mesh}\r\n", aoide_storage::wire_auth::HEADER_MESH));
            }
        }
        head.push_str(&format!("Content-Length: {}\r\nConnection: close\r\n\r\n", body.len()));
        let mut bytes = head.into_bytes();
        bytes.extend_from_slice(body);
        bytes
    }

    /// Send one raw request to a live listener and return the whole response.
    fn send_raw(port: u16, request: &[u8]) -> String {
        use std::io::{Read, Write};
        let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.write_all(request).unwrap();
        let mut out = Vec::new();
        stream.read_to_end(&mut out).unwrap();
        String::from_utf8_lossy(&out).to_string()
    }

    /// A free loopback port (bind `:0`, read it back, drop) — the reuse race
    /// is the same one every other loopback fixture in this tree accepts.
    fn free_loopback_port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
    }

    /// Raise the adapter's serve loop, `serve_mail_from`, on a background thread and
    /// wait until it answers a connect. Its start is in the past, so a request signed
    /// at once is admitted; the public `serve_mail` stamps its own start, and its
    /// test waits that out. The thread lives for the rest of the test process, which
    /// is what `serve`'s own loop does by construction. The registry is leaked
    /// because `serve_mail_from` takes it `&'static`, exactly as `a2a serve` does.
    /// `audit_log` is the one the adapter writes through.
    fn raise_mail_adapter(port: u16, audit_log: &Path) {
        let audit_log = audit_log.to_path_buf();
        std::thread::spawn(move || {
            let _ = serve_mail_from(port, &audit_log, Box::leak(Box::new(full_card_registry())), 0);
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
                return;
            }
            assert!(std::time::Instant::now() < deadline, "the adapter never came up on 127.0.0.1:{port}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn the_adapter_serves_over_its_own_socket_ignores_the_host_header_and_binds_loopback_only() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, saved) = adapter_root("socket");
        let port = free_loopback_port();
        raise_mail_adapter(port, Path::new("/dev/null"));

        // A foreign Host — through cloudflared this is the tunnel hostname —
        // is never consulted: the card still answers.
        let card = send_raw(port, &raw_request("aoide.necoconeco.net", "GET", "/.well-known/agent-card.json", b"", None, None));
        assert!(card.starts_with("HTTP/1.1 200 OK"), "{card}");
        let body = card.split("\r\n\r\n").nth(1).unwrap();
        let card_value: Value = serde_json::from_str(body).unwrap();
        let keys: Vec<&str> = card_value.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, vec!["name", "protocolVersion", "url"], "{card_value}");

        // The same foreign Host on a mail POST reaches the mail arm.
        let poll = send_raw(
            port,
            &raw_request(
                "aoide.necoconeco.net",
                "POST",
                "/",
                br#"{"jsonrpc":"2.0","id":1,"method":"aoide/mailPoll","params":{"node":"box-b"}}"#,
                None,
                None,
            ),
        );
        assert!(poll.starts_with("HTTP/1.1 200 OK"), "{poll}");
        let poll_body: Value = serde_json::from_str(poll.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(poll_body["error"]["code"], -32010, "{poll_body}");

        // A door method over the same socket is refused by NAME — never
        // reached, never routed.
        let send = send_raw(
            port,
            &raw_request(
                "aoide.necoconeco.net",
                "POST",
                "/",
                br#"{"jsonrpc":"2.0","id":2,"method":"message/send","params":{"id":"x","message":{"text":"hi"}}}"#,
                None,
                None,
            ),
        );
        let send_body: Value = serde_json::from_str(send.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(send_body["error"]["code"], -32601, "{send_body}");

        // The bind is `127.0.0.1` ONLY: the adapter's own port is not
        // reachable at this host's routable address. Skip the negative when
        // this box has no non-loopback IPv4 to try (a hermetic sandbox may
        // not) — the positive half above plus the bind constant still hold.
        if let Some(ip) = first_non_loopback_ipv4() {
            let refused = std::net::TcpStream::connect_timeout(
                &std::net::SocketAddr::from((ip, port)),
                Duration::from_millis(500),
            );
            assert!(refused.is_err(), "the adapter must not answer on {ip}:{port} — it binds loopback only");
        }

        adapter_cleanup(&root, saved);
    }

    /// This host's first non-loopback IPv4, if it has one — the negative half
    /// of the bind assertion. A box with only loopback returns `None` and the
    /// test says so by skipping it.
    fn first_non_loopback_ipv4() -> Option<std::net::Ipv4Addr> {
        let probe = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
        probe.connect("192.0.2.1:9").ok()?; // TEST-NET-1: no packets leave
        match probe.local_addr().ok()?.ip() {
            std::net::IpAddr::V4(v4) if !v4.is_loopback() => Some(v4),
            _ => None,
        }
    }

    #[test]
    fn the_adapters_early_paths_tag_their_audit_lines_with_the_listener() {
        // F2: a malformed request and a refused signature are the two EARLY
        // exits from `handle_connection`, and they used to write untagged
        // details — exactly the lines an operator wants to attribute when a
        // hostile flood hits the tunnel rather than the LAN.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, saved) = adapter_root("early-audit");
        let audit_log = root.join("log");
        let port = free_loopback_port();
        raise_mail_adapter(port, &audit_log);

        // A malformed request line — a 400 with no JSON-RPC method to label.
        let bad = send_raw(port, b"NOT-HTTP\r\n\r\n");
        assert!(bad.starts_with("HTTP/1.1 400"), "{bad}");

        // A signed request whose signature is garbage — refused before any
        // dispatch, the fail-closed `-32007` path.
        let kp = setup_signed_node_with_allows("box-b", &["message"]);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let mut forged = raw_request(
            "127.0.0.1",
            "POST",
            "/",
            br#"{"jsonrpc":"2.0","id":1,"method":"aoide/mailPoll","params":{"node":"box-b"}}"#,
            Some((&kp, "box-b", now, &unique_nonce("early-audit"))),
            None,
        );
        let text = String::from_utf8_lossy(&forged).to_string();
        let tampered = text.replace("X-Aoide-Signature: ", "X-Aoide-Signature: 00");
        forged = tampered.into_bytes();
        let refused = send_raw(port, &forged);
        let refused_body: Value = serde_json::from_str(refused.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(refused_body["error"]["code"], -32007, "{refused_body}");

        let log = std::fs::read_to_string(&audit_log).unwrap_or_default();
        let bad_line = log.lines().find(|l| l.contains("a2a.bad-request")).unwrap_or_default();
        assert!(bad_line.contains("via mail-adapter"), "the parse failure names its listener: {bad_line}");
        assert!(bad_line.contains("from loopback"), "{bad_line}");
        let sig_line = log.lines().find(|l| l.contains("a2a.signed-request")).unwrap_or_default();
        assert!(sig_line.contains("via mail-adapter"), "the signature refusal names its listener: {sig_line}");

        // The DOOR's bytes are unchanged on both paths — the tag is the
        // adapter's alone.
        assert_eq!(Listener::A2a.audit_detail(400, ConnOrigin::Loopback), "HTTP 400");
        assert_eq!(Listener::A2a.audit_detail_with("boom", ConnOrigin::Loopback), "boom");
        assert_eq!(Listener::Mail.audit_detail(400, ConnOrigin::Loopback), "HTTP 400 from loopback via mail-adapter");
        assert_eq!(
            Listener::Mail.audit_detail_with("boom", ConnOrigin::Unknown),
            "boom (from unknown via mail-adapter)"
        );

        adapter_cleanup(&root, saved);
    }

    fn bad_request_lines(audit_log: &Path) -> usize {
        std::fs::read_to_string(audit_log).unwrap_or_default().lines().filter(|l| l.contains("a2a.bad-request")).count()
    }

    #[test]
    fn a_connection_that_closes_before_sending_a_byte_is_idle_not_a_bad_request() {
        let empty = parse_http_request(&mut std::io::Cursor::new(Vec::<u8>::new()), Instant::now()).unwrap_err();
        assert!(empty.idle, "EOF before the request line is an idle probe: {empty:?}");
        let garbage = parse_http_request(&mut std::io::Cursor::new(b"NOT-HTTP\r\n\r\n".to_vec()), Instant::now()).unwrap_err();
        assert!(!garbage.idle, "{garbage:?}");
        let partial = parse_http_request(&mut std::io::Cursor::new(b"GET".to_vec()), Instant::now()).unwrap_err();
        assert!(!partial.idle, "bytes arrived, so it is a malformed request: {partial:?}");
    }

    #[test]
    fn a_connect_and_drop_probe_writes_no_response_and_no_audit_line() {
        use std::io::{Read, Write};
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, saved) = adapter_root("idle-probe");
        let audit_log = root.join("log");
        let port = free_loopback_port();
        raise_mail_adapter(port, &audit_log);

        // The reuse probe of a client's ssh-tunnel: connect, send nothing, close.
        for _ in 0..3 {
            drop(std::net::TcpStream::connect(("127.0.0.1", port)).unwrap());
        }
        // A later, valid request orders the probes before the read of the log.
        let ok = send_raw(port, b"GET /nothing HTTP/1.1\r\nHost: x\r\n\r\n");
        assert!(!ok.starts_with("HTTP/1.1 400"), "{ok}");
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(bad_request_lines(&audit_log), 0, "a probe that sent nothing is not a bad request");

        // Control: bytes that stop mid request line are still a 400 and one audit line.
        let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.write_all(b"GET").unwrap();
        stream.shutdown(std::net::Shutdown::Write).unwrap();
        let mut out = Vec::new();
        stream.read_to_end(&mut out).unwrap();
        assert!(String::from_utf8_lossy(&out).starts_with("HTTP/1.1 400"), "{}", String::from_utf8_lossy(&out));
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(bad_request_lines(&audit_log), 1);

        adapter_cleanup(&root, saved);
    }

    #[test]
    fn a_non_mail_method_on_the_adapter_audits_under_its_own_label() {
        // F5: the adapter used to label a door method name it cannot serve.
        // A log reader scanning for `message/send` must find the attempt —
        // marked as refused by a listener with no such method.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, saved) = adapter_root("refused-label");
        let audit_log = root.join("log");
        let registry = full_card_registry();

        let (status, body, label) = route_mail(
            &mail_req(
                "POST",
                "/",
                br#"{"jsonrpc":"2.0","id":1,"method":"message/send","params":{"id":"x","message":{"text":"hi"}}}"#,
            ),
            MAIL_ADAPTER_BIND,
            MAIL_ADAPTER_PORT_DEFAULT,
            &audit_log,
            ConnOrigin::Loopback,
            &registry,
            None,
        );
        assert_eq!(status, 200);
        assert_eq!(label, "a2a.mail-adapter.refused message/send", "the door's method name is never the label");
        let body_val: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body_val["error"]["code"], -32601, "{body_val}");

        // An unknown (or unparsable) method collapses rather than
        // interpolating attacker-controlled text into the label.
        let (_, _, unknown) = route_mail(
            &mail_req("POST", "/", br#"{"jsonrpc":"2.0","id":1,"method":"../etc/passwd"}"#),
            MAIL_ADAPTER_BIND,
            MAIL_ADAPTER_PORT_DEFAULT,
            &audit_log,
            ConnOrigin::Loopback,
            &registry,
            None,
        );
        assert_eq!(unknown, "a2a.mail-adapter.refused", "a hostile method name is not interpolated");
        assert_eq!(mail_method_label(None), "mail-adapter.refused");
        assert_eq!(mail_method_label(Some("aoide/mailPoll")), "aoide/mailPoll");
        for method in ["message/stream", "tasks/resubscribe", "tasks/cancel"] {
            assert_eq!(
                mail_method_label(Some(method)),
                format!("mail-adapter.refused {method}"),
                "every refused door method keeps its name in the audit line"
            );
        }

        // And it is what actually lands in the log, over a real socket.
        let port = free_loopback_port();
        raise_mail_adapter(port, &audit_log);
        let sent = send_raw(
            port,
            &raw_request(
                "aoide.necoconeco.net",
                "POST",
                "/",
                br#"{"jsonrpc":"2.0","id":2,"method":"aoide/graphSummary"}"#,
                None,
                None,
            ),
        );
        assert!(sent.starts_with("HTTP/1.1 200"), "{sent}");
        let log = std::fs::read_to_string(&audit_log).unwrap_or_default();
        let line = log
            .lines()
            .find(|l| l.contains("mail-adapter.refused"))
            .unwrap_or_default();
        assert!(
            line.contains("a2a.mail-adapter.refused aoide/graphSummary"),
            "the graphSummary attempt is named as refused BY THE ADAPTER: {line}"
        );
        assert!(
            !log.lines().any(|l| l.contains(r#""command":"a2a.aoide/graphSummary""#)),
            "no door method label is emitted by the adapter: {log}"
        );

        adapter_cleanup(&root, saved);
    }

    #[test]
    fn a_replayed_signed_request_is_refused_over_the_adapter_socket_too() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, saved) = adapter_root("replay");
        let port = free_loopback_port();
        raise_mail_adapter(port, Path::new("/dev/null"));

        let kp = setup_signed_node_with_allows("box-b", &["message"]);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let nonce = unique_nonce("adapter-replay");
        let body = br#"{"jsonrpc":"2.0","id":1,"method":"aoide/mailPoll","params":{"node":"box-b"}}"#;

        let first = send_raw(port, &raw_request("127.0.0.1", "POST", "/", body, Some((&kp, "box-b", now, &nonce)), None));
        let first_body: Value = serde_json::from_str(first.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert!(first_body.get("error").is_none(), "the first signed request must be admitted: {first_body}");
        assert_eq!(first_body["result"]["envelopes"], json!([]), "{first_body}");

        let second = send_raw(port, &raw_request("127.0.0.1", "POST", "/", body, Some((&kp, "box-b", now, &nonce)), None));
        let second_body: Value = serde_json::from_str(second.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(second_body["error"]["code"], -32009, "the same nonce twice is a replay: {second_body}");

        adapter_cleanup(&root, saved);
    }

    /// One signed `POST /` from `box-b` to a live door, answered as JSON.
    fn signed_post(port: u16, kp: &aoide_storage::identity::Keypair, body: &[u8]) -> Value {
        let nonce = unique_nonce("aud");
        let raw = send_raw(port, &raw_request("127.0.0.1", "POST", "/", body, Some((kp, "box-b", epoch_now(), &nonce)), None));
        serde_json::from_str(raw.split("\r\n\r\n").nth(1).unwrap()).unwrap()
    }

    #[test]
    fn a_signed_request_for_another_door_is_refused_here_though_this_door_trusts_the_signer() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, saved) = adapter_root("audience-wrong-to");
        let audit = root.join("log");
        let mail_port = free_loopback_port();
        raise_mail_adapter(mail_port, &audit);
        let a2a_port = free_loopback_port();
        raise_a2a_door(a2a_port, &audit);

        // The signer is this process's own identity, registered as a verified
        // node: this door trusts it. `to` names some OTHER door's key, the J of
        // the replay, so the frame is refused at both doors.
        let kp = setup_signed_node_with_allows("box-b", &["message"]);
        let j = aoide_storage::identity::mint_ephemeral().unwrap().info().pubkey_hex;
        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "aoide/mailPoll", "params": { "node": "box-b", "to": j } });
        for port in [mail_port, a2a_port] {
            let answer = signed_post(port, &kp, body.to_string().as_bytes());
            assert_eq!(answer["error"]["code"], -32012, "wrong-audience at port {port}: {answer}");
            assert!(answer["error"]["message"].as_str().unwrap_or_default().contains("wrong-audience"), "{answer}");
        }

        adapter_cleanup(&root, saved);
    }

    #[test]
    fn an_untagged_call_to_a_method_off_the_allow_list_is_refused_with_the_upgrade_line() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, saved) = adapter_root("audience-untagged-refused");
        let audit = root.join("log");
        let port = free_loopback_port();
        raise_a2a_door(port, &audit);
        let kp = setup_signed_node("box-b");

        let bodies = [
            json!({ "jsonrpc": "2.0", "id": 1, "method": "aoide/graphSummary", "params": {} }),
            json!({ "jsonrpc": "2.0", "id": 1, "method": "tasks/get", "params": { "id": "no-such-task" } }),
            json!({ "jsonrpc": "2.0", "id": 1, "method": "message/send", "params": { "message": {
                "role": "user", "messageId": "m-spawn", "parts": [{ "kind": "text", "text": "hi" }] } } }),
            json!({ "jsonrpc": "2.0", "id": 1, "method": "message/send", "params": { "message": {
                "role": "user", "messageId": "m-inject", "contextId": "ctx-1", "parts": [{ "kind": "text", "text": "hi" }] } } }),
        ];
        for body in bodies {
            let answer = signed_post(port, &kp, body.to_string().as_bytes());
            assert_eq!(answer["error"]["code"], -32012, "{body} -> {answer}");
            assert!(answer["error"]["message"].as_str().unwrap_or_default().contains("upgrade"), "{answer}");
        }

        adapter_cleanup(&root, saved);
    }

    #[test]
    fn an_untagged_mail_poll_is_dispatched_and_audited_as_untagged() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, saved) = adapter_root("audience-untagged-poll");
        let audit = root.join("log");
        let port = free_loopback_port();
        raise_mail_adapter(port, &audit);
        let kp = setup_signed_node_with_allows("box-b", &["message"]);

        let answer = signed_post(port, &kp, br#"{"jsonrpc":"2.0","id":1,"method":"aoide/mailPoll","params":{"node":"box-b"}}"#);
        assert!(answer.get("error").is_none(), "an old client's mailPoll is still admitted: {answer}");
        let log = std::fs::read_to_string(&audit).unwrap_or_default();
        assert!(
            log.lines().any(|l| l.contains(r#""status":"untagged""#) && l.contains("aoide/mailPoll")),
            "the untagged call leaves its own audit line: {log}"
        );

        adapter_cleanup(&root, saved);
    }

    #[test]
    fn a_call_addressed_to_this_door_is_not_refused_by_the_audience_step() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, saved) = adapter_root("audience-tagged-ok");
        let audit = root.join("log");
        let port = free_loopback_port();
        raise_a2a_door(port, &audit);
        let kp = setup_signed_node_with_allows("box-b", &["message", "read"]);
        let own = kp.info().pubkey_hex;

        let bodies = [
            json!({ "jsonrpc": "2.0", "id": 1, "method": "aoide/graphSummary", "params": { "to": own } }),
            json!({ "jsonrpc": "2.0", "id": 1, "method": "tasks/get", "params": { "id": "no-such-task", "to": own } }),
            json!({ "jsonrpc": "2.0", "id": 1, "method": "message/send", "params": { "to": own, "message": {
                "role": "user", "messageId": "m-tagged", "parts": [{ "kind": "text", "text": "hi" }] } } }),
            json!({ "jsonrpc": "2.0", "id": 1, "method": "aoide/mailPoll", "params": { "node": "box-b", "to": own } }),
        ];
        for body in bodies {
            let answer = signed_post(port, &kp, body.to_string().as_bytes());
            assert_ne!(answer["error"]["code"], -32012, "a call to this door is admitted: {body} -> {answer}");
        }

        adapter_cleanup(&root, saved);
    }

    /// A signed frame addressed to another door records nothing: the same nonce,
    /// addressed right, is still fresh and admitted once. The refusal is taken before
    /// the nonce step, so only the admitted frame takes the nonce.
    #[test]
    fn a_frame_addressed_to_another_door_records_nothing_so_the_same_nonce_admits_once() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, saved) = adapter_root("wrong-to-records-nothing");
        let audit = root.join("log");
        let port = free_loopback_port();
        raise_mail_adapter(port, &audit);
        let kp = setup_signed_node_with_allows("box-b", &["message"]);
        let own = kp.info().pubkey_hex.clone();
        let elsewhere = aoide_storage::identity::mint_ephemeral().unwrap().info().pubkey_hex;
        let nonce = unique_nonce("wrong-to");
        let send = |to: &str| {
            let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "aoide/mailPoll", "params": { "node": "box-b", "to": to } }).to_string();
            let request = raw_request("127.0.0.1", "POST", "/", body.as_bytes(), Some((&kp, "box-b", epoch_now(), &nonce)), None);
            let answer = send_raw(port, &request);
            serde_json::from_str::<Value>(answer.split_once("\r\n\r\n").unwrap().1).unwrap()
        };

        let refused = send(elsewhere.as_str());
        assert_eq!(refused["error"]["code"], -32012, "{refused}");
        let admitted = send(own.as_str());
        assert!(admitted.get("error").is_none(), "the same nonce, addressed to this door, is still fresh: {admitted}");

        adapter_cleanup(&root, saved);
    }

    /// The unloadable-declaration arm judges a frame the way every other signed frame
    /// is judged: a frame it refuses for its audience records nothing, so the same
    /// nonce, addressed right, still reaches the config-invalid answer.
    #[test]
    fn the_unloadable_arm_refuses_a_wrong_to_and_records_nothing() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = aoide_test_support::EnvSaver::capture(&[
            "AOIDE_ROOT",
            "STRILLIX_ROOT",
            "AOIDE_STATE_DIR",
            "AOIDE_STAGE_DIR",
            "AOIDE_CONFIG",
            "AOIDE_A2A_NODE_NAME",
        ]);
        let root = aoide_test_support::unique_tmp("arm-wrong-to");
        charter_machine(&root, "receiver", "receiverbox");
        let line = aoide_storage::charter::node_line().unwrap();
        let in_force = aoide_storage::charter::in_force_path("home");
        std::fs::create_dir_all(in_force.parent().unwrap()).unwrap();
        std::fs::write(&in_force, format!("mesh = \"home\"\nversion = 0\nrelays = []\n\n[nodes]\n{line}\n")).unwrap();
        std::fs::write(root.join("receiver").join("config.toml"), "[mesh.home.status]\nnobody = \"down\"\n").unwrap();

        let kp = aoide_storage::identity::load_or_mint().unwrap().0;
        let own = kp.info().pubkey_hex.clone();
        let elsewhere = aoide_storage::identity::mint_ephemeral().unwrap().info().pubkey_hex;
        let port = free_loopback_port();
        raise_mail_adapter(port, &root.join("receiver").join("log"));
        let nonce = unique_nonce("arm-wrong-to");
        let send = |to: &str| {
            let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "aoide/mailPoll", "params": { "node": "box-b", "to": to } }).to_string();
            let request = raw_request("127.0.0.1", "POST", "/", body.as_bytes(), Some((&kp, "receiverbox", epoch_now(), &nonce)), Some("home"));
            let answer = send_raw(port, &request);
            serde_json::from_str::<Value>(answer.split_once("\r\n\r\n").unwrap().1).unwrap()
        };

        let refused = send(elsewhere.as_str());
        assert_eq!(refused["error"]["code"], -32012, "a frame for another door is refused: {refused}");
        let answered = send(own.as_str());
        assert_eq!(answered["result"]["reason"], "config-invalid", "the same nonce, addressed here, is still fresh: {answered}");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A signed JSON array is not a call. It is refused before its nonce is recorded,
    /// so the same nonce on one call is still fresh, and no method runs for the array.
    #[test]
    fn a_signed_json_array_is_refused_before_it_records_and_reaches_no_method() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, saved) = adapter_root("array-body");
        let audit = root.join("log");
        let mail_port = free_loopback_port();
        raise_mail_adapter(mail_port, &audit);
        let a2a_port = free_loopback_port();
        raise_a2a_door(a2a_port, &audit);
        let kp = setup_signed_node_with_allows("box-b", &["message"]);
        let batch: &[u8] = br#"[{"jsonrpc":"2.0","id":1,"method":"aoide/mailPoll","params":{"node":"box-b"}}]"#;
        let call: &[u8] = br#"{"jsonrpc":"2.0","id":1,"method":"aoide/mailPoll","params":{"node":"box-b"}}"#;
        for (port, code) in [(a2a_port, -32600_i64), (mail_port, -32601_i64)] {
            let nonce = unique_nonce("array-body");
            let refused = send_raw(port, &raw_request("127.0.0.1", "POST", "/", batch, Some((&kp, "box-b", epoch_now(), &nonce)), None));
            let refused_body: Value = serde_json::from_str(refused.split_once("\r\n\r\n").unwrap().1).unwrap();
            assert_eq!(refused_body["error"]["code"], code, "a batch is not a call, at port {port}: {refused_body}");
            let admitted = send_raw(port, &raw_request("127.0.0.1", "POST", "/", call, Some((&kp, "box-b", epoch_now(), &nonce)), None));
            let admitted_body: Value = serde_json::from_str(admitted.split_once("\r\n\r\n").unwrap().1).unwrap();
            assert!(admitted_body.get("error").is_none(), "the same nonce on a call is still fresh at port {port}: {admitted_body}");
        }
        adapter_cleanup(&root, saved);
    }

    /// The audience step answers a body it cannot read as its listener always did,
    /// and admits an untagged mail method as untagged.
    #[test]
    fn audience_answers_an_unreadable_body_as_its_listener_always_did() {
        let own = "ab".repeat(32);
        let not_calls: [&[u8]; 4] = [b"[]", b"42", br#"{"params":{}}"#, br#"{"method":5}"#];
        for body in not_calls {
            assert_eq!(audience(Listener::A2a, body, Some(own.as_str())).unwrap_err().0, -32600, "{body:?}");
            assert_eq!(audience(Listener::Mail, body, Some(own.as_str())).unwrap_err().0, -32601, "{body:?}");
        }
        for listener in [Listener::A2a, Listener::Mail] {
            assert_eq!(audience(listener, b"not json", Some(own.as_str())).unwrap_err().0, -32700);
        }
        assert_eq!(
            audience(Listener::A2a, UNTAGGED_POLL, None),
            Ok(Audience::Untagged { method: "aoide/mailPoll".to_string() })
        );
    }

    /// The start a door names in its own start refusal (`-32008`).
    fn start_named_in(message: &str) -> i64 {
        let named = message.split("this door's start (`").nth(1).and_then(|rest| rest.split('`').next()).unwrap_or_default();
        aoide_storage::time::parse_iso_utc(named).unwrap_or_else(|| panic!("the refusal names the door's start: {message}"))
    }

    /// One signed `POST /` stamped `ts`, answered as JSON.
    fn signed_post_at(port: u16, kp: &aoide_storage::identity::Keypair, body: &[u8], ts: i64) -> Value {
        let nonce = unique_nonce("start-bound");
        let raw = send_raw(port, &raw_request("127.0.0.1", "POST", "/", body, Some((kp, "box-b", ts, &nonce)), None));
        serde_json::from_str(raw.split_once("\r\n\r\n").unwrap().1).unwrap()
    }

    /// Wait until something answers a connect on `port`, five seconds at most.
    fn wait_for_listener(port: u16) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::net::TcpStream::connect(("127.0.0.1", port)).is_err() {
            assert!(std::time::Instant::now() < deadline, "nothing listens on 127.0.0.1:{port}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Wait until the wall clock reads `second` or later.
    fn wait_until_clock(second: i64) {
        while epoch_now() < second {
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// The real `serve` stamps its own start and holds the start bound. A probe
    /// stamped at or before the start is refused and names the start the door took;
    /// a frame stamped at start + 5s is refused; a frame stamped at start + 6s, sent
    /// once the clock is within its ahead bound, is admitted.
    #[test]
    fn the_real_serve_refuses_a_frame_at_its_start_bound_and_admits_one_after_it() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, saved) = adapter_root("real-serve");
        let port = free_loopback_port();
        let audit = root.join("log");
        let secrets = root.join("secrets.sock");
        let kp = setup_signed_node_with_allows("box-b", &["message"]);
        let before = epoch_now();
        let registry: &'static Registry = Box::leak(Box::new(full_card_registry()));
        let (serve_audit, serve_secrets) = (audit.clone(), secrets.clone());
        std::thread::spawn(move || {
            let _ = serve("127.0.0.1", port, &serve_audit, "", "", "box-a", "", "", &serve_secrets, false, registry);
        });
        wait_for_listener(port);

        let probe = signed_post_at(port, &kp, UNTAGGED_POLL, before);
        assert_eq!(probe["error"]["code"], -32008, "a frame stamped at or before the start is refused: {probe}");
        let start = start_named_in(probe["error"]["message"].as_str().unwrap_or_default());
        let at_bound = signed_post_at(port, &kp, UNTAGGED_POLL, start + 5);
        assert_eq!(at_bound["error"]["code"], -32008, "a frame stamped at start + 5s is refused: {at_bound}");
        wait_until_clock(start + 1);
        let after = signed_post_at(port, &kp, UNTAGGED_POLL, start + 6);
        assert!(after.get("error").is_none(), "a frame stamped at start + 6s is admitted: {after}");

        adapter_cleanup(&root, saved);
    }

    /// The same bound on the real `serve_mail`: the adapter stamps its own start.
    #[test]
    fn the_real_serve_mail_refuses_a_frame_at_its_start_bound_and_admits_one_after_it() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, saved) = adapter_root("real-serve-mail");
        let port = free_loopback_port();
        let kp = setup_signed_node_with_allows("box-b", &["message"]);
        let before = epoch_now();
        let registry: &'static Registry = Box::leak(Box::new(full_card_registry()));
        std::thread::spawn(move || {
            let _ = serve_mail(port, Path::new("/dev/null"), registry);
        });
        wait_for_listener(port);

        let probe = signed_post_at(port, &kp, UNTAGGED_POLL, before);
        assert_eq!(probe["error"]["code"], -32008, "a frame stamped at or before the start is refused: {probe}");
        let start = start_named_in(probe["error"]["message"].as_str().unwrap_or_default());
        let at_bound = signed_post_at(port, &kp, UNTAGGED_POLL, start + 5);
        assert_eq!(at_bound["error"]["code"], -32008, "a frame stamped at start + 5s is refused: {at_bound}");
        wait_until_clock(start + 1);
        let after = signed_post_at(port, &kp, UNTAGGED_POLL, start + 6);
        assert!(after.get("error").is_none(), "a frame stamped at start + 6s is admitted: {after}");

        adapter_cleanup(&root, saved);
    }

    /// The A2A door's own per-connection handler on a background thread: the
    /// same `handle_connection` `serve` runs for each accepted socket. The
    /// listener is bound before the thread starts, so a connect cannot race it.
    fn raise_a2a_door(port: u16, audit_log: &Path) {
        let listener = std::net::TcpListener::bind(("127.0.0.1", port)).unwrap();
        let audit_log = audit_log.to_path_buf();
        std::thread::spawn(move || {
            let registry: &'static Registry = Box::leak(Box::new(full_card_registry()));
            for stream in listener.incoming().flatten() {
                let audit_log = audit_log.clone();
                std::thread::spawn(move || {
                    let _ = handle_connection(stream, "127.0.0.1", port, &audit_log, "", "", "box-a", None, Listener::A2a, registry, 0);
                });
            }
        });
    }

    fn epoch_now() -> i64 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
    }

    /// The check every signed-answer socket test shares: the answer is HTTP
    /// 200 and carries `X-Aoide-Answer-Signature`, and that signature verifies
    /// under `key` for the request it answers. A stranger's key, and another
    /// request's signature, both fail it.
    fn assert_answer_signed_for(request: &[u8], answer: &str, key: &aoide_storage::identity::Keypair) {
        use aoide_storage::wire_auth::{verify_answer, HEADER_ANSWER_SIGNATURE, HEADER_NONCE, HEADER_SIGNATURE};
        use aoide_test_support::raw_header;

        let request = String::from_utf8_lossy(request).to_string();
        let request_signature = raw_header(&request, HEADER_SIGNATURE).unwrap();
        let request_nonce = raw_header(&request, HEADER_NONCE).unwrap();
        assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
        let answer_body = answer.split_once("\r\n\r\n").unwrap().1;
        let answer_signature = raw_header(answer, HEADER_ANSWER_SIGNATURE).expect("a verified request's answer is signed");

        let door = key.info().pubkey_hex;
        assert!(
            verify_answer(&door, &request_signature, &request_nonce, 200, answer_body.as_bytes(), &answer_signature),
            "the door's own key verifies its answer"
        );
        let stranger = aoide_storage::identity::mint_ephemeral().unwrap().info().pubkey_hex;
        assert!(!verify_answer(&stranger, &request_signature, &request_nonce, 200, answer_body.as_bytes(), &answer_signature));
        assert!(
            !verify_answer(&door, &"00".repeat(64), &request_nonce, 200, answer_body.as_bytes(), &answer_signature),
            "another request's signature does not verify it"
        );
    }

    #[test]
    fn a_verified_mail_request_is_answered_signed_by_the_door_key() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, saved) = adapter_root("answer-signed");
        let port = free_loopback_port();
        raise_mail_adapter(port, Path::new("/dev/null"));

        let kp = setup_signed_node_with_allows("box-b", &["message"]);
        let body = br#"{"jsonrpc":"2.0","id":1,"method":"aoide/mailPoll","params":{"node":"box-b"}}"#;
        let request = raw_request("127.0.0.1", "POST", "/", body, Some((&kp, "box-b", epoch_now(), &unique_nonce("answer-signed"))), None);
        let answer = send_raw(port, &request);

        assert_answer_signed_for(&request, &answer, &kp);
        adapter_cleanup(&root, saved);
    }

    #[test]
    fn a_verified_request_is_answered_signed_on_the_a2a_door_too() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, saved) = adapter_root("answer-signed-a2a");
        let port = free_loopback_port();
        raise_a2a_door(port, Path::new("/dev/null"));

        let kp = setup_signed_node_with_allows("box-b", &["message"]);
        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "aoide/no-such-method", "params": { "to": kp.info().pubkey_hex } }).to_string();
        let request = raw_request("127.0.0.1", "POST", "/", body.as_bytes(), Some((&kp, "box-b", epoch_now(), &unique_nonce("a2a-answer-signed"))), None);
        let answer = send_raw(port, &request);

        let answer_body: Value = serde_json::from_str(answer.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert!(answer_body.get("error").is_some(), "an unknown method is an error answer, and it is still signed: {answer}");
        assert_answer_signed_for(&request, &answer, &kp);
        adapter_cleanup(&root, saved);
    }

    #[test]
    fn a_tampered_request_gets_an_unsigned_refusal() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, saved) = adapter_root("answer-tampered");
        let port = free_loopback_port();
        raise_mail_adapter(port, Path::new("/dev/null"));

        let kp = setup_signed_node_with_allows("box-b", &["message"]);
        let body = br#"{"jsonrpc":"2.0","id":1,"method":"aoide/mailPoll","params":{"node":"box-b"}}"#;
        let mut request = raw_request("127.0.0.1", "POST", "/", body, Some((&kp, "box-b", epoch_now(), &unique_nonce("answer-tampered"))), None);
        // The same length, with one byte of the body the signature covers changed.
        let tampered = String::from_utf8(body.to_vec()).unwrap().replace("\"id\":1", "\"id\":9");
        request.truncate(request.len() - body.len());
        request.extend_from_slice(tampered.as_bytes());
        let answer = send_raw(port, &request);

        let answer_body: Value = serde_json::from_str(answer.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(answer_body["error"]["code"], -32007, "{answer_body}");
        assert_eq!(aoide_test_support::raw_header(&answer, aoide_storage::wire_auth::HEADER_ANSWER_SIGNATURE), None, "{answer}");
        adapter_cleanup(&root, saved);
    }

    #[test]
    fn a_replayed_request_gets_an_unsigned_refusal() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, saved) = adapter_root("answer-replayed");
        let port = free_loopback_port();
        raise_mail_adapter(port, Path::new("/dev/null"));

        let kp = setup_signed_node_with_allows("box-b", &["message"]);
        let body = br#"{"jsonrpc":"2.0","id":1,"method":"aoide/mailPoll","params":{"node":"box-b"}}"#;
        let request = raw_request("127.0.0.1", "POST", "/", body, Some((&kp, "box-b", epoch_now(), &unique_nonce("answer-replayed"))), None);

        let first = send_raw(port, &request);
        assert_answer_signed_for(&request, &first, &kp);

        let second = send_raw(port, &request);
        let second_body: Value = serde_json::from_str(second.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(second_body["error"]["code"], -32009, "{second_body}");
        assert_eq!(aoide_test_support::raw_header(&second, aoide_storage::wire_auth::HEADER_ANSWER_SIGNATURE), None, "{second}");
        adapter_cleanup(&root, saved);
    }

    /// The nonce is single-use in any letter case (relay-proof review, HIGH).
    /// The signature folds a nonce's case, so a captured request whose nonce
    /// header is upper-cased still verifies. The replay cache must key it the
    /// same way, or that copy runs a second time with a signed answer.
    #[test]
    fn a_replayed_request_with_its_nonce_upper_cased_is_refused_unsigned() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, saved) = adapter_root("nonce-case-replay");
        let port = free_loopback_port();
        raise_mail_adapter(port, Path::new("/dev/null"));

        let kp = setup_signed_node_with_allows("box-b", &["message"]);
        let body = br#"{"jsonrpc":"2.0","id":1,"method":"aoide/mailPoll","params":{"node":"box-b"}}"#;
        let nonce = unique_nonce("NonceCase");
        let request = raw_request("127.0.0.1", "POST", "/", body, Some((&kp, "box-b", epoch_now(), &nonce)), None);
        let first = send_raw(port, &request);
        assert_answer_signed_for(&request, &first, &kp);

        // The captured request, byte for byte, except the nonce header's case.
        let replay = String::from_utf8(request.clone()).unwrap().replacen(
            &format!("{}: {nonce}", aoide_storage::wire_auth::HEADER_NONCE),
            &format!("{}: {}", aoide_storage::wire_auth::HEADER_NONCE, nonce.to_uppercase()),
            1,
        );
        let second = send_raw(port, replay.as_bytes());
        let second_body: Value = serde_json::from_str(second.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(second_body["error"]["code"], -32009, "the upper-cased copy is a replay: {second_body}");
        assert_eq!(aoide_test_support::raw_header(&second, aoide_storage::wire_auth::HEADER_ANSWER_SIGNATURE), None, "{second}");
        adapter_cleanup(&root, saved);
    }

    #[test]
    fn an_unsigned_request_and_the_card_carry_no_answer_signature() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, saved) = adapter_root("answer-unsigned");
        let port = free_loopback_port();
        raise_mail_adapter(port, Path::new("/dev/null"));

        let body = br#"{"jsonrpc":"2.0","id":1,"method":"aoide/mailPoll","params":{"node":"box-b"}}"#;
        let unsigned = send_raw(port, &raw_request("127.0.0.1", "POST", "/", body, None, None));
        assert_eq!(aoide_test_support::raw_header(&unsigned, aoide_storage::wire_auth::HEADER_ANSWER_SIGNATURE), None, "{unsigned}");

        let card = send_raw(port, &raw_request("127.0.0.1", "GET", "/.well-known/agent-card.json", b"", None, None));
        assert!(card.starts_with("HTTP/1.1 200"), "{card}");
        assert_eq!(aoide_test_support::raw_header(&card, aoide_storage::wire_auth::HEADER_ANSWER_SIGNATURE), None, "{card}");
        adapter_cleanup(&root, saved);
    }

    /// Only POST is a signed request: a signed GET is refused before it is
    /// verified, so the door signs nothing about it. The unsigned card GET
    /// is served as before, and it is never signed either.
    #[test]
    fn a_signed_get_is_refused_and_the_card_stays_unsigned() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, saved) = adapter_root("answer-card-signed");
        let port = free_loopback_port();
        raise_mail_adapter(port, Path::new("/dev/null"));

        let kp = setup_signed_node_with_allows("box-b", &["message"]);
        let signed = send_raw(
            port,
            &raw_request(
                "127.0.0.1",
                "GET",
                "/.well-known/agent-card.json",
                b"",
                Some((&kp, "box-b", epoch_now(), &unique_nonce("answer-card-signed"))),
                None,
            ),
        );
        let refused: Value = serde_json::from_str(signed.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(refused["error"]["code"], -32007, "{signed}");
        assert_eq!(aoide_test_support::raw_header(&signed, aoide_storage::wire_auth::HEADER_ANSWER_SIGNATURE), None, "{signed}");

        let card = send_raw(port, &raw_request("127.0.0.1", "GET", "/.well-known/agent-card.json", b"", None, None));
        assert!(card.starts_with("HTTP/1.1 200"), "{card}");
        assert_eq!(aoide_test_support::raw_header(&card, aoide_storage::wire_auth::HEADER_ANSWER_SIGNATURE), None, "{card}");
        adapter_cleanup(&root, saved);
    }

    /// Domain separation (relay-proof, review finding 3): a request whose method
    /// is spelled `ANSWER` carries a valid signature over the `answer\0…`
    /// form. It is refused before verification, so it spends no nonce: the
    /// same nonce sent as a POST is then admitted and answered.
    #[test]
    fn a_signed_request_whose_method_is_answer_is_refused_before_verification() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, saved) = adapter_root("answer-method-tag");
        let port = free_loopback_port();
        raise_mail_adapter(port, Path::new("/dev/null"));

        let kp = setup_signed_node_with_allows("box-b", &["message"]);
        let body = br#"{"jsonrpc":"2.0","id":1,"method":"aoide/mailPoll","params":{"node":"box-b"}}"#;
        let nonce = unique_nonce("answer-method-tag");
        let as_answer = send_raw(
            port,
            &raw_request("127.0.0.1", "ANSWER", "/", body, Some((&kp, "box-b", epoch_now(), &nonce)), None),
        );
        let refused: Value = serde_json::from_str(as_answer.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(refused["error"]["code"], -32007, "{as_answer}");
        assert_eq!(aoide_test_support::raw_header(&as_answer, aoide_storage::wire_auth::HEADER_ANSWER_SIGNATURE), None, "{as_answer}");

        let post = raw_request("127.0.0.1", "POST", "/", body, Some((&kp, "box-b", epoch_now(), &nonce)), None);
        let as_post = send_raw(port, &post);
        assert_answer_signed_for(&post, &as_post, &kp);
        adapter_cleanup(&root, saved);
    }

    /// A config-invalid answer to a verified mail request, whose charter is in
    /// force but was never signed for this node (no `.sig`, no operator to decide
    /// it): nothing governs the charter, so the taught word goes out UNSIGNED.
    #[test]
    fn a_config_invalid_answer_without_a_governing_charter_is_unsigned() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = aoide_test_support::EnvSaver::capture(&[
            "AOIDE_ROOT",
            "STRILLIX_ROOT",
            "AOIDE_STATE_DIR",
            "AOIDE_STAGE_DIR",
            "AOIDE_CONFIG",
            "AOIDE_A2A_NODE_NAME",
        ]);
        let root = aoide_test_support::unique_tmp("answer-config-invalid");
        charter_machine(&root, "receiver", "receiverbox");
        // The receiver's own charter line is the key the request is signed under.
        let line = aoide_storage::charter::node_line().unwrap();
        let in_force = aoide_storage::charter::in_force_path("home");
        std::fs::create_dir_all(in_force.parent().unwrap()).unwrap();
        std::fs::write(&in_force, format!("mesh = \"home\"\nversion = 0\nrelays = []\n\n[nodes]\n{line}\n")).unwrap();
        // A declaration set that will not load: the mail methods read config-invalid.
        std::fs::write(root.join("receiver").join("config.toml"), "[mesh.home.status]\nnobody = \"down\"\n").unwrap();
        assert!(aoide_storage::charter::governing("home").is_none(), "nothing signed or trusted governs this charter");

        let kp = aoide_storage::identity::load_or_mint().unwrap().0;
        let port = free_loopback_port();
        raise_mail_adapter(port, &root.join("receiver").join("log"));
        let body = br#"{"jsonrpc":"2.0","id":1,"method":"aoide/mailPoll","params":{"node":"box-b"}}"#;
        let request = raw_request(
            "127.0.0.1",
            "POST",
            "/",
            body,
            Some((&kp, "receiverbox", epoch_now(), &unique_nonce("answer-config-invalid"))),
            Some("home"),
        );
        let answer = send_raw(port, &request);

        let answer_body: Value = serde_json::from_str(answer.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(answer_body["result"]["reason"], "config-invalid", "{answer_body}");
        assert_eq!(aoide_test_support::raw_header(&answer, aoide_storage::wire_auth::HEADER_ANSWER_SIGNATURE), None, "{answer}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The config-invalid answer is UNSIGNED even when the charter is one the
    /// operator signed and this node accepted. The config-invalid word only comes
    /// from `config::load` failing, and `charter::governing` reads that same
    /// config (its operator line) before it will name any charter, so no
    /// config-invalid answer is ever made under a charter this node governs, and
    /// the door never signs one. This pins both facts.
    #[test]
    fn a_config_invalid_answer_is_unsigned_even_under_an_accepted_charter() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = aoide_test_support::EnvSaver::capture(&[
            "AOIDE_ROOT",
            "STRILLIX_ROOT",
            "AOIDE_STATE_DIR",
            "AOIDE_STAGE_DIR",
            "AOIDE_CONFIG",
            "AOIDE_A2A_NODE_NAME",
        ]);
        let root = aoide_test_support::unique_tmp("answer-config-invalid-governed");
        charter_machine(&root, "operator", "opbox");
        let init = aoide_storage::charter::init("home").unwrap();
        let op_line = aoide_storage::charter::node_line().unwrap();
        charter_machine(&root, "receiver", "receiverbox");
        let receiver_line = aoide_storage::charter::node_line().unwrap();

        charter_machine(&root, "operator", "opbox");
        std::fs::write(
            aoide_storage::charter::source_path("home"),
            format!("mesh = \"home\"\nversion = 0\nrelays = []\n\n[nodes]\n{op_line}\n{receiver_line}\n"),
        )
        .unwrap();
        aoide_storage::charter::sign("home", None).unwrap();
        let bytes = std::fs::read(aoide_storage::charter::source_path("home")).unwrap();
        let sig = std::fs::read(aoide_storage::charter::source_sig_path("home")).unwrap();

        charter_machine(&root, "receiver", "receiverbox");
        let config = root.join("receiver").join("config.toml");
        std::fs::write(&config, format!("[mesh.home]\noperator = \"ed25519:{}\"\n", init.operator)).unwrap();
        assert_eq!(aoide_storage::charter::accept(&bytes, &sig).unwrap().charter.version, 1);
        std::fs::write(
            &config,
            format!("[mesh.home]\noperator = \"ed25519:{}\"\n\n[mesh.home.status]\nnobody = \"down\"\n", init.operator),
        )
        .unwrap();
        assert!(aoide_storage::routing::declarations().is_err(), "the declarations will not load");
        let refusal = aoide_storage::charter::governing_refusal("home").unwrap_err();
        assert_eq!(refusal.reason, aoide_storage::charter::CONFIG_UNREADABLE, "governing reads the same unloadable config: {refusal:?}");

        let kp = aoide_storage::identity::load_or_mint().unwrap().0;
        let port = free_loopback_port();
        raise_mail_adapter(port, &root.join("receiver").join("log"));
        let body = br#"{"jsonrpc":"2.0","id":1,"method":"aoide/mailPoll","params":{"node":"box-b"}}"#;
        let request = raw_request(
            "127.0.0.1",
            "POST",
            "/",
            body,
            Some((&kp, "receiverbox", epoch_now(), &unique_nonce("answer-config-invalid-accepted"))),
            Some("home"),
        );
        let answer = send_raw(port, &request);

        let answer_body: Value = serde_json::from_str(answer.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(answer_body["result"]["reason"], "config-invalid", "{answer_body}");
        assert_eq!(aoide_test_support::raw_header(&answer, aoide_storage::wire_auth::HEADER_ANSWER_SIGNATURE), None, "{answer}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The answer path only LOADS the box's identity. With no key on disk, a
    /// verified request is answered unsigned, audited, and no key file appears.
    /// The signer's key is a node on file with the key held only in memory.
    #[test]
    fn an_answer_with_no_identity_on_disk_is_unsigned_and_mints_no_key() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, saved) = adapter_root("answer-no-identity");
        let port = free_loopback_port();
        let audit_log = root.join("log");
        raise_mail_adapter(port, &audit_log);

        let signer = aoide_storage::identity::mint_ephemeral().unwrap();
        let mut node = fixture_node("box-b", "http://node/");
        node.verified = true;
        node.pubkey = Some(signer.info().pubkey_hex);
        node.grants = aoide_storage::node_store::grants_in("home", &["message"]);
        aoide_storage::node_store::save_nodes(&[node]).unwrap();

        let body = br#"{"jsonrpc":"2.0","id":1,"method":"aoide/mailPoll","params":{"node":"box-b"}}"#;
        let answer = send_raw(
            port,
            &raw_request("127.0.0.1", "POST", "/", body, Some((&signer, "box-b", epoch_now(), &unique_nonce("answer-no-identity"))), None),
        );

        let answer_body: Value = serde_json::from_str(answer.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert!(answer_body.get("result").is_some(), "the request verified and was answered: {answer}");
        assert_eq!(aoide_test_support::raw_header(&answer, aoide_storage::wire_auth::HEADER_ANSWER_SIGNATURE), None, "{answer}");
        assert!(!aoide_storage::identity::identity_dir().join("ed25519.key").exists(), "and the answer minted no key");
        let log = std::fs::read_to_string(&audit_log).unwrap_or_default();
        assert!(log.contains("a2a.answer-unsigned"), "the unsigned answer is audited: {log}");
        adapter_cleanup(&root, saved);
    }
}
