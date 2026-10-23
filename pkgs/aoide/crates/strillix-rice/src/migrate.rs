//! The rice runtime's one-shot moves, and the rice state's status. The old
//! `~/Aoide/song` tree moves into `$STRILLIX_ROOT`, and `~/.local/state/lyra`
//! moves into `~/.local/state/strillix`. Both run only when asked
//! (`STRILLIX_MIGRATE=1` or `strx migrate`). The core binaries run none of
//! this: `aoide_storage::fs::migrate_root_once` moves only the core trees.

use crate::paths::{stage_dir, strillix_root};
use aoide_storage::fs::{atomic_write, migrate_dir, migrate_file, root};

/// The rice runtime's move, which no core binary runs: the old
/// `~/Aoide/song/stage`, the rice tree
/// (`$AOIDE_ROOT/song/*`, see [`migrate_rice_tree`]) into `$STRILLIX_ROOT`,
/// the file renames inside it, the names and paths those files store
/// ([`migrate_stored_names`]), and the lyra link tree ([`migrate_strillix_state`]).
/// Runs only when asked: the strillix binary calls it from [`rice_root_at_start`]
/// when `STRILLIX_MIGRATE=1`, and from `strx migrate`. Idempotent, like every
/// piece here. While an aoide binary resolves ([`aoide_protocol::bin::core_present`])
/// and core's legacy state is still due ([`legacy_state_pending`]), it returns an
/// error, moving nothing: core moves that state when an aoide binary runs. With
/// no aoide binary, nothing would move that state, so the move runs and leaves
/// core's legacy `~/Aoide/state` where it is.
pub fn migrate_rice_root() -> std::io::Result<()> {
    if aoide_protocol::bin::core_present() && legacy_state_pending() {
        return Err(std::io::Error::other(format!(
            "core has not moved its old root yet ({} still holds state): run any aoide command once, then retry; nothing moved",
            aoide_protocol::aoide_home().join("Aoide").join("state").display()
        )));
    }
    let refused = match rice_status() {
        RiceStatus::Both(paths) => Some(("held by both roots", paths)),
        RiceStatus::Refused(paths) => Some(("held across homes", paths)),
        RiceStatus::Settled | RiceStatus::Due(_) => None,
    };
    if let Some((why, paths)) = refused {
        return Err(std::io::Error::other(format!(
            "rice state is {why} ({}); nothing moved",
            paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
        )));
    }
    migrate_strillix_state(&aoide_protocol::aoide_home())?;
    let old_root = aoide_protocol::aoide_home().join("Aoide");
    let new_root = root();
    if old_root != new_root {
        migrate_dir(&old_root.join("song").join("stage"), &new_root.join("song").join("stage"))?;
    }
    migrate_rice_tree(&new_root.join("song"))?;
    let declared = strillix_root().join("declared");
    migrate_dir(&declared.join("songbook"), &declared.join("rices"))?;
    migrate_theme_files(&strillix_root());
    migrate_wallpaper_files(&strillix_root())?;
    migrate_stored_names(&strillix_root(), &new_root.join("song"));
    Ok(())
}

/// Whether core's legacy `~/Aoide/state` is still due to move: it holds state,
/// `$AOIDE_STATE_DIR` is unset, and the root's `state/` is absent or empty. Those
/// are the conditions under which `aoide_storage::fs::migrate_root_once` moves it.
/// A legacy `state/` that core would not move (the root's `state/` already holds
/// anything) is not waited on. [`migrate_rice_root`] waits on this state while an
/// aoide binary resolves. The legacy audit log is not checked: the aoided
/// units export `AOIDE_AUDIT_LOG`, which keeps `migrate_root_once` from moving a
/// legacy log in that process.
fn legacy_state_pending() -> bool {
    let legacy = aoide_protocol::aoide_home().join("Aoide");
    let current = root();
    let current_state = current.join("state");
    let current_state_free =
        !current_state.exists() || std::fs::read_dir(&current_state).is_ok_and(|mut d| d.next().is_none());
    legacy != current
        && std::env::var("AOIDE_STATE_DIR").is_err()
        && holds_state(&legacy.join("state"))
        && current_state_free
}

/// Where the rice state stands, read without moving anything (`strx migrate
/// --check` reports it). The paths name what holds rice state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RiceStatus {
    /// Nothing old is left to move: a fresh box, a finished move, or the marker.
    Settled,
    /// Old rice state that the next migration run moves.
    Due(Vec<std::path::PathBuf>),
    /// Rice state held by the old tree AND the strillix root: nothing moves.
    Both(Vec<std::path::PathBuf>),
    /// Rice state whose two roots resolve under different homes: nothing moves.
    Refused(Vec<std::path::PathBuf>),
}

/// The rice state's status, read from disk and moving nothing. Judges the tree
/// move ([`migrate_rice_tree`]) the way [`migrate_rice_root`] would: a `song`
/// that is a file is the marker. The renames after the move (the declared
/// `songbook/` folder, the theme and wallpaper files, the stored names) never
/// change the status. The old `~/Aoide/song/stage` hops through `song` on its
/// way to the strillix root, so it and `song` are judged together before
/// anything moves: rice state in both roots is [`RiceStatus::Both`], and rice
/// state the roots would carry across homes is [`RiceStatus::Refused`].
pub fn rice_status() -> RiceStatus {
    let home = aoide_protocol::aoide_home();
    let old_root = home.join("Aoide");
    let new_root = root();
    let old_song = new_root.join("song");
    let target = strillix_root();
    let legacy_stage = old_root.join("song").join("stage");
    let mut rice = Vec::new();
    if old_root != new_root && holds_state(&legacy_stage) {
        rice.push(legacy_stage);
    }
    if old_song != target && !old_song.is_file() && old_rice_held(&old_song) {
        rice.push(old_song.clone());
    }
    if !rice.is_empty() {
        if !rice_move_allowed(&old_song, &target) {
            return RiceStatus::Refused(rice);
        }
        if strillix_holds_rice(&target) {
            return RiceStatus::Both(rice);
        }
    }
    let mut held = rice;
    if holds_state(&home.join(".local").join("state").join("lyra")) {
        held.push(home.join(".local").join("state").join("lyra"));
    }
    if held.is_empty() {
        RiceStatus::Settled
    } else {
        RiceStatus::Due(held)
    }
}

/// The rice migration's start-up hook, the strillix binary's only call into
/// it. `STRILLIX_MIGRATE=1` (the home activation sets it) runs the rice move,
/// the lyra link tree with it: Hyprland sources that link until the rebuild
/// rewrites its config, so a move from any other start would break the next
/// login. Without the flag nothing moves and no marker is written: old state
/// gets ONE line naming `strx migrate`, and the binary carries on. `strillix`
/// itself refuses every verb that could write until the state settles.
pub fn rice_root_at_start() -> std::io::Result<()> {
    if std::env::var("STRILLIX_MIGRATE").is_ok_and(|v| v == "1") {
        return migrate_rice_root();
    }
    let names = |paths: &[std::path::PathBuf]| paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ");
    match rice_status() {
        RiceStatus::Settled => {}
        RiceStatus::Due(paths) => eprintln!(
            "strillix: old rice state is still at {}; run `strx migrate` (a rebuild runs it). Nothing moved",
            names(&paths)
        ),
        RiceStatus::Both(paths) => eprintln!(
            "strillix: rice state is in both {} and the strillix root; run `strx migrate --check`. Nothing moved",
            names(&paths)
        ),
        RiceStatus::Refused(paths) => eprintln!(
            "strillix: rice state at {} would cross homes; set both $AOIDE_ROOT and $STRILLIX_ROOT or neither. Nothing moved",
            names(&paths)
        ),
    }
    Ok(())
}

/// `livery.json` becomes `theme.json` in the stage, in the declared twin, and in
/// every rice and draft under `<strillix>/rices`. Runs after the rice tree moved, so
/// it reads the tree in its final place. No-clobber and idempotent, like every
/// other piece here: a file already at its new name is left alone.
fn migrate_theme_files(strillix: &std::path::Path) {
    migrate_file(&stage_dir().join("livery.json"), &stage_dir().join("theme.json"));
    migrate_file(&strillix.join("declared").join("livery.json"), &strillix.join("declared").join("theme.json"));
    for rice in std::fs::read_dir(strillix.join("rices")).into_iter().flatten().flatten() {
        let dir = rice.path();
        migrate_file(&dir.join("livery.json"), &dir.join("theme.json"));
        for draft in std::fs::read_dir(dir.join("drafts")).into_iter().flatten().flatten() {
            migrate_file(&draft.path().join("livery.json"), &draft.path().join("theme.json"));
        }
    }
}

/// The wallpaper names: `stage/cover.json` becomes
/// `stage/wallpaper.json`, `covers/` becomes `wallpapers/`, and every rice's
/// `cover/` becomes `wallpaper/`, with each draft's `cover.json` following its
/// stage file. No-clobber and idempotent like [`migrate_theme_files`].
fn migrate_wallpaper_files(strillix: &std::path::Path) -> std::io::Result<()> {
    migrate_file(&stage_dir().join("cover.json"), &stage_dir().join("wallpaper.json"));
    migrate_dir(&strillix.join("covers"), &strillix.join("wallpapers"))?;
    for rice in std::fs::read_dir(strillix.join("rices")).into_iter().flatten().flatten() {
        let dir = rice.path();
        migrate_dir(&dir.join("cover"), &dir.join("wallpaper"))?;
        for draft in std::fs::read_dir(dir.join("drafts")).into_iter().flatten().flatten() {
            migrate_file(&draft.path().join("cover.json"), &draft.path().join("wallpaper.json"));
        }
    }
    Ok(())
}

/// The names and paths the moved state files store, rewritten to the names the
/// files now carry. Rewritten: the stage's `theme.json` and `wallpaper.json`, the
/// declared `theme.json`, every `declared/rices/*.json`, each rice's `theme.json`,
/// each draft's `theme.json` and `wallpaper.json`, and every `history/*.json` (an
/// entry, see [`migrate_stored_file`]). Nothing else is read: never `rices-aside/`, a rice's
/// own folders, or the other stage files (`mode.json` is
/// [`migrate_mode_song_key`]'s). A missing file or folder is skipped. No-clobber
/// and idempotent, like its siblings.
fn migrate_stored_names(strillix: &std::path::Path, old_song: &std::path::Path) {
    let stored = |file: &std::path::Path, history: bool| {
        migrate_stored_file(file, history, strillix, old_song)
    };
    stored(&strillix.join("stage").join("theme.json"), false);
    stored(&strillix.join("stage").join("wallpaper.json"), false);
    stored(&strillix.join("declared").join("theme.json"), false);
    for file in json_files(&strillix.join("declared").join("rices")) {
        stored(&file, false);
    }
    for rice in children(&strillix.join("rices")) {
        stored(&rice.join("theme.json"), false);
        for file in json_files(&rice.join("history")) {
            stored(&file, true);
        }
        for draft in children(&rice.join("drafts")) {
            stored(&draft.join("theme.json"), false);
            stored(&draft.join("wallpaper.json"), false);
            for file in json_files(&draft.join("history")) {
                stored(&file, true);
            }
        }
    }
}

/// Rewrites one stored file in place. Its top-level `song` key becomes `rice`.
/// A history entry (`history`) also takes `entry`, `theme` and `wallpaper` for
/// `take`, `livery` and `cover`, and the `song` keys of its `theme` and
/// `wallpaper` objects become `rice`. Then every string value, at any depth,
/// runs through [`moved_path`]. A missing or unreadable file is skipped. A file
/// that is not a JSON object is left as it is, narrated. A file whose value did
/// not change is not written, and a failed write is narrated.
fn migrate_stored_file(
    path: &std::path::Path,
    history: bool,
    strillix: &std::path::Path,
    old_song: &std::path::Path,
) {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return;
    };
    let parsed: Option<serde_json::Value> = serde_json::from_str(&raw).ok();
    let Some(mut value) = parsed.filter(serde_json::Value::is_object) else {
        eprintln!(
            "aoide: {} is not a JSON object; its names are left as they are",
            path.display()
        );
        return;
    };
    let mut changed = rename_key(&mut value, "song", "rice");
    if history {
        changed |= rename_key(&mut value, "take", "entry");
        changed |= rename_key(&mut value, "livery", "theme");
        changed |= rename_key(&mut value, "cover", "wallpaper");
        for object in ["theme", "wallpaper"] {
            if let Some(inner) = value.get_mut(object) {
                changed |= rename_key(inner, "song", "rice");
            }
        }
    }
    changed |= rewrite_paths(&mut value, old_song, strillix);
    if !changed {
        return;
    }
    let Ok(text) = serde_json::to_string_pretty(&value) else {
        eprintln!(
            "aoide: could not serialize {}; its names are left as they are",
            path.display()
        );
        return;
    };
    if let Err(e) = atomic_write(path, &text) {
        eprintln!(
            "aoide: could not rewrite the stored names in {} ({e})",
            path.display()
        );
    }
}

/// Runs every string value in `value`, at any depth, through [`moved_path`] and
/// keeps each one that moved. A moved value is kept only when its new home
/// exists: a path whose new home does not exist stays exactly as it was,
/// because the move kept that file where it was, or the file is gone. Keys are
/// names, not paths, and stay as they are. True when any value moved.
fn rewrite_paths(
    value: &mut serde_json::Value,
    old_song: &std::path::Path,
    strillix: &std::path::Path,
) -> bool {
    let mut moved = false;
    match value {
        serde_json::Value::String(text) => {
            if let Some(new) = moved_path(text.as_str(), old_song, strillix)
                .filter(|new| std::path::Path::new(new).exists())
            {
                *text = new;
                moved = true;
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                moved |= rewrite_paths(item, old_song, strillix);
            }
        }
        serde_json::Value::Object(fields) => {
            for field in fields.values_mut() {
                moved |= rewrite_paths(field, old_song, strillix);
            }
        }
        _ => {}
    }
    moved
}

/// The new home of a path the migration moved, or `None` when it did not move.
/// A path under `old_song` takes its first folder's new name: `stage` and
/// `declared` keep theirs, `covers` becomes `wallpapers`, `songbook` becomes
/// `rices`, and `songbook-aside` becomes `rices-aside`. Any other first folder
/// stayed behind, so the path is `None`. A
/// path under `strillix` keeps its root and renames only `covers`. Then the
/// inner renames apply ([`renamed_inside`]), and a path left where it was is
/// `None`.
fn moved_path(
    path: &str,
    old_song: &std::path::Path,
    strillix: &std::path::Path,
) -> Option<String> {
    let path = std::path::Path::new(path);
    let mut segs = match below(path, old_song) {
        Some(mut segs) => {
            let to = match segs.first()?.as_str() {
                "stage" => "stage",
                "declared" => "declared",
                "covers" => "wallpapers",
                "songbook" => "rices",
                "songbook-aside" => "rices-aside",
                _ => return None,
            };
            segs[0] = to.to_string();
            segs
        }
        None => {
            let mut segs = below(path, strillix)?;
            if segs.first().is_some_and(|first| first == "covers") {
                segs[0] = "wallpapers".to_string();
            }
            segs
        }
    };
    renamed_inside(&mut segs);
    let moved = segs
        .iter()
        .fold(strillix.to_path_buf(), |dir, seg| dir.join(seg));
    (moved.as_path() != path).then(|| moved.to_string_lossy().into_owned())
}

/// The inner renames on a path's segments under the strillix root, each keyed by
/// position: a stage or declared file, the declared `songbook` folder, and within
/// a rice (or a draft) its `cover`, `takes` and `livery.json` (or `cover.json`)
/// names. `rices-aside` never matches `rices`, and a rice's own folders match none
/// of these.
fn renamed_inside(segs: &mut [String]) {
    let names: Vec<&str> = segs.iter().map(String::as_str).collect();
    let (at, to) = match names.as_slice() {
        ["stage", "livery.json"] => (1, "theme.json"),
        ["stage", "cover.json"] => (1, "wallpaper.json"),
        ["declared", "livery.json"] => (1, "theme.json"),
        ["declared", "songbook", ..] => (1, "rices"),
        ["rices", _, "cover", ..] => (2, "wallpaper"),
        ["rices", _, "takes", ..] => (2, "history"),
        ["rices", _, "livery.json"] => (2, "theme.json"),
        ["rices", _, "drafts", _, "takes", ..] => (4, "history"),
        ["rices", _, "drafts", _, "livery.json"] => (4, "theme.json"),
        ["rices", _, "drafts", _, "cover.json"] => (4, "wallpaper.json"),
        _ => return,
    };
    segs[at] = to.to_string();
}

/// The segments of `path` below `root`, or `None` when `path` is not under it.
/// Any component below `root` that is not a plain name (a `..`, say) gives
/// `None` too, so a path with `..` is never rewritten.
fn below(path: &std::path::Path, root: &std::path::Path) -> Option<Vec<String>> {
    path.strip_prefix(root)
        .ok()?
        .components()
        .map(|component| match component {
            std::path::Component::Normal(name) => name.to_str().map(str::to_string),
            _ => None,
        })
        .collect()
}

/// The paths directly inside `dir`; none when `dir` is missing.
fn children(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .collect()
}

/// The `.json` files directly inside `dir`.
fn json_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    children(dir)
        .into_iter()
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect()
}

/// The rice tree's move out of aoide's root (`$AOIDE_ROOT/song/` into
/// `$STRILLIX_ROOT`):
/// `song/stage` becomes the stage, `song/declared` keeps its name,
/// `song/covers` becomes `wallpapers`, `song/songbook` becomes `rices`,
/// `song/songbook-aside` becomes `rices-aside`, and every rice's `takes/`
/// becomes `history/` (its own drafts' included). `mode.json`'s `song` and
/// `stagingSong` keys become `rice` and `stagingRice`. `old_song` is the old
/// tree; the target is [`strillix_root`], the rice tree root every reader
/// derives from. A missing tree, or one already in place, moves nothing; only
/// `mode.json`'s key renames ([`migrate_mode_song_key`]) still run.
///
/// Two states stop the move before it starts. A `song` that is a FILE is the
/// marker a finished move leaves behind ([`mark_rice_tree_moved`]): already
/// migrated, nothing to do. Rice state held by BOTH roots is refused: nothing
/// moves, both paths are narrated, and the binaries keep running on the
/// strillix root. A move that leaves `song` empty replaces it with the marker;
/// one that leaves anything behind keeps `song` and narrates what is left.
fn migrate_rice_tree(old_song: &std::path::Path) -> std::io::Result<()> {
    let target = strillix_root();
    if old_song == target || old_song.is_file() || !old_rice_held(old_song) {
        migrate_mode_song_key(&stage_dir());
        return Ok(());
    }
    migrate_dir(&old_song.join("stage"), &stage_dir())?;
    migrate_dir(&old_song.join("declared"), &target.join("declared"))?;
    migrate_dir(&old_song.join("covers"), &target.join("wallpapers"))?;
    migrate_dir(&old_song.join("songbook"), &target.join("rices"))?;
    migrate_dir(&old_song.join("songbook-aside"), &target.join("rices-aside"))?;
    for rice in std::fs::read_dir(target.join("rices")).into_iter().flatten().flatten() {
        let dir = rice.path();
        migrate_dir(&dir.join("takes"), &dir.join("history"))?;
        for draft in std::fs::read_dir(dir.join("drafts")).into_iter().flatten().flatten() {
            migrate_dir(&draft.path().join("takes"), &draft.path().join("history"))?;
        }
    }
    migrate_mode_song_key(&stage_dir());
    mark_rice_tree_moved(old_song, &target)
}

/// The rice folders the old tree carried. Anything else under `song/`
/// is not rice state: a stray file there neither blocks a move nor counts as
/// something one would refuse over.
const OLD_RICE_FOLDERS: [&str; 5] = ["stage", "declared", "covers", "songbook", "songbook-aside"];

/// Whether the old tree at `old_song` holds rice state (one of
/// [`OLD_RICE_FOLDERS`] has a file at any depth).
fn old_rice_held(old_song: &std::path::Path) -> bool {
    OLD_RICE_FOLDERS.iter().any(|folder| holds_state(&old_song.join(folder)))
}

/// Whether `dir` holds state: any file, symlink or dangling link at any
/// depth. Empty directories do not count, so the empty roots activation
/// pre-creates (`$STRILLIX_ROOT/stage`) never trip the refusal.
fn holds_state(dir: &std::path::Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else { return false };
    entries.flatten().any(|entry| {
        let path = entry.path();
        match std::fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_dir() => holds_state(&path),
            Ok(_) => true,
            Err(_) => false,
        }
    })
}

/// Whether the strillix root `target` holds rice state: only its rice
/// folders count, never aoide's own `state/` or `run/` when both roots are
/// one directory, and never the old `song/` tree that sits inside it.
fn strillix_holds_rice(target: &std::path::Path) -> bool {
    ["stage", "declared", "rices", "wallpapers", "rices-aside"]
        .iter()
        .any(|folder| holds_state(&target.join(folder)))
}

/// The top-level names in `dir`, sorted and comma-joined, for a narration.
/// Empty when `dir` is missing or empty.
fn held_by(dir: &std::path::Path) -> String {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names.join(", ")
}

/// Replaces an emptied `old_song` directory with a regular file of the same
/// name that says where the rice state went. An old binary that opens
/// `song/stage` now fails with ENOTDIR instead of recreating a second live
/// tree. A `old_song` that still holds anything is kept and narrated: the
/// marker is only written over an empty directory, never over a leftover.
fn mark_rice_tree_moved(old_song: &std::path::Path, target: &std::path::Path) -> std::io::Result<()> {
    if !held_by(old_song).is_empty() {
        eprintln!(
            "aoide: {} keeps {} that did not move; no marker is written",
            old_song.display(),
            held_by(old_song)
        );
        return Ok(());
    }
    let marker = format!(
        "rice state moved to {}; rebuild or restart the old binaries, which fail on this file instead of recreating a second live tree\n",
        target.display()
    );
    if let Err(e) = std::fs::remove_dir(old_song).and_then(|()| atomic_write(old_song, &marker)) {
        eprintln!("aoide: could not replace {} with the moved-marker ({e})", old_song.display());
    }
    Ok(())
}

/// Whether the rice tree may move from aoide's root into the rice root: both
/// roots were named by absolute env overrides, or both resolve under the one
/// [`aoide_protocol::aoide_home`] (the single home source [`root`] and
/// [`strillix_root`] both default through). A mismatch — an `$AOIDE_ROOT`
/// outside the home that `$STRILLIX_ROOT` defaults into — would carry a rice
/// tree across homes, so it stays put.
fn rice_move_allowed(old_song: &std::path::Path, target: &std::path::Path) -> bool {
    let explicit = |var: &str| std::env::var(var).is_ok_and(|v| std::path::Path::new(&v).is_absolute());
    if explicit("AOIDE_ROOT") && explicit("STRILLIX_ROOT") {
        return true;
    }
    let home = aoide_protocol::aoide_home();
    old_song.starts_with(&home) && target.starts_with(&home)
}

/// Renames the key `from` to `to` in the JSON object `value`, and only when `to`
/// is absent: a key already under its new name is never overwritten. True when a
/// key moved; a value that is not an object moves nothing.
fn rename_key(value: &mut serde_json::Value, from: &str, to: &str) -> bool {
    let Some(obj) = value.as_object_mut() else {
        return false;
    };
    if obj.contains_key(to) {
        return false;
    }
    let Some(moved) = obj.remove(from) else {
        return false;
    };
    obj.insert(to.to_string(), moved);
    true
}

/// Renames a staged `mode.json`'s `song` key to `rice` and its `stagingSong` key
/// to `stagingRice` (the same values, the new names). Each key is judged on its
/// own by [`rename_key`], so a file carrying both names of a pair keeps both. A
/// file with neither key, or one that is not a JSON object, is left as it is; an
/// unparseable file is left alone and narrated, never clobbered. The file is
/// written only when a key moved. A file it cannot rewrite keeps its old
/// keys and is narrated, the migration goes on, and the next explicit run
/// retries it.
fn migrate_mode_song_key(stage: &std::path::Path) {
    let path = stage.join("mode.json");
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return;
    };
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&raw) else {
        eprintln!(
            "aoide: {} is not JSON; its `song` and `stagingSong` keys are left as they are",
            path.display()
        );
        return;
    };
    let renamed_song = rename_key(&mut value, "song", "rice");
    let renamed_staging = rename_key(&mut value, "stagingSong", "stagingRice");
    if !renamed_song && !renamed_staging {
        return;
    }
    let Ok(text) = serde_json::to_string_pretty(&value) else {
        eprintln!(
            "aoide: could not serialize {}; its old keys stay",
            path.display()
        );
        return;
    };
    if let Err(e) = atomic_write(&path, &text) {
        eprintln!(
            "aoide: could not rewrite {} ({e}); its old keys stay",
            path.display()
        );
    }
}

/// `~/.local/state/lyra` becomes `~/.local/state/strillix`. Hyprland sources
/// `rice.conf` from there until the rebuild rewrites its config, so the move
/// runs in that activation ([`migrate_rice_root`] under `STRILLIX_MIGRATE=1`),
/// never from a plain start. Same no-clobber move as [`migrate_dir`]; `home` is
/// the caller's `$HOME`.
pub fn migrate_strillix_state(home: &std::path::Path) -> std::io::Result<()> {
    let state = home.join(".local").join("state");
    migrate_dir(&state.join("lyra"), &state.join("strillix"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_test_support::MigrationEnvGuard;

    fn scratch(name: &str) -> String {
        std::env::temp_dir().join(name).to_string_lossy().to_string()
    }

    /// The two steps in the order an operator runs them: core's root move (any
    /// `aoide` run), then the rice tree. The strillix binary runs just the second step.
    fn migrate_all() -> std::io::Result<()> {
        aoide_storage::fs::migrate_root_once()?;
        migrate_rice_root()
    }

    /// Both roots named, a private home: the tree the rice-tree tests below
    /// move. Returns the base dir; `aoide` and `strillix` sit under it.
    fn rice_roots(tag: &str) -> std::path::PathBuf {
        let base = std::env::temp_dir().join(format!("aoide-rice-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("home")).unwrap();
        std::env::set_var("HOME", base.join("home"));
        std::env::set_var("AOIDE_ROOT", base.join("aoide"));
        std::env::set_var("STRILLIX_ROOT", base.join("strillix"));
        base
    }

    /// Points `AOIDE_CORE_BIN` at `base/bin/aoide`, the tier `bin::core_present`
    /// resolves first: a planted file when `installed`, a missing path otherwise.
    /// The returned saver restores the variable on drop.
    fn core_bin_at(base: &std::path::Path, installed: bool) -> aoide_test_support::EnvSaver {
        let env = aoide_test_support::EnvSaver::capture(&["AOIDE_CORE_BIN"]);
        let bin = base.join("bin").join("aoide");
        if installed {
            std::fs::create_dir_all(base.join("bin")).unwrap();
            std::fs::write(&bin, "").unwrap();
        }
        std::env::set_var("AOIDE_CORE_BIN", &bin);
        env
    }

    /// Core's legacy `state/` under the private HOME `rice_roots` sets, holding
    /// what `migrate_root_once` moves: a file, and a file one folder down.
    fn plant_legacy_state(base: &std::path::Path) -> std::path::PathBuf {
        let state = base.join("home").join("Aoide").join("state");
        std::fs::create_dir_all(state.join("nested")).unwrap();
        std::fs::write(state.join("usage.json"), "{}").unwrap();
        std::fs::write(state.join("nested").join("sessions.json"), "[1,2]").unwrap();
        state
    }

    /// Every file under `dir` with its bytes, paths relative to `dir` and sorted:
    /// a tree that a run touched in any way reads back different.
    fn file_bytes(dir: &std::path::Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
        fn walk(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<(std::path::PathBuf, Vec<u8>)>) {
            for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(root, &path, out);
                } else {
                    out.push((path.strip_prefix(root).unwrap().to_path_buf(), std::fs::read(&path).unwrap()));
                }
            }
        }
        let mut out = Vec::new();
        walk(dir, dir, &mut out);
        out.sort();
        out
    }

    #[test]
    fn migrate_root_once_second_run_is_a_no_op() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let home = std::env::temp_dir().join(format!("aoide-migrate-root-idempotent-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("HOME", &home);

        let old_stage = home.join("Aoide").join("song").join("stage");
        std::fs::create_dir_all(&old_stage).unwrap();
        std::fs::write(old_stage.join("theme.json"), "first-boot").unwrap();

        migrate_all().unwrap();
        let new_stage = home.join(".strillix").join("stage");
        assert_eq!(std::fs::read_to_string(new_stage.join("theme.json")).unwrap(), "first-boot");
        assert!(!old_stage.exists());

        // Second call: nothing left at the old path — must run cleanly and
        // change nothing.
        migrate_all().unwrap();
        assert_eq!(
            std::fs::read_to_string(new_stage.join("theme.json")).unwrap(),
            "first-boot",
            "a second migration pass must be a pure no-op"
        );

        let _ = std::fs::remove_dir_all(&home);
    }

    /// An `$AOIDE_ROOT` outside the home `$STRILLIX_ROOT` defaults into is a
    /// cross-home move: the rice tree stays where it is, and nothing lands
    /// under the default home.
    #[test]
    fn migrate_root_once_refuses_a_rice_tree_whose_roots_name_different_homes() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = std::env::temp_dir().join(format!("aoide-migrate-mismatch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let home = base.join("home");
        let other = base.join("other");
        std::env::set_var("HOME", &home);
        std::env::set_var("AOIDE_ROOT", other.join(".aoide"));

        let old_declared = other.join(".aoide").join("song").join("declared");
        std::fs::create_dir_all(&old_declared).unwrap();
        std::fs::write(old_declared.join("rice.conf"), "sonata").unwrap();

        assert!(
            migrate_all().is_err(),
            "a refused move is an error, so the verb and the binary exit nonzero"
        );
        assert!(matches!(rice_status(), RiceStatus::Refused(_)), "the cross-home tree reads as refused, not due");
        assert_eq!(std::fs::read_to_string(old_declared.join("rice.conf")).unwrap(), "sonata");
        assert!(!home.join(".strillix").join("declared").exists(), "nothing may land under the default home");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Both roots named by absolute env overrides is an explicit choice, so the
    /// tree moves across the two, and a second run changes nothing.
    #[test]
    fn migrate_root_once_moves_the_rice_tree_when_both_roots_are_named() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = std::env::temp_dir().join(format!("aoide-migrate-named-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::env::set_var("HOME", base.join("home"));
        std::env::set_var("AOIDE_ROOT", base.join("aoide"));
        std::env::set_var("STRILLIX_ROOT", base.join("strillix"));

        let old_declared = base.join("aoide").join("song").join("declared");
        std::fs::create_dir_all(&old_declared).unwrap();
        std::fs::write(old_declared.join("rice.conf"), "sonata").unwrap();

        migrate_all().unwrap();
        migrate_all().unwrap();
        assert_eq!(std::fs::read_to_string(base.join("strillix").join("declared").join("rice.conf")).unwrap(), "sonata");
        assert!(!old_declared.exists(), "the old rice tree must have moved");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn migrate_root_once_moves_the_rice_tree_and_rewrites_mode_song_key() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let home = std::env::temp_dir().join(format!("aoide-migrate-rice-tree-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("HOME", &home);

        let old_song = home.join(".aoide").join("song");
        std::fs::create_dir_all(old_song.join("stage")).unwrap();
        std::fs::write(old_song.join("stage").join("mode.json"), r#"{"song":"moonlight"}"#).unwrap();
        std::fs::create_dir_all(old_song.join("declared")).unwrap();
        std::fs::write(old_song.join("declared").join("rice.conf"), "moonlight").unwrap();
        std::fs::create_dir_all(old_song.join("covers")).unwrap();
        std::fs::write(old_song.join("covers").join("cover.png"), "art").unwrap();
        let old_rice = old_song.join("songbook").join("moonlight");
        std::fs::create_dir_all(old_rice.join("takes").join("v1")).unwrap();
        std::fs::write(old_rice.join("takes").join("v1").join("bar.qml"), "old").unwrap();
        std::fs::write(old_rice.join("takes").join("head.json"), "{}").unwrap();
        std::fs::create_dir_all(old_rice.join("drafts").join("d1").join("takes")).unwrap();
        std::fs::write(old_rice.join("drafts").join("d1").join("takes").join("x"), "draft").unwrap();

        migrate_all().unwrap();

        let strillix = home.join(".strillix");
        assert_eq!(
            std::fs::read_to_string(strillix.join("stage").join("mode.json")).unwrap(),
            serde_json::to_string_pretty(&serde_json::json!({"rice": "moonlight"})).unwrap(),
            "the staged mode.json must carry the rice key, not the song key"
        );
        assert_eq!(std::fs::read_to_string(strillix.join("declared").join("rice.conf")).unwrap(), "moonlight");
        assert_eq!(std::fs::read_to_string(strillix.join("wallpapers").join("cover.png")).unwrap(), "art");
        let new_rice = strillix.join("rices").join("moonlight");
        assert_eq!(std::fs::read_to_string(new_rice.join("history").join("v1").join("bar.qml")).unwrap(), "old");
        assert_eq!(std::fs::read_to_string(new_rice.join("history").join("head.json")).unwrap(), "{}");
        assert!(!new_rice.join("takes").exists(), "the rice's takes/ must have moved to history/");
        assert_eq!(
            std::fs::read_to_string(new_rice.join("drafts").join("d1").join("history").join("x")).unwrap(),
            "draft"
        );
        assert!(!old_song.join("songbook").exists(), "the old songbook tree must have moved");
        assert!(!old_song.join("stage").exists(), "the old song/stage tree must have moved");

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_fresh_box_creates_no_song_tree_and_no_marker() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = rice_roots("fresh");
        migrate_all().unwrap();
        migrate_all().unwrap();
        assert!(!base.join("aoide").join("song").exists(), "a fresh box must not grow a song tree or a marker");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn an_old_box_moves_the_rice_tree_aside_and_marks_song_once() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = rice_roots("old-only");
        let song = base.join("aoide").join("song");
        let strillix = base.join("strillix");
        std::fs::create_dir_all(song.join("stage")).unwrap();
        std::fs::write(song.join("stage").join("mode.json"), r#"{"song":"sonata"}"#).unwrap();
        std::fs::create_dir_all(song.join("declared")).unwrap();
        std::fs::write(song.join("declared").join("rice.conf"), "sonata").unwrap();
        std::fs::create_dir_all(song.join("songbook").join("sonata")).unwrap();
        std::fs::write(song.join("songbook").join("sonata").join("rice.nix"), "{}").unwrap();
        std::fs::create_dir_all(song.join("songbook-aside").join("cadenza-20261008")).unwrap();
        std::fs::write(song.join("songbook-aside").join("cadenza-20261008").join("livery.json"), "{}").unwrap();
        // The new root's empty pre-created stage (activation's tmpfiles) must not refuse the move.
        std::fs::create_dir_all(strillix.join("stage")).unwrap();

        migrate_all().unwrap();
        let marker = std::fs::read_to_string(&song).expect("song is now a marker file");
        assert!(marker.contains(&strillix.display().to_string()), "the marker names the strillix root: {marker}");
        assert_eq!(std::fs::read_to_string(strillix.join("stage").join("mode.json")).unwrap(), serde_json::to_string_pretty(&serde_json::json!({"rice": "sonata"})).unwrap());
        assert_eq!(std::fs::read_to_string(strillix.join("declared").join("rice.conf")).unwrap(), "sonata");
        assert!(strillix.join("rices").join("sonata").join("rice.nix").is_file());
        assert!(
            strillix.join("rices-aside").join("cadenza-20261008").join("livery.json").is_file(),
            "the aside moves whole, its old names untouched"
        );

        // An old binary that still mkdirs song/stage fails loudly on the marker.
        assert!(std::fs::create_dir_all(song.join("stage")).is_err(), "an old binary must not recreate a second tree");

        migrate_all().unwrap();
        assert_eq!(std::fs::read_to_string(&song).unwrap(), marker, "a second run leaves the marker alone");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_marker_already_in_place_is_already_migrated() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = rice_roots("marked");
        let song = base.join("aoide").join("song");
        std::fs::create_dir_all(base.join("aoide")).unwrap();
        std::fs::write(&song, "rice state moved elsewhere\n").unwrap();
        let strillix = base.join("strillix");
        std::fs::create_dir_all(strillix.join("rices").join("sonata")).unwrap();
        std::fs::write(strillix.join("rices").join("sonata").join("rice.nix"), "live").unwrap();

        migrate_all().unwrap();
        assert_eq!(std::fs::read_to_string(&song).unwrap(), "rice state moved elsewhere\n");
        assert_eq!(std::fs::read_to_string(strillix.join("rices").join("sonata").join("rice.nix")).unwrap(), "live");
        assert!(!strillix.join("stage").join("mode.json").exists(), "a marked tree moves nothing");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn rice_state_in_both_roots_moves_nothing_and_is_refused_as_an_error() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = rice_roots("both");
        let song = base.join("aoide").join("song");
        let strillix = base.join("strillix");
        std::fs::create_dir_all(song.join("stage")).unwrap();
        std::fs::write(song.join("stage").join("mode.json"), r#"{"song":"sonata"}"#).unwrap();
        std::fs::create_dir_all(song.join("songbook").join("sonata")).unwrap();
        std::fs::write(song.join("songbook").join("sonata").join("rice.nix"), "old").unwrap();
        std::fs::create_dir_all(strillix.join("stage")).unwrap();
        std::fs::write(strillix.join("stage").join("theme.json"), "live").unwrap();
        std::fs::write(strillix.join("stage").join("livery.json"), "live-old-name").unwrap();

        assert!(migrate_all().is_err(), "a both-roots state is refused as an error");
        assert!(song.is_dir(), "the old tree stays a directory, with no marker");
        assert_eq!(std::fs::read_to_string(song.join("stage").join("mode.json")).unwrap(), r#"{"song":"sonata"}"#);
        assert_eq!(std::fs::read_to_string(song.join("songbook").join("sonata").join("rice.nix")).unwrap(), "old");
        assert_eq!(std::fs::read_to_string(strillix.join("stage").join("theme.json")).unwrap(), "live");
        assert_eq!(
            std::fs::read_to_string(strillix.join("stage").join("livery.json")).unwrap(),
            "live-old-name",
            "a refused tree renames nothing inside the live root"
        );
        assert!(!strillix.join("rices").exists(), "no rice moved into the live root");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The second run after a stray file: the known folders moved on the
    /// first run, so the second reads the tree as settled and refuses nothing.
    #[test]
    fn a_stray_file_does_not_refuse_the_second_run() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = rice_roots("stray-twice");
        let song = base.join("aoide").join("song");
        std::fs::create_dir_all(song.join("stage")).unwrap();
        std::fs::write(song.join("stage").join("mode.json"), r#"{"song":"sonata"}"#).unwrap();
        std::fs::write(song.join("mystery.txt"), "not ours").unwrap();

        migrate_all().unwrap();
        assert_eq!(rice_status(), RiceStatus::Settled, "the stray file is not rice state");
        migrate_all().unwrap();
        assert!(song.join("mystery.txt").is_file(), "the stray file stays where it is");
        assert!(base.join("strillix").join("stage").join("mode.json").is_file());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn an_unknown_leftover_keeps_song_and_writes_no_marker() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = rice_roots("leftover");
        let song = base.join("aoide").join("song");
        std::fs::create_dir_all(song.join("stage")).unwrap();
        std::fs::write(song.join("stage").join("mode.json"), r#"{"song":"sonata"}"#).unwrap();
        std::fs::write(song.join("notes.txt"), "not ours").unwrap();

        migrate_all().unwrap();
        assert!(song.is_dir(), "song keeps what did not move, so it stays a directory");
        assert_eq!(std::fs::read_to_string(song.join("notes.txt")).unwrap(), "not ours");
        assert!(base.join("strillix").join("stage").join("mode.json").is_file(), "what could move, did");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Old rice state under `song/` and the strillix root empty: the core
    /// migration moves none of it, and reports it as due.
    #[test]
    fn a_core_run_moves_no_rice_state_and_reports_it_due() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = rice_roots("core-only");
        let song = base.join("aoide").join("song");
        std::fs::create_dir_all(song.join("stage")).unwrap();
        std::fs::write(song.join("stage").join("mode.json"), r#"{"song":"sonata"}"#).unwrap();

        aoide_storage::fs::migrate_root_once().unwrap();
        assert_eq!(std::fs::read_to_string(song.join("stage").join("mode.json")).unwrap(), r#"{"song":"sonata"}"#);
        assert!(!base.join("strillix").join("stage").exists(), "the core run creates no rice root");
        assert!(song.is_dir(), "no marker without the rice move");
        assert!(matches!(rice_status(), RiceStatus::Due(_)), "the old tree is reported due");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The paint link tree the old Hyprland config sources, planted under the
    /// private HOME `rice_roots` sets. Returns its `rice.conf`.
    fn plant_lyra(base: &std::path::Path) -> std::path::PathBuf {
        let lyra = base.join("home").join(".local").join("state").join("lyra");
        std::fs::create_dir_all(&lyra).unwrap();
        std::fs::write(lyra.join("rice.conf"), "source\n").unwrap();
        lyra.join("rice.conf")
    }

    /// Without `STRILLIX_MIGRATE=1` the strillix start-up hook moves nothing
    /// and writes no marker, with or without old state, the lyra link tree
    /// included: Hyprland sources that link until the activation moves it.
    #[test]
    fn the_start_up_hook_moves_nothing_unless_the_flag_is_set() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = rice_roots("hook-unset");
        let song = base.join("aoide").join("song");
        std::fs::create_dir_all(song.join("stage")).unwrap();
        std::fs::write(song.join("stage").join("mode.json"), r#"{"song":"sonata"}"#).unwrap();
        let lyra_link = plant_lyra(&base);

        rice_root_at_start().unwrap();
        rice_root_at_start().unwrap();
        assert!(song.is_dir(), "no marker: the tree stays a directory");
        assert_eq!(std::fs::read_to_string(song.join("stage").join("mode.json")).unwrap(), r#"{"song":"sonata"}"#);
        assert!(!base.join("strillix").join("stage").exists());
        assert!(lyra_link.is_file(), "the lyra link tree stays until the flag runs the move");
        assert!(!base.join("home/.local/state/strillix").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// `STRILLIX_MIGRATE=1` runs the rice move from the start-up hook, once,
    /// the lyra link tree with it; `rice_status` reads it as settled afterwards.
    #[test]
    fn the_start_up_hook_moves_the_rice_tree_when_the_flag_is_set() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = rice_roots("hook-set");
        let song = base.join("aoide").join("song");
        std::fs::create_dir_all(song.join("stage")).unwrap();
        std::fs::write(song.join("stage").join("mode.json"), r#"{"song":"sonata"}"#).unwrap();
        plant_lyra(&base);
        std::env::set_var("STRILLIX_MIGRATE", "1");

        rice_root_at_start().unwrap();
        assert!(base.join("strillix").join("stage").join("mode.json").is_file());
        assert!(base.join("home/.local/state/strillix/rice.conf").is_file(), "the lyra link moved with the rice tree");
        assert!(!base.join("home/.local/state/lyra").exists());
        assert!(song.is_file(), "the finished move leaves the marker");
        assert_eq!(rice_status(), RiceStatus::Settled);
        rice_root_at_start().unwrap();
        let _ = std::fs::remove_dir_all(&base);
    }

    /// With an aoide binary installed, core's legacy `state/` still in `~/Aoide`
    /// holds the rice move back: it returns an error and moves nothing, the lyra
    /// link included. Once core has moved the state (`migrate_root_once`), the
    /// same tree moves.
    #[test]
    fn the_rice_move_waits_while_core_has_not_moved_its_legacy_state() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = rice_roots("legacy-state");
        let _core = core_bin_at(&base, true);
        let legacy_state = base.join("home").join("Aoide").join("state");
        std::fs::create_dir_all(&legacy_state).unwrap();
        std::fs::write(legacy_state.join("usage.json"), "{}").unwrap();
        let song = base.join("aoide").join("song");
        std::fs::create_dir_all(song.join("stage")).unwrap();
        std::fs::write(song.join("stage").join("mode.json"), r#"{"song":"sonata"}"#).unwrap();
        let lyra_link = plant_lyra(&base);

        let err = migrate_rice_root().expect_err("the rice move waits for core");
        assert!(err.to_string().contains("core has not moved its old root yet"), "{err}");
        assert!(song.join("stage").join("mode.json").is_file(), "the old rice tree stays");
        assert!(lyra_link.is_file(), "the lyra link tree stays");
        assert!(legacy_state.join("usage.json").is_file(), "core's legacy state stays for core to move");
        assert!(!base.join("strillix").exists(), "nothing lands in the strillix root");

        aoide_storage::fs::migrate_root_once().unwrap();
        migrate_rice_root().unwrap();
        assert!(base.join("strillix").join("stage").join("mode.json").is_file());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// With no aoide binary installed, nothing would move core's legacy `state/`,
    /// so the rice move does not wait on it: the rice tree and the lyra link
    /// move, and the legacy state stays byte for byte where it is.
    #[test]
    fn the_rice_move_runs_without_aoide_and_leaves_core_legacy_state_in_place() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = rice_roots("no-aoide");
        let _core = core_bin_at(&base, false);
        let legacy_state = plant_legacy_state(&base);
        let before = file_bytes(&legacy_state);
        let song = base.join("aoide").join("song");
        std::fs::create_dir_all(song.join("stage")).unwrap();
        std::fs::write(song.join("stage").join("mode.json"), r#"{"song":"sonata"}"#).unwrap();
        plant_lyra(&base);

        migrate_rice_root().expect("with no aoide the rice move does not wait on core");
        assert!(base.join("strillix").join("stage").join("mode.json").is_file(), "the rice tree moved");
        assert!(base.join("home/.local/state/strillix/rice.conf").is_file(), "the lyra link moved");
        assert_eq!(file_bytes(&legacy_state), before, "core's legacy state is untouched");
        assert!(!base.join("aoide").join("state").exists(), "the rice move creates no core state");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The rice move leaves core's legacy `state/` for an aoide run: core's own
    /// `migrate_root_once`, run later, moves it into the core root with its bytes.
    #[test]
    fn a_later_core_run_moves_the_legacy_state_the_rice_move_left_behind() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = rice_roots("core-after");
        let _core = core_bin_at(&base, false);
        let legacy_state = plant_legacy_state(&base);
        let before = file_bytes(&legacy_state);
        let song = base.join("aoide").join("song");
        std::fs::create_dir_all(song.join("stage")).unwrap();
        std::fs::write(song.join("stage").join("mode.json"), r#"{"song":"sonata"}"#).unwrap();

        migrate_rice_root().unwrap();
        aoide_storage::fs::migrate_root_once().unwrap();
        assert!(!legacy_state.exists(), "core moved its legacy state out of ~/Aoide");
        assert_eq!(file_bytes(&base.join("aoide").join("state")), before, "the core root holds the same bytes");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Rice state in both roots reads as `Both`, and the move refuses it.
    #[test]
    fn rice_status_reads_both_roots_as_refused() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = rice_roots("status-both");
        let song = base.join("aoide").join("song");
        std::fs::create_dir_all(song.join("songbook").join("sonata")).unwrap();
        std::fs::write(song.join("songbook").join("sonata").join("rice.nix"), "old").unwrap();
        std::fs::create_dir_all(base.join("strillix").join("stage")).unwrap();
        std::fs::write(base.join("strillix").join("stage").join("theme.json"), "live").unwrap();

        assert!(matches!(rice_status(), RiceStatus::Both(_)));
        assert!(migrate_rice_root().is_err(), "both roots held: refused as an error");
        assert!(song.is_dir(), "both roots held: the old tree is kept");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The pre-theme `livery.json` arrives as `theme.json` in the stage, in each
    /// rice and in each draft. A second run changes nothing, and a `theme.json`
    /// already in place is never overwritten by an old `livery.json` beside it.
    #[test]
    fn migrate_root_once_renames_each_livery_file_to_theme_once() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let home = std::env::temp_dir().join(format!("aoide-migrate-theme-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("HOME", &home);

        let old_song = home.join(".aoide").join("song");
        std::fs::create_dir_all(old_song.join("stage")).unwrap();
        std::fs::write(old_song.join("stage").join("livery.json"), "stage-palette").unwrap();
        let old_rice = old_song.join("songbook").join("moonlight");
        std::fs::create_dir_all(old_rice.join("drafts").join("d1")).unwrap();
        std::fs::write(old_rice.join("livery.json"), "rice-palette").unwrap();
        std::fs::write(old_rice.join("drafts").join("d1").join("livery.json"), "draft-palette").unwrap();
        let strillix = home.join(".strillix");

        migrate_all().unwrap();
        migrate_all().unwrap();

        let kept = strillix.join("rices").join("sonata");
        std::fs::create_dir_all(&kept).unwrap();
        std::fs::write(kept.join("theme.json"), "newer").unwrap();
        std::fs::write(kept.join("livery.json"), "stale").unwrap();
        migrate_theme_files(&strillix);

        let new_rice = strillix.join("rices").join("moonlight");
        assert_eq!(std::fs::read_to_string(strillix.join("stage").join("theme.json")).unwrap(), "stage-palette");
        assert_eq!(std::fs::read_to_string(new_rice.join("theme.json")).unwrap(), "rice-palette");
        assert_eq!(std::fs::read_to_string(new_rice.join("drafts").join("d1").join("theme.json")).unwrap(), "draft-palette");
        assert!(!new_rice.join("livery.json").exists(), "the old rice file must have moved");
        assert!(!strillix.join("stage").join("livery.json").exists(), "the old stage file must have moved");
        assert_eq!(std::fs::read_to_string(kept.join("theme.json")).unwrap(), "newer", "a theme.json in place is never clobbered");
        assert_eq!(std::fs::read_to_string(kept.join("livery.json")).unwrap(), "stale", "the file beside it is left, not moved over it");

        let _ = std::fs::remove_dir_all(&home);
    }

    /// Step 5's wallpaper names move once. A fresh box has nothing to move; an
    /// old box's cover names become the wallpaper names; a second run is a
    /// no-op; a file already at the new name is never clobbered.
    #[test]
    fn migrate_root_once_moves_the_wallpaper_names_once() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let home = std::env::temp_dir().join(format!("aoide-migrate-wallpaper-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("HOME", &home);
        let strillix = home.join(".strillix");

        migrate_all().unwrap();
        assert!(!strillix.join("wallpapers").exists(), "a fresh box makes no wallpapers tree");

        std::fs::create_dir_all(strillix.join("stage")).unwrap();
        std::fs::write(strillix.join("stage").join("cover.json"), "stage-pick").unwrap();
        std::fs::create_dir_all(strillix.join("covers")).unwrap();
        std::fs::write(strillix.join("covers").join("dusk.png"), "library").unwrap();
        let rice = strillix.join("rices").join("moonlight");
        std::fs::create_dir_all(rice.join("cover")).unwrap();
        std::fs::write(rice.join("cover").join("moonlight-desktop.png"), "desk").unwrap();
        std::fs::create_dir_all(rice.join("drafts").join("d1")).unwrap();
        std::fs::write(rice.join("drafts").join("d1").join("cover.json"), "draft-pick").unwrap();

        migrate_all().unwrap();
        migrate_all().unwrap();

        assert_eq!(std::fs::read_to_string(strillix.join("stage").join("wallpaper.json")).unwrap(), "stage-pick");
        assert!(!strillix.join("stage").join("cover.json").exists(), "the old stage pick must have moved");
        assert_eq!(std::fs::read_to_string(strillix.join("wallpapers").join("dusk.png")).unwrap(), "library");
        assert!(!strillix.join("covers").exists(), "the old library must have moved");
        assert_eq!(std::fs::read_to_string(rice.join("wallpaper").join("moonlight-desktop.png")).unwrap(), "desk");
        assert!(!rice.join("cover").exists(), "the old rice folder must have moved");
        assert_eq!(
            std::fs::read_to_string(rice.join("drafts").join("d1").join("wallpaper.json")).unwrap(),
            "draft-pick"
        );

        std::fs::write(strillix.join("stage").join("cover.json"), "stale").unwrap();
        migrate_all().unwrap();
        assert_eq!(std::fs::read_to_string(strillix.join("stage").join("wallpaper.json")).unwrap(), "stage-pick");
        assert_eq!(std::fs::read_to_string(strillix.join("stage").join("cover.json")).unwrap(), "stale");

        let _ = std::fs::remove_dir_all(&home);
    }

    /// A wallpaper tree whose parent refuses the rename keeps its old name, and
    /// the migration still returns Ok: the library is never lost to a failed move.
    #[cfg(unix)]
    #[test]
    fn migrate_wallpaper_files_keeps_the_old_library_when_its_parent_is_read_only() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let home = std::env::temp_dir().join(format!("aoide-migrate-wallpaper-readonly-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("HOME", &home);
        let strillix = home.join(".strillix");
        std::fs::create_dir_all(strillix.join("covers")).unwrap();
        std::fs::write(strillix.join("covers").join("dusk.png"), "library").unwrap();
        std::fs::set_permissions(&strillix, std::fs::Permissions::from_mode(0o500)).unwrap();

        let result = migrate_wallpaper_files(&strillix);

        std::fs::set_permissions(&strillix, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(result.is_ok());
        assert_eq!(std::fs::read_to_string(strillix.join("covers").join("dusk.png")).unwrap(), "library");
        assert!(!strillix.join("wallpapers").exists());
        let _ = std::fs::remove_dir_all(&home);
    }

    /// The activation seed may create the new root before any binary runs. An
    /// empty target is replaced by the move, so the old tree still lands.
    #[test]
    fn migrate_root_once_moves_into_an_empty_pre_created_root() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let home = std::env::temp_dir().join(format!("aoide-migrate-empty-target-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("HOME", &home);

        let old_stage = home.join(".aoide").join("song").join("stage");
        std::fs::create_dir_all(&old_stage).unwrap();
        std::fs::write(old_stage.join("mode.json"), r#"{"song":"moonlight"}"#).unwrap();
        std::fs::create_dir_all(home.join(".strillix").join("stage")).unwrap();
        std::fs::create_dir_all(home.join(".strillix").join("declared")).unwrap();

        migrate_all().unwrap();

        assert!(home.join(".strillix").join("stage").join("mode.json").exists(), "the staged pointer must reach the new root");
        assert!(!old_stage.exists(), "the old stage must have moved");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A populated new root is never clobbered: the old tree stays put, and
    /// nothing in it is lost.
    #[test]
    fn migrate_root_once_keeps_the_old_tree_when_the_new_root_holds_data() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let home = std::env::temp_dir().join(format!("aoide-migrate-populated-target-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("HOME", &home);

        let old_stage = home.join(".aoide").join("song").join("stage");
        std::fs::create_dir_all(&old_stage).unwrap();
        std::fs::write(old_stage.join("mode.json"), r#"{"song":"moonlight"}"#).unwrap();
        let new_stage = home.join(".strillix").join("stage");
        std::fs::create_dir_all(&new_stage).unwrap();
        std::fs::write(new_stage.join("theme.json"), "fresh").unwrap();

        assert!(migrate_all().is_err(), "rice state in both roots is refused, not moved");

        assert_eq!(std::fs::read_to_string(old_stage.join("mode.json")).unwrap(), r#"{"song":"moonlight"}"#);
        assert_eq!(std::fs::read_to_string(new_stage.join("theme.json")).unwrap(), "fresh");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// The declared twin's `livery.json` arrives as `theme.json`, once.
    #[test]
    fn migrate_root_once_renames_the_declared_livery_twin() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let home = std::env::temp_dir().join(format!("aoide-migrate-declared-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("HOME", &home);

        let declared = home.join(".aoide").join("song").join("declared");
        std::fs::create_dir_all(&declared).unwrap();
        std::fs::write(declared.join("livery.json"), "{}").unwrap();

        migrate_all().unwrap();

        let new_declared = home.join(".strillix").join("declared");
        assert_eq!(std::fs::read_to_string(new_declared.join("theme.json")).unwrap(), "{}");
        assert!(!new_declared.join("livery.json").exists(), "the old declared livery.json must have moved");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A rice file whose directory refuses the rename keeps its old name and
    /// the migration carries on, so the old file is never lost.
    #[cfg(unix)]
    #[test]
    fn migrate_theme_files_keeps_the_old_file_when_its_directory_is_read_only() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let home = std::env::temp_dir().join(format!("aoide-migrate-theme-readonly-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("HOME", &home);
        let rice = home.join(".strillix").join("rices").join("locked");
        std::fs::create_dir_all(&rice).unwrap();
        std::fs::write(rice.join("livery.json"), "palette").unwrap();
        std::fs::set_permissions(&rice, std::fs::Permissions::from_mode(0o500)).unwrap();

        migrate_theme_files(&home.join(".strillix"));

        std::fs::set_permissions(&rice, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(std::fs::read_to_string(rice.join("livery.json")).unwrap(), "palette", "the old file survives a refused rename");
        assert!(!rice.join("theme.json").exists());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn migrate_root_once_never_clobbers_a_newer_rice_at_the_new_path() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let home = std::env::temp_dir().join(format!("aoide-migrate-rice-noclobber-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("HOME", &home);

        let old_rice = home.join(".aoide").join("song").join("songbook").join("sonata");
        std::fs::create_dir_all(&old_rice).unwrap();
        std::fs::write(old_rice.join("marker"), "old").unwrap();
        let new_rice = home.join(".strillix").join("rices").join("sonata");
        std::fs::create_dir_all(&new_rice).unwrap();
        std::fs::write(new_rice.join("marker"), "newer").unwrap();

        assert!(migrate_all().is_err(), "rice state in both roots is refused on every run");
        assert!(migrate_all().is_err());

        assert_eq!(
            std::fs::read_to_string(new_rice.join("marker")).unwrap(),
            "newer",
            "a rice already at the new path must never be overwritten by the old tree"
        );

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn migrate_root_once_never_clobbers_a_newer_new_root_tree() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let home = std::env::temp_dir().join(format!("aoide-migrate-root-no-clobber-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("HOME", &home);

        let old_stage = home.join("Aoide").join("song").join("stage");
        let new_stage = home.join(".strillix").join("stage");
        std::fs::create_dir_all(&old_stage).unwrap();
        std::fs::create_dir_all(&new_stage).unwrap();
        std::fs::write(old_stage.join("theme.json"), "stale").unwrap();
        std::fs::write(new_stage.join("theme.json"), "fresh").unwrap();

        assert!(matches!(rice_status(), RiceStatus::Both(_)), "the legacy stage counts toward both roots");
        assert!(migrate_all().is_err(), "rice state in both roots is refused, not parked");

        assert_eq!(std::fs::read_to_string(new_stage.join("theme.json")).unwrap(), "fresh");
        assert_eq!(std::fs::read_to_string(old_stage.join("theme.json")).unwrap(), "stale");
        assert!(!home.join(".aoide").join("song").exists(), "nothing hops through the aoide root");

        let _ = std::fs::remove_dir_all(&home);
    }

    /// `$AOIDE_STAGE_DIR` is the conducting stage's override, and it never
    /// steers the rice move: the old `song/stage` lands in `$STRILLIX_ROOT/stage`
    /// whatever it names, while the core state moves on its own.
    #[test]
    fn the_conducting_override_never_steers_the_rice_move() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let home = std::env::temp_dir().join(format!("aoide-migrate-root-piecewise-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("HOME", &home);
        std::env::set_var("AOIDE_STAGE_DIR", scratch("aoide-migrate-root-piecewise-elsewhere"));

        let old_stage = home.join("Aoide").join("song").join("stage");
        let old_state = home.join("Aoide").join("state");
        std::fs::create_dir_all(&old_stage).unwrap();
        std::fs::write(old_stage.join("theme.json"), "rice-state").unwrap();
        std::fs::create_dir_all(&old_state).unwrap();
        std::fs::write(old_state.join("usage.json"), "moves").unwrap();

        migrate_all().unwrap();

        assert_eq!(
            std::fs::read_to_string(home.join(".strillix").join("stage").join("theme.json")).unwrap(),
            "rice-state",
            "the rice stage lands in $STRILLIX_ROOT/stage, whatever AOIDE_STAGE_DIR names"
        );
        assert!(!old_stage.exists(), "the old song/stage must have moved");
        assert!(!old_state.exists(), "AOIDE_STATE_DIR is absent, so the state tree migrates");
        assert_eq!(
            std::fs::read_to_string(home.join(".aoide").join("state").join("usage.json")).unwrap(),
            "moves"
        );

        let _ = std::fs::remove_dir_all(&home);
    }
    /// The old `~/Aoide/song/stage` hops through `$AOIDE_ROOT` into the strillix
    /// root. With `$AOIDE_ROOT` left to its default and the strillix root named
    /// into another home, that hop would carry rice state across homes: refused,
    /// and nothing moves.
    #[test]
    fn legacy_rice_state_refuses_a_strillix_root_in_another_home() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = std::env::temp_dir().join(format!("aoide-migrate-legacy-xhome-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let home = base.join("home");
        let other = base.join("other");
        std::env::set_var("HOME", &home);
        std::env::set_var("STRILLIX_ROOT", other.join(".strillix"));

        let legacy_stage = home.join("Aoide").join("song").join("stage");
        std::fs::create_dir_all(&legacy_stage).unwrap();
        std::fs::write(legacy_stage.join("mode.json"), r#"{"song":"sonata"}"#).unwrap();

        assert!(matches!(rice_status(), RiceStatus::Refused(_)), "the legacy stage reads as refused, not due");
        assert!(migrate_all().is_err(), "a refused move is an error");
        assert_eq!(std::fs::read_to_string(legacy_stage.join("mode.json")).unwrap(), r#"{"song":"sonata"}"#);
        assert!(!home.join(".aoide").exists(), "nothing hops through the default aoide root");
        assert!(!other.join(".strillix").exists(), "nothing lands in the other home");

        let _ = std::fs::remove_dir_all(&base);
    }

    /// `migrate_root_once` targets `root()`, the current resolution that honors
    /// an explicit `$AOIDE_ROOT`, never the unconditional `default_root()`. A host
    /// that sets `AOIDE_ROOT` to a custom absolute path still needs its pre-L-C2
    /// data where every getter looks for it; `default_root()` would strand it at
    /// `<home>/.aoide`, silently unreachable. The other migration tests only
    /// exercise the unset-`AOIDE_ROOT` case, which falls back to `default_root()`;
    /// this one pins a genuinely different custom root.
    #[test]
    fn migrate_root_once_targets_a_custom_aoide_root_not_the_default() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let home = std::env::temp_dir().join(format!("aoide-migrate-root-custom-{}", std::process::id()));
        let custom_root =
            std::env::temp_dir().join(format!("aoide-migrate-root-custom-target-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&custom_root);
        std::env::set_var("HOME", &home);
        std::env::set_var("AOIDE_ROOT", &custom_root);
        std::env::set_var("STRILLIX_ROOT", &custom_root);

        let old_stage = home.join("Aoide").join("song").join("stage");
        std::fs::create_dir_all(&old_stage).unwrap();
        std::fs::write(old_stage.join("theme.json"), "custom-root-bound").unwrap();

        migrate_all().unwrap();

        assert_eq!(
            std::fs::read_to_string(custom_root.join("stage").join("theme.json")).unwrap(),
            "custom-root-bound",
            "the pre-L-C2 tree walks through the CUSTOM AOIDE_ROOT and lands in the rice root"
        );
        assert!(
            !home.join(".aoide").exists(),
            "nothing should ever have been written under the unused default root"
        );
        assert!(!old_stage.exists(), "the old tree must have moved");

        std::env::remove_var("AOIDE_ROOT");
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&custom_root);
    }

    /// Writes `value` as the JSON file at `path`, folder included.
    fn write_json(path: &std::path::Path, value: serde_json::Value) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, value.to_string()).unwrap();
    }

    /// Reads the JSON file at `path` back as a value.
    fn read_json(path: &std::path::Path) -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    /// The old tree the stored-names tests move, under `base/aoide/song`: one
    /// file of each kind the pass rewrites, and what it must leave alone (the
    /// aside, a rice's own `rice.nix`).
    fn plant_old_stored_tree(base: &std::path::Path) {
        let song = base.join("aoide").join("song");
        let song_path = song.display().to_string();
        let livery = || {
            serde_json::json!({
                "schemaVersion": "0",
                "song": "barcarolle",
                "palette": {"bg": "#000000", "fg": "#ffffff", "accent": "#111111", "urgent": "#222222"}
            })
        };
        let rice = song.join("songbook").join("barcarolle");
        let draft = rice.join("drafts").join("draft-1");
        let cover_path = format!("{song_path}/songbook/barcarolle/cover/barcarolle-desktop.jpg");
        std::fs::create_dir_all(song.join("covers")).unwrap();
        std::fs::write(song.join("covers").join("bliss.jpg"), "jpeg").unwrap();
        std::fs::create_dir_all(rice.join("cover")).unwrap();
        std::fs::write(rice.join("cover").join("barcarolle-desktop.jpg"), "jpeg").unwrap();
        std::fs::write(rice.join("rice.nix"), "{}").unwrap();
        let pick = serde_json::json!({"kind": "static", "path": format!("{song_path}/covers/bliss.jpg"), "pick": true, "song": "barcarolle"});
        write_json(&song.join("stage").join("cover.json"), pick.clone());
        write_json(&draft.join("cover.json"), pick);
        write_json(&song.join("stage").join("livery.json"), livery());
        write_json(
            &song.join("stage").join("mode.json"),
            serde_json::json!({"mode": "staging", "song": "barcarolle", "stagingSong": "barcarolle", "since": "2026-10-19T22:32:13Z"}),
        );
        write_json(
            &rice.join("takes").join("head.json"),
            serde_json::json!({"head": 7}),
        );
        write_json(
            &rice.join("takes").join("0007.json"),
            serde_json::json!({"take": 7, "parent": 6, "at": "t", "cause": "reload", "livery": livery(),
                "cover": {"kind": "static", "path": cover_path.clone(), "pick": false, "song": "barcarolle"}}),
        );
        write_json(
            &draft.join("takes").join("0001.json"),
            serde_json::json!({"take": 1, "at": "t", "cause": "reload", "livery": livery(),
                "cover": {"kind": "static", "path": cover_path, "pick": false, "song": "barcarolle"}}),
        );
        write_json(
            &draft.join("livery.json"),
            serde_json::json!({"schemaVersion": "0", "song": "barcarolle"}),
        );
        write_json(
            &song.join("declared").join("livery.json"),
            serde_json::json!({"schemaVersion": "0", "song": "barcarolle"}),
        );
        write_json(
            &song.join("declared").join("songbook").join("cadenza.json"),
            serde_json::json!({"schemaVersion": "0", "song": "cadenza", "source": "/nix/store/x-templates", "files": {}}),
        );
        let aside = song.join("songbook-aside").join("cadenza-20261008");
        std::fs::create_dir_all(&aside).unwrap();
        std::fs::write(aside.join("livery.json"), r#"{"song":"cadenza"}"#).unwrap();
    }

    /// An old box's stored names and paths follow the move: the stage pick and
    /// mode marker, a history entry and the paths it holds, a draft, and the
    /// declared records. The aside keeps its names.
    #[test]
    fn an_old_box_s_stored_paths_and_names_follow_the_move() {
        let _guard = aoide_test_support::env_lock()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = rice_roots("stored-names");
        plant_old_stored_tree(&base);
        let s = base.join("strillix");
        let rice = s.join("rices").join("barcarolle");
        let draft = rice.join("drafts").join("draft-1");

        migrate_all().unwrap();

        assert_eq!(
            read_json(&s.join("stage").join("wallpaper.json")),
            serde_json::json!({"kind": "static", "path": format!("{}/wallpapers/bliss.jpg", s.display()), "pick": true, "rice": "barcarolle"})
        );
        assert!(s.join("wallpapers").join("bliss.jpg").is_file());
        let mode = read_json(&s.join("stage").join("mode.json"));
        assert_eq!(mode["rice"], "barcarolle");
        assert_eq!(mode["stagingRice"], "barcarolle");
        assert!(mode.get("song").is_none() && mode.get("stagingSong").is_none());
        assert_eq!(
            crate::mode::load_mode_marker().staging_rice,
            Some("barcarolle".into())
        );
        let theme = read_json(&s.join("stage").join("theme.json"));
        assert_eq!(theme["rice"], "barcarolle");
        assert!(theme.get("song").is_none());

        let wallpaper = format!(
            "{}/rices/barcarolle/wallpaper/barcarolle-desktop.jpg",
            s.display()
        );
        let entry = read_json(&rice.join("history").join("0007.json"));
        assert_eq!(entry["entry"], 7);
        assert_eq!(entry["theme"]["rice"], "barcarolle");
        assert_eq!(entry["wallpaper"]["rice"], "barcarolle");
        assert_eq!(entry["wallpaper"]["path"], wallpaper);
        for old in ["take", "livery", "cover"] {
            assert!(entry.get(old).is_none(), "{old} must have moved");
        }
        let wallpaper_file = rice.join("wallpaper").join("barcarolle-desktop.jpg");
        assert!(wallpaper_file.is_file());
        let loaded = crate::history::load_history("barcarolle", None, 7);
        assert!(loaded.is_some_and(|record| !record.theme.is_null()));
        assert_eq!(
            read_json(&rice.join("history").join("head.json")),
            serde_json::json!({"head": 7})
        );

        assert_eq!(read_json(&draft.join("theme.json"))["rice"], "barcarolle");
        let pick = read_json(&draft.join("wallpaper.json"));
        assert_eq!(pick["rice"], "barcarolle");
        assert_eq!(
            pick["path"],
            format!("{}/wallpapers/bliss.jpg", s.display())
        );
        let draft_entry = read_json(&draft.join("history").join("0001.json"));
        assert_eq!(draft_entry["entry"], 1);
        assert_eq!(draft_entry["theme"]["rice"], "barcarolle");
        assert_eq!(draft_entry["wallpaper"]["rice"], "barcarolle");
        assert_eq!(draft_entry["wallpaper"]["path"], wallpaper);
        for old in ["take", "livery", "cover"] {
            assert!(draft_entry.get(old).is_none(), "{old} must have moved");
        }

        let record = read_json(&s.join("declared").join("rices").join("cadenza.json"));
        assert_eq!(record["rice"], "cadenza");
        assert!(record.get("song").is_none());
        assert!(!s.join("declared").join("songbook").exists());
        assert_eq!(
            read_json(&s.join("declared").join("theme.json"))["rice"],
            "barcarolle"
        );
        let aside = s
            .join("rices-aside")
            .join("cadenza-20261008")
            .join("livery.json");
        assert_eq!(
            std::fs::read_to_string(aside).unwrap(),
            r#"{"song":"cadenza"}"#
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A second pass over a moved tree changes no byte: the pass writes only
    /// what changes.
    #[test]
    fn the_stored_names_pass_is_idempotent() {
        let _guard = aoide_test_support::env_lock()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = rice_roots("stored-idempotent");
        plant_old_stored_tree(&base);
        migrate_all().unwrap();
        assert_eq!(
            read_json(&base.join("strillix").join("stage").join("wallpaper.json"))["rice"],
            "barcarolle",
            "the first run converts the stored names, or the second pass proves nothing"
        );
        let strillix = base.join("strillix");
        let before = file_bytes(&strillix);
        migrate_rice_root().unwrap();
        assert_eq!(
            file_bytes(&strillix),
            before,
            "a second pass must change no byte"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A stored name never overwrites a name already in place: each pair of keys
    /// is judged on its own, so a file carrying both names of a pair keeps both.
    #[test]
    fn a_stored_name_never_clobbers_a_new_one() {
        let _guard = aoide_test_support::env_lock()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = rice_roots("stored-clobber");
        let s = base.join("strillix");
        let stage = s.join("stage");
        std::fs::create_dir_all(s.join("wallpapers")).unwrap();
        std::fs::write(s.join("wallpapers").join("x.jpg"), "jpeg").unwrap();
        let pick = serde_json::json!({"kind": "static", "song": "a", "rice": "b", "path": format!("{}/covers/x.jpg", s.display())});
        let mode = serde_json::json!({"mode": "staging", "song": "x", "stagingSong": "a", "stagingRice": "b"});
        write_json(&stage.join("wallpaper.json"), pick);
        write_json(&stage.join("mode.json"), mode);

        migrate_all().unwrap();

        let expect_pick = serde_json::json!({"kind": "static", "song": "a", "rice": "b", "path": format!("{}/wallpapers/x.jpg", s.display())});
        let expect_mode = serde_json::json!({"mode": "staging", "rice": "x", "stagingSong": "a", "stagingRice": "b"});
        assert_eq!(read_json(&stage.join("wallpaper.json")), expect_pick);
        assert_eq!(read_json(&stage.join("mode.json")), expect_mode);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A stored path whose file did not move keeps its exact text: a path is
    /// rewritten only when its new home exists, and this one has none.
    #[test]
    fn a_stored_path_with_no_moved_file_stays() {
        let _guard = aoide_test_support::env_lock()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = rice_roots("stored-gone");
        let song = base.join("aoide").join("song");
        let song_path = song.display().to_string();
        std::fs::create_dir_all(song.join("stage")).unwrap();
        std::fs::create_dir_all(song.join("covers")).unwrap();
        std::fs::write(song.join("covers").join("kept.jpg"), "jpeg").unwrap();
        write_json(
            &song.join("stage").join("cover.json"),
            serde_json::json!({"kind": "static", "path": format!("{song_path}/covers/gone.jpg"), "pick": true, "song": "barcarolle"}),
        );
        write_json(
            &song.join("stage").join("mode.json"),
            serde_json::json!({"song": "barcarolle"}),
        );

        migrate_all().unwrap();

        assert_eq!(
            read_json(&base.join("strillix").join("stage").join("wallpaper.json")),
            serde_json::json!({"kind": "static", "path": format!("{song_path}/covers/gone.jpg"), "pick": true, "rice": "barcarolle"})
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A `mode.json` the migration cannot rewrite keeps its old keys, and the
    /// call returns: the failed write is narrated, never raised.
    #[cfg(unix)]
    #[test]
    fn a_mode_file_that_cannot_be_rewritten_never_aborts() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = aoide_test_support::env_lock()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let dir = std::env::temp_dir().join(format!("aoide-mode-readonly-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("mode.json"), r#"{"song":"a"}"#).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();

        migrate_mode_song_key(&dir);

        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("mode.json")).unwrap(),
            r#"{"song":"a"}"#
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Each move maps an old path to where its file now sits. A path that did not
    /// move, a rice's own folder, and `rices-aside/` below its top level map to
    /// `None`.
    #[test]
    fn moved_path_follows_every_move() {
        let song = std::path::Path::new("/h/.aoide/song");
        let s = std::path::Path::new("/h/.strillix");
        let moved = |path: &str| moved_path(path, song, s);
        let old = |rel: &str| format!("/h/.aoide/song/{rel}");
        let at_s = |rel: &str| format!("/h/.strillix/{rel}");
        let new = |rel: &str| Some(format!("/h/.strillix/{rel}"));
        assert_eq!(moved(&old("stage/x")), new("stage/x"));
        assert_eq!(moved(&old("declared/x")), new("declared/x"));
        assert_eq!(moved(&old("covers/x")), new("wallpapers/x"));
        assert_eq!(moved(&old("songbook/x")), new("rices/x"));
        assert_eq!(moved(&old("songbook-aside/x")), new("rices-aside/x"));
        assert_eq!(
            moved(&old("songbook-aside/r/cover/x")),
            new("rices-aside/r/cover/x")
        );
        assert_eq!(moved(&at_s("covers/x")), new("wallpapers/x"));
        assert_eq!(moved(&at_s("declared/songbook/x")), new("declared/rices/x"));
        assert_eq!(moved(&at_s("stage/livery.json")), new("stage/theme.json"));
        assert_eq!(
            moved(&at_s("stage/cover.json")),
            new("stage/wallpaper.json")
        );
        assert_eq!(
            moved(&at_s("declared/livery.json")),
            new("declared/theme.json")
        );
        assert_eq!(moved(&at_s("rices/r/cover/x")), new("rices/r/wallpaper/x"));
        assert_eq!(moved(&at_s("rices/r/takes/x")), new("rices/r/history/x"));
        assert_eq!(
            moved(&at_s("rices/r/livery.json")),
            new("rices/r/theme.json")
        );
        assert_eq!(
            moved(&at_s("rices/r/drafts/d/takes/x")),
            new("rices/r/drafts/d/history/x")
        );
        assert_eq!(
            moved(&at_s("rices/r/drafts/d/livery.json")),
            new("rices/r/drafts/d/theme.json")
        );
        assert_eq!(
            moved(&at_s("rices/r/drafts/d/cover.json")),
            new("rices/r/drafts/d/wallpaper.json")
        );
        assert_eq!(moved(&old("elsewhere/x")), None);
        assert_eq!(moved("/nix/store/x"), None);
        assert_eq!(moved(&at_s("rices-aside/r/cover/x")), None);
        assert_eq!(moved(&at_s("wallpapers/x")), None);
        assert_eq!(moved(&at_s("rices/r/widgets/cover/x")), None);
        assert_eq!(moved(&old("../song/covers/x")), None);
        assert_eq!(moved(&old("songbook/r/../../../x")), None);
        assert_eq!(moved("/h/.aoide/songbook/x"), None);
        assert_eq!(moved("/h/.aoide/song"), None);
        assert_eq!(moved("/h/.strillix"), None);
    }
}

/// The paint state link's one-shot move, on a private `$HOME` only: the real
/// `~/.local/state` is never named here. Every temp `$HOME` is judged by
/// `ensure_write_dir` against the aoide state dir, which these paths never sit
/// under, so no test in this module locks anything it does not own.
#[cfg(all(test, unix))]
mod strillix_state_tests {
    use super::migrate_strillix_state;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::path::{Path, PathBuf};

    fn private_home(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("strillix-state-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The old paint state: a `rice.conf` link into a declared file, as the
    /// Hyprland `source` line expects it.
    fn old_state(home: &Path) -> PathBuf {
        let target = home.join("declared-rice.conf");
        let lyra = home.join(".local/state/lyra");
        std::fs::create_dir_all(&lyra).unwrap();
        std::fs::write(&target, "source\n").unwrap();
        symlink(&target, lyra.join("rice.conf")).unwrap();
        target
    }

    #[test]
    fn a_fresh_box_creates_no_state_tree() {
        let home = private_home("fresh");
        migrate_strillix_state(&home).unwrap();
        assert!(!home.join(".local/state/lyra").exists());
        assert!(!home.join(".local/state/strillix").exists());
    }

    #[test]
    fn an_old_box_moves_its_link_once_and_a_second_run_is_a_no_op() {
        let home = private_home("moves");
        let target = old_state(&home);
        migrate_strillix_state(&home).unwrap();
        let new = home.join(".local/state/strillix/rice.conf");
        assert!(!home.join(".local/state/lyra").exists());
        assert_eq!(std::fs::read_link(&new).unwrap(), target);
        migrate_strillix_state(&home).unwrap();
        assert_eq!(std::fs::read_link(&new).unwrap(), target);
    }

    #[test]
    fn a_tree_already_at_the_new_name_is_never_clobbered() {
        let home = private_home("noclobber");
        old_state(&home);
        let new = home.join(".local/state/strillix");
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(new.join("keep"), "new\n").unwrap();
        migrate_strillix_state(&home).unwrap();
        assert_eq!(std::fs::read_to_string(new.join("keep")).unwrap(), "new\n");
        assert!(home.join(".local/state/lyra/rice.conf").exists());
    }

    #[test]
    fn a_refused_move_keeps_the_old_tree_and_does_not_fail_the_boot() {
        let home = private_home("refused");
        old_state(&home);
        let state = home.join(".local/state");
        std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o500)).unwrap();
        let probe = state.join("probe");
        let writable = std::fs::File::create(&probe).is_ok();
        let _ = std::fs::remove_file(&probe);
        let result = migrate_strillix_state(&home);
        std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o700)).unwrap();
        if writable {
            // Root writes through a 0500 directory, so the refusal cannot be staged.
            return;
        }
        result.unwrap();
        assert!(home.join(".local/state/lyra/rice.conf").exists());
        assert!(!home.join(".local/state/strillix").exists());
    }
}
