use std::fs;
use std::path::{Path, PathBuf};

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

/// Lints the workspace denies, and the groups that would lift them. Only exec
/// may hold them, as whole words.
const EXEC_LINTS: [&str; 4] = [
    concat!("disallowed", "_methods"),
    concat!("disallowed", "_types"),
    concat!("clippy::", "all"),
    concat!("clippy::", "style"),
];
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

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("the directory is read") {
        let path = entry.expect("a directory entry").path();
        if !path.is_dir() {
            out.push(path);
        } else if !path.ends_with("target") {
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
            for lint in EXEC_LINTS {
                assert!(!holds_word(&text, lint), "{path} holds `{lint}`");
            }
        }
    }
}

#[test]
fn no_source_holds_an_include_or_a_path_attribute() {
    for (path, text) in sources() {
        for word in [concat!("include", "!("), concat!("#[", "path")] {
            assert!(!text.contains(word), "{path} holds `{word}`");
        }
    }
}

#[test]
fn every_member_takes_the_workspace_lints() {
    let root = root();
    let workspace = fs::read_to_string(root.join("Cargo.toml")).expect("the workspace manifest is read");
    for (header, line) in DENIES {
        assert!(table(&workspace, header).contains(&line), "{header} does not hold `{line}`");
    }
    let mut manifests = Vec::new();
    walk(&root.join("crates"), &mut manifests);
    manifests.retain(|p| p.file_name().is_some_and(|name| name == "Cargo.toml"));
    for path in &manifests {
        let member = fs::read_to_string(path).expect("a member manifest is read");
        let first = table(&member, "[lints]").first().copied();
        assert_eq!(first, Some("workspace = true"), "{} does not take the workspace lints", path.display());
    }
    assert_eq!(manifests.len(), 2, "the scan found {} members", manifests.len());
}
