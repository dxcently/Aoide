//! One mesh's DECLARED routing table, read once: `relays` in preference order,
//! a node's `[status]`, the `[gates]` out of the mesh, a node's `address` and
//! identity key, and the reverse — which declared NAME a verifying key is.
//!
//! **A mesh's table has exactly one source, and this is the one read of
//! either.** A CHARTER mesh's is its signed charter; its config section names
//! the operator key and nothing else, which the validator enforces. A PAIR
//! mesh's is its `[mesh.<name>]` section, with keys from the paired records it
//! names. There is never a merge of the two, and never a precedence question.
//!
//! **Which kind it is, is [`charter::charter_shaped`]'s answer** — state only,
//! the same answer the door asks for, so a config that cannot be read cannot
//! reopen a chartered mesh's paired records. Charter-shaped and holding no
//! readable charter is a refusal ([`Declarations`] carries the word), never the
//! pair table underneath it.
//!
//! **The names here are the policy names.** A charter mesh resolves a key to
//! the name on its CHARTER LINE; only a pair mesh reads one out of
//! `state/nodes.json`, where the paired record IS the declaration. A nickname
//! is display and never an input.
//!
//! **And this is where a letter is routed.** [`Letter::route`] is the four
//! steps of MAIL.md §Transit, run over the declarations this box holds and
//! nothing else: no dial, no clock, no spool write on any path. It answers
//! where a letter goes next, the mesh it rides by then, and the reason every
//! step picked or passed — a hop that can deliver delivers first, `relays` is
//! the fallback in declaration order, only a declared gate rewrites a letter's
//! mesh, and a box that knows two meshes never moves a letter between them
//! because it happens to know the destination.

use std::collections::{BTreeMap, BTreeSet};

use crate::charter::{self, Charter, Dial, Refusal};
use crate::{config, node_store, seal};

/// One node name carrying two different identity keys — "one node, one
/// identity key, in every mesh" (MAIL.md §Transit).
pub const KEY_DIVERGENCE: &str = "key-divergence";
/// A gate the mesh on the other side does not answer back, or one a mesh
/// points at itself — a gate is symmetric or it is nothing (MAIL.md §Transit).
pub const ONE_SIDED_GATE: &str = "one-sided-gate";
/// No mesh of this name is declared here — neither a charter in force nor a
/// `[mesh.<name>]` section.
pub const NO_DECLARATION: &str = "no-declaration";
/// This box is not a member of the mesh it was asked to act in: a CHARTER mesh
/// does not carry its identity key, so the mesh never gave it a name, and a
/// hostname is not a name a mesh gives anyone. The word `mail route` and both
/// lanes answer with, so a stranger reads the same refusal wherever it asks.
pub const NOT_A_MEMBER: &str = "not-a-member";

/// One mesh's load outcome: its declaration, or the taught refusal that stands
/// in for it. A mesh that cannot be read is refused HERE, in its own entry, and
/// never takes another mesh out of routing with it.
#[derive(Debug, Clone, PartialEq)]
pub struct Loaded {
    pub mesh: String,
    pub declaration: Result<Declaration, Refusal>,
}

/// Every mesh this host declares or holds state for — config's `[mesh.<name>]`
/// sections plus every mesh with charter state (`charter::meshes_with_state`,
/// the set `aoide mesh` reports) — sorted, each loaded ON ITS OWN, and each
/// then checked against the others for the two invariants no single
/// declaration can see: one node name never carries two identity keys, and a
/// gate is answered by the mesh on the other side.
///
/// **One snapshot of `config.toml` and `state/nodes.json` for the whole set.**
/// A mesh is read per entry, but the files under them are read once, so two
/// entries can never disagree with each other about what config says. A config
/// that will not load refuses the SET, with its own word
/// ([`charter::CONFIG_UNREADABLE`]): a broken config is not a name source — the
/// pair meshes it declares cannot even be listed — and a routing table built by
/// dropping the file that names half of it is the guess this seam exists to
/// refuse.
///
/// A violation is refused against the mesh whose own declaration is the
/// inconsistent one — the pair mesh whose record disagrees with a charter, the
/// mesh that declares an un-answered gate — so a disagreement between two
/// meshes fails closed for the mesh that is wrong and leaves the rest routing.
/// When no mesh is subordinate to another (one name carrying two keys across
/// two charters, or across two pair meshes) then BOTH are wrong, and both are
/// refused: keeping one by mesh-name order would leave the other routing.
pub fn declarations() -> Result<Vec<Loaded>, Refusal> {
    let loaded =
        config::load().map_err(|e| Refusal::new(charter::CONFIG_UNREADABLE, e.to_string()))?;
    let records = node_store::load_nodes();

    let mut names: BTreeSet<String> = loaded.config.mesh.keys().cloned().collect();
    names.extend(charter::meshes_with_state());

    let mut out: Vec<Loaded> = names
        .into_iter()
        .map(|mesh| {
            let declaration = if charter::charter_shaped(&mesh) {
                Declaration::from_charter(&mesh)
            } else {
                Declaration::from_section(&mesh, &loaded, &records)
            };
            Loaded { mesh, declaration }
        })
        .collect();
    for (mesh, refusal) in set_verdicts(&out) {
        if let Some(loaded) = out.iter_mut().find(|l| l.mesh == mesh) {
            loaded.declaration = Err(refusal);
        }
    }
    Ok(out)
}

/// The one zone-name resolution: an EMPTY name is the home mesh on either side
/// (`crate::config::home_mesh`), because a container minted before the mesh was
/// carried names none. Every lookup that compares two mesh names goes through it
/// — the walk's crossing check, this module's key resolution, and the door's
/// own mesh comparison.
pub fn zone_name(mesh: &str) -> String {
    let t = mesh.trim();
    if t.is_empty() {
        crate::config::home_mesh()
    } else {
        t.to_ascii_lowercase()
    }
}

/// The declaration the set holds for `mesh`, normalised with [`zone_name`]:
/// `Some(Ok)` where the mesh is declared and readable, `Some(Err)` where the set
/// REFUSES it, `None` where the set does not name the mesh at all. The three
/// answers are different questions — "declared", "cannot be read", "not here" —
/// and a caller that reads a binding, an address or a hop must tell them apart:
/// a refused mesh must PARK or refuse, never fall through to a weaker source.
pub fn declaration_of<'a>(set: &'a [Loaded], mesh: &str) -> Option<&'a Result<Declaration, Refusal>> {
    let mesh = zone_name(mesh);
    set.iter().find(|loaded| zone_name(&loaded.mesh) == mesh).map(|loaded| &loaded.declaration)
}

/// The identity key `node` holds in `mesh` — the ONE resolution every
/// verification reads (a hop's entry, an origin's two signatures, a binding's
/// signer), over the declaration SET the caller already loaded.
///
/// **A mesh the set holds answers from its declaration and from nothing else**,
/// and a mesh the set REFUSES answers `None`: a key read out of a declaration
/// that will not load — a charter that is missing, tampered with or one-sided, a
/// pair mesh whose records disagree — is a key nobody vouched for, and reading
/// the paired records underneath it is exactly how a revoked or stale key comes
/// back. A paired record answers ONLY where the mesh is absent from the set AND
/// no charter governs it, which is the pre-charter lane.
pub fn key_in(set: &[Loaded], mesh: &str, node: &str) -> Option<String> {
    let mesh = zone_name(mesh);
    if let Some(loaded) = set.iter().find(|loaded| zone_name(&loaded.mesh) == mesh) {
        return loaded
            .declaration
            .as_ref()
            .ok()
            .and_then(|declaration| declaration.key_of(node))
            .map(str::to_string);
    }
    if charter::charter_shaped(&mesh) {
        return None;
    }
    node_store::load_nodes()
        .iter()
        .find(|n| n.name == node)
        .and_then(|n| n.pubkey.clone())
}

/// Does the mesh `from` declare `node` as its gate into `to` — read from the SET,
/// so a mesh the set refuses (a one-sided gate, a read that fails) declares
/// nothing and a crossing through it is refused rather than waved through.
pub fn gate_in(set: &[Loaded], from: &str, to: &str, node: &str) -> bool {
    let from = zone_name(from);
    let to = zone_name(to);
    set.iter()
        .find(|loaded| zone_name(&loaded.mesh) == from)
        .and_then(|loaded| loaded.declaration.as_ref().ok())
        .and_then(|declaration| declaration.gates().get(&to))
        .map(|gate| gate == node)
        .unwrap_or(false)
}

/// This box's own name IN `mesh`: the declared name its identity `key` belongs
/// to, or `None` for a key the mesh does not carry — which is exactly "not a
/// member of this mesh". The ONE resolution every policy, routing and audit
/// lookup that starts from a verifying key goes through
/// ([`Declaration::name_of_key`]); the OS hostname and a `nodes.json` nickname
/// are display facts and never inputs. A mesh this node cannot read names
/// nobody, so a box whose own mesh has fallen over cannot speak for it.
pub fn declared_name(set: &[Loaded], mesh: &str, key: &str) -> Option<String> {
    set.iter()
        .find(|loaded| loaded.mesh == mesh)
        .and_then(|loaded| loaded.declaration.as_ref().ok())
        .and_then(|declaration| declaration.name_of_key(key))
        .map(str::to_string)
}

/// The declared status of `node` in `mesh`, over a set the caller already
/// loaded: `charter::STATUS_VALUES` or `None`. The ONE predicate the door, the
/// drain, the poll and the report ask before reaching a node by name — `None`
/// for a mesh the set does not hold, a mesh it refuses, a node it does not
/// declare, and a node declared with no status, because all four are "not
/// declared `down` HERE" and only the second is worth telling apart from the
/// rest at the call site ([`declaration_of`] answers that one).
pub fn status_of<'a>(set: &'a [Loaded], mesh: &str, node: &str) -> Option<&'a str> {
    declaration_of(set, mesh)?.as_ref().ok()?.status_of(node)
}

/// This box's own name in `mesh` — what `from` is for the four steps, for the
/// hop an entry is signed as, and for the door's audit stamp.
///
/// A name is the one the mesh's declaration gives this box's identity `key`
/// ([`declared_name`]). Failing that, the address form of this box's own name
/// (`crate::display::local_node_name`) answers **only for a mesh no charter
/// governs**: a pair mesh's own records are all about OTHER boxes, so a box
/// that has no record of itself is still itself there. A CHARTER mesh that does
/// not carry this box's key names nobody, and the hostname is not a name: a box
/// whose hostname happens to spell a member's name is a stranger in that mesh —
/// `None` here — never that member.
pub fn own_name_in(set: &[Loaded], mesh: &str, key: &str) -> Option<String> {
    if let Some(declared) = declared_name(set, mesh, key) {
        return Some(declared);
    }
    if charter::charter_shaped(mesh) {
        return None;
    }
    Some(crate::display::local_node_name())
}

/// The two invariants only a SET of declarations can see, each refused against
/// the mesh that is inconsistent, as `mesh name → refusal`.
///
/// **One node, one identity key, in every mesh.** A node may sit in several
/// meshes with a different grant in each; identity is not per mesh, so a name
/// carrying two keys is a load error, and every declaration's keys count — a
/// pair mesh's too, which is how a paired record left over from before a
/// charter re-keyed a name is caught.
///
/// Where the declarations a name's copies live in AGREE on its key, that key is
/// the authority and only a copy that disagrees is refused — so a pair mesh
/// whose paired record still holds the key from before a charter re-keyed the
/// name is the copy that yields, and every charter that agrees keeps routing.
/// Only where the CHARTERS disagree with each other is there nothing to yield
/// to, and then no copy of that name can be kept: all of them are refused
/// rather than one by mesh-name order, which would leave the others routing
/// behind it.
///
/// **A gate is symmetric or it is nothing.** Each gate a mesh declares must be
/// answered, by the mesh it names, with the same node; a mesh cannot be its own
/// answer (`gate_verdict`). A gate into a mesh this host does not hold cannot
/// be judged here and is left to the two declarations that do.
fn set_verdicts(loaded: &[Loaded]) -> BTreeMap<String, Refusal> {
    let mut verdicts: BTreeMap<String, Refusal> = BTreeMap::new();
    // node name -> every declaration that gives it a key, charters first (the
    // authority order) and mesh-name order inside each half.
    let mut keyed: BTreeMap<&str, Vec<(&str, &str, bool)>> = BTreeMap::new();
    let mut ordered: Vec<&Declaration> = loaded.iter().filter_map(|l| l.declaration.as_ref().ok()).collect();
    ordered.sort_by_key(|d| !d.is_charter());
    for declaration in ordered {
        for (node, key) in declaration.declared_keys() {
            keyed.entry(node).or_default().push((declaration.mesh(), key, declaration.is_charter()));
        }
    }

    for (node, members) in &keyed {
        let mut keys: Vec<&str> = members.iter().map(|(_, key, _)| *key).collect();
        keys.sort();
        keys.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        if keys.len() < 2 {
            continue;
        }
        // The key the charters agree on is the authority: what they all list
        // for a name is that name's key, and every copy that disagrees with it
        // is the copy refused — a pair mesh's record included, which is the
        // re-key case. Only where the CHARTERS disagree with EACH OTHER is
        // there no authority to yield to, and then no copy at all can be kept.
        let charter_keys: Vec<&str> = members
            .iter()
            .filter(|(_, _, charter)| *charter)
            .map(|(_, key, _)| *key)
            .collect();
        let mut distinct: Vec<&str> = charter_keys.clone();
        distinct.sort();
        distinct.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        let authority = match distinct.as_slice() {
            [only] => Some(*only),
            // No charter here at all (two pair meshes), or charters that
            // disagree: nothing is subordinate to anything else.
            _ => None,
        };
        for (mesh, key, _) in members {
            if authority.map(|only| only.eq_ignore_ascii_case(key)) == Some(true) {
                continue;
            }
            // The other copy the refusal names, by declaration order (a
            // charter before a pair; mesh name inside each) so the sentence is
            // the same on every box that reads the same set.
            let (other_mesh, other_key) = members
                .iter()
                .find(|(other, other_key, _)| *other != *mesh && !other_key.eq_ignore_ascii_case(key))
                .map(|(other, other_key, _)| (*other, *other_key))
                .unwrap_or_default();
            verdicts.entry(mesh.to_string()).or_insert_with(|| {
                Refusal::new(
                    KEY_DIVERGENCE,
                    format!(
                        "`{node}` carries two identity keys — `{other_key}` in mesh `{other_mesh}` and \
                         `{key}` in mesh `{mesh}`. One node, one identity key, in every mesh: a name that \
                         stands for two machines is a name no policy lookup can trust"
                    ),
                )
            });
        }
    }

    let held: Vec<&Declaration> = loaded
        .iter()
        .filter(|l| !verdicts.contains_key(&l.mesh))
        .filter_map(|l| l.declaration.as_ref().ok())
        .collect();
    for declaration in &held {
        for (other, gate) in declaration.gates() {
            let back = held.iter().find(|d| d.mesh() == other).map(|d| d.gates());
            if let Some(refusal) = gate_verdict(declaration.mesh(), other, gate, back) {
                verdicts.insert(declaration.mesh().to_string(), refusal);
                break;
            }
        }
    }
    verdicts
}

/// Does `mesh`'s gate into `other` through `gate` hold? `None` when it does,
/// the refusal when it does not. `back` is the gated mesh's own gates, or
/// `None` for a mesh this host does not hold — a mesh is never its own answer,
/// so a gate at itself is refused without any lookup, and a gate nobody here
/// can answer for is left alone.
fn gate_verdict(
    mesh: &str,
    other: &str,
    gate: &str,
    back: Option<&BTreeMap<String, String>>,
) -> Option<Refusal> {
    if other == mesh {
        return Some(Refusal::new(
            ONE_SIDED_GATE,
            format!(
                "mesh `{mesh}` gates into itself through `{gate}` — a gate carries transit into ANOTHER \
                 mesh, and `relays`/`[status]` are how this one describes its own members"
            ),
        ));
    }
    let back = back?;
    (back.get(mesh).map(String::as_str) != Some(gate)).then(|| {
        Refusal::new(
            ONE_SIDED_GATE,
            format!(
                "mesh `{mesh}` gates into `{other}` through `{gate}`, but `{other}` does not gate back \
                 through it — a gate is symmetric or it is nothing"
            ),
        )
    })
}

/// One mesh's routing declaration. Every declaration this box holds is built by
/// one of three readers and nothing else: [`Declaration::load`] for a caller
/// with one mesh in hand, and the two it shares with [`declarations`]
/// ([`Declaration::from_charter`]/[`Declaration::from_section`], which take the
/// snapshot the caller already read) — never a caller assembling one out of a
/// section or a charter file it read itself.
#[derive(Debug, Clone, PartialEq)]
pub struct Declaration {
    mesh: String,
    kind: Kind,
}

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Charter(Charter),
    Pair(Pair),
}

#[derive(Debug, Clone, PartialEq, Default)]
struct Pair {
    relays: Vec<String>,
    status: BTreeMap<String, String>,
    gates: BTreeMap<String, String>,
    addresses: BTreeMap<String, String>,
    keys: BTreeMap<String, String>,
}

impl Declaration {
    /// Read `mesh`'s declaration on its own: the charter in force where the
    /// mesh is charter-shaped, its `[mesh.<name>]` section otherwise.
    ///
    /// **A whole set is read by [`declarations`], which takes ONE snapshot of
    /// `config.toml` and `state/nodes.json` for every mesh** and calls
    /// [`from_charter`]/[`from_section`] with it; this entry point exists for a
    /// caller that has one mesh in hand and is what that call reduces to.
    pub fn load(mesh: &str) -> Result<Declaration, Refusal> {
        if charter::charter_shaped(mesh) {
            return Declaration::from_charter(mesh);
        }
        let loaded =
            config::load().map_err(|e| Refusal::new(charter::CONFIG_UNREADABLE, e.to_string()))?;
        Declaration::from_section(mesh, &loaded, &node_store::load_nodes())
    }

    /// The charter branch: the signed file in force, and config is not
    /// consulted for it at all. Fails closed — a charter-shaped mesh with no
    /// readable charter is refused with that charter's own word, never answered
    /// from the paired records.
    fn from_charter(mesh: &str) -> Result<Declaration, Refusal> {
        let charter = charter::governing_refusal(mesh)?;
        Ok(Declaration { mesh: mesh.to_string(), kind: Kind::Charter(charter) })
    }

    /// The pair branch: this mesh's `[mesh.<name>]` section out of a snapshot
    /// the caller already read, with the keys of the records it names read out
    /// of that same snapshot.
    fn from_section(
        mesh: &str,
        loaded: &config::Loaded,
        records: &[node_store::Node],
    ) -> Result<Declaration, Refusal> {
        let section = loaded.config.mesh.get(mesh).ok_or_else(|| {
            Refusal::new(
                NO_DECLARATION,
                format!(
                    "no declaration for mesh `{mesh}` at this node — no charter is in force for it and \
                     {} declares no `[mesh.{mesh}]` section",
                    loaded.path.display()
                ),
            )
        })?;
        Ok(Declaration { mesh: mesh.to_string(), kind: Kind::Pair(pair_from(section, records, mesh)) })
    }

    /// The mesh this declaration is for.
    pub fn mesh(&self) -> &str {
        &self.mesh
    }

    /// `true` exactly when the signed charter is the declaration being read.
    pub fn is_charter(&self) -> bool {
        matches!(self.kind, Kind::Charter(_))
    }

    /// The mesh's relays, in declaration order — the preference order the
    /// route's relay step reads. Empty means this mesh declares no transit hub,
    /// so a destination it does not itself hold is `no-route`.
    pub fn relays(&self) -> &[String] {
        match &self.kind {
            Kind::Charter(c) => &c.relays,
            Kind::Pair(p) => &p.relays,
        }
    }

    /// The declared status of `node` (`charter::STATUS_VALUES`), or `None` for
    /// a node declared with no status — the ordinary case, which is not the
    /// same fact as a node that is not declared at all.
    pub fn status_of(&self, node: &str) -> Option<&str> {
        match &self.kind {
            Kind::Charter(c) => c.status.get(node).map(String::as_str),
            Kind::Pair(p) => p.status.get(node).map(String::as_str),
        }
    }

    /// This mesh's gates: the OTHER mesh's name → the node of THIS mesh that
    /// carries transit into it.
    pub fn gates(&self) -> &BTreeMap<String, String> {
        match &self.kind {
            Kind::Charter(c) => &c.gates,
            Kind::Pair(p) => &p.gates,
        }
    }

    /// The age binding `node` publishes IN THIS MESH — a charter line's own
    /// `age` key (the nodelist carrying identity, MAIL.md §Transit), and `None`
    /// for a pair mesh: a paired node's binding arrives by the signed exchange
    /// and is learnt under `state/age-bindings/`, so a record is never a source
    /// for one. The line was verified as the operator wrote it (`charter::parse`
    /// refuses a line whose binding does not verify under that line's OWN key),
    /// which is what makes it the same trust a pinned record gives
    /// ([`crate::seal::learn_binding`]).
    pub fn binding_of(&self, node: &str) -> Option<&seal::Binding> {
        match &self.kind {
            Kind::Charter(c) => c.nodes.get(node).map(|line| &line.age),
            Kind::Pair(_) => None,
        }
    }

    /// The address `node` is declared at — `ssh://…`, `https://…` or `poll` —
    /// or `None` for a node this declaration does not name. An address is a
    /// declaration, not a dial target; [`charter::dial_of`] is the one turn
    /// from one to the other, and the route below reads reachability through
    /// it (`docs/architecture/HTTPS-MESH-API.md`, "Transports and relays").
    pub fn address_of(&self, node: &str) -> Option<&str> {
        match &self.kind {
            Kind::Charter(c) => c.nodes.get(node).map(|line| line.address.as_str()),
            Kind::Pair(p) => p.addresses.get(node).map(String::as_str),
        }
    }

    /// The identity key (bare lowercase hex) this declaration gives `node`, or
    /// `None`. For a charter mesh that is the charter's line; for a pair mesh
    /// the node's VERIFIED paired record — nothing else, because a relay never
    /// supplies a key (MAIL.md §Transit, "Keys come from trust"). Read by
    /// [`key_in`], which is where a caller resolves a key: this is the
    /// declaration's own half.
    pub fn key_of(&self, node: &str) -> Option<&str> {
        match &self.kind {
            Kind::Charter(c) => c.nodes.get(node).map(|line| line.key.as_str()),
            Kind::Pair(p) => p.keys.get(node).map(String::as_str),
        }
    }

    /// Is `node` a node of this mesh AT ALL — declared, whatever its trust or
    /// its `[status]`? A line always carries a key and an address, so the two
    /// accessors above are its two halves; this is the one question a route
    /// asks before it asks anything harder ("is the destination in a mesh this
    /// box holds, and is this box in that mesh too?").
    pub fn declares(&self, node: &str) -> bool {
        self.address_of(node).is_some() || self.key_of(node).is_some()
    }

    /// The declared NAME the identity key `key` belongs to — what a policy,
    /// routing or audit lookup starting from a verifying key resolves to. Bare
    /// hex, case-insensitive, the same comparison [`Charter::grant_for_key`]
    /// makes. The door's own audit stamp moves onto this when the door reads a
    /// declaration; until then it still prefers a matching record's name, so
    /// the two disagree for a record whose name is not the charter line's.
    pub fn name_of_key(&self, key: &str) -> Option<&str> {
        match &self.kind {
            Kind::Charter(c) => c
                .nodes
                .iter()
                .find(|(_, line)| line.key.eq_ignore_ascii_case(key))
                .map(|(name, _)| name.as_str()),
            Kind::Pair(p) => p
                .keys
                .iter()
                .find(|(_, held)| held.eq_ignore_ascii_case(key))
                .map(|(name, _)| name.as_str()),
        }
    }

    /// The names this declaration gives a key, as `set_verdicts` compares them.
    fn declared_keys(&self) -> Vec<(&str, &str)> {
        match &self.kind {
            Kind::Charter(c) => c.nodes.iter().map(|(n, l)| (n.as_str(), l.key.as_str())).collect(),
            Kind::Pair(p) => p.keys.iter().map(|(n, k)| (n.as_str(), k.as_str())).collect(),
        }
    }
}

/// The `[mesh.<name>]` section as a declaration, with the keys of the records
/// it names read out of the `state/nodes.json` snapshot the caller holds.
///
/// **A pair mesh's keys are the verified records GRANTED in it**, whether or not
/// the section also hands them a hop address: `mesh pair` writes a section whose
/// grant is the whole statement (the direct edge needs no map entry), and a hop
/// line is additive — an address. A verified record with no grant in this mesh is
/// not a key of it (a pairing in another mesh says nothing about this one).
fn pair_from(section: &config::Mesh, records: &[node_store::Node], mesh: &str) -> Pair {
    let mut keys: BTreeMap<String, String> = section
        .nodes
        .keys()
        .filter_map(|node| {
            let record = records.iter().find(|r| &r.name == node)?;
            if !record.verified {
                return None;
            }
            record.pubkey.clone().map(|key| (node.clone(), key))
        })
        .collect();
    for record in records {
        if !record.verified || !record.grant(mesh).iter().any(|cap| cap == "message") {
            continue;
        }
        if let Some(key) = record.pubkey.clone().filter(|key| !key.is_empty()) {
            keys.entry(record.name.clone()).or_insert(key);
        }
    }
    Pair {
        relays: section.relays.clone(),
        status: section.status.clone(),
        gates: section.gates.clone(),
        addresses: section.nodes.clone(),
        keys,
    }
}

// ── The four steps (MAIL.md §Transit) ───────────────────────────────────

/// No path to the destination from this hop, in the mesh the letter rides.
pub const NO_ROUTE: &str = "no-route";

/// One letter's question at one hop: this box's own declared name, the node
/// the letter is addressed to, and the mesh it rides (`envelope.mesh`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Letter<'a> {
    pub from: &'a str,
    pub to: &'a str,
    pub mesh: &'a str,
}

/// The hop a route picks: the node this box hands the letter to, the mesh in
/// force when it arrives there, and whether that hop HOLDS it rather than
/// dialling — a `poll` address, or a node declared `hold`, leaves only when
/// the far end asks for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hop {
    pub next: String,
    pub mesh: String,
    pub held: bool,
}

/// One dry run: the hop, or the refusal that stands in for it, and every step
/// that ran with its reason.
#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    /// Where the letter goes next, or the word that says why it goes nowhere:
    /// the letter's OWN mesh's refusal when that mesh cannot be read,
    /// [`NO_ROUTE`] when no step produced a hop, or
    /// [`seal::ZONE_VIOLATION`] when the only thing in the way is the wall
    /// between two meshes this box happens to sit in.
    pub outcome: Result<Hop, Refusal>,
    /// Each step, in order, with the reason it picked or passed.
    pub trail: Vec<String>,
}

impl<'a> Letter<'a> {
    /// The four steps, over the declarations this box holds and nothing else.
    ///
    /// **Step 1 comes first at every hop.** A box that can hand the letter to
    /// the destination does, even when it is also a declared relay: `relays`
    /// is the fallback in declaration order, never a mandatory chain. Step 1
    /// is also the zone wall — it applies to the mesh the letter RIDES, so a
    /// box in two meshes reaches the destination in one of them only through
    /// the gate the letter's mesh declares (step 3). Nothing here dials, reads
    /// a clock, or writes a spool: the answer is a hop, a mesh and a trail,
    /// and the caller is what acts on it.
    pub fn route(&self, set: &[Loaded]) -> Route {
        let mut trail = Vec::new();
        let mut walked: BTreeSet<String> = BTreeSet::new();
        walked.insert(self.mesh.to_string());
        let outcome = self.walk(set, self.mesh, &mut trail, &mut walked);
        Route { outcome, trail }
    }

    /// The same steps in `mesh` — the mesh the letter rides, which only the
    /// gate clause ever changes. `walked` is every mesh a rewrite has already
    /// carried this letter out of: a gate chain that comes back to one of them
    /// is a loop, and is refused rather than walked.
    fn walk(
        &self,
        set: &[Loaded],
        mesh: &str,
        trail: &mut Vec<String>,
        walked: &mut BTreeSet<String>,
    ) -> Result<Hop, Refusal> {
        let Some(loaded) = set.iter().find(|l| l.mesh == mesh) else {
            trail.push(format!("mesh `{mesh}`: no declaration at this node"));
            return Err(Refusal::new(
                NO_DECLARATION,
                format!(
                    "no declaration for mesh `{mesh}` at this node, so nothing here can say whether \
                     `{}` is a member of it, trusted, reachable or `down`",
                    self.to
                ),
            ));
        };
        // A mesh this node cannot read routes NOTHING, and only for itself:
        // its own word, and never a fallback to another mesh's declaration or
        // to the paired records under this one.
        let declaration = match &loaded.declaration {
            Ok(declaration) => declaration,
            Err(refusal) => {
                trail.push(format!("mesh `{mesh}` cannot be read: {refusal}"));
                return Err(refusal.clone());
            }
        };

        // A letter addressed to this box is filed here, whatever the mesh says
        // about trust or reachability: there is nothing to hand on.
        if self.to == self.from {
            trail.push(format!("step 1: `{}` is this box — file it here", self.to));
            return Ok(Hop { next: self.to.to_string(), mesh: mesh.to_string(), held: false });
        }

        // Steps 1 and 2: the destination in the mesh the letter rides, first;
        // then that mesh's relays, in declaration order.
        if let Some(hop) =
            by_steps_one_and_two(declaration, self.from, self.to, trail, "step 1", "step 2")
        {
            return Ok(hop);
        }

        // Step 3: the destination is not in the mesh the letter rides, but a
        // node of another mesh this box holds, and the letter's mesh declares
        // the node that carries transit into it. Only the declared gate may
        // cross, and only the gate itself may rewrite `envelope.mesh` — a dual
        // member's own knowledge of the destination is not a route.
        let mut walled: Option<&str> = None;
        for other in set.iter().filter(|l| l.mesh != mesh) {
            let Ok(theirs) = &other.declaration else { continue };
            // The precondition is MAIL.md's own: a destination the letter's
            // mesh declares is this mesh's problem — a relay of it, or no
            // route — and never a reason to carry the letter into another zone.
            if declaration.declares(self.to) || !theirs.declares(self.to) {
                continue;
            }
            let their_mesh = other.mesh.as_str();
            match declaration.gates().get(their_mesh) {
                None => {
                    if theirs.declares(self.from) {
                        trail.push(format!(
                            "step 3: `{}` is a node of mesh `{their_mesh}` too, and mesh `{mesh}` gates \
                             into no mesh that holds `{}` — this box may not bridge",
                            self.from, self.to
                        ));
                        walled = walled.or(Some(their_mesh));
                    } else {
                        trail.push(format!(
                            "step 3: `{}` is a node of mesh `{their_mesh}`, which mesh `{mesh}` declares \
                             no gate into",
                            self.to
                        ));
                    }
                }
                Some(gate) if gate == self.from => {
                    if !walked.insert(their_mesh.to_string()) {
                        let detail = format!(
                            "mesh `{mesh}` gates into `{their_mesh}` and `{their_mesh}` gates back into \
                             `{mesh}`, and this box is the gate both ways — the letter would bounce"
                        );
                        trail.push(format!("step 3: {detail}"));
                        return Err(Refusal::new(NO_ROUTE, detail));
                    }
                    trail.push(format!(
                        "step 3: this box is `{gate}`, the node mesh `{mesh}` declares as its gate into \
                         `{their_mesh}` — the letter's mesh is rewritten and the steps restart there"
                    ));
                    return self.walk(set, their_mesh, trail, walked);
                }
                Some(gate) => {
                    trail.push(format!(
                        "step 3: `{}` is a node of mesh `{their_mesh}`, and mesh `{mesh}` gates into it \
                         through `{gate}`",
                        self.to
                    ));
                    if let Some(hop) =
                        by_steps_one_and_two(declaration, self.from, gate, trail, "step 3", "step 3")
                    {
                        return Ok(hop);
                    }
                    // The declared way in is out of reach, and this box is in
                    // the destination's mesh too: the wall is what is left.
                    if theirs.declares(self.from) {
                        walled = walled.or(Some(their_mesh));
                    }
                }
            }
        }

        // Step 4: nothing above produced a hop.
        match walled {
            Some(other) => Err(Refusal::new(
                seal::ZONE_VIOLATION,
                format!(
                    "`{}` is a node of mesh `{other}`, which this box is also in, and mesh `{mesh}` \
                     gives this box no way to carry the letter there. A box that knows two meshes does \
                     not move a letter between them because it happens to know the destination — only \
                     the gate the letter's own mesh declares crosses a zone, and this box is not it",
                    self.to
                ),
            )),
            None if declaration.declares(self.to) => Err(Refusal::new(
                NO_ROUTE,
                if declaration.status_of(self.to) == Some(charter::STATUS_DOWN) {
                    format!(
                        "`{}` is declared `down` in mesh `{mesh}`: a `down` node is never a hop and never \
                         a destination, and no relay is asked to hold its letters",
                        self.to
                    )
                } else {
                    format!(
                        "`{}` is a node of mesh `{mesh}` and no step of the route from `{}` produced a hop: \
                         not the destination itself, not a relay of that mesh and not a gate into another",
                        self.to, self.from
                    )
                },
            )),
            None => Err(Refusal::new(
                NO_ROUTE,
                format!(
                    "`{}` is not a node of mesh `{mesh}`, and no mesh this node holds that `{mesh}` \
                     declares a gate into declares it either",
                    self.to
                ),
            )),
        }
    }
}

/// Steps 1 and 2 with `target` as the target: the target itself when this box
/// can hand it the letter, else the first relay of the mesh it trusts and can
/// reach, in declaration order.
///
/// The two labels are the caller's, because they name where the attempt is:
/// the main walk's are `step 1` and `step 2`, and a gate standing in for a
/// destination in another mesh reaches its own target under `step 3`.
fn by_steps_one_and_two(
    declaration: &Declaration,
    from: &str,
    target: &str,
    trail: &mut Vec<String>,
    direct: &str,
    via_relay: &str,
) -> Option<Hop> {
    match reach(declaration, from, target) {
        Ok(hop) => {
            trail.push(format!("{direct}: {}", picked(declaration, &hop)));
            return Some(hop);
        }
        Err(reason) => trail.push(format!("{direct}: {reason}")),
    }
    // A target declared `down` is never a hop and never a relay's problem:
    // `down` refuses fast, and a letter for it stays where it is rather than
    // being moved to a hub that cannot reach it either (MAIL.md §Delivery). A
    // letter already queued for it is KEPT where it is — never evicted — and
    // that is the drain's business, not this one's.
    if declaration.declares(target) && declaration.status_of(target) == Some(charter::STATUS_DOWN) {
        trail.push(format!(
            "{direct}: `{target}` is declared `down` in mesh `{}` — no relay is asked to hold its \
             letters",
            declaration.mesh()
        ));
        return None;
    }
    for relay in declaration.relays() {
        match reach(declaration, from, relay) {
            Ok(hop) => {
                trail.push(format!("{via_relay}: {}", picked(declaration, &hop)));
                return Some(hop);
            }
            Err(reason) => trail.push(format!("{via_relay}: {reason}")),
        }
    }
    if declaration.relays().is_empty() {
        trail.push(format!("{via_relay}: mesh `{}` declares no relay", declaration.mesh()));
    }
    None
}

/// Why a step picked this hop, in the trail's own words.
fn picked(declaration: &Declaration, hop: &Hop) -> String {
    let address = declaration.address_of(&hop.next).unwrap_or_default();
    if hop.held {
        format!("`{}` holds it at `{address}` — it leaves when `{}` asks for it", hop.next, hop.next)
    } else {
        format!("`{}` takes it at `{address}`", hop.next)
    }
}

/// Whether this hop can hand the letter to `node`, and what that hop IS. `Ok`
/// is the hop it makes; `Err` is the reason, in the trail's own words.
///
/// Every fact is the DECLARATION's, which keeps the route pure and testable:
/// trust is a key this box holds for the name ([`Declaration::key_of`] — a
/// charter line, or a paired record for a pair mesh), status is `[status]`,
/// and reachability is the node's `address` read as a dial target
/// ([`charter::dial_of`]). A link that is merely down right now is NOT
/// consulted: a relay this box can address but cannot presently reach is still
/// chosen, and the letter waits for it
/// (`docs/architecture/HTTPS-MESH-API.md`, "Degenerate topologies").
///
/// A `poll` node is reachable through the box it asks, and that box is its
/// mesh's relay: the letter is HELD there for the node's own `mailPoll`, and a
/// `poll` node whose relay is not this box is not something this hop can serve
/// directly.
fn reach(declaration: &Declaration, from: &str, node: &str) -> Result<Hop, String> {
    if node == from {
        return Err(format!("`{node}` is this box — a letter already here is handed to nobody"));
    }
    // **`down` is decided HERE, before the address picks a lane.** Every step
    // reads its target through this one call, so the guarantee is the function's
    // own order rather than a fact about one branch: a `down` node is never a hop
    // and never a destination, whichever lane would have answered it (MAIL.md
    // §Status).
    if declaration.status_of(node) == Some(charter::STATUS_DOWN) {
        return Err(format!("`{node}` is declared `down` in mesh `{}`", declaration.mesh()));
    }
    let Some(address) = declaration.address_of(node) else {
        // No hop line for it — but it may still be a key of this mesh (a verified
        // record GRANTED here, the shape `mesh pair` writes without an address).
        // That node's edge is the DIRECT one: the record's own address answers,
        // and `poll` holds it for its own ask.
        if declaration.key_of(node).is_none() {
            return Err(format!("`{node}` is not a node of mesh `{}`", declaration.mesh()));
        }
        let record = node_store::load_nodes().into_iter().find(|r| r.name == node);
        let Some(record) = record else {
            return Err(format!(
                "`{node}` is trusted in mesh `{}` but this box holds no record of it, so it has no \
                 address to be handed a letter at",
                declaration.mesh()
            ));
        };
        return Ok(Hop {
            next: node.to_string(),
            mesh: declaration.mesh().to_string(),
            held: record.never_dialled(),
        });
    };
    if declaration.key_of(node).is_none() {
        return Err(format!(
            "`{node}` carries no identity key in mesh `{}`, so this box does not trust it there",
            declaration.mesh()
        ));
    }
    let dial = charter::dial_of(address).map_err(|e| {
        format!("`{node}`'s address in mesh `{}` is not a transport: {e}", declaration.mesh())
    })?;
    let held =
        declaration.status_of(node) == Some(charter::STATUS_HOLD) || matches!(dial, Dial::Poll);
    if matches!(dial, Dial::Poll) && !declaration.relays().iter().any(|relay| relay == from) {
        return Err(format!(
            "`{node}` is a `poll` node of mesh `{}` and asks its relay, not `{from}` — the letter has \
             to be handed to a relay that holds it",
            declaration.mesh()
        ));
    }
    Ok(Hop { next: node.to_string(), mesh: declaration.mesh().to_string(), held })
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_test_support::{unique_tmp, EnvSaver};
    use std::path::{Path, PathBuf};
    use std::sync::MutexGuard;

    /// One test's scratch root, removed however the test ends — a panic leaves
    /// no fixture behind for the next run to trip over.
    struct Scratch {
        root: PathBuf,
    }

    impl Scratch {
        fn new(tag: &str) -> Scratch {
            Scratch { root: unique_tmp(&format!("routing-{tag}")) }
        }

        /// One "machine" under this root: its own directory, standing in for
        /// its own box.
        fn dir(&self, name: &str) -> PathBuf {
            let dir = self.root.join(name);
            std::fs::create_dir_all(&dir).unwrap();
            dir
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    /// Point every root at `dir` and name this "machine" `name` — the same
    /// switch `charter`'s own tests make, so `node_line` reads THIS machine's
    /// identity and produces THIS machine's line.
    fn machine(dir: &Path, name: &str) {
        std::env::set_var("AOIDE_ROOT", dir);
        std::env::set_var("AOIDE_STATE_DIR", dir);
        std::env::remove_var("AOIDE_CONFIG");
        std::env::set_var("AOIDE_A2A_NODE_NAME", name);
    }

    fn isolate() -> (MutexGuard<'static, ()>, EnvSaver) {
        let guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saver = EnvSaver::capture(&[
            "AOIDE_ROOT",
            "AOIDE_STATE_DIR",
            "AOIDE_CONFIG",
            "AOIDE_A2A_NODE_NAME",
        ]);
        (guard, saver)
    }

    /// One node's charter line, minted on its own machine — `node_line` reads
    /// whatever root the env points at, so this is how a fixture gives a mesh
    /// more than one identity.
    fn line_for(root: &Path, name: &str) -> String {
        machine(root, name);
        charter::node_line().unwrap()
    }

    /// One VERIFIED paired record — the shape `aoide pair` writes, made by hand
    /// because a ceremony is not what these tests are about.
    fn record(name: &str, pubkey: &str) -> node_store::Node {
        node_store::Node {
            name: name.to_string(),
            url: format!("https://{name}.example/"),
            autogate: false,
            token_file: None,
            bearer_secret: None,
            hub: false,
            pubkey: Some(pubkey.to_string()),
            verified: true,
            grants: node_store::Grants::new(),
            narrowed: node_store::Grants::new(),
            via: None,
            added_at: "2026-09-27T00:00:00Z".to_string(),
        }
    }

    /// Root `mesh` on the machine `dir` — which is the one holding that mesh's
    /// operator key — write `body` as its signed source, and sign it, which
    /// applies it in force THERE. `body` is an argument, so a caller that needs
    /// node lines from other machines has already been and come back by the
    /// time the env is switched to the operator.
    fn sign_on(dir: &Path, mesh: &str, body: &str) {
        machine(dir, "opbox");
        if !charter::source_path(mesh).exists() {
            charter::init(mesh).unwrap();
        }
        std::fs::write(charter::source_path(mesh), format!("mesh = \"{mesh}\"\nversion = 0\n{body}")).unwrap();
        charter::sign(mesh, None).unwrap();
    }

    /// A charter body: relays, then the node lines, then `[status]` and
    /// `[gates]` — the document `Charter` describes, assembled from the parts a
    /// test varies.
    fn body(relays: &[&str], status: &[(&str, &str)], gates: &[(&str, &str)], nodes: &[String]) -> String {
        let quoted: Vec<String> = relays.iter().map(|r| format!("\"{r}\"")).collect();
        let mut out = format!("relays = [{}]\n\n[nodes]\n{}\n", quoted.join(", "), nodes.join("\n"));
        if !status.is_empty() {
            out.push_str("\n[status]\n");
            for (node, s) in status {
                out.push_str(&format!("{node} = \"{s}\"\n"));
            }
        }
        if !gates.is_empty() {
            out.push_str("\n[gates]\n");
            for (mesh, node) in gates {
                out.push_str(&format!("{mesh} = \"{node}\"\n"));
            }
        }
        out
    }

    /// One node's charter line, minted on its own machine, at a declared
    /// address. `node_line` writes a line with no address at all (which the
    /// charter reads as `poll`), so a test that needs a hop this box can DIAL
    /// has to say so.
    fn line_at(root: &Path, name: &str, address: &str) -> String {
        let line = line_for(root, name);
        if address == charter::DEFAULT_ADDRESS {
            return line;
        }
        let at = line.replacen(" }", &format!(", address = \"{address}\" }}"), 1);
        assert!(at.contains(address), "line at {address}: {at}");
        at
    }

    /// The hop each box of the five-edge fixture answers on: `chiyo` is the
    /// `poll` node a route ends at (no inbound transport at all), every other
    /// box answers on ssh.
    fn hop_of(name: &str) -> String {
        match name {
            "chiyo" => charter::DEFAULT_ADDRESS.to_string(),
            other => format!("ssh://{other}"),
        }
    }

    /// The five-edge fixture's home mesh: osaka, the relay sakaki, yomi and the
    /// `poll` node chiyo — each at its own hop, gated into `away` through
    /// sakaki, with yomi declared `down`.
    fn home_of(nodes: &[(PathBuf, &str)]) -> String {
        body(
            &["sakaki"],
            &[("yomi", "down")],
            &[("away", "sakaki")],
            &nodes
                .iter()
                .map(|(dir, name)| line_at(dir, name, &hop_of(name)))
                .collect::<Vec<_>>(),
        )
    }

    /// The one declaration in the set for `mesh`, which must have loaded.
    fn ok(mesh: &str) -> Declaration {
        let set = set();
        set.iter()
            .find(|l| l.mesh == mesh)
            .unwrap_or_else(|| panic!("`{mesh}` is in the set: {set:?}"))
            .declaration
            .clone()
            .unwrap_or_else(|e| panic!("`{mesh}` loads: {e}"))
    }

    /// The declarations in force, which must load: a config that will not is
    /// the set's own refusal, and its own test.
    fn set() -> Vec<Loaded> {
        declarations().unwrap_or_else(|e| panic!("the set loads: {e}"))
    }

    /// The refusal on `mesh`'s own entry.
    fn refused(mesh: &str) -> Refusal {
        let set = set();
        set.iter()
            .find(|l| l.mesh == mesh)
            .unwrap_or_else(|| panic!("`{mesh}` is in the set: {set:?}"))
            .declaration
            .as_ref()
            .err()
            .cloned()
            .unwrap_or_else(|| panic!("`{mesh}` is refused: {set:?}"))
    }

    #[test]
    fn a_charter_mesh_carries_status_and_gates_from_the_signed_file() {
        let (_guard, _env) = isolate();
        let scratch = Scratch::new("charter");
        let operator = scratch.dir("operator");
        let nodes: Vec<(PathBuf, &str)> =
            ["osaka", "sakaki", "yomi", "chiyo"].iter().map(|n| (scratch.dir(n), *n)).collect();
        sign_on(&operator, "home", &home_of(&nodes));

        let declaration = ok("home");
        assert!(declaration.is_charter(), "a charter in force is the declaration being read");
        assert_eq!(declaration.mesh(), "home");
        assert_eq!(declaration.relays(), ["sakaki".to_string()], "relays in declaration order");
        assert_eq!(declaration.status_of("yomi"), Some("down"));
        assert_eq!(declaration.status_of("osaka"), None, "no status is not the same fact as no node at all");
        assert_eq!(declaration.gates()["away"], "sakaki");
        assert_eq!(declaration.address_of("chiyo"), Some("poll"));
        assert_eq!(declaration.address_of("nobody"), None);
        assert_eq!(declaration.key_of("sakaki").unwrap().len(), 64, "bare lowercase hex");
    }

    #[test]
    fn a_pair_mesh_declares_relays_status_and_gates_and_they_validate() {
        let (_guard, _env) = isolate();
        let scratch = Scratch::new("pair");
        let root = scratch.dir("osaka");
        machine(&root, "osaka");
        std::fs::write(
            root.join("config.toml"),
            "[mesh.friends]\n\
             relays = [\"sakaki\"]\n\
             [mesh.friends.nodes]\n\
             sakaki = \"ssh://khoa@192.168.1.202\"\n\
             evo = \"ssh://evo@192.168.1.40\"\n\
             [mesh.friends.status]\n\
             evo = \"hold\"\n\
             [mesh.friends.gates]\n\
             home = \"sakaki\"\n\
             [mesh.home.nodes]\n\
             sakaki = \"ssh://khoa@192.168.1.202\"\n\
             [mesh.home.gates]\n\
             friends = \"sakaki\"\n",
        )
        .unwrap();

        let declaration = ok("friends");
        assert!(!declaration.is_charter(), "no charter is shaped here, so the config section IS the declaration");
        assert_eq!(declaration.relays(), ["sakaki".to_string()]);
        assert_eq!(declaration.status_of("evo"), Some("hold"));
        assert_eq!(declaration.gates()["home"], "sakaki");
        assert_eq!(declaration.address_of("sakaki"), Some("ssh://khoa@192.168.1.202"));
        assert_eq!(declaration.key_of("sakaki"), None, "no paired record, so no identity key comes from here");
    }

    #[test]
    fn a_config_operator_line_with_no_charter_state_loads_an_empty_pair_declaration() {
        let (_guard, _env) = isolate();
        let scratch = Scratch::new("operator-only");
        let root = scratch.dir("osaka");
        machine(&root, "osaka");
        std::fs::write(
            root.join("config.toml"),
            format!("[mesh.home]\n{}\n", charter::operator_line(&"ab".repeat(32))),
        )
        .unwrap();

        // No state, so this mesh is not charter-shaped and the section is what
        // is read — and the section may carry nothing beside the operator line,
        // so the declaration is empty: nothing to route by, which is the
        // fail-closed answer rather than a fallback to paired records.
        let declaration = ok("home");
        assert!(!declaration.is_charter(), "a mesh with no charter state is read through its config");
        assert!(declaration.relays().is_empty());
        assert!(declaration.gates().is_empty());
        assert_eq!(declaration.status_of("sakaki"), None);
        assert_eq!(declaration.key_of("sakaki"), None);
    }

    #[test]
    fn a_mesh_declared_nowhere_has_no_declaration() {
        let (_guard, _env) = isolate();
        let scratch = Scratch::new("absent");
        machine(&scratch.dir("osaka"), "osaka");
        // A mesh nothing declares is not in the set at all — this is what a
        // caller that NAMED a mesh it does not hold gets.
        assert!(set().is_empty(), "nothing is declared here");
        let refusal = Declaration::load("home").unwrap_err();
        assert_eq!(refusal.reason, NO_DECLARATION, "{refusal}");
    }

    /// Two charters that AGREE on a name are that name's authority: a stale
    /// pair record disagreeing with them is the copy refused, and the charters
    /// keep routing.
    #[test]
    fn an_agreeing_charter_is_not_refused_by_a_stale_pair_record() {
        let (_guard, _env) = isolate();
        let scratch = Scratch::new("agreeing");
        let operator = scratch.dir("operator");
        let sakaki_line = line_for(&scratch.dir("sakaki"), "sakaki");
        sign_on(&operator, "home", &body(&[], &[], &[], &[sakaki_line.clone()]));
        sign_on(&operator, "away", &body(&[], &[], &[], &[sakaki_line]));
        let key = ok("home").key_of("sakaki").unwrap().to_string();

        std::fs::write(
            operator.join("config.toml"),
            "[mesh.friends.nodes]\nsakaki = \"ssh://khoa@192.168.1.202\"\n",
        )
        .unwrap();
        node_store::save_nodes(&[record("sakaki", &"cd".repeat(32))]).unwrap();

        // The charters agree, so the key they carry is the authority.
        assert_eq!(ok("home").key_of("sakaki"), Some(key.as_str()));
        assert_eq!(ok("away").key_of("sakaki"), Some(key.as_str()));
        // Only the record that disagrees with it is refused.
        let refusal = refused("friends");
        assert_eq!(refusal.reason, KEY_DIVERGENCE, "{refusal}");
        assert!(refusal.detail.contains("sakaki"), "{refusal}");
    }

    #[test]
    fn a_config_that_cannot_be_read_refuses_the_whole_set() {
        let (_guard, _env) = isolate();
        let scratch = Scratch::new("broken-config");
        let operator = scratch.dir("operator");
        let nodes: Vec<(PathBuf, &str)> =
            ["osaka", "sakaki", "yomi", "chiyo"].iter().map(|n| (scratch.dir(n), *n)).collect();
        sign_on(&operator, "home", &home_of(&nodes));

        // A config that will not load is not a name source: the pair meshes it
        // would declare cannot even be LISTED, so the set refuses as a set
        // rather than dropping them and routing the rest of the table.
        std::fs::write(operator.join("config.toml"), "this is not = toml at all\n").unwrap();
        let refusal = declarations().expect_err("an unreadable config refuses the set");
        assert_eq!(refusal.reason, charter::CONFIG_UNREADABLE, "{refusal}");
        assert!(refusal.detail.contains("config.toml"), "{refusal}");
    }

    #[test]
    fn a_joined_mesh_with_no_charter_in_force_is_refused_by_itself_alone() {
        let (_guard, _env) = isolate();
        let scratch = Scratch::new("joined");
        let operator = scratch.dir("operator");
        let nodes: Vec<(PathBuf, &str)> =
            ["osaka", "sakaki", "yomi", "chiyo"].iter().map(|n| (scratch.dir(n), *n)).collect();
        sign_on(&operator, "home", &home_of(&nodes));
        let home = ok("home");

        // A mesh the operator was joined to by key: a trust record, no charter
        // accepted yet. Its own entry refuses; `home` still loads.
        let trust = charter::Trust { operator: home.key_of("sakaki").unwrap().to_string(), ..Default::default() };
        std::fs::create_dir_all(charter::mesh_state_dir("away")).unwrap();
        std::fs::write(charter::trust_path("away"), serde_json::to_string(&trust).unwrap()).unwrap();
        assert!(charter::charter_shaped("away"), "a non-empty operator in trust.json shapes the mesh");
        assert!(!charter::in_force_path("away").exists(), "and nothing has been accepted for it");

        let refusal = refused("away");
        assert_eq!(refusal.reason, charter::NO_CHARTER_IN_FORCE, "{refusal}");
        assert!(refusal.detail.contains("away"), "{refusal}");
        assert_eq!(ok("home").key_of("sakaki"), Some(home.key_of("sakaki").unwrap()), "home is untouched");
    }

    #[test]
    fn a_corrupt_or_foreign_charter_is_refused_as_tampered_and_never_as_a_pair_mesh() {
        let (_guard, _env) = isolate();
        let scratch = Scratch::new("corrupt");
        let operator = scratch.dir("operator");
        let osaka_line = line_for(&scratch.dir("osaka"), "osaka");
        let nodes: Vec<(PathBuf, &str)> =
            ["osaka", "sakaki", "yomi", "chiyo"].iter().map(|n| (scratch.dir(n), *n)).collect();
        sign_on(&operator, "home", &home_of(&nodes));
        // `away` is charter-shaped (a trust record), and the document in force
        // for it is not a charter at all.
        let trust = charter::Trust { operator: "ab".repeat(32), ..Default::default() };
        std::fs::create_dir_all(charter::mesh_state_dir("away")).unwrap();
        std::fs::write(charter::trust_path("away"), serde_json::to_string(&trust).unwrap()).unwrap();
        std::fs::write(charter::in_force_path("away"), "not a charter at all").unwrap();
        let refusal = refused("away");
        assert_eq!(refusal.reason, charter::CHARTER_TAMPERED, "{refusal}");

        // A document that parses but declares another mesh is the same answer:
        // it is not THIS mesh's charter.
        std::fs::write(
            charter::in_force_path("away"),
            format!("mesh = \"home\"\nversion = 1\nrelays = []\n\n[nodes]\n{osaka_line}\n"),
        )
        .unwrap();
        let refusal = refused("away");
        assert_eq!(refusal.reason, charter::CHARTER_TAMPERED, "{refusal}");
        assert!(refusal.detail.contains("home"), "{refusal}");
    }

    #[test]
    fn status_is_read_by_declared_name_not_by_nickname() {
        let (_guard, _env) = isolate();
        let scratch = Scratch::new("nickname");
        let operator = scratch.dir("operator");
        let sakaki_line = line_for(&scratch.dir("sakaki"), "sakaki");
        let yomi_line = line_for(&scratch.dir("yomi"), "yomi");
        sign_on(&operator, "home", &body(&[], &[("sakaki", "down")], &[], &[yomi_line, sakaki_line]));

        // A record for the SAME key under a nickname: policy still reads the
        // charter line's name, never the record's.
        let key = ok("home").key_of("sakaki").unwrap().to_string();
        node_store::save_nodes(&[record("sakaki-router", &key)]).unwrap();

        let declaration = ok("home");
        assert_eq!(declaration.status_of("sakaki"), Some("down"));
        assert_eq!(declaration.status_of("sakaki-router"), None, "a nickname is not a declared node");
        assert_eq!(declaration.name_of_key(&key), Some("sakaki"), "the charter line's name, never the record's");
    }

    #[test]
    fn a_pair_mesh_resolves_a_key_to_its_paired_record_name() {
        let (_guard, _env) = isolate();
        let scratch = Scratch::new("pair-key");
        let root = scratch.dir("osaka");
        machine(&root, "osaka");
        std::fs::write(root.join("config.toml"), "[mesh.friends.nodes]\nevo = \"ssh://evo@192.168.1.40\"\n").unwrap();
        let key = "ab".repeat(32);
        node_store::save_nodes(&[record("evo", &key)]).unwrap();

        let declaration = ok("friends");
        assert_eq!(declaration.key_of("evo"), Some(key.as_str()));
        assert_eq!(declaration.name_of_key(&key), Some("evo"));
        assert_eq!(declaration.name_of_key(&"cd".repeat(32)), None, "a stranger's key resolves to no name");
    }

    #[test]
    fn one_node_with_two_keys_anywhere_is_a_load_error() {
        let (_guard, _env) = isolate();
        let scratch = Scratch::new("two-keys");
        let operator = scratch.dir("operator");
        let sakaki_line = line_for(&scratch.dir("sakaki"), "sakaki");
        let two_keys = line_for(&scratch.dir("imposter"), "sakaki");
        sign_on(&operator, "home", &body(&[], &[], &[], &[sakaki_line]));
        sign_on(&operator, "away", &body(&[], &[], &[], &[two_keys]));

        // Two charters, one name, two keys: NEITHER is an authority over the
        // other, so neither can be kept — keeping one by mesh-name order would
        // leave the imposter routing behind it. Both are refused, and the rest
        // of each mesh's own facts stand as the file wrote them.
        for mesh in ["home", "away"] {
            let refusal = refused(mesh);
            assert_eq!(refusal.reason, KEY_DIVERGENCE, "{refusal}");
            assert!(refusal.detail.contains("sakaki"), "{refusal}");
        }
    }

    #[test]
    fn a_record_left_behind_by_a_charter_rekey_is_refused_on_the_pair_mesh() {
        let (_guard, _env) = isolate();
        let scratch = Scratch::new("rekeyed");
        let operator = scratch.dir("operator");
        let sakaki_line = line_for(&scratch.dir("sakaki"), "sakaki");
        sign_on(&operator, "home", &body(&[], &[], &[], &[sakaki_line]));
        let current = ok("home").key_of("sakaki").unwrap().to_string();

        // A PAIR mesh whose paired record for the same name still holds the key
        // from before the charter re-keyed it — the stale copy a real host keeps
        // from the pairing that came before the charter. The charter is the
        // authority for a name it lists, so the record is what yields, and the
        // pair mesh is the mesh refused.
        std::fs::write(
            operator.join("config.toml"),
            "[mesh.friends.nodes]\nsakaki = \"ssh://khoa@192.168.1.202\"\n",
        )
        .unwrap();
        node_store::save_nodes(&[record("sakaki", &"cd".repeat(32))]).unwrap();
        let refusal = refused("friends");
        assert_eq!(refusal.reason, KEY_DIVERGENCE, "{refusal}");
        assert!(refusal.detail.contains("sakaki"), "{refusal}");
        assert_eq!(ok("home").key_of("sakaki"), Some(current.as_str()), "the charter's key is the live one");
    }

    #[test]
    fn one_node_one_key_across_two_meshes_loads() {
        let (_guard, _env) = isolate();
        let scratch = Scratch::new("one-key");
        let operator = scratch.dir("operator");
        let sakaki_line = line_for(&scratch.dir("sakaki"), "sakaki");
        let evo_line = line_for(&scratch.dir("evo"), "evo");
        sign_on(&operator, "home", &body(&[], &[], &[], &[sakaki_line.clone()]));
        sign_on(&operator, "away", &body(&[], &[], &[], &[sakaki_line, evo_line]));

        assert_eq!(ok("home").key_of("sakaki"), ok("away").key_of("sakaki"), "one name, one key");
    }

    /// **A one-sided gate refuses the crossing even though the gate's own charter
    /// parses.** The set refuses the mesh that declares a gate the other side does
    /// not answer (`one-sided-gate`), and the hop chain reads THAT answer: a hop
    /// carrying a letter out of a mesh whose declaration the set refuses has no
    /// declared gate, so the crossing is the zone wall. Resolving a gate by
    /// loading the file directly — outside the validated set — is what would wave
    /// it through, and the walk calls this one function.
    #[test]
    fn a_one_sided_gate_refused_by_the_set_answers_no_crossing() {
        let (_guard, _env) = isolate();
        let scratch = Scratch::new("one-sided-gate");
        let operator = scratch.dir("operator");
        let line = line_for(&scratch.dir("sakaki"), "sakaki");
        sign_on(&operator, "home", &body(&[], &[], &[("away", "sakaki")], &[line.clone()]));
        sign_on(&operator, "away", &body(&[], &[], &[], &[line]));

        assert_eq!(refused("home").reason, ONE_SIDED_GATE);
        let set = crate::routing::declarations().unwrap();
        assert!(
            !gate_in(&set, "home", "away", "sakaki"),
            "the gate question reads the set's refusal, not `home`'s own file"
        );
        assert!(!gate_in(&set, "away", "home", "sakaki"), "and the mesh that answered nothing has no gate");
    }

    #[test]
    fn a_one_sided_gate_across_two_charters_is_refused() {
        let (_guard, _env) = isolate();
        let scratch = Scratch::new("one-sided");
        let operator = scratch.dir("operator");
        let line = line_for(&scratch.dir("sakaki"), "sakaki");
        sign_on(&operator, "home", &body(&[], &[], &[("away", "sakaki")], &[line.clone()]));
        sign_on(&operator, "away", &body(&[], &[], &[], &[line]));

        let refusal = refused("home");
        assert_eq!(refusal.reason, ONE_SIDED_GATE, "{refusal}");
        assert!(refusal.detail.contains("away"), "{refusal}");
        assert_eq!(ok("away").gates().len(), 0, "the mesh that declared no gate still loads");
    }

    #[test]
    fn a_symmetric_gate_across_two_charters_loads() {
        let (_guard, _env) = isolate();
        let scratch = Scratch::new("symmetric");
        let operator = scratch.dir("operator");
        let line = line_for(&scratch.dir("sakaki"), "sakaki");
        let evo_line = line_for(&scratch.dir("evo"), "evo");
        sign_on(&operator, "home", &body(&[], &[], &[("away", "sakaki")], &[line.clone()]));
        sign_on(&operator, "away", &body(&[], &[], &[("home", "sakaki")], &[line, evo_line]));

        let away = ok("away");
        assert_eq!(away.gates()["home"], "sakaki");
        assert!(away.relays().is_empty(), "a mesh that declares no relay has none");
        assert_eq!(away.key_of("evo").unwrap().len(), 64);
    }

    #[test]
    fn a_mesh_is_never_its_own_gate() {
        let refusal = gate_verdict("home", "home", "sakaki", None).expect("a self-gate is refused");
        assert_eq!(refusal.reason, ONE_SIDED_GATE, "{refusal}");
        assert!(refusal.detail.contains("itself"), "{refusal}");

        let mut back = BTreeMap::new();
        back.insert("home".to_string(), "sakaki".to_string());
        assert!(gate_verdict("home", "away", "sakaki", Some(&back)).is_none(), "answered both ways");
        assert!(
            gate_verdict("home", "away", "sakaki", None).is_none(),
            "a mesh this host does not hold cannot be judged here"
        );
        assert!(gate_verdict("home", "away", "sakaki", Some(&BTreeMap::new())).is_some(), "one-sided");
    }

    // ── The four steps ──────────────────────────────────────────────────

    /// The five-edge fixture's two meshes, signed on the operator's box, with
    /// the env left pointing at it — so `declarations()` reads both. `home` is
    /// {osaka, sakaki (relay + gate), yomi (`down`), chiyo (`poll`)} and `away`
    /// is {evo, sakaki}.
    fn five_edges(scratch: &Scratch) -> PathBuf {
        let operator = scratch.dir("operator");
        let nodes: Vec<(PathBuf, &str)> =
            ["osaka", "sakaki", "yomi", "chiyo"].iter().map(|n| (scratch.dir(n), *n)).collect();
        let sakaki = line_for(&scratch.dir("sakaki"), "sakaki");
        let evo = line_for(&scratch.dir("evo"), "evo");
        sign_on(&operator, "home", &home_of(&nodes));
        sign_on(&operator, "away", &body(&["sakaki"], &[], &[("home", "sakaki")], &[evo, sakaki]));
        operator
    }

    /// One route, asked of the declarations currently in force, with the trail
    /// joined for an assertion that names the step it came from.
    fn route(from: &str, to: &str, mesh: &str) -> Route {
        Letter { from, to, mesh }.route(&set())
    }

    fn next_of(route: &Route) -> &Hop {
        route.outcome.as_ref().unwrap_or_else(|e| panic!("expected a hop, got {e}\n{}", trail(route)))
    }

    fn trail(route: &Route) -> String {
        route.trail.join("\n")
    }

    #[test]
    fn the_router_prefers_a_reachable_destination_in_the_letters_mesh() {
        let (_guard, _env) = isolate();
        let scratch = Scratch::new("prefer");
        let operator = scratch.dir("operator");
        let nodes: Vec<(PathBuf, &str)> =
            ["osaka", "sakaki", "yomi"].iter().map(|n| (scratch.dir(n), *n)).collect();
        // Two relays, and the destination is a plain member: the destination
        // wins at every hop, so the first relay is never consulted.
        let lines: Vec<String> =
            nodes.iter().map(|(dir, name)| line_at(dir, name, &format!("ssh://{name}"))).collect();
        sign_on(&operator, "home", &body(&["sakaki", "osaka"], &[], &[], &lines));

        let at_origin = route("osaka", "yomi", "home");
        assert_eq!(next_of(&at_origin).next, "yomi", "{}", trail(&at_origin));
        assert!(!next_of(&at_origin).held);
        assert_eq!(at_origin.trail.len(), 1, "step 1 decided it: {}", trail(&at_origin));
        assert!(trail(&at_origin).starts_with("step 1:"), "{}", trail(&at_origin));

        // The same at a hop: `sakaki` is a declared relay AND can deliver.
        let at_relay = route("sakaki", "yomi", "home");
        assert_eq!(next_of(&at_relay).next, "yomi", "{}", trail(&at_relay));
        assert_eq!(at_relay.trail.len(), 1, "{}", trail(&at_relay));
    }

    #[test]
    fn an_unshared_mesh_is_no_route() {
        let (_guard, _env) = isolate();
        let scratch = Scratch::new("unshared");
        let operator = scratch.dir("operator");
        let osaka = line_for(&scratch.dir("osaka"), "osaka");
        let stranger = line_for(&scratch.dir("stranger"), "stranger");
        // Neither mesh declares a relay or a gate, which is the shape a mesh
        // that shares nothing has.
        sign_on(&operator, "home", &body(&[], &[], &[], &[osaka]));
        sign_on(&operator, "club", &body(&[], &[], &[], &[stranger]));

        let out = route("osaka", "stranger", "home");
        assert_eq!(outcome_word(&out), Some(NO_ROUTE), "{}", trail(&out));
        assert_eq!(out.trail.len(), 3, "steps 1 and 2 passed before step 4: {}", trail(&out));
    }

    fn outcome_word(route: &Route) -> Option<&str> {
        route.outcome.as_ref().err().map(|r| r.reason.as_str())
    }

    #[test]
    fn a_down_relay_is_skipped_and_the_next_declared_one_taken() {
        let (_guard, _env) = isolate();
        let scratch = Scratch::new("down-relay");
        let operator = scratch.dir("operator");
        let home_nodes: Vec<(PathBuf, &str)> =
            ["osaka", "sakaki", "chiyo"].iter().map(|n| (scratch.dir(n), *n)).collect();
        let evo = line_for(&scratch.dir("evo"), "evo");
        sign_on(&operator, "home", &home_of_relays(&home_nodes, &["sakaki", "chiyo"], &[("sakaki", "down")]));
        sign_on(&operator, "away", &body(&[], &[], &[], &[evo]));

        let out = route("osaka", "evo", "home");
        assert_eq!(next_of(&out).next, "chiyo", "{}", trail(&out));
        assert!(trail(&out).contains("`sakaki` is declared `down`"), "{}", trail(&out));
    }

    /// `home_of`'s shape with the relays and statuses a test varies, and no
    /// gate — the mesh a letter leaves for another one by relay alone. Every
    /// box here answers on ssh, including the relays: a relay this hop cannot
    /// DIAL is not one it can hand a letter to.
    fn home_of_relays(nodes: &[(PathBuf, &str)], relays: &[&str], status: &[(&str, &str)]) -> String {
        body(
            relays,
            status,
            &[],
            &nodes
                .iter()
                .map(|(dir, name)| line_at(dir, name, &format!("ssh://{name}")))
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn a_letter_to_a_down_destination_is_no_route_and_is_not_handed_to_a_relay() {
        let (_guard, _env) = isolate();
        let scratch = Scratch::new("down-dest");
        let _operator = five_edges(&scratch);

        // `yomi` is declared `down` in `home`, and `home` has a relay that can
        // take the letter — which must not: `down` refuses fast, and a relay
        // that holds a `down` node's letters is a letter moved toward a node
        // the mesh declared unreachable.
        let out = route("osaka", "yomi", "home");
        assert_eq!(outcome_word(&out), Some(NO_ROUTE), "{}", trail(&out));
        assert!(trail(&out).contains("declared `down`"), "{}", trail(&out));
        assert!(!trail(&out).contains("takes it"), "no hop was picked: {}", trail(&out));
    }

    /// **A `down` destination in a PAIR mesh is `no-route`, and is never handed
    /// to the mesh's relay.** `mesh pair` writes an address per node and a
    /// `[status]` line for the ones the operator quarantines; the relay here can
    /// take the letter and must not, and the destination itself must not be
    /// picked — `down` refuses fast, wherever the declaration lives.
    #[test]
    fn a_down_destination_in_a_pair_mesh_is_no_route_and_is_not_handed_to_a_relay() {
        let (_guard, _env) = isolate();
        let scratch = Scratch::new("pair-down");
        let root = scratch.dir("osaka");
        machine(&root, "osaka");
        std::fs::write(
            root.join("config.toml"),
            "[mesh.home]\nrelays = [\"relay\"]\n\
             [mesh.home.nodes]\nrelay = \"ssh://relay\"\nevo = \"ssh://evo\"\n\
             [mesh.home.status]\nevo = \"down\"\n",
        )
        .unwrap();
        node_store::save_nodes(&[record("relay", &"ab".repeat(32)), record("evo", &"cd".repeat(32))]).unwrap();

        let set = set();
        assert_eq!(status_of(&set, "home", "evo"), Some("down"), "the one predicate, by name");
        assert_eq!(status_of(&set, "home", "relay"), None, "no status is not `down`");
        assert_eq!(status_of(&set, "nowhere", "evo"), None, "a mesh the set does not hold names nothing");

        let out = route("relay", "evo", "home");
        assert_eq!(outcome_word(&out), Some(NO_ROUTE), "{}", trail(&out));
        assert!(trail(&out).contains("declared `down`"), "{}", trail(&out));
        assert!(!trail(&out).contains("takes it"), "no hop was picked: {}", trail(&out));
    }

    /// Step 3 is for a destination the letter's mesh does NOT declare. A node
    /// this mesh declares is this mesh's own problem, however many other meshes
    /// this box also knows it in.
    #[test]
    fn a_destination_in_the_letters_own_mesh_is_never_carried_across_a_gate() {
        let (_guard, _env) = isolate();
        let scratch = Scratch::new("own-mesh");
        let operator = scratch.dir("operator");
        let osaka = line_at(&scratch.dir("osaka"), "osaka", "ssh://osaka");
        let sakaki = line_at(&scratch.dir("sakaki"), "sakaki", "ssh://sakaki");
        // `both` sits in BOTH meshes — one key, one machine — and takes no
        // inbound connection in `home`, whose only edge to it would be through
        // the gate into `away`.
        let both = line_at(&scratch.dir("both"), "both", charter::DEFAULT_ADDRESS);
        let evo = line_at(&scratch.dir("evo"), "evo", "ssh://evo");
        sign_on(
            &operator,
            "home",
            &body(&[], &[], &[("away", "sakaki")], &[osaka, sakaki.clone(), both.clone()]),
        );
        sign_on(&operator, "away", &body(&[], &[], &[("home", "sakaki")], &[evo, sakaki, both]));

        let out = route("osaka", "both", "home");
        assert_eq!(outcome_word(&out), Some(NO_ROUTE), "{}", trail(&out));
        assert!(!trail(&out).contains("step 3"), "no gate was consulted: {}", trail(&out));
    }

    #[test]
    fn a_poll_destination_spools_held_at_its_relay_at_the_route_step() {
        let (_guard, _env) = isolate();
        let scratch = Scratch::new("poll");
        let _operator = five_edges(&scratch);

        // From a plain member the poll node is not this box's to hold: the
        // letter goes to the relay.
        let from_member = route("osaka", "chiyo", "home");
        assert_eq!(next_of(&from_member).next, "sakaki", "{}", trail(&from_member));
        assert!(!next_of(&from_member).held);
        assert!(trail(&from_member).contains("asks its relay"), "{}", trail(&from_member));

        // At the relay it is held for the node's own poll.
        let at_relay = route("sakaki", "chiyo", "home");
        assert_eq!(next_of(&at_relay).next, "chiyo", "{}", trail(&at_relay));
        assert!(next_of(&at_relay).held, "{}", trail(&at_relay));
    }

    #[test]
    fn a_dual_member_that_is_not_the_gate_refuses_zone_violation() {
        let (_guard, _env) = isolate();
        let scratch = Scratch::new("wall");
        let operator = scratch.dir("operator");
        let a = line_for(&scratch.dir("a"), "a");
        let b = line_for(&scratch.dir("b"), "b");
        let evo = line_for(&scratch.dir("evo"), "evo");
        // Two meshes that share one machine and declare no gate between them.
        sign_on(&operator, "alpha", &body(&[], &[], &[], &[a, evo.clone()]));
        sign_on(&operator, "beta", &body(&[], &[], &[], &[b, evo]));

        // `evo` knows both and may not bridge: the destination is in `beta`,
        // the letter rides `alpha`, and no gate crosses.
        let out = route("evo", "b", "alpha");
        assert_eq!(outcome_word(&out), Some(seal::ZONE_VIOLATION), "{}", trail(&out));

        // A box in `alpha` alone has no wall to be stopped by — it simply has
        // no route.
        let plain = route("a", "b", "alpha");
        assert_eq!(outcome_word(&plain), Some(NO_ROUTE), "{}", trail(&plain));
    }

    #[test]
    fn a_symmetric_gate_rewrites_the_mesh_and_the_steps_restart() {
        let (_guard, _env) = isolate();
        let scratch = Scratch::new("gate");
        let _operator = five_edges(&scratch);

        // A plain member cannot bridge either: it hands the letter to the gate
        // and the mesh it rides stays `home`.
        let from_member = route("osaka", "evo", "home");
        assert_eq!(next_of(&from_member).next, "sakaki", "{}", trail(&from_member));
        assert_eq!(next_of(&from_member).mesh, "home");

        // The gate itself rewrites the mesh and restarts: `evo` is a node of
        // `away`, reachable there.
        let at_gate = route("sakaki", "evo", "home");
        assert_eq!(next_of(&at_gate).next, "evo", "{}", trail(&at_gate));
        assert_eq!(next_of(&at_gate).mesh, "away", "{}", trail(&at_gate));
        assert!(trail(&at_gate).contains("rewritten"), "{}", trail(&at_gate));
    }

    #[test]
    fn a_refused_mesh_routes_nothing_and_the_other_meshes_still_route() {
        let (_guard, _env) = isolate();
        let scratch = Scratch::new("refused");
        let operator = scratch.dir("operator");
        let osaka = line_at(&scratch.dir("osaka"), "osaka", "ssh://osaka");
        let sakaki = line_at(&scratch.dir("sakaki"), "sakaki", "ssh://sakaki");
        let evo = line_at(&scratch.dir("evo"), "evo", "ssh://evo");
        // `away` gates into `home` and `home` does not answer back: the gate is
        // one-sided, so `away`'s own declaration is the one refused.
        sign_on(&operator, "home", &body(&["sakaki"], &[], &[], &[osaka, sakaki.clone()]));
        sign_on(&operator, "away", &body(&[], &[], &[("home", "sakaki")], &[evo, sakaki]));
        assert_eq!(refused("away").reason, ONE_SIDED_GATE);

        let refused_route = route("osaka", "osaka", "away");
        assert_eq!(outcome_word(&refused_route), Some(ONE_SIDED_GATE), "{}", trail(&refused_route));

        // The mesh that is not inconsistent keeps routing.
        let other = route("osaka", "sakaki", "home");
        assert_eq!(next_of(&other).next, "sakaki", "{}", trail(&other));
    }
}
