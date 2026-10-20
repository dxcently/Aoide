//! `state/mail/` — the addressed, signed, append-only mailbase
//! (`docs/architecture/MAIL.md`). Absorbs the old per-host `inbox.json`
//! receipt log (MAIL.md decision 8): today's two LOCAL delivery seams
//! (`conduct/graph/send.rs`, `server/a2a.rs`) file a `receipt` entry here,
//! and `mail send` files a `letter` — locally via [`file_letter`] for
//! `self/<name>`, or over the wire for a direct paired edge (P-M2): the
//! sender mints with [`mint_outbound_letter`] and spools the signed
//! [`Envelope`] into [`crate::outbox`]; the far door's `aoide/mailDeposit`
//! arm calls [`deposit`] here to verify and file it, ONLY ever via a
//! [`file_received_entry`] (never re-minted — the envelope arrives already
//! signed). **Transit is not filed here**: a sealed container addressed to
//! another node is a HOP, and [`crate::seal`] answers it as one
//! (`ContainerOutcome::Hopped`) — the hub spools the container toward the next
//! node and writes no line here, and this module never opens it, never names a
//! mailbox for it, and never rings a reader. The zone checks a receiver runs live in [`crate::seal`]'s chain walk.
//! The doorbell's own latch and its targeting queries ([`arms`],
//! [`ring_targets`], [`stamp_rung`], [`armed_names_for_reader`],
//! [`enrol_reader`]) live here (P-M5a-1); the ring itself — injecting a byte
//! into a pty — is `aoide-conduct`'s (P-M5a-2).
//!
//! ## The envelope
//!
//! [`Header`] is the closed, eight-field canonical string MAIL.md's "The
//! envelope" section defines — `version · from.node · from.name · to.node
//! · to.name · type · mintedAt · originMesh`, NUL-joined, **verbatim bytes**
//! (no trim, no case fold, no JSON — [`canonical_header_bytes`], not
//! `wire_auth::canonical_string`, which folds both). [`Envelope`] is that
//! header plus `text`, `sig` (origin's ed25519 over `header ‖ 0x00 ‖ text`),
//! and `msgid` (hex sha256 over `sig ‖ header ‖ 0x00 ‖ text` — no separator
//! before `header`, one before `text`; see [`seal`]). Deliberately absent:
//! the wire envelope's sibling `mesh`/`transit` fields — those are routing
//! facts nothing here consults yet: the zone check (MAIL.md step 3) belongs
//! to the transit lane, which a PLAINTEXT envelope never rides, so it is
//! skipped by shape rather than by omission (a sealed container takes it) —
//! and `header.origin_mesh` (always `""` here) stands in for them wherever a
//! "mesh" column is rendered.
//!
//! `self` resolves at mint time through [`crate::display::local_node_name`]
//! (the ADDRESS form of this box's own name — see that function: an OS host
//! name is case-preserved and native Windows' is conventionally uppercase
//! while a node name is grammar-lowercase) — the literal `"self"` never enters
//! a header, a signature, or a `msgid` (architect's ruling 3); `--to
//! self/<name>` is surface sugar the command layer resolves before calling
//! [`file_letter`]. Every node this box stamps on its OWN mail — `from` on the
//! way out, `to` on a local filing — is that same form, so the name it accepts
//! in an address and the name it writes down are one name.
//!
//! ## The store
//!
//! Files under `state_dir()/mail/` (MAIL.md "Store"):
//!
//! ```text
//! boxes/<name>.log   one Entry per line, a letter's box (append-only)
//! boxes/_legacy.log  letters whose name the grammar refuses
//! receipts.log       receipts, rotated to receipts.log.1 at ROTATE_AT
//! cursors.json       { "<name>": { "<reader>": { "seq": n } } }
//! seq                the last sequence number issued, ASCII decimal
//! ```
//!
//! **Write order, under the lock, no other:** the counter ([`next_seq`]) is
//! written first, then the line is appended and fsynced by
//! [`feed::append_line`], which cuts a torn tail before it writes. There is no
//! second file to keep in step: the box line IS the filing, and its `msgid` is
//! the dedup key (a re-offer finds it in the box and is a duplicate). A crash
//! after the counter write leaves a gap in `seq`, never a reused number.
//! Readers ([`read_entries_unlocked`]) read bytes, drop a final segment with no
//! newline, and skip any line that does not parse; they write nothing.
//! `crate::ledger`'s `append_ledger_entry`/`read_ledger` pair is this crate's
//! existing append-only-JSONL precedent.
//!
//! **Every mutation AND every read funnels through [`with_lock`]**, one
//! process-wide [`crate::fs::try_stage_lock`] acquisition — the SAME
//! `.stage.lock` [`crate::fs::with_stage_lock`] already flocks; a second
//! lock file under `state/mail/` would be a new abstraction for zero added
//! correctness. Unlike that fail-open sibling, a write that cannot take
//! this lock **fails** (`Err`) rather than running anyway (architect's
//! ruling 1) — the one behavioural change this phase makes to the locking
//! story, confined to mail's own writers. `with_lock` also runs
//! [`migrate_if_needed`] and [`migrate_cursors_if_needed`] first, every
//! time, so the very first store touch by ANY command — read or write —
//! performs the one-shot `inbox.json` → `inbox.json.migrated` field-mapping
//! AND the one-shot flat-to-per-reader `cursors.json` rewrite, both under
//! the SAME critical section (ruling 2), and every call after that sees
//! both already done and does nothing.
//!
//! ## Origin, hop, and hop-forgery
//!
//! Two independent lookups (P-M2 spec item 3), on purpose: the HOP (the
//! caller of `aoide/mailDeposit`) is whichever node record's stored pubkey
//! verified the *connection* signature (`a2a::verify_signed_request`,
//! threaded through as `ctx.signed_caller`) — this module never
//! consults it directly, only receives it as `via`. The ORIGIN is
//! `header.from.node`, checked in [`verify_origin_signature`] against the
//! ONE key `nodes.json` has on record under that exact name — never the
//! connection path's try-every-verified-key ladder, or a paired node
//! signing as another paired node's name would verify and file under the
//! wrong identity. Origin and hop coincide on the direct lane and are resolved
//! separately, because a relayed container's last hop is not its origin.
//!
//! ## Commands
//!
//! `aoide_client::commands::register_mail` wires `mail`, `mail send`
//! (`--to self/<name>` or `--to <node>/<name>` for a direct verified edge),
//! `mail read`, `mail show`, `mail mark`, `mail rm`, `mail outbox`, `mail
//! outbox rm` and `mail outbox retry` — moved out of this crate's own
//! `commands.rs` at P-M2 because sending over a direct edge needs the wire
//! lane (`aoide-client`'s own domain); this module stays the mailbase's
//! storage layer regardless of which crate dispatches into it. `mail route`
//! and the `--transit` surface are not registered yet, and `--hold` is a
//! spool flavor a send may carry rather than a command of its own.

use crate::display;
use aoide_protocol::feed;
use crate::fs::{atomic_write_private, state_dir};
use crate::identity;
use crate::stage::{load_stage, write_stage};
use crate::time::{now_iso_utc, parse_iso_utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::PathBuf;

/// The canonical header's `version` field for this phase. Bumps only if the
/// eight-field shape itself changes — independent of `CONTRACTS.md`'s crate
/// version (ruling 4: no bump this phase).
pub const ENVELOPE_VERSION: &str = "1";
/// `Header.kind`/`Entry.kind`'s value for a `mail send`-filed letter.
pub const ENTRY_TYPE_LETTER: &str = "letter";
/// `Header.kind`/`Entry.kind`'s value for a delivery receipt (what
/// `inbox.json` carried, per MAIL.md "Store").
pub const ENTRY_TYPE_RECEIPT: &str = "receipt";
/// The taught word for a `charter` envelope that arrived as PLAINTEXT: the
/// kind exists, but a plaintext envelope carries no charter to apply, and
/// filing it as correspondence would file a letter whose only content is a
/// claim about itself.
pub const REFUSAL_NOT_CORRESPONDENCE: &str = "not-correspondence";
/// The far box has no room for this letter (MAIL.md §Wire): a fact about the
/// box's state, not the letter's bytes. The sender defers and keeps the letter,
/// and a poller keeps the hub's custody of the container.
pub const REFUSAL_BOX_FULL: &str = "box-full";
/// A letter names a box this node does not have. Nothing is filed or acked,
/// and the sender parks the letter: it is a fact about the name, not a verdict
/// on the bytes, and the answer names no box ([`UNKNOWN_BOX_DETAIL`]).
pub const REFUSAL_UNKNOWN_BOX: &str = "unknown-box";
/// The fixed detail every `unknown-box` answer carries. It names no box, so a
/// relay or a hostile sender learns nothing about which names exist.
pub const UNKNOWN_BOX_DETAIL: &str = "no mailbox of that name is on this node; nothing was filed or acknowledged";
/// `Header.kind`/`Entry.kind`'s value for a `charter` letter (P-CHARTER) — a signed
/// charter travelling as mail. Unlike a letter and a receipt this kind is never
/// FILED: the enclosed charter is applied to `state/mesh/`, and the letter's local
/// envelope exists only as the outbox's bookkeeping key. Which is also why
/// [`arms`] is false for it and why the drain retires it on a delivered
/// deposit instead of waiting for an ack — there is no mailbox to ack from.
pub const ENTRY_TYPE_CHARTER: &str = "charter";
/// `Entry.kind`'s value for a SEALED CONTAINER this node is relaying
/// (MAIL.md §Transit): a letter in transit, held as the container and its
/// routing metadata — never an opened envelope, never a mailbox name, and never
/// letter bytes. A hub neither opens it nor files it as correspondence, so this
/// kind is `false` for [`arms`] like a receipt, and readers hide it.
pub const ENTRY_TYPE_TRANSIT: &str = "transit";

/// Which entry kinds ARM a reader's doorbell. A `receipt` (a delivery
/// record, or a deposit ack filed under the origin's name) never does, and
/// neither does a `charter` letter (applied, never filed).
pub fn arms(kind: &str) -> bool {
    kind == ENTRY_TYPE_LETTER
}

/// `<node>/<name>` — one side of a header. `from.name` and a receipt's
/// `to.name` stay free text — attribution only, nothing here validates
/// them against a mesh declaration (P-M1 has none). A letter's `to.name`
/// is grammar-validated at filing instead ([`file_letter`],
/// [`mint_outbound_letter`]), never clamped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Address {
    pub node: String,
    pub name: String,
}

/// The eight closed canonical-header fields (MAIL.md "The envelope").
/// [`canonical_header_bytes`] is the ONLY place their signed byte order is
/// defined; every other reader of this struct goes through JSON field names.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Header {
    pub version: String,
    pub from: Address,
    pub to: Address,
    #[serde(rename = "type")]
    pub kind: String,
    pub minted_at: String,
    /// The zone the origin posted in. Always `""` in P-M1 — there is no
    /// mesh declaration yet to post into (kept as a real field, not
    /// omitted, because it is part of the closed eight-field header
    /// MAIL.md's byte formula signs; a later phase's mesh-aware sender
    /// simply stops leaving it blank).
    pub origin_mesh: String,
}

/// The signed, immutable unit that moves (MAIL.md "The envelope"). No
/// `mesh`/`transit` fields — a plaintext envelope never rides a zone; see
/// this module's doc.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    pub header: Header,
    pub text: String,
    /// Hex ed25519 signature over `header ‖ 0x00 ‖ text` ([`seal`]).
    pub sig: String,
    /// Hex sha256 over `sig ‖ header ‖ 0x00 ‖ text` ([`seal`]).
    pub msgid: String,
}

/// One `base.jsonl` line: an envelope plus local, never-signed facts
/// (MAIL.md "Store").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// Local to this node, like an NNTP article number — never crosses a
    /// link. `last line's seq + 1`, read under the lock ([`next_seq`]).
    pub seq: u64,
    pub received_at: String,
    /// Mirrors `envelope.header.kind` — cheap filtering without parsing
    /// the full envelope (the JSON shape MAIL.md's "Store" example shows).
    #[serde(rename = "type")]
    pub kind: String,
    /// The hop that deposited it; always `"self"` in P-M1 — there is no
    /// other hop yet.
    pub via: String,
    /// The verified origin key the letter was deposited under, for the sender's
    /// quarter ([`Caps`]). Empty for a local letter and a migrated line, which
    /// have no quarter, and skipped when empty, so those lines are unchanged.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub origin: String,
    pub envelope: Envelope,
}

/// One reader's high-water mark, like an NNTP `.newsrc` line (MAIL.md
/// "Store").
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Mark {
    #[serde(default)]
    pub seq: u64,
    /// The doorbell latch (MAIL.md "Delivery and the doorbell"): the highest
    /// arming `seq` this reader has been rung for. Armed again only once
    /// `seq` (the reader's own read) reaches it. Zero = never rung; omitted
    /// from disk while zero so an unrung file is byte-identical to before.
    #[serde(default, skip_serializing_if = "u64_is_zero")]
    pub rung: u64,
}

fn u64_is_zero(v: &u64) -> bool {
    *v == 0
}

/// One name's readers, each with its own high-water mark (MAIL.md "Store":
/// "a name maps to a set of readers, each with its own high-water mark").
/// The key set IS the doorbell's enrolment list: a reader is enrolled by
/// having a key here, [`Mark::rung`] is that reader's latch, and
/// [`ring_targets`] reads both to decide who is armed.
pub type Cursor = BTreeMap<String, Mark>;

/// `cursors.json`'s whole shape is this bare map of maps — no wrapper, no
/// schema version (MAIL.md shows it unwrapped; this module does not add
/// one).
pub type Cursors = BTreeMap<String, Cursor>;

/// `$AOIDE_STATE_DIR/mail/` (ordinary [`state_dir`] resolution — see that
/// function's own doc for the `AOIDE_STATE_DIR`-then-`AOIDE_ROOT/state`
/// precedence every state file shares).
pub fn mail_dir() -> PathBuf {
    state_dir().join("mail")
}
pub fn base_path() -> PathBuf {
    mail_dir().join("base.jsonl")
}
pub fn cursors_path() -> PathBuf {
    mail_dir().join("cursors.json")
}
pub fn seen_path() -> PathBuf {
    mail_dir().join("seen.jsonl")
}
/// `state/mail/boxes/`: one capped log per mailbox, plus the legacy box.
pub fn boxes_dir() -> PathBuf {
    mail_dir().join("boxes")
}
/// Every receipt filed on this box, `receipts.log`; its rotated generation is
/// `receipts.log.1`.
pub fn receipts_log_path() -> PathBuf {
    mail_dir().join("receipts.log")
}
/// The byte cap past which `receipts.log` rotates to `.1` (MAIL.md "Store").
pub const ROTATE_AT: u64 = 4 * 1024 * 1024;
/// The longest box name, the DNS label limit, so a 64-hex board id is never a
/// box name.
pub const BOX_NAME_MAX: usize = 63;

/// The largest envelope a letter may serialize to. A letter over it is refused
/// `too-large` at the mint and at the door, before any box is read.
pub const LETTER_MAX: usize = 256 * 1024;
/// The most a single box file may hold, measured after the append.
pub const BOX_MAX: u64 = 16 * 1024 * 1024;
/// The most all box files together may hold. With the two rotating logs at
/// [`ROTATE_AT`] each, all mail stays under 256 MiB by construction.
pub const BOXES_MAX: u64 = 240 * 1024 * 1024;

/// The caps a deposit is admitted under. [`CAPS`] is the store's own; a test
/// passes a small one through [`deposit_within`] and [`fit`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caps {
    pub box_max: u64,
    pub boxes_max: u64,
}

pub const CAPS: Caps = Caps { box_max: BOX_MAX, boxes_max: BOXES_MAX };

/// Which cap refused a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Full {
    /// The target box would exceed `box_max`.
    Box,
    /// The sender's share of the target box would exceed a quarter of `box_max`.
    Origin,
    /// The sender's share of every box would exceed a quarter of `boxes_max`.
    OriginTotal,
    /// All boxes together would exceed `boxes_max`.
    Total,
}

/// The bytes one verified origin key holds: in the target box, and across every
/// box together.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Held {
    pub in_box: u64,
    pub in_store: u64,
}

/// Admits one line of `line_len` bytes to a box of `box_len`, given the bytes
/// the sender holds (`held`, `None` for a local letter, which has no quarter),
/// and all boxes' bytes (`boxes_len`). Pure: the store's policy is this one
/// comparison chain.
pub fn admit(caps: Caps, box_len: u64, held: Option<Held>, boxes_len: u64, line_len: u64) -> Result<(), Full> {
    if box_len + line_len > caps.box_max {
        return Err(Full::Box);
    }
    if let Some(held) = held {
        if held.in_box + line_len > caps.box_max / 4 {
            return Err(Full::Origin);
        }
        if held.in_store + line_len > caps.boxes_max / 4 {
            return Err(Full::OriginTotal);
        }
    }
    if boxes_len + line_len > caps.boxes_max {
        return Err(Full::Total);
    }
    Ok(())
}

/// Refusal word for a letter the target box has no room for. Nothing is filed
/// or acknowledged, and the sender defers the letter and keeps it.
/// (`REFUSAL_BOX_FULL` is declared above.)
pub const REFUSAL_TOO_LARGE: &str = "too-large";
/// The fixed detail a `too-large` answer carries.
pub const TOO_LARGE_DETAIL: &str = "the letter is over 256 KiB serialized; nothing was filed; send less text";
/// The fixed detail a `box-full` answer carries. It names no box.
pub const BOX_FULL_DETAIL: &str = "a mailbox on this node is full; nothing was filed or acknowledged; keep the letter and retry";

/// The counter's value, `None` while no counter has been written yet.
fn read_counter() -> Result<Option<u64>, String> {
    let path = seq_path();
    match std::fs::read_to_string(&path) {
        Ok(raw) => raw
            .trim()
            .parse::<u64>()
            .map(Some)
            .map_err(|_| format!("{}: not a sequence number", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// The pre-mail `state/inbox.json` path, recomputed directly rather than
/// through the now-deleted `inbox` module — `inbox.rs`'s own
/// `inbox_path()` was exactly this.
fn legacy_inbox_path() -> PathBuf {
    state_dir().join("inbox.json")
}

/// Just enough of the old `InboxFile`/`InboxEntry` shape to field-map it
/// once (ruling 7/8: `context` and the read flag are both dropped, so
/// neither is declared here — an unknown field is simply ignored by serde,
/// never an error).
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyInboxEntry {
    #[serde(default)]
    from: String,
    #[serde(default)]
    target: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    received_at: String,
}

#[derive(Debug, Default, Deserialize)]
struct LegacyInboxFile {
    #[serde(default)]
    entries: Vec<LegacyInboxEntry>,
}

/// Hex-encode raw bytes, lowercase, no separator. A small local copy, not a
/// reach into `identity.rs`'s or `wire_auth.rs`'s own private copies — this
/// crate's established convention (`wire_auth.rs`'s own doc on its copy) is
/// a tiny per-module copy over a shared `pub` utility.
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Decode a hex string to raw bytes — `None` on an odd length or any
/// non-hex-digit character, never a panic (feeds on a stored/tampered
/// field in the msgid-recomputation check, never trusted as well-formed).
fn hex_decode(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if s.is_empty() || s.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let hi = (bytes[i] as char).to_digit(16)?;
        let lo = (bytes[i + 1] as char).to_digit(16)?;
        out.push(((hi << 4) | lo) as u8);
        i += 2;
    }
    Some(out)
}

/// The canonical header string, **verbatim bytes**: the eight fields
/// NUL-joined in MAIL.md's fixed order, no trim, no case fold, no JSON — so
/// key order and whitespace can never make two hops disagree, and no hop
/// can change the case of a name without breaking the signature. The ONLY
/// place this order is defined; [`seal`] and [`compute_msgid`] both go
/// through here rather than assembling the string twice.
///
/// `pub(crate)` since P-SEAL: `crate::seal` frames this exact byte string
/// as the inner envelope's `header` field, so the sealed plaintext carries
/// the signed bytes themselves rather than an unpinned JSON rendering
/// (crates/AGENTS.md: widen a symbol a consumer needs, never fork it).
pub(crate) fn canonical_header_bytes(h: &Header) -> Vec<u8> {
    [
        h.version.as_str(),
        h.from.node.as_str(),
        h.from.name.as_str(),
        h.to.node.as_str(),
        h.to.name.as_str(),
        h.kind.as_str(),
        h.minted_at.as_str(),
        h.origin_mesh.as_str(),
    ]
    .join("\0")
    .into_bytes()
}

/// [`canonical_header_bytes`]'s exact inverse: split the eight NUL-joined
/// fields back into a [`Header`], or `None` when the bytes are not eight
/// NUL-free fields. Lossless in both directions — [`canonical_header_bytes`]
/// of the result is the input byte for byte — so a receiver can frame the
/// signed bytes and still recover the fields it must file.
pub(crate) fn header_from_canonical_bytes(bytes: &[u8]) -> Option<Header> {
    let text = std::str::from_utf8(bytes).ok()?;
    let parts: Vec<&str> = text.split('\0').collect();
    if parts.len() != 8 {
        return None;
    }
    Some(Header {
        version: parts[0].to_string(),
        from: Address { node: parts[1].to_string(), name: parts[2].to_string() },
        to: Address { node: parts[3].to_string(), name: parts[4].to_string() },
        kind: parts[5].to_string(),
        minted_at: parts[6].to_string(),
        origin_mesh: parts[7].to_string(),
    })
}

/// Is `text` exactly one msgid: 64 lowercase hex digits, nothing around them.
/// The one shape a receipt's text may take. A receipt's text is the msgid it
/// acknowledges, and it becomes a path component in the outbox (`<msgid>.json`),
/// so anything looser — a trailing newline, a `../`, a capital — is refused
/// before it can name a file, not after.
pub fn is_msgid(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Recompute `msgid` from a (possibly tampered) header/text/sig triple —
/// `hex sha256 over sig ‖ header ‖ 0x00 ‖ text`, no separator before
/// `header`, one before `text` (MAIL.md "The envelope"). `None` only when
/// `sig_hex` itself doesn't decode; a bad/tampered `header` or `text` still
/// recomputes cleanly, just to a DIFFERENT `msgid` — that mismatch is the
/// tamper-evidence this function exists to make checkable.
///
/// `pub(crate)` since P-SEAL: `crate::seal` recomputes the inner envelope's
/// id from the opened plaintext, through this same function rather than a
/// second copy of the formula.
pub(crate) fn compute_msgid(header: &Header, text: &str, sig_hex: &str) -> Option<String> {
    let sig_bytes = hex_decode(sig_hex)?;
    let header_bytes = canonical_header_bytes(header);
    let mut input = Vec::with_capacity(sig_bytes.len() + header_bytes.len() + 1 + text.len());
    input.extend_from_slice(&sig_bytes);
    input.extend_from_slice(&header_bytes);
    input.push(0u8);
    input.extend_from_slice(text.as_bytes());
    Some(hex_encode(&Sha256::digest(&input)))
}

/// Sign `header ‖ 0x00 ‖ text` with `kp` and derive `msgid` from the result
/// — the one place a fresh envelope is signed. Origin-signed only (ruling
/// 6): P-M1 never verifies anyone else's signature, since there is no one
/// else yet.
fn seal(header: &Header, text: &str, kp: &identity::Keypair) -> (String, String) {
    let mut sig_input = canonical_header_bytes(header);
    sig_input.push(0u8);
    sig_input.extend_from_slice(text.as_bytes());
    let sig_hex = hex_encode(&kp.sign(&sig_input).to_bytes());
    let msgid = compute_msgid(header, text, &sig_hex)
        .expect("a signature this function just hex-encoded always decodes");
    (sig_hex, msgid)
}

/// Fail-closed choke point EVERY mail operation, read or write, funnels
/// through exactly once at its own top level: takes the shared stage lock
/// ([`crate::fs::try_stage_lock`]), runs [`migrate_if_needed`], then `f`.
/// `try_stage_lock` is re-entrant on its thread, so a caller already inside
/// `with_stage_lock` (the reaper's [`retire_readers`]) runs this in that same
/// hold. Every raw helper below (`read_entries_unlocked`, `append_base_line`,
/// `next_seq`, …) is written assuming its caller already holds this lock, and
/// never calls `with_lock` itself: the migrations at its top are not meant to
/// run twice in one operation.
fn with_lock<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    match crate::fs::try_stage_lock(move || {
        ensure_mail_dir()?;
        migrate_base_if_needed()?;
        migrate_if_needed()?;
        migrate_cursors_if_needed()?;
        f()
    }) {
        Ok(inner) => inner,
        Err(lock_err) => Err(lock_err),
    }
}

fn ensure_mail_dir() -> Result<(), String> {
    let dir = mail_dir();
    crate::fs::ensure_write_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))
}

/// One-shot split of the old single `base.jsonl` into boxes, receipts and the
/// legacy box (MAIL.md "Store"). It runs first in every locked operation and
/// does nothing once `base.jsonl` is gone. A first migration keeps every line's
/// `seq` and every mark, and files each line once: a line already held is
/// skipped, so a retry after a crash never duplicates. A `base.jsonl` that
/// appears after the migration (an older binary wrote it) is imported with fresh
/// numbers, since its letters were accepted and may already be acknowledged,
/// and archived raw. The archive is never read. Transit lines are not entries,
/// so they stay in the archive.
fn migrate_base_if_needed() -> Result<(), String> {
    migrate_base_within(CAPS)
}

/// The bytes a migration has put in each box and in the store, so each migrated
/// line is admitted from memory: a per-line `measure` would read the store once
/// for every line.
struct MigrationBudget {
    boxes: BTreeMap<PathBuf, u64>,
    total: u64,
}

impl MigrationBudget {
    fn load() -> Result<Self, String> {
        let mut budget = MigrationBudget { boxes: BTreeMap::new(), total: 0 };
        for path in box_paths()? {
            let len = std::fs::metadata(&path).map_err(|e| format!("{}: {e}", path.display()))?.len();
            budget.boxes.insert(path, len);
            budget.total += len;
        }
        Ok(budget)
    }

    fn admits(&self, caps: Caps, path: &std::path::Path, line_len: u64) -> bool {
        let box_len = self.boxes.get(path).copied().unwrap_or(0);
        admit(caps, box_len, None, self.total, line_len).is_ok()
    }

    fn add(&mut self, path: &std::path::Path, line_len: u64) {
        *self.boxes.entry(path.to_path_buf()).or_insert(0) += line_len;
        self.total += line_len;
    }
}

/// Files one migrated line under `caps`, and says where it went. A migrated line
/// has no origin, so only the box and store caps apply to it. A line its own box
/// has no room for is parked in `_legacy`, which is counted in the store and read
/// by no box reader, so it is kept but not shown. Returns whether the line was
/// filed; one that fits neither stays in the archive the migration keeps, and
/// the migration says so on stderr and in its audit line.
fn file_migrated(entry: &Entry, caps: Caps, budget: &mut MigrationBudget) -> Result<bool, String> {
    if entry.kind == ENTRY_TYPE_RECEIPT {
        append_entry(entry)?;
        return Ok(true);
    }
    let line = serde_json::to_string(entry).map_err(|e| e.to_string())?;
    let line_len = line.len() as u64 + 1;
    let own = box_path(&entry.envelope.header.to.name);
    if budget.admits(caps, &own, line_len) {
        append_entry(entry)?;
        budget.add(&own, line_len);
        return Ok(true);
    }
    let legacy = legacy_box_path();
    if budget.admits(caps, &legacy, line_len) {
        feed::append_line(&legacy, line.as_bytes()).map_err(|e| format!("{}: {e}", legacy.display()))?;
        budget.add(&legacy, line_len);
        return Ok(true);
    }
    eprintln!(
        "[aoide/mail] migration: no room for a line to `{}` in its box or in _legacy; it stays in base.jsonl.migrated",
        entry.envelope.header.to.name
    );
    Ok(false)
}

/// [`migrate_base_if_needed`] under `caps`. A test passes a small one.
fn migrate_base_within(caps: Caps) -> Result<(), String> {
    let base = base_path();
    if !base.exists() {
        return Ok(());
    }
    let archive = mail_dir().join("base.jsonl.migrated");
    let late = archive.exists();
    let lines = feed::read_lines(&base).map_err(|e| format!("{}: {e}", base.display()))?;
    let mut filed = Filed::load()?;
    let mut budget = MigrationBudget::load()?;
    let mut unfiled = 0usize;
    for line in &lines {
        let Ok(mut entry) = serde_json::from_slice::<Entry>(line) else {
            unfiled += 1;
            continue;
        };
        if filed.holds(&entry) {
            continue;
        }
        if late {
            entry.seq = next_seq()?;
        }
        if file_migrated(&entry, caps, &mut budget)? {
            filed.add(&entry);
        } else {
            unfiled += 1;
        }
    }
    // One audit line per migration, written before the archive is, so a line
    // that cannot be audited leaves base.jsonl in place for the next try.
    aoide_protocol::audit::audit(
        &aoide_protocol::audit::default_audit_log(),
        aoide_protocol::Door::Cli,
        aoide_protocol::audit::EventClass::Audit,
        "mail.migrate",
        "ok",
        &format!("migration of base.jsonl: {} lines, unfiled {unfiled}, kept in base.jsonl.migrated", lines.len()),
    )
    .map_err(|e| format!("the audit log cannot be written, so the migration waits: {e}"))?;
    if late {
        for line in &lines {
            feed::append_line(&archive, line).map_err(|e| format!("{}: {e}", archive.display()))?;
        }
        std::fs::remove_file(&base).map_err(|e| format!("{}: {e}", base.display()))?;
        return Ok(());
    }
    let floor = seq_floor()?.max(read_counter()?.unwrap_or(0));
    atomic_write_private(&seq_path(), format!("{floor}\n").as_bytes())
        .map_err(|e| format!("{}: {e}", seq_path().display()))?;
    std::fs::rename(&base, &archive).map_err(|e| format!("{}: {e}", base.display()))?;
    for old in [seen_path(), mail_dir().join("receipts.seen")] {
        if old.exists() {
            let mut dest = old.clone().into_os_string();
            dest.push(".migrated");
            std::fs::rename(&old, PathBuf::from(dest)).map_err(|e| format!("{}: {e}", old.display()))?;
        }
    }
    Ok(())
}

/// Every msgid and receipt key already filed, read once per migration so each
/// line costs a lookup, not a rescan. Kept current with each line it files.
struct Filed {
    msgids: HashSet<String>,
    keys: HashSet<String>,
}

impl Filed {
    fn load() -> Result<Self, String> {
        let mut filed = Filed { msgids: HashSet::new(), keys: HashSet::new() };
        for entry in read_entries_unlocked()? {
            filed.add(&entry);
        }
        Ok(filed)
    }

    fn holds(&self, entry: &Entry) -> bool {
        self.msgids.contains(&entry.envelope.msgid)
            || (entry.kind == ENTRY_TYPE_RECEIPT && self.keys.contains(&receipt_key(&entry.envelope)))
    }

    fn add(&mut self, entry: &Entry) {
        self.msgids.insert(entry.envelope.msgid.clone());
        if entry.kind == ENTRY_TYPE_RECEIPT {
            self.keys.insert(receipt_key(&entry.envelope));
        }
    }
}

/// One-shot, idempotent `inbox.json` → typed `receipt` entries, run under
/// the SAME lock acquisition as whichever operation triggered it (ruling
/// 2) — never hung off [`crate::fs::migrate_root_once`], which stays
/// exactly as `a30d25f` left it. Checking `legacy_inbox_path()`'s mere
/// existence as the "already migrated?" test is what makes a concurrent
/// second opener a clean no-op: the first opener's rename (at the end of
/// this function, inside the same critical section) is the one fact a
/// second, lock-serialized caller observes.
///
/// The old per-entry `read` flag has no high-water-mark counterpart and is
/// not carried (ruling 8: every migrated entry is unread once); `context`
/// is dropped too (ruling 7) — [`LegacyInboxEntry`] never declares either
/// field, so there is nothing to carry even by accident.
///
/// A crash between appending row N and the rename below leaves `inbox.json`
/// in place, so a retry re-enters this same loop over the same rows —
/// [`seal`] is deterministic, so a row already filed recomputes the exact
/// same `msgid`. Reading `seen.jsonl` into a set ONCE before the loop and
/// skipping a row already in it is what makes that retry file each legacy
/// row exactly once instead of duplicating rows `1..N`.
fn migrate_if_needed() -> Result<(), String> {
    let old_path = legacy_inbox_path();
    let raw = match std::fs::read_to_string(&old_path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("{}: {e}", old_path.display())),
    };
    let legacy: LegacyInboxFile = serde_json::from_str(&raw)
        .map_err(|e| format!("{}: unreadable legacy inbox: {e}", old_path.display()))?;

    let node = display::local_node_name();
    let (kp, _) = identity::load_or_mint().map_err(|e| e.to_string())?;
    let already_seen: HashSet<String> = read_entries_unlocked()?.into_iter().map(|e| e.envelope.msgid).collect();

    for row in &legacy.entries {
        let header = Header {
            version: ENVELOPE_VERSION.to_string(),
            from: Address { node: node.clone(), name: row.from.clone() },
            to: Address { node: node.clone(), name: row.target.clone() },
            kind: ENTRY_TYPE_RECEIPT.to_string(),
            minted_at: row.received_at.clone(),
            origin_mesh: String::new(),
        };
        let (sig, msgid) = seal(&header, &row.text, &kp);
        if already_seen.contains(&msgid) {
            continue;
        }
        let envelope = Envelope { header, text: row.text.clone(), sig, msgid: msgid.clone() };
        let entry = Entry {
            seq: next_seq()?,
            received_at: row.received_at.clone(),
            kind: ENTRY_TYPE_RECEIPT.to_string(),
            via: "self".to_string(),
            origin: String::new(),
            envelope,
        };
        append_entry(&entry)?;
    }

    let migrated_path = match old_path.parent() {
        Some(p) => p.join("inbox.json.migrated"),
        None => return Err(format!("{}: no parent directory", old_path.display())),
    };
    if !migrated_path.exists() {
        if let Some(dir) = old_path.parent() {
            crate::fs::ensure_write_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::rename(&old_path, &migrated_path).map_err(|e| {
            format!("{}: rename to {}: {e}", old_path.display(), migrated_path.display())
        })?;
    }
    Ok(())
}

/// One-shot, idempotent flat-to-per-reader `cursors.json` rewrite, run under
/// the SAME lock acquisition as [`migrate_if_needed`] (MAIL.md "Store": "A
/// `cursors.json` carrying the flat `{ "<name>": { "seq", "readers" } }`
/// shape migrates on first open under the same lock: each recorded reader
/// inherits the name's old `seq`, and a name with no recorded reader keeps
/// its mark under the name itself"). Read as untyped JSON, never as
/// [`Cursors`] — the old shape's `{"seq": n, "readers": [...]}` does not
/// deserialize as a reader-keyed [`Cursor`], so a typed load here would
/// error instead of migrate.
///
/// A name is old-shape iff its value has a `seq` field that is a bare JSON
/// *number* — the new shape only ever nests `seq` inside a per-reader
/// object, even for a reader literally named `"seq"`. That per-name shape
/// check is what makes a second call a clean no-op with no sentinel file to
/// consult: every name this rewrites no longer matches the old-shape test
/// on the next pass, so nobody re-reads what they had already read.
fn migrate_cursors_if_needed() -> Result<(), String> {
    let path = cursors_path();
    let raw = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let top: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&raw)
        .map_err(|e| format!("{}: unreadable cursors: {e}", path.display()))?;

    let mut rewritten = false;
    let mut out = Cursors::new();
    for (name, value) in top {
        match value.get("seq").and_then(serde_json::Value::as_u64) {
            Some(old_seq) => {
                // Old flat shape: `{ "seq": n, "readers": [...] }`.
                rewritten = true;
                let readers: Vec<String> = value
                    .get("readers")
                    .and_then(serde_json::Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(serde_json::Value::as_str)
                            .filter(|s| !s.is_empty())
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                let mut cursor = Cursor::new();
                if readers.is_empty() {
                    cursor.insert(name.clone(), Mark { seq: old_seq, rung: 0 });
                } else {
                    for reader in readers {
                        cursor.insert(migrated_reader_key(&name, reader), Mark { seq: old_seq, rung: 0 });
                    }
                }
                out.insert(name, cursor);
            }
            None => {
                // Already the new per-reader shape — parse it as such and
                // pass it through untouched.
                let cursor: Cursor = serde_json::from_value(value)
                    .map_err(|e| format!("{}: unreadable cursor for {name:?}: {e}", path.display()))?;
                let mut migrated = Cursor::new();
                for (reader, mark) in cursor {
                    let key = migrated_reader_key(&name, reader.clone());
                    rewritten |= key != reader;
                    migrated.insert(key, mark);
                }
                out.insert(name, migrated);
            }
        }
    }

    if rewritten {
        save_cursors(&out)?;
    }
    Ok(())
}

/// Every line of one capped log that parses as a correspondence entry, in file
/// order. A malformed line is skipped, never an error. Raw: assumes the caller
/// holds the lock when the file can change under it.
fn read_file_entries(path: &std::path::Path) -> Result<Vec<Entry>, String> {
    let lines = feed::read_lines(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(lines.iter().filter_map(|l| serde_json::from_slice::<Entry>(l).ok()).collect())
}

/// Rotates a rotating log before a record of `record_len` bytes (newline
/// included) is appended, so the log stays under `cap` after the append and a
/// reader can bound its read at the cap.
pub(crate) fn rotate_before_append(path: &std::path::Path, cap: u64, record_len: usize) -> Result<(), String> {
    feed::rotate_at(path, cap.saturating_sub(record_len as u64)).map_err(|e| format!("{}: {e}", path.display()))
}

/// The records of one rotating log, read up to [`ROTATE_AT`] bytes. Every
/// append keeps a generation under the cap ([`rotate_before_append`]), so this
/// reads all of it; a file past the cap is read only to the cap.
pub(crate) fn read_rotating_entries(path: &std::path::Path) -> Result<Vec<Entry>, String> {
    let lines = feed::read_lines_up_to(path, ROTATE_AT).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(lines.iter().filter_map(|l| serde_json::from_slice::<Entry>(l).ok()).collect())
}

/// `receipts.log` and its one rotated generation, in that order.
fn receipt_generations() -> [PathBuf; 2] {
    let current = receipts_log_path();
    let rotated = PathBuf::from(format!("{}.1", current.display()));
    [current, rotated]
}

/// The box name the legacy file holds: every letter whose `to.name` the grammar
/// refuses. Its name starts with `_`, which the grammar never does.
const LEGACY_BOX: &str = "_legacy";

/// A box name the grammar accepts: a node name, at most [`BOX_NAME_MAX`] bytes,
/// and not a DOS device name. One rule on every OS: a box file must be
/// creatable on Windows too, so `con.log` is refused here, not there.
pub fn valid_box_name(name: &str) -> bool {
    crate::node_store::valid_node_name(name) && name.len() <= BOX_NAME_MAX && !is_device_name(name)
}

/// `con`, `prn`, `aux`, `nul`, and `com0`..`com9`, `lpt0`..`lpt9`.
fn is_device_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    matches!(name, "con" | "prn" | "aux" | "nul")
        || (bytes.len() == 4 && (name.starts_with("com") || name.starts_with("lpt")) && bytes[3].is_ascii_digit())
}

/// Whether `name`'s box exists. A box exists only because a local act created
/// it ([`box_add`], [`enrol_reader`], [`file_letter`]), never because a far
/// deposit named it.
pub fn box_exists(name: &str) -> bool {
    valid_box_name(name) && box_path(name).exists()
}

/// Creates `name`'s box file, owner-only and empty, if it does not exist.
/// Returns whether it created it. Raw — assumes the caller holds the lock.
fn create_box_unlocked(name: &str) -> Result<bool, String> {
    let path = box_path(name);
    if path.exists() {
        return Ok(false);
    }
    ensure_mail_dir()?;
    crate::fs::ensure_write_dir(&boxes_dir()).map_err(|e| format!("{}: {e}", boxes_dir().display()))?;
    feed::ensure_log(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(true)
}

/// The box a daemon creates when it starts, so a far letter to the conductor
/// files from the first deposit. It is the one role box that is not a session.
pub const CONDUCTOR_BOX: &str = "conductor";

/// Creates the boxes a daemon owns at start: [`CONDUCTOR_BOX`]. A local act, so a
/// far deposit to a name no local act created is still refused.
pub fn ensure_auto_boxes() -> Result<(), String> {
    box_add(CONDUCTOR_BOX).map(|_| ())
}

/// The session boxes this store knows, and whether each is dead: a box is
/// recorded when its session's first insert makes it (`ensure_session_box`), and
/// `retire_readers` marks it dead when the roster it is handed no longer names
/// its session, and live again when the roster does (MAIL.md "Store").
pub type SessionBoxes = BTreeMap<String, bool>;

pub fn session_boxes_path() -> PathBuf {
    mail_dir().join("session-boxes.json")
}

fn load_session_boxes() -> Result<SessionBoxes, String> {
    match std::fs::read(session_boxes_path()) {
        Ok(raw) => serde_json::from_slice(&raw).map_err(|e| format!("{}: {e}", session_boxes_path().display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(SessionBoxes::new()),
        Err(e) => Err(format!("{}: {e}", session_boxes_path().display())),
    }
}

fn save_session_boxes(boxes: &SessionBoxes) -> Result<(), String> {
    let body = serde_json::to_vec(boxes).map_err(|e| e.to_string())?;
    atomic_write_private(&session_boxes_path(), &body).map_err(|e| format!("{}: {e}", session_boxes_path().display()))
}

/// The one rule for a dead box (MAIL.md "Store"). A box is dead when its name is
/// a session box the roster has gone without, or when every reader key it had was
/// retired (its reader map is empty). A name a pseudo-reader still holds is not
/// empty, a name nothing recorded and nothing read has neither mark, and the role
/// box `conductor` is never dead.
fn is_dead(name: &str, cursors: &Cursors, sessions: &SessionBoxes) -> bool {
    name != CONDUCTOR_BOX
        && (sessions.get(name) == Some(&true) || cursors.get(name).is_some_and(BTreeMap::is_empty))
}

/// Whether a far letter may file to `name`: the box exists and is not dead.
fn box_accepts(name: &str, cursors: &Cursors, sessions: &SessionBoxes) -> bool {
    box_exists(name) && !is_dead(name, cursors, sessions)
}

/// Reap's half of a dead box's life (MAIL.md "Store"): deletes every dead box
/// that holds no letter, with its cursor key, and audits each deletion before it
/// happens. A dead box that still holds mail stays, dead, until its letters are
/// gone. Returns the names it deleted. Runs under the stage lock.
pub fn reap_dead_boxes() -> Result<Vec<String>, String> {
    if !cursors_path().exists() && !session_boxes_path().exists() {
        return Ok(Vec::new());
    }
    with_lock(|| {
        let mut cursors = load_cursors()?;
        let mut sessions = load_session_boxes()?;
        let names: BTreeSet<String> = cursors.keys().chain(sessions.keys()).cloned().collect();
        let dead: Vec<String> = names.into_iter().filter(|name| is_dead(name, &cursors, &sessions)).collect();
        let mut removed = Vec::new();
        let mut failure = Ok(());
        for name in dead {
            match reap_dead_box(&name) {
                Ok(true) => {
                    cursors.remove(&name);
                    sessions.remove(&name);
                    removed.push(name);
                }
                Ok(false) => {}
                Err(e) => {
                    failure = Err(e);
                    break;
                }
            }
        }
        if !removed.is_empty() {
            save_cursors(&cursors)?;
            save_session_boxes(&sessions)?;
        }
        failure.map(|()| removed)
    })
}

/// Deletes one dead box's file if it holds no letter, after its audit line.
/// `Ok(false)` when it holds mail and stays.
fn reap_dead_box(name: &str) -> Result<bool, String> {
    let path = box_path(name);
    if path.exists() {
        if !box_lines(&path)?.is_empty() {
            return Ok(false);
        }
        aoide_protocol::audit::audit(
            &aoide_protocol::audit::default_audit_log(),
            aoide_protocol::Door::Cli,
            aoide_protocol::audit::EventClass::Audit,
            "mail.box.reap",
            "ok",
            &format!("reaping dead mailbox `{name}`: its session is gone and it holds no letters"),
        )
        .map_err(|e| format!("the audit log cannot be written, so the mailbox is kept: {e}"))?;
        std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(true)
}

/// Gives a session its mailbox on every roster insert, a local act like
/// [`box_add`], and records it as a live session box, so an insert after a reap
/// brings the box back. Best effort: a name the grammar refuses has no box and
/// no record, and the insert is not refused for it.
pub fn ensure_session_box(session_id: &str) {
    if !valid_box_name(session_id) {
        return;
    }
    let name = session_id.to_string();
    let recorded = with_lock(move || {
        create_box_unlocked(&name)?;
        let mut sessions = load_session_boxes()?;
        sessions.insert(name, false);
        save_session_boxes(&sessions)
    });
    if let Err(e) = recorded {
        eprintln!("[aoide/mail] session box `{session_id}` not recorded: {e}");
    }
}

/// `mail box add <name>`: creates the box `name`, empty, if it is absent, so a
/// far letter to it can file. Returns whether this call created it.
pub fn box_add(name: &str) -> Result<bool, String> {
    if !valid_box_name(name) {
        return Err(format!("invalid mailbox name `{name}`: must match ^[a-z0-9][a-z0-9-]*$, at most 63 bytes, and not a device name"));
    }
    let name = name.to_string();
    with_lock(move || create_box_unlocked(&name))
}

/// The legacy box file: letters whose name the grammar refuses.
pub fn legacy_box_path() -> PathBuf {
    box_path(LEGACY_BOX)
}

/// The file a letter to `to_name` lives in: `boxes/<to_name>.log`, or the
/// legacy box for a name the grammar refuses.
pub fn box_path(to_name: &str) -> PathBuf {
    let stem = if valid_box_name(to_name) { to_name } else { LEGACY_BOX };
    boxes_dir().join(format!("{stem}.log"))
}

/// Every box file that exists, sorted: `boxes/<name>.log` for a grammar name,
/// plus the legacy file. Anything else in the directory (an atomic-write temp,
/// a stray file) is not a box.
pub fn box_paths() -> Result<Vec<PathBuf>, String> {
    let dir = boxes_dir();
    let read = match std::fs::read_dir(&dir) {
        Ok(read) => read,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{}: {e}", dir.display())),
    };
    let mut out = Vec::new();
    for entry in read {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        let name = entry.file_name();
        let Some(stem) = name.to_str().and_then(|n| n.strip_suffix(".log")) else { continue };
        if stem == LEGACY_BOX || valid_box_name(stem) {
            out.push(entry.path());
        }
    }
    out.sort();
    Ok(out)
}

/// One line of a box as it sits on disk: its bytes without the newline, and the
/// entry it parses to (`None` for a line that does not parse, which is kept).
struct BoxLine {
    raw: Vec<u8>,
    entry: Option<Entry>,
}

impl BoxLine {
    /// The bytes this line takes on disk, its newline included.
    fn bytes(&self) -> u64 {
        self.raw.len() as u64 + 1
    }
}

/// Every line of one box file, in order. A missing file is no lines.
fn box_lines(path: &std::path::Path) -> Result<Vec<BoxLine>, String> {
    let raw = feed::read_lines(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(raw.into_iter().map(|raw| BoxLine { entry: serde_json::from_slice(&raw).ok(), raw }).collect())
}

/// The total bytes of every box file, from `stat` alone.
fn boxes_bytes() -> Result<u64, String> {
    let mut total = 0u64;
    for path in box_paths()? {
        total += std::fs::metadata(&path).map_err(|e| format!("{}: {e}", path.display()))?.len();
    }
    Ok(total)
}

/// The floor of `name`: the lowest mark among its readers, so every line at or
/// below it has been read by every reader. Zero when the name has no readers,
/// so nothing unread is ever reclaimed.
fn floor_of(cursors: &Cursors, name: &str) -> u64 {
    cursors.get(name).and_then(|readers| readers.values().map(|m| m.seq).min()).unwrap_or(0)
}

/// Rewrites one box without the lines every reader has read (at or below their
/// box's floor). A line that does not parse is kept. Writes nothing when no
/// line is reclaimable: the first parseable line's seq is checked first, so a
/// refused flood against an unread box costs one read and no write. Returns
/// whether it rewrote the box. Raw — assumes the caller holds the lock.
fn compact_unlocked(path: &std::path::Path, cursors: &Cursors) -> Result<bool, String> {
    let lines = box_lines(path)?;
    let reclaimable = |e: &Entry| e.seq <= floor_of(cursors, &e.envelope.header.to.name);
    let Some(first) = lines.iter().find_map(|l| l.entry.as_ref()) else {
        return Ok(false);
    };
    if !reclaimable(first) {
        return Ok(false);
    }
    let kept: Vec<&BoxLine> = lines.iter().filter(|l| l.entry.as_ref().is_none_or(|e| !reclaimable(e))).collect();
    if kept.len() == lines.len() {
        return Ok(false);
    }
    let mut body = Vec::new();
    for line in kept {
        body.extend_from_slice(&line.raw);
        body.push(b'\n');
    }
    atomic_write_private(path, &body).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(true)
}

/// [`compact_unlocked`] over every box file, in name order.
fn compact_every_box(cursors: &Cursors) -> Result<(), String> {
    for path in box_paths()? {
        compact_unlocked(&path, cursors)?;
    }
    Ok(())
}

/// The bytes [`admit`] reads for a line to `target` under `origin`: the target
/// box's length, the sender's bytes in it (`None` when there is no quarter),
/// and every box's total. Raw — assumes the caller holds the lock.
fn measure(target: &std::path::Path, origin: Option<&str>) -> Result<(u64, Option<Held>, u64), String> {
    let box_len = match std::fs::metadata(target) {
        Ok(m) => m.len(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => 0,
        Err(e) => return Err(format!("{}: {e}", target.display())),
    };
    let Some(key) = origin else {
        return Ok((box_len, None, boxes_bytes()?));
    };
    // The key's share of every box: one read of each box file, so a deposit
    // costs the store's size, bounded by the store cap (MAIL.md "Caps").
    let (mut in_box, mut in_store, mut boxes_len) = (0u64, 0u64, 0u64);
    for path in box_paths()? {
        boxes_len += std::fs::metadata(&path).map_err(|e| format!("{}: {e}", path.display()))?.len();
        let held: u64 = box_lines(&path)?
            .iter()
            .filter(|l| l.entry.as_ref().is_some_and(|e| e.origin == key))
            .map(BoxLine::bytes)
            .sum();
        in_store += held;
        if path == target {
            in_box = held;
        }
    }
    Ok((box_len, Some(Held { in_box, in_store }), boxes_len))
}

/// Whether one line of `line_len` bytes fits `target` under `caps`, compacting
/// first when a cap refuses it. A box-level or quarter refusal compacts the
/// target; a total refusal, box-wide or per key, then compacts every box.
/// Returns `false` when the line still does not fit. Raw — assumes the caller
/// holds the lock.
fn fit(caps: Caps, target: &std::path::Path, origin: Option<&str>, line_len: u64) -> Result<bool, String> {
    let check = |target: &std::path::Path| -> Result<Result<(), Full>, String> {
        let (box_len, held, boxes_len) = measure(target, origin)?;
        Ok(admit(caps, box_len, held, boxes_len, line_len))
    };
    if check(target)?.is_ok() {
        return Ok(true);
    }
    let cursors = load_cursors()?;
    compact_unlocked(target, &cursors)?;
    match check(target)? {
        Ok(()) => Ok(true),
        Err(Full::Total | Full::OriginTotal) => {
            compact_every_box(&cursors)?;
            Ok(check(target)?.is_ok())
        }
        Err(_) => Ok(false),
    }
}

/// The bytes one entry takes as a line, newline included, if it were filed now
/// with `origin`. Serialized with the seq [`peek_next_seq`] names.
fn line_bytes(envelope: &Envelope, via: &str, origin: &str) -> Result<u64, String> {
    let entry = Entry {
        seq: peek_next_seq()?,
        received_at: now_iso_utc(),
        kind: envelope.header.kind.clone(),
        via: via.to_string(),
        origin: origin.to_string(),
        envelope: envelope.clone(),
    };
    Ok(serde_json::to_vec(&entry).map_err(|e| e.to_string())?.len() as u64 + 1)
}

/// The serialized size of an envelope, as a letter's cap measures it.
fn envelope_len(envelope: &Envelope) -> usize {
    serde_json::to_vec(envelope).map_or(usize::MAX, |v| v.len())
}

/// Every correspondence entry on disk, in one global `seq` order: every box,
/// then both receipt generations. Raw: assumes the caller holds the lock, or
/// is a lock-free peek that accepts a line being appended beside it.
fn read_entries_unlocked() -> Result<Vec<Entry>, String> {
    let mut out = read_boxes_unlocked()?;
    out.extend(read_receipts_unlocked()?);
    out.sort_by_key(|e| e.seq);
    Ok(out)
}

/// Every letter in every box, unsorted. Receipts are not here: a receipt names
/// a sender as its `to`, never a mailbox a reader can open.
fn read_boxes_unlocked() -> Result<Vec<Entry>, String> {
    let mut out = Vec::new();
    for path in box_paths()? {
        out.extend(read_file_entries(&path)?);
    }
    Ok(out)
}

/// Every receipt on both generations of `receipts.log`, unsorted.
fn read_receipts_unlocked() -> Result<Vec<Entry>, String> {
    let mut out = Vec::new();
    for path in receipt_generations() {
        out.extend(read_rotating_entries(&path)?);
    }
    Ok(out)
}

/// Every letter in every box, in `seq` order, under the lock. The bare-mail
/// and `--all-names` listings read this: they name mailboxes, not senders.
pub fn read_boxes() -> Result<Vec<Entry>, String> {
    with_lock(|| {
        let mut out = read_boxes_unlocked()?;
        out.sort_by_key(|e| e.seq);
        Ok(out)
    })
}

/// Every receipt filed on this box, in `seq` order, under the lock: the only
/// reader of `receipts.log`. A receipt is never a letter a reader pulls.
pub fn read_receipts() -> Result<Vec<Entry>, String> {
    with_lock(|| {
        let mut out = read_receipts_unlocked()?;
        out.sort_by_key(|e| e.seq);
        Ok(out)
    })
}

/// The highest `seq` any line on disk names, in the boxes, the receipts, and an
/// unmigrated `base.jsonl`. A line of any shape counts, so an older binary's
/// line is never reissued. Raw — assumes the caller holds the lock.
fn tail_seq_unlocked() -> Result<u64, String> {
    #[derive(Deserialize)]
    struct SeqOnly {
        seq: u64,
    }
    let mut paths = box_paths()?;
    paths.extend(receipt_generations());
    paths.push(base_path());
    let mut max = 0;
    for path in paths {
        for line in feed::read_lines(&path).map_err(|e| format!("{}: {e}", path.display()))? {
            if let Ok(s) = serde_json::from_slice::<SeqOnly>(&line) {
                max = max.max(s.seq);
            }
        }
    }
    Ok(max)
}

/// The seq of the last complete line of `base.jsonl`, the one file an older
/// binary appends to, or 0 when there is none or its last line does not parse.
/// Reads backward from the end in 64 KiB chunks, so filing never reads the
/// whole file. Raw — assumes the caller holds the lock.
fn base_tail_seq_unlocked() -> Result<u64, String> {
    use std::io::{Read, Seek, SeekFrom};
    #[derive(Deserialize)]
    struct SeqOnly {
        seq: u64,
    }
    let seq_of = |line: &[u8]| serde_json::from_slice::<SeqOnly>(line).map_or(0, |s| s.seq);
    const CHUNK: u64 = 64 * 1024;
    let path = base_path();
    let mut file = match std::fs::File::open(&path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let io = |e: std::io::Error| format!("{}: {e}", path.display());
    let mut end = file.metadata().map_err(io)?.len();
    let mut buf: Vec<u8> = Vec::new();
    loop {
        let start = end.saturating_sub(CHUNK);
        let mut chunk = vec![0u8; (end - start) as usize];
        file.seek(SeekFrom::Start(start)).map_err(io)?;
        file.read_exact(&mut chunk).map_err(io)?;
        chunk.append(&mut buf);
        buf = chunk;
        end = start;
        // Bytes after the last newline are a torn record; the complete line
        // before them is the tail, and it may begin in an earlier chunk.
        match buf.iter().rposition(|&b| b == b'\n') {
            None if start == 0 => return Ok(0),
            None => {}
            Some(last) => {
                let body = &buf[..last];
                match body.iter().rposition(|&b| b == b'\n') {
                    Some(i) => return Ok(seq_of(&body[i + 1..])),
                    None if start == 0 => return Ok(seq_of(body)),
                    None => {}
                }
            }
        }
    }
}

/// The next global sequence number, issued under the lock (P4). The counter
/// file is the source of truth, and it is never reset by a prune: a line an
/// older binary appended to `base.jsonl` is counted too (`max` with that file's
/// tail), so no number is ever issued twice. Box and receipt lines are not
/// re-read here: the counter already covers them. A missing counter starts from
/// the highest seq any line or mark names (`seq_floor`). The new value is
/// written durably before the caller appends the line it numbers, so a crash
/// leaves a gap and never a reuse. A counter at `u64::MAX` is refused, never
/// wrapped. Raw — assumes the caller already holds the lock.
fn next_seq() -> Result<u64, String> {
    let seq = peek_next_seq()?;
    let path = seq_path();
    ensure_mail_dir()?;
    atomic_write_private(&path, format!("{seq}\n").as_bytes()).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(seq)
}

/// The seq [`next_seq`] would issue, written nowhere. A caller sizes a line
/// with it before it commits to filing. Raw — assumes the caller holds the lock.
fn peek_next_seq() -> Result<u64, String> {
    let path = seq_path();
    let counter = match std::fs::read_to_string(&path) {
        Ok(raw) => raw
            .trim()
            .parse::<u64>()
            .map_err(|_| format!("{}: not a sequence number", path.display()))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => seq_floor()?,
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    counter
        .max(base_tail_seq_unlocked()?)
        .checked_add(1)
        .ok_or_else(|| format!("{}: the sequence counter is u64::MAX; fix: remove the line numbered {} under state/mail/", path.display(), u64::MAX))
}

/// The floor a missing counter starts from: the highest seq of any line, or of
/// any mark or rung. Raw — assumes the caller holds the lock.
fn seq_floor() -> Result<u64, String> {
    let marks = load_cursors()?.values().flat_map(|c| c.values()).map(|m| m.seq.max(m.rung)).max().unwrap_or(0);
    Ok(tail_seq_unlocked()?.max(marks))
}

fn seq_path() -> PathBuf {
    mail_dir().join("seq")
}

/// The one routing rule for a filed entry: a receipt goes to `receipts.log`,
/// every other kind to the box its `to.name` names.
fn route(kind: &str, to_name: &str) -> PathBuf {
    if kind == ENTRY_TYPE_RECEIPT {
        receipts_log_path()
    } else {
        box_path(to_name)
    }
}

/// Append one entry to the file its kind and name route it to, through the
/// capped-log primitive (which cuts a torn tail and fsyncs). A receipt rotates
/// `receipts.log` first when it has reached [`ROTATE_AT`]. Raw — assumes the
/// caller holds the lock.
fn append_entry(entry: &Entry) -> Result<(), String> {
    let line = serde_json::to_string(entry).map_err(|e| e.to_string())?;
    let path = route(&entry.kind, &entry.envelope.header.to.name);
    ensure_mail_dir()?;
    crate::fs::ensure_write_dir(&boxes_dir()).map_err(|e| format!("{}: {e}", boxes_dir().display()))?;
    if entry.kind == ENTRY_TYPE_RECEIPT {
        rotate_before_append(&path, ROTATE_AT, line.len() + 1)?;
    }
    feed::append_line(&path, line.as_bytes()).map_err(|e| format!("{}: {e}", path.display()))
}

/// The index a filed receipt is keyed on: the signer, the mesh the receipt is
/// signed in, and the msgid of the letter it acknowledges. Its own msgid is not
/// in it, since a re-ack is a new envelope.
fn receipt_key(envelope: &Envelope) -> String {
    dedup_key(&envelope.header.from.node, &envelope.header.origin_mesh, &envelope.text)
}

/// The key a far acknowledgement is deduplicated on, and a re-ack is limited
/// on: the origin node, the mesh, and the acked msgid. [`receipt_key`] is this
/// key over a receipt's text.
fn dedup_key(origin_node: &str, origin_mesh: &str, acked_msgid: &str) -> String {
    serde_json::to_string(&(origin_node, origin_mesh, acked_msgid)).unwrap_or_default()
}

/// How long one re-ack per key is held back, in seconds.
pub const REACK_WINDOW_SECS: u64 = 3600;
/// The cap on `reacks.log`, which rotates to `.1` past it.
pub const REACKS_MAX: u64 = 1024 * 1024;

/// One line of `reacks.log`: the last time an ack for `key` was spooled.
#[derive(Serialize, Deserialize)]
struct ReAck {
    key: String,
    at: u64,
}

fn reacks_log_path() -> PathBuf {
    mail_dir().join("reacks.log")
}

/// The last `at` recorded for `key` across both generations, if any. Each
/// generation is read up to [`REACKS_MAX`].
fn last_reack_unlocked(key: &str) -> Result<Option<u64>, String> {
    let mut last: Option<u64> = None;
    for path in [reacks_log_path(), PathBuf::from(format!("{}.1", reacks_log_path().display()))] {
        let lines = feed::read_lines_up_to(&path, REACKS_MAX).map_err(|e| format!("{}: {e}", path.display()))?;
        for line in lines {
            if let Ok(r) = serde_json::from_slice::<ReAck>(&line) {
                if r.key == key {
                    last = Some(last.map_or(r.at, |l| l.max(r.at)));
                }
            }
        }
    }
    Ok(last)
}

fn record_reack_unlocked(key: &str, at: u64) -> Result<(), String> {
    let line = serde_json::to_string(&ReAck { key: key.to_string(), at }).map_err(|e| e.to_string())?;
    let path = reacks_log_path();
    ensure_mail_dir()?;
    rotate_before_append(&path, REACKS_MAX, line.len() + 1)?;
    feed::append_line(&path, line.as_bytes()).map_err(|e| format!("{}: {e}", path.display()))
}

/// Records that an ack for this letter was spooled at `now` (epoch seconds),
/// so a duplicate of it inside [`REACK_WINDOW_SECS`] is held back.
pub fn note_ack(origin_node: &str, origin_mesh: &str, acked_msgid: &str, now: u64) -> Result<(), String> {
    with_lock(|| record_reack_unlocked(&dedup_key(origin_node, origin_mesh, acked_msgid), now))
}

/// Whether a duplicate letter's ack goes out now: at most one per key per
/// [`REACK_WINDOW_SECS`], so N replays inside the window spool one ack between
/// them. When it is due the spool is recorded under the same lock, so two
/// concurrent duplicates cannot both pass.
pub fn reack_due(origin_node: &str, origin_mesh: &str, acked_msgid: &str, now: u64) -> Result<bool, String> {
    with_lock(|| {
        let key = dedup_key(origin_node, origin_mesh, acked_msgid);
        if let Some(at) = last_reack_unlocked(&key)? {
            if now.saturating_sub(at) < REACK_WINDOW_SECS {
                return Ok(false);
            }
        }
        record_reack_unlocked(&key, now)?;
        Ok(true)
    })
}

/// Every receipt key already filed, across both generations of `receipts.log`.
/// The read grows with the letters this box has sent, never with the boxes.
fn read_receipt_keys_unlocked() -> Result<HashSet<String>, String> {
    let mut keys = HashSet::new();
    for path in receipt_generations() {
        for entry in read_rotating_entries(&path)? {
            keys.insert(receipt_key(&entry.envelope));
        }
    }
    Ok(keys)
}

fn load_cursors() -> Result<Cursors, String> {
    load_stage(&cursors_path())
}
fn save_cursors(c: &Cursors) -> Result<(), String> {
    write_stage(&cursors_path(), c)
}

/// The reader identity a cursor mutation is attributed to: `reader_session`
/// when given and non-empty, and `name` itself otherwise (MAIL.md "Store":
/// "a reader is identified by its conducting session id... and by the
/// mailbox name itself when that is unset — so a stray read from an
/// unconducted terminal advances a pseudo-reader and never a live agent's
/// mark"). Every caller resolves this once per name, before looking at any
/// entry under it — never re-derived per row.
fn reader_id(name: &str, reader_session: Option<&str>) -> String {
    match reader_session {
        Some(s) if !s.is_empty() => session_reader_key(s),
        _ => name.to_string(),
    }
}

/// The prefix every session reader's cursor key carries. A session's own box
/// is named by its session id, so a bare session id as a reader key would be
/// the box's pseudo-reader key (the name itself): an unconducted read would
/// then consume the live session's mark. `:` is refused by the box grammar
/// ([`valid_box_name`]), so no box name is a session key and no session key is
/// a box name.
const SESSION_KEY_PREFIX: &str = "session:";

/// The cursor key a session reads under. Every session reader key is made
/// here; callers pass the bare session id.
pub fn session_reader_key(session: &str) -> String {
    format!("{SESSION_KEY_PREFIX}{session}")
}

/// The session id behind a reader key, or `None` for the pseudo-reader.
fn session_of_key(key: &str) -> Option<&str> {
    key.strip_prefix(SESSION_KEY_PREFIX)
}

/// A reader key under `name` as it must read after the migration: the
/// pseudo-reader (`name` itself) and keys already prefixed stay as they are,
/// and any other reader is a bare session id that gains the prefix.
fn migrated_reader_key(name: &str, reader: String) -> String {
    if reader == name || reader.starts_with(SESSION_KEY_PREFIX) {
        reader
    } else {
        session_reader_key(&reader)
    }
}

/// Mint + sign + file one entry (letter or receipt) ORIGINATING on this box
/// — `via` names the hop that deposited it, `"self"` for everything this
/// box mints for itself (P-M1's every caller; a remotely-deposited envelope
/// is filed by [`file_received_entry`] instead, which skips minting
/// entirely since the envelope already arrived signed). The one place
/// [`append_base_line`] and [`append_seen_line`] are called together, in
/// that order. Raw — called only from inside [`with_lock`]'s closure
/// (`file_letter`/`file_receipt`/[`migrate_if_needed`]'s own inline copy of
/// this same shape).
fn file_entry(kind: &str, from: Address, to: Address, text: &str, via: &str) -> Result<Entry, String> {
    let (kp, _) = identity::load_or_mint().map_err(|e| e.to_string())?;
    let header = Header {
        version: ENVELOPE_VERSION.to_string(),
        from,
        to,
        kind: kind.to_string(),
        minted_at: now_iso_utc(),
        origin_mesh: String::new(),
    };
    let (sig, msgid) = seal(&header, text, &kp);
    let envelope = Envelope { header, text: text.to_string(), sig, msgid: msgid.clone() };
    if kind == ENTRY_TYPE_LETTER {
        // A local letter has no quarter, but it has the box and store caps.
        check_letter_size(&envelope)?;
        let target = box_path(&envelope.header.to.name);
        if !fit(CAPS, &target, None, line_bytes(&envelope, via, "")?)? {
            return Err(box_full_teaching(&envelope.header.to.name));
        }
    }
    let entry = Entry {
        seq: next_seq()?,
        received_at: now_iso_utc(),
        kind: kind.to_string(),
        via: via.to_string(),
        origin: String::new(),
        envelope,
    };
    append_entry(&entry)?;
    Ok(entry)
}

/// File an envelope that arrived ALREADY SEALED over the wire (P-M2,
/// `aoide/mailDeposit`) — the received-mail counterpart to [`file_entry`]:
/// no minting, no signing, the envelope's own `sig`/`msgid` are filed
/// verbatim exactly as the origin produced them. `via` is the HOP's
/// resolved name (`ctx.signed_caller`, never a name read out of the
/// envelope itself — the door already resolved this by key before calling
/// here). Raw — called only from inside [`with_lock`]'s closure
/// ([`deposit`]'s own).
fn file_received_entry(envelope: Envelope, via: &str, origin: &str) -> Result<Entry, String> {
    let entry = Entry {
        seq: next_seq()?,
        received_at: now_iso_utc(),
        kind: envelope.header.kind.clone(),
        via: via.to_string(),
        origin: origin.to_string(),
        envelope,
    };
    append_entry(&entry)?;
    Ok(entry)
}

/// A letter over [`LETTER_MAX`] serialized is refused at the mint and at the door.
fn check_letter_size(envelope: &Envelope) -> Result<(), String> {
    let len = envelope_len(envelope);
    if len > LETTER_MAX {
        return Err(format!(
            "the letter is {len} bytes serialized, over the {LETTER_MAX}-byte cap; send less text, or split it across letters"
        ));
    }
    Ok(())
}

/// The error a local send to a full box returns: what is full, and the two
/// ways out of it.
fn box_full_teaching(name: &str) -> String {
    format!(
        "mailbox `{name}` cannot take this letter: its box or the mail store is full. Read it with `aoide mail read --for {name}`, or remove it with `aoide mail box rm {name} --force`, then send again"
    )
}

/// File one delivery receipt — `mail::file_receipt`, the direct replacement
/// for the two `inbox::receive` writer seams (`conduct/graph/send.rs`,
/// `server/a2a.rs`). **Three arguments, not `inbox::receive`'s four**:
/// `context` is dropped, not carried (ruling 7) — the canonical header
/// stays the closed eight fields, and the retiring field was a reserved
/// passthrough nothing ever set or read. Signed by this box's own identity,
/// minting it on first use if none exists yet (ruling 6); best-effort by
/// design at BOTH call sites, same as before — a failed filing must never
/// turn an already-succeeded delivery into a reported failure, which is why
/// this still returns a real `Result` rather than swallowing the error
/// itself: the caller decides whether/how to surface it. Always `via:
/// "self"` — both call sites file a LOCAL delivery, never a remote one.
pub fn file_receipt(from: &str, to_name: &str, text: &str) -> Result<(), String> {
    let node = display::local_node_name();
    let from_addr = Address { node: node.clone(), name: from.to_string() };
    let to_addr = Address { node, name: to_name.to_string() };
    let stored = local_receipt_text(text);
    with_lock(move || file_entry(ENTRY_TYPE_RECEIPT, from_addr, to_addr, &stored, "self").map(|_| ()))
}

/// What a local receipt keeps of the message it acknowledges: its sha256 and
/// byte length, never the text. A `send` receipt's message is on its conduct
/// audit line. The doorbell receipt has no audit line, and its text is only the
/// fixed nudge, so its digest covers the nudge and nothing more. Nothing reads
/// a local receipt's text back.
fn local_receipt_text(text: &str) -> String {
    format!("sha256:{} {}B", hex_encode(&Sha256::digest(text.as_bytes())), text.len())
}

/// File one letter locally — `mail send --to self/<name>`'s engine, and the
/// same-node-loopback path of a `--to <node>/<name>` send whose `<node>` is
/// this box's own name (P-M2's `--to` resolution happens one layer up, in
/// the command handler). Always `via: "self"`.
pub fn file_letter(from_name: &str, to_name: &str, text: &str) -> Result<Entry, String> {
    if !valid_box_name(to_name) {
        return Err("invalid mailbox name: must match ^[a-z0-9][a-z0-9-]*$, at most 63 bytes, and not a device name".to_string());
    }
    let node = display::local_node_name();
    let from_addr = Address { node: node.clone(), name: from_name.to_string() };
    let to_addr = Address { node, name: to_name.to_string() };
    let text = text.to_string();
    let box_name = to_name.to_string();
    with_lock(move || {
        create_box_unlocked(&box_name)?;
        file_entry(ENTRY_TYPE_LETTER, from_addr, to_addr, &text, "self")
    })
}

/// File a letter ADDRESSED TO A REMOTE NODE — mints and signs exactly like
/// [`file_letter`] (this box IS the origin), but the caller supplies
/// `to_node` directly rather than this box's own name, since the whole
/// point is a `to` that names somewhere else. Returns the signed
/// [`Envelope`] (not an [`Entry`] — nothing is filed into THIS box's own
/// mailbase; a letter to another node is never also a local copy) for the
/// caller to hand to the outbox. `mail send`'s command layer is what
/// decides self vs. remote and calls the matching one of these two.
/// **The mesh-aware mint** (P-CHARTER, review finding 1). `origin_mesh` is
/// SIGNED into the header (`seal`), so it cannot be patched after the fact —
/// and it is one of the two signed values the receiver compares
/// (`seal::deposit_container`: the container's `ctx.origin_mesh` against the
/// mesh the request's own signature covers). A letter minted in the home mesh
/// while its request is signed for another mesh is therefore undepositable,
/// which is exactly the bug this parameter fixes: the SENDER's mint and the
/// SENDER's signing derive the mesh from one rule and cannot disagree.
pub fn mint_outbound_letter_in_mesh(
    from_name: &str,
    to_node: &str,
    to_name: &str,
    text: &str,
    mesh: &str,
) -> Result<Envelope, String> {
    mint_kind(ENTRY_TYPE_LETTER, from_name, to_node, to_name, text, mesh)
}

/// [`mint_outbound_letter_in_mesh`] with the SENDER's node name stated explicitly
/// — the name the letter is signed as, which for a charter mesh is the name the
/// charter gives this box's identity key and not its OS hostname
/// (`routing::own_name_in`). `from.node` is inside the signed header, so the
/// receiver's origin check and every hop's entry-1 check read it: a box whose
/// hostname is not its charter line's name must be able to say so here.
pub fn mint_outbound_letter_from(
    from_node: &str,
    from_name: &str,
    to_node: &str,
    to_name: &str,
    text: &str,
    mesh: &str,
) -> Result<Envelope, String> {
    mint_kind_from(ENTRY_TYPE_LETTER, from_node, from_name, to_node, to_name, text, mesh)
}

/// Mint the local envelope a `charter` letter spools under (P-CHARTER).
///
/// The envelope is **bookkeeping only**: a charter container's payload is the
/// charter file and its `.sig`, never this envelope, so nothing on the far end
/// ever sees these bytes. What the entry needs from it is its `kind` (the drain
/// retires a delivered charter like a receipt) and its `msgid` (the spool file's
/// name) — and both sides of a charter are operators, which is the free-text
/// attribution `to.name`/`from.name` carry. `mesh` is the CHARTER's mesh: a
/// charter letter is origin-signed for the mesh it carries, and never the
/// home mesh by default — the letter that roots a mesh from outside cannot
/// be minted in a mesh its receiver has never heard of.
pub fn mint_charter_letter(to_node: &str, text: &str, mesh: &str) -> Result<Envelope, String> {
    mint_kind(ENTRY_TYPE_CHARTER, "operator", to_node, "operator", text, mesh)
}

/// The one mint every typed letter goes through — `from_name` is the sender's
/// mailbox (a session id for a letter, `operator` for a charter), and `kind`
/// is the only field that differs between them. `mesh` is the letter's signed
/// `origin_mesh`, the value the receiver compares against the mesh the
/// request's own signature names.
fn mint_kind(
    kind: &str,
    from_name: &str,
    to_node: &str,
    to_name: &str,
    text: &str,
    mesh: &str,
) -> Result<Envelope, String> {
    mint_kind_from(kind, &display::local_node_name(), from_name, to_node, to_name, text, mesh)
}

/// [`mint_kind`] with `from.node` stated: the one body both entry points share,
/// so the two can never disagree about what a mint signs.
fn mint_kind_from(
    kind: &str,
    from_node: &str,
    from_name: &str,
    to_node: &str,
    to_name: &str,
    text: &str,
    mesh: &str,
) -> Result<Envelope, String> {
    if !crate::node_store::valid_node_name(to_name) {
        return Err("invalid mailbox name: must match ^[a-z0-9][a-z0-9-]*$".to_string());
    }
    let (kp, _) = identity::load_or_mint().map_err(|e| e.to_string())?;
    let header = Header {
        version: ENVELOPE_VERSION.to_string(),
        from: Address { node: from_node.to_string(), name: from_name.to_string() },
        to: Address { node: to_node.to_string(), name: to_name.to_string() },
        kind: kind.to_string(),
        minted_at: now_iso_utc(),
        origin_mesh: mesh.to_string(),
    };
    let (sig, msgid) = seal(&header, text, &kp);
    let envelope = Envelope { header, text: text.to_string(), sig, msgid };
    if kind == ENTRY_TYPE_LETTER {
        check_letter_size(&envelope)?;
    }
    Ok(envelope)
}

/// The mesh a letter or ack to `node` is minted in when the caller names
/// none: the destination's sole granted mesh, else the home mesh — the same
/// rule the request-signing path applies
/// (`aoide_storage::node_store::resolve_mesh`, over the destination record's
/// granted meshes). `""` (UNNAMED) only when the destination is trusted in
/// more than one mesh and the home mesh is NOT one of them, which is
/// `resolve_mesh`'s own refusal: more than one candidate and no home to break
/// the tie, so a mint cannot guess which mesh the request will be signed for.
/// A destination trusted in several meshes WITH the home mesh among them
/// resolves to the home mesh, not to `""`. The operator's lever for the
/// refused case is `--mesh` on the sending command, which mints through
/// [`mint_outbound_letter_in_mesh`] outright.
pub fn mesh_for_node(node: &str) -> String {
    let record = crate::node_store::load_nodes().into_iter().find(|p| p.name == node);
    let grants = record.as_ref().map(|p| p.grants.clone()).unwrap_or_default();
    crate::node_store::resolve_mesh(None, &grants, &crate::config::home_mesh(), "message").unwrap_or_default()
}

/// Mint a letter for the mesh this box derives for its destination
/// ([`mesh_for_node`]) — the plain entry point every caller that has no
/// `--mesh` to honour uses, tests included. `mail send` goes through
/// [`mint_outbound_letter_in_mesh`] instead, because it has an operator's
/// `--mesh` to apply.
pub fn mint_outbound_letter(from_name: &str, to_node: &str, to_name: &str, text: &str) -> Result<Envelope, String> {
    let mesh = mesh_for_node(to_node);
    mint_outbound_letter_in_mesh(from_name, to_node, to_name, text, &mesh)
}

/// [`mint_ack`]'s mesh-aware form, the same rule [`mint_outbound_letter_in_mesh`]
/// states: an ack rides ONE mesh back to the origin, and the mesh its header
/// names is a SIGNED value the destination compares against the request's.
/// `spool_and_drain_ack` passes the incoming letter's own `origin_mesh`, so an
/// ack travels in the mesh the letter arrived in. `from.node` is the OS
/// hostname here; a caller whose box is named differently in that mesh uses
/// [`mint_ack_from`].
pub fn mint_ack_in_mesh(from_name: &str, to: Address, acked_msgid: &str, mesh: &str) -> Result<Envelope, String> {
    mint_ack_from(&display::local_node_name(), from_name, to, acked_msgid, mesh)
}

/// [`mint_ack_in_mesh`] with the SENDER's node name stated explicitly, the
/// receipt's twin of [`mint_outbound_letter_from`]: the origin checks the ack
/// against the key it holds for `from.node`, so a box whose hostname is not its
/// charter name must sign the ack as that name or the origin never retires its
/// letter. Both mints share this body, so a letter and an ack can never
/// disagree about what a mint signs.
pub fn mint_ack_from(
    from_node: &str,
    from_name: &str,
    to: Address,
    acked_msgid: &str,
    mesh: &str,
) -> Result<Envelope, String> {
    let (kp, _) = identity::load_or_mint().map_err(|e| e.to_string())?;
    let header = Header {
        version: ENVELOPE_VERSION.to_string(),
        from: Address { node: from_node.to_string(), name: from_name.to_string() },
        to,
        kind: ENTRY_TYPE_RECEIPT.to_string(),
        minted_at: now_iso_utc(),
        origin_mesh: mesh.to_string(),
    };
    let (sig, msgid) = seal(&header, acked_msgid, &kp);
    Ok(Envelope { header, text: acked_msgid.to_string(), sig, msgid })
}

/// Mint a receipt envelope ACKing `acked_msgid` back to `to` — the ack this
/// box spools to its own outbox once it has filed someone else's letter
/// (MAIL.md, spec item 6: "acks are ordinary envelopes, type=receipt,
/// to=origin, text=acked msgid, signed by destination"). `from_name` is the
/// mailbox that received the letter (the identity doing the acking);
/// `to` is the original letter's own `header.from` (origin, not hop —
/// P-M2's two lookups stay distinct even here). Not filed into this box's
/// own mailbase (an ack is outbound-only until IT is deposited somewhere,
/// same as [`mint_outbound_letter`]). The mesh comes from the origin's own
/// record ([`mesh_for_node`]); the ack-while-filing path names the incoming
/// letter's mesh outright ([`mint_ack_in_mesh`]).
pub fn mint_ack(from_name: &str, to: Address, acked_msgid: &str) -> Result<Envelope, String> {
    let mesh = mesh_for_node(&to.node);
    mint_ack_in_mesh(from_name, to, acked_msgid, &mesh)
}

/// Verify `envelope`'s origin signature against the ONE key this box has on
/// record for `header.from.node` — a DIFFERENT question from
/// `a2a::verify_signed_request`'s "which verified node's key verifies this
/// connection," and deliberately not reused for it: this tries EXACTLY the one
/// key on record under the CLAIMED origin name, never every verified key, so a
/// paired node signing as another paired node's name is refused rather than
/// silently verifying and filing under the wrong identity. Collapses "no node
/// named `from.node`," "that node has no recorded key," and "the key on record
/// doesn't verify" into the SAME `false` — same non-oracle discipline
/// `verify_signed_request`'s own doc states for its ladder ("never an existence
/// oracle over the registry").
///
/// **The one key is the DECLARATION's for the mesh the letter names**
/// (`header.origin_mesh`): a charter mesh's line for that name, which is the only
/// place a node this box never paired with has a key at all — and the name the
/// charter gave it, so a letter signed by a charter line's name verifies here and
/// one signed by a nickname does not. Where that mesh has no declaration, the
/// paired RECORD answers, exactly as it did before the declarations existed. The
/// SEALED lane calls [`verify_origin_signature_in`] with the set its own deposit
/// already loaded, so the container's two halves cannot read two different sets.
pub fn verify_origin_signature(envelope: &Envelope) -> bool {
    verified_origin_key(envelope).is_some()
}

/// The key that verifies `envelope`'s origin signature, the one a deposit is
/// quartered under. `None` when no key on record for `from.node` in its mesh
/// verifies it, or when the declarations will not load.
pub fn verified_origin_key(envelope: &Envelope) -> Option<String> {
    let set = crate::routing::declarations().ok()?;
    verified_origin_key_in(envelope, &set)
}

/// [`verify_origin_signature`] over a declaration set the caller already holds —
/// what a SEALED deposit uses, so one deposit reads the declarations once and
/// its two signature checks cannot disagree about which set they read.
pub fn verify_origin_signature_in(envelope: &Envelope, set: &[crate::routing::Loaded]) -> bool {
    verified_origin_key_in(envelope, set).is_some()
}

/// [`verified_origin_key`] over a declaration set the caller already holds.
pub fn verified_origin_key_in(envelope: &Envelope, set: &[crate::routing::Loaded]) -> Option<String> {
    let pubkey_hex = crate::routing::key_in(set, &envelope.header.origin_mesh, &envelope.header.from.node)?;
    let mut sig_input = canonical_header_bytes(&envelope.header);
    sig_input.push(0u8);
    sig_input.extend_from_slice(envelope.text.as_bytes());
    crate::wire_auth::verify_signature_hex(&pubkey_hex, &sig_input, &envelope.sig).then_some(pubkey_hex)
}

/// The outcome of [`deposit`]'s policy chain, once the caller has already
/// cleared admission (verified + `message` — the door's job, before ever
/// calling here). The zone check (MAIL.md step 3) belongs to the transit lane,
/// which this plaintext lane never rides: skipped by shape, not stubbed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DepositOutcome {
    /// A fresh envelope, filed. `msgid` is its own, for the caller to ack
    /// (a letter) or simply acknowledge process (a receipt).
    Filed { msgid: String, kind: String },
    /// A `msgid` already in `seen.jsonl`. `filed_letter` is `true` when the
    /// ORIGINAL filing was a `letter` — the destination re-spools its ack
    /// in that case (spec item 5: "a lost ack is the ordinary reason for a
    /// re-offer"), never for a duplicate receipt (nothing acks an ack).
    Duplicate { filed_letter: bool },
    /// `msgid` does not recompute from `(header, text, sig)` — tampered or
    /// corrupt in transit.
    BadMsgid,
    /// A verified `receipt` whose `text` is not exactly one msgid
    /// ([`is_msgid`]). It names no letter this box could retire, so it files
    /// nothing. Answered on the wire as `bad-msgid`, with its own detail.
    MalformedAck,
    /// [`verify_origin_signature`] returned `false`.
    UnverifiedOrigin,
    /// A kind other than `letter` or `receipt` arrived as plaintext: a `charter`
    /// is applied from a sealed container and never filed; any other kind is
    /// not correspondence ([`REFUSAL_NOT_CORRESPONDENCE`]).
    NotCorrespondence,
    /// A letter to a name with no box on this node, or to a dead box ([`is_dead`]:
    /// its every reader was retired). Nothing is filed and no box is created: a
    /// far deposit never creates one ([`REFUSAL_UNKNOWN_BOX`]), and the origin
    /// keeps its copy.
    UnknownBox,
    /// A letter over [`LETTER_MAX`] serialized. Nothing is read or filed.
    TooLarge,
    /// The box has no room for this letter under one of [`admit`]'s caps, even
    /// after compaction. Nothing is filed or acknowledged ([`REFUSAL_BOX_FULL`]).
    BoxFull,
}

// L2 (the branch review): there was a `Refused { reason, detail }` variant
// here, "carrying the taught word a container refusal names". Nothing ever
// constructed it — a plaintext envelope has no shape to carry a refusal word,
// and the container path answers `seal::ContainerOutcome::Refused`, a
// different type entirely — so it and its two match arms were dead. A
// variant that cannot be built is worse than no variant: it reads as a
// reachable branch to every later reader.

/// The `kind` of the entry filed under `msgid`, if one is filed — the
/// **filed record** is the truth about filing, never a gate's own memory.
/// A duplicate container consults this so its answer survives a crash
/// between the filing and any dedup write (MAIL.md, "A crash between base
/// and seen re-accepts exactly once").
pub fn filed_kind(msgid: &str) -> Option<String> {
    show(msgid).ok().flatten().map(|entry| entry.kind)
}

/// `aoide/mailDeposit`'s policy chain from "msgid recomputes" onward (spec
/// item 4) — admission (verified caller holding `message`) already ran in
/// the door, above this. Runs entirely under [`with_lock`]: recompute,
/// verify, dedup, and file are all local-disk checks, never network I/O, so
/// holding the lock across all four costs nothing a caller need avoid
/// (contrast the OUTBOX drain, which must never hold this lock across an
/// ssh dial). `via` is the hop's resolved name (`ctx.signed_caller`).
pub fn deposit(envelope: Envelope, via: &str) -> Result<DepositOutcome, String> {
    deposit_within(envelope, via, CAPS)
}

/// [`deposit`] under `caps`. A test passes a small one; the store's own is [`CAPS`].
pub(crate) fn deposit_within(envelope: Envelope, via: &str, caps: Caps) -> Result<DepositOutcome, String> {
    with_lock(move || {
        let Some(recomputed) = compute_msgid(&envelope.header, &envelope.text, &envelope.sig) else {
            return Ok(DepositOutcome::BadMsgid);
        };
        if recomputed != envelope.msgid {
            return Ok(DepositOutcome::BadMsgid);
        }
        let Some(origin) = verified_origin_key(&envelope) else {
            return Ok(DepositOutcome::UnverifiedOrigin);
        };
        // Only a letter and a receipt are filed. A `charter` is APPLIED from the
        // sealed container it arrives in, never filed, and any other kind is not
        // correspondence at all. Refused before the dedup read or any write.
        let kind = envelope.header.kind.as_str();
        if kind != ENTRY_TYPE_LETTER && kind != ENTRY_TYPE_RECEIPT {
            return Ok(DepositOutcome::NotCorrespondence);
        }
        // A receipt names the one letter it acknowledges, and nothing else. A
        // verified signer's receipt whose text is anything but one msgid names
        // no letter this box can retire, so it is refused as a bad msgid.
        if kind == ENTRY_TYPE_RECEIPT && !is_msgid(&envelope.text) {
            return Ok(DepositOutcome::MalformedAck);
        }
        // A receipt files only for a letter this box still holds, addressed to the
        // name that letter went out as. Anything else names no letter this box can
        // retire, so it files nothing, and is answered `duplicate`: nothing is
        // filed and nothing is owed, so the sender retires its copy instead of
        // parking it. This bounds the receipts to the letters this box has sent: a
        // flood of distinct msgids is refused before any write.
        if kind == ENTRY_TYPE_RECEIPT {
            if crate::outbox::spooled_letter_sender(&envelope.text)?.as_deref() != Some(envelope.header.to.node.as_str()) {
                return Ok(DepositOutcome::Duplicate { filed_letter: false });
            }
            // A signer acknowledges a letter once, per mesh: a second receipt from the
            // same signer, in the same mesh, for the same acked msgid files nothing. A
            // receipt in another mesh is a different acknowledgement and is not conflated.
            if read_receipt_keys_unlocked()?.contains(&receipt_key(&envelope)) {
                return Ok(DepositOutcome::Duplicate { filed_letter: false });
            }
            let kind = kind.to_string();
            let msgid = envelope.msgid.clone();
            file_received_entry(envelope, via, &origin)?;
            return Ok(DepositOutcome::Filed { msgid, kind });
        }
        // A letter: the size cap first, then the box must exist, then the box is
        // the dedup memory, then the caps. None of it writes until it fits.
        if envelope_len(&envelope) > LETTER_MAX {
            return Ok(DepositOutcome::TooLarge);
        }
        if !box_accepts(&envelope.header.to.name, &load_cursors()?, &load_session_boxes()?) {
            return Ok(DepositOutcome::UnknownBox);
        }
        let target = box_path(&envelope.header.to.name);
        if let Some(found) = box_lines(&target)?
            .into_iter()
            .filter_map(|l| l.entry)
            .find(|e| e.envelope.msgid == envelope.msgid)
        {
            return Ok(DepositOutcome::Duplicate { filed_letter: found.kind == ENTRY_TYPE_LETTER });
        }
        // The quarter is the verified origin key's, and only this box's own key
        // has none: a hop's name never decides it.
        let quarter = (!is_own_key(&origin)).then_some(origin.as_str());
        if !fit(caps, &target, quarter, line_bytes(&envelope, via, quarter.unwrap_or(""))?)? {
            return Ok(DepositOutcome::BoxFull);
        }
        let kind = kind.to_string();
        let msgid = envelope.msgid.clone();
        file_received_entry(envelope, via, quarter.unwrap_or(""))?;
        Ok(DepositOutcome::Filed { msgid, kind })
    })
}

/// Whether `key` is this box's own identity key, the one key a letter has no
/// quarter under. A missing identity is no own key, so the quarter applies.
fn is_own_key(key: &str) -> bool {
    identity::load_existing().ok().flatten().is_some_and(|kp| kp.info().pubkey_hex == key)
}

/// One mailbox's correspondence entries, read under the lock: the file a letter
/// to `name` lands in, which is the legacy box for a name the grammar refuses.
pub fn read_box(name: &str) -> Result<Vec<Entry>, String> {
    let name = name.to_string();
    with_lock(move || read_file_entries(&box_path(&name)))
}

/// The whole base, tolerantly parsed, in file order. The one PUBLIC,
/// locked reader — everything else in this module that needs entries calls
/// the raw [`read_entries_unlocked`] instead, from inside its own
/// [`with_lock`] closure.
pub fn read_base() -> Result<Vec<Entry>, String> {
    with_lock(read_entries_unlocked)
}

/// `cursors.json` read with no lock, no migration and no write: the reader
/// maps, each name's pseudo-reader (the key equal to the name) dropped, and a
/// name left with no reader omitted. A missing file is an empty map, since
/// nobody is enrolled. `None` when the file is there but not in the per-reader
/// shape: a legacy flat file that only a locked read migrates, or one that does
/// not parse. A caller reports that as unknown, never as none.
pub fn peek_cursors() -> Option<Cursors> {
    let raw = match std::fs::read_to_string(cursors_path()) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Some(Cursors::new()),
        Err(_) => return None,
    };
    let top: BTreeMap<String, serde_json::Value> = serde_json::from_str(&raw).ok()?;
    let mut out = Cursors::new();
    for (name, value) in top {
        // The migration's own test for the old flat shape, verbatim.
        if value.get("seq").and_then(serde_json::Value::as_u64).is_some() {
            return None;
        }
        let cursor: Cursor = serde_json::from_value(value).ok()?;
        let readers: Cursor = cursor.into_iter().filter(|(reader, _)| *reader != name).collect();
        if !readers.is_empty() {
            out.insert(name, readers);
        }
    }
    Some(out)
}

/// `base.jsonl`'s correspondence entries, read with no lock and no write. Only
/// the bytes up to the last newline count: the write path treats a tail with no
/// newline as torn and truncates it, so a peek never reports a line the next
/// write would remove. Transit and malformed lines are skipped, as
/// [`read_entries_unlocked`] skips them. A missing file is empty.
pub fn peek_base() -> Result<Vec<Entry>, String> {
    read_entries_unlocked()
}

/// The letters `name` holds that are UNREAD BY `reader_session`, plus that
/// reader's own mark — a PEEK. **Writes nothing**: no `save_cursors`, no mark
/// created for a reader that has none (absent reads as `0`, the same floor
/// `names_with_unread` uses), no `read_for`, no `mark`. The caller's own
/// cursors are therefore untouched, and two observers watching one mailbox
/// cannot consume each other's mail — the read-only half of the cursor
/// contract, on the reader axis (`read_base` is cursor-free but reader-blind;
/// `names_with_unread` is reader-aware but entry-blind; this is both).
pub fn unread_for(name: &str, reader_session: Option<&str>) -> Result<(u64, Vec<Entry>), String> {
    let name = name.to_string();
    let reader_session = reader_session.map(|s| s.to_string());
    with_lock(move || {
        let entries = read_entries_unlocked()?;
        let cursors = load_cursors()?;
        let reader = reader_id(&name, reader_session.as_deref());
        let mark = cursors
            .get(&name)
            .and_then(|cursor| cursor.get(&reader))
            .map(|m| m.seq)
            .unwrap_or(0);
        let mut matched: Vec<Entry> = entries
            .into_iter()
            .filter(|e| e.envelope.header.to.name == name && e.seq > mark)
            .collect();
        matched.sort_by_key(|e| e.seq);
        Ok((mark, matched))
    })
}

/// `mail read --for <name> [--reread]`: entries filed to `name`, newest
/// last, advancing ONLY the calling reader's mark under `name` to the
/// highest `seq` returned (`--reread` widens what is PRINTED — every entry
/// for `name`, not just the unread ones — but the mark still only ever
/// advances forward, so a plain `mail read --for X` right after never
/// reprints what `--reread` just showed). A second reader's mark under the
/// same name is untouched, and it still sees every entry it has not itself
/// read (MAIL.md "Store": "a read advances only the caller's... two agents
/// sharing a mailbox never consume each other's mail").
pub fn read_for(name: &str, reread: bool, reader_session: Option<&str>) -> Result<Vec<Entry>, String> {
    let name = name.to_string();
    let reader_session = reader_session.map(|s| s.to_string());
    with_lock(move || {
        if !box_path(&name).exists() {
            return Ok(Vec::new());
        }
        let entries = read_file_entries(&box_path(&name))?;
        let mut cursors = load_cursors()?;
        let reader = reader_id(&name, reader_session.as_deref());
        let cursor = cursors.entry(name.clone()).or_default();
        let mark = cursor.entry(reader).or_default();
        let floor = if reread { 0 } else { mark.seq };
        let mut matched: Vec<Entry> =
            entries.into_iter().filter(|e| e.envelope.header.to.name == name && e.seq > floor).collect();
        matched.sort_by_key(|e| e.seq);
        if let Some(max_seq) = matched.iter().map(|e| e.seq).max() {
            if max_seq > mark.seq {
                mark.seq = max_seq;
            }
        }
        save_cursors(&cursors)?;
        Ok(matched)
    })
}

/// `mail read --all-names [--reread]`: the same as [`read_for`], run once
/// per name that appears anywhere in the base, results concatenated in
/// `seq` order. Each name's reader identity is resolved separately (an
/// unconducted caller's pseudo-reader differs per name, since it falls back
/// to the name itself), but only ever once per name — never per entry.
pub fn read_all_names(reread: bool, reader_session: Option<&str>) -> Result<Vec<Entry>, String> {
    let reader_session = reader_session.map(|s| s.to_string());
    with_lock(move || {
        let entries = read_boxes_unlocked()?;
        let mut cursors = load_cursors()?;
        let mut names: Vec<String> =
            entries.iter().map(|e| e.envelope.header.to.name.clone()).collect();
        names.sort();
        names.dedup();

        let mut out = Vec::new();
        for name in names {
            let reader = reader_id(&name, reader_session.as_deref());
            let cursor = cursors.entry(name.clone()).or_default();
            let mark = cursor.entry(reader).or_default();
            let floor = if reread { 0 } else { mark.seq };
            let mut matched: Vec<Entry> = entries
                .iter()
                .filter(|e| e.envelope.header.to.name == name && e.seq > floor)
                .cloned()
                .collect();
            if let Some(max_seq) = matched.iter().map(|e| e.seq).max() {
                if max_seq > mark.seq {
                    mark.seq = max_seq;
                }
            }
            out.append(&mut matched);
        }
        out.sort_by_key(|e| e.seq);
        save_cursors(&cursors)?;
        Ok(out)
    })
}

/// `mail mark --for <name>`: advance ONLY the caller's mark under `name` to
/// the current max `seq`, without printing anything. Returns that mark's
/// resulting `seq`. A name with nothing filed under it yet is a clean no-op
/// (the mark stays at 0), not an error — same "a mailbox exists by being
/// named" tolerance MAIL.md's "Addressing and filing" section states for
/// `mail send`/`mail read` alike.
pub fn mark(name: &str, reader_session: Option<&str>) -> Result<u64, String> {
    let name = name.to_string();
    let reader_session = reader_session.map(|s| s.to_string());
    with_lock(move || {
        if !box_path(&name).exists() {
            return Ok(0);
        }
        let entries = read_file_entries(&box_path(&name))?;
        let max_seq = entries
            .iter()
            .filter(|e| e.envelope.header.to.name == name)
            .map(|e| e.seq)
            .max()
            .unwrap_or(0);
        let mut cursors = load_cursors()?;
        let reader = reader_id(&name, reader_session.as_deref());
        let cursor = cursors.entry(name.clone()).or_default();
        let reader_mark = cursor.entry(reader).or_default();
        if max_seq > reader_mark.seq {
            reader_mark.seq = max_seq;
        }
        let result = reader_mark.seq;
        save_cursors(&cursors)?;
        Ok(result)
    })
}

/// `mail show <msgid>`: one entry by its exact `msgid`, or `None` — no
/// cursor effect (this is a peek, not a read; MAIL.md's Commands table
/// lists it without the "print + advance cursor" annotation `mail read`
/// carries).
pub fn show(msgid: &str) -> Result<Option<Entry>, String> {
    let msgid = msgid.to_string();
    with_lock(move || Ok(read_entries_unlocked()?.into_iter().find(|e| e.envelope.msgid == msgid)))
}

/// `aoide mail`'s bare listing: names with mail unread BY THIS READER
/// (MAIL.md ruling 11 — the "caller's own new letters" half needs the
/// reader binding and is P-M5's; this is the names half only). A peek, like
/// [`show`] — it never mutates a cursor, only compares against the
/// caller's own mark under each name.
pub fn names_with_unread(reader_session: Option<&str>) -> Result<Vec<String>, String> {
    let reader_session = reader_session.map(|s| s.to_string());
    with_lock(move || {
        let entries = read_boxes_unlocked()?;
        let cursors = load_cursors()?;
        let mut max_by_name: BTreeMap<String, u64> = BTreeMap::new();
        for e in &entries {
            let slot = max_by_name.entry(e.envelope.header.to.name.clone()).or_insert(0);
            if e.seq > *slot {
                *slot = e.seq;
            }
        }
        let mut names: Vec<String> = max_by_name
            .into_iter()
            .filter(|(name, max_seq)| {
                let reader = reader_id(name, reader_session.as_deref());
                let read_through = cursors.get(name).and_then(|c| c.get(&reader)).map(|m| m.seq).unwrap_or(0);
                read_through < *max_seq
            })
            .map(|(name, _)| name)
            .collect();
        names.sort();
        Ok(names)
    })
}

/// One doorbell poll's targets for `name` (MAIL.md "Delivery and the
/// doorbell") — the ringer's one read of "who do I wake, and which real
/// readers even exist to wake." `armed` pairs each armed reader with the
/// arming high-water mark it is armed FOR, so a caller's own
/// [`stamp_rung`] has the exact `seq` to latch without a second scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RingTargets {
    /// (reader key, highest arming seq) for every reader that is armed.
    pub armed: Vec<(String, u64)>,
    /// Reader keys under this name other than the pseudo-reader — the full
    /// enrolment roster, armed or merely latched. There is no separate count
    /// field: the count is `.len()`.
    pub enrolled: Vec<String>,
}

/// Who is armed under `name`, and which real (non-pseudo) readers are
/// enrolled at all (MAIL.md "Delivery and the doorbell"). The pseudo-reader
/// (cursor key == `name` itself, the identity an unconducted caller reads
/// under) is never armed and never counted in `enrolled` — it is nobody's
/// terminal to ring, and its presence must never suppress the ringer's
/// petname fallback. A reader is armed once unread arming mail ([`arms`],
/// currently `letter` only) exists beyond both what it has read (`mark.seq`)
/// and what it was last rung for (`mark.rung`): rung once, a reader stays
/// latched until its own read catches up to that point, and re-arms only on
/// the next arming entry after that. Read-only — whether an enrolled reader
/// is still alive is a session-store question this module cannot answer;
/// that liveness filtering belongs to the caller.
pub fn ring_targets(name: &str) -> Result<RingTargets, String> {
    let name = name.to_string();
    with_lock(move || {
        let entries = read_file_entries(&box_path(&name))?;
        let max_arm = entries
            .iter()
            .filter(|e| e.envelope.header.to.name == name && arms(&e.kind))
            .map(|e| e.seq)
            .max()
            .unwrap_or(0);
        let cursors = load_cursors()?;
        let mut armed = Vec::new();
        let mut enrolled = Vec::new();
        if let Some(cursor) = cursors.get(&name) {
            for (reader, mark) in cursor {
                let Some(session) = session_of_key(reader) else { continue };
                enrolled.push(session.to_string());
                if max_arm > mark.seq && mark.rung <= mark.seq {
                    armed.push((session.to_string(), max_arm));
                }
            }
        }
        Ok(RingTargets { armed, enrolled })
    })
}

/// Latch `reader`'s ring for `name` at `seq` (MAIL.md "Delivery and the
/// doorbell") — the ringer's own write, made only once a nudge has actually
/// reached a socket and never before (a failed write must leave the reader
/// armed for the next trigger). `rung` only ever advances (`rung =
/// max(rung, seq)`); `seq` — the reader's own read mark — is never touched
/// here, since reading and being rung are two different events on the same
/// [`Mark`]. `reader` must already be a key under `name`'s cursor map: this
/// stamps an existing mark, it never inserts one — an unenrolled reader is
/// an error, never a silent enrolment. [`enrol_reader`] is the seam that
/// enrols.
pub fn stamp_rung(name: &str, reader: &str, seq: u64) -> Result<(), String> {
    let name = name.to_string();
    let reader = reader.to_string();
    with_lock(move || {
        let mut cursors = load_cursors()?;
        let mark = cursors
            .get_mut(&name)
            .and_then(|c| c.get_mut(&session_reader_key(&reader)))
            .ok_or_else(|| format!("{reader}: not an enrolled reader of {name}"))?;
        if seq > mark.rung {
            mark.rung = seq;
        }
        save_cursors(&cursors)?;
        Ok(())
    })
}

/// Every name where `reader` (a session/wrap id) is currently armed, paired
/// with the arming high-water mark it is armed for, sorted by name — the
/// Stop-hook replay's own read (MAIL.md "Delivery and the doorbell"): the
/// reader that just went idle asks "what should ring me now" across every
/// mailbox at once, not one name at a time. The same armed rule as
/// [`ring_targets`]; a name under which `reader` is that name's OWN
/// pseudo-reader (`reader == name`) is excluded there exactly as it would
/// be from that name's own [`ring_targets`] call.
pub fn armed_names_for_reader(reader: &str) -> Result<Vec<(String, u64)>, String> {
    let reader = reader.to_string();
    with_lock(move || {
        let cursors = load_cursors()?;
        let mut out = Vec::new();
        let key = session_reader_key(&reader);
        for (name, cursor) in &cursors {
            if name == &reader {
                continue;
            }
            let Some(mark) = cursor.get(&key) else { continue };
            let max_arm = read_file_entries(&box_path(name))?
                .iter()
                .filter(|e| &e.envelope.header.to.name == name && arms(&e.kind))
                .map(|e| e.seq)
                .max()
                .unwrap_or(0);
            if max_arm > mark.seq && mark.rung <= mark.seq {
                out.push((name.clone(), max_arm));
            }
        }
        Ok(out)
    })
}

/// Enrol `reader` under `name` with a fresh, unread, unrung [`Mark`] —
/// idempotent, and never moves a mark already there (MAIL.md "Delivery and
/// the doorbell"): the ringer's petname fallback calls this on its resolved
/// target BEFORE stamping, so a fallback target latches on its next ring
/// exactly like any reader that arrived by reading. Refuses `reader ==
/// name` (the pseudo-reader is never enrolled — it is not a target) and an
/// invalid `name`; writes nothing in either case.
pub fn enrol_reader(name: &str, reader: &str) -> Result<(), String> {
    if !valid_box_name(name) {
        return Err("invalid mailbox name: must match ^[a-z0-9][a-z0-9-]*$, at most 63 bytes, and not a device name".to_string());
    }
    if reader == name {
        return Err(format!("{reader}: the pseudo-reader is never enrolled"));
    }
    let name = name.to_string();
    let reader = reader.to_string();
    with_lock(move || {
        create_box_unlocked(&name)?;
        let mut cursors = load_cursors()?;
        cursors.entry(name).or_default().entry(session_reader_key(&reader)).or_default();
        save_cursors(&cursors)?;
        Ok(())
    })
}

/// Retires every reader key that is neither a session in `live` nor its name's
/// own pseudo-reader (MAIL.md "Store"). A name left with no reader keeps an
/// empty reader map, which is what makes its box dead ([`is_dead`]): a far
/// letter to it is refused, and [`reap_dead_boxes`] may delete it once it is
/// empty. A dead reader's mark no longer holds its box's floor down, so the
/// box's read lines become reclaimable on the next compaction. Writes
/// `cursors.json` only when a key leaves, and touches nothing when there is no
/// `cursors.json`, so the reaper never creates a mail store. Returns the retired
/// `(name, reader)` pairs, in name order.
pub fn retire_readers(live: &std::collections::HashSet<&str>) -> Result<Vec<(String, String)>, String> {
    if !cursors_path().exists() && !session_boxes_path().exists() {
        return Ok(Vec::new());
    }
    with_lock(|| {
        let mut retired = Vec::new();
        if cursors_path().exists() {
            let mut cursors = load_cursors()?;
            for (name, readers) in cursors.iter_mut() {
                readers.retain(|reader, _| {
                    let session = session_of_key(reader);
                    let keep = reader == name || session.is_some_and(|id| live.contains(id));
                    if !keep {
                        retired.push((name.clone(), session.unwrap_or(reader).to_string()));
                    }
                    keep
                });
            }
            if !retired.is_empty() {
                save_cursors(&cursors)?;
            }
        }
        // The roster decides each session box's liveness, and nothing else does:
        // a box whose session is not on the roster is dead, one that is, is live.
        let mut sessions = load_session_boxes()?;
        let mut changed = false;
        for (name, dead) in sessions.iter_mut() {
            let now_dead = !live.contains(name.as_str());
            if *dead != now_dead {
                *dead = now_dead;
                changed = true;
            }
        }
        if changed {
            save_session_boxes(&sessions)?;
        }
        Ok(retired)
    })
}

/// Cross-process serializer for the ring (`conduct::graph::doorbell`,
/// MAIL.md "Delivery and the doorbell"): `flock`s a DEDICATED `.ring.lock`
/// file in [`mail_dir`] for the whole closure — never the `.stage.lock`
/// [`with_lock`]'s callers take. A ring holds this lock across real socket
/// I/O (one write per armed target, plus the submit-keystroke delay), which
/// can run for tens of milliseconds per target; every OTHER mail primitive
/// in this module holds `.stage.lock` only for an in-memory read-modify-write
/// of `base.jsonl`/`cursors.json`, microseconds at most. Sharing one lock
/// file would make a single slow ring stall every unrelated stage writer on
/// the desktop (session starts, hook phases, the reaper) for its whole
/// duration — a hazard nothing before the ring created, so the ring gets its
/// own file rather than widening `.stage.lock`'s blast radius.
///
/// Blocking (`LOCK_EX`): a second filer's ring queues behind the first
/// rather than racing it, so two letters filed to the same name in quick
/// succession still ring the target at most once each, in order (P-M5a-2
/// "a concurrent burst of filers rings once"). Fail-closed like
/// [`crate::fs::try_stage_lock`]: if the lock file can't be opened or
/// locked, the ring is refused rather than run unlocked.
pub fn with_ring_lock<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    crate::fs::lock_path(mail_dir().join(".ring.lock"), f)
}

/// What `mail box rm` removed: the letters in the box's log and the reader
/// marks under its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoxRemoval {
    pub entries: usize,
    pub readers: usize,
}

/// `mail box rm <name> [--force]`: removes the box's log and its cursor keys.
/// `_legacy` is removable by its own name, the one refused name that is.
/// Refused while any letter in the box is unread by its readers (every reader
/// on the name, the pseudo-reader included, since a role box is read under its
/// own name) or while nobody has read it and it holds mail, unless `force`. The
/// removal is written to the audit log first, and an audit line that cannot be
/// written stops it: no box is removed without its line. A name with no box is
/// refused, and nothing is created.
pub fn box_rm(name: &str, force: bool) -> Result<BoxRemoval, String> {
    if name != LEGACY_BOX && !valid_box_name(name) {
        return Err(format!("invalid mailbox name `{name}`: must match ^[a-z0-9][a-z0-9-]*$, at most 63 bytes, and not a device name"));
    }
    let name = name.to_string();
    with_lock(move || {
        let path = box_path(&name);
        if !path.exists() {
            return Err(format!("no mailbox named `{name}`"));
        }
        let entries = read_file_entries(&path)?;
        let mut cursors = load_cursors()?;
        let readers = cursors.get(&name).cloned().unwrap_or_default();
        let top = entries.iter().map(|e| e.seq).max().unwrap_or(0);
        let unread = if readers.is_empty() { !entries.is_empty() } else { readers.values().any(|m| m.seq < top) };
        if unread && !force {
            return Err(format!(
                "mailbox `{name}` holds mail no reader has read; read it first, or pass --force to remove it with its unread letters"
            ));
        }
        let removal = BoxRemoval { entries: entries.len(), readers: readers.len() };
        aoide_protocol::audit::audit(
            &aoide_protocol::audit::default_audit_log(),
            aoide_protocol::Door::Cli,
            aoide_protocol::audit::EventClass::Audit,
            "mail.box.rm",
            "ok",
            &format!("removing mailbox `{name}`: {} letters, {} reader marks, force={force}", removal.entries, removal.readers),
        )
        .map_err(|e| format!("the audit log cannot be written, so the mailbox is kept: {e}"))?;
        if let Err(e) = std::fs::remove_file(&path) {
            // The audit line above already says the removal began: this one says it
            // did not finish, so the log never reads as removed when it is not.
            let _ = aoide_protocol::audit::audit(
                &aoide_protocol::audit::default_audit_log(),
                aoide_protocol::Door::Cli,
                aoide_protocol::audit::EventClass::Audit,
                "mail.box.rm",
                "failed",
                &format!("removal of mailbox `{name}` failed after its audit line: {e}"),
            );
            return Err(format!("{}: {e}", path.display()));
        }
        cursors.remove(&name);
        save_cursors(&cursors)?;
        Ok(removal)
    })
}

/// `mail rm --older-than <Nd|Nh>`: the only pruning (MAIL.md "Store",
/// "Keep-all"). Rewrites `base.jsonl` atomically with the survivors,
/// preserving each one's original `seq` — never renumbers, so a reader's
/// cursor comparisons stay valid across a prune. Never touches
/// `seen.jsonl`: a pruned letter re-offered later is still a duplicate
/// (the RFC 5537 §3.3 coupling MAIL.md names). An entry whose
/// `receivedAt` fails to parse is KEPT, not guessed at.
pub fn rm_older_than(max_age_secs: u64) -> Result<usize, String> {
    with_lock(move || {
        let cutoff = parse_iso_utc(&now_iso_utc()).unwrap_or(0) - max_age_secs as i64;
        let mut dropped = 0;
        let mut paths = box_paths()?;
        paths.extend(receipt_generations());
        for path in paths {
            let lines = feed::read_lines(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let mut body: Vec<u8> = Vec::new();
            let mut changed = false;
            for raw in lines {
                // A line that does not parse is kept as it is: only a letter or a
                // receipt the age test drops is ever removed.
                let old = serde_json::from_slice::<Entry>(&raw)
                    .ok()
                    .and_then(|e| parse_iso_utc(&e.received_at))
                    .is_some_and(|epoch| epoch < cutoff);
                if old {
                    dropped += 1;
                    changed = true;
                    continue;
                }
                body.extend_from_slice(&raw);
                body.push(b'\n');
            }
            if changed {
                atomic_write_private(&path, &body).map_err(|e| format!("{}: {e}", path.display()))?;
            }
        }
        Ok(dropped)
    })
}

/// `--older-than <Nd|Nh>`: a positive integer followed by exactly one unit
/// letter, `d` (days) or `h` (hours) — `"7d"`, `"12h"`. Returns the
/// threshold in seconds; anything else (empty, no digits, a zero count, a
/// third unit, trailing junk) is `None`, a usage error before anything
/// runs. Ported verbatim from `rice/src/commands/history.rs::parse_older_than`
/// (architect's ruling 10) rather than shared: `storage` may not depend on
/// `rice`, and the two crates live in different workspaces after the
/// crate split.
pub fn parse_older_than(raw: &str) -> Option<u64> {
    if raw.len() < 2 {
        return None;
    }
    let (digits, unit) = raw.split_at(raw.len() - 1);
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let n: u64 = digits.parse().ok()?;
    if n == 0 {
        return None;
    }
    match unit {
        "d" => Some(n * 86400),
        "h" => Some(n * 3600),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(tag: &str) -> (aoide_test_support::EnvSaver, PathBuf) {
        aoide_test_support::isolated_mail_root(tag)
    }

    /// A session's own box is named by its session id. Its read as the session
    /// moves the session's key only, and an unconducted read afterwards still
    /// sees the letter: the pseudo-reader never took it.
    #[test]
    fn an_own_box_read_moves_the_session_and_never_the_pseudo_reader() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, _dir) = root("mail-own-box");
        file_letter("alice", "sess-a", "for the session").unwrap();
        assert_eq!(read_for("sess-a", false, Some("sess-a")).unwrap().len(), 1);
        let cursors = load_cursors().unwrap();
        assert_eq!(cursors["sess-a"].get("sess-a"), None, "the pseudo-reader's mark moved: {cursors:?}");
        assert_eq!(read_for("sess-a", false, None).unwrap().len(), 1, "an unconducted read lost the letter");
    }

    /// The one-shot migration prefixes every bare session reader and leaves the
    /// pseudo-reader and already-prefixed keys alone; a second run is a no-op.
    #[test]
    fn the_reader_key_migration_prefixes_sessions_once() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, _dir) = root("mail-reader-migrate");
        std::fs::create_dir_all(mail_dir()).unwrap();
        let legacy = serde_json::json!({
            "box-x": { "box-x": { "seq": 1, "rung": 0 }, "sess-b": { "seq": 3, "rung": 0 } },
            "box-y": { "session:sess-c": { "seq": 2, "rung": 0 } }
        });
        std::fs::write(cursors_path(), legacy.to_string()).unwrap();
        with_lock(migrate_cursors_if_needed).unwrap();
        let once = load_cursors().unwrap();
        assert_eq!(once["box-x"]["box-x"].seq, 1, "the pseudo-reader keeps its key");
        assert_eq!(once["box-x"]["session:sess-b"].seq, 3, "a bare session key gains the prefix");
        assert!(!once["box-x"].contains_key("sess-b"), "{once:?}");
        assert_eq!(once["box-y"]["session:sess-c"].seq, 2, "a prefixed key is left as it is");
        with_lock(migrate_cursors_if_needed).unwrap();
        assert_eq!(load_cursors().unwrap(), once, "a second migration changed the cursors");
    }

    /// Reaping retires a session key that left the roster and keeps a live one,
    /// with the key stored under its prefix.
    #[test]
    fn retire_readers_retires_a_session_key_that_left_the_roster() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, _dir) = root("mail-reap-keys");
        file_letter("alice", "box-r", "x").unwrap();
        enrol_reader("box-r", "live-s").unwrap();
        enrol_reader("box-r", "gone-s").unwrap();
        let live: std::collections::HashSet<&str> = ["live-s"].into_iter().collect();
        let retired = retire_readers(&live).unwrap();
        assert_eq!(retired, vec![("box-r".to_string(), "gone-s".to_string())]);
        let cursors = load_cursors().unwrap();
        assert!(cursors["box-r"].contains_key("session:live-s"), "{cursors:?}");
        assert!(!cursors["box-r"].contains_key("session:gone-s"), "{cursors:?}");
    }

    /// Each raw mail writer locks a wide `state/` before it creates or appends
    /// under it, even with the mail dir already present. Each is widened and
    /// checked on its own, so a writer that skips the gate fails by name.
    #[cfg(unix)]
    #[test]
    fn mail_writes_lock_a_wide_state_they_write_into() {
        use std::os::unix::fs::PermissionsExt;
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("mail-wide-state");
        let state = crate::fs::state_dir();
        std::fs::create_dir_all(mail_dir()).unwrap();
        let mode = || std::fs::metadata(&state).unwrap().permissions().mode() & 0o777;
        let widen = || std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o755)).unwrap();

        std::fs::create_dir_all(boxes_dir()).unwrap();
        widen();
        append_entry(&letter_entry("bob", 1, "first")).unwrap();
        assert_eq!(mode(), 0o700, "append_entry left state/ wide");

        widen();
        std::fs::write(box_path("bob"), "{\"v\":0}\n{\"v\":").unwrap();
        append_entry(&letter_entry("bob", 2, "after the tear")).unwrap();
        assert_eq!(mode(), 0o700, "a torn-tail cut left state/ wide");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn a_legacy_inbox_with_no_rows_locks_state_before_it_renames_itself() {
        use std::os::unix::fs::PermissionsExt;
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("migrate-wide-state");
        let state = crate::fs::state_dir();
        // Mint first: a fresh key locks state/ on its own, which would hide the
        // rename. With the key in place, only the rename can lock it.
        identity::load_or_mint().unwrap();
        std::fs::write(legacy_inbox_path(), r#"{"entries":[]}"#).unwrap();
        std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o755)).unwrap();

        migrate_if_needed().unwrap();

        assert!(state.join("inbox.json.migrated").exists(), "the legacy inbox was not renamed");
        assert_eq!(
            std::fs::metadata(&state).unwrap().permissions().mode() & 0o777,
            0o700,
            "migrate_if_needed renamed inside a wide state/"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A signed letter from `alice` to `to`, at `seq`, filed the way a deposit files
    /// it: [`append_entry`] routes it to its box.
    fn letter_entry(to: &str, seq: u64, text: &str) -> Entry {
        let h = header("alice", to, ENTRY_TYPE_LETTER, &now_iso_utc());
        let (kp, _) = identity::load_or_mint().unwrap();
        let (sig, msgid) = seal(&h, text, &kp);
        Entry {
            seq,
            received_at: now_iso_utc(),
            kind: ENTRY_TYPE_LETTER.to_string(),
            via: "self".to_string(),
            origin: String::new(),
            envelope: Envelope { header: h, text: text.to_string(), sig, msgid },
        }
    }

    fn header(from: &str, to: &str, text_kind: &str, minted_at: &str) -> Header {
        Header {
            version: ENVELOPE_VERSION.to_string(),
            from: Address { node: "yomi".to_string(), name: from.to_string() },
            to: Address { node: "yomi".to_string(), name: to.to_string() },
            kind: text_kind.to_string(),
            minted_at: minted_at.to_string(),
            origin_mesh: String::new(),
        }
    }

    #[test]
    fn no_fixture_escapes_the_test_root() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("no-escape");

        file_letter("alice", "bob", "hi").unwrap();

        assert!(base_path().starts_with(&dir), "base.jsonl must land under the isolated root");
        assert!(
            !base_path().to_string_lossy().contains("/.aoide/"),
            "must never resolve into a real ~/.aoide tree"
        );

        let envelope = mint_outbound_letter("alice", "elsewhere", "carol", "hi").unwrap();
        crate::outbox::write_entry("elsewhere", &crate::outbox::OutboxEntry::fresh(envelope)).unwrap();
        let outbox_dir = crate::outbox::outbox_dir();
        assert!(outbox_dir.starts_with(&dir), "state/outbox must land under the isolated root");
        assert!(
            !outbox_dir.to_string_lossy().contains("/.aoide/"),
            "must never resolve into a real ~/.aoide tree"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── peeks: the lock-free reads the find index is built from ──────────

    #[test]
    fn a_peek_drops_the_pseudo_reader_and_keeps_the_real_readers() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("peek-pseudo");
        std::fs::create_dir_all(mail_dir()).unwrap();
        std::fs::write(
            cursors_path(),
            r#"{"alice":{"alice":{"seq":2},"sess-1":{"seq":2,"rung":1}},"bob":{"bob":{"seq":1}}}"#,
        )
        .unwrap();

        let want = Cursors::from([(
            "alice".to_string(),
            Cursor::from([("sess-1".to_string(), Mark { seq: 2, rung: 1 })]),
        )]);
        assert_eq!(
            peek_cursors(),
            Some(want),
            "the pseudo-reader is nobody, so bob, whose only key is that, is absent"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_cursors_file_is_no_readers_and_a_legacy_one_is_unknown() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("peek-missing-legacy");
        assert_eq!(peek_cursors(), Some(Cursors::new()), "nobody is enrolled yet");

        std::fs::create_dir_all(mail_dir()).unwrap();
        std::fs::write(cursors_path(), r#"{"alice":{"seq":3,"readers":["sess-1"]}}"#).unwrap();
        assert_eq!(peek_cursors(), None, "the flat shape is unknown here, never an empty map");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_peek_never_writes_and_never_migrates_a_legacy_file() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("peek-untouched");
        file_letter("alice", "bob", "hi").unwrap();
        std::fs::write(cursors_path(), r#"{"alice":{"seq":3,"readers":["sess-1"]}}"#).unwrap();

        // An old mtime, so any write since would move it to now.
        let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
        let before: Vec<_> = [box_path("bob"), cursors_path()]
            .into_iter()
            .map(|p| {
                std::fs::OpenOptions::new().write(true).open(&p).unwrap().set_modified(old).unwrap();
                let bytes = std::fs::read(&p).unwrap();
                let mtime = std::fs::metadata(&p).unwrap().modified().unwrap();
                (p, bytes, mtime)
            })
            .collect();

        assert_eq!(peek_cursors(), None);
        assert_eq!(peek_base().unwrap().len(), 1);
        for (p, bytes, mtime) in &before {
            assert_eq!(&std::fs::read(p).unwrap(), bytes, "{} bytes changed", p.display());
            assert_eq!(std::fs::metadata(p).unwrap().modified().unwrap(), *mtime, "{} was written", p.display());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_peek_skips_a_torn_last_line_that_is_otherwise_complete_json() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("peek-torn");
        file_letter("alice", "bob", "one").unwrap();
        let line = std::fs::read_to_string(box_path("bob")).unwrap();
        let torn = line.trim_end_matches('\n').to_string();
        assert!(serde_json::from_str::<Entry>(&torn).is_ok(), "the tail parses: only the missing newline makes it torn");
        std::fs::write(box_path("bob"), format!("{line}{torn}")).unwrap();

        let entries = peek_base().unwrap();
        assert_eq!(entries.len(), 1, "a tail with no newline is not part of the file yet");
        assert_eq!(entries[0].seq, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_peek_takes_no_lock_and_completes_while_the_stage_lock_is_held() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("peek-no-lock");
        file_letter("alice", "bob", "hi").unwrap();

        crate::fs::try_stage_lock(|| {
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send((peek_base().map(|e| e.len()), peek_cursors().is_some()));
            });
            let got = rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .expect("a peek must not wait on the stage lock");
            assert_eq!(got, (Ok(1), true));
        })
        .unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_plaintext_charter_envelope_is_refused_and_never_filed() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("plaintext-charter");

        // A peer this node has paired with, holding `message` — so the
        // envelope clears origin verification and reaches the kind check.
        let kp = identity::mint_ephemeral().unwrap();
        let mut nodes = Vec::new();
        crate::node_store::upsert_paired_node(
            &mut nodes,
            "alice-host",
            "https://alice",
            &kp.info().pubkey_hex,
            "2026-09-06T00:00:00Z",
            &["message".to_string()],
            "home",
        );
        crate::node_store::save_nodes(&nodes).unwrap();

        let mut h = header("alice", "bob", ENTRY_TYPE_CHARTER, "2026-09-06T00:00:00Z");
        h.from.node = "alice-host".to_string();
        let text = "charter home v1";
        let (sig, msgid) = seal(&h, text, &kp);
        let envelope = Envelope { header: h, text: text.to_string(), sig, msgid };

        assert_eq!(
            deposit(envelope.clone(), "alice-host").unwrap(),
            DepositOutcome::NotCorrespondence,
            "a charter is applied from a sealed container; a plaintext one has nothing to apply"
        );
        assert!(read_base().unwrap().is_empty(), "nothing is filed");
        assert!(
            filed_kind(&envelope.msgid).is_none(),
            "and nothing is recorded as filed — the filed record is the truth about filing"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn origin_verification_is_bound_to_the_key_on_record_for_from_node() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("origin-verification");

        let alice_kp = identity::mint_ephemeral().unwrap();
        let mallory_kp = identity::mint_ephemeral().unwrap();
        let mut nodes = Vec::new();
        crate::node_store::upsert_paired_node(
            &mut nodes,
            "alice-host",
            "https://alice",
            &alice_kp.info().pubkey_hex,
            "2026-09-06T00:00:00Z",
            &["message".to_string()],
            "home",
        );
        crate::node_store::upsert_paired_node(
            &mut nodes,
            "mallory-host",
            "https://mallory",
            &mallory_kp.info().pubkey_hex,
            "2026-09-06T00:00:00Z",
            &["message".to_string()],
            "home",
        );
        crate::node_store::save_nodes(&nodes).unwrap();

        let mut genuine_header = header("alice", "bob", ENTRY_TYPE_LETTER, "2026-09-06T00:00:00Z");
        genuine_header.from.node = "alice-host".to_string();
        let (genuine_sig, genuine_msgid) = seal(&genuine_header, "hello", &alice_kp);
        let genuine = Envelope {
            header: genuine_header.clone(),
            text: "hello".to_string(),
            sig: genuine_sig,
            msgid: genuine_msgid,
        };
        assert!(verify_origin_signature(&genuine), "alice-host signing as itself must verify");

        // Mallory signs a header CLAIMING to be alice-host — the exact
        // forgery spec item 3 rules out: only the ONE key on record for
        // that exact `from.node` is ever tried, never every verified
        // node's key.
        let (forged_sig, forged_msgid) = seal(&genuine_header, "hello", &mallory_kp);
        let forged = Envelope { header: genuine_header, text: "hello".to_string(), sig: forged_sig, msgid: forged_msgid };
        assert!(
            !verify_origin_signature(&forged),
            "a different paired node signing as alice-host must NOT verify"
        );

        // No key on record at all for this `from.node`.
        let mut unknown_header = header("alice", "bob", ENTRY_TYPE_LETTER, "2026-09-06T00:00:00Z");
        unknown_header.from.node = "nobody-host".to_string();
        let (unknown_sig, unknown_msgid) = seal(&unknown_header, "hello", &alice_kp);
        let unknown =
            Envelope { header: unknown_header, text: "hello".to_string(), sig: unknown_sig, msgid: unknown_msgid };
        assert!(
            !verify_origin_signature(&unknown),
            "no key on record for from.node must refuse, never fall back to any other key"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A verified signer's receipt for `alice-host`, as that node would mint it.
    fn alice_receipt(text: &str, minted_at: &str, alice: &identity::Keypair) -> Envelope {
        let mut h = header("alice", "bob", ENTRY_TYPE_RECEIPT, minted_at);
        h.from.node = "alice-host".to_string();
        let (sig, msgid) = seal(&h, text, alice);
        Envelope { header: h, text: text.to_string(), sig, msgid }
    }

    /// [`alice_receipt`] signed in `mesh` (`header.origin_mesh`), so the same
    /// acknowledgement can be minted once per mesh.
    fn alice_receipt_in(text: &str, minted_at: &str, mesh: &str, alice: &identity::Keypair) -> Envelope {
        let mut h = header("alice", "bob", ENTRY_TYPE_RECEIPT, minted_at);
        h.from.node = "alice-host".to_string();
        h.origin_mesh = mesh.to_string();
        let (sig, msgid) = seal(&h, text, alice);
        Envelope { header: h, text: text.to_string(), sig, msgid }
    }

    /// A letter this box sent as `yomi` and still holds under `msgid`: the one
    /// record a receipt for it is filed against. [`header`] names `yomi` as the
    /// sending node, so a receipt addressed to `yomi` names this box.
    fn spool_letter_sent_as_yomi(msgid: &str) {
        let envelope = Envelope {
            header: header("bob", "alice", ENTRY_TYPE_LETTER, "2026-09-06T00:00:00Z"),
            text: String::new(),
            sig: String::new(),
            msgid: msgid.to_string(),
        };
        crate::outbox::write_entry("alice-host", &crate::outbox::OutboxEntry::fresh(envelope)).unwrap();
    }

    /// `alice-host` on record under a fresh key, so its receipts verify.
    fn alice_on_record() -> identity::Keypair {
        let alice = identity::mint_ephemeral().unwrap();
        let mut nodes = Vec::new();
        crate::node_store::upsert_paired_node(
            &mut nodes,
            "alice-host",
            "https://alice",
            &alice.info().pubkey_hex,
            "2026-09-06T00:00:00Z",
            &["message".to_string()],
            "home",
        );
        crate::node_store::save_nodes(&nodes).unwrap();
        alice
    }

    /// **A receipt names exactly one letter, or it files nothing.** Its text
    /// becomes a path in the outbox, so a receipt whose text is anything but one
    /// lowercase 64-hex msgid is refused as a bad msgid, signed or not. A proper
    /// one still files.
    #[test]
    fn a_receipt_whose_text_is_not_exactly_one_msgid_is_refused() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("deposit-ack-shape");
        let alice = alice_on_record();
        let letter = "ab".repeat(32);

        for text in [format!("{letter} "), format!("{letter}\n"), format!("../{letter}"), letter.to_uppercase()] {
            let ack = alice_receipt(&text, "2026-09-06T00:00:01Z", &alice);
            assert_eq!(deposit(ack, "alice-host").unwrap(), DepositOutcome::MalformedAck, "{text:?}");
        }
        assert!(read_base().unwrap().is_empty(), "no malformed receipt is filed");

        spool_letter_sent_as_yomi(&letter);
        let ack = alice_receipt(&letter, "2026-09-06T00:00:01Z", &alice);
        assert!(matches!(deposit(ack, "alice-host").unwrap(), DepositOutcome::Filed { .. }));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A signer acknowledges a letter once.** A second receipt from the same
    /// signer naming the same msgid — a re-ack minted in a later second, so a new
    /// msgid — is answered `duplicate` and files nothing, however many times a
    /// sender re-acks.
    #[test]
    fn a_second_receipt_from_the_same_signer_for_the_same_letter_files_nothing() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("deposit-ack-dedup");
        let alice = alice_on_record();
        let letter = "cd".repeat(32);

        spool_letter_sent_as_yomi(&letter);
        let first = alice_receipt(&letter, "2026-09-06T00:00:01Z", &alice);
        let again = alice_receipt(&letter, "2026-09-06T00:00:02Z", &alice);
        assert_ne!(first.msgid, again.msgid, "a re-ack is a new envelope");
        assert!(matches!(deposit(first, "alice-host").unwrap(), DepositOutcome::Filed { .. }));
        assert_eq!(deposit(again, "alice-host").unwrap(), DepositOutcome::Duplicate { filed_letter: false });
        assert_eq!(read_base().unwrap().len(), 1, "one receipt for the letter, however many re-acks arrive");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A flood of receipts for letters this box never sent files nothing.**
    /// Each receipt names a fresh, well-formed msgid and is properly signed, so
    /// every one passes the shape and signature checks. None names a letter this
    /// box holds, so none is filed, and the base and the receipt index stay empty.
    #[test]
    fn a_flood_of_receipts_for_letters_never_sent_files_nothing() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("deposit-ack-flood");
        let alice = alice_on_record();

        for i in 0..200u32 {
            let text = format!("{i:064x}");
            let ack = alice_receipt(&text, "2026-09-06T00:00:01Z", &alice);
            assert_eq!(deposit(ack, "alice-host").unwrap(), DepositOutcome::Duplicate { filed_letter: false }, "{i}");
        }
        assert!(read_base().unwrap().is_empty(), "no receipt for an unsent letter is filed");
        assert!(!receipts_log_path().exists(), "and nothing is filed either");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A receipt addressed to a name other than the one its letter went out as
    /// names a letter this box did send, but not to this name: it is not filed.
    #[test]
    fn a_receipt_addressed_to_another_name_files_nothing() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("deposit-ack-addressee");
        let alice = alice_on_record();
        let letter = "ef".repeat(32);
        spool_letter_sent_as_yomi(&letter);

        let mut h = header("alice", "bob", ENTRY_TYPE_RECEIPT, "2026-09-06T00:00:01Z");
        h.from.node = "alice-host".to_string();
        h.to.node = "elsewhere".to_string();
        let (sig, msgid) = seal(&h, &letter, &alice);
        let ack = Envelope { header: h, text: letter.clone(), sig, msgid };
        assert_eq!(deposit(ack, "alice-host").unwrap(), DepositOutcome::Duplicate { filed_letter: false });
        assert!(read_base().unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **The same acknowledgement in another mesh is a different acknowledgement.**
    /// The dedup key is the signer, the mesh and the acked msgid, so a receipt
    /// signed in mesh `far` does not stand in for the one signed in `home`.
    #[test]
    fn the_same_letters_receipt_in_another_mesh_is_not_conflated() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("deposit-ack-mesh");
        let alice = alice_on_record();
        let letter = "ab".repeat(32);
        spool_letter_sent_as_yomi(&letter);

        let home = alice_receipt_in(&letter, "2026-09-06T00:00:01Z", "home", &alice);
        let far = alice_receipt_in(&letter, "2026-09-06T00:00:02Z", "far", &alice);
        assert!(matches!(deposit(home, "alice-host").unwrap(), DepositOutcome::Filed { .. }));
        assert!(matches!(deposit(far, "alice-host").unwrap(), DepositOutcome::Filed { .. }), "filed, not a duplicate");
        assert_eq!(read_base().unwrap().len(), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_msgid_that_does_not_recompute_is_rejected() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("deposit-bad-msgid");

        let kp = identity::mint_ephemeral().unwrap();
        let mut nodes = Vec::new();
        crate::node_store::upsert_paired_node(
            &mut nodes,
            "alice-host",
            "https://alice",
            &kp.info().pubkey_hex,
            "2026-09-06T00:00:00Z",
            &["message".to_string()],
            "home",
        );
        crate::node_store::save_nodes(&nodes).unwrap();

        let mut h = header("alice", "bob", ENTRY_TYPE_LETTER, "2026-09-06T00:00:00Z");
        h.from.node = "alice-host".to_string();
        let (sig, msgid) = seal(&h, "hello", &kp);
        let mut envelope = Envelope { header: h, text: "hello".to_string(), sig, msgid };
        envelope.msgid = "not-the-real-msgid".to_string(); // tampered after sealing

        let outcome = deposit(envelope, "alice-host").unwrap();
        assert_eq!(outcome, DepositOutcome::BadMsgid);
        assert!(read_base().unwrap().is_empty(), "a bad-msgid deposit must never file");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn append_only_invariants_hold_across_multiple_files() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("append-only");

        let e1 = file_letter("alice", "bob", "one").unwrap();
        let e2 = file_letter("alice", "bob", "two").unwrap();
        let e3 = file_letter("alice", "carol", "three").unwrap();

        assert_eq!(e1.seq, 1);
        assert_eq!(e2.seq, 2);
        assert_eq!(e3.seq, 3);

        let all = read_base().unwrap();
        assert_eq!(all.len(), 3, "every filed entry survives, none overwritten");
        assert_eq!(all[0].envelope.text, "one");
        assert_eq!(all[1].envelope.text, "two");
        assert_eq!(all[2].envelope.text, "three");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_torn_tail_is_truncated_before_the_next_append() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("torn-tail");

        let good = file_letter("alice", "bob", "good line").unwrap();

        // Simulate a crash mid-write: a partial line with NO trailing newline.
        {
            let mut f = std::fs::OpenOptions::new().append(true).open(box_path("bob")).unwrap();
            std::io::Write::write_all(&mut f, br#"{"seq":99,"receivedAt":"broke"#).unwrap();
        }
        let raw_before = std::fs::read_to_string(box_path("bob")).unwrap();
        assert!(!raw_before.ends_with('\n'), "the fixture really did leave a torn tail");

        let next = file_letter("alice", "bob", "after the crash").unwrap();
        assert_eq!(next.seq, good.seq + 1, "the torn tail's bogus seq 99 must never be counted");

        let all = read_base().unwrap();
        assert_eq!(all.len(), 2, "the torn line is gone, not folded in as a third entry");
        assert_eq!(all[0].envelope.text, "good line");
        assert_eq!(all[1].envelope.text, "after the crash");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_torn_mid_utf8_tail_wedges_neither_reads_nor_the_next_write() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("torn-utf8");
        let good = file_letter("alice", "bob", "good").unwrap();
        {
            // Half of a three-byte character after the last complete line.
            let mut f = std::fs::OpenOptions::new().append(true).open(box_path("bob")).unwrap();
            std::io::Write::write_all(&mut f, &[0xE2, 0x82]).unwrap();
        }
        assert_eq!(read_base().unwrap().len(), 1, "a read skips the torn tail instead of erroring");

        let next = file_letter("alice", "bob", "after the crash").unwrap();
        assert_eq!(next.seq, good.seq + 1);
        assert_eq!(read_base().unwrap().len(), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn seq_never_restarts_after_a_prune_empties_the_store() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("seq-after-prune");
        file_letter("alice", "bob", "one").unwrap();
        file_letter("alice", "bob", "two").unwrap();
        file_letter("alice", "bob", "three").unwrap();
        assert_eq!(read_for("bob", false, None).unwrap().len(), 3, "bob has read all three");

        // A prune that drops every line, rewritten the way rm_older_than writes it.
        atomic_write_private(&box_path("bob"), b"").unwrap();

        let fresh = file_letter("alice", "bob", "four").unwrap();
        assert!(fresh.seq > 3, "the counter survives the prune, got seq {}", fresh.seq);
        let (_, unread) = unread_for("bob", None).unwrap();
        assert_eq!(unread.len(), 1, "bob's mark at 3 must not hide the new letter");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn next_seq_follows_a_line_an_older_writer_appended() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("older-writer");
        file_letter("alice", "bob", "one").unwrap();

        // An older binary appends to base.jsonl, the one file it writes, a
        // line this binary's counter never issued.
        let old = letter_entry("bob", 9, "from the old writer");
        feed::append_line(&base_path(), serde_json::to_string(&old).unwrap().as_bytes()).unwrap();

        let next = file_letter("alice", "bob", "two").unwrap();
        assert_eq!(next.seq, 10, "a number the older writer used is never reissued");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Filing reads the counter and base.jsonl's tail, never the boxes: a line
    /// with a huge seq in a box does not move the counter on the next filing.
    #[test]
    fn a_box_line_is_not_reread_per_filing() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("box-not-reread");
        file_letter("alice", "bob", "one").unwrap();
        append_entry(&letter_entry("bob", 1_000_000, "planted")).unwrap();

        let next = file_letter("alice", "bob", "two").unwrap();
        assert_eq!(next.seq, 2, "the counter moved to a seq read from a box line");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A counter at u64::MAX refuses the filing with a taught error and writes
    /// no line, instead of wrapping to 0 and hiding later mail.
    #[test]
    fn next_seq_refuses_to_wrap_past_u64_max() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("seq-wrap");
        file_letter("alice", "bob", "one").unwrap();
        let before = std::fs::read(box_path("bob")).unwrap();
        std::fs::write(seq_path(), format!("{}\n", u64::MAX)).unwrap();

        let err = file_letter("alice", "bob", "two").unwrap_err();
        assert!(err.contains("u64::MAX"), "the refusal names the exhausted counter, got {err}");
        assert_eq!(std::fs::read(box_path("bob")).unwrap(), before, "no line was written");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A letter from `alice-host` to `bob`, signed by `alice` the way a far node
    /// signs what it sends.
    fn alice_letter(text: &str, alice: &identity::Keypair) -> Envelope {
        let mut h = header("alice", "bob", ENTRY_TYPE_LETTER, "2026-09-06T00:00:00Z");
        h.from.node = "alice-host".to_string();
        let (sig, msgid) = seal(&h, text, alice);
        Envelope { header: h, text: text.to_string(), sig, msgid }
    }

    fn write_base(lines: &[String]) {
        std::fs::create_dir_all(mail_dir()).unwrap();
        let body: String = lines.iter().map(|l| format!("{l}\n")).collect();
        std::fs::write(base_path(), body).unwrap();
    }

    fn seqs(entries: &[Entry]) -> Vec<u64> {
        entries.iter().map(|e| e.seq).collect()
    }

    /// The old single base splits into one file per mailbox, every seq and every
    /// mark kept, so a reader's place in its own mailbox survives the move.
    #[test]
    fn migration_splits_base_keeping_every_seq_and_mark() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("migrate-split");
        write_base(&[
            serde_json::to_string(&letter_entry("bob", 1, "to bob")).unwrap(),
            serde_json::to_string(&letter_entry("carol", 2, "to carol")).unwrap(),
            serde_json::to_string(&letter_entry("bob", 3, "bob again")).unwrap(),
        ]);
        std::fs::write(cursors_path(), r#"{"bob":{"bob":{"seq":1}}}"#).unwrap();

        assert_eq!(seqs(&read_base().unwrap()), vec![1, 2, 3]);
        assert!(!base_path().exists(), "the old base is archived, not left in place");
        assert!(mail_dir().join("base.jsonl.migrated").exists());
        assert_eq!(seqs(&read_file_entries(&box_path("bob")).unwrap()), vec![1, 3]);
        assert_eq!(seqs(&read_file_entries(&box_path("carol")).unwrap()), vec![2]);
        let (_, unread) = unread_for("bob", None).unwrap();
        assert_eq!(seqs(&unread), vec![3], "bob's mark at 1 still stands after the split");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Receipts go to `receipts.log`; transit lines are not entries, so they stay
    /// in the archive and no mailbox ever shows them.
    #[test]
    fn migration_routes_receipts_and_leaves_transit_in_the_archive() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("migrate-receipts");
        let receipt_header = header("alice", "bob", ENTRY_TYPE_RECEIPT, "2026-09-06T00:00:00Z");
        let (kp, _) = identity::load_or_mint().unwrap();
        let text = format!("{:064x}", 7);
        let (sig, msgid) = seal(&receipt_header, &text, &kp);
        let receipt = Entry {
            seq: 1,
            received_at: now_iso_utc(),
            kind: ENTRY_TYPE_RECEIPT.to_string(),
            via: "self".to_string(),
            origin: String::new(),
            envelope: Envelope { header: receipt_header, text, sig, msgid },
        };
        let transit = r#"{"seq":2,"receivedAt":"2026-09-06T00:00:00Z","type":"transit","via":"self","digest":"d","next":"n","mesh":"","held":false}"#;
        write_base(&[serde_json::to_string(&receipt).unwrap(), transit.to_string()]);

        assert_eq!(seqs(&read_base().unwrap()), vec![1], "the transit line is not an entry");
        assert_eq!(seqs(&read_file_entries(&receipts_log_path()).unwrap()), vec![1]);
        assert!(box_paths().unwrap().is_empty(), "a receipt makes no mailbox");
        let archived = std::fs::read_to_string(mail_dir().join("base.jsonl.migrated")).unwrap();
        assert!(archived.contains(transit), "the transit line is kept, unread, in the archive");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A migration killed after one line is filed files each line once on retry:
    /// the line already in its box is skipped, the rest are filed.
    #[test]
    fn a_migration_killed_midway_files_each_line_once_on_retry() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("migrate-killed");
        let first = letter_entry("bob", 1, "first");
        write_base(&[
            serde_json::to_string(&first).unwrap(),
            serde_json::to_string(&letter_entry("bob", 2, "second")).unwrap(),
            serde_json::to_string(&letter_entry("carol", 3, "third")).unwrap(),
        ]);
        // The first run filed line one, then died before it archived the base.
        append_entry(&first).unwrap();

        assert_eq!(seqs(&read_base().unwrap()), vec![1, 2, 3]);
        assert_eq!(seqs(&read_file_entries(&box_path("bob")).unwrap()), vec![1, 2], "line one is not filed twice");
        assert_eq!(seqs(&read_file_entries(&box_path("carol")).unwrap()), vec![3]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A `base.jsonl` an older binary writes after the migration is imported with
    /// fresh numbers: its seq 1 collides with nothing, and a line this box already
    /// holds is skipped. The raw bytes are archived, never read again.
    #[test]
    fn a_late_base_from_an_older_binary_is_imported_renumbered() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("migrate-late");
        let filed = letter_entry("bob", 1, "already here");
        write_base(&[serde_json::to_string(&filed).unwrap()]);
        assert_eq!(seqs(&read_base().unwrap()), vec![1]);

        let late = letter_entry("bob", 1, "from the old binary");
        write_base(&[serde_json::to_string(&filed).unwrap(), serde_json::to_string(&late).unwrap()]);
        let got = read_base().unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[1].envelope.text, "from the old binary");
        assert!(got[1].seq > 1, "the late letter takes a fresh number, got {}", got[1].seq);
        assert!(!base_path().exists(), "the late base is removed once imported");
        let archived = std::fs::read_to_string(mail_dir().join("base.jsonl.migrated")).unwrap();
        assert!(archived.contains("from the old binary"), "the raw bytes are archived");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The box is the dedup memory: a second deposit of one letter is a duplicate
    /// that says the first filing was a letter, and no `seen.jsonl` is ever made.
    #[test]
    fn a_redeposit_is_duplicate_with_no_seen_file() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("redeposit");
        let alice = alice_on_record();
        box_add("bob").unwrap();
        let letter = alice_letter("hello", &alice);

        assert!(matches!(deposit(letter.clone(), "alice-host").unwrap(), DepositOutcome::Filed { .. }));
        assert_eq!(
            deposit(letter, "alice-host").unwrap(),
            DepositOutcome::Duplicate { filed_letter: true }
        );
        assert_eq!(seqs(&read_file_entries(&box_path("bob")).unwrap()), vec![1]);
        assert!(!seen_path().exists(), "no seen file is written");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A crash after the box's fsync leaves the letter filed and no ack sent. The
    /// re-offer finds it in its box and answers duplicate, which re-acks it.
    #[test]
    fn a_crash_after_the_box_fsync_answers_duplicate_and_reacks() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("crash-after-fsync");
        let alice = alice_on_record();
        let letter = alice_letter("hello again", &alice);
        let entry = Entry {
            seq: 1,
            received_at: now_iso_utc(),
            kind: ENTRY_TYPE_LETTER.to_string(),
            via: "alice-host".to_string(),
            origin: String::new(),
            envelope: letter.clone(),
        };
        append_entry(&entry).unwrap();

        assert_eq!(deposit(letter, "alice-host").unwrap(), DepositOutcome::Duplicate { filed_letter: true });
        assert_eq!(seqs(&read_base().unwrap()), vec![1], "the duplicate files nothing new");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unwritable_lock_path_makes_every_mail_write_err() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("unwritable-lock");

        // Force `stage_dir()/.stage.lock` to be a directory, so opening it
        // for writing fails cleanly (EISDIR) rather than ever taking the flock.
        std::fs::create_dir_all(crate::fs::conducting_stage_dir()).unwrap();
        std::fs::create_dir_all(crate::fs::conducting_stage_dir().join(".stage.lock")).unwrap();

        let err = file_letter("alice", "bob", "should never land").unwrap_err();
        assert!(!err.is_empty());
        assert!(read_entries_unlocked().unwrap().is_empty(), "nothing was ever appended");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_held_lock_serializes_a_second_writer_behind_the_first_rather_than_failing_it() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("held-lock");
        std::fs::create_dir_all(crate::fs::conducting_stage_dir()).unwrap();

        let lock_path = crate::fs::conducting_stage_dir().join(".stage.lock");
        let held = std::fs::OpenOptions::new().create(true).write(true).open(&lock_path).unwrap();
        assert!(crate::fs::lock_exclusive(&held), "test setup must actually hold the lock");

        let released = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let released_writer = released.clone();
        let handle = std::thread::spawn(move || {
            let out = file_letter("alice", "bob", "waited its turn");
            // The write must not observe itself completing before the
            // holder released — proven by the holder flipping the flag
            // strictly before it unlocks, below.
            assert!(released_writer.load(std::sync::atomic::Ordering::SeqCst));
            out
        });

        std::thread::sleep(std::time::Duration::from_millis(150));
        assert!(!handle.is_finished(), "a genuinely held lock must block the second writer, not fail it");

        released.store(true, std::sync::atomic::Ordering::SeqCst);
        crate::fs::unlock(&held);
        drop(held);

        let result = handle.join().unwrap();
        assert!(result.is_ok(), "the second writer must succeed once the lock frees up, not error");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_readers_own_read_leaves_a_second_readers_mark_untouched() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("cursor-per-reader");

        file_letter("alice", "bob", "one").unwrap();
        file_letter("alice", "bob", "two").unwrap();

        let got = read_for("bob", false, Some("sess-1")).unwrap();
        assert_eq!(got.len(), 2);

        let cursors = load_cursors().unwrap();
        let cursor = cursors.get("bob").unwrap();
        assert_eq!(cursor.get(&session_reader_key("sess-1")).unwrap().seq, 2);
        assert!(cursor.get(&session_reader_key("sess-2")).is_none(), "a second reader has no mark until it reads");

        // A second reader, same name: still sees every entry — sess-1's
        // read never touched it.
        let got2 = read_for("bob", false, Some("sess-2")).unwrap();
        assert_eq!(got2.len(), 2, "a second reader consumes nothing sess-1 already read");

        let cursors = load_cursors().unwrap();
        let cursor = cursors.get("bob").unwrap();
        assert_eq!(cursor.get(&session_reader_key("sess-1")).unwrap().seq, 2, "sess-1's mark is untouched by sess-2's read");
        assert_eq!(cursor.get(&session_reader_key("sess-2")).unwrap().seq, 2);

        // sess-1 reading again with nothing new returns nothing, and does
        // not duplicate anything.
        let again = read_for("bob", false, Some("sess-1")).unwrap();
        assert!(again.is_empty());
        assert_eq!(load_cursors().unwrap().get("bob").unwrap().len(), 2, "still exactly two readers");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_readers_own_read_all_names_leaves_a_second_readers_mark_untouched_across_names() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("all-names-cursor-per-reader");

        file_letter("alice", "bob", "one").unwrap();
        file_letter("alice", "bob", "two").unwrap();
        file_letter("alice", "carol", "three").unwrap();

        let got = read_all_names(false, Some("sess-1")).unwrap();
        assert_eq!(got.len(), 3, "sess-1's first read_all_names sees every entry, across every name");

        let cursors = load_cursors().unwrap();
        assert_eq!(cursors.get("bob").unwrap().get(&session_reader_key("sess-1")).unwrap().seq, 2);
        assert_eq!(cursors.get("carol").unwrap().get(&session_reader_key("sess-1")).unwrap().seq, 3);
        assert!(cursors.get("bob").unwrap().get(&session_reader_key("sess-2")).is_none(), "a second reader has no mark until it reads");
        assert!(cursors.get("carol").unwrap().get(&session_reader_key("sess-2")).is_none());

        // A second reader, same names: still sees every entry — sess-1's
        // read_all_names never touched it.
        let got2 = read_all_names(false, Some("sess-2")).unwrap();
        assert_eq!(got2.len(), 3, "a second reader consumes nothing sess-1 already read, across every name");

        let cursors = load_cursors().unwrap();
        assert_eq!(cursors.get("bob").unwrap().get(&session_reader_key("sess-1")).unwrap().seq, 2, "sess-1's mark under bob is untouched by sess-2's read_all_names");
        assert_eq!(cursors.get("carol").unwrap().get(&session_reader_key("sess-1")).unwrap().seq, 3, "sess-1's mark under carol is untouched by sess-2's read_all_names");
        assert_eq!(cursors.get("bob").unwrap().get(&session_reader_key("sess-2")).unwrap().seq, 2);
        assert_eq!(cursors.get("carol").unwrap().get(&session_reader_key("sess-2")).unwrap().seq, 3);

        // sess-1 reading again with nothing new returns nothing.
        let again = read_all_names(false, Some("sess-1")).unwrap();
        assert!(again.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unconducted_read_advances_only_the_name_keyed_pseudo_reader() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("cursor-unconducted");

        file_letter("alice", "bob", "one").unwrap();

        // A conducted reader reads first.
        let got = read_for("bob", false, Some("sess-1")).unwrap();
        assert_eq!(got.len(), 1);

        // An unconducted read (no session id) still sees the letter: its
        // pseudo-reader, keyed by the mailbox name itself, has never read
        // anything, and is not sess-1.
        let got_unconducted = read_for("bob", false, None).unwrap();
        assert_eq!(got_unconducted.len(), 1, "the pseudo-reader has its own, fresh mark");

        let cursors = load_cursors().unwrap();
        let cursor = cursors.get("bob").unwrap();
        assert_eq!(cursor.get(&session_reader_key("sess-1")).unwrap().seq, 1, "the conducted reader's mark is untouched");
        assert_eq!(cursor.get("bob").unwrap().seq, 1, "the pseudo-reader is keyed by the mailbox name");

        // A second unconducted read now sees nothing new.
        let got_again = read_for("bob", false, None).unwrap();
        assert!(got_again.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_flat_cursors_file_migrates_each_listed_reader_inheriting_the_old_seq() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("cursor-migration");

        std::fs::create_dir_all(mail_dir()).unwrap();
        let flat = serde_json::json!({
            "bob": { "seq": 5, "readers": ["sess-1", "sess-2"] },
            "carol": { "seq": 3, "readers": [] }
        });
        std::fs::write(cursors_path(), serde_json::to_string(&flat).unwrap()).unwrap();

        // Any store touch migrates it, under the same lock.
        read_base().unwrap();

        let cursors = load_cursors().unwrap();
        let bob = cursors.get("bob").unwrap();
        assert_eq!(bob.get(&session_reader_key("sess-1")).unwrap().seq, 5, "each listed reader inherits the name's old seq");
        assert_eq!(bob.get(&session_reader_key("sess-2")).unwrap().seq, 5);
        let carol = cursors.get("carol").unwrap();
        assert_eq!(carol.len(), 1, "no listed reader: exactly one reader, the name itself");
        assert_eq!(carol.get("carol").unwrap().seq, 3, "a name with no listed reader keeps its mark under the name");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn migrating_cursors_twice_is_the_same_as_once() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("cursor-migration-idempotent");

        std::fs::create_dir_all(mail_dir()).unwrap();
        let flat = serde_json::json!({ "bob": { "seq": 5, "readers": ["sess-1"] } });
        std::fs::write(cursors_path(), serde_json::to_string(&flat).unwrap()).unwrap();

        read_base().unwrap();
        let once = load_cursors().unwrap();

        read_base().unwrap();
        let twice = load_cursors().unwrap();

        assert_eq!(once, twice, "a second open must not undo or redo the migration");
        assert_eq!(twice.get("bob").unwrap().get(&session_reader_key("sess-1")).unwrap().seq, 5);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_reader_named_seq_is_not_confused_with_the_flat_shape_in_either_direction() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("cursor-migration-seq-reader");

        std::fs::create_dir_all(mail_dir()).unwrap();
        let flat = serde_json::json!({ "bob": { "seq": 5, "readers": ["seq"] } });
        std::fs::write(cursors_path(), serde_json::to_string(&flat).unwrap()).unwrap();

        // A legacy row naming a reader "seq" must migrate exactly like any
        // other reader — the old-shape test looks at the per-NAME value's
        // own `seq` field, never at what a reader happens to be called.
        read_base().unwrap();
        let once = load_cursors().unwrap();
        let bob = once.get("bob").unwrap();
        assert_eq!(bob.len(), 1, "the only listed reader is the one literally named \"seq\"");
        assert_eq!(bob.get(&session_reader_key("seq")).unwrap().seq, 5, "a reader named seq inherits the old seq like any other");

        let raw = std::fs::read_to_string(cursors_path()).unwrap();
        let raw_json: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(
            raw_json["bob"],
            serde_json::json!({ "session:seq": { "seq": 5 } }),
            "the migrated shape nests seq inside the reader object, never as a bare number"
        );

        // A second pass must not mistake this reader's own nested
        // `{"seq":5}` for the old flat shape's bare `seq` field and
        // re-migrate (or otherwise disturb) it.
        read_base().unwrap();
        let twice = load_cursors().unwrap();
        assert_eq!(once, twice, "a reader named seq must be left untouched by a second migration pass");
        assert_eq!(twice.get("bob").unwrap().get(&session_reader_key("seq")).unwrap().seq, 5);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reread_returns_everything_for_the_caller_without_moving_another_readers_mark() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("cursor-reread");

        file_letter("alice", "bob", "one").unwrap();
        file_letter("alice", "bob", "two").unwrap();

        read_for("bob", false, Some("sess-1")).unwrap();
        read_for("bob", false, Some("sess-2")).unwrap();

        let reread = read_for("bob", true, Some("sess-1")).unwrap();
        assert_eq!(reread.len(), 2, "--reread returns every entry for the caller, not just new ones");

        let cursors = load_cursors().unwrap();
        let cursor = cursors.get("bob").unwrap();
        assert_eq!(cursor.get(&session_reader_key("sess-1")).unwrap().seq, 2, "reread still only ever advances forward");
        assert_eq!(cursor.get(&session_reader_key("sess-2")).unwrap().seq, 2, "a reread by sess-1 never moves sess-2's mark");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mark_advances_only_the_callers_mark() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("cursor-mark");

        file_letter("alice", "bob", "one").unwrap();
        file_letter("alice", "bob", "two").unwrap();

        let seq = mark("bob", Some("sess-1")).unwrap();
        assert_eq!(seq, 2);

        let cursors = load_cursors().unwrap();
        let cursor = cursors.get("bob").unwrap();
        assert_eq!(cursor.get(&session_reader_key("sess-1")).unwrap().seq, 2);
        assert!(cursor.get(&session_reader_key("sess-2")).is_none(), "mark never touches a reader that never called it");

        // sess-2 still sees both letters as new — mark is scoped to sess-1.
        let got = read_for("bob", false, Some("sess-2")).unwrap();
        assert_eq!(got.len(), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_name_fully_read_by_one_reader_is_still_unread_for_another_reader() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("names-unread-per-reader");

        file_letter("alice", "bob", "one").unwrap();

        // Reader A reads bob all the way to the end.
        let got = read_for("bob", false, Some("sess-1")).unwrap();
        assert_eq!(got.len(), 1);

        assert_eq!(
            names_with_unread(Some("sess-1")).unwrap(),
            Vec::<String>::new(),
            "sess-1 fully read bob and must not see it as unread"
        );
        assert_eq!(
            names_with_unread(Some("sess-2")).unwrap(),
            vec!["bob".to_string()],
            "sess-2 has never read bob — it must still be unread for this reader"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn migration_maps_legacy_inbox_rows_into_typed_receipts() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("migration");

        let legacy = serde_json::json!({
            "schemaVersion": "0",
            "entries": [
                { "from": "alice", "target": "sess-1", "text": "hi", "receivedAt": "2026-08-21T00:00:00Z", "read": false },
                { "from": "", "target": "sess-2", "text": "anon ping", "receivedAt": "2026-08-21T00:01:00Z", "read": true }
            ]
        });
        std::fs::create_dir_all(state_dir()).unwrap();
        std::fs::write(legacy_inbox_path(), serde_json::to_string(&legacy).unwrap()).unwrap();

        let all = read_base().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].kind, ENTRY_TYPE_RECEIPT);
        assert_eq!(all[0].envelope.header.from.name, "alice");
        assert_eq!(all[0].envelope.header.to.name, "sess-1");
        assert_eq!(all[0].envelope.text, "hi");
        assert_eq!(all[0].received_at, "2026-08-21T00:00:00Z");
        assert_eq!(all[1].envelope.header.from.name, "");

        assert!(!legacy_inbox_path().exists(), "the old file is renamed away");
        assert!(state_dir().join("inbox.json.migrated").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_migration_interrupted_after_some_rows_files_each_legacy_row_exactly_once_on_retry() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("migration-retry");

        let legacy = serde_json::json!({
            "entries": [
                { "from": "alice", "target": "sess-1", "text": "first", "receivedAt": "2026-08-21T00:00:00Z" },
                { "from": "bob", "target": "sess-2", "text": "second", "receivedAt": "2026-08-21T00:01:00Z" }
            ]
        });
        std::fs::create_dir_all(state_dir()).unwrap();
        std::fs::write(legacy_inbox_path(), serde_json::to_string(&legacy).unwrap()).unwrap();

        // Simulate a crash after row 1 was filed but before the rename:
        // hand-build exactly the entry a first partial attempt would have
        // produced for row 1, and seed base.jsonl/seen.jsonl with it
        // directly. inbox.json stays present, as it would after a real
        // crash — nothing here goes through `migrate_if_needed`.
        std::fs::create_dir_all(mail_dir()).unwrap();
        let node = display::local_node_name();
        let (kp, _) = identity::load_or_mint().unwrap();
        let row1_header = Header {
            version: ENVELOPE_VERSION.to_string(),
            from: Address { node: node.clone(), name: "alice".to_string() },
            to: Address { node: node.clone(), name: "sess-1".to_string() },
            kind: ENTRY_TYPE_RECEIPT.to_string(),
            minted_at: "2026-08-21T00:00:00Z".to_string(),
            origin_mesh: String::new(),
        };
        let (sig, row1_msgid) = seal(&row1_header, "first", &kp);
        let row1_entry = Entry {
            seq: 1,
            received_at: "2026-08-21T00:00:00Z".to_string(),
            kind: ENTRY_TYPE_RECEIPT.to_string(),
            via: "self".to_string(),
            origin: String::new(),
            envelope: Envelope { header: row1_header, text: "first".to_string(), sig, msgid: row1_msgid.clone() },
        };
        std::fs::write(base_path(), format!("{}\n", serde_json::to_string(&row1_entry).unwrap())).unwrap();
        // Retry: inbox.json is still present, exactly as after a crash.
        let all = read_base().unwrap();
        assert_eq!(all.len(), 2, "row 1 must not be duplicated, row 2 must be filed");
        assert_eq!(all[0].envelope.msgid, row1_msgid, "the pre-seeded row 1 is untouched");
        assert_eq!(all[1].envelope.header.from.name, "bob");
        assert_eq!(all[1].envelope.text, "second");

        assert!(!legacy_inbox_path().exists(), "the retry still completes the rename");
        assert!(state_dir().join("inbox.json.migrated").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_concurrent_second_opener_never_double_migrates() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("migration-race");

        let legacy = serde_json::json!({
            "entries": [{ "from": "alice", "target": "sess-1", "text": "hi", "receivedAt": "2026-08-21T00:00:00Z" }]
        });
        std::fs::create_dir_all(state_dir()).unwrap();
        std::fs::write(legacy_inbox_path(), serde_json::to_string(&legacy).unwrap()).unwrap();

        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let b1 = barrier.clone();
        let t1 = std::thread::spawn(move || {
            b1.wait();
            names_with_unread(None)
        });
        let b2 = barrier.clone();
        let t2 = std::thread::spawn(move || {
            b2.wait();
            names_with_unread(None)
        });
        t1.join().unwrap().unwrap();
        t2.join().unwrap().unwrap();

        let all = read_base().unwrap();
        assert_eq!(all.len(), 1, "the lock must serialize the two opens, never migrate twice");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn msgid_recomputation_rejects_a_tampered_field() {
        // Pure/in-memory only (`mint_ephemeral` never touches disk) — no
        // isolated root needed for this one.
        let h = header("alice", "bob", ENTRY_TYPE_LETTER, "2026-09-06T00:00:00Z");
        let kp = identity::mint_ephemeral().unwrap();
        let (sig, msgid) = seal(&h, "hello", &kp);
        assert_eq!(compute_msgid(&h, "hello", &sig).unwrap(), msgid, "an untampered recompute matches");

        assert_ne!(
            compute_msgid(&h, "HELLO", &sig).unwrap(),
            msgid,
            "a tampered text must change the recomputed msgid"
        );

        let mut tampered_header = h.clone();
        tampered_header.to.name = "carol".to_string();
        assert_ne!(
            compute_msgid(&tampered_header, "hello", &sig).unwrap(),
            msgid,
            "a tampered header field must change the recomputed msgid"
        );
    }

    #[test]
    fn sign_and_verify_round_trip_over_the_canonical_header() {
        // Pure/in-memory only — exercises exactly the bytes
        // `verify_origin_signature` binds itself to (canonical header ‖
        // 0x00 ‖ text), independent of any node lookup; the lookup half is
        // `origin_verification_is_bound_to_the_key_on_record_for_from_node`'s
        // job.
        let h = header("alice", "bob", ENTRY_TYPE_LETTER, "2026-09-06T00:00:00Z");
        let kp = identity::mint_ephemeral().unwrap();
        let (sig, _msgid) = seal(&h, "hello", &kp);

        let mut sig_input = canonical_header_bytes(&h);
        sig_input.push(0u8);
        sig_input.extend_from_slice(b"hello");

        assert!(
            crate::wire_auth::verify_signature_hex(&kp.info().pubkey_hex, &sig_input, &sig),
            "a correctly-signed envelope's signature must verify against its own signer's pubkey"
        );

        let other = identity::mint_ephemeral().unwrap();
        assert!(
            !crate::wire_auth::verify_signature_hex(&other.info().pubkey_hex, &sig_input, &sig),
            "the same signature must not verify against a DIFFERENT signer's pubkey"
        );
    }

    #[test]
    fn canonical_header_is_byte_exact_case_and_whitespace_change_the_msgid() {
        let h1 = header("alice", "bob", ENTRY_TYPE_LETTER, "2026-09-06T00:00:00Z");
        let mut h2 = h1.clone();
        h2.to.name = "Bob".to_string(); // case differs

        let kp = identity::mint_ephemeral().unwrap();
        let (sig1, msgid1) = seal(&h1, "hello", &kp);
        let msgid2 = compute_msgid(&h2, "hello", &sig1).unwrap();
        assert_ne!(msgid1, msgid2, "case must not fold in the canonical header");

        let mut h3 = h1.clone();
        h3.to.name = " bob".to_string(); // leading whitespace differs
        let msgid3 = compute_msgid(&h3, "hello", &sig1).unwrap();
        assert_ne!(msgid1, msgid3, "whitespace must not trim in the canonical header");

        assert_ne!(
            canonical_header_bytes(&h1),
            canonical_header_bytes(&h2),
            "the two headers' verbatim bytes must differ"
        );
    }

    #[test]
    fn a_ring_target_is_armed_only_while_unread_arming_mail_exists_beyond_the_last_ring() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("ring-armed-basic");

        enrol_reader("bob", "sess-1").unwrap();
        let before = ring_targets("bob").unwrap();
        assert!(before.armed.is_empty(), "no arming mail at all: nothing armed");
        assert_eq!(before.enrolled.len(), 1);

        file_letter("alice", "bob", "one").unwrap();
        let after = ring_targets("bob").unwrap();
        assert_eq!(
            after.armed,
            vec![("sess-1".to_string(), 1)],
            "unread arming mail beyond the last ring arms the reader"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_readers_own_read_rearms_the_ring() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("ring-rearm");

        enrol_reader("bob", "sess-1").unwrap(); // enrols sess-1, seq 0
        file_letter("alice", "bob", "one").unwrap(); // seq 1, arming
        let t1 = ring_targets("bob").unwrap();
        assert_eq!(t1.armed, vec![("sess-1".to_string(), 1)], "an unread letter arms");

        stamp_rung("bob", "sess-1", 1).unwrap();
        let t2 = ring_targets("bob").unwrap();
        assert!(t2.armed.is_empty(), "latched at seq 1 with no newer arming mail: not armed");

        read_for("bob", false, Some("sess-1")).unwrap(); // seq catches up to 1
        file_letter("alice", "bob", "two").unwrap(); // seq 2, new arming mail
        let t3 = ring_targets("bob").unwrap();
        assert_eq!(
            t3.armed,
            vec![("sess-1".to_string(), 2)],
            "the reader's own read caught up to the rung point; the next arming entry re-arms it"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_hundred_letters_ring_once_until_mail_read() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("ring-hundred");

        enrol_reader("bob", "sess-1").unwrap();
        file_letter("alice", "bob", "seed").unwrap(); // seq 1
        stamp_rung("bob", "sess-1", 1).unwrap();

        for i in 0..100 {
            file_letter("alice", "bob", &format!("letter {i}")).unwrap();
        }
        let t = ring_targets("bob").unwrap();
        assert!(t.armed.is_empty(), "a hundred more arming letters never re-arm a latched, still-unread reader");

        read_for("bob", false, Some("sess-1")).unwrap(); // seq catches up to 101
        file_letter("alice", "bob", "the one that rings").unwrap(); // seq 102
        let t2 = ring_targets("bob").unwrap();
        assert_eq!(t2.armed, vec![("sess-1".to_string(), 102)], "only mail after the read re-arms");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_pseudo_reader_is_never_a_ring_target_and_does_not_count_as_enrolled() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("ring-pseudo-reader");

        file_letter("alice", "bob", "one").unwrap();
        read_for("bob", false, None).unwrap(); // enrols only the pseudo-reader, key "bob"

        let t = ring_targets("bob").unwrap();
        assert_eq!(t.enrolled.len(), 0, "the pseudo-reader is never counted as enrolled");
        assert!(t.armed.is_empty(), "the pseudo-reader is never a ring target");

        // Unread arming mail piles up for it too — still never a target.
        file_letter("alice", "bob", "two").unwrap();
        let t2 = ring_targets("bob").unwrap();
        assert!(t2.armed.is_empty(), "still never a target, even with unread arming mail outstanding");
        assert_eq!(t2.enrolled.len(), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_latched_reader_still_counts_as_enrolled() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("ring-latched-enrolled");

        enrol_reader("bob", "sess-1").unwrap();
        file_letter("alice", "bob", "one").unwrap();
        stamp_rung("bob", "sess-1", 1).unwrap();

        let t = ring_targets("bob").unwrap();
        assert!(t.armed.is_empty(), "latched immediately after stamping, before any read: not armed");
        assert_eq!(t.enrolled.len(), 1, "a latched reader is still enrolled");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ring_targets_names_every_enrolled_reader_not_just_a_count() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("ring-enrolled-names");

        enrol_reader("bob", "sess-1").unwrap();
        enrol_reader("bob", "sess-2").unwrap();

        let t = ring_targets("bob").unwrap();
        let mut names = t.enrolled.clone();
        names.sort();
        assert_eq!(names, vec!["sess-1".to_string(), "sess-2".to_string()], "every enrolled reader key, not merely how many");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_pseudo_reader_is_absent_from_the_enrolled_names() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("ring-enrolled-names-pseudo");

        file_letter("alice", "bob", "one").unwrap();
        read_for("bob", false, None).unwrap(); // enrols only the pseudo-reader, key "bob"
        enrol_reader("bob", "sess-1").unwrap();

        let t = ring_targets("bob").unwrap();
        assert_eq!(t.enrolled, vec!["sess-1".to_string()], "the pseudo-reader's own key must never appear among the names");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_mark_without_rung_reads_as_armed_and_the_file_gains_no_rung_until_stamped() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("ring-legacy-mark");

        std::fs::create_dir_all(mail_dir()).unwrap();
        let legacy = serde_json::json!({ "bob": { "sess-1": { "seq": 0 } } });
        std::fs::write(cursors_path(), serde_json::to_string(&legacy).unwrap()).unwrap();

        file_letter("alice", "bob", "one").unwrap();

        let t = ring_targets("bob").unwrap();
        assert_eq!(
            t.armed,
            vec![("sess-1".to_string(), 1)],
            "a mark with no `rung` on disk defaults to 0, which is <= seq(0): armed"
        );

        read_for("bob", false, Some("sess-1")).unwrap();
        let raw = std::fs::read_to_string(cursors_path()).unwrap();
        assert!(!raw.contains("rung"), "a read alone must never write an unrung mark's `rung` field");

        stamp_rung("bob", "sess-1", 1).unwrap();
        let raw = std::fs::read_to_string(cursors_path()).unwrap();
        assert!(raw.contains("rung"), "stamping a nonzero rung must appear on disk");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stamping_a_rung_never_moves_seq_and_a_second_reader_is_untouched() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("stamp-seq-untouched");

        file_letter("alice", "bob", "one").unwrap();
        read_for("bob", false, Some("sess-1")).unwrap(); // seq 1
        enrol_reader("bob", "sess-2").unwrap(); // seq 0, rung 0

        stamp_rung("bob", "sess-1", 5).unwrap(); // stamped beyond the current seq, on purpose

        let cursors = load_cursors().unwrap();
        let cursor = cursors.get("bob").unwrap();
        assert_eq!(cursor.get(&session_reader_key("sess-1")).unwrap().seq, 1, "stamp_rung never moves seq");
        assert_eq!(cursor.get(&session_reader_key("sess-1")).unwrap().rung, 5);
        assert_eq!(cursor.get(&session_reader_key("sess-2")).unwrap().seq, 0, "a second reader is untouched by another reader's stamp");
        assert_eq!(cursor.get(&session_reader_key("sess-2")).unwrap().rung, 0);

        stamp_rung("bob", "sess-1", 2).unwrap(); // lower than the current rung
        let cursors = load_cursors().unwrap();
        assert_eq!(cursors.get("bob").unwrap().get(&session_reader_key("sess-1")).unwrap().rung, 5, "rung only ever advances");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stamping_an_unenrolled_reader_is_an_error_not_an_insertion() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("stamp-unenrolled");

        // No cursor map for "bob" at all yet.
        let err = stamp_rung("bob", "sess-1", 1).unwrap_err();
        assert!(!err.is_empty());
        assert!(load_cursors().unwrap().get("bob").is_none(), "stamping an unenrolled reader must insert nothing");

        // A cursor map exists for the name, but not this reader.
        enrol_reader("bob", "sess-2").unwrap();
        let err2 = stamp_rung("bob", "sess-1", 1).unwrap_err();
        assert!(!err2.is_empty());
        let cursors = load_cursors().unwrap();
        assert!(cursors.get("bob").unwrap().get(&session_reader_key("sess-1")).is_none(), "stamping an unenrolled reader must never insert one");
        assert_eq!(cursors.get("bob").unwrap().len(), 1, "only the already-enrolled reader exists");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_receipt_never_arms_a_reader() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("ring-receipt-never-arms");

        enrol_reader("bob", "sess-1").unwrap();
        file_receipt("x", "bob", "t").unwrap();

        let t = ring_targets("bob").unwrap();
        assert!(t.armed.is_empty(), "a receipt never arms a reader");

        file_letter("alice", "bob", "a real letter").unwrap();
        let t2 = ring_targets("bob").unwrap();
        assert_eq!(t2.armed, vec![("sess-1".to_string(), 2)], "a letter arms; the receipt filed before it never counted");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn armed_names_for_reader_lists_only_names_where_this_reader_is_armed() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("armed-names-for-reader");

        enrol_reader("bob", "sess-1").unwrap();
        enrol_reader("carol", "sess-1").unwrap();
        file_letter("alice", "bob", "one").unwrap(); // arms bob for sess-1; carol gets nothing new

        let armed = armed_names_for_reader("sess-1").unwrap();
        assert_eq!(armed, vec![("bob".to_string(), 1)], "only the name with unread arming mail for this reader is listed");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_letter_to_an_invalid_name_is_refused_before_anything_is_written() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("invalid-name-refused");

        for bad in ["Bob", "$(x)", "-lead", "a\nb"] {
            assert!(file_letter("alice", bad, "hi").is_err(), "file_letter must refuse {bad:?}");
            assert!(
                mint_outbound_letter("alice", "elsewhere", bad, "hi").is_err(),
                "mint_outbound_letter must refuse {bad:?}"
            );
        }

        assert!(!boxes_dir().exists(), "a refused name must never create a box");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_old_letter_to_a_name_outside_the_grammar_stays_readable() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("old-name-outside-grammar");

        let h = header("alice", "Old_Name", ENTRY_TYPE_LETTER, &now_iso_utc());
        let (kp, _) = identity::load_or_mint().unwrap();
        let (sig, msgid) = seal(&h, "grandfathered", &kp);
        let envelope = Envelope { header: h, text: "grandfathered".to_string(), sig, msgid: msgid.clone() };
        let entry = Entry {
            seq: 1,
            received_at: now_iso_utc(),
            kind: ENTRY_TYPE_LETTER.to_string(),
            via: "self".to_string(),
            origin: String::new(),
            envelope,
        };
        std::fs::create_dir_all(mail_dir()).unwrap();
        append_entry(&entry).unwrap();
        assert!(legacy_box_path().exists(), "a grammar-refused name is filed in the legacy box");

        let got = read_for("Old_Name", false, None).unwrap();
        assert_eq!(got.len(), 1, "a pre-existing off-grammar name stays filed and readable forever");
        assert_eq!(got[0].envelope.msgid, msgid);

        let shown = show(&msgid).unwrap();
        assert!(shown.is_some(), "show must still find a grandfathered entry by msgid");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn enrolling_a_reader_is_idempotent_and_starts_unread_and_unrung() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("enrol-idempotent");

        enrol_reader("bob", "sess-1").unwrap();
        enrol_reader("bob", "sess-1").unwrap();
        let cursors = load_cursors().unwrap();
        let cursor = cursors.get("bob").unwrap();
        assert_eq!(cursor.len(), 1, "enrolling twice creates exactly one key");
        assert_eq!(cursor.get(&session_reader_key("sess-1")).unwrap().seq, 0);
        assert_eq!(cursor.get(&session_reader_key("sess-1")).unwrap().rung, 0);

        file_letter("alice", "bob", "one").unwrap();
        read_for("bob", false, Some("sess-1")).unwrap(); // seq -> 1
        enrol_reader("bob", "sess-1").unwrap(); // must not reset the mark
        let cursors = load_cursors().unwrap();
        assert_eq!(
            cursors.get("bob").unwrap().get(&session_reader_key("sess-1")).unwrap().seq,
            1,
            "enrolling an already-enrolled reader never moves its mark"
        );

        let err = enrol_reader("bob", "bob").unwrap_err();
        assert!(!err.is_empty(), "the pseudo-reader is never enrolled");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bare_mail_no_longer_lists_session_ids() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("bare-mail-no-session-ids");

        file_letter("alice", "bob", "for bob").unwrap();
        file_receipt("sess-1", "alice", "some-msgid").unwrap();
        assert_eq!(names_with_unread(None).unwrap(), vec!["bob".to_string()]);
        assert!(read_all_names(false, None).unwrap().iter().all(|e| e.envelope.header.to.name == "bob"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mail_read_for_a_session_id_shows_no_receipts() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("read-for-session-no-receipts");

        file_receipt("bob", "sess-1", "m1").unwrap();
        assert_eq!(read_receipts().unwrap().len(), 1, "the receipt is filed, and read_receipts sees it");
        assert!(read_for("sess-1", false, None).unwrap().is_empty());
        assert!(read_all_names(false, None).unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn armed_names_reads_only_the_readers_boxes() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("armed-names-readers-boxes");

        file_letter("alice", "bob", "wake up").unwrap();
        enrol_reader("bob", "sess-1").unwrap();
        // A box nobody enrolled `sess-1` in: unreadable as a file, so any read
        // of it would fail the whole call.
        std::fs::create_dir_all(box_path("dave")).unwrap();
        assert_eq!(armed_names_for_reader("sess-1").unwrap(), vec![("bob".to_string(), 1)]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A receipt past the read cap is never read: `receipts.log` is read up to
    /// [`ROTATE_AT`], so a valid receipt beyond it does not count.
    #[test]
    fn receipts_read_is_bounded_to_the_rotation_cap() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("receipts-read-cap");
        file_receipt("bob", "alice", "one").unwrap();
        let line = std::fs::read(receipts_log_path()).unwrap();
        let mut body = vec![b'x'; ROTATE_AT as usize];
        body.push(b'\n');
        body.extend_from_slice(&line);
        std::fs::write(receipts_log_path(), body).unwrap();

        assert!(read_receipts().unwrap().is_empty(), "a receipt past the cap is never read");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An append rotates a receipt generation first when the record would
    /// cross the cap, so `receipts.log` stays under [`ROTATE_AT`] after it.
    #[test]
    fn a_receipt_log_never_grows_past_the_cap() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("receipts-cap");
        file_receipt("bob", "alice", "one").unwrap();
        let mut full = vec![b'x'; ROTATE_AT as usize - 41];
        full.push(b'\n');
        std::fs::write(receipts_log_path(), full).unwrap();

        file_receipt("bob", "alice", "two").unwrap();
        assert!(receipt_generations()[1].exists(), "the full generation rotated to .1");
        let len = std::fs::metadata(receipts_log_path()).unwrap().len();
        assert!(len < ROTATE_AT, "receipts.log is {len} bytes, past the {ROTATE_AT} cap");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// N replays of one duplicate inside the hour spool one ack between them,
    /// and the first replay after the hour spools another.
    #[test]
    fn replays_inside_the_hour_spool_one_ack_and_a_replay_after_it_spools_another() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("reack-window");
        let t0 = 1_000_000;
        let mut spooled = 0;
        spooled += usize::from(reack_due("alice-host", "home", "ab", t0).unwrap());
        for i in 1..=5 {
            spooled += usize::from(reack_due("alice-host", "home", "ab", t0 + i).unwrap());
        }
        assert_eq!(spooled, 1, "N replays inside the hour spool one ack");
        assert!(!reack_due("alice-host", "home", "ab", t0 + REACK_WINDOW_SECS - 1).unwrap());
        assert!(reack_due("alice-host", "home", "ab", t0 + REACK_WINDOW_SECS).unwrap(), "a replay after the hour spools another");
        assert!(!reack_due("alice-host", "home", "ab", t0 + REACK_WINDOW_SECS + 1).unwrap());
        assert!(reack_due("alice-host", "other-mesh", "ab", t0).unwrap(), "the mesh is part of the key");
        assert!(reack_due("alice-host", "home", "cd", t0).unwrap(), "the msgid is part of the key");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A first ack noted for a letter holds back its own duplicate inside the hour.
    #[test]
    fn a_noted_first_ack_holds_back_the_next_duplicate() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("reack-noted");
        note_ack("alice-host", "home", "ab", 500).unwrap();
        assert!(!reack_due("alice-host", "home", "ab", 501).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The re-ack record rotates under its cap instead of growing past it.
    #[test]
    fn the_reack_record_rotates_under_its_cap() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("reack-cap");
        std::fs::create_dir_all(mail_dir()).unwrap();
        let mut full = vec![b'x'; REACKS_MAX as usize - 40];
        full.push(b'\n');
        std::fs::write(reacks_log_path(), full).unwrap();
        assert!(reack_due("alice-host", "home", "ab", 1).unwrap());
        assert!(PathBuf::from(format!("{}.1", reacks_log_path().display())).exists(), "the full generation rotated");
        let len = std::fs::metadata(reacks_log_path()).unwrap().len();
        assert!(len < REACKS_MAX, "reacks.log is {len} bytes, past the cap");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_local_receipt_stores_a_digest() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("local-receipt-digest");

        file_receipt("bob", "alice", "the message body").unwrap();
        let got = read_receipts().unwrap();
        assert_eq!(got.len(), 1);
        let expected = format!("sha256:{} 16B", hex_encode(&Sha256::digest(b"the message body")));
        assert_eq!(got[0].envelope.text, expected, "the text is the digest and the length, never the message");
        let e = &got[0].envelope;
        assert_eq!(compute_msgid(&e.header, &e.text, &e.sig).as_deref(), Some(e.msgid.as_str()), "the digest text is what is signed");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A letter from `alice-host` to `to` of `kind`, signed by `alice` (on
    /// record, see [`alice_on_record`]): what a far node deposits.
    fn signed_from_alice(to: &str, kind: &str, text: &str, alice: &identity::Keypair) -> Envelope {
        let mut h = header("alice", to, kind, "2026-09-06T00:00:00Z");
        h.from.node = "alice-host".to_string();
        let (sig, msgid) = seal(&h, text, alice);
        Envelope { header: h, text: text.to_string(), sig, msgid }
    }

    /// A far deposit to a box nobody created is refused `unknown-box`: it
    /// creates no box file and no cursor.
    #[test]
    fn a_far_deposit_to_an_unknown_box_is_refused_and_creates_nothing() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("s5-unknown-box");
        let alice = alice_on_record();
        let outcome = deposit(signed_from_alice("carol", ENTRY_TYPE_LETTER, "hi", &alice), "sakaki").unwrap();
        assert_eq!(outcome, DepositOutcome::UnknownBox);
        assert!(!box_path("carol").exists(), "a far deposit created a box");
        assert!(load_cursors().unwrap().get("carol").is_none(), "a far deposit wrote a cursor");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The local act that creates a box lets the same far letter file.
    #[test]
    fn mail_box_add_then_the_deposit_files() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("s5-box-add");
        let alice = alice_on_record();
        assert!(box_add("carol").unwrap(), "the first add creates the box");
        assert!(!box_add("carol").unwrap(), "a second add finds it and writes nothing");
        assert!(box_path("carol").exists());
        let outcome = deposit(signed_from_alice("carol", ENTRY_TYPE_LETTER, "hi", &alice), "sakaki").unwrap();
        assert!(matches!(outcome, DepositOutcome::Filed { .. }), "{outcome:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Reserved device names and names past 63 bytes are refused on every OS,
    /// by the one grammar, and `box add` refuses them before it writes.
    #[test]
    fn reserved_and_overlong_names_are_refused_on_every_os() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("s5-reserved");
        for name in ["con", "nul", "prn", "aux", "com0", "com1", "lpt9", &"a".repeat(64)] {
            assert!(!valid_box_name(name), "{name} must be refused");
            assert!(box_add(name).is_err(), "box add must refuse {name}");
            assert!(!box_path(name).exists(), "{name} wrote a box");
        }
        assert!(valid_box_name(&"a".repeat(63)), "63 bytes is the limit");
        assert!(valid_box_name("carol"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Only a letter is correspondence a box holds: any other kind is
    /// `not-correspondence`, filed nowhere.
    #[test]
    fn a_far_post_kind_is_not_correspondence() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("s5-post");
        let alice = alice_on_record();
        box_add("carol").unwrap();
        let outcome = deposit(signed_from_alice("carol", "post", "hi", &alice), "sakaki").unwrap();
        assert_eq!(outcome, DepositOutcome::NotCorrespondence);
        assert!(read_file_entries(&box_path("carol")).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Reading a box nobody created writes no cursor, and never creates the box.
    #[test]
    fn a_read_of_an_absent_box_writes_no_cursor() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("s5-absent-read");
        assert!(read_for("ghost", true, None).unwrap().is_empty());
        assert_eq!(mark("ghost", None).unwrap(), 0);
        assert!(load_cursors().unwrap().get("ghost").is_none(), "a read of an absent box wrote a cursor");
        assert!(!box_path("ghost").exists(), "a read created the box");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Enrolling a reader creates the box it reads, so a far letter can land.
    #[test]
    fn enrolling_a_reader_creates_its_box() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("s5-enrol-box");
        enrol_reader("carol", "sess-1").unwrap();
        assert!(box_path("carol").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `box rm` refuses a box with unread mail, and removes nothing, unless
    /// `--force` is passed. A reader's pseudo-reader counts too: a role box is
    /// read by unconducted terminals under its own name.
    #[test]
    fn box_rm_refuses_unread_mail_without_force_and_removes_nothing() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("box-rm-refuse");
        file_letter("alice", "bob", "unread").unwrap();
        let before = std::fs::read(box_path("bob")).unwrap();

        assert!(box_rm("bob", false).is_err(), "unread mail must refuse");
        assert_eq!(std::fs::read(box_path("bob")).unwrap(), before, "a refusal changed the box");

        enrol_reader("bob", "sess-1").unwrap();
        assert!(box_rm("bob", false).is_err(), "a reader behind the top letter must refuse");
        assert!(box_path("bob").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Once every reader has read through the top letter, `box rm` removes the
    /// box's log and its cursor keys, and nothing else's.
    #[test]
    fn box_rm_of_a_read_box_removes_its_log_and_its_cursor_keys() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("box-rm-read");
        file_letter("alice", "bob", "one").unwrap();
        enrol_reader("bob", "sess-1").unwrap();
        read_for("bob", false, Some("sess-1")).unwrap();
        file_letter("alice", "carol", "kept").unwrap();
        enrol_reader("carol", "sess-2").unwrap();

        let removed = box_rm("bob", false).unwrap();
        assert_eq!(removed, BoxRemoval { entries: 1, readers: 1 });
        assert!(!box_path("bob").exists(), "the box's log is gone");
        let cursors = load_cursors().unwrap();
        assert!(cursors.get("bob").is_none(), "the box's cursor keys are gone");
        assert!(cursors.get("carol").is_some_and(|c| c.contains_key(&session_reader_key("sess-2"))), "another box's keys stay");
        assert!(box_path("carol").exists(), "another box stays");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `--force` removes unread letters too, which is the explicit lever.
    #[test]
    fn box_rm_force_removes_unread_letters() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("box-rm-force");
        file_letter("alice", "bob", "unread").unwrap();
        assert_eq!(box_rm("bob", true).unwrap(), BoxRemoval { entries: 1, readers: 0 });
        assert!(!box_path("bob").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The removal is audited, and an audit that cannot be written stops it: a
    /// box is never removed without the line that says so.
    #[test]
    fn box_rm_is_audited_and_does_not_remove_what_it_cannot_audit() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("box-rm-audit");
        file_letter("alice", "bob", "one").unwrap();

        let log = aoide_protocol::audit::default_audit_log();
        std::fs::create_dir_all(&log).unwrap();
        assert!(box_rm("bob", true).is_err(), "no audit line, no removal");
        assert!(box_path("bob").exists(), "the box stays when its audit cannot be written");

        std::fs::remove_dir_all(&log).unwrap();
        box_rm("bob", true).unwrap();
        let line = std::fs::read_to_string(&log).unwrap();
        assert!(line.contains("mail.box.rm") && line.contains("bob"), "{line}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A migrated base cannot push a box past its cap: a line its box has no room
    /// for is parked in `_legacy`, and one that fits neither stays in the archive.
    #[test]
    fn a_migrated_box_never_grows_past_box_max() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("s7b-migrate-cap");
        let proto = letter_entry("bob", 0, &"x".repeat(1000));
        let caps = small_caps(20_000);
        let mut body = String::new();
        for i in 0..60u64 {
            let mut e = proto.clone();
            e.seq = i + 1;
            e.envelope.msgid = format!("{:064x}", i + 1);
            body.push_str(&serde_json::to_string(&e).unwrap());
            body.push('\n');
        }
        std::fs::create_dir_all(mail_dir()).unwrap();
        std::fs::write(base_path(), body).unwrap();

        // `with_lock` would migrate under the store's own caps first, so the test
        // takes the same lock and runs the migration under the small ones.
        crate::fs::try_stage_lock(|| {
            ensure_mail_dir()?;
            migrate_base_within(caps)
        })
        .unwrap()
        .unwrap();
        let bob = std::fs::metadata(box_path("bob")).unwrap().len();
        assert!(bob <= caps.box_max, "the migrated box holds {bob} bytes, past the cap");
        let parked = std::fs::metadata(legacy_box_path()).map(|m| m.len()).unwrap_or(0);
        assert!(parked > 0, "the overflow is parked in _legacy");
        let live = read_entries_unlocked().unwrap().len();
        assert!(live < 60, "the caps refuse some lines: {live} filed");
        let archived = std::fs::read_to_string(mail_dir().join("base.jsonl.migrated")).unwrap().lines().count();
        assert_eq!(archived, 60, "every line the migration could not file is still in the archive");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `_legacy` is removable by its own name, the one name the grammar refuses
    /// that `box rm` accepts; every other refused name stays refused.
    #[test]
    fn box_rm_removes_the_legacy_box_by_its_own_name() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("s7b-legacy-rm");
        append_entry(&letter_entry("Bad Name", 1, "grandfathered")).unwrap();
        assert!(legacy_box_path().exists());
        assert_eq!(box_rm(LEGACY_BOX, true).unwrap(), BoxRemoval { entries: 1, readers: 0 });
        assert!(!legacy_box_path().exists(), "the legacy box is gone");
        assert!(box_rm("Bad Name", true).is_err(), "any other refused name stays refused");
        assert!(box_add(LEGACY_BOX).is_err(), "and the legacy box is never created by add");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A removal that fails after its audit line writes a second line that says
    /// so: the log never shows a box removed that is still there.
    #[cfg(unix)]
    #[test]
    fn a_failed_box_removal_writes_a_second_audit_line() {
        use std::os::unix::fs::PermissionsExt;
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("s7b-rm-failed");
        file_letter("alice", "bob", "one").unwrap();
        let log = aoide_protocol::audit::default_audit_log();
        let boxes = boxes_dir();
        std::fs::set_permissions(&boxes, std::fs::Permissions::from_mode(0o500)).unwrap();

        let result = box_rm("bob", true);
        std::fs::set_permissions(&boxes, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(result.is_err(), "the removal fails");
        assert!(box_path("bob").exists(), "the box is still there");
        let lines = std::fs::read_to_string(&log).unwrap();
        assert!(lines.contains("removing mailbox `bob`"), "the first line: {lines}");
        assert!(lines.contains("failed") && lines.contains("`bob`"), "the second line says it failed: {lines}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A daemon start makes the conductor's box, and a far letter files into it,
    /// while a far letter to a name no local act created is refused.
    #[test]
    fn the_daemon_start_makes_the_conductor_box_and_no_far_name() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("auto-boxes");
        ensure_auto_boxes().unwrap();
        assert!(box_exists(CONDUCTOR_BOX));
        let alice = key_on_record("alice-host");
        assert!(matches!(
            deposit(signed_from("alice-host", CONDUCTOR_BOX, ENTRY_TYPE_LETTER, "hi", &alice), "sakaki").unwrap(),
            DepositOutcome::Filed { .. }
        ));
        assert_eq!(
            deposit(signed_from("alice-host", "ghost", ENTRY_TYPE_LETTER, "hi", &alice), "sakaki").unwrap(),
            DepositOutcome::UnknownBox
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The refusal details are fixed text that names no box, so an answer cannot
    /// say which boxes exist: a box's name is in none of them.
    #[test]
    fn the_refusal_detail_never_names_the_box() {
        for name in ["bob", "zebra-9"] {
            for detail in [BOX_FULL_DETAIL, UNKNOWN_BOX_DETAIL, TOO_LARGE_DETAIL] {
                assert!(!detail.contains(name), "{detail} names {name}");
            }
        }
    }

    /// A box nobody created is not there to remove, and nothing is created.
    #[test]
    fn box_rm_of_an_absent_box_is_refused_and_creates_nothing() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("box-rm-absent");
        assert!(box_rm("ghost", true).is_err());
        assert!(!box_path("ghost").exists());
        assert!(box_rm("con", true).is_err(), "a name the grammar refuses is refused");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A signed envelope from `node` to `to` of `kind`, signed by `kp` (on record
    /// for `node`, see [`key_on_record`]): what a far node deposits.
    fn signed_from(node: &str, to: &str, kind: &str, text: &str, kp: &identity::Keypair) -> Envelope {
        let mut h = header("alice", to, kind, "2026-09-06T00:00:00Z");
        h.from.node = node.to_string();
        let (sig, msgid) = seal(&h, text, kp);
        Envelope { header: h, text: text.to_string(), sig, msgid }
    }

    /// A fresh key on record for `node`, so its letters verify.
    fn key_on_record(node: &str) -> identity::Keypair {
        let kp = identity::mint_ephemeral().unwrap();
        same_key_on_record(node, &kp);
        kp
    }

    /// `kp` on record for `node`, which may be a second name for one key.
    fn same_key_on_record(node: &str, kp: &identity::Keypair) {
        let mut nodes = crate::node_store::load_nodes();
        crate::node_store::upsert_paired_node(
            &mut nodes,
            node,
            "https://far",
            &kp.info().pubkey_hex,
            "2026-09-06T00:00:00Z",
            &["message".to_string()],
            "home",
        );
        crate::node_store::save_nodes(&nodes).unwrap();
    }

    /// Pads `name`'s box file to exactly [`BOX_MAX`] with lines that do not parse:
    /// a full box, with no letters for compaction to reclaim.
    fn fill_box_to_the_cap(name: &str) {
        std::fs::create_dir_all(boxes_dir()).unwrap();
        let line = format!("{}\n", "x".repeat(1023));
        std::fs::write(box_path(name), line.repeat((BOX_MAX / 1024) as usize)).unwrap();
        assert_eq!(std::fs::metadata(box_path(name)).unwrap().len(), BOX_MAX);
    }

    fn small_caps(box_max: u64) -> Caps {
        Caps { box_max, boxes_max: u64::MAX / 2 }
    }

    #[test]
    fn admit_refuses_a_line_past_the_box_cap() {
        assert_eq!(admit(CAPS, BOX_MAX - 10, None, 0, 10), Ok(()));
        assert_eq!(admit(CAPS, BOX_MAX - 9, None, 0, 10), Err(Full::Box));
    }

    #[test]
    fn admit_refuses_past_a_quarter_of_the_box_for_one_origin() {
        let quarter = BOX_MAX / 4;
        let held = |in_box| Some(Held { in_box, in_store: 0 });
        assert_eq!(admit(CAPS, 0, held(quarter - 10), 0, 10), Ok(()));
        assert_eq!(admit(CAPS, 0, held(quarter - 9), 0, 10), Err(Full::Origin));
    }

    #[test]
    fn admit_refuses_past_a_quarter_of_the_store_for_one_origin() {
        let quarter = BOXES_MAX / 4;
        let held = |in_store| Some(Held { in_box: 0, in_store });
        assert_eq!(admit(CAPS, 0, held(quarter - 10), 0, 10), Ok(()));
        assert_eq!(admit(CAPS, 0, held(quarter - 9), 0, 10), Err(Full::OriginTotal));
    }

    #[test]
    fn admit_refuses_past_the_store_total() {
        assert_eq!(admit(CAPS, 0, None, BOXES_MAX - 10, 10), Ok(()));
        assert_eq!(admit(CAPS, 0, None, BOXES_MAX - 9, 10), Err(Full::Total));
    }

    #[test]
    fn admit_exempts_a_local_letter_from_the_quarter() {
        let quarter = BOX_MAX / 4;
        assert_eq!(admit(CAPS, 0, None, 0, quarter + 1), Ok(()), "a local letter has no quarter, only the box cap");
    }

    /// A letter over 256 KiB serialized is refused at the mint and at the door,
    /// and the door reads no box first.
    #[test]
    fn a_letter_over_256k_is_refused_too_large_at_mint_and_at_the_door() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("s6-too-large");
        let big = "a".repeat(300_000);
        let me = crate::display::local_node_name();
        let minted = mint_outbound_letter("alice", &me, "bob", &big);
        assert!(minted.is_err(), "the mint refuses an over-cap letter");
        assert!(minted.unwrap_err().contains("over the 262144-byte cap"));

        let alice = key_on_record("alice-host");
        let outcome = deposit(signed_from("alice-host", "bob", ENTRY_TYPE_LETTER, &big, &alice), "sakaki").unwrap();
        assert_eq!(outcome, DepositOutcome::TooLarge);
        assert!(!box_path("bob").exists(), "an over-cap letter creates and reads no box");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A full box refuses `box-full` before any line is written: the box is
    /// byte-for-byte unchanged, and the letter is not `Filed`, so no ack is owed
    /// (the ack is spooled only on `Filed`, `client::mail_wire::settle_deposit`).
    #[test]
    fn a_full_box_refuses_box_full_before_filing_and_before_any_ack() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("s6-box-full");
        box_add("bob").unwrap();
        fill_box_to_the_cap("bob");
        let before = std::fs::read(box_path("bob")).unwrap();
        let alice = key_on_record("alice-host");

        let outcome = deposit(signed_from("alice-host", "bob", ENTRY_TYPE_LETTER, "hi", &alice), "sakaki").unwrap();
        assert_eq!(outcome, DepositOutcome::BoxFull);
        assert_eq!(std::fs::read(box_path("bob")).unwrap(), before, "a full box took a line");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// One origin key may hold a quarter of a box. Past it, that key is refused
    /// `box-full` while the box itself still has room, and another key still files
    /// into the same box.
    #[test]
    fn one_origin_key_is_held_to_a_quarter_while_another_files() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("s6-quarter");
        box_add("bob").unwrap();
        let alice = key_on_record("alice-host");
        let bob_host = key_on_record("bob-host");
        let caps = small_caps(20_000);

        let mut filed = 0;
        while filed < 50 {
            let letter = signed_from("alice-host", "bob", ENTRY_TYPE_LETTER, &format!("alice {filed}"), &alice);
            match deposit_within(letter, "sakaki", caps).unwrap() {
                DepositOutcome::Filed { .. } => filed += 1,
                DepositOutcome::BoxFull => break,
                other => panic!("{other:?}"),
            }
        }
        assert!(filed >= 2, "a quarter holds more than one letter: {filed}");
        let len = std::fs::metadata(box_path("bob")).unwrap().len();
        assert!(len < caps.box_max, "the refusal came from the quarter, not the box: {len} bytes");
        assert!(matches!(
            deposit_within(signed_from("bob-host", "bob", ENTRY_TYPE_LETTER, "bob", &bob_host), "sakaki", caps).unwrap(),
            DepositOutcome::Filed { .. }
        ), "another key files into the same box");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A hop's name is never a reason to skip the quarter. A far letter whose
    /// hop is named `self` is held to its origin's quarter like any other, and
    /// the box holds no more than a quarter of the far key's bytes.
    #[test]
    fn a_far_hop_named_self_is_held_to_the_quarter() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("s9-hop-self");
        box_add("bob").unwrap();
        let alice = key_on_record("alice-host");
        let caps = small_caps(20_000);

        let mut filed = 0;
        while filed < 50 {
            let letter = signed_from("alice-host", "bob", ENTRY_TYPE_LETTER, &format!("alice {filed}"), &alice);
            match deposit_within(letter, "self", caps).unwrap() {
                DepositOutcome::Filed { .. } => filed += 1,
                DepositOutcome::BoxFull => break,
                other => panic!("{other:?}"),
            }
        }
        let len = std::fs::metadata(box_path("bob")).unwrap().len();
        assert!(len <= caps.box_max / 4, "a far hop named self took {len} bytes past the quarter");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// This box's own signed letter is exempt from the quarter whatever hop
    /// brought it, so the box fills to its cap, not to a quarter.
    #[test]
    fn a_letter_this_box_signed_is_exempt_from_the_quarter() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("s9-own-key");
        box_add("bob").unwrap();
        let me = crate::display::local_node_name();
        let (own, _) = identity::load_or_mint().unwrap();
        same_key_on_record(&me, &own);
        let caps = small_caps(20_000);

        let mut filed = 0;
        while filed < 200 {
            let letter = signed_from(&me, "bob", ENTRY_TYPE_LETTER, &format!("mine {filed}"), &own);
            match deposit_within(letter, "sakaki", caps).unwrap() {
                DepositOutcome::Filed { .. } => filed += 1,
                DepositOutcome::BoxFull => break,
                other => panic!("{other:?}"),
            }
        }
        let len = std::fs::metadata(box_path("bob")).unwrap().len();
        assert!(len > caps.box_max / 4, "its own letters are not held to the quarter: {len} bytes");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// One origin key may hold a quarter of the STORE across every box, not only
    /// a quarter of each box: a key that fills sixty boxes in turn is refused once
    /// it holds the store's quarter, and the store keeps room for another key.
    #[test]
    fn one_key_filling_sixty_boxes_is_held_to_a_quarter_of_the_store() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("s6-store-quarter");
        let names: Vec<String> = (0..60).map(|i| format!("b{i:02}")).collect();
        for name in &names {
            box_add(name).unwrap();
        }
        let alice = key_on_record("alice-host");
        let bob = key_on_record("bob-host");
        let caps = Caps { box_max: 20_000, boxes_max: 40_000 };

        let mut refused = false;
        'fill: for round in 0..3 {
            for name in &names {
                let letter = signed_from("alice-host", name, ENTRY_TYPE_LETTER, &format!("alice {round}"), &alice);
                match deposit_within(letter, "sakaki", caps).unwrap() {
                    DepositOutcome::Filed { .. } => {}
                    DepositOutcome::BoxFull => {
                        refused = true;
                        break 'fill;
                    }
                    other => panic!("{other:?}"),
                }
            }
        }
        assert!(refused, "one key was never refused");
        let total: u64 = box_paths().unwrap().iter().map(|p| std::fs::metadata(p).unwrap().len()).sum();
        assert!(total <= caps.boxes_max / 4, "one key held {total} bytes of the store");
        assert!(matches!(
            deposit_within(signed_from("bob-host", "b00", ENTRY_TYPE_LETTER, "bob", &bob), "sakaki", caps).unwrap(),
            DepositOutcome::Filed { .. }
        ), "another key still files");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The store quarter is the verified key's: one key under two node names
    /// shares it, so the second name is refused where it would otherwise file.
    #[test]
    fn one_key_under_two_names_shares_the_store_quarter() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("s6-store-quarter-two-names");
        let spread = ["b00", "b01", "b02", "b03"];
        for name in spread.iter().chain(["b10"].iter()) {
            box_add(name).unwrap();
        }
        let alice = key_on_record("alice-host");
        same_key_on_record("alice-two", &alice);
        let caps = Caps { box_max: 20_000, boxes_max: 40_000 };

        // Spread over four boxes, so each box's own quarter stays clear and the
        // store's quarter is what refuses.
        let mut filed = 0;
        for round in 0..200 {
            let letter = signed_from("alice-host", spread[round % spread.len()], ENTRY_TYPE_LETTER, &format!("one {round}"), &alice);
            match deposit_within(letter, "sakaki", caps).unwrap() {
                DepositOutcome::Filed { .. } => filed += 1,
                DepositOutcome::BoxFull => break,
                other => panic!("{other:?}"),
            }
        }
        assert!(filed >= 2, "the first name files: {filed}");
        assert_eq!(
            deposit_within(signed_from("alice-two", "b10", ENTRY_TYPE_LETTER, "two", &alice), "sakaki", caps).unwrap(),
            DepositOutcome::BoxFull,
            "the second name shares the first name's store quarter"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The quarter is the verified key's, not the name's: one key that signs
    /// under two node names is one sender, so it shares one quarter.
    #[test]
    fn one_key_under_two_mesh_names_gets_one_quarter() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("s6-one-key");
        box_add("bob").unwrap();
        let alice = key_on_record("alice-host");
        same_key_on_record("alice-two", &alice);
        let caps = small_caps(20_000);

        let mut filed = 0;
        while filed < 50 {
            let letter = signed_from("alice-host", "bob", ENTRY_TYPE_LETTER, &format!("one {filed}"), &alice);
            match deposit_within(letter, "sakaki", caps).unwrap() {
                DepositOutcome::Filed { .. } => filed += 1,
                DepositOutcome::BoxFull => break,
                other => panic!("{other:?}"),
            }
        }
        assert!(filed >= 2, "the first name files: {filed}");
        assert_eq!(
            deposit_within(signed_from("alice-two", "bob", ENTRY_TYPE_LETTER, "two", &alice), "sakaki", caps).unwrap(),
            DepositOutcome::BoxFull,
            "the second name shares the first name's quarter"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Compaction removes only the lines every reader of their box has read.
    #[test]
    fn compaction_reclaims_only_lines_every_reader_has_read() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("s6-compact");
        for text in ["one", "two", "three"] {
            file_letter("alice", "bob", text).unwrap();
        }
        enrol_reader("bob", "r1").unwrap();
        let mut cursors = load_cursors().unwrap();
        cursors.get_mut("bob").unwrap().get_mut(&session_reader_key("r1")).unwrap().seq = 2;
        save_cursors(&cursors).unwrap();

        assert!(compact_unlocked(&box_path("bob"), &load_cursors().unwrap()).unwrap(), "two lines are read");
        let left: Vec<u64> = box_lines(&box_path("bob")).unwrap().iter().filter_map(|l| l.entry.as_ref()).map(|e| e.seq).collect();
        assert_eq!(left, vec![3], "only the unread line remains, and it keeps its seq");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Under pressure, an unread letter is never reclaimed: with no reader past it
    /// the floor is zero, and the flood is refused instead.
    #[test]
    fn an_unread_letter_survives_compaction_under_pressure() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("s6-unread-survives");
        let alice = key_on_record("alice-host");
        box_add("bob").unwrap();
        enrol_reader("bob", "r1").unwrap();
        assert!(matches!(
            deposit_within(signed_from("alice-host", "bob", ENTRY_TYPE_LETTER, "unread", &alice), "sakaki", small_caps(1 << 20)).unwrap(),
            DepositOutcome::Filed { .. }
        ));
        let len = std::fs::metadata(box_path("bob")).unwrap().len();
        let tight = small_caps(len + 10);
        let flood = signed_from("alice-host", "bob", ENTRY_TYPE_LETTER, "flood", &alice);
        assert_eq!(deposit_within(flood, "sakaki", tight).unwrap(), DepositOutcome::BoxFull);
        let kept: Vec<u64> = box_lines(&box_path("bob")).unwrap().iter().filter_map(|l| l.entry.as_ref()).map(|e| e.seq).collect();
        assert_eq!(kept, vec![1], "the unread letter is still there");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The store cap reclaims read lines from other boxes before it refuses: a
    /// letter that would not fit beside carol's read mail fits once carol's read
    /// mail is compacted.
    #[test]
    fn the_boxes_cap_compacts_other_boxes_before_refusing() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("s6-total");
        // Carol's lines are longer than the letter, so three of them outweigh it.
        for text in ["1", "2", "3"] {
            file_letter("alice", "carol", &format!("{text}{}", "c".repeat(200))).unwrap();
        }
        enrol_reader("carol", "r").unwrap();
        read_for("carol", false, Some("r")).unwrap();
        let carol_len = std::fs::metadata(box_path("carol")).unwrap().len();
        box_add("bob").unwrap();
        let alice = key_on_record("alice-host");
        let letter = signed_from("alice-host", "bob", ENTRY_TYPE_LETTER, "fits after carol", &alice);
        // The store is four of the letter's lines: the letter fits its own store
        // quarter, and the store is full only while carol's read mail is in it.
        let line = line_bytes(&letter, "sakaki", &alice.info().pubkey_hex).unwrap();
        assert!(carol_len > 3 * line, "carol's three lines outweigh the letter: {carol_len} vs {line}");
        let caps = Caps { box_max: 1 << 20, boxes_max: 4 * line };
        assert!(matches!(
            deposit_within(letter, "sakaki", caps).unwrap(),
            DepositOutcome::Filed { .. }
        ), "carol's read mail is reclaimed, so the store has room");
        assert!(box_lines(&box_path("carol")).unwrap().is_empty(), "carol's read lines were compacted");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A refused flood against a box nobody has read rewrites nothing: the box
    /// keeps its inode and its mtime, because nothing is reclaimable.
    #[cfg(unix)]
    #[test]
    fn a_refused_flood_rewrites_nothing_when_nothing_is_read() {
        use std::os::unix::fs::MetadataExt;
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("s6-no-rewrite");
        let alice = key_on_record("alice-host");
        box_add("bob").unwrap();
        enrol_reader("bob", "r1").unwrap();
        assert!(matches!(
            deposit_within(signed_from("alice-host", "bob", ENTRY_TYPE_LETTER, "first", &alice), "sakaki", small_caps(1 << 20)).unwrap(),
            DepositOutcome::Filed { .. }
        ));
        let before = std::fs::metadata(box_path("bob")).unwrap();
        let tight = small_caps(before.len() + 10);
        for i in 0..5 {
            let flood = signed_from("alice-host", "bob", ENTRY_TYPE_LETTER, &format!("flood {i}"), &alice);
            assert_eq!(deposit_within(flood, "sakaki", tight).unwrap(), DepositOutcome::BoxFull);
        }
        let after = std::fs::metadata(box_path("bob")).unwrap();
        assert_eq!(after.ino(), before.ino(), "the box was replaced");
        assert_eq!(after.len(), before.len());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A local send to a full box returns the way out, not a bare refusal.
    #[test]
    fn local_send_to_a_full_box_teaches_the_fix() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("s6-teach");
        box_add("bob").unwrap();
        fill_box_to_the_cap("bob");
        let err = file_letter("alice", "bob", "hi").unwrap_err();
        assert!(err.contains("aoide mail read --for bob") && err.contains("aoide mail box rm bob --force"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Reaping retires a dead session's reader key and keeps the pseudo-reader,
    /// the key equal to its box's name, which no session owns.
    #[test]
    fn pseudo_readers_are_kept() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("s7-pseudo");
        file_letter("alice", "bob", "hi").unwrap();
        read_for("bob", false, None).unwrap();
        enrol_reader("bob", "dead").unwrap();

        let retired = retire_readers(&std::collections::HashSet::new()).unwrap();
        assert_eq!(retired, vec![("bob".to_string(), "dead".to_string())]);
        let readers: Vec<String> = load_cursors().unwrap()["bob"].keys().cloned().collect();
        assert_eq!(readers, vec!["bob".to_string()], "the pseudo-reader keeps its mark");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A dead reader that never read pins its box at floor zero, so a deposit is
    /// refused. Once reaping retires it, the live reader's mark is the floor and
    /// the same deposit compacts the read letters away and files.
    #[test]
    fn a_box_pinned_by_a_dead_reader_compacts_after_reap() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("s7-pinned");
        let alice = key_on_record("alice-host");
        box_add("bob").unwrap();
        enrol_reader("bob", "live").unwrap();
        enrol_reader("bob", "dead").unwrap();
        // Local letters carry no quarter, so eight of them fill the box past
        // anything a far sender's quarter would refuse on its own.
        for i in 0..8 {
            file_letter("carol", "bob", &format!("read {i}")).unwrap();
        }
        read_for("bob", false, Some("live")).unwrap();
        let tight = small_caps(std::fs::metadata(box_path("bob")).unwrap().len() + 10);
        let next = || signed_from("alice-host", "bob", ENTRY_TYPE_LETTER, "next", &alice);
        assert_eq!(deposit_within(next(), "sakaki", tight).unwrap(), DepositOutcome::BoxFull, "the dead reader pins the box");

        retire_readers(&["live"].into_iter().collect()).unwrap();
        assert!(matches!(deposit_within(next(), "sakaki", tight).unwrap(), DepositOutcome::Filed { .. }));
        let left: Vec<String> =
            box_lines(&box_path("bob")).unwrap().iter().filter_map(|l| l.entry.as_ref()).map(|e| e.envelope.text.clone()).collect();
        assert_eq!(left, vec!["next".to_string()], "the read letters were compacted away");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A box whose every reader was retired is dead: a far letter to it is
    /// refused `unknown-box` and the origin keeps it. A box no reader ever
    /// touched, and a box a pseudo-reader still holds, are not dead.
    #[test]
    fn a_far_deposit_to_a_dead_box_is_refused_and_a_live_box_still_files() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("dead-box-deposit");
        let alice = key_on_record("alice-host");
        box_add("erin").unwrap();
        enrol_reader("erin", "gone").unwrap();
        retire_readers(&std::collections::HashSet::new()).unwrap();
        let to_erin = signed_from("alice-host", "erin", ENTRY_TYPE_LETTER, "hello?", &alice);
        assert_eq!(deposit(to_erin, "sakaki").unwrap(), DepositOutcome::UnknownBox, "a dead box refuses");

        box_add("frank").unwrap();
        let to_frank = signed_from("alice-host", "frank", ENTRY_TYPE_LETTER, "untouched", &alice);
        assert!(matches!(deposit(to_frank, "sakaki").unwrap(), DepositOutcome::Filed { .. }), "a box no reader touched files");

        box_add("gina").unwrap();
        read_for("gina", false, None).unwrap();
        enrol_reader("gina", "gone").unwrap();
        retire_readers(&std::collections::HashSet::new()).unwrap();
        let to_gina = signed_from("alice-host", "gina", ENTRY_TYPE_LETTER, "pseudo", &alice);
        assert!(matches!(deposit(to_gina, "sakaki").unwrap(), DepositOutcome::Filed { .. }), "a pseudo-reader keeps its box alive");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Reap deletes a dead box once it holds no letter, and says so in the audit
    /// log. A dead box that still holds mail stays, and a resurrected session's
    /// box is created again from its first deposit.
    #[test]
    fn reap_deletes_an_empty_dead_box_audited_and_keeps_one_holding_mail() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("dead-box-reap");
        let alice = key_on_record("alice-host");
        box_add("erin").unwrap();
        enrol_reader("erin", "gone").unwrap();
        box_add("hank").unwrap();
        file_letter("alice", "hank", "mail stays").unwrap();
        enrol_reader("hank", "gone").unwrap();
        retire_readers(&std::collections::HashSet::new()).unwrap();

        assert_eq!(reap_dead_boxes().unwrap(), vec!["erin".to_string()], "only the empty dead box goes");
        assert!(!box_path("erin").exists(), "the empty dead box is deleted");
        assert!(box_path("hank").exists(), "a dead box holding mail stays");
        assert!(!load_cursors().unwrap().contains_key("erin"), "its cursor key goes with it");
        let line = std::fs::read_to_string(aoide_protocol::audit::default_audit_log()).unwrap();
        assert!(line.contains("mail.box.reap") && line.contains("erin"), "{line}");

        ensure_session_box("erin");
        ensure_session_box("not a name");
        assert!(box_exists("erin"), "a resurrected session gets its box back");
        assert!(!box_exists("not a name"), "a name the grammar refuses has no box, and no error");
        let to_erin = signed_from("alice-host", "erin", ENTRY_TYPE_LETTER, "back", &alice);
        assert!(matches!(deposit(to_erin, "sakaki").unwrap(), DepositOutcome::Filed { .. }), "the new box files");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A migration writes one audit line that says how many lines it left
    /// unfiled in the archive. A line that fits no box is that count.
    #[test]
    fn a_migration_audits_the_unfiled_count_it_leaves_in_the_archive() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("migrate-audit");
        let small = serde_json::to_string(&letter_entry("bob", 1, "fits")).unwrap();
        let big = serde_json::to_string(&letter_entry("bob", 2, &"x".repeat(2000))).unwrap();
        write_base(&[small, big]);

        migrate_base_within(small_caps(1000)).unwrap();
        assert!(mail_dir().join("base.jsonl.migrated").exists(), "the base is archived");
        let line = std::fs::read_to_string(aoide_protocol::audit::default_audit_log()).unwrap();
        assert!(line.contains("mail.migrate") && line.contains("unfiled 1"), "{line}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A session that never read is still reaped: its box is dead, a far letter
    /// to it is refused, and the reap deletes the empty box with an audit line.
    #[test]
    fn a_reaped_session_that_never_read_is_dead_and_its_empty_box_is_reaped() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("session-box-never-read");
        let alice = key_on_record("alice-host");
        ensure_session_box("sess-a");
        assert!(box_exists("sess-a"), "the session's box exists from its first insert");

        retire_readers(&std::collections::HashSet::new()).unwrap();
        let far = signed_from("alice-host", "sess-a", ENTRY_TYPE_LETTER, "late", &alice);
        assert_eq!(deposit(far, "sakaki").unwrap(), DepositOutcome::UnknownBox, "a reaped session's box refuses");

        assert_eq!(reap_dead_boxes().unwrap(), vec!["sess-a".to_string()], "the empty dead box is reaped");
        assert!(!box_path("sess-a").exists(), "its file is gone");
        let line = std::fs::read_to_string(aoide_protocol::audit::default_audit_log()).unwrap();
        assert!(line.contains("mail.box.reap") && line.contains("sess-a"), "{line}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A live session that never read still files: the roster, not a read, says
    /// whether its box is dead.
    #[test]
    fn a_live_session_that_never_read_still_files() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("session-box-live");
        let alice = key_on_record("alice-host");
        ensure_session_box("sess-b");

        retire_readers(&["sess-b"].into_iter().collect()).unwrap();
        let far = signed_from("alice-host", "sess-b", ENTRY_TYPE_LETTER, "hello", &alice);
        assert!(matches!(deposit(far, "sakaki").unwrap(), DepositOutcome::Filed { .. }), "a live session's box files");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A re-inserted session gets a live box back: after a reap deleted it, the
    /// same far letter files again.
    #[test]
    fn a_reinserted_session_files_again_after_its_box_was_reaped() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("session-box-reinsert");
        let alice = key_on_record("alice-host");
        ensure_session_box("sess-c");
        retire_readers(&std::collections::HashSet::new()).unwrap();
        assert_eq!(reap_dead_boxes().unwrap(), vec!["sess-c".to_string()]);

        ensure_session_box("sess-c");
        let far = signed_from("alice-host", "sess-c", ENTRY_TYPE_LETTER, "back", &alice);
        assert!(matches!(deposit(far, "sakaki").unwrap(), DepositOutcome::Filed { .. }), "the re-inserted session files");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A reaped session's box that still holds mail stays, refuses far letters,
    /// and the reap drops none of its letters.
    #[test]
    fn a_reaped_session_box_holding_mail_stays_and_drops_nothing() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("session-box-holds-mail");
        let alice = key_on_record("alice-host");
        ensure_session_box("sess-d");
        file_letter("alice", "sess-d", "keep me").unwrap();

        retire_readers(&std::collections::HashSet::new()).unwrap();
        let far = signed_from("alice-host", "sess-d", ENTRY_TYPE_LETTER, "late", &alice);
        assert_eq!(deposit(far, "sakaki").unwrap(), DepositOutcome::UnknownBox, "a dead box with mail still refuses");
        assert_eq!(reap_dead_boxes().unwrap(), Vec::<String>::new(), "a box holding mail is not reaped");
        let texts: Vec<String> =
            read_file_entries(&box_path("sess-d")).unwrap().into_iter().map(|e| e.envelope.text).collect();
        assert_eq!(texts, vec!["keep me".to_string()], "no letter is dropped");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
