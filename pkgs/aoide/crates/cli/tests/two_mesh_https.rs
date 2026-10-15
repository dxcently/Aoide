//! The two-mesh mail round trip over real HTTPS, each end box fenced to outbound
//! 443 and nothing else. Linux only, `#[ignore]`d:
//!
//!   cargo test -p aoide-cli --test two_mesh_https -- --ignored --nocapture
//!
//! Topology. Charter meshes `home` and `lab` share one gate, a node named in both
//! charters under one identity key. `homebox` (home) and `labbox` (lab) are `poll`
//! nodes. `farbox` (mesh `far`) has no gate. Every command an end box runs goes
//! through `fixtures/two_mesh/fence.sh`: a fresh user+net namespace whose only
//! route is a bridge on 127.0.0.1:443, carried out over a unix socket to a host
//! bridge. Each box is its own `AOIDE_ROOT` and its own process tree.
//!
//!   homebox ── fence ── 127.0.0.1:443 ─┐ unix sockets   ┌─ TLS front ── aoide mail serve
//!   labbox  ── fence ── 127.0.0.1:443 ─┘ host bridges   ┘  (127.0.0.1, plain HTTP)
//!
//! External gate (the live variant). Set all three of `AOIDE_TWO_MESH_GATE_URL`
//! (`https://<ip-literal>:<port>/`), `AOIDE_TWO_MESH_GATE_CA` (the PEM bundle the
//! boxes trust) and `AOIDE_TWO_MESH_GATE_LINE` (a file holding the gate's node
//! line, named `gate`, without an address). The harness then starts no gate, and
//! steps that read the gate's own state print a skip line. IP literals only: the
//! namespace has no name resolution path to the gate.
//!
//! Bootstrap seed. One step hand-writes what pairing would teach a box: a letter
//! to another mesh is sealed with the destination's age binding, which the origin
//! learns from the destination's charter line. A charter declares only its own
//! mesh's end boxes, so a box cannot read another mesh's binding from a charter it
//! holds. Marked `BOOTSTRAP` where it is written (`seed_cross_mesh`, and the
//! `seed_peer` in `labbox_takes_home`). It stands in for the unbuilt cross-mesh
//! origin gate; delete it once a seal reads that binding from the charter itself.

#![cfg(target_os = "linux")]

mod common;

use aoide_storage::{charter, node_store, seal};
use aoide_test_support::{built_aoide_bin, short_tmp, EnvSaver};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::net::{IpAddr, TcpStream};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const HOME: &str = "home";
const LAB: &str = "lab";
const FAR: &str = "far";
const GATE: &str = "gate";
const HOMEBOX: &str = "homebox";
const LABBOX: &str = "labbox";
const FARBOX: &str = "farbox";
const OPERATOR: &str = "opbox";

const LOCAL_GATE_IP: &str = "127.0.0.1";
const LOCAL_GATE_PORT: u16 = 443;

const ENV_GATE_URL: &str = "AOIDE_TWO_MESH_GATE_URL";
const ENV_GATE_CA: &str = "AOIDE_TWO_MESH_GATE_CA";
const ENV_GATE_LINE: &str = "AOIDE_TWO_MESH_GATE_LINE";

const TIMEOUT: Duration = Duration::from_secs(120);
const TEXT_HOME_TO_LAB: &str = "hello from homebox over https";
const TEXT_LAB_TO_HOME: &str = "reply from labbox over https";

static SEQ: AtomicUsize = AtomicUsize::new(0);

fn label(what: &str) -> String {
    format!("{:03}-{what}", SEQ.fetch_add(1, Ordering::SeqCst))
}

fn pass(id: &str, evidence: impl std::fmt::Display) {
    eprintln!("[{id}] PASS: {evidence}");
}

fn skip(id: &str, why: &str) {
    eprintln!("[{id}] SKIPPED: {why}");
}

fn short(hex: &str) -> String {
    hex.chars().take(12).collect()
}

fn str_list(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

// ── Processes ───────────────────────────────────────────────────────────

fn kill_group(pid: u32) {
    let _ = Command::new("kill")
        .args(["-KILL", "--", &format!("-{pid}")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// The long-lived children: the gate's `mail serve`, its TLS front, the host
/// bridges. Each runs in its own process group, killed as a group on drop.
#[derive(Default)]
struct Reaper {
    children: Vec<Option<Child>>,
}

impl Reaper {
    fn spawn(&mut self, cmd: &mut Command, log: &Path, what: &str) -> usize {
        let file = fs::File::create(log).unwrap();
        let child = cmd
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(file.try_clone().unwrap())
            .stderr(file)
            .spawn()
            .unwrap_or_else(|e| panic!("spawn {what}: {e}"));
        self.children.push(Some(child));
        self.children.len() - 1
    }

    fn stop(&mut self, index: usize) {
        if let Some(mut child) = self.children[index].take() {
            kill_group(child.id());
            let _ = child.wait();
        }
    }

    fn stop_all(&mut self) {
        for index in 0..self.children.len() {
            self.stop(index);
        }
    }
}

impl Drop for Reaper {
    fn drop(&mut self) {
        self.stop_all();
    }
}

/// One command taken to completion.
struct Ran {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Ran {
    fn dump(&self) -> String {
        format!("exit {:?}\n--- stdout\n{}\n--- stderr\n{}", self.code, self.stdout, self.stderr)
    }

    /// The command's JSON envelope. A refusal prints it on stderr, with exit 1.
    fn json(&self, what: &str) -> Value {
        let text = if self.stdout.trim().is_empty() { self.stderr.trim() } else { self.stdout.trim() };
        serde_json::from_str(text).unwrap_or_else(|e| panic!("{what}: not JSON ({e})\n{}", self.dump()))
    }
}

/// Runs `cmd` to completion in its own group, output into files under `runs`. A
/// run past [`TIMEOUT`] is killed and fails the test. The group is taken down
/// afterwards too, so a bridge the fence did not reap cannot outlive the run.
fn run_to_end(cmd: &mut Command, runs: &Path, what: &str) -> Ran {
    let base = runs.join(label(what));
    let out_path = base.with_extension("stdout");
    let err_path = base.with_extension("stderr");
    let mut child = cmd
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(fs::File::create(&out_path).unwrap())
        .stderr(fs::File::create(&err_path).unwrap())
        .spawn()
        .unwrap_or_else(|e| panic!("spawn `{what}`: {e}"));
    let pid = child.id();
    let deadline = Instant::now() + TIMEOUT;
    let code = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status.code();
        }
        if Instant::now() >= deadline {
            kill_group(pid);
            let _ = child.wait();
            panic!("`{what}` ran past {TIMEOUT:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    kill_group(pid);
    Ran {
        code,
        stdout: fs::read_to_string(&out_path).unwrap_or_default(),
        stderr: fs::read_to_string(&err_path).unwrap_or_default(),
    }
}

// ── Boxes ───────────────────────────────────────────────────────────────

/// How an end box's commands leave their namespace: the fence script, the unix
/// socket to the host bridge, and the gate address the bridge serves.
#[derive(Clone)]
struct Fence {
    script: PathBuf,
    sock: PathBuf,
    ip: String,
    port: u16,
}

/// One `AOIDE_ROOT`, and how its commands run.
#[derive(Clone)]
struct Root {
    name: &'static str,
    dir: PathBuf,
    runs: PathBuf,
    bin: PathBuf,
    /// Set for an end box: its commands run inside the fence.
    fence: Option<Fence>,
    /// The CA bundle its https calls trust. `None` leaves system roots only.
    ca: Option<PathBuf>,
}

impl Root {
    fn new(lab: &Path, name: &'static str, bin: &Path) -> Root {
        let dir = lab.join(name);
        fs::create_dir_all(dir.join("run")).unwrap();
        Root {
            name,
            dir,
            runs: lab.join("runs"),
            bin: bin.to_path_buf(),
            fence: None,
            ca: None,
        }
    }

    fn without_ca(&self) -> Root {
        Root { ca: None, ..self.clone() }
    }

    /// `program args` as this box, rebuilt from nothing so no ambient `AOIDE_*`,
    /// `CURL_CA_BUNDLE` or `SSL_CERT_FILE` leaks in, and inside the fence when the
    /// box has one.
    fn command(&self, program: &Path, args: &[&str]) -> Command {
        let mut cmd = match &self.fence {
            None => Command::new(program),
            Some(f) => {
                let mut c = Command::new("sh");
                c.arg(&f.script)
                    .arg(&f.sock)
                    .arg(&f.ip)
                    .arg(f.port.to_string())
                    .arg("--")
                    .arg(program);
                c
            }
        };
        cmd.args(args)
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", &self.dir)
            .env("XDG_RUNTIME_DIR", self.dir.join("run"))
            .env("AOIDE_ROOT", &self.dir)
            .env("AOIDE_A2A_NODE_NAME", self.name)
            .env("AOIDE_AUDIT_LOG", self.dir.join("log"))
            .env("AOIDE_DAEMON_SOCKET", self.dir.join("run/aoided.sock"))
            .env("AOIDE_SECRETS_SOCKET", self.dir.join("run/secrets.sock"));
        if let Some(ca) = &self.ca {
            cmd.env("CURL_CA_BUNDLE", ca).env("SSL_CERT_FILE", ca);
        }
        cmd
    }

    fn aoide_cmd(&self, args: &[&str]) -> Command {
        self.command(&self.bin, args)
    }

    fn run(&self, what: &str, args: &[&str]) -> Ran {
        let mut cmd = self.aoide_cmd(args);
        run_to_end(&mut cmd, &self.runs, &format!("{}-{what}", self.name))
    }

    /// `sh -c script` as this box. The exit code is the script's own.
    fn probe(&self, what: &str, script: &str) -> Ran {
        let mut cmd = self.command(Path::new("sh"), &["-c", script]);
        run_to_end(&mut cmd, &self.runs, &format!("{}-{what}", self.name))
    }
}

// ── Charters and records ────────────────────────────────────────────────

/// The identity a box mints, as `(bare hex key, node line)`.
fn identity_of(root: &Root) -> (String, String) {
    let ran = root.run("identity", &["identity", "--json"]);
    let parsed = ran.json("identity");
    let data = &parsed["data"];
    let line = data["nodeLine"]
        .as_str()
        .unwrap_or_else(|| panic!("no nodeLine: {}", ran.dump()))
        .trim()
        .to_string();
    let key = data["pubkeyHex"].as_str().unwrap().to_string();
    assert_eq!(key, key_of_line(&line), "{}: the node line carries the identity key", root.name);
    (key, line)
}

/// The bare hex identity key a node line carries.
fn key_of_line(line: &str) -> String {
    let marker = "key = \"ed25519:";
    let at = line.find(marker).unwrap_or_else(|| panic!("no identity key on `{line}`")) + marker.len();
    let hex = &line[at..at + 64];
    assert!(hex.chars().all(|c| c.is_ascii_hexdigit()), "identity key is not hex: `{line}`");
    hex.to_ascii_lowercase()
}

/// A node line with its address declared, as a charter carries it.
fn with_address(line: &str, address: &str) -> String {
    let at = line.rfind(" }").unwrap_or_else(|| panic!("no closing brace on `{line}`"));
    format!("{}, address = \"{address}\"{}", &line[..at], &line[at..])
}

/// A charter source at `version` (the one signed last; `sign` writes the next one),
/// with its relays, node lines, and gates into other meshes.
fn charter_source(mesh: &str, version: u64, relays: &[&str], nodes: &[&str], gates: &[(&str, &str)]) -> String {
    let relays = relays.iter().map(|r| format!("\"{r}\"")).collect::<Vec<_>>().join(", ");
    let mut text = format!("mesh    = \"{mesh}\"\nversion = {version}\nrelays  = [{relays}]\n\n[nodes]\n");
    for node in nodes {
        text.push_str(node);
        text.push('\n');
    }
    if !gates.is_empty() {
        text.push_str("\n[gates]\n");
        for (into, gate) in gates {
            text.push_str(&format!("{into} = \"{gate}\"\n"));
        }
    }
    text
}

fn sig_of(source: &Path) -> PathBuf {
    let mut name = source.as_os_str().to_owned();
    name.push(".sig");
    PathBuf::from(name)
}

/// Carries one signed charter into `root`'s inbox, trusts the operator for its
/// mesh, and accepts it. Every step is the box's own `aoide` process.
fn take_charter(root: &Root, operator_key: &str, source: &Path, mesh: &str) {
    let inbox = root.dir.join("inbox");
    fs::create_dir_all(&inbox).unwrap();
    let carried = inbox.join(format!("{mesh}.toml"));
    fs::copy(source, &carried).unwrap();
    fs::copy(sig_of(source), sig_of(&carried)).unwrap();
    let operator = format!("ed25519:{operator_key}");
    let joined = root.run(&format!("join-{mesh}"), &["mesh", "join", mesh, "--operator", &operator, "--json"]);
    assert_eq!(joined.json("join")["status"], "ok", "{} trusts the {mesh} operator: {}", root.name, joined.dump());
    let carried_path = carried.display().to_string();
    let accepted = root.run(&format!("accept-{mesh}"), &["mesh", "charter", "accept", &carried_path, "--json"]);
    assert_eq!(accepted.json("accept")["status"], "ok", "{} takes the {mesh} charter: {}", root.name, accepted.dump());
}

/// A record as `aoide pair` leaves one: verified and pinned to `pubkey`, granted
/// `message` in `grant_in` when given.
fn record(name: &str, url: &str, pubkey: &str, grant_in: Option<&str>) -> node_store::Node {
    node_store::Node {
        name: name.to_string(),
        url: url.to_string(),
        token_file: None,
        bearer_secret: None,
        hub: false,
        pubkey: Some(pubkey.to_string()),
        verified: true,
        grants: grant_in.map_or_else(node_store::Grants::new, |mesh| node_store::grants_in(mesh, &["message"])),
        narrowed: node_store::Grants::new(),
        via: None,
        added_at: "2026-10-12T00:00:00Z".to_string(),
    }
}

/// Runs `f` with `AOIDE_ROOT` pointed at `root`. Only the bootstrap seeds use it:
/// no box runs in this process.
fn in_root<T>(root: &Path, f: impl FnOnce() -> T) -> T {
    let _lock = common::env_lock();
    let _env = EnvSaver::capture(&["AOIDE_ROOT", "AOIDE_STATE_DIR", "AOIDE_STAGE_DIR"]);
    std::env::set_var("AOIDE_ROOT", root);
    std::env::remove_var("AOIDE_STATE_DIR");
    std::env::remove_var("AOIDE_STAGE_DIR");
    f()
}

/// BOOTSTRAP (cross-mesh seal): a node of ANOTHER mesh, pinned by its key, with its
/// age binding learned from its own charter line.
fn seed_peer(root: &Path, dest: &str, dest_key: &str, source: &Path) {
    let peer = record(dest, "poll", dest_key, None);
    in_root(root, || {
        let mut nodes = node_store::load_nodes();
        nodes.retain(|n| n.name != dest);
        nodes.push(peer.clone());
        node_store::save_nodes(&nodes).unwrap();
    });
    let text = fs::read_to_string(source).unwrap();
    let binding = charter::parse(&text).unwrap().nodes[dest].age.clone();
    in_root(root, || seal::learn_binding(dest, &binding).unwrap());
}

// ── Mail readers ────────────────────────────────────────────────────────

fn entries_of(ran: &Ran, what: &str) -> Vec<Value> {
    ran.json(what)["data"]["entries"].as_array().cloned().unwrap_or_default()
}

fn msgids_of(entries: &[Value]) -> Vec<String> {
    entries
        .iter()
        .filter_map(|e| e["msgid"].as_str().map(str::to_string))
        .collect()
}

fn outbox_of(root: &Root, what: &str) -> Vec<String> {
    let ran = root.run(what, &["mail", "outbox", "--json"]);
    msgids_of(&entries_of(&ran, what))
}

fn read_base(root: &Path) -> Vec<Value> {
    fs::read_to_string(root.join("state/mail/base.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

fn mesh_in<'a>(show: &'a Value, mesh: &str) -> &'a Value {
    show["data"]["meshes"]
        .as_array()
        .and_then(|ms| ms.iter().find(|m| m["mesh"] == mesh))
        .unwrap_or_else(|| panic!("no `{mesh}` in `charter show`: {show}"))
}

fn key_in(mesh: &Value, node: &str) -> Option<String> {
    let found = mesh["nodes"].as_array()?.iter().find(|n| n["name"] == node)?;
    found["key"].as_str().map(str::to_string)
}

// ── The lab ─────────────────────────────────────────────────────────────

/// The three external-gate variables, all or none.
fn external_gate_env() -> Option<(String, String, String)> {
    match [ENV_GATE_URL, ENV_GATE_CA, ENV_GATE_LINE].map(std::env::var) {
        [Err(_), Err(_), Err(_)] => None,
        [Ok(url), Ok(ca), Ok(line)] => Some((url, ca, line)),
        _ => panic!("external gate: set all of {ENV_GATE_URL}, {ENV_GATE_CA} and {ENV_GATE_LINE}, or none"),
    }
}

/// `https://<ip-literal>:<port>/` split into its literal and its port.
fn split_gate_url(url: &str) -> (String, u16) {
    let authority = url
        .strip_prefix("https://")
        .unwrap_or_else(|| panic!("{ENV_GATE_URL} must be https://…: `{url}`"))
        .trim_end_matches('/');
    let (host, port) = authority
        .rsplit_once(':')
        .unwrap_or_else(|| panic!("{ENV_GATE_URL} needs a port: `{url}`"));
    let ip: IpAddr = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .parse()
        .unwrap_or_else(|_| panic!("{ENV_GATE_URL} names the gate by IP literal, not `{host}`"));
    (ip.to_string(), port.parse().unwrap_or_else(|_| panic!("bad port in `{url}`")))
}

/// Test CA and a gate certificate with IP SAN 127.0.0.1, written under `pki`, plus
/// the gate's combined certificate and key for socat.
fn make_local_pki(pki: &Path) {
    let openssl = |args: &[&str]| {
        let status = Command::new("openssl")
            .args(args)
            .current_dir(pki)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        assert!(status.map(|s| s.success()).unwrap_or(false), "openssl {args:?} failed");
    };
    openssl(&[
        "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", "ca.key", "-out", "ca.pem", "-days", "2",
        "-subj", "/CN=aoide-two-mesh-test-ca",
        "-addext", "basicConstraints=critical,CA:TRUE",
        "-addext", "keyUsage=critical,keyCertSign,cRLSign",
    ]);
    openssl(&["req", "-newkey", "rsa:2048", "-nodes", "-keyout", "gate.key", "-out", "gate.csr", "-subj", "/CN=127.0.0.1"]);
    fs::write(
        pki.join("ext.cnf"),
        "subjectAltName=IP:127.0.0.1\nbasicConstraints=CA:FALSE\nkeyUsage=digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n",
    )
    .unwrap();
    openssl(&[
        "x509", "-req", "-in", "gate.csr", "-CA", "ca.pem", "-CAkey", "ca.key", "-CAcreateserial",
        "-out", "gate.pem", "-days", "2", "-extfile", "ext.cnf",
    ]);
    let mut bundle = fs::read_to_string(pki.join("gate.pem")).unwrap();
    bundle.push_str(&fs::read_to_string(pki.join("gate.key")).unwrap());
    fs::write(pki.join("gate-bundle.pem"), bundle).unwrap();
}

fn wait_until(what: &str, ready: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !ready() {
        assert!(Instant::now() < deadline, "{what} never came up");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn tcp_up(port: u16) -> impl Fn() -> bool {
    move || TcpStream::connect(("127.0.0.1", port)).is_ok()
}

fn path_up(path: PathBuf) -> impl Fn() -> bool {
    move || path.exists()
}

/// Why this host cannot run the fence, or `None` when it can.
fn tools_missing() -> Option<String> {
    let probes: [(&str, &[&str]); 4] = [
        ("unshare", &["-rn", "true"]),
        ("socat", &["-V"]),
        ("curl", &["--version"]),
        ("ip", &["-V"]),
    ];
    probes.iter().find_map(|(program, args)| {
        let ok = Command::new(program)
            .args(*args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        (!ok).then(|| {
            format!(
                "`{program} {}` does not run here: the fence needs unprivileged user namespaces (`unshare -rn`), and the rest need socat, curl and iproute2 on PATH",
                args.join(" ")
            )
        })
    })
}

fn openssl_missing() -> bool {
    !Command::new("openssl")
        .arg("version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

struct Gate {
    /// The gate's own box, when the harness runs the gate. `None` when external.
    root: Option<Root>,
    /// The address every end box dials, as both charters declare it.
    url: String,
    /// The gate's address inside each end box's namespace.
    fence_ip: String,
    /// The local gate's TLS front and mail adapter, and the front's reaper slot.
    front: Option<usize>,
    front_port: Option<u16>,
    mail_port: Option<u16>,
}

struct Lab {
    dir: PathBuf,
    reaper: Reaper,
    ca: PathBuf,
    operator: Root,
    gate: Gate,
    homebox: Root,
    labbox: Root,
    operator_keys: BTreeMap<&'static str, String>,
    node_keys: BTreeMap<&'static str, String>,
    lines: BTreeMap<&'static str, String>,
    sources: BTreeMap<&'static str, PathBuf>,
    /// Assertions recorded without aborting the run. `finish` fails on any.
    failed: Vec<&'static str>,
}

impl Lab {
    fn build(external: Option<(String, String, String)>) -> Lab {
        let dir = short_tmp("2m");
        let bin = built_aoide_bin();
        fs::create_dir_all(dir.join("runs")).unwrap();
        let mut reaper = Reaper::default();
        let operator = Root::new(&dir, OPERATOR, &bin);
        let mut homebox = Root::new(&dir, HOMEBOX, &bin);
        let mut labbox = Root::new(&dir, LABBOX, &bin);
        let farbox = Root::new(&dir, FARBOX, &bin);
        let pki = dir.join("pki");

        let (ca, gate_root, given_line, gate_url, gate_fence_ip, gate_fence_port, mut upstream) = match external {
            None => {
                fs::create_dir_all(&pki).unwrap();
                make_local_pki(&pki);
                (
                    pki.join("ca.pem"),
                    Some(Root::new(&dir, GATE, &bin)),
                    None,
                    format!("https://{LOCAL_GATE_IP}:{LOCAL_GATE_PORT}/"),
                    LOCAL_GATE_IP.to_string(),
                    LOCAL_GATE_PORT,
                    None,
                )
            }
            Some((url, ca, line_file)) => {
                let (ip, port) = split_gate_url(&url);
                let line = fs::read_to_string(&line_file).unwrap_or_else(|e| panic!("{ENV_GATE_LINE}: {e}"));
                (PathBuf::from(ca), None, Some(line.trim().to_string()), url, ip.clone(), port, Some((ip, port)))
            }
        };

        let (gate_key, gate_raw) = match (&gate_root, given_line) {
            (Some(g), _) => identity_of(g),
            (None, Some(line)) => (key_of_line(&line), line),
            (None, None) => unreachable!("a gate is either local or given"),
        };

        let mut gate_front = None;
        let mut front_port = None;
        let mut mail_port = None;
        if let Some(g) = &gate_root {
            let mport = common::free_port();
            let fport = common::free_port();
            let mut serve = g.aoide_cmd(&["mail", "serve", "--port", &mport.to_string()]);
            reaper.spawn(&mut serve, &dir.join("gate-mail.log"), "gate mail serve");
            wait_until("the gate's mail adapter", tcp_up(mport));
            let mut front = Command::new("socat");
            front
                .arg(format!(
                    "OPENSSL-LISTEN:{fport},bind={LOCAL_GATE_IP},reuseaddr,fork,cert={},verify=0",
                    pki.join("gate-bundle.pem").display()
                ))
                .arg(format!("TCP:127.0.0.1:{mport}"));
            gate_front = Some(reaper.spawn(&mut front, &dir.join("gate-front.log"), "gate TLS front"));
            wait_until("the gate's TLS front", tcp_up(fport));
            front_port = Some(fport);
            mail_port = Some(mport);
            upstream = Some((LOCAL_GATE_IP.to_string(), fport));
        }

        let (up_ip, up_port) = upstream.expect("a gate upstream");
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/two_mesh/fence.sh");
        for (root, slot) in [(&mut homebox, HOMEBOX), (&mut labbox, LABBOX)] {
            let sock = dir.join(format!("{slot}.sock"));
            let mut bridge = Command::new("socat");
            bridge
                .arg(format!("UNIX-LISTEN:{},fork", sock.display()))
                .arg(format!("TCP:{up_ip}:{up_port}"));
            reaper.spawn(&mut bridge, &dir.join(format!("{slot}-bridge.log")), "host bridge");
            wait_until("the host bridge socket", path_up(sock.clone()));
            root.fence = Some(Fence {
                script: script.clone(),
                sock,
                ip: gate_fence_ip.clone(),
                port: gate_fence_port,
            });
            root.ca = Some(ca.clone());
        }

        let mut node_keys: BTreeMap<&'static str, String> = BTreeMap::new();
        let mut lines: BTreeMap<&'static str, String> = BTreeMap::new();
        node_keys.insert(GATE, gate_key);
        lines.insert(GATE, with_address(&gate_raw, &gate_url));
        for (root, name) in [(&homebox, HOMEBOX), (&labbox, LABBOX), (&farbox, FARBOX)] {
            let (key, raw) = identity_of(root);
            node_keys.insert(name, key);
            lines.insert(name, with_address(&raw, "poll"));
        }

        let mut operator_keys: BTreeMap<&'static str, String> = BTreeMap::new();
        for mesh in [HOME, LAB, FAR] {
            let ran = operator.run(&format!("init-{mesh}"), &["mesh", "charter", "init", mesh, "--json"]);
            let parsed = ran.json("charter init");
            operator_keys.insert(mesh, parsed["data"]["operator"].as_str().unwrap().to_string());
        }

        let gate_l = lines[GATE].clone();
        let home_l = lines[HOMEBOX].clone();
        let lab_l = lines[LABBOX].clone();
        let far_l = lines[FARBOX].clone();
        let texts = [
            (HOME, charter_source(HOME, 0, &[GATE], &[gate_l.as_str(), home_l.as_str()], &[(LAB, GATE)])),
            (LAB, charter_source(LAB, 0, &[GATE], &[gate_l.as_str(), lab_l.as_str()], &[(HOME, GATE)])),
            (FAR, charter_source(FAR, 0, &[], &[far_l.as_str()], &[])),
        ];
        let mut sources: BTreeMap<&'static str, PathBuf> = BTreeMap::new();
        for (mesh, text) in texts {
            let path = operator.dir.join("charters").join(format!("{mesh}.toml"));
            fs::write(&path, text).unwrap();
            let signed = operator.run(&format!("sign-{mesh}"), &["mesh", "charter", "sign", mesh, "--json"]);
            assert_eq!(signed.json("charter sign")["status"], "ok", "sign {mesh}: {}", signed.dump());
            sources.insert(mesh, path);
        }

        if let Some(g) = &gate_root {
            for mesh in [HOME, LAB] {
                take_charter(g, &operator_keys[mesh], &sources[mesh], mesh);
            }
        }
        for mesh in [HOME, LAB, FAR] {
            take_charter(&homebox, &operator_keys[mesh], &sources[mesh], mesh);
        }
        take_charter(&labbox, &operator_keys[LAB], &sources[LAB], LAB);

        Lab {
            dir,
            reaper,
            ca,
            operator,
            gate: Gate {
                root: gate_root,
                url: gate_url,
                fence_ip: gate_fence_ip,
                front: gate_front,
                front_port,
                mail_port,
            },
            homebox,
            labbox,
            operator_keys,
            node_keys,
            lines,
            sources,
            failed: Vec::new(),
        }
    }

    fn finish(&self) {
        assert!(
            self.failed.is_empty(),
            "assertions failed: {:?} (roots and logs kept at {})",
            self.failed,
            self.dir.display()
        );
    }

    fn runs(&self) -> PathBuf {
        self.dir.join("runs")
    }

    /// BOOTSTRAP (cross-mesh seal): homebox learns labbox's binding from lab's charter.
    /// Why: home's charter does not declare labbox, and the cross-mesh origin gate is unbuilt.
    fn seed_cross_mesh(&self) {
        seed_peer(&self.homebox.dir, LABBOX, &self.node_keys[LABBOX], &self.sources[LAB]);
    }

    /// A1: each charter verifies where it is held, is at the version signed, and the
    /// gate carries one key in both meshes.
    fn a1(&self) {
        let mut evidence = Vec::new();
        if let Some(gate) = &self.gate.root {
            let show = gate.run("a1-show", &["mesh", "charter", "show", "--json"]).json("A1 gate");
            let mut keys = Vec::new();
            for mesh in [HOME, LAB] {
                let m = mesh_in(&show, mesh);
                assert_eq!(m["honoured"], true, "A1: `{mesh}` is honoured on the gate: {show}");
                assert_eq!(m["highWater"], 1, "A1: `{mesh}` is at version 1 on the gate: {show}");
                assert!(m.to_string().contains(&self.operator_keys[mesh]), "A1: `{mesh}` is signed under its operator key: {m}");
                let key = key_in(m, GATE).unwrap_or_else(|| panic!("A1: the gate is a node of `{mesh}`: {m}"));
                keys.push(key);
            }
            assert_eq!(keys[0], keys[1], "A1: the gate carries one key in home and in lab");
            evidence.push(format!("gate key {} in home and lab", short(&keys[0])));
        } else {
            skip("A1-gate", "the gate is external, so its own box is not here to ask");
        }
        for (root, meshes) in [(&self.homebox, vec![HOME, LAB, FAR]), (&self.labbox, vec![LAB])] {
            let show = root.run("a1-show", &["mesh", "charter", "show", "--json"]).json("A1 box");
            for mesh in meshes {
                let m = mesh_in(&show, mesh);
                assert_eq!(m["honoured"], true, "A1: `{mesh}` is honoured on {}: {show}", root.name);
                assert_eq!(m["highWater"], 1, "A1: `{mesh}` is at version 1 on {}", root.name);
                evidence.push(format!("{}:{mesh} v1", root.name));
            }
        }
        pass("A1", evidence.join(", "));
    }

    /// A2: homebox's route for labbox goes through the gate as home's relay (step 2).
    /// The gate's own route for labbox is the step-3 crossing into lab.
    fn a2(&self) {
        let route = self
            .homebox
            .run("a2-route", &["mail", "route", "labbox/conductor", "--mesh", "home", "--json"])
            .json("A2 homebox route");
        let d = &route["data"];
        assert_eq!(d["next"], GATE, "A2: homebox's next hop is the gate: {route}");
        assert_eq!(d["nextMesh"], HOME, "A2: the letter rides home to that hop: {route}");
        assert_eq!(d["held"], false, "A2: the gate is dialled, not held: {route}");
        assert_eq!(d["dial"], self.gate.url.as_str(), "A2: the hop dials the gate's address: {route}");
        assert!(
            str_list(&d["steps"]).iter().any(|s| s.starts_with("step 2")),
            "A2: the gate is taken as home's relay at step 2: {route}"
        );
        pass("A2a", format!("homebox next=gate at {} (step 2, home's relay)", self.gate.url));

        if let Some(gate) = &self.gate.root {
            let g = gate
                .run("a2-gate", &["mail", "route", "labbox/conductor", "--mesh", "home", "--json"])
                .json("A2 gate route");
            let gd = &g["data"];
            assert_eq!(gd["next"], LABBOX, "A2b: the gate's next hop for labbox is labbox: {g}");
            assert_eq!(gd["nextMesh"], LAB, "A2b: the gate rewrites the letter into mesh lab: {g}");
            assert_eq!(gd["held"], true, "A2b: labbox is a poll node, held at the gate: {g}");
            assert!(
                str_list(&gd["steps"]).iter().any(|s| s.starts_with("step 3")),
                "A2b: the step-3 crossing is in the gate's trail: {g}"
            );
            pass("A2b", "gate: step-3 crossing home->lab, next labbox held in mesh lab");
        } else {
            skip("A2b", "the gate is external, so its own route is not here to ask");
        }
    }

    /// A3 and A4: homebox sends. The gate holds the letter for labbox in mesh lab.
    fn a3_a4(&self) -> String {
        let sent = self.homebox.run(
            "a3-send",
            &["mail", "send", "--to", "labbox/conductor", "--mesh", "home", "--json", "--", TEXT_HOME_TO_LAB],
        );
        let send = sent.json("A3 send");
        assert_eq!(send["status"], "ok", "A3: the send is ok: {}", sent.dump());
        let d = &send["data"];
        assert_eq!(d["next"], GATE, "A3: the next hop is the gate: {send}");
        assert_eq!(d["header"]["to"]["node"], LABBOX, "A3: the letter is for labbox: {send}");
        assert_ne!(d["delivery"]["status"], "refused", "A3: the letter is not parked at homebox: {send}");
        let msgid = d["msgid"].as_str().expect("A3: a msgid").to_string();
        pass("A3", format!("ok, next=gate, msgid {}, delivery {}", short(&msgid), d["delivery"]["status"]));

        if let Some(gate) = &self.gate.root {
            let ran = gate.run("a4-outbox", &["mail", "outbox", LABBOX, "--json"]);
            let held = msgids_of(&entries_of(&ran, "A4 outbox"));
            assert!(held.contains(&msgid), "A4: the gate holds the letter for labbox: {held:?}");
            let transit = read_base(&gate.dir)
                .into_iter()
                .find(|e| e["type"] == "transit" && e["next"] == LABBOX)
                .unwrap_or_else(|| panic!("A4: the gate filed a transit line toward labbox"));
            assert_eq!(transit["mesh"], LAB, "A4: the transit line is in mesh lab: {transit}");
            assert_eq!(transit["held"], true, "A4: labbox is held at the gate, not dialled: {transit}");
            pass("A4", format!("gate holds {} for labbox; transit mesh=lab held=true", short(&msgid)));
        } else {
            skip("A4", "the gate is external, so its transit record is not here to read");
        }
        msgid
    }

    /// A5n: labbox does not yet hold home's charter. Its poll files nothing and the
    /// gate keeps the letter.
    fn a5n(&self, msgid: &str) {
        let poll = self.labbox.run("a5n-poll", &["mail", "poll", GATE, "--mesh", "lab", "--json"]);
        let p = poll.json("A5n poll");
        assert_eq!(p["data"]["filed"], 0, "A5n: nothing filed without home's charter: {p}");
        assert!(
            p["data"]["nodes"][0].to_string().contains("unverified-origin"),
            "A5n: refused as unverified-origin: {p}"
        );
        if let Some(gate) = &self.gate.root {
            let ran = gate.run("a5n-gate-outbox", &["mail", "outbox", LABBOX, "--json"]);
            let held = msgids_of(&entries_of(&ran, "A5n gate outbox"));
            assert!(held.iter().any(|m| m == msgid), "A5n: the gate still holds the letter after the refusal: {held:?}");
        }
        pass("A5n", format!("labbox refuses {} as unverified-origin without home's charter", short(msgid)));
    }

    /// BOOTSTRAP (cross-mesh seal): labbox takes home's charter by hand, then learns
    /// homebox's binding for its reply. Done here so A5n sees the missing charter.
    /// Why: lab's charter does not declare homebox, and the cross-mesh origin gate is unbuilt.
    fn labbox_takes_home(&self) {
        take_charter(&self.labbox, &self.operator_keys[HOME], &self.sources[HOME], HOME);
        seed_peer(&self.labbox.dir, HOMEBOX, &self.node_keys[HOMEBOX], &self.sources[HOME]);
    }

    /// A5: labbox polls again, files exactly the letter A3 sent, and verifies its origin
    /// against home's charter.
    fn a5(&self, msgid: &str) {
        self.labbox_takes_home();
        let poll = self.labbox.run("a5-poll", &["mail", "poll", GATE, "--mesh", "lab", "--json"]);
        let p = poll.json("A5 poll");
        assert_eq!(p["data"]["filed"], 1, "A5: exactly one letter is filed: {p}");
        let read = self.labbox.run("a5-read", &["mail", "read", "--for", "conductor", "--json"]);
        let filed = entries_of(&read, "A5 read")
            .into_iter()
            .find(|e| e["envelope"]["msgid"] == msgid)
            .unwrap_or_else(|| panic!("A5: the filed letter is the one A3 sent: {}", read.dump()));
        assert_eq!(filed["envelope"]["text"], TEXT_HOME_TO_LAB, "A5: the text is the letter's: {filed}");
        assert_eq!(filed["envelope"]["header"]["from"]["node"], HOMEBOX, "A5: the origin is homebox: {filed}");
        assert_eq!(filed["envelope"]["header"]["originMesh"], HOME, "A5: the origin mesh is home: {filed}");
        pass("A5", format!("labbox files {} from homebox; origin verified against home's charter", short(msgid)));
    }

    /// A6: labbox replies over the gate, and homebox files the reply. A6b, the receipt
    /// for the A3 letter retiring from homebox's outbox, is recorded rather than
    /// asserted, so A7 through A9a still run and the failure is reported at the end.
    fn a6(&mut self, a3_msgid: &str) {
        let sent = self.labbox.run(
            "a6-send",
            &["mail", "send", "--to", "homebox/conductor", "--mesh", "lab", "--json", "--", TEXT_LAB_TO_HOME],
        );
        let send = sent.json("A6 send");
        assert_eq!(send["status"], "ok", "A6: labbox's reply is ok: {}", sent.dump());
        assert_eq!(send["data"]["next"], GATE, "A6: the reply's next hop is the gate: {send}");
        let reply = send["data"]["msgid"].as_str().unwrap().to_string();

        let poll = self.homebox.run("a6-poll", &["mail", "poll", GATE, "--mesh", "home", "--json"]);
        let p = poll.json("A6 poll");
        assert!(p["data"]["filed"].as_u64().unwrap_or(0) >= 1, "A6: homebox files the reply: {p}");
        let read = self.homebox.run("a6-read", &["mail", "read", "--for", "conductor", "--json"]);
        let filed = entries_of(&read, "A6 read")
            .into_iter()
            .find(|e| e["envelope"]["msgid"] == reply.as_str())
            .unwrap_or_else(|| panic!("A6: the reply is filed at homebox: {}", read.dump()));
        assert_eq!(filed["envelope"]["text"], TEXT_LAB_TO_HOME, "A6: the reply's text: {filed}");

        pass("A6", format!("reply {} filed at homebox", short(&reply)));
        let home_out = outbox_of(&self.homebox, "a6-homebox-outbox");
        if home_out.iter().any(|m| m == a3_msgid) {
            eprintln!(
                "[A6b] FAIL: the receipt for the A3 letter did not return, so {} stays in homebox's outbox; the ack is parked at labbox as a hold entry (labbox is not a member of mesh home, so its route for the ack is refused not-a-member)",
                short(a3_msgid)
            );
            self.failed.push("A6b");
        } else {
            pass("A6b", format!("labbox's receipt retired A3 letter {} from homebox's outbox", short(a3_msgid)));
        }
    }

    /// A7: from inside each fence the bridge answers and nothing else. Each probe
    /// runs with the bridge up, so a refusal is the fence's doing. The host-side
    /// controls show the same targets are live from outside the fence.
    fn a7(&self) {
        for r in [&self.homebox, &self.labbox] {
            let bridge = r.probe(
                "a7-bridge",
                &format!("curl -sS -o /dev/null --max-time 10 -X POST -d '{{}}' {}", self.gate.url),
            );
            assert_eq!(bridge.code, Some(0), "A7 {}: the bridge reaches the gate: {}", r.name, bridge.dump());

            let public = r.probe("a7-public", "curl -sS -o /dev/null --max-time 5 https://1.1.1.1:443/");
            assert_eq!(public.code, Some(7), "A7 {}: TCP to 1.1.1.1:443 cannot connect: {}", r.name, public.dump());

            let udp = r.probe("a7-udp", "printf x | socat -T3 - UDP:8.8.8.8:53");
            assert!(udp.stdout.is_empty(), "A7 {}: UDP to 8.8.8.8:53 gets no answer: {}", r.name, udp.dump());

            // A plain TCP connect to port 22 (the ssh port): socat exits 0 only when
            // the connection is made, so any nonzero exit is a refusal or a timeout.
            let public_22 = r.probe("a7-public-22", "socat -T3 /dev/null TCP:1.1.1.1:22,connect-timeout=3");
            assert_ne!(public_22.code, Some(0), "A7 {}: TCP to 1.1.1.1:22 cannot connect: {}", r.name, public_22.dump());

            let gate_22 = r.probe(
                "a7-gate-22",
                &format!("socat -T3 /dev/null TCP:{}:22,connect-timeout=3", self.gate.fence_ip),
            );
            assert_ne!(gate_22.code, Some(0), "A7 {}: TCP to the gate's port 22 cannot connect: {}", r.name, gate_22.dump());

            if let (Some(front), Some(plain)) = (self.gate.front_port, self.gate.mail_port) {
                let via_front = r.probe("a7-front", &format!("curl -sS -o /dev/null --max-time 5 https://127.0.0.1:{front}/"));
                assert_eq!(via_front.code, Some(7), "A7 {}: the front is not reachable from the fence: {}", r.name, via_front.dump());
                let via_plain = r.probe("a7-plain", &format!("curl -sS -o /dev/null --max-time 5 http://127.0.0.1:{plain}/"));
                assert_eq!(via_plain.code, Some(7), "A7 {}: the adapter is not reachable from the fence: {}", r.name, via_plain.dump());
            }
            pass("A7", format!("{}: bridge answers; 1.1.1.1:443, UDP 8.8.8.8:53, TCP 22, and the gate's front and adapter fail", r.name));
        }

        if let (Some(front), Some(plain)) = (self.gate.front_port, self.gate.mail_port) {
            let mut c = Command::new("curl");
            c.args(["-sS", "-o", "/dev/null", "-w", "%{http_code}", "--max-time", "5", "-X", "POST", "-d", "{}"])
                .arg(format!("https://127.0.0.1:{front}/"))
                .env("CURL_CA_BUNDLE", &self.ca);
            let control = run_to_end(&mut c, &self.runs(), "a7-control-front");
            assert_eq!(control.code, Some(0), "A7 control: the host reaches the front: {}", control.dump());
            let mut c = Command::new("curl");
            c.args(["-sS", "-o", "/dev/null", "-w", "%{http_code}", "--max-time", "5", "-X", "POST", "-d", "{}"])
                .arg(format!("http://127.0.0.1:{plain}/"));
            let plain_control = run_to_end(&mut c, &self.runs(), "a7-control-plain");
            assert_eq!(plain_control.code, Some(0), "A7 control: the host reaches the adapter: {}", plain_control.dump());
            pass(
                "A7-control",
                format!("from the host: front answers {}, adapter answers {}", control.stdout.trim(), plain_control.stdout.trim()),
            );
        }
    }

    /// A8: with no CA bundle, every https attempt fails and nothing is filed. The
    /// bridge is up, so the failure is trust, not reachability.
    fn a8_no_ca(&self) {
        let bare = self.labbox.without_ca();
        let probe = bare.probe("a8-curl", &format!("curl -sS -o /dev/null --max-time 5 -X POST -d '{{}}' {}", self.gate.url));
        assert_eq!(probe.code, Some(60), "A8: an https attempt without a CA fails at verification: {}", probe.dump());
        assert!(probe.stdout.is_empty(), "A8: no body comes back without trust: {}", probe.dump());

        let poll = bare.run("a8-poll", &["mail", "poll", GATE, "--mesh", "lab", "--json"]);
        let p = poll.json("A8 poll");
        assert_eq!(p["data"]["nodes"][0]["status"], "unreachable", "A8: labbox's poll cannot reach the gate without trust: {p}");
        assert_eq!(p["data"]["filed"], 0, "A8: nothing is filed without trust: {p}");
        pass("A8", "no CA: curl exit 60 with no body; labbox poll unreachable, filed 0");
    }

    /// A8 (send): homebox's send without a CA stays spooled and undelivered. The test
    /// then retires that letter by name, so later steps see a clean outbox.
    fn a8_send_no_ca(&self) {
        let bare = self.homebox.without_ca();
        let sent = bare.run(
            "a8-send",
            &["mail", "send", "--to", "labbox/conductor", "--mesh", "home", "--json", "--", "no CA here"],
        );
        let msgid = sent.json("A8 send")["data"]["msgid"].as_str().unwrap_or_default().to_string();
        assert!(!msgid.is_empty(), "A8: the send wrote a letter: {}", sent.dump());
        let after = self.homebox.run("a8-after", &["mail", "outbox", "--json"]);
        let spooled: Vec<Value> = entries_of(&after, "A8 outbox")
            .into_iter()
            .filter(|e| e["msgid"] == msgid.as_str())
            .collect();
        assert_eq!(spooled.len(), 1, "A8: the letter stays spooled in homebox's outbox: {}", after.dump());
        assert_ne!(spooled[0]["delivery"]["status"], "delivered", "A8: nothing is delivered without trust: {}", spooled[0]);
        let removed = self.homebox.run("a8-rm", &["mail", "outbox", "rm", &msgid, "--json"]);
        assert_eq!(removed.json("A8 rm")["status"], "ok", "A8: the test retires its own letter: {}", removed.dump());
        pass("A8-send", format!("no-CA send stays spooled (delivery {}), then retired by name", spooled[0]["delivery"]["status"]));
    }

    /// A9b: a third mesh with no gate. homebox's own route hands a far-bound letter to
    /// home's relay (step 2), so the no-route refusal is the gate's, not the sender's.
    fn a9b_no_gate(&self) {
        let Some(gate) = &self.gate.root else {
            skip("A9b", "the gate is external, so its route is not here to ask");
            return;
        };
        let before = outbox_of(&self.homebox, "a9b-before");
        let hop = self
            .homebox
            .run("a9b-home-route", &["mail", "route", "farbox/conductor", "--mesh", "home", "--json"])
            .json("A9b homebox route");
        assert_eq!(hop["data"]["next"], GATE, "A9b: homebox hands the far-bound letter to home's relay: {hop}");
        let route = gate
            .run("a9b-gate-route", &["mail", "route", "farbox/conductor", "--mesh", "home", "--json"])
            .json("A9b gate route");
        assert_eq!(route["data"]["reason"], "no-route", "A9b: the gate refuses the letter no-route: {route}");
        let after = outbox_of(&self.homebox, "a9b-after");
        assert_eq!(before, after, "A9b: a dry route spools nothing");
        pass("A9b", "homebox hands far-bound mail to the gate (step 2); the gate refuses it no-route");
    }

    /// A9c: labbox holds no record of the gate, so the charter alone names its relay,
    /// and the relay is down. The poll reports unreachable, never polled, and files
    /// nothing.
    fn a9c_no_relay(&mut self) {
        let Some(front) = self.gate.front else {
            skip("A9c", "the gate is external: the harness cannot take its front down");
            return;
        };
        assert!(
            !in_root(&self.labbox.dir, node_store::load_nodes).iter().any(|n| n.name == GATE),
            "A9c: labbox holds no record of the gate, so the charter alone names its relay"
        );
        self.reaper.stop(front);
        let poll = self.labbox.run("a9c-poll", &["mail", "poll", GATE, "--mesh", "lab", "--json"]);
        let p = poll.json("A9c poll");
        let row = &p["data"]["nodes"][0];
        assert_ne!(row["status"], "polled", "A9c: a poll with no reachable relay does not report polled: {p}");
        assert_eq!(p["data"]["filed"], 0, "A9c: nothing is filed: {p}");
        pass("A9c", format!("no record, charter relay down: poll reports {}, filed 0", row["status"]));
    }

    /// A9a: home's charter without `[gates]`, so lab's gate into home is one-sided.
    /// The gate's lab route to labbox is the control: next=labbox before, refused
    /// after. The refusal reads `not-a-member`, not `one-sided-gate`, so the
    /// label is recorded as a finding, not asserted. Runs last: it re-signs home.
    fn a9a_one_sided_gate(&self) {
        let Some(gate) = &self.gate.root else {
            skip("A9a", "the gate is external, so its routing is not here to ask");
            return;
        };
        let route = |what: &str| {
            gate.run(what, &["mail", "route", "labbox/conductor", "--mesh", "lab", "--json"])
                .json("A9a route")
        };
        let before = route("a9a-before");
        assert_eq!(before["data"]["next"], LABBOX, "A9a: the gate's lab route reaches labbox before the drop: {before}");

        let gate_l = self.lines[GATE].as_str();
        let home_l = self.lines[HOMEBOX].as_str();
        fs::write(&self.sources[HOME], charter_source(HOME, 1, &[GATE], &[gate_l, home_l], &[])).unwrap();
        let signed = self.operator.run("a9a-sign", &["mesh", "charter", "sign", HOME, "--json"]);
        assert_eq!(signed.json("A9a sign")["status"], "ok", "A9a: the one-sided home signs: {}", signed.dump());
        for root in [gate, &self.homebox, &self.labbox] {
            take_charter(root, &self.operator_keys[HOME], &self.sources[HOME], HOME);
        }

        let after = route("a9a-after");
        assert_eq!(after["data"]["reason"], "not-a-member", "A9a: the gate refuses lab's crossing once home drops its gate: {after}");
        assert!(after["data"]["next"].is_null(), "A9a: no hop is picked for lab: {after}");
        let show = gate.run("a9a-show", &["mesh", "charter", "show", "--json"]).json("A9a gate show");
        let honoured = mesh_in(&show, LAB)["honoured"].clone();
        eprintln!(
            "[A9a-finding] lab is honoured {honoured} in `charter show` on the gate, yet its route is refused not-a-member; one-sided-gate is not named in any output"
        );
        pass("A9a", "home without [gates]: the gate's lab route flips from next=labbox to refused (not-a-member)");
    }
}

impl Drop for Lab {
    fn drop(&mut self) {
        self.reaper.stop_all();
        if std::thread::panicking() {
            eprintln!("two_mesh_https: the run failed; its roots and logs are kept at {}", self.dir.display());
        } else {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
}

#[test]
#[ignore = "two charter meshes, a gate and two fenced boxes over real HTTPS: run with --ignored (cargo test -p aoide-cli --test two_mesh_https -- --ignored --nocapture)"]
fn two_mesh_mail_round_trips_over_https_with_443_only_boxes() {
    let external = external_gate_env();
    // A missing tool fails the run with its reason. The test is ignored, so it runs
    // only when asked for: a skip here would read as a pass that checked nothing.
    if let Some(missing) = tools_missing() {
        panic!("two_mesh_https: cannot run: {missing}");
    }
    if external.is_none() && openssl_missing() {
        panic!(
            "two_mesh_https: cannot run: `openssl` does not run here, so the test CA cannot be made. Put openssl on PATH (the dev shell carries it), or set {ENV_GATE_URL}, {ENV_GATE_CA} and {ENV_GATE_LINE} to run against an external gate."
        );
    }
    let mut lab = Lab::build(external);
    lab.a1();
    lab.a2();
    lab.seed_cross_mesh();
    let msgid = lab.a3_a4();
    lab.a5n(&msgid);
    lab.a5(&msgid);
    lab.a6(&msgid);
    lab.a7();
    lab.a8_no_ca();
    lab.a8_send_no_ca();
    lab.a9b_no_gate();
    lab.a9c_no_relay();
    lab.a9a_one_sided_gate();
    lab.finish();
}
