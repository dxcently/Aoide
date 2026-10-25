//! Every production call of `can_approve()` or `refuse_unapproving_door(` is a
//! door decision some command makes. The desktop app's floor refuses a grant
//! that names a command marked `approves` (read through `schema --json`), and
//! `cli/src/registry.rs` pins the marked paths in
//! `approves_paths_match_the_golden_snapshot`.
//!
//! This scans each crate's `src/`: every `.rs` file but those named `tests.rs`.
//! In each file it skips each line whose first non-blank characters are `//`,
//! and stops at the first line that starts, after blanks, with `mod tests {` or
//! `pub(crate) mod tests {`. It counts the lines naming either check among
//! those it reads. It does not read `tests/`, build scripts, examples or
//! benches. It enforces the list and nothing more. The list is exact, not a
//! ratchet: a file whose count differs, and a file the list does not name,
//! fails until the list moves. A new check moves the list in the same commit,
//! and the command whose handler reaches it gets `approves: true`, with its
//! path in the approves golden. A removed check's command loses `approves`, and
//! its path leaves the golden, in the same commit, unless another limb of the
//! rule still holds for it. The counts are per file, so a removed check and an
//! added one in the same listed file keep the count and pass; review checks
//! that. The scan cannot see which command a check serves, or a new caller of
//! an existing check; review checks those.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const CALLS: [&str; 2] = ["can_approve", "refuse_unapproving_door"];

/// (path under `crates/`, number of lines holding an approve check).
const CHECKS: &[(&str, usize)] = &[
    ("client/src/commands.rs", 1),
    ("conduct/src/graph/common.rs", 2),
    ("conduct/src/graph/conduct.rs", 1),
    ("conduct/src/graph/doorbell.rs", 1),
    ("conduct/src/graph/job.rs", 3),
    ("conduct/src/graph/pending.rs", 3),
    ("conduct/src/graph/permit.rs", 1),
    ("conduct/src/graph/resurrect.rs", 1),
    ("conduct/src/graph/send.rs", 2),
    ("conduct/src/graph/session_store.rs", 2),
    ("protocol/src/audit.rs", 1),
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

/// Lines of production code that name an approve check. The walk skips each
/// line whose first non-blank characters are `//`, and stops at the first line
/// that starts, after blanks, with `mod tests {` or `pub(crate) mod tests {`.
fn sites(path: &Path) -> usize {
    let text = std::fs::read_to_string(path).unwrap();
    text.lines()
        .map(str::trim)
        .take_while(|l| !l.starts_with("mod tests {") && !l.starts_with("pub(crate) mod tests {"))
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

const MOVE: &str = "a new or removed approve check moves this list in the same \
    commit. The command whose handler reaches a new check gets \
    `approves: true` on its registry entry, and its path joins \
    `approves_paths_match_the_golden_snapshot` (cli/src/registry.rs) in that \
    commit too. A removed check's command loses `approves`, and its path \
    leaves that golden in the same commit, unless another limb of the rule \
    still holds for it. This list differs from the tree:";

#[test]
fn approve_checks_match_the_list() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let diff = drift(crates, CHECKS);
    assert!(diff.is_empty(), "{MOVE}\n  {}", diff.join("\n  "));
}

const CALL: &str = "    let _ = inv.door.can_approve();\n";

/// A fresh temp tree holding each `CHECKS` file with its listed count of
/// calls. `name` keeps the tests' trees apart: they share one process.
fn listed_tree(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("aoide_approve_checks_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for (rel, n) in CHECKS {
        let file = root.join(rel);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, CALL.repeat(*n)).unwrap();
    }
    root
}

/// The scan's own proof: in a temp tree that matches the list exactly, one
/// extra call in a listed file is the one drift reported.
#[test]
fn a_new_approve_check_fails_the_scan() {
    let root = listed_tree("extra_call");
    let exact = drift(&root, CHECKS);

    let permit = root.join("conduct/src/graph/permit.rs");
    let mut text = std::fs::read_to_string(&permit).unwrap();
    text.push_str(CALL);
    std::fs::write(&permit, text).unwrap();
    let diff = drift(&root, CHECKS);
    let _ = std::fs::remove_dir_all(&root);
    assert!(exact.is_empty(), "the fixture matches the list before the extra call: {exact:?}");
    assert_eq!(diff, ["conduct/src/graph/permit.rs: 2 approve checks, 1 listed"]);
}

/// A file the list does not name, holding one check, is the one drift
/// reported, as `not listed`.
#[test]
fn a_new_file_with_a_check_is_not_listed() {
    let root = listed_tree("new_file");
    std::fs::write(root.join("conduct/src/graph/extra.rs"), CALL).unwrap();
    let diff = drift(&root, CHECKS);
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(diff, ["conduct/src/graph/extra.rs: 1 approve checks, not listed"]);
}

/// A `mod tests;` declaration is not an inline module: the checks below it
/// still count, so the tree still matches the list.
#[test]
fn a_mod_tests_declaration_does_not_stop_the_scan() {
    let root = listed_tree("mod_decl");
    let permit = root.join("conduct/src/graph/permit.rs");
    std::fs::write(&permit, format!("mod tests;\n{CALL}")).unwrap();
    let diff = drift(&root, CHECKS);
    let _ = std::fs::remove_dir_all(&root);
    assert!(diff.is_empty(), "a `mod tests;` line changes nothing: {diff:?}");
}
