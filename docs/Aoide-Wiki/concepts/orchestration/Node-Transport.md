---
type: concept
created: 2026-08-27
updated: 2026-08-29
tags: [aoide, agent, a2a, orchestration, node, transport, ssh]
---

# Node Transport — reaching a loopback door from another box

Every [[A2A-Door]] stays loopback-bound, always — a node's `aoide-a2a`
listens on `127.0.0.1` and nothing here moves that. What a **paired** node
can do instead is reach that loopback socket through a session-scoped ssh
tunnel: an internal `-L` forward `aoide-client` opens on demand, dials
through, and closes with the session that opened it. The tunnel is a
TRANSPORT hop, not a protocol relay — the signed node identity crosses it
end to end: the signature over the canonical string (path included, host
never) is what the far door verifies, resolving the caller by the key that
signed, not by the `X-Aoide-Node` name it carries ([[Node-Federation]]'s
Signature rung) — the exact same request it always verified. Spec:
`docs/architecture/PAIRING.md`'s Transport section; wire shape in
`CONTRACTS.md` §6 (signature-outranks-loopback) and §7 (`Node.via`).

## Why it exists

A door that only ever answers `127.0.0.1` is unreachable from a second box
on its own terms. A direct dial requires the node's address to be
HTTP-routable — true on a flat LAN, false the moment a router filters
cross-segment traffic (the gap `docs/architecture/PAIRING.md`'s Discovery
section documents) or the door is deliberately kept off any routable
interface. Ssh already crosses that boundary on a trusted
home network, and aoide already assumes ssh keys are set up between paired
boxes — so instead of a raw interface, a node reaches the door through a
tunnel aoide opens for the duration of one session, never a standing pipe
the operator maintains by hand.

## The `via` marker

`Node.via` (`ssh://[user@]host[:port]`, `aoide_storage::tunnel::parse_via`)
is the only thing that turns a dial on: absent, every outbound call to that
node — every signed POST and `node add`'s own unsigned AgentCard GET —
dials `url` directly. Present, it
names the ssh target `aoide-client`'s dial resolution forwards through.
`set_node_via` is the sole writer, a sibling to `upsert_paired_node` rather
than a parameter on it, and a caller passing no value never clears an
existing marker — a plain `aoide pair` re-pair leaves an earlier
`via` untouched.

Four ways a node picks one up:

- **`--via` on the command itself** — `node add`/`aoide pair`/`node spawn` —
  always outranks a node's own recorded `via`.
- **`aoide pair <name>`'s hostname arm derives one automatically** from the
  discovery advertisement's OBSERVED source address plus its claimed ssh
  login — the wire is `{v, name, host, user}`, a rendezvous claim only
  (never a URL, key, or credential), and the observed address is what gets
  dialed. The same derived `via` drives the ceremony's own two POSTs and is
  recorded on the resulting node once pairing is approved, so its future
  calls have a working transport marker too.
- **The approver's own commit records one from the requester's `selfVia`
  claim.** A loopback-only requester's `aoide/pairRequest` arrives over its
  own tunnel looking like loopback, so the approver has no observed address
  to derive anything from; the request carries an optional self-asserted
  `ssh://[user@]host` claim (`--self-via` overrides the `$USER`/ outbound-address
  default — and a dial whose route resolves to LOOPBACK carries no claim at
  all, since two daemons on one machine have no hop between them to name,
  D5), and `aoide pair`'s commit sets the node's `via` to the
  claim and rewrites its `url` to `http://127.0.0.1:<port>/` — `<port>`
  parsed off the requester's own advertised url — in the same write.
  Self-asserted data, a transport marker only: trust stays in the pubkeys
  and the SAS comparison.
- **A plain, URL-targeted `aoide pair` with no claim** records nothing — a
  node paired without `--via` dials directly.

## Dial resolution

`resolve_dial_url(logical_url, via, tunnel_key)` is the funnel every
cross-box network call resolves through before it reaches `commands::
post_json` — POST or GET, signed or not. `via: None` returns the logical
url unchanged; `via: Some` opens or reuses the tunnel
(`aoide_client::tunnel::open_or_reuse`) and rewrites only the
`scheme://host:port` authority to `http://127.0.0.1:<local port>`, the
forward's own local end. The **path is preserved verbatim** —
`aoide_storage::tunnel::dial_url` delegates the extraction to
`node_store::url_path`, the same function `sign_headers_for_node`'s
canonical string binds to, so the two can never drift apart. The logical url is what a signature signs; the
rewritten url is only ever what gets dialed. This is also why `node add`'s
AgentCard fetch — its one verification call, a plain GET — dials through
the same funnel when `--via` is given: the exact scenario `--via` exists
for is a door reachable only through the tunnel, so the verification GET
must ride it too.
Either way `node add` records the node under its LOGICAL `url`, never the
rewritten one.

## Security posture — a tunneled request is remote

Every request that arrives through the forward reaches the far door's
socket as `ConnOrigin::Loopback` — that is what a forward IS, and the door
extends Loopback's unconditional Inject-delivery free pass to an
*unsigned* connection, tunnel or not. The narrower rule: a request carrying a verified
per-request signature (`aoide-server::a2a::verify_signed_request`) is, by
construction, never a local caller, so `origin_for_inject` strips Loopback's
free pass from it before the delivery decision runs, regardless of which
address it arrived from. A signed, non-autogate node's send lands in the
far end's pending queue with node attribution, same as any other signed
node reaching the door any other way — with one exception, and it is the
pairing's own shape: a node that is the PARENT of the session it writes into
(a child's spawner is `signed non-autogate`, exactly this shape) delivers its
steer without pending on a matched remote-parent claim (CONTRACTS.md §6),
riding the exemption `origin_for_inject` makes for that match — without it the
loopback-classified tunnel would coerce to `Unknown`, which delivers nothing.
For every OTHER signed node, `state/nodes.json`'s per-node `autogate` flag —
not connection origin — is what restores auto-delivery. The spawn arm needs no
separate carve-out here: it already accepts only the
Signature rung, tunneled or not, so a spawn refusal reads exactly as taught
whichever way the request traveled. `via`/`--via` are safe to use against a
real node for this reason — the tunnel changes reachability, never trust.

## Lifecycle — persistent within a session, ephemeral across sessions

A tunnel is opened lazily on first use, keyed by `(session id, node name)`,
and reused by every later action under the same conducted session — no
second `ssh` spawns while a live one already answers the probe. It is never
allowed to become a resident daemon:

- **The fast path.** A clean `session end` closes every tunnel it opened
  (`aoide_client::tunnel::close_all_for_session`, called from `aoide-conduct
  ::graph::session_store::do_session_end`) — `close` removes a tunnel's
  record only once its `ssh` child is confirmed dead; a stubborn/hung one
  that survives the bounded `SIGTERM`+wait keeps its record on disk instead
  of losing it, so the backstop below still has a name to find.
  `open_or_reuse` holds the same rule on the OPEN side: a stale record
  whose old `ssh` survives the bounded kill REFUSES the reopen with a
  taught error rather than overwriting it — a second forward is never
  opened to the same target while the first is still alive and untracked.
- **The backstop.** `aoide-conduct::reap::sweep_orphan_tunnels` catches a
  session that never got to run that exit — SUPER+Q, SIGKILL, anything
  [[Session-Graph]]'s liveness reaping already exists to catch — and
  retries, on every sweep pass, any tunnel a kill only ATTEMPTED to clear
  (the fast path's own, or an earlier sweep pass's), until it is finally
  confirmed dead. A record's session leaving the roster is what makes it a
  candidate for a genuinely dead session — but a session that ended
  cleanly (`state: "done"`) counts as gone for THIS purpose the moment it
  ends, even before `session prune` removes its record from the roster, so
  a survivor `close` had to keep is retried promptly rather than waiting on
  prune's own schedule. It runs as
  part of the same sweep pass that collects orphan sockets, but not under
  the same lock: `reap_inner` only GATHERS candidates while holding the
  stage lock (`orphan_tunnel_candidates`, a roster/settle check against
  already-loaded state, no process signaling), and `reap` runs the KILL half
  outside it (`sweep_orphan_tunnels`) — a still-answering `ssh -N` child is
  signaled (`kill_if_still_our_ssh`), and its record is unlinked ONLY once
  that call confirms the pid actually gone; a survivor keeps its record for
  the next pass, same as the fast path. Socket cleanup stays fully inside
  the lock because unlinking a leftover file is cheap; signaling a live
  child is not, and holding the stage lock across that would block every
  other stage writer on a process that might not even need killing.
- **A bare shell with no conducted session** gets a tunnel keyed
  `pid-<pid>` instead of a session id — process-scoped, not persistent. It
  has no session-exit fast path to hook, so the same sweep's ordinary
  dead-pid arm collects it once the one-shot CLI process has already
  exited.

## Proven behavior (the yomi↔sakaki gate)

- The pairing ceremony dials through the tunnel and records `via` at
  approve.
- One `ssh` child is opened and reused per session per node — a second
  action under the same session never spawns a second forward.
- A signed send from a non-autogate node lands in the far end's pending
  queue, carrying node attribution — unless that node is the session's own
  remote parent, whose steer delivers without pending
  (`autogate-remote-parent`, CONTRACTS.md §6). A target conducting a shell
  is the exception to both: its line is held pending whoever asks, because
  a submitted line in a shell RUNS (N1).
- Spawn is refused with `-32006` the moment `allows` drops `spawn`, with a
  taught error — the same instant either side of a live tunnel.
- A settled, roster-less tunnel record is collected by the resident
  reaper.

## Known limitations

- **LAN discovery stays link-local and firewall-gated.** The broadcast
  advertisement (255.255.255.255:8711, heard only while the far end's
  `node advertise on` switch or its force-on equivalents are set) never
  crosses a router by definition, and a default-deny firewall drops the
  inbound datagram before any aoide socket sees it — the diagnosis
  `docs/architecture/PAIRING.md`'s Discovery section traces, confirmed
  independent of any aoide code. This transport is the workaround for a
  door otherwise unreachable across that same boundary, not a fix to
  discovery itself: `aoide pair <url> --via`/`node add --via` still need
  the far host named by hand when discovery can't hear it.

## Related

- [[A2A-Door]] — the loopback-bound door this transport reaches, and the
  signature-outranks-loopback amendment this lane exercises against a real
  proxy hop.
- [[Node-Federation]] — the node registry `Node.via` lives on, and the
  federation door the tunnel carries requests to.
- [[Pairing-Ceremony]] — the ceremony that records the `via` marker on the
  resulting node at approve (`aoide pair <name>`'s hostname arm derives it,
  the requester's `selfVia` claim sets it on the approver's commit; `--via`
  sets it explicitly).
- [[Conductor-Channel]] — `send`'s remote delivery, gated the same way
  whether or not the call happened to travel through a tunnel.
- [[Session-Graph]] — the liveness reaping a tunnel's own backstop sweep
  rides.
- `docs/architecture/PAIRING.md` — the design authority for both the
  pairing ceremony and this transport.
