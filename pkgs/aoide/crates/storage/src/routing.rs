//! The mesh declaration seam (P-M4 slice 1, `docs/architecture/MAIL.md`
//! §Transit): one read that answers how one mesh routes — its relays in
//! preference order, a node's declared status, the gates out of it, a node's
//! address and its identity key, and the reverse: which declared NAME a
//! verifying key belongs to.
//!
//! **Two places hold a declaration, and this is the one place that reads
//! either** (decision D1). A CHARTER mesh's is its signed charter — `relays`,
//! each node's `address`, and optional `[status]`/`[gates]`, signed by the
//! operator; its config names only the operator key, and the validator refuses
//! a node list, a grant or a transit table beside that line
//! (`config::validate_mesh`). A PAIR mesh's is its `[mesh.<name>]` section. So
//! there is exactly one resolution for the door and the router, and never a
//! merge: a mesh's routing table has one source, and a second one is a load
//! error rather than a precedence question.
//!
//! **Which kind a mesh is, is [`charter::charter_shaped`]'s answer, never a
//! second discovery path.** That function reads STATE ONLY, on purpose (its
//! own doc: a config that cannot be read must leave a chartered mesh
//! fail-closed rather than silently resolving to "no charter here"), so this
//! seam asks it the same question the door asks and gets the same answer. A
//! mesh that is charter-shaped and holds no readable charter in force has NO
//! declaration here — a refusal, never an empty table to fall back to.
//!
//! **Pure where it can be, one read per call site.** [`Declaration::load`]
//! does the I/O once (the charter in force, or the config section plus the
//! paired records a pair mesh's keys come from); every accessor is a lookup on
//! what was read. [`declarations`] loads every mesh this host declares or
//! holds state for, and refuses the two invariants only the SET can see: one
//! node name never carries two identity keys, and a gate is answered by the
//! mesh on the other side.
//!
//! **Names in policy come from the declaration, and only from the declaration
//! (decision D5, ruled 2026-09-29).** A charter mesh resolves a verifying key
//! to the NAME ON ITS CHARTER LINE — for policy, for routing AND for the audit
//! stamp on the line a request leaves — because the line is that mesh's
//! authority; the `nodes.json` nickname is a display fact and never an input
//! to policy (MAIL.md §Transit). Only a pair mesh reads a name out of
//! `state/nodes.json` ([`Declaration::name_of_key`]), because there the paired
//! record is the whole declaration. The door's own caller resolution still
//! prefers a matching record's name (`aoide-server::a2a`), so its audit stamp
//! changes when that call site moves onto this accessor — a wire-visible
//! change the ruling already made, landing with the door's declaration read.

use std::collections::{BTreeMap, BTreeSet};

use crate::charter::{self, Charter, Refusal};
use crate::{config, node_store};

/// No mesh of this name is declared here — neither a charter in force nor a
/// `[mesh.<name>]` config section.
pub const NO_DECLARATION: &str = "no-declaration";
/// One node name carries two different identity keys across two charters —
/// "one node, one identity key, in every mesh" (MAIL.md §Transit).
pub const KEY_DIVERGENCE: &str = "key-divergence";
/// A gate the mesh on the other side does not answer — a gate is symmetric or
/// it is nothing (MAIL.md §Transit).
pub const ONE_SIDED_GATE: &str = "one-sided-gate";

/// One mesh's routing declaration. [`Declaration::load`] is the only
/// constructor: it decides the kind, reads that kind's single source, and
/// answers every accessor below off it.
#[derive(Debug, Clone, PartialEq)]
pub struct Declaration {
    mesh: String,
    kind: Kind,
}

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    /// A charter mesh: the signed document IS the whole declaration.
    Charter(Charter),
    /// A pair mesh (or a mesh whose state holds no charter): the config
    /// section, with keys read out of the paired records it names.
    Pair(Pair),
}

/// A `[mesh.<name>]` section, with the identity keys of the records it names
/// gathered once at load.
#[derive(Debug, Clone, PartialEq, Default)]
struct Pair {
    relays: Vec<String>,
    status: BTreeMap<String, String>,
    gates: BTreeMap<String, String>,
    /// Node name → its declared address/hop, exactly as config writes it.
    addresses: BTreeMap<String, String>,
    /// Node name → the identity key of its VERIFIED paired record, for the
    /// nodes that have one. A record that is unpaired, unverified, or holds no
    /// `pubkey` contributes nothing: an identity is what a pairing proved, and
    /// there is no second source for one.
    keys: BTreeMap<String, String>,
}

impl Declaration {
    /// Read `mesh`'s declaration: the charter in force where the mesh is
    /// charter-shaped, its `[mesh.<name>]` section otherwise. Fails closed —
    /// a charter-shaped mesh with no readable charter is a refusal, never the
    /// pair path (that is the stale-records door the state-only kind test
    /// exists to keep shut).
    pub fn load(mesh: &str) -> Result<Declaration, Refusal> {
        if charter::charter_shaped(mesh) {
            let charter = charter::governing(mesh).ok_or_else(|| {
                charter::trusted_operator(mesh).err().unwrap_or_else(|| {
                    Refusal::new(
                        charter::LOCAL_IO,
                        format!(
                            "mesh `{mesh}` is charter-shaped at this node but holds no readable charter \
                             in force — a charter that does not parse is read as none (`aoide mesh charter \
                             show {mesh}` reads what is on disk)"
                        ),
                    )
                })
            })?;
            return Ok(Declaration { mesh: mesh.to_string(), kind: Kind::Charter(charter) });
        }
        let loaded = config::load().map_err(|e| Refusal::new(charter::CONFIG_UNREADABLE, e.to_string()))?;
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
        Ok(Declaration { mesh: mesh.to_string(), kind: Kind::Pair(pair_from(section)) })
    }

    /// The mesh this declaration is for.
    pub fn mesh(&self) -> &str {
        &self.mesh
    }

    /// Is this mesh a charter mesh? `true` exactly when the signed charter is
    /// the declaration being read.
    pub fn is_charter(&self) -> bool {
        matches!(self.kind, Kind::Charter(_))
    }

    /// The mesh's relays, in declaration order — the preference order step 2
    /// of MAIL.md's route reads. Empty means this mesh declares no transit
    /// hub, so a destination it does not itself hold is `no-route`.
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

    /// The address `node` is declared at — `ssh://…`, `https://…` or `poll` —
    /// or `None` for a node this declaration does not name. The address is a
    /// declaration, not a dial target: turning it into one is the caller's
    /// (`docs/architecture/HTTPS-MESH-API.md` "Transports and relays").
    pub fn address_of(&self, node: &str) -> Option<&str> {
        match &self.kind {
            Kind::Charter(c) => c.nodes.get(node).map(|line| line.address.as_str()),
            Kind::Pair(p) => p.addresses.get(node).map(String::as_str),
        }
    }

    /// The identity key (bare lowercase hex) this declaration gives `node`, or
    /// `None`. For a charter mesh that is the charter's line; for a pair mesh
    /// it is the node's verified paired record — and nothing else, because a
    /// relay never supplies a key (MAIL.md §Transit, "Keys come from trust").
    pub fn key_of(&self, node: &str) -> Option<&str> {
        match &self.kind {
            Kind::Charter(c) => c.nodes.get(node).map(|line| line.key.as_str()),
            Kind::Pair(p) => p.keys.get(node).map(String::as_str),
        }
    }

    /// The declared NAME the identity key `key` belongs to, for the policy and
    /// audit lookups that start from a verifying key (decision D5). A charter
    /// mesh answers from its own node lines; a pair mesh answers from its
    /// paired records. Bare hex, case-insensitive, the same comparison
    /// [`Charter::grant_for_key`] makes.
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

    /// Every node this declaration names, with the key its own source gives it
    /// — what [`declarations`]' cross-mesh invariant compares.
    fn declared_keys(&self) -> Vec<(&str, &str)> {
        match &self.kind {
            Kind::Charter(c) => c.nodes.iter().map(|(n, l)| (n.as_str(), l.key.as_str())).collect(),
            Kind::Pair(p) => p.keys.iter().map(|(n, k)| (n.as_str(), k.as_str())).collect(),
        }
    }
}

/// The `[mesh.<name>]` section as a declaration, with the identity keys of the
/// records it names read once.
fn pair_from(section: &config::Mesh) -> Pair {
    let records = node_store::load_nodes();
    let keys = section
        .nodes
        .keys()
        .filter_map(|node| {
            let record = records.iter().find(|r| &r.name == node)?;
            let pubkey = record.pubkey.as_ref()?;
            record.verified.then(|| (node.clone(), pubkey.clone()))
        })
        .collect();
    Pair {
        relays: section.relays.clone(),
        status: section.status.clone(),
        gates: section.gates.clone(),
        addresses: section.nodes.clone(),
        keys,
    }
}

/// Every mesh this host declares or holds charter state for — the same set
/// `aoide mesh` reports (`config`'s `[mesh.<name>]` sections plus
/// [`charter::meshes_with_state`]), sorted, each read once, and then the two
/// invariants that are only true of the set checked. One unloadable mesh
/// refuses the whole set: a partial answer is how a routing decision gets made
/// against half a declaration.
pub fn declarations() -> Result<Vec<Declaration>, Refusal> {
    let loaded = config::load().map_err(|e| Refusal::new(charter::CONFIG_UNREADABLE, e.to_string()))?;
    let mut names: BTreeSet<String> = loaded.config.mesh.keys().cloned().collect();
    names.extend(charter::meshes_with_state());
    let mut out = Vec::with_capacity(names.len());
    for mesh in &names {
        out.push(Declaration::load(mesh)?);
    }
    validate(&out)?;
    Ok(out)
}

/// The two invariants a single declaration cannot check, because each is a
/// statement about two of them:
///
/// 1. **One node, one identity key, in every mesh.** A node may sit in several
///    meshes with a different grant in each; identity is not per mesh
///    (MAIL.md decision 16), so a name carrying two keys across two charters
///    is a load error. Only charters can violate it: a pair mesh's key comes
///    from a record keyed by name alone, so two pair meshes cannot disagree,
///    and where a charter lists a name the paired record's key is inert
///    (`docs/architecture/HTTPS-MESH-API.md` "Keys").
/// 2. **A gate is symmetric or it is nothing.** Each gate a loaded mesh
///    declares must be answered, by the mesh it names, with the same node.
///    A gate into a mesh this host does not hold cannot be judged here and is
///    left to the two configs or charters that do (the same limit
///    `config::validate_pair_transit` states).
pub fn validate(set: &[Declaration]) -> Result<(), Refusal> {
    let mut keyed: BTreeMap<&str, (&str, &str)> = BTreeMap::new();
    for declaration in set.iter().filter(|d| d.is_charter()) {
        for (node, key) in declaration.declared_keys() {
            match keyed.get(node) {
                Some((first_key, first_mesh)) if !first_key.eq_ignore_ascii_case(key) => {
                    return Err(Refusal::new(
                        KEY_DIVERGENCE,
                        format!(
                            "`{node}` carries two identity keys — `{first_key}` in mesh `{first_mesh}` \
                             and `{key}` in mesh `{}`. One node, one identity key, in every mesh: a name \
                             that stands for two machines is a name no policy lookup can trust",
                            declaration.mesh()
                        ),
                    ));
                }
                Some(_) => {}
                None => {
                    keyed.insert(node, (key, declaration.mesh()));
                }
            }
        }
    }
    for declaration in set {
        for (other, gate) in declaration.gates() {
            let Some(back) = set.iter().find(|d| d.mesh() == other) else {
                continue;
            };
            if back.gates().get(declaration.mesh()).map(String::as_str) != Some(gate.as_str()) {
                return Err(Refusal::new(
                    ONE_SIDED_GATE,
                    format!(
                        "mesh `{}` gates into `{other}` through `{gate}`, but `{other}` does not gate \
                         back through it — a gate is symmetric or it is nothing",
                        declaration.mesh()
                    ),
                ));
            }
        }
    }
    Ok(())
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_test_support::{unique_tmp, EnvSaver};
    use std::path::{Path, PathBuf};
    use std::sync::MutexGuard;

    /// A fresh scratch directory standing in for one machine.
    fn machine_dir(tag: &str) -> PathBuf {
        let dir = unique_tmp(&format!("routing-{tag}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
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

    /// The five-edge fixture's home mesh (plan §3b): osaka, the relay sakaki,
    /// yomi and the `poll` node chiyo — gated into `away` through sakaki, with
    /// yomi declared `down`.
    fn home_of(nodes: &[(PathBuf, &str)]) -> String {
        body(
            &["sakaki"],
            &[("yomi", "down")],
            &[("away", "sakaki")],
            &nodes.iter().map(|(dir, name)| line_for(dir, name)).collect::<Vec<_>>(),
        )
    }

    #[test]
    fn a_charter_mesh_carries_status_and_gates_from_the_signed_file() {
        let (_guard, _env) = isolate();
        let operator = machine_dir("operator");
        let nodes: Vec<(PathBuf, &str)> =
            ["osaka", "sakaki", "yomi", "chiyo"].iter().map(|n| (machine_dir(n), *n)).collect();
        sign_on(&operator, "home", &home_of(&nodes));

        let declaration = Declaration::load("home").unwrap();
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
        let root = machine_dir("pair");
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

        let declaration = Declaration::load("friends").unwrap();
        assert!(!declaration.is_charter(), "no charter is shaped here, so the config section IS the declaration");
        assert_eq!(declaration.relays(), ["sakaki".to_string()]);
        assert_eq!(declaration.status_of("evo"), Some("hold"));
        assert_eq!(declaration.gates()["home"], "sakaki");
        assert_eq!(declaration.address_of("sakaki"), Some("ssh://khoa@192.168.1.202"));
        assert_eq!(declaration.key_of("sakaki"), None, "no paired record, so no identity key comes from here");
    }

    #[test]
    fn a_mesh_declared_nowhere_has_no_declaration() {
        let (_guard, _env) = isolate();
        let root = machine_dir("absent");
        machine(&root, "osaka");
        let refusal = Declaration::load("home").unwrap_err();
        assert_eq!(refusal.reason, NO_DECLARATION, "{refusal}");
    }

    #[test]
    fn status_is_read_by_declared_name_not_by_nickname() {
        let (_guard, _env) = isolate();
        let operator = machine_dir("operator");
        let sakaki = machine_dir("sakaki");
        let yomi = machine_dir("yomi");
        let sakaki_line = line_for(&sakaki, "sakaki");
        let yomi_line = line_for(&yomi, "yomi");
        sign_on(&operator, "home", &body(&[], &[("sakaki", "down")], &[], &[yomi_line, sakaki_line]));

        // A record for the SAME key under a nickname: policy still reads the
        // charter line's name, never the record's (decision D5).
        let key = Declaration::load("home").unwrap().key_of("sakaki").unwrap().to_string();
        node_store::save_nodes(&[record("sakaki-router", &key)]).unwrap();

        let declaration = Declaration::load("home").unwrap();
        assert_eq!(declaration.status_of("sakaki"), Some("down"));
        assert_eq!(declaration.status_of("sakaki-router"), None, "a nickname is not a declared node");
        assert_eq!(declaration.name_of_key(&key), Some("sakaki"), "the charter line's name, never the record's");
    }

    #[test]
    fn a_pair_mesh_resolves_a_key_to_its_paired_record_name() {
        let (_guard, _env) = isolate();
        let root = machine_dir("pair-key");
        machine(&root, "osaka");
        std::fs::write(root.join("config.toml"), "[mesh.friends.nodes]\nevo = \"ssh://evo@192.168.1.40\"\n").unwrap();
        let key = "ab".repeat(32);
        node_store::save_nodes(&[record("evo", &key)]).unwrap();

        let declaration = Declaration::load("friends").unwrap();
        assert_eq!(declaration.key_of("evo"), Some(key.as_str()));
        assert_eq!(declaration.name_of_key(&key), Some("evo"));
        assert_eq!(declaration.name_of_key(&"cd".repeat(32)), None, "a stranger's key resolves to no name");
    }

    #[test]
    fn one_node_with_two_keys_anywhere_is_a_load_error() {
        let (_guard, _env) = isolate();
        let operator = machine_dir("operator");
        let sakaki = machine_dir("sakaki");
        let imposter = machine_dir("imposter");
        let sakaki_line = line_for(&sakaki, "sakaki");
        let two_keys = line_for(&imposter, "sakaki");
        sign_on(&operator, "home", &body(&[], &[], &[], &[sakaki_line]));
        sign_on(&operator, "away", &body(&[], &[], &[], &[two_keys]));

        let refusal = declarations().unwrap_err();
        assert_eq!(refusal.reason, KEY_DIVERGENCE, "{refusal}");
        assert!(refusal.detail.contains("sakaki"), "{refusal}");
    }

    #[test]
    fn one_node_one_key_across_two_meshes_loads() {
        let (_guard, _env) = isolate();
        let operator = machine_dir("operator");
        let sakaki = machine_dir("sakaki");
        let evo = machine_dir("evo");
        let sakaki_line = line_for(&sakaki, "sakaki");
        let evo_line = line_for(&evo, "evo");
        sign_on(&operator, "home", &body(&[], &[], &[], &[sakaki_line.clone()]));
        sign_on(&operator, "away", &body(&[], &[], &[], &[sakaki_line, evo_line]));

        let set = declarations().unwrap();
        assert_eq!(set.len(), 2, "{set:?}");
        let home = set.iter().find(|d| d.mesh() == "home").expect("home loads");
        let away = set.iter().find(|d| d.mesh() == "away").expect("away loads");
        assert_eq!(
            home.key_of("sakaki"),
            away.key_of("sakaki"),
            "the same machine in two meshes is the same key"
        );
    }

    #[test]
    fn a_one_sided_gate_across_two_charters_is_refused() {
        let (_guard, _env) = isolate();
        let operator = machine_dir("operator");
        let sakaki = machine_dir("sakaki");
        let line = line_for(&sakaki, "sakaki");
        sign_on(&operator, "home", &body(&[], &[], &[("away", "sakaki")], &[line.clone()]));
        sign_on(&operator, "away", &body(&[], &[], &[], &[line]));

        let refusal = declarations().unwrap_err();
        assert_eq!(refusal.reason, ONE_SIDED_GATE, "{refusal}");
        assert!(refusal.detail.contains("away"), "{refusal}");
    }

    #[test]
    fn a_symmetric_gate_across_two_charters_loads() {
        let (_guard, _env) = isolate();
        let operator = machine_dir("operator");
        let sakaki = machine_dir("sakaki");
        let evo = machine_dir("evo");
        let line = line_for(&sakaki, "sakaki");
        let evo_line = line_for(&evo, "evo");
        sign_on(&operator, "home", &body(&[], &[], &[("away", "sakaki")], &[line.clone()]));
        sign_on(&operator, "away", &body(&[], &[], &[("home", "sakaki")], &[line, evo_line]));

        let set = declarations().unwrap();
        let away = set.iter().find(|d| d.mesh() == "away").expect("away loads");
        assert_eq!(away.gates()["home"], "sakaki");
        assert!(away.relays().is_empty(), "a mesh that declares no relay has none");
        assert_eq!(away.key_of("evo").unwrap().len(), 64);
    }
}
