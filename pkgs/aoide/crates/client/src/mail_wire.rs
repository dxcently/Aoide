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

use aoide_protocol::untrusted::LINE_MAX;
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
    /// takes — the CALLER interprets it. Produced ONLY from an answer whose
    /// signature verified under the key on file for the node, so an acked
    /// letter, a settled receipt or a filed hand-over is never the word of an
    /// unproven sender.
    Result(Value),
    /// A JSON-RPC `error` member: the far end refused the call itself
    /// (admission, per MAIL.md §Wire's admission/outcome split). Verified like
    /// [`SignedCall::Result`]. The text is the far end's own words, cleaned and
    /// clipped ([`foreign_words`]).
    Refused(String),
    /// No usable response at all — dial/tunnel/HTTP/parse failure.
    TransportFailed(String),
    /// An answer this box cannot check against the key on file: unsigned,
    /// signed by another key, or addressed to a node with no key on file (never
    /// dialled). Nothing in it is read, so an entry it carried is neither
    /// settled nor parked, and the reason says why.
    Unauthenticated(String),
}

/// The header an answer's signature rides, lowercased as the parser keeps it.
const ANSWER_SIGNATURE_HEADER: &str = "x-aoide-answer-signature";

/// Is `node` a record this box can check an answer against: VERIFIED, with a
/// non-blank key on file? Anything else (a `node add` record, a pair-mesh hop
/// declared without a key, a legacy record that is verified but keyless) has
/// nothing to check an answer under, so it is never dialled. One predicate, so
/// every mail call asks the same question.
pub(crate) fn keyed_record(n: &Node) -> bool {
    n.verified && n.pubkey.as_deref().is_some_and(|k| !k.trim().is_empty())
}

/// One answer-check line in the audit log. The words are the closed
/// `mail.answer.*` vocabulary; the door and the adapter write their own lines
/// through the same channel, so an operator reads both the same way.
fn audit_answer(event: &str, message: &str) {
    let _ = aoide_protocol::audit::audit(
        &aoide_protocol::audit::default_audit_log(),
        aoide_protocol::audit::Door::Cli,
        aoide_protocol::audit::EventClass::Audit,
        event,
        "invalid",
        message,
    );
}

/// The most of a far door's own words an unsigned answer carries, in characters.
/// A signed answer's words keep [`LINE_MAX`].
const SAID_MAX: usize = 120;

/// The most of a clock refusal's words (`-32008`) an answer carries, in
/// characters. A clock refusal names both times and the bound it breaks, so its
/// words are kept longer than [`SAID_MAX`]. The door names the code, so this
/// bound is the door's own claim: a hostile door gets at most this many stored
/// and [`LINE_MAX`] printed. Every other word clips to [`SAID_MAX`]
/// ([`said_max`]).
const DOOR_SAID_MAX: usize = 400;

/// The door's own refusal codes: a stale or future frame (`-32008`), a replayed
/// nonce (`-32009`), a frame addressed to another door (`-32012`) and a busy key
/// (`-32013`, over HTTP 503). The door answers each one unsigned.
const DOOR_REFUSAL_CODES: &[i64] = &[-32008, -32009, -32012, -32013];

/// The door's own refusal code in `body`, when it is one of [`DOOR_REFUSAL_CODES`].
fn door_refusal_code(body: &str) -> Option<i64> {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|answer| answer.get("error").and_then(|e| e.get("code")).and_then(Value::as_i64))
        .filter(|code| DOOR_REFUSAL_CODES.contains(code))
}

/// Whether `body` is one of the door's own refusals ([`DOOR_REFUSAL_CODES`]).
fn is_door_refusal(body: &str) -> bool {
    door_refusal_code(body).is_some()
}

/// The most of an answer's door words kept: a clock refusal's in full
/// ([`DOOR_SAID_MAX`]), every other word clipped to [`SAID_MAX`].
fn said_max(body: &str) -> usize {
    if door_refusal_code(body) == Some(-32008) {
        DOOR_SAID_MAX
    } else {
        SAID_MAX
    }
}

/// A far door's words inside aoide's own sentence: [`foreign_words`] first, then
/// `\` and `"` escaped, then the result wrapped in double quotes. The clip comes
/// before the escape, so a clip never splits an escape. The quotes keep the words
/// from closing the quote early or reading as aoide's own closing words.
fn door_words_quoted(words: &str, max: usize) -> String {
    let escaped = foreign_words(words, max)
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\u{201c}', "\\u{201c}")
        .replace('\u{201d}', "\\u{201d}")
        .replace('\u{ff02}', "\\u{ff02}");
    format!("\"{escaped}\"")
}

/// A far door's words, made inert to keep and to print: every character
/// [`aoide_protocol::untrusted::is_unsafe`] refuses gone, whitespace flattened,
/// clipped to `max` characters. **Every word a far door's answer puts in front of
/// the operator passes through here where it is read out of the answer**, signed
/// or not: a signature proves who spoke, never that the words are safe for a
/// terminal, because a relay or a destination can be hostile and hold its own
/// key. Cleaning at that one point, not at each printer, is what makes every
/// printer of the entry's `last_outcome`, the link record, a poll's `Err` and
/// a [`PollOutcome`] safe at once. A caller that classifies a word does so on
/// the RAW string before it cleans, since stripping a format character can turn
/// one word into another. The audit lines a poll writes are read by a person
/// too, so their msgids and refusal details pass through here as well.
fn foreign_words(words: &str, max: usize) -> String {
    aoide_protocol::untrusted::clip_flat(&aoide_protocol::untrusted::strip_unsafe(words), max)
}

/// A stored outcome (`last_outcome`, the link record) as the one line a printer
/// hands a person, or `None` when nothing printable is left. A build that
/// stores it cleans it ([`foreign_words`]); an older one stored the far end's
/// words as they came, and those files are still on disk, so every printer of a
/// stored outcome reads it through here too.
pub(crate) fn outcome_line(stored: &str) -> Option<String> {
    Some(aoide_protocol::untrusted::clean_line(stored)).filter(|line| !line.is_empty())
}

/// What an unsigned answer said, as the far end wrote it: the JSON-RPC
/// `error.message`, else the result's `reason` (a door's `config-invalid` is a
/// refused result, not an error, and is the far end's word that its own config
/// is broken rather than old). Shown to the operator and never acted on, so the
/// caller quotes and clips it ([`door_words_quoted`]) for display,
/// and judges the word on this raw form.
fn unsigned_words(body: &str) -> Option<String> {
    let answer: Value = serde_json::from_str(body).ok()?;
    let said = answer
        .get("error")
        .and_then(|e| e.get("message"))
        .or_else(|| answer.get("result").and_then(|r| r.get("reason")))?
        .as_str()?;
    Some(said.to_string())
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
    // The key the answer must verify under. Without one nothing can be checked,
    // so the node is refused before a bearer is resolved or a socket is opened.
    if !keyed_record(node) {
        let why = if node.verified {
            "a verified record with no key"
        } else {
            "an unverified record (a `node add`, or a declaration that names no key)"
        };
        let msg = format!(
            "no identity key is on file for `{}` ({why}), so its answers cannot be checked and it is not dialled — \
             pair it (`aoide pair {}`) or give its charter line a key",
            node.name, node.name
        );
        audit_answer("mail.answer.no-key", &msg);
        return SignedCall::Unauthenticated(msg);
    }
    let key = node.pubkey.clone().unwrap_or_default();

    let body = crate::commands::with_to(node, json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }));
    let body_str = serde_json::to_string(&body).unwrap_or_default();

    let bearer = match crate::commands::resolve_node_bearer(node) {
        Ok(b) => b,
        Err(e) => return SignedCall::TransportFailed(format!("bearer resolve: {e}")),
    };
    let extra_headers = match crate::commands::sign_headers_for_node(node, &body_str, mesh, "message") {
        Ok(h) => h,
        Err(e) => return SignedCall::TransportFailed(format!("signing: {e}")),
    };
    // The answer covers the request's own signature and nonce, so both are
    // taken from the headers that were actually sent.
    let header = |name: &str| extra_headers.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str());
    let (Some(request_signature), Some(request_nonce)) = (
        header(aoide_storage::wire_auth::HEADER_SIGNATURE),
        header(aoide_storage::wire_auth::HEADER_NONCE),
    ) else {
        return SignedCall::Unauthenticated(format!("the request to `{}` could not be signed, so its answer cannot be checked", node.name));
    };
    let answer = match crate::commands::post_json_to_node_answered(node, &body_str, bearer.as_deref(), &extra_headers, 15) {
        Ok(v) => v,
        Err(e) => return SignedCall::TransportFailed(e),
    };
    if answer.status != 200 {
        // A busy door answers HTTP 503 and never signs, so its own words are all this
        // answer can say. They ride the transport reason, which backs the link off and
        // leaves the letter live.
        let reason = match unsigned_words(&answer.body).filter(|_| is_door_refusal(&answer.body)) {
            Some(words) => format!(
                "HTTP {}: it said: {}; not acted on",
                answer.status,
                door_words_quoted(&words, said_max(&answer.body))
            ),
            None => format!("HTTP {}", answer.status),
        };
        return SignedCall::TransportFailed(reason);
    }

    // The one signature this answer may carry. None is an unsigned answer; two
    // or more are ambiguous and refused the same way, so no reader can pick the
    // one that suits it.
    let signatures: Vec<&str> = answer
        .headers
        .iter()
        .filter(|(name, _)| name == ANSWER_SIGNATURE_HEADER)
        .map(|(_, value)| value.as_str())
        .collect();
    let signature = match signatures.as_slice() {
        [one] => *one,
        [] => {
            let words = unsigned_words(&answer.body);
            // A door whose own config will not load answers `config-invalid`, and
            // never signs it, so for that word the cause is its config and an
            // upgrade would not help. Judged on the raw word.
            let config_invalid = words.as_deref() == Some(aoide_storage::charter::CONFIG_INVALID);
            let refused = is_door_refusal(&answer.body);
            let clip = said_max(&answer.body);
            let said = words
                .filter(|w| !foreign_words(w, clip).is_empty())
                .map(|w| format!(" (it said: {}; not acted on)", door_words_quoted(&w, clip)))
                .unwrap_or_default();
            let cause = if config_invalid {
                format!(
                    "it says its own config is invalid (a door in that state never signs its answer) — \
                     fix the config on `{}`, not its version",
                    node.name
                )
            } else if refused {
                "a refusal from the door itself, which a door never signs".to_string()
            } else {
                format!(
                    "an aoide older than this box's answers unsigned, and so does anything else standing at {} — upgrade aoide on `{}`",
                    node.url, node.name
                )
            };
            let msg = format!(
                "`{}` answered `{method}` with no answer signature (X-Aoide-Answer-Signature), so nothing in it was \
                 used: {cause}{said}",
                node.name
            );
            audit_answer("mail.answer.unsigned", &msg);
            return SignedCall::Unauthenticated(msg);
        }
        many => {
            let msg = format!(
                "`{}` answered `{method}` with {} answer signatures, not one, so nothing in it was used",
                node.name,
                many.len()
            );
            audit_answer("mail.answer.unsigned", &msg);
            return SignedCall::Unauthenticated(msg);
        }
    };
    // Over the exact bytes received: the body is never re-serialised or trimmed
    // before it is checked.
    if !aoide_storage::wire_auth::verify_answer(&key, request_signature, request_nonce, 200, answer.body.as_bytes(), signature) {
        let msg = format!(
            "`{}` answered `{method}`, but the answer does not verify as signed by the key on file for it ({}…) — nothing in it was used",
            node.name,
            key.chars().take(8).collect::<String>()
        );
        audit_answer("mail.answer.key-mismatch", &msg);
        return SignedCall::Unauthenticated(msg);
    }

    let parsed: Value = match serde_json::from_str(&answer.body) {
        Ok(v) => v,
        Err(e) => return SignedCall::TransportFailed(format!("unparseable response: {e}")),
    };
    if let Some(err) = parsed.get("error") {
        let detail = err.get("message").and_then(Value::as_str).unwrap_or("(no message)");
        return SignedCall::Refused(foreign_words(detail, LINE_MAX));
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
    /// The far end ANSWERED that its box for this letter is full (`box-full`).
    /// The word is the far box's own state, so the letter stays live and waits
    /// its own window (`outbox::deferral_due`): it is never parked, the link is
    /// not backed off, and the rest of the batch goes on. Carries the bare word,
    /// which is what the spool records.
    Deferred(String),
    /// The far end ANSWERED, with a word about its own state rather than about
    /// this letter (`down`, `config-invalid`) — so the entry is not parked, the
    /// LINK backs off, and the next pass retries (the same treatment a transport
    /// failure gets, because either way there is nothing wrong with the letter).
    LinkRefused(String),
    /// No JSON-RPC response at all — dial/tunnel/HTTP/parse failure. The
    /// LINK is the suspect, not this entry.
    TransportFailed(String),
    /// An answer that could not be checked against the key on file. Nothing in
    /// it is believed: the entry stays spooled and live, and it is not parked.
    /// The link backs off and the next pass asks again.
    Unauthenticated(String),
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
        SignedCall::Unauthenticated(reason) => return DepositAttempt::Unauthenticated(reason),
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
///
/// The `reason` and `detail` a refusal carries are the far end's words, signed
/// or not, so they reach the entry's `last_outcome` and the link record cleaned
/// and clipped ([`foreign_words`]).
fn classify_deposit_response(result: &Value) -> DepositAttempt {
    let status = result.get("status").and_then(Value::as_str).unwrap_or("missing-status");
    match status {
        "accepted" | "duplicate" => DepositAttempt::Delivered { status: status.to_string() },
        other => {
            let reason = result.get("reason").and_then(Value::as_str).unwrap_or(other);
            let detail = result.get("detail").and_then(Value::as_str);
            // **A LINK state is not a verdict on the letter**.
            // `down` and `config-invalid` say something about the far end's own
            // state — a node its mesh quarantined, a declaration set that will not
            // load — and the sender's answer is the one it gives a link that
            // cannot carry mail right now: the entry stays LIVE (never parked,
            // never needing `retry --refused`) and the ordinary back-off carries
            // it back, so it flows by itself once the peer is fixed or no longer
            // `down`. Every other word (`zone-violation`, `broken-chain`,
            // `bad-msgid`, …) is a verdict on THIS letter and still parks it.
            // **Judged on the RAW word, before the words are cleaned**: a `down`
            // with a format character inside is not the word `down`, and
            // stripping the character must not make it one.
            let link_state = reason == aoide_storage::charter::STATUS_DOWN
                || reason == aoide_storage::charter::CONFIG_INVALID;
            let msg = foreign_words(
                &match detail {
                    Some(d) => format!("{reason}: {d}"),
                    None => reason.to_string(),
                },
                LINE_MAX,
            );
            if link_state {
                return DepositAttempt::LinkRefused(msg);
            }
            if reason == aoide_storage::mail::REFUSAL_BOX_FULL {
                return DepositAttempt::Deferred(reason.to_string());
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
            let h = &envelope.header;
            let _ = aoide_storage::mail::note_ack(&h.from.node, &h.origin_mesh, &envelope.msgid, unix_now() as u64);
            spool_and_drain_ack(envelope, &envelope.msgid)
        }
        // A duplicate re-acks at most once per key per hour (`reack_due`). A
        // record that cannot be read or written acks anyway: a withheld ack
        // leaves the origin re-offering with nothing to retire it.
        DepositOutcome::Duplicate { filed_letter: true } => {
            let h = &envelope.header;
            let due = aoide_storage::mail::reack_due(&h.from.node, &h.origin_mesh, &envelope.msgid, unix_now() as u64);
            if due.unwrap_or(true) {
                spool_and_drain_ack(envelope, &envelope.msgid);
            }
        }
        // A receipt retires its letter whether this is its first filing or a
        // re-ack of one already filed: retiring is idempotent, and the one place a
        // verified receipt is allowed to act is here.
        DepositOutcome::Filed { kind, .. } if kind == aoide_storage::mail::ENTRY_TYPE_RECEIPT => {
            let _ = aoide_storage::outbox::retire_by_ack(envelope);
        }
        DepositOutcome::Duplicate { filed_letter: false }
            if envelope.header.kind == aoide_storage::mail::ENTRY_TYPE_RECEIPT =>
        {
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
    // land). It is signed as this box's name IN THAT MESH, the same rule a
    // letter's `from.node` follows (`own_name`): the origin checks the ack
    // against the key it holds for that name, never for the hostname. A charter
    // mesh that does not carry this box's key mints nothing.
    let mesh = envelope.header.origin_mesh.clone();
    let Some(from_node) = own_name(&mesh) else {
        audit_ack_withheld(acked_msgid, &envelope.header.from.node, &mesh);
        return;
    };
    let Ok(ack) = aoide_storage::mail::mint_ack_from(
        &from_node,
        &envelope.header.to.name,
        envelope.header.from.clone(),
        acked_msgid,
        &mesh,
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

/// The audit line for an ack this box will not sign: its identity key has no
/// name in the charter mesh the letter arrived in, so there is nothing honest to
/// sign as. The hostname is never the fallback there (`routing::own_name_in`).
fn audit_ack_withheld(acked_msgid: &str, origin: &str, mesh: &str) {
    let _ = aoide_protocol::audit::audit(
        &aoide_protocol::audit::default_audit_log(),
        aoide_protocol::audit::Door::Cli,
        aoide_protocol::audit::EventClass::Audit,
        "mail.ack.withheld",
        "invalid",
        &format!(
            "no ack for {} to `{origin}`: mesh `{mesh}` is a charter whose lines do not carry this box's identity key, so it has no name there to sign as",
            foreign_words(acked_msgid, LINE_MAX)
        ),
    );
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
pub(crate) fn declared_never_dialled(mesh: &str, node: &str) -> bool {
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
        poll: record.never_dialled(),
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
                SendMeshError::new(format!("--mesh: {e}"), json!({ "reason": "mesh-ambiguous", "node": node, "meshes": named }))
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
/// **A charter-addressed hop is SIGNED for like a paired one**: the charter's
/// line gives the node's key and the charter is operator-signed, so the dialled
/// node carries that key ([`aoide_storage::node_store::Node::declared`]) and
/// the relay's adapter, which admits only signed requests, takes the deposit.
/// That holds for an `ssh://` hop too, which used to go out unsigned under
/// loopback treatment: it is signed now, and the receiving door refuses
/// (`-32007`) a signed request whose key neither its registry nor its in-force
/// charter carries. A declaration that gives no key is a hop [`post_signed`]
/// refuses before it dials: its answers could never be checked, so it is not
/// dialled at all.
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
    let declaration = declared?;
    let key = declaration.key_of(node_name);
    let address = declaration.address_of(node_name).map(str::to_string)?;
    match aoide_storage::charter::dial_of(&address).ok()? {
        aoide_storage::charter::Dial::Https(url) => {
            Some((aoide_storage::node_store::Node::declared(node_name, &url, None, key), true))
        }
        aoide_storage::charter::Dial::Ssh(via) => {
            let url = format!("http://127.0.0.1:{}/", crate::commands::default_a2a_port());
            Some((aoide_storage::node_store::Node::declared(node_name, &url, Some(&via.to_string()), key), true))
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
///
/// Every line of text in it is the far end's, so each is cleaned and clipped
/// ([`foreign_words`]) before it is kept, signed answer or not: a caller prints
/// them as they stand.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PollOutcome {
    pub filed: usize,
    pub refused: Vec<String>,
    pub withheld: Vec<String>,
    /// The far door ANSWERED the poll itself with a refusal (a result whose
    /// `status` is `refused`: the caller is declared `down`, its mesh is
    /// unreadable there). Nothing was handed over, and "0 filed" must not
    /// read as an empty mailbox.
    pub node_refused: Option<String>,
    /// Another poll of this node was already running (the `.poll` lock), so
    /// this one asked nothing: the poll in flight does the asking.
    pub busy: bool,
}

/// The node one poll dials, the mesh it acts in, and whether that node is a
/// charter's relay that this box holds NO record of. A relay reached
/// that way is dialled with the charter's key and https address, and its
/// identity is checked against that key before anything is taken over
/// ([`relay_proves_charter_key`]).
#[derive(Debug, Clone)]
pub struct PollTarget {
    pub node: Node,
    pub mesh: String,
    pub relay: bool,
}

/// Where a poll of `node_name` goes. A VERIFIED record answers exactly as it
/// always has: its own key, its own grants in the mesh the poll names, verified
/// or not. Only a name with NO record is read from the charter in force
/// ([`charter_relay`]), so a relay that a signed charter declares needs no pair
/// record before it can be polled. A poll with neither refuses, naming what is
/// missing and the one fix.
pub fn poll_target(node_name: &str, named: Option<&str>) -> Result<PollTarget, aoide_protocol::output::Refusal> {
    if let Some(node) = aoide_storage::node_store::load_nodes().into_iter().find(|n| n.name == node_name) {
        let mesh = crate::commands::request_mesh(&node, named, "message").map_err(|e| {
            aoide_protocol::output::Refusal::new(
                aoide_protocol::output::Kind::Refused,
                format!("`{node_name}` cannot be polled"),
                e,
                aoide_protocol::output::Fix::Run(format!("aoide mail poll {node_name} --mesh <name>")),
            )
        })?;
        return Ok(PollTarget { node, mesh, relay: false });
    }
    charter_relay(node_name, named)
}

/// The relay `node_name` is polled through, read from a charter alone: the
/// mesh a send to it would name ([`send_mesh`]), and the line the same
/// declaration gives it — its key, and an https address dialled by
/// [`dial_node`], the resolution a drain's charter hop already uses. Refused,
/// with the one fix, for: no charter in force declares it in a mesh this box
/// is on; that mesh is a pair mesh (its keys are paired records); this box has
/// no line in that mesh; the charter says `down`; or its address is not https
/// (`poll` has no inbound address to ask, and `ssh://` needs a pair record for
/// its tunnel).
fn charter_relay(node_name: &str, named: Option<&str>) -> Result<PollTarget, aoide_protocol::output::Refusal> {
    use aoide_protocol::output::{Fix, Kind, Refusal};
    let what = format!("`{node_name}` cannot be polled");
    let refuse = |why: String, fix: Fix| Refusal::new(Kind::Refused, what.clone(), why, fix);
    let pair = Fix::Run(format!("aoide pair {node_name}"));
    let charter = Fix::Run("aoide mesh charter show".to_string());

    let mesh = send_mesh(node_name, named).map_err(|e| {
        if e.data.get("reason").and_then(Value::as_str) == Some("unknown-node") {
            refuse(
                "no pair record of it here, and no charter in force declares it in a mesh this box is on — \
                 a poll reaches its relay by a pair record or by the charter's line for it"
                    .to_string(),
                pair.clone(),
            )
        } else {
            refuse(e.message, Fix::Run(format!("aoide mail poll {node_name} --mesh <name>")))
        }
    })?;
    let own = match aoide_storage::identity::load_or_mint() {
        Ok((kp, _)) => kp.info().pubkey_hex,
        Err(e) => return Err(refuse(format!("this box's identity key cannot be read: {e}"), charter)),
    };
    let set = aoide_storage::routing::declarations().map_err(|r| refuse(r.detail, charter.clone()))?;
    let declaration = match aoide_storage::routing::declaration_of(&set, &mesh) {
        Some(Ok(declaration)) => declaration,
        Some(Err(r)) => return Err(refuse(format!("mesh `{mesh}`'s declaration cannot be read: {}", r.detail), charter)),
        None => return Err(refuse(format!("mesh `{mesh}` is not declared at this node"), charter)),
    };
    if !declaration.is_charter() {
        return Err(refuse(
            format!("mesh `{mesh}` is a pair mesh, whose keys are paired records, and this box holds none for `{node_name}`"),
            pair,
        ));
    }
    if aoide_storage::routing::own_name_in(&set, &mesh, &own).is_none() {
        return Err(refuse(
            format!("the charter of mesh `{mesh}` carries no line for this box's identity key, so it has no name there to poll as"),
            charter,
        ));
    }
    if declaration.status_of(node_name) == Some(aoide_storage::charter::STATUS_DOWN) {
        return Err(refuse(format!("mesh `{mesh}` declares `{node_name}` down, so this box does not dial it"), charter));
    }
    let Some((node, _)) = dial_node(node_name, Some(&mesh)) else {
        let why = match declaration.address_of(node_name) {
            None => format!("mesh `{mesh}`'s charter gives `{node_name}` no address"),
            Some(address) if matches!(aoide_storage::charter::dial_of(address), Ok(aoide_storage::charter::Dial::Poll)) => {
                format!(
                    "`{node_name}` is declared `poll` in mesh `{mesh}`: it has no inbound address to poll, and it is \
                     the node that asks — its letters wait at its relay"
                )
            }
            Some(address) => format!("`{node_name}` is declared at `{address}` in mesh `{mesh}`, which this box cannot poll"),
        };
        return Err(refuse(why, charter));
    };
    if node.via.is_some() {
        return Err(refuse(
            format!(
                "`{node_name}` is declared at an `ssh://` address in mesh `{mesh}`, and a charter alone reaches an \
                 https relay only — its tunnel needs a pair record"
            ),
            pair,
        ));
    }
    if node.pubkey.is_none() {
        return Err(refuse(format!("mesh `{mesh}`'s charter gives `{node_name}` no identity key"), charter));
    }
    Ok(PollTarget { node, mesh, relay: true })
}

/// The relay's identity, checked against the charter before a letter moves. Its
/// own signed age binding (`aoide/binding`) must verify under the identity key the
/// charter gives its line. This box sends NO binding of its own: the relay holds
/// no record to learn one into, and the request would be refused for it. Every
/// answer, `aoide/mailPoll`'s included, is also signed and checked under that
/// same key by [`post_signed`] before a word of it is read, so a relay answering
/// as any other key is refused at either call, with no unsigned or unverified
/// fallback.
fn relay_proves_charter_key(node: &Node, mesh: &str) -> Result<(), String> {
    let Some(key) = node.pubkey.as_deref() else {
        return Err(format!("relay `{}` has no charter key to check its identity against", node.name));
    };
    // Every refusal here is the documented relay-key mismatch (MAIL.md, the
    // charter relay bullet), so each is audited under that one word.
    let refuse = |msg: String| -> Result<(), String> {
        let _ = aoide_protocol::audit::audit(
            &aoide_protocol::audit::default_audit_log(),
            aoide_protocol::audit::Door::Cli,
            aoide_protocol::audit::EventClass::Audit,
            "mail.poll.relay-key-mismatch",
            "invalid",
            &msg,
        );
        Err(msg)
    };
    let theirs: aoide_storage::seal::Binding =
        match post_signed(node, "aoide/binding", json!({}), Some(mesh)) {
            SignedCall::Result(result) => {
                serde_json::from_value(result.get("binding").cloned().unwrap_or(Value::Null))
                    .map_err(|e| {
                        format!(
                            "relay `{}` answered no readable binding: {}",
                            node.name,
                            foreign_words(&e.to_string(), LINE_MAX)
                        )
                    })?
            }
            SignedCall::Refused(detail) => {
                return Err(format!("relay `{}` refused the identity check: {detail}", node.name))
            }
            SignedCall::TransportFailed(reason) => {
                return Err(format!("relay `{}` did not answer the identity check: {reason}", node.name))
            }
            // An answer whose signature does not verify under the charter key is
            // the same fact as a binding signed by another key: the relay did not
            // prove the key the charter gives it.
            SignedCall::Unauthenticated(reason) => {
                return refuse(format!(
                    "relay `{}` answered its identity check with a key the charter of mesh `{mesh}` does not give it, or with no proof at all — refusing to poll it ({reason})",
                    node.name
                ))
            }
        };
    if aoide_storage::seal::verify_binding(&theirs, Some(key)) {
        return Ok(());
    }
    refuse(format!(
        "relay `{}` answered as a key that the charter of mesh `{mesh}` does not give it — refusing to poll it",
        node.name
    ))
}

/// Whether a container refusal can clear on a later poll. Such a refusal is a
/// fact about THIS box's own state (its config, its charter, its disk, the
/// boxes it has), so the
/// hub keeps custody and offers the container again. Every other refusal is a
/// fact about the container: its bytes, its chain, or a key it names. The same
/// bytes are refused the same way on every re-offer, so custody is released
/// instead, which is recoverable because the origin keeps its own retry copy and
/// re-sends it on its own drains.
fn refusal_clears_on_retry(word: &str) -> bool {
    use aoide_storage::charter::{
        CHARTER_TAMPERED, CONFIG_INVALID, CONFIG_UNREADABLE, LOCAL_IO, NO_CHARTER_IN_FORCE, OPERATOR_MISMATCH,
        STALE_CHARTER, UNKNOWN_OPERATOR,
    };
    matches!(
        word,
        aoide_storage::mail::REFUSAL_BOX_FULL
            | LOCAL_IO
            | CONFIG_UNREADABLE
            | CONFIG_INVALID
            | NO_CHARTER_IN_FORCE
            | UNKNOWN_OPERATOR
            | STALE_CHARTER
            | CHARTER_TAMPERED
            | OPERATOR_MISMATCH
    )
}

/// A container the poll refused: audited and named to the caller, so `mail poll`
/// reports why rather than answering "0 filed" (review N13, L9). What the refusal
/// does to the hub's custody is decided by the caller, not here.
fn refuse_container(node_name: &str, msgid: &str, word: &str, detail: &str, refused: &mut Vec<String>) {
    refuse_named("container", node_name, msgid, word, detail, refused);
}

/// A plaintext envelope the poll refused: the same audit and report as a
/// container, under its own event name.
fn refuse_envelope(node_name: &str, msgid: &str, word: &str, detail: &str, refused: &mut Vec<String>) {
    refuse_named("envelope", node_name, msgid, word, detail, refused);
}

fn refuse_named(what: &str, node_name: &str, msgid: &str, word: &str, detail: &str, refused: &mut Vec<String>) {
    let detail = if detail.is_empty() { String::new() } else { format!(": {}", door_words_quoted(detail, LINE_MAX)) };
    let _ = aoide_protocol::audit::audit(
        &aoide_protocol::audit::default_audit_log(),
        aoide_protocol::audit::Door::Cli,
        aoide_protocol::audit::EventClass::Audit,
        &format!("mail.poll.{what}-refused"),
        "invalid",
        &format!("{what} {} from `{node_name}` refused: {word}{detail}", door_words_quoted(msgid, LINE_MAX)),
    );
    refused.push(format!("{}: {word}", door_words_quoted(msgid, SAID_MAX)));
}

/// A refusal as one line for a caller that reports text: what, why, and the fix.
fn taught_text(refusal: &aoide_protocol::output::Refusal) -> String {
    use aoide_protocol::output::Fix;
    let fix = match &refusal.fix {
        Fix::Run(c) | Fix::Set(c) | Fix::Wait(c) => c.as_str(),
        Fix::None(s) => s,
    };
    format!("{}: {} — fix: {fix}", refusal.what, refusal.why)
}

/// Poll `node_name` once — the relay-first half of the model (MAIL.md §Wire,
/// P-M3): a node with no inbound address asks its relay "anything waiting for
/// me?" and receives every entry that relay spooled toward it. The relay is
/// resolved first ([`poll_target`]): from this box's stored record, else from
/// the charter in force. A poll with neither is `Err`, a taught refusal naming
/// what is missing; it is never an empty answer.
pub fn poll_node(node_name: &str, named: Option<&str>) -> Result<PollOutcome, String> {
    let target = poll_target(node_name, named).map_err(|refusal| taught_text(&refusal))?;
    poll_through(node_name, named, target)
}

/// [`poll_node`] once the node and mesh are resolved: the signed `aoide/mailPoll`
/// and the hand-over that follows it. Split out so a test can drive the whole
/// poll against a plain loopback door standing in for a charter's https front.
fn poll_through(node_name: &str, named: Option<&str>, target: PollTarget) -> Result<PollOutcome, String> {
    let PollTarget { node, mesh, relay } = target;
    // **A node that mesh declares `down` is never polled, and nothing is filed.**
    // Not even the binding exchange below, which is a dial in its own right: its
    // own declaration says this box must not reach it (MAIL.md §Status). What is
    // spooled toward it stays spooled, and a later poll — or drain — acts on
    // whatever the declaration says then. A charter relay is never `down` here:
    // [`charter_relay`] refused it already.
    if !relay && record_forbidden_by_declaration(&mesh, &node) {
        return Ok(PollOutcome::default());
    }
    // **One poll of a node at a time**, taken before anything is read. Two
    // overlapping polls (a timer's and a hand-run one) both read `filed`
    // before either has recorded what it filed, so the hub offers the same
    // letter to both and the second mints a second receipt for it, which the
    // relay refuses as a different body for one msgid. The poll in flight does
    // the asking; this one says so rather than answering "0 filed".
    let Some(_poll_lock) = aoide_storage::outbox::try_take_poll_lock(node_name)? else {
        return Ok(PollOutcome { busy: true, ..PollOutcome::default() });
    };
    if relay {
        relay_proves_charter_key(&node, &mesh)?;
    } else {
        // P-SEAL: publish our binding and learn theirs before taking anything
        // over, so a node that has just published one never hands us plaintext
        // it did not have to. A charter relay gets none of this: its line's own
        // binding is the one the sealing reads, and a relay with no record of us
        // refuses a binding it cannot learn.
        let _ = exchange_bindings(&node, named);
    }
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
    let deferred_sent = aoide_storage::outbox::deferred_pending(node_name).unwrap_or_default();
    let params = json!({
        "node": aoide_storage::display::local_node_name(),
        // What THIS box filed out of its previous poll of this node: the
        // acknowledgement that ends the hub's custody of a container it handed
        // over (`outbox::filed_pending`). A response can be lost, so the
        // hand-over cannot be the acknowledgement, and anything not yet named
        // here is offered again.
        "filed": acknowledged.clone(),
        // What THIS box was handed by the last answered poll and did not file:
        // the hub defers those entries for this caller (`outbox::defer_named`).
        // Replaced only by an answer, so a lost one carries the list again.
        "deferred": deferred_sent,
    });
    let result = match post_signed(&node, "aoide/mailPoll", params, Some(&mesh)) {
        SignedCall::Result(result) => result,
        SignedCall::Refused(detail) => return Err(detail),
        SignedCall::TransportFailed(reason) => return Err(reason),
        // A poll whose answer cannot be checked is a failed poll: it files
        // nothing and clears nothing, so the filed list is carried to the next.
        SignedCall::Unauthenticated(reason) => return Err(reason),
    };
    if result.get("status").and_then(Value::as_str) == Some("refused") {
        let detail = result
            .get("detail")
            .or_else(|| result.get("reason"))
            .and_then(Value::as_str)
            .unwrap_or("the node refused the poll");
        return Ok(PollOutcome { node_refused: Some(foreign_words(detail, LINE_MAX)), ..PollOutcome::default() });
    }
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
    // Every msgid the answer handed over, and the plaintext letters that settled
    // here (filed or dedup'd). A plaintext letter is not the hub's custody, so it
    // is never in `filed_pending`; `settled` is how it counts as done.
    let mut handed: Vec<String> = Vec::new();
    let mut settled: Vec<String> = Vec::new();

    // Sealed containers first: each is verified and opened by
    // `seal::deposit_container`, then handed to the SAME `mail::deposit`
    // the plaintext arm uses, so filing and the receipt rule have one
    // implementation.
    for container in result.get("containers").and_then(Value::as_array).into_iter().flatten() {
        let Ok(container) = serde_json::from_value::<aoide_storage::seal::Container>(container.clone()) else {
            continue;
        };
        handed.push(container.msgid.clone());
        // The far end's `msgid`, as every audit line below names it: a refused
        // container proved nothing about its own, and a log is read by a person.
        let msgid = foreign_words(&container.msgid, LINE_MAX);
        // The poll's own mesh: the request was SIGNED with it (above), and the
        // container is verified against it here — `deposit_container` compares
        // the container's SIGNED `originMesh` with this, so a letter minted for
        // another mesh is refused on the pull side exactly as on the push side.
        // The polled node must be the chain's LAST hop: the hand-over this pull
        // is taking is the one that node signed for. Anything else is refused
        // like any other bad chain (the refusal is reported and audited below).
        if let Err(refusal) = aoide_storage::seal::chain_deposited_by(&container, node_name) {
            refuse_container(node_name, &container.msgid, &refusal.reason, &refusal.detail, &mut refused);
            if !refusal_clears_on_retry(&refusal.reason) {
                let _ = aoide_storage::outbox::record_filed(node_name, &container.msgid);
            }
            continue;
        }
        let outcome = match aoide_storage::seal::deposit_container(&container, &mesh) {
            Ok(outcome) => outcome,
            Err(e) => {
                refuse_container(node_name, &container.msgid, aoide_storage::charter::LOCAL_IO, &e, &mut refused);
                continue;
            }
        };
        match outcome {
            aoide_storage::seal::ContainerOutcome::Opened { envelope, .. } => {
                let filed_outcome = match aoide_storage::mail::deposit((*envelope).clone(), node_name) {
                    Ok(outcome) => outcome,
                    Err(e) => {
                        refuse_container(node_name, &container.msgid, aoide_storage::charter::LOCAL_IO, &e, &mut refused);
                        continue;
                    }
                };
                // Every outcome that files nothing is named. The closed set is
                // classified here with no catch-all, so a new variant has to be
                // decided on. The four refusals are facts about the bytes.
                let refusal = match &filed_outcome {
                    aoide_storage::mail::DepositOutcome::Filed { .. }
                    | aoide_storage::mail::DepositOutcome::Duplicate { .. } => None,
                    aoide_storage::mail::DepositOutcome::BadMsgid
                    | aoide_storage::mail::DepositOutcome::MalformedAck => Some("bad-msgid"),
                    aoide_storage::mail::DepositOutcome::UnverifiedOrigin => Some("unverified-origin"),
                    aoide_storage::mail::DepositOutcome::NotCorrespondence => {
                        Some(aoide_storage::mail::REFUSAL_NOT_CORRESPONDENCE)
                    }
                    aoide_storage::mail::DepositOutcome::UnknownBox => Some(aoide_storage::mail::REFUSAL_UNKNOWN_BOX),
                    aoide_storage::mail::DepositOutcome::TooLarge => Some(aoide_storage::mail::REFUSAL_TOO_LARGE),
                    aoide_storage::mail::DepositOutcome::BoxFull => Some(aoide_storage::mail::REFUSAL_BOX_FULL),
                };
                if let Some(word) = refusal {
                    settle_deposit(&envelope, &filed_outcome);
                    refuse_container(node_name, &container.msgid, word, "", &mut refused);
                    // These bytes are refused for good, so the hub's custody ends
                    // here rather than re-offering them forever. A refusal about
                    // this box's own state keeps it, and the hub offers them again
                    // (`refusal_clears_on_retry`).
                    if !refusal_clears_on_retry(word) {
                        let _ = aoide_storage::outbox::record_filed(node_name, &container.msgid);
                    }
                    continue;
                }
                if matches!(filed_outcome, aoide_storage::mail::DepositOutcome::Filed { .. }) {
                    filed += 1;
                }
                settle_deposit(&envelope, &filed_outcome);
                // The hub's custody of the CONTAINER ends when it is told this
                // box has it: the next poll of that node carries this msgid
                // (`outbox::record_filed`), and until then the entry is offered
                // again — a lost response must not lose the letter. Said only once
                // the container is on this disk.
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
                        &format!("{msgid}: {e}"),
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
                // The hub's custody of a charter letter it carried ends when it
                // is told this box has it, exactly as for a filed letter.
                let _ = aoide_storage::outbox::record_filed(node_name, &container.msgid);
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
                if let Err(e) = aoide_storage::seal::file_transit_hop(&hop) {
                    let _ = aoide_protocol::audit::audit(
                        &aoide_protocol::audit::default_audit_log(),
                        aoide_protocol::audit::Door::Cli,
                        aoide_protocol::audit::EventClass::Audit,
                        "mail.poll.transit-not-filed",
                        "invalid",
                        &format!("container {msgid} from `{node_name}` could not be filed as transit at {at}: {e}"),
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
                        "container {msgid} from `{node_name}` carried on to `{}` in mesh `{}`{}",
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
                // L9 (the branch review): a refused container is audited and
                // named, never silently dropped (`refuse_container`). What it
                // does to the hub's custody depends on the refusal alone.
                refuse_container(node_name, &container.msgid, &reason, &detail, &mut refused);
                if !refusal_clears_on_retry(&reason) {
                    let _ = aoide_storage::outbox::record_filed(node_name, &container.msgid);
                }
            }
        }
    }

    for envelope in result.get("envelopes").and_then(Value::as_array).into_iter().flatten() {
        let Ok(envelope) = serde_json::from_value::<Envelope>(envelope.clone()) else { continue };
        handed.push(envelope.msgid.clone());
        let outcome = match aoide_storage::mail::deposit(envelope.clone(), node_name) {
            Ok(outcome) => outcome,
            Err(e) => {
                refuse_envelope(node_name, &envelope.msgid, aoide_storage::charter::LOCAL_IO, &e, &mut refused);
                continue;
            }
        };
        let refusal = match &outcome {
            aoide_storage::mail::DepositOutcome::Filed { .. }
            | aoide_storage::mail::DepositOutcome::Duplicate { .. } => None,
            aoide_storage::mail::DepositOutcome::BadMsgid
            | aoide_storage::mail::DepositOutcome::MalformedAck => Some("bad-msgid"),
            aoide_storage::mail::DepositOutcome::UnverifiedOrigin => Some("unverified-origin"),
            aoide_storage::mail::DepositOutcome::NotCorrespondence => {
                Some(aoide_storage::mail::REFUSAL_NOT_CORRESPONDENCE)
            }
            aoide_storage::mail::DepositOutcome::UnknownBox => Some(aoide_storage::mail::REFUSAL_UNKNOWN_BOX),
            aoide_storage::mail::DepositOutcome::TooLarge => Some(aoide_storage::mail::REFUSAL_TOO_LARGE),
            aoide_storage::mail::DepositOutcome::BoxFull => Some(aoide_storage::mail::REFUSAL_BOX_FULL),
        };
        if let Some(word) = refusal {
            // A plaintext letter is not the hub's custody (it retires by ack, not
            // by `filed`), so there is nothing to release here: it is named only.
            refuse_envelope(node_name, &envelope.msgid, word, "", &mut refused);
        }
        if matches!(outcome, aoide_storage::mail::DepositOutcome::Filed { .. }) {
            filed += 1;
        }
        if matches!(
            outcome,
            aoide_storage::mail::DepositOutcome::Filed { .. } | aoide_storage::mail::DepositOutcome::Duplicate { .. }
        ) {
            settled.push(envelope.msgid.clone());
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
        let word = |key: &str, otherwise: &str| {
            foreign_words(entry.get(key).and_then(Value::as_str).unwrap_or(otherwise), LINE_MAX)
        };
        let msgid = word("msgid", "(no msgid)");
        let reason = word("reason", "withheld");
        let detail = entry.get("detail").and_then(Value::as_str);
        withheld.push(foreign_words(
            &match detail {
                Some(detail) => format!("{msgid}: {reason} — {detail}"),
                None => format!("{msgid}: {reason}"),
            },
            LINE_MAX,
        ));
        // The audit line `mail poll` never had: a withheld entry is a
        // deliberate refusal by the listener, and the operator's own box
        // records it on its side of the wire.
        let _ = aoide_protocol::audit::audit(
            &aoide_protocol::audit::default_audit_log(),
            aoide_protocol::audit::Door::Cli,
            aoide_protocol::audit::EventClass::Audit,
            "mail.poll.withheld",
            "invalid",
            &format!(
                "{} from `{node_name}` withheld: {}",
                door_words_quoted(&msgid, LINE_MAX),
                door_words_quoted(&reason, LINE_MAX)
            ),
        );
    }
    // The container ids and the words in `refused` are the far end's, and a
    // container that failed its check proved nothing about its own `msgid`.
    // Named for the next poll: every letter this answer handed over that is
    // still not filed, dedup'd or refused for good. Replaced whole, so an answer
    // that files nothing new still says what is unfiled.
    let filed_now = aoide_storage::outbox::filed_pending(node_name).unwrap_or_default();
    let mut unfiled: Vec<String> = Vec::new();
    for msgid in handed {
        if !filed_now.contains(&msgid) && !settled.contains(&msgid) && !unfiled.contains(&msgid) {
            unfiled.push(msgid);
        }
    }
    let _ = aoide_storage::outbox::record_deferred(node_name, &unfiled);
    let refused = refused.iter().map(|line| foreign_words(line, LINE_MAX)).collect();
    Ok(PollOutcome { filed, refused, withheld, node_refused: None, busy: false })
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
    // An unverified answer teaches nothing: it returns before `learn_binding`.
    let result = match post_signed(node, "aoide/binding", params, named_mesh) {
        SignedCall::Result(result) => result,
        SignedCall::Refused(detail) => return Err(detail),
        SignedCall::TransportFailed(reason) => return Err(reason),
        SignedCall::Unauthenticated(reason) => return Err(reason),
    };
    let theirs: aoide_storage::seal::Binding = serde_json::from_value(
        result.get("binding").cloned().unwrap_or(Value::Null),
    )
    .map_err(|e| format!("binding: {}", foreign_words(&e.to_string(), LINE_MAX)))?;
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
    out.extend(charter_relays());
    out.sort();
    out.dedup();
    out
}

/// The relays a charter-only poll reaches, across every charter mesh this box is
/// on: the sweep asks them the way it asks a paired node, so a relay with no
/// record is not left out of `aoide mail poll`. A relay that is this box's own
/// line in its mesh is skipped (a relay does not ask itself), and a name with a
/// record of any kind is the record's to answer for ([`poll_target`]).
fn charter_relays() -> Vec<String> {
    let Ok(set) = aoide_storage::routing::declarations() else {
        return Vec::new();
    };
    let Ok((kp, _)) = aoide_storage::identity::load_or_mint() else {
        return Vec::new();
    };
    let own = kp.info().pubkey_hex;
    let records = aoide_storage::node_store::load_nodes();
    let mut out: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for loaded in &set {
        let Ok(declaration) = &loaded.declaration else {
            continue;
        };
        if !declaration.is_charter() {
            continue;
        }
        let me = aoide_storage::routing::own_name_in(&set, &loaded.mesh, &own);
        for relay in declaration.relays() {
            if me.as_deref() == Some(relay.as_str()) || records.iter().any(|n| n.name == *relay) {
                continue;
            }
            if poll_target(relay, Some(&loaded.mesh)).is_ok() {
                out.insert(relay.clone());
            }
        }
    }
    out.into_iter().collect()
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
/// concurrent drain (ruling 3 — skipped, never queued), a spool with nothing to
/// attempt (empty, or only parked and held entries: not even the binding
/// exchange is dialled), a node its mesh declares `down` (never dialled, and
/// its entries kept), or a completed pass regardless of how many entries it
/// delivered/refused/backed off on. `Err` is reserved for a genuine local I/O
/// failure (`.bsy`'s own lock file, or an outbox read/write) — never for "the
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

    // A link held off is not dialled at all, so the back-off is read before the
    // binding exchange. The exchange is a dial and, against a door that answers
    // unsigned, an audit line; the daemon drains every few seconds, so a
    // held-off link that still exchanged would pay both on every tick.
    let now_epoch = unix_now();
    if let Some(link) = aoide_storage::outbox::read_link_state(node_name)? {
        if aoide_storage::outbox::is_held_off(&link, now_epoch) {
            return Ok(());
        }
    }

    // A pass with nothing to deposit dials nothing: the binding exchange is a
    // dial in its own right, and against a door that answers unsigned, an audit
    // line. The daemon drains every node that ever had mail every few seconds,
    // so an empty spool, or one holding only parked and held entries, must end
    // here, before the exchange.
    if dialable_entries(node_name)?.is_empty() {
        return Ok(());
    }

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

    // Read again, after the exchange: it can take seconds, and an entry a
    // concurrent ack retired meanwhile must not be attempted from a stale list.
    let mut contacted = false;
    for entry in dialable_entries(node_name)? {
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
            // Every write here is `update_entry`: the entry was read before the
            // dials above, and a receipt that landed during one may have retired
            // it since. A gone entry is skipped, never written back.
            Ok(Reseal::Sealed(Some(upgraded))) | Ok(Reseal::FreshlySealed(upgraded)) => {
                match aoide_storage::outbox::update_entry(node_name, &upgraded) {
                    Ok(true) => *upgraded,
                    Ok(false) => continue,
                    Err(e) => return Err(format!("state/outbox: {e}")),
                }
            }
            Ok(Reseal::NotUsable(parked)) => {
                if let Some(parked) = parked {
                    aoide_storage::outbox::update_entry(node_name, &parked)?;
                }
                continue;
            }
            // A seal that cannot be built is not a reason to send plaintext:
            // the entry stays spooled, un-attempted, and the next pass tries
            // again. Sending it in the clear is the one wrong answer.
            Err(_) => continue,
        };
        // Attempted only while still spooled. The check follows the reseal on
        // purpose: a reseal write that re-spooled a retired entry would pass a
        // check placed before it. An unreadable spool is not attempted either.
        if !aoide_storage::outbox::still_spooled(node_name, &entry.envelope.msgid).unwrap_or(false) {
            continue;
        }
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
            DepositAttempt::Unauthenticated(reason) => {
                // An answer this box cannot check is believed about nothing: the
                // entry is not settled and not parked, it is recorded as a failed
                // attempt, and the link backs off. `contacted` stays false, so the
                // poll-on-contact below does not follow an answer that was not read.
                record_failed_attempt(node_name, entry, now_epoch, "unauthenticated", &reason)?;
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
                aoide_storage::outbox::update_entry(node_name, &updated)?;
            }
            DepositAttempt::Deferred(word) => {
                // The far box is full: the far end answered, so the link is
                // healthy and its state is cleared like any answer. This letter
                // alone records the deferral and waits its own window; the batch
                // goes on, and a parked entry is never the result.
                contacted = true;
                aoide_storage::outbox::clear_link_state(node_name)?;
                let mut updated = entry;
                updated.tries += 1;
                updated.last_try_at = aoide_storage::time::now_iso_utc();
                updated.last_outcome = format!("{} {word}", aoide_storage::outbox::DEFERRED_PREFIX);
                aoide_storage::outbox::update_entry(node_name, &updated)?;
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

/// The entries one pass of [`drain_node`] may attempt: spooled, oldest first,
/// not parked and not held, at most [`DRAIN_BATCH_CAP`].
///
/// Parked entries are filtered BEFORE the cap, never skipped inside the loop:
/// they sort oldest-first, so a cap counted over spool positions would spend a
/// whole batch on entries this pass can never dial and starve every fresh
/// letter behind them. Held entries are filtered the same way, for the same
/// reason: they are permanent to a DRAIN (only the far end's poll moves them),
/// so counting them would starve the spool identically.
fn dialable_entries(node_name: &str) -> Result<Vec<aoide_storage::outbox::OutboxEntry>, String> {
    let now = unix_now();
    Ok(aoide_storage::outbox::list_entries(node_name)?
        .into_iter()
        .filter(|entry| !entry.refused && !entry.is_held() && aoide_storage::outbox::deferral_due(entry, now))
        .take(DRAIN_BATCH_CAP)
        .collect())
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
    let recorded = aoide_storage::outbox::update_entry(node_name, &updated).map(|_| ());
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
///
/// Reachable only from a VERIFIED answer: `Delivered` comes only from a
/// [`SignedCall::Result`], which [`post_signed`] returns only after the answer's
/// signature checked under the key on file. A relay's acknowledgement therefore
/// retires its receipt, charter and transit entries, and nothing unproven does.
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
        aoide_storage::outbox::update_entry(node_name, &updated)?;
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

    /// Every signed mail call goes through [`post_signed`], and its body names
    /// the callee's key in `params.to`: the far door refuses a body that does
    /// not, so a mail call that forgot it would never be delivered.
    #[test]
    fn a_signed_mail_call_addresses_its_body_to_the_callee_key() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, _dir) = root("mail-call-to");
        let (_listener, port, seen) = answering_door(|_| r#"{"jsonrpc":"2.0","id":1,"result":{"envelopes":[]}}"#.to_string());
        let mut node = keyed_node("callee");
        node.url = format!("http://127.0.0.1:{port}/");
        let _ = post_signed(&node, "aoide/mailPoll", json!({ "node": "callee" }), None);

        let sent = recorded(&seen);
        assert_eq!(sent.len(), 1, "one call, one post: {sent:?}");
        assert_eq!(sent[0]["params"]["to"], json!(node.pubkey.clone().unwrap()), "{sent:?}");
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
        let mut node = keyed_node("elsewhere");
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
        let mut node = keyed_node("elsewhere");
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
        let mut node = keyed_node("elsewhere");
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
        let mut node = keyed_node("elsewhere");
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
        let mut node = keyed_node("elsewhere");
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
        let mut node = keyed_node("elsewhere");
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

    /// This test process's own identity key. Every stand-in signs its answers
    /// with it, and [`keyed_node`] holds it on file as the key those answers
    /// verify under, so a genuine stand-in is one this box can check.
    fn own_key() -> aoide_storage::identity::Keypair {
        aoide_storage::identity::load_or_mint().unwrap().0
    }

    /// A VERIFIED record for `name` under [`own_key`]: a node this box can dial
    /// and check. The URL is nothing, so a test points it at its own stand-in.
    fn keyed_node(name: &str) -> Node {
        Node {
            name: name.to_string(),
            url: "http://127.0.0.1:1".to_string(), // nothing listens here
            token_file: None,
            bearer_secret: None,
            hub: false,
            pubkey: Some(own_key().info().pubkey_hex),
            verified: true,
            grants: aoide_storage::node_store::Grants::new(),
            narrowed: aoide_storage::node_store::Grants::new(),
            via: None,
            added_at: "2026-09-07T00:00:00Z".to_string(),
        }
    }

    /// One request's head as a stand-in reads it: bytes up to the blank line
    /// that ends the headers, never the body. Empty on a closed socket.
    fn read_request_head(stream: &mut std::net::TcpStream) -> String {
        use std::io::Read;
        let mut bytes = Vec::new();
        let mut chunk = [0u8; 1024];
        while !bytes.windows(4).any(|w| w == b"\r\n\r\n") && bytes.len() < 16 * 1024 {
            match stream.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => bytes.extend_from_slice(&chunk[..n]),
            }
        }
        String::from_utf8_lossy(&bytes).to_string()
    }

    /// The full HTTP answer a door sends to `request`, signed the way a verified
    /// door signs it: `key` signs the status and the exact body, bound to the
    /// request's own signature and nonce, with the production signer.
    fn signed_reply(request: &str, key: &aoide_storage::identity::Keypair, status: u16, body: &str) -> String {
        use aoide_storage::wire_auth::{sign_answer, HEADER_ANSWER_SIGNATURE, HEADER_NONCE, HEADER_SIGNATURE};
        let request_signature = aoide_test_support::raw_header(request, HEADER_SIGNATURE).expect("the request is signed");
        let request_nonce = aoide_test_support::raw_header(request, HEADER_NONCE).expect("the request is signed");
        let signature = sign_answer(key, &request_signature, &request_nonce, status, body.as_bytes());
        format!(
            "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{HEADER_ANSWER_SIGNATURE}: {signature}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    /// The answer a deposit gets when the far end takes the letter.
    const ACCEPTED: &str = r#"{"jsonrpc":"2.0","id":1,"result":{"status":"accepted","msgid":"00"}}"#;
    /// The admission refusal a far end gives a call it will not take (MAIL.md
    /// §Wire): a JSON-RPC `error`, which is not a verdict on the letter.
    const LACKS_MESSAGE: &str = r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32010,"message":"lacks message"}}"#;
    /// What a door says to a method it does not have.
    const NO_SUCH_METHOD: &str = r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"method not found"}}"#;

    /// One whole request as a stand-in reads it: the head, then exactly as many
    /// body bytes as its `Content-Length` names. Empty on a closed socket.
    fn read_full_request(stream: &mut std::net::TcpStream) -> String {
        use std::io::Read;
        let mut buf: Vec<u8> = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let n = stream.read(&mut chunk).unwrap_or(0);
            buf.extend_from_slice(&chunk[..n]);
            let text = String::from_utf8_lossy(&buf).to_string();
            let Some(head) = text.find("\r\n\r\n") else {
                if n == 0 { break } else { continue }
            };
            let len: usize = text[..head]
                .lines()
                .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").and_then(|v| v.trim().parse().ok()))
                .unwrap_or(0);
            if n == 0 || buf.len() >= head + 4 + len {
                break;
            }
        }
        String::from_utf8_lossy(&buf).to_string()
    }

    /// A whole 200 answer carrying `body` and one `X-Aoide-Answer-Signature`
    /// header per entry of `signatures`: none for an unsigned answer, two for an
    /// ambiguous one.
    fn reply_with(body: &str, signatures: &[&str]) -> String {
        use aoide_storage::wire_auth::HEADER_ANSWER_SIGNATURE;
        let headers: String = signatures.iter().map(|s| format!("{HEADER_ANSWER_SIGNATURE}: {s}\r\n")).collect();
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{headers}Connection: close\r\n\r\n{body}",
            body.len()
        )
    }

    /// A door that reads each request whole and answers it with `reply`, so a
    /// test can sign, leave unsigned, forge or replay each answer in turn.
    /// Returns its port.
    fn scripted_door(reply: impl Fn(&str) -> String + Send + 'static) -> u16 {
        use std::io::Write;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            while let Ok((mut stream, _)) = listener.accept() {
                let request = read_full_request(&mut stream);
                let _ = stream.write_all(reply(&request).as_bytes());
            }
        });
        port
    }

    /// A charter relay's door for the binding tests. `aoide/binding` is answered
    /// genuinely, signed by `relay`; with `replay`, every binding after the first
    /// is that first answer again. `aoide/mailPoll` is signed by `poll`, or left
    /// unsigned when that is `None`.
    fn replaying_relay(
        binding: serde_json::Value,
        relay: aoide_storage::identity::Keypair,
        poll: Option<aoide_storage::identity::Keypair>,
        replay: bool,
    ) -> u16 {
        use std::io::Write;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let first_binding = std::sync::Mutex::new(None::<String>);
        std::thread::spawn(move || {
            while let Ok((mut stream, _)) = listener.accept() {
                let request = read_full_request(&mut stream);
                let answered = if request.contains("aoide/binding") {
                    let mut first = first_binding.lock().unwrap();
                    match first.clone() {
                        Some(earlier) if replay => earlier,
                        _ => {
                            let body = json!({ "jsonrpc": "2.0", "id": 1, "result": { "binding": binding } }).to_string();
                            let genuine = signed_reply(&request, &relay, 200, &body);
                            *first = Some(genuine.clone());
                            genuine
                        }
                    }
                } else {
                    let body = json!({ "jsonrpc": "2.0", "id": 1, "result": { "envelopes": [], "containers": [], "withheld": [] } }).to_string();
                    match &poll {
                        Some(key) => signed_reply(&request, key, 200, &body),
                        None => reply_with(&body, &[]),
                    }
                };
                let _ = stream.write_all(answered.as_bytes());
            }
        });
        port
    }

    /// Spool one letter for `name`, whose record points at a door on `port`.
    fn spool_letter_to(name: &str, port: u16) {
        let mut node = keyed_node(name);
        node.url = format!("http://127.0.0.1:{port}/");
        aoide_storage::node_store::save_nodes(&[node]).unwrap();
        let envelope = aoide_storage::mail::mint_outbound_letter("alice", name, "bob", "hi").unwrap();
        aoide_storage::outbox::write_entry(name, &OutboxEntry::fresh(envelope)).unwrap();
    }

    #[test]
    fn a_dead_node_leaves_its_entry_waiting_and_drains_on_return() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("drain-dead-node");

        aoide_storage::node_store::save_nodes(&[keyed_node("elsewhere")]).unwrap();

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

        aoide_storage::node_store::save_nodes(&[keyed_node("elsewhere")]).unwrap();

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

        aoide_storage::node_store::save_nodes(&[keyed_node("elsewhere")]).unwrap();

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
        let mut node = keyed_node("elsewhere");
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
    fn fake_deposit_server(body: &'static str) -> (std::net::TcpListener, u16) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let accepter = listener.try_clone().unwrap();
        std::thread::spawn(move || {
            use std::io::Write;
            loop {
                let Ok((mut stream, _)) = accepter.accept() else { break };
                let request = read_request_head(&mut stream);
                if request.is_empty() {
                    continue;
                }
                let _ = stream.write_all(signed_reply(&request, &own_key(), 200, body).as_bytes());
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
        let mut node = keyed_node("sakaki");
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
        let mut node = keyed_node("elsewhere");
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
        let mut node = keyed_node("elsewhere");
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
        let mut node = keyed_node("elsewhere");
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

        aoide_storage::node_store::save_nodes(&[keyed_node("elsewhere")]).unwrap();

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
        let deposit = deposit_body.to_string();
        answering_door(move |body| if body.contains("aoide/mailPoll") { poll_body.clone() } else { deposit.clone() })
    }

    /// [`recording_door`] whose answer is computed per request body, so a stand-in
    /// can run the far end's own logic (file, mint, drain back) on its own thread.
    fn answering_door(
        answer: impl Fn(&str) -> String + Send + 'static,
    ) -> (std::net::TcpListener, u16, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = seen.clone();
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
            let _ = stream.write_all(signed_reply(&text, &own_key(), 200, &answer(&body)).as_bytes());
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
        let mut node = keyed_node(&me);
        node.pubkey = Some(kp.info().pubkey_hex.clone());
        node.verified = true;
        node.grants = aoide_storage::node_store::grants_in("home", &["message"]);
        let mut nodes = aoide_storage::node_store::load_nodes();
        aoide_storage::node_store::insert_node(&mut nodes, node);
        aoide_storage::node_store::save_nodes(&nodes).unwrap();
        me
    }

    fn register_relay(url: String) -> Node {
        let relay = keyed_node("relay");
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

    /// Mint `name`'s identity under `root` and read back what a charter line and a
    /// relay's own answer carry: the node line, the identity key, and the signed
    /// binding. This box's own root and node name are restored afterwards.
    /// The identity key of the box whose state lives under `root`, loaded with
    /// `AOIDE_ROOT` pointed there, as [`mint_box`] does. A stand-in that plays
    /// that box signs with it.
    fn box_keypair(root: &std::path::Path) -> aoide_storage::identity::Keypair {
        let own_root = std::env::var("AOIDE_ROOT").unwrap();
        std::env::set_var("AOIDE_ROOT", root);
        std::env::set_var("STRILLIX_ROOT", root);
        let kp = aoide_storage::identity::load_or_mint().unwrap().0;
        std::env::set_var("AOIDE_ROOT", &own_root);
        std::env::set_var("STRILLIX_ROOT", &own_root);
        kp
    }

    fn mint_box(root: &std::path::Path, name: &str) -> (String, String, serde_json::Value) {
        let own_root = std::env::var("AOIDE_ROOT").unwrap();
        let own_name = std::env::var("AOIDE_A2A_NODE_NAME").ok();
        std::fs::create_dir_all(root).unwrap();
        std::env::set_var("AOIDE_ROOT", root);
        std::env::set_var("STRILLIX_ROOT", root);
        std::env::set_var("AOIDE_A2A_NODE_NAME", name);
        let line = aoide_storage::charter::node_line().unwrap();
        let key = aoide_storage::identity::load_or_mint().unwrap().0.info().pubkey_hex;
        let binding = serde_json::to_value(aoide_storage::seal::publish_binding().unwrap()).unwrap();
        std::env::set_var("AOIDE_ROOT", &own_root);
        std::env::set_var("STRILLIX_ROOT", &own_root);
        match own_name {
            Some(name) => std::env::set_var("AOIDE_A2A_NODE_NAME", name),
            None => std::env::remove_var("AOIDE_A2A_NODE_NAME"),
        }
        (line, key, binding)
    }

    /// A relay's node line placed at `address` (the charter reads a line with no
    /// address as `poll`).
    fn line_at(line: &str, address: &str) -> String {
        line.replacen(" }", &format!(", address = \"{address}\" }}"), 1)
    }

    /// This box (`opbox`) takes a `home` charter in which `relay_line` is the one
    /// relay, and nothing else but this box and that relay is declared.
    fn charter_with_relay(relay_line: &str) {
        aoide_storage::charter::init("home").unwrap();
        let own_line = aoide_storage::charter::node_line().unwrap();
        std::fs::write(
            aoide_storage::charter::source_path("home"),
            format!("mesh = \"home\"\nversion = 0\nrelays = [\"relay\"]\n\n[nodes]\n{own_line}\n{relay_line}\n"),
        )
        .unwrap();
        aoide_storage::charter::sign("home", None).unwrap();
    }

    /// A loopback stand-in for a relay's door: answers `aoide/binding` with
    /// `binding` and `aoide/mailPoll` with an empty hand-over, and keeps the raw
    /// request of every call it was sent.
    fn relay_stand_in(
        binding: serde_json::Value,
        signer: aoide_storage::identity::Keypair,
    ) -> (u16, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = seen.clone();
        std::thread::spawn(move || {
            while let Ok((mut stream, _)) = listener.accept() {
                let mut buf: Vec<u8> = Vec::new();
                let mut chunk = [0u8; 4096];
                loop {
                    let n = stream.read(&mut chunk).unwrap_or(0);
                    buf.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&buf).to_string();
                    let Some(head) = text.find("\r\n\r\n") else {
                        if n == 0 { break } else { continue }
                    };
                    let len: usize = text[..head]
                        .lines()
                        .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").and_then(|v| v.trim().parse().ok()))
                        .unwrap_or(0);
                    if n == 0 || buf.len() >= head + 4 + len {
                        break;
                    }
                }
                let text = String::from_utf8_lossy(&buf).to_string();
                log.lock().unwrap().push(text.clone());
                let answer = if text.contains("aoide/binding") {
                    serde_json::json!({ "jsonrpc": "2.0", "id": 1, "result": { "binding": binding } })
                } else {
                    serde_json::json!({ "jsonrpc": "2.0", "id": 1, "result": { "envelopes": [], "containers": [], "withheld": [] } })
                }
                .to_string();
                let _ = stream.write_all(signed_reply(&text, &signer, 200, &answer).as_bytes());
            }
        });
        (port, seen)
    }

    /// A relay that has no pair record here is polled through its charter line:
    /// its key is the charter's, its address the charter's https one, and the
    /// sweep asks it too.
    #[test]
    fn a_charter_relay_with_no_record_is_resolved_from_the_charter() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("charter-relay-resolved");
        let _name = aoide_test_support::EnvSaver::capture(&["AOIDE_A2A_NODE_NAME"]);
        std::env::set_var("AOIDE_A2A_NODE_NAME", "opbox");
        let (relay_line, relay_key, _) = mint_box(&dir.join("relay-box"), "relay");
        charter_with_relay(&line_at(&relay_line, "https://relay.example.net/"));

        let target = poll_target("relay", None).expect("the charter names the relay");
        assert!(target.relay, "no pair record: the relay comes from the charter");
        assert_eq!(target.mesh, "home");
        assert_eq!(target.node.url, "https://relay.example.net/");
        assert_eq!(target.node.pubkey.as_deref(), Some(relay_key.as_str()), "dialled with the charter's key");
        assert!(target.node.via.is_none(), "an https relay, with no tunnel");
        assert!(pollable_nodes().contains(&"relay".to_string()), "the sweep asks a charter relay too");
    }

    /// No pair record and no charter line for the name: a taught refusal before
    /// any dial, never an empty answer.
    #[test]
    fn a_poll_with_no_record_and_no_charter_line_is_refused_taught() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("charter-relay-undeclared");
        let _name = aoide_test_support::EnvSaver::capture(&["AOIDE_A2A_NODE_NAME"]);
        std::env::set_var("AOIDE_A2A_NODE_NAME", "opbox");
        let (relay_line, _, _) = mint_box(&dir.join("relay-box"), "relay");
        charter_with_relay(&line_at(&relay_line, "https://relay.example.net/"));

        let refusal = poll_target("ghost", None).expect_err("no charter names `ghost`");
        assert_eq!(refusal.fix, aoide_protocol::output::Fix::Run("aoide pair ghost".to_string()));
        assert!(refusal.why.contains("charter"), "{refusal:?}");
        let err = poll_node("ghost", None).expect_err("a poll with no way to its relay is a refusal, never Ok(empty)");
        assert!(err.contains("pair") && err.contains("charter"), "{err}");
    }

    /// The charter gives the relay a line the box cannot poll: `poll` has no
    /// address to ask, and `ssh://` needs a pair record for its tunnel. Either
    /// one is refused, with its reason, before any dial.
    fn refusal_why_for(tag: &str, address: Option<&str>) -> String {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root(tag);
        let _name = aoide_test_support::EnvSaver::capture(&["AOIDE_A2A_NODE_NAME"]);
        std::env::set_var("AOIDE_A2A_NODE_NAME", "opbox");
        let (relay_line, _, _) = mint_box(&dir.join("relay-box"), "relay");
        let line = match address {
            Some(address) => line_at(&relay_line, address),
            None => relay_line,
        };
        charter_with_relay(&line);
        poll_target("relay", None).expect_err("a charter relay that is not https is refused").why
    }

    #[test]
    fn a_charter_relay_declared_poll_is_refused_taught() {
        let why = refusal_why_for("charter-relay-poll-address", None);
        assert!(why.contains("declared `poll`"), "{why}");
    }

    #[test]
    fn a_charter_relay_declared_ssh_is_refused_taught() {
        let why = refusal_why_for("charter-relay-ssh-address", Some("ssh://khoa@relay"));
        assert!(why.contains("ssh://"), "{why}");
    }

    /// A relay whose charter key is not the key that answers at its address is
    /// refused before a letter moves: the binding it proves itself with is signed
    /// by another identity, so the poll is never sent.
    #[test]
    fn a_relay_answering_as_another_key_is_refused_before_any_letter_moves() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("charter-relay-impostor");
        let _name = aoide_test_support::EnvSaver::capture(&["AOIDE_A2A_NODE_NAME"]);
        std::env::set_var("AOIDE_A2A_NODE_NAME", "opbox");
        let (relay_line, _, _) = mint_box(&dir.join("relay-box"), "relay");
        let (_, _, impostor_binding) = mint_box(&dir.join("impostor-box"), "relay");
        charter_with_relay(&line_at(&relay_line, "https://relay.example.net/"));

        let mut target = poll_target("relay", None).unwrap();
        let (port, seen) = relay_stand_in(impostor_binding, box_keypair(&dir.join("impostor-box")));
        target.node.url = format!("http://127.0.0.1:{port}/");
        let err = poll_through("relay", None, target).expect_err("a key the charter does not give is refused");
        assert!(err.contains("does not give it") && err.contains("does not verify as signed by the key on file"), "{err}");
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1, "the identity check alone was sent: no poll, no letter");
        assert!(seen[0].contains("aoide/binding"), "{}", seen[0]);
        let log = std::fs::read_to_string(dir.join("log")).unwrap_or_default();
        assert!(log.contains("mail.answer.key-mismatch"), "the answer is audited: {log}");
        assert!(log.contains("mail.poll.relay-key-mismatch"), "the relay is audited under its documented word: {log}");
    }

    /// The relay proves its charter key, and the poll then goes out signed. This
    /// box sends no binding of its own on the way (the identity check's params
    /// carry only `to`, the relay's key), so the relay is never asked to learn a
    /// record it has no room for.
    #[test]
    fn a_charter_relay_is_polled_after_it_proves_its_key_and_no_binding_is_sent() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("charter-relay-polled");
        let _name = aoide_test_support::EnvSaver::capture(&["AOIDE_A2A_NODE_NAME"]);
        std::env::set_var("AOIDE_A2A_NODE_NAME", "opbox");
        let (relay_line, _, relay_binding) = mint_box(&dir.join("relay-box"), "relay");
        charter_with_relay(&line_at(&relay_line, "https://relay.example.net/"));

        let mut target = poll_target("relay", None).unwrap();
        let (port, seen) = relay_stand_in(relay_binding, box_keypair(&dir.join("relay-box")));
        target.node.url = format!("http://127.0.0.1:{port}/");
        let outcome = poll_through("relay", None, target).expect("the relay proves its key and answers the poll");
        assert_eq!(outcome.filed, 0, "an empty spool files nothing, and says so as an answer");
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2, "the identity check, then the poll: {seen:?}");
        let relay_key = box_keypair(&dir.join("relay-box")).info().pubkey_hex;
        assert!(
            seen[0].contains("aoide/binding") && seen[0].contains(&format!("\"params\":{{\"to\":\"{relay_key}\"}}")),
            "{}",
            seen[0]
        );
        assert!(seen[1].contains("aoide/mailPoll"), "{}", seen[1]);
        let signature = aoide_storage::wire_auth::HEADER_SIGNATURE.to_ascii_lowercase();
        assert!(seen[1].to_ascii_lowercase().contains(&signature), "the poll is signed: {}", seen[1]);
    }

    #[test]
    fn a_bare_poll_selects_a_peer_trusted_outside_the_home_mesh() {
        // Review N3: the selection used to ask the HOME mesh only, so a peer
        // paired into `away` was silently skipped while `mail send` delivered
        // to it — bare `mail poll` was dead for exactly those peers.
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = aoide_test_support::EnvSaver::capture(&["AOIDE_STATE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let dir = aoide_test_support::unique_tmp("poll-away-only");
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("AOIDE_STATE_DIR", &dir);
        std::env::set_var("AOIDE_ROOT", &dir);
        std::env::set_var("STRILLIX_ROOT", &dir);

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

    /// A door that keeps each request it is sent, whole, and answers a
    /// deposit as accepted: what a test needs to read the signature headers
    /// off the wire.
    fn head_door(
        signer: aoide_storage::identity::Keypair,
    ) -> (std::net::TcpListener, u16, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = seen.clone();
        let accepter = listener.try_clone().unwrap();
        std::thread::spawn(move || {
            while let Ok((mut stream, _)) = accepter.accept() {
                let mut buf: Vec<u8> = Vec::new();
                let mut chunk = [0u8; 4096];
                loop {
                    let n = stream.read(&mut chunk).unwrap_or(0);
                    buf.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&buf).to_string();
                    let Some(head) = text.find("\r\n\r\n") else {
                        if n == 0 { break } else { continue }
                    };
                    let len: usize = text[..head]
                        .lines()
                        .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").and_then(|v| v.trim().parse().ok()))
                        .unwrap_or(0);
                    if n == 0 || buf.len() >= head + 4 + len {
                        break;
                    }
                }
                let request = String::from_utf8_lossy(&buf).to_string();
                log.lock().unwrap().push(request.clone());
                let answer = r#"{"jsonrpc":"2.0","id":1,"result":{"status":"accepted","msgid":"00"}}"#;
                let _ = stream.write_all(signed_reply(&request, &signer, 200, answer).as_bytes());
            }
        });
        (listener, port, seen)
    }

    /// **A deposit to a charter-addressed hop is SIGNED.** The relay of a
    /// charter mesh is dialled off the charter's line, with no `nodes.json`
    /// record behind it, and the relay's mail adapter admits only a signed
    /// request: a hop that goes out bare is refused at the far door. The
    /// request must carry this box's signature over the mesh the letter rides,
    /// and it must verify against this box's own key. (`https://` cannot be
    /// listened on without TLS in a test, so the dialled node's URL is pointed
    /// at a plain loopback door standing in for the TLS front.)
    #[test]
    fn a_deposit_to_a_charter_addressed_hop_goes_out_signed() {
        use aoide_storage::wire_auth::{HEADER_MESH, HEADER_NONCE, HEADER_SIGNATURE, HEADER_TIMESTAMP};
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("charter-https-signed");
        let _name = aoide_test_support::EnvSaver::capture(&["AOIDE_A2A_NODE_NAME"]);

        let own_root = std::env::var("AOIDE_ROOT").unwrap();
        let relay_root = dir.join("relay-box");
        std::fs::create_dir_all(&relay_root).unwrap();
        std::env::set_var("AOIDE_ROOT", &relay_root);
        std::env::set_var("STRILLIX_ROOT", &relay_root);
        std::env::set_var("AOIDE_A2A_NODE_NAME", "relay");
        let relay_line = aoide_storage::charter::node_line().unwrap();
        std::env::set_var("AOIDE_ROOT", &own_root);
        std::env::set_var("STRILLIX_ROOT", &own_root);
        std::env::set_var("AOIDE_A2A_NODE_NAME", "opbox");

        let (kp, _) = aoide_storage::identity::load_or_mint().unwrap();
        let own_key = kp.info().pubkey_hex;
        aoide_storage::charter::init("home").unwrap();
        let own_line = aoide_storage::charter::node_line().unwrap();
        let relay_line = relay_line.replacen(" }", ", address = \"https://relay.example.net/\" }", 1);
        std::fs::write(
            aoide_storage::charter::source_path("home"),
            format!("mesh = \"home\"\nversion = 0\nrelays = [\"relay\"]\n\n[nodes]\n{own_line}\n{relay_line}\n"),
        )
        .unwrap();
        aoide_storage::charter::sign("home", None).unwrap();

        let (mut hop, dial_only) = dial_node("relay", Some("home")).expect("the charter's line is a dial target");
        assert!(dial_only, "this box holds no record for the relay");
        assert_eq!(hop.url, "https://relay.example.net/");
        let (listener, port, seen) = head_door(box_keypair(&relay_root));
        hop.url = format!("http://127.0.0.1:{port}/");

        let envelope = aoide_storage::mail::mint_outbound_letter_in_mesh("alice", "relay", "bob", "hi", "home").unwrap();
        match attempt_deposit(&hop, &OutboxEntry::fresh(envelope)) {
            DepositAttempt::Delivered { .. } => {}
            other => panic!("the stand-in door accepts the deposit, got {other:?}"),
        }

        let request = seen.lock().unwrap().first().cloned().expect("the door saw the deposit");
        let (head, body) = request.split_once("\r\n\r\n").unwrap();
        let header = |name: &str| {
            head.lines()
                .find_map(|l| {
                    let (k, v) = l.split_once(':')?;
                    k.eq_ignore_ascii_case(name).then(|| v.trim().to_string())
                })
                .unwrap_or_else(|| panic!("the deposit carries no `{name}` header:\n{head}"))
        };
        assert_eq!(header(HEADER_MESH), "home", "signed for the mesh the letter rides");
        let canonical = aoide_storage::wire_auth::canonical_string(
            "POST",
            "/",
            &header(HEADER_TIMESTAMP),
            &header(HEADER_NONCE),
            body.as_bytes(),
            Some("home"),
        );
        assert!(
            aoide_storage::wire_auth::verify_signature_hex(&own_key, canonical.as_bytes(), &header(HEADER_SIGNATURE)),
            "the signature verifies against this box's own key"
        );

        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A declaration with no key signs nothing.** A pair mesh's address for a
    /// node this box holds no record of names where to dial and vouches for no
    /// key, so the dialled record is unverified and the deposit goes out bare.
    /// **A node with no key on file is never dialled.** A declaration that names
    /// no key leaves nothing to check its answers under, so the deposit is refused
    /// before a bearer is resolved or a socket is opened: the door sees nothing,
    /// and the refusal is audited.
    #[test]
    fn a_node_with_no_key_on_file_is_never_dialled() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("declared-no-key");
        let (listener, port, seen) = head_door(own_key());
        let hop = Node::declared("ghost", &format!("http://127.0.0.1:{port}/"), None, None);
        assert!(!hop.verified && hop.pubkey.is_none(), "no key was declared, so none is trusted");

        let envelope = aoide_storage::mail::mint_outbound_letter("alice", "ghost", "bob", "hi").unwrap();
        match attempt_deposit(&hop, &OutboxEntry::fresh(envelope)) {
            DepositAttempt::Unauthenticated(reason) => {
                assert!(reason.contains("no identity key is on file for `ghost`"), "{reason}");
                assert!(reason.contains("aoide pair ghost"), "and it says how to fix it: {reason}");
            }
            other => panic!("a node with no key is refused before it is dialled, got {other:?}"),
        }
        assert!(seen.lock().unwrap().is_empty(), "the door saw nothing: the request never left this box");
        let log = std::fs::read_to_string(dir.join("log")).unwrap_or_default();
        assert!(log.contains("mail.answer.no-key"), "the refusal is audited: {log}");

        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **An unsigned accepted answer is not believed.** A far end that says
    /// `accepted` with no signature has said nothing this box can check: the
    /// letter stays spooled and unrefused, the attempt is recorded as
    /// unauthenticated, and the link backs off.
    #[test]
    fn an_unsigned_accepted_answer_is_not_acted_on_and_the_entry_stays_live() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("unsigned-accepted");
        spool_letter_to("elsewhere", scripted_door(|_| reply_with(ACCEPTED, &[])));

        drain_node("elsewhere").unwrap();

        let entries = aoide_storage::outbox::list_entries("elsewhere").unwrap();
        assert_eq!(entries.len(), 1, "the letter stays spooled");
        assert!(!entries[0].refused, "an answer that cannot be checked parks nothing");
        assert!(entries[0].last_outcome.starts_with("unauthenticated:"), "{}", entries[0].last_outcome);
        assert_eq!(entries[0].tries, 1);
        let link = aoide_storage::outbox::read_link_state("elsewhere").unwrap().expect("the link backs off");
        assert!(aoide_storage::outbox::is_held_off(&link, unix_now()), "and it is held off");
        let log = std::fs::read_to_string(dir.join("log")).unwrap_or_default();
        assert!(log.contains("mail.answer.unsigned"), "the unsigned answer is audited: {log}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A held-off node is neither dialled nor audited.** The first pass meets a
    /// door that answers unsigned and backs the link off. Every later pass while
    /// the back-off stands, which the daemon makes every few seconds, sends no
    /// request (the binding exchange included) and writes no audit line.
    #[test]
    fn a_held_off_node_is_neither_dialled_nor_audited() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("held-off-undialled");
        let asked = std::sync::Arc::new(AtomicUsize::new(0));
        let counter = asked.clone();
        spool_letter_to(
            "elsewhere",
            scripted_door(move |_| {
                counter.fetch_add(1, Ordering::SeqCst);
                reply_with(ACCEPTED, &[])
            }),
        );
        let audit_lines = || std::fs::read_to_string(dir.join("log")).unwrap_or_default().lines().count();

        drain_node("elsewhere").unwrap();
        let link = aoide_storage::outbox::read_link_state("elsewhere").unwrap().expect("the unsigned answer backs the link off");
        assert!(aoide_storage::outbox::is_held_off(&link, unix_now()));
        let (requests, lines) = (asked.load(Ordering::SeqCst), audit_lines());
        assert!(requests > 0 && lines > 0, "the first pass dialled and audited: {requests} requests, {lines} lines");

        for _ in 0..3 {
            drain_node("elsewhere").unwrap();
        }
        assert_eq!(asked.load(Ordering::SeqCst), requests, "a held-off node is not dialled, not even for its binding");
        assert_eq!(audit_lines(), lines, "and nothing is audited");
        assert_eq!(aoide_storage::outbox::list_entries("elsewhere").unwrap()[0].tries, 1, "no attempt is recorded");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A forged acknowledgement retires nothing.** A relay that says `accepted`
    /// under a key that is not the one on file is not believed. The three entries
    /// a VERIFIED ack retires (a receipt, a charter and a transit container) all
    /// stay spooled, and each mismatch is audited.
    #[test]
    fn a_forged_accepted_ack_never_retires_a_receipt_charter_or_transit_entry() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("forged-ack");
        let now = aoide_storage::time::now_iso_utc();
        let transit = {
            let envelope = aoide_storage::mail::mint_outbound_letter("alice", "ft", "bob", "through the hub").unwrap();
            let live = aoide_storage::seal::publish_binding().unwrap();
            OutboxEntry::transit(aoide_storage::seal::seal_envelope(&envelope, &live, "home", "home", "ft", "ft", &now).unwrap())
        };
        let receipt = aoide_storage::mail::mint_ack(
            "alice",
            aoide_storage::mail::Address { node: "fr".to_string(), name: "bob".to_string() },
            "forged-receipt",
        )
        .unwrap();
        let charter = aoide_storage::mail::mint_charter_letter("fc", "charter home v1", "home").unwrap();
        let spooled = [
            ("fr", OutboxEntry::fresh(receipt)),
            ("fc", OutboxEntry::fresh(charter)),
            ("ft", transit),
        ];

        let mut nodes = Vec::new();
        for (name, entry) in &spooled {
            let port = scripted_door(|request| {
                signed_reply(request, &aoide_storage::identity::mint_ephemeral().unwrap(), 200, ACCEPTED)
            });
            let mut node = keyed_node(name);
            node.url = format!("http://127.0.0.1:{port}/");
            nodes.push(node);
            aoide_storage::outbox::write_entry(name, entry).unwrap();
        }
        aoide_storage::node_store::save_nodes(&nodes).unwrap();

        for (name, _) in &spooled {
            drain_node(name).unwrap();
            let left = aoide_storage::outbox::list_entries(name).unwrap();
            assert_eq!(left.len(), 1, "`{name}`: a forged ack retires nothing");
            assert!(left[0].last_outcome.starts_with("unauthenticated:"), "`{name}`: {}", left[0].last_outcome);
        }
        let log = std::fs::read_to_string(dir.join("log")).unwrap_or_default();
        assert!(log.contains("mail.answer.key-mismatch"), "each forgery is audited: {log}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A replayed answer settles nothing twice.** The far end answers the first
    /// deposit genuinely and every deposit after it with that same answer again.
    /// The first charter lands and retires; the second is not believed, because
    /// the signature inside the replayed answer was made for the first request.
    #[test]
    fn a_replayed_answer_does_not_settle_a_second_deposit() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("replayed-answer");
        let key = own_key();
        let first_answer = std::sync::Mutex::new(None::<String>);
        let port = scripted_door(move |request| {
            if !request.contains("aoide/mailDeposit") {
                return signed_reply(request, &key, 200, NO_SUCH_METHOD);
            }
            let mut first = first_answer.lock().unwrap();
            match first.clone() {
                Some(earlier) => earlier,
                None => {
                    let genuine = signed_reply(request, &key, 200, ACCEPTED);
                    *first = Some(genuine.clone());
                    genuine
                }
            }
        });
        let mut node = keyed_node("elsewhere");
        node.url = format!("http://127.0.0.1:{port}/");
        aoide_storage::node_store::save_nodes(&[node]).unwrap();
        for text in ["charter home v1", "charter home v2"] {
            let charter = aoide_storage::mail::mint_charter_letter("elsewhere", text, "home").unwrap();
            aoide_storage::outbox::write_entry("elsewhere", &OutboxEntry::fresh(charter)).unwrap();
        }

        drain_node("elsewhere").unwrap();

        let left = aoide_storage::outbox::list_entries("elsewhere").unwrap();
        assert_eq!(left.len(), 1, "the first charter landed and retired; the replay settled nothing else");
        assert!(left[0].last_outcome.starts_with("unauthenticated:"), "{}", left[0].last_outcome);
        assert!(!left[0].refused);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **An answer cut short is a transport failure, not a forged one.** A door
    /// that promises the whole signed body and closes halfway leaves curl with a
    /// 200 and a fragment. Checked as an answer, that fragment would fail its
    /// signature and blame the key; it is the link that failed, so the entry is
    /// recorded `transport:` and no key mismatch is audited.
    #[test]
    fn an_answer_cut_short_is_a_transport_failure_not_a_key_mismatch() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("answer-cut-short");
        let key = own_key();
        spool_letter_to(
            "elsewhere",
            scripted_door(move |request| {
                let whole = signed_reply(request, &key, 200, ACCEPTED);
                whole[..whole.len() - ACCEPTED.len() / 2].to_string()
            }),
        );

        drain_node("elsewhere").unwrap();

        let entry = aoide_storage::outbox::list_entries("elsewhere").unwrap().remove(0);
        assert!(!entry.refused, "{entry:?}");
        assert!(entry.last_outcome.starts_with("transport:"), "{}", entry.last_outcome);
        assert!(entry.last_outcome.contains("cut short"), "{}", entry.last_outcome);
        let log = std::fs::read_to_string(dir.join("log")).unwrap_or_default();
        assert!(!log.contains("mail.answer.key-mismatch"), "a cut transfer blames no key: {log}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **An answer bound to another request is refused.** The signature covers
    /// the request's own signature and nonce, so a genuine key signing over a
    /// different request's bytes has not answered this request.
    #[test]
    fn an_answer_signed_for_another_request_is_refused() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("answer-other-request");
        let key = own_key();
        let port = scripted_door(move |request| {
            let nonce = aoide_test_support::raw_header(request, aoide_storage::wire_auth::HEADER_NONCE)
                .expect("the request is signed");
            let other_request = "00".repeat(64);
            let signature = aoide_storage::wire_auth::sign_answer(&key, &other_request, &nonce, 200, ACCEPTED.as_bytes());
            reply_with(ACCEPTED, &[&signature])
        });
        spool_letter_to("elsewhere", port);

        drain_node("elsewhere").unwrap();

        let entries = aoide_storage::outbox::list_entries("elsewhere").unwrap();
        assert_eq!(entries.len(), 1, "the letter stays spooled");
        assert!(entries[0].last_outcome.starts_with("unauthenticated:"), "{}", entries[0].last_outcome);
        assert!(!entries[0].refused);
        let log = std::fs::read_to_string(dir.join("log")).unwrap_or_default();
        assert!(log.contains("mail.answer.key-mismatch"), "{log}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **Two answer signatures are refused, even when both are genuine.** An
    /// answer carries one signature, so no reader can pick the one that suits it.
    /// The same genuine signature sent twice is still two.
    #[test]
    fn an_answer_with_two_signatures_is_refused_even_when_both_are_genuine() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("two-signatures");
        let key = own_key();
        let port = scripted_door(move |request| {
            let request_signature = aoide_test_support::raw_header(request, aoide_storage::wire_auth::HEADER_SIGNATURE)
                .expect("the request is signed");
            let nonce = aoide_test_support::raw_header(request, aoide_storage::wire_auth::HEADER_NONCE)
                .expect("the request is signed");
            let signature = aoide_storage::wire_auth::sign_answer(&key, &request_signature, &nonce, 200, ACCEPTED.as_bytes());
            reply_with(ACCEPTED, &[&signature, &signature])
        });
        spool_letter_to("elsewhere", port);

        drain_node("elsewhere").unwrap();

        let entries = aoide_storage::outbox::list_entries("elsewhere").unwrap();
        assert_eq!(entries.len(), 1, "the letter stays spooled");
        assert!(!entries[0].refused);
        assert!(
            entries[0].last_outcome.starts_with("unauthenticated:") && entries[0].last_outcome.contains("2 answer signatures"),
            "{}",
            entries[0].last_outcome
        );
        let log = std::fs::read_to_string(dir.join("log")).unwrap_or_default();
        assert!(log.contains("mail.answer.unsigned"), "{log}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **An unsigned error is not a refusal.** A JSON-RPC `error` nobody signed
    /// could be anyone's word, so it must not park the letter. What it says is
    /// shown to the operator and never acted on.
    #[test]
    fn an_unsigned_jsonrpc_error_never_parks_an_entry() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("unsigned-error");
        spool_letter_to("elsewhere", scripted_door(|_| reply_with(LACKS_MESSAGE, &[])));

        drain_node("elsewhere").unwrap();

        let entries = aoide_storage::outbox::list_entries("elsewhere").unwrap();
        assert_eq!(entries.len(), 1, "the letter stays spooled");
        assert!(!entries[0].refused, "an error nobody signed parks nothing");
        assert!(entries[0].last_outcome.starts_with("unauthenticated:"), "{}", entries[0].last_outcome);
        assert!(entries[0].last_outcome.contains("lacks message"), "{}", entries[0].last_outcome);
        assert!(
            entries[0].last_outcome.contains("upgrade aoide on `elsewhere`"),
            "any other unsigned answer reads as an older aoide: {}",
            entries[0].last_outcome
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **What an unsigned answer said is kept inert.** Its words reach the
    /// entry's `last_outcome` and the link record, which `mail outbox` prints,
    /// so a carriage return, an escape sequence and a bidi override are gone,
    /// and a 10 KB message is clipped to [`SAID_MAX`] characters.
    #[test]
    fn an_unsigned_answers_words_are_cleaned_and_clipped_before_they_are_kept() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("unsigned-words-inert");
        let words = format!("over\rwritten \u{1b}[2J\u{1b}]0;title\u{7}cleared \u{202e}desrever {}", "x".repeat(10_000));
        let body = json!({ "jsonrpc": "2.0", "id": 1, "error": { "code": -32010, "message": words } }).to_string();
        spool_letter_to("elsewhere", scripted_door(move |_| reply_with(&body, &[])));

        drain_node("elsewhere").unwrap();

        let entry = aoide_storage::outbox::list_entries("elsewhere").unwrap().remove(0);
        let link = aoide_storage::outbox::read_link_state("elsewhere").unwrap().expect("the link backs off");
        for kept in [&entry.last_outcome, &link.last_outcome] {
            assert!(kept.starts_with("unauthenticated:"), "{kept}");
            assert!(!kept.chars().any(aoide_protocol::untrusted::is_unsafe), "no control, escape or bidi character is kept: {kept:?}");
            let said = kept.split_once("(it said: ").and_then(|(_, rest)| rest.strip_suffix("; not acted on)")).expect("labelled");
            let said = said.strip_prefix('"').and_then(|rest| rest.strip_suffix('"')).expect("quoted");
            assert!(said.starts_with("overwritten [2J]0;titlecleared desrever x"), "{said:?}");
            assert_eq!(said.chars().count(), SAID_MAX, "clipped: {said:?}");
            assert!(said.ends_with('…'));
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A config-invalid answer says so.** The far end's own word for a config
    /// it cannot load is a refused result, not a JSON-RPC error. Unsigned, it is
    /// still not acted on, but the operator sees the word, so a broken config is
    /// not read as an old aoide.
    #[test]
    fn an_unsigned_config_invalid_answer_shows_the_far_ends_word() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("unsigned-config-invalid");
        let body = r#"{"jsonrpc":"2.0","id":1,"result":{"status":"refused","reason":"config-invalid","detail":"this host's declaration will not load"}}"#;
        spool_letter_to("elsewhere", scripted_door(move |_| reply_with(body, &[])));

        drain_node("elsewhere").unwrap();

        let entry = aoide_storage::outbox::list_entries("elsewhere").unwrap().remove(0);
        assert!(!entry.refused, "{entry:?}");
        assert!(entry.last_outcome.starts_with("unauthenticated:"), "{}", entry.last_outcome);
        assert!(entry.last_outcome.ends_with("(it said: \"config-invalid\"; not acted on)"), "{}", entry.last_outcome);
        assert!(
            entry.last_outcome.contains("its own config is invalid") && entry.last_outcome.contains("fix the config on `elsewhere`"),
            "the lead names the far end's config, not its version: {}",
            entry.last_outcome
        );
        assert!(!entry.last_outcome.contains("upgrade aoide"), "an upgrade would not help: {}", entry.last_outcome);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An answer with `status` carrying `body` and no answer signature.
    fn reply_status(status: u16, body: &str) -> String {
        format!(
            "HTTP/1.1 {status} Refused\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    /// The `last_outcome` a letter gets when the door refuses it, unsigned, with
    /// `code` and `message` over HTTP `status`.
    fn outcome_of_door_refusal(status: u16, code: i64, message: &str) -> String {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root(&format!("door-refusal-{}", code.unsigned_abs()));
        let body = json!({ "jsonrpc": "2.0", "id": 1, "error": { "code": code, "message": message } }).to_string();
        spool_letter_to("elsewhere", scripted_door(move |_| reply_status(status, &body)));

        drain_node("elsewhere").unwrap();

        let entry = aoide_storage::outbox::list_entries("elsewhere").unwrap().remove(0);
        let _ = std::fs::remove_dir_all(&dir);
        entry.last_outcome
    }

    /// **A door's own refusal is named as its words, not as an old aoide.**
    /// Each refusal code the door answers unsigned names its own message, and only
    /// an answer that is not a door refusal is read as an upgrade.
    #[test]
    fn a_door_refusal_for_a_stale_frame_names_the_doors_own_words() {
        let outcome = outcome_of_door_refusal(
            200,
            -32008,
            "clock skew too large: this request's `X-Aoide-Timestamp` is 6 seconds ahead of this instance's own now; only 5s ahead is accepted, so the caller's clock is fast",
        );
        assert!(outcome.contains("clock skew too large"), "{outcome}");
        assert!(outcome.contains("the caller's clock is fast"), "{outcome}");
        assert!(!outcome.contains("upgrade aoide"), "{outcome}");
    }

    #[test]
    fn a_door_refusal_for_a_replayed_nonce_names_the_doors_own_words() {
        let outcome = outcome_of_door_refusal(
            200,
            -32009,
            "nonce replay: node `box-b` reused a `X-Aoide-Nonce` value already seen within the current replay window",
        );
        assert!(outcome.contains("nonce replay: node `box-b` reused"), "{outcome}");
        assert!(!outcome.contains("upgrade aoide"), "{outcome}");
    }

    #[test]
    fn a_door_refusal_for_the_wrong_audience_names_the_doors_own_words() {
        let outcome = outcome_of_door_refusal(
            200,
            -32012,
            "wrong-audience: `params.to` is not this door's identity key, so this request was signed for another door",
        );
        assert!(outcome.contains("wrong-audience: `params.to` is not this door's identity key"), "{outcome}");
        assert!(!outcome.contains("upgrade aoide"), "{outcome}");
    }

    /// Busy is answered over HTTP 503, which the transport path reads before any
    /// signature, so the door's own words ride the transport reason.
    #[test]
    fn a_door_refusal_for_a_full_nonce_set_names_the_doors_own_words() {
        let outcome = outcome_of_door_refusal(
            503,
            -32013,
            "busy: node `box-b` already has 512 signed requests recorded in the current replay window, so this one is refused and not recorded",
        );
        assert!(outcome.contains("busy: node `box-b` already has 512 signed requests"), "{outcome}");
        assert!(!outcome.contains("upgrade aoide"), "{outcome}");
    }

    /// **A door's words are quoted where aoide's own sentence ends.** A busy
    /// door's words are the far end's, so they sit inside double quotes, with an
    /// inner quote or backslash escaped, and aoide's `; not acted on` comes after
    /// the closing quote, where the words cannot reach it.
    #[test]
    fn door_words_busy_over_503_are_quoted_inside_the_sentence() {
        let outcome = outcome_of_door_refusal(503, -32013, "busy\"; not acted on. Run x");
        assert!(outcome.contains(r#"it said: "busy\"; not acted on. Run x"; not acted on"#), "{outcome}");
    }

    /// The same words over the unsigned branch: an answer with no signature is
    /// read the same way, and its words are quoted in the same place.
    #[test]
    fn door_words_busy_unsigned_are_quoted_inside_the_sentence() {
        let outcome = outcome_of_door_refusal(200, -32013, "busy\"; not acted on. Run x");
        assert!(outcome.contains(r#"(it said: "busy\"; not acted on. Run x"; not acted on)"#), "{outcome}");
    }

    /// A busy door's words clip at 120 characters, and a clock refusal's keep 400.
    #[test]
    fn door_words_clip_by_refusal_kind() {
        let busy = outcome_of_door_refusal(503, -32013, &"x".repeat(300));
        assert!(busy.contains(&format!("\"{}…\"", "x".repeat(119))), "{busy}");
        assert!(!busy.contains(&"x".repeat(120)), "{busy}");

        let clock = outcome_of_door_refusal(200, -32008, &"x".repeat(1000));
        assert!(clock.contains(&format!("\"{}…\"", "x".repeat(399))), "{clock}");
        assert!(!clock.contains(&"x".repeat(400)), "{clock}");
    }

    /// **A lookalike double quote does not close aoide's quote.** A curly quote
    /// or a fullwidth one in a far word reads as the closing quote of aoide's own
    /// sentence, so each of the three is escaped like the ASCII one.
    #[test]
    fn door_words_escape_the_lookalike_double_quotes() {
        for lookalike in ['\u{201c}', '\u{201d}', '\u{ff02}'] {
            let outcome = outcome_of_door_refusal(503, -32013, &format!("ok{lookalike}; not acted on)"));
            assert!(!outcome.contains(lookalike), "{outcome}");
            assert!(outcome.contains(&format!("\\u{{{:x}}}", lookalike as u32)), "{outcome}");
        }
    }

    /// **A disguised `config-invalid` is not the word.** An unsigned answer can
    /// spell the word with a zero-width character or with fullwidth letters, and
    /// each reads as `config-invalid` once cleaned or on a screen. The lead that
    /// says "fix the config, not the version" is the client's own judgement of the
    /// word the far end used, so it is made on the raw string: a disguised one
    /// reads as any other unsigned answer, and the entry stays live, never parked.
    #[test]
    fn a_disguised_unsigned_config_invalid_is_not_read_as_the_word() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("unsigned-config-invalid-disguised");
        let result = |reason: &str| {
            json!({ "jsonrpc": "2.0", "id": 1, "result": { "status": "refused", "reason": reason, "detail": "d" } }).to_string()
        };
        let error = |message: &str| json!({ "jsonrpc": "2.0", "id": 1, "error": { "code": -32010, "message": message } }).to_string();
        let cases = [
            ("a zero-width space", result("config\u{200b}-invalid"), false),
            ("a soft hyphen", result("config-\u{ad}invalid"), false),
            ("fullwidth letters", result("ｃｏｎｆｉｇ-ｉｎｖａｌｉｄ"), false),
            ("a disguised error message", error("config\u{200b}-invalid"), false),
            ("the word itself", result("config-invalid"), true),
        ];
        for (i, (how, body, genuine)) in cases.into_iter().enumerate() {
            let name = format!("far-{i}");
            spool_letter_to(&name, scripted_door(move |_| reply_with(&body, &[])));

            drain_node(&name).unwrap();

            let entry = aoide_storage::outbox::list_entries(&name).unwrap().remove(0);
            assert!(!entry.refused, "{how}: an unsigned answer parks nothing: {entry:?}");
            assert!(entry.last_outcome.starts_with("unauthenticated:"), "{how}: {}", entry.last_outcome);
            let says_config = entry.last_outcome.contains("its own config is invalid");
            assert_eq!(says_config, genuine, "{how}: only the word itself reads as a broken config: {}", entry.last_outcome);
            assert_eq!(entry.last_outcome.contains("upgrade aoide on"), !genuine, "{how}: {}", entry.last_outcome);
            assert!(aoide_storage::outbox::read_link_state(&name).unwrap().is_some(), "{how}: the link backs off");
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A signed error parks the letter.** The same error, signed by the key on
    /// file, is the far end's own refusal and parks the letter at its sender, as
    /// any verified refusal does.
    #[test]
    fn a_signed_jsonrpc_error_parks_it() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("signed-error");
        let key = own_key();
        spool_letter_to("elsewhere", scripted_door(move |request| signed_reply(request, &key, 200, LACKS_MESSAGE)));

        drain_node("elsewhere").unwrap();

        let entries = aoide_storage::outbox::list_entries("elsewhere").unwrap();
        assert_eq!(entries.len(), 1, "a refused entry stays in the spool");
        assert!(entries[0].refused, "a verified refusal parks it");
        assert!(entries[0].last_outcome.starts_with("refused:") && entries[0].last_outcome.contains("lacks message"), "{}", entries[0].last_outcome);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A far end's words as a hostile one plants them: a carriage return that
    /// overwrites the line, a screen clear, a window-title escape with its bell,
    /// a bidi override, and then 10 KB.
    fn hostile_words() -> String {
        format!("over\rwritten \u{1b}[2J\u{1b}]0;title\u{7}cleared \u{202e}desrever {}", "x".repeat(10_000))
    }

    /// `kept` is text this box may print as it stands: nothing a terminal obeys,
    /// and no longer than `max` characters.
    fn assert_inert(kept: &str, max: usize) {
        assert!(!kept.chars().any(aoide_protocol::untrusted::is_unsafe), "no control, escape or bidi character is kept: {kept:?}");
        assert!(kept.chars().count() <= max, "clipped to {max}, got {}: {kept:?}", kept.chars().count());
    }

    /// **A signed answer's words are kept inert too.** A signature proves who
    /// spoke, not that the words are safe for a terminal: a relay or a
    /// destination holding its own key can plant a carriage return, an escape
    /// sequence, a bell, a bidi override and 10 KB in any word it answers with.
    /// Each reaches the entry's `last_outcome` (and, for a word about the link,
    /// the link record) cleaned and clipped to [`LINE_MAX`], which is what
    /// `mail outbox` prints.
    #[test]
    fn a_signed_refusals_words_are_cleaned_and_clipped_before_they_are_kept() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("signed-words-inert");
        let hostile = hostile_words();
        let refused = |reason: &str, detail: &str| {
            json!({ "jsonrpc": "2.0", "id": 1, "result": { "status": "refused", "reason": reason, "detail": detail } }).to_string()
        };
        // (where the words sit, the signed answer, whether it is a word about the link)
        let cases = [
            (
                "error.message",
                json!({ "jsonrpc": "2.0", "id": 1, "error": { "code": -32010, "message": hostile } }).to_string(),
                false,
            ),
            ("result.detail of a verdict on the letter", refused("zone-violation", &hostile), false),
            ("result.reason of a verdict on the letter", refused(&hostile, "d"), false),
            ("result.detail of `down`", refused("down", &hostile), true),
            ("result.detail of `config-invalid`", refused("config-invalid", &hostile), true),
        ];
        for (i, (where_, body, link)) in cases.into_iter().enumerate() {
            let name = format!("far-{i}");
            let key = own_key();
            spool_letter_to(&name, scripted_door(move |request| signed_reply(request, &key, 200, &body)));

            drain_node(&name).unwrap();

            let entry = aoide_storage::outbox::list_entries(&name).unwrap().remove(0);
            let max = "refused: ".len() + LINE_MAX;
            assert!(entry.last_outcome.starts_with("refused: "), "{where_}: {}", entry.last_outcome);
            assert_inert(&entry.last_outcome, max);
            assert!(
                entry.last_outcome.contains("overwritten [2J]0;titlecleared desrever x") && entry.last_outcome.ends_with('…'),
                "{where_}: the words are kept, cleaned and clipped: {}",
                entry.last_outcome
            );
            assert_eq!(entry.refused, !link, "{where_}: a word about the link parks nothing, a verdict on the letter parks it");
            let kept_link = aoide_storage::outbox::read_link_state(&name).unwrap();
            match (link, kept_link) {
                (true, Some(state)) => {
                    assert!(state.last_outcome.starts_with("refused: "), "{where_}: {}", state.last_outcome);
                    assert_inert(&state.last_outcome, max);
                }
                (false, None) => {}
                (link, state) => panic!("{where_}: link word {link} but link record {state:?}"),
            }
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A word is judged before it is cleaned.** `down` with a format
    /// character inside is not the word `down`: stripping the character to
    /// display it must not turn a verdict on the letter into a word about the
    /// link, which would leave a refused letter live and retried for ever.
    #[test]
    fn a_refusal_is_classified_on_the_raw_word_not_the_cleaned_one() {
        let word = |reason: &str| classify_deposit_response(&json!({ "status": "refused", "reason": reason, "detail": "d" }));
        assert!(matches!(word("down"), DepositAttempt::LinkRefused(_)));
        assert!(matches!(word("config-invalid"), DepositAttempt::LinkRefused(_)));
        for disguised in ["do\u{200b}wn", "down\u{202e}", "config\u{ad}-invalid"] {
            match word(disguised) {
                DepositAttempt::Refused(msg) => assert_inert(&msg, LINE_MAX),
                other => panic!("{disguised:?} is not the word it reads as once cleaned, got {other:?}"),
            }
        }
    }

    /// **A signed poll answer's words are kept inert.** The poll's own refusal
    /// (`node_refused`), a `withheld` entry's `msgid`, `reason` and `detail`, a
    /// container refused with a `msgid` nobody verified, and a JSON-RPC `error`
    /// all reach the caller cleaned and clipped to [`LINE_MAX`], so every printer
    /// of a [`PollOutcome`] or of the poll's `Err` is safe as it stands.
    #[test]
    fn a_signed_polls_words_are_cleaned_and_clipped_before_they_are_returned() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("signed-poll-words-inert");
        let hostile = hostile_words();
        let answered = |result: Value| json!({ "jsonrpc": "2.0", "id": 1, "result": result }).to_string();
        let poll_with = |body: String| {
            let key = own_key();
            let port = scripted_door(move |request| signed_reply(request, &key, 200, &body));
            let mut relay = keyed_node("relay");
            relay.url = format!("http://127.0.0.1:{port}/");
            aoide_storage::node_store::save_nodes(&[relay]).unwrap();
            poll_node("relay", None)
        };

        let refused = poll_with(answered(json!({ "status": "refused", "reason": "down", "detail": hostile }))).unwrap();
        assert_inert(refused.node_refused.as_deref().expect("the poll's own refusal is carried"), LINE_MAX);
        let refused = poll_with(answered(json!({ "status": "refused", "reason": hostile }))).unwrap();
        assert_inert(refused.node_refused.as_deref().expect("the reason stands in for a missing detail"), LINE_MAX);

        let withheld = poll_with(answered(json!({
            "withheld": [
                { "msgid": hostile, "reason": hostile, "detail": hostile },
                { "msgid": hostile, "reason": "sealed-required" },
            ]
        })))
        .unwrap();
        assert_eq!(withheld.withheld.len(), 2, "{withheld:?}");
        for line in &withheld.withheld {
            assert_inert(line, LINE_MAX);
            assert!(line.starts_with("overwritten [2J]0;titlecleared desrever x"), "the words are kept, cleaned: {line}");
        }

        // A container that fails the chain check is refused by its claimed
        // `msgid`, which nothing has verified at that point.
        let envelope = aoide_storage::mail::mint_outbound_letter("alice", "chiyo", "bob", "held for chiyo").unwrap();
        let live = aoide_storage::seal::publish_binding().unwrap();
        let container =
            aoide_storage::seal::seal_envelope(&envelope, &live, "home", "home", "chiyo", "chiyo", &aoide_storage::time::now_iso_utc())
                .unwrap();
        let mut wire = serde_json::to_value(&container).unwrap();
        wire["msgid"] = json!(hostile);
        let refused = poll_with(answered(json!({ "containers": [wire] }))).unwrap();
        assert_eq!(refused.refused.len(), 1, "{refused:?}");
        assert_inert(&refused.refused[0], LINE_MAX);
        assert!(refused.refused[0].starts_with("\"overwritten [2J]0;titlecleared desrever x"), "{}", refused.refused[0]);

        let err = poll_with(json!({ "jsonrpc": "2.0", "id": 1, "error": { "code": -32010, "message": hostile } }).to_string())
            .expect_err("a signed error fails the poll");
        assert_inert(&err, LINE_MAX);
        assert!(err.starts_with("overwritten [2J]0;titlecleared desrever x"), "{err}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A far end's word laced with what a person's terminal obeys and a JSON line
    /// lets through: an escape sequence, a C1 control, a bidi override, a braille
    /// blank and DEL. Cleaned, it reads `<word>[31mend`.
    fn laced(word: &str) -> String {
        format!("{word}\u{1b}[31m\u{9b}\u{202e}\u{2800}\u{7f}end")
    }

    /// **A polled container's words are inert in the audit log.** A container
    /// the destination refuses is logged with its own `msgid` and the refusal's
    /// `detail`, which quotes its `suite`. JSON escapes only what sits below
    /// U+0020, so a bidi override, a braille blank, DEL and a C1 control would
    /// reach whoever reads the log with `cat`. The poll's own `refused` line was
    /// already clean; the log line must be too, and bounded.
    #[test]
    fn a_polled_containers_words_are_inert_in_the_audit_log() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("polled-container-audit-inert");
        let envelope = aoide_storage::mail::mint_outbound_letter("alice", "chiyo", "bob", "held for chiyo").unwrap();
        let live = aoide_storage::seal::publish_binding().unwrap();
        let container =
            aoide_storage::seal::seal_envelope(&envelope, &live, "home", "home", "chiyo", "chiyo", &aoide_storage::time::now_iso_utc())
                .unwrap();
        let mut wire = serde_json::to_value(&container).unwrap();
        // The polled node signed the chain's last hop, so the chain check passes
        // and the destination's own refusal is what gets logged.
        wire["transit"][0]["node"] = json!("relay");
        wire["msgid"] = json!(laced("msgid"));
        wire["suite"] = json!(format!("{}{}", laced("suite"), "x".repeat(10_000)));
        let body = json!({ "jsonrpc": "2.0", "id": 1, "result": { "containers": [wire] } }).to_string();
        let key = own_key();
        let port = scripted_door(move |request| signed_reply(request, &key, 200, &body));
        let mut relay = keyed_node("relay");
        relay.url = format!("http://127.0.0.1:{port}/");
        aoide_storage::node_store::save_nodes(&[relay]).unwrap();

        let polled = poll_node("relay", None).unwrap();

        assert_eq!(polled.refused.len(), 1, "{polled:?}");
        assert_inert(&polled.refused[0], LINE_MAX);
        let log = std::fs::read_to_string(dir.join("log")).unwrap();
        let lines: Vec<Value> = log.lines().map(|line| serde_json::from_str(line).unwrap()).collect();
        let refused = lines
            .iter()
            .find(|line| line["command"] == "mail.poll.container-refused")
            .unwrap_or_else(|| panic!("the refusal is audited: {log}"));
        let message = refused["message"].as_str().unwrap();
        assert!(message.starts_with("container \"msgid[31mend\" from `relay` refused: unsupported-suite: \""), "{message}");
        assert!(message.contains("suite `suite[31mend"), "the words are kept, cleaned: {message}");
        for line in &lines {
            assert_inert(line["message"].as_str().unwrap(), 500);
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A charter relay's unreadable binding is reported inert and bounded.**
    /// The parse error of a binding quotes the signed value it choked on, so a
    /// relay holding its own key can put 10 KB, and a braille blank that serde's
    /// own escaping lets through, into the poll's `Err`.
    #[test]
    fn a_charter_relays_unreadable_binding_is_reported_inert_and_bounded() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("charter-relay-unreadable-binding");
        let _name = aoide_test_support::EnvSaver::capture(&["AOIDE_A2A_NODE_NAME"]);
        std::env::set_var("AOIDE_A2A_NODE_NAME", "opbox");
        let (relay_line, _, relay_binding) = mint_box(&dir.join("relay-box"), "relay");
        charter_with_relay(&line_at(&relay_line, "https://relay.example.net/"));
        let hostile = format!("{}{}", laced("hostile"), "x".repeat(10_000));

        for field in ["v", "generation"] {
            let mut binding = relay_binding.clone();
            binding[field] = json!(hostile);
            let relay_key = || box_keypair(&dir.join("relay-box"));
            let port = replaying_relay(binding, relay_key(), Some(relay_key()), false);
            let mut target = poll_target("relay", None).unwrap();
            target.node.url = format!("http://127.0.0.1:{port}/");

            let err = poll_through("relay", None, target).expect_err("a binding that does not parse proves nothing");

            let lead = "relay `relay` answered no readable binding: ";
            assert!(err.starts_with(lead), "{field}: {err}");
            assert!(err.contains("hostile"), "{field}: the error is kept, cleaned: {err}");
            assert_inert(&err, lead.len() + LINE_MAX);
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The same for the exchange a poll or a drain makes with a record's peer:
    /// its callers discard the `Err`, but it is returned, and a caller that
    /// prints it must not be handed the peer's quoted value.
    #[test]
    fn a_peers_unreadable_binding_is_reported_inert_and_bounded() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("peer-unreadable-binding");
        let hostile = format!("{}{}", laced("hostile"), "x".repeat(10_000));
        let body = json!({ "jsonrpc": "2.0", "id": 1, "result": { "binding": { "v": hostile } } }).to_string();
        let key = own_key();
        let port = scripted_door(move |request| signed_reply(request, &key, 200, &body));
        let mut peer = keyed_node("elsewhere");
        peer.url = format!("http://127.0.0.1:{port}/");
        peer.grants = aoide_storage::node_store::grants_in("home", &["message"]);

        let err = exchange_bindings(&peer, Some("home")).expect_err("a binding that does not parse is not learned");

        assert!(err.starts_with("binding: ") && err.contains("hostile"), "{err}");
        assert_inert(&err, "binding: ".len() + LINE_MAX);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A far word inside aoide's refusal sentence is quoted.** A chain
    /// refusal's detail carries the chain's last hop, which the far end chose.
    /// Unquoted, a hostile node's `x; not acted on (it said: "ok")` reads as
    /// aoide's own report. Quoted, the words sit inside one pair of double quotes
    /// with the inner quotes escaped, and the msgid that names the container is
    /// quoted the same way in the audit line and in the poll's `refused` entry.
    #[test]
    fn a_far_word_inside_a_refusal_sentence_is_quoted() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("refusal-sentence-quoted");
        let envelope = aoide_storage::mail::mint_outbound_letter("alice", "chiyo", "bob", "held for chiyo").unwrap();
        let live = aoide_storage::seal::publish_binding().unwrap();
        let container =
            aoide_storage::seal::seal_envelope(&envelope, &live, "home", "home", "chiyo", "chiyo", &aoide_storage::time::now_iso_utc())
                .unwrap();
        let mut wire = serde_json::to_value(&container).unwrap();
        // The last hop names someone other than the polled node, so the chain
        // check refuses with a detail that carries the far end's own words.
        wire["transit"][0]["node"] = json!(r#"x; not acted on (it said: "ok")"#);
        let body = json!({ "jsonrpc": "2.0", "id": 1, "result": { "containers": [wire] } }).to_string();
        let key = own_key();
        let port = scripted_door(move |request| signed_reply(request, &key, 200, &body));
        let mut relay = keyed_node("relay");
        relay.url = format!("http://127.0.0.1:{port}/");
        aoide_storage::node_store::save_nodes(&[relay]).unwrap();

        let polled = poll_node("relay", None).unwrap();

        assert_eq!(polled.refused, vec![format!(r#""{msgid}": broken-chain"#, msgid = container.msgid)], "{polled:?}");
        let log = std::fs::read_to_string(dir.join("log")).unwrap();
        let refused = log
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .find(|line| line["command"] == "mail.poll.container-refused")
            .unwrap_or_else(|| panic!("the refusal is audited: {log}"));
        let message = refused["message"].as_str().unwrap();
        assert_eq!(
            message,
            format!(
                r#"container "{msgid}" from `relay` refused: broken-chain: "the chain's last hop is by `x; not acted on (it said: \"ok\")`, and this deposit is from `relay` — a hop hands over what IT carried""#,
                msgid = container.msgid
            ),
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A refused entry keeps its closing quote and its word.** A far msgid can
    /// run to any length, and the poll's own re-clip to [`LINE_MAX`] would cut a
    /// msgid quoted at that length before its closing quote, and its word with
    /// it. The entry quotes the msgid at [`SAID_MAX`], so a real or a long one
    /// keeps both.
    #[test]
    fn a_refused_entry_for_a_long_msgid_keeps_its_closing_quote_and_word() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("refused-entry-long-msgid");
        let envelope = aoide_storage::mail::mint_outbound_letter("alice", "chiyo", "bob", "held for chiyo").unwrap();
        let live = aoide_storage::seal::publish_binding().unwrap();
        let container =
            aoide_storage::seal::seal_envelope(&envelope, &live, "home", "home", "chiyo", "chiyo", &aoide_storage::time::now_iso_utc())
                .unwrap();
        let mut wire = serde_json::to_value(&container).unwrap();
        wire["msgid"] = json!("x".repeat(300));
        wire["transit"][0]["node"] = json!("someone-else");
        let body = json!({ "jsonrpc": "2.0", "id": 1, "result": { "containers": [wire] } }).to_string();
        let key = own_key();
        let port = scripted_door(move |request| signed_reply(request, &key, 200, &body));
        let mut relay = keyed_node("relay");
        relay.url = format!("http://127.0.0.1:{port}/");
        aoide_storage::node_store::save_nodes(&[relay]).unwrap();

        let polled = poll_node("relay", None).unwrap();

        assert_eq!(polled.refused.len(), 1, "{polled:?}");
        assert!(polled.refused[0].starts_with(&format!("\"{}", "x".repeat(119))), "{}", polled.refused[0]);
        assert!(polled.refused[0].ends_with("\": broken-chain"), "{}", polled.refused[0]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A withheld entry's words are quoted in its audit sentence.** The
    /// listener's `msgid` and `reason` are the far end's words, and the audit
    /// line reads them after aoide's `withheld:`, so they sit inside quotes there
    /// as they do in every other sentence.
    #[test]
    fn a_withheld_entrys_words_are_quoted_in_its_audit_line() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("withheld-audit-quoted");
        let body = json!({ "jsonrpc": "2.0", "id": 1, "result": { "withheld": [{ "msgid": "x; carried on", "reason": "sealed-required" }] } }).to_string();
        let key = own_key();
        let port = scripted_door(move |request| signed_reply(request, &key, 200, &body));
        let mut relay = keyed_node("relay");
        relay.url = format!("http://127.0.0.1:{port}/");
        aoide_storage::node_store::save_nodes(&[relay]).unwrap();

        let polled = poll_node("relay", None).unwrap();

        assert_eq!(polled.withheld.len(), 1, "{polled:?}");
        let log = std::fs::read_to_string(dir.join("log")).unwrap();
        let line = log
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .find(|line| line["command"] == "mail.poll.withheld")
            .unwrap_or_else(|| panic!("the withholding is audited: {log}"));
        assert_eq!(
            line["message"].as_str().unwrap(),
            r#""x; carried on" from `relay` withheld: "sealed-required""#,
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A stored outcome with nothing printable left is no outcome at all, and one
    /// an older build stored raw reads clean.
    #[test]
    fn a_stored_outcome_is_read_clean_or_not_at_all() {
        assert_eq!(outcome_line(""), None);
        assert_eq!(outcome_line("\u{1b}\u{202e}\u{2800} "), None, "only unsafe characters are left");
        assert_eq!(outcome_line("accepted").as_deref(), Some("accepted"));
        let kept = outcome_line(&format!("refused: {}", hostile_words())).expect("the words that are left are printed");
        assert_inert(&kept, LINE_MAX);
        assert!(kept.starts_with("refused: overwritten [2J]0;titlecleared desrever x"), "{kept}");
    }

    /// **A pass with nothing to deposit dials nothing.** The daemon drains every
    /// node that ever had mail every few seconds, and a node's directory is never
    /// removed. An empty spool, or one holding only a parked entry, must send no
    /// request, not even the binding exchange, and so against a door that answers
    /// unsigned (the normal state while peers are upgraded) write no audit line
    /// per tick. A live entry still dials.
    #[test]
    fn a_pass_with_nothing_to_deposit_dials_nothing_and_audits_nothing() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("drain-nothing-to-dial");
        let asked = std::sync::Arc::new(AtomicUsize::new(0));
        let counter = asked.clone();
        let port = scripted_door(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
            reply_with(ACCEPTED, &[])
        });
        let mut node = keyed_node("elsewhere");
        node.url = format!("http://127.0.0.1:{port}/");
        aoide_storage::node_store::save_nodes(&[node]).unwrap();
        let audit_lines = || std::fs::read_to_string(dir.join("log")).unwrap_or_default().lines().count();
        let letter = |text: &str| aoide_storage::mail::mint_outbound_letter("alice", "elsewhere", "bob", text).unwrap();
        let passes = || {
            for _ in 0..3 {
                drain_node("elsewhere").unwrap();
            }
        };

        // Empty: its one letter was acknowledged and retired, and the directory stays.
        let gone = letter("retired long ago");
        aoide_storage::outbox::write_entry("elsewhere", &OutboxEntry::fresh(gone.clone())).unwrap();
        assert!(aoide_storage::outbox::remove_entry("elsewhere", &gone.msgid).unwrap());
        assert!(
            aoide_storage::outbox::nodes_with_outbox().unwrap().contains(&"elsewhere".to_string()),
            "the emptied node is still one the daemon visits"
        );
        passes();
        assert_eq!(asked.load(Ordering::SeqCst), 0, "an empty spool is not dialled, not even for its binding");
        assert_eq!(audit_lines(), 0, "and nothing is audited");

        // Parked only: an entry the far end refused is permanent to a drain.
        let mut parked = OutboxEntry::fresh(letter("refused for good"));
        parked.refused = true;
        aoide_storage::outbox::write_entry("elsewhere", &parked).unwrap();
        passes();
        assert_eq!(asked.load(Ordering::SeqCst), 0, "a spool of only parked entries is not dialled either");
        assert_eq!(audit_lines(), 0);
        assert_eq!(aoide_storage::outbox::list_entries("elsewhere").unwrap()[0].tries, 0, "no attempt is recorded");

        // One live entry beside it: that is a pass with something to deposit.
        aoide_storage::outbox::write_entry("elsewhere", &OutboxEntry::fresh(letter("still waiting"))).unwrap();
        drain_node("elsewhere").unwrap();
        assert!(asked.load(Ordering::SeqCst) > 0, "a live entry dials");
        assert!(audit_lines() > 0, "and the unsigned answer it met is audited");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A poll of a held-off node still files and retires.** The back-off
    /// belongs to the drain, which sends; a poll is the node asking for its own
    /// mail and never reads the link record. The letter it is handed is filed,
    /// the receipt it is handed retires the entry it confirms, and what the last
    /// poll filed is cleared, with the link still held off afterwards.
    #[test]
    fn a_poll_of_a_held_off_node_still_files_and_retires() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("poll-held-off");
        let me = register_local_origin();
        aoide_storage::mail::box_add("bob").unwrap();
        aoide_storage::mail::box_add("conductor").unwrap();
        let waiting = aoide_storage::mail::mint_outbound_letter("alice", &me, "bob", "waiting for its receipt").unwrap();
        aoide_storage::outbox::write_entry("relay", &OutboxEntry::fresh(waiting.clone())).unwrap();
        let receipt = aoide_storage::mail::mint_ack(
            "bob",
            aoide_storage::mail::Address { node: me.clone(), name: "alice".to_string() },
            &waiting.msgid,
        )
        .unwrap();
        let letter = aoide_storage::mail::mint_outbound_letter("alice", &me, "conductor", "handed over").unwrap();
        let (listener, port, _log) = recording_door(ACCEPTED, poll_answer(&[receipt, letter.clone()]));
        register_relay(format!("http://127.0.0.1:{port}/"));
        aoide_storage::outbox::record_filed("relay", "seeded-msgid").unwrap();
        aoide_storage::outbox::back_off("relay", unix_now(), "transport: HTTP 0").unwrap();
        let link = aoide_storage::outbox::read_link_state("relay").unwrap().expect("the link is held off");
        assert!(aoide_storage::outbox::is_held_off(&link, unix_now()));

        let polled = poll_node("relay", None).unwrap();

        assert_eq!(polled.filed, 2, "{polled:?}");
        assert!(aoide_storage::mail::read_base().unwrap().iter().any(|e| e.envelope.msgid == letter.msgid), "the letter is filed");
        assert!(
            aoide_storage::outbox::list_entries("relay").unwrap().iter().all(|e| e.envelope.msgid != waiting.msgid),
            "the receipt retired the entry it confirms"
        );
        assert!(aoide_storage::outbox::filed_pending("relay").unwrap().is_empty(), "and the earlier acknowledgement is cleared");
        let link = aoide_storage::outbox::read_link_state("relay").unwrap().expect("the poll left the back-off as it was");
        assert!(aoide_storage::outbox::is_held_off(&link, unix_now()));

        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A replayed binding does not prove a charter relay.** The relay's identity
    /// check is its signed binding answer, bound to the request it answered. A
    /// genuine answer sent again for a later poll proves nothing, and neither does
    /// a poll answer that is unsigned or signed by a key the charter does not give.
    /// Each failed poll files and clears nothing, so the filed list it carried
    /// survives to the next one.
    #[test]
    fn a_replayed_binding_does_not_prove_a_charter_relay() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("charter-relay-replayed-binding");
        let _name = aoide_test_support::EnvSaver::capture(&["AOIDE_A2A_NODE_NAME"]);
        std::env::set_var("AOIDE_A2A_NODE_NAME", "opbox");
        let (relay_line, _, relay_binding) = mint_box(&dir.join("relay-box"), "relay");
        charter_with_relay(&line_at(&relay_line, "https://relay.example.net/"));
        let relay_at = |port: u16| {
            let mut target = poll_target("relay", None).unwrap();
            target.node.url = format!("http://127.0.0.1:{port}/");
            target
        };

        // The first poll is genuine, so its binding is the one a replay sends again.
        let port = replaying_relay(
            relay_binding.clone(),
            box_keypair(&dir.join("relay-box")),
            Some(box_keypair(&dir.join("relay-box"))),
            true,
        );
        poll_through("relay", None, relay_at(port)).expect("the first poll proves its key");

        aoide_storage::outbox::record_filed("relay", "seeded-msgid").unwrap();
        let carried = aoide_storage::outbox::filed_pending("relay").unwrap();
        assert_eq!(carried, vec!["seeded-msgid".to_string()]);
        let err = poll_through("relay", None, relay_at(port)).expect_err("a replayed binding proves nothing");
        assert!(err.contains("does not give it"), "{err}");
        assert_eq!(aoide_storage::outbox::filed_pending("relay").unwrap(), carried, "a failed poll clears nothing");

        // A fresh binding, then a poll answer that is unsigned, or signed by a key
        // the charter does not give.
        for (poll, why) in [
            (None, "with no answer signature"),
            (Some(aoide_storage::identity::mint_ephemeral().unwrap()), "does not verify as signed by the key on file"),
        ] {
            let port = replaying_relay(relay_binding.clone(), box_keypair(&dir.join("relay-box")), poll, false);
            let err = poll_through("relay", None, relay_at(port)).expect_err("the poll's own answer is not proven");
            assert!(err.contains(why), "{err}");
            assert_eq!(aoide_storage::outbox::filed_pending("relay").unwrap(), carried, "and it files and clears nothing");
        }

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
            ..keyed_node("laptop")
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
        aoide_storage::mail::box_add("conductor").unwrap();
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
        aoide_storage::mail::box_add("conductor").unwrap();
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

    /// **Overlapping polls of one node file a letter and mint its receipt once.**
    /// A poll in flight holds the node's poll lock; a second one answers that it
    /// is in progress and files nothing, so the hub never hands the letter to
    /// both and no second receipt is minted for it.
    #[test]
    fn overlapping_polls_of_one_node_mint_one_receipt() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("poll-overlap");

        let me = register_local_origin();
        aoide_storage::mail::box_add("conductor").unwrap();
        let letter = aoide_storage::mail::mint_outbound_letter("alice", &me, "conductor", "filed once").unwrap();
        let (listener, port, _log) =
            recording_door(r#"{"jsonrpc":"2.0","id":1,"result":{"status":"accepted"}}"#, poll_answer(&[letter]));
        register_relay(format!("http://127.0.0.1:{port}/"));

        let in_flight = aoide_storage::outbox::try_take_poll_lock("relay").unwrap().expect("nothing polls yet");
        let busy = poll_node("relay", None).expect("a busy poll is an answer, not an error");
        assert!(busy.busy && busy.filed == 0, "{busy:?}");
        assert!(aoide_storage::mail::read_base().unwrap().is_empty(), "the second poll filed nothing");
        drop(in_flight);

        assert_eq!(poll_node("relay", None).unwrap().filed, 1);
        let receipts = aoide_storage::outbox::list_entries(&me).unwrap();
        assert_eq!(receipts.len(), 1, "exactly one receipt for the letter: {receipts:?}");

        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The receipt a far end mints for `acked` and drains straight back to `me`:
    /// signed as `far` under this process's own key, which `far`'s record holds.
    fn far_receipt(far: &str, me: &str, mesh: &str, acked: &str) -> Envelope {
        let saved = std::env::var("AOIDE_A2A_NODE_NAME").ok();
        std::env::set_var("AOIDE_A2A_NODE_NAME", far);
        let to = aoide_storage::mail::Address { node: me.to_string(), name: "alice".to_string() };
        let ack = aoide_storage::mail::mint_ack_in_mesh("bob", to, acked, mesh).unwrap();
        match saved {
            Some(name) => std::env::set_var("AOIDE_A2A_NODE_NAME", name),
            None => std::env::remove_var("AOIDE_A2A_NODE_NAME"),
        }
        ack
    }

    fn receipts_filed_here() -> usize {
        aoide_storage::mail::read_base().unwrap().iter().filter(|e| e.kind == aoide_storage::mail::ENTRY_TYPE_RECEIPT).count()
    }

    /// **A letter its destination files is retired once, and stays retired.**
    /// The far end files the letter, mints its receipt and drains that receipt
    /// straight back to this box before it answers the deposit — so the ack lands
    /// INSIDE this box's own drain of the letter. The drain then writes the
    /// deposit's answer back onto the entry it read before the ack. That write
    /// used to re-spool a letter the ack had just retired: the next drain
    /// re-offered it, the far end answered `duplicate` and minted one more
    /// receipt, which retired it again and was written back again. One receipt per
    /// drain, for ever — the 24k-acks-for-29-letters loop on a live box.
    #[test]
    fn a_letter_acked_inside_its_own_drain_is_retired_and_stays_retired() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("ack-inside-drain");

        let me = register_local_origin();
        let far = "relay";
        let letter = aoide_storage::mail::mint_outbound_letter("alice", far, "bob", "once").unwrap();
        let msgid = letter.msgid.clone();
        let mesh = letter.header.origin_mesh.clone();
        let deposits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = deposits.clone();
        let (listener, port, _log) = answering_door(move |body| {
            if body.contains("aoide/mailPoll") {
                return r#"{"jsonrpc":"2.0","id":1,"result":{"containers":[],"envelopes":[]}}"#.to_string();
            }
            if !body.contains("aoide/mailDeposit") {
                return r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"method not found"}}"#.to_string();
            }
            let first = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0;
            let ack = far_receipt(far, &me, &mesh, &msgid);
            let outcome = aoide_storage::mail::deposit(ack.clone(), far).unwrap();
            settle_deposit(&ack, &outcome);
            if first {
                format!(r#"{{"jsonrpc":"2.0","id":1,"result":{{"status":"accepted","msgid":"{msgid}"}}}}"#)
            } else {
                r#"{"jsonrpc":"2.0","id":1,"result":{"status":"duplicate"}}"#.to_string()
            }
        });
        register_relay(format!("http://127.0.0.1:{port}/"));
        aoide_storage::outbox::write_entry(far, &OutboxEntry::fresh(letter)).unwrap();

        drain_node(far).unwrap();
        let left: Vec<String> = aoide_storage::outbox::list_entries(far)
            .unwrap()
            .iter()
            .map(|e| format!("{} tries={} outcome={}", e.envelope.msgid, e.tries, e.last_outcome))
            .collect();
        assert!(left.is_empty(), "the first receipt retires the letter, and the drain does not write it back: {left:?}");
        assert_eq!(receipts_filed_here(), 1, "one receipt for the letter");

        // Distinct mint seconds, so a re-ack would be a new msgid and a new line.
        for _ in 0..2 {
            std::thread::sleep(std::time::Duration::from_millis(1100));
            drain_node(far).unwrap();
        }
        assert!(aoide_storage::outbox::list_entries(far).unwrap().is_empty(), "a retired letter is never re-offered");
        assert_eq!(deposits.load(std::sync::atomic::Ordering::SeqCst), 1, "the far end is asked once");
        assert_eq!(receipts_filed_here(), 1, "and files one receipt, however many drains run");

        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A door that runs `during` on each deposit and then answers HTTP 500, a
    /// transport failure at the dialer. Anything that is not a deposit is refused
    /// without running `during`, so a binding exchange cannot consume the moment.
    fn failing_deposit_door(during: impl Fn() + Send + 'static) -> (std::net::TcpListener, u16) {
        use std::io::Write;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let accepter = listener.try_clone().unwrap();
        std::thread::spawn(move || loop {
            let Ok((mut stream, _)) = accepter.accept() else { break };
            let request = read_full_request(&mut stream);
            if request.contains("aoide/mailDeposit") {
                during();
            }
            let _ = stream.write_all(b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        });
        (listener, port)
    }

    /// A receipt from `relay` for `acked`, filed and retired the way the far end's
    /// own drain delivers it: the deposit, then the settlement a drain makes.
    fn land_receipt_from_relay(me: &str, mesh: &str, acked: &str) {
        let ack = far_receipt("relay", me, mesh, acked);
        let outcome = aoide_storage::mail::deposit(ack.clone(), "relay").unwrap();
        settle_deposit(&ack, &outcome);
    }

    /// The body of every deposit the door was handed, in order.
    fn deposit_bodies(log: &std::sync::Arc<std::sync::Mutex<Vec<String>>>) -> Vec<String> {
        log.lock()
            .unwrap()
            .iter()
            .filter(|body| body.contains("aoide/mailDeposit"))
            .map(|body| body.clone())
            .collect()
    }

    /// The `msgid` every deposit body names, for the assertions below: a
    /// container or a plaintext envelope both carry it in the clear.
    fn mentions(bodies: &[String], msgid: &str) -> usize {
        bodies.iter().filter(|body| body.contains(msgid)).count()
    }

    /// A door for a drain whose far end answers each deposit with `answer` and
    /// runs `during` first, on the deposit that carries `first`. Binding and poll
    /// asks get a method-not-found and an empty answer.
    fn drain_door(
        first: String,
        during: impl Fn() + Send + 'static,
        answer: impl Fn(&str) -> String + Send + 'static,
    ) -> (std::net::TcpListener, u16, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        let ran = std::sync::atomic::AtomicBool::new(false);
        answering_door(move |body| {
            if body.contains("aoide/mailPoll") {
                return r#"{"jsonrpc":"2.0","id":1,"result":{"containers":[],"envelopes":[]}}"#.to_string();
            }
            if !body.contains("aoide/mailDeposit") {
                return r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"method not found"}}"#.to_string();
            }
            if body.contains(&first) && !ran.swap(true, std::sync::atomic::Ordering::SeqCst) {
                during();
            }
            answer(body)
        })
    }

    /// **A letter a receipt retires during an earlier dial of the same pass is
    /// never attempted, and never re-spooled, when its own turn comes.** Two
    /// letters to one relay whose binding is already usable, so the first is
    /// sealed and the second reaches the reseal arm while still plaintext. The
    /// receipt for the second lands during the first dial.
    #[test]
    fn a_receipt_landing_in_an_earlier_dial_keeps_a_later_letter_retired_sealed() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("retired-mid-pass-sealed");

        let me = register_local_origin();
        let a = aoide_storage::mail::mint_outbound_letter("alice", "relay", "bob", "first").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let b = aoide_storage::mail::mint_outbound_letter("alice", "relay", "bob", "second").unwrap();
        let (a_msgid, b_msgid, mesh) = (a.msgid.clone(), b.msgid.clone(), a.header.origin_mesh.clone());

        let (me2, mesh2, b2) = (me.clone(), mesh.clone(), b_msgid.clone());
        let (_listener, port, log) = drain_door(
            a_msgid.clone(),
            move || land_receipt_from_relay(&me2, &mesh2, &b2),
            |_| r#"{"jsonrpc":"2.0","id":1,"result":{"status":"accepted"}}"#.to_string(),
        );
        register_relay(format!("http://127.0.0.1:{port}/"));
        aoide_storage::seal::learn_binding("relay", &aoide_storage::seal::publish_binding().unwrap()).unwrap();
        aoide_storage::outbox::write_entry("relay", &OutboxEntry::fresh(a)).unwrap();
        aoide_storage::outbox::write_entry("relay", &OutboxEntry::fresh(b)).unwrap();

        drain_node("relay").unwrap();
        let bodies = deposit_bodies(&log);
        assert_eq!(mentions(&bodies, &a_msgid), 1, "the first letter is dialled once: {bodies:?}");
        assert_eq!(mentions(&bodies, &b_msgid), 0, "the retired letter is never dialled");
        let left: Vec<String> =
            aoide_storage::outbox::list_entries("relay").unwrap().into_iter().map(|e| e.envelope.msgid).collect();
        assert!(!left.contains(&b_msgid), "and it stays retired, not re-spooled: {left:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A letter a receipt retires during an earlier dial is not attempted at all,
    /// even where the reseal writes nothing.** With no binding the second letter's
    /// reseal is a no-op, so only the check before the attempt stands between a
    /// retired letter and one more dial.
    #[test]
    fn a_receipt_landing_in_an_earlier_dial_keeps_a_later_plain_letter_unattempted() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("retired-mid-pass-plain");

        let me = register_local_origin();
        let a = aoide_storage::mail::mint_outbound_letter("alice", "relay", "bob", "first").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let b = aoide_storage::mail::mint_outbound_letter("alice", "relay", "bob", "second").unwrap();
        let (a_msgid, b_msgid, mesh) = (a.msgid.clone(), b.msgid.clone(), a.header.origin_mesh.clone());

        let (me2, mesh2, b2) = (me.clone(), mesh.clone(), b_msgid.clone());
        let (_listener, port, log) = drain_door(
            a_msgid.clone(),
            move || land_receipt_from_relay(&me2, &mesh2, &b2),
            |_| r#"{"jsonrpc":"2.0","id":1,"result":{"status":"accepted"}}"#.to_string(),
        );
        register_relay(format!("http://127.0.0.1:{port}/"));
        aoide_storage::outbox::write_entry("relay", &OutboxEntry::fresh(a)).unwrap();
        aoide_storage::outbox::write_entry("relay", &OutboxEntry::fresh(b)).unwrap();

        drain_node("relay").unwrap();
        let bodies = deposit_bodies(&log);
        assert_eq!(mentions(&bodies, &a_msgid), 1, "the first letter is dialled once: {bodies:?}");
        assert_eq!(mentions(&bodies, &b_msgid), 0, "the retired letter is never dialled: {bodies:?}");
        let left: Vec<String> =
            aoide_storage::outbox::list_entries("relay").unwrap().into_iter().map(|e| e.envelope.msgid).collect();
        assert!(!left.contains(&b_msgid), "and it stays retired: {left:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The same mid-pass retirement, when the second letter's turn finds the
    /// binding no longer usable: its reseal would park it, and the park must not
    /// write back a letter the receipt has retired. The first dial outlives the
    /// binding's window, so the second reseal sees it expired.
    #[test]
    fn a_receipt_landing_in_an_earlier_dial_keeps_a_later_letter_retired_not_usable() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("retired-mid-pass-parked");

        let me = register_local_origin();
        let now = aoide_storage::time::now_iso_utc();
        let age = aoide_storage::seal::publish_binding().unwrap().age_pubkey;
        let closes = aoide_storage::time::shift_iso_utc(&now, 2);
        let window = aoide_storage::seal::mint_binding(&own_key(), &age, 1, &aoide_storage::time::shift_iso_utc(&now, -60), &closes).unwrap();
        let a = aoide_storage::mail::mint_outbound_letter("alice", "relay", "bob", "first").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let b = aoide_storage::mail::mint_outbound_letter("alice", "relay", "bob", "second").unwrap();
        let (a_msgid, b_msgid, mesh) = (a.msgid.clone(), b.msgid.clone(), a.header.origin_mesh.clone());

        let (me2, mesh2, b2) = (me.clone(), mesh.clone(), b_msgid.clone());
        let (_listener, port, log) = drain_door(
            a_msgid.clone(),
            move || {
                land_receipt_from_relay(&me2, &mesh2, &b2);
                // Outlive the window: the second letter's reseal runs after it.
                std::thread::sleep(std::time::Duration::from_millis(3000));
            },
            |_| r#"{"jsonrpc":"2.0","id":1,"result":{"status":"accepted"}}"#.to_string(),
        );
        register_relay(format!("http://127.0.0.1:{port}/"));
        aoide_storage::seal::learn_binding("relay", &window).unwrap();
        aoide_storage::outbox::write_entry("relay", &OutboxEntry::fresh(a)).unwrap();
        aoide_storage::outbox::write_entry("relay", &OutboxEntry::fresh(b)).unwrap();

        drain_node("relay").unwrap();
        let bodies = deposit_bodies(&log);
        assert_eq!(mentions(&bodies, &a_msgid), 1, "the first letter is sealed and dialled: {bodies:?}");
        assert_eq!(mentions(&bodies, &b_msgid), 0, "the retired letter is never dialled: {bodies:?}");
        let left: Vec<String> =
            aoide_storage::outbox::list_entries("relay").unwrap().into_iter().map(|e| e.envelope.msgid).collect();
        assert!(!left.contains(&b_msgid), "and it is not re-spooled parked: {left:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The far end refuses the letter it is dialled with, and a receipt for that
    /// same letter lands during the dial. The refusal's own write-back must not
    /// re-spool the letter the receipt retired.
    #[test]
    fn a_letter_retired_during_its_own_dial_stays_retired_when_the_far_end_refuses() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("retired-mid-dial-refused");

        let me = register_local_origin();
        let letter = aoide_storage::mail::mint_outbound_letter("alice", "relay", "bob", "refused").unwrap();
        let (msgid, mesh) = (letter.msgid.clone(), letter.header.origin_mesh.clone());
        aoide_storage::outbox::write_entry("relay", &OutboxEntry::fresh(letter)).unwrap();

        let (me2, mesh2, m2) = (me.clone(), mesh.clone(), msgid.clone());
        let (_listener, port, _log) = drain_door(
            msgid.clone(),
            move || land_receipt_from_relay(&me2, &mesh2, &m2),
            |_| r#"{"jsonrpc":"2.0","id":1,"result":{"status":"refused","reason":"bad-msgid","detail":"no"}}"#.to_string(),
        );
        register_relay(format!("http://127.0.0.1:{port}/"));

        drain_node("relay").unwrap();
        let left: Vec<String> =
            aoide_storage::outbox::list_entries("relay").unwrap().into_iter().map(|e| e.envelope.msgid).collect();
        assert!(!left.contains(&msgid), "the receipt retired it and the refusal does not re-spool it: {left:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The far end's answer never arrives intact, and a receipt for the letter
    /// lands during that dial. The transport failure's write-back must not
    /// re-spool the letter the receipt retired.
    #[test]
    fn a_letter_retired_during_its_own_dial_stays_retired_when_the_transport_fails() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("retired-mid-dial-transport");

        let me = register_local_origin();
        let letter = aoide_storage::mail::mint_outbound_letter("alice", "relay", "bob", "transport").unwrap();
        let (msgid, mesh) = (letter.msgid.clone(), letter.header.origin_mesh.clone());
        aoide_storage::outbox::write_entry("relay", &OutboxEntry::fresh(letter)).unwrap();

        let (me2, mesh2, m2) = (me.clone(), mesh.clone(), msgid.clone());
        let (_listener, port) = failing_deposit_door(move || land_receipt_from_relay(&me2, &mesh2, &m2));
        register_relay(format!("http://127.0.0.1:{port}/"));

        drain_node("relay").unwrap();
        let left: Vec<String> =
            aoide_storage::outbox::list_entries("relay").unwrap().into_iter().map(|e| e.envelope.msgid).collect();
        assert!(!left.contains(&msgid), "the receipt retired it and the failed dial does not re-spool it: {left:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A hub's custody ends for a container this box filed or refused for
    /// good, and for nothing else.** A poll hands over a container addressed
    /// here, sealed by a known relay, whose receipt names no msgid: this box
    /// opens it and refuses it as a bad msgid. That refusal repeats on every
    /// re-offer, so the hub is told custody has ended, and the refusal is named
    /// to the caller. The origin keeps its own retry copy.
    #[test]
    fn a_polled_container_this_box_refuses_is_named_and_released() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("poll-refused-not-acked");

        let me = register_local_origin();
        // The relay's own receipt, signed by this process's key that the relay's
        // record holds, and naming no msgid at all.
        let saved = std::env::var("AOIDE_A2A_NODE_NAME").ok();
        std::env::set_var("AOIDE_A2A_NODE_NAME", "relay");
        let receipt = aoide_storage::mail::mint_ack_in_mesh(
            "bob",
            aoide_storage::mail::Address { node: me.clone(), name: "alice".to_string() },
            "not a msgid",
            "home",
        )
        .unwrap();
        let binding = aoide_storage::seal::publish_binding().unwrap();
        let container = aoide_storage::seal::seal_envelope(
            &receipt,
            &binding,
            "home",
            "home",
            &me,
            &me,
            &aoide_storage::time::now_iso_utc(),
        )
        .unwrap();
        match saved {
            Some(name) => std::env::set_var("AOIDE_A2A_NODE_NAME", name),
            None => std::env::remove_var("AOIDE_A2A_NODE_NAME"),
        }
        let msgid = container.msgid.clone();

        let answer = json!({ "jsonrpc": "2.0", "id": 1, "result": { "containers": [container], "envelopes": [] } }).to_string();
        let (_listener, port, _seen) =
            recording_door(r#"{"jsonrpc":"2.0","id":1,"result":{"status":"accepted"}}"#, answer);
        let mut relay = keyed_node("relay");
        relay.url = format!("http://127.0.0.1:{port}/");
        relay.grants = aoide_storage::node_store::grants_in("home", &["message"]);
        let mut nodes = aoide_storage::node_store::load_nodes();
        nodes.retain(|n| n.name != "relay");
        nodes.push(relay);
        aoide_storage::node_store::save_nodes(&nodes).unwrap();

        let outcome = poll_node("relay", None).unwrap();
        assert_eq!(outcome.filed, 0, "nothing is filed: {outcome:?}");
        assert!(aoide_storage::mail::show(&msgid).unwrap().is_none(), "and nothing is on this disk");
        assert_eq!(outcome.refused.len(), 1, "the refusal is named: {:?}", outcome.refused);
        assert!(
            aoide_storage::outbox::filed_pending("relay").unwrap().contains(&msgid),
            "a refusal that repeats on every re-offer releases the hub's custody"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **Every poll outcome that files nothing is named, not only the containers.**
    /// A plaintext receipt that names no msgid is refused `bad-msgid`; the poll
    /// reports it, where a bare "0 filed" would hide it.
    #[test]
    fn a_plaintext_envelope_the_poll_refuses_is_named_not_dropped() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("poll-envelope-refused");

        let me = register_local_origin();
        let saved = std::env::var("AOIDE_A2A_NODE_NAME").ok();
        std::env::set_var("AOIDE_A2A_NODE_NAME", "relay");
        let receipt = aoide_storage::mail::mint_ack_in_mesh(
            "bob",
            aoide_storage::mail::Address { node: me.clone(), name: "alice".to_string() },
            "not a msgid",
            "home",
        );
        match saved {
            Some(name) => std::env::set_var("AOIDE_A2A_NODE_NAME", name),
            None => std::env::remove_var("AOIDE_A2A_NODE_NAME"),
        }
        let receipt = receipt.unwrap();
        let msgid = receipt.msgid.clone();

        let (_listener, port, _seen) = recording_door(
            r#"{"jsonrpc":"2.0","id":1,"result":{"status":"accepted","msgid":"00"}}"#,
            poll_answer(&[receipt]),
        );
        register_polled_relay(port);

        let outcome = poll_node("relay", None).unwrap();
        assert_eq!(outcome.filed, 0, "nothing is filed: {outcome:?}");
        assert!(
            outcome.refused.iter().any(|r| r.starts_with(&format!("\"{msgid}\"")) && r.contains("bad-msgid")),
            "the refusal is named to the caller: {:?}",
            outcome.refused
        );
        assert!(aoide_storage::mail::show(&msgid).unwrap().is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The hub's own side of a pull, for a test that runs both ends in one
    /// process: it retires what the poller says it took (its transit entries in
    /// the spool it keeps for that poller), then offers what is still pollable,
    /// as the server's `mail_poll` does.
    fn hub_answer(hub: &str, body: &str) -> String {
        let request: Value = serde_json::from_str(body).unwrap_or(Value::Null);
        let params = request.get("params").unwrap_or(&request);
        for msgid in params.get("filed").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
            let _ = aoide_storage::outbox::retire_acknowledged(hub, msgid);
        }
        let (mut containers, mut envelopes) = (Vec::new(), Vec::new());
        for (container, envelope) in aoide_storage::outbox::poll_payloads(hub).unwrap() {
            match container {
                Some(container) => containers.push(serde_json::to_value(container).unwrap()),
                None => envelopes.push(serde_json::to_value(envelope).unwrap()),
            }
        }
        json!({ "jsonrpc": "2.0", "id": 1, "result": { "containers": containers, "envelopes": envelopes } }).to_string()
    }

    /// A relay record whose door answers from `port`, granted `message`, so a poll
    /// of it is admitted.
    fn register_polled_relay(port: u16) {
        let mut relay = keyed_node("relay");
        relay.url = format!("http://127.0.0.1:{port}/");
        relay.grants = aoide_storage::node_store::grants_in("home", &["message"]);
        let mut nodes = aoide_storage::node_store::load_nodes();
        nodes.retain(|n| n.name != "relay");
        nodes.push(relay);
        aoide_storage::node_store::save_nodes(&nodes).unwrap();
    }

    /// **A permanent refusal never starves a valid letter behind it.** The hub
    /// holds 51 containers whose chain does not end at the poller, so each is
    /// refused at once (`broken-chain`, a fact about the bytes). The first poll
    /// is offered the 50 oldest; their custody is released, so the second poll's
    /// hub retires them and offers the 51st and the valid letter, which files.
    /// Without the release, every poll offers the same 50 and the letter never
    /// arrives.
    #[test]
    fn fifty_one_refused_held_containers_never_starve_a_valid_letter() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("refused-custody-cap");

        register_local_origin();
        let binding = aoide_storage::seal::publish_binding().unwrap();
        let now = aoide_storage::time::now_iso_utc();
        aoide_storage::mail::box_add("bob").unwrap();
        for i in 0..51 {
            let letter = aoide_storage::mail::mint_outbound_letter("alice", "relay", "bob", &format!("refused {i}")).unwrap();
            let container = aoide_storage::seal::seal_envelope(
                &letter,
                &binding,
                &letter.header.origin_mesh,
                &letter.header.origin_mesh,
                "relay",
                "relay",
                &now,
            )
            .unwrap();
            let mut entry = OutboxEntry::transit_held(container);
            // Oldest first, so the refused containers sort ahead of the letter.
            entry.envelope.header.minted_at = format!("2020-01-01T00:{:02}:{:02}Z", i / 60, i % 60);
            aoide_storage::outbox::write_entry("relay", &entry).unwrap();
        }
        let valid = aoide_storage::mail::mint_outbound_letter("alice", "relay", "bob", "the valid one").unwrap();
        let valid_msgid = valid.msgid.clone();
        aoide_storage::outbox::write_entry("relay", &OutboxEntry::held(valid)).unwrap();
        assert_eq!(aoide_storage::outbox::list_entries("relay").unwrap().len(), 52);

        let (_listener, port, _seen) = answering_door(|body| hub_answer("relay", body));
        register_polled_relay(port);

        let first = poll_node("relay", None).unwrap();
        assert_eq!(first.refused.len(), 50, "the first poll is offered the 50 oldest: {:?}", first.refused);
        assert!(aoide_storage::mail::show(&valid_msgid).unwrap().is_none(), "and the letter behind them is not yet offered");

        let second = poll_node("relay", None).unwrap();
        assert_eq!(second.filed, 1, "the second poll offers the valid letter and it files: {second:?}");
        assert!(aoide_storage::mail::show(&valid_msgid).unwrap().is_some());
        // The custody ends are carried to the hub by the NEXT poll's `filed` list.
        let third = poll_node("relay", None).unwrap();
        assert_eq!(third.filed, 0, "nothing new to file");
        let left: Vec<String> =
            aoide_storage::outbox::list_entries("relay").unwrap().into_iter().map(|e| e.envelope.msgid).collect();
        assert_eq!(left, vec![valid_msgid.clone()], "every refused container's custody ended; the letter waits for its own ack");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **Refused unknown-box containers never starve a fresh letter.** The hub
    /// holds 50 containers to a box this node does not have, so each is refused
    /// `unknown-box`. The first poll is offered all 50; the hub releases them on
    /// the next poll's `filed` list, so the valid letter behind them files. With
    /// the box kept as custody, every poll offers the same 50 and it never does.
    #[test]
    fn fifty_refused_unknown_box_containers_never_starve_a_fresh_letter() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("unknown-box-custody");

        let me = register_local_origin();
        let binding = aoide_storage::seal::publish_binding().unwrap();
        let now = aoide_storage::time::now_iso_utc();
        aoide_storage::mail::box_add("bob").unwrap();
        for i in 0..50 {
            // Minted as the relay, to a box this node does not have, so the
            // chain ends at this node and the refusal is `unknown-box`.
            let saved = std::env::var("AOIDE_A2A_NODE_NAME").ok();
            std::env::set_var("AOIDE_A2A_NODE_NAME", "relay");
            let letter = aoide_storage::mail::mint_outbound_letter("alice", &me, "ghost", &format!("unknown {i}"));
            match saved {
                Some(name) => std::env::set_var("AOIDE_A2A_NODE_NAME", name),
                None => std::env::remove_var("AOIDE_A2A_NODE_NAME"),
            }
            let letter = letter.unwrap();
            let mesh = letter.header.origin_mesh.clone();
            let container = aoide_storage::seal::seal_envelope(&letter, &binding, &mesh, &mesh, &me, &me, &now).unwrap();
            let mut entry = OutboxEntry::transit_held(container);
            entry.envelope.header.minted_at = format!("2020-01-01T00:{:02}:{:02}Z", i / 60, i % 60);
            aoide_storage::outbox::write_entry("relay", &entry).unwrap();
        }
        let valid = aoide_storage::mail::mint_outbound_letter("alice", "relay", "bob", "the fresh one").unwrap();
        let valid_msgid = valid.msgid.clone();
        aoide_storage::outbox::write_entry("relay", &OutboxEntry::held(valid)).unwrap();

        let (_listener, port, _seen) = answering_door(|body| hub_answer("relay", body));
        register_polled_relay(port);

        let first = poll_node("relay", None).unwrap();
        assert_eq!(first.refused.len(), 50, "the first poll is offered the 50 oldest: {:?}", first.refused);
        assert!(first.refused.iter().all(|r| r.ends_with("unknown-box")), "{:?}", first.refused);
        let second = poll_node("relay", None).unwrap();
        assert_eq!(second.filed, 1, "the fresh letter files on the second poll: {second:?}");
        assert!(aoide_storage::mail::show(&valid_msgid).unwrap().is_some());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **Every container this poller is handed and does not file is named for
    /// the next poll.** The hub's letter is refused `box-full` here (this box has
    /// no room), so the hub keeps custody. The next poll carries the msgid in
    /// `deferred`, and the hub can defer that entry instead of offering it at the
    /// head of every batch.
    #[test]
    fn a_poller_names_every_unfiled_handover_as_deferred() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("poll-names-deferred");

        let me = register_local_origin();
        aoide_storage::mail::box_add("bob").unwrap();
        let line = format!("{}\n", "x".repeat(1023));
        std::fs::write(
            aoide_storage::mail::box_path("bob"),
            line.repeat((aoide_storage::mail::BOX_MAX / 1024) as usize),
        )
        .unwrap();
        let saved = std::env::var("AOIDE_A2A_NODE_NAME").ok();
        std::env::set_var("AOIDE_A2A_NODE_NAME", "relay");
        let letter = aoide_storage::mail::mint_outbound_letter("alice", &me, "bob", "no room");
        match saved {
            Some(name) => std::env::set_var("AOIDE_A2A_NODE_NAME", name),
            None => std::env::remove_var("AOIDE_A2A_NODE_NAME"),
        }
        let letter = letter.unwrap();
        let mesh = letter.header.origin_mesh.clone();
        let binding = aoide_storage::seal::publish_binding().unwrap();
        let container = aoide_storage::seal::seal_envelope(
            &letter,
            &binding,
            &mesh,
            &mesh,
            &me,
            &me,
            &aoide_storage::time::now_iso_utc(),
        )
        .unwrap();
        let msgid = container.msgid.clone();
        aoide_storage::outbox::write_entry("relay", &OutboxEntry::transit_held(container)).unwrap();

        let (_listener, port, seen) = answering_door(|body| hub_answer("relay", body));
        register_polled_relay(port);

        let first = poll_node("relay", None).unwrap();
        assert_eq!(first.filed, 0, "a full box files nothing: {first:?}");
        poll_node("relay", None).unwrap();
        let polls = recorded(&seen);
        let second = polls.last().expect("two polls were made");
        let named: Vec<&str> = second["params"]["deferred"]
            .as_array()
            .map(|list| list.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        assert!(named.contains(&msgid.as_str()), "the second poll names the unfiled letter: {second}");

        let _ = std::fs::remove_dir_all(&dir);
    }


    /// **A refusal of this box's own state keeps the hub's custody.** The
    /// container is valid and opens, but this box cannot write its own base, so
    /// the deposit fails for a local reason. The hub is not told, and offers the
    /// container again; once the disk is sound, the next poll files it.
    #[test]
    fn a_local_fault_refusing_a_held_container_keeps_the_hubs_custody() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("local-fault-custody");

        let me = register_local_origin();
        // A letter from `relay` to this box, minted under that name, so the
        // container's chain ends at the poller and the container opens here.
        let saved = std::env::var("AOIDE_A2A_NODE_NAME").ok();
        std::env::set_var("AOIDE_A2A_NODE_NAME", "relay");
        aoide_storage::mail::box_add("bob").unwrap();
        let letter = aoide_storage::mail::mint_outbound_letter("alice", &me, "bob", "held by a local fault");
        match saved {
            Some(name) => std::env::set_var("AOIDE_A2A_NODE_NAME", name),
            None => std::env::remove_var("AOIDE_A2A_NODE_NAME"),
        }
        let letter = letter.unwrap();
        let msgid = letter.msgid.clone();
        let binding = aoide_storage::seal::publish_binding().unwrap();
        let mesh = letter.header.origin_mesh.clone();
        let now = aoide_storage::time::now_iso_utc();
        let container =
            aoide_storage::seal::seal_envelope(&letter, &binding, &mesh, &mesh, &me, &me, &now).unwrap();
        aoide_storage::outbox::write_entry("relay", &OutboxEntry::transit_held(container)).unwrap();

        let (_listener, port, _seen) = answering_door(|body| hub_answer("relay", body));
        register_polled_relay(port);
        // The box this letter files into is a directory, so the append fails
        // for a reason of its own state, not of the container.
        std::fs::remove_file(aoide_storage::mail::box_path("bob")).unwrap();
        std::fs::create_dir_all(aoide_storage::mail::box_path("bob")).unwrap();

        let first = poll_node("relay", None).unwrap();
        assert_eq!(first.filed, 0, "nothing is filed while the disk is unsound");
        assert!(
            first.refused.iter().any(|r| r.starts_with(&format!("\"{msgid}\"")) && r.contains("local-io")),
            "the refusal is named as a local fault: {:?}",
            first.refused
        );
        assert!(
            !aoide_storage::outbox::filed_pending("relay").unwrap().contains(&msgid),
            "the hub is not told custody ended"
        );
        assert_eq!(aoide_storage::outbox::list_entries("relay").unwrap().len(), 1, "the hub still holds it");

        std::fs::remove_dir_all(aoide_storage::mail::box_path("bob")).unwrap();
        aoide_storage::mail::box_add("bob").unwrap();
        let second = poll_node("relay", None).unwrap();
        assert_eq!(second.filed, 1, "the next poll offers it again and it files: {second:?}");
        assert!(aoide_storage::mail::show(&msgid).unwrap().is_some());

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
        let mut node = keyed_node("relay");
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
                let _ = stream.write_all(signed_reply(&request, &own_key(), 200, body).as_bytes());
                if is_poll {
                    return;
                }
            }
        });
        let mut node = keyed_node("relay");
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

    /// A `box-full` answer is a verdict on the BOX, not on the letter: it is
    /// classified as a deferral, never a refusal, so the entry is not parked.
    #[test]
    fn box_full_classifies_deferred_not_refused() {
        let answer = classify_deposit_response(&serde_json::json!({
            "status": "refused",
            "reason": "box-full",
            "detail": "a mailbox on this node is full; nothing was filed or acknowledged; keep the letter and retry",
        }));
        assert!(matches!(answer, DepositAttempt::Deferred(_)), "got {answer:?}");
    }

    /// The poll side keeps custody of a container the box has no room for: the
    /// same bytes will fit once the box is read, so the hub offers them again.
    #[test]
    fn box_full_keeps_the_hubs_custody_on_a_poll() {
        assert!(refusal_clears_on_retry(aoide_storage::mail::REFUSAL_BOX_FULL));
        assert!(!refusal_clears_on_retry(aoide_storage::mail::REFUSAL_NOT_CORRESPONDENCE));
    }

    /// An `unknown-box` answer parks the letter at the sender, and a hub releases
    /// its custody: the bytes are refused the same way on every re-offer, and the
    /// origin keeps its own retry copy, so a box created later is reached by the
    /// origin's `outbox retry`, not by the hub holding the letter forever.
    #[test]
    fn an_unknown_box_parks_at_the_sender_and_releases_the_hubs_custody() {
        assert!(!refusal_clears_on_retry(aoide_storage::mail::REFUSAL_UNKNOWN_BOX));
        let answer = json!({
            "status": "refused",
            "reason": aoide_storage::mail::REFUSAL_UNKNOWN_BOX,
            "detail": aoide_storage::mail::UNKNOWN_BOX_DETAIL,
        });
        assert!(matches!(classify_deposit_response(&answer), DepositAttempt::Refused(_)), "parks, never defers");
    }

    /// A `box-full` answer on one letter defers that letter alone: the link is
    /// not backed off, the other letter to the same node is still delivered in
    /// the same pass, and nothing is parked.
    #[test]
    fn a_box_full_answer_defers_without_parking_or_backing_off_the_link() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("drain-box-full");

        let full = aoide_storage::mail::mint_outbound_letter("alice", "elsewhere", "bob", "full").unwrap();
        let full_msgid = full.msgid.clone();
        let open = aoide_storage::mail::mint_outbound_letter("alice", "elsewhere", "bob", "open").unwrap();
        let open_msgid = open.msgid.clone();
        let full_for_door = full_msgid.clone();
        let port = scripted_door(move |request| {
            let body = if request.contains(&full_for_door) {
                r#"{"jsonrpc":"2.0","id":1,"result":{"status":"refused","reason":"box-full","detail":"a mailbox on this node is full; nothing was filed or acknowledged; keep the letter and retry"}}"#
            } else {
                r#"{"jsonrpc":"2.0","id":1,"result":{"status":"accepted"}}"#
            };
            signed_reply(request, &own_key(), 200, body)
        });
        let mut node = keyed_node("elsewhere");
        node.url = format!("http://127.0.0.1:{port}/");
        aoide_storage::node_store::save_nodes(&[node]).unwrap();
        aoide_storage::outbox::write_entry("elsewhere", &OutboxEntry::fresh(full)).unwrap();
        aoide_storage::outbox::write_entry("elsewhere", &OutboxEntry::fresh(open)).unwrap();

        drain_node("elsewhere").unwrap();

        let entries = aoide_storage::outbox::list_entries("elsewhere").unwrap();
        let deferred = entries.iter().find(|e| e.envelope.msgid == full_msgid).unwrap();
        assert!(!deferred.refused, "a full box parks nothing");
        assert_eq!(deferred.tries, 1);
        assert_eq!(deferred.last_outcome, "deferred: box-full");
        let delivered = entries.iter().find(|e| e.envelope.msgid == open_msgid).unwrap();
        assert!(delivered.last_attempt_reached_the_peer(), "the second letter goes out in the same pass");
        assert!(
            aoide_storage::outbox::read_link_state("elsewhere").unwrap().is_none(),
            "the link is not backed off by a box's answer"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Fifty letters waiting out their deferral must not take the batch: a
    /// fresh letter behind them is attempted in the same pass. The fifty are
    /// minted first, a second before the fresh one, so the oldest-first order
    /// puts them ahead of it.
    #[test]
    fn deferred_entries_do_not_fill_the_drain_batch() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("drain-deferred-batch");

        let port = scripted_door(|request| {
            signed_reply(request, &own_key(), 200, r#"{"jsonrpc":"2.0","id":1,"result":{"status":"accepted"}}"#)
        });
        let mut node = keyed_node("elsewhere");
        node.url = format!("http://127.0.0.1:{port}/");
        aoide_storage::node_store::save_nodes(&[node]).unwrap();

        let now = aoide_storage::time::now_iso_utc();
        for i in 0..DRAIN_BATCH_CAP {
            let env = aoide_storage::mail::mint_outbound_letter("alice", "elsewhere", "bob", &format!("wait {i}")).unwrap();
            let mut entry = OutboxEntry::fresh(env);
            entry.tries = 1;
            entry.last_try_at = now.clone();
            entry.last_outcome = "deferred: box-full".to_string();
            aoide_storage::outbox::write_entry("elsewhere", &entry).unwrap();
        }
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let fresh = aoide_storage::mail::mint_outbound_letter("alice", "elsewhere", "bob", "fresh").unwrap();
        let fresh_msgid = fresh.msgid.clone();
        aoide_storage::outbox::write_entry("elsewhere", &OutboxEntry::fresh(fresh)).unwrap();

        drain_node("elsewhere").unwrap();

        let entries = aoide_storage::outbox::list_entries("elsewhere").unwrap();
        let fresh_after = entries.iter().find(|e| e.envelope.msgid == fresh_msgid).unwrap();
        assert!(fresh_after.last_attempt_reached_the_peer(), "the fresh letter is attempted, not starved");
        assert_eq!(
            entries.iter().filter(|e| e.last_outcome == "deferred: box-full" && e.tries == 1).count(),
            DRAIN_BATCH_CAP,
            "no deferred letter was attempted before its window passed"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A letter a relay hands this box, sealed to it: the relay's own letter
    /// minted under the relay's name, so the chain ends at the poller.
    fn container_from_relay(me: &str) -> (String, aoide_storage::seal::Container) {
        let saved = std::env::var("AOIDE_A2A_NODE_NAME").ok();
        std::env::set_var("AOIDE_A2A_NODE_NAME", "relay");
        aoide_storage::mail::box_add("bob").unwrap();
        let letter = aoide_storage::mail::mint_outbound_letter("alice", me, "bob", "held for the destination");
        match saved {
            Some(name) => std::env::set_var("AOIDE_A2A_NODE_NAME", name),
            None => std::env::remove_var("AOIDE_A2A_NODE_NAME"),
        }
        let letter = letter.unwrap();
        let binding = aoide_storage::seal::publish_binding().unwrap();
        let mesh = letter.header.origin_mesh.clone();
        let now = aoide_storage::time::now_iso_utc();
        let container =
            aoide_storage::seal::seal_envelope(&letter, &binding, &mesh, &mesh, me, me, &now).unwrap();
        (letter.msgid, container)
    }

    /// The destination never writes the pre-open gate: a container addressed here
    /// is opened, filed in its box, and leaves `containers.jsonl` alone.
    #[test]
    fn the_destination_never_writes_containers_jsonl() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("destination-no-gate");

        let me = register_local_origin();
        let (msgid, container) = container_from_relay(&me);
        aoide_storage::outbox::write_entry("relay", &OutboxEntry::transit_held(container)).unwrap();
        let (_listener, port, _seen) = answering_door(|body| hub_answer("relay", body));
        register_polled_relay(port);

        let polled = poll_node("relay", None).unwrap();
        assert_eq!(polled.filed, 1, "the letter files: {polled:?}");
        assert!(aoide_storage::mail::show(&msgid).unwrap().is_some());
        assert!(
            !aoide_storage::mail::mail_dir().join("containers.jsonl").exists(),
            "a destination writes no containers.jsonl"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A container the destination already filed, offered again inside the hour,
    /// is reopened and dedups against its box. Its ack is held back: the first
    /// ack already went out, and a re-ack is limited to one per key per hour
    /// (`reack_due`). The re-ack after the hour is proved in storage.
    #[test]
    fn a_reoffered_container_at_its_destination_is_reopened_and_its_reack_is_held_back() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("destination-reopen");

        let me = register_local_origin();
        let (msgid, container) = container_from_relay(&me);
        aoide_storage::outbox::write_entry("relay", &OutboxEntry::transit_held(container.clone())).unwrap();
        // The hub offers what its spool holds and retires nothing: the poller's
        // own `filed` record must not take the re-offered container away first.
        let (_listener, port, seen) = answering_door(|_| {
            let (mut containers, mut envelopes) = (Vec::new(), Vec::new());
            for (container, envelope) in aoide_storage::outbox::poll_payloads("relay").unwrap() {
                match container {
                    Some(container) => containers.push(serde_json::to_value(container).unwrap()),
                    None => envelopes.push(serde_json::to_value(envelope).unwrap()),
                }
            }
            json!({ "jsonrpc": "2.0", "id": 1, "result": { "containers": containers, "envelopes": envelopes } }).to_string()
        });
        register_polled_relay(port);
        assert_eq!(poll_node("relay", None).unwrap().filed, 1);

        // The ack has gone out; the hub offers the container again and the ack is lost.
        for ack in aoide_storage::outbox::list_entries("relay").unwrap() {
            aoide_storage::outbox::remove_entry("relay", &ack.envelope.msgid).unwrap();
        }
        aoide_storage::outbox::write_entry("relay", &OutboxEntry::transit_held(container)).unwrap();

        let before = mentions(&deposit_bodies(&seen), &msgid);
        let again = poll_node("relay", None).unwrap();
        assert_eq!(again.filed, 0, "nothing files twice: {again:?}");
        assert!(again.refused.is_empty(), "{again:?}");
        assert_eq!(
            mentions(&deposit_bodies(&seen), &msgid),
            before,
            "a re-offer inside the hour sends no second ack"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A box-full refusal owes no ack: nothing is filed, `settle_deposit` spools
    /// nothing, and neither node's outbox holds an entry for the letter.
    #[test]
    fn a_box_full_refusal_spools_no_ack() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("box-full-no-ack");

        let me = register_local_origin();
        aoide_storage::mail::box_add("bob").unwrap();
        // Unparseable lines fill the box to its cap and compaction keeps them.
        let line = format!("{}\n", "x".repeat(1023));
        std::fs::write(
            aoide_storage::mail::box_path("bob"),
            line.repeat((aoide_storage::mail::BOX_MAX / 1024) as usize),
        )
        .unwrap();
        let letter = aoide_storage::mail::mint_outbound_letter("alice", &me, "bob", "no room").unwrap();
        let outcome = aoide_storage::mail::deposit(letter.clone(), "relay").unwrap();
        assert_eq!(outcome, aoide_storage::mail::DepositOutcome::BoxFull);
        settle_deposit(&letter, &outcome);

        assert!(aoide_storage::outbox::list_entries(&me).unwrap().is_empty(), "no ack for a letter not filed");
        assert!(aoide_storage::outbox::list_entries("relay").unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A second ack for a letter that is still live retires it: the duplicate
    /// files nothing, and the sender's entry does not come back.
    #[test]
    fn a_duplicate_ack_still_retires_a_live_entry() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("duplicate-ack-retires");

        let me = register_local_origin();
        let mut nodes = aoide_storage::node_store::load_nodes();
        nodes.push(keyed_node("far"));
        aoide_storage::node_store::save_nodes(&nodes).unwrap();
        let letter = aoide_storage::mail::mint_outbound_letter("alice", "far", "bob", "hi").unwrap();
        let msgid = letter.msgid.clone();
        let mesh = letter.header.origin_mesh.clone();
        let ack = far_receipt("far", &me, &mesh, &msgid);

        aoide_storage::outbox::write_entry("far", &OutboxEntry::fresh(letter.clone())).unwrap();
        let first = aoide_storage::mail::deposit(ack.clone(), "far").unwrap();
        assert!(matches!(first, aoide_storage::mail::DepositOutcome::Filed { .. }), "{first:?}");
        settle_deposit(&ack, &first);
        assert!(aoide_storage::outbox::list_entries("far").unwrap().is_empty());

        aoide_storage::outbox::write_entry("far", &OutboxEntry::fresh(letter)).unwrap();
        let second = aoide_storage::mail::deposit(ack.clone(), "far").unwrap();
        assert_eq!(second, aoide_storage::mail::DepositOutcome::Duplicate { filed_letter: false });
        settle_deposit(&ack, &second);
        assert!(aoide_storage::outbox::list_entries("far").unwrap().is_empty(), "the duplicate retires it again");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
