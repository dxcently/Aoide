//! `aoide mesh` (task #135 P4) and `aoide mesh pair` (P5): compare every
//! declared `[mesh.<name>]` (`aoide_storage::config::Mesh`) against the live
//! node registry (`aoide_storage::node_store::load_nodes`), report where
//! they diverge — and, on the converge, close that divergence by running
//! the ordinary pairing ceremony over it.
//!
//! **Declaration vs. registry — intent vs. state.** `config.toml`'s
//! `[mesh.*]` is INTENT: the operator's own roster of who SHOULD be paired,
//! written once and rarely touched. `state/nodes.json` is STATE: the
//! product of actually running the pairing ceremony
//! (`docs/architecture/PAIRING.md`), rebuilt by that ceremony alone. This
//! module never writes either — it only reads both fresh on every call and
//! diffs them. No new persisted state, no field added to
//! [`aoide_storage::node_store::Node`]: a mesh's shape lives entirely in
//! `config.mesh`, recomputed from scratch each time rather than cached
//! anywhere a second copy could go stale.
//!
//! **[`drift`] is pure** — no I/O, no clock, no env — so every ruling below
//! is a plain unit test against in-memory values, never a fixture on disk.
//! [`handle_mesh`] is the only impure edge: it resolves
//! `aoide_storage::config::load()`,
//! `aoide_storage::node_store::load_nodes()`, and
//! `aoide_storage::display::local_host_name()`, then hands the three
//! results in.
//!
//! **Three drift classes**, checked in this order per declared node:
//! - [`DriftClass::Missing`] — declared, but no node record by that name
//!   exists at all.
//! - [`DriftClass::Unverified`] — a node record exists, but the pairing
//!   ceremony never confirmed it (`Node::verified == false`). Checked
//!   before via, since an unconfirmed node's `via` is not yet meaningful.
//! - [`DriftClass::ViaMismatch`] — verified, but the live `via` does not
//!   match the mesh's declared hop. `recorded: None` is the severe case:
//!   a call with no `via` dials the node's bare `url` directly, which for
//!   an already-paired node is commonly a loopback address — so the call
//!   silently dials THIS box's own loopback instead of hopping anywhere.
//!
//! A node that matches (verified, `via` equal to the declared hop) gets no
//! row at all — [`MeshSection::rows`] holds drift only, never a clean bill
//! of health per node.
//!
//! **`allows` divergence is deliberately NOT a drift class.** A mesh's
//! `grant` is a default for the moment a pair is MINTED, never a continuous
//! invariant over it: [`handle_mesh_pair`] hands it to the ceremony, and
//! `node_store::upsert_paired_node` stamps a capability set only on a FIRST
//! verification, leaving an already-verified node's `allows` exactly as it
//! was. A human then narrows or widens that set deliberately, one node at a
//! time, with `node allow`. Comparing the result back against the
//! declaration would report every one of those decisions as permanent drift
//! and invite the operator to "fix" his own revocation — so the comparison
//! is not made.
//!
//! **`paired-but-not-declared` is reported, never accused.** A verified
//! node named in no mesh lands in [`MeshReport::undeclared`] — its own
//! section, no drift count, no suggested action. Plenty of legitimate
//! nodes (anything paired before this feature existed, anything
//! deliberately kept outside every declared mesh) are undeclared forever;
//! this module states the fact and stops.
//!
//! **The local host is skipped silently.** A mesh declared identically
//! across every member box will list that box's own name among its nodes
//! (the same file, deployed everywhere) — comparing a box against itself
//! is not a node relationship, so [`drift`] drops that one entry before it
//! ever becomes a row, an undeclared entry, or anything else visible.
//!
//! **Drift is never itself a failure — bare `mesh` is [`Outcome::ok`]
//! whenever the config loads.** Like `config`/`node list`, this is a
//! report of what's on disk, not a pass/fail gate — drift is surfaced in
//! the message and `data.report`, never turned into a non-zero exit by
//! itself. The one exception is a config that fails to load or validate at
//! all: that is [`Outcome::error`] (`reason: "config-unreadable"`, no
//! `data.report` — there is nothing to compare), the same shape `aoide
//! config` itself already uses for the same failure.
//!
//! **`mesh pair` is the converge, and it consumes exactly the [`drift`]
//! above.** There is no second comparison anywhere in the tree: [`plan`]
//! reads [`MeshSection::rows`] and selects `missing` + `unverified`, in
//! declared-name order; `via-mismatch` comes back `skipped`, naming `aoide
//! pair <name>` as the fix. That skip is a ruling, not an omission — a
//! converge NEVER modifies an existing verified node, because re-pairing
//! rotates key material and because writing `via` outside a ceremony commit
//! would make this a second writer of a field
//! `node_store::set_node_via` reserves to that commit. It also makes a
//! converge idempotent by construction: run it twice and the second run is
//! all-`skipped`.
//!
//! Each selected node goes through `commands::run_pair_request` and nothing
//! else — the ordinary two-POST ceremony, the ordinary park, the ordinary
//! blocking wait. **Zero ceremony logic lives here**; a duplicated poll loop
//! is the design error the `poll_outbound_once`/`commit_outbound` split
//! exists to prevent. What a converge adds over typing `aoide pair` N times
//! is the selection, one pre-flight confirm for the whole run, the mesh's
//! declared `grant`, and a report in one vocabulary — completed / parked /
//! UNREACHABLE / skipped ([`ConvergeOutcome`]).
//!
//! **`sameOperator` is declared and not acted on.** Whether a converge may
//! ever satisfy the far side's typed code on an operator's behalf is
//! undecided (`docs/architecture/PAIRING.md`'s "Mesh declaration" section),
//! so a mesh declaring it converges byte-identically to one that does not:
//! every node paired with both codes typed. The report carries one note
//! saying the flag was seen and not acted on — a note, never a row, never a
//! status, never a refusal.

use aoide_protocol::output::Outcome;
use aoide_protocol::registry::{arg, cmd, flag, Registry};
use aoide_protocol::Invocation;
use aoide_storage::config::Mesh;
use aoide_storage::node_store::Node;
use serde::Serialize;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

/// Why one declared node diverges from the live registry. Internally
/// tagged (`class`) so [`MeshRow`]'s `#[serde(flatten)]` puts `class`
/// alongside `node` in one flat JSON object, never a nested `class: {...}`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "class", rename_all = "kebab-case")]
pub enum DriftClass {
    /// Declared, but no node record by this name exists at all.
    Missing,
    /// A node record exists, but pairing was never confirmed.
    Unverified,
    /// A node record exists and is verified, but its live `via` does not
    /// match the mesh's declared hop. `recorded: None` is the severe case
    /// — see the module doc.
    ViaMismatch { declared: String, recorded: Option<String> },
}

/// One divergent node inside one declared mesh. [`MeshSection::rows`]
/// holds these — never a row for a node that matches.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MeshRow {
    pub node: String,
    #[serde(flatten)]
    pub class: DriftClass,
}

/// Which KIND of declaration a mesh's routing table comes from — the same two
/// sources [`MeshSource`] names, read as a fact about the mesh rather than as
/// the choice one of them wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MeshKind {
    /// `[mesh.<name>]` in config: the paired records are the declaration.
    Pair,
    /// A signed charter is the declaration.
    Charter,
}

/// **One node a mesh declares**, with what this box can state about it. Every
/// fact here is the DECLARATION's, read through the one routing seam, except
/// [`NodeRow::nickname`] (display only) and [`NodeRow::liveness`] (observation
/// only).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NodeRow {
    /// The name the DECLARATION gives this node: a charter LINE's name for a
    /// charter mesh — the name every policy, routing and audit lookup uses for
    /// it — and the recorded name for a pair mesh, where the record IS the
    /// declaration.
    pub name: String,
    /// The `nodes.json` nickname this node's key is ALSO recorded under, where
    /// one exists and differs from `name`. Display, never an input: a mesh's
    /// own answer about a node comes from its declaration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
    /// The declared status — `active` (declared with none), `hold` or `down` —
    /// exactly the words `[status]` uses.
    pub status: String,
    /// `relay`, `gate` or `member`.
    pub role: String,
    /// The other meshes this node is THIS mesh's declared gate into. Empty
    /// unless `role` is `gate`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub gates: Vec<String>,
    /// Where this node's identity key comes from: `charter` (a signed line) or
    /// `record` (a paired record).
    #[serde(rename = "keySource")]
    pub key_source: String,
    /// What this box has OBSERVED about reaching it: `reachable` (a recorded
    /// attempt reached the peer), `unreachable` (a recorded attempt did not), or
    /// `unverified` — nothing observed at all. **`unverified` is never `dead`
    /// and never `down`**: liveness is observation, and `down` is a
    /// declaration, so the two are different facts about different things.
    pub liveness: String,
}

/// One declared `[mesh.<name>]`, compared. `grant`/`same_operator_note` are
/// copied straight off the declaration for display — [`drift`] never
/// compares them against anything (see the module doc's note on grants).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MeshSection {
    pub name: String,
    /// Which source answers a caller's grant in this mesh HERE — the same
    /// choice `aoide-server::a2a::grant_in_mesh` makes at one function.
    /// [`drift`] is pure and knows nothing about charters, so it always sets
    /// [`MeshSource::Paired`]; [`report`] is the one place that flips it, from
    /// the charter state it reads beside the comparison.
    pub source: MeshSource,
    /// The mesh's DECLARED grant — the default a pair minted here gets,
    /// never a continuous invariant over the live registry.
    pub grant: Option<Vec<String>>,
    /// The pre-charter `sameOperator` claim, RETIRED (P-CHARTER: one operator
    /// is one charter signer). Present only when the mesh still declares it,
    /// and only ever a sentence: the flag changes no row, no count and no
    /// status, and nothing acts on it. `config::Mesh::same_operator_note` is
    /// the one place the sentence is written.
    #[serde(rename = "sameOperatorNote", skip_serializing_if = "Option::is_none")]
    pub same_operator_note: Option<String>,
    /// **The nodelist view's per-mesh grants** (P-CHARTER): every declared
    /// node COMPARED in this mesh, mapped to the grant the live registry
    /// holds for it in this mesh ([`Node::grant`]) — `[]` for a node with a
    /// record that is trusted in no mesh at all, and no entry at all for a
    /// node with no record (`rows` already says `missing`). This is the fact
    /// the door reads, shown where the operator declares meshes: a grant in
    /// another mesh never appears here, which is the whole point of a
    /// per-mesh view — the same node's row in another section will carry
    /// that mesh's grant instead.
    pub grants: BTreeMap<String, Vec<String>>,
    /// How many of this mesh's declared nodes were actually COMPARED — the
    /// local host's own entry (if declared) is excluded, the same as it is
    /// from `rows`, so `declared - rows.len()` (the "N/M ok" ratio) is never
    /// inflated by an entry that was never checked against anything. This
    /// is NOT the raw size of the mesh's `nodes` map in `config.toml` — a
    /// mesh naming itself plus two others declares 3 but compares 2.
    pub declared: usize,
    /// Was this box's own name (`display::local_node_name()` — the NODE name,
    /// folded) found among this mesh's declared node keys? `false` means either
    /// this box genuinely isn't part of the mesh, or it's declared under the
    /// wrong key (a typo, a name outside the address grammar). The comparison
    /// MUST use the folded form: a mesh key is forced through
    /// `node_store::valid_node_name` (`storage::config`), so the raw,
    /// case-preserving host name can never equal one — comparing the raw name
    /// made a box whose host name differs in case report its OWN declared entry
    /// as an unpaired row and `selfDeclared:false`. The two are
    /// indistinguishable from here, so this is a note, never a drift row:
    /// it changes neither `rows` nor `declared`.
    #[serde(rename = "selfDeclared")]
    pub self_declared: bool,
    pub rows: Vec<MeshRow>,
    /// Which kind of declaration this mesh's routing table is.
    pub kind: MeshKind,
    /// The version of the charter in force, for a charter mesh — the version
    /// every check in that mesh is decided by. Absent for a pair mesh.
    #[serde(rename = "charterVersion", skip_serializing_if = "Option::is_none")]
    pub charter_version: Option<u64>,
    /// The word this box's declaration SET refuses this mesh with, when it does
    /// (`charter-tampered`, `key-divergence`, …). A refused mesh fails closed for
    /// ITSELF alone: its `nodes` stay empty, because nothing in it is decidable
    /// here, and the other meshes still report.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal: Option<String>,
    /// **The nodelist view**: every node this mesh declares, with the facts
    /// [`NodeRow`] carries. Empty for a mesh this box cannot read.
    pub nodes: Vec<NodeRow>,
}

/// The whole comparison, every declared mesh plus the separate
/// never-accused undeclared list. What [`handle_mesh`] renders and what
/// `--json` serializes under `data.report`.
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct MeshReport {
    pub sections: Vec<MeshSection>,
    /// Verified nodes named in no declared mesh. Reported, never accused
    /// — see the module doc.
    pub undeclared: Vec<String>,
    /// **The charter rows** (P-CHARTER): every mesh with a charter IN FORCE at
    /// this node, declared or not — one row per mesh, [`CharterRow`]'s own doc
    /// for what each field answers. Empty on a box that has accepted none,
    /// which is every box before P-CHARTER.
    pub charters: Vec<CharterRow>,
}

/// Where one mesh's trust comes from — the source choice every gated arm makes
/// at one function (`aoide-server::a2a::grant_in_mesh`), shown where the
/// operator declares and reviews meshes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MeshSource {
    /// A charter governs this mesh here: the caller's line is the grant, a
    /// paired record's own entry is inert, and local `node allow … off` is
    /// the only thing that narrows it.
    Charter,
    /// The ordinary pair mesh: the pairwise records are the grant.
    Paired,
}

/// **One charter in force at this node**, as the operator needs to review it:
/// which mesh, how far it has been seen, who signed it, what the last version
/// re-keyed, and which of this box's own paired records its rule makes inert.
///
/// The re-keyed list is the reason this row exists at all: a charter applied
/// by an unattended poll can land with nobody watching, and "a new key on an
/// old name is what a stolen operator key would sign"
/// (`docs/architecture/HTTPS-MESH-API.md` "Charters") is something an operator
/// has to be able to SEE afterwards, not only read in a log.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CharterRow {
    pub mesh: String,
    /// Does `config.toml` also declare this mesh (`[mesh.<name>]`)? A charter
    /// mesh need not be declared — a machine that took its first charter by
    /// file has state and no declaration — so this says whether the section
    /// above is about the same mesh, never whether the charter is real.
    pub declared: bool,
    /// Is a charter DOCUMENT in force here (parsed, from
    /// `state/mesh/<mesh>/charter.toml`)? `false` with a row present means the
    /// mesh is charter-SHAPED without a document this node can read — an
    /// operator key is recorded (a join that has not yet accepted anything) or
    /// the stored document no longer parses. The door refuses everything in
    /// such a mesh rather than falling back to the paired records (review F2).
    #[serde(rename = "inForce")]
    pub in_force: bool,
    /// The version in force.
    pub version: u64,
    /// The operator key's durable fingerprint (`SHA256:<hex>`,
    /// `charter::fingerprint_of_key`) — the value an operator compares out of
    /// band, and the one every refusal names.
    pub operator: String,
    /// The operator key itself, bare hex: public material, printed by
    /// `mesh charter init` as `operator = "ed25519:<hex>"`, and shown here so
    /// the row and the config line can be compared without a second command.
    #[serde(rename = "operatorKey")]
    pub operator_key: String,
    /// Where this node's trust in that key is written down —
    /// `config`, `state`, `both`, or `none` (the last being
    /// `unknown-operator`: a charter on disk this node has no way to honour).
    /// A DISAGREEMENT between the two is `operator-mismatch`, and then nothing
    /// about the mesh is decidable, which `trusted` below says.
    pub trust: String,
    /// Is the operator key decidable (`charter::trusted_operator`)? `false` is
    /// `operator-mismatch` or an unreadable config — no charter governs the
    /// mesh while it holds, and its grants are not read.
    pub trusted: bool,
    /// The highest version applied for the key this node trusts, out of
    /// `trust.json`'s per-operator high-water mark.
    #[serde(rename = "highWater")]
    pub high_water: u64,
    /// The nodes the LAST applied version changed the identity key of.
    pub rekeyed: Vec<RekeyedRow>,
    /// Nodes on the charter that this box ALSO holds a paired record for, in
    /// this mesh: the record's `grants` entry is not what the door reads (the
    /// charter's line is), so the row reports it rather than letting an
    /// operator believe the record's own grant is live.
    ///
    /// **A record here is inert as a GRANT, not as a whole.** If it also
    /// carries `autogate`, the door's unsigned address/token rail still
    /// resolves it — and that rail names no mesh, so the door judges the record
    /// by its HOME mesh's rules. Where this row IS the home mesh, `autogated`
    /// below names exactly those records, which is what stops this list from
    /// implying an `autogate` record has nothing left that answers here.
    pub inert: Vec<String>,
    /// **This box's records whose UNSIGNED autogate rail answers by THIS
    /// charter** — every registered record with `autogate` set, reported only
    /// on the row for the home mesh (`[pairing] homeMesh`), because that is the
    /// mesh the door judges such a record by (`aoide-server::a2a::
    /// rail_admits` reads `effective_mesh(None)`: the rail carries no signed
    /// mesh to name another). On any other mesh's row the rail answers by
    /// home's rules, not by these, so naming them there would be the same class
    /// of lie the `inert` list was fixed for. A record in both lists is a
    /// record whose own grant is inert AND whose rail this charter judges: its
    /// key on the line with `message` ⇒ delivers, otherwise the send is held
    /// PENDING (never refused).
    pub autogated: Vec<String>,
    /// How many nodes the charter lists.
    pub nodes: usize,
}

/// One re-keyed node, flattened for display: who, from, to, when.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RekeyedRow {
    pub node: String,
    pub from: String,
    pub to: String,
    pub version: u64,
    pub at: String,
}

/// Compare every declared mesh against the live node registry. Pure — see
/// the module doc. `meshes`/`nodes` come from
/// `aoide_storage::config::load()`/`aoide_storage::node_store::load_nodes()`
/// respectively; `local_name` from
/// `aoide_storage::display::local_host_name()`. Iteration follows each
/// input `BTreeMap`'s own sorted order, and `undeclared` is sorted
/// explicitly, so two calls over the same declarations and a differently
/// ordered `nodes` slice render identically.
pub fn drift(meshes: &BTreeMap<String, Mesh>, nodes: &[Node], local_name: &str) -> MeshReport {
    let mut declared_names: BTreeSet<&str> = BTreeSet::new();
    let mut sections = Vec::with_capacity(meshes.len());

    for (name, mesh) in meshes {
        let mut rows = Vec::new();
        let mut grants: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut compared = 0usize;
        for (node_name, hop) in &mesh.nodes {
            if node_name == local_name {
                continue; // this box naming itself — not a node, not an error
            }
            compared += 1;
            declared_names.insert(node_name.as_str());
            let record = nodes.iter().find(|p| &p.name == node_name);
            if let Some(p) = record {
                grants.insert(node_name.clone(), p.grant(name).to_vec());
            }
            let class = match record {
                None => DriftClass::Missing,
                Some(p) if !p.verified => DriftClass::Unverified,
                Some(p) if p.via.as_deref() != Some(hop.as_str()) => {
                    DriftClass::ViaMismatch { declared: hop.clone(), recorded: p.via.clone() }
                }
                Some(_) => continue, // matches — no row
            };
            rows.push(MeshRow { node: node_name.clone(), class });
        }
        sections.push(MeshSection {
            name: name.clone(),
            source: MeshSource::Paired,
            grant: mesh.grant.clone(),
            same_operator_note: mesh.same_operator_note(name),
            grants,
            declared: compared,
            self_declared: mesh.nodes.contains_key(local_name),
            rows,
            // The declaration's own facts, and what this box has observed, are
            // [`report`]'s to fill: [`drift`] compares two sources and knows
            // nothing about a charter or an outbox.
            kind: MeshKind::Pair,
            charter_version: None,
            refusal: None,
            nodes: Vec::new(),
        });
    }

    let mut undeclared: Vec<String> = nodes
        .iter()
        .filter(|p| p.verified && p.name != local_name && !declared_names.contains(p.name.as_str()))
        .map(|p| p.name.clone())
        .collect();
    undeclared.sort();

    MeshReport { sections, undeclared, charters: Vec::new() }
}

/// **The whole report the handler renders: the pure comparison above, plus the
/// charter rows.** The ONE place that reads charter state on this path —
/// `drift` stays a pure comparison of declarations against the live registry,
/// and this composes what an operator actually needs to see: which source
/// answers each declared mesh, and every charter in force whether declared or
/// not.
pub fn report(meshes: &BTreeMap<String, Mesh>, nodes: &[Node], local_name: &str) -> MeshReport {
    let mut report = drift(meshes, nodes, local_name);
    let charters = charter_rows(&meshes.keys().cloned().collect(), nodes);
    // ONE read of the declaration set for the whole view, so every mesh's kind,
    // refusal and node list are answered from the same snapshot — and a set that
    // will not load leaves every section without a node list rather than
    // inventing one from a second, weaker source.
    let set = aoide_storage::routing::declarations().ok();
    for section in &mut report.sections {
        let charter = charters.iter().find(|c| c.mesh == section.name);
        if charter.is_some() {
            section.source = MeshSource::Charter;
        }
        section.kind = if charter.is_some() { MeshKind::Charter } else { MeshKind::Pair };
        section.charter_version = charter.map(|c| c.version);
        match set.as_deref().and_then(|set| aoide_storage::routing::declaration_of(set, &section.name)) {
            // A refused mesh is its word and nothing else: no node list can be
            // read out of a declaration this box could not honour.
            Some(Err(refusal)) => section.refusal = Some(refusal.reason.clone()),
            Some(Ok(declaration)) => {
                section.nodes = node_rows(&section.name, meshes.get(&section.name), declaration, nodes);
            }
            None => {}
        }
    }
    report.charters = charters;
    report
}

/// The nodelist rows of one mesh: every node its declaration names, in name
/// order, with the facts [`NodeRow`] carries.
///
/// **The names are the DECLARATION's.** A charter mesh lists its signed lines —
/// the names every lookup uses for it — and a pair mesh lists the section's own
/// keys plus every record GRANTED `message` in it, which is the shape `mesh pair`
/// writes without a hop address. The `nodes.json` nickname rides beside a name
/// only where a record's key proves the same node is recorded under another one.
fn node_rows(
    mesh: &str,
    section: Option<&Mesh>,
    declaration: &aoide_storage::routing::Declaration,
    records: &[Node],
) -> Vec<NodeRow> {
    let mut names: BTreeSet<String> = if declaration.is_charter() {
        aoide_storage::charter::governing(mesh)
            .map(|charter| charter.nodes.keys().cloned().collect())
            .unwrap_or_default()
    } else {
        section.map(|section| section.nodes.keys().cloned().collect()).unwrap_or_default()
    };
    if !declaration.is_charter() {
        for record in records {
            if record.grant(mesh).iter().any(|cap| cap == "message") {
                names.insert(record.name.clone());
            }
        }
    }
    names
        .into_iter()
        .map(|name| {
            let key = declaration.key_of(&name);
            let nickname = records
                .iter()
                .find(|record| {
                    record.name != name
                        && match (key, record.pubkey.as_deref()) {
                            (Some(key), Some(recorded)) => key.eq_ignore_ascii_case(recorded),
                            _ => false,
                        }
                })
                .map(|record| record.name.clone());
            let gates: Vec<String> = declaration
                .gates()
                .iter()
                .filter(|(_, gate)| gate.as_str() == name)
                .map(|(other, _)| other.clone())
                .collect();
            // A node can be both the mesh's relay and its gate (the fixture's
            // `sakaki` is), so the two facts are both reported: `role` is the
            // relay when there is one — transit is what the route reads first —
            // and `gates` names the meshes it carries transit into.
            let role = if declaration.relays().iter().any(|relay| relay == &name) {
                "relay"
            } else if !gates.is_empty() {
                "gate"
            } else {
                "member"
            };
            let status =
                declaration.status_of(&name).map(str::to_string).unwrap_or_else(|| "active".to_string());
            let key_source = if declaration.is_charter() { "charter" } else { "record" }.to_string();
            let liveness = liveness_of(&name).to_string();
            NodeRow { name, nickname, status, role: role.to_string(), gates, key_source, liveness }
        })
        .collect()
}

/// What this box has OBSERVED about reaching `node`: a recorded attempt that
/// reached the peer, one that did not, or nothing at all. It reads the outbox's
/// own bookkeeping (each entry's `tries`/`lastOutcome`) and the link's back-off —
/// **no probe, no dial, no network I/O** — because liveness here is a fact about
/// the past, and a nodelist command that reached out would be its own witness.
fn liveness_of(node: &str) -> &'static str {
    match aoide_storage::outbox::list_entries(node) {
        Ok(entries) if entries.iter().any(|entry| entry.last_attempt_reached_the_peer()) => "reachable",
        Ok(entries) if entries.iter().any(|entry| !entry.last_try_at.is_empty()) => "unreachable",
        _ => match aoide_storage::outbox::read_link_state(node) {
            Ok(Some(_)) => "unreachable",
            _ => "unverified",
        },
    }
}

/// The charter rows: every mesh with a charter on disk at this node
/// (`charter::meshes_with_state`, so an undeclared one is reported too), with
/// the facts [`CharterRow`] carries. Reads `state/mesh/<m>/` only — the same
/// state `charter::governing` reads, minus its trust gate, because a report
/// must be able to say `trusted: false` about a charter the door would refuse
/// to honour rather than hiding it.
pub fn charter_rows(declared: &BTreeSet<String>, nodes: &[Node]) -> Vec<CharterRow> {
    let mut names: BTreeSet<String> = declared.iter().cloned().collect();
    names.extend(aoide_storage::charter::meshes_with_state());
    let home = aoide_storage::config::home_mesh();
    let mut out = Vec::new();
    for mesh in names {
        // F2: the row is on CHARTER-SHAPED, so a mesh whose operator key is
        // undecidable still reports itself as a charter mesh (`trusted: false`,
        // and no `inForce` document where none parses) instead of vanishing
        // from the report while the door refuses everything in it.
        if !aoide_storage::charter::charter_shaped(&mesh) {
            continue;
        }
        let in_force = aoide_storage::charter::in_force_charter(&mesh);
        let trust = aoide_storage::charter::load_trust(&mesh).ok().flatten().unwrap_or_default();
        let config_line = aoide_storage::charter::config_operator(&mesh).ok().flatten();
        let state_key = Some(trust.operator.clone()).filter(|k| !k.is_empty());
        out.push(CharterRow {
            declared: declared.contains(&mesh),
            mesh: mesh.clone(),
            in_force: in_force.is_some(),
            version: in_force.as_ref().map(|c| c.version).unwrap_or(0),
            operator: aoide_storage::charter::fingerprint_of_key(&trust.operator),
            operator_key: trust.operator.clone(),
            trust: match (&config_line, &state_key) {
                (Some(_), Some(_)) => "both".to_string(),
                (Some(_), None) => "config".to_string(),
                (None, Some(_)) => "state".to_string(),
                (None, None) => "none".to_string(),
            },
            trusted: aoide_storage::charter::trusted_operator(&mesh).is_ok(),
            high_water: trust.versions.get(&trust.operator).copied().unwrap_or(0),
            rekeyed: trust
                .rekeyed
                .iter()
                .map(|r| RekeyedRow {
                    node: r.node.clone(),
                    from: r.from.clone(),
                    to: r.to.clone(),
                    version: r.version,
                    at: r.at.clone(),
                })
                .collect(),
            inert: nodes
                .iter()
                .filter(|n| n.verified && !n.grant(&mesh).is_empty())
                .map(|n| n.name.clone())
                .collect(),
            autogated: if mesh == home {
                nodes.iter().filter(|n| n.autogate).map(|n| n.name.clone()).collect()
            } else {
                Vec::new()
            },
            nodes: in_force.as_ref().map(|c| c.nodes.len()).unwrap_or(0),
        });
    }
    out
}

/// The human-text rendering `aoide mesh`'s message carries — `--json`
/// serializes the same [`MeshReport`] structured instead, under
/// `data.report`. `local_name` is display-only here (it never changes what
/// was already decided in [`drift`]) — it lets the not-self-declared note
/// name the exact key a section is missing.
fn render_report(report: &MeshReport, local_name: &str) -> String {
    if report.sections.is_empty() {
        return if report.undeclared.is_empty() {
            "no mesh declared — see `aoide config` for [mesh.<name>]".to_string()
        } else {
            render_undeclared(&report.undeclared)
        };
    }
    let mut lines = Vec::new();
    for section in &report.sections {
        let clean = section.declared.saturating_sub(section.rows.len());
        let source = match report.charters.iter().find(|c| c.mesh == section.name) {
            Some(charter) => format!(
                "source: charter v{} ({}{})",
                charter.version,
                charter.operator,
                if charter.trusted { "" } else { ", NOT HONOURED: the operator key is undecidable here" }
            ),
            None => "source: paired records".to_string(),
        };
        lines.push(format!(
            "mesh.{}  {clean}/{} ok  —  {source}",
            section.name, section.declared,
        ));
        for (node, caps) in &section.grants {
            lines.push(format!("  {node}: {}", if caps.is_empty() { "—".to_string() } else { caps.join(",") }));
        }
        for row in &section.rows {
            lines.push(format!("  {}", render_row(row)));
        }
        if let Some(refusal) = &section.refusal {
            lines.push(format!(
                "  refused here: {refusal} — no node list: nothing in this mesh is decidable on this box"
            ));
        }
        for node in &section.nodes {
            lines.push(format!("  {}", render_node(node)));
        }
        if !section.self_declared {
            lines.push(format!(
                "  note: this box is not named in mesh.{} — if it should be, \
                 the declared key must be exactly `{local_name}`",
                section.name
            ));
        }
    }
    // The charter meshes a config never declared, and the re-keyed nodes every
    // charter row reports: a charter applied by an unattended poll has to be
    // visible afterwards, which is the whole reason these rows exist.
    let undeclared_charters: Vec<&CharterRow> = report.charters.iter().filter(|c| !c.declared).collect();
    if !report.charters.is_empty() {
        lines.push(String::new());
        for charter in &report.charters {
            lines.push(render_charter(charter));
        }
        if !undeclared_charters.is_empty() {
            lines.push(format!(
                "  ({} of them are not declared in config.toml — reported, never accused)",
                undeclared_charters.len()
            ));
        }
    }
    if !report.undeclared.is_empty() {
        lines.push(String::new());
        lines.push(render_undeclared(&report.undeclared));
    }
    lines.join("\n")
}

/// One nodelist row, human-rendered: the declared name, any nickname its key is
/// also recorded under, and the four facts this box can state about it.
fn render_node(node: &NodeRow) -> String {
    let mut line = node.name.clone();
    if let Some(nickname) = &node.nickname {
        line.push_str(&format!(" (recorded as `{nickname}`)"));
    }
    line.push_str(&format!(
        "  {}/{}  key: {}  liveness: {}",
        node.status, node.role, node.key_source, node.liveness
    ));
    if !node.gates.is_empty() {
        line.push_str(&format!("  gate into: {}", node.gates.join(",")));
    }
    line
}

/// One charter row, human-rendered: what it is, how far it is seen, who signed
/// it, what it re-keyed, and which local records its rule makes inert.
fn render_charter(charter: &CharterRow) -> String {
    let mut line = format!(
        "charter {} v{} — operator {} (trust: {}, {}){}",
        charter.mesh,
        if charter.in_force {
            charter.version.to_string()
        } else {
            "none in force".to_string()
        },
        charter.operator,
        charter.trust,
        if charter.trusted { "honoured" } else { "NOT HONOURED — resolve the operator key" },
        if charter.declared { "" } else { ", undeclared" },
    );
    if charter.nodes > 0 {
        line.push_str(&format!(", {} node(s)", charter.nodes));
    }
    for r in &charter.rekeyed {
        line.push_str(&format!(
            "\n  RE-KEYED {}: {} -> {} (v{}, {})",
            r.node, r.from, r.to, r.version, r.at
        ));
    }
    if !charter.inert.is_empty() {
        line.push_str(&format!(
            "\n  inert here (paired, but the charter's line is what this door reads): {}",
            charter.inert.join(", ")
        ));
    }
    if !charter.autogated.is_empty() {
        line.push_str(&format!(
            "\n  autogate rail here (unsigned send auto-delivers by address/token): {} — \
             this charter is what judges it (home mesh): the record verified and its key on \
             the line with `message` ⇒ delivered, otherwise held PENDING",
            charter.autogated.join(", ")
        ));
    }
    line
}

fn render_row(row: &MeshRow) -> String {
    match &row.class {
        DriftClass::Missing => {
            format!("{} — missing (declared, no node record by this name)", row.node)
        }
        DriftClass::Unverified => {
            format!("{} — unverified (node record exists, pairing never confirmed)", row.node)
        }
        DriftClass::ViaMismatch { declared, recorded: None } => format!(
            "{} — SEVERE via-mismatch: declared {declared}, recorded none \
             (a call dials the bare url directly — likely this box's own loopback)",
            row.node
        ),
        DriftClass::ViaMismatch { declared, recorded: Some(recorded) } => {
            format!("{} — via-mismatch: declared {declared}, recorded {recorded}", row.node)
        }
    }
}

fn render_undeclared(names: &[String]) -> String {
    format!("undeclared (paired, named in no mesh — reported, not accused): {}", names.join(", "))
}

/// `aoide mesh [--json]` — `Outcome::ok` whenever the config loads;
/// `Outcome::error` (never a panic, never a swallowed failure) when it does
/// not. See the module doc.
fn handle_mesh(_inv: &Invocation) -> Outcome {
    let cmd = "mesh";
    let loaded = match aoide_storage::config::load() {
        Ok(l) => l,
        Err(e) => {
            return Outcome::error(cmd, e.to_string())
                .with_data(json!({ "reason": "config-unreadable", "path": e.path().to_string_lossy() }));
        }
    };
    let nodes = aoide_storage::node_store::load_nodes();
    // As a NODE, not as a host: every mesh key and every node record's `name`
    // is `valid_node_name`-shaped, so this box must present the folded form to
    // be recognised as itself (`display::local_node_name`'s own doc has the
    // host-case argument). See `MeshSection::self_declared`'s field doc for the
    // defect this closes.
    let local_name = aoide_storage::display::local_node_name();
    let report = report(&loaded.config.mesh, &nodes, &local_name);
    let text = render_report(&report, &local_name);
    Outcome::ok(cmd, text).with_data(json!({ "report": report }))
}

// ────────────────────────────────────────────────────────────────────────
// `mesh pair` — the converge (task #135 P5)
// ────────────────────────────────────────────────────────────────────────

/// What a converge does with one declared node. Decided from [`drift`]'s
/// own classification and nothing else — a converge runs the SAME
/// comparison the read side does, never a second one.
#[derive(Debug, Clone, PartialEq)]
pub enum PlannedAction {
    /// [`DriftClass::Missing`] or [`DriftClass::Unverified`] — run the
    /// pairing ceremony through `hop`, the declared `ssh://` marker.
    Pair { hop: String },
    /// Reported, never attempted. `reason` is what the converge report
    /// carries and names the command that DOES fix it.
    Skip { reason: String },
}

/// One declared node and what the converge will do with it.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedNode {
    pub node: String,
    pub action: PlannedAction,
}

/// What a converge over one declared mesh will attempt, and what it will
/// not. Pure, and the whole of the selection ruling:
///
/// - Only [`MeshSection::rows`] are considered, so a node that already
///   matches is never touched and the local box (dropped inside [`drift`],
///   never here) is invisible.
/// - `missing` and `unverified` are paired. An `unverified` record is a
///   `node add` row the ceremony never confirmed; `upsert_paired_node`
///   updates it in place.
/// - **`via-mismatch` is skipped, always.** A converge NEVER modifies an
///   existing verified node: re-pairing rotates key material,
///   `commands::confirm_repair_if_verified` already gates that behind a
///   human y/N, and writing `via` outside a ceremony commit would make this
///   a second writer of a field `node_store::set_node_via` reserves to the
///   ceremony. The fix is a human re-pair (`aoide pair <name>`), started
///   from whichever box holds the wrong record — and it is that skip which
///   makes a converge idempotent by construction: run it twice and the
///   second run is all-`skipped`.
///
/// Order is [`MeshSection::rows`]' own, which is `Mesh::nodes`' `BTreeMap`
/// order — lexicographic by declared name, so the same declaration always
/// converges in the same sequence.
pub fn plan(section: &MeshSection, mesh: &Mesh) -> Vec<PlannedNode> {
    section
        .rows
        .iter()
        .map(|row| {
            let action = match &row.class {
                DriftClass::Missing | DriftClass::Unverified => match mesh.nodes.get(&row.node) {
                    Some(hop) => PlannedAction::Pair { hop: hop.clone() },
                    // Unreachable by construction — `drift` builds every row
                    // out of this same map — so this arm exists to keep the
                    // match total rather than to guard anything.
                    None => PlannedAction::Skip { reason: format!("no hop declared for `{}`", row.node) },
                },
                DriftClass::ViaMismatch { .. } => {
                    PlannedAction::Skip { reason: format!("via-mismatch; fix with `aoide pair {}`", row.node) }
                }
            };
            PlannedNode { node: row.node.clone(), action }
        })
        .collect()
}

/// How one selected node's converge attempt came out, in the four words
/// this report is allowed (the pairing tombstone slice's locked vocabulary
/// — completed / parked / UNREACHABLE / skipped; a fifth word is a spec
/// change, not an implementation detail).
///
/// [`Unreachable`](ConvergeOutcome::Unreachable) carries NO id, structurally:
/// `pairing::park_outbound` runs only after BOTH ceremony POSTs succeed, so
/// a request to an offline box parks nothing at all and there is no entry a
/// later `aoide pair <id>` could resume. A report that handed one back would
/// be inviting the operator to resume something that does not exist.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum ConvergeOutcome {
    /// The far operator typed the reply code and the node record is
    /// verified — `commit_outbound`'s own success.
    Completed,
    /// The request is parked and resumable under `id`: `--wait 0`, a wait
    /// that ran out, an approval with no terminal to type the reply code
    /// into, or a declined confirm. `detail` is the ceremony's own message
    /// and rides the human line beside the resume: the id alone cannot say
    /// whether THIS run parked it or an older entry survived a request that
    /// never landed ([`parked_id_for`]), and that difference is the whole
    /// news when a box has gone offline.
    Parked { id: String, detail: String },
    /// The ceremony left nothing parked and nothing committed. `detail` is
    /// the ceremony's own message — the human line shows THIS half, since
    /// here it is the only half worth acting on.
    Unreachable { detail: String },
    /// Never attempted. See [`plan`].
    Skipped { reason: String },
}

/// One node's converge result. `#[serde(flatten)]` for the same reason
/// [`MeshRow`] uses it — one flat `{"node": …, "outcome": …}` object per
/// row, never a nested tag.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ConvergeRow {
    pub node: String,
    #[serde(flatten)]
    pub outcome: ConvergeOutcome,
}

/// One converge, whole. What [`handle_mesh_pair`] renders and what `--json`
/// serializes under `data.report`.
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct ConvergeReport {
    pub mesh: String,
    pub rows: Vec<ConvergeRow>,
    /// Present only when the mesh declares `sameOperator = true`. Its own
    /// field, deliberately outside [`rows`](ConvergeReport::rows): the flag
    /// changes no node's status and no count, so folding it into a per-node
    /// result would misreport what happened.
    #[serde(rename = "sameOperatorNote", skip_serializing_if = "Option::is_none")]
    pub same_operator_note: Option<String>,
}

/// What a declared `sameOperator = true` gets: a sentence, and nothing else.
/// The flag is RETIRED (P-CHARTER: one operator is one charter signer, so
/// nodes sharing a charter need no pairing between them and a converge has
/// nothing to skip), so a converge runs the flag's `false` path exactly —
/// every node paired with both codes typed, by two people or by one person at
/// two screens — and says so rather than leaving the operator to wonder
/// whether a declared flag quietly did something. The sentence itself is
/// `config::Mesh::same_operator_note`'s, the one place it is written.

/// Assemble one converge's report. Pure, so the retirement above is a unit
/// test over in-memory values: the note's presence is the ONLY thing
/// `same_operator` changes about a report.
fn converge_report(mesh_name: &str, mesh: &Mesh, rows: Vec<ConvergeRow>) -> ConvergeReport {
    ConvergeReport {
        mesh: mesh_name.to_string(),
        rows,
        same_operator_note: mesh.same_operator_note(mesh_name),
    }
}

/// Fold one node's ceremony envelope into the locked vocabulary. Pure: both
/// facts it decides on are handed in — `out` is whatever
/// `commands::run_pair_request` returned, and `parked_id` is what
/// `pairing::list_outbound` says about that node AFTERWARD (the only way
/// this module ever reads parked state, so a new optional field on a parked
/// entry stays invisible to it).
///
/// Keyed on those two facts and nothing else — never on a `data.reason`
/// string, which would make the vocabulary a hostage to every future
/// wording change inside the ceremony:
///
/// - `confirmed: true` on an Ok envelope is `commit_outbound`'s own success
///   shape, and the only thing that means a node record was written.
/// - Otherwise, an entry parked under this node's name is exactly what
///   "resume it later" needs, whatever the envelope's status was — a
///   mistyped reply code leaves the request parked and is reported as such,
///   not as an unreachable box.
/// - Nothing committed and nothing parked is [`ConvergeOutcome::Unreachable`],
///   which by construction cannot carry an id.
pub fn classify(out: &Outcome, parked_id: Option<String>) -> ConvergeOutcome {
    let confirmed = out
        .data
        .as_ref()
        .and_then(|d| d.get("confirmed"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if out.status == aoide_protocol::output::Status::Ok && confirmed {
        return ConvergeOutcome::Completed;
    }
    match parked_id {
        Some(id) => ConvergeOutcome::Parked { id, detail: out.message.clone() },
        None => ConvergeOutcome::Unreachable { detail: out.message.clone() },
    }
}

/// Which declared mesh a converge runs over. A bare `mesh pair` with
/// exactly one declared mesh is unambiguous and takes it; anything else
/// names what it found rather than guessing.
fn resolve_section<'a>(cmd: &str, report: &'a MeshReport, arg: Option<&str>) -> Result<&'a MeshSection, Outcome> {
    let declared: Vec<&str> = report.sections.iter().map(|s| s.name.as_str()).collect();
    match arg {
        Some(name) => report.sections.iter().find(|s| s.name == name).ok_or_else(|| {
            Outcome::usage(
                cmd,
                format!(
                    "no `[mesh.{name}]` in config.toml — declared: {}",
                    if declared.is_empty() { "(none)".to_string() } else { declared.join(", ") }
                ),
            )
            .with_data(json!({ "reason": "unknown-mesh", "mesh": name, "declared": declared }))
        }),
        None if declared.is_empty() => Err(Outcome::error(
            cmd,
            "no mesh declared — add a `[mesh.<name>]` section to config.toml, then `aoide mesh` to see the drift this would converge",
        )
        .with_data(json!({ "reason": "no-mesh-declared" }))),
        None if declared.len() == 1 => Ok(&report.sections[0]),
        None => Err(Outcome::usage(
            cmd,
            format!("more than one mesh is declared — name the one to converge: {}", declared.join(", ")),
        )
        .with_data(json!({ "reason": "ambiguous-mesh", "declared": declared }))),
    }
}

/// The ceremony's post-request behaviour for a converge: `pair`'s OWN
/// `--wait`/`--yes` parse ([`crate::commands::pair_finish_from`] — one
/// parser and one taught error for a flag both commands spell the same),
/// with the grant taken from the MESH rather than from an `--allow` flag
/// `mesh pair` deliberately does not have. A declared grant is what a first
/// verification stamps; `None` (the mesh declares no override) falls
/// through to `commands::resolve_grant`, which reads `[pairing]
/// defaultGrant`. Never `Some(vec![])` for an absent declaration — the
/// empty list is the distinct, real "grant nothing" intent and must stay
/// distinguishable from "declared no override".
fn converge_finish(inv: &Invocation, mesh: &Mesh) -> Result<crate::commands::PairFinish, String> {
    let mut finish = crate::commands::pair_finish_from(inv)?;
    finish.grant = mesh.grant.clone();
    // `--yes` here buys the ONE pre-flight confirm below, never a code gate.
    // `commands::outbound_gate_from` reads `skip_confirm` ahead of the tty
    // test, so carrying it through would resolve every leg to
    // `CodeGate::Unavailable` and park the whole converge without committing
    // anything — N ids to type by hand, which is what this command exists to
    // replace. The converge replaces the N requests, not the N codes.
    finish.skip_confirm = false;
    Ok(finish)
}

/// How many of a plan's entries would actually send a request. Everything
/// that turns on "does this run do anything at all" — the pre-flight, the
/// detached-grant refusal — asks this one function, so the two can never
/// disagree about whether a converge is a no-op.
fn pairs_planned(plan: &[PlannedNode]) -> usize {
    plan.iter().filter(|p| matches!(p.action, PlannedAction::Pair { .. })).count()
}

/// The whole converge, laid out for the one pre-flight confirm: which
/// nodes, in what order, through which hops, at what grant, and how long
/// each will wait. Pure — the prompt is rendered here and only READ by the
/// confirm below.
fn render_preflight(mesh_name: &str, mesh: &Mesh, plan: &[PlannedNode], wait_secs: u64) -> String {
    let mut lines = vec![format!("converge mesh.{mesh_name}:")];
    for planned in plan {
        if let PlannedAction::Pair { hop } = &planned.action {
            lines.push(format!("  pair {} via {hop}", planned.node));
        }
    }
    for planned in plan {
        if let PlannedAction::Skip { reason } = &planned.action {
            lines.push(format!("  skip {} ({reason})", planned.node));
        }
    }
    lines.push(match &mesh.grant {
        Some(g) if g.is_empty() => format!("grant: nothing (mesh.{mesh_name} declares an empty grant)"),
        Some(g) => format!("grant: {} (declared by mesh.{mesh_name})", g.join(", ")),
        None => "grant: config.toml's [pairing] defaultGrant".to_string(),
    });
    lines.push(match wait_secs {
        0 => "each request is parked and returns immediately (--wait 0)".to_string(),
        n => format!("each far operator types the pairing code and reads a reply code back; up to {n}s per node"),
    });
    lines.join("\n")
}

/// ONE confirmation for the whole converge, before the loop (never one per
/// node — N prompts for a single decision is friction, not safety).
/// `--yes` skips it exactly as it skips `pair`'s own sweep proceed-prompt:
/// nothing is bypassed by that, because every far operator still types a
/// code, and the pairing codes remain the gate that actually secures each
/// pair. `Err` is the finished [`Outcome`] to return — a decline is an Ok
/// "nothing sent", not a failure.
fn confirm_preflight(
    cmd: &str,
    inv: &Invocation,
    mesh_name: &str,
    mesh: &Mesh,
    plan: &[PlannedNode],
    wait_secs: u64,
) -> Result<(), Outcome> {
    let to_pair = pairs_planned(plan);
    if to_pair == 0 || inv.flag_present("yes") {
        return Ok(());
    }
    let listing = render_preflight(mesh_name, mesh, plan, wait_secs);
    if !aoide_protocol::pick::interactive(inv.door) {
        return Err(Outcome::error(
            cmd,
            format!("{listing}\n— no terminal to confirm this on; re-run with --yes to proceed"),
        )
        .with_data(json!({ "reason": "no-preflight-confirm", "mesh": mesh_name, "toPair": to_pair })));
    }
    eprintln!("{listing}");
    match aoide_protocol::pick::confirm(&format!("proceed — pair {to_pair} node(s) in mesh.{mesh_name}?")) {
        Ok(true) => Ok(()),
        Ok(false) => Err(Outcome::ok(cmd, "not confirmed — nothing sent")
            .with_data(json!({ "confirmed": false, "mesh": mesh_name }))),
        Err(e) => Err(Outcome::error(cmd, e)),
    }
}

/// What `pairing::list_outbound` holds for `node` at this moment — the id of
/// the entry a later `aoide pair <id>` would resume, or `None` when nothing
/// is parked under that name. The ONE way this module reads parked state:
/// never the file, never a second index.
///
/// It answers about the NAME, not about this run: `park_outbound` dedups by
/// pubkey, so an entry a previous converge left behind survives a request
/// that never reached the box, and this returns that older id. The row is
/// still true — that id really is resumable — but it is not evidence this
/// run made progress, which is why [`render_converge_outcome`] prints a
/// parked row's `detail` beside its id rather than the id alone.
fn parked_id_for(node: &str) -> Option<String> {
    let now_epoch = aoide_storage::time::parse_iso_utc(&aoide_storage::time::now_iso_utc()).unwrap_or(0);
    aoide_storage::pairing::list_outbound(now_epoch).into_iter().find(|e| e.name == node).map(|e| e.id)
}

/// Run the ceremony for one selected node, through its declared hop. Every
/// line of ceremony logic lives in `commands::run_pair_request` — this
/// composes its arguments and nothing more. A duplicated poll loop is the
/// design error this whole split exists to prevent.
///
/// The dial url's HOST is irrelevant and deliberately loopback:
/// `commands::resolve_dial_url` discards a logical url's authority whenever
/// a `via` is set, rewriting the dial to the tunnel's own local end. So
/// `http://127.0.0.1:<default_a2a_port()>/` is both the correct logical url
/// and exactly the record shape a paired node already carries.
///
/// The MESH rides the request (P-CHARTER): a converge is the operator saying
/// "these nodes belong to this mesh", and the node's own operator is not
/// asked to guess it — the far end commits the same mesh by construction.
fn converge_one(
    cmd: &str,
    node: &str,
    hop: &str,
    mesh: &str,
    finish: &crate::commands::PairFinish,
) -> ConvergeOutcome {
    let via = match aoide_storage::tunnel::parse_via(hop) {
        Ok(v) => v,
        // Unreachable through `config::load` (`validate_mesh` runs the same
        // parser), so this is the total-match arm, not a second validation.
        Err(e) => return ConvergeOutcome::Skipped { reason: format!("declared hop `{hop}` does not parse: {e}") },
    };
    let dial_url = format!("http://127.0.0.1:{}/", crate::commands::default_a2a_port());
    let self_url = crate::commands::default_self_url();
    let self_via = crate::commands::default_self_via(&via.host);
    let out = crate::commands::run_pair_request(
        cmd,
        &dial_url,
        node,
        &self_url,
        self_via.as_deref(),
        Some(&via),
        Some(hop.to_string()),
        Some(mesh),
        finish,
    );
    classify(&out, parked_id_for(node))
}

/// The human-text rendering `aoide mesh pair`'s message carries — `--json`
/// serializes the same [`ConvergeReport`] under `data.report`, detail
/// included for every row.
fn render_converge(report: &ConvergeReport) -> String {
    let mut lines = Vec::new();
    if report.rows.is_empty() {
        lines.push(format!("mesh.{} — every declared node is already paired at its declared hop", report.mesh));
    } else {
        let width = report.rows.iter().map(|r| r.node.chars().count()).max().unwrap_or(0);
        for row in &report.rows {
            lines.push(format!("  {:width$} — {}", row.node, render_converge_outcome(&row.outcome)));
        }
    }
    if let Some(note) = &report.same_operator_note {
        lines.push(String::new());
        lines.push(format!("  {note}"));
    }
    lines.join("\n")
}

/// One row's outcome word plus the half of its detail that is actionable —
/// the resume for a parked request, the failure for an unreachable one, the
/// fix for a skip. The other half is never lost: `--json` carries every
/// field of [`ConvergeOutcome`] verbatim.
fn render_converge_outcome(outcome: &ConvergeOutcome) -> String {
    match outcome {
        ConvergeOutcome::Completed => "completed   (far operator typed the reply code)".to_string(),
        ConvergeOutcome::Parked { id, detail } => {
            format!("parked      (resume with `aoide pair {id}` — {detail})")
        }
        ConvergeOutcome::Unreachable { detail } => format!("UNREACHABLE ({detail})"),
        ConvergeOutcome::Skipped { reason } => format!("skipped     ({reason})"),
    }
}

/// `aoide mesh pair [<mesh>] [--wait N] [--yes] [--json]` — make a declared
/// mesh true, one ordinary pairwise ceremony at a time. See the module doc.
fn handle_mesh_pair(inv: &Invocation) -> Outcome {
    let cmd = "mesh.pair";
    const USAGE: &str = "usage: aoide mesh pair [<mesh>] [--wait SECS] [--yes] [--json] — pairs every declared node this box has no verified record of; the mesh may be omitted when exactly one is declared";
    if inv.args.len() > 1 {
        return Outcome::usage(cmd, USAGE);
    }
    let loaded = match aoide_storage::config::load() {
        Ok(l) => l,
        Err(e) => {
            return Outcome::error(cmd, e.to_string())
                .with_data(json!({ "reason": "config-unreadable", "path": e.path().to_string_lossy() }));
        }
    };
    let nodes = aoide_storage::node_store::load_nodes();
    // As a NODE (folded), for the same reason `handle_mesh`'s own call site is:
    // the mesh's declared keys are `valid_node_name`-shaped, so the raw host
    // name can never match one.
    let local_name = aoide_storage::display::local_node_name();
    let report = drift(&loaded.config.mesh, &nodes, &local_name);

    let arg = inv.args.first().map(|s| s.trim()).filter(|s| !s.is_empty());
    let section = match resolve_section(cmd, &report, arg) {
        Ok(s) => s,
        Err(out) => return out,
    };
    let Some(mesh) = loaded.config.mesh.get(&section.name) else {
        // `drift` builds a section per declared mesh, keyed by that same
        // map — the total-match arm, not a guard.
        return Outcome::error(cmd, format!("mesh.{} vanished between the read and the converge", section.name));
    };

    let plan = plan(section, mesh);
    let finish = match converge_finish(inv, mesh) {
        Ok(f) => f,
        Err(e) => return Outcome::usage(cmd, format!("{USAGE} — {e}")),
    };
    // The same refusal `pair --allow --wait 0` already gives, by the SAME
    // rule — `commands::refuse_detached_grant` is asked, so if `pair` ever
    // changes when a detached grant is refused this follows without a
    // second copy of the condition. Only the WORDING is replaced: that
    // message names `--allow`, and here the grant came from the
    // declaration, not from a flag anybody typed.
    //
    // Asked only when something would actually be sent, the same condition
    // the pre-flight uses. A converged mesh plans no request, so there is no
    // grant to detach and nothing to refuse — an all-`skipped` run stays Ok
    // whatever flags it carries, which is what makes the second run over a
    // converged mesh a usable scripted check.
    if pairs_planned(&plan) > 0 && crate::commands::refuse_detached_grant(cmd, &finish).is_some() {
        return Outcome::usage(
            cmd,
            format!(
                "mesh.{} declares a grant, and `--wait 0` parks every request before anything commits — \
                 a grant is never persisted on a parked entry, so this one would be silently dropped. \
                 Drop `--wait 0` so each pair finishes while its grant is still in hand.",
                section.name
            ),
        )
        .with_data(json!({ "reason": "detached-grant", "mesh": section.name }));
    }
    if let Err(out) = confirm_preflight(cmd, inv, &section.name, mesh, &plan, finish.wait_secs) {
        return out;
    }

    let mut rows = Vec::with_capacity(plan.len());
    for planned in &plan {
        let outcome = match &planned.action {
            PlannedAction::Skip { reason } => ConvergeOutcome::Skipped { reason: reason.clone() },
            PlannedAction::Pair { hop } => converge_one(cmd, &planned.node, hop, &section.name, &finish),
        };
        rows.push(ConvergeRow { node: planned.node.clone(), outcome });
    }

    let completed = rows.iter().any(|r| r.outcome == ConvergeOutcome::Completed);
    let converged = converge_report(&section.name, mesh, rows);
    let text = render_converge(&converged);
    let out = Outcome::ok(cmd, text).with_data(json!({ "report": converged }));
    if completed {
        out.changed(vec![aoide_storage::node_store::nodes_path().to_string_lossy().into_owned()])
    } else {
        out
    }
}

/// `mesh` then `mesh pair`, appended newest (Registry discipline,
/// `pkgs/aoide/crates/AGENTS.md`) into `cli`'s `commands::all()` — LAST,
/// after every other `register*` call.
///
/// **Neither is door-gated, and `mesh pair` is not gated for the same
/// reason `pair` is not.** Over any door but the CLI,
/// `aoide_protocol::pick::interactive` is false, so both legs' `CodeGate`
/// resolves to `Unavailable` (`commands::approve_inbound_leg`,
/// `commands::outbound_gate_from`): a remote caller can START requests and
/// can never COMMIT one. A converge is a loop over that same ceremony and
/// inherits that answer whole, so it needs no gate of its own — the
/// convention already answers. This says nothing about whether a converge
/// that could satisfy a far side's code mechanically would need one; no
/// such path exists, and if one is ever ruled in it brings its own gate and
/// its own reason.
pub fn register(r: &mut Registry) {
    r.insert(cmd!(
        path: ["mesh"],
        summary: "Compare every declared [mesh.<name>] in config.toml against the live node registry and report where they diverge (missing/unverified/via-mismatch), plus any paired node named in no mesh. Drift is never itself a failure; a config that fails to load is.",
        args: [],
        flags: [],
        gated: false,
        implemented: true,
        handler: handle_mesh,
        examples: ["mesh", "mesh --json"],
    ));
    r.insert(cmd!(
        path: ["mesh", "pair"],
        summary: "Converge a declared [mesh.<name>]: run the ordinary pairing ceremony against every declared node this box has no verified record of (missing or unverified), in declared-name order, through each one's declared ssh hop, stamping the mesh's own grant. A verified node is NEVER modified — a via-mismatch is reported as skipped and fixed by a human re-pair — so a second run is all-skipped. One pre-flight confirm for the whole converge (--yes skips it); every far operator still types a pairing code and reads a reply code back.",
        args: [arg!("mesh", "string", false, "Which declared mesh to converge. Omitted: the one declared mesh, when exactly one is declared.")],
        flags: [
            flag!("wait", "int", "Seconds to block per node for the far operator (default 600). --wait 0 parks every request and returns immediately, to be finished later with `aoide pair <id>` or `aoide pair watch`. Refused when the mesh declares a grant and there is anything to pair: a parked entry carries no grant, so the declared one would be silently dropped."),
            flag!("yes", "bool", "Skip the pre-flight confirm. Never a bypass of the pairing codes: each node's commit still needs a typed code on both sides."),
        ],
        gated: false,
        implemented: true,
        handler: handle_mesh_pair,
        examples: ["mesh pair", "mesh pair home", "mesh pair home --wait 0", "mesh pair home --yes --json"],
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_protocol::Door;

    fn mesh(nodes: &[(&str, &str)]) -> Mesh {
        Mesh {
            grant: None,
            same_operator: false,
            nodes: nodes.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            operator: None,
            ..Default::default()
        }
    }

    fn node(name: &str, verified: bool, via: Option<&str>) -> Node {
        Node {
            name: name.to_string(),
            url: format!("https://{name}.example/"),
            autogate: false,
            token_file: None,
            bearer_secret: None,
            hub: false,
            pubkey: None,
            verified,
            grants: aoide_storage::node_store::Grants::new(),
            narrowed: aoide_storage::node_store::Grants::new(),
            via: via.map(str::to_string),
            added_at: String::new(),
        }
    }

    fn cli_inv(path: &[&str]) -> Invocation {
        Invocation {
            path: path.iter().map(|s| s.to_string()).collect(),
            args: Vec::new(),
            flags: BTreeMap::new(),
            door: Door::Cli,
        }
    }

    /// Sandboxes `handle_mesh`'s two real reads — `config::load()`
    /// (`AOIDE_ROOT`/`AOIDE_CONFIG`) and `node_store::load_nodes()`
    /// (`AOIDE_STATE_DIR`) — at one fresh scratch dir, so this module's
    /// handler test never reads the developer's own `~/.aoide` (mirrors
    /// `commands::tests::with_node_state`). `EnvSaver` restores the three
    /// vars on drop even if `f` panics; the scratch dir itself is best-effort
    /// removed after `f` returns.
    fn with_config_root<T>(tag: &str, f: impl FnOnce(&std::path::Path) -> T) -> T {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _saver = aoide_test_support::EnvSaver::capture(&["AOIDE_ROOT", "AOIDE_STATE_DIR", "AOIDE_CONFIG"]);
        let dir = aoide_test_support::unique_tmp(&format!("mesh-{tag}"));
        std::env::set_var("AOIDE_ROOT", &dir);
        std::env::set_var("AOIDE_STATE_DIR", &dir);
        std::env::remove_var("AOIDE_CONFIG");
        let out = f(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        out
    }

    // ── drift: this box as a NODE ────────────────────────────────────────────

    /// The box is recognised as ITSELF — as a NODE — when this host's name
    /// differs in case from the grammar-lowercase key a mesh must declare it
    /// under. Driven through the SAME expression both production call sites use
    /// (`display::local_node_name()`), so the fold is pinned at that boundary,
    /// and the second half shows the defect the fold corrects: with the raw,
    /// case-preserving host name, `self_declared` is false and this box's own
    /// declared entry reads as an unpaired row — which is exactly what a host
    /// named `ThinkChiyo` reported against a mesh declaring `thinkchiyo`,
    /// because a mesh key is forced through `node_store::valid_node_name`.
    #[test]
    fn a_mixed_case_host_name_is_recognised_as_self_under_its_node_name() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _saver = aoide_test_support::EnvSaver::capture(&["AOIDE_A2A_NODE_NAME"]);
        std::env::set_var("AOIDE_A2A_NODE_NAME", "ThinkChiyo");

        let meshes = BTreeMap::from([("home".to_string(), mesh(&[("thinkchiyo", "ssh://k@h")]))]);
        let report = drift(&meshes, &[], &aoide_storage::display::local_node_name());
        assert!(report.sections[0].self_declared, "this box IS the declared node: {report:?}");
        assert!(
            report.sections[0].rows.is_empty(),
            "and its own entry is never an unpaired row: {report:?}"
        );

        let raw = drift(&meshes, &[], &aoide_storage::display::local_host_name());
        assert!(
            !raw.sections[0].self_declared,
            "the premise the fix corrects: the raw spelling matches no declared key: {raw:?}"
        );
    }

    /// **The charter rows** (P-CHARTER, the surfaces slice): a mesh with a
    /// charter in force is reported as `source: charter` with its version, the
    /// operator's fingerprinted key and its trust source — and a paired record
    /// this box also holds in that mesh is reported INERT, because the
    /// charter's line, not the record's own grant, is what the door reads.
    /// The row exists whether or not the config declares the mesh.
    #[test]
    fn a_charter_in_force_reports_its_source_operator_and_inert_records() {
        with_config_root("charter-rows", |dir| {
            // The charter: this machine roots `home`, lists its own node line,
            // signs — and `sign` applies it here, so it is in force.
            let init = aoide_storage::charter::init("home").unwrap();
            let line = aoide_storage::charter::node_line().unwrap();
            let src = format!("mesh = \"home\"\nversion = 0\nrelays = []\n\n[nodes]\n{line}\n");
            std::fs::write(aoide_storage::charter::source_path("home"), &src).unwrap();
            aoide_storage::charter::sign("home", None).unwrap();

            // A verified record in that mesh which the charter does not list:
            // its own grant entry is not what the door reads there.
            let mut peer = node("peerbox", true, Some("ssh://k@h"));
            peer.pubkey = Some("aa".repeat(32));
            peer.grants = aoide_storage::node_store::grants_in("home", &["message"]);
            peer.autogate = true;
            aoide_storage::node_store::save_nodes(&[peer]).unwrap();

            // A SECOND charter mesh, not this box's home: its row exists (F2
            // reports a shaped mesh) but must not claim the rail, because the
            // rail carries no signed mesh and is judged by home alone.
            std::fs::create_dir_all(aoide_storage::charter::mesh_state_dir("away")).unwrap();
            std::fs::write(aoide_storage::charter::in_force_path("away"), "mesh = \"away\"\nversion = 1\n")
                .unwrap();

            // Declared, so the section's own `source` is exercised too.
            std::fs::write(
                std::path::Path::new(dir).join("config.toml"),
                "[mesh.home]\nnodes = { peerbox = \"ssh://k@h\" }\n",
            )
            .unwrap();

            let loaded = aoide_storage::config::load().unwrap();
            let nodes = aoide_storage::node_store::load_nodes();
            let report = report(&loaded.config.mesh, &nodes, "this-box");

            assert_eq!(
                report.charters.len(),
                2,
                "the home charter and the away fixture's own, each a row: {:?}",
                report.charters
            );
            let row = report.charters.iter().find(|c| c.mesh == "home").expect("the home row");
            assert_eq!(row.mesh, "home");
            assert_eq!(row.version, 1);
            assert!(row.declared, "the mesh is also declared in config.toml");
            assert!(row.trusted, "this machine rooted it, so the key is decidable");
            assert_eq!(row.operator, aoide_storage::charter::fingerprint_of_key(&init.operator));
            assert_eq!(row.operator_key, init.operator);
            assert_eq!(row.high_water, 1, "the version just applied is the mark");
            assert!(row.rekeyed.is_empty(), "nothing was re-keyed by v1");
            assert_eq!(row.inert, vec!["peerbox".to_string()], "the record's own grant is not the answer here");
            assert_eq!(
                row.autogated,
                vec!["peerbox".to_string()],
                "but the record is inert as a GRANT, not as a whole: its `autogate` rail still answers, and home is the mesh that judges it"
            );
            let text = render_charter(row);
            assert!(
                text.contains("autogate rail here") && text.contains("held PENDING"),
                "the surface says the rail exists and what this charter does with it: {text}"
            );

            // The rail is judged by HOME and nobody else: the away mesh's own
            // row must not claim a say over it.
            let away = report.charters.iter().find(|c| c.mesh == "away").expect("the shaped mesh is reported too");
            assert!(
                away.autogated.is_empty(),
                "a charter that is not this box's home judges no rail: {away:?}"
            );
            assert!(!render_charter(away).contains("autogate rail here"), "and says nothing about one");

            assert_eq!(
                report.sections[0].source,
                MeshSource::Charter,
                "and the declared section says which source answers there"
            );

            // The pure comparison alone never claims a charter: `drift` knows
            // nothing about them, and `report` is the one place that flips it.
            let pure = drift(&loaded.config.mesh, &nodes, "this-box");
            assert!(pure.charters.is_empty());
            assert_eq!(pure.sections[0].source, MeshSource::Paired);
        });
    }

    // ── drift: matches produce nothing ──────────────────────────────────────
    #[test]
    fn a_verified_node_whose_via_matches_the_declared_hop_produces_no_row() {
        let meshes = BTreeMap::from([("home".to_string(), mesh(&[("sakaki", "ssh://khoa@h")]))]);
        let nodes = vec![node("sakaki", true, Some("ssh://khoa@h"))];
        let report = drift(&meshes, &nodes, "this-box");
        assert_eq!(report.sections[0].rows, Vec::new());
        assert!(report.undeclared.is_empty());
    }

    // ── drift: the three classes ────────────────────────────────────────────

    #[test]
    fn a_declared_node_with_no_record_at_all_is_missing() {
        let meshes = BTreeMap::from([("home".to_string(), mesh(&[("sakaki", "ssh://khoa@h")]))]);
        let report = drift(&meshes, &[], "this-box");
        assert_eq!(report.sections[0].rows, vec![MeshRow { node: "sakaki".into(), class: DriftClass::Missing }]);
    }

    #[test]
    fn a_declared_node_that_is_not_yet_verified_is_unverified_even_with_a_matching_via() {
        let meshes = BTreeMap::from([("home".to_string(), mesh(&[("sakaki", "ssh://khoa@h")]))]);
        let nodes = vec![node("sakaki", false, Some("ssh://khoa@h"))];
        let report = drift(&meshes, &nodes, "this-box");
        assert_eq!(report.sections[0].rows, vec![MeshRow { node: "sakaki".into(), class: DriftClass::Unverified }]);
    }

    #[test]
    fn a_verified_node_with_a_different_via_is_via_mismatch() {
        let meshes = BTreeMap::from([("home".to_string(), mesh(&[("sakaki", "ssh://khoa@h")]))]);
        let nodes = vec![node("sakaki", true, Some("ssh://khoa@other"))];
        let report = drift(&meshes, &nodes, "this-box");
        assert_eq!(
            report.sections[0].rows,
            vec![MeshRow {
                node: "sakaki".into(),
                class: DriftClass::ViaMismatch {
                    declared: "ssh://khoa@h".into(),
                    recorded: Some("ssh://khoa@other".into())
                }
            }]
        );
    }

    #[test]
    fn a_verified_node_with_no_recorded_via_is_via_mismatch_with_recorded_none() {
        let meshes = BTreeMap::from([("home".to_string(), mesh(&[("chiyo", "ssh://khoa@h")]))]);
        let nodes = vec![node("chiyo", true, None)];
        let report = drift(&meshes, &nodes, "this-box");
        assert_eq!(
            report.sections[0].rows,
            vec![MeshRow {
                node: "chiyo".into(),
                class: DriftClass::ViaMismatch { declared: "ssh://khoa@h".into(), recorded: None }
            }]
        );
        // The severe case renders distinctly.
        let text = render_row(&report.sections[0].rows[0]);
        assert!(text.contains("SEVERE"), "{text}");
    }

    // ── undeclared: reported, never accused ─────────────────────────────────

    #[test]
    fn a_verified_node_named_in_no_mesh_is_undeclared_not_a_drift_row() {
        let meshes = BTreeMap::from([("home".to_string(), mesh(&[]))]);
        let nodes = vec![node("osaka", true, None)];
        let report = drift(&meshes, &nodes, "this-box");
        assert_eq!(report.sections[0].rows, Vec::new());
        assert_eq!(report.undeclared, vec!["osaka".to_string()]);
    }

    #[test]
    fn an_unverified_node_named_in_no_mesh_is_not_undeclared() {
        let meshes = BTreeMap::new();
        let nodes = vec![node("osaka", false, None)];
        let report = drift(&meshes, &nodes, "this-box");
        assert!(report.undeclared.is_empty(), "an unpaired node has nothing to declare");
    }

    #[test]
    fn undeclared_is_sorted_regardless_of_input_order() {
        let meshes = BTreeMap::new();
        let nodes = vec![node("zeta", true, None), node("alpha", true, None), node("mu", true, None)];
        let report = drift(&meshes, &nodes, "this-box");
        assert_eq!(report.undeclared, vec!["alpha".to_string(), "mu".to_string(), "zeta".to_string()]);
    }

    // ── allows divergence is not drift ──────────────────────────────────────

    #[test]
    fn grants_divergent_from_grant_produce_no_row_and_are_never_inspected() {
        let mut m = mesh(&[("sakaki", "ssh://khoa@h")]);
        m.grant = Some(vec!["read".to_string()]);
        let meshes = BTreeMap::from([("home".to_string(), m)]);
        let mut p = node("sakaki", true, Some("ssh://khoa@h"));
        p.grants = aoide_storage::node_store::grants_in("home", &["spawn"]); // deliberately NOT "read" — still no row
        let report = drift(&meshes, &[p], "this-box");
        assert_eq!(report.sections[0].rows, Vec::new());
    }

    // ── self-skip ────────────────────────────────────────────────────────────

    #[test]
    fn the_local_host_named_in_its_own_mesh_is_skipped_silently() {
        let meshes =
            BTreeMap::from([("home".to_string(), mesh(&[("this-box", "ssh://khoa@self"), ("sakaki", "ssh://khoa@h")]))]);
        let nodes = vec![node("sakaki", true, Some("ssh://khoa@h"))];
        let report = drift(&meshes, &nodes, "this-box");
        assert_eq!(report.sections[0].rows, Vec::new(), "this-box's own entry must not surface as missing");
        assert!(report.sections[0].self_declared, "this-box's own key IS present in mesh.nodes");
        assert_eq!(
            report.sections[0].declared, 1,
            "declared counts what was actually COMPARED — the self-entry is excluded, same as rows"
        );
    }

    #[test]
    fn a_mesh_missing_this_boxs_own_key_is_not_self_declared_and_its_other_node_still_compares() {
        // Reproduces the false-accusation gap: the operator's config names
        // this box "yomi" but `local_host_name()` actually returns
        // "yomi-strix" (an FQDN/typo/rename mismatch). Nothing here special-
        // cases that — `sakaki` still compares normally — but the section
        // is flagged as not self-declared so the gap is discoverable.
        let meshes = BTreeMap::from([("home".to_string(), mesh(&[("sakaki", "ssh://khoa@h")]))]);
        let nodes = vec![node("sakaki", true, Some("ssh://khoa@h"))];
        let report = drift(&meshes, &nodes, "yomi-strix");
        assert!(!report.sections[0].self_declared);
        assert_eq!(report.sections[0].rows, Vec::new(), "sakaki still compares cleanly regardless");
    }

    #[test]
    fn render_report_notes_a_mesh_that_does_not_self_declare_this_box() {
        let meshes = BTreeMap::from([("home".to_string(), mesh(&[("sakaki", "ssh://khoa@h")]))]);
        let nodes = vec![node("sakaki", true, Some("ssh://khoa@h"))];
        let report = drift(&meshes, &nodes, "yomi-strix");
        let text = render_report(&report, "yomi-strix");
        assert!(text.contains("not named in mesh.home"), "{text}");
        assert!(text.contains("yomi-strix"), "{text}");
    }

    #[test]
    fn render_report_is_silent_when_this_box_is_self_declared() {
        let meshes = BTreeMap::from([("home".to_string(), mesh(&[("this-box", "ssh://khoa@self")]))]);
        let report = drift(&meshes, &[], "this-box");
        let text = render_report(&report, "this-box");
        assert!(!text.contains("note:"), "{text}");
    }

    // ── determinism ──────────────────────────────────────────────────────────

    #[test]
    fn two_shuffled_node_orderings_render_an_identical_report() {
        let meshes = BTreeMap::from([(
            "home".to_string(),
            mesh(&[("sakaki", "ssh://khoa@h1"), ("chiyo", "ssh://khoa@h2"), ("osaka", "ssh://khoa@h3")]),
        )]);
        let a = vec![
            node("sakaki", true, Some("ssh://khoa@h1")),
            node("chiyo", false, None),
            node("osaka", true, Some("ssh://khoa@wrong")),
        ];
        let mut b = a.clone();
        b.reverse();
        assert_eq!(drift(&meshes, &a, "this-box"), drift(&meshes, &b, "this-box"));
    }

    #[test]
    fn bare_mesh_with_nothing_declared_and_nothing_paired_still_renders() {
        let report = drift(&BTreeMap::new(), &[], "this-box");
        assert_eq!(
            render_report(&report, "this-box"),
            "no mesh declared — see `aoide config` for [mesh.<name>]"
        );
    }

    // ── handler wiring ──────────────────────────────────────────────────────

    #[test]
    fn handle_mesh_is_ok_with_a_report_when_config_loads() {
        // A sandboxed root with no config.toml at all still loads (an absent
        // file is UNMANAGED-empty, not an error) — status is Ok and the
        // envelope carries a `report`, never a `reason`.
        let out = with_config_root("ok", |_dir| handle_mesh(&cli_inv(&["mesh"])));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "{out:?}");
        let data = out.data.as_ref().expect("ok envelope carries data");
        assert!(data.get("report").is_some(), "{data:?}");
        assert!(data.get("reason").is_none(), "{data:?}");
    }

    #[test]
    fn handle_mesh_is_error_when_config_fails_to_load() {
        // `grant` values are validated against the closed capability
        // vocabulary at load time — "root" is not a member, so this file
        // never parses into a `Loaded`. The module doc's one carve-out:
        // drift itself never fails, but a config that won't load does.
        let out = with_config_root("error", |dir| {
            std::fs::write(dir.join("config.toml"), "[mesh.home]\ngrant = [\"root\"]\n").unwrap();
            handle_mesh(&cli_inv(&["mesh"]))
        });
        assert_eq!(out.status, aoide_protocol::output::Status::Error, "{out:?}");
        let data = out.data.as_ref().expect("error envelope carries data");
        assert_eq!(data.get("reason").and_then(|v| v.as_str()), Some("config-unreadable"), "{data:?}");
        assert!(data.get("report").is_none(), "{data:?}");
    }

    // ── mesh pair: selection ────────────────────────────────────────────────

    /// A mesh whose four declared nodes cover every drift class plus this
    /// box itself — one fixture the selection rulings are all read off.
    fn converge_fixture() -> (BTreeMap<String, Mesh>, Vec<Node>) {
        let mut m = mesh(&[
            ("this-box", "ssh://khoa@self"),
            ("sakaki", "ssh://khoa@h1"),
            ("chiyo", "ssh://khoa@h2"),
            ("osaka", "ssh://khoa@h3"),
            ("yuzu", "ssh://khoa@h4"),
        ]);
        m.grant = None;
        let nodes = vec![
            // sakaki: no record at all -> missing
            node("chiyo", false, None),                    // unverified
            node("osaka", true, Some("ssh://khoa@h3")),     // matches -> no row
            node("yuzu", true, Some("ssh://khoa@wrong")),   // via-mismatch
        ];
        (BTreeMap::from([("home".to_string(), m)]), nodes)
    }

    #[test]
    fn a_converge_selects_exactly_the_missing_and_unverified_nodes_in_declared_order() {
        let (meshes, nodes) = converge_fixture();
        let report = drift(&meshes, &nodes, "this-box");
        let plan = plan(&report.sections[0], &meshes["home"]);
        let paired: Vec<&str> = plan
            .iter()
            .filter(|p| matches!(p.action, PlannedAction::Pair { .. }))
            .map(|p| p.node.as_str())
            .collect();
        // chiyo (unverified) before sakaki (missing) — lexicographic by
        // declared name, never by drift class.
        assert_eq!(paired, vec!["chiyo", "sakaki"]);
    }

    #[test]
    fn a_converge_never_touches_this_box_or_a_node_that_already_matches() {
        let (meshes, nodes) = converge_fixture();
        let report = drift(&meshes, &nodes, "this-box");
        let plan = plan(&report.sections[0], &meshes["home"]);
        let named: Vec<&str> = plan.iter().map(|p| p.node.as_str()).collect();
        assert!(!named.contains(&"this-box"), "the local box is invisible, not a skipped row: {named:?}");
        assert!(!named.contains(&"osaka"), "an already-matching node is not a row at all: {named:?}");
    }

    #[test]
    fn a_via_mismatch_is_skipped_and_names_the_human_re_pair_as_the_fix() {
        let (meshes, nodes) = converge_fixture();
        let report = drift(&meshes, &nodes, "this-box");
        let plan = plan(&report.sections[0], &meshes["home"]);
        let yuzu = plan.iter().find(|p| p.node == "yuzu").expect("yuzu is planned");
        match &yuzu.action {
            PlannedAction::Skip { reason } => assert_eq!(reason, "via-mismatch; fix with `aoide pair yuzu`"),
            other => panic!("a verified node is never re-paired by a converge: {other:?}"),
        }
    }

    #[test]
    fn a_second_converge_over_a_converged_mesh_is_all_skipped() {
        // §2.2's payoff, stated as a test: once the missing/unverified nodes
        // are paired at their declared hops, the only rows left are the
        // via-mismatches, and every one of them is a skip.
        let (meshes, _) = converge_fixture();
        let nodes = vec![
            node("sakaki", true, Some("ssh://khoa@h1")),
            node("chiyo", true, Some("ssh://khoa@h2")),
            node("osaka", true, Some("ssh://khoa@h3")),
            node("yuzu", true, Some("ssh://khoa@wrong")),
        ];
        let report = drift(&meshes, &nodes, "this-box");
        let plan = plan(&report.sections[0], &meshes["home"]);
        assert!(
            plan.iter().all(|p| matches!(p.action, PlannedAction::Skip { .. })),
            "a re-run converges nothing: {plan:?}"
        );
    }

    // ── mesh pair: the outcome fold ─────────────────────────────────────────

    #[test]
    fn a_committed_ceremony_folds_to_completed() {
        let out = Outcome::ok("pair", "paired with `sakaki` — verified, granted read")
            .with_data(json!({ "confirmed": true, "node": "sakaki" }));
        assert_eq!(classify(&out, None), ConvergeOutcome::Completed);
    }

    #[test]
    fn a_ceremony_that_left_an_entry_parked_folds_to_parked_with_its_resumable_id() {
        let out = Outcome::ok("pair", "no answer from `osaka` within 600s")
            .with_data(json!({ "reason": "wait-timeout", "id": "4f2a91bc" }));
        assert_eq!(
            classify(&out, Some("4f2a91bc".to_string())),
            ConvergeOutcome::Parked { id: "4f2a91bc".into(), detail: "no answer from `osaka` within 600s".into() }
        );
    }

    #[test]
    fn a_mistyped_reply_code_is_parked_not_unreachable_because_the_entry_survives() {
        // An Error envelope whose entry is still parked is resumable, and
        // the fold says so — it keys on what the parked store holds, never
        // on the envelope's `data.reason` wording.
        let out = Outcome::error("pair", "reply-code mismatch — try 1 of 3")
            .with_data(json!({ "reason": "code-mismatch", "id": "4f2a91bc", "tries": 1 }));
        assert!(matches!(classify(&out, Some("4f2a91bc".to_string())), ConvergeOutcome::Parked { .. }));
    }

    #[test]
    fn an_offline_box_folds_to_unreachable_and_carries_no_resumable_id() {
        // `park_outbound` runs only after both ceremony POSTs succeed, so a
        // request to an offline box parks nothing — the variant has no id
        // field at all, so no report can imply a resume that would fail.
        let out = Outcome::error("pair", "sending the pairing request to http://127.0.0.1:8710/: connection refused")
            .with_data(json!({ "reason": "fetch-failed", "url": "http://127.0.0.1:8710/" }));
        let outcome = classify(&out, None);
        assert!(matches!(outcome, ConvergeOutcome::Unreachable { .. }), "{outcome:?}");
        let json = serde_json::to_value(&outcome).unwrap();
        assert_eq!(json.get("outcome").and_then(|v| v.as_str()), Some("unreachable"));
        assert!(json.get("id").is_none(), "UNREACHABLE never carries a resumable id: {json}");
    }

    #[test]
    fn an_id_on_an_envelope_the_ceremony_never_parked_is_still_unreachable() {
        // A reveal that fails carries the APPROVER's id, but `park_outbound`
        // has not run yet — nothing on this side is resumable, and the fold
        // asks the parked store rather than trusting the envelope's `id`.
        let out = Outcome::error("pair", "revealing the nonce to http://127.0.0.1:8710/: HTTP 500")
            .with_data(json!({ "reason": "reveal-http-error", "id": "4f2a91bc" }));
        assert!(matches!(classify(&out, None), ConvergeOutcome::Unreachable { .. }));
    }

    // ── mesh pair: sameOperator is a note and nothing else ──────────────────

    fn one_row() -> Vec<ConvergeRow> {
        vec![ConvergeRow { node: "sakaki".into(), outcome: ConvergeOutcome::Completed }]
    }

    #[test]
    fn same_operator_true_adds_a_note_and_changes_no_row_and_no_count() {
        let plain = mesh(&[("sakaki", "ssh://khoa@h")]);
        let mut claimed = plain.clone();
        claimed.same_operator = true;
        let a = converge_report("home", &plain, one_row());
        let b = converge_report("home", &claimed, one_row());
        assert_eq!(a.rows, b.rows, "the flag changes no per-node result");
        assert_eq!(a.rows.len(), b.rows.len());
        assert!(a.same_operator_note.is_none());
        let note = b.same_operator_note.as_deref().expect("a declared sameOperator is noted");
        assert!(note.starts_with("mesh.home declares sameOperator = true"), "{note}");
        assert!(note.contains("RETIRED"), "{note}");
        assert!(note.contains("charter"), "{note}");
    }

    #[test]
    fn the_same_operator_note_is_its_own_json_field_never_a_per_node_result() {
        let mut claimed = mesh(&[("sakaki", "ssh://khoa@h")]);
        claimed.same_operator = true;
        let json = serde_json::to_value(converge_report("home", &claimed, one_row())).unwrap();
        assert!(json.get("sameOperatorNote").is_some(), "{json}");
        let row = &json["rows"][0];
        assert_eq!(row.get("outcome").and_then(|v| v.as_str()), Some("completed"));
        assert!(row.get("sameOperatorNote").is_none(), "never folded into a row: {row}");
    }

    #[test]
    fn same_operator_true_selects_exactly_what_same_operator_false_selects() {
        let (mut meshes, nodes) = converge_fixture();
        let report_false = drift(&meshes, &nodes, "this-box");
        let plan_false = plan(&report_false.sections[0], &meshes["home"]);
        meshes.get_mut("home").unwrap().same_operator = true;
        let report_true = drift(&meshes, &nodes, "this-box");
        let plan_true = plan(&report_true.sections[0], &meshes["home"]);
        assert_eq!(plan_false, plan_true, "the converge runs the flag's `false` path either way");
    }

    #[test]
    fn the_rendered_report_carries_the_note_below_the_rows_never_as_one() {
        let mut claimed = mesh(&[("sakaki", "ssh://khoa@h")]);
        claimed.same_operator = true;
        let text = render_converge(&converge_report("home", &claimed, one_row()));
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines[0].contains("sakaki") && lines[0].contains("completed"), "{text}");
        assert!(lines.last().unwrap().contains("sameOperator = true"), "{text}");
        assert_eq!(lines.iter().filter(|l| l.contains("sakaki")).count(), 1, "the note is not a row: {text}");
    }

    // ── mesh pair: the declared grant reaches the ceremony ──────────────────

    #[test]
    fn a_declared_grant_reaches_pair_finish_and_an_absent_one_is_none_never_empty() {
        let inv = cli_inv(&["mesh", "pair"]);
        let none = converge_finish(&inv, &mesh(&[])).expect("no flags, no parse failure");
        assert_eq!(none.grant, None, "an absent declaration falls through to resolve_grant, never to []");

        let mut declared = mesh(&[]);
        declared.grant = Some(vec!["read".to_string(), "spawn".to_string()]);
        let some = converge_finish(&inv, &declared).expect("no flags, no parse failure");
        assert_eq!(some.grant, Some(vec!["read".to_string(), "spawn".to_string()]));
        assert_ne!(none.grant, some.grant, "a mesh that declares a grant stamps a different one");

        let mut empty = mesh(&[]);
        empty.grant = Some(Vec::new());
        let nothing = converge_finish(&inv, &empty).expect("no flags, no parse failure");
        assert_eq!(nothing.grant, Some(Vec::new()), "`grant = []` is the real `grant nothing` intent");
        assert_ne!(nothing.grant, none.grant);
    }

    #[test]
    fn yes_buys_the_preflight_and_never_reaches_the_code_gate() {
        // `commands::outbound_gate_from` reads `skip_confirm` BEFORE it
        // tests for a tty, so a `--yes` carried into the finish would
        // resolve every leg to `CodeGate::Unavailable` — the whole converge
        // parks, nothing commits, and the operator types N ids by hand.
        // `--yes` buys exactly one thing here: the pre-flight, which
        // `the_preflight_is_refused_off_a_tty_without_yes_and_skipped_with_it`
        // pins separately off the invocation.
        let mut inv = cli_inv(&["mesh", "pair"]);
        inv.flags.insert("yes".to_string(), "true".to_string());
        let finish = converge_finish(&inv, &mesh(&[])).expect("--yes parses");
        assert!(!finish.skip_confirm, "--yes must not reach the ceremony's own gate: {finish:?}");
        assert!(inv.flag_present("yes"), "and it must still be readable for the pre-flight");
    }

    // ── mesh pair: which mesh, and the pre-flight ───────────────────────────

    #[test]
    fn a_bare_converge_takes_the_one_declared_mesh_and_names_them_when_there_are_several() {
        let one = drift(&BTreeMap::from([("home".to_string(), mesh(&[]))]), &[], "this-box");
        assert_eq!(resolve_section("mesh.pair", &one, None).unwrap().name, "home");

        let two = drift(
            &BTreeMap::from([("home".to_string(), mesh(&[])), ("lab".to_string(), mesh(&[]))]),
            &[],
            "this-box",
        );
        let err = resolve_section("mesh.pair", &two, None).unwrap_err();
        assert_eq!(err.status, aoide_protocol::output::Status::Usage, "{err:?}");
        assert!(err.message.contains("home, lab"), "{}", err.message);

        let none = drift(&BTreeMap::new(), &[], "this-box");
        let err = resolve_section("mesh.pair", &none, None).unwrap_err();
        assert_eq!(err.data.unwrap().get("reason").and_then(|v| v.as_str()), Some("no-mesh-declared"));
    }

    #[test]
    fn naming_a_mesh_that_is_not_declared_lists_the_ones_that_are() {
        let one = drift(&BTreeMap::from([("home".to_string(), mesh(&[]))]), &[], "this-box");
        let err = resolve_section("mesh.pair", &one, Some("lab")).unwrap_err();
        assert!(err.message.contains("no `[mesh.lab]`"), "{}", err.message);
        assert!(err.message.contains("declared: home"), "{}", err.message);
    }

    #[test]
    fn the_preflight_lists_every_node_its_hop_the_grant_and_the_wait() {
        let (meshes, nodes) = converge_fixture();
        let mut m = meshes["home"].clone();
        m.grant = Some(vec!["read".to_string()]);
        let report = drift(&meshes, &nodes, "this-box");
        let plan = plan(&report.sections[0], &m);
        let text = render_preflight("home", &m, &plan, 600);
        assert!(text.contains("pair chiyo via ssh://khoa@h2"), "{text}");
        assert!(text.contains("pair sakaki via ssh://khoa@h1"), "{text}");
        assert!(text.contains("skip yuzu"), "{text}");
        assert!(text.contains("grant: read (declared by mesh.home)"), "{text}");
        assert!(text.contains("600s per node"), "{text}");
    }

    #[test]
    fn the_preflight_is_refused_off_a_tty_without_yes_and_skipped_with_it() {
        let (meshes, nodes) = converge_fixture();
        let m = &meshes["home"];
        let report = drift(&meshes, &nodes, "this-box");
        let plan = plan(&report.sections[0], m);

        let mut inv = cli_inv(&["mesh", "pair"]);
        inv.door = Door::A2a; // never interactive
        let err = confirm_preflight("mesh.pair", &inv, "home", m, &plan, 600).unwrap_err();
        assert!(err.message.contains("re-run with --yes"), "{}", err.message);
        assert_eq!(err.data.unwrap().get("reason").and_then(|v| v.as_str()), Some("no-preflight-confirm"));

        inv.flags.insert("yes".to_string(), "true".to_string());
        assert!(confirm_preflight("mesh.pair", &inv, "home", m, &plan, 600).is_ok());
    }

    #[test]
    fn a_converge_with_nothing_to_pair_needs_no_confirmation_at_all() {
        // All-`skipped` (the idempotent second run) sends nothing, so there
        // is nothing to confirm — it must not refuse off a non-tty door.
        let (meshes, _) = converge_fixture();
        let nodes = vec![node("yuzu", true, Some("ssh://khoa@wrong"))];
        let report = drift(&meshes, &nodes, "this-box");
        let plan: Vec<PlannedNode> = plan(&report.sections[0], &meshes["home"])
            .into_iter()
            .filter(|p| matches!(p.action, PlannedAction::Skip { .. }))
            .collect();
        let mut inv = cli_inv(&["mesh", "pair"]);
        inv.door = Door::A2a;
        assert!(confirm_preflight("mesh.pair", &inv, "home", &meshes["home"], &plan, 600).is_ok());
    }

    // ── mesh pair: handler wiring ───────────────────────────────────────────

    #[test]
    fn handle_mesh_pair_with_no_mesh_declared_refuses_and_pairs_nothing() {
        let out = with_config_root("pair-nomesh", |_dir| handle_mesh_pair(&cli_inv(&["mesh", "pair"])));
        assert_eq!(out.status, aoide_protocol::output::Status::Error, "{out:?}");
        let data = out.data.as_ref().expect("error envelope carries data");
        assert_eq!(data.get("reason").and_then(|v| v.as_str()), Some("no-mesh-declared"), "{data:?}");
    }

    #[test]
    fn handle_mesh_pair_is_error_when_config_fails_to_load() {
        let out = with_config_root("pair-badconfig", |dir| {
            std::fs::write(dir.join("config.toml"), "[mesh.home]\ngrant = [\"root\"]\n").unwrap();
            handle_mesh_pair(&cli_inv(&["mesh", "pair"]))
        });
        assert_eq!(out.status, aoide_protocol::output::Status::Error, "{out:?}");
        let data = out.data.as_ref().expect("error envelope carries data");
        assert_eq!(data.get("reason").and_then(|v| v.as_str()), Some("config-unreadable"), "{data:?}");
    }

    #[test]
    fn handle_mesh_pair_over_an_all_skipped_mesh_dials_nothing_and_reports_every_skip() {
        // A declaration whose only drift is a via-mismatch: the converge
        // sends nothing (no tunnel, no POST, no confirm) and comes back with
        // one `skipped` row — the idempotent second run, end to end.
        let out = with_config_root("pair-skipped", |dir| {
            std::fs::write(
                dir.join("config.toml"),
                "[mesh.home.nodes]\nyuzu = \"ssh://khoa@h4\"\n",
            )
            .unwrap();
            std::fs::create_dir_all(dir).unwrap();
            let nodes = vec![node("yuzu", true, Some("ssh://khoa@wrong"))];
            aoide_storage::node_store::save_nodes(&nodes).unwrap();
            handle_mesh_pair(&cli_inv(&["mesh", "pair"]))
        });
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "{out:?}");
        assert!(out.changed.is_empty(), "nothing was written: {out:?}");
        assert!(out.message.contains("yuzu") && out.message.contains("skipped"), "{}", out.message);
        let report = out.data.as_ref().and_then(|d| d.get("report")).expect("ok envelope carries a report");
        assert_eq!(report["rows"].as_array().map(Vec::len), Some(1), "{report}");
        assert_eq!(report["rows"][0]["outcome"].as_str(), Some("skipped"), "{report}");
    }

    #[test]
    fn handle_mesh_pair_refuses_wait_zero_when_the_mesh_declares_a_grant() {
        // The same rule `pair --allow --wait 0` already holds: a parked
        // entry never carries a grant, so a declared one would be silently
        // dropped on the resume. Refused up front, nothing sent.
        let out = with_config_root("pair-detached-grant", |dir| {
            std::fs::write(
                dir.join("config.toml"),
                "[mesh.home]\ngrant = [\"read\"]\n\n[mesh.home.nodes]\nsakaki = \"ssh://khoa@h1\"\n",
            )
            .unwrap();
            let mut inv = cli_inv(&["mesh", "pair"]);
            inv.flags.insert("wait".to_string(), "0".to_string());
            inv.flags.insert("yes".to_string(), "true".to_string());
            handle_mesh_pair(&inv)
        });
        assert_eq!(out.status, aoide_protocol::output::Status::Usage, "{out:?}");
        assert!(out.message.contains("mesh.home declares a grant"), "{}", out.message);
        assert!(out.message.contains("never persisted on a parked entry"), "{}", out.message);
        assert!(!out.message.contains("retype --allow"), "no flag was typed here: {}", out.message);
        let data = out.data.as_ref().expect("usage envelope carries data");
        assert_eq!(data.get("reason").and_then(|v| v.as_str()), Some("detached-grant"), "{data:?}");
    }

    #[test]
    fn wait_zero_over_a_converged_mesh_is_ok_even_when_a_grant_is_declared() {
        // The refusal above is about a grant that would be DROPPED, and a
        // converged mesh sends nothing to drop it from. Refusing here would
        // make `mesh pair --wait 0 --json` — the scripted drift check — fail
        // forever on any mesh that declares a grant, contradicting the
        // all-skipped second run the command promises.
        let out = with_config_root("pair-converged-grant", |dir| {
            std::fs::write(
                dir.join("config.toml"),
                "[mesh.home]\ngrant = [\"read\"]\n\n[mesh.home.nodes]\nyuzu = \"ssh://khoa@h4\"\n",
            )
            .unwrap();
            let nodes = vec![node("yuzu", true, Some("ssh://khoa@wrong"))];
            aoide_storage::node_store::save_nodes(&nodes).unwrap();
            let mut inv = cli_inv(&["mesh", "pair"]);
            inv.flags.insert("wait".to_string(), "0".to_string());
            handle_mesh_pair(&inv)
        });
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "{out:?}");
        assert!(out.changed.is_empty(), "nothing was written: {out:?}");
        let report = out.data.as_ref().and_then(|d| d.get("report")).expect("ok envelope carries a report");
        assert_eq!(report["rows"][0]["outcome"].as_str(), Some("skipped"), "{report}");
    }

    #[test]
    fn parked_id_for_answers_about_the_name_not_about_this_run() {
        // `park_outbound` dedups by pubkey, so an entry an earlier converge
        // left behind outlives a request that never reached the box. The id
        // it returns is genuinely resumable — it is simply not proof that
        // THIS run made progress, which is why a parked row prints its
        // detail beside the id.
        with_config_root("parked-id", |_dir| {
            assert_eq!(parked_id_for("sakaki"), None, "nothing parked yet");
            aoide_storage::pairing::park_outbound(aoide_storage::pairing::OutboundPairingRequest {
        binding: None,
                id: "abc123".to_string(),
                url: "http://127.0.0.1:8710/".to_string(),
                name: "sakaki".to_string(),
                pubkey_hex: "aa".repeat(32),
                requester_nonce_hex: "bb".repeat(16),
                approver_nonce_hex: "cc".repeat(16),
                requested_at: "2026-09-03T00:00:00Z".to_string(),
                expires_at: "2099-01-01T00:00:00Z".to_string(),
                state: aoide_storage::pairing::OutboundState::AwaitingApproval,
                tries: 0,
                via: Some("ssh://khoa@h1".to_string()),
                mesh: None,
            })
            .expect("parking writes to the scratch root");
            assert_eq!(parked_id_for("sakaki").as_deref(), Some("abc123"));
            assert_eq!(parked_id_for("osaka"), None, "another name is not this one's entry");
        });
    }
}
