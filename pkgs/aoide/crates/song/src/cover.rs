//! Cover-art derivation + resolution for the shared library `song/covers/`,
//! and the stage seam that decides what `stage/cover.json` holds: a user pick
//! or the song's own default (CONTRACTS.md §4). Moved out of
//! `pkgs/aoide/src/commands/{rice,cover}.rs` (Phase 5b restructure,
//! docs/architecture/PACKAGE-LAYOUT.md). Reads env-derived paths and stats
//! candidates; the stage writers are the only side effects.
//!
//! A pick names WHAT it is as well as which file: `kind` is `static`, `video` or
//! `we` (a Wallpaper Engine scene, whose identity is a workshop id in place of a
//! path). Absent reads as `static` — see [`KIND_FIELD`].

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

/// WHAT a pick is (CONTRACTS.md §4): a still, a video, or a Wallpaper Engine
/// scene. Additive — absent reads as [`KIND_STATIC`], which is every file
/// written before the field existed.
pub const KIND_FIELD: &str = "kind";

/// A scene's identity: the workshop id, in place of `path`. Only a `we` pick
/// carries it.
pub const WE_ID_FIELD: &str = "weId";

/// The three pick kinds, spelled as the provider's own protocol spells them
/// (`wall_proto::kind`).
pub const KIND_STATIC: &str = "static";
pub const KIND_VIDEO: &str = "video";
pub const KIND_WE: &str = "we";

/// The kinds a pick may carry, in the order CONTRACTS.md §4 lists them.
pub const KINDS: [&str; 3] = [KIND_STATIC, KIND_VIDEO, KIND_WE];

/// Extensions the PROVIDER recognises as video (`paper-control`'s `VIDEO_EXTS`,
/// mirrored here so a pick made through Aoide lands on the same kind the
/// provider derives for the same file). A `.gif` counts as one: upstream decides
/// that by decoding the file, and this crate does not decode, so every gif
/// stages as a video.
pub const VIDEO_EXTS: &[&str] =
    &["mp4", "mkv", "webm", "mov", "avi", "m4v", "flv", "wmv", "h264", "ivf", "gif"];

/// The kind a file path implies: [`KIND_VIDEO`] for a video extension (case
/// insensitive), [`KIND_STATIC`] otherwise.
pub fn kind_for_path(path: &Path) -> &'static str {
    let video = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| VIDEO_EXTS.iter().any(|v| e.eq_ignore_ascii_case(v)));
    if video {
        KIND_VIDEO
    } else {
        KIND_STATIC
    }
}

/// The `we:<id>` token a scene pick is spelled with on the command line, and
/// what the provider is applied with.
pub fn scene_token(we_id: &str) -> String {
    format!("{KIND_WE}:{we_id}")
}

/// `we:<id>` → the id, when the argument is a scene token with a non-empty id.
/// Anything else — including a bare id — is not a token this reads as a scene.
pub fn parse_scene_token(arg: &str) -> Option<&str> {
    arg.strip_prefix("we:")
        .map(str::trim)
        .filter(|id| !id.is_empty())
}

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

/// The staged pick's kind: `kind` when the file spells one of [`KINDS`], else
/// [`KIND_STATIC`] — null and a torn or unknown value read the same way a
/// missing one does.
pub fn staged_kind() -> String {
    read_staged()
        .and_then(|doc| doc.get(KIND_FIELD).and_then(|v| v.as_str()).map(str::to_string))
        .filter(|k| KINDS.contains(&k.as_str()))
        .unwrap_or_else(|| KIND_STATIC.to_string())
}

/// The staged scene's workshop id, when the pick is a scene.
pub fn staged_we_id() -> Option<String> {
    let id = read_staged()?.get(WE_ID_FIELD)?.as_str()?.to_string();
    (!id.is_empty()).then_some(id)
}

/// What the staged pick NAMES, as `(kind, identity)`: the file path for a still
/// or a video, the workshop id for a scene. `None` when nothing is named — no
/// file, no marker, or an empty identity. This is the pair the RECORD door
/// compares against a provider's report; [`crate::wallpaper_provider::sync`]
/// applies it without comparing anything.
pub fn staged_identity() -> Option<(String, String)> {
    match staged_kind().as_str() {
        KIND_WE => staged_we_id().map(|id| (KIND_WE.to_string(), id)),
        _ => staged_path().map(|path| (staged_kind(), path)),
    }
}

/// Is the staged cover the user's pick? Requires the marker AND an identity to
/// show — a path, or (for a scene) a workshop id. The same pair the shell's own
/// gate requires.
pub fn staged_is_pick() -> bool {
    let Some(doc) = read_staged() else {
        return false;
    };
    doc.get(PICK_FIELD).and_then(|v| v.as_bool()).unwrap_or(false) && staged_identity().is_some()
}

/// Stage a pick — what `cover set` writes. `song` is the song staged at write
/// time; `None` (nothing staged) writes the legacy shape, which applies to
/// whichever song loads. The kind follows the file: a video extension stages a
/// video pick, anything else a still.
pub fn stage_pick(path: &Path, song: Option<&str>) -> std::io::Result<PathBuf> {
    let kind = kind_for_path(path);
    let path = path.to_string_lossy();
    write_cover(match song {
        Some(song) => serde_json::json!({
            "path": path, KIND_FIELD: kind, PICK_FIELD: true, SONG_FIELD: song
        }),
        None => serde_json::json!({
            "path": path, KIND_FIELD: kind, PICK_FIELD: true
        }),
    })
}

/// Stage a Wallpaper Engine scene as a pick: the workshop id IS the identity, so
/// there is no path (CONTRACTS.md §4).
pub fn stage_scene(we_id: &str, song: Option<&str>) -> std::io::Result<PathBuf> {
    write_cover(match song {
        Some(song) => serde_json::json!({
            KIND_FIELD: KIND_WE, WE_ID_FIELD: we_id, PICK_FIELD: true, SONG_FIELD: song
        }),
        None => serde_json::json!({
            KIND_FIELD: KIND_WE, WE_ID_FIELD: we_id, PICK_FIELD: true
        }),
    })
}

/// Stage what an external PROVIDER reports it now shows: `kind` plus the
/// identity it named — a path for a still or a video, the workshop id for a
/// scene. The shape is `stage_pick`'s, off the kind the CALLER resolved (the
/// provider's own `%type%`), which is why the kind is not re-derived from the
/// identity here.
pub fn stage_provider_pick(kind: &str, identity: &str, song: Option<&str>) -> std::io::Result<PathBuf> {
    let mut doc = if kind == KIND_WE {
        serde_json::json!({ KIND_FIELD: KIND_WE, WE_ID_FIELD: identity })
    } else {
        serde_json::json!({ KIND_FIELD: kind, "path": identity })
    };
    if let Some(obj) = doc.as_object_mut() {
        obj.insert(PICK_FIELD.to_string(), serde_json::Value::Bool(true));
        if let Some(song) = song {
            obj.insert(SONG_FIELD.to_string(), serde_json::Value::String(song.to_string()));
        }
    }
    write_cover(doc)
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

    // ── The pick's KIND (CONTRACTS.md §4) ───────────────────────────────────

    /// A video extension is a video, anything else a still — the provider's own
    /// rule (`paper-control`'s `VIDEO_EXTS`), case-insensitively.
    #[test]
    fn kind_follows_the_files_extension_case_insensitively() {
        for video in ["/x/clip.mp4", "/x/CLIP.MP4", "/x/a.webm", "/x/a.mkv", "/x/a.h264", "/x/anim.gif"] {
            assert_eq!(kind_for_path(Path::new(video)), KIND_VIDEO, "{video}");
        }
        for still in ["/x/a.png", "/x/a.webp", "/x/noext", "/x/dir.mp4/a"] {
            assert_eq!(kind_for_path(Path::new(still)), KIND_STATIC, "{still}");
        }
    }

    /// The `we:<id>` token, and nothing else, is a scene.
    #[test]
    fn a_scene_token_is_we_colon_a_non_empty_id() {
        assert_eq!(parse_scene_token("we:123"), Some("123"));
        assert_eq!(parse_scene_token("we: 123 "), Some("123"));
        assert_eq!(scene_token("123"), "we:123");
        for not_a_token in ["123", "we:", "we: ", "web:1", "/x/a.png", ""] {
            assert_eq!(parse_scene_token(not_a_token), None, "{not_a_token}");
        }
    }

    /// A scene pick carries NO path: the identity is the workshop id, and it is
    /// what makes the pick a pick (and what keeps it across a re-stage of its
    /// own song).
    #[test]
    fn a_scene_pick_is_marked_with_an_id_and_no_path() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        let root = unique_tmp("cover-scene");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        stage_scene("123", Some("cadenza")).unwrap();
        assert!(staged_is_pick(), "a scene with an id is a pick");
        assert_eq!(staged_kind(), KIND_WE);
        assert_eq!(staged_we_id().as_deref(), Some("123"));
        assert_eq!(staged_path(), None, "a scene has no path to render");
        assert_eq!(staged_identity(), Some((KIND_WE.to_string(), "123".to_string())));
        assert!(matches!(
            stage_for_song("cadenza").unwrap(),
            CoverStage::PickKept
        ));

        // A still stages the path as its identity, with the kind the file
        // implies.
        stage_pick(Path::new("/tmp/clip.mp4"), Some("cadenza")).unwrap();
        assert_eq!(staged_kind(), KIND_VIDEO);
        assert_eq!(
            staged_identity(),
            Some((KIND_VIDEO.to_string(), "/tmp/clip.mp4".to_string()))
        );

        // A scene entry with an EMPTY id names nothing: not a pick.
        std::fs::write(
            staged_cover_json(),
            "{\"kind\":\"we\",\"weId\":\"\",\"pick\":true}\n",
        )
        .unwrap();
        assert!(!staged_is_pick());
        assert_eq!(staged_identity(), None);

        // An unknown kind reads as a still, the shape a path-only pick always
        // had (additive field).
        std::fs::write(
            staged_cover_json(),
            "{\"kind\":\"hologram\",\"path\":\"/tmp/a.png\",\"pick\":true}\n",
        )
        .unwrap();
        assert_eq!(staged_kind(), KIND_STATIC);
        assert_eq!(
            staged_identity(),
            Some((KIND_STATIC.to_string(), "/tmp/a.png".to_string()))
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// What a provider reports is stamped exactly like a pick the user made —
    /// same marker, same `song` — off the kind the CALLER named.
    #[test]
    fn a_provider_pick_is_stamped_like_any_other() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        let root = unique_tmp("cover-engine-pick");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        stage_provider_pick(KIND_VIDEO, "/tmp/clip.mp4", Some("cadenza")).unwrap();
        assert!(staged_is_pick());
        assert_eq!(staged_song().as_deref(), Some("cadenza"));
        assert_eq!(staged_kind(), KIND_VIDEO);
        assert_eq!(staged_path().as_deref(), Some("/tmp/clip.mp4"));

        stage_provider_pick(KIND_WE, "456", None).unwrap();
        assert!(staged_is_pick(), "no song staged: the legacy shape still applies");
        assert_eq!(staged_song(), None);
        assert_eq!(staged_we_id().as_deref(), Some("456"));
        assert_eq!(staged_path(), None);

        let _ = std::fs::remove_dir_all(&root);
    }
}
