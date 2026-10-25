use std::fs;
use std::path::{Component, Path, PathBuf};

const EXEC: &str = "crates/shell/src/exec.rs";
const FAKE_EIDOLON: &str = "crates/shell/src/bin/fake-eidolon.rs";

/// Words no source in the desktop workspace may hold.
const EVERYWHERE: [&str; 6] = [
    concat!("aoided", ".sock"),
    concat!("\"dis", "patch\""),
    concat!("\"sub", "scribe\""),
    concat!("/api/", "launch"),
    concat!("sh", " -c"),
    concat!("SIG", "KILL"),
];

/// Words that start a process or send a signal: exec only.
const EXEC_ONLY: [&str; 7] = [
    concat!("Command", "::new("),
    concat!("libc", "::fork"),
    concat!("exec", "v"),
    concat!("posix", "_spawn"),
    concat!("pre", "_exec"),
    concat!("libc", "::kill"),
    concat!("set", "sid"),
];

/// The lints the workspace denies, the groups that would lift them, and the
/// names the lints went by. Only exec may hold them, as whole words, on the
/// text with its ASCII whitespace removed.
const EXEC_LINTS: [&str; 6] = [
    concat!("disallowed", "_methods"),
    concat!("disallowed", "_types"),
    concat!("disallowed", "_method"),
    concat!("disallowed", "_type"),
    concat!("clippy::", "all"),
    concat!("clippy::", "style"),
];
/// Forms that bring in code the scan does not read. No source holds them.
const INCLUDES: [&str; 2] = [concat!("include", "!("), concat!("#[", "path")];
/// The lint the unchecked surface is kept out by, and the keyword it is named
/// by, spelled in pieces here.
const UNSAFE_CODE: &str = concat!("un", "safe", "_code");
const WORD_UNSAFE: &str = concat!("un", "safe");
const UNSAFE_OK: [&str; 2] = [EXEC, FAKE_EIDOLON];
/// The lines the workspace manifest holds under each table.
const DENIES: [(&str, &str); 3] = [
    ("[workspace.lints.rust]", concat!("un", "safe", "_code = \"deny\"")),
    ("[workspace.lints.clippy]", concat!("disallowed", "_methods = \"deny\"")),
    ("[workspace.lints.clippy]", concat!("disallowed", "_types = \"deny\"")),
];
/// A dollar sign, the only way a macro variable is written.
const DOLLAR: &str = "\u{24}";

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

fn source(path: &str) -> String {
    sources()
        .into_iter()
        .find(|(p, _)| p == path)
        .map(|(_, text)| text)
        .unwrap_or_else(|| panic!("{path} is scanned"))
}

/// The lines under `header`, up to the next table.
fn table<'a>(manifest: &'a str, header: &str) -> Vec<&'a str> {
    manifest
        .lines()
        .skip_while(|line| *line != header)
        .skip(1)
        .take_while(|line| !line.starts_with('['))
        .collect()
}

/// Whether `word` occurs in `text` as a whole word, not inside a longer name.
fn holds_word(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + word.len()..].chars().next();
        !before.is_some_and(name_char) && !after.is_some_and(name_char)
    })
}

fn name_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The text with its ASCII whitespace removed, so a lift spelled with spaces
/// meets the same needle as one spelled without.
fn squeezed(text: &str) -> String {
    text.chars().filter(|c| !c.is_ascii_whitespace()).collect()
}

/// The text up to the parenthesis that closes the one `text` begins inside, or
/// the whole of `text` if that one never closes.
fn group(text: &str) -> &str {
    let mut depth = 0;
    for (at, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' if depth == 0 => return &text[..at],
            ')' => depth -= 1,
            _ => {}
        }
    }
    text
}

/// The member paths the root manifest lists: its one `members` line, a list of
/// quoted paths. Any other form fails, and says so.
fn listed_members(workspace: &str) -> Vec<String> {
    let line = workspace
        .lines()
        .find(|line| line.starts_with("members"))
        .expect("the root manifest lists its members");
    let list = line
        .strip_prefix("members = [")
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or_else(|| panic!("members must be one line of quoted paths, found `{line}`"));
    list.split(',')
        .map(str::trim)
        .filter(|member| !member.is_empty())
        .map(|member| {
            member
                .strip_prefix('"')
                .and_then(|quoted| quoted.strip_suffix('"'))
                .unwrap_or_else(|| panic!("a member is one quoted path, found `{member}`"))
                .to_string()
        })
        .collect()
}

/// Whether a listed member is a relative path that stays under the root.
fn is_under_root(member: &str) -> bool {
    Path::new(member).components().all(|c| matches!(c, Component::Normal(_)))
}

/// The directory a manifest lies in, relative to the root.
fn relative_dir(root: &Path, manifest: &Path) -> String {
    let dir = manifest.parent().expect("a manifest has a directory");
    dir.strip_prefix(root).expect("a manifest is under the root").to_string_lossy().into_owned()
}

#[test]
fn the_scan_reads_the_sources() {
    let found = sources();
    assert!(found.iter().any(|(path, _)| path == EXEC), "the scan found no exec.rs");
    assert!(found.iter().any(|(path, _)| path == FAKE_EIDOLON), "the scan found no fake eidolon");
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
fn process_starting_words_appear_only_in_exec() {
    for (path, text) in sources() {
        if path == EXEC {
            continue;
        }
        for word in EXEC_ONLY {
            assert!(!text.contains(word), "{path} holds `{word}`");
        }
    }
}

#[test]
fn exec_holds_the_process_starting_words() {
    let text = source(EXEC);
    for word in [EXEC_ONLY[0], EXEC_ONLY[1], EXEC_ONLY[4], EXEC_ONLY[5], EXEC_ONLY[6]] {
        assert!(text.contains(word), "exec.rs does not hold `{word}`");
    }
}

#[test]
fn lifts_and_the_word_unsafe_appear_only_where_allowed() {
    for (path, text) in sources() {
        if !UNSAFE_OK.contains(&path.as_str()) {
            assert!(!holds_word(&text, UNSAFE_CODE), "{path} holds `{UNSAFE_CODE}`");
            assert!(!holds_word(&text, WORD_UNSAFE), "{path} holds the word {WORD_UNSAFE}");
        }
        if path != EXEC {
            let bare = squeezed(&text);
            for lint in EXEC_LINTS {
                assert!(!holds_word(&bare, lint), "{path} holds `{lint}`");
            }
        }
    }
}

#[test]
fn no_source_holds_an_include_or_a_path_attribute() {
    for (path, text) in sources() {
        let bare = squeezed(&text);
        for word in INCLUDES {
            assert!(!bare.contains(word), "{path} holds `{word}`");
        }
    }
}

/// Outside exec.rs, no source builds an attribute or a lint name from a macro
/// variable: a dollar sign right after the bracket of an attribute, or after the
/// path separator of a lint, or in the arguments of an allow, expect or cfg_attr.
#[test]
fn no_source_outside_exec_builds_an_attribute_from_a_macro_variable() {
    for (path, text) in sources() {
        if path == EXEC {
            continue;
        }
        let bare = squeezed(&text);
        for word in [concat!("#[", "\u{24}"), concat!("#![", "\u{24}"), concat!("clippy::", "\u{24}")] {
            assert!(!bare.contains(word), "{path} holds `{word}`");
        }
        for head in [concat!("allow", "("), concat!("expect", "("), concat!("cfg_attr", "(")] {
            for (at, _) in bare.match_indices(head) {
                let args = group(&bare[at + head.len()..]);
                assert!(!args.contains(DOLLAR), "{path} holds `{head}` over a macro variable");
            }
        }
    }
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
fn every_member_takes_the_workspace_lints() {
    let root = root();
    let workspace = fs::read_to_string(root.join("Cargo.toml")).expect("the workspace manifest is read");
    assert!(!workspace.contains("[package]"), "the root manifest holds a [package]");
    for (header, line) in DENIES {
        assert!(table(&workspace, header).contains(&line), "{header} does not hold `{line}`");
    }
    let mut listed = listed_members(&workspace);
    for member in &listed {
        assert!(is_under_root(member), "{member} is listed but is not under the root");
    }
    let mut files = Vec::new();
    walk(&root, &mut files);
    let root_manifest = root.join("Cargo.toml");
    let manifests: Vec<PathBuf> = files
        .into_iter()
        .filter(|p| p.file_name().is_some_and(|name| name == "Cargo.toml") && *p != root_manifest)
        .collect();
    let mut found: Vec<String> = manifests.iter().map(|p| relative_dir(&root, p)).collect();
    listed.sort();
    found.sort();
    assert_eq!(found, listed, "the manifests under the root are not the listed members");
    for path in &manifests {
        let member = fs::read_to_string(path).expect("a member manifest is read");
        let body: Vec<&str> = table(&member, "[lints]").into_iter().filter(|line| !line.trim().is_empty()).collect();
        assert_eq!(body, ["workspace = true"], "{} does not take the workspace lints", path.display());
        assert!(!member.contains("[lints."), "{} has a lints table of its own", path.display());
        assert!(!member.contains("proc-macro"), "{} is a proc-macro", path.display());
    }
    assert_eq!(manifests.len(), listed.len(), "the scan found {} members", manifests.len());
}
