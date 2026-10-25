use std::fs;
use std::path::{Path, PathBuf};

const EXEC: &str = "crates/shell/src/exec.rs";
const FAKE_EIDOLON: &str = "crates/shell/src/bin/fake-eidolon.rs";

/// Words no source in the desktop workspace may hold.
const EVERYWHERE: [&str; 6] = [
    concat!("aoided", ".sock"),
    concat!("dis", "patch"),
    concat!("sub", "scribe"),
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

/// The unsafe block opener: exec, and the fake eidolon's signal wait, only.
const UNSAFE_BLOCK: &str = concat!("unsafe", " {");
const UNSAFE_OK: [&str; 2] = [EXEC, FAKE_EIDOLON];

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
fn the_unsafe_block_opener_appears_only_in_exec_and_the_fake_eidolon() {
    for (path, text) in sources() {
        if !UNSAFE_OK.contains(&path.as_str()) {
            assert!(!text.contains(UNSAFE_BLOCK), "{path} opens an unsafe block");
        }
    }
}
