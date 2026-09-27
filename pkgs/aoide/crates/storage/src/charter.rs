//! `charter` — one operator's machines, signed by that operator's own key
//! (P-CHARTER, `docs/architecture/HTTPS-MESH-API.md` "Charters", MAIL.md's
//! P-CHARTER slice).
//!
//! A charter is a mesh's node list: each node's identity key, its self-signed
//! age binding, its address and its grant, plus the mesh's relays. It is
//! shaped like agenix — public keys in one file, one signer — and the
//! signature covers the **digest of the operator's file bytes**, never a
//! canonical re-serialization: `sign` is the last write to a charter file.
//!
//! ## What is here
//!
//! - [`Charter`] and [`parse`]: the document and everything a reader must
//!   check before believing a line of it (name grammar, capability
//!   vocabulary, addresses, relays that are nodes of the mesh, and each node's
//!   binding verifying under the key on its own line).
//! - [`accept`]: the five steps the design lists, in the order that makes each
//!   taught refusal the true one — the digest check needs no parse and no
//!   trust, so a touched file is [`CHARTER_TAMPERED`] and never a bad key.
//! - The **operator key** ([`operator_key_path`], [`load_operator_key`],
//!   [`replace_operator_key`]) — one Ed25519 key per mesh, `0600` beside the
//!   identity key's own discipline, never printed by anything but its public
//!   line.
//! - The **state a node keeps**: `state/mesh/<mesh>/charter.toml` and its
//!   `.sig` in force, and `state/mesh/<mesh>/trust.json` — the operator key
//!   this node trusts for that mesh plus the highest version seen per
//!   `(mesh, operator key)`, and the nodes the last version re-keyed.
//! - [`node_line`]: the line a machine prints for its operator to paste, and
//!   [`sign`]/[`reroot`]/[`init`], the operator-side flows.
//!
//! ## What is deliberately NOT here
//!
//! **No expiry window and no pre-committed successor key.** Both are the
//! User's ruling of 2026-09-26: charters carry no validity window for the
//! beta (a relay dropping a newer charter delays a revocation, and that gap is
//! named in the Threat model; adding a window later is a new charter format
//! version), and re-rooting stays one touch per machine.
//!
//! **No grants enforcement.** A charter line's `grant` is parsed and carried
//! here; reading it at a door, per mesh, is the trust-per-mesh slice's
//! (`aoide-server`'s), not this module's.

use crate::config;
use crate::display;
use crate::fs;
use crate::identity;
use crate::mail;
use crate::node_store;
use crate::outbox;
use crate::seal;
use crate::time::now_iso_utc;
use crate::wire_auth;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

// ── The taught refusals ─────────────────────────────────────────────────

/// No operator key is trusted for that mesh — neither the config line nor a
/// state record names one.
pub const UNKNOWN_OPERATOR: &str = "unknown-operator";
/// A version not above the high-water mark for `(mesh, operator key)`.
pub const STALE_CHARTER: &str = "stale-charter";
/// The received bytes are not the artifact the signature was made over. This
/// is what a trailing newline, a formatter or a line-ending change produces,
/// and it is a different answer from a key that does not verify.
pub const CHARTER_TAMPERED: &str = "charter-tampered";
/// The config's operator line and the state record disagree about which key
/// signs this mesh.
pub const OPERATOR_MISMATCH: &str = "operator-mismatch";
/// `config.toml` exists and could not be honoured, so which operator key this
/// host trusts is unknowable. Never a missing file (that is defaults).
pub const CONFIG_UNREADABLE: &str = "config-unreadable";
/// The charter itself was fine; THIS node could not read or write its own
/// state (the mesh's lock, `charter.toml`, its `.sig`, `trust.json`). A
/// distinct word from the four above because it says nothing about the
/// charter — and, unlike them, it can leave the writes it had already made in
/// place (see `accept`'s write order).
pub const LOCAL_IO: &str = "local-io";

/// The prefix every identity key carries wherever it is written down: a
/// charter line's `key`, and config's `[mesh.<name>] operator`.
pub const KEY_PREFIX: &str = "ed25519:";

/// A node with no declared address answers on no inbound transport; it
/// connects out and polls (`HTTPS-MESH-API.md` "Transports and relays").
pub const DEFAULT_ADDRESS: &str = "poll";
/// A node with no declared grant gets `message` and nothing else: the
/// conservative default, because writing the line is what grants more.
pub const DEFAULT_GRANT: &str = "message";
/// The closed vocabulary `[status]` may hold (MAIL.md §Transit). Read by
/// P-M4's router; parsed and checked here so a typo is refused at `sign`
/// rather than ignored at routing time.
pub const STATUS_VALUES: &[&str] = &["hold", "down"];

// ── Paths ───────────────────────────────────────────────────────────────

/// The operator's charter sources: `$AOIDE_ROOT/charters/`. Public keys and
/// addresses only, which is what makes a charter file safe to keep in a Nix
/// repository.
pub fn charters_dir() -> PathBuf {
    fs::root().join("charters")
}

/// `$AOIDE_ROOT/charters/<mesh>.toml` — the default source `sign` reads and
/// bumps, and the file `init` writes.
pub fn source_path(mesh: &str) -> PathBuf {
    charters_dir().join(format!("{mesh}.toml"))
}

/// The detached signature's path: `<source>.sig`, APPENDED to whatever the
/// source's name is, so `--file /some/repo/home.toml` signs
/// `/some/repo/home.toml.sig` and never replaces an extension.
pub fn sig_path_for(source: &Path) -> PathBuf {
    let mut name = source.as_os_str().to_os_string();
    name.push(".sig");
    PathBuf::from(name)
}

/// `$AOIDE_ROOT/charters/<mesh>.toml.sig`.
pub fn source_sig_path(mesh: &str) -> PathBuf {
    sig_path_for(&source_path(mesh))
}

/// One mesh's own state directory: `state/mesh/<mesh>/`.
pub fn mesh_state_dir(mesh: &str) -> PathBuf {
    fs::state_dir().join("mesh").join(mesh)
}

/// The charter in force at this node: `state/mesh/<mesh>/charter.toml`.
pub fn in_force_path(mesh: &str) -> PathBuf {
    mesh_state_dir(mesh).join("charter.toml")
}

/// Its detached signature: `state/mesh/<mesh>/charter.toml.sig`.
pub fn in_force_sig_path(mesh: &str) -> PathBuf {
    sig_path_for(&in_force_path(mesh))
}

/// This node's record for one mesh: what it trusts and how far it has seen.
pub fn trust_path(mesh: &str) -> PathBuf {
    mesh_state_dir(mesh).join("trust.json")
}

/// The lock every read and write of one mesh's charter state is serialized
/// under: `state/mesh/<mesh>/.charter.lock`, held through
/// [`crate::fs::lock_path`]'s blocking `flock(LOCK_EX)`.
///
/// **Per mesh, and one file for the whole apply.** [`accept`]'s sequence is a
/// read-modify-write of three files (the mark, the charter, its `.sig`), and
/// the door is thread-per-connection while a `poll` process can be running
/// beside it — two applies that each read the old mark would let the OLDER
/// version win the write, silently un-applying a revocation and leaving the
/// mark below a version that has already been applied. One lock per mesh makes
/// the apply exclusive; two different meshes never contend, and no other
/// reader in this crate takes a charter lock at all.
pub fn charter_lock_path(mesh: &str) -> PathBuf {
    mesh_state_dir(mesh).join(".charter.lock")
}

/// `state/operator/` — one directory beside `state/identity/`.
pub fn operator_dir() -> PathBuf {
    fs::state_dir().join("operator")
}

/// `state/operator/<mesh>.key` — one mesh's operator key, raw 32-byte seed,
/// `0600`, on the operator's machine only.
pub fn operator_key_path(mesh: &str) -> PathBuf {
    operator_dir().join(format!("{mesh}.key"))
}

// ── The document ────────────────────────────────────────────────────────

/// One node's line in a charter, after validation: the identity key (bare
/// lowercase hex, the `ed25519:` prefix stripped — this is the shape a
/// binding's `identity_key` and a container's `origin.key` are in), the
/// verified age binding, the address and the grant.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeLine {
    pub key: String,
    pub age: seal::Binding,
    pub address: String,
    pub grant: Vec<String>,
}

/// A parsed and validated charter.
///
/// `relays`, `address` and `grant` are carried rather than validated-and-
/// dropped: they are the routing table and the grants P-M4 and the door read
/// out of the charter in force, and a parser that silently discards half the
/// document is a liar to the next reader. `[status]` and `[gates]` are the
/// opposite case — their semantics are P-M4's alone, so this slice checks
/// their shape and carries nothing.
#[derive(Debug, Clone, PartialEq)]
pub struct Charter {
    pub mesh: String,
    pub version: u64,
    pub relays: Vec<String>,
    pub nodes: BTreeMap<String, NodeLine>,
}

impl Charter {
    /// The identity key (bare hex) this charter lists for `node`, if any.
    pub fn node_key(&self, node: &str) -> Option<String> {
        self.nodes.get(node).map(|line| line.key.clone())
    }
}

/// `Charter`'s deserialization twin: the same document with the fields
/// validation turns into types (`key` → bare hex + verified binding) still in
/// their written form, and with `[status]`/`[gates]` present so they can be
/// checked at all. `deny_unknown_fields` on both: a typo'd key in a file that
/// grants mesh access is refused by name, never ignored.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCharter {
    mesh: String,
    version: u64,
    #[serde(default)]
    relays: Vec<String>,
    #[serde(default)]
    nodes: BTreeMap<String, RawNode>,
    #[serde(default)]
    status: BTreeMap<String, String>,
    #[serde(default)]
    gates: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawNode {
    key: String,
    age: String,
    #[serde(default = "default_address")]
    address: String,
    #[serde(default = "default_grant")]
    grant: Vec<String>,
}

fn default_address() -> String {
    DEFAULT_ADDRESS.to_string()
}

fn default_grant() -> Vec<String> {
    vec![DEFAULT_GRANT.to_string()]
}

/// Parse and validate a charter source. Every failure is a taught string
/// naming the line at fault; nothing here reads or writes state.
pub fn parse(text: &str) -> Result<Charter, String> {
    let raw: RawCharter = toml::from_str(text).map_err(|e| e.to_string())?;
    if !node_store::valid_node_name(&raw.mesh) {
        return Err(format!(
            "`{}` is not a valid mesh name — lowercase letters, digits, and `-`, starting with a letter or digit",
            raw.mesh
        ));
    }
    let mut nodes = BTreeMap::new();
    let mut owner: BTreeMap<String, String> = BTreeMap::new();
    for (name, line) in &raw.nodes {
        if !node_store::valid_node_name(name) {
            return Err(format!(
                "`{name}` is not a valid node name — lowercase letters, digits, and `-`, starting with a letter or digit"
            ));
        }
        let key = key_hex(&line.key).map_err(|e| format!("node `{name}`: {e}"))?;
        if let Some(first) = owner.insert(key.clone(), name.clone()) {
            return Err(format!(
                "`{name}` and `{first}` are the same identity key — one key is one node, whatever a charter calls it"
            ));
        }
        let binding: seal::Binding = serde_json::from_str(&line.age)
            .map_err(|e| format!("node `{name}`: `age` is not a signed binding ({e})"))?;
        if !seal::verify_binding(&binding, Some(&key)) {
            return Err(format!(
                "node `{name}`: its age binding does not verify under the key on its own line"
            ));
        }
        validate_address(name, &line.address)?;
        for cap in &line.grant {
            if !node_store::valid_capability(cap) {
                return Err(format!(
                    "node `{name}`: grant `{cap}` is not one of {}",
                    node_store::NODE_CAPABILITIES.join(", ")
                ));
            }
        }
        nodes.insert(
            name.clone(),
            NodeLine {
                key,
                age: binding,
                address: line.address.clone(),
                grant: line.grant.clone(),
            },
        );
    }
    for relay in &raw.relays {
        if !node_store::valid_node_name(relay) {
            return Err(format!("relay `{relay}` is not a valid node name"));
        }
        if !nodes.contains_key(relay) {
            return Err(format!(
                "relay `{relay}` is not a node on this charter — a relay is a node of the mesh, with a key and an address of its own"
            ));
        }
    }
    for (node, status) in &raw.status {
        if !node_store::valid_node_name(node) {
            return Err(format!("`[status]` names `{node}`, which is not a valid node name"));
        }
        if !nodes.contains_key(node) {
            return Err(format!(
                "`[status]` names `{node}`, which is not a node on this charter — a status for a line that is not there is a silent no-op, which is exactly the drift `sign` exists to catch"
            ));
        }
        if !STATUS_VALUES.contains(&status.as_str()) {
            return Err(format!(
                "`[status]` gives `{node}` the status `{status}` — one of {} or absent",
                STATUS_VALUES.join(", ")
            ));
        }
    }
    for (mesh, node) in &raw.gates {
        if !node_store::valid_node_name(mesh) {
            return Err(format!("`[gates]` names `{mesh}`, which is not a valid mesh name"));
        }
        if !nodes.contains_key(node) {
            return Err(format!(
                "`[gates]` sends mesh `{mesh}` through `{node}`, which is not a node on this charter"
            ));
        }
    }
    Ok(Charter {
        mesh: raw.mesh,
        version: raw.version,
        relays: raw.relays,
        nodes,
    })
}

/// A node's `address` is one of the three transports the design names, or it
/// is nothing: `poll`, `ssh://[user@]host[:port]` (through the one parser
/// that grammar already has), or `https://host`.
fn validate_address(node: &str, address: &str) -> Result<(), String> {
    if address == DEFAULT_ADDRESS {
        return Ok(());
    }
    if address.starts_with("ssh://") {
        return crate::tunnel::parse_via(address)
            .map(|_| ())
            .map_err(|e| format!("node `{node}`: address: {e}"));
    }
    if address.starts_with("https://") && node_store::url_host(address).is_some() {
        return Ok(());
    }
    Err(format!(
        "node `{node}`: address `{address}` is not `poll`, `ssh://…` or `https://…`"
    ))
}

/// The bare lowercase hex of an `ed25519:<hex>` field, refusing anything else
/// — the one shape check a charter line's `key` and config's `operator` line
/// share.
pub fn key_hex(field: &str) -> Result<String, String> {
    let hex = field
        .strip_prefix(KEY_PREFIX)
        .ok_or_else(|| format!("`{field}` is not an `ed25519:` key — the prefix is required"))?;
    if hex.len() != 64 || !hex.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')) {
        return Err(format!("`{field}` is not 32 bytes of lowercase hex"));
    }
    Ok(hex.to_string())
}

/// The durable fingerprint of a bare-hex key: `SHA256:<hex>` over its raw 32
/// bytes — the value an operator compares out of band. Total for any input
/// (`""` for hex that does not decode) because it is used on the reporting
/// path, where a fingerprint must never itself be the failure.
pub fn fingerprint_of_key(hex: &str) -> String {
    match hex_decode(hex) {
        Some(raw) => identity::node_fingerprint(&raw),
        None => String::new(),
    }
}

// ── A node's own line ───────────────────────────────────────────────────

/// `operator = "ed25519:<hex>"` — the line a machine pastes into config, and
/// the line `init`/`reroot` print.
pub fn operator_line(hex: &str) -> String {
    format!("operator = \"{KEY_PREFIX}{hex}\"")
}

/// This machine's node line, ready to paste under a charter's `[nodes]`:
///
/// ```text
/// thinkchiyo = { key = "ed25519:<hex>", age = '<binding json>' }   # SHA256:<fp>
/// ```
///
/// The `age` value is the binding's own JSON inside a TOML literal string
/// (single quotes): no field of a binding can contain a single quote, so the
/// line is pasteable and hand-editable without escaping. It mints this node's
/// age key and binding on first call (`seal::publish_binding`, idempotent
/// afterwards), because the line's whole purpose is to publish them. Nothing
/// private is printed.
pub fn node_line() -> Result<String, String> {
    let (kp, _) = identity::load_or_mint().map_err(|e| e.to_string())?;
    let info = kp.info();
    let binding = seal::publish_binding()?;
    let json = serde_json::to_string(&binding).map_err(|e| e.to_string())?;
    if json.contains('\'') {
        return Err("this node's binding cannot be written as a TOML literal string".to_string());
    }
    Ok(format!(
        "{} = {{ key = \"{KEY_PREFIX}{}\", age = '{}' }}   # {}",
        display::local_node_name(),
        info.pubkey_hex,
        json,
        info.node_fingerprint
    ))
}

// ── The operator key ────────────────────────────────────────────────────

/// Load this mesh's operator key, if this machine holds it. The key is stored
/// exactly as the identity key is — a raw 32-byte seed, `0600`, in a `0700`
/// directory — and is never the identity key, even on the machine that is both
/// the operator and a node.
pub fn load_operator_key(mesh: &str) -> Result<identity::Keypair, String> {
    let path = operator_key_path(mesh);
    if !path.exists() {
        return Err(format!(
            "no operator key for mesh `{mesh}` on this machine ({}) — `aoide mesh charter init {mesh}` mints one, and only the operator's own machine holds it",
            path.display()
        ));
    }
    identity::load_or_mint_seed_file(&path)
        .map(|(kp, _)| kp)
        .map_err(|e| e.to_string())
}

/// Mint this mesh's operator key if absent, and leave it alone if present: a
/// mesh root is never silently replaced (that is [`replace_operator_key`],
/// and it is `reroot`).
pub fn mint_operator_key(mesh: &str) -> Result<identity::Keypair, String> {
    identity::load_or_mint_seed_file(&operator_key_path(mesh))
        .map(|(kp, _)| kp)
        .map_err(|e| e.to_string())
}

/// **Replace this mesh's operator key** — `reroot`. The old key is deleted,
/// never archived: trust is replaced, not added, and keeping a lost or
/// compromised root on disk is a liability with no reader. Every other machine
/// still trusts the old key until its own operator line changes; that cost is
/// the design's ("one touch per machine") and is why reroot prints the new
/// line.
pub fn replace_operator_key(mesh: &str) -> Result<identity::Keypair, String> {
    let path = operator_key_path(mesh);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    mint_operator_key(mesh)
}

// ── Trust at rest ───────────────────────────────────────────────────────

/// One node's record for one mesh: which operator key it trusts, how far it
/// has seen that key, and which nodes the last applied version re-keyed.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trust {
    /// The operator key trusted for this mesh, bare lowercase hex.
    pub operator: String,
    /// The highest version applied, **per operator key** (bare hex) — so a
    /// re-root restarts the mark under the new key while the old key's mark
    /// stays where it was.
    #[serde(default)]
    pub versions: BTreeMap<String, u64>,
    /// The nodes the last applied version changed the identity key of. A
    /// durable mark, not just a message: a charter applied by a poll can land
    /// unattended, and "a new key on an old name is what a stolen operator key
    /// would sign" is something an operator has to be able to see afterwards.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rekeyed: Vec<Rekeyed>,
}

/// One node whose identity key a charter version changed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rekeyed {
    pub node: String,
    /// The key it had, as a durable fingerprint.
    pub from: String,
    /// The key it has now, as a durable fingerprint.
    pub to: String,
    pub version: u64,
    pub at: String,
}

/// Read this node's record for a mesh. A file that exists but does not parse
/// is an ERROR, never "no record": silently resetting the trusted key or the
/// high-water mark is exactly the weakening this record exists to prevent.
pub fn load_trust(mesh: &str) -> Result<Option<Trust>, String> {
    let path = trust_path(mesh);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|e| format!("{}: {e}", path.display()))
}

fn write_trust(mesh: &str, trust: &Trust) -> Result<(), String> {
    let text = serde_json::to_string_pretty(trust).map_err(|e| e.to_string())?;
    fs::atomic_write(&trust_path(mesh), &(text + "\n")).map_err(|e| e.to_string())
}

/// Which operator key this node trusts for `mesh`, cross-checking the two
/// places it can be written down. The config line and the state record must
/// AGREE once both exist: a disagreement means two sources name the mesh's
/// root, and nothing about the mesh can be decided until a human resolves it.
///
/// `pub` because the accept path is not the only reader — a later door reads
/// it to know whose signature a mesh's charters carry.
pub fn trusted_operator(mesh: &str) -> Result<String, Refusal> {
    let declared = config_operator(mesh).map_err(|e| Refusal::new(CONFIG_UNREADABLE, e))?;
    let from_state = load_trust(mesh)
        .map_err(|e| Refusal::new(LOCAL_IO, e))?
        .map(|trust| trust.operator)
        .filter(|key| !key.is_empty());
    match (declared, from_state) {
        (Some(config_key), Some(state)) => {
            if config_key != state {
                return Err(Refusal::new(
                    OPERATOR_MISMATCH,
                    format!(
                        "config.toml declares mesh.{mesh}.operator = `{config_key}`, this node's record holds `{state}` — resolve which key signs `{mesh}` before any charter for it is honoured"
                    ),
                ));
            }
            Ok(state)
        }
        (Some(config_key), None) => Ok(config_key),
        (None, Some(state)) => Ok(state),
        (None, None) => Err(Refusal::new(
            UNKNOWN_OPERATOR,
            format!(
                "no operator key is trusted for mesh `{mesh}` — put {} in config.toml's `[mesh.{mesh}]`, rendered by Nix or written by hand",
                operator_line("<hex>")
            ),
        )),
    }
}

/// The operator key `config.toml` declares for `mesh`, as bare hex, or `None`
/// when this machine's config names none. The ONE reader of that line — so
/// "does this machine declare it at all?" and "which key does it trust?" can
/// never be answered from two parses that disagree.
pub fn config_operator(mesh: &str) -> Result<Option<String>, String> {
    let loaded = config::load().map_err(|e| e.to_string())?;
    match loaded.config.mesh.get(mesh).and_then(|m| m.operator.clone()) {
        Some(line) => key_hex(&line).map(Some),
        None => Ok(None),
    }
}

// ── Accept ──────────────────────────────────────────────────────────────

/// A charter refusal: the taught word an operator acts on, and the sentence
/// that says what to do about it.
#[derive(Debug, Clone, PartialEq)]
pub struct Refusal {
    pub reason: String,
    pub detail: String,
}

impl Refusal {
    pub fn new(reason: &str, detail: impl Into<String>) -> Self {
        Refusal {
            reason: reason.to_string(),
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.reason, self.detail)
    }
}

/// An accepted charter, for the caller to report and audit.
#[derive(Debug, Clone, PartialEq)]
pub struct Accepted {
    pub charter: Charter,
    /// The operator key (bare hex) the signature verified under.
    pub operator: String,
    pub rekeyed: Vec<Rekeyed>,
}

/// **Accept a signed charter** — the design's steps, in the order that makes
/// each taught refusal the true one, and nothing stored before all of them
/// pass.
///
/// 1. The detached signature's own framing, then the digest it carries against
///    `sha256` of the received bytes. This needs no parse and no trust, and it
///    is checked FIRST so a touched file answers [`CHARTER_TAMPERED`] rather
///    than being misreported as a key that does not verify.
/// 2. The parse (`parse`), which is where every line of the document is
///    checked. A file that does not parse cannot be the artifact that was
///    signed, so it is [`CHARTER_TAMPERED`] too.
/// 3. The operator key this node trusts for the mesh the file declares —
///    config line against state record ([`trusted_operator`]), so
///    [`OPERATOR_MISMATCH`] and [`UNKNOWN_OPERATOR`] land here.
/// 4. The signature: `operator Ed25519 over frame("aoide/charter", [mesh,
///    version, sha256(file bytes)])`. The mesh and version are the parsed
///    ones — they are the only source there is, and the digest is what binds
///    the artifact — so a signature that does not verify under the trusted key
///    is [`UNKNOWN_OPERATOR`]: this node does not trust the key that signed
///    this.
/// 5. The version, against the high-water mark for `(mesh, operator key)`
///    ([`STALE_CHARTER`]), so a replayed older charter never returns. A
///    re-root restarts the mark by construction: the map is keyed by the
///    operator key.
///
/// Only then is anything written: the charter and its `.sig` into
/// `state/mesh/<mesh>/`, and the record (trusted key, new mark, and the nodes
/// this version re-keyed) into `trust.json`. A refusal leaves no state at all.
pub fn accept(file_bytes: &[u8], sig_bytes: &[u8]) -> Result<Accepted, Refusal> {
    let (sig, digest) =
        seal::parse_charter_sig_frame(sig_bytes).map_err(|e| Refusal::new(CHARTER_TAMPERED, e))?;
    let file_digest = seal::sha256(file_bytes);
    if digest != file_digest {
        return Err(Refusal::new(
            CHARTER_TAMPERED,
            "the charter file's bytes are not the ones this signature was made over — sign it again (`sign` is the last write to a charter file)",
        ));
    }
    let text = std::str::from_utf8(file_bytes).map_err(|e| {
        Refusal::new(CHARTER_TAMPERED, format!("the charter file is not UTF-8: {e}"))
    })?;
    let charter = parse(text).map_err(|e| Refusal::new(CHARTER_TAMPERED, e))?;

    // Every remaining step is a read-modify-write of this mesh's state (the
    // mark, the charter, its `.sig`), so the whole of it runs under the mesh's
    // own lock: two applies that each read the old mark would let the older
    // version win the write (see [`charter_lock_path`]).
    let locked = fs::lock_path(charter_lock_path(&charter.mesh), || {
        accept_locked(&charter, text, &sig, &digest)
    });
    locked.map_err(|e| Refusal::new(LOCAL_IO, e))?
}

/// [`accept`]'s steps 3-5 and its three writes, under the mesh's lock.
///
/// **The write order is `.sig`, then the charter, then the mark**, and nothing
/// else claims the charter is in force until the mark lands:
///
/// 1. the `.sig` for the incoming version;
/// 2. `charter.toml`, the bytes it is a signature OF;
/// 3. `trust.json`, whose `versions` entry is the claim of application.
///
/// A failure between them therefore leaves one of two states, and neither is a
/// silent success: a `.sig` beside the PREVIOUS charter (the pair does not
/// verify, and the next accept of the same version writes both again), or a new
/// charter with a valid `.sig` and the mark NOT advanced (the next delivery of
/// the same version re-applies it — the same bytes, the same in-force charter,
/// no new key material). That is why the mark is last and why the word for a
/// failure here is [`LOCAL_IO`], not one of the four that answer for the
/// charter: the charter was fine, this node could not finish writing it.
fn accept_locked(
    charter: &Charter,
    text: &str,
    sig: &[u8],
    digest: &[u8],
) -> Result<Accepted, Refusal> {
    let operator = trusted_operator(&charter.mesh)?;
    let input = seal::charter_sig_input(&charter.mesh, charter.version, digest);
    if !wire_auth::verify_signature_hex(&operator, &input, &hex_encode(sig)) {
        return Err(Refusal::new(
            UNKNOWN_OPERATOR,
            format!(
                "this charter is not signed by the key this node trusts for mesh `{}` ({})",
                charter.mesh,
                fingerprint_of_key(&operator)
            ),
        ));
    }

    let mut trust = load_trust(&charter.mesh)
        .map_err(|e| Refusal::new(LOCAL_IO, e))?
        .unwrap_or_default();
    let mark = trust.versions.get(&operator).copied().unwrap_or(0);
    if charter.version <= mark {
        return Err(Refusal::new(
            STALE_CHARTER,
            format!(
                "charter `{}` version {} is not above the highest this node has applied ({mark})",
                charter.mesh, charter.version
            ),
        ));
    }

    let now = now_iso_utc();
    let rekeyed = detect_rekeyed(in_force_charter(&charter.mesh).as_ref(), charter, &now);
    let spath = in_force_sig_path(&charter.mesh);
    fs::atomic_write_bytes(&spath, sig)
        .map_err(|e| Refusal::new(LOCAL_IO, format!("{}: {e}", spath.display())))?;
    let path = in_force_path(&charter.mesh);
    fs::atomic_write(&path, text)
        .map_err(|e| Refusal::new(LOCAL_IO, format!("{}: {e}", path.display())))?;

    trust.operator = operator.clone();
    trust.versions.insert(operator.clone(), charter.version);
    trust.rekeyed = rekeyed.clone();
    write_trust(&charter.mesh, &trust).map_err(|e| Refusal::new(LOCAL_IO, e))?;

    Ok(Accepted {
        charter: charter.clone(),
        operator,
        rekeyed,
    })
}

/// The charter in force at this node, if there is one and it parses. A file
/// that does not parse reads as none: its only reader besides the report is
/// the re-key comparison, and refusing every later accept forever because a
/// human corrupted a state file would be worse than the lost comparison.
pub fn in_force_charter(mesh: &str) -> Option<Charter> {
    let text = std::fs::read_to_string(in_force_path(mesh)).ok()?;
    parse(&text).ok()
}

/// The nodes `next` changes the identity key of, against the charter in force.
/// A node that is new, or gone, is not a re-key.
fn detect_rekeyed(previous: Option<&Charter>, next: &Charter, at: &str) -> Vec<Rekeyed> {
    let Some(previous) = previous else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (name, line) in &next.nodes {
        let Some(old) = previous.nodes.get(name) else { continue };
        if old.key != line.key {
            out.push(Rekeyed {
                node: name.clone(),
                from: fingerprint_of_key(&old.key),
                to: fingerprint_of_key(&line.key),
                version: next.version,
                at: at.to_string(),
            });
        }
    }
    out
}

// ── The operator's side: init, sign, reroot ─────────────────────────────

/// What `init` did, for the command to print.
#[derive(Debug, Clone, PartialEq)]
pub struct Init {
    pub mesh: String,
    /// The operator key's public hex, bare.
    pub operator: String,
    pub fingerprint: String,
    pub path: PathBuf,
    /// The operator key's file path — a path, never the key.
    pub key_path: PathBuf,
    pub trust_path: PathBuf,
}

/// What `sign` did, for the command to print.
#[derive(Debug, Clone, PartialEq)]
pub struct Signed {
    pub mesh: String,
    pub version: u64,
    pub operator: String,
    pub fingerprint: String,
    pub path: PathBuf,
    pub sig_path: PathBuf,
    /// The nodes this version re-keyed, if it changed any.
    pub rekeyed: Vec<Rekeyed>,
    /// The nodes the signed pair was spooled to (every node on the charter
    /// but this one).
    pub spooled: Vec<String>,
}

/// The empty source `init` writes: a mesh name, version zero, no relays, no
/// nodes. `sign` writes version 1 into it.
pub fn empty_source(mesh: &str) -> String {
    format!("mesh    = \"{mesh}\"\nversion = 0\nrelays  = []\n\n[nodes]\n")
}

/// **Root a mesh** on the operator's machine: mint (or keep) the operator key,
/// write the empty source, and record the key as this mesh's root in state so
/// this machine trusts its own charters.
///
/// It never overwrites an existing source: an operator's edited file is not
/// something a command may eat, and re-running `init` on a rooted mesh is
/// answered by the file that is already there.
pub fn init(mesh: &str) -> Result<Init, String> {
    if !node_store::valid_node_name(mesh) {
        return Err(format!(
            "`{mesh}` is not a valid mesh name — lowercase letters, digits, and `-`, starting with a letter or digit"
        ));
    }
    let path = source_path(mesh);
    if path.exists() {
        return Err(format!(
            "{} already exists — `charter init` never overwrites a source. Edit it, then `aoide mesh charter sign {mesh}`",
            path.display()
        ));
    }
    // A machine that already trusts an operator for this mesh is a NODE of it,
    // not its root. `init` would mint a fresh key and repoint this machine's
    // trust at one no other node on the mesh knows — a mistyped command on a
    // node is enough to break it, with no trace of where the real root was. So
    // it refuses, and it refuses BEFORE minting anything: what a machine
    // already trusts is read, never replaced.
    let key_on_disk = if operator_key_path(mesh).exists() {
        Some(load_operator_key(mesh)?.info().pubkey_hex)
    } else {
        None
    };
    match trusted_operator(mesh) {
        Ok(key) if Some(&key) == key_on_disk.as_ref() => {}
        Ok(key) => {
            return Err(format!(
                "this machine already trusts operator {} for mesh `{mesh}` — it is a NODE of that mesh, and {} would mint a different key and repoint that trust. Leave the `operator` line alone: a charter reaches this machine through its own trust-entry step. To replace a lost or compromised root, run `aoide mesh charter reroot {mesh}` ON THE OPERATOR'S MACHINE",
                fingerprint_of_key(&key),
                path.display()
            ))
        }
        Err(refusal) if refusal.reason == UNKNOWN_OPERATOR => {}
        Err(refusal) => {
            return Err(format!(
                "this machine's own trust for mesh `{mesh}` cannot be read, so `charter init` will not mint anything: {refusal}"
            ))
        }
    }
    let keypair = mint_operator_key(mesh)?;
    let operator = keypair.info().pubkey_hex;
    let fingerprint = keypair.info().node_fingerprint;
    fs::atomic_write(&path, &empty_source(mesh)).map_err(|e| format!("{}: {e}", path.display()))?;

    let mut trust = load_trust(mesh)?.unwrap_or_default();
    trust.operator = operator.clone();
    write_trust(mesh, &trust)?;

    Ok(Init {
        mesh: mesh.to_string(),
        operator,
        fingerprint,
        path,
        key_path: operator_key_path(mesh),
        trust_path: trust_path(mesh),
    })
}

/// **Sign the current source.** Validates it, writes the next `version` into
/// the file (in place, comments intact), hashes those exact bytes, signs the
/// digest with the mesh's operator key, writes `<file>.sig` beside it, applies
/// the result HERE (the operator's machine is a node of its own mesh), and
/// spools the signed pair to every other node on the charter.
///
/// The order is the point: `sign` is the last write to a charter file, so the
/// file is finished before it is hashed, and the digest inside the `.sig` is
/// the digest of exactly what is on disk.
pub fn sign(mesh: &str, source: Option<&Path>) -> Result<Signed, String> {
    let path = source.map(Path::to_path_buf).unwrap_or_else(|| source_path(mesh));
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let parsed = parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    if parsed.mesh != mesh {
        return Err(format!(
            "{} declares mesh `{}`, not `{mesh}` — sign it under its own name",
            path.display(),
            parsed.mesh
        ));
    }
    let keypair = load_operator_key(mesh)?;
    let operator = keypair.info().pubkey_hex;
    let fingerprint = keypair.info().node_fingerprint;

    let version = parsed.version.checked_add(1).ok_or_else(|| {
        format!(
            "{} declares version {} — the next one does not fit in 64 bits, and a version that wraps is a signed file that can never be ordered against another. Nothing was written",
            path.display(),
            parsed.version
        )
    })?;
    let bumped = bump_version(&text, version)?;
    fs::atomic_write(&path, &bumped).map_err(|e| format!("{}: {e}", path.display()))?;

    let digest = seal::sha256(bumped.as_bytes());
    let sig_hex = wire_auth::sign_hex(&keypair, &seal::charter_sig_input(mesh, version, &digest));
    let sig_bytes = hex_decode(&sig_hex).ok_or_else(|| "the operator signature is not hex".to_string())?;
    let sig_frame = seal::charter_sig_frame(&sig_bytes, &digest);
    let spath = sig_path_for(&path);
    fs::atomic_write_bytes(&spath, &sig_frame).map_err(|e| format!("{}: {e}", spath.display()))?;

    // Apply locally through the SAME accept path every other node runs, so a
    // charter this node would refuse is never one it has silently blessed.
    let accepted = accept(bumped.as_bytes(), &sig_frame).map_err(|e| {
        format!(
            "the signed charter was written to {}, but this machine refused it: {e}",
            path.display()
        )
    })?;
    let spooled = spool(&accepted.charter, bumped.as_bytes(), &sig_frame)?;

    Ok(Signed {
        mesh: mesh.to_string(),
        version,
        operator,
        fingerprint,
        path,
        sig_path: spath,
        rekeyed: accepted.rekeyed,
        spooled,
    })
}

/// **Re-root a mesh**: mint a new operator key, replace the record of which key
/// this machine trusts (the old key's version marks stay where they were, so
/// nothing about it is inherited), and sign the current source under the new
/// key. Every other machine trusts the new key only when its own trust-entry
/// step says so — that is the one-touch-per-machine cost the User ruled in.
///
/// **It checks this machine's own trust FIRST, and refuses before it mints,
/// bumps or signs anything.** Two shapes, both of which would otherwise fail
/// *after* the source's version had been bumped and its `.sig` written — a
/// machine with a new version on disk and no node told about it:
///
/// - the config line and the state record disagree (or the record cannot be
///   read) — this machine's trust is already inconsistent, and a re-root would
///   bury the disagreement under a third key;
/// - the config DECLARES this mesh's operator key. That line pins the key a
///   re-root is about to replace, so the machine would refuse its own new
///   charter `operator-mismatch` the moment the record moved.
///
/// Both print the same resolution: the new key reaches a machine through that
/// machine's own trust-entry step — its `[mesh.<mesh>] operator` line, or
/// `aoide mesh join <mesh> --operator <key>` on a host whose config is not
/// hand-editable — and **never** by editing `state/mesh/<mesh>/trust.json`,
/// which is where the version mark lives and where a hand edit would silently
/// reset it.
pub fn reroot(mesh: &str) -> Result<Signed, String> {
    if !source_path(mesh).exists() && load_trust(mesh)?.is_none() {
        return Err(format!(
            "mesh `{mesh}` is not rooted on this machine — `aoide mesh charter init {mesh}` mints its operator key first"
        ));
    }
    let declared = config_operator(mesh)?;
    match trusted_operator(mesh) {
        Ok(_) if declared.is_none() => {}
        Ok(_) => {
            return Err(format!(
                "this machine's config declares mesh.{mesh}.operator, which pins the key a re-root is about to replace: the line and this node's record would disagree and every charter for `{mesh}` — including the new one — would be refused `operator-mismatch`. Nothing was minted, bumped or signed.\n  \
                 Take the new key the way every other machine does: its own `[mesh.{mesh}] operator` line, or `aoide mesh join {mesh} --operator <key>` on a host whose config is not hand-editable (that command is the trust-per-mesh slice's, and this build has no other way to record a key). Never edit `state/mesh/{mesh}/trust.json`: it holds the version mark. Or run the re-root on a machine that declares no operator line for `{mesh}`"
            ));
        }
        Err(refusal) => {
            return Err(format!(
                "this machine's own trust for mesh `{mesh}` does not hold together, so a re-root will not bury it under a third key: {refusal}\n  \
                 Bring the config line and this node's record back into agreement first — the same trust-entry step that put the key there: `aoide mesh join {mesh} --operator <key>` (the trust-per-mesh slice's), or the `[mesh.{mesh}] operator` line itself — then re-root. Never edit `state/mesh/{mesh}/trust.json`: it holds the version mark"
            ))
        }
    }
    let keypair = replace_operator_key(mesh)?;
    let mut trust = load_trust(mesh)?.unwrap_or_default();
    trust.operator = keypair.info().pubkey_hex;
    write_trust(mesh, &trust)?;
    sign(mesh, None)
}

/// Write `version` into the source's own text, in place: comments, spacing and
/// key order all survive, because the bytes this returns are the bytes that get
/// signed. The value's **decorations are carried over** — including a trailing
/// comment on the `version` line itself, which a plain assignment to
/// `doc["version"]` would drop (the operator's note about what wrote the line
/// is not `sign`'s to delete).
fn bump_version(text: &str, version: u64) -> Result<String, String> {
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| e.to_string())?;
    let value = i64::try_from(version).map_err(|_| {
        format!(
            "version {version} is past what a charter file can hold — a TOML integer is 64 bits signed, so this mesh's version cannot advance any further. Nothing was written"
        )
    })?;
    match doc.get_mut("version").and_then(|item| item.as_value_mut()) {
        Some(existing) => {
            let mut replacement = toml_edit::Value::from(value);
            *replacement.decor_mut() = existing.decor().clone();
            *existing = replacement;
        }
        None => doc["version"] = toml_edit::value(value),
    }
    Ok(doc.to_string())
}

/// Spool the signed pair to every node on the charter but this one. The
/// binding each copy is sealed to comes from that node's OWN line on the
/// charter — which is the whole reason a charter can be delivered to a machine
/// this one has never exchanged a binding with — and each copy is a `charter`
/// letter like any other letter: ordinary outbox, ordinary drain, ordinary
/// poll.
pub fn spool(charter: &Charter, file_bytes: &[u8], sig_bytes: &[u8]) -> Result<Vec<String>, String> {
    let local = display::local_node_name();
    let now = now_iso_utc();
    let mut spooled = Vec::new();
    for (name, line) in &charter.nodes {
        if *name == local {
            continue;
        }
        let text = format!("charter {} v{}", charter.mesh, charter.version);
        let envelope = mail::mint_charter_letter(name, &text, &charter.mesh)?;
        let container = seal::seal_charter(
            file_bytes,
            sig_bytes,
            &line.age,
            &charter.mesh,
            charter.version,
            name,
            &now,
        )?;
        outbox::write_entry(name, &outbox::OutboxEntry::sealed(envelope, container))?;
        spooled.push(name.clone());
    }
    Ok(spooled)
}

// ── Small local helpers ─────────────────────────────────────────────────

/// Lowercase hex, no separator — this crate's established per-module copy over
/// a shared `pub` utility (`wire_auth`'s and `identity`'s own note).
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

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_test_support::{unique_tmp, EnvSaver};

    /// A fresh scratch directory standing in for one machine.
    fn machine_dir(tag: &str) -> PathBuf {
        let dir = unique_tmp(&format!("charter-{tag}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Point every root at `dir` and name this "machine" `name`: the three
    /// vars every read below resolves through, plus the node-name override
    /// `display::local_node_name` reads.
    fn machine(dir: &Path, name: &str) {
        std::env::set_var("AOIDE_ROOT", dir);
        std::env::set_var("AOIDE_STATE_DIR", dir);
        std::env::remove_var("AOIDE_CONFIG");
        std::env::set_var("AOIDE_A2A_NODE_NAME", name);
    }

    /// The vars a machine switch touches, captured for the whole test.
    fn isolate() -> (std::sync::MutexGuard<'static, ()>, EnvSaver) {
        let guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saver = EnvSaver::capture(&[
            "AOIDE_ROOT",
            "AOIDE_STATE_DIR",
            "AOIDE_CONFIG",
            "AOIDE_A2A_NODE_NAME",
        ]);
        (guard, saver)
    }

    /// `charters/<mesh>.toml` under an explicit root — so a test can read
    /// machine A's artifact while the env points at machine B.
    fn source_under(root: &Path, mesh: &str) -> PathBuf {
        root.join("charters").join(format!("{mesh}.toml"))
    }

    fn sig_under(root: &Path, mesh: &str) -> PathBuf {
        sig_path_for(&source_under(root, mesh))
    }

    /// Write a machine's `config.toml` operator line — the one touch by which
    /// a machine trusts a mesh's operator key.
    fn trust_operator_line(root: &Path, mesh: &str, key_hex: &str) {
        let line = format!("[mesh.{mesh}]\n{}\n", operator_line(key_hex));
        fs::atomic_write(&root.join("config.toml"), &line).unwrap();
    }

    fn read_pair(root: &Path, mesh: &str) -> (Vec<u8>, Vec<u8>) {
        (
            std::fs::read(source_under(root, mesh)).unwrap(),
            std::fs::read(sig_under(root, mesh)).unwrap(),
        )
    }

    /// The node line is the operator's paste target, so it must survive being
    /// pasted: a parse of a charter built only from it, with the binding
    /// verified under the key on its own line — and a parse that REFUSES the
    /// same line once the key is flopped, which is the check that makes the
    /// line worth having.
    #[test]
    fn the_node_line_round_trips_through_a_charter() {
        let (_guard, _saver) = isolate();
        let op = machine_dir("node-line");
        machine(&op, "opbox");

        let line = node_line().unwrap();
        assert!(
            line.starts_with("opbox = { key = \"ed25519:"),
            "the line names this machine's node name: {line}"
        );
        assert!(line.contains("# SHA256:"), "and carries the durable fingerprint: {line}");

        let src = format!("mesh = \"home\"\nversion = 0\nrelays = []\n\n[nodes]\n{line}\n");
        let charter = parse(&src).unwrap();
        let hex = charter.node_key("opbox").expect("the line registers the node");
        assert_eq!(hex.len(), 64);
        assert_eq!(charter.nodes["opbox"].address, DEFAULT_ADDRESS, "no address means poll");
        assert_eq!(charter.nodes["opbox"].grant, vec![DEFAULT_GRANT.to_string()], "and no grant means message");
        assert_eq!(
            identity::node_fingerprint(&hex_decode(&hex).unwrap()),
            identity::node_fingerprint(&hex_decode(&hex).unwrap()),
            "the fingerprint is over the raw key, stably"
        );

        // A key that is not the one its binding names is refused, by the
        // parse, before anything else looks at the line.
        let (head, tail) = line.split_once("key = \"ed25519:").unwrap();
        let flopped = match tail.chars().next().unwrap() {
            '0' => format!("1{}", &tail[1..]),
            _ => format!("0{}", &tail[1..]),
        };
        let bad = format!("mesh = \"home\"\nversion = 0\n\n[nodes]\n{head}key = \"ed25519:{flopped}");
        let err = parse(&bad).unwrap_err();
        assert!(err.contains("does not verify under the key on its own line"), "{err}");

        let _ = std::fs::remove_dir_all(&op);
    }

    #[test]
    fn signature_and_freshness() {
        let (_guard, _saver) = isolate();
        let op = machine_dir("fresh-op");
        let peer = machine_dir("fresh-peer");

        machine(&op, "opbox");
        let init = init("home").unwrap();
        let operator = init.operator.clone();
        let op_line = node_line().unwrap();
        machine(&peer, "peerbox");
        let peer_line = node_line().unwrap();

        machine(&op, "opbox");
        let src = format!("mesh = \"home\"\nversion = 0\nrelays = []\n\n[nodes]\n{op_line}\n{peer_line}\n");
        fs::atomic_write(&source_path("home"), &src).unwrap();
        let signed = sign("home", None).unwrap();
        assert_eq!(signed.version, 1);
        assert_eq!(signed.operator, operator);
        assert_eq!(signed.spooled, vec!["peerbox".to_string()], "spooled to the other node only");
        assert_eq!(in_force_charter("home").unwrap().version, 1, "and applied here");

        let (v1, v1_sig) = read_pair(&op, "home");

        // An equal (or older) version is refused.
        let refusal = accept(&v1, &v1_sig).unwrap_err();
        assert_eq!(refusal.reason, STALE_CHARTER, "{}", refusal.detail);

        // A flipped byte, as a relay could deliver it.
        let mut flipped = v1.clone();
        flipped[10] ^= 0x01;
        assert_eq!(accept(&flipped, &v1_sig).unwrap_err().reason, CHARTER_TAMPERED);

        // A trailing newline is the SAME defect, and specifically not a
        // bad-signature answer: the digest is checked before the signature.
        let mut touched = v1.clone();
        touched.push(b'\n');
        let refusal = accept(&touched, &v1_sig).unwrap_err();
        assert_eq!(refusal.reason, CHARTER_TAMPERED, "{}", refusal.detail);
        assert!(refusal.detail.contains("sign it again"), "and says what to do: {}", refusal.detail);

        // A signature by a key this node does not trust for the mesh.
        let stray = identity::mint_ephemeral().unwrap();
        let digest = seal::sha256(&v1);
        let stray_sig = wire_auth::sign_hex(&stray, &seal::charter_sig_input("home", 1, &digest));
        let stray_frame = seal::charter_sig_frame(&hex_decode(&stray_sig).unwrap(), &digest);
        let refusal = accept(&v1, &stray_frame).unwrap_err();
        assert_eq!(refusal.reason, UNKNOWN_OPERATOR, "{}", refusal.detail);

        // A config operator line that disagrees with the state record refuses
        // every charter for that mesh.
        trust_operator_line(&op, "home", &stray.info().pubkey_hex);
        let refusal = accept(&v1, &v1_sig).unwrap_err();
        assert_eq!(refusal.reason, OPERATOR_MISMATCH, "{}", refusal.detail);
        std::fs::remove_file(op.join("config.toml")).unwrap();

        let _ = std::fs::remove_dir_all(&op);
        let _ = std::fs::remove_dir_all(&peer);
    }

    /// **F2.** A machine that already trusts an operator for a mesh is a NODE
    /// of it: `init` there would mint a key no other node knows and repoint
    /// this machine's trust at it. It refuses, and it mints nothing.
    #[test]
    fn init_refuses_a_machine_that_already_trusts_an_operator() {
        let (_guard, _saver) = isolate();
        let op = machine_dir("init-op");
        let node = machine_dir("init-node");
        let told = machine_dir("init-told");

        machine(&op, "opbox");
        let rooted = init("home").unwrap();
        let op_line = node_line().unwrap();
        machine(&node, "nodebox");
        let node_line_text = node_line().unwrap();
        machine(&op, "opbox");
        let src = format!("mesh = \"home\"\nversion = 0\nrelays = []\n\n[nodes]\n{op_line}\n{node_line_text}\n");
        fs::atomic_write(&source_path("home"), &src).unwrap();
        sign("home", None).unwrap();
        let (v1, v1_sig) = read_pair(&op, "home");

        // A node that has ACCEPTED a charter: a trust record, no local source.
        machine(&node, "nodebox");
        trust_operator_line(&node, "home", &rooted.operator);
        accept(&v1, &v1_sig).unwrap();
        let before = load_trust("home").unwrap().unwrap();
        let err = init("home").unwrap_err();
        assert!(err.contains("already trusts operator"), "{err}");
        assert!(err.contains("NODE of that mesh"), "and names what it is: {err}");
        assert!(err.contains("mesh charter reroot"), "and the fix: {err}");
        assert_eq!(load_trust("home").unwrap().unwrap(), before, "its trust is untouched");
        assert!(!operator_key_path("home").exists(), "and no operator key was minted here");

        // A node that has only been TOLD the key (a config line, no record):
        // the same refusal, because the machine's trust is already spoken for.
        machine(&told, "toldbox");
        trust_operator_line(&told, "home", &rooted.operator);
        let err = init("home").unwrap_err();
        assert!(err.contains("already trusts operator"), "{err}");
        assert!(!operator_key_path("home").exists(), "still nothing minted");

        // And the machine that DOES hold the key is not refused: a re-init with
        // the source gone re-creates an empty one under the same root.
        machine(&op, "opbox");
        std::fs::remove_file(source_path("home")).unwrap();
        let again = init("home").unwrap();
        assert_eq!(again.operator, rooted.operator, "the same key, never a second one");

        let _ = std::fs::remove_dir_all(&op);
        let _ = std::fs::remove_dir_all(&node);
        let _ = std::fs::remove_dir_all(&told);
    }

    /// **F3.** Two applies that each read the old mark can let the OLDER version
    /// win the write — a revocation silently un-applied, with the mark left
    /// below a version that has already been applied. One lock per mesh makes
    /// the apply exclusive, and this proves `accept` really takes it: with the
    /// lock held by this thread, the accept running in another has not
    /// finished. Without the lock it returns in microseconds, so the wait is
    /// the observable.
    #[test]
    fn accept_waits_for_the_meshs_own_lock() {
        let (_guard, _saver) = isolate();
        let op = machine_dir("lock-op");
        let node = machine_dir("lock-node");

        machine(&op, "opbox");
        let rooted = init("home").unwrap();
        let op_line = node_line().unwrap();
        machine(&node, "nodebox");
        let node_line_text = node_line().unwrap();
        machine(&op, "opbox");
        let src = format!(
            "mesh = \"home\"\nversion = 0\nrelays = []\n\n[nodes]\n{op_line}\n{node_line_text}\n"
        );
        fs::atomic_write(&source_path("home"), &src).unwrap();
        sign("home", None).unwrap();
        let (v1, v1_sig) = read_pair(&op, "home");
        fs::atomic_write(&source_path("home"), &src.replace("version = 0", "version = 1")).unwrap();
        sign("home", None).unwrap();
        let (v2, v2_sig) = read_pair(&op, "home");

        machine(&node, "nodebox");
        trust_operator_line(&node, "home", &rooted.operator);
        accept(&v1, &v1_sig).unwrap();
        assert_eq!(in_force_charter("home").unwrap().version, 1);

        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = done.clone();
        let waiter = crate::fs::lock_path(charter_lock_path("home"), move || {
            let flag = flag.clone();
            let thread = std::thread::spawn(move || {
                let outcome = accept(&v2, &v2_sig);
                flag.store(true, std::sync::atomic::Ordering::SeqCst);
                outcome
            });
            std::thread::sleep(std::time::Duration::from_millis(400));
            assert!(
                !done.load(std::sync::atomic::Ordering::SeqCst),
                "accept must wait for the mesh's charter lock, never race it"
            );
            thread
        })
        .unwrap();

        let outcome = waiter.join().unwrap();
        assert!(outcome.is_ok(), "and then it applies: {outcome:?}");
        assert_eq!(in_force_charter("home").unwrap().version, 2);
        let trust = load_trust("home").unwrap().unwrap();
        assert_eq!(trust.versions.get(&rooted.operator), Some(&2), "the mark followed");

        let _ = std::fs::remove_dir_all(&op);
        let _ = std::fs::remove_dir_all(&node);
    }

    #[test]
    fn a_version_that_rekeys_a_node_is_applied_and_reported() {
        let (_guard, _saver) = isolate();
        let op = machine_dir("rekey-op");
        let peer = machine_dir("rekey-peer");
        let replacement = machine_dir("rekey-replacement");

        machine(&op, "opbox");
        init("home").unwrap();
        let op_line = node_line().unwrap();
        machine(&peer, "peerbox");
        let peer_line = node_line().unwrap();
        // The SAME node name, minted on another machine: an identity key
        // replaced under an old name, which is what a stolen operator key
        // would sign.
        machine(&replacement, "peerbox");
        let rekeyed_line = node_line().unwrap();
        let old_hex = parse(&format!("mesh = \"home\"\nversion = 0\n\n[nodes]\n{peer_line}\n"))
            .unwrap()
            .node_key("peerbox")
            .unwrap();
        let new_hex = parse(&format!("mesh = \"home\"\nversion = 0\n\n[nodes]\n{rekeyed_line}\n"))
            .unwrap()
            .node_key("peerbox")
            .unwrap();
        assert_ne!(old_hex, new_hex);

        machine(&op, "opbox");
        let src = format!("mesh = \"home\"\nversion = 0\nrelays = []\n\n[nodes]\n{op_line}\n{peer_line}\n");
        fs::atomic_write(&source_path("home"), &src).unwrap();
        assert!(sign("home", None).unwrap().rekeyed.is_empty(), "the first version re-keys nothing");

        let src = format!("mesh = \"home\"\nversion = 1\nrelays  = []\n\n[nodes]\n{op_line}\n{rekeyed_line}\n");
        fs::atomic_write(&source_path("home"), &src).unwrap();
        let signed = sign("home", None).unwrap();
        assert_eq!(signed.version, 2);
        assert_eq!(signed.rekeyed.len(), 1, "{:?}", signed.rekeyed);
        assert_eq!(signed.rekeyed[0].node, "peerbox");
        assert_eq!(signed.rekeyed[0].from, fingerprint_of_key(&old_hex));
        assert_eq!(signed.rekeyed[0].to, fingerprint_of_key(&new_hex));

        // And it is a DURABLE mark, not just a message: a charter applied by
        // an unattended poll is still visible afterwards.
        let trust = load_trust("home").unwrap().unwrap();
        assert_eq!(trust.rekeyed, signed.rekeyed);
        assert_eq!(trust.versions.get(&signed.operator), Some(&2));

        let _ = std::fs::remove_dir_all(&op);
        let _ = std::fs::remove_dir_all(&peer);
        let _ = std::fs::remove_dir_all(&replacement);
    }

    #[test]
    fn reroot_replaces_trust_and_restarts_the_high_water_mark() {
        let (_guard, _saver) = isolate();
        let op = machine_dir("reroot-op");
        let peer = machine_dir("reroot-peer");
        let told = machine_dir("reroot-told");

        machine(&op, "opbox");
        let rooted = init("home").unwrap();
        let k1 = rooted.operator.clone();
        let op_line = node_line().unwrap();
        machine(&peer, "peerbox");
        let peer_line = node_line().unwrap();

        machine(&op, "opbox");
        let src = format!("mesh = \"home\"\nversion = 0\nrelays = []\n\n[nodes]\n{op_line}\n{peer_line}\n");
        fs::atomic_write(&source_path("home"), &src).unwrap();
        sign("home", None).unwrap();
        let (k1_v1, k1_v1_sig) = read_pair(&op, "home");

        // The peer trusts K1 by its config line and is working on v1.
        machine(&peer, "peerbox");
        trust_operator_line(&peer, "home", &k1);
        accept(&k1_v1, &k1_v1_sig).unwrap();
        assert_eq!(in_force_charter("home").unwrap().version, 1);

        // Re-root on the operator's machine, which declares no operator line.
        machine(&op, "opbox");
        let rerooted = reroot("home").unwrap();
        let k2 = rerooted.operator.clone();
        assert_ne!(k2, k1, "a new key, not the same one");
        assert_eq!(rerooted.version, 2);
        let trust = load_trust("home").unwrap().unwrap();
        assert_eq!(trust.operator, k2, "this machine now trusts the new key");
        assert_eq!(trust.versions.get(&k2), Some(&2), "and its mark starts under the new key");
        assert_eq!(trust.versions.get(&k1), Some(&1), "while the old key's mark stays where it was");

        // Charters under the OLD key are refused here now.
        assert_eq!(accept(&k1_v1, &k1_v1_sig).unwrap_err().reason, UNKNOWN_OPERATOR);

        // A fresh version under the new key, above the restart.
        let v3 = sign("home", None).unwrap();
        assert_eq!(v3.version, 3);
        let (k2_v3, k2_v3_sig) = read_pair(&op, "home");
        let source_before = std::fs::read_to_string(source_path("home")).unwrap();
        let sig_before = std::fs::read(sig_path_for(&source_path("home"))).unwrap();
        let key_before = load_operator_key("home").unwrap().info().pubkey_hex;

        // **A config line that pins the key is refused BEFORE anything is
        // minted, bumped or signed** — the shape that used to die after the
        // version was written and the `.sig` replaced, with no node told.
        trust_operator_line(&op, "home", &k2);
        let err = reroot("home").unwrap_err();
        assert!(err.contains("pins the key a re-root is about to replace"), "{err}");
        assert!(err.contains("aoide mesh join home --operator"), "names the trust-entry step: {err}");
        assert!(err.contains("Nothing was minted, bumped or signed"), "{err}");
        assert_eq!(std::fs::read_to_string(source_path("home")).unwrap(), source_before, "the source is untouched");
        assert_eq!(std::fs::read(sig_path_for(&source_path("home"))).unwrap(), sig_before, "and its .sig");
        assert_eq!(load_operator_key("home").unwrap().info().pubkey_hex, key_before, "and the operator key");
        std::fs::remove_file(op.join("config.toml")).unwrap();

        // A machine that has not been re-rooted refuses the new key and keeps
        // working on its last charter.
        machine(&peer, "peerbox");
        let refusal = accept(&k2_v3, &k2_v3_sig).unwrap_err();
        assert_eq!(refusal.reason, UNKNOWN_OPERATOR, "{}", refusal.detail);
        assert_eq!(
            in_force_charter("home").unwrap().version,
            1,
            "a machine that has not been re-rooted keeps its last charter"
        );

        // Changing its config line alone is NOT enough: the record still holds
        // K1, so the two disagree and every charter for the mesh stops. The
        // resolution is that machine's own trust-entry step — A3's
        // `aoide mesh join --operator` — and this test does not fake it by
        // reaching into private state, because there is nothing to reach for.
        trust_operator_line(&peer, "home", &k2);
        let refusal = accept(&k2_v3, &k2_v3_sig).unwrap_err();
        assert_eq!(refusal.reason, OPERATOR_MISMATCH, "{}", refusal.detail);

        // A machine TOLD the new key by its config line, with no record to
        // disagree with, takes the new version and refuses the old key.
        machine(&told, "toldbox");
        trust_operator_line(&told, "home", &k2);
        let accepted = accept(&k2_v3, &k2_v3_sig).unwrap();
        assert_eq!(accepted.charter.version, 3);
        assert_eq!(accepted.operator, k2);
        assert_eq!(accept(&k1_v1, &k1_v1_sig).unwrap_err().reason, UNKNOWN_OPERATOR);

        let _ = std::fs::remove_dir_all(&op);
        let _ = std::fs::remove_dir_all(&peer);
        let _ = std::fs::remove_dir_all(&told);
    }

    /// The carriage path end to end at the container level: a machine that
    /// trusts only the operator key accepts a charter by LETTER from an origin
    /// it has never paired with, and a refused one leaves nothing behind.
    #[test]
    fn a_charter_letter_verifies_the_charter_before_the_letter_origin() {
        let (_guard, _saver) = isolate();
        let op = machine_dir("carriage-op");
        let peer = machine_dir("carriage-peer");

        machine(&op, "opbox");
        let init = init("home").unwrap();
        let op_line = node_line().unwrap();
        machine(&peer, "peerbox");
        let peer_line = node_line().unwrap();
        machine(&op, "opbox");
        let src = format!("mesh = \"home\"\nversion = 0\nrelays = []\n\n[nodes]\n{op_line}\n{peer_line}\n");
        fs::atomic_write(&source_path("home"), &src).unwrap();
        sign("home", None).unwrap();
        let (v1, v1_sig) = read_pair(&op, "home");
        let charter = parse(&src).unwrap();
        let peer_binding = charter.nodes["peerbox"].age.clone();
        let now = now_iso_utc();
        let container =
            seal::seal_charter(&v1, &v1_sig, &peer_binding, "home", 1, "peerbox", &now).unwrap();

        // The peer trusts ONLY the operator key — no pairing, no nodes.json
        // entry for `opbox`, which is exactly the bootstrap case.
        machine(&peer, "peerbox");
        trust_operator_line(&peer, "home", &init.operator);
        assert!(
            crate::node_store::load_nodes().iter().all(|n| n.name != "opbox"),
            "the origin is unknown to this node"
        );
        let digest = match seal::deposit_container(&container, &container.origin_mesh).unwrap() {
            seal::ContainerOutcome::Applied { mesh, version, digest, .. } => {
                assert_eq!(mesh, "home");
                assert_eq!(version, 1);
                digest
            }
            other => panic!("expected the charter to apply, got {other:?}"),
        };
        assert_eq!(in_force_charter("home").unwrap().version, 1);
        assert_eq!(load_trust("home").unwrap().unwrap().operator, init.operator);

        // The caller records it once it is applied, and then a re-offer is a
        // duplicate with no second apply.
        seal::record_admitted(&container, &digest).unwrap();
        match seal::deposit_container(&container, &container.origin_mesh).unwrap() {
            seal::ContainerOutcome::Duplicate { filed_letter } => {
                assert!(!filed_letter, "a charter is never filed, so it never owes an ack");
            }
            other => panic!("expected a duplicate, got {other:?}"),
        }

        // The window the record leaves open, stated rather than hidden: an
        // unrecorded re-offer re-runs the accept, finds the version it is
        // already on, and is refused `stale-charter` — the charter is in force
        // either way and nothing is written a second time.
        std::fs::remove_file(crate::mail::mail_dir().join("containers.jsonl")).unwrap();
        let refusal = match seal::deposit_container(&container, &container.origin_mesh).unwrap() {
            seal::ContainerOutcome::Refused { reason, detail } => Refusal::new(&reason, detail),
            other => panic!("expected a refusal once the record is gone, got {other:?}"),
        };
        assert_eq!(refusal.reason, STALE_CHARTER, "{}", refusal.detail);
        assert_eq!(in_force_charter("home").unwrap().version, 1, "and the charter in force is untouched");

        let _ = std::fs::remove_dir_all(&op);
        let _ = std::fs::remove_dir_all(&peer);
    }

    /// A charter letter whose enclosed charter fails ANY accept step is
    /// refused before anything else in the container is trusted, and leaves no
    /// state at all — not a charter, not a signature, not a version mark, not
    /// a dedup record.
    #[test]
    fn a_charter_letter_under_an_untrusted_key_leaves_no_state_behind() {
        let (_guard, _saver) = isolate();
        let op = machine_dir("untrusted-op");
        let peer = machine_dir("untrusted-peer");

        machine(&op, "opbox");
        init("home").unwrap();
        let op_line = node_line().unwrap();
        machine(&peer, "peerbox");
        let peer_line = node_line().unwrap();
        machine(&op, "opbox");
        let src = format!("mesh = \"home\"\nversion = 0\nrelays = []\n\n[nodes]\n{op_line}\n{peer_line}\n");
        fs::atomic_write(&source_path("home"), &src).unwrap();
        sign("home", None).unwrap();
        let (v1, v1_sig) = read_pair(&op, "home");
        let charter = parse(&src).unwrap();
        let peer_binding = charter.nodes["peerbox"].age.clone();
        let now = now_iso_utc();
        let container =
            seal::seal_charter(&v1, &v1_sig, &peer_binding, "home", 1, "peerbox", &now).unwrap();

        // This machine is rooted in the SAME mesh name with its OWN key: it
        // trusts an operator key, just not the one that signed this charter.
        machine(&peer, "peerbox");
        init("home").unwrap();
        let before = load_trust("home").unwrap().unwrap();

        let refusal = match seal::deposit_container(&container, &container.origin_mesh).unwrap() {
            seal::ContainerOutcome::Refused { reason, detail } => Refusal::new(&reason, detail),
            other => panic!("expected a refusal, got {other:?}"),
        };
        assert_eq!(refusal.reason, UNKNOWN_OPERATOR, "{}", refusal.detail);

        assert!(!in_force_path("home").exists(), "no charter was put in force");
        assert!(!in_force_sig_path("home").exists(), "and no signature stored");
        assert!(!crate::mail::mail_dir().join("containers.jsonl").exists(), "and no dedup record");
        let after = load_trust("home").unwrap().unwrap();
        assert_eq!(after, before, "the trust record is exactly what was there before");
        assert!(after.versions.is_empty(), "no version mark was written for the refused key");

        let _ = std::fs::remove_dir_all(&op);
        let _ = std::fs::remove_dir_all(&peer);
    }

    /// The exact bytes an operator signs, pinned: `frame("aoide/charter",
    /// [mesh, version, sha256(file bytes)])` with the digest of an EMPTY file,
    /// so the example in the design doc is checkable by anyone with
    /// `sha256sum` and the frame rule — not just by this test.
    #[test]
    fn the_charter_signature_input_is_the_documented_frame() {
        let digest = seal::sha256(b"");
        assert_eq!(
            hex_encode(&digest),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "sha256 of an empty file, the digest the worked example uses"
        );
        let input = seal::charter_sig_input("home", 12, &digest);
        assert_eq!(input.len(), 70, "13-byte label + NUL + (4+4) + (4+8) + (4+32)");
        assert_eq!(
            hex_encode(&input),
            "616f6964652f636861727465720000000004686f6d6500000008000000000000\
             000c00000020e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495\
             991b7852b855",
            "the worked charter_sig input"
        );
        // And the detached signature's own frame is the pair, digest included.
        let sig = [0x11u8; 64];
        let frame = seal::charter_sig_frame(&sig, &digest);
        let (back, back_digest) = seal::parse_charter_sig_frame(&frame).unwrap();
        assert_eq!(back, sig.to_vec());
        assert_eq!(back_digest, digest.to_vec());
    }

    /// A version this build cannot advance is refused **before** anything is
    /// written: no bump, no `.sig`, no spool. The wrapper the code cannot reach
    /// (`u64::MAX`, since a charter's integer is a TOML `i64`) is guarded by
    /// `checked_add`; this pins the reachable boundary — `i64::MAX`, whose
    /// successor cannot be written back as a TOML integer at all.
    #[test]
    fn sign_refuses_a_version_that_cannot_advance() {
        let (_guard, _saver) = isolate();
        let op = machine_dir("version-boundary");
        machine(&op, "opbox");
        init("home").unwrap();
        let line = node_line().unwrap();
        let src = format!(
            "mesh = \"home\"\nversion = 9223372036854775807\nrelays = []\n\n[nodes]\n{line}\n"
        );
        fs::atomic_write(&source_path("home"), &src).unwrap();

        let err = sign("home", None).unwrap_err();
        assert!(err.contains("cannot advance any further"), "{err}");
        assert_eq!(
            std::fs::read_to_string(source_path("home")).unwrap(),
            src,
            "the source is untouched — a version that cannot advance is never half-written"
        );
        assert!(!source_sig_path("home").exists(), "and no signature appears beside it");

        let _ = std::fs::remove_dir_all(&op);
    }

    /// `sign` is the last write to the file, so it may not eat what the
    /// operator wrote: the version is set in place, and the comments, key order
    /// and spacing around it survive into the bytes that get signed.
    #[test]
    fn sign_keeps_the_operators_comments() {
        let (_guard, _saver) = isolate();
        let op = machine_dir("comments");
        machine(&op, "opbox");
        init("home").unwrap();
        let line = node_line().unwrap();
        let src = format!(
            "# home: the User's own machines\n\
             mesh    = \"home\"\n\
             version = 0  # written by `sign`, never by hand\n\
             relays  = []\n\
             \n\
             [nodes]\n\
             # paste each machine's `aoide identity` line here\n\
             {line}\n"
        );
        fs::atomic_write(&source_path("home"), &src).unwrap();
        let signed = sign("home", None).unwrap();
        assert_eq!(signed.version, 1);

        let after = std::fs::read_to_string(source_path("home")).unwrap();
        assert!(after.contains("# home: the User's own machines"), "{after}");
        assert!(after.contains("version = 1"), "{after}");
        assert!(
            after.contains("# written by `sign`, never by hand"),
            "the comment on the version line itself survives: {after}"
        );
        assert!(after.contains("# paste each machine's `aoide identity` line here"), "{after}");
        assert!(after.contains("mesh    = \"home\""), "and the operator's own spacing: {after}");

        let _ = std::fs::remove_dir_all(&op);
    }

    /// The zone check, made operable on the one authority that can answer it
    /// before any registry does: a letter whose carrier relabels the zone it
    /// arrived in is refused `zone-violation` — and the charter it carried
    /// stays in force, because the charter's authority is the operator
    /// signature and never the letter's routing claim.
    #[test]
    fn a_charter_letter_in_the_wrong_zone_is_refused() {
        let (_guard, _saver) = isolate();
        let op = machine_dir("zone-op");
        let peer = machine_dir("zone-peer");

        machine(&op, "opbox");
        let rooted = init("home").unwrap();
        let op_line = node_line().unwrap();
        machine(&peer, "peerbox");
        let peer_line = node_line().unwrap();
        machine(&op, "opbox");
        let src = format!("mesh = \"home\"\nversion = 0\nrelays = []\n\n[nodes]\n{op_line}\n{peer_line}\n");
        fs::atomic_write(&source_path("home"), &src).unwrap();
        sign("home", None).unwrap();
        let (v1, v1_sig) = read_pair(&op, "home");
        let binding = parse(&src).unwrap().nodes["peerbox"].age.clone();
        let mut container = seal::seal_charter(&v1, &v1_sig, &binding, "home", 1, "peerbox", &now_iso_utc()).unwrap();
        // `mesh` is hop-mutable and outside `ctx`, so a carrier can relabel it
        // without touching a byte the origin signed — which is exactly the
        // claim this check refuses.
        container.mesh = "elsewhere".to_string();

        machine(&peer, "peerbox");
        trust_operator_line(&peer, "home", &rooted.operator);
        match seal::deposit_container(&container, &container.origin_mesh).unwrap() {
            seal::ContainerOutcome::Refused { reason, detail } => {
                assert_eq!(reason, seal::ZONE_VIOLATION, "{detail}");
                assert!(detail.contains("elsewhere"), "and names the zone it actually arrived in: {detail}");
            }
            other => panic!("expected a zone violation, got {other:?}"),
        }
        assert_eq!(
            in_force_charter("home").unwrap().version,
            1,
            "the charter it carried is applied anyway — its authority is the operator signature"
        );

        let _ = std::fs::remove_dir_all(&op);
        let _ = std::fs::remove_dir_all(&peer);
    }

    #[test]
    fn the_source_is_validated_before_it_is_signed() {
        let (_guard, _saver) = isolate();
        let op = machine_dir("validate-op");
        machine(&op, "opbox");
        init("home").unwrap();
        let line = node_line().unwrap();

        let cases: Vec<(String, &str)> = vec![
            (format!("mesh = \"Home\"\nversion = 0\n\n[nodes]\n{line}\n"), "valid mesh name"),
            (format!("mesh = \"home\"\nversion = 0\nrelays = [\"nowhere\"]\n\n[nodes]\n{line}\n"), "not a node on this charter"),
            (
                format!("mesh = \"home\"\nversion = 0\n\n[nodes]\n{}\n", line.replace(" }", ", port = 22 }")),
                "unknown field",
            ),
            (
                format!(
                    "mesh = \"home\"\nversion = 0\n\n[nodes]\n{}\n",
                    line.replace(" }", ", grant = [\"admin\"] }")
                ),
                "grant",
            ),
            (
                format!(
                    "mesh = \"home\"\nversion = 0\n\n[nodes]\n{}\n",
                    line.replace(" }", ", address = \"http://x\" }")
                ),
                "is not `poll`",
            ),
            (
                format!("mesh = \"home\"\nversion = 0\n\n[status]\nopbox = \"dead\"\n\n[nodes]\n{line}\n"),
                "hold",
            ),
            (
                format!("mesh = \"home\"\nversion = 0\n\n[status]\nnowhere = \"hold\"\n\n[nodes]\n{line}\n"),
                "not a node on this charter",
            ),
            (
                format!("mesh = \"home\"\nversion = 0\n\n[gates]\nfriends = \"nowhere\"\n\n[nodes]\n{line}\n"),
                "not a node on this charter",
            ),
            (
                format!("mesh = \"home\"\nversion = 0\n\n[nodes]\n{line}\n{}\n", line.replace("opbox = ", "same-key = ")),
                "are the same identity key",
            ),
        ];
        for (text, needle) in cases {
            let err = parse(&text).unwrap_err();
            assert!(err.contains(needle), "expected `{needle}` in: {err}");
        }

        // And a bad source is never signed: the file on disk keeps version 0
        // and no signature appears beside it.
        let bad = format!("mesh = \"home\"\nversion = 0\nrelays = [\"nowhere\"]\n\n[nodes]\n{line}\n");
        fs::atomic_write(&source_path("home"), &bad).unwrap();
        assert!(sign("home", None).is_err());
        assert!(!source_sig_path("home").exists());
        assert!(std::fs::read_to_string(source_path("home")).unwrap().contains("version = 0"));

        let _ = std::fs::remove_dir_all(&op);
    }
}

