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

use std::collections::{BTreeMap, BTreeSet};

use crate::charter::{self, Charter, Refusal};
use crate::{config, node_store};

/// One node name carrying two different identity keys — "one node, one
/// identity key, in every mesh" (MAIL.md §Transit).
pub const KEY_DIVERGENCE: &str = "key-divergence";
/// A gate the mesh on the other side does not answer back, or one a mesh
/// points at itself — a gate is symmetric or it is nothing (MAIL.md §Transit).
pub const ONE_SIDED_GATE: &str = "one-sided-gate";
/// No mesh of this name is declared here — neither a charter in force nor a
/// `[mesh.<name>]` section.
pub const NO_DECLARATION: &str = "no-declaration";

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
/// A violation is refused against the mesh whose own declaration is the
/// inconsistent one — the pair mesh whose record disagrees with a charter, the
/// mesh that declares an un-answered gate — so a disagreement between two
/// meshes fails closed for the mesh that is wrong and leaves the rest routing.
pub fn declarations() -> Vec<Loaded> {
    let mut names: BTreeSet<String> = match config::load() {
        Ok(loaded) => loaded.config.mesh.keys().cloned().collect(),
        // A config that will not load is not a name source; every mesh whose
        // state answers for itself still loads below, and a mesh that needed
        // the section is refused `no-declaration` rather than silently gone.
        Err(_) => BTreeSet::new(),
    };
    names.extend(charter::meshes_with_state());

    let mut out: Vec<Loaded> = names
        .into_iter()
        .map(|mesh| Loaded { declaration: Declaration::load(&mesh), mesh })
        .collect();
    for (mesh, refusal) in set_verdicts(&out) {
        if let Some(loaded) = out.iter_mut().find(|l| l.mesh == mesh) {
            loaded.declaration = Err(refusal);
        }
    }
    out
}

/// The two invariants only a SET of declarations can see, each refused against
/// the mesh that is inconsistent, as `mesh name → refusal`.
///
/// **One node, one identity key, in every mesh.** A node may sit in several
/// meshes with a different grant in each; identity is not per mesh, so a name
/// carrying two keys is a load error. Every declaration's keys count — a pair
/// mesh's too, which is how a paired record left over from before a charter
/// re-keyed a name is caught — and the CHARTERS are read first: a charter is
/// the authority for a name it lists, so a record that disagrees is the copy
/// that yields, and the pair mesh is the mesh refused.
///
/// **A gate is symmetric or it is nothing.** Each gate a mesh declares must be
/// answered, by the mesh it names, with the same node; a mesh cannot be its own
/// answer (`gate_verdict`). A gate into a mesh this host does not hold cannot
/// be judged here and is left to the two declarations that do.
fn set_verdicts(loaded: &[Loaded]) -> BTreeMap<String, Refusal> {
    let mut verdicts: BTreeMap<String, Refusal> = BTreeMap::new();
    let mut keyed: BTreeMap<&str, (&str, &str)> = BTreeMap::new();
    let mut ordered: Vec<&Declaration> = loaded.iter().filter_map(|l| l.declaration.as_ref().ok()).collect();
    ordered.sort_by_key(|d| !d.is_charter());
    for declaration in ordered {
        for (node, key) in declaration.declared_keys() {
            match keyed.get(node) {
                Some((first_key, first_mesh)) if !first_key.eq_ignore_ascii_case(key) => {
                    verdicts.entry(declaration.mesh().to_string()).or_insert_with(|| {
                        Refusal::new(
                            KEY_DIVERGENCE,
                            format!(
                                "`{node}` carries two identity keys — `{first_key}` in mesh `{first_mesh}` \
                                 and `{key}` in mesh `{}`. One node, one identity key, in every mesh: a \
                                 name that stands for two machines is a name no policy lookup can trust",
                                declaration.mesh()
                            ),
                        )
                    });
                }
                Some(_) => {}
                None => {
                    keyed.insert(node, (key, declaration.mesh()));
                }
            }
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

/// One mesh's routing declaration. `load` is the only constructor: it decides
/// the kind, reads that kind's single source, and answers every accessor below
/// off what it read.
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
    /// Read `mesh`'s declaration: the charter in force where the mesh is
    /// charter-shaped, its `[mesh.<name>]` section otherwise. Fails closed —
    /// a charter-shaped mesh with no readable charter is refused with that
    /// charter's own word, never answered from the paired records.
    pub fn load(mesh: &str) -> Result<Declaration, Refusal> {
        if charter::charter_shaped(mesh) {
            let charter = charter::governing_refusal(mesh)?;
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

    /// The address `node` is declared at — `ssh://…`, `https://…` or `poll` —
    /// or `None` for a node this declaration does not name. An address is a
    /// declaration, not a dial target; turning one into the other is the
    /// caller's (`docs/architecture/HTTPS-MESH-API.md` "Transports and relays").
    pub fn address_of(&self, node: &str) -> Option<&str> {
        match &self.kind {
            Kind::Charter(c) => c.nodes.get(node).map(|line| line.address.as_str()),
            Kind::Pair(p) => p.addresses.get(node).map(String::as_str),
        }
    }

    /// The identity key (bare lowercase hex) this declaration gives `node`, or
    /// `None`. For a charter mesh that is the charter's line; for a pair mesh
    /// the node's VERIFIED paired record — nothing else, because a relay never
    /// supplies a key (MAIL.md §Transit, "Keys come from trust").
    pub fn key_of(&self, node: &str) -> Option<&str> {
        match &self.kind {
            Kind::Charter(c) => c.nodes.get(node).map(|line| line.key.as_str()),
            Kind::Pair(p) => p.keys.get(node).map(String::as_str),
        }
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
/// it names read once.
fn pair_from(section: &config::Mesh) -> Pair {
    let records = node_store::load_nodes();
    let keys = section
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
    Pair {
        relays: section.relays.clone(),
        status: section.status.clone(),
        gates: section.gates.clone(),
        addresses: section.nodes.clone(),
        keys,
    }
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

    /// The five-edge fixture's home mesh: osaka, the relay sakaki, yomi and the
    /// `poll` node chiyo — gated into `away` through sakaki, with yomi declared
    /// `down`.
    fn home_of(nodes: &[(PathBuf, &str)]) -> String {
        body(
            &["sakaki"],
            &[("yomi", "down")],
            &[("away", "sakaki")],
            &nodes.iter().map(|(dir, name)| line_for(dir, name)).collect::<Vec<_>>(),
        )
    }

    /// The one declaration in the set for `mesh`, which must have loaded.
    fn ok(mesh: &str) -> Declaration {
        let set = declarations();
        set.iter()
            .find(|l| l.mesh == mesh)
            .unwrap_or_else(|| panic!("`{mesh}` is in the set: {set:?}"))
            .declaration
            .clone()
            .unwrap_or_else(|e| panic!("`{mesh}` loads: {e}"))
    }

    /// The refusal on `mesh`'s own entry.
    fn refused(mesh: &str) -> Refusal {
        let set = declarations();
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
        assert!(declarations().is_empty(), "nothing is declared here");
        let refusal = Declaration::load("home").unwrap_err();
        assert_eq!(refusal.reason, NO_DECLARATION, "{refusal}");
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

        // Two charters, one name, two keys: the second declaration to give the
        // name a key is the one refused, and the first keeps routing. Neither is
        // an authority over the other, so the order is the mesh name's.
        let refusal = refused("home");
        assert_eq!(refusal.reason, KEY_DIVERGENCE, "{refusal}");
        assert!(refusal.detail.contains("sakaki"), "{refusal}");
        assert_eq!(ok("away").key_of("sakaki").unwrap().len(), 64);
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
}
