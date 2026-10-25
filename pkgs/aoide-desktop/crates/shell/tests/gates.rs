#![deny(unsafe_code)]

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

/// Attributes that lift a lint. Only exec and the fake eidolon may hold them.
const LIFTS: [&str; 3] = [
    concat!("allow(", "unsafe_code"),
    concat!("allow(clippy::", "disallowed_methods"),
    concat!("allow(clippy::", "disallowed_types"),
];
/// The keyword the unchecked surface is named by, spelled in pieces here.
const WORD_UNSAFE: &str = concat!("un", "safe");
const UNSAFE_OK: [&str; 2] = [EXEC, FAKE_EIDOLON];
const DENY_ROOT: &str = concat!("#![deny(", "unsafe_code)]");

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("the directory is read") {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            walk(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// Each `.rs` under `crates/`, and each file under `ui/` once that exists, as
/// its path relative to the desktop workspace and its text.
fn sources() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut paths = Vec::new();
    walk(&root.join("crates"), &mut paths);
    paths.retain(|p| p.extension().is_some_and(|e| e == "rs"));
    if root.join("ui").is_dir() {
        walk(&root.join("ui"), &mut paths);
    }
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
fn lifts_and_the_word_unsafe_appear_only_in_exec_and_the_fake_eidolon() {
    for (path, text) in sources() {
        if UNSAFE_OK.contains(&path.as_str()) {
            continue;
        }
        for lift in LIFTS {
            assert!(!text.contains(lift), "{path} holds `{lift}`");
        }
        assert!(!holds_word(&text, WORD_UNSAFE), "{path} holds the word {WORD_UNSAFE}");
    }
}

#[test]
fn every_crate_root_denies_unsafe_code_but_the_fake_eidolon() {
    let mut roots = 0;
    for (path, text) in sources() {
        let root = path.ends_with("/src/lib.rs") || path.contains("/src/bin/") || path.contains("/tests/");
        if !root || path == FAKE_EIDOLON {
            continue;
        }
        roots += 1;
        assert!(text.contains(DENY_ROOT), "{path} does not deny unsafe_code");
    }
    assert!(roots >= 10, "the scan found {roots} crate roots");
}
