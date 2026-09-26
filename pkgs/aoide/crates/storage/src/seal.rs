//! `seal` — the P-SEAL container and the signed age key binding
//! (`docs/architecture/HTTPS-MESH-API.md` "Container", "Encodings" and
//! "Keys", MAIL.md's P-SEAL slice).
//!
//! **The one module that touches `age`.** The workspace's `age` entry
//! exists for this file alone; deleting `seal.rs` and that manifest line
//! removes the dependency from the tree.
//!
//! ## What is here
//!
//! - [`frame`], the ONE encoding primitive every signed or hashed byte in
//!   this design is built from: a NUL-terminated label then a `u32`
//!   big-endian length and its bytes for every field, no field ever
//!   omitted. The design's reserved label table is the `L_*` constants
//!   below; a new frame adds a label here and nowhere else.
//! - The node's own age X25519 key ([`age_key_path`],
//!   [`load_or_mint_age_identity`], [`rotate_age_key`]) and the
//!   **self-signed binding** ([`Binding`]) that publishes it: a monotonic
//!   `generation`, a validity window, the accepted suites, and the
//!   identity-key fingerprint it is keyed by.
//! - [`Container`], the outer object a letter or post travels in, and the
//!   two verification halves [`deposit_container`] implements: the shared
//!   keyless steps, then the destination branch that opens, matches and
//!   files.
//!
//! ## What is deliberately NOT here
//!
//! **No private key ever enters a `Serialize`/`Deserialize` type.** The
//! age key is written as the stock `AGE-SECRET-KEY-1…` text an
//! `age-keygen` produces, into `state/identity/age.key` at `0600` via the
//! same [`crate::fs`] helpers `identity.rs` uses, and read back through
//! `age::x25519::Identity`'s own parser. Nothing in this module holds a
//! secret in a struct that derives `Serialize`, so the
//! `no_private_material_in_any_serialize_type` discipline `identity.rs`
//! established holds here by construction.
//!
//! **No hand-rolled cryptography.** Every primitive is `age`'s
//! (X25519 + ChaCha20-Poly1305 STREAM) or `sha2`'s; the only thing this
//! module composes is the documented signature-and-`ctx` binding the
//! design fixes.
//!
//! **`age_fingerprint` is `sha256` of the canonical bech32 recipient
//! string**, not of raw key bytes. `age::x25519::Recipient` exposes its
//! key only as that string (`Display` re-encodes it from the key, so the
//! string is injective in the key and already lowercase-canonical), and
//! reaching the raw bytes would mean either a second direct cryptography
//! dependency or a hand-rolled bech32 decoder — both refused.

use crate::fs;
use crate::identity;
use crate::mail;
use crate::node_store;
use crate::time::now_iso_utc;
use crate::wire_auth;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io;
use std::path::PathBuf;

// ── Frame labels: the design's reserved table, and the one place a frame
// ── is named. Adding a frame touches this block and nothing else. ────────

const L_CTX: &str = "aoide/mail-ctx";
const L_PT: &str = "aoide/mail-pt";
const L_ENVELOPE: &str = "aoide/mail-envelope";
const L_OUTER: &str = "aoide/mail-outer";
const L_BINDING: &str = "aoide/mail-binding";
const L_SUITES: &str = "aoide/mail-suites";
const L_DEDUP: &str = "aoide/mail-dedup";
const L_HOP: &str = "aoide/mail-hop";
const L_HOP_ENTRY: &str = "aoide/mail-hop-entry";

// ── Registered identifiers ──────────────────────────────────────────────

/// The container frame version. Separate from `suite`: this versions the
/// encoding, `suite` versions the cipher.
pub const CONTAINER_VERSION: u64 = 1;
/// The binding frame version.
pub const BINDING_VERSION: u64 = 1;

/// The one registered suite identifier ("Encodings"): the age v1 format
/// with X25519 recipients, named as one atom so a downgrade cannot be
/// expressed by mixing a format version with a recipient type.
pub const SUITE_AGE_V1_X25519: &str = "age-v1-x25519";
/// The accepted-suite list a binding publishes: one entry.
pub const ACCEPTED_SUITES: &[&str] = &[SUITE_AGE_V1_X25519];

/// The binding's `purpose` field, the design's "(mail sealing)".
pub const BINDING_PURPOSE: &str = "mail-sealing";

/// A container whose payload is a letter or a receipt.
pub const PURPOSE_MAIL: &str = "mail";
/// A container whose payload is a board post.
pub const PURPOSE_POST: &str = "post";
/// A container whose payload is a board epoch key wrapped to a member node.
pub const PURPOSE_WRAP: &str = "wrap";
/// A container whose payload is a charter file and its signature.
pub const PURPOSE_CHARTER: &str = "charter";

/// Every `purpose` value this version defines. A container naming anything
/// else is refused.
pub const PURPOSES: &[&str] = &[PURPOSE_MAIL, PURPOSE_POST, PURPOSE_WRAP, PURPOSE_CHARTER];

// ── The primitive ───────────────────────────────────────────────────────

/// `label ‖ 0x00 ‖ (len(f) as u32 BE ‖ f)*` — the one encoding every
/// signed or hashed byte in this design is built from.
///
/// Injective by construction: the parse (read to the first NUL for the
/// label, then read a `u32` big-endian length and that many bytes, until
/// the input is exhausted) is total and deterministic, so re-encoding what
/// was parsed reproduces the input byte for byte. The `u32` prefix is
/// prefix-free, which kills the `("ab","c")` versus `("a","bc")` collision
/// at every nesting depth, and the NUL terminator makes labels
/// prefix-free too.
pub fn frame(label: &str, fields: &[&[u8]]) -> Vec<u8> {
    debug_assert!(!label.contains('\0'), "a frame label is NUL-free");
    let mut out = Vec::with_capacity(label.len() + 1 + fields.iter().map(|f| f.len() + 4).sum::<usize>());
    out.extend_from_slice(label.as_bytes());
    out.push(0);
    for field in fields {
        out.extend_from_slice(&(field.len() as u32).to_be_bytes());
        out.extend_from_slice(field);
    }
    out
}

/// Parse a frame, requiring exactly `label` and returning its fields.
/// Every failure — a wrong label, a truncated length, a field running past
/// the end, trailing bytes that do not form a field — collapses to one
/// error string, because a caller can do nothing different about any of
/// them.
fn parse_frame<'a>(label: &str, bytes: &'a [u8]) -> Result<Vec<&'a [u8]>, String> {
    let mut it = bytes.iter().position(|b| *b == 0).ok_or_else(|| "frame: no label terminator".to_string())?;
    if &bytes[..it] != label.as_bytes() {
        return Err("frame: wrong label".to_string());
    }
    it += 1;
    let mut fields = Vec::new();
    while it < bytes.len() {
        if it + 4 > bytes.len() {
            return Err("frame: truncated length".to_string());
        }
        let len = u32::from_be_bytes([bytes[it], bytes[it + 1], bytes[it + 2], bytes[it + 3]]) as usize;
        it += 4;
        if it + len > bytes.len() {
            return Err("frame: truncated field".to_string());
        }
        fields.push(&bytes[it..it + len]);
        it += len;
    }
    Ok(fields)
}

pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// Lowercase hex, no separator — this crate's established per-module copy
/// over a shared `pub` utility (`wire_auth`'s and `identity`'s own note).
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(s.len() / 2);
    let mut i = 0;
    while i < b.len() {
        let hi = (b[i] as char).to_digit(16)?;
        let lo = (b[i + 1] as char).to_digit(16)?;
        out.push(((hi << 4) | lo) as u8);
        i += 2;
    }
    Some(out)
}

/// Decode a fixed-width hex field, refusing any other length. The one
/// shape check every hex-typed `ctx` field goes through, so a wire value
/// that is not exactly 32 bytes can never reach a frame.
fn hex_array<const N: usize>(s: &str) -> Result<[u8; N], String> {
    let raw = hex_decode(s).ok_or_else(|| format!("not hex: {s:?}"))?;
    raw.as_slice().try_into().map_err(|_| format!("expected {N} bytes, found {}", raw.len()))
}

// ── The age key ─────────────────────────────────────────────────────────

/// This node's age private key: `state/identity/age.key`, the same
/// directory as `ed25519.key` and locked down the same way.
pub fn age_key_path() -> PathBuf {
    identity::identity_dir().join("age.key")
}

/// The directory retired age keys live in while their grace window runs.
pub fn retired_dir() -> PathBuf {
    identity::identity_dir().join("age-retired")
}

/// Load this node's age identity, minting one on first need.
///
/// Written as the stock `AGE-SECRET-KEY-1…` text an `age-keygen` produces,
/// so a human with the file and the stock tooling can open anything sealed
/// to this node even if Aoide is gone — the recovery property the design
/// buys by choosing age. `minted` is true only on the first mint of a
/// given state directory, the same `changed` signal
/// [`identity::load_or_mint`] gives.
pub fn load_or_mint_age_identity() -> io::Result<(age::x25519::Identity, bool)> {
    let path = age_key_path();
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            let key = text.trim();
            let id: age::x25519::Identity = key.parse().map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "{}: not an age secret key ({e}) — refusing to mint over an existing age key",
                        path.display()
                    ),
                )
            })?;
            Ok((id, false))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            fs::secure_private_dir(&identity::identity_dir())?;
            let id = age::x25519::Identity::generate();
            let secret = age::secrecy::ExposeSecret::expose_secret(&id.to_string()).to_string();
            fs::atomic_write_private(&path, &(secret + "\n").into_bytes())?;
            Ok((id, true))
        }
        Err(e) => Err(e),
    }
}

/// The identity's recipient string (`age1…`), the canonical bech32 form.
pub fn recipient_of(id: &age::x25519::Identity) -> String {
    id.to_public().to_string()
}

/// The durable fingerprint of an age recipient: `sha256` over the
/// canonical bech32 string's bytes ("Encodings"). `Recipient`'s `Display`
/// re-encodes the string from the key, so it is injective in the key and
/// already lowercase-canonical — see this module's header for why the raw
/// bytes are not reachable through the crate's API.
pub fn age_fingerprint(recipient: &str) -> String {
    hex_encode(&sha256(recipient.as_bytes()))
}

// ── The binding ─────────────────────────────────────────────────────────

/// The self-signed age key binding (`HTTPS-MESH-API.md` "Keys"). Keyed by
/// the **identity** key fingerprint, never by a node name: a node in two
/// meshes has two names and one key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Binding {
    pub v: u64,
    pub purpose: String,
    /// The X25519 recipient, canonical bech32.
    pub age_pubkey: String,
    /// Lowercase hex `sha256` of `age_pubkey`'s bytes.
    pub age_fingerprint: String,
    /// The accepted suites, in preference order.
    pub suites: Vec<String>,
    /// Strictly increasing per identity key.
    pub generation: u64,
    pub rotated_at: String,
    pub not_before: String,
    pub not_after: String,
    /// The signer's Ed25519 public key, hex.
    pub identity_key: String,
    /// Lowercase hex, 64 raw signature bytes over [`binding_bytes`].
    pub sig: String,
}

/// The suites list as a labelled frame, so a list can never be read as
/// anything but a list of suite identifiers.
pub fn suites_frame(suites: &[String]) -> Vec<u8> {
    let refs: Vec<&[u8]> = suites.iter().map(|s| s.as_bytes()).collect();
    frame(L_SUITES, &refs)
}

/// The bytes a binding's signature covers. Every field is a typed value:
/// the pubkey and fingerprint enter as their own bytes, never as a
/// rendering, and the identity key as its raw 32 bytes.
fn binding_bytes(b: &Binding) -> Result<Vec<u8>, String> {
    let fingerprint = hex_array::<32>(&b.age_fingerprint)?;
    let identity_key = hex_array::<32>(&b.identity_key)?;
    Ok(frame(
        L_BINDING,
        &[
            &b.v.to_be_bytes(),
            b.purpose.as_bytes(),
            b.age_pubkey.as_bytes(),
            &fingerprint,
            &suites_frame(&b.suites),
            &b.generation.to_be_bytes(),
            b.rotated_at.as_bytes(),
            b.not_before.as_bytes(),
            b.not_after.as_bytes(),
            &identity_key,
        ],
    ))
}

/// Mint a fresh binding for `generation`. `not_after` is the validity
/// window's close; the window opens now. One year is the caller's default
/// (`BINDING_WINDOW_SECS`), chosen so a node can be offline for a long
/// time and still seal, while a revoked key still ages out.
pub fn mint_binding(
    kp: &identity::Keypair,
    age_pubkey: &str,
    generation: u64,
    not_before: &str,
    not_after: &str,
) -> Result<Binding, String> {
    let mut b = Binding {
        v: BINDING_VERSION,
        purpose: BINDING_PURPOSE.to_string(),
        age_pubkey: age_pubkey.to_string(),
        age_fingerprint: age_fingerprint(age_pubkey),
        suites: ACCEPTED_SUITES.iter().map(|s| s.to_string()).collect(),
        generation,
        rotated_at: not_before.to_string(),
        not_before: not_before.to_string(),
        not_after: not_after.to_string(),
        identity_key: kp.info().pubkey_hex,
        sig: String::new(),
    };
    let bytes = binding_bytes(&b)?;
    b.sig = wire_auth::sign_hex(kp, &bytes);
    Ok(b)
}

/// Verify a binding: its own signature under the key it names, its
/// fingerprint recomputing from its recipient, and (when the caller knows
/// which key it expects) that the signer IS that key.
///
/// Collapses "the signature does not verify", "the fingerprint does not
/// recompute", "the version or purpose is not one this build knows" and
/// "the signer is not the key I expected" into one `false`, the same
/// non-oracle discipline [`mail::verify_origin_signature`] states: a caller
/// cannot act differently on any of them, and a distinguishing error would
/// be a fingerprint oracle over keys it does not trust.
pub fn verify_binding(b: &Binding, expected_identity_key: Option<&str>) -> bool {
    if b.v != BINDING_VERSION || b.purpose != BINDING_PURPOSE {
        return false;
    }
    if b.suites.is_empty() || b.suites.iter().any(|s| !ACCEPTED_SUITES.contains(&s.as_str())) {
        return false;
    }
    if age_fingerprint(&b.age_pubkey) != b.age_fingerprint {
        return false;
    }
    if let Some(expected) = expected_identity_key {
        if expected != b.identity_key {
            return false;
        }
    }
    match binding_bytes(b) {
        Ok(bytes) => wire_auth::verify_signature_hex(&b.identity_key, &bytes, &b.sig),
        Err(_) => false,
    }
}

/// The binding's validity window has closed.
pub fn binding_expired(b: &Binding, now: &str) -> bool {
    // ISO-8601 UTC strings sort lexicographically within one offset, which
    // is what `time::now_iso_utc` always writes.
    now > b.not_after.as_str()
}

/// The binding is not yet valid.
pub fn binding_not_yet_valid(b: &Binding, now: &str) -> bool {
    now < b.not_before.as_str()
}

// ── Our own binding at rest ─────────────────────────────────────────────

fn own_binding_path() -> PathBuf {
    identity::identity_dir().join("age-binding.json")
}

/// This node's current binding, or `None` before anything has minted one.
pub fn own_binding() -> Option<Binding> {
    let text = std::fs::read_to_string(own_binding_path()).ok()?;
    serde_json::from_str(&text).ok()
}

/// Publish (mint, store and return) this node's binding for the current age
/// key and generation. Idempotent: a valid stored binding for the current
/// key is returned unchanged, so calling this on every door read costs one
/// small file read and never rotates anything.
pub fn publish_binding() -> Result<Binding, String> {
    let (id, _) = load_or_mint_age_identity().map_err(|e| e.to_string())?;
    let recipient = recipient_of(&id);
    if let Some(existing) = own_binding() {
        if existing.age_pubkey == recipient && verify_binding(&existing, None) {
            return Ok(existing);
        }
    }
    let (kp, _) = identity::load_or_mint().map_err(|e| e.to_string())?;
    let now = now_iso_utc();
    let generation = own_binding().map(|b| b.generation + 1).unwrap_or(1);
    let binding = mint_binding(&kp, &recipient, generation, &now, &window_end(&now))?;
    write_own_binding(&binding)?;
    Ok(binding)
}

/// The validity window's length in seconds: one year. Long enough that an
/// offline node still seals on return; short enough that a leaked key ages
/// out without an operator action.
pub const BINDING_WINDOW_SECS: u64 = 365 * 24 * 60 * 60;

/// `now` plus [`BINDING_WINDOW_SECS`], in the same ISO-8601 UTC shape.
fn window_end(now: &str) -> String {
    crate::time::shift_iso_utc(now, BINDING_WINDOW_SECS as i64)
}

fn write_own_binding(binding: &Binding) -> Result<(), String> {
    let text = serde_json::to_string_pretty(binding).map_err(|e| e.to_string())?;
    fs::atomic_write(&own_binding_path(), &(text + "\n")).map_err(|e| e.to_string())
}

/// **Rotate this node's age key.** The old private key moves into the
/// retired store with its binding's `not_after` as its grace deadline, and
/// a fresh key is minted with the next generation. Nothing is re-paired and
/// no charter is re-signed: peers accept the new binding because it is
/// signed by the same identity key, and letters sealed to the old key
/// still open until the deadline passes ([`open_to_me`]).
pub fn rotate_age_key() -> Result<Binding, String> {
    let _ = load_or_mint_age_identity().map_err(|e| e.to_string())?;
    let now = now_iso_utc();
    let previous_deadline = own_binding().map(|b| b.not_after).unwrap_or_else(|| now.clone());
    if age_key_path().exists() {
        fs::secure_private_dir(&retired_dir()).map_err(|e| e.to_string())?;
        let (id, _) = load_or_mint_age_identity().map_err(|e| e.to_string())?;
        let secret = age::secrecy::ExposeSecret::expose_secret(&id.to_string()).to_string();
        let generation = own_binding().map(|b| b.generation).unwrap_or(0);
        fs::atomic_write_private(&retired_key_path(generation), &(secret + "\n").into_bytes())
            .map_err(|e| e.to_string())?;
        fs::atomic_write(&retired_until_path(generation), &(previous_deadline + "\n"))
            .map_err(|e| e.to_string())?;
        // The tombstone: the address this key answered to. Written here,
        // never deleted, so `key-retired` survives the sweep (M3).
        fs::atomic_write(&retired_recipient_path(generation), &(recipient_of(&id) + "\n"))
            .map_err(|e| e.to_string())?;
        std::fs::remove_file(age_key_path()).map_err(|e| e.to_string())?;
    }
    let (kp, _) = identity::load_or_mint().map_err(|e| e.to_string())?;
    let new = load_or_mint_age_identity().map_err(|e| e.to_string())?;
    let recipient = recipient_of(&new.0);
    let generation = own_binding().map(|b| b.generation).unwrap_or(0) + 1;
    let binding = mint_binding(&kp, &recipient, generation, &now, &window_end(&now))?;
    write_own_binding(&binding)?;
    Ok(binding)
}

fn retired_key_path(generation: u64) -> PathBuf {
    retired_dir().join(format!("{generation}.key"))
}

fn retired_until_path(generation: u64) -> PathBuf {
    retired_dir().join(format!("{generation}.until"))
}

/// The recipient a retired key belonged to — the **tombstone** (the branch
/// review's M3). Written at rotation and never deleted, so the address of a
/// retired key outlives the key itself and a container sealed to it can still
/// be refused `key-retired` rather than degrading to `open-failed`.
fn retired_recipient_path(generation: u64) -> PathBuf {
    retired_dir().join(format!("{generation}.recipient"))
}

/// Every retired key whose grace window is still open, newest generation
/// last. A retired key past its deadline is skipped here and swept by
/// [`sweep_retired_keys`]; a key sealed to one is [`OPEN_KEY_RETIRED`].
fn live_retired_identities(now: &str) -> Vec<age::x25519::Identity> {
    let Ok(entries) = std::fs::read_dir(retired_dir()) else { return Vec::new() };
    let mut found: Vec<(u64, age::x25519::Identity)> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(stem) = name.strip_suffix(".until") else { continue };
        let Ok(generation) = stem.parse::<u64>() else { continue };
        let Ok(until) = std::fs::read_to_string(entry.path()) else { continue };
        if now > until.trim() {
            continue;
        }
        let Ok(secret) = std::fs::read_to_string(retired_key_path(generation)) else { continue };
        if let Ok(id) = secret.trim().parse::<age::x25519::Identity>() {
            found.push((generation, id));
        }
    }
    found.sort_by_key(|(generation, _)| *generation);
    found.into_iter().map(|(_, id)| id).collect()
}

/// Delete every retired key whose grace window has closed. Called on every
/// open, so the deletion is not a housekeeping chore an operator has to
/// remember.
pub fn sweep_retired_keys(now: &str) -> usize {
    let Ok(entries) = std::fs::read_dir(retired_dir()) else { return 0 };
    let mut swept = 0;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(stem) = name.strip_suffix(".until") else { continue };
        let Ok(until) = std::fs::read_to_string(entry.path()) else { continue };
        if now <= until.trim() {
            continue;
        }
        if let Ok(generation) = stem.parse::<u64>() {
            // The PRIVATE KEY goes; the tombstone stays (the branch review's
            // M3). `.until` and `.recipient` are what let a container sealed
            // to this key still be refused `key-retired` after the sweep —
            // deleting them degraded the refusal to `open-failed`, which age
            // returns just as readily for a wrong recipient or a tampered
            // `ct`, so the origin's retry loop learned nothing actionable and
            // an operator could not tell a rotation from an attack.
            // Count only an ACTUAL deletion (L13). The `.until` marker is a
            // permanent tombstone now, so counting every expired marker
            // would re-report the same sweep on every later call while
            // deleting nothing.
            if std::fs::remove_file(retired_key_path(generation)).is_ok() {
                swept += 1;
            }
        }
    }
    swept
}

/// Refusal: a container addressed to an age key this node has retired and
/// whose grace window has closed.
pub const OPEN_KEY_RETIRED: &str = "key-retired";
/// Refusal: the ciphertext does not open under any identity this node holds
/// — a wrong recipient, or a tampered `ct`. One word, because age cannot
/// distinguish them and neither can a caller.
pub const OPEN_FAILED: &str = "open-failed";

/// Is `recipient` a key this node retired whose grace window has closed?
///
/// Decided from the **tombstone**, not from the key: a retired key's address
/// outlives the key itself (see [`sweep_retired_keys`]), so this answer does
/// not change the moment housekeeping runs — which is the whole reason the
/// tombstone exists. A record with no tombstone (a pre-M3 installation) is
/// unrecognisable, which is the same answer as "never ours" and the same
/// answer as before the tombstone landed.
fn retired_and_closed(recipient: &str, now: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(retired_dir()) else { return false };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(stem) = name.strip_suffix(".until") else { continue };
        let Ok(generation) = stem.parse::<u64>() else { continue };
        let Ok(until) = std::fs::read_to_string(entry.path()) else { continue };
        if now <= until.trim() {
            continue;
        }
        // The TOMBSTONE, not the key: once swept the private key is gone, and
        // a refusal word must not depend on whether housekeeping has run.
        let Ok(address) = std::fs::read_to_string(retired_recipient_path(generation)) else { continue };
        if address.trim() == recipient {
            return true;
        }
    }
    false
}

// ── Peer bindings ───────────────────────────────────────────────────────

/// Refusal: a binding that does not verify under the key it names, or that
/// names a key this node is not known by.
pub const BINDING_MISMATCH: &str = "binding-mismatch";
/// Refusal: a binding whose generation is not above the high-water mark.
pub const STALE_BINDING: &str = "stale-binding";

/// Where a peer's accepted binding lives: `state/age-bindings/<node>.json`.
///
/// A store of its own rather than a field on `nodes.json`'s record, for the
/// reason `crates/AGENTS.md` gives for every such choice: the binding is one
/// removable file per node, `nodes.json`'s shape stays exactly what P-CHARTER
/// is scheduled to amend, and a node that is removed and re-added gets no
/// inherited trust — [`binding_for`] re-verifies against the node's key ON
/// RECORD on every read, so a stale file left behind by a removed node
/// simply fails to verify and is inert.
pub fn peer_bindings_dir() -> PathBuf {
    fs::state_dir().join("age-bindings")
}

fn peer_binding_path(node_name: &str) -> PathBuf {
    peer_bindings_dir().join(format!("{node_name}.json"))
}

/// Learn a peer's self-signed binding. The high-water mark is the generation
/// of the binding already stored for that node, so a replayed superseded
/// binding is refused for current use — byte-for-byte identical to the
/// current one only by being lower, which is what makes this check load-
/// bearing.
///
/// `Ok(Some(binding))` is a store that changed; `Ok(None)` is a binding
/// already current (idempotent, not an error); `Err` carries the refusal
/// word.
pub fn learn_binding(node_name: &str, binding: &Binding) -> Result<Option<Binding>, String> {
    let nodes = node_store::load_nodes();
    let Some(node) = nodes.iter().find(|n| n.name == node_name) else {
        return Err(format!("no node named `{node_name}`"));
    };
    // L5 (the branch review): an age key is accepted ONLY inside a binding
    // signed by an identity key that was itself pinned. A node record with no
    // `pubkey` on file pins nothing, so there is nothing to verify against and
    // the binding is refused — never accepted unverified. `verify_binding`
    // treats `None` the same way, so both reads agree.
    let Some(pinned) = node.pubkey.as_deref().filter(|k| !k.is_empty()) else {
        return Err(BINDING_MISMATCH.to_string());
    };
    if !verify_binding(binding, Some(pinned)) {
        return Err(BINDING_MISMATCH.to_string());
    }
    // The high-water mark is compared ONLY against a stored binding that
    // still verifies under the key NOW on record (M2). Keyed by node name
    // alone, a re-pair with a fresh identity key left a generation-5 file
    // signed by the dead key: the new key's generation-1 binding passed
    // verification and then lost the `1 < 5` comparison forever — a permanent
    // `stale-binding` wedge that silently downgraded that peer to plaintext
    // for the life of the install. The design's rule is generations per
    // IDENTITY KEY, so a file signed by a key this node no longer holds is
    // not a predecessor to compare against; it is stale residue, and it is
    // overwritten.
    if let Some(current) = binding_for(node_name) {
        if binding.generation < current.generation {
            return Err(STALE_BINDING.to_string());
        }
        if binding.generation == current.generation {
            // An equal generation with the same bytes is a replay of the
            // binding already in force — nothing to learn, nothing to
            // refuse. An equal generation with DIFFERENT bytes is a
            // conflict: two claims to one generation, which nothing here
            // can order.
            return if *binding == current { Ok(None) } else { Err(STALE_BINDING.to_string()) };
        }
    }
    std::fs::create_dir_all(peer_bindings_dir()).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(binding).map_err(|e| e.to_string())?;
    fs::atomic_write(&peer_binding_path(node_name), &(text + "\n")).map_err(|e| e.to_string())?;
    Ok(Some(binding.clone()))
}

/// The stored binding, whatever it says, without re-checking it against the
/// node record. [`binding_for`] is its ONLY caller — `learn_binding` goes
/// through that (M2), so a high-water comparison is never measured against a
/// file signed by a key this node no longer holds.
fn stored_binding(node_name: &str) -> Option<Binding> {
    let text = std::fs::read_to_string(peer_binding_path(node_name)).ok()?;
    serde_json::from_str(&text).ok()
}

/// The binding this node holds for `node_name`, re-verified against the key
/// on record for that node on every read. A file whose signer is not the
/// node's own key is ignored rather than reported: the file is not the
/// authority, the node record is, and a caller can do nothing with a
/// half-trusted binding.
pub fn binding_for(node_name: &str) -> Option<Binding> {
    let node = node_store::load_nodes().into_iter().find(|n| n.name == node_name)?;
    // L5: no pinned key on file means no acceptance — see `learn_binding`.
    let pinned = node.pubkey.as_deref().filter(|k| !k.is_empty())?;
    let binding = stored_binding(node_name)?;
    verify_binding(&binding, Some(pinned)).then_some(binding)
}

/// Drop a node's stored binding, because the node is gone. Returns whether
/// there was one.
///
/// A binding is keyed by node NAME; `node remove` therefore has to take it
/// too, or a node re-added under the same name inherits residue from its
/// predecessor. `binding_for` re-verifies against the key on record on every
/// read, so the residue is inert rather than trusted — but "inert" was not
/// true of the high-water comparison before M2, and the safe rule is that a
/// removed node leaves nothing behind.
pub fn forget_binding(node_name: &str) -> Result<bool, String> {
    match std::fs::remove_file(peer_binding_path(node_name)) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("{}: {e}", peer_binding_path(node_name).display())),
    }
}

/// A peer binding that may be sealed to right now: present, current, and
/// inside its window.
pub fn usable_binding_for(node_name: &str, now: &str) -> Option<Binding> {
    let binding = binding_for(node_name)?;
    if binding_expired(&binding, now) || binding_not_yet_valid(&binding, now) {
        return None;
    }
    binding.suites.iter().any(|s| s == SUITE_AGE_V1_X25519).then_some(binding)
}

// ── The container ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Party {
    pub node: String,
    /// Lowercase hex, 32 raw Ed25519 public key bytes.
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Destination {
    pub node: String,
    /// The canonical bech32 age recipient.
    pub age: String,
}

/// One link in the hop chain. `prev` is deliberately absent: it is
/// recomputed from the entry before this one and never read from the wire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitEntry {
    pub node: String,
    pub next: String,
    pub at: String,
    pub mesh: String,
    /// Lowercase hex, 64 raw signature bytes.
    pub sig: String,
}

/// The outer object (`HTTPS-MESH-API.md` "Container"). `ct` and `sig` are
/// lowercase hex on the wire, the same rendering every other signature and
/// digest in this design uses; both enter their frames as raw bytes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Container {
    pub v: u64,
    pub purpose: String,
    pub msgid: String,
    pub generation: u64,
    pub origin: Party,
    pub to: Destination,
    pub origin_mesh: String,
    pub mesh: String,
    pub suite: String,
    pub ct: String,
    pub sig: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transit: Vec<TransitEntry>,
    /// Lowercase hex of the 32 board-id bytes; absent for a letter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub epoch: Option<u64>,
}

/// The recomputed context: every field a verifier derives from the outer
/// object and never takes off the wire.
#[derive(Debug, Clone, PartialEq)]
pub struct Ctx {
    pub v: u64,
    pub purpose: String,
    pub msgid: [u8; 32],
    pub origin_node: String,
    pub origin_key: [u8; 32],
    pub to_node: String,
    pub to_age: String,
    pub origin_mesh: String,
    pub suite: String,
    pub generation: u64,
    pub board: Option<[u8; 32]>,
    pub epoch: u64,
}

impl Ctx {
    /// Recompute `ctx` from the outer fields. Fails closed on any field
    /// whose shape is wrong, so nothing malformed can reach a frame.
    pub fn from_container(c: &Container) -> Result<Ctx, String> {
        Ok(Ctx {
            v: c.v,
            purpose: c.purpose.clone(),
            msgid: hex_array::<32>(&c.msgid)?,
            origin_node: c.origin.node.clone(),
            origin_key: hex_array::<32>(&c.origin.key)?,
            to_node: c.to.node.clone(),
            to_age: c.to.age.clone(),
            origin_mesh: c.origin_mesh.clone(),
            suite: c.suite.clone(),
            generation: c.generation,
            board: match &c.board {
                Some(hex) => Some(hex_array::<32>(hex)?),
                None => None,
            },
            epoch: c.epoch.unwrap_or(0),
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let board: &[u8] = self.board.as_ref().map(|b| b.as_slice()).unwrap_or(&[]);
        frame(
            L_CTX,
            &[
                &self.v.to_be_bytes(),
                self.purpose.as_bytes(),
                &self.msgid,
                self.origin_node.as_bytes(),
                &self.origin_key,
                self.to_node.as_bytes(),
                self.to_age.as_bytes(),
                self.origin_mesh.as_bytes(),
                self.suite.as_bytes(),
                &self.generation.to_be_bytes(),
                board,
                &self.epoch.to_be_bytes(),
            ],
        )
    }
}

/// Refusal: the container's `purpose` and its `board`/`epoch` fields
/// disagree.
pub const PURPOSE_MISMATCH: &str = "purpose-mismatch";
/// Refusal: the inner and outer `ctx` differ, or the inner claims disagree
/// with the outer.
pub const CONTEXT_MISMATCH: &str = "context-mismatch";
/// Refusal: the inner envelope is addressed somewhere other than the outer
/// `to`/`origin`.
pub const ADDRESSING_MISMATCH: &str = "addressing-mismatch";
/// Refusal: the hop chain does not walk from `msgid` to self.
pub const BROKEN_CHAIN: &str = "broken-chain";
/// Refusal: a container for a suite this build does not implement.
pub const UNSUPPORTED_SUITE: &str = "unsupported-suite";
/// Refusal: a container whose frame version this build does not know. Its own
/// word, deliberately not `purpose-mismatch`'s: an operator debugging a v2
/// container must not be sent hunting a board/epoch bug.
pub const UNSUPPORTED_CONTAINER_VERSION: &str = "unsupported-container-version";

/// Check the purpose/board/epoch rule the design fixes: `post` and `wrap`
/// carry a board, `mail` and `charter` carry none and epoch 0. Without
/// this the frame is still injective, but its meaning would be ambiguous.
pub fn check_purpose_board(c: &Container) -> Result<(), String> {
    if !PURPOSES.contains(&c.purpose.as_str()) {
        return Err(PURPOSE_MISMATCH.to_string());
    }
    let carries_board = matches!(c.purpose.as_str(), PURPOSE_POST | PURPOSE_WRAP);
    let has_board = c.board.is_some();
    let epoch = c.epoch.unwrap_or(0);
    if carries_board != has_board {
        return Err(PURPOSE_MISMATCH.to_string());
    }
    if !carries_board && epoch != 0 {
        return Err(PURPOSE_MISMATCH.to_string());
    }
    Ok(())
}

/// The outer signature input: `frame("aoide/mail-outer", [ctx, ct])`.
fn outer_bytes(ctx: &Ctx, ct: &[u8]) -> Vec<u8> {
    frame(L_OUTER, &[&ctx.to_bytes(), ct])
}

/// The dedup digest: everything `ctx` covers plus `ct` and `sig`, and
/// nothing hop-mutable, so a retry over another route is a duplicate
/// rather than a collision.
fn dedup_bytes(ctx: &Ctx, ct: &[u8], sig: &[u8]) -> Vec<u8> {
    frame(L_DEDUP, &[&ctx.to_bytes(), ct, sig])
}

/// The inner envelope's four values, framed: the envelope's own field
/// equations are untouched, this only fixes the order they are framed in.
fn envelope_frame(envelope: &mail::Envelope) -> Result<Vec<u8>, String> {
    let sig = hex_array::<64>(&envelope.sig)?;
    let msgid = hex_array::<32>(&envelope.msgid)?;
    Ok(frame(
        L_ENVELOPE,
        &[
            &mail::canonical_header_bytes(&envelope.header),
            envelope.text.as_bytes(),
            &sig,
            &msgid,
        ],
    ))
}

/// `frame("aoide/mail-hop", [msgid, prev, node, next, at, mesh])`.
pub fn hop_bytes(msgid: &[u8; 32], prev: &[u8; 32], node: &str, next: &str, at: &str, mesh: &str) -> Vec<u8> {
    frame(
        L_HOP,
        &[msgid, prev, node.as_bytes(), next.as_bytes(), at.as_bytes(), mesh.as_bytes()],
    )
}

/// `frame("aoide/mail-hop-entry", [msgid, prev, node, next, at, mesh, sig])`
/// — what `prev` is the digest of.
fn hop_entry_bytes(
    msgid: &[u8; 32],
    prev: &[u8; 32],
    entry: &TransitEntry,
) -> Result<Vec<u8>, String> {
    let sig = hex_array::<64>(&entry.sig)?;
    Ok(frame(
        L_HOP_ENTRY,
        &[
            msgid,
            prev,
            entry.node.as_bytes(),
            entry.next.as_bytes(),
            entry.at.as_bytes(),
            entry.mesh.as_bytes(),
            &sig,
        ],
    ))
}

/// Seal `envelope` to a peer's binding. Returns the container; nothing is
/// written here, so a caller that fails to spool the result has lost
/// nothing but the work.
///
/// The origin becomes the first hop: entry 1 names the origin itself and
/// the node it hands the letter to, signed under the origin's own identity
/// key, so the record of who handed the letter to whom starts at the
/// machine that wrote it and a relay cannot erase it.
pub fn seal_envelope(
    envelope: &mail::Envelope,
    binding: &Binding,
    origin_mesh: &str,
    mesh: &str,
    to_node: &str,
    at: &str,
) -> Result<Container, String> {
    let (kp, _) = identity::load_or_mint().map_err(|e| e.to_string())?;
    let origin_key_hex = kp.info().pubkey_hex;
    let msgid = hex_array::<32>(&envelope.msgid)?;
    let ctx = Ctx {
        v: CONTAINER_VERSION,
        purpose: PURPOSE_MAIL.to_string(),
        msgid,
        origin_node: envelope.header.from.node.clone(),
        origin_key: hex_array::<32>(&origin_key_hex)?,
        to_node: to_node.to_string(),
        to_age: binding.age_pubkey.clone(),
        origin_mesh: origin_mesh.to_string(),
        suite: SUITE_AGE_V1_X25519.to_string(),
        generation: binding.generation,
        board: None,
        epoch: 0,
    };

    let pt = frame(L_PT, &[&ctx.to_bytes(), &envelope_frame(envelope)?]);
    let recipient: age::x25519::Recipient = binding
        .age_pubkey
        .parse()
        .map_err(|e| format!("binding recipient is not an age key ({e})"))?;
    let ct = encrypt_to(&recipient, &pt)?;

    let mut container = Container {
        v: CONTAINER_VERSION,
        purpose: ctx.purpose.clone(),
        msgid: hex_encode(&ctx.msgid),
        generation: ctx.generation,
        origin: Party {
            node: ctx.origin_node.clone(),
            key: origin_key_hex,
        },
        to: Destination { node: ctx.to_node.clone(), age: binding.age_pubkey.clone() },
        origin_mesh: ctx.origin_mesh.clone(),
        mesh: mesh.to_string(),
        suite: ctx.suite.clone(),
        ct: hex_encode(&ct),
        sig: String::new(),
        transit: Vec::new(),
        board: None,
        epoch: None,
    };
    container.sig = wire_auth::sign_hex(&kp, &outer_bytes(&ctx, &ct));
    container.transit.push(TransitEntry {
        node: ctx.origin_node.clone(),
        next: to_node.to_string(),
        at: at.to_string(),
        mesh: mesh.to_string(),
        sig: wire_auth::sign_hex(&kp, &hop_bytes(&msgid, &msgid, &ctx.origin_node, to_node, at, mesh)),
    });
    Ok(container)
}

/// Seal `payload` to `recipient` — the one place `age` encrypts.
fn encrypt_to(recipient: &age::x25519::Recipient, payload: &[u8]) -> Result<Vec<u8>, String> {
    use std::io::Write;
    let encryptor = age::Encryptor::with_recipients(std::iter::once(recipient as &dyn age::Recipient))
        .map_err(|e| format!("age: {e}"))?;
    let mut out = Vec::new();
    let mut writer = encryptor.wrap_output(&mut out).map_err(|e| format!("age: {e}"))?;
    writer.write_all(payload).map_err(|e| format!("age: {e}"))?;
    writer.finish().map_err(|e| format!("age: {e}"))?;
    Ok(out)
}

/// Open `ct` with the first identity that fits — the current key first,
/// then every retired key whose grace window is still open.
fn decrypt_with_identities(ct: &[u8]) -> Result<Vec<u8>, String> {
    let now = now_iso_utc();
    // Sweep only on the CURRENT key's own success — never before an attempt
    // and never on a failure. Housekeeping must not decide a refusal word:
    // deleting a just-expired key before the caller can look at it turns an
    // actionable `key-retired` into a bare "cannot open". Nothing depends on
    // the sweep happening at any particular moment, only on it happening
    // before the next ordinary open.
    let (current, _) = load_or_mint_age_identity().map_err(|e| e.to_string())?;
    let decryptor = age::Decryptor::new(ct).map_err(|e| format!("age: {e}"))?;
    if let Ok(reader) = decryptor.decrypt(std::iter::once(&current as &dyn age::Identity)) {
        let _ = sweep_retired_keys(&now);
        return read_to_end(reader);
    }
    // A container for another node's key fails here for the same reason a
    // tampered one does: age cannot say which. So we try every retired key
    // and, failing all of them, report the failure — the caller checks the
    // ADDRESS to tell `key-retired` from a plain open failure, and the
    // retired record is left intact for that check.
    for identity in live_retired_identities(&now) {
        let Ok(decryptor) = age::Decryptor::new(ct) else { continue };
        if let Ok(reader) = decryptor.decrypt(std::iter::once(&identity as &dyn age::Identity)) {
            return read_to_end(reader);
        }
    }
    Err("no live age identity opens this container".to_string())
}

fn read_to_end(mut reader: impl std::io::Read) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    reader.read_to_end(&mut out).map_err(|e| format!("age: {e}"))?;
    Ok(out)
}

// ── Dedup: the pre-open gate ────────────────────────────────────────────

fn container_seen_path() -> PathBuf {
    mail::mail_dir().join("containers.jsonl")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContainerSeen {
    origin_key: String,
    to_node: String,
    msgid: String,
    digest: String,
}

/// What the pre-open dedup gate found.
pub enum Seen {
    Fresh,
    Duplicate { filed_letter: bool },
    Collision,
}

/// The pre-open dedup gate. The lookup key is the authenticated
/// `(origin.key, to.node, msgid)` tuple — never `msgid` alone, since a relay
/// cannot verify the inner hash and one origin must not be able to reserve
/// another origin's message id. Within that tuple the comparison is the
/// digest of the immutable container fields, so the same letter over
/// another route is a duplicate and the same `msgid` with different bytes
/// is a collision.
fn check_seen(container: &Container, digest: &str) -> Result<Seen, String> {
    let entries = read_container_seen()?;
    let mut hit = None;
    for entry in entries.into_iter().rev() {
        if entry.origin_key == container.origin.key
            && entry.to_node == container.to.node
            && entry.msgid == container.msgid
        {
            hit = Some(entry);
            break;
        }
    }
    match hit {
        None => Ok(Seen::Fresh),
        Some(entry) if entry.digest == digest => Ok(Seen::Duplicate {
            // The truth about filing is the filed record itself, never the
            // gate's own memory: a crash between filing and the gate's
            // write must not lose the receipt.
            filed_letter: mail::filed_kind(&container.msgid)
                .map(|k| k == mail::ENTRY_TYPE_LETTER)
                .unwrap_or(false),
        }),
        Some(_) => Ok(Seen::Collision),
    }
}

fn read_container_seen() -> Result<Vec<ContainerSeen>, String> {
    let path = container_seen_path();
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    Ok(text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<ContainerSeen>(l).ok())
        .collect())
}

/// Record an admitted-and-filed container, so the next identical attempt
/// takes the gate's cheap path instead of opening again.
///
/// **Called by the caller, after it filed — never from
/// [`deposit_container`]** (the branch review's M1). The gate's whole promise
/// is "a previously admitted duplicate returns `duplicate` without opening",
/// and that promise is only honest if a record means the letter is actually
/// on disk. Two failure shapes follow from recording earlier, both observed:
///
/// - **A refusal past the gate becomes permanent.** `context-mismatch`,
///   `addressing-mismatch`, `broken-chain`, `bad-msgid`, `open-failed` and
///   `key-retired` are all decided *after* the gate. Recorded first, the
///   first attempt reports the taught word and every retry of the same bytes
///   reports `duplicate` — so `key-retired` "reaches the origin's retry
///   loop" exactly once and then stops being true.
/// - **A crash claims a filing that never happened.** Recorded first, a kill
///   between the gate and `base.jsonl` leaves the container admitted and
///   unfiled; the retry answers `duplicate`, the origin records `duplicate`,
///   spools no ack, and the letter is on no disk while `mail outbox` reports
///   it accepted.
///
/// Filed-then-recorded inverts both into the design's own discipline: a
/// retry that was never filed RE-RUNS and re-answers, and a retry whose
/// filing landed but whose record did not is re-opened once and answered
/// `duplicate` from `seen.jsonl` (`mail::filed_kind`'s authority), which is
/// what re-spools the ack.
pub fn record_admitted(container: &Container, digest: &str) -> Result<(), String> {
    record_seen(container, digest)
}

fn record_seen(container: &Container, digest: &str) -> Result<(), String> {
    let line = serde_json::to_string(&ContainerSeen {
        origin_key: container.origin.key.clone(),
        to_node: container.to.node.clone(),
        msgid: container.msgid.clone(),
        digest: digest.to_string(),
    })
    .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(mail::mail_dir()).map_err(|e| e.to_string())?;
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(container_seen_path())
        .map_err(|e| e.to_string())?;
    writeln!(f, "{line}").map_err(|e| e.to_string())
}

// ── The two verification halves ─────────────────────────────────────────

/// The shared, keyless steps every receiver runs, then the branch. `self` is
/// this node's own name.
///
/// Returns either a refusal (with its taught word), a duplicate, or the
/// opened envelope for the caller to hand to [`mail::deposit`] — which is
/// where filing, the seen set and the receipt rule already live, so the
/// container path inherits every one of those invariants rather than
/// reimplementing them.
pub enum ContainerOutcome {
    Opened {
        envelope: Box<mail::Envelope>,
        /// The dedup digest of the immutable container fields. The caller
        /// records the container with this ONCE IT HAS FILED — see
        /// [`record_admitted`] for why the record cannot be written here.
        digest: String,
    },
    Duplicate { filed_letter: bool },
    Refused { reason: String, detail: String },
}

impl std::fmt::Debug for ContainerOutcome {
    /// Hand-written rather than derived: `Opened` holds a whole envelope,
    /// and every call site that prints one of these is a test or an audit
    /// line that wants the SHAPE, not a letter body. `Refused` carries its
    /// word, which is the part a reader acts on.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContainerOutcome::Opened { envelope, .. } => f
                .debug_struct("Opened")
                .field("msgid", &envelope.msgid)
                .finish(),
            ContainerOutcome::Duplicate { filed_letter } => f
                .debug_struct("Duplicate")
                .field("filed_letter", filed_letter)
                .finish(),
            ContainerOutcome::Refused { reason, detail } => f
                .debug_struct("Refused")
                .field("reason", reason)
                .field("detail", detail)
                .finish(),
        }
    }
}

/// Verify a container as far as a receiver with no key can, and then, if
/// self is the destination, open it and check it against the outer claims.
///
/// A hub never opens anything: this function is the destination branch, and
/// the hop branch is a caller that spools the container onward without
/// calling it. P-SEAL's only transport is the direct lane, so the hop
/// branch is exercised in tests rather than in the field.
pub fn deposit_container(container: &Container) -> Result<ContainerOutcome, String> {
    let refusal = |reason: &str, detail: String| ContainerOutcome::Refused {
        reason: reason.to_string(),
        detail,
    };

    // 2. Recompute ctx from the outer fields, never from the wire.
    if container.v != CONTAINER_VERSION {
        return Ok(refusal(
            UNSUPPORTED_CONTAINER_VERSION,
            format!("container version {} is not {CONTAINER_VERSION}", container.v),
        ));
    }
    if let Err(e) = check_purpose_board(container) {
        return Ok(refusal(&e, "purpose and board/epoch fields disagree".to_string()));
    }
    if container.suite != SUITE_AGE_V1_X25519 {
        return Ok(refusal(UNSUPPORTED_SUITE, format!("suite `{}` is not accepted", container.suite)));
    }
    let ctx = match Ctx::from_container(container) {
        Ok(ctx) => ctx,
        Err(e) => return Ok(refusal(CONTEXT_MISMATCH, format!("outer fields are malformed: {e}"))),
    };
    let ct = match hex_decode(&container.ct) {
        Some(ct) => ct,
        None => return Ok(refusal(CONTEXT_MISMATCH, "ct is not hex".to_string())),
    };
    let sig = match hex_array::<64>(&container.sig) {
        Ok(sig) => sig,
        Err(e) => return Ok(refusal(CONTEXT_MISMATCH, format!("sig: {e}"))),
    };

    // 3. The outer origin signature, under the key `origin.key` names,
    //    resolved through the paired record (a charter mesh is P-CHARTER's).
    let nodes = node_store::load_nodes();
    let Some(origin) = nodes.iter().find(|n| n.name == ctx.origin_node) else {
        return Ok(refusal("unverified-origin", format!("no node named `{}`", ctx.origin_node)));
    };
    let Some(recorded) = origin.pubkey.as_deref() else {
        return Ok(refusal("unverified-origin", format!("`{}` has no key on record", ctx.origin_node)));
    };
    if hex_decode(recorded).as_deref() != Some(ctx.origin_key.as_slice()) {
        return Ok(refusal(
            "unverified-origin",
            format!("`{}` signs as a key this node does not have on record", ctx.origin_node),
        ));
    }
    if !wire_auth::verify_signature_hex(recorded, &outer_bytes(&ctx, &ct), &container.sig) {
        return Ok(refusal("unverified-origin", "the outer signature does not verify".to_string()));
    }

    // 5. Dedup, before anything is opened.
    let digest = hex_encode(&sha256(&dedup_bytes(&ctx, &ct, &sig)));
    match check_seen(container, &digest)? {
        Seen::Collision => {
            return Ok(refusal(
                CONTEXT_MISMATCH,
                format!("msgid {} reappeared with different immutable bytes", container.msgid),
            ))
        }
        Seen::Duplicate { filed_letter } => return Ok(ContainerOutcome::Duplicate { filed_letter }),
        Seen::Fresh => {
            // Deliberately NOT recorded here (the branch review's M1). The
            // gate means "admitted AND filed", so the caller records with
            // the digest it gets back on `Opened` once `mail::deposit` has
            // actually filed. Recording at this point instead would (a) turn
            // every refusal past this line into a permanent `duplicate` on
            // the retry that should have re-run it, and (b) claim a filing
            // that a crash between here and `base.jsonl` never performed.
        }
    }

    // Destination branch: self is `to.node`.
    if container.to.node != crate::display::local_host_name() {
        // Not ours to open. A hub reaching here has spooled it onward
        // without ever calling this; a direct-lane receiver that is not the
        // destination has nothing to do with it either.
        return Ok(refusal(
            ADDRESSING_MISMATCH,
            format!("container is addressed to `{}`, not this node", container.to.node),
        ));
    }

    // 1. Open.
    let pt = match decrypt_with_identities(&ct) {
        Ok(pt) => pt,
        Err(_) => {
            // Report retirement honestly when that is what it is: `to.age`
            // names a key this node retired whose grace window has closed.
            // age cannot tell us — a wrong recipient and a tampered
            // ciphertext fail identically, and neither yields a reason — so
            // the check is on the ADDRESS, not on the failure.
            let reason = if retired_and_closed(&container.to.age, &now_iso_utc()) {
                OPEN_KEY_RETIRED
            } else {
                OPEN_FAILED
            };
            return Ok(refusal(
                reason,
                "the ciphertext does not open under any identity this node holds".to_string(),
            ));
        }
    };
    let fields = match parse_frame(L_PT, &pt) {
        Ok(fields) => fields,
        Err(e) => return Ok(refusal(CONTEXT_MISMATCH, e)),
    };
    if fields.len() != 2 {
        return Ok(refusal(CONTEXT_MISMATCH, "the sealed plaintext is not a two-field frame".to_string()));
    }
    // 2. The inner ctx must equal the recomputed outer ctx, byte for byte.
    if fields[0] != ctx.to_bytes().as_slice() {
        return Ok(refusal(CONTEXT_MISMATCH, "the inner ctx differs from the outer".to_string()));
    }
    let inner = match parse_frame(L_ENVELOPE, fields[1]) {
        Ok(inner) => inner,
        Err(e) => return Ok(refusal(CONTEXT_MISMATCH, format!("inner payload: {e}"))),
    };
    if inner.len() != 4 {
        return Ok(refusal(CONTEXT_MISMATCH, "the inner payload is not a four-field frame".to_string()));
    }
    let Ok(text) = std::str::from_utf8(inner[1]) else {
        return Ok(refusal(CONTEXT_MISMATCH, "inner text is not UTF-8".to_string()));
    };
    // The header travels as its signed bytes, so recovering the fields is
    // the split that is the exact inverse of `canonical_header_bytes` — a
    // header that does not re-encode to the same bytes (something inside a
    // field tried to be a NUL) is refused rather than reinterpreted.
    let Some(header) = mail::header_from_canonical_bytes(inner[0]) else {
        return Ok(refusal(CONTEXT_MISMATCH, "inner header is not eight NUL-free fields".to_string()));
    };
    if mail::canonical_header_bytes(&header) != inner[0] {
        return Ok(refusal(CONTEXT_MISMATCH, "inner header does not re-encode to its own bytes".to_string()));
    }
    let envelope = mail::Envelope {
        header,
        text: text.to_string(),
        sig: hex_encode(inner[2]),
        msgid: hex_encode(inner[3]),
    };
    // 3. The inner msgid must recompute to the container's.
    let Some(recomputed) = mail::compute_msgid(&envelope.header, &envelope.text, &envelope.sig) else {
        return Ok(refusal("bad-msgid", "inner signature is not hex".to_string()));
    };
    if recomputed != container.msgid {
        return Ok(refusal("bad-msgid", format!("inner msgid {recomputed} is not the container's")));
    }
    // 4. Addressing.
    if envelope.header.from.node != container.origin.node {
        return Ok(refusal(
            ADDRESSING_MISMATCH,
            format!("inner origin `{}` is not the container's", envelope.header.from.node),
        ));
    }
    if envelope.header.to.node != container.to.node {
        return Ok(refusal(
            ADDRESSING_MISMATCH,
            format!("inner destination `{}` is not the container's", envelope.header.to.node),
        ));
    }
    if envelope.header.origin_mesh != ctx.origin_mesh {
        return Ok(refusal(
            ADDRESSING_MISMATCH,
            "inner originMesh is not the container's".to_string(),
        ));
    }
    // 6. The inner envelope's own signature.
    if !mail::verify_origin_signature(&envelope) {
        return Ok(refusal("unverified-origin", "the inner envelope signature does not verify".to_string()));
    }
    // 7. Walk the chain.
    if let Err(e) = walk_chain(container, &ctx, &nodes) {
        return Ok(refusal(BROKEN_CHAIN, e));
    }
    Ok(ContainerOutcome::Opened { envelope: Box::new(envelope), digest })
}

/// Walk the hop chain from `msgid` to self. Entry 1 must name the origin
/// and verify under `origin.key` — that is what makes the origin
/// unerasable, and what refuses a relay that dropped entry 1 and
/// re-appended itself.
pub fn walk_chain(
    container: &Container,
    ctx: &Ctx,
    nodes: &[node_store::Node],
) -> Result<(), String> {
    let msgid = ctx.msgid;
    let mut prev = msgid;
    let mut expected_next: Option<String> = None;
    for (index, entry) in container.transit.iter().enumerate() {
        if let Some(expected) = &expected_next {
            if &entry.node != expected {
                return Err(format!("entry {} is by `{}`, not the `{expected}` the entry before it named", index + 1, entry.node));
            }
        }
        let key = if index == 0 {
            if entry.node != ctx.origin_node {
                return Err(format!(
                    "entry 1 is by `{}`, not the origin `{}`",
                    entry.node, ctx.origin_node
                ));
            }
            container.origin.key.clone()
        } else {
            match nodes.iter().find(|n| n.name == entry.node).and_then(|n| n.pubkey.clone()) {
                Some(key) => key,
                None => return Err(format!("no key on record for hop `{}`", entry.node)),
            }
        };
        let body = hop_bytes(&msgid, &prev, &entry.node, &entry.next, &entry.at, &entry.mesh);
        if !wire_auth::verify_signature_hex(&key, &body, &entry.sig) {
            return Err(format!("hop `{}` does not verify this entry", entry.node));
        }
        prev = sha256(&hop_entry_bytes(&msgid, &prev, entry)?);
        expected_next = Some(entry.next.clone());
    }
    match expected_next {
        Some(last) if last == crate::display::local_host_name() => Ok(()),
        Some(last) => Err(format!("the last hop hands the letter to `{last}`, not this node")),
        None => Err("the hop chain is empty".to_string()),
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_test_support::EnvSaver;

    fn env(dir: &std::path::Path) {
        std::env::set_var("AOIDE_STATE_DIR", dir);
        std::env::set_var("AOIDE_ROOT", dir);
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        aoide_test_support::unique_tmp(name)
    }

    fn identity_keypair() -> identity::Keypair {
        identity::load_or_mint().unwrap().0
    }

    fn binding_for_self(generation: u64) -> Binding {
        let kp = identity_keypair();
        let (id, _) = load_or_mint_age_identity().unwrap();
        let now = now_iso_utc();
        mint_binding(&kp, &recipient_of(&id), generation, &now, &window_end(&now)).unwrap()
    }

    #[test]
    fn frame_is_injective_across_the_shapes_that_would_collide() {
        // The classic concatenation collision, and the label collision.
        assert_ne!(frame("a", &[b"ab", b"c"]), frame("a", &[b"a", b"bc"]));
        assert_ne!(frame("aoide/mail-hop", &[]), frame("aoide/mail-hop-entry", &[]));
        // An absent field is a present zero-length field, never an omission.
        assert_ne!(frame("l", &[b""]), frame("l", &[]));
        // And the parse is the inverse of the encode.
        let f = frame("aoide/mail-ctx", &[b"one", b"", b"three"]);
        assert_eq!(parse_frame("aoide/mail-ctx", &f).unwrap(), vec![&b"one"[..], b"", b"three"]);
    }

    #[test]
    fn frame_parse_refuses_truncation_and_a_wrong_label() {
        let f = frame("aoide/mail-ctx", &[b"abcd"]);
        assert!(parse_frame("aoide/mail-ctx", &f[..f.len() - 1]).is_err());
        assert!(parse_frame("aoide/mail-pt", &f).is_err());
        assert!(parse_frame("aoide/mail-ctx", b"no terminator").is_err());
    }

    #[test]
    fn purpose_and_board_must_agree() {
        let mut c = sample_container();
        assert!(check_purpose_board(&c).is_ok(), "a letter carries no board");
        c.purpose = PURPOSE_POST.to_string();
        assert_eq!(check_purpose_board(&c).unwrap_err(), PURPOSE_MISMATCH, "a post must carry one");

        c.board = Some("00".repeat(32));
        assert!(check_purpose_board(&c).is_ok(), "a post with a board is well formed");
        c.epoch = Some(7);
        assert!(check_purpose_board(&c).is_ok(), "a post may name any epoch");

        c.purpose = PURPOSE_WRAP.to_string();
        assert!(check_purpose_board(&c).is_ok(), "a wrap carries a board like a post");
        c.board = None;
        assert_eq!(check_purpose_board(&c).unwrap_err(), PURPOSE_MISMATCH, "a wrap must carry one");

        c.purpose = PURPOSE_MAIL.to_string();
        c.epoch = Some(0);
        assert!(check_purpose_board(&c).is_ok());
        c.epoch = Some(3);
        assert_eq!(check_purpose_board(&c).unwrap_err(), PURPOSE_MISMATCH, "a letter carries epoch 0");

        c.purpose = "something-else".to_string();
        assert_eq!(check_purpose_board(&c).unwrap_err(), PURPOSE_MISMATCH);
    }

    fn sample_container() -> Container {
        Container {
            v: CONTAINER_VERSION,
            purpose: PURPOSE_MAIL.to_string(),
            msgid: "00".repeat(32),
            generation: 1,
            origin: Party { node: "a".to_string(), key: "00".repeat(32) },
            to: Destination { node: "b".to_string(), age: "age1".to_string() },
            origin_mesh: "home".to_string(),
            mesh: "home".to_string(),
            suite: SUITE_AGE_V1_X25519.to_string(),
            ct: String::new(),
            sig: "00".repeat(64),
            transit: Vec::new(),
            board: None,
            epoch: None,
        }
    }

    #[test]
    fn the_age_key_is_minted_next_to_the_identity_key_and_reloads() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STATE_DIR", "AOIDE_ROOT"]);
        env(&scratch("seal-age-key"));

        let (first, minted) = load_or_mint_age_identity().unwrap();
        assert!(minted);
        let (second, minted_again) = load_or_mint_age_identity().unwrap();
        assert!(!minted_again);
        assert_eq!(recipient_of(&first), recipient_of(&second));

        // Beside the identity key, in the same locked-down directory.
        let _ = identity::load_or_mint().unwrap();
        assert_eq!(age_key_path().file_name().unwrap(), "age.key");
        assert_eq!(age_key_path().parent(), Some(identity::identity_dir().as_path()));
        assert!(identity::identity_dir().join("ed25519.key").exists());
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(age_key_path()).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "the age key is owner-only");
    }

    #[test]
    fn a_binding_verifies_only_under_the_key_it_names() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STATE_DIR", "AOIDE_ROOT"]);
        env(&scratch("seal-binding-verify"));

        let b = binding_for_self(1);
        assert!(verify_binding(&b, None));
        assert!(verify_binding(&b, Some(&b.identity_key.clone())));
        assert!(!verify_binding(&b, Some(&"ab".repeat(32))));

        let mut tampered = b.clone();
        tampered.age_pubkey = recipient_of(&age::x25519::Identity::generate());
        assert!(!verify_binding(&tampered, None), "a swapped recipient breaks the signature");

        let mut wrong_purpose = b.clone();
        wrong_purpose.purpose = "something-else".to_string();
        assert!(!verify_binding(&wrong_purpose, None));

        let mut wrong_version = b.clone();
        wrong_version.v = 99;
        assert!(!verify_binding(&wrong_version, None));

        let mut unknown_suite = b.clone();
        unknown_suite.suites = vec!["age-v1-ssh-ed25519".to_string()];
        assert!(!verify_binding(&unknown_suite, None));
    }

    #[test]
    fn publishing_is_idempotent_and_rotation_retires_the_old_key() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STATE_DIR", "AOIDE_ROOT"]);
        env(&scratch("seal-rotation"));

        let first = publish_binding().unwrap();
        assert_eq!(first.generation, 1);
        let again = publish_binding().unwrap();
        assert_eq!(again, first, "publishing an unchanged key mints nothing new");

        let second = rotate_age_key().unwrap();
        assert_eq!(second.generation, 2);
        assert_ne!(second.age_pubkey, first.age_pubkey, "rotation mints a new age key");
        assert!(verify_binding(&second, None));

        let now = now_iso_utc();
        let live = live_retired_identities(&now);
        assert_eq!(live.len(), 1, "the superseded key is kept inside its grace window");
        assert_eq!(recipient_of(&live[0]), first.age_pubkey);
    }

    #[test]
    fn a_retired_key_stops_opening_after_its_window_and_is_swept() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STATE_DIR", "AOIDE_ROOT"]);
        env(&scratch("seal-retired-sweep"));

        let first = publish_binding().unwrap();
        let _ = rotate_age_key().unwrap();
        assert_eq!(live_retired_identities(&now_iso_utc()).len(), 1);

        // Wind the retired key's deadline back into the past.
        let deadline = crate::time::shift_iso_utc(&now_iso_utc(), -60);
        std::fs::write(retired_until_path(1), format!("{deadline}\n")).unwrap();
        assert!(retired_until_path(1).exists());
        assert!(live_retired_identities(&now_iso_utc()).is_empty());
        assert_eq!(sweep_retired_keys(&now_iso_utc()), 1);
        assert!(!retired_key_path(1).exists(), "the retired private key is deleted");
        // The TOMBSTONE stays (M3): `.until` and `.recipient` outlive the key
        // so a container sealed to it is still refused `key-retired` rather
        // than degrading to `open-failed`, which age returns just as readily
        // for a wrong recipient or a tampered `ct`.
        assert!(retired_until_path(1).exists(), "the window marker is a tombstone, not litter");
        assert!(retired_recipient_path(1).exists(), "and so is the address it answered to");
        let _ = first;
    }

    #[test]
    fn a_replayed_superseded_binding_cannot_become_current() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STATE_DIR", "AOIDE_ROOT"]);
        env(&scratch("seal-stale-binding"));

        // A peer record for a node, with that node's key.
        let kp = identity_keypair();
        let peer_key = kp.info().pubkey_hex.clone();
        let mut nodes = node_store::load_nodes();
        node_store::upsert_paired_node(&mut nodes, "peer", "ssh://peer", &peer_key, &now_iso_utc(), &["message".to_string()]);
        node_store::save_nodes(&nodes).unwrap();

        // The peer signs its own binding: sign with a keypair whose public
        // half is the one on record, which for this test is our own.
        let (id, _) = load_or_mint_age_identity().unwrap();
        let now = now_iso_utc();
        let older = mint_binding(&kp, &recipient_of(&id), 4, &now, &window_end(&now)).unwrap();
        let newer = mint_binding(&kp, &recipient_of(&id), 5, &now, &window_end(&now)).unwrap();

        assert!(learn_binding("peer", &newer).unwrap().is_some());
        assert_eq!(binding_for("peer").unwrap().generation, 5);
        assert_eq!(learn_binding("peer", &newer).unwrap(), None, "equal generation is idempotent");
        assert_eq!(
            learn_binding("peer", &older).unwrap_err(),
            STALE_BINDING,
            "a replayed superseded binding is refused for current use"
        );
        assert_eq!(binding_for("peer").unwrap().generation, 5);

        // A binding signed by a key that is not the one on record.
        let mut forged = newer.clone();
        forged.generation = 6;
        assert_eq!(learn_binding("peer", &forged).unwrap_err(), BINDING_MISMATCH);
    }

    #[test]
    fn binding_expiry_is_the_windows_close() {
        // Env-free on purpose: this is a comparison over ISO strings and
        // needs no state dir, so it takes no lock and cannot race one.
        let kp = identity::mint_ephemeral().unwrap();
        let now = now_iso_utc();
        let b = mint_binding(&kp, "age1example", 1, &now, &crate::time::shift_iso_utc(&now, 3600)).unwrap();
        assert!(!binding_expired(&b, &b.not_before));
        assert!(!binding_expired(&b, &b.not_after));
        assert!(binding_expired(&b, &crate::time::shift_iso_utc(&b.not_after, 1)));
        assert!(binding_not_yet_valid(&b, &crate::time::shift_iso_utc(&b.not_before, -1)));
    }

    #[test]
    fn no_private_material_in_any_serialize_type() {
        // The same mechanical gate `identity.rs` runs on itself: read this
        // file's own text and refuse a derive(Serialize) struct whose
        // fields look like key material.
        let src = include_str!("seal.rs");
        let mut in_serialize_struct = false;
        let mut suspicious = Vec::new();
        for line in src.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("pub struct ") {
                in_serialize_struct = false;
            }
            if trimmed.starts_with("#[derive(") && trimmed.contains("Serialize") {
                in_serialize_struct = true;
                continue;
            }
            if in_serialize_struct {
                if trimmed.starts_with("pub struct ") {
                    in_serialize_struct = false;
                } else if trimmed.starts_with("pub ") && (trimmed.contains("secret") || trimmed.contains("private")) {
                    suspicious.push(trimmed.to_string());
                }
            }
        }
        assert!(suspicious.is_empty(), "private material in a Serialize type: {suspicious:?}");
    }
}

/// The container's own acceptance evidence (`HTTPS-MESH-API.md`'s P-SEAL
/// test list, MAIL.md's slice entry). Every test here is a whole round trip
/// on one box: this node publishes a binding, seals a letter to itself, and
/// the two verification halves run against it. That is exactly the SSH
/// direct lane's shape — one hop, origin to destination — with the dial
/// replaced by a function call.
#[cfg(test)]
mod container_tests {
    use super::*;
    use aoide_test_support::EnvSaver;

    fn env(dir: &std::path::Path) {
        std::env::set_var("AOIDE_STATE_DIR", dir);
        std::env::set_var("AOIDE_ROOT", dir);
    }

    /// This node published as a peer of itself, plus a sealed letter to
    /// itself. Returns `(container, envelope)`.
    ///
    /// The body carries a counter, not just a timestamp: `minted_at` has
    /// one-second resolution, so two calls in the same second would mint
    /// byte-identical envelopes — same `msgid` — while age's fresh ephemeral
    /// key gives each a different `ct`. That is exactly the
    /// same-immutable-key/different-immutable-bytes collision the dedup gate
    /// refuses, so a fixture that did it accidentally would be testing the
    /// gate's collision arm instead of whatever the test is about.
    fn sealed_letter() -> (Container, mail::Envelope) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let body = format!("sealed letter body {}", SEQ.fetch_add(1, Ordering::SeqCst));

        let me = crate::display::local_host_name();
        let kp = identity::load_or_mint().unwrap().0;
        let mut nodes = node_store::load_nodes();
        node_store::upsert_paired_node(
            &mut nodes,
            &me,
            "ssh://self",
            &kp.info().pubkey_hex,
            &now_iso_utc(),
            &["message".to_string()],
        );
        node_store::save_nodes(&nodes).unwrap();
        let binding = publish_binding().unwrap();
        let envelope = mail::mint_outbound_letter("alice", &me, "bob", &body).unwrap();
        let container = seal_envelope(&envelope, &binding, "", "", &me, &now_iso_utc()).unwrap();
        (container, envelope)
    }

    fn reason(outcome: &ContainerOutcome) -> String {
        match outcome {
            ContainerOutcome::Refused { reason, .. } => reason.clone(),
            ContainerOutcome::Opened { .. } => "opened".to_string(),
            ContainerOutcome::Duplicate { .. } => "duplicate".to_string(),
        }
    }

    #[test]
    fn a_sealed_letter_round_trips_and_files_byte_identical_to_the_local_filing() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STATE_DIR", "AOIDE_ROOT"]);
        env(&aoide_test_support::unique_tmp("seal-round-trip"));

        let (container, envelope) = sealed_letter();
        let outcome = deposit_container(&container).unwrap();
        let ContainerOutcome::Opened { envelope: opened, .. } = &outcome else {
            panic!("a well-formed container opens: {outcome:?}");
        };
        assert_eq!(
            **opened, envelope,
            "the opened envelope is the minted one, field for field"
        );

        // …and it files through the same machinery a local letter does,
        // which is the slice's own acceptance wording.
        let filed = mail::deposit((**opened).clone(), "self").unwrap();
        assert!(matches!(filed, mail::DepositOutcome::Filed { .. }), "{filed:?}");
        let base = mail::read_base().unwrap();
        assert_eq!(base.len(), 1);
        assert_eq!(base[0].envelope, envelope, "filed byte-identical to the local filing");
        assert_eq!(base[0].envelope.text, envelope.text, "and the body came through");
    }

    /// M1 (the branch review): the gate means **admitted AND filed**, so it
    /// only ever answers for a container whose letter is actually on disk.
    #[test]
    fn a_filed_container_is_a_gated_duplicate_that_never_opens_again() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STATE_DIR", "AOIDE_ROOT"]);
        env(&aoide_test_support::unique_tmp("seal-duplicate"));

        let (container, envelope) = sealed_letter();
        let digest = match deposit_container(&container).unwrap() {
            ContainerOutcome::Opened { digest, .. } => digest,
            other => panic!("opens: {other:?}"),
        };

        // Not yet filed, not yet recorded: a second attempt RE-RUNS rather
        // than answering `duplicate` for a letter that is on no disk.
        assert!(
            matches!(deposit_container(&container).unwrap(), ContainerOutcome::Opened { .. }),
            "an unfiled, unrecorded container re-runs — the gate has nothing to gate on"
        );

        // Now file it and record it, which is the door's own order.
        let filed = mail::deposit(envelope.clone(), "self").unwrap();
        assert!(matches!(filed, mail::DepositOutcome::Filed { .. }), "{filed:?}");
        record_admitted(&container, &digest).unwrap();

        let again = deposit_container(&container).unwrap();
        match again {
            ContainerOutcome::Duplicate { filed_letter } => {
                assert!(filed_letter, "the filed record says this was a letter, so an ack is owed");
            }
            other => panic!("now it is a gated duplicate: {other:?}"),
        }
    }

    /// The review's own M1 case (b): a refusal decided AFTER the gate must
    /// answer the same refusal on every retry, not `duplicate`.
    #[test]
    fn a_refusal_past_the_gate_answers_the_same_refusal_on_a_retry() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STATE_DIR", "AOIDE_ROOT"]);
        env(&aoide_test_support::unique_tmp("seal-refusal-retries"));

        // An inner `ctx` that disagrees with the outer one: decided after the
        // outer signature and the gate, at the open.
        let (container, envelope) = sealed_letter();
        let (kp, _) = (identity::load_or_mint().unwrap().0, ());
        let mut other = Ctx::from_container(&container).unwrap();
        other.origin_mesh = "somewhere-else".to_string();
        let pt = frame(L_PT, &[&Ctx::from_container(&container).unwrap().to_bytes(), &envelope_frame(&envelope).unwrap()]);
        let recipient: age::x25519::Recipient = container.to.age.parse().unwrap();
        let ct = encrypt_to(&recipient, &pt).unwrap();
        let mut broken = container.clone();
        broken.ct = hex_encode(&ct);
        broken.origin_mesh = "somewhere-else".to_string();
        broken.sig = wire_auth::sign_hex(&kp, &outer_bytes(&other, &ct));

        for attempt in 1..=3 {
            assert_eq!(
                reason(&deposit_container(&broken).unwrap()),
                CONTEXT_MISMATCH,
                "attempt {attempt} answers the taught word, not `duplicate`"
            );
        }
    }

    /// The review's own M1 case (a): a crash between the gate's write and the
    /// filing must RE-ACCEPT, not report a filing that never happened.
    #[test]
    fn a_crash_between_the_container_record_and_the_filing_re_accepts() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STATE_DIR", "AOIDE_ROOT"]);
        env(&aoide_test_support::unique_tmp("seal-crash-reaccept"));

        let (container, envelope) = sealed_letter();
        let digest = match deposit_container(&container).unwrap() {
            ContainerOutcome::Opened { digest, .. } => digest,
            other => panic!("opens: {other:?}"),
        };
        // The crash: the letter FILED (base + seen) but the gate's own record
        // was never written.
        let _ = mail::deposit(envelope.clone(), "self").unwrap();

        // The retry re-opens once — it is not recorded — and the filing is
        // then answered from `seen.jsonl`, which is what re-spools the lost
        // ack. The letter is never filed twice and never silently dropped.
        let reopened = match deposit_container(&container).unwrap() {
            ContainerOutcome::Opened { envelope, digest: again } => {
                assert_eq!(again, digest, "the same immutable bytes: a re-open, not a collision");
                envelope
            }
            other => panic!("an unrecorded container re-opens: {other:?}"),
        };
        assert_eq!(reopened.msgid, envelope.msgid);
        assert_eq!(
            mail::filed_kind(&envelope.msgid).as_deref(),
            Some(mail::ENTRY_TYPE_LETTER),
            "and the filed record is the authority the duplicate answer comes from"
        );
        let second = mail::deposit((*reopened).clone(), "self").unwrap();
        match second {
            mail::DepositOutcome::Duplicate { filed_letter } => {
                assert!(filed_letter, "re-accepted exactly once, and the ack is owed")
            }
            other => panic!("a second filing is a duplicate, never a second filing: {other:?}"),
        }
        assert_eq!(mail::read_base().unwrap().len(), 1, "and nothing was filed twice");

        // Once recorded, the gate takes over and the open never happens again.
        record_admitted(&container, &digest).unwrap();
        assert!(matches!(
            deposit_container(&container).unwrap(),
            ContainerOutcome::Duplicate { .. }
        ));
    }

    #[test]
    fn a_collision_on_msgid_with_different_immutable_bytes_is_refused() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STATE_DIR", "AOIDE_ROOT"]);
        env(&aoide_test_support::unique_tmp("seal-collision"));

        let (container, _) = sealed_letter();
        assert!(matches!(deposit_container(&container).unwrap(), ContainerOutcome::Opened { .. }));

        // Same `(origin.key, to.node, msgid)`, different `ct`: one origin
        // must not be able to reserve a message id and then substitute the
        // bytes it stands for. Re-signed over the new `ct` with the same
        // `ctx`, so the outer check passes and dedup is what catches it.
        let mut swapped = container.clone();
        let other = sealed_letter().0;
        let ctx = Ctx::from_container(&swapped).unwrap();
        let ct = hex_decode(&other.ct).unwrap();
        swapped.ct = other.ct;
        let (kp, _) = (identity::load_or_mint().unwrap().0, ());
        swapped.sig = wire_auth::sign_hex(&kp, &outer_bytes(&ctx, &ct));
        let outcome = deposit_container(&swapped).unwrap();
        assert_eq!(reason(&outcome), CONTEXT_MISMATCH, "{outcome:?}");
    }

    #[test]
    fn a_tampered_field_is_refused_and_never_opened() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STATE_DIR", "AOIDE_ROOT"]);
        env(&aoide_test_support::unique_tmp("seal-tamper"));

        let (container, _) = sealed_letter();

        // A flipped byte in `ct` — the outer signature no longer covers it.
        let mut flipped = container.clone();
        let mut bytes = hex_decode(&flipped.ct).unwrap();
        bytes[0] ^= 0x01;
        flipped.ct = hex_encode(&bytes);
        assert_eq!(reason(&deposit_container(&flipped).unwrap()), "unverified-origin");

        // A flipped byte in an outer `ctx` field.
        let mut ctx_tampered = container.clone();
        ctx_tampered.generation += 1;
        assert_eq!(reason(&deposit_container(&ctx_tampered).unwrap()), "unverified-origin");

        // `to.age` re-pointed.
        let mut repointed = container.clone();
        repointed.to.age = recipient_of(&age::x25519::Identity::generate());
        assert_eq!(reason(&deposit_container(&repointed).unwrap()), "unverified-origin");

        // `msgid` altered.
        let mut msgid_tampered = container.clone();
        msgid_tampered.msgid = "1f".repeat(32);
        assert_eq!(reason(&deposit_container(&msgid_tampered).unwrap()), "unverified-origin");
    }

    #[test]
    fn a_container_for_another_recipient_fails_to_open() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STATE_DIR", "AOIDE_ROOT"]);
        env(&aoide_test_support::unique_tmp("seal-wrong-recipient"));

        let (container, _) = sealed_letter();
        // Seal the same letter to a key this node does not hold, then put
        // that `ct` in place of ours — signed over, so the outer layer
        // accepts it and the OPEN is what fails.
        let stranger = age::x25519::Identity::generate();
        let (kp, _) = (identity::load_or_mint().unwrap().0, ());
        let ctx = Ctx::from_container(&container).unwrap();
        let pt = frame(L_PT, &[&ctx.to_bytes(), &envelope_frame(&sealed_letter().1).unwrap()]);
        let ct = encrypt_to(&stranger.to_public(), &pt).unwrap();
        let mut wrong = container.clone();
        wrong.ct = hex_encode(&ct);
        wrong.sig = wire_auth::sign_hex(&kp, &outer_bytes(&ctx, &ct));
        assert_eq!(reason(&deposit_container(&wrong).unwrap()), OPEN_FAILED);
    }

    #[test]
    fn a_reshaped_ctx_is_refused_before_the_open() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STATE_DIR", "AOIDE_ROOT"]);
        env(&aoide_test_support::unique_tmp("seal-purpose-mismatch"));

        let (container, _) = sealed_letter();

        let mut postless = container.clone();
        postless.purpose = PURPOSE_POST.to_string();
        assert_eq!(reason(&deposit_container(&postless).unwrap()), PURPOSE_MISMATCH);

        let mut boarded = container.clone();
        boarded.board = Some("ab".repeat(32));
        boarded.epoch = Some(0);
        assert_eq!(reason(&deposit_container(&boarded).unwrap()), PURPOSE_MISMATCH);

        let mut suite = container.clone();
        suite.suite = "age-v2-x25519".to_string();
        assert_eq!(reason(&deposit_container(&suite).unwrap()), UNSUPPORTED_SUITE);

        let mut version = container.clone();
        version.v = 99;
        assert_eq!(reason(&deposit_container(&version).unwrap()), UNSUPPORTED_CONTAINER_VERSION);

        let mut malformed = container.clone();
        malformed.msgid = "not-hex".to_string();
        assert_eq!(reason(&deposit_container(&malformed).unwrap()), CONTEXT_MISMATCH);
    }

    #[test]
    fn an_unverified_origin_is_refused_before_anything_is_opened() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STATE_DIR", "AOIDE_ROOT"]);
        env(&aoide_test_support::unique_tmp("seal-unverified-origin"));

        let (container, _) = sealed_letter();
        // Forget the origin's record: no key on record, no verification.
        node_store::save_nodes(&[]).unwrap();
        assert_eq!(reason(&deposit_container(&container).unwrap()), "unverified-origin");
    }

    #[test]
    fn a_broken_chain_is_refused_and_the_origin_cannot_be_erased() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STATE_DIR", "AOIDE_ROOT"]);
        env(&aoide_test_support::unique_tmp("seal-chain"));

        let (container, _) = sealed_letter();
        assert_eq!(container.transit.len(), 1, "the direct lane is exactly one entry");
        assert_eq!(container.transit[0].node, container.origin.node, "and it is the origin's");

        // Every probe below uses a FRESH, undeposited letter: `transit` is
        // deliberately outside the dedup digest, so tampering with a chain
        // leaves the immutable bytes alone and a second probe of the same
        // container would correctly come back `duplicate` instead. That is
        // the property, not a limitation — it is what makes a retry over
        // another route a duplicate rather than a collision.
        let me = crate::display::local_host_name();
        let msgid_of = |c: &Container| Ctx::from_container(c).unwrap().msgid;

        // A hop signing itself as an entry, with an origin it is not: the
        // origin's own entry 1 is what makes this fail.
        let (cut, _) = sealed_letter();
        let msgid = msgid_of(&cut);
        let stranger = identity::mint_ephemeral().unwrap();
        let at = now_iso_utc();
        let mut erased = cut.clone();
        erased.transit = vec![TransitEntry {
            node: "not-the-origin".to_string(),
            next: me.clone(),
            at: at.clone(),
            mesh: String::new(),
            sig: wire_auth::sign_hex(&stranger, &hop_bytes(&msgid, &msgid, "not-the-origin", &me, &at, "")),
        }];
        assert_eq!(reason(&deposit_container(&erased).unwrap()), BROKEN_CHAIN);

        // A hop signing itself AS the origin, without the origin's key: the
        // origin's signature cannot be forged, so entry 1 fails under it.
        let (forged_src, _) = sealed_letter();
        let forged_msgid = msgid_of(&forged_src);
        let mut forged_origin = forged_src.clone();
        forged_origin.transit[0].sig =
            wire_auth::sign_hex(&stranger, &hop_bytes(&forged_msgid, &forged_msgid, &me, &me, &at, ""));
        assert_eq!(reason(&deposit_container(&forged_origin).unwrap()), BROKEN_CHAIN);

        // An empty chain.
        let (empty_src, _) = sealed_letter();
        let mut empty = empty_src.clone();
        empty.transit.clear();
        assert_eq!(reason(&deposit_container(&empty).unwrap()), BROKEN_CHAIN);

        // A chain that stops short of this node.
        let (short_src, _) = sealed_letter();
        let mut elsewhere = short_src.clone();
        elsewhere.transit[0].next = "somewhere-else".to_string();
        assert_eq!(reason(&deposit_container(&elsewhere).unwrap()), BROKEN_CHAIN);
    }

    #[test]
    fn an_inner_ctx_that_disagrees_with_the_outer_is_refused() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STATE_DIR", "AOIDE_ROOT"]);
        env(&aoide_test_support::unique_tmp("seal-context-mismatch"));

        let (container, envelope) = sealed_letter();
        let (kp, _) = (identity::load_or_mint().unwrap().0, ());
        let ctx = Ctx::from_container(&container).unwrap();

        // The relay attack the design names: re-sign someone else's `ct`
        // under our own key and a `ctx` we chose, with the INNER copy still
        // carrying the original. The equality check catches it at the open,
        // before any inner field is used.
        let mut other = ctx.clone();
        other.origin_mesh = "somewhere-else".to_string();
        let pt = frame(L_PT, &[&ctx.to_bytes(), &envelope_frame(&envelope).unwrap()]);
        let recipient: age::x25519::Recipient = container.to.age.parse().unwrap();
        let ct = encrypt_to(&recipient, &pt).unwrap();
        let mut resigned = container.clone();
        resigned.ct = hex_encode(&ct);
        resigned.origin_mesh = "somewhere-else".to_string();
        resigned.sig = wire_auth::sign_hex(&kp, &outer_bytes(&other, &ct));
        assert_eq!(reason(&deposit_container(&resigned).unwrap()), CONTEXT_MISMATCH);
    }

    #[test]
    fn the_outbox_and_the_wire_hold_no_letter_bytes() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STATE_DIR", "AOIDE_ROOT"]);
        env(&aoide_test_support::unique_tmp("seal-no-plaintext"));

        let (container, envelope) = sealed_letter();
        let serialized = serde_json::to_string(&container).unwrap();
        for needle in ["sealed letter body", "alice", "bob"] {
            assert!(
                !serialized.contains(needle),
                "the container holds no {needle:?}: {serialized}"
            );
        }
        // What it DOES leak is the design's own list, and nothing more: the
        // body is inside `ct`.
        assert!(serialized.contains(&container.origin.node));
        assert!(serialized.contains(&container.to.age));

        // And the opened envelope proves the body survived the round trip,
        // so the absence above is encryption and not loss.
        let ContainerOutcome::Opened { envelope: opened, .. } = deposit_container(&container).unwrap() else {
            panic!("opens");
        };
        assert_eq!(opened.text, envelope.text);
    }

    #[test]
    fn a_resend_resends_the_stored_container_byte_identical() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STATE_DIR", "AOIDE_ROOT"]);
        env(&aoide_test_support::unique_tmp("seal-identical-resend"));

        let (container, envelope) = sealed_letter();
        let entry = crate::outbox::OutboxEntry::sealed(envelope, container);
        crate::outbox::write_entry("elsewhere", &entry).unwrap();
        let first = serde_json::to_string(&entry).unwrap();

        // A retry reads the spool back and sends what it read.
        let reread = crate::outbox::list_entries("elsewhere").unwrap();
        assert_eq!(reread.len(), 1);
        assert_eq!(serde_json::to_string(&reread[0]).unwrap(), first, "byte-identical on a retry");
        assert!(reread[0].is_sealed(), "and still sealed");
    }

    #[test]
    fn a_retired_recipient_reports_key_retired_once_its_window_closes() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STATE_DIR", "AOIDE_ROOT"]);
        env(&aoide_test_support::unique_tmp("seal-key-retired"));

        // A letter sealed to generation 1, then rotated away from.
        let (container, _) = sealed_letter();
        // A second, undeposited letter sealed to the SAME generation-1 key:
        // the first one is already through the dedup gate.
        let gen1_binding = publish_binding().unwrap();
        let me = crate::display::local_host_name();
        let second_letter = mail::mint_outbound_letter("alice", &me, "bob", "second body").unwrap();
        let to_gen1 = seal_envelope(&second_letter, &gen1_binding, "", "", &me, &now_iso_utc()).unwrap();
        assert_eq!(to_gen1.to.age, container.to.age);
        let _ = rotate_age_key().unwrap();

        // Inside the grace window the old key still opens it.
        assert!(matches!(deposit_container(&to_gen1).unwrap(), ContainerOutcome::Opened { .. }));

        // Wind the deadline past, and a THIRD container to the same retired
        // key is `key-retired` rather than a bare open failure — the one
        // case an operator can act on.
        let third_letter = mail::mint_outbound_letter("alice", &me, "bob", "third body").unwrap();
        let to_retired = seal_envelope(&third_letter, &gen1_binding, "", "", &me, &now_iso_utc()).unwrap();
        let closed = crate::time::shift_iso_utc(&now_iso_utc(), -60);
        std::fs::write(retired_until_path(1), format!("{closed}\n")).unwrap();
        assert_eq!(reason(&deposit_container(&to_retired).unwrap()), OPEN_KEY_RETIRED);

        // Once swept, the key is gone and an ordinary open failure is the
        // honest answer: "retired" and "never ours" are the same fact from
        // here.
        assert_eq!(sweep_retired_keys(&now_iso_utc()), 1);
        // M3: the refusal does NOT degrade once the key is swept. The
        // tombstone keeps the address, so the origin's retry loop still
        // learns `key-retired` — which is the whole point of keeping it.
        let fifth_letter = mail::mint_outbound_letter("alice", &me, "bob", "fifth body").unwrap();
        let fifth = seal_envelope(&fifth_letter, &gen1_binding, "", "", &me, &now_iso_utc()).unwrap();
        assert_eq!(
            reason(&deposit_container(&fifth).unwrap()),
            OPEN_KEY_RETIRED,
            "a swept key still answers by its tombstone"
        );
        assert!(!retired_key_path(1).exists(), "and its private key is gone");
    }
}

/// M2, L6, L7 (the branch review): the binding high-water across a re-key, a
/// chain with more than one entry, and "already-filed mail survives key loss".
#[cfg(test)]
mod review_fix_tests {
    use super::*;
    use aoide_test_support::EnvSaver;

    fn env(dir: &std::path::Path) {
        std::env::set_var("AOIDE_STATE_DIR", dir);
        std::env::set_var("AOIDE_ROOT", dir);
    }

    /// M2: the high-water mark is per IDENTITY KEY, so a re-pair with a fresh
    /// key must be LEARNED even though its generation is lower than the
    /// residue left by the dead key.
    #[test]
    fn a_repair_with_a_fresh_identity_key_is_learned_over_a_stored_binding() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STATE_DIR", "AOIDE_ROOT"]);
        env(&aoide_test_support::unique_tmp("seal-rekey"));

        // A peer record, and a generation-5 binding signed by its key.
        let old_key = identity::mint_ephemeral().unwrap();
        let mut nodes = node_store::load_nodes();
        node_store::upsert_paired_node(
            &mut nodes,
            "peer",
            "ssh://peer",
            &old_key.info().pubkey_hex,
            &now_iso_utc(),
            &["message".to_string()],
        );
        node_store::save_nodes(&nodes).unwrap();
        let now = now_iso_utc();
        let end = crate::time::shift_iso_utc(&now, 3600);
        let old_binding = mint_binding(&old_key, "age1oldkey", 5, &now, &end).unwrap();
        learn_binding("peer", &old_binding).unwrap();
        assert_eq!(binding_for("peer").unwrap().generation, 5);

        // The peer re-mints its identity key and re-pairs — the supported
        // recovery path. Its fresh binding is generation 1.
        let new_key = identity::mint_ephemeral().unwrap();
        let mut nodes = node_store::load_nodes();
        node_store::upsert_paired_node(
            &mut nodes,
            "peer",
            "ssh://peer",
            &new_key.info().pubkey_hex,
            &now_iso_utc(),
            &["message".to_string()],
        );
        node_store::save_nodes(&nodes).unwrap();
        let fresh = mint_binding(&new_key, "age1newkey", 1, &now, &end).unwrap();

        // Before M2 this was a permanent `stale-binding` wedge: `1 < 5`
        // against a file signed by a key this node no longer holds, never
        // overwritten, never deleted — the peer silently downgraded to
        // plaintext for the life of the install.
        assert!(
            learn_binding("peer", &fresh).is_ok(),
            "a generation-1 binding under a NEW identity key is learned, not wedged"
        );
        assert_eq!(binding_for("peer").unwrap().age_pubkey, "age1newkey");
        assert_eq!(binding_for("peer").unwrap().generation, 1);

        // And `node remove`'s own path leaves nothing behind.
        assert!(forget_binding("peer").unwrap());
        assert!(binding_for("peer").is_none());
        assert!(!forget_binding("peer").unwrap(), "forgetting twice is a no-op, not an error");
    }

    /// L5: no pinned key on file means no acceptance — and `node remove`
    /// deletes the binding so a re-added name inherits nothing.
    #[test]
    fn a_node_with_no_key_on_file_accepts_no_binding() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STATE_DIR", "AOIDE_ROOT"]);
        env(&aoide_test_support::unique_tmp("seal-unpinned"));

        let kp = identity::mint_ephemeral().unwrap();
        let mut nodes = node_store::load_nodes();
        // `node add` path: registered, pubkey None, unverified.
        node_store::upsert_paired_node(&mut nodes, "peer", "ssh://peer", "", &now_iso_utc(), &[]);
        if let Some(n) = nodes.iter_mut().find(|n| n.name == "peer") {
            n.pubkey = None;
        }
        node_store::save_nodes(&nodes).unwrap();

        let now = now_iso_utc();
        let end = crate::time::shift_iso_utc(&now, 3600);
        let b = mint_binding(&kp, "age1unpinned", 1, &now, &end).unwrap();
        assert_eq!(
            learn_binding("peer", &b).unwrap_err(),
            BINDING_MISMATCH,
            "a binding is accepted only inside a key that was pinned"
        );
    }

    /// L6: a chain with MORE than one entry — the interior `prev`
    /// recomputation, the `expected_next` link check and the per-hop key
    /// lookup, none of which the direct lane's single-entry fixtures reach.
    #[test]
    fn a_two_entry_chain_walks_and_an_interior_truncation_is_refused() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STATE_DIR", "AOIDE_ROOT"]);
        env(&aoide_test_support::unique_tmp("seal-two-hop"));

        let me = crate::display::local_host_name();
        let origin_kp = identity::load_or_mint().unwrap().0;
        let relay_kp = identity::mint_ephemeral().unwrap();
        let mut nodes = node_store::load_nodes();
        for (name, key) in [(&me, origin_kp.info().pubkey_hex), (&"relay".to_string(), relay_kp.info().pubkey_hex)] {
            node_store::upsert_paired_node(
                &mut nodes,
                name,
                "ssh://x",
                &key,
                &now_iso_utc(),
                &["message".to_string()],
            );
        }
        node_store::save_nodes(&nodes).unwrap();

        let msgid = sha256(b"a msgid for the two-hop fixture");
        let at = now_iso_utc();
        let mesh = "home";
        let entry1_sig =
            wire_auth::sign_hex(&origin_kp, &hop_bytes(&msgid, &msgid, &me, "relay", &at, mesh));
        let entry1 = TransitEntry {
            node: me.clone(),
            next: "relay".to_string(),
            at: at.clone(),
            mesh: mesh.to_string(),
            sig: entry1_sig,
        };
        let prev2 = sha256(&hop_entry_bytes(&msgid, &msgid, &entry1).unwrap());
        let entry2 = TransitEntry {
            node: "relay".to_string(),
            next: me.clone(),
            at: at.clone(),
            mesh: mesh.to_string(),
            sig: wire_auth::sign_hex(&relay_kp, &hop_bytes(&msgid, &prev2, "relay", &me, &at, mesh)),
        };

        let ctx = Ctx {
            v: CONTAINER_VERSION,
            purpose: PURPOSE_MAIL.to_string(),
            msgid,
            origin_node: me.clone(),
            origin_key: hex_decode(&origin_kp.info().pubkey_hex).unwrap().try_into().unwrap(),
            to_node: me.clone(),
            to_age: "age1x".to_string(),
            origin_mesh: mesh.to_string(),
            suite: SUITE_AGE_V1_X25519.to_string(),
            generation: 1,
            board: None,
            epoch: 0,
        };

        let container = |transit: Vec<TransitEntry>| Container {
            v: CONTAINER_VERSION,
            purpose: PURPOSE_MAIL.to_string(),
            msgid: hex_encode(&msgid),
            generation: 1,
            origin: Party { node: me.clone(), key: origin_kp.info().pubkey_hex.clone() },
            to: Destination { node: me.clone(), age: "age1x".to_string() },
            origin_mesh: mesh.to_string(),
            mesh: mesh.to_string(),
            suite: SUITE_AGE_V1_X25519.to_string(),
            ct: String::new(),
            sig: String::new(),
            transit,
            board: None,
            epoch: None,
        };
        let nodes = node_store::load_nodes();

        // The whole chain walks.
        if let Err(e) = walk_chain(&container(vec![entry1.clone(), entry2.clone()]), &ctx, &nodes) {
            panic!("origin -> relay -> self must walk: {e}");
        }

        // Interior truncation: drop entry 1 and keep entry 2. Its `prev` was
        // computed over entry 1's frame, and `prev` is recomputed, so the
        // link breaks — the cut-and-reappend the chain exists to refuse.
        let err = walk_chain(&container(vec![entry2.clone()]), &ctx, &nodes).unwrap_err();
        assert!(err.contains("not the origin"), "{err}");

        // Reordering breaks the `next` link: entry 2 was rewritten to hand
        // the letter to `self`, so entry 3 cannot be relay's.
        let mut reordered = entry2.clone();
        reordered.next = me.clone();
        let err = walk_chain(&container(vec![entry1.clone(), reordered, entry2.clone()]), &ctx, &nodes)
            .unwrap_err();
        assert!(err.contains("entry 3 is by `relay`"), "{err}");

        // A relay's own key is what verifies entry 2: sign it with the wrong
        // one and the interior hop fails.
        let mut forged = entry2.clone();
        forged.sig = wire_auth::sign_hex(&origin_kp, &hop_bytes(&msgid, &prev2, "relay", &me, &at, mesh));
        let err = walk_chain(&container(vec![entry1, forged]), &ctx, &nodes).unwrap_err();
        assert!(err.contains("does not verify"), "{err}");
    }

    /// L7: "Already-filed letters survive loss of the age key" — structurally
    /// true because `base.jsonl` holds the opened envelope in plaintext, and
    /// asserted here rather than assumed.
    #[test]
    fn already_filed_mail_survives_the_loss_of_the_age_key() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STATE_DIR", "AOIDE_ROOT"]);
        env(&aoide_test_support::unique_tmp("seal-filed-survives"));

        let me = crate::display::local_host_name();
        let (kp, _) = identity::load_or_mint().unwrap();
        let mut nodes = node_store::load_nodes();
        node_store::upsert_paired_node(&mut nodes, &me, "ssh://self", &kp.info().pubkey_hex, &now_iso_utc(), &["message".to_string()]);
        node_store::save_nodes(&nodes).unwrap();
        let binding = publish_binding().unwrap();
        let envelope = mail::mint_outbound_letter("alice", &me, "bob", "filed and safe").unwrap();
        let container = seal_envelope(&envelope, &binding, "", "", &me, &now_iso_utc()).unwrap();

        let ContainerOutcome::Opened { envelope: opened, .. } = deposit_container(&container).unwrap() else {
            panic!("opens");
        };
        assert!(matches!(
            mail::deposit((*opened).clone(), "self").unwrap(),
            mail::DepositOutcome::Filed { .. }
        ));

        // The key is gone — rotated away with no retired copy, the worst case.
        std::fs::remove_file(age_key_path()).unwrap();
        let _ = std::fs::remove_dir_all(retired_dir());

        // The filed letter is untouched, body and all; only sealed-but-unfiled
        // copies are lost.
        let base = mail::read_base().unwrap();
        assert_eq!(base.len(), 1);
        assert_eq!(base[0].envelope.text, "filed and safe");
        assert_eq!(mail::filed_kind(&envelope.msgid).as_deref(), Some(mail::ENTRY_TYPE_LETTER));
    }
}
