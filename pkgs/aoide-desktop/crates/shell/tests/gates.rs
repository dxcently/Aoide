use std::fs;
use std::path::{Path, PathBuf};

const EXEC: &str = "crates/shell/src/exec.rs";

/// Words no source in the desktop workspace may hold.
const EVERYWHERE: [&str; 5] = [
    concat!("aoided", ".sock"),
    concat!("dis", "patch"),
    concat!("sub", "scribe"),
    concat!("/api/", "launch"),
    concat!("sh", " -c"),
];

/// Words that start a process, or open unsafe code, and may appear only in exec.
const EXEC_ONLY: [&str; 5] = [
    concat!("Command", "::new("),
    concat!("libc", "::fork"),
    concat!("exec", "v"),
    concat!("posix", "_spawn"),
    concat!("unsafe", " {"),
];

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

#[test]
fn the_scan_reads_the_sources() {
    assert!(sources().iter().any(|(path, _)| path == EXEC), "the scan found no exec.rs");
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
fn exec_holds_the_process_starting_word() {
    let (_, text) = sources()
        .into_iter()
        .find(|(path, _)| path == EXEC)
        .expect("exec.rs is scanned");
    assert!(text.contains(EXEC_ONLY[0]), "exec.rs starts no process");
}
