//! Argument enforcement is declared in the registry (`Flag::required`,
//! `Command::one_of`, `Arg::required`) and checked once by `Command::check`;
//! a handler no longer writes its own "missing --x" / "usage: aoide …".
//!
//! This scans every crate's non-test source for the three hand-written
//! spellings and fails when a file has more sites than `ALLOWED` grants it.
//! The list is a ratchet: each slice that migrates a command deletes its
//! hand check and lowers (or removes) that file's number; nothing may raise
//! one or add a file.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const SPELLINGS: [&str; 3] = ["missing --", "requires --", "usage: aoide"];

/// (path under `crates/`, number of hand-enforcement lines it may hold).
const ALLOWED: &[(&str, usize)] = &[
    ("client/src/context.rs", 1),
    ("client/src/mcp_client.rs", 1),
    ("conduct/src/commands/hooks.rs", 1),
    ("conduct/src/graph/common.rs", 2),
    ("conduct/src/graph/conduct.rs", 1),
    ("conduct/src/graph/doorbell.rs", 1),
    ("conduct/src/graph/grant.rs", 2),
    ("conduct/src/graph/manage.rs", 1),
    ("conduct/src/graph/send.rs", 3),
    ("conduct/src/graph/spawn.rs", 1),
    ("conduct/src/graph/undying.rs", 1),
    ("conduct/src/graph/workspace.rs", 4),
    ("lyra/src/commands/pair.rs", 7),
    ("lyra/src/commands/preview_tools.rs", 3),
    ("lyra/src/commands/secrets.rs", 3),
    ("lyra/src/commands/stubs.rs", 1),
    ("screen/src/commands.rs", 1),
    ("screen/src/diff.rs", 1),
    ("screen/src/ocr.rs", 1),
    ("screen/src/point.rs", 2),
    ("screen/src/send.rs", 1),
    ("screen/src/text.rs", 1),
    ("song/src/commands/cover.rs", 3),
    ("song/src/commands/draft.rs", 2),
    ("song/src/commands/livery.rs", 2),
    ("song/src/commands/mode.rs", 1),
    ("song/src/commands/rice.rs", 3),
    ("song/src/commands/take.rs", 2),
    ("storage/src/commands.rs", 1),
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
/// that carry one of the hand-written spellings.
fn sites(path: &Path) -> usize {
    let text = std::fs::read_to_string(path).unwrap();
    text.lines()
        .map(str::trim)
        .take_while(|l| !l.starts_with("mod tests") && !l.starts_with("pub(crate) mod tests"))
        .filter(|l| !l.starts_with("//"))
        .filter(|l| SPELLINGS.iter().any(|s| l.contains(s)))
        .count()
}

#[test]
fn no_hand_enforcement() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let allowed: BTreeMap<&str, usize> = ALLOWED.iter().copied().collect();
    let mut files = Vec::new();
    for krate in std::fs::read_dir(crates).unwrap().flatten() {
        let src = krate.path().join("src");
        if src.is_dir() {
            sources(&src, &mut files);
        }
    }
    let mut over = Vec::new();
    for file in files {
        let n = sites(&file);
        let rel = file.strip_prefix(crates).unwrap().to_string_lossy().replace('\\', "/");
        if n > allowed.get(rel.as_str()).copied().unwrap_or(0) {
            over.push(format!("{rel}: {n} hand-written checks, {} allowed", allowed.get(rel.as_str()).copied().unwrap_or(0)));
        }
    }
    assert!(
        over.is_empty(),
        "declare it on the registry entry (`required: true`, `one_of`) instead of checking by hand:\n  {}",
        over.join("\n  ")
    );
}

#[test]
fn every_allowed_file_exists() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    for (rel, _) in ALLOWED {
        assert!(crates.join(rel).is_file(), "{rel} is listed but gone — delete its line");
    }
}
