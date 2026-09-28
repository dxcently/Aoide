//! Cover-art derivation + resolution for the shared library `song/covers/`,
//! and the stage seam that decides what `stage/cover.json` holds: a user pick
//! or the song's own default (CONTRACTS.md §4). Moved out of
//! `pkgs/aoide/src/commands/{rice,cover}.rs` (Phase 5b restructure,
//! docs/architecture/PACKAGE-LAYOUT.md). Reads env-derived paths and stats
//! candidates; the stage writers are the only side effects.

use std::path::{Path, PathBuf};

/// Extensions we recognise as cover art, in preference order.
pub const COVER_EXTS: &[&str] = &["webp", "png", "jpg", "jpeg"];

/// Marks `stage/cover.json` as the USER's pick. Absent — in every file written
/// before the field existed, and in every `rice stage` write — means the song's
/// own default.
pub const PICK_FIELD: &str = "pick";

/// The song a cover was staged for. Every writer stamps it; a file without it
/// is legacy and applies to whichever song is staged.
pub const SONG_FIELD: &str = "song";

/// Derive a physical cover-art file for a song, or `None` when none exists.
///
/// v0 notes carry no runtime cover field (the schema is palette-closed; the
/// build-time `aoide.livery.wallpaper` is a nix path, not a song/ runtime read),
/// so a cover is only ever staged when one is physically present. Covers live
/// in the shared library `song/covers/` — one dir any song (or other consumer)
/// draws from — so the derivable name is `<name>.<ext>` there (a bare
/// `cover.<ext>` would be ambiguous in a shared dir).
pub fn derive_cover(name: &str) -> Option<PathBuf> {
    let covers = aoide_storage::fs::song_dir().join("covers");
    for ext in COVER_EXTS {
        let p = covers.join(format!("{name}.{ext}"));
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// Resolve a `cover set <path|name>` argument to an absolute cover path.
///
/// An absolute `arg` is taken literally; anything else resolves against the
/// shared cover library `song/covers/`. Does NOT check existence — the caller
/// (`handle_cover_set`) does that so it can render its own "no such file"
/// error with the resolved path in it.
pub fn resolve_cover_arg(arg: &str) -> PathBuf {
    let literal = PathBuf::from(arg);
    if literal.is_absolute() {
        literal
    } else {
        aoide_storage::fs::song_dir().join("covers").join(arg)
    }
}

// ── The stage seam: the song's own cover, or the user's pick for it ──────────

/// `<stage>/cover.json`.
pub fn staged_cover_json() -> PathBuf {
    aoide_storage::fs::stage_dir().join("cover.json")
}

fn read_staged() -> Option<serde_json::Value> {
    let raw = std::fs::read_to_string(staged_cover_json()).ok()?;
    serde_json::from_str(&raw).ok()
}

/// The cover path `stage/cover.json` names, or `None` when the file is absent,
/// unreadable, or carries no non-empty `path`.
pub fn staged_path() -> Option<String> {
    let path = read_staged()?.get("path")?.as_str()?.to_string();
    (!path.is_empty()).then_some(path)
}

/// The song the staged cover was written for. `None` is a legacy file, written
/// before writers stamped it — it applies to whichever song is staged.
pub fn staged_song() -> Option<String> {
    read_staged()?
        .get(SONG_FIELD)?
        .as_str()
        .map(str::to_string)
}

/// Is the staged cover the user's pick? Requires the marker AND a path to
/// render — the same pair the shell's own gate requires.
pub fn staged_is_pick() -> bool {
    let Some(doc) = read_staged() else {
        return false;
    };
    doc.get(PICK_FIELD).and_then(|v| v.as_bool()).unwrap_or(false)
        && doc
            .get("path")
            .and_then(|v| v.as_str())
            .is_some_and(|path| !path.is_empty())
}

/// Stage a pick — what `cover set` writes. `song` is the song staged at write
/// time; `None` (nothing staged) writes the legacy shape, which applies to
/// whichever song loads.
pub fn stage_pick(path: &Path, song: Option<&str>) -> std::io::Result<PathBuf> {
    let path = path.to_string_lossy();
    write_cover(match song {
        Some(song) => serde_json::json!({ "path": path, PICK_FIELD: true, SONG_FIELD: song }),
        None => serde_json::json!({ "path": path, PICK_FIELD: true }),
    })
}

/// Stage `song`'s own cover as a default — what `rice stage` writes.
pub fn stage_default(path: &Path, song: &str) -> std::io::Result<PathBuf> {
    write_cover(serde_json::json!({ "path": path.to_string_lossy(), SONG_FIELD: song }))
}

/// Remove `stage/cover.json`.
pub fn clear_staged() -> std::io::Result<CoverWrite> {
    match std::fs::remove_file(staged_cover_json()) {
        Ok(()) => Ok(CoverWrite::Removed),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(CoverWrite::Absent),
        Err(e) => Err(e),
    }
}

/// What a default write did to `stage/cover.json`.
pub enum CoverWrite {
    /// The song's own cover, written as a default.
    Written(PathBuf),
    /// A `cover.json` that no longer belonged to this song was removed.
    Removed,
    /// Nothing derivable, and nothing there to remove.
    Absent,
}

impl CoverWrite {
    /// The word the command envelopes report as `coverStage`.
    pub fn as_str(&self) -> &'static str {
        match self {
            CoverWrite::Written(_) => "default",
            CoverWrite::Removed => "cleared",
            CoverWrite::Absent => "none",
        }
    }
}

/// [`stage_for_song`]'s answer.
pub enum CoverStage {
    /// A pick for THIS song was standing: left byte-identical.
    PickKept,
    /// The song's own default was (re)established.
    Wrote(CoverWrite),
}

/// `song`'s own default: its derivable cover, else no `cover.json` at all.
/// Keeps no pick — a caller that wants one preserved asks [`stage_for_song`].
pub fn stage_song_default(song: &str) -> std::io::Result<CoverWrite> {
    match derive_cover(song) {
        Some(path) => {
            stage_default(&path, song)?;
            Ok(CoverWrite::Written(path))
        }
        None => clear_staged(),
    }
}

/// `rice stage <song>`'s cover half (CONTRACTS.md §4). A cover carries its
/// song: a pick the user made for THIS song is what shows, so re-staging that
/// song — a bare `rice stage`, a `rice mode` re-pin, a rebuild — leaves it
/// alone. Any other cover standing (another song's pick or default, nothing at
/// all) is replaced by this song's own default.
pub fn stage_for_song(song: &str) -> std::io::Result<CoverStage> {
    if staged_is_pick() && staged_song().as_deref() == Some(song) {
        return Ok(CoverStage::PickKept);
    }
    stage_song_default(song).map(CoverStage::Wrote)
}

fn write_cover(value: serde_json::Value) -> std::io::Result<PathBuf> {
    let dst = staged_cover_json();
    let body = serde_json::to_string_pretty(&value).unwrap_or_default() + "\n";
    aoide_storage::fs::atomic_write(&dst, &body)?;
    Ok(dst)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_test_support::*;

    #[test]
    fn resolve_cover_arg_takes_an_absolute_path_literally() {
        let resolved = resolve_cover_arg("/tmp/somewhere/cover.png");
        assert_eq!(resolved, PathBuf::from("/tmp/somewhere/cover.png"));
    }

    #[test]
    fn resolve_cover_arg_resolves_a_bare_name_against_the_covers_library() {
        // Shares `aoide_test_support::env_lock()` with every other env-touching
        // test in the crate (rice.rs, mode.rs, draft.rs) — this used to lock a
        // separate crate-local mutex, so its `set_var("AOIDE_STAGE_DIR", …)`
        // could race a rice.rs/mode.rs test holding the OTHER lock and clobber
        // its env mid-flight.
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        std::env::set_var("AOIDE_STAGE_DIR", "/tmp/aoide-cover-test/stage");

        let resolved = resolve_cover_arg("sonata.webp");
        assert_eq!(
            resolved,
            PathBuf::from("/tmp/aoide-cover-test/covers/sonata.webp")
        );
    }

    #[test]
    fn derive_cover_finds_the_first_matching_extension_or_none() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        let root =
            std::env::temp_dir().join(format!("aoide-song-derive-cover-{}", std::process::id()));
        let stage = root.join("stage");
        let covers = root.join("covers");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::create_dir_all(&covers).unwrap();
        std::fs::write(covers.join("dusk.png"), b"\x89PNG stub").unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        assert_eq!(derive_cover("dusk"), Some(covers.join("dusk.png")));
        assert_eq!(derive_cover("nope"), None);

        let _ = std::fs::remove_dir_all(&root);
    }

    // ── The stage seam: a pick vs a song default (CONTRACTS.md §4) ───────────

    /// A pick is marked; the song's own default is NOT. This is the whole
    /// discriminator AoideWallpaper.qml's board gate rides.
    #[test]
    fn a_pick_is_marked_and_a_default_is_not() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        let root = unique_tmp("cover-pick-field");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        stage_pick(Path::new("/tmp/user-choice.png"), Some("dusk")).unwrap();
        assert!(staged_is_pick());
        assert_eq!(staged_path().as_deref(), Some("/tmp/user-choice.png"));
        assert_eq!(staged_song().as_deref(), Some("dusk"));

        stage_default(Path::new("/tmp/song-default.png"), "dusk").unwrap();
        assert!(!staged_is_pick(), "a song default carries no pick field");
        assert_eq!(staged_path().as_deref(), Some("/tmp/song-default.png"));
        assert_eq!(staged_song().as_deref(), Some("dusk"));

        // A pick with no song to attribute it to (nothing was staged) keeps
        // the whole path requirement: the marker needs a path to mean anything.
        stage_pick(Path::new("/tmp/user-choice.png"), None).unwrap();
        assert!(staged_is_pick());
        assert_eq!(staged_song(), None);
        std::fs::write(
            staged_cover_json(),
            "{\"path\":\"\",\"pick\":true,\"song\":\"dusk\"}\n",
        )
        .unwrap();
        assert!(!staged_is_pick(), "the marker alone is not a pick");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Back-compat: a file with no `song` field is legacy and applies to
    /// whichever song is staged — but a legacy PICK is not kept by a re-stage,
    /// because keeping one requires knowing which song it was made for.
    #[test]
    fn a_legacy_cover_json_without_the_song_field_is_never_kept() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        let root = unique_tmp("cover-v0-field");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        std::fs::write(staged_cover_json(), "{\"path\":\"/tmp/old.png\"}\n").unwrap();
        assert!(!staged_is_pick());
        assert_eq!(staged_path().as_deref(), Some("/tmp/old.png"));
        assert_eq!(staged_song(), None);

        // …and an explicit `false` (or garbage) is not a pick either.
        std::fs::write(staged_cover_json(), "{\"path\":\"/tmp/old.png\",\"pick\":false}").unwrap();
        assert!(!staged_is_pick());
        std::fs::write(staged_cover_json(), "{not json").unwrap();
        assert!(!staged_is_pick(), "a torn file is never a pick");
        assert_eq!(staged_song(), None);

        std::fs::write(
            staged_cover_json(),
            "{\"path\":\"/tmp/old.png\",\"pick\":true}\n",
        )
        .unwrap();
        assert!(staged_is_pick());
        assert!(matches!(
            stage_for_song("dusk").unwrap(),
            CoverStage::Wrote(CoverWrite::Removed)
        ));

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The ruling, in one test: re-staging the pick's OWN song keeps it; any
    /// other song replaces it with that song's own default.
    #[test]
    fn restaging_the_same_song_keeps_a_pick_and_a_song_switch_drops_it() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        let root = unique_tmp("cover-song-switch");
        let stage = root.join("stage");
        let covers = root.join("covers");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::create_dir_all(&covers).unwrap();
        std::fs::write(covers.join("dusk.png"), b"\x89PNG stub").unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        stage_pick(Path::new("/tmp/user-choice.png"), Some("cadenza")).unwrap();
        assert!(matches!(stage_for_song("cadenza").unwrap(), CoverStage::PickKept));
        assert!(staged_is_pick(), "the pick's own song is staged");
        assert_eq!(staged_path().as_deref(), Some("/tmp/user-choice.png"));

        // A different song takes its own cover, as a default.
        match stage_for_song("dusk").unwrap() {
            CoverStage::Wrote(CoverWrite::Written(path)) => {
                assert!(path.ends_with("covers/dusk.png"), "{path:?}");
            }
            _ => panic!("a switch stages the new song's own default"),
        }
        assert!(!staged_is_pick(), "the new song's default is not a pick");
        assert_eq!(staged_song().as_deref(), Some("dusk"));
        assert!(staged_path().unwrap().ends_with("covers/dusk.png"));

        // A song with nothing derivable leaves no cover at all, so the
        // previous song's wallpaper cannot stand over it.
        stage_pick(Path::new("/tmp/user-choice.png"), Some("cadenza")).unwrap();
        assert!(matches!(
            stage_for_song("nocturne").unwrap(),
            CoverStage::Wrote(CoverWrite::Removed)
        ));
        assert!(!staged_cover_json().exists());
        assert_eq!(staged_path(), None);

        // Nothing staged and nothing derivable: idempotent, not an error.
        assert!(matches!(
            stage_for_song("nocturne").unwrap(),
            CoverStage::Wrote(CoverWrite::Absent)
        ));

        let _ = std::fs::remove_dir_all(&root);
    }
}
