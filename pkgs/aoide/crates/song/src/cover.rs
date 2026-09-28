//! Cover-art derivation + resolution — the shared logic `cover set` and
//! `rice stage` both need to turn a name/arg into a physical cover file
//! under the shared library `song/covers/` — plus the one seam that decides
//! WHAT `stage/cover.json` holds: a song's own default or the user's pick
//! (CONTRACTS.md §4).
//!
//! Moved out of `pkgs/aoide/src/commands/rice.rs` and
//! `pkgs/aoide/src/commands/cover.rs` (Phase 5b restructure,
//! docs/architecture/PACKAGE-LAYOUT.md). The resolution functions are pure
//! aside from reading `aoide_storage::fs::song_dir()` (env-derived path
//! resolution) and stat-ing candidate files; the stage writers below are the
//! only side effects here, and they all go through
//! `aoide_storage::fs::atomic_write` — no `Outcome`/`Invocation` ever
//! reaches this module.

use std::path::{Path, PathBuf};

/// Extensions we recognise as cover art, in preference order.
pub const COVER_EXTS: &[&str] = &["webp", "png", "jpg", "jpeg"];

/// The `stage/cover.json` field that marks a USER PICK (CONTRACTS.md §4).
/// Absent — as in every file written before the field existed, and in every
/// file `rice stage` writes — means "the song's own default".
pub const PICK_FIELD: &str = "pick";

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

// ── The stage seam: a song default, a user pick, or neither ──────────────────

/// `<stage>/cover.json` — the one wallpaper seam every writer and reader of
/// this module shares (CONTRACTS.md §4).
pub fn cover_dst() -> PathBuf {
    aoide_storage::fs::stage_dir().join("cover.json")
}

/// The cover path `stage/cover.json` currently names, or `None` when the file
/// is absent, unreadable, or carries no non-empty `path`.
pub fn staged_path() -> Option<String> {
    let raw = std::fs::read_to_string(cover_dst()).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let path = parsed.get("path")?.as_str()?;
    (!path.is_empty()).then(|| path.to_string())
}

/// Does the staged `cover.json` hold a USER PICK — [`PICK_FIELD`] `true`?
///
/// A missing file, a file that will not parse, an absent field and an explicit
/// `false` all answer `false`: the field is additive over v0, so every
/// `cover.json` written before it existed reads as the song's own default,
/// which is exactly what it was.
pub fn staged_is_pick() -> bool {
    let raw = match std::fs::read_to_string(cover_dst()) {
        Ok(raw) => raw,
        Err(_) => return false,
    };
    match serde_json::from_str::<serde_json::Value>(&raw) {
        Ok(parsed) => parsed
            .get(PICK_FIELD)
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        Err(_) => false,
    }
}

/// Stage a USER PICK: `{ "path": …, "pick": true }` — what `cover set` writes.
pub fn stage_pick(path: &Path) -> std::io::Result<PathBuf> {
    write_cover(serde_json::json!({ "path": path.to_string_lossy(), PICK_FIELD: true }))
}

/// Stage a song DEFAULT: `{ "path": … }` with NO `pick` field — what
/// `rice stage` writes, so a reader can never mistake the song's own cover for
/// a pick the user made.
pub fn stage_default(path: &Path) -> std::io::Result<PathBuf> {
    write_cover(serde_json::json!({ "path": path.to_string_lossy() }))
}

/// Remove `stage/cover.json` — the inverse of both writers above, and the
/// "this song has no cover of its own" answer. `Ok(false)` when there was
/// nothing there (a removal is idempotent, not an error).
pub fn clear_staged() -> std::io::Result<bool> {
    match std::fs::remove_file(cover_dst()) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

/// What [`stage_for_song`] did to `stage/cover.json`.
pub enum CoverStage {
    /// The SAME song, with a pick standing: left byte-identical.
    PickKept,
    /// The song's own derivable cover, written as a DEFAULT (no `pick`).
    Default(PathBuf),
    /// No derivable cover: a stale `cover.json` was removed (`true`), or was
    /// not there to begin with.
    Cleared(bool),
}

/// The whole cover half of `rice stage <name>` (CONTRACTS.md §4).
///
/// A song's wallpaper — its derivable cover, and its own live `wallpaper`
/// board, if it authors one — is only its DEFAULT. A user pick (`cover set`)
/// is what shows while it stands, and it SURVIVES staging the same song again
/// (a bare `rice mode stage`, the RICE toggle, a declarative re-seed), because
/// re-staging is not a statement about the wallpaper. Staging a DIFFERENT song
/// resets to that song's own default: the pick is dropped and, with it, any
/// cover the previous song left — which is what used to leak a previous song's
/// wallpaper onto the new one when the new song derived no cover of its own.
///
/// `prev_song` is the song `stage/livery.json` carried BEFORE this call's
/// write — `commands/mode.rs`'s `current_staged_song`, the one source of truth
/// for "the current rice". Anything other than `Some(name)` (a different song,
/// or no stage file at all) is a song switch.
pub fn stage_for_song(name: &str, prev_song: Option<&str>) -> std::io::Result<CoverStage> {
    if prev_song == Some(name) && staged_is_pick() {
        return Ok(CoverStage::PickKept);
    }
    match derive_cover(name) {
        Some(path) => {
            stage_default(&path)?;
            Ok(CoverStage::Default(path))
        }
        None => clear_staged().map(CoverStage::Cleared),
    }
}

fn write_cover(value: serde_json::Value) -> std::io::Result<PathBuf> {
    let dst = cover_dst();
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

        stage_pick(Path::new("/tmp/user-choice.png")).unwrap();
        assert!(staged_is_pick(), "cover set stages a PICK");
        assert_eq!(staged_path().as_deref(), Some("/tmp/user-choice.png"));

        stage_default(Path::new("/tmp/song-default.png")).unwrap();
        assert!(!staged_is_pick(), "a song default carries no pick field");
        assert_eq!(staged_path().as_deref(), Some("/tmp/song-default.png"));

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Back-compat: a v0 file (`{"path": …}`, no field) reads as the song's
    /// own default — which is what it was when it was written.
    #[test]
    fn a_v0_cover_json_without_the_field_is_not_a_pick() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        let root = unique_tmp("cover-v0-field");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        std::fs::write(cover_dst(), "{\"path\":\"/tmp/old.png\"}\n").unwrap();
        assert!(!staged_is_pick());
        assert_eq!(staged_path().as_deref(), Some("/tmp/old.png"));

        // …and an explicit `false` (or garbage) is not a pick either.
        std::fs::write(cover_dst(), "{\"path\":\"/tmp/old.png\",\"pick\":false}").unwrap();
        assert!(!staged_is_pick());
        std::fs::write(cover_dst(), "{not json").unwrap();
        assert!(!staged_is_pick(), "a torn file is never a pick");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The ruling, in one test: re-staging the SAME song keeps the pick;
    /// staging a DIFFERENT song drops it and writes that song's own default.
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

        stage_pick(Path::new("/tmp/user-choice.png")).unwrap();
        let kept = stage_for_song("cadenza", Some("cadenza")).unwrap();
        assert!(matches!(kept, CoverStage::PickKept));
        assert!(staged_is_pick(), "same song → the pick is untouched");
        assert_eq!(staged_path().as_deref(), Some("/tmp/user-choice.png"));

        // A different song: the pick is dropped and dusk's own cover staged
        // as a DEFAULT — never as a pick.
        let switched = stage_for_song("dusk", Some("cadenza")).unwrap();
        match switched {
            CoverStage::Default(path) => assert!(path.ends_with("covers/dusk.png")),
            _ => panic!("a song switch stages the new song's own default"),
        }
        assert!(!staged_is_pick(), "the new song's default is not a pick");
        assert!(staged_path().unwrap().ends_with("covers/dusk.png"));

        // And a song WITH no derivable cover on a switch clears the old
        // cover rather than leaving the previous song's wallpaper standing
        // (the leak this rule exists to close).
        stage_pick(Path::new("/tmp/user-choice.png")).unwrap();
        let cleared = stage_for_song("nocturne", Some("dusk")).unwrap();
        assert!(matches!(cleared, CoverStage::Cleared(true)));
        assert!(!cover_dst().exists(), "no derivable cover → no cover.json");
        assert_eq!(staged_path(), None);

        // An unknown previous song (a cold stage, a hand-written file) is a
        // switch too — a pick can only survive when we KNOW the song matches.
        stage_pick(Path::new("/tmp/user-choice.png")).unwrap();
        assert!(matches!(
            stage_for_song("nocturne", None).unwrap(),
            CoverStage::Cleared(true)
        ));
        assert!(!cover_dst().exists());

        // Clearing when nothing is staged is idempotent, not an error.
        assert!(matches!(
            stage_for_song("nocturne", Some("dusk")).unwrap(),
            CoverStage::Cleared(false)
        ));

        let _ = std::fs::remove_dir_all(&root);
    }
}
