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

/// Words no door or shell source may hold: each one opens foreign code that the
/// lints do not see. Exec is the one member that allows unsafe code, so it is
/// not scanned for them.
const NO_FOREIGN: [&str; 2] = [concat!("ext", "ern"), concat!("#[", "link")];

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
[lib]
doctest = false
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
[lib]
doctest = false
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
[lib]
doctest = false
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

/// The root `clippy.toml`'s golden text, compared as the manifests' are.
const CLIPPY_CONFIG: &str = r##"# The desktop gate's clippy run. A call through an alias of any path below
# fails it. The exec crate's lints table allows these lints, and every other
# member forbids them.
disallowed-types = [
    { path = "std::process::Command", reason = "only the exec crate starts a process" },
]
disallowed-methods = [
    { path = "std::process::Command::new", reason = "only the exec crate starts a process" },
    { path = "std::os::unix::process::CommandExt::pre_exec", reason = "only the exec crate runs code between fork and exec" },
    { path = "std::os::unix::process::CommandExt::exec", reason = "only the exec crate replaces a process image" },
    { path = "std::process::Child::kill", reason = "it sends the uncatchable signal; a child is signalled through the exec crate" },
]
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
fn normalize_nix_config(text: &str, vendor: &str) -> String {
    text.lines()
        .map(|line| {
            let body = line.trim_start();
            let value = body.strip_prefix("directory =").map(|value| value.trim().trim_matches('"'));
            if value == Some(vendor) {
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
        let after = &rest[at + STORE.len()..];
        match store_path_len(after) {
            Some(len) => {
                out.push_str("<store-path>");
                rest = &after[len..];
            }
            None => {
                out.push_str(STORE);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The length of the store path at the start of `after` (the text past
/// `/nix/store/`): nix's 32 base-32 characters, `-`, and a name up to the next
/// `/`, `"` or the end. Anything else is no store path.
fn store_path_len(after: &str) -> Option<usize> {
    const BASE32: &str = "0123456789abcdfghijklmnpqrsvwxyz";
    let hash = after.get(..32)?;
    if !hash.chars().all(|c| BASE32.contains(c)) || after.as_bytes().get(32) != Some(&b'-') {
        return None;
    }
    let name = &after[33..];
    let end = name.find(|c: char| !(c.is_ascii_alphanumeric() || "+._?=-".contains(c))).unwrap_or(name.len());
    (end > 0 && name[end..].starts_with(['/', '"'])).then_some(33 + end)
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
fn no_door_or_shell_source_holds_a_foreign_word() {
    for member in ["crates/door", "crates/shell"] {
        let mut files = Vec::new();
        walk(&root().join(member).join("src"), &mut files);
        for path in files {
            let text = String::from_utf8_lossy(&fs::read(&path).expect("a source is read")).into_owned();
            for word in NO_FOREIGN {
                assert!(!text.contains(word), "{} holds `{word}`", path.display());
            }
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

/// The files rustc read to build exec, as its dep-info files name them: each
/// file lies in `src/lib.rs`, the root's `clippy.toml` or `Cargo.toml`, or
/// exec's `Cargo.toml`, and every dep-info file lists `src/lib.rs`. Rustc writes
/// one file in a dep-info file per line: a target line, `<target>: <files>`,
/// whose target lies in the deps directory; a `<file>:` line; or a
/// `# env-dep:` line. A `#[path]` module or an `include!` in exec would compile
/// another file under exec's table, and this test names it. Any other line
/// reds, naming the file and the line.
#[test]
fn the_exec_crate_compiles_only_its_lib_rs() {
    let root = fs::canonicalize(root()).expect("the root is read");
    let deps = std::env::current_exe().expect("the test binary is known");
    let deps = deps.parent().expect("the test binary lies in a directory").to_path_buf();
    let deps_canonical = fs::canonicalize(&deps).expect("the deps directory is read");
    let lib = fs::canonicalize(root.join(EXEC)).expect("exec's lib.rs is read");
    let allowed: Vec<PathBuf> = [EXEC, "clippy.toml", "Cargo.toml", "crates/exec/Cargo.toml"]
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
        let mut lists_lib = false;
        for line in text.lines() {
            if line.trim().is_empty() || line.starts_with("# env-dep:") {
                continue;
            }
            let shown = format!("{} line `{line}`", dep_info.display());
            let Some((listed, rest)) = line.split_once(':') else {
                panic!("{shown} is not a target, a file or an env-dep line");
            };
            let files = if rest.trim().is_empty() {
                vec![unescape(listed)]
            } else {
                let target = fs::canonicalize(root.join(unescape(listed))).ok();
                let in_deps = target.and_then(|t| t.parent().map(|p| p == deps_canonical));
                assert!(in_deps == Some(true), "{shown} is a target outside the deps directory");
                dep_files(rest)
            };
            for file in files {
                let joined = root.join(&file);
                let read = fs::canonicalize(&joined).unwrap_or(joined);
                assert!(
                    allowed.contains(&read),
                    "{shown} lists {}, which rustc read to build exec and which is not one of its allowed files",
                    read.display()
                );
                lists_lib |= read == lib;
            }
        }
        assert!(lists_lib, "{} lists no src/lib.rs, so it read nothing of exec", dep_info.display());
    }
    assert!(found > 0, "no aoide_desktop_exec-*.d dep-info file lies in {}", deps.display());
}

/// A name in a dep-info file: `\ ` is a space.
fn unescape(name: &str) -> String {
    name.replace("\\ ", " ")
}

/// The files a dep-info line lists after its colon: spaces part them, and `\ `
/// is a space within a name.
fn dep_files(list: &str) -> Vec<String> {
    let mut files = vec![String::new()];
    let mut chars = list.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&' ') => {
                chars.next();
                files.last_mut().expect("a file is open").push(' ');
            }
            ' ' => files.push(String::new()),
            c => files.last_mut().expect("a file is open").push(c),
        }
    }
    files.retain(|file| !file.is_empty());
    files
}

#[test]
fn only_the_roots_clippy_config_is_read() {
    let root = root();
    let mut files = Vec::new();
    walk(&root, &mut files);
    for path in files {
        let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
        let hidden = name == ".clippy.toml";
        let stray = name == "clippy.toml" && path.parent() != Some(root.as_path());
        assert!(
            !hidden && !stray,
            "{} is a clippy config, and the root's clippy.toml is the only one allowed",
            path.display()
        );
    }
}

/// No cargo config applies to the workspace: none in the root, in any ancestor
/// up to `/` (a git root does not stop the walk), or in the cargo home. One is
/// accepted: `.cargo/config.toml` at `$NIX_BUILD_TOP`, the name the pinned
/// nixpkgs' cargo setup hook writes, which points cargo at the vendored crates.
/// It is accepted only when its text, with every store path and the vendor
/// directory `cargo-vendor-dir` (the relative value the pinned hook writes,
/// which cargo resolves against the build top) replaced by a placeholder,
/// equals the golden template in `tests/fixtures/nix-cargo-config.toml`. The
/// golden fixes the linker line too. Outside nix, `NIX_BUILD_TOP` is unset or
/// holds no such file, and nothing is accepted. The cargo home is
/// `$CARGO_HOME`, or `$HOME/.cargo` when that is unset.
#[test]
fn no_cargo_config_applies_to_the_workspace() {
    let build_top = std::env::var_os("NIX_BUILD_TOP").and_then(|top| fs::canonicalize(top).ok());
    let vendor = "cargo-vendor-dir";
    let mut dir = fs::canonicalize(root()).expect("the root is read");
    loop {
        for name in [".cargo/config", ".cargo/config.toml"] {
            let path = dir.join(name);
            if build_top.as_ref() == Some(&dir) && name == ".cargo/config.toml" && path.exists() {
                let text = fs::read_to_string(&path).expect("the nix build's cargo config is read");
                let found = normalize_nix_config(&text, vendor);
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
    let home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| Path::new(&home).join(".cargo")))
        .expect("CARGO_HOME or HOME names the cargo home");
    for name in ["config", "config.toml"] {
        let path = home.join(name);
        assert!(!path.exists(), "{} would configure every build", path.display());
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
    let clippy = root.join("clippy.toml");
    assert!(
        fs::symlink_metadata(&clippy).is_ok_and(|meta| meta.is_file()),
        "{} is not a file; the gate's clippy run reads no config without it",
        clippy.display()
    );
    let text = fs::read_to_string(&clippy).expect("clippy.toml is read");
    assert_eq!(
        golden_lines(&text),
        golden_lines(CLIPPY_CONFIG),
        "clippy.toml differs from its golden text; its golden moves with it, in the same commit"
    );
}

/// The normalizer is pinned by a table: the accepted lines normalize to the
/// golden text, and each refused line does not, so the golden compare reds on it.
#[test]
fn the_config_normalizer_is_pinned_by_a_table_of_known_bad_paths() {
    let vendor = "cargo-vendor-dir";
    let hash = "0123456789abcdfghijklmnpqrsvwxyz";
    let linker = "\"linker\" = \"<store-path>/bin/cc\"";
    let dir = "directory = \"<vendor-dir>\"";
    let accepted = [
        (format!("\"linker\" = \"/nix/store/{hash}-cc/bin/cc\""), linker),
        (format!("    \"linker\" = \"/nix/store/{hash}-cc/bin/cc\""), "    \"linker\" = \"<store-path>/bin/cc\""),
        ("directory = \"cargo-vendor-dir\"".to_string(), dir),
    ];
    for (line, golden) in &accepted {
        assert_eq!(normalize_nix_config(line, vendor), format!("{golden}\n"), "{line} is accepted");
    }
    let refused = [
        ("\"linker\" = \"/nix/store/../bin/cc\"".to_string(), linker),
        (format!("\"linker\" = \"/nix/store/{}-cc/bin/cc\"", &hash[..31]), linker),
        (format!("\"linker\" = \"/nix/store/e{}-cc/bin/cc\"", &hash[1..]), linker),
        (format!("\"linker\" = \"/nix/store/{hash}-cc\"/bin/cc"), linker),
        (format!("\"linker\" = \"/nix/store/{hash}-cc/bin/cc/extra\""), linker),
        (format!("\"linker\" = \"/nix/store/{hash}-c c/bin/cc\""), linker),
        ("directory = \"cargo-vendor-dir2\"".to_string(), dir),
        ("directory = \"cargo-vendor-di\"".to_string(), dir),
    ];
    for (line, golden) in &refused {
        assert_ne!(normalize_nix_config(line, vendor), format!("{golden}\n"), "{line} is refused");
    }
}
