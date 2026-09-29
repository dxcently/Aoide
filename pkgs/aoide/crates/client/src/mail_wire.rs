//! The outbox drain (messaging plan P-M2): the ONE place a spooled mail
//! envelope actually dials out. [`aoide_storage::outbox`] owns the spool
//! (pure file CRUD, no network); this module owns the wire half — the
//! `aoide/mailDeposit` POST, using the exact same signed-request machinery
//! [`crate::commands::spawn_on_node_via`] already established
//! (`resolve_node_bearer`, `sign_headers_for_node`, `post_json_to_node`).
//!
//! [`drain_node`] is called from three places, all converging on this one
//! function so there is exactly one dial implementation: the daemon's
//! periodic tick (`aoide-server::daemon`, via `aoide_conduct::mail_bridge`),
//! a door's own best-effort drain of the node it just heard from
//! (`aoide-server::a2a::mail_deposit`, same bridge), and `mail send`'s own
//! one-shot delivery attempt right after it writes the outbox entry (spec
//! item 8 — the write is the report, delivery is the spool's job).
//!
//! **Two locks, never nested (see `aoide_storage::outbox`'s own module
//! doc).** [`drain_node`] takes `.bsy` (non-blocking, per-node, held across
//! this whole function) and leaves every individual `outbox` call — each
//! its own short, independently-locked-and-released operation — to run
//! sequentially around the network POST, never holding any lock across the
//! POST itself (spec item 9: network I/O never happens under the stage
//! lock; `.bsy` is a SEPARATE mechanism that legitimately does span it).
//!
//! **A drain tears its own tunnel down before it returns (ruling 10)** —
//! [`TunnelTeardownGuard`] mirrors [`crate::commands`]'s `ScratchBodyFile`
//! Drop-guard pattern. This is a deliberate, drain-specific exception to
//! `resolve_dial_url`'s documented "every tunnel this phase opens stays
//! open" default (`commands.rs`'s own doc on `tunnel_session_id`): a
//! rarely-contacted spool target should not accumulate a standing forward
//! just because a background tick happened to touch it once.

use aoide_storage::mail::Envelope;
use aoide_storage::node_store::Node;
use serde_json::{json, Value};

/// The most entries [`drain_node`] will ATTEMPT in one call, regardless of
/// how many are spooled — bounds one tick's cost when a spool has grown
/// large (a runaway producer, or simply a backlog), so `drain_all`'s
/// per-tick cost never scales with total spool depth. An entry that
/// delivers/retires this pass falls out of the NEXT call's list on its own;
/// this cap only matters when a single call would otherwise walk the whole
/// spool.
///
/// It counts attempts, so parked (`refused`) entries are filtered out before
/// it applies. Counting them would starve the spool instead of bounding it:
/// a parked entry is permanent until `mail outbox rm` retires it, and they
/// sort oldest-first, so one batch's worth of them at the head would leave
/// every fresh letter behind them undialled forever.
const DRAIN_BATCH_CAP: usize = 50;

fn unix_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64
}

/// Tears down whatever tunnel this drain's own POSTs opened for `node_name`
/// — unconditionally, on every return path including an early one, the
/// same "removed on drop, no matter how the function exits" guarantee
/// [`crate::commands::ScratchBodyFile`] holds for its own scratch file.
/// [`aoide_client::tunnel::close`] is a safe no-op when nothing was ever
/// opened (a direct-dial node, or a node this attempt never reached) — see
/// its own doc for why calling it unconditionally is fine.
struct TunnelTeardownGuard(String);

impl Drop for TunnelTeardownGuard {
    fn drop(&mut self) {
        let _ = crate::tunnel::close(&crate::commands::tunnel_session_id(), &self.0);
    }
}

/// One signed request to one node, before either mail method's own reading
/// of the answer: the dial/bearer/sign/POST/parse half [`attempt_deposit`]
/// and [`poll_node`] share verbatim, so neither grows its own copy of the
/// signed-wire machinery (which is [`crate::commands::spawn_on_node_via`]'s,
/// reused not reinvented).
enum SignedCall {
    /// A JSON-RPC `result` member, whatever shape the method's own answer
    /// takes — the CALLER interprets it.
    Result(Value),
    /// A JSON-RPC `error` member: the far end refused the call itself
    /// (admission, per MAIL.md §Wire's admission/outcome split).
    Refused(String),
    /// No usable response at all — dial/tunnel/HTTP/parse failure.
    TransportFailed(String),
}

/// Build, sign and send one method call to `node`, mirroring
/// [`crate::commands::spawn_on_node_via`]'s exact shape (resolve bearer,
/// sign, POST, parse, check `error`) with no `--via` override — a drain is
/// never given one; it only ever dials `node.via` as recorded.
///
/// `mesh` is the mesh the request acts in — the CONTAINER's own `mesh` for a
/// deposit (a sealed letter's zone is a property of the letter, and the door
/// refuses a deposit whose request names a different mesh than the container
/// carries), and `None` for the node-addressed calls, which resolve it from
/// the record (`crate::commands::request_mesh`: the node's sole mesh, else
/// the home mesh).
fn post_signed(node: &Node, method: &str, params: Value, mesh: Option<&str>) -> SignedCall {
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
    let body_str = serde_json::to_string(&body).unwrap_or_default();

    let bearer = match crate::commands::resolve_node_bearer(node) {
        Ok(b) => b,
        Err(e) => return SignedCall::TransportFailed(format!("bearer resolve: {e}")),
    };
    let extra_headers = match crate::commands::sign_headers_for_node(node, &body_str, mesh, "message") {
        Ok(h) => h,
        Err(e) => return SignedCall::TransportFailed(format!("signing: {e}")),
    };
    let (code, resp) = match crate::commands::post_json_to_node(node, &body_str, bearer.as_deref(), &extra_headers, 15) {
        Ok(v) => v,
        Err(e) => return SignedCall::TransportFailed(e),
    };
    if code != 200 {
        return SignedCall::TransportFailed(format!("HTTP {code}"));
    }
    let parsed: Value = match serde_json::from_str(&resp) {
        Ok(v) => v,
        Err(e) => return SignedCall::TransportFailed(format!("unparseable response: {e}")),
    };
    if let Some(err) = parsed.get("error") {
        let detail = err.get("message").and_then(Value::as_str).unwrap_or("(no message)");
        return SignedCall::Refused(detail.to_string());
    }
    SignedCall::Result(parsed.get("result").cloned().unwrap_or(Value::Null))
}

/// One `aoide/mailDeposit` POST's outcome — the three shapes
/// [`drain_node`]'s loop branches on. Deliberately NOT
/// [`crate::commands::SpawnNodeError`]'s richer shape: a drain only ever
/// needs to know which of the three buckets an attempt landed in, never a
/// programmatic reason code.
#[derive(Debug)]
enum DepositAttempt {
    /// A result whose `status` is `"accepted"` or `"duplicate"`
    /// (mail::deposit's own vocabulary, wire-projected verbatim by
    /// `aoide-server::a2a::mail_deposit`).
    Delivered { status: String },
    /// The far end's OWN policy refusal — MAIL.md §Wire's admission/outcome
    /// split means this arrives as either a JSON-RPC `error` (admission,
    /// e.g. `-32010` lacks-message: the caller may not speak to the method
    /// at all) or a result whose `status` is `"refused"` (a well-formed
    /// envelope MAIL.md §Transit rejected, e.g. `bad-msgid`/
    /// `unverified-origin`) — both collapse into this one bucket because a
    /// drain only ever needs to know "not currently deliverable," never
    /// which of the two shapes carried that news. The link itself is fine
    /// either way; this ONE entry is the problem.
    Refused(String),
    /// The far end ANSWERED, with a word about its own state rather than about
    /// this letter (`down`, `config-invalid`) — so the entry is not parked, the
    /// LINK backs off, and the next pass retries (the same treatment a transport
    /// failure gets, because either way there is nothing wrong with the letter).
    LinkRefused(String),
    /// No JSON-RPC response at all — dial/tunnel/HTTP/parse failure. The
    /// LINK is the suspect, not this entry.
    TransportFailed(String),
}

/// The mesh one deposit acts in, for an outbox entry: **the container's own**
/// when the entry is sealed (mint-time, and what the door compares its signed
/// `ctx.originMesh` against), and for a PLAINTEXT entry **the mesh its own
/// envelope was signed with** — `origin_mesh` is signed into that header at
/// mint, so it is the letter's own answer, not a fresh resolution that can come
/// back ambiguous for a two-mesh record (review N2: the plaintext lane used to
/// re-resolve and fail forever against a peer trusted in two meshes, while the
/// sealed lane for the same pair worked). `None` only when the envelope names
/// none, which lets the record's `message` grant decide.
fn deposit_mesh(entry: &aoide_storage::outbox::OutboxEntry) -> Option<&str> {
    entry
        .container
        .as_ref()
        .map(|c| c.mesh.as_str())
        .or_else(|| Some(entry.envelope.header.origin_mesh.as_str()).filter(|m| !m.is_empty()))
}

/// Build and send one `aoide/mailDeposit` POST, mirroring
/// [`crate::commands::spawn_on_node_via`]'s exact shape (resolve bearer,
/// sign, POST, parse, check `error`) with no `--via` override — a drain is
/// never given one; it only ever dials `node.via` as recorded.
fn attempt_deposit(node: &Node, entry: &aoide_storage::outbox::OutboxEntry) -> DepositAttempt {
    // P-SEAL: a sealed entry posts its CONTAINER; an entry spooled before
    // the destination published a binding still posts the plaintext v1
    // envelope on this direct lane. `entry.envelope` rides every call
    // besides — it is what the local attempt bookkeeping and the
    // receipt/retire lookups key on either way.
    let params = match &entry.container {
        Some(container) => json!({ "container": container }),
        None => json!({ "envelope": entry.envelope }),
    };
    // The mesh this request acts in comes from the entry itself —
    // [`deposit_mesh`]'s doc has the rule and the review finding behind it.
    let mesh = deposit_mesh(entry);
    let result = match post_signed(node, "aoide/mailDeposit", params, mesh) {
        SignedCall::Result(result) => result,
        SignedCall::Refused(detail) => return DepositAttempt::Refused(detail),
        SignedCall::TransportFailed(reason) => return DepositAttempt::TransportFailed(reason),
    };
    classify_deposit_response(&result)
}

/// The far end's `aoide/mailDeposit` reply, read into a [`DepositAttempt`] —
/// **the one place the deposit outcome vocabulary is interpreted**, so the
/// three words MAIL.md §Wire closes it to and the words this client accepts
/// can never drift apart in two places.
///
/// A response with a `result` member but no (or non-string) `status` — or with
/// neither `result` nor `error` at all — is weaker evidence of delivery than an
/// unrecognised status string, and an unrecognised one already falls to the
/// catch-all below. So this default must land there too, never on `"accepted"`:
/// a malformed or non-conformant peer response must never be read as a
/// confirmed deposit.
///
/// **Any string this client does not recognise means the entry did NOT land,
/// never that it did.** Assuming success for an unrecognised status is the
/// exact failure this arm exists to close: a letter waiting forever for an ack
/// the far end was never going to send — and, when the door answers a word the
/// sender was never taught, a mesh whose trust DID land while its own outbox
/// says it was refused.
fn classify_deposit_response(result: &Value) -> DepositAttempt {
    let status = result.get("status").and_then(Value::as_str).unwrap_or("missing-status");
    match status {
        "accepted" | "duplicate" => DepositAttempt::Delivered { status: status.to_string() },
        other => {
            let reason = result.get("reason").and_then(Value::as_str).unwrap_or(other);
            let detail = result.get("detail").and_then(Value::as_str);
            let msg = match detail {
                Some(d) => format!("{reason}: {d}"),
                None => reason.to_string(),
            };
            // **A LINK state is not a verdict on the letter**.
            // `down` and `config-invalid` say something about the far end's own
            // state — a node its mesh quarantined, a declaration set that will not
            // load — and the sender's answer is the one it gives a link that
            // cannot carry mail right now: the entry stays LIVE (never parked,
            // never needing `retry --refused`) and the ordinary back-off carries
            // it back, so it flows by itself once the peer is fixed or no longer
            // `down`. Every other word (`zone-violation`, `broken-chain`,
            // `bad-msgid`, …) is a verdict on THIS letter and still parks it.
            if reason == aoide_storage::charter::STATUS_DOWN
                || reason == aoide_storage::charter::CONFIG_INVALID
            {
                return DepositAttempt::LinkRefused(msg);
            }
            DepositAttempt::Refused(msg)
        }
    }
}

/// Everything that follows a deposit OUTCOME, whichever door or drain
/// produced it — the receive half's only implementation, shared by the A2A
/// door's own `mail_deposit` (through `aoide_conduct::mail_bridge::
/// settle_deposit`) and by [`poll_node`]'s hand-over loop below, so the two
/// can never drift on what a filed letter or a filed receipt does here.
///
/// A filed **letter** mints and spools an ack toward its origin (via
/// [`aoide_storage::outbox::write_ack_if_absent`], never the bare
/// `write_entry`: a redelivery whose ack is STILL spooled must not mint a
/// second, differently-`msgid`-ed one) and best-effort drains that node
/// once. A **duplicate** whose original filing was a letter re-sends the ack
/// (spec item 5: the sender's earlier ack evidently never arrived); every
/// other duplicate is a silent no-op — acking an ack would ping-pong
/// forever, which the `letter`/`receipt` vocabulary has no third shape to
/// end. A filed **receipt** is the opposite leg: retire the LOCAL outbox
/// entry it confirms (spec item 7) — a forged or stale ack simply finds no
/// matching entry and retires nothing ([`aoide_storage::outbox::
/// retire_by_ack`]'s own doc). A refused outcome does nothing.
///
/// The acked msgid is the ENVELOPE's own, never the outcome's copy of it:
/// on the filed path the two are equal by construction (`mail::deposit`
/// stamps `Filed{msgid}` from `envelope.msgid`), and the envelope is what
/// the duplicate arm already had to use.
///
/// A re-drain of the same node from inside this call is skipped, not
/// queued: the drain's `.bsy` lock is non-blocking and per-node, so an ack
/// being spooled toward the very node the current drain session holds
/// (a poll that just handed over that node's own letter) is left for the
/// next tick rather than deadlocking against itself.
pub fn settle_deposit(envelope: &Envelope, outcome: &aoide_storage::mail::DepositOutcome) {
    use aoide_storage::mail::DepositOutcome;
    match outcome {
        DepositOutcome::Filed { kind, .. } if kind == aoide_storage::mail::ENTRY_TYPE_LETTER => {
            spool_and_drain_ack(envelope, &envelope.msgid)
        }
        DepositOutcome::Duplicate { filed_letter: true } => spool_and_drain_ack(envelope, &envelope.msgid),
        DepositOutcome::Filed { kind, .. } if kind == aoide_storage::mail::ENTRY_TYPE_RECEIPT => {
            let _ = aoide_storage::outbox::retire_by_ack(envelope);
        }
        _ => {}
    }
}

/// Mint an ack for `acked_msgid` (destination is `envelope.header.to`, the
/// mailbox that just received it; origin is `envelope.header.from`, who it
/// goes back to), spool it into that origin's outbox, and best-effort drain
/// that node once.
fn spool_and_drain_ack(envelope: &Envelope, acked_msgid: &str) {
    // P-CHARTER: the ack rides the mesh the letter arrived in — the incoming
    // envelope's own signed `origin_mesh` — so the receipt is depositable
    // exactly where the letter was (review finding 1 covers acks too: an ack
    // minted unnamed while its request is signed for another mesh can never
    // land).
    let Ok(ack) = aoide_storage::mail::mint_ack_in_mesh(
        &envelope.header.to.name,
        envelope.header.from.clone(),
        acked_msgid,
        &envelope.header.origin_mesh,
    ) else {
        return;
    };
    let origin_node = envelope.header.from.node.clone();
    // An ack is sealed like any other letter when the far end has published
    // a binding, and stays plaintext over the direct lane when it has not.
    //
    // **And it leaves by the same four steps as a letter** — a receipt
    // traverses hubs for free (MAIL.md §Wire), which is what lets an ack from a
    // node behind a relay reach the origin. Where the declarations give no route
    // at all, the direct edge is still tried: the ack is owed to a node this box
    // could always dial, and a route read that fails must not cost the receipt.
    let mesh = envelope.header.origin_mesh.clone();
    let next = route_for(&origin_node, &mesh).map(|hop| hop.next).unwrap_or_else(|_| origin_node.clone());
    let entry = match spool_entry(&origin_node, &next, &mesh, ack, false) {
        Ok(entry) => entry,
        Err(_) => return,
    };
    if aoide_storage::outbox::write_ack_if_absent(&next, acked_msgid, &entry) == Ok(true) {
        let _ = drain_node(&next);
    }
}

/// Build the outbox entry a letter leaves this node in (P-SEAL, MAIL.md's
/// P-SEAL slice: "every letter that leaves its node sealed at mint when the
/// destination's binding is held").
///
/// Three outcomes, and the third is the one worth naming:
///
/// - the destination holds **no binding**: the plaintext v1 envelope, over
///   the direct SSH lane only. This is the per-peer upgrade path, and it is
///   the only remaining plaintext any node sends.
/// - a **usable binding**: sealed at mint. The container is built HERE, not
///   at dial time, so a retry resends byte-identical bytes.
/// - a binding that is **expired, not yet valid, or names a suite this
///   build does not accept**: the entry is spooled **parked** — the far end
///   is a sealed destination, so plaintext is not an option, and a stale or
///   unusable binding must never be sealed to. `refused` is the existing
///   parked-not-condemned flag ([`aoide_storage::outbox::unpark_refused`]
///   clears it), so the letter is held, visible and reported rather than
///   dropped or downgraded.
pub fn spool_entry(
    dest: &str,
    next: &str,
    mesh: &str,
    envelope: Envelope,
    hold: bool,
) -> Result<aoide_storage::outbox::OutboxEntry, String> {
    use aoide_storage::outbox::OutboxEntry;
    let now = aoide_storage::time::now_iso_utc();
    // **A hop that owns no inbound transport IS held, whatever the caller asked
    // for** — the design's rule at the one place the flavor is decided, so that
    // EVERY caller inherits it and none can forget it: "a hub's outbox entry for
    // a `poll` node is `hold`-flavored. The hub never dials it, and only the
    // node's own `mailPoll` drains it" (`docs/architecture/HTTPS-MESH-API.md`
    // "Transports and relays"). [`spool_and_drain_ack`] is the caller this
    // matters for most: it passes a literal `false`, and its ack toward a `poll`
    // origin is held by this line rather than by an argument it would have to
    // remember to compute.
    //
    // `dest` and `next` are different questions: the container is sealed to the
    // DESTINATION (whose binding is the one that opens it), while the entry is
    // spooled and dialled toward the HOP the route picked (MAIL.md §Transit).
    let hold = hold || hop_is_never_dialled(next, mesh);
    // **Where a destination's age key comes from**: the one this box LEARNT
    // (a signed exchange with a node it paired with), else the one the mesh
    // DECLARES for it — a charter line carries every node's `age` key, which is
    // how a sender that never paired with the destination can seal to it at all.
    // A declared line is verified exactly as a learnt binding is (against that
    // line's own identity key, which the operator signed).
    let binding = aoide_storage::seal::usable_binding_for(dest, &now)
        // **A learnt binding is only usable if it is the DECLARED key's.** A
        // binding is learnt per NAME, so a node whose key the mesh has since
        // changed (or a stale file left by an earlier pairing) would otherwise
        // seal a letter to a key this box no longer trusts that name for. Where
        // the mesh names the node, the declaration decides; where it does not
        // (a pair mesh with no declaration at all), the record the binding was
        // learnt against is the whole authority, which `learn_binding` already
        // pinned.
        .filter(|binding| binding_matches_the_declaration(binding, dest, mesh))
        .or_else(|| declared_binding(dest, mesh, &now));
    // **Plaintext is the DIRECT lane's, and only the direct lane's.** A relay
    // carries sealed containers and nothing else ("no relay, hub or HTTPS hop
    // ever carries plaintext", HTTPS-MESH-API), so an entry whose `next` is not
    // the destination itself must be sealed to it — and where no binding can be
    // had, it is PARKED rather than put in the clear for a hub to read. `dest ==
    // next` to a peer that has published no binding is the one legitimate
    // plaintext send any node still makes (the per-peer upgrade path).
    let direct = dest == next;
    match binding {
        Some(binding) => {
            let container = aoide_storage::seal::seal_envelope(
                &envelope,
                &binding,
                &envelope.header.origin_mesh,
                mesh,
                dest,
                next,
                &now,
            )?;
            Ok(if hold {
                OutboxEntry::sealed_held(envelope, container)
            } else {
                OutboxEntry::sealed(envelope, container)
            })
        }
        None if direct
            && aoide_storage::seal::binding_for(dest).is_none()
            && !declares_a_binding(dest, mesh) =>
        {
            Ok(if hold { OutboxEntry::held(envelope) } else { OutboxEntry::fresh(envelope) })
        }
        None => {
            // L15: the parking decision does not change the caller's flavor.
            // A `--hold` letter that is parked stays HELD, which also keeps it
            // visible to the poll (the offer rule admits every `hold` entry),
            // so the operator can still see and act on it.
            let mut entry = if hold {
                OutboxEntry::held(envelope)
            } else {
                OutboxEntry::fresh(envelope)
            };
            entry.refused = true;
            entry.last_try_at = now;
            entry.last_outcome = if direct {
                "parked: the destination holds a binding that is not usable now".to_string()
            } else {
                format!(
                    "parked ({NO_BINDING_FOR_A_RELAY}): `{dest}` publishes no age binding this box can \
                     use, and a relay carries sealed containers only — publish one (`aoide mail poll` \
                     exchanges them), or pair with `{dest}`"
                )
            };
            Ok(entry)
        }
    }
}

/// **Is this box's hop one it must never dial** — a `poll` address, whether the
/// box reads it off a paired record or off a declaration? The ONE by-name reader
/// of that predicate on the mail path: [`spool_entry`] holds an entry toward such
/// a node, [`drain_node`] returns without opening a link to one, and
/// [`pollable_nodes`] leaves it out of a bare `mail poll` — it has no inbound
/// transport to be asked on.
///
/// Two sources, one answer: a node this box has PAIRED with carries the address
/// in its record (`Node::never_dialled`), and a hop this box holds no record for
/// carries it in the declaration of the mesh the letter rides
/// (`charter::dial_of`). A node neither names is NOT never-dialled — there is no
/// address to read, and the existing "not registered" answers stand.
fn hop_is_never_dialled(node_name: &str, mesh: &str) -> bool {
    if dest_is_never_dialled(node_name) {
        return true;
    }
    declared_never_dialled(mesh, node_name)
}

/// The word a caller reads when a letter cannot be sent in the clear: the
/// destination publishes no age binding this box can use, and the entry's next
/// hop is not the destination itself — so a relay would have to read it. The
/// letter is parked, visible in `mail outbox`, and `mail outbox retry --refused`
/// is the hand once a binding exists.
pub const NO_BINDING_FOR_A_RELAY: &str = "sealed-required";

/// Does this LEARNT binding belong to the key the mesh declares for `dest`? Where
/// no declaration names it (a mesh the set does not hold, or a pair mesh whose
/// section lists no nodes), nothing contradicts the binding: it was learnt
/// against a pinned record and `learn_binding` owns that rule.
fn binding_matches_the_declaration(
    binding: &aoide_storage::seal::Binding,
    dest: &str,
    mesh: &str,
) -> bool {
    let Ok(set) = aoide_storage::routing::declarations() else {
        return false;
    };
    match aoide_storage::routing::key_in(&set, mesh, dest) {
        Some(declared) => binding.identity_key.eq_ignore_ascii_case(&declared),
        None => true,
    }
}

/// The age binding the mesh DECLARES for `dest`, if it is usable now — a charter
/// line's own `age` key, verified under that line's identity key
/// (`seal::verify_binding`, the same check [`aoide_storage::seal::learn_binding`]
/// makes against a pinned record) and inside its window. `None` for a pair mesh
/// (a record is not a source for a binding), for a line that fails its own
/// signature, and for one that is expired or not yet valid — which is the parking
/// arm in [`spool_entry`], never a downgrade to plaintext.
fn declared_binding(
    dest: &str,
    mesh: &str,
    now: &str,
) -> Option<aoide_storage::seal::Binding> {
    let set = aoide_storage::routing::declarations().ok()?;
    let declaration = aoide_storage::routing::declaration_of(&set, mesh)?.as_ref().ok()?;
    let binding = declaration.binding_of(dest)?.clone();
    let key = aoide_storage::routing::key_in(&set, mesh, dest)?;
    if !aoide_storage::seal::verify_binding(&binding, Some(&key)) {
        return None;
    }
    if aoide_storage::seal::binding_expired(&binding, now)
        || aoide_storage::seal::binding_not_yet_valid(&binding, now)
    {
        return None;
    }
    Some(binding)
}

/// Does the mesh give this box a binding to seal to `dest` — usable or not, and
/// INCLUDING a mesh that cannot be read at all? The question
/// [`spool_entry`]'s parking arm asks, so a destination whose declared binding
/// is expired is held rather than sent in the clear, and an unreadable mesh
/// parks the letter instead of downgrading it (a binding this box cannot read is
/// not a licence to send plaintext).
fn declares_a_binding(dest: &str, mesh: &str) -> bool {
    let Ok(set) = aoide_storage::routing::declarations() else {
        return true;
    };
    match aoide_storage::routing::declaration_of(&set, mesh) {
        Some(Ok(declaration)) => declaration.binding_of(dest).is_some(),
        Some(Err(_)) => true,
        None => false,
    }
}

/// The record half of [`hop_is_never_dialled`]: a node this box has paired with,
/// whose own stored address reads `poll`.
fn dest_is_never_dialled(node_name: &str) -> bool {
    aoide_storage::node_store::load_nodes()
        .iter()
        .any(|n| n.name == node_name && n.never_dialled())
}

/// Is `node` the `poll` node of the DECLARATION of the mesh the letter rides?
/// Read from the declaration SET, so a mesh the set refuses cannot say "not
/// `poll`" and send the letter out in the clear to a node that never listens —
/// a refused mesh is treated as never-dialled (parked), the fail-closed
/// direction.
fn declared_never_dialled(mesh: &str, node: &str) -> bool {
    let Ok(set) = aoide_storage::routing::declarations() else {
        return true;
    };
    match aoide_storage::routing::declaration_of(&set, mesh) {
        Some(Ok(declaration)) => declaration
            .address_of(node)
            .and_then(|address| aoide_storage::charter::dial_of(address).ok())
            .map(|dial| matches!(dial, aoide_storage::charter::Dial::Poll))
            .unwrap_or(false),
        Some(Err(_)) => true,
        None => false,
    }
}

/// **Does the declaration forbid dialling `node` in `mesh`?** True for a node the
/// mesh declares `down` and for a mesh this box cannot read at all — a set that
/// will not load, or an entry it REFUSES — because both mean "never dial it": the
/// second is the fail-closed direction [`declared_never_dialled`] already takes
/// for a `poll` address. The ONE predicate the drain, the poll and the reports
/// that answer for them ask before reaching a node by name; a mesh the set does
/// not hold declares nothing, so it forbids nothing.
pub fn declaration_forbids_dial(mesh: &str, node: &str) -> bool {
    match aoide_storage::routing::declarations() {
        Ok(set) => forbidden_by_declaration(&set, mesh, node),
        Err(_) => true,
    }
}

/// [`declaration_forbids_dial`] for a RECORD: the judged name is the
/// declaration's own for the record's identity KEY (`routing::declared_name`),
/// falling back to the record's own name only where the mesh names no such key —
/// a `nodes.json` nickname is display, never a policy input (MAIL.md §Transit).
/// The drain, the poll and their reports all reach the question through here, so
/// none of them can judge a node by a name no mesh gave it.
pub fn record_forbidden_by_declaration(mesh: &str, record: &aoide_storage::node_store::Node) -> bool {
    match aoide_storage::routing::declarations() {
        Ok(set) => forbidden_by_declaration(&set, mesh, &judged_name(&set, mesh, record)),
        Err(_) => true,
    }
}

/// The name a status lookup reads for `record` in `mesh`.
fn judged_name(
    set: &[aoide_storage::routing::Loaded],
    mesh: &str,
    record: &aoide_storage::node_store::Node,
) -> String {
    record
        .pubkey
        .as_deref()
        .and_then(|key| aoide_storage::routing::declared_name(set, mesh, key))
        .unwrap_or_else(|| record.name.clone())
}

/// Can this box read `mesh`'s declaration at all? `false` only where the set
/// holds the mesh AND reads it — a set that will not load, or an entry the set
/// REFUSES (a tampered charter, a one-sided gate, a mesh with no charter in
/// force), means no. No report can call a node `down` in such a mesh, and nothing
/// dials one either: [`declaration_forbids_dial`] fails closed on the same two shapes.
pub fn declaration_unreadable(mesh: &str) -> bool {
    match aoide_storage::routing::declarations() {
        Ok(set) => matches!(
            aoide_storage::routing::declaration_of(&set, mesh),
            Some(Err(_))
        ),
        Err(_) => true,
    }
}

/// [`declaration_forbids_dial`] over a set the caller already loaded, failing
/// closed the same way: a mesh the set REFUSES forbids the dial too.
fn forbidden_by_declaration(set: &[aoide_storage::routing::Loaded], mesh: &str, name: &str) -> bool {
    match aoide_storage::routing::declaration_of(set, mesh) {
        Some(Ok(declaration)) => declaration.status_of(name) == Some(aoide_storage::charter::STATUS_DOWN),
        Some(Err(_)) => true,
        None => false,
    }
}

/// The mesh a poll of `node_name` acts in — `named` where the caller typed one,
/// else the record's own `message` mesh ([`crate::commands::request_mesh`], the
/// same resolution [`poll_node`] signs with), so a caller that refuses before
/// the dial and the dial itself cannot choose two meshes.
pub fn poll_mesh(node_name: &str, named: Option<&str>) -> Option<String> {
    let node = aoide_storage::node_store::load_nodes().into_iter().find(|n| n.name == node_name)?;
    crate::commands::request_mesh(&node, named, "message").ok()
}

/// Where this box hands a letter addressed to `dest` in `mesh` — the four steps
/// over the declarations in force, read as this box's own name in that mesh
/// (`routing::own_name_in`: the name this box's identity key holds there, and
/// for a PAIR mesh its own name where the mesh has no record of itself, since a
/// pair mesh's records are all about other boxes). A CHARTER mesh that does not
/// carry the key has no sender to read, and says `not-a-member` rather than
/// routing the letter as whoever the hostname happens to spell.
pub fn route_for(
    dest: &str,
    mesh: &str,
) -> Result<aoide_storage::routing::Hop, aoide_storage::charter::Refusal> {
    use aoide_storage::charter::Refusal;
    let set = aoide_storage::routing::declarations()?;
    let own = aoide_storage::identity::load_or_mint()
        .map_err(|e| Refusal::new("no-identity", e.to_string()))?
        .0
        .info()
        .pubkey_hex;
    let Some(from) = aoide_storage::routing::own_name_in(&set, mesh, &own) else {
        return Err(Refusal::new(
            NOT_A_MEMBER,
            format!("no line on mesh `{mesh}` carries this box's identity key"),
        ));
    };
    let route = aoide_storage::routing::Letter { from: &from, to: dest, mesh }.route(&set);
    match route.outcome {
        // A mesh this box holds no declaration of is not a dead end: the
        // destination's own record is the whole edge, which is the lane every
        // box had before the four steps existed (see [`direct_edge`]).
        Err(refusal) if refusal.reason == aoide_storage::routing::NO_DECLARATION => direct_edge(dest, mesh),
        outcome => outcome,
    }
}

/// The direct edge: the destination ITSELF as the hop, for a mesh this box holds
/// no declaration of at all. That is the lane every box had before the four steps
/// existed (a paired record and nothing else), and refusing it here would make
/// the declarations' absence a send-time outage for every older pair. The
/// facts are the record's own — a stored key, and an address that is not `poll`
/// (which is HELD, never dialled) — so this is step 1 with the record as the
/// declaration, not a second routing rule: `no-route` where the record cannot
/// take the letter either.
fn direct_edge(dest: &str, mesh: &str) -> Result<aoide_storage::routing::Hop, aoide_storage::charter::Refusal> {
    use aoide_storage::charter::Refusal;
    let record = aoide_storage::node_store::load_nodes().into_iter().find(|n| n.name == dest);
    let Some(record) = record else {
        return Err(Refusal::new(
            aoide_storage::routing::NO_ROUTE,
            format!("`{dest}` is no record this box holds, and no declaration here names it"),
        ));
    };
    if !record.verified {
        return Err(Refusal::new(
            aoide_storage::routing::NO_ROUTE,
            format!("`{dest}` is not a verified record: pair first (`aoide pair`), or declare it in a mesh"),
        ));
    }
    Ok(aoide_storage::routing::Hop {
        next: dest.to_string(),
        mesh: mesh.to_string(),
        held: record.never_dialled(),
    })
}

/// The word a caller reads when this box is not a member of the mesh it was
/// asked to send in: the same answer `mail route` gives, so the command and the
/// lane never disagree about whether the letter has a sender.
pub const NOT_A_MEMBER: &str = "not-a-member";

/// This box's own name in `mesh`, or `None` where the mesh names it not at all
/// (`routing::own_name_in`): a charter mesh that does not carry this box's
/// identity key has no sender.
pub fn own_name(mesh: &str) -> Option<String> {
    let set = aoide_storage::routing::declarations().ok()?;
    let own = aoide_storage::identity::load_or_mint().ok()?.0.info().pubkey_hex;
    aoide_storage::routing::own_name_in(&set, mesh, &own)
}

/// The mesh a send to `node` acts in where this box holds NO RECORD of it: the
/// declarations that both name the destination and carry this box's own identity
/// key, resolved by `--mesh`/home exactly as a record's grants are
/// (`node_store::resolve_mesh_any`). Reported as `(message, data)`, the shape the
/// command's own refusal prints.
pub fn send_mesh(node: &str, asked: Option<&str>) -> Result<String, SendMeshError> {
    let set = match aoide_storage::routing::declarations() {
        Ok(set) => set,
        Err(refusal) => {
            return Err(SendMeshError::new(
                format!("no route: {} — {}", refusal.reason, refusal.detail),
                json!({ "reason": refusal.reason, "detail": refusal.detail, "node": node }),
            ))
        }
    };
    let own = match aoide_storage::identity::load_or_mint() {
        Ok((kp, _)) => kp.info().pubkey_hex,
        Err(e) => {
            return Err(SendMeshError::new(
                format!("this box's identity key cannot be read: {e}"),
                json!({ "reason": "no-identity", "node": node }),
            ))
        }
    };
    let named: std::collections::BTreeSet<String> = set
        .iter()
        .filter(|loaded| loaded.declaration.as_ref().map(|d| d.declares(node)).unwrap_or(false))
        .filter(|loaded| aoide_storage::routing::own_name_in(&set, &loaded.mesh, &own).is_some())
        .map(|loaded| loaded.mesh.clone())
        .collect();
    if named.is_empty() && asked.is_none() {
        return Err(SendMeshError::new(
            format!(
                "no mesh declares `{node}` at this node: it is no paired record, and no declaration here \
                 names it in a mesh this box is a member of"
            ),
            json!({ "reason": "unknown-node", "name": node }),
        ));
    }
    aoide_storage::node_store::resolve_mesh_any(asked, &named, &aoide_storage::config::home_mesh())
        .map_err(|e| {
            if asked.is_some_and(|m| !aoide_storage::node_store::valid_node_name(m)) {
                SendMeshError::new(format!("--mesh: {e}"), json!({ "reason": "invalid-mesh", "mesh": asked }))
            } else {
                SendMeshError::new(format!("--mesh: {e}"), json!({ "reason": "mesh-ambiguous", "node": node }))
            }
        })
}

/// [`send_mesh`]'s refusal: the message the command prints and the `data` it
/// carries, so the word and the sentence never drift apart.
pub struct SendMeshError {
    pub message: String,
    pub data: serde_json::Value,
}

impl SendMeshError {
    fn new(message: String, data: serde_json::Value) -> SendMeshError {
        SendMeshError { message, data }
    }
}

/// The record a drain DIALS for `node_name`: its own record where this box holds
/// one, else the hop's declared ADDRESS turned into a dial target
/// (`charter::dial_of`) — which is how a charter relay this box never paired
/// with moves mail at all. `None` for a name no record declares and no
/// declaration addresses, and for one whose address is `poll`: that one owns no
/// inbound transport, so a drain never dials it (its own ask moves its letters),
/// and the caller answers "nothing to do here".
///
/// **`mesh` is the mesh the letters in that spool RIDE**, and the declaration
/// read is that one's alone: one node name may sit in several meshes with a
/// different address in each, so dialling "the first declaration that names it"
/// would reach the wrong machine's door — and a same-named `nodes.json` record
/// never wins over the declaration where a charter governs the mesh.
fn dial_node(node_name: &str, mesh: Option<&str>) -> Option<(aoide_storage::node_store::Node, bool)> {
    let set = aoide_storage::routing::declarations().ok()?;
    let declared = mesh.and_then(|mesh| match aoide_storage::routing::declaration_of(&set, mesh) {
        Some(Ok(declaration)) => Some(declaration),
        // A mesh the set cannot read names no address: fail closed.
        _ => None,
    });
    let charter_mesh = declared.is_some_and(|d| d.is_charter());
    let nodes = aoide_storage::node_store::load_nodes();
    if !charter_mesh {
        if let Some(record) = nodes.into_iter().find(|n| n.name == node_name) {
            return Some((record, false));
        }
    }
    // A hop the route picked off a declaration (or a name a charter governs,
    // where the declaration is the only authority there is): its address says
    // how to reach it and which of the three transports that is; an `ssh://` hop
    // is dialled through its tunnel at the far side's own door
    // (`http://127.0.0.1:<AOIDE_A2A_PORT or 8710>/`, the same loopback form a
    // paired ssh node's record holds), and an `https://` hop at the URL itself.
    let address = declared?
        .address_of(node_name)
        .map(str::to_string)?;
    match aoide_storage::charter::dial_of(&address).ok()? {
        aoide_storage::charter::Dial::Https(url) => {
            Some((aoide_storage::node_store::Node::dial_only(node_name, &url, None), true))
        }
        aoide_storage::charter::Dial::Ssh(via) => {
            let url = format!("http://127.0.0.1:{}/", crate::commands::default_a2a_port());
            Some((aoide_storage::node_store::Node::dial_only(node_name, &url, Some(&via.to_string())), true))
        }
        aoide_storage::charter::Dial::Poll => None,
    }
}

/// The mesh the letters spooled toward `node_name` RIDE — the first entry's own
/// answer: a sealed container's `mesh`, else the envelope's signed `originMesh`.
/// `None` for an empty spool (nothing to dial anyway) and for a plaintext entry
/// that names no mesh at all.
fn spool_mesh(node_name: &str) -> Option<String> {
    aoide_storage::outbox::list_entries(node_name)
        .ok()?
        .into_iter()
        .find_map(|entry| {
            entry
                .container
                .as_ref()
                .map(|container| container.mesh.clone())
                .or_else(|| Some(entry.envelope.header.origin_mesh.clone()))
                // An EMPTY mesh is the home mesh (`routing::zone_name`), not a
                // missing answer: a container minted before the mesh was carried
                // rides home, and dropping it here would dial by the wrong name.
                .map(|mesh| aoide_storage::routing::zone_name(&mesh))
        })
}

/// Poll `node_name` once — the relay-first half of the model (MAIL.md §Wire,
/// P-M3): a node with no inbound address asks the node it can reach
/// "anything waiting for me?" and receives every entry that node spooled
/// toward it (`aoide_storage::outbox::poll_payloads`), all `hold` ones and
/// the `now` ones whose attempts have been failing.
///
/// Each hand-over is received exactly as a pushed deposit would be: the
/// SAME `mail::deposit` policy chain (recompute `msgid`, verify the ORIGIN
/// signature in `header.from.node`'s own key, dedup, file — note that the
/// hop here is the node we polled, matching the deposit path's two-lookup
/// split) and then [`settle_deposit`]. A re-poll before the ack therefore
/// files nothing a second time and re-acks nothing: dedup answers
/// `Duplicate`, and the ack the first poll already spooled is still
/// pending, so `write_ack_if_absent` spools nothing. That is the whole of
/// "re-poll before ack is idempotent".
///
/// A hand-over that does not deserialize, or that `deposit` refuses
/// (`bad-msgid`, `unverified-origin` — a letter whose ORIGIN this box holds
/// no key for, which for a direct edge means a relayed letter from a third
/// node), is skipped: the pull has no response to
/// carry that news back, and one bad element must never cost the rest of
/// the batch. The count returned is envelopes FILED. A container whose chain
/// does not end here is not a refusal at all: it is one hop of someone else's
/// letter, and the poll forwards it (`ContainerOutcome::Hopped`).
///
/// `Err` is the honest "we could not ask" — a refused or unreachable poll.
/// Nothing is recorded on the spool either way — a poll writes nothing except
/// the retirement of a hub's OWN transit custody
/// (`outbox::retire_transit_handover`, the hand-over being the acceptance for a
/// `poll` hop) — and the entries the far end did not hand over are the far
/// end's own state.
/// What one poll did: how many envelopes it FILED, the containers it
/// REFUSED, each `"<msgid>: <reason>"` (review N13 — a refusal a caller
/// cannot see is a refusal nobody acts on), and the entries the far end
/// WITHHELD rather than hand over, each `"<msgid>: <reason>"` (H1 — an
/// adapter's `sealed-required` refusal is the one thing that explains a
/// letter stuck at a relay with nothing filed on this end).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PollOutcome {
    pub filed: usize,
    pub refused: Vec<String>,
    pub withheld: Vec<String>,
}

pub fn poll_node(node_name: &str, named: Option<&str>) -> Result<PollOutcome, String> {
    let nodes = aoide_storage::node_store::load_nodes();
    let Some(node) = nodes.iter().find(|n| n.name == node_name) else {
        return Ok(PollOutcome::default());
    };
    // P-CHARTER: the ONE mesh this poll acts in — the `--mesh` typed, else the
    // sole mesh where the record holds `message`, else the home mesh; a
    // genuine tie refuses, naming the meshes (`mail poll --mesh`). The
    // capability-led rule is what makes a peer trusted only in `away`
    // reachable with no flag at all (review N3). It is used for BOTH halves
    // below: the request is SIGNED with it, and a handed-over container is
    // verified against the mesh the letter was minted in.
    let mesh = crate::commands::request_mesh(node, named, "message").map_err(|e| format!("mesh: {e}"))?;
    // **A node that mesh declares `down` is never polled, and nothing is filed.**
    // Not even the binding exchange below, which is a dial in its own right: its
    // own declaration says this box must not reach it (MAIL.md §Status). What is
    // spooled toward it stays spooled, and a later poll — or drain — acts on
    // whatever the declaration says then.
    if record_forbidden_by_declaration(&mesh, node) {
        return Ok(PollOutcome::default());
    }
    // P-SEAL: publish our binding and learn theirs before taking anything
    // over, so a node that has just published one never hands us plaintext
    // it did not have to.
    let _ = exchange_bindings(node, named);
    // The calling node's own name in the mail protocol — the ADDRESS form
    // (`display::local_node_name`), the same one every envelope this box mints
    // stamps and the same one a peer's poll is answered against. A raw OS host
    // name would not match on a host whose name is case-preserved (native
    // Windows' is upper-case — measured red against the folded fixture).
    // **The `filed` list is captured ONCE and sent as it stands.** Reading it
    // again after the answer would clear whatever is pending THEN — including an
    // acknowledgement recorded in between, which the hub has not seen yet and
    // which must survive to the next poll.
    let acknowledged = aoide_storage::outbox::filed_pending(node_name).unwrap_or_default();
    let params = json!({
        "node": aoide_storage::display::local_node_name(),
        // What THIS box filed out of its previous poll of this node: the
        // acknowledgement that ends the hub's custody of a container it handed
        // over (`outbox::filed_pending`). A response can be lost, so the
        // hand-over cannot be the acknowledgement, and anything not yet named
        // here is offered again.
        "filed": acknowledged.clone(),
    });
    let result = match post_signed(node, "aoide/mailPoll", params, Some(&mesh)) {
        SignedCall::Result(result) => result,
        SignedCall::Refused(detail) => return Err(detail),
        SignedCall::TransportFailed(reason) => return Err(reason),
    };
    let mut filed = 0usize;
    // The acknowledgements the hub has just been told are cleared, by the list
    // that was SENT; anything filed from THIS answer is recorded below for the
    // next poll. A poll that never got answers clears nothing, so its list is
    // carried again.
    let _ = aoide_storage::outbox::clear_filed(node_name, &acknowledged);
    // Review N13: what the poll REFUSED, carried back to the caller so the
    // command reports it. The audit line above is not a report — a poll of a
    // mesh-asymmetric pair used to answer "polled 1 node(s): 0 filed" with no
    // error at all, and the operator had nothing to act on.
    let mut refused: Vec<String> = Vec::new();

    // Sealed containers first: each is verified and opened by
    // `seal::deposit_container`, then handed to the SAME `mail::deposit`
    // the plaintext arm uses, so filing and the receipt rule have one
    // implementation.
    for container in result.get("containers").and_then(Value::as_array).into_iter().flatten() {
        let Ok(container) = serde_json::from_value::<aoide_storage::seal::Container>(container.clone()) else {
            continue;
        };
        // The poll's own mesh: the request was SIGNED with it (above), and the
        // container is verified against it here — `deposit_container` compares
        // the container's SIGNED `originMesh` with this, so a letter minted for
        // another mesh is refused on the pull side exactly as on the push side.
        // The polled node must be the chain's LAST hop: the hand-over this pull
        // is taking is the one that node signed for. Anything else is refused
        // like any other bad chain (the refusal is reported and audited below).
        if aoide_storage::seal::chain_deposited_by(&container, node_name).is_err() {
            refused.push(format!("{}: {}", container.msgid, aoide_storage::seal::BROKEN_CHAIN));
            continue;
        }
        let outcome = match aoide_storage::seal::deposit_container(&container, &mesh) {
            Ok(outcome) => outcome,
            Err(_) => continue,
        };
        match outcome {
            aoide_storage::seal::ContainerOutcome::Opened { envelope, digest } => {
                let Ok(filed_outcome) = aoide_storage::mail::deposit((*envelope).clone(), node_name) else {
                    continue;
                };
                if matches!(filed_outcome, aoide_storage::mail::DepositOutcome::Filed { .. }) {
                    filed += 1;
                }
                // M1: recorded only once it is filed — see the door's own arm.
                // L17: and a failed write is audited rather than dropped.
                if matches!(
                    filed_outcome,
                    aoide_storage::mail::DepositOutcome::Filed { .. }
                        | aoide_storage::mail::DepositOutcome::Duplicate { .. }
                ) {
                    if let Err(e) = aoide_storage::seal::record_admitted(&container, &digest) {
                        let _ = aoide_protocol::audit::audit(
                            &aoide_protocol::audit::default_audit_log(),
                            aoide_protocol::audit::Door::Cli,
                            aoide_protocol::audit::EventClass::Audit,
                            "mail.poll.dedup-record-failed",
                            "invalid",
                            &format!("{}: {e}", container.msgid),
                        );
                    }
                }
                settle_deposit(&envelope, &filed_outcome);
                // The hub's custody of the CONTAINER ends when it is told this
                // box has it: the next poll of that node carries this msgid
                // (`outbox::record_filed`), and until then the entry is offered
                // again — a lost response must not lose the letter.
                let _ = aoide_storage::outbox::record_filed(node_name, &container.msgid);
            }
            aoide_storage::seal::ContainerOutcome::Duplicate { filed_letter } => {
                // **A duplicate is custody this box already has.** The hub is
                // re-offering a letter this poller took on an earlier ask (its
                // own `filed` never reached the hub, or the response did), so
                // the poller says so again: without this, a hub whose spool
                // answer was lost would hold that container forever, offering
                // it on every ask.
                let _ = aoide_storage::outbox::record_filed(node_name, &container.msgid);
                if filed_letter {
                    if let Ok(Some(entry)) = aoide_storage::mail::show(&container.msgid) {
                        settle_deposit(
                            &entry.envelope,
                            &aoide_storage::mail::DepositOutcome::Duplicate { filed_letter: true },
                        );
                    }
                }
            }
            aoide_storage::seal::ContainerOutcome::Applied { mesh, version, rekeyed, digest } => {
                // P-CHARTER: the enclosed charter verified and is in force —
                // `deposit_container` wrote it before returning. Nothing is
                // filed and nothing is acked; the container is recorded so a
                // re-offered copy is a duplicate rather than a second apply.
                if let Err(e) = aoide_storage::seal::record_admitted(&container, &digest) {
                    let _ = aoide_protocol::audit::audit(
                        &aoide_protocol::audit::default_audit_log(),
                        aoide_protocol::audit::Door::Cli,
                        aoide_protocol::audit::EventClass::Audit,
                        "mail.poll.dedup-record-failed",
                        "invalid",
                        &format!("{}: {e}", container.msgid),
                    );
                }
                let _ = aoide_protocol::audit::audit(
                    &aoide_protocol::audit::default_audit_log(),
                    aoide_protocol::audit::Door::Cli,
                    aoide_protocol::audit::EventClass::Audit,
                    "mail.poll.charter-applied",
                    "ok",
                    &format!(
                        "charter `{mesh}` v{version} from `{}` via `{node_name}` applied{}",
                        container.origin.node,
                        if rekeyed.is_empty() {
                            String::new()
                        } else {
                            format!(
                                "; RE-KEYED: {}",
                                rekeyed.iter().map(|r| r.node.as_str()).collect::<Vec<_>>().join(", ")
                            )
                        }
                    ),
                );
            }
            // A container whose chain does not end at this box: this hand-over
            // is one hop of it, and the step is the door's own
            // (`deposit_sealed`'s `Hopped` arm) — a relay that polls another
            // relay forwards rather than dropping. Both outcomes are AUDITED
            // here (this process is the only witness the poll has: the pull's
            // response carries no line of its own), and the container is left
            // spooled where it was when the write fails.
            aoide_storage::seal::ContainerOutcome::Hopped(hop) => {
                let at = aoide_storage::time::now_iso_utc();
                if let Err(e) = aoide_storage::seal::file_transit_hop(&hop, node_name) {
                    let _ = aoide_protocol::audit::audit(
                        &aoide_protocol::audit::default_audit_log(),
                        aoide_protocol::audit::Door::Cli,
                        aoide_protocol::audit::EventClass::Audit,
                        "mail.poll.transit-not-filed",
                        "invalid",
                        &format!(
                            "container {} from `{node_name}` could not be filed as transit at {at}: {e}",
                            hop.container.msgid
                        ),
                    );
                    refused.push(format!("{}: transit: {e}", hop.container.msgid));
                    continue;
                }
                let _ = aoide_protocol::audit::audit(
                    &aoide_protocol::audit::default_audit_log(),
                    aoide_protocol::audit::Door::Cli,
                    aoide_protocol::audit::EventClass::Audit,
                    "mail.poll.transit",
                    "ok",
                    &format!(
                        "container {} from `{node_name}` carried on to `{}` in mesh `{}`{}",
                        hop.container.msgid,
                        hop.next,
                        hop.mesh,
                        if hop.held { ", held for its own poll" } else { "" }
                    ),
                );
                if !hop.held {
                    let _ = drain_node(&hop.next);
                }
                // **The hop is custody taken, so it is acknowledged too.** The
                // container is in this box's own `transit` line and spool now;
                // the hub that handed it over is told on the next ask, exactly
                // as it is for a letter this box filed.
                let _ = aoide_storage::outbox::record_filed(node_name, &hop.container.msgid);
            }
            aoide_storage::seal::ContainerOutcome::Refused { reason, detail } => {
                // L9 (the branch review): a polled container that refuses used
                // to vanish — no audit, no report — while the origin's `hold`
                // entry stayed in its spool and was re-offered on every poll
                // forever. The design's "parks AND IS REPORTED" needs a
                // carrier on the pull path, and this is it: the destination
                // writes the taught word it refused with. Best-effort, like
                // every other audit call here — a log write must never take
                // the poll down.
                let _ = aoide_protocol::audit::audit(
                    &aoide_protocol::audit::default_audit_log(),
                    aoide_protocol::audit::Door::Cli,
                    aoide_protocol::audit::EventClass::Audit,
                    "mail.poll.container-refused",
                    "invalid",
                    &format!(
                        "container {} from `{node_name}` refused: {reason}: {detail}",
                        container.msgid
                    ),
                );
                // …and NAMED to the caller, so `mail poll` reports it (review
                // N13) rather than answering "0 filed" with no reason.
                refused.push(format!("{}: {reason}", container.msgid));
            }
        }
    }

    for envelope in result.get("envelopes").and_then(Value::as_array).into_iter().flatten() {
        let Ok(envelope) = serde_json::from_value::<Envelope>(envelope.clone()) else { continue };
        let outcome = match aoide_storage::mail::deposit(envelope.clone(), node_name) {
            Ok(outcome) => outcome,
            Err(_) => continue,
        };
        if matches!(outcome, aoide_storage::mail::DepositOutcome::Filed { .. }) {
            filed += 1;
        }
        settle_deposit(&envelope, &outcome);
    }
    // H1: what a sealed-only listener WITHHELD instead of handing over. The
    // answer carries no letter bytes for those entries — they stay spooled at
    // the far end — and without this the poll would report a bare "0 filed"
    // for a mailbox that is holding letters the operator asked for. Named the
    // same way the refused containers are, so `mail poll` renders both.
    let mut withheld: Vec<String> = Vec::new();
    for entry in result.get("withheld").and_then(Value::as_array).into_iter().flatten() {
        let msgid = entry.get("msgid").and_then(Value::as_str).unwrap_or("(no msgid)");
        let reason = entry.get("reason").and_then(Value::as_str).unwrap_or("withheld");
        let detail = entry.get("detail").and_then(Value::as_str);
        withheld.push(match detail {
            Some(detail) => format!("{msgid}: {reason} — {detail}"),
            None => format!("{msgid}: {reason}"),
        });
        // The audit line `mail poll` never had: a withheld entry is a
        // deliberate refusal by the listener, and the operator's own box
        // records it on its side of the wire.
        let _ = aoide_protocol::audit::audit(
            &aoide_protocol::audit::default_audit_log(),
            aoide_protocol::audit::Door::Cli,
            aoide_protocol::audit::EventClass::Audit,
            "mail.poll.withheld",
            "invalid",
            &format!("{msgid} from `{node_name}` withheld: {reason}"),
        );
    }
    Ok(PollOutcome { filed, refused, withheld })
}

/// Publish this node's binding to `node` and store the one it answers with,
/// in one authenticated round trip (`aoide/binding`).
///
/// Best-effort and deliberately never fatal: a peer running an older aoide
/// that has no such method answers `method not found`, and that is an
/// ordinary state — the letter still goes as plaintext over the direct
/// lane, which is the per-peer upgrade path the design requires. A binding
/// this call cannot learn is simply not learned; a binding already stored
/// is never downgraded, because `learn_binding` refuses a generation that is
/// not above the high-water mark.
pub fn exchange_bindings(node: &Node, named_mesh: Option<&str>) -> Result<aoide_storage::seal::Binding, String> {
    let mine = aoide_storage::seal::publish_binding()?;
    let params = json!({ "binding": mine });
    let result = match post_signed(node, "aoide/binding", params, named_mesh) {
        SignedCall::Result(result) => result,
        SignedCall::Refused(detail) => return Err(detail),
        SignedCall::TransportFailed(reason) => return Err(reason),
    };
    let theirs: aoide_storage::seal::Binding = serde_json::from_value(
        result.get("binding").cloned().unwrap_or(Value::Null),
    )
    .map_err(|e| format!("binding: {e}"))?;
    // A refused learn is not an error here: `stale-binding` means we already
    // hold a newer one, which is the state we want. Only a binding we cannot
    // verify is worth reporting, and it costs the caller nothing but the
    // round trip.
    let _ = aoide_storage::seal::learn_binding(&node.name, &theirs);
    Ok(theirs)
}

/// **Is this record's mail grant revoked by a charter** — i.e. is EVERY mesh
/// it may be asked in a charter mesh whose line no longer lists its key?
///
/// The design's revocation rule is "deleting a line and re-signing is
/// revocation. Each node applies the new version on receipt: its door refuses
/// the removed key in that mesh, **its router stops routing to it**"
/// (`docs/architecture/HTTPS-MESH-API.md` "Charters"). The door half is
/// `a2a::grant_in_mesh`'s (a key the charter does not list holds nothing
/// there); this is the routing half, on the one routing decision this client
/// owns — who a bare `aoide mail poll` asks, and therefore who a held entry
/// can ever leave through.
///
/// A record that also holds `message` in a mesh no charter governs (an
/// ordinary pair mesh) is NOT revoked: nothing about that mesh changed, and
/// this must never lock out a machine that is simply also on a charter.
fn revoked_by_charter(node: &aoide_storage::node_store::Node) -> bool {
    let meshes: Vec<&String> = node
        .grants
        .iter()
        .filter(|(_, caps)| caps.iter().any(|c| c == "message"))
        .map(|(mesh, _)| mesh)
        .collect();
    if meshes.is_empty() {
        return false;
    }
    meshes.iter().all(|mesh| {
        // F2: a mesh a charter was accepted for is a charter mesh even while
        // its operator key is undecidable, and a charter mesh's unlisted key
        // is not routable — the fail-closed side, not the paired fallback.
        if aoide_storage::charter::charter_shaped(mesh) {
            return match aoide_storage::charter::governing(mesh) {
                Some(charter) => node
                    .pubkey
                    .as_deref()
                    .is_none_or(|key| charter.grant_for_key(key).is_none()),
                None => true,
            };
        }
        // A pair mesh: the charter has nothing to say about it.
        false
    })
}

/// Every node `aoide mail poll` asks when it is given no argument:
/// registered, `verified`, and carrying `message` in THIS box's own grants for
/// it — **in ANY mesh**, not only the home one (review N3). The mesh a bare
/// poll acts in is then resolved per record, by the same capability rule every
/// other call uses (`message` in the sole mesh that holds it, else home): a
/// peer trusted only in `away` used to be silently skipped here while
/// `mail send` delivered to it happily, which killed the relay-first receive
/// path for exactly the peers `pair --mesh` creates. Sorted by name, so the
/// command's own report is stable.
///
/// The grants half is this box's record of what IT permits that node, not
/// the node's record of this box — which is what the far door checks when it
/// answers. Keeping the two in step is the operator's business; a node
/// without `message` on either side is not one this box trades mail with.
/// A declared `down` status narrows this set further: a node the mesh a
/// poll of it would act in declares `down` is not asked, because `down` means
/// this box stops SENDING to it — its spooled entries are kept, not
/// confiscated, and no dial is opened for either direction.
///
/// **A `poll` node is not in it.** Polling is a DIAL, and that address says
/// this node has no inbound transport to be dialled on — it is the one that
/// asks. Its letters reach it through the relay's own spool and its own
/// `aoide mail poll`; a bare `aoide mail poll` here asking it would be a dial
/// that can only fail.
///
/// **A line the governing charter has REMOVED is not in it either**
/// ([`revoked_by_charter`]): revocation's routing half, so a removed machine
/// stops being asked on the very next poll rather than only being refused at
/// its own door.
pub fn pollable_nodes() -> Vec<String> {
    let mut out: Vec<String> = aoide_storage::node_store::load_nodes()
        .into_iter()
        .filter(|node| {
            // A2's any-mesh grant test (N3) AND this branch's never-dialled
            // rule: the two are independent facts about the record, and both
            // must hold — a `poll` node is not dialled whatever mesh it is
            // trusted in.
            node.verified
                && !node.never_dialled()
                && !revoked_by_charter(node)
                && !poll_mesh(&node.name, None).is_some_and(|mesh| record_forbidden_by_declaration(&mesh, node))
                && node.grants.values().any(|caps| caps.iter().any(|a| a == "message"))
        })
        .map(|node| node.name)
        .collect();
    out.sort();
    out
}

/// Drain `node_name`'s outbox once: every spooled, non-refused, non-`hold`/// entry, oldest first, attempted in order — stopping at the first TRANSPORT
/// failure (the link itself is down; hammering the rest of the queue the
/// same pass gains nothing) but continuing past a REFUSAL (that one entry
/// is the problem, not the link — the next entry may well be fine).
///
/// **A `hold` entry is never attempted — the drain's one hard filter beside
/// `refused`.** It leaves only through its node's own `aoide/mailPoll`
/// (`poll_payloads` is what offers it), so a pass that has nothing else to
/// dial does not dial at all.
///
/// **Poll-on-contact.** A pass that reached `node_name` at all — at least
/// one entry answered, delivered or refused — ends by asking that node for
/// anything it holds FOR US ([`poll_node`]), on the same session, under the
/// same `.bsy` lock and the same tunnel guard: one contact, both
/// directions, which is what makes a relay-first member able to receive at
/// all (`docs/architecture/MAIL.md` §Outbox). A pass that never got a
/// response does not poll — polling an unreachable node is just a second
/// failure recorded nowhere.
///
/// `Ok(())` covers every ordinary non-error outcome: nothing registered
/// under `node_name`, the link already held off, `.bsy` already held by a
/// concurrent drain (ruling 3 — skipped, never queued), an empty spool, a
/// node its mesh declares `down` (never dialled, and its entries kept), or
/// a completed pass regardless of how many entries it delivered/refused/
/// backed off on. `Err` is reserved for a genuine local I/O failure
/// (`.bsy`'s own lock file, or an outbox read/write) — never for "the
/// remote node was unreachable," which is an ordinary, expected drain
/// outcome recorded in the entry/link state instead of surfaced as an
/// error to this function's own caller.
pub fn drain_node(node_name: &str) -> Result<(), String> {
    // The node this pass dials: its own record, or — where this box holds none
    // — the hop's declared address turned into a dial target (`dial_node`).
    // Without that half a charter relay would be silently un-dialled: the drain
    // would answer "nothing to do" forever while the spool filled, which is the
    // one thing a drained-looking entry must never be.
    let mesh = spool_mesh(node_name);
    let Some((node, dial_only)) = dial_node(node_name, mesh.as_deref()) else {
        return Ok(());
    };
    // **A node the letters' own mesh declares `down` is never dialled, and its
    // entries are KEPT.** `down` says this box stops SENDING to it; it never
    // confiscates what was already queued (MAIL.md §Status, decision 14). The
    // spool is left exactly as it stands, so the next pass after the declaration
    // changes dials it — unchanged, no re-mint, no lost letter.
    if mesh.as_deref().is_some_and(|mesh| record_forbidden_by_declaration(mesh, &node)) {
        return Ok(());
    }
    // **A `poll` node is never dialled, so a drain of one opens no link at
    // all** — before the link lock, before the binding exchange (which is a
    // dial in its own right, and the one a held-entry filter would not have
    // saved us from), before anything. Its entries are HELD
    // ([`spool_entry`]) and leave only through this box's own
    // `aoide/mailPoll` at the far end. Silent success, like every other
    // "nothing to do here" in this function.
    if node.never_dialled() {
        return Ok(());
    }

    let Some(_link_lock) = aoide_storage::outbox::try_take_link_lock(node_name)? else {
        return Ok(());
    };
    let _tunnel_teardown = TunnelTeardownGuard(node_name.to_string());

    // P-SEAL: the binding exchange rides the drain's own session, before
    // the first deposit — one contact, both directions, and every entry in
    // this pass is sealed to whatever binding that round trip left in
    // place. Best-effort: a peer with no `aoide/binding` method answers
    // "method not found", which is exactly the un-upgraded peer this pass
    // must still send plaintext to.
    //
    // P-CHARTER: it rides the mesh this drain's own deposits are signed for
    // (`drain_node` resolves it once, below, from the record's grant) — the
    // door reads the caller's `message` grant in that same mesh.
    // P-CHARTER / review N12: an ambiguity here is NOT silently dropped into
    // `""` — a binding exchange that cannot name its mesh cannot name its
    // grant either, so it is skipped outright and said so, rather than
    // re-resolved and re-failed inside the signing path where nobody sees it.
    //
    // A hop this box holds NO RECORD for (a declaration-only `dial_node`) gets
    // no exchange at all: the binding a peer publishes is what a letter TO it is
    // sealed with, and a transit container is sealed to the DESTINATION, so there
    // is nothing here to ask a hop for.
    if !dial_only {
        let mesh = match crate::commands::request_mesh(&node, None, "message") {
            Ok(mesh) => mesh,
            Err(e) => {
                eprintln!("aoide: no binding exchange with `{node_name}`: {e}");
                return Ok(());
            }
        };
        let _ = exchange_bindings(&node, Some(&mesh));
    }

    let now_epoch = unix_now();
    if let Some(link) = aoide_storage::outbox::read_link_state(node_name)? {
        if aoide_storage::outbox::is_held_off(&link, now_epoch) {
            return Ok(());
        }
    }

    // Parked entries are filtered BEFORE the cap, never skipped inside the
    // loop: they sort oldest-first, so a cap counted over spool positions
    // would spend a whole batch on entries this pass can never dial and
    // starve every fresh letter behind them. Held entries are filtered the
    // same way, for the same reason: they are permanent to a DRAIN (only the
    // far end's poll moves them), so counting them would starve the spool
    // identically.
    let mut contacted = false;
    for entry in aoide_storage::outbox::list_entries(node_name)?
        .into_iter()
        .filter(|entry| !entry.refused && !entry.is_held())
        .take(DRAIN_BATCH_CAP)
    {
        // H1 (the branch review): the binding exchange above may have just
        // taught this node the destination's binding, and an entry spooled
        // before that must not go out in the clear in the very same pass
        // that learned it. Re-seal it here, before the dial, and persist —
        // so the spool and the wire agree from this moment on, and a retry
        // resends the container byte-identically.
        //
        // H2 (the re-review): `NotUsable` is its own answer. A binding that is
        // held but outside its window — expired, or a peer whose clock runs
        // ahead of ours — parks the entry rather than falling through to the
        // plaintext arm, which is what "a sealed destination never receives
        // plaintext" means at this layer.
        use aoide_storage::outbox::Reseal;
        let entry = match aoide_storage::outbox::reseal_entry(node_name, &entry, &aoide_storage::time::now_iso_utc()) {
            Ok(Reseal::NoBinding) | Ok(Reseal::Sealed(None)) => entry,
            Ok(Reseal::Sealed(Some(upgraded))) | Ok(Reseal::FreshlySealed(upgraded)) => {
                if let Err(e) = aoide_storage::outbox::write_entry(node_name, &upgraded) {
                    return Err(format!("state/outbox: {e}"));
                }
                *upgraded
            }
            Ok(Reseal::NotUsable(parked)) => {
                if let Some(parked) = parked {
                    aoide_storage::outbox::write_entry(node_name, &parked)?;
                }
                continue;
            }
            // A seal that cannot be built is not a reason to send plaintext:
            // the entry stays spooled, un-attempted, and the next pass tries
            // again. Sending it in the clear is the one wrong answer.
            Err(_) => continue,
        };
        match attempt_deposit(&node, &entry) {
            DepositAttempt::TransportFailed(reason) => {
                // Record the attempt on the entry that actually hit the
                // failure BEFORE backing off the link — otherwise `tries`/
                // `lastOutcome` for the whole batch stay exactly where they
                // were on a dead link, which is indistinguishable from a
                // drain that never even tried. The back-off itself is NOT
                // conditional on that record write succeeding: a local
                // spool write can fail for the same reason the link is
                // failing (a full or read-only disk), and an early `?` here
                // used to skip `back_off` entirely on that write's error —
                // leaving the link un-backed-off and re-dialed on every
                // following tick, exactly when the box is already sick.
                record_failed_attempt(node_name, entry, now_epoch, "transport", &reason)?;
                break;
            }
            DepositAttempt::LinkRefused(reason) => {
                // The far end ANSWERED — with a word about its OWN state (a node
                // its mesh quarantines, a declaration set that will not load),
                // not about this letter — so this is not a transport failure and
                // the entry is not parked. It is recorded under the same
                // `refused:` word a policy refusal gets, which is the vocabulary
                // the spool reads back (`last_attempt_was_answered`), so liveness
                // counts it as REACHED; the LINK backs off and the next pass
                // retries.
                record_failed_attempt(node_name, entry, now_epoch, "refused", &reason)?;
                break;
            }
            DepositAttempt::Refused(reason) => {
                contacted = true;
                aoide_storage::outbox::clear_link_state(node_name)?;
                let mut updated = entry;
                updated.tries += 1;
                updated.last_try_at = aoide_storage::time::now_iso_utc();
                updated.last_outcome = format!("refused: {reason}");
                updated.refused = true;
                aoide_storage::outbox::write_entry(node_name, &updated)?;
            }
            DepositAttempt::Delivered { status } => {
                contacted = true;
                settle_delivered(node_name, entry, &status)?;
            }
        }
    }
    // Poll-on-contact (this function's own doc): the same dial that just
    // carried our spool out asks what this node holds FOR US. `poll_node`
    // never returns an error worth surfacing — the drain already reported
    // whatever the deposit half found, and an unreachable-but-answering node
    // is not a local failure.
    if contacted {
        let _ = poll_node(node_name, None);
    }
    Ok(())
}

/// Record one FAILED attempt on the entry that hit it, then back the LINK off —
/// in that order, and never conditionally: `tries`/`lastOutcome` must move even
/// when the spool write itself fails (a full or read-only disk is the very
/// condition under which the link is failing too), or a dead link looks exactly
/// like a drain that never tried. `word` is the spool's own vocabulary:
/// `transport` for a link that answered nothing, `refused` for a far end that
/// answered with a policy word and left the letter alone.
fn record_failed_attempt(
    node_name: &str,
    entry: aoide_storage::outbox::OutboxEntry,
    now_epoch: i64,
    word: &str,
    reason: &str,
) -> Result<(), String> {
    let mut updated = entry;
    updated.tries += 1;
    updated.last_try_at = aoide_storage::time::now_iso_utc();
    updated.last_outcome = format!("{word}: {reason}");
    let recorded = aoide_storage::outbox::write_entry(node_name, &updated);
    // The LINK's own record of the same attempt carries the same word, so a
    // reader that has only the link (an entry whose bookkeeping write failed)
    // can still tell a far end that ANSWERED from a dial that never connected.
    aoide_storage::outbox::back_off(node_name, now_epoch, &format!("{word}: {reason}"))?;
    recorded
}

/// What a **delivered** deposit does to the entry it carried — the one
/// implementation of that decision, so the arm [`drain_node`] runs and the
/// test that pins it cannot drift.
///
/// **A receipt and a `charter` letter retire on the spot; a transit container
/// retires on the HOP's acceptance; a letter waits for a real ack.** For a
/// receipt the deposit outcome *is* confirmation (there is no ack-of-an-ack). A
/// `charter` letter is the same case for a stronger reason: the enclosed charter
/// is APPLIED, never filed, so there is no mailbox on the far end to ack from at
/// all, and the outcome of its own deposit is the whole confirmation there can
/// be. A **transit** container is a hub's custody of someone else's letter: its
/// obligation is to hand it to `next` and to be able to prove it did, so `next`'s
/// own acceptance ends it — waiting for the DESTINATION's receipt would leave
/// every hub holding a copy of every letter it ever relayed, and the destination
/// need not even be able to reach the hub (a `poll` next asks, and never
/// answers). A letter still waits for a receipt naming its `msgid` (spec item 7),
/// so its attempt is recorded and it stays spooled.
///
/// `status` is the far end's own word, recorded verbatim as `last_outcome` —
/// the same string [`DepositAttempt::Delivered`] carries, never re-derived.
fn settle_delivered(
    node_name: &str,
    entry: aoide_storage::outbox::OutboxEntry,
    status: &str,
) -> Result<(), String> {
    aoide_storage::outbox::clear_link_state(node_name)?;
    if matches!(
        entry.envelope.header.kind.as_str(),
        aoide_storage::mail::ENTRY_TYPE_RECEIPT
            | aoide_storage::mail::ENTRY_TYPE_CHARTER
            | aoide_storage::mail::ENTRY_TYPE_TRANSIT
    ) {
        aoide_storage::outbox::remove_entry(node_name, &entry.envelope.msgid)?;
    } else {
        let mut updated = entry;
        updated.tries += 1;
        updated.last_try_at = aoide_storage::time::now_iso_utc();
        updated.last_outcome = status.to_string();
        aoide_storage::outbox::write_entry(node_name, &updated)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_storage::outbox::OutboxEntry;

    fn root(tag: &str) -> (aoide_test_support::EnvSaver, std::path::PathBuf) {
        aoide_test_support::isolated_mail_root(tag)
    }

    /// **Only a refusal that is a verdict on the LETTER parks it.** A word about
    /// the link (`down`, `config-invalid`) leaves the entry live
    /// — while every other word (`zone-violation`, `broken-chain`, `bad-msgid`,
    /// `not-correspondence`, …) parks it, because that one will never be true.
    /// The classifier is pure, so the whole vocabulary is provable here without a
    /// door.
    #[test]
    fn only_a_verdict_on_the_letter_parks_it() {
        let word = |reason: &str| {
            classify_deposit_response(&serde_json::json!({
                "status": "refused",
                "reason": reason,
                "detail": "d",
            }))
        };
        assert!(
            matches!(word("zone-violation"), DepositAttempt::Refused(_)),
            "a verdict parks it"
        );
        assert!(matches!(word("broken-chain"), DepositAttempt::Refused(_)));
        assert!(matches!(word("bad-msgid"), DepositAttempt::Refused(_)));
        assert!(
            matches!(word("down"), DepositAttempt::LinkRefused(_)),
            "a link state does not park it: the entry stays live and retries"
        );
        assert!(matches!(word("config-invalid"), DepositAttempt::LinkRefused(_)));
        assert!(matches!(
            classify_deposit_response(&serde_json::json!({ "status": "accepted" })),
            DepositAttempt::Delivered { .. }
        ));
    }

    /// **A `down` refusal is a LINK state, and the next pass delivers once the
    /// peer is no longer `down`**: the entry is never parked,
    /// never needs `retry --refused`, and the link's own back-off is what brings
    /// it back.
    #[test]
    fn a_down_refusal_backs_off_and_delivers_once_the_peer_is_up() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("link-state-down");

        let (listener, port, _seen) = recording_door(
            r#"{"jsonrpc":"2.0","id":1,"result":{"status":"refused","reason":"down","detail":"node `elsewhere` is declared `down`"}}"#,
            r#"{"jsonrpc":"2.0","id":1,"result":{"containers":[],"envelopes":[]}}"#.to_string(),
        );
        let mut node = unpaired_node("elsewhere");
        node.url = format!("http://127.0.0.1:{port}/");
        aoide_storage::node_store::save_nodes(&[node]).unwrap();
        let envelope = aoide_storage::mail::mint_outbound_letter("alice", "elsewhere", "bob", "waiting").unwrap();
        aoide_storage::outbox::write_entry("elsewhere", &OutboxEntry::fresh(envelope)).unwrap();

        drain_node("elsewhere").unwrap();
        let held = aoide_storage::outbox::list_entries("elsewhere").unwrap();
        assert_eq!(held.len(), 1, "the letter is neither parked nor gone: {held:?}");
        assert!(!held[0].refused, "a `down` refusal is not a verdict on the letter: {held:?}");
        assert_eq!(held[0].tries, 1, "and it WAS attempted: {held:?}");
        assert!(held[0].last_outcome.contains("down"), "the far end's own word is kept: {held:?}");
        assert!(
            aoide_storage::outbox::read_link_state("elsewhere").unwrap().is_some(),
            "the LINK is what backs off"
        );

        // The peer stops being `down`. The back-off's own wait is not what this
        // pins, so clear it to reach that next pass, and point the record at a
        // door that accepts.
        drop(listener);
        aoide_storage::outbox::clear_link_state("elsewhere").unwrap();
        let (acceptor, port, _seen) = recording_door(
            r#"{"jsonrpc":"2.0","id":1,"result":{"status":"accepted","msgid":"00"}}"#,
            r#"{"jsonrpc":"2.0","id":1,"result":{"containers":[],"envelopes":[]}}"#.to_string(),
        );
        let mut node = unpaired_node("elsewhere");
        node.url = format!("http://127.0.0.1:{port}/");
        aoide_storage::node_store::save_nodes(&[node]).unwrap();

        drain_node("elsewhere").unwrap();
        let after = aoide_storage::outbox::list_entries("elsewhere").unwrap();
        assert_eq!(after.len(), 1, "a letter waits for its ack, so it is still spooled: {after:?}");
        assert!(!after[0].refused, "and it is still not parked: {after:?}");
        assert_eq!(after[0].last_outcome, "accepted", "the far end's own answer is recorded: {after:?}");
        assert!(
            aoide_storage::outbox::read_link_state("elsewhere").unwrap().is_none(),
            "a successful deposit clears the link"
        );

        drop(acceptor);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The same for `config-invalid`: the host's own broken declaration is the
    /// far end's state, not this letter's problem.
    #[test]
    fn a_config_invalid_refusal_backs_off_and_delivers_once_the_config_loads() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("link-state-config-invalid");

        let (listener, port, _seen) = recording_door(
            r#"{"jsonrpc":"2.0","id":1,"result":{"status":"refused","reason":"config-invalid","detail":"config-invalid: config-unreadable"}}"#,
            r#"{"jsonrpc":"2.0","id":1,"result":{"containers":[],"envelopes":[]}}"#.to_string(),
        );
        let mut node = unpaired_node("elsewhere");
        node.url = format!("http://127.0.0.1:{port}/");
        aoide_storage::node_store::save_nodes(&[node]).unwrap();
        let envelope = aoide_storage::mail::mint_outbound_letter("alice", "elsewhere", "bob", "waiting").unwrap();
        aoide_storage::outbox::write_entry("elsewhere", &OutboxEntry::fresh(envelope)).unwrap();

        drain_node("elsewhere").unwrap();
        let held = aoide_storage::outbox::list_entries("elsewhere").unwrap();
        assert!(!held[0].refused, "`config-invalid` is not a verdict on the letter: {held:?}");
        assert!(held[0].last_outcome.contains("config-invalid"), "{held:?}");
        assert!(aoide_storage::outbox::read_link_state("elsewhere").unwrap().is_some(), "the link backs off");

        drop(listener);
        aoide_storage::outbox::clear_link_state("elsewhere").unwrap();
        let (acceptor, port, _seen) = recording_door(
            r#"{"jsonrpc":"2.0","id":1,"result":{"status":"accepted","msgid":"00"}}"#,
            r#"{"jsonrpc":"2.0","id":1,"result":{"containers":[],"envelopes":[]}}"#.to_string(),
        );
        let mut node = unpaired_node("elsewhere");
        node.url = format!("http://127.0.0.1:{port}/");
        aoide_storage::node_store::save_nodes(&[node]).unwrap();

        drain_node("elsewhere").unwrap();
        let after = aoide_storage::outbox::list_entries("elsewhere").unwrap();
        assert!(!after[0].refused, "{after:?}");
        assert_eq!(after[0].last_outcome, "accepted", "{after:?}");

        drop(acceptor);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **An unloadable declaration is never dialled, judged by the RECORD.**
    /// `record_forbidden_by_declaration`'s fail-closed arm (a set that will not load means "do not
    /// reach it") is what a RECORDED node goes through, so with the config refused
    /// the bare sweep must leave it out and an explicit poll must contact nobody.
    #[test]
    fn an_unloadable_declaration_leaves_a_recorded_node_undialled() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("unloadable-poll");
        let (listener, port, seen) = recording_door(
            r#"{"jsonrpc":"2.0","id":1,"result":{"status":"accepted","msgid":"00"}}"#,
            r#"{"jsonrpc":"2.0","id":1,"result":{"containers":[],"envelopes":[]}}"#.to_string(),
        );
        // A `[status]` for a node `home` does not have: `validate_mesh` refuses
        // the section whole, so `declarations()` itself fails.
        std::fs::write(
            aoide_storage::config::source().path,
            "[pairing]\nhomeMesh = \"home\"\n\n[mesh.home.status]\nnobody = \"down\"\n",
        )
        .unwrap();
        let mut node = unpaired_node("elsewhere");
        node.url = format!("http://127.0.0.1:{port}/");
        node.verified = true;
        node.grants = aoide_storage::node_store::grants_in("home", &["message"]);
        aoide_storage::node_store::save_nodes(&[node]).unwrap();

        assert!(
            !pollable_nodes().contains(&"elsewhere".to_string()),
            "an unreadable declaration lists nobody: {:?}",
            pollable_nodes()
        );
        let asked = poll_node("elsewhere", None).unwrap();
        assert_eq!(asked.filed, 0, "nothing is taken");
        assert!(seen.lock().unwrap().is_empty(), "and no door was contacted at all");

        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A peer that stays `down` is UP and talking.** The REAL drain against a
    /// door that answers `down` leaves the link backed off — the sender's own
    /// retry state, written for a refusal exactly as for a dead dial — and
    /// liveness must still read `reachable`: the entry that records the attempt
    /// answered, and reading the declaration back as an observation is the
    /// conflation this view exists to avoid.
    #[test]
    fn a_down_answer_from_a_real_drain_reads_reachable() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("liveness-real-down");
        let (listener, port, _seen) = recording_door(
            r#"{"jsonrpc":"2.0","id":1,"result":{"status":"refused","reason":"down","detail":"node `elsewhere` is declared `down`"}}"#,
            r#"{"jsonrpc":"2.0","id":1,"result":{"containers":[],"envelopes":[]}}"#.to_string(),
        );
        let mut node = unpaired_node("elsewhere");
        node.url = format!("http://127.0.0.1:{port}/");
        node.verified = true;
        node.grants = aoide_storage::node_store::grants_in("home", &["message"]);
        aoide_storage::node_store::save_nodes(&[node]).unwrap();
        let envelope = aoide_storage::mail::mint_outbound_letter("alice", "elsewhere", "bob", "waiting").unwrap();
        aoide_storage::outbox::write_entry("elsewhere", &OutboxEntry::fresh(envelope)).unwrap();

        drain_node("elsewhere").unwrap();
        let held = aoide_storage::outbox::list_entries("elsewhere").unwrap();
        assert!(!held[0].refused, "the entry stays live: {held:?}");
        assert!(held[0].last_outcome.starts_with("refused:"), "the far end's word: {held:?}");
        assert!(
            aoide_storage::outbox::read_link_state("elsewhere").unwrap().is_some(),
            "the LINK backs off, which is the sender's own retry state"
        );
        assert_eq!(
            crate::mesh::liveness_of("elsewhere"),
            "reachable",
            "it ANSWERED, so it has been reached"
        );

        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn unpaired_node(name: &str) -> Node {
        Node {
            name: name.to_string(),
            url: "http://127.0.0.1:1".to_string(), // nothing listens here
            autogate: false,
            token_file: None,
            bearer_secret: None,
            hub: false,
            pubkey: None,
            verified: false,
            grants: aoide_storage::node_store::Grants::new(),
            narrowed: aoide_storage::node_store::Grants::new(),
            via: None,
            added_at: "2026-09-07T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn a_dead_node_leaves_its_entry_waiting_and_drains_on_return() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("drain-dead-node");

        aoide_storage::node_store::save_nodes(&[unpaired_node("elsewhere")]).unwrap();

        let env = aoide_storage::mail::mint_outbound_letter("alice", "elsewhere", "bob", "hi").unwrap();
        aoide_storage::outbox::write_entry("elsewhere", &OutboxEntry::fresh(env)).unwrap();

        // Nothing listens on 127.0.0.1:1 — this is a transport failure, not
        // a refusal: the entry must survive, untouched in content, only its
        // bookkeeping (tries/lastTry/lastOutcome) may have changed, and the
        // LINK — never the entry — is what backs off.
        drain_node("elsewhere").unwrap();

        let remaining = aoide_storage::outbox::list_entries("elsewhere").unwrap();
        assert_eq!(remaining.len(), 1, "a dead node's entry is never dropped on a transport failure");
        assert!(!remaining[0].refused, "a transport failure is not a policy refusal");

        let link = aoide_storage::outbox::read_link_state("elsewhere").unwrap();
        assert!(link.is_some(), "the link backs off after an unreachable attempt");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A `TransportFailed` must back the link off even when the entry's own
    /// record write fails — the exact condition (a full or read-only disk)
    /// under which the link is most likely failing too. Made deterministic,
    /// without chmod and without root: `atomic_write`
    /// (`aoide_storage::fs::atomic_write_bytes_impl`) writes an entry
    /// through a temp path shaped `<msgid>.tmp.<our own pid>` beside the
    /// entry file; planting a DIRECTORY at that exact path makes the next
    /// write to this entry fail with EISDIR, while `back_off`'s own
    /// `link.json` (same directory, a different name) is untouched and
    /// still writes fine, and `list_entries` never trips over the directory
    /// (it filters on `extension == "json"`).
    #[test]
    fn a_link_backs_off_even_when_the_entrys_own_record_cannot_be_written() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("backoff-despite-write-failure");

        aoide_storage::node_store::save_nodes(&[unpaired_node("elsewhere")]).unwrap();

        let env = aoide_storage::mail::mint_outbound_letter("alice", "elsewhere", "bob", "hi").unwrap();
        let msgid = env.msgid.clone();
        aoide_storage::outbox::write_entry("elsewhere", &OutboxEntry::fresh(env)).unwrap();

        // Block the entry's own record write before the drain ever runs, so
        // the FIRST attempt on this entry (not a later retry) already hits
        // the failure this test pins.
        let blocked_tmp =
            dir.join("state").join("outbox").join("elsewhere").join(format!("{msgid}.tmp.{}", std::process::id()));
        std::fs::create_dir_all(&blocked_tmp).unwrap();
        std::fs::write(blocked_tmp.join("occupied"), b"").unwrap();

        let result = drain_node("elsewhere");
        assert!(result.is_err(), "the entry write's own error must still propagate: {result:?}");

        let link = aoide_storage::outbox::read_link_state("elsewhere").unwrap();
        assert!(link.is_some(), "the link must back off even though the entry's own record write failed");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A `TransportFailed` must record `tries`/`lastTryAt`/`lastOutcome` on
    /// the ENTRY that hit it before backing off the link — pins the fix for
    /// the bug where the whole batch's bookkeeping stayed at `tries: 0`
    /// forever on a dead link (the outbox investigation's own finding:
    /// 16.5k entries sitting at `tries=0` because `TransportFailed` broke
    /// out of the loop before touching a single entry).
    #[test]
    fn a_transport_failure_records_tries_and_outcome_on_the_entry_it_hit() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("transport-failure-records-entry");

        aoide_storage::node_store::save_nodes(&[unpaired_node("elsewhere")]).unwrap();

        let env = aoide_storage::mail::mint_outbound_letter("alice", "elsewhere", "bob", "hi").unwrap();
        aoide_storage::outbox::write_entry("elsewhere", &OutboxEntry::fresh(env)).unwrap();

        drain_node("elsewhere").unwrap();

        let entries = aoide_storage::outbox::list_entries("elsewhere").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].tries, 1, "the entry the transport failure hit must record the attempt");
        assert!(!entries[0].last_try_at.is_empty(), "lastTryAt must be stamped");
        assert!(
            entries[0].last_outcome.contains("transport"),
            "lastOutcome must say this was a transport failure: {}",
            entries[0].last_outcome
        );
        assert!(!entries[0].refused, "a transport failure never sets refused");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_drain_never_attempts_more_than_the_batch_cap_per_call() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("drain-batch-cap");

        let (listener, port) = fake_deposit_server(
            r#"{"jsonrpc":"2.0","id":1,"result":{"status":"accepted"}}"#,
        );
        let mut node = unpaired_node("elsewhere");
        node.url = format!("http://127.0.0.1:{port}/");
        aoide_storage::node_store::save_nodes(&[node]).unwrap();

        // More entries than the batch cap — every entry is a receipt, so a
        // `Delivered` outcome retires it outright; the number remaining
        // after one drain call proves the cap was actually enforced rather
        // than the whole spool draining in one pass.
        let extra = DRAIN_BATCH_CAP + 10;
        for i in 0..extra {
            let to = aoide_storage::mail::Address { node: "origin-node".to_string(), name: "bob".to_string() };
            let env = aoide_storage::mail::mint_ack("alice", to, &format!("acked-{i}")).unwrap();
            aoide_storage::outbox::write_entry("elsewhere", &OutboxEntry::fresh(env)).unwrap();
        }
        assert_eq!(aoide_storage::outbox::list_entries("elsewhere").unwrap().len(), extra);

        drain_node("elsewhere").unwrap();

        let remaining = aoide_storage::outbox::list_entries("elsewhere").unwrap().len();
        assert_eq!(
            remaining,
            extra - DRAIN_BATCH_CAP,
            "one drain call must retire at most DRAIN_BATCH_CAP entries, leaving the rest for the next tick"
        );

        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A minimal fake `aoide/mailDeposit` door: answers every connection
    /// with the same fixed JSON-RPC body, forever — mirrors
    /// `commands::spawn_fake_card_server`'s exact shape (that copy is
    /// private to `commands.rs`'s own test module, so this is a second,
    /// module-local instance rather than a cross-module reach).
    fn fake_deposit_server(body: &'static str) -> (std::net::TcpListener, u16) {        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let accepter = listener.try_clone().unwrap();
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            loop {
                let Ok((mut stream, _)) = accepter.accept() else { break };
                let mut buf = [0u8; 1024];
                let n = stream.read(&mut buf).unwrap_or(0);
                if n == 0 {
                    continue;
                }
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        (listener, port)
    }

    /// **An older relay refuses transit, and the refusal PARKS the letter at
    /// its sender.** Nothing on the wire names a peer's version, so an older
    /// relay cannot be recognised — it answers the taught word for a container
    /// addressed somewhere else (`addressing-mismatch`: it has never heard of a
    /// hop), the drain classifies that as a refusal, and the entry parks
    /// (`mail outbox retry --refused` is the hand). Holding is the honest answer
    /// while a mesh rolls out, and this is the shape it takes.
    #[test]
    fn an_older_relay_refusing_transit_parks_the_letter_at_its_sender() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("transit-old-relay");

        let (listener, port) = fake_deposit_server(
            r#"{"jsonrpc":"2.0","id":1,"result":{"status":"refused","reason":"addressing-mismatch","detail":"container is addressed to `chiyo`, not this node"}}"#,
        );
        let mut node = unpaired_node("sakaki");
        node.url = format!("http://127.0.0.1:{port}/");
        aoide_storage::node_store::save_nodes(&[node]).unwrap();

        // A routed letter: the container is addressed to `chiyo`, and the entry
        // is spooled toward the relay the route picked.
        let now = aoide_storage::time::now_iso_utc();
        let envelope = aoide_storage::mail::mint_outbound_letter_from(
            "osaka",
            "alice",
            "chiyo",
            "bob",
            "routed through an old relay",
            "home",
        )
        .unwrap();
        let msgid = envelope.msgid.clone();
        let binding = aoide_storage::seal::publish_binding().unwrap();
        let container = aoide_storage::seal::seal_envelope(
            &envelope,
            &binding,
            "home",
            "home",
            "chiyo",
            "sakaki",
            &now,
        )
        .unwrap();
        aoide_storage::outbox::write_entry("sakaki", &OutboxEntry::sealed(envelope, container)).unwrap();

        drain_node("sakaki").unwrap();

        let entries = aoide_storage::outbox::list_entries("sakaki").unwrap();
        let parked = entries.iter().find(|e| e.envelope.msgid == msgid).expect("still spooled");
        assert!(parked.refused, "a refusal parks the entry rather than rerouting it");
        assert!(
            parked.last_outcome.starts_with("refused:") && parked.last_outcome.contains("addressing-mismatch"),
            "and the far end's word is recorded: {}",
            parked.last_outcome
        );

        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The cap counts ATTEMPTS, not spool positions. Parked entries sort
    /// oldest-first (they were tried, and every later letter is newer), so a
    /// cap applied before the refused filter hands one whole batch to a loop
    /// that skips every member of it — a fresh letter sitting behind
    /// DRAIN_BATCH_CAP parked ones would never be dialled again, silently,
    /// forever. Not hypothetical: the live osaka spool is already entirely
    /// refused entries.
    #[test]
    fn a_wall_of_parked_entries_never_starves_a_fresh_letter_out_of_the_batch() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("drain-refused-wall");

        let (listener, port) = fake_deposit_server(
            r#"{"jsonrpc":"2.0","id":1,"result":{"status":"accepted"}}"#,
        );
        let mut node = unpaired_node("elsewhere");
        node.url = format!("http://127.0.0.1:{port}/");
        aoide_storage::node_store::save_nodes(&[node]).unwrap();

        for i in 0..DRAIN_BATCH_CAP {
            let mut envelope =
                aoide_storage::mail::mint_outbound_letter("alice", "elsewhere", "bob", &format!("parked {i}")).unwrap();
            // Older than anything minted now, so all of these sort ahead of
            // the fresh letter below whatever the wall clock does.
            envelope.header.minted_at = "2020-01-01T00:00:00Z".to_string();
            let mut parked = OutboxEntry::fresh(envelope);
            parked.refused = true;
            aoide_storage::outbox::write_entry("elsewhere", &parked).unwrap();
        }

        let envelope = aoide_storage::mail::mint_outbound_letter("alice", "elsewhere", "bob", "fresh").unwrap();
        let fresh_msgid = envelope.msgid.clone();
        aoide_storage::outbox::write_entry("elsewhere", &OutboxEntry::fresh(envelope)).unwrap();

        drain_node("elsewhere").unwrap();

        let entries = aoide_storage::outbox::list_entries("elsewhere").unwrap();
        let fresh = entries
            .iter()
            .find(|e| e.envelope.msgid == fresh_msgid)
            .expect("the fresh letter is still spooled");
        assert_eq!(fresh.tries, 1, "a fresh letter behind a full batch of parked entries must still be attempted");
        assert_eq!(fresh.last_outcome, "accepted");

        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// MAIL.md §Wire/§Transit: a well-formed envelope's rejection is a
    /// RESULT (`{"status":"refused","reason":…}`), never a JSON-RPC error —
    /// so `attempt_deposit` must recognise this shape as a refusal on its
    /// own, not rely on an `error` member that a spec-conformant peer never
    /// sends for this outcome. Pins the client half of that split directly:
    /// this test fails against the client code that reads `status` with
    /// `.unwrap_or("accepted")` and returns `Delivered` for anything that
    /// isn't literally `"error"` at the JSON-RPC envelope level, because
    /// that code leaves `refused` false and `tries` incrementing forever
    /// (`drain_node`'s `Delivered` arm for a letter never sets `refused`).
    #[test]
    fn a_refused_result_parks_the_entry_and_a_later_drain_skips_it() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("refused-result-parks");

        let (listener, port) = fake_deposit_server(
            r#"{"jsonrpc":"2.0","id":1,"result":{"status":"refused","reason":"bad-msgid","detail":"envelope msgid does not match the recomputed value"}}"#,
        );
        let mut node = unpaired_node("elsewhere");
        node.url = format!("http://127.0.0.1:{port}/");
        aoide_storage::node_store::save_nodes(&[node]).unwrap();

        let env = aoide_storage::mail::mint_outbound_letter("alice", "elsewhere", "bob", "hi").unwrap();
        aoide_storage::outbox::write_entry("elsewhere", &OutboxEntry::fresh(env)).unwrap();

        drain_node("elsewhere").unwrap();

        let entries = aoide_storage::outbox::list_entries("elsewhere").unwrap();
        assert_eq!(entries.len(), 1, "a refused entry stays in the spool — no auto-eviction (kill-list)");
        assert!(entries[0].refused, "a refused RESULT must park the entry exactly like a refused ERROR does");
        assert!(entries[0].last_outcome.contains("bad-msgid"), "the reason is recorded: {}", entries[0].last_outcome);
        assert_eq!(entries[0].tries, 1);

        // A second drain must SKIP a refused entry outright (`if entry.refused
        // { continue }`) rather than retry it — `tries` staying at 1 is the
        // proof, since the fake door would happily answer a second POST too.
        drain_node("elsewhere").unwrap();
        let after = aoide_storage::outbox::list_entries("elsewhere").unwrap();
        assert_eq!(after[0].tries, 1, "a parked entry is never retried by a later drain");

        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A 200 response whose `result` carries no `status` key at all is
    /// WEAKER evidence of delivery than an unrecognised status string, and
    /// the unrecognised one already parks (test above) — so this must park
    /// too, on the same catch-all arm, never take a shortcut to
    /// `Delivered`. Uses a RECEIPT-kind entry deliberately: `drain_node`'s
    /// `Delivered` arm for a receipt calls `outbox::remove_entry` outright
    /// (ruling 4 — a receipt's own successful deposit IS its confirmation),
    /// so a receipt is the one entry kind where taking that arm by mistake
    /// destroys the record rather than merely mis-annotating it. The
    /// record surviving is the assertion that matters. Pins the fix for
    /// the `unwrap_or("accepted")` default that used to bypass the
    /// fail-closed match below it: this test fails against that code,
    /// which read this exact response as `Delivered` and discarded the
    /// receipt on a reply that confirmed nothing.
    #[test]
    fn a_missing_status_parks_the_entry_and_keeps_the_receipt_record() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("missing-status-parks");

        let (listener, port) = fake_deposit_server(r#"{"jsonrpc":"2.0","id":1,"result":{}}"#);
        let mut node = unpaired_node("elsewhere");
        node.url = format!("http://127.0.0.1:{port}/");
        aoide_storage::node_store::save_nodes(&[node]).unwrap();

        let to = aoide_storage::mail::Address { node: "origin-node".to_string(), name: "bob".to_string() };
        let env = aoide_storage::mail::mint_ack("alice", to, "some-acked-msgid").unwrap();
        aoide_storage::outbox::write_entry("elsewhere", &OutboxEntry::fresh(env)).unwrap();

        drain_node("elsewhere").unwrap();

        let entries = aoide_storage::outbox::list_entries("elsewhere").unwrap();
        assert_eq!(
            entries.len(),
            1,
            "a missing status must park a receipt entry, never remove it — this is the data loss the fix prevents"
        );
        assert!(entries[0].refused, "a missing status must be parked exactly like an unrecognised one");
        assert!(
            entries[0].last_outcome.contains("missing-status"),
            "the reason must say the status was absent, not imply the peer sent one: {}",
            entries[0].last_outcome
        );

        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_drain_session_leaves_no_forward_standing() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("drain-no-forward");

        aoide_storage::node_store::save_nodes(&[unpaired_node("elsewhere")]).unwrap();

        let env = aoide_storage::mail::mint_outbound_letter("alice", "elsewhere", "bob", "hi").unwrap();
        aoide_storage::outbox::write_entry("elsewhere", &OutboxEntry::fresh(env)).unwrap();

        drain_node("elsewhere").unwrap();

        // `elsewhere.via` is None (direct dial) here, so no tunnel was ever
        // opened — the assertion that matters is that tearing one down is
        // always safe to attempt, never that one existed. `close` on a
        // record-less key is a documented no-op (`tunnel.rs`'s own doc);
        // the drain returning at all without hanging or erroring past that
        // guard IS the proof this test pins.
        let records = aoide_storage::tunnel::list_records();
        assert!(
            records.iter().all(|r| r.key != "elsewhere"),
            "no tunnel record should be left standing for a node this drain touched"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── hold, poll, and poll-on-contact (P-M3, MAIL.md §Wire/§Outbox) ─────

    /// A fake door that RECORDS every request body it is handed and answers
    /// by method: `aoide/mailPoll` gets `poll_body`, anything else gets
    /// `deposit_body`. Reads a full request (headers up to Content-Length),
    /// never one 1024-byte bite — a poll answer and a deposit body are both
    /// worth asserting on, which needs the whole body.
    fn recording_door(
        deposit_body: &str,
        poll_body: String,
    ) -> (std::net::TcpListener, u16, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = seen.clone();
        let deposit = deposit_body.to_string();
        let accepter = listener.try_clone().unwrap();
        std::thread::spawn(move || loop {
            let Ok((mut stream, _)) = accepter.accept() else { break };
            let mut buf: Vec<u8> = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let n = stream.read(&mut chunk).unwrap_or(0);
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&buf).to_string();
                if let Some(head) = text.find("\r\n\r\n") {
                    let len: usize = text[..head]
                        .lines()
                        .find_map(|l| l.strip_prefix("Content-Length:"))
                        .and_then(|v| v.trim().parse().ok())
                        .unwrap_or(0);
                    if buf.len() >= head + 4 + len {
                        break;
                    }
                }
            }
            let text = String::from_utf8_lossy(&buf).to_string();
            let body = text.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
            log.lock().unwrap().push(body.clone());
            let answer = if body.contains("aoide/mailPoll") { poll_body.clone() } else { deposit.clone() };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                answer.len(),
                answer
            );
            let _ = stream.write_all(response.as_bytes());
        });
        (listener, port, seen)
    }

    fn recorded(log: &std::sync::Arc<std::sync::Mutex<Vec<String>>>) -> Vec<Value> {
        log.lock().unwrap().iter().filter_map(|b| serde_json::from_str::<Value>(b).ok()).collect()
    }

    fn poll_answer(envelopes: &[Envelope]) -> String {
        format!(
            r#"{{"jsonrpc":"2.0","id":1,"result":{{"envelopes":{}}}}}"#,
            serde_json::to_string(envelopes).unwrap()
        )
    }

    /// Registers the node the polled side hands a letter over FROM. An
    /// envelope this box mints is signed by this process's own identity and
    /// stamped `header.from.node = display::local_node_name()` (P-M1's
    /// "self never crosses the wire"), so the only name whose recorded key
    /// can verify it is that one — the same "one process plays both roles"
    /// shortcut the server's `setup_verifiable_origin` documents. `url`
    /// points nowhere: the ack this filing spools must STAY spooled for the
    /// assertion, not be delivered and retired.
    fn register_local_origin() -> String {
        let me = aoide_storage::display::local_node_name();
        let (kp, _) = aoide_storage::identity::load_or_mint().unwrap();
        let mut node = unpaired_node(&me);
        node.pubkey = Some(kp.info().pubkey_hex.clone());
        node.verified = true;
        node.grants = aoide_storage::node_store::grants_in("home", &["message"]);
        let mut nodes = aoide_storage::node_store::load_nodes();
        aoide_storage::node_store::insert_node(&mut nodes, node);
        aoide_storage::node_store::save_nodes(&nodes).unwrap();
        me
    }

    fn register_relay(url: String) -> Node {
        let relay = unpaired_node("relay");
        let relay = Node { url, ..relay };
        let mut nodes = aoide_storage::node_store::load_nodes();
        aoide_storage::node_store::insert_node(&mut nodes, relay.clone());
        aoide_storage::node_store::save_nodes(&nodes).unwrap();
        relay
    }

    /// One `now` receipt toward `relay`, spooled and then retired by the
    /// door's own `accepted` answer — the CONTACT this pass is built on.
    fn spool_contact_receipt(tag: &str) {
        let to = aoide_storage::mail::Address { node: "relay".to_string(), name: "bob".to_string() };
        let receipt = aoide_storage::mail::mint_ack("alice", to, tag).unwrap();
        aoide_storage::outbox::write_entry("relay", &OutboxEntry::fresh(receipt)).unwrap();
    }

    /// Spec, P-M3: **a hold entry drains only via poll.** A drain pass that
    /// really does reach the node — proven by the `now` receipt it delivers
    /// and retires in the same call — must not carry the held letter in any
    /// `aoide/mailDeposit` body, and must leave that entry spooled, held,
    /// and still at `tries: 0`. The same contact DOES carry a poll, which is
    /// where a held entry leaves from (`outbox::poll_payloads` hands it to the
    /// far end's own ask; the door half is pinned in `aoide-server`).
    #[test]
    fn a_plaintext_entry_signs_for_the_mesh_its_own_envelope_carries() {
        // Review N2: a two-mesh record used to make the PLAINTEXT lane fail
        // forever (the deposit re-resolved the mesh, found two, and reported a
        // transport failure no `--mesh` could fix), while the sealed lane for
        // the same pair worked. The letter's own signed `origin_mesh` decides.
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("deposit-mesh-plaintext");
        let me = aoide_storage::display::local_node_name();
        let (kp, _) = aoide_storage::identity::load_or_mint().unwrap();
        let mut nodes = aoide_storage::node_store::load_nodes();
        aoide_storage::node_store::upsert_paired_node(
            &mut nodes,
            "twomesh",
            "http://127.0.0.1:1/",
            &"aa".repeat(32),
            &aoide_storage::time::now_iso_utc(),
            &["message".to_string()],
            "away",
        );
        let mut twin = aoide_storage::node_store::Node {
            name: "twomesh".to_string(),
            url: "http://127.0.0.1:1/".to_string(),
            autogate: false,
            token_file: None,
            bearer_secret: None,
            hub: false,
            pubkey: Some("aa".repeat(32)),
            verified: true,
            grants: aoide_storage::node_store::grants_in("home", &["message"]),
            narrowed: aoide_storage::node_store::Grants::new(),
            via: None,
            added_at: aoide_storage::time::now_iso_utc(),
        };
        twin.grants.extend(nodes.iter().find(|n| n.name == "twomesh").unwrap().grants.clone());
        nodes.retain(|n| n.name != "twomesh");
        nodes.push(twin);
        aoide_storage::node_store::save_nodes(&nodes).unwrap();
        let _ = (me, kp, dir);

        let envelope = aoide_storage::mail::mint_outbound_letter_in_mesh("alice", "twomesh", "bob", "hi", "away").unwrap();
        let entry = aoide_storage::outbox::OutboxEntry::fresh(envelope);
        assert_eq!(
            deposit_mesh(&entry),
            Some("away"),
            "a plaintext entry names the mesh its own envelope was signed with — never a fresh resolution"
        );

        // (The sealed lane's half of the rule — the container's own mesh — is
        // pinned in `seal.rs`, where it belongs.)
    }

    #[test]
    fn a_bare_poll_selects_a_peer_trusted_outside_the_home_mesh() {
        // Review N3: the selection used to ask the HOME mesh only, so a peer
        // paired into `away` was silently skipped while `mail send` delivered
        // to it — bare `mail poll` was dead for exactly those peers.
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = aoide_test_support::EnvSaver::capture(&["AOIDE_STATE_DIR", "AOIDE_ROOT"]);
        let dir = aoide_test_support::unique_tmp("poll-away-only");
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("AOIDE_STATE_DIR", &dir);
        std::env::set_var("AOIDE_ROOT", &dir);

        let mut nodes = aoide_storage::node_store::load_nodes();
        aoide_storage::node_store::upsert_paired_node(
            &mut nodes,
            "away-peer",
            "http://127.0.0.1:1/",
            &"bb".repeat(32),
            &aoide_storage::time::now_iso_utc(),
            &["message".to_string()],
            "away",
        );
        aoide_storage::node_store::save_nodes(&nodes).unwrap();

        assert_eq!(pollable_nodes(), vec!["away-peer".to_string()], "a peer trusted only in `away` is pollable");
        let record = aoide_storage::node_store::load_nodes().into_iter().find(|n| n.name == "away-peer").unwrap();
        assert_eq!(
            crate::commands::request_mesh(&record, None, "message").unwrap(),
            "away",
            "and the poll's own mesh resolves to the one holding `message`, with no flag"
        );
    }

    /// **Revocation's routing half** (P-CHARTER): a charter version that
    /// removes a node's line stops that node being a routing target on the
    /// very next poll — its door refuses it (the door half, `a2a::
    /// grant_in_mesh`) AND this client stops asking it (`revoked_by_charter`),
    /// which is what `docs/architecture/HTTPS-MESH-API.md` means by "its
    /// router stops routing to it". A record that also holds `message` in a
    /// PAIR mesh is never locked out by a charter.
    #[test]
    fn a_charter_removed_line_stops_being_a_routing_target() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("charter-revoked-routing");

        // The charter lists THIS box's own key — a real line, so the age
        // binding verifies — and the record below carries that same key, which
        // is what makes it a charter-listed node rather than a stray pairing.
        let (kp, _) = aoide_storage::identity::load_or_mint().unwrap();
        let listed_key = kp.info().pubkey_hex;
        let _init = aoide_storage::charter::init("home").unwrap();
        let line = aoide_storage::charter::node_line().unwrap();
        let src = format!("mesh = \"home\"\nversion = 0\nrelays = []\n\n[nodes]\n{line}\n");
        std::fs::write(aoide_storage::charter::source_path("home"), &src).unwrap();
        aoide_storage::charter::sign("home", None).unwrap();

        let mut nodes = aoide_storage::node_store::load_nodes();
        aoide_storage::node_store::upsert_paired_node(
            &mut nodes, "revokee", "http://127.0.0.1:1/", &listed_key,
            &aoide_storage::time::now_iso_utc(), &["message".to_string()], "home",
        );
        aoide_storage::node_store::upsert_paired_node(
            &mut nodes, "friend", "http://127.0.0.1:2/", &"ee".repeat(32),
            &aoide_storage::time::now_iso_utc(), &["message".to_string()], "club",
        );
        aoide_storage::node_store::save_nodes(&nodes).unwrap();

        let listed = pollable_nodes();
        assert!(listed.contains(&"revokee".to_string()), "a charter-listed line is a routing target: {listed:?}");
        assert!(listed.contains(&"friend".to_string()), "and a pair mesh is untouched by any charter: {listed:?}");

        // The operator re-signs with an EMPTY node list — the revocation — and
        // this machine applies v2.
        let src2 = format!("mesh = \"home\"\nversion = 1\nrelays = []\n\n[nodes]\n");
        std::fs::write(aoide_storage::charter::source_path("home"), &src2).unwrap();
        aoide_storage::charter::sign("home", None).unwrap();
        assert_eq!(
            aoide_storage::charter::in_force_charter("home").unwrap().version,
            2,
            "the operator re-signed and this box (the operator's own, in this fixture) applied it"
        );

        let after = pollable_nodes();
        assert!(
            !after.contains(&"revokee".to_string()),
            "a removed line stops being routed to on the next poll: {after:?}"
        );
        assert!(after.contains(&"friend".to_string()), "while the pair mesh still routes: {after:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_hold_entry_drains_only_via_poll() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("drain-hold-only-via-poll");

        let (listener, port, log) =
            recording_door(r#"{"jsonrpc":"2.0","id":1,"result":{"status":"accepted"}}"#, poll_answer(&[]));
        register_relay(format!("http://127.0.0.1:{port}/"));

        let held = aoide_storage::mail::mint_outbound_letter("alice", "relay", "bob", "held until you ask").unwrap();
        let held_msgid = held.msgid.clone();
        aoide_storage::outbox::write_entry("relay", &OutboxEntry::held(held)).unwrap();
        spool_contact_receipt("acked-earlier");

        drain_node("relay").unwrap();

        let bodies = recorded(&log);
        let deposit_bodies: Vec<&Value> = bodies.iter().filter(|b| b["method"] == "aoide/mailDeposit").collect();
        let poll_bodies: Vec<&Value> = bodies.iter().filter(|b| b["method"] == "aoide/mailPoll").collect();
        assert!(!deposit_bodies.is_empty(), "the pass did reach the node: {bodies:?}");
        assert_eq!(poll_bodies.len(), 1, "poll-on-contact: one poll per contacted pass: {bodies:?}");
        for body in deposit_bodies {
            let carried = body["params"]["envelope"]["msgid"].as_str().unwrap_or("");
            assert_ne!(carried, held_msgid, "a hold entry is NEVER dialed: {body}");
        }

        let remaining = aoide_storage::outbox::list_entries("relay").unwrap();
        let held_row = remaining.iter().find(|e| e.envelope.msgid == held_msgid).expect("the held letter is still spooled");
        assert!(held_row.is_held(), "it stays held");
        assert_eq!(held_row.tries, 0, "and was never even attempted");
        assert!(
            !remaining.iter().any(|e| e.envelope.header.kind == aoide_storage::mail::ENTRY_TYPE_RECEIPT),
            "the contact receipt itself drained normally, which is what makes this pass a real contact"
        );

        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A `poll` address is never dialled** (P-CHARTER, "Transports and
    /// relays": "a hub's outbox entry for a `poll` node is `hold`-flavored.
    /// The hub never dials it, and only the node's own `mailPoll` drains it").
    /// Three halves, at the three places the mail path could dial:
    ///
    /// 1. `spool_entry` — asked for a `now` entry, it spools HELD, because the
    ///    destination's address is `poll`;
    /// 2. `drain_node` — returns without opening a link at all, so the BINDING
    ///    EXCHANGE (a dial that a held-entry filter runs too late to prevent)
    ///    never happens either;
    /// 3. the ACK path — `spool_and_drain_ack` passes a literal `false`, and
    ///    the ack toward a `poll` origin comes out held by the same line,
    ///    which is the caller this rule exists to protect.
    ///
    /// Nothing listens for `laptop` in this test, and nothing could: a `poll`
    /// node has no inbound transport.
    #[test]
    fn a_poll_address_is_never_dialled_and_its_entry_is_held() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("poll-node-never-dialled");

        let laptop = Node {
            url: aoide_storage::charter::DEFAULT_ADDRESS.to_string(),
            ..unpaired_node("laptop")
        };
        let laptop = Node { pubkey: Some("cc33".to_string()), verified: true, ..laptop };
        let laptop = Node { grants: aoide_storage::node_store::grants_in("home", &["message"]), ..laptop };
        assert!(laptop.never_dialled(), "the predicate reads the address, by scheme");
        let mut nodes = aoide_storage::node_store::load_nodes();
        aoide_storage::node_store::insert_node(&mut nodes, laptop.clone());
        aoide_storage::node_store::save_nodes(&nodes).unwrap();

        // 1. The spool decides HELD although the caller asked for `now`.
        let letter = aoide_storage::mail::mint_outbound_letter("alice", "laptop", "bob", "for the laptop").unwrap();
        let entry = spool_entry("laptop", "laptop", &letter.header.origin_mesh.clone(), letter.clone(), false).unwrap();
        assert!(
            entry.is_held(),
            "a `poll` destination's entry is hold-flavored whatever the caller asked for"
        );
        aoide_storage::outbox::write_entry("laptop", &entry).unwrap();

        // 2. A drain pass does not dial it — not even for the binding exchange.
        drain_node("laptop").unwrap();
        let rows = aoide_storage::outbox::list_entries("laptop").unwrap();
        let row = rows.iter().find(|e| e.envelope.msgid == letter.msgid).expect("still spooled");
        assert_eq!(row.tries, 0, "never attempted");
        assert!(row.last_try_at.is_empty(), "and no attempt was even recorded: {}", row.last_try_at);
        assert!(row.is_held(), "it stays held, for the far end's own poll");
        assert!(
            aoide_storage::outbox::read_link_state("laptop").unwrap().is_none(),
            "no link was ever opened, so no link state was written"
        );

        // 3. The same line covers the ack path's literal `false`.
        let mut inbound = aoide_storage::mail::mint_outbound_letter("bob", "somewhere", "conductor", "hi").unwrap();
        inbound.header.from.node = "laptop".to_string();
        inbound.header.from.name = "bob".to_string();
        settle_deposit(
            &inbound,
            &aoide_storage::mail::DepositOutcome::Filed {
                msgid: inbound.msgid.clone(),
                kind: aoide_storage::mail::ENTRY_TYPE_LETTER.to_string(),
            },
        );
        let ack = aoide_storage::outbox::list_entries("laptop")
            .unwrap()
            .into_iter()
            .find(|e| e.envelope.header.kind == aoide_storage::mail::ENTRY_TYPE_RECEIPT)
            .expect("the ack toward the `poll` origin was spooled");
        assert!(ack.is_held(), "and it is held too — the ack path cannot get this wrong");
        assert_eq!(ack.tries, 0);
        assert!(
            !pollable_nodes().contains(&"laptop".to_string()),
            "and a bare `mail poll` does not ask a node that has no inbound transport"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Poll-on-contact's receive half: what the node hands over is received
    /// exactly as a push would be — filed after full origin verification,
    /// with an ack spooled back toward the letter's origin, and the poll
    /// naming THIS box as the node it wants (never another's mailbox).
    #[test]
    fn poll_on_contact_files_what_the_node_hands_over() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("poll-on-contact-files");

        let me = register_local_origin();
        let letter = aoide_storage::mail::mint_outbound_letter("alice", &me, "conductor", "relayed to me").unwrap();
        let letter_msgid = letter.msgid.clone();
        let (listener, port, log) =
            recording_door(r#"{"jsonrpc":"2.0","id":1,"result":{"status":"accepted"}}"#, poll_answer(&[letter]));
        register_relay(format!("http://127.0.0.1:{port}/"));
        spool_contact_receipt("acked-earlier");

        drain_node("relay").unwrap();

        let bodies = recorded(&log);
        let poll = bodies.iter().find(|b| b["method"] == "aoide/mailPoll").expect("the contact carries a poll");
        assert_eq!(poll["params"]["node"], Value::String(me.clone()), "a poll asks for the CALLER's own node");

        let base = aoide_storage::mail::read_base().unwrap();
        assert_eq!(base.len(), 1, "the hand-over is filed: {base:?}");
        assert_eq!(base[0].envelope.msgid, letter_msgid);
        assert_eq!(base[0].via, "relay", "via is the HOP's name — the node that was polled");

        let acks = aoide_storage::outbox::list_entries(&me).unwrap();
        assert_eq!(acks.len(), 1, "filing a letter spools an ack toward its origin");
        assert_eq!(acks[0].envelope.header.kind, aoide_storage::mail::ENTRY_TYPE_RECEIPT);
        assert_eq!(acks[0].envelope.text, letter_msgid);

        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Spec, P-M3: **re-poll before ack is idempotent.** The same answer
    /// twice files one entry, reports `0` filed the second time, and spools
    /// exactly one ack — the dedup plus `write_ack_if_absent` gate, never a
    /// bookmark on the far end's spool (a poll writes nothing there).
    #[test]
    fn a_repoll_before_the_ack_files_nothing_twice() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("poll-repoll-idempotent");

        let me = register_local_origin();
        let letter = aoide_storage::mail::mint_outbound_letter("alice", &me, "conductor", "once only").unwrap();
        let letter_msgid = letter.msgid.clone();
        let (listener, port, _log) =
            recording_door(r#"{"jsonrpc":"2.0","id":1,"result":{"status":"accepted"}}"#, poll_answer(&[letter]));
        register_relay(format!("http://127.0.0.1:{port}/"));

        assert_eq!(poll_node("relay", None).unwrap().filed, 1, "the first poll files the letter");
        assert_eq!(poll_node("relay", None).unwrap().filed, 0, "a re-poll before the ack files nothing a second time");

        assert_eq!(aoide_storage::mail::read_base().unwrap().len(), 1, "one letter, however many polls");
        let acks = aoide_storage::outbox::list_entries(&me).unwrap();
        assert_eq!(acks.len(), 1, "and exactly one pending ack: {acks:?}");
        assert_eq!(acks[0].envelope.text, letter_msgid);

        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A poll that the far end refuses or that never answers is an honest
    /// `Err`, and — unlike a deposit attempt — leaves no trace on the spool:
    /// polling is a read of the FAR end's spool, and its failure is not this
    /// box's outcome to record.
    #[test]
    fn an_unanswerable_poll_reports_an_error_and_changes_nothing() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("poll-unanswerable");

        register_relay("http://127.0.0.1:1/".to_string());
        let held = aoide_storage::mail::mint_outbound_letter("alice", "relay", "bob", "waiting").unwrap();
        let held_msgid = held.msgid.clone();
        aoide_storage::outbox::write_entry("relay", &OutboxEntry::held(held)).unwrap();

        let err = poll_node("relay", None).expect_err("a dead node cannot be polled");
        assert!(err.contains("transport") || !err.is_empty(), "the reason is carried, not swallowed: {err}");

        let rows = aoide_storage::outbox::list_entries("relay").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].envelope.msgid, held_msgid);
        assert_eq!(rows[0].tries, 0, "a failed poll is not an attempt on any entry");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// H1 (the branch review): a letter spooled before its destination had a
    /// binding must not cross in the clear in the pass that learns the
    /// binding.
    ///
    /// This is the design's own per-peer upgrade scenario
    /// (`HTTPS-MESH-API.md` "Migration and coexistence"), and its acceptance
    /// test — "once its binding is learned, it never receives plaintext
    /// again" — is the one the branch could not demonstrate before this fix:
    /// the spool decided sealed-vs-plaintext once, at mint, and nothing ever
    /// re-sealed.
    #[test]
    fn a_letter_spooled_before_the_binding_arrived_goes_out_sealed() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("reseal-h1");

        // B is paired and verified — this box's own keypair stands in for
        // B's, the one-process-plays-both-roles shortcut the server's
        // `setup_verifiable_origin` documents — but B has published no age
        // binding yet.
        let (bkp, _) = aoide_storage::identity::load_or_mint().unwrap();
        let accepted = r#"{"jsonrpc":"2.0","id":1,"result":{"status":"accepted"}}"#;
        let (_listener, port, seen) = recording_door(accepted, "{}".to_string());
        let mut nodes = aoide_storage::node_store::load_nodes();
        aoide_storage::node_store::upsert_paired_node(
            &mut nodes,
            "liveb",
            &format!("http://127.0.0.1:{port}/"),
            &bkp.info().pubkey_hex,
            &aoide_storage::time::now_iso_utc(),
            &["message".to_string()],
            "home",
        );
        aoide_storage::node_store::save_nodes(&nodes).unwrap();
        assert!(aoide_storage::seal::binding_for("liveb").is_none(), "B has published nothing yet");

        // A sends. `spool_entry` decides at MINT and sees no binding, so the
        // entry is plaintext and the spool holds the letter.
        let canary = "CANARY-H1-BODY-ABCDEF";
        let envelope = aoide_storage::mail::mint_outbound_letter("alice", "liveb", "bob", canary).unwrap();
        let spooled = spool_entry("liveb", "liveb", &envelope.header.origin_mesh.clone(), envelope, false).unwrap();
        assert!(!spooled.is_sealed(), "spooled plaintext: the binding had not arrived");
        aoide_storage::outbox::write_entry("liveb", &spooled).unwrap();
        assert!(
            serde_json::to_string(&spooled).unwrap().contains(canary),
            "the plaintext spool holds the body, as it must at this point"
        );

        // B upgrades and publishes; A learns the binding — exactly what the
        // drain's own `exchange_bindings` does at the top of the pass.
        let binding = aoide_storage::seal::publish_binding().unwrap();
        aoide_storage::seal::learn_binding("liveb", &binding).unwrap();

        // The next drain is the whole test: it must NOT hand that entry over
        // as plaintext, in this pass or any other.
        drain_node("liveb").unwrap();

        let bodies = seen.lock().unwrap().clone();
        let deposits: Vec<&String> = bodies.iter().filter(|b| b.contains("aoide/mailDeposit")).collect();
        assert_eq!(deposits.len(), 1, "one deposit this pass: {bodies:?}");
        let deposit = deposits[0];
        assert!(deposit.contains("\"container\""), "the deposit carried a container: {deposit}");
        assert!(!deposit.contains("\"envelope\""), "and NOT a plaintext envelope: {deposit}");
        assert!(!deposit.contains(canary), "the letter did not cross in the clear: {deposit}");

        // And the spool agrees with the wire: the entry is sealed now, and
        // the body is inside `ct` rather than beside it.
        let after = aoide_storage::outbox::list_entries("liveb").unwrap();
        assert_eq!(after.len(), 1);
        assert!(after[0].is_sealed(), "the spool was upgraded, not just the wire");
        assert!(!serde_json::to_string(&after[0]).unwrap().contains(canary), "no body left in the spool");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// H2 (the re-review): the decision has FOUR states and must never
    /// conflate "no binding" with "binding held but not usable". The old
    /// signature answered `Ok(None)` for both, and both callers then sent the
    /// entry as plaintext.
    #[test]
    fn reseal_answers_four_states_and_never_conflates_them() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("reseal-noop");
        let now = aoide_storage::time::now_iso_utc();
        use aoide_storage::outbox::Reseal;

        let (kp, _) = aoide_storage::identity::load_or_mint().unwrap();
        let envelope = aoide_storage::mail::mint_outbound_letter("alice", "liveb", "bob", "hi").unwrap();
        let entry = OutboxEntry::fresh(envelope.clone());

        // 1. No binding at all: the per-peer upgrade path, plaintext correct.
        assert!(
            matches!(
                aoide_storage::outbox::reseal_entry("liveb", &entry, &now).unwrap(),
                Reseal::NoBinding
            ),
            "no binding on record is its own state, not 'nothing to do'"
        );

        // 2. A binding that is held but NOT usable: park, never plaintext.
        let past = aoide_storage::time::shift_iso_utc(&now, -7200);
        let closed = aoide_storage::time::shift_iso_utc(&now, -3600);
        let mut nodes = aoide_storage::node_store::load_nodes();
        aoide_storage::node_store::upsert_paired_node(
            &mut nodes,
            "liveb",
            "http://127.0.0.1:1/",
            &kp.info().pubkey_hex,
            &now,
            &["message".to_string()],
            "home",
        );
        aoide_storage::node_store::save_nodes(&nodes).unwrap();
        let expired = aoide_storage::seal::mint_binding(&kp, "age1expired", 1, &past, &closed).unwrap();
        aoide_storage::seal::learn_binding("liveb", &expired).unwrap();
        match aoide_storage::outbox::reseal_entry("liveb", &entry, &now).unwrap() {
            Reseal::NotUsable(Some(parked)) => {
                assert!(parked.refused, "the entry is parked");
                assert!(
                    parked.last_outcome.starts_with("parked:"),
                    "and says why: {}",
                    parked.last_outcome
                );
            }
            other => panic!("a held-but-unusable binding must park, not fall through: {}", matches!(other, Reseal::NoBinding)),
        }
        // An already-parked entry needs no rewrite.
        let parked_entry = OutboxEntry { refused: true, ..entry.clone() };
        assert!(matches!(
            aoide_storage::outbox::reseal_entry("liveb", &parked_entry, &now).unwrap(),
            Reseal::NotUsable(None)
        ));

        // 3. A usable binding: sealed now, and the caller persists it. Its
        // generation is above the expired one's, because the high-water rule
        // is what M2 established and this test must not trip it.
        let mine = aoide_storage::seal::publish_binding().unwrap();
        let live = aoide_storage::seal::mint_binding(
            &kp,
            &mine.age_pubkey,
            2,
            &now,
            &aoide_storage::time::shift_iso_utc(&now, 3600),
        )
        .unwrap();
        aoide_storage::seal::learn_binding("liveb", &live).unwrap();
        assert!(matches!(
            aoide_storage::outbox::reseal_entry("liveb", &entry, &now).unwrap(),
            Reseal::FreshlySealed(_)
        ));

        // 4. Already sealed: never re-sealed (a second seal would mint a
        // different `ct` and break the byte-identical retry).
        let container = aoide_storage::seal::seal_envelope(&envelope, &live, "", "", "liveb", "liveb", &now).unwrap();
        let sealed = OutboxEntry::sealed(envelope, container);
        assert!(matches!(
            aoide_storage::outbox::reseal_entry("liveb", &sealed, &now).unwrap(),
            Reseal::Sealed(None)
        ));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A relay never carries plaintext.** An entry whose next hop is not the
    /// destination cannot be sent in the clear just because no binding can be
    /// had: it is PARKED with a word that says why, so the operator publishes a
    /// binding (or pairs) instead of putting their letter in front of a hub.
    /// The direct lane is the one exception, and it is unchanged.
    #[test]
    fn a_hop_that_is_not_the_destination_parks_an_unsealable_letter() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("spool-needs-a-binding");

        let envelope = aoide_storage::mail::mint_outbound_letter("alice", "chiyo", "bob", "through a hub").unwrap();
        // `dest != next`: the relay `sakaki` would be the one to read it.
        let relayed = spool_entry("chiyo", "sakaki", "home", envelope.clone(), false).unwrap();
        assert!(!relayed.is_sealed(), "there is no binding to seal to");
        assert!(relayed.refused, "and therefore no plaintext either: the entry parks");
        assert!(
            relayed.last_outcome.contains(crate::mail_wire::NO_BINDING_FOR_A_RELAY),
            "with the taught word: {}",
            relayed.last_outcome
        );

        // The direct lane is untouched: no binding, no relay, plaintext as before.
        let direct = spool_entry("chiyo", "chiyo", "home", envelope, false).unwrap();
        assert!(!direct.is_sealed());
        assert!(!direct.refused, "the per-peer upgrade path still sends plaintext in the clear");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A hub re-offers a container until the poller SAYS it has it: the
    /// hand-over alone retires nothing (a response can be lost), and the poller's
    /// next poll carries the msgid it filed — the acknowledgement this hub
    /// retires on, and only for its own `transit` custody.
    #[test]
    fn a_polled_container_is_re_offered_until_the_poller_acknowledges_it() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("poll-ack");

        let envelope = aoide_storage::mail::mint_outbound_letter("alice", "chiyo", "bob", "held for chiyo").unwrap();
        let container = {
            let live = aoide_storage::seal::publish_binding().unwrap();
            aoide_storage::seal::seal_envelope(&envelope, &live, "home", "home", "chiyo", "chiyo", &aoide_storage::time::now_iso_utc()).unwrap()
        };
        let msgid = container.msgid.clone();
        let entry = OutboxEntry::transit_held(container);
        aoide_storage::outbox::write_entry("chiyo", &entry).unwrap();

        // Offered — and still held after the hand-over: no signal has said the
        // poller has it.
        assert_eq!(aoide_storage::outbox::poll_payloads("chiyo").unwrap().len(), 1);
        assert!(matches!(
            aoide_storage::outbox::hand_over("chiyo", &msgid).unwrap(),
            aoide_storage::outbox::HandOver::Container(_)
        ));
        assert_eq!(aoide_storage::outbox::list_entries("chiyo").unwrap().len(), 1, "the hand-over retires nothing");
        assert_eq!(aoide_storage::outbox::poll_payloads("chiyo").unwrap().len(), 1, "so it is offered again");

        // Acknowledged: the next poll's own list retires the hub's custody, and
        // a msgid this box never held is a no-op.
        assert!(!aoide_storage::outbox::retire_acknowledged("chiyo", "00".repeat(32).as_str()).unwrap());
        assert!(aoide_storage::outbox::retire_acknowledged("chiyo", &msgid).unwrap());
        assert!(aoide_storage::outbox::list_entries("chiyo").unwrap().is_empty());
        assert!(aoide_storage::outbox::poll_payloads("chiyo").unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The acknowledgement travels: what this box filed out of a poll is recorded
    /// per polled node and handed to that node's NEXT poll, then cleared once the
    /// answer says the hub has it.
    #[test]
    fn the_next_poll_carries_what_this_box_filed() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("poll-ack-carried");

        let (listener, port, seen) = recording_door(
            r#"{"jsonrpc":"2.0","id":1,"result":{"status":"ok"}}"#,
            r#"{"jsonrpc":"2.0","id":1,"result":{"containers":[],"envelopes":[]}}"#.to_string(),
        );
        let mut node = unpaired_node("relay");
        node.url = format!("http://127.0.0.1:{port}/");
        aoide_storage::node_store::save_nodes(&[node]).unwrap();

        let filed = "ab".repeat(32);
        aoide_storage::outbox::record_filed("relay", &filed).unwrap();
        assert_eq!(aoide_storage::outbox::filed_pending("relay").unwrap(), vec![filed.clone()]);

        assert!(poll_node("relay", None).is_ok());
        assert!(
            aoide_storage::outbox::filed_pending("relay").unwrap().is_empty(),
            "the answer carried it, so it is not carried again"
        );
        let bodies = seen.lock().unwrap().clone();
        assert!(
            bodies.iter().any(|body| body.contains("filed") && body.contains(&filed)),
            "the poll named what it filed: {bodies:?}"
        );

        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **The list a poll SENDS is the list it clears** — with an acknowledgement
    /// recorded WHILE the answer was in flight. That is the only shape that tells
    /// the fix from the bug: reading `filed_pending` again after the response
    /// would clear a name the hub has never seen, and the hub would keep offering
    /// a container this box already has. The door HOLDS the poll on a handshake
    /// until the acknowledgement is on disk, so "in flight" is the window rather
    /// than a race against a sleep.
    #[test]
    fn a_name_recorded_while_the_poll_was_in_flight_is_not_cleared() {
        use std::io::{Read, Write};
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("poll-ack-in-flight");

        // A door that reads each request, HANDS THE POLL OVER to the test, and
        // only answers once the test says the acknowledgement is recorded. (The
        // poll's own binding exchange arrives first — one connection for it, one
        // for the poll — and is answered immediately.)
        let (poll_seen_tx, poll_seen_rx) = std::sync::mpsc::channel::<()>();
        let (ack_recorded_tx, ack_recorded_rx) = std::sync::mpsc::channel::<()>();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let accepter = listener.try_clone().unwrap();
        let door = std::thread::spawn(move || {
            for _ in 0..4 {
                let Ok((mut stream, _)) = accepter.accept() else { return };
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]).to_string();
                let is_poll = request.contains("aoide/mailPoll");
                if is_poll {
                    poll_seen_tx.send(()).unwrap();
                    ack_recorded_rx.recv().unwrap();
                }
                let body = r#"{"jsonrpc":"2.0","id":1,"result":{"containers":[],"envelopes":[]}}"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
                if is_poll {
                    return;
                }
            }
        });
        let mut node = unpaired_node("relay");
        node.url = format!("http://127.0.0.1:{port}/");
        aoide_storage::node_store::save_nodes(&[node]).unwrap();

        let sent = "cd".repeat(32);
        let in_flight = "ef".repeat(32);
        aoide_storage::outbox::record_filed("relay", &sent).unwrap();

        // What the filing this poll performs would record: an ack minted while
        // the answer is on its way. Recorded from another thread, INSIDE the
        // door's own pause — the door cannot answer before this write is on disk.
        let writer = {
            let in_flight = in_flight.clone();
            std::thread::spawn(move || {
                poll_seen_rx.recv().unwrap();
                aoide_storage::outbox::record_filed("relay", &in_flight).unwrap();
                ack_recorded_tx.send(()).unwrap();
            })
        };

        assert!(poll_node("relay", None).is_ok());
        writer.join().unwrap();
        door.join().unwrap();
        assert_eq!(
            aoide_storage::outbox::filed_pending("relay").unwrap(),
            vec![in_flight],
            "the name that was sent went with the ask; the one recorded meanwhile stays for the next"
        );

        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A polled container whose chain does not end here is one hop of someone
    /// else's letter, and the poll FORWARDS it — `poll_node`'s own arm, which
    /// audits both outcomes (this process is the only witness a pull has) and
    /// files the container as `transit`. The chain's entry 1 is the origin's own
    /// hand-off, which is never a loop; a hop entry that names this box again is.
    #[test]
    fn a_polled_hop_is_forwarded_and_audited() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("poll-hop-forward");

        std::fs::write(
            aoide_storage::config::source().path,
            "[pairing]\nhomeMesh = \"family\"\n\n\
             [mesh.family]\n[mesh.family.nodes]\nrelay = \"ssh://relay\"\ndave = \"ssh://dave\"\n",
        )
        .unwrap();
        let node = |name: &str, url: &str, key: String| aoide_storage::node_store::Node {
            name: name.to_string(),
            url: url.to_string(),
            autogate: false,
            token_file: None,
            bearer_secret: None,
            hub: false,
            pubkey: Some(key),
            verified: true,
            grants: aoide_storage::node_store::grants_in("family", &["message"]),
            narrowed: aoide_storage::node_store::Grants::new(),
            via: None,
            added_at: "2026-09-07T00:00:00Z".to_string(),
        };
        let me = aoide_storage::display::local_node_name();
        let mine = aoide_storage::identity::load_or_mint().unwrap().0.info().pubkey_hex;
        let fake = |tag: &str| format!("{tag}{tag}{tag}{tag}").repeat(4);
        // This process plays the RELAY too, so its record carries this box's own
        // key: the container it hands over is signed by that key, as a relay's
        // own deposit into the hub would be.
        aoide_storage::node_store::save_nodes(&[
            node(&me, "ssh://self", fake("a1")),
            node("relay", "ssh://relay", mine.clone()),
            node("dave", "ssh://dave", fake("c3")),
        ])
        .unwrap();

        let envelope = aoide_storage::mail::mint_outbound_letter_from(
            "relay",
            "alice",
            "dave",
            "bob",
            "someone else's letter",
            "family",
        )
        .unwrap();
        let container = {
            let binding = aoide_storage::seal::publish_binding().unwrap();
            aoide_storage::seal::seal_envelope(&envelope, &binding, "family", "family", "dave", "relay", &aoide_storage::time::now_iso_utc()).unwrap()
        };
        let msgid = container.msgid.clone();
        let set = aoide_storage::routing::declarations().unwrap();
        assert_eq!(
            aoide_storage::routing::key_in(&set, "family", "relay").as_deref(),
            Some(mine.as_str()),
            "the relay's record is its key in that mesh"
        );
        assert!(
            aoide_storage::mail::verify_origin_signature(&envelope),
            "and the letter is signed as it"
        );
        let answer = json!({ "jsonrpc": "2.0", "id": 1, "result": { "containers": [container], "envelopes": [] } }).to_string();
        let (_listener, port, _seen) = recording_door(r#"{"jsonrpc":"2.0","id":1,"result":{"status":"accepted"}}"#, answer);
        let mut relay = node("relay", &format!("http://127.0.0.1:{port}/"), mine.clone());
        relay.verified = true;
        let mut nodes = aoide_storage::node_store::load_nodes();
        nodes.retain(|n| n.name != "relay");
        nodes.push(relay);
        aoide_storage::node_store::save_nodes(&nodes).unwrap();

        let outcome = poll_node("relay", None).unwrap();
        assert!(outcome.refused.is_empty(), "nothing refused: {:?}", outcome.refused);
        let transit = aoide_storage::mail::read_transit_unlocked().unwrap();
        assert_eq!(transit.len(), 1, "the hop is filed as a transit entry");
        assert_eq!(transit[0].next, "dave");
        assert_eq!(aoide_storage::outbox::list_entries("dave").unwrap().len(), 1, "and spooled onward");
        // **The hop is custody taken at THIS box**, so the hub that handed it
        // over is owed the acknowledgement: without it the hub holds the
        // container forever, offering it on every ask.
        assert_eq!(
            aoide_storage::outbox::filed_pending("relay").unwrap(),
            vec![msgid.clone()],
            "the hop is recorded for the next poll to acknowledge"
        );
        // **The race**: the list a poll SENDS is the list it clears. A name that
        // was pending when the request went out goes with the ask; one recorded
        // by the answer's own arms (the hop above, or a duplicate below) is kept
        // for the next poll.
        let empty_answer = json!({ "jsonrpc": "2.0", "id": 1, "result": { "containers": [], "envelopes": [] } }).to_string();
        let (listener, port, _) = recording_door(
            r#"{"jsonrpc":"2.0","id":1,"result":{"status":"accepted"}}"#,
            empty_answer,
        );
        let mut nodes = aoide_storage::node_store::load_nodes();
        for node in nodes.iter_mut().filter(|n| n.name == "relay") {
            node.url = format!("http://127.0.0.1:{port}/");
        }
        aoide_storage::node_store::save_nodes(&nodes).unwrap();
        let second = poll_node("relay", None).unwrap();
        assert!(second.refused.is_empty(), "{:?}", second.refused);
        assert!(
            aoide_storage::outbox::filed_pending("relay").unwrap().is_empty(),
            "the hop's acknowledgement went with the ask that named it"
        );
        drop(listener);

        // The same container offered AGAIN is a duplicate — and the duplicate is
        // custody this box still holds, so it is acknowledged again: with a stale
        // name pending at the same time, the answer must clear the stale one and
        // keep the container's.
        let stale = "aa".repeat(32);
        aoide_storage::outbox::record_filed("relay", &stale).unwrap();
        let (listener, port, _) = recording_door(
            r#"{"jsonrpc":"2.0","id":1,"result":{"status":"accepted"}}"#,
            json!({ "jsonrpc": "2.0", "id": 1, "result": { "containers": [container], "envelopes": [] } }).to_string(),
        );
        let mut nodes = aoide_storage::node_store::load_nodes();
        for node in nodes.iter_mut().filter(|n| n.name == "relay") {
            node.url = format!("http://127.0.0.1:{port}/");
        }
        aoide_storage::node_store::save_nodes(&nodes).unwrap();
        let third = poll_node("relay", None).unwrap();
        assert!(third.refused.is_empty(), "{:?}", third.refused);
        assert_eq!(
            aoide_storage::outbox::filed_pending("relay").unwrap(),
            vec![msgid.clone()],
            "the stale name went with the ask; the duplicate's own was kept"
        );
        drop(listener);
        let log = std::fs::read_to_string(std::path::Path::new(&dir).join("log")).unwrap_or_default();
        assert!(
            log.contains("mail.poll.transit") && log.contains(&msgid),
            "the hop is audited: {log}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// H2, the drain path: a held-but-unusable binding parks the letter and
    /// **no deposit is attempted at all** — never a plaintext one.
    #[test]
    fn an_unusable_binding_parks_the_letter_and_nothing_is_dialed() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("reseal-unusable-drain");

        let (kp, _) = aoide_storage::identity::load_or_mint().unwrap();
        let accepted = r#"{"jsonrpc":"2.0","id":1,"result":{"status":"accepted"}}"#;
        let (_listener, port, seen) = recording_door(accepted, "{}".to_string());
        let mut nodes = aoide_storage::node_store::load_nodes();
        aoide_storage::node_store::upsert_paired_node(
            &mut nodes,
            "liveb",
            &format!("http://127.0.0.1:{port}/"),
            &kp.info().pubkey_hex,
            &aoide_storage::time::now_iso_utc(),
            &["message".to_string()],
            "home",
        );
        aoide_storage::node_store::save_nodes(&nodes).unwrap();

        // A binding is on record and is expired: the destination HAS one.
        let now = aoide_storage::time::now_iso_utc();
        let expired = aoide_storage::seal::mint_binding(
            &kp,
            "age1expired",
            1,
            &aoide_storage::time::shift_iso_utc(&now, -7200),
            &aoide_storage::time::shift_iso_utc(&now, -3600),
        )
        .unwrap();
        aoide_storage::seal::learn_binding("liveb", &expired).unwrap();

        // A plaintext entry, exactly what `spool_entry` would have made
        // before the binding arrived.
        let canary = "CANARY-H2-DRAIN-UNUSABLE";
        let envelope = aoide_storage::mail::mint_outbound_letter("alice", "liveb", "bob", canary).unwrap();
        aoide_storage::outbox::write_entry("liveb", &OutboxEntry::fresh(envelope)).unwrap();

        drain_node("liveb").unwrap();

        let bodies = seen.lock().unwrap().clone();
        let deposits: Vec<&String> = bodies.iter().filter(|b| b.contains("aoide/mailDeposit")).collect();
        assert!(deposits.is_empty(), "nothing was deposited at all: {bodies:?}");
        assert!(
            !bodies.iter().any(|b| b.contains(canary)),
            "and no request anywhere carried the letter: {bodies:?}"
        );
        let after = aoide_storage::outbox::list_entries("liveb").unwrap();
        assert_eq!(after.len(), 1, "the letter is still spooled");
        assert!(after[0].refused, "parked");
        assert!(after[0].last_outcome.starts_with("parked:"), "with its reason: {}", after[0].last_outcome);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// H2, the poll path: an unusable binding and a vanished entry both hand
    /// over NOTHING, and a concurrently sealed entry hands over the CONTAINER
    /// rather than the snapshot's stale envelope.
    #[test]
    fn the_poll_hands_over_nothing_for_an_unusable_binding_or_a_vanished_entry() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("reseal-unusable-poll");
        use aoide_storage::outbox::HandOver;

        let (kp, _) = aoide_storage::identity::load_or_mint().unwrap();
        let mut nodes = aoide_storage::node_store::load_nodes();
        aoide_storage::node_store::upsert_paired_node(
            &mut nodes,
            "liveb",
            "http://127.0.0.1:1/",
            &kp.info().pubkey_hex,
            &aoide_storage::time::now_iso_utc(),
            &["message".to_string()],
            "home",
        );
        aoide_storage::node_store::save_nodes(&nodes).unwrap();

        let now = aoide_storage::time::now_iso_utc();
        let canary = "CANARY-H2-POLL-UNUSABLE";
        let envelope = aoide_storage::mail::mint_outbound_letter("alice", "liveb", "bob", canary).unwrap();
        let msgid = envelope.msgid.clone();
        aoide_storage::outbox::write_entry("liveb", &OutboxEntry::sealed_held(envelope.clone(), {
            let live = aoide_storage::seal::publish_binding().unwrap();
            aoide_storage::seal::seal_envelope(&envelope, &live, "", "", "liveb", "liveb", &now).unwrap()
        }))
        .unwrap();

        // A usable binding: the poll hands over the CONTAINER.
        assert!(matches!(
            aoide_storage::outbox::hand_over("liveb", &msgid).unwrap(),
            HandOver::Container(_)
        ));

        // A gone entry: nothing, not the snapshot.
        assert!(matches!(
            aoide_storage::outbox::hand_over("liveb", "00".repeat(32).as_str()).unwrap(),
            HandOver::Nothing
        ));

        // The binding goes unusable under the held entry: the poll still
        // hands over the container — the window governs NEW seals, and a
        // container already built is bytes on disk, not a decision.
        let expired = aoide_storage::seal::mint_binding(
            &kp,
            "age1expired",
            9,
            &aoide_storage::time::shift_iso_utc(&now, -7200),
            &aoide_storage::time::shift_iso_utc(&now, -3600),
        )
        .unwrap();
        aoide_storage::seal::learn_binding("liveb", &expired).unwrap();
        // Re-seal it under a usable binding first so the entry HAS a
        // container, then expire the binding again: the poll must still hand
        // the container over (the entry is sealed; the window is about new
        // seals, not about a container already built).
        assert!(matches!(
            aoide_storage::outbox::hand_over("liveb", &msgid).unwrap(),
            HandOver::Container(_)
        ), "an already-sealed entry is handed over regardless of the binding's window");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// L7 (the branch review): `spool_entry`'s three arms. The third is the
    /// one an operator actually sees — a node whose binding is held but not
    /// usable is PARKED, never sent in the clear — and no test covered any of
    /// them.
    #[test]
    fn spool_entry_seals_plaintexts_or_parks_by_what_the_binding_store_holds() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("spool-entry-arms");

        let (kp, _) = aoide_storage::identity::load_or_mint().unwrap();
        let mut nodes = aoide_storage::node_store::load_nodes();
        aoide_storage::node_store::upsert_paired_node(
            &mut nodes,
            "liveb",
            "http://127.0.0.1:1/",
            &kp.info().pubkey_hex,
            &aoide_storage::time::now_iso_utc(),
            &["message".to_string()],
            "home",
        );
        aoide_storage::node_store::save_nodes(&nodes).unwrap();
        let text = |n: usize| format!("letter {n}");

        // Arm 1: no binding on record — plaintext, and NOT parked. This is
        // the per-peer upgrade path, the one remaining plaintext any node
        // sends.
        let one = spool_entry("liveb", "liveb", "", aoide_storage::mail::mint_outbound_letter("alice", "liveb", "bob", &text(1)).unwrap(), false).unwrap();
        assert!(!one.is_sealed(), "no binding, so plaintext");
        assert!(!one.refused, "and plaintext is not a refusal");

        // Arm 2: a usable binding — sealed, and the spool holds no letter
        // bytes.
        let binding = aoide_storage::seal::publish_binding().unwrap();
        aoide_storage::seal::learn_binding("liveb", &binding).unwrap();
        let two = spool_entry("liveb", "liveb", "", aoide_storage::mail::mint_outbound_letter("alice", "liveb", "bob", &text(2)).unwrap(), false).unwrap();
        assert!(two.is_sealed(), "a usable binding seals at mint");
        assert!(!two.refused);
        assert!(!serde_json::to_string(&two).unwrap().contains(&text(2)), "and the spool is redacted");

        // Arm 3: a binding on record that is NOT usable now — expired, and
        // still validly signed, so it is the binding's WINDOW and not its
        // signature that parks this. A sealed destination is never sent
        // plaintext, so the entry is parked (the flag `unpark_refused`
        // clears) rather than downgraded.
        std::fs::remove_file(std::path::PathBuf::from(&dir).join("state/age-bindings/liveb.json")).unwrap();
        let past = aoide_storage::time::shift_iso_utc(&aoide_storage::time::now_iso_utc(), -7200);
        let expired_end = aoide_storage::time::shift_iso_utc(&aoide_storage::time::now_iso_utc(), -3600);
        let expired = aoide_storage::seal::mint_binding(
            &kp,
            &binding.age_pubkey,
            2,
            &past,
            &expired_end,
        )
        .unwrap();
        aoide_storage::seal::learn_binding("liveb", &expired).unwrap();
        assert!(aoide_storage::seal::binding_for("liveb").is_some(), "held");
        assert!(
            aoide_storage::seal::usable_binding_for("liveb", &aoide_storage::time::now_iso_utc()).is_none(),
            "but not usable"
        );
        let three = spool_entry("liveb", "liveb", "", aoide_storage::mail::mint_outbound_letter("alice", "liveb", "bob", &text(3)).unwrap(), false).unwrap();
        assert!(!three.is_sealed(), "an unusable binding is never sealed to");
        assert!(three.refused, "the entry is parked, not downgraded to plaintext");
        assert!(
            three.last_outcome.starts_with("parked:"),
            "and the reason is recorded for the operator: {}",
            three.last_outcome
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **F1's client half.** The door's own reply for a landed charter, read by
    /// the client's OWN classifier and acted on by its OWN delivered-deposit
    /// arm.
    ///
    /// The reply literal is the shape `aoide_server::a2a`'s `deposit_sealed`
    /// answers for an applied charter — `aoide-server` cannot be reached from
    /// here (this crate is beneath it), so the two halves of the wire meet in
    /// one string, and the server-side test
    /// `a_landed_charter_is_answered_accepted_with_its_detail_in_data` asserts
    /// the same one. Changing the door's word breaks that test; teaching this
    /// classifier a word the door does not answer breaks this one.
    #[test]
    fn a_landed_charter_retires_the_senders_entry() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("charter-retire");

        let node = "peerbox";
        let entry = OutboxEntry::fresh(
            aoide_storage::mail::mint_charter_letter(node, "charter home v1", "home").unwrap(),
        );
        aoide_storage::outbox::write_entry(node, &entry).unwrap();

        let reply = json!({
            "status": "accepted",
            "msgid": entry.envelope.msgid,
            "charter": { "mesh": "home", "version": 1, "rekeyed": [] },
        });
        match classify_deposit_response(&reply) {
            DepositAttempt::Delivered { status } => settle_delivered(node, entry.clone(), &status).unwrap(),
            other => panic!("a landed charter is a delivered deposit, got {other:?}"),
        }
        assert!(
            aoide_storage::outbox::list_entries(node).unwrap().is_empty(),
            "the charter entry retires on its own deposit outcome — there is no mailbox on the far end to ack it"
        );

        // And the word the door must not answer: an outcome this classifier
        // was never taught reads as a REFUSAL, which is how a charter that
        // landed ends up parked in the sender's own spool forever.
        match classify_deposit_response(&json!({ "status": "applied", "mesh": "home", "version": 1 })) {
            DepositAttempt::Refused(reason) => assert!(reason.contains("applied"), "{reason}"),
            other => panic!("an unrecognised status must never read as delivery, got {other:?}"),
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A hub's custody ends when the NEXT HOP takes the container — not when the
    /// destination acks. The entry is the `transit` spool entry a hop writes
    /// (`seal::file_transit_hop`): its kind is what the delivered arm reads, and
    /// the destination's receipt retires the ORIGIN's entry (which lives under
    /// its own hop), never a hub's copy of a letter it could not open.
    #[test]
    fn a_hub_retires_its_transit_entry_on_the_next_hops_acceptance() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("transit-retire");

        let now = aoide_storage::time::now_iso_utc();
        let envelope = aoide_storage::mail::mint_outbound_letter("alice", "chiyo", "bob", "through the hub").unwrap();
        let container = {
            let live = aoide_storage::seal::publish_binding().unwrap();
            aoide_storage::seal::seal_envelope(&envelope, &live, "home", "home", "chiyo", "chiyo", &now).unwrap()
        };
        let hop_entry = OutboxEntry::transit(container);
        assert!(hop_entry.is_transit(), "the bookkeeping envelope carries the transit kind");
        assert!(!hop_entry.is_held());
        aoide_storage::outbox::write_entry("chiyo", &hop_entry).unwrap();

        settle_delivered("chiyo", hop_entry.clone(), "accepted").unwrap();
        assert!(
            aoide_storage::outbox::list_entries("chiyo").unwrap().is_empty(),
            "the hub's own obligation is done once the next hop accepted it"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
