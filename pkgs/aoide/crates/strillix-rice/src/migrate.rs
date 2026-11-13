//! The rice runtime's one-shot move: Strillix's trees leave core's root. Each
//! of [`TREES`] moves from `$AOIDE_ROOT` to the same place under
//! `$STRILLIX_ROOT`, merging into a directory the strillix root already holds,
//! and the `song` marker file the rice tree's earlier move left in
//! `$AOIDE_ROOT` is deleted. An old path keeps only what could not move: an
//! entry the strillix root already holds with other bytes, or one the
//! filesystem refused, stays where it is and is named on stderr. Only `strx
//! migrate` runs it, and the home activation calls that verb on every switch.

use crate::paths::strillix_root;
use aoide_storage::fs::root;
use std::path::{Path, PathBuf};

/// Each tree that moves, relative to both roots.
pub const TREES: [&str; 5] = ["run/qml", "run/elements", "state/skwd-wall", "state/captures", "state/pointer-pos.json"];

/// The old paths a run would move or delete, moving nothing (`strx migrate --check`).
pub fn pending() -> Vec<PathBuf> {
    pending_between(&root(), &strillix_root())
}

/// Moves every tree and deletes the marker. Returns the old paths that stay.
pub fn migrate() -> Vec<PathBuf> {
    migrate_between(&root(), &strillix_root())
}

fn pending_between(old: &Path, new: &Path) -> Vec<PathBuf> {
    let mut due: Vec<PathBuf> =
        TREES.iter().filter(|tree| due(&old.join(tree), &new.join(tree))).map(|tree| old.join(tree)).collect();
    if is_marker(&old.join("song")) {
        due.push(old.join("song"));
    }
    due
}

fn migrate_between(old: &Path, new: &Path) -> Vec<PathBuf> {
    let mut stayed = Vec::new();
    for tree in TREES {
        if due(&old.join(tree), &new.join(tree)) {
            move_path(&old.join(tree), &new.join(tree), &mut stayed);
        }
    }
    let _ = std::fs::remove_dir(old.join("run"));
    if is_marker(&old.join("song")) {
        remove(&old.join("song"), &mut stayed);
    }
    stayed
}

/// Whether `from` still has to go: it exists, and it is a link or is not
/// already `to` (one root for both, or a parent linked across).
fn due(from: &Path, to: &Path) -> bool {
    from.symlink_metadata().is_ok_and(|m| m.file_type().is_symlink() || !same_path(from, to))
}

fn is_marker(song: &Path) -> bool {
    song.symlink_metadata().is_ok_and(|m| m.is_file())
}

/// Moves `from` to `to`, merging into a directory `to` already is: an entry
/// `to` lacks moves, one it holds with the same bytes is deleted, and anything
/// else stays and is pushed onto `stayed`. A link moves as a link, and a link
/// that already resolves to `to` is unlinked.
fn move_path(from: &Path, to: &Path, stayed: &mut Vec<PathBuf>) {
    let Ok(meta) = from.symlink_metadata() else { return };
    if same_path(from, to) {
        if meta.file_type().is_symlink() {
            remove(from, stayed);
        }
        return;
    }
    match to.symlink_metadata() {
        Err(_) => {
            let made = to.parent().map_or(Ok(()), std::fs::create_dir_all);
            if let Err(e) = made.and_then(|()| std::fs::rename(from, to)) {
                stay(from, &e.to_string(), stayed);
            }
        }
        Ok(held) if meta.is_dir() && held.is_dir() => match std::fs::read_dir(from) {
            Ok(entries) => {
                let before = stayed.len();
                for entry in entries.flatten() {
                    move_path(&entry.path(), &to.join(entry.file_name()), stayed);
                }
                if stayed.len() == before {
                    if let Err(e) = std::fs::remove_dir(from) {
                        stay(from, &e.to_string(), stayed);
                    }
                }
            }
            Err(e) => stay(from, &e.to_string(), stayed),
        },
        Ok(held) if !meta.is_dir() && !held.is_dir() && same_bytes(from, to) => remove(from, stayed),
        Ok(_) => stay(from, &format!("{} already holds something else", to.display()), stayed),
    }
}

fn remove(path: &Path, stayed: &mut Vec<PathBuf>) {
    if let Err(e) = std::fs::remove_file(path) {
        stay(path, &e.to_string(), stayed);
    }
}

fn stay(path: &Path, why: &str, stayed: &mut Vec<PathBuf>) {
    eprintln!("strillix: {} stays where it is: {why}", path.display());
    stayed.push(path.to_path_buf());
}

fn same_bytes(a: &Path, b: &Path) -> bool {
    let size = |p: &Path| std::fs::metadata(p).map(|m| m.len()).ok();
    size(a).is_some() && size(a) == size(b) && matches!((std::fs::read(a), std::fs::read(b)), (Ok(x), Ok(y)) if x == y)
}

fn same_path(a: &Path, b: &Path) -> bool {
    a == b || matches!((a.canonicalize(), b.canonicalize()), (Ok(x), Ok(y)) if x == y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    /// A fresh pair of roots, core's and Strillix's, under one scratch dir.
    fn roots(tag: &str) -> (PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("strillix-migrate-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        (base.join("core"), base.join("strillix"))
    }

    fn put(path: &Path, bytes: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    /// One file in each tree, as a box before the move holds them, and the marker.
    const FILES: [&str; 5] = [
        "run/qml/shell.qml",
        "run/elements/waybar/config",
        "state/skwd-wall/library/sea.png",
        "state/captures/shot.png",
        "state/pointer-pos.json",
    ];

    fn old_box(old: &Path) {
        for file in FILES {
            put(&old.join(file), file);
        }
        put(&old.join("song"), "");
    }

    #[test]
    fn every_tree_moves_whole_and_a_second_run_finds_nothing() {
        let (old, new) = roots("whole");
        old_box(&old);
        assert_eq!(pending_between(&old, &new).len(), 6, "five trees and the marker are due");
        assert!(migrate_between(&old, &new).is_empty());
        for file in FILES {
            assert_eq!(std::fs::read_to_string(new.join(file)).unwrap(), file, "{file} moved");
            assert!(!old.join(file).exists(), "{file} left nothing behind");
        }
        assert!(!old.join("run").exists(), "the emptied run/ goes");
        assert!(!old.join("song").exists(), "the marker goes");
        assert!(old.join("state").is_dir(), "core's state/ stays");
        assert!(pending_between(&old, &new).is_empty());
        assert!(migrate_between(&old, &new).is_empty());
        let _ = std::fs::remove_dir_all(old.parent().unwrap());
    }

    #[test]
    fn a_tree_merges_into_the_one_the_strillix_root_holds() {
        let (old, new) = roots("merge");
        put(&old.join("state/skwd-wall/library/sea.png"), "sea");
        put(&old.join("state/skwd-wall/library/same.png"), "same");
        put(&old.join("state/skwd-wall/library/other.png"), "old");
        put(&new.join("state/skwd-wall/library/sky.png"), "sky");
        put(&new.join("state/skwd-wall/library/same.png"), "same");
        put(&new.join("state/skwd-wall/library/other.png"), "new");
        let stayed = migrate_between(&old, &new);
        assert_eq!(stayed, vec![old.join("state/skwd-wall/library/other.png")]);
        assert_eq!(std::fs::read_to_string(new.join("state/skwd-wall/library/sea.png")).unwrap(), "sea");
        assert_eq!(std::fs::read_to_string(new.join("state/skwd-wall/library/sky.png")).unwrap(), "sky");
        assert!(!old.join("state/skwd-wall/library/same.png").exists(), "the same bytes count as moved");
        assert_eq!(std::fs::read_to_string(new.join("state/skwd-wall/library/other.png")).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(old.join("state/skwd-wall/library/other.png")).unwrap(), "old");
        let _ = std::fs::remove_dir_all(old.parent().unwrap());
    }

    #[test]
    fn a_song_tree_and_the_rest_of_run_stay() {
        let (old, new) = roots("song");
        put(&old.join("song/stage/mode.json"), "{}");
        put(&old.join("run/other/keep"), "keep");
        put(&old.join("run/qml/shell.qml"), "qml");
        assert!(migrate_between(&old, &new).is_empty());
        assert!(old.join("song/stage/mode.json").is_file(), "a rice tree the earlier move never took stays");
        assert!(old.join("run/other/keep").is_file(), "run/ keeps what is not Strillix's");
        assert!(new.join("run/qml/shell.qml").is_file());
        let _ = std::fs::remove_dir_all(old.parent().unwrap());
    }

    #[test]
    fn a_link_to_the_new_tree_is_unlinked_and_any_other_link_moves_as_a_link() {
        let (old, new) = roots("links");
        put(&new.join("state/captures/shot.png"), "shot");
        std::fs::create_dir_all(old.join("state")).unwrap();
        symlink(new.join("state/captures"), old.join("state/captures")).unwrap();
        let elsewhere = old.parent().unwrap().join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::fs::create_dir_all(old.join("run")).unwrap();
        symlink(&elsewhere, old.join("run/elements")).unwrap();
        assert!(migrate_between(&old, &new).is_empty());
        assert!(old.join("state/captures").symlink_metadata().is_err(), "the link to the new tree is gone");
        assert!(new.join("state/captures/shot.png").is_file(), "and what it pointed at is kept");
        assert_eq!(std::fs::read_link(new.join("run/elements")).unwrap(), elsewhere);
        let _ = std::fs::remove_dir_all(old.parent().unwrap());
    }

    #[test]
    fn one_root_for_both_moves_nothing() {
        let (old, _) = roots("one");
        for file in FILES {
            put(&old.join(file), file);
        }
        assert!(pending_between(&old, &old).is_empty());
        assert!(migrate_between(&old, &old).is_empty());
        for file in FILES {
            assert!(old.join(file).is_file(), "{file} stays in the one root");
        }
        let _ = std::fs::remove_dir_all(old.parent().unwrap());
    }

    #[test]
    fn a_tree_the_filesystem_refuses_stays_and_is_named() {
        let (old, new) = roots("refused");
        put(&old.join("state/captures/shot.png"), "shot");
        std::fs::set_permissions(old.join("state"), std::fs::Permissions::from_mode(0o555)).unwrap();
        let stayed = migrate_between(&old, &new);
        std::fs::set_permissions(old.join("state"), std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(stayed, vec![old.join("state/captures")]);
        assert!(old.join("state/captures/shot.png").is_file());
        let _ = std::fs::remove_dir_all(old.parent().unwrap());
    }
}
