use std::fs;
use std::path::{Path, PathBuf};

const EXEC: &str = "crates/exec/src/lib.rs";

/// Words no source in the desktop workspace may hold. A tripwire for a slip: it
/// reads the raw text, so a split spelling passes it. The lints are the guard.
const EVERYWHERE: [&str; 6] = [
    concat!("aoided", ".sock"),
    concat!("\"dis", "patch\""),
    concat!("\"sub", "scribe\""),
    concat!("/api/", "launch"),
    concat!("sh", " -c"),
    concat!("SIG", "KILL"),
];

/// Each manifest's golden text. The test compares its lines less the blank and
/// comment lines, so a change to a manifest moves its golden in the same commit.
const ROOT_MANIFEST: &str = r##"[workspace]
resolver = "2"
members = ["crates/door", "crates/exec", "crates/shell"]
[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
libc = "0.2"
sha2 = "0.10"
[workspace.lints.rust]
unsafe_code = "forbid"
[workspace.lints.clippy]
disallowed_methods = "forbid"
disallowed_types = "forbid"
"##;
const DOOR_MANIFEST: &str = r##"[package]
name = "aoide-desktop-door"
version = "0.1.0"
edition = "2021"
publish = false
[features]
fake = []
[dependencies]
libc.workspace = true
serde.workspace = true
serde_json.workspace = true
[lints]
workspace = true
"##;
const EXEC_MANIFEST: &str = r##"[package]
name = "aoide-desktop-exec"
version = "0.1.0"
edition = "2021"
publish = false
[features]
fakes = []
[dependencies]
aoide-protocol = { path = "../../../aoide/crates/protocol" }
libc.workspace = true
serde.workspace = true
serde_json.workspace = true
[lints.rust]
unsafe_code = "allow"
[lints.clippy]
disallowed_methods = "allow"
disallowed_types = "allow"
"##;
const SHELL_MANIFEST: &str = r##"[package]
name = "aoide-desktop-shell"
version = "0.1.0"
edition = "2021"
publish = false
[features]
fakes = ["aoide-desktop-door/fake", "aoide-desktop-exec/fakes"]
[[bin]]
name = "fake-aoide"
path = "src/bin/fake-aoide.rs"
required-features = ["fakes"]
[[bin]]
name = "fake-eidolon"
path = "src/bin/fake-eidolon.rs"
required-features = ["fakes"]
[dependencies]
aoide-desktop-door = { path = "../door" }
aoide-desktop-exec = { path = "../exec" }
libc.workspace = true
serde.workspace = true
serde_json.workspace = true
sha2.workspace = true
[lints]
workspace = true
"##;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every file under `dir`, but not under the root's own `target` directory.
fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let target = root().join("target");
    for entry in fs::read_dir(dir).expect("the directory is read") {
        let path = entry.expect("a directory entry").path();
        if !path.is_dir() {
            out.push(path);
        } else if path != target {
            walk(&path, out);
        }
    }
}

/// Each `.rs` under the desktop workspace root, and each file under `ui/` once
/// that exists, as its path relative to the root and its text.
fn sources() -> Vec<(String, String)> {
    let root = root();
    let ui = root.join("ui");
    let mut paths = Vec::new();
    walk(&root, &mut paths);
    paths.retain(|p| p.extension().is_some_and(|e| e == "rs") || p.starts_with(&ui));
    paths
        .into_iter()
        .map(|p| {
            let rel = p.strip_prefix(&root).expect("a source is under the root");
            let bytes = fs::read(&p).expect("a source is read");
            (rel.to_string_lossy().into_owned(), String::from_utf8_lossy(&bytes).into_owned())
        })
        .collect()
}

/// The lines of a manifest that its golden text holds: not blank, and not a comment.
fn golden_lines(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|line| {
            let line = line.trim_start();
            !line.is_empty() && !line.starts_with('#')
        })
        .collect()
}

#[test]
fn the_scan_reads_the_sources() {
    let found = sources();
    assert!(found.iter().any(|(path, _)| path == EXEC), "the scan found no {EXEC}");
}

#[test]
fn no_source_holds_an_everywhere_word() {
    for (path, text) in sources() {
        for word in EVERYWHERE {
            assert!(!text.contains(word), "{path} holds `{word}`");
        }
    }
}

#[test]
fn no_member_has_a_build_script() {
    let mut files = Vec::new();
    walk(&root(), &mut files);
    for path in files {
        let name = path.file_name().and_then(|name| name.to_str());
        assert!(name != Some("build.rs"), "{} is a build script", path.display());
    }
}

#[test]
fn the_exec_crate_has_one_source_file() {
    let root = root();
    let mut files = Vec::new();
    walk(&root.join("crates/exec"), &mut files);
    let mut rs: Vec<String> = files
        .iter()
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .map(|path| path.strip_prefix(&root).expect("a file is under the root").to_string_lossy().into_owned())
        .collect();
    rs.sort();
    assert_eq!(
        rs,
        [EXEC],
        "the exec crate's one source file is its lib.rs: cargo makes a target of any other .rs under src/bin, tests, examples or benches"
    );
}

#[test]
fn only_the_roots_clippy_config_is_read() {
    let root = root();
    let mut files = Vec::new();
    walk(&root, &mut files);
    for path in files {
        let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
        let config = name == "clippy.toml" || name == ".clippy.toml";
        assert!(
            !config || path.parent() == Some(root.as_path()),
            "{} is a clippy config that is not the root's",
            path.display()
        );
    }
}

/// No cargo config applies to the workspace: none in the root, or in any
/// ancestor up to the git root, the first one with a `.git` entry.
#[test]
fn no_cargo_config_applies_to_the_workspace() {
    let mut dir = fs::canonicalize(root()).expect("the root is read");
    loop {
        for name in [".cargo/config", ".cargo/config.toml"] {
            let path = dir.join(name);
            assert!(!path.exists(), "{} would configure the gate's build", path.display());
        }
        if dir.join(".git").exists() || !dir.pop() {
            break;
        }
    }
}

#[test]
fn the_manifests_are_the_golden_ones() {
    let root = root();
    let mut files = Vec::new();
    walk(&root, &mut files);
    let mut found: Vec<String> = files
        .iter()
        .filter(|path| path.file_name().is_some_and(|name| name == "Cargo.toml"))
        .map(|path| path.strip_prefix(&root).expect("a manifest is under the root").to_string_lossy().into_owned())
        .collect();
    found.sort();
    assert_eq!(
        found,
        ["Cargo.toml", "crates/door/Cargo.toml", "crates/exec/Cargo.toml", "crates/shell/Cargo.toml"],
        "the manifests under the root are not the four golden ones; a new member brings its golden in the same commit"
    );
    for (path, golden) in [
        ("Cargo.toml", ROOT_MANIFEST),
        ("crates/door/Cargo.toml", DOOR_MANIFEST),
        ("crates/exec/Cargo.toml", EXEC_MANIFEST),
        ("crates/shell/Cargo.toml", SHELL_MANIFEST),
    ] {
        let text = fs::read_to_string(root.join(path)).expect("a manifest is read");
        assert_eq!(
            golden_lines(&text),
            golden_lines(golden),
            "{path} differs from its golden text; its golden moves with it, in the same commit"
        );
    }
}
