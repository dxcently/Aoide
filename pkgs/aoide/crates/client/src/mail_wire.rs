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
fn post_signed(node: &Node, method: &str, params: Value) -> SignedCall {
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
    let body_str = serde_json::to_string(&body).unwrap_or_default();

    let bearer = match crate::commands::resolve_node_bearer(node) {
        Ok(b) => b,
        Err(e) => return SignedCall::TransportFailed(format!("bearer resolve: {e}")),
    };
    let extra_headers = match crate::commands::sign_headers_for_node(node, &body_str) {
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
    /// No JSON-RPC response at all — dial/tunnel/HTTP/parse failure. The
    /// LINK is the suspect, not this entry.
    TransportFailed(String),
}

/// Build and send one `aoide/mailDeposit` POST, mirroring
/// [`crate::commands::spawn_on_node_via`]'s exact shape (resolve bearer,
/// sign, POST, parse, check `error`) with no `--via` override — a drain is
/// never given one; it only ever dials `node.via` as recorded.
fn attempt_deposit(node: &Node, envelope: &Envelope) -> DepositAttempt {
    let params = json!({ "envelope": envelope });
    let result = match post_signed(node, "aoide/mailDeposit", params) {
        SignedCall::Result(result) => result,
        SignedCall::Refused(detail) => return DepositAttempt::Refused(detail),
        SignedCall::TransportFailed(reason) => return DepositAttempt::TransportFailed(reason),
    };
    // A response with a `result` member but no (or non-string) `status` —
    // or with neither `result` nor `error` at all — is weaker evidence of
    // delivery than an unrecognised status string, and an unrecognised one
    // already falls to the catch-all below. So this default must land
    // there too, never on `"accepted"`: a malformed or non-conformant
    // peer response must never be read as a confirmed deposit.
    let status = result.get("status").and_then(Value::as_str).unwrap_or("missing-status");
    match status {
        "accepted" | "duplicate" => DepositAttempt::Delivered { status: status.to_string() },
        // MAIL.md §Wire's outcome vocabulary is closed to the three above —
        // `"refused"` and any string this client does not recognise both
        // mean the entry did NOT land, never that it did. Assuming success
        // for an unrecognised status is the exact failure this arm exists
        // to close: a letter waiting forever for an ack the far end was
        // never going to send.
        other => {
            let reason = result.get("reason").and_then(Value::as_str).unwrap_or(other);
            let detail = result.get("detail").and_then(Value::as_str);
            let msg = match detail {
                Some(d) => format!("{reason}: {d}"),
                None => reason.to_string(),
            };
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
    let Ok(ack) = aoide_storage::mail::mint_ack(&envelope.header.to.name, envelope.header.from.clone(), acked_msgid)
    else {
        return;
    };
    let origin_node = envelope.header.from.node.clone();
    let entry = aoide_storage::outbox::OutboxEntry::fresh(ack);
    if aoide_storage::outbox::write_ack_if_absent(&origin_node, acked_msgid, &entry) == Ok(true) {
        let _ = drain_node(&origin_node);
    }
}

/// Poll `node_name` once — the relay-first half of the model (MAIL.md §Wire,
/// P-M3): a node with no inbound address asks the node it can reach
/// "anything waiting for me?" and receives every entry that node spooled
/// toward it (`aoide_storage::outbox::poll_entries`), all `hold` ones and
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
/// no key for, which for a direct edge at P-M3 means a relayed letter from a
/// third node, P-M4's transit), is skipped: the pull has no response to
/// carry that news back, and one bad element must never cost the rest of
/// the batch. The count returned is envelopes FILED.
///
/// `Err` is the honest "we could not ask" — a refused or unreachable poll.
/// Nothing is recorded on the spool either way: a poll writes nothing, and
/// the entries the far end did not hand over are the far end's own state.
pub fn poll_node(node_name: &str) -> Result<usize, String> {
    let nodes = aoide_storage::node_store::load_nodes();
    let Some(node) = nodes.iter().find(|n| n.name == node_name) else {
        return Ok(0);
    };
    let params = json!({ "node": aoide_storage::display::local_host_name() });
    let result = match post_signed(node, "aoide/mailPoll", params) {
        SignedCall::Result(result) => result,
        SignedCall::Refused(detail) => return Err(detail),
        SignedCall::TransportFailed(reason) => return Err(reason),
    };
    let mut filed = 0usize;
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
    Ok(filed)
}

/// Drain `node_name`'s outbox once: every spooled, non-refused, non-`hold`
/// entry, oldest first, attempted in order — stopping at the first TRANSPORT
/// failure (the link itself is down; hammering the rest of the queue the
/// same pass gains nothing) but continuing past a REFUSAL (that one entry
/// is the problem, not the link — the next entry may well be fine).
///
/// **A `hold` entry is never attempted — the drain's one hard filter beside
/// `refused`.** It leaves only through its node's own `aoide/mailPoll`
/// (`poll_entries` is what offers it), so a pass that has nothing else to
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
/// concurrent drain (ruling 3 — skipped, never queued), an empty spool, or
/// a completed pass regardless of how many entries it delivered/refused/
/// backed off on. `Err` is reserved for a genuine local I/O failure
/// (`.bsy`'s own lock file, or an outbox read/write) — never for "the
/// remote node was unreachable," which is an ordinary, expected drain
/// outcome recorded in the entry/link state instead of surfaced as an
/// error to this function's own caller.
pub fn drain_node(node_name: &str) -> Result<(), String> {
    let nodes = aoide_storage::node_store::load_nodes();
    let Some(node) = nodes.iter().find(|n| n.name == node_name) else {
        return Ok(());
    };

    let Some(_link_lock) = aoide_storage::outbox::try_take_link_lock(node_name)? else {
        return Ok(());
    };
    let _tunnel_teardown = TunnelTeardownGuard(node_name.to_string());

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
        match attempt_deposit(node, &entry.envelope) {
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
                let mut updated = entry;
                updated.tries += 1;
                updated.last_try_at = aoide_storage::time::now_iso_utc();
                updated.last_outcome = format!("transport: {reason}");
                let recorded = aoide_storage::outbox::write_entry(node_name, &updated);
                aoide_storage::outbox::back_off(node_name, now_epoch, &reason)?;
                recorded?;
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
                aoide_storage::outbox::clear_link_state(node_name)?;
                if entry.envelope.header.kind == aoide_storage::mail::ENTRY_TYPE_RECEIPT {
                    // Ruling 4: a receipt's own successful deposit outcome
                    // (accepted OR duplicate — the far end has it now
                    // either way) IS confirmation; there is no separate
                    // ack-of-an-ack to wait for.
                    aoide_storage::outbox::remove_entry(node_name, &entry.envelope.msgid)?;
                } else {
                    // A letter waits for a REAL ack (spec item 7) — record
                    // the attempt and move on, never remove here.
                    let mut updated = entry;
                    updated.tries += 1;
                    updated.last_try_at = aoide_storage::time::now_iso_utc();
                    updated.last_outcome = status;
                    aoide_storage::outbox::write_entry(node_name, &updated)?;
                }
            }
        }
    }
    // Poll-on-contact (this function's own doc): the same dial that just
    // carried our spool out asks what this node holds FOR US. `poll_node`
    // never returns an error worth surfacing — the drain already reported
    // whatever the deposit half found, and an unreachable-but-answering node
    // is not a local failure.
    if contacted {
        let _ = poll_node(node_name);
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
            allows: Vec::new(),
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
    /// stamped `header.from.node = display::local_host_name()` (P-M1's
    /// "self never crosses the wire"), so the only name whose recorded key
    /// can verify it is that one — the same "one process plays both roles"
    /// shortcut the server's `setup_verifiable_origin` documents. `url`
    /// points nowhere: the ack this filing spools must STAY spooled for the
    /// assertion, not be delivered and retired.
    fn register_local_origin() -> String {
        let me = aoide_storage::display::local_host_name();
        let (kp, _) = aoide_storage::identity::load_or_mint().unwrap();
        let mut node = unpaired_node(&me);
        node.pubkey = Some(kp.info().pubkey_hex.clone());
        node.verified = true;
        node.allows = vec!["message".to_string()];
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
    /// where a held entry leaves from (`outbox::poll_entries` hands it to the
    /// far end's own ask; the door half is pinned in `aoide-server`).
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

        assert_eq!(poll_node("relay").unwrap(), 1, "the first poll files the letter");
        assert_eq!(poll_node("relay").unwrap(), 0, "a re-poll before the ack files nothing a second time");

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

        let err = poll_node("relay").expect_err("a dead node cannot be polled");
        assert!(err.contains("transport") || !err.is_empty(), "the reason is carried, not swallowed: {err}");

        let rows = aoide_storage::outbox::list_entries("relay").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].envelope.msgid, held_msgid);
        assert_eq!(rows[0].tries, 0, "a failed poll is not an attempt on any entry");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
