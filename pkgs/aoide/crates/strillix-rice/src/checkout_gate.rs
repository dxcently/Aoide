//! The no-archive gate for `strillix rice declare <song>` (CONTRACTS.md §5, "The
//! rices is versioned score"): the copy writes over `rices/<song>/`
//! in the checkout, so it runs only when git can show nothing uncommitted in
//! that folder. Git history is the archive; a copy over an uncommitted edit is
//! the one loss this seam can cause, and the gate is what refuses it.
//!
//! Three answers refuse, each naming the folder it judged: git not installed,
//! the checkout not a git repository, and uncommitted changes under the folder
//! (tracked edits and untracked files alike). `--host` never refuses; it reads
//! [`dirty_lines`] and reports the same changes as a warning. A fourth refusal,
//! [`is_old_song_shape`], is the copy's other guard: a staged `rice.nix` still
//! in the old function-module shape would break the checkout's build.

use aoide_protocol::output::{Fix, Kind, Outcome};
use aoide_storage::git;
use serde_json::json;
use std::path::Path;

/// Why a song folder in the checkout cannot be copied over.
#[derive(Debug, PartialEq, Eq)]
pub enum Blocked {
    GitMissing,
    NotARepo,
    Dirty(Vec<String>),
    StatusFailed(String),
}

/// `Ok` when `folder` (relative to `checkout`) has nothing uncommitted.
pub fn check_folder(checkout: &Path, folder: &str) -> Result<(), Blocked> {
    if !git::installed() {
        return Err(Blocked::GitMissing);
    }
    if !is_repo_top(checkout) {
        return Err(Blocked::NotARepo);
    }
    match git::status_under(checkout, folder) {
        Ok(lines) if lines.is_empty() => Ok(()),
        Ok(lines) => Err(Blocked::Dirty(lines)),
        Err(e) => Err(Blocked::StatusFailed(e)),
    }
}

/// `git rev-parse --show-toplevel` answers for any folder inside a repository,
/// so the checkout is a repository only when its own folder is that top.
fn is_repo_top(checkout: &Path) -> bool {
    match (git::toplevel(checkout), checkout.canonicalize()) {
        (Ok(top), Ok(own)) => top.canonicalize().is_ok_and(|top| top == own),
        _ => false,
    }
}

/// The uncommitted lines under `folder`; empty when it is clean or git cannot
/// say. Only the `--host` warning reads this, and that path never refuses.
pub fn dirty_lines(checkout: &Path, folder: &str) -> Vec<String> {
    match check_folder(checkout, folder) {
        Err(Blocked::Dirty(lines)) => lines,
        _ => Vec::new(),
    }
}

/// True when a staged `rice.nix` is NOT a song shape v1 body (CONTRACTS.md
/// §5): the body is a plain attrset, its first code character is `{`, and that
/// brace's match ends the file. The old shape opens with a function header,
/// `{ lib, config, ... }:`, whose first brace is followed by `:`; that is what
/// `lib/checks.nix` refuses as "not an attrset". Full-line `#` comments are
/// skipped first, and braces are counted raw, so a brace inside a string or an
/// inline comment can miscount. A miscount only ever refuses (a body that never
/// closes, or closes too early, reads as old), never copies an old file.
pub fn is_old_song_shape(rice: &str) -> bool {
    let code: String = rice
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    let code = code.trim_start();
    if !code.starts_with('{') {
        return true;
    }
    let mut depth = 0usize;
    for (i, c) in code.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return code[i + 1..].trim_start().starts_with(':');
                }
            }
            _ => {}
        }
    }
    true
}

impl Blocked {
    /// The taught refusal `command` returns for this answer about `folder`.
    pub fn into_outcome(self, command: &str, checkout: &Path, folder: &str) -> Outcome {
        let (reason, what, why, fix) = match self {
            Blocked::GitMissing => (
                "git-missing",
                format!("cannot check `{folder}` for uncommitted changes: `git` is not on PATH"),
                "`rice declare` copies over a song folder only when git shows nothing uncommitted in it; git history is the archive".to_string(),
                Fix::Set("`git` on PATH, then declare again".into()),
            ),
            Blocked::NotARepo => (
                "not-a-git-repo",
                format!("`{}` is not a git repository, so `{folder}` cannot be checked", checkout.display()),
                "`rice declare` copies over a song folder only when git shows nothing uncommitted in it; git history is the archive".to_string(),
                Fix::Set("`$AOIDE_FLAKE_ROOT` at the checkout's git repository".into()),
            ),
            Blocked::Dirty(lines) => (
                "dirty-folder",
                format!("`{folder}` has uncommitted changes: {}", lines.join("; ")),
                "copying over them would lose them, and git history is the archive; the copy is refused".to_string(),
                Fix::Run(format!("git -C {} status -- {folder}", checkout.display())),
            ),
            Blocked::StatusFailed(e) => (
                "git-status-failed",
                format!("cannot read git status for `{folder}`"),
                e,
                Fix::None("the checkout's git repository must answer `git status`"),
            ),
        };
        Outcome::refuse(command, Kind::Refused, what, why, fix)
            .with_fields(json!({ "reason": reason, "folder": folder }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_test_support::{git_in, unique_tmp};
    use std::path::PathBuf;

    /// A checkout that is a git repository, with `rices/dusk/` committed.
    fn repo_with_dusk(tag: &str) -> PathBuf {
        let root = unique_tmp(tag);
        git_in(&root, &["init", "-q"]);
        let folder = root.join("rices/dusk");
        std::fs::create_dir_all(folder.join("widgets")).unwrap();
        std::fs::write(folder.join("theme.json"), "{}").unwrap();
        std::fs::write(folder.join("rice.nix"), "{ }").unwrap();
        git_in(&root, &["add", "-A"]);
        git_in(&root, &["commit", "-qm", "dusk"]);
        root
    }

    #[test]
    fn a_committed_folder_with_nothing_changed_passes() {
        let root = repo_with_dusk("gate-clean");
        assert_eq!(check_folder(&root, "rices/dusk"), Ok(()));
        assert!(dirty_lines(&root, "rices/dusk").is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_uncommitted_edit_to_a_tracked_file_is_dirty() {
        let root = repo_with_dusk("gate-tracked");
        std::fs::write(root.join("rices/dusk/theme.json"), "{\"a\":1}").unwrap();
        match check_folder(&root, "rices/dusk") {
            Err(Blocked::Dirty(lines)) => {
                assert_eq!(lines, vec![" M rices/dusk/theme.json".to_string()]);
            }
            other => panic!("want Dirty, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_untracked_file_in_the_folder_is_dirty() {
        let root = repo_with_dusk("gate-untracked");
        std::fs::write(root.join("rices/dusk/widgets/bar.qml"), "").unwrap();
        match check_folder(&root, "rices/dusk") {
            Err(Blocked::Dirty(lines)) => {
                assert_eq!(lines, vec!["?? rices/dusk/widgets/".to_string()]);
            }
            other => panic!("want Dirty, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_dirty_sibling_folder_does_not_block_this_one() {
        let root = repo_with_dusk("gate-sibling");
        let other = root.join("rices/other");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("theme.json"), "{}").unwrap();
        assert_eq!(check_folder(&root, "rices/dusk"), Ok(()));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_checkout_that_is_not_a_repository_is_refused() {
        let root = unique_tmp("gate-not-repo");
        std::fs::create_dir_all(root.join("rices/dusk")).unwrap();
        assert_eq!(check_folder(&root, "rices/dusk"), Err(Blocked::NotARepo));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_checkout_nested_in_another_repository_is_refused() {
        let outer = repo_with_dusk("gate-nested");
        let nested = outer.join("nonrepo");
        std::fs::create_dir_all(nested.join("rices/dusk")).unwrap();
        std::fs::write(nested.join("rices/dusk/theme.json"), "{}").unwrap();
        git_in(&outer, &["add", "-A"]);
        git_in(&outer, &["commit", "-qm", "nested"]);
        assert_eq!(check_folder(&nested, "rices/dusk"), Err(Blocked::NotARepo));
        let _ = std::fs::remove_dir_all(&outer);
    }

    #[test]
    fn a_plain_attrset_body_is_the_v1_shape() {
        let body = "# rices/dusk/rice.nix\n{\n  system = {\n    module = { lib, song, ... }: {\n      config = lib.mkDefault { };\n    };\n  };\n}\n";
        assert!(!is_old_song_shape(body));
    }

    #[test]
    fn a_function_header_is_the_old_shape() {
        let old = "{\n  lib,\n  config,\n  song,\n  borrow,\n  ...\n}:\nlet\n  widgets = borrow \"dusk\";\nin\n{\n  config = lib.mkIf (config.aoide.song == \"dusk\") { };\n}\n";
        assert!(is_old_song_shape(old));
        assert!(is_old_song_shape("{ ... }: { }\n"));
    }

    #[test]
    fn a_file_that_is_not_an_attrset_is_the_old_shape() {
        assert!(is_old_song_shape("let x = 1; in { }\n"));
        assert!(is_old_song_shape(""));
    }

    #[test]
    fn a_refusal_names_the_folder_and_its_changes() {
        let root = repo_with_dusk("gate-outcome");
        std::fs::write(root.join("rices/dusk/theme.json"), "{\"a\":1}").unwrap();
        let out = Blocked::Dirty(vec![" M rices/dusk/theme.json".into()]).into_outcome(
            "rice.declare",
            &root,
            "rices/dusk",
        );
        assert_eq!(out.status, aoide_protocol::output::Status::Error);
        assert!(out.message.contains("rices/dusk"), "{}", out.message);
        let data = out.data.expect("a refusal carries data");
        assert_eq!(data["reason"], "dirty-folder");
        assert_eq!(data["folder"], "rices/dusk");
        let _ = std::fs::remove_dir_all(&root);
    }
}
