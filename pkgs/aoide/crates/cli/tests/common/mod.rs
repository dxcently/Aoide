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
//! routable at all: every box but `chiyo` is declared at
//! `https://127.0.0.1:<port>/` (its own free loopback port, where a REAL door
//! can be raised), and
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
    /// Node name → the loopback port that box's `https://` address names, where
    /// a REAL door can be raised (`Fixture::doors`). `chiyo` has one too, though
    /// its declared address is `poll` — nothing dials it.
    pub ports: BTreeMap<&'static str, u16>,
}

/// Real doors for some of the fixture's boxes: one `aoide a2a serve` child per
/// box, on that box's own port and root, killed when this drops.
///
/// The children are the fixture's own, spawned and reaped by the test that asked
/// for them (never by a shell, never in the background, and never found again by
/// name): a door-level test needs a door, and `aoide mail serve`'s own round trip
/// (`mail_adapter_round_trip.rs`) is the shape this follows.
pub struct Doors {
    /// Each `Door` kills its own child when it drops, so the vec is the whole
    /// lifetime: no `Drop` of its own to keep in step with the field.
    doors: Vec<Door>,
}

/// A real door for an arbitrary box root: one `aoide a2a serve` child, on
/// `port`, with that root's state — killed and reaped when this drops.
///
/// The child MUST be handed the same state directory the caller uses
/// (`AOIDE_STATE_DIR` = the box root, which is what `enter` sets): a door that
/// resolves its own `state/` looks at a different `nodes.json` than the test
/// wrote, and every signed request then resolves nobody.
pub struct Door {
    child: std::process::Child,
}

impl Drop for Door {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Raise one door on `port` for `root`, and wait until it answers TCP.
pub fn raise_door(root: &std::path::Path, name: &str, port: u16) -> Door {
    let runtime = root.join("runtime");
    std::fs::create_dir_all(&runtime).unwrap();
    let child = std::process::Command::new(aoide_test_support::built_aoide_bin())
        .args(["a2a", "serve", "--bind", "127.0.0.1", "--port", &port.to_string()])
        .env("AOIDE_ROOT", root)
        .env("AOIDE_STATE_DIR", root)
        .env("AOIDE_STAGE_DIR", root)
        .env("AOIDE_AUDIT_LOG", root.join("log"))
        .env("AOIDE_A2A_NODE_NAME", name)
        .env("XDG_RUNTIME_DIR", &runtime)
        .env_remove("AOIDE_SESSION_ID")
        .env_remove("AOIDE_CONFIG")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .expect("the built aoide binary runs");
    let door = Door { child };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return door;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the door for `{name}` never came up on port {port}"
        );
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

/// One JSON-RPC POST to a door, signed the way every node signs a request:
/// the four headers over `wire_auth::canonical_string` (plus `X-Aoide-Mesh` when
/// the request acts in one), over plain HTTP to the loopback port it answers on.
/// The client's own dial cannot reach a `https://` address with no TLS in front
/// of it (`dial_of` knows no `http://`), so a test that deposits AT A NAMED DOOR
/// speaks the wire itself.
pub fn door_post(
    port: u16,
    from: &str,
    mesh: &str,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    use std::io::{Read, Write};
    let body = serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }).to_string();
    let ts = aoide_storage::time::now_iso_utc();
    let nonce = aoide_storage::pairing::random_hex(16);
    let canonical = aoide_storage::wire_auth::canonical_string(
        "POST",
        "/",
        &ts,
        &nonce,
        body.as_bytes(),
        (!mesh.is_empty()).then_some(mesh),
    );
    let signing = aoide_storage::identity::load_or_mint().unwrap().0;
    let sig = aoide_storage::wire_auth::sign_hex(&signing, canonical.as_bytes());
    let request = format!(
        "POST / HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{}: {}\r\n{}: {}\r\n{}: {}\r\n{}: {}\r\n{}Connection: close\r\n\r\n{}",
        body.len(),
        aoide_storage::wire_auth::HEADER_NODE,
        from,
        aoide_storage::wire_auth::HEADER_TIMESTAMP,
        ts,
        aoide_storage::wire_auth::HEADER_NONCE,
        nonce,
        aoide_storage::wire_auth::HEADER_SIGNATURE,
        sig,
        if mesh.is_empty() {
            String::new()
        } else {
            format!("{}: {}\r\n", aoide_storage::wire_auth::HEADER_MESH, mesh)
        },
        body
    );
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let (_, payload) = response.split_once("\r\n\r\n").unwrap_or(("", ""));
    serde_json::from_str(payload).unwrap_or_else(|e| panic!("door answered junk: {e}\n{response}"))
}

impl Fixture {
    /// The URL a box's door answers on, as its own charter line declares it.
    pub fn url_of(&self, name: &str) -> String {
        format!("https://127.0.0.1:{}/", self.ports[name])
    }

    /// Raise a real door for each name, and wait until each answers TCP.
    pub fn doors(&self, names: &[&'static str]) -> Doors {
        let doors: Vec<Door> = names
            .iter()
            .map(|name| raise_door(&self.boxes[*name], name, self.ports[*name]))
            .collect();
        Doors { doors }
    }
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
        // answers on the loopback door its own port names.
        let ports: BTreeMap<&'static str, u16> =
            BOXES.iter().map(|name| (*name, free_port())).collect();
        let mut lines = BTreeMap::new();
        for (name, dir) in &boxes {
            enter(dir, name);
            let line = line_at(&charter::node_line().unwrap(), &hop_of(name, ports[name]));
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

        let fixture = Fixture { root, operator: operator_root, boxes, operators, signed, lines, ports };
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

    /// Re-sign `home` with a new `[status]` block and hand it to `name`: a
    /// declaration change applied the way every charter is — the operator signs,
    /// the box accepts. The fixture's own node lines fill the `[nodes]` block and
    /// the version bumps on its own, so nothing about the mesh moves but the
    /// statuses this call carries.
    pub fn set_home_status(&self, name: &str, status: &[(&str, &str)]) {
        self.enter(OPERATOR);
        // Start from the version IN FORCE and let `sign` bump it: a re-sign is
        // the next version or it is refused `stale-charter`.
        let current = charter::in_force_charter(HOME).map(|c| c.version).unwrap_or(0);
        let mut body = format!(
            "mesh = \"{HOME}\"\nversion = {current}\nrelays = [\"sakaki\"]\n\n[nodes]\n{}\n{}\n{}\n{}\n",
            self.lines["osaka"], self.lines["sakaki"], self.lines["yomi"], self.lines["chiyo"],
        );
        if !status.is_empty() {
            body.push_str("\n[status]\n");
            for (node, s) in status {
                body.push_str(&format!("{node} = \"{s}\"\n"));
            }
        }
        body.push_str(&format!("\n[gates]\n{AWAY} = \"sakaki\"\n"));
        std::fs::write(charter::source_path(HOME), body).unwrap();
        charter::sign(HOME, None).unwrap();
        let source = std::fs::read(charter::source_path(HOME)).unwrap();
        let sig = std::fs::read(charter::source_sig_path(HOME)).unwrap();

        self.enter(name);
        charter::accept(&source, &sig)
            .unwrap_or_else(|e| panic!("{name} takes the re-signed `{HOME}`: {e}"));
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
/// and every other box answers on the loopback door its `https://` address names.
/// A REAL door can be raised on that port (`Fixture::doors`), which is what makes
/// the door-level tests door-level.
fn hop_of(name: &str, port: u16) -> String {
    match name {
        "chiyo" => charter::DEFAULT_ADDRESS.to_string(),
        _ => format!("https://127.0.0.1:{port}/"),
    }
}

/// A free loopback port: bind `:0`, read it back, drop the listener. A tiny
/// re-bind race is acceptable in a test.
pub fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
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
