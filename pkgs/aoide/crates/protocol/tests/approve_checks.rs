//! Every production call of `can_approve()` or `refuse_unapproving_door(` is a
//! door decision some command makes. The desktop app's floor refuses a grant
//! that names a command marked `approves` (read through `schema --json`), and
//! `cli/src/registry.rs` pins the marked paths in
//! `approves_paths_match_the_golden_snapshot`.
//!
//! This scans every crate's production source (up to the file's `mod tests`,
//! comments skipped) and counts the lines holding either call. The list is
//! exact, not a ratchet: a file whose count differs, and a file the list does
//! not name, fails until the list and the command's mark move together.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const CALLS: [&str; 2] = ["can_approve()", "refuse_unapproving_door("];

/// (path under `crates/`, number of lines holding an approve check).
const CHECKS: &[(&str, usize)] = &[
    ("client/src/commands.rs", 1),
    ("conduct/src/graph/common.rs", 2),
    ("conduct/src/graph/conduct.rs", 1),
    ("conduct/src/graph/doorbell.rs", 1),
    ("conduct/src/graph/job.rs", 2),
    ("conduct/src/graph/pending.rs", 2),
    ("conduct/src/graph/permit.rs", 1),
    ("conduct/src/graph/resurrect.rs", 1),
    ("conduct/src/graph/send.rs", 2),
    ("conduct/src/graph/session_store.rs", 1),
];

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") && path.file_name().is_some_and(|n| n != "tests.rs") {
            out.push(path);
        }
    }
}

/// Lines of production code (up to the file's `mod tests`, comments skipped)
/// that hold an approve check.
fn sites(path: &Path) -> usize {
    let text = std::fs::read_to_string(path).unwrap();
    text.lines()
        .map(str::trim)
        .take_while(|l| !l.starts_with("mod tests") && !l.starts_with("pub(crate) mod tests"))
        .filter(|l| !l.starts_with("//"))
        .filter(|l| CALLS.iter().any(|c| l.contains(c)))
        .count()
}

/// Every way the approve checks under `crates` differ from `checks`: a file
/// whose count is not the listed one, and a file the list does not name.
fn drift(crates: &Path, checks: &[(&str, usize)]) -> Vec<String> {
    let mut files = Vec::new();
    for krate in std::fs::read_dir(crates).unwrap().flatten() {
        let src = krate.path().join("src");
        if src.is_dir() {
            sources(&src, &mut files);
        }
    }
    let mut found: BTreeMap<String, usize> = BTreeMap::new();
    for file in files {
        let n = sites(&file);
        if n > 0 {
            let rel = file.strip_prefix(crates).unwrap().to_string_lossy().replace('\\', "/");
            found.insert(rel, n);
        }
    }
    let listed: BTreeMap<&str, usize> = checks.iter().copied().collect();
    let mut out = Vec::new();
    for (rel, want) in &listed {
        let got = found.get(*rel).copied().unwrap_or(0);
        if got != *want {
            out.push(format!("{rel}: {got} approve checks, {want} listed"));
        }
    }
    for (rel, got) in &found {
        if !listed.contains_key(rel.as_str()) {
            out.push(format!("{rel}: {got} approve checks, not listed"));
        }
    }
    out
}

const MOVE: &str = "a new approve check needs its command marked: the command whose handler \
    reaches the call gets `approves: true` on its registry entry, its path joins \
    `approves_paths_match_the_golden_snapshot` (cli/src/registry.rs), and this list \
    moves with them in the same commit:";

#[test]
fn approve_checks_match_the_list() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let diff = drift(crates, CHECKS);
    assert!(diff.is_empty(), "{MOVE}\n  {}", diff.join("\n  "));
}

/// The scan's own proof: in a temp tree that matches the list exactly, one
/// extra call in a listed file is the one drift reported.
#[test]
fn a_new_approve_check_fails_the_scan() {
    let root = std::env::temp_dir().join(format!("aoide_approve_checks_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let call = "    let _ = inv.door.can_approve();\n";
    for (rel, n) in CHECKS {
        let file = root.join(rel);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, call.repeat(*n)).unwrap();
    }
    let exact = drift(&root, CHECKS);
    assert!(exact.is_empty(), "the fixture matches the list before the extra call: {exact:?}");

    let permit = root.join("conduct/src/graph/permit.rs");
    let mut text = std::fs::read_to_string(&permit).unwrap();
    text.push_str(call);
    std::fs::write(&permit, text).unwrap();
    let diff = drift(&root, CHECKS);
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(diff, ["conduct/src/graph/permit.rs: 2 approve checks, 1 listed"]);
}
