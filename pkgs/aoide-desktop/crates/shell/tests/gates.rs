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
members = ["crates/exec", "crates/shell", "crates/app"]
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
[patch.crates-io]
wry = { path = "vendor/wry" }
"##;
const DOOR_MANIFEST: &str = r##"[package]
name = "aoide-door"
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
[lints.rust]
unsafe_code = "forbid"
"##;
/// The door is a core crate now, beside this workspace, and is scanned here
/// by the same tests as the members.
const DOOR: &str = "../aoide/crates/door";
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
const APP_MANIFEST: &str = r##"[package]
name = "aoide-desktop-app"
version = "0.1.0"
edition = "2021"
publish = false

[lib]
# A doctest is a separate crate that no clippy run lints; no member runs them.
doctest = false

[[bin]]
name = "aoide-desktop"
path = "src/main.rs"

[[test]]
name = "smoke"
path = "tests/smoke.rs"
required-features = ["smoke"]

[features]
# The headless smoke run (B2). No default or release build turns it on.
smoke = []

[dependencies]
aoide-desktop-shell = { path = "../shell" }
serde.workspace = true
serde_json.workspace = true
tauri = { version = "=2.12.5", default-features = false, features = ["wry", "custom-protocol"] }
gtk = "=0.18.2"

[build-dependencies]
tauri-build = { version = "=2.7.4", default-features = false }

[dev-dependencies]
# The mock runtime's `test` feature, and the shell's fake aoide and eidolon.
tauri = { version = "=2.12.5", default-features = false, features = ["wry", "custom-protocol", "test"] }
aoide-desktop-shell = { path = "../shell", features = ["fakes"] }

[lints]
workspace = true
"##;
const SHELL_MANIFEST: &str = r##"[package]
name = "aoide-desktop-shell"
version = "0.1.0"
edition = "2021"
publish = false
[lib]
doctest = false
[features]
fakes = ["aoide-door/fake", "aoide-desktop-exec/fakes"]
[[bin]]
name = "fake-aoide"
path = "src/bin/fake-aoide.rs"
required-features = ["fakes"]
[[bin]]
name = "fake-eidolon"
path = "src/bin/fake-eidolon.rs"
required-features = ["fakes"]
[dependencies]
aoide-door = { path = "../../../aoide/crates/door" }
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

/// Every file under `dir`, but not under the root's own `target` directory or
/// its `vendor` directory, which holds the vendored wry the last gate pins.
/// Each entry's own file type decides what it is, so the walk follows no
/// symbolic link: it refuses one by name, since a walk through a link could
/// read outside the root.
fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let target = root().join("target");
    let vendor = root().join("vendor");
    for entry in fs::read_dir(dir).expect("the directory is read") {
        let entry = entry.expect("a directory entry");
        let path = entry.path();
        let kind = entry.file_type().expect("the entry's file type is read");
        assert!(!kind.is_symlink(), "{} is a symbolic link, and a walk through one could read outside the root", path.display());
        if !kind.is_dir() {
            out.push(path);
        } else if path != target && path != vendor {
            walk(&path, out);
        }
    }
}

/// Each `.rs` under the desktop workspace root and under the door, and each
/// file under `ui/` once that exists, as its path relative to the root (the
/// door's as `DOOR/...`) and its text.
fn sources() -> Vec<(String, String)> {
    let root = root();
    let door = root.join(DOOR);
    let ui = root.join("ui");
    let mut paths = Vec::new();
    walk(&root, &mut paths);
    walk(&door, &mut paths);
    paths.retain(|p| p.extension().is_some_and(|e| e == "rs") || p.starts_with(&ui));
    paths
        .into_iter()
        .map(|p| {
            let rel = match p.strip_prefix(&root) {
                Ok(rel) => rel.to_string_lossy().into_owned(),
                Err(_) => format!("{DOOR}/{}", p.strip_prefix(&door).expect("a source is under the root or the door").display()),
            };
            let bytes = fs::read(&p).expect("a source is read");
            (rel, String::from_utf8_lossy(&bytes).into_owned())
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
    for member in ["crates/shell", DOOR] {
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

/// The door client spawns nothing: it only dials. Spelled in pieces so this file
/// does not hold the words it bars.
const NO_SPAWN: [&str; 3] = [concat!("Comm", "and"), concat!("pre_", "exec"), concat!("process::", "Child")];

#[test]
fn the_door_client_spawns_no_process() {
    let mut files = Vec::new();
    walk(&root().join(DOOR).join("src"), &mut files);
    assert!(!files.is_empty(), "the scan found no source under {DOOR}/src");
    for path in files {
        let text = String::from_utf8_lossy(&fs::read(&path).expect("a source is read")).into_owned();
        for word in NO_SPAWN {
            assert!(!text.contains(word), "{} holds `{word}`", path.display());
        }
    }
}

/// The one build script, the app's: tauri-build's manifest of the window's
/// commands. Its golden text is compared without blank and comment lines.
const BUILD_SCRIPT: &str = r##"const COMMANDS: &[&str] = &["palette", "run", "sessions", "chat_open", "chat_say", "chat_close", "smoke_note"];

fn main() {
    // A smoke build reads the smoke capability too. No other build does.
    let capabilities = if std::env::var_os("CARGO_FEATURE_SMOKE").is_some() {
        "./capabilities/*.json"
    } else {
        "./capabilities/main.json"
    };
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .capabilities_path_pattern(capabilities)
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("tauri-build writes the app's permissions");
}
"##;

#[test]
fn the_one_build_script_is_the_apps() {
    let root = root();
    let mut files = Vec::new();
    walk(&root, &mut files);
    let mut scripts: Vec<String> = files
        .iter()
        .filter(|path| path.file_name().is_some_and(|name| name == "build.rs"))
        .map(|path| path.strip_prefix(&root).expect("a build script is under the root").to_string_lossy().into_owned())
        .collect();
    scripts.sort();
    assert_eq!(scripts, ["crates/app/build.rs"], "the only build script is the app's");
    let mut door_files = Vec::new();
    walk(&root.join(DOOR), &mut door_files);
    for path in door_files {
        let name = path.file_name().and_then(|name| name.to_str());
        assert!(name != Some("build.rs"), "{} is a build script", path.display());
    }
    let text = fs::read_to_string(root.join("crates/app/build.rs")).expect("the app's build script is read");
    assert_eq!(golden_lines(&text), golden_lines(BUILD_SCRIPT), "the app's build script differs from its golden text");
}

/// The lines of `text` after a `#[tauri::command]`, and the names the attribute marks.
fn command_fns(text: &str) -> Vec<(String, Option<String>)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if line.trim() != "#[tauri::command]" {
            continue;
        }
        let next = lines[i + 1..]
            .iter()
            .map(|l| l.trim())
            .find(|l| !l.starts_with("#[") && !l.starts_with("//") && !l.is_empty())
            .unwrap_or("");
        let name = next
            .split_once("fn ")
            .map(|(_, rest)| rest.split(['(', '<']).next().unwrap_or("").trim().to_owned());
        out.push((next.to_owned(), name));
    }
    out
}

#[test]
fn every_tauri_command_is_an_async_fn() {
    let mut found = 0;
    for (path, text) in sources() {
        if !path.starts_with("crates/app/src/") {
            continue;
        }
        for (next, _) in command_fns(&text) {
            found += 1;
            assert!(
                next.starts_with("async fn ") || next.starts_with("pub async fn "),
                "{path}: a #[tauri::command] is followed by `{next}`, not an `async fn` (every command runs off the main thread)"
            );
        }
    }
    assert!(found > 0, "the scan found no commands; the app's commands are under crates/app/src");
}

/// The names `generate_handler!` lists in the app's lib, the names build.rs's
/// manifest lists, and the names the capabilities grant, each as a sorted
/// list. The three must be the same set, so an unregistered command fails.
#[test]
fn every_handler_command_is_in_the_manifest_and_granted_to_main() {
    let root = root();
    let lib = fs::read_to_string(root.join("crates/app/src/lib.rs")).expect("the app's lib is read");
    let start = lib.find("generate_handler![").expect("the lib names its handler");
    let body = &lib[start + "generate_handler![".len()..];
    let body = &body[..body.find("\n    ]").expect("the handler list closes")];
    // An entry may carry a `#[cfg]` line before its path; the path is what counts.
    let mut handler: Vec<String> = body
        .split(',')
        .map(|entry| entry.lines().map(str::trim).filter(|l| !l.starts_with("#[")).collect::<String>())
        .filter(|e| !e.is_empty())
        .map(|e| e.trim_start_matches("commands::").to_owned())
        .collect();
    handler.sort();

    let build = fs::read_to_string(root.join("crates/app/build.rs")).expect("the app's build script is read");
    let list = &build[build.find("COMMANDS: &[&str] = &[").expect("the manifest lists commands")..];
    let list = &list["COMMANDS: &[&str] = &[".len()..];
    let list = &list[..list.find(']').expect("the manifest list closes")];
    let mut manifest: Vec<String> = list
        .split(',')
        .map(|e| e.trim().trim_matches('"').to_owned())
        .filter(|e| !e.is_empty())
        .collect();
    manifest.sort();

    // main.json grants the window's commands; smoke.json grants the smoke
    // command, which only a smoke build reads. Together they are the grants.
    let mut granted: Vec<String> = Vec::new();
    for capability in ["crates/app/capabilities/main.json", "crates/app/capabilities/smoke.json"] {
        let caps: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(root.join(capability)).expect("the capability is read"),
        )
        .expect("the capability is JSON");
        granted.extend(
            caps["permissions"]
                .as_array()
                .expect("the capability lists permissions")
                .iter()
                .map(|p| p.as_str().expect("a permission is a string").strip_prefix("allow-").expect("an allow permission").replace('-', "_")),
        );
    }
    granted.sort();

    let mut sources_defined: Vec<String> = Vec::new();
    for (path, text) in sources() {
        if path.starts_with("crates/app/src/") {
            sources_defined.extend(command_fns(&text).into_iter().filter_map(|(_, name)| name));
        }
    }
    sources_defined.sort();

    assert_eq!(handler, manifest, "generate_handler! and build.rs's manifest name different commands");
    assert_eq!(handler, granted, "generate_handler! and the capabilities name different commands");
    assert_eq!(handler, sources_defined, "every #[tauri::command] must be listed in generate_handler!");
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
    walk(&root.join(DOOR), &mut files);
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
        ["Cargo.toml", "crates/app/Cargo.toml", "crates/exec/Cargo.toml", "crates/shell/Cargo.toml"],
        "the manifests under the root are not the four golden ones; a new member brings its golden in the same commit"
    );
    for (path, golden) in [
        ("Cargo.toml", ROOT_MANIFEST),
        ("crates/app/Cargo.toml", APP_MANIFEST),
        ("crates/exec/Cargo.toml", EXEC_MANIFEST),
        ("crates/shell/Cargo.toml", SHELL_MANIFEST),
        ("../aoide/crates/door/Cargo.toml", DOOR_MANIFEST),
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

/// The four lines the fork adds to wry's web_context.rs, as the file holds them.
const SANDBOX_BLOCK: &str = "    // aoide fork: WebKit's sandbox is opt-in and is set only before the first\n    // WebView exists, which this function precedes. This is the only change\n    // from wry 0.57.0; see pkgs/aoide-desktop/README.md.\n    context.set_sandbox_enabled(true);\n";

/// The digest of wry 0.57.0 as the registry ships it: sha256 over `path NUL
/// sha256(file) LF` for each file, sorted by path.
const WRY_0_57_0_DIGEST: &str = "f0dfea1188ecc1c4d1317e70b47542ec3bee528d8554e9013bf5354c49d33936";

/// The vendored wry is wry 0.57.0 less the sandbox block, file for file: the
/// block is removed from web_context.rs before the digest is taken.
#[test]
fn the_vendored_wry_is_0_57_0_plus_the_sandbox_block() {
    use sha2::{Digest, Sha256};
    let vendor = root().join("vendor/wry");
    let mut files = Vec::new();
    walk(&vendor, &mut files);
    let mut rows: Vec<(String, String)> = Vec::new();
    for path in files {
        let rel = path.strip_prefix(&vendor).expect("a vendored file is under vendor/wry").to_string_lossy().into_owned();
        let mut bytes = fs::read(&path).expect("a vendored file is read");
        if rel == "src/webkitgtk/web_context.rs" {
            let text = String::from_utf8(bytes).expect("web_context.rs is UTF-8");
            assert_eq!(text.matches(SANDBOX_BLOCK).count(), 1, "web_context.rs holds the sandbox block once");
            bytes = text.replacen(SANDBOX_BLOCK, "", 1).into_bytes();
        }
        rows.push((rel, format!("{:x}", Sha256::digest(&bytes))));
    }
    rows.sort();
    let listing: String = rows.iter().map(|(rel, hash)| format!("{rel}\0{hash}\n")).collect();
    assert_eq!(
        format!("{:x}", Sha256::digest(listing.as_bytes())),
        WRY_0_57_0_DIGEST,
        "vendor/wry is not wry 0.57.0 plus the sandbox block; re-sync it as the README says"
    );
}
