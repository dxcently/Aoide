use std::fs;
use std::path::{Path, PathBuf};

const EXEC: &str = "crates/exec/src/lib.rs";

/// The golden template of the cargo config a nix build writes; the config
/// test reads it.
const NIX_CONFIG_GOLDEN: &str = "crates/shell/tests/fixtures/nix-cargo-config.toml";

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
/// Each entry's own file type decides what it is, so the walk follows no
/// symbolic link: it refuses one by name, since a walk through a link could
/// read outside the root.
fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let target = root().join("target");
    for entry in fs::read_dir(dir).expect("the directory is read") {
        let entry = entry.expect("a directory entry");
        let path = entry.path();
        let kind = entry.file_type().expect("the entry's file type is read");
        assert!(!kind.is_symlink(), "{} is a symbolic link, and a walk through one could read outside the root", path.display());
        if !kind.is_dir() {
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

/// The nix build's cargo config with every store path and the vendor directory
/// replaced by a fixed placeholder, so the golden template is the same text on
/// every build.
fn normalize_nix_config(text: &str) -> String {
    text.lines()
        .map(|line| {
            let body = line.trim_start();
            if body.starts_with("directory =") {
                format!("{}directory = \"<vendor-dir>\"\n", &line[..line.len() - body.len()])
            } else {
                format!("{}\n", replace_store_paths(line))
            }
        })
        .collect()
}

fn replace_store_paths(line: &str) -> String {
    const STORE: &str = "/nix/store/";
    let mut out = String::new();
    let mut rest = line;
    while let Some(at) = rest.find(STORE) {
        out.push_str(&rest[..at]);
        out.push_str("<store-path>");
        let after = &rest[at + STORE.len()..];
        let end = after.find(|c: char| matches!(c, '/' | '"' | '\'') || c.is_whitespace());
        rest = &after[end.unwrap_or(after.len())..];
    }
    out.push_str(rest);
    out
}

/// Where two texts first differ: the line number, and both lines there.
fn first_difference(found: &str, golden: &str) -> String {
    let (mut found, mut golden) = (found.lines(), golden.lines());
    let mut n = 0;
    loop {
        n += 1;
        match (found.next(), golden.next()) {
            (None, None) => return String::from("the final newline"),
            (a, b) if a != b => {
                return format!(
                    "line {n}: found `{}`, golden `{}`",
                    a.unwrap_or("<end of file>"),
                    b.unwrap_or("<end of file>")
                );
            }
            _ => {}
        }
    }
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

/// The files rustc read to build exec are its `src/lib.rs`, the root's
/// `clippy.toml`, the root's `Cargo.toml` and exec's `Cargo.toml`: clippy's
/// check builds list the last three. rustc lists every file it read in the
/// dep-info file beside the test binary, one `file:` line each, with `\ ` for
/// a space. A `#[path]` module or an `include!` in exec would compile another
/// file under exec's table, and this test names it.
#[test]
fn the_exec_crate_compiles_only_its_lib_rs() {
    let root = fs::canonicalize(root()).expect("the root is read");
    let deps = std::env::current_exe().expect("the test binary is known");
    let deps = deps.parent().expect("the test binary lies in a directory").to_path_buf();
    let allowed: Vec<PathBuf> = ["crates/exec/src/lib.rs", "clippy.toml", "Cargo.toml", "crates/exec/Cargo.toml"]
        .iter()
        .map(|rel| fs::canonicalize(root.join(rel)).unwrap_or_else(|_| root.join(rel)))
        .collect();
    let mut found = 0;
    for entry in fs::read_dir(&deps).expect("the deps directory is read") {
        let dep_info = entry.expect("a directory entry").path();
        let name = dep_info.file_name().and_then(|name| name.to_str()).unwrap_or_default();
        if !(name.starts_with("aoide_desktop_exec-") && name.ends_with(".d")) {
            continue;
        }
        found += 1;
        let text = fs::read_to_string(&dep_info).expect("a dep-info file is read");
        for line in text.lines().filter(|line| !line.starts_with('#')) {
            let Some(read) = line.strip_suffix(':') else { continue };
            let joined = root.join(read.replace("\\ ", " "));
            let read = fs::canonicalize(&joined).unwrap_or(joined);
            assert!(
                allowed.contains(&read),
                "{} lists {}, which rustc read to build exec and which is not one of its four allowed files",
                dep_info.display(),
                read.display()
            );
        }
    }
    assert!(found > 0, "no aoide_desktop_exec-*.d dep-info file lies in {}", deps.display());
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

/// No cargo config applies to the workspace: none in the root, in any ancestor
/// up to `/` (a git root does not stop the walk), or in `$CARGO_HOME` when
/// that is set. One is accepted: the config that nixpkgs' cargo setup hook
/// writes at `$NIX_BUILD_TOP/.cargo/config.toml` (`config` in an older
/// nixpkgs), which points cargo at the vendored crates. It is accepted only
/// when its text, with every store path and the vendor directory replaced by
/// a placeholder, equals the golden template in
/// `tests/fixtures/nix-cargo-config.toml`. Outside nix, `NIX_BUILD_TOP` is
/// unset or holds no such file, and nothing is accepted.
#[test]
fn no_cargo_config_applies_to_the_workspace() {
    let build_top = std::env::var_os("NIX_BUILD_TOP").and_then(|top| fs::canonicalize(top).ok());
    let mut dir = fs::canonicalize(root()).expect("the root is read");
    loop {
        for name in [".cargo/config", ".cargo/config.toml"] {
            let path = dir.join(name);
            if build_top.as_ref() == Some(&dir) && path.exists() {
                let text = fs::read_to_string(&path).expect("the nix build's cargo config is read");
                let found = normalize_nix_config(&text);
                let golden = fs::read_to_string(root().join(NIX_CONFIG_GOLDEN)).unwrap_or_default();
                assert!(
                    found == golden,
                    "{} differs from the golden template at {}\n--- normalized config ---\n{found}",
                    path.display(),
                    first_difference(&found, &golden)
                );
            } else {
                assert!(!path.exists(), "{} would configure the gate's build", path.display());
            }
        }
        if !dir.pop() {
            break;
        }
    }
    if let Some(home) = std::env::var_os("CARGO_HOME") {
        for name in ["config", "config.toml"] {
            let path = Path::new(&home).join(name);
            assert!(!path.exists(), "{} would configure every build", path.display());
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
