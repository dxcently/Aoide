//! The five-edge fixture mesh, raised by any test file that needs it
//! (`mod common;`): a mesh pair to route, deposit and poll over, as close to
//! real boxes as one process can make it.
//!
//! **Two charter meshes, five edges, one shared node.** Mesh `home` is
//! {osaka, sakaki(relay), yomi, chiyo} and mesh `away` is {evo, sakaki};
//! `sakaki` sits in both — the same machine, one identity key — and is the
//! symmetric gate `home.gates.away = "sakaki"` / `away.gates.home = "sakaki"`.
//! `evo` is the DUAL MEMBER that is not a gate: it knows both meshes, so a
//! zone check has a node that must refuse to bridge. The five edges are
//! osaka→sakaki, sakaki→chiyo, osaka→yomi, evo→sakaki and sakaki→evo.
//!
//! **And each box answers on its own hop**, which is what makes those edges
//! routable at all: every box but `chiyo` is declared at `ssh://<name>`, and
//! `chiyo` is the `poll` node the route ends at — no inbound transport, so its
//! letters are held at the relay until it asks for them. A mesh whose nodes all
//! read `poll` (the shape a bare node line has) routes nowhere: a hop cannot
//! hand a letter to a node it cannot dial.
//!
//! **Every box is a REAL isolated root.** `AOIDE_ROOT` is process-global, so
//! the fixture builds each box one at a time (mint its identity, take its node
//! line) and a caller then `enter`s one box to read or dial from it. The
//! operator box ROOTS both meshes and signs both charters; every other box
//! trusts the operator's key from its own `config.toml` and TAKES the signed
//! pair — which is how a charter reaches a machine for real.
//!
//! The two deliberately broken charters live in `tests/fixtures/`, as
//! templates: a node line carries a live key and a live age binding, so it can
//! only be minted at run time.

// A shared helper module is used piecewise: each test binary that includes it
// names a subset of what is here.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use aoide_storage::charter;
use aoide_test_support::{unique_tmp, EnvSaver};

/// The two meshes of the fixture.
pub const HOME: &str = "home";
pub const AWAY: &str = "away";
/// The box that roots and signs both charters.
pub const OPERATOR: &str = "opbox";
/// Every box the fixture raises, in node-name order. `sakaki` is the relay and
/// the gate; `chiyo` is the node a route ends at through `poll`.
pub const BOXES: [&str; 5] = ["osaka", "sakaki", "yomi", "chiyo", "evo"];

/// The broken charters, as templates with `{sakaki}`/`{stranger}` placeholders
/// for the node lines the fixture mints.
pub const ONE_SIDED_GATE: &str = include_str!("../fixtures/one_sided_gate/away.toml");
pub const TWO_KEYS: &str = include_str!("../fixtures/two_keys/away.toml");

/// One raised fixture: a root holding a box per node plus the operator's own,
/// removed when the `Fixture` drops (a failed assertion is not a reason to
/// leave a scratch mesh on disk).
pub struct Fixture {
    pub root: PathBuf,
    /// The operator's own box: it roots both meshes and signs their charters.
    pub operator: PathBuf,
    /// Node name → that box's root.
    pub boxes: BTreeMap<&'static str, PathBuf>,
    /// The operator key each mesh is signed under, as `charter::init` minted
    /// it — what every box's `config.toml` trusts.
    pub operators: BTreeMap<&'static str, String>,
    /// The signed `(source, .sig)` bytes, for a box that TAKES a charter.
    pub signed: BTreeMap<&'static str, (Vec<u8>, Vec<u8>)>,
    /// Every box's node line, as minted on its own machine: `sakaki`'s goes
    /// into both charters, which is the fixture's one-node-one-key case.
    pub lines: BTreeMap<&'static str, String>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Point every root at `box` and name it: the switch each box is read under.
fn enter(box_root: &Path, name: &str) {
    std::env::set_var("AOIDE_ROOT", box_root);
    std::env::set_var("AOIDE_STATE_DIR", box_root);
    std::env::set_var("AOIDE_STAGE_DIR", box_root);
    std::env::set_var("AOIDE_A2A_NODE_NAME", name);
    std::env::remove_var("AOIDE_CONFIG");
}

impl Fixture {
    /// Raise the whole fixture under a fresh root.
    pub fn build(tag: &str) -> Fixture {
        let root = unique_tmp(&format!("mesh-transit-{tag}"));
        let operator_root = root.join(OPERATOR);
        let mut boxes = BTreeMap::new();
        for name in BOXES {
            let dir = root.join(name);
            std::fs::create_dir_all(&dir).unwrap();
            boxes.insert(name, dir);
        }
        std::fs::create_dir_all(&operator_root).unwrap();

        // Every box's node line, minted on its own root — the identity is a
        // host fact, so it is the box that publishes it. Each line carries the
        // hop its box answers on: `chiyo` is the `poll` node, every other box
        // answers on ssh.
        let mut lines = BTreeMap::new();
        for (name, dir) in &boxes {
            enter(dir, name);
            let line = line_at(&charter::node_line().unwrap(), &hop_of(name));
            lines.insert(*name, line);
        }

        // The operator roots both meshes and signs both charters. `sakaki`'s
        // one line goes into both, which is the invariant the pair shares.
        enter(&operator_root, OPERATOR);
        charter::init(HOME).unwrap();
        charter::init(AWAY).unwrap();
        let operators: BTreeMap<&str, String> = [HOME, AWAY]
            .iter()
            .map(|mesh| (*mesh, charter::load_operator_key(mesh).unwrap().info().pubkey_hex))
            .collect();
        std::fs::write(
            charter::source_path(HOME),
            format!(
                "mesh = \"{HOME}\"\nversion = 0\nrelays = [\"sakaki\"]\n\n[nodes]\n{}\n{}\n{}\n{}\n\n\
                 [status]\nyomi = \"down\"\n\n[gates]\n{AWAY} = \"sakaki\"\n",
                lines["osaka"], lines["sakaki"], lines["yomi"], lines["chiyo"],
            ),
        )
        .unwrap();
        std::fs::write(
            charter::source_path(AWAY),
            format!(
                "mesh = \"{AWAY}\"\nversion = 0\nrelays = [\"sakaki\"]\n\n[nodes]\n{}\n{}\n\n\
                 [gates]\n{HOME} = \"sakaki\"\n",
                lines["evo"], lines["sakaki"],
            ),
        )
        .unwrap();
        charter::sign(HOME, None).unwrap();
        charter::sign(AWAY, None).unwrap();

        let signed: BTreeMap<&str, (Vec<u8>, Vec<u8>)> = [HOME, AWAY]
            .iter()
            .map(|mesh| {
                (
                    *mesh,
                    (
                        std::fs::read(charter::source_path(mesh)).unwrap(),
                        std::fs::read(charter::source_sig_path(mesh)).unwrap(),
                    ),
                )
            })
            .collect();

        let fixture = Fixture { root, operator: operator_root, boxes, operators, signed, lines };
        // Every other box trusts the operator key and TAKES both charters —
        // the real path by which a signature reaches a machine.
        for name in BOXES {
            fixture.take_both(name);
        }
        fixture
    }

    /// Point the process at `name`'s box — or at the operator's own, under
    /// [`OPERATOR`].
    pub fn enter(&self, name: &str) {
        enter(self.boxes.get(name).unwrap_or(&self.operator), name);
    }

    /// Write one box's `config.toml` — the operator lines it trusts, and
    /// nothing else (a charter mesh's section may carry no other key).
    fn write_config(&self, name: &str) {
        let config = format!(
            "[pairing]\nhomeMesh = \"{HOME}\"\n\n[mesh.{HOME}]\n{}\n[mesh.{AWAY}]\n{}\n",
            charter::operator_line(&self.operators[HOME]),
            charter::operator_line(&self.operators[AWAY]),
        );
        std::fs::write(self.boxes[name].join("config.toml"), config).unwrap();
    }

    /// `name`'s box takes both signed charters.
    pub fn take_both(&self, name: &str) {
        self.enter(name);
        self.write_config(name);
        for (mesh, (source, sig)) in &self.signed {
            charter::accept(source, sig).unwrap_or_else(|e| panic!("{name} takes `{mesh}`: {e}"));
        }
    }

    /// `name`'s box takes a BROKEN `away` charter — the fixture's two negative
    /// cases, whose template is filled with the real node lines. Signed where
    /// the operator key is (the operator's own box), then handed to `name` the
    /// way any charter is: one box signs, another accepts. The template's
    /// version is one above the good charter's, so the broken one is applied
    /// rather than refused `stale-charter`.
    pub fn take_broken_away(&self, name: &str, template: &str) {
        self.enter(OPERATOR);
        let body = template
            .replace("{sakaki}", &self.lines["sakaki"])
            .replace("{stranger}", &rename(&self.lines["evo"], "evo", "sakaki"));
        std::fs::write(charter::source_path(AWAY), body).unwrap();
        charter::sign(AWAY, None).unwrap();
        let source = std::fs::read(charter::source_path(AWAY)).unwrap();
        let sig = std::fs::read(charter::source_sig_path(AWAY)).unwrap();

        self.enter(name);
        charter::accept(&source, &sig)
            .unwrap_or_else(|e| panic!("{name} takes the broken `{AWAY}`: {e}"));
    }
}

/// Take a minted node line's own name off it, leaving the identity it declares
/// — how the fixture gives one name a second machine's key.
fn rename(line: &str, from: &str, to: &str) -> String {
    let line = line.replacen(&format!("{from} = "), &format!("{to} = "), 1);
    assert!(line.starts_with(&format!("{to} = ")), "renamed line: {line}");
    line
}

/// The hop one box of the fixture answers on: `chiyo` takes no inbound
/// connection at all (`poll` — its letters wait at the relay for its own ask),
/// every other box answers on ssh.
fn hop_of(name: &str) -> String {
    match name {
        "chiyo" => charter::DEFAULT_ADDRESS.to_string(),
        other => format!("ssh://{other}"),
    }
}

/// One node's line at a declared address. `charter::node_line` writes a line
/// with no address at all, which a charter reads as `poll`.
fn line_at(line: &str, address: &str) -> String {
    if address == charter::DEFAULT_ADDRESS {
        return line.to_string();
    }
    let at = line.replacen(" }", &format!(", address = \"{address}\" }}"), 1);
    assert!(at.contains(address), "line at {address}: {at}");
    at
}

/// A fixture that restores the environment when its guard drops, and removes
/// its own root when the [`Fixture`] does.
pub fn fixture(tag: &str) -> (EnvSaver, Fixture) {
    let guard = EnvSaver::capture(&[
        "AOIDE_ROOT",
        "AOIDE_STATE_DIR",
        "AOIDE_STAGE_DIR",
        "AOIDE_A2A_NODE_NAME",
        "AOIDE_CONFIG",
    ]);
    let fixture = Fixture::build(tag);
    (guard, fixture)
}

/// The lock every env-touching test in a binary that raises the fixture holds:
/// the boxes are switched through process-global variables.
pub fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner())
}
