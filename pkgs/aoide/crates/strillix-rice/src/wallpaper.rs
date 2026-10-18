//! Wallpaper-art derivation + resolution for the shared library `rice/wallpapers/`,
//! and the stage seam that decides what `stage/wallpaper.json` holds: a user pick
//! or the rice's own default (CONTRACTS.md §4). Moved out of
//! `pkgs/aoide/src/commands/{rice,wallpaper}.rs` (Phase 5b restructure,
//! docs/architecture/PACKAGE-LAYOUT.md). Reads env-derived paths and stats
//! candidates; the stage writers are the only side effects.
//!
//! A pick names WHAT it is as well as which file: `kind` is `static`, `video` or
//! `we` (a Wallpaper Engine scene, whose identity is a workshop id in place of a
//! path). Absent reads as `static` — see [`KIND_FIELD`]. A still pick also says
//! how it is laid on the screen: `tile` when it repeats at 1:1, else it is
//! cropped to fill — see [`pick_tiles`].

use std::path::{Path, PathBuf};

/// Extensions we recognise as wallpaper art, in preference order.
pub const WALLPAPER_EXTS: &[&str] = &["webp", "png", "jpg", "jpeg"];

/// Marks `stage/wallpaper.json` as the USER's pick. Absent — in every file written
/// before the field existed, and in every `rice stage` write — means the rice's
/// own default.
pub const PICK_FIELD: &str = "pick";

/// The rice a wallpaper was staged for. Every writer stamps it; a file without it
/// is legacy and applies to whichever rice is staged.
pub const RICE_FIELD: &str = "rice";

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

/// How a still pick is laid out when `wallpaper set --position` forced it: written
/// only for `tile` or `fill`, so `auto` (the default) writes none.
pub const POSITION_FIELD: &str = "position";

/// `true` when the pick is painted repeated at 1:1 instead of cropped to fill.
/// Written only when it tiles; absent is no tile, which is every file written
/// before the field existed.
pub const TILE_FIELD: &str = "tile";

/// The `--position` words; `auto` is the default.
pub const POSITION_AUTO: &str = "auto";
pub const POSITION_TILE: &str = "tile";
pub const POSITION_FILL: &str = "fill";
pub const POSITIONS: [&str; 3] = [POSITION_AUTO, POSITION_TILE, POSITION_FILL];

/// The automatic rule's mark: a still whose file name carries it tiles.
pub const TILE_MARK: &str = "_tile";

/// Does a pick of the file at `path` tile? Only a still can. `tile` forces it,
/// `fill` forbids it, and anything else (`auto`, or no position) tiles a file
/// whose name — the last path component, lowercased — contains [`TILE_MARK`].
pub fn pick_tiles(path: &Path, position: Option<&str>) -> bool {
    kind_for_path(path) == KIND_STATIC
        && match position {
            Some(POSITION_TILE) => true,
            Some(POSITION_FILL) => false,
            _ => path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().to_lowercase().contains(TILE_MARK)),
        }
}

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

/// The stem of a rice's own desktop picture, inside its `wallpaper/` folder:
/// `wallpaper/<name>-desktop.<ext>`. The declared `aoide.theme.wallpaper` names
/// the same file at build time, so the seed and a staged rice agree on it.
pub const DESKTOP_WALLPAPER_STEM: &str = "desktop";

/// Derive a physical wallpaper-art file for a rice, or `None` when none exists.
///
/// Two places, in order. The rice's own picture — `rices/<name>/wallpaper/
/// <name>-desktop.<ext>`, the runtime copy of the folder the rice ships — wins;
/// then the shared library `rice/wallpapers/<name>.<ext>`, one dir any rice (or
/// other consumer) draws from. A bare `wallpaper.<ext>` would be ambiguous in the
/// shared dir, so both names carry the rice's name.
pub fn derive_wallpaper(name: &str) -> Option<PathBuf> {
    let own = aoide_storage::fs::rice_dir(name).join("wallpaper");
    let wallpapers = aoide_storage::fs::strillix_dir().join("wallpapers");
    WALLPAPER_EXTS
        .iter()
        .map(|ext| own.join(format!("{name}-{DESKTOP_WALLPAPER_STEM}.{ext}")))
        .chain(
            WALLPAPER_EXTS
                .iter()
                .map(|ext| wallpapers.join(format!("{name}.{ext}"))),
        )
        .find(|p| p.is_file())
}

/// The lock picture a rice ships: `lockscreen.<ext>` at the root of its own
/// folder, the first of [`WALLPAPER_EXTS`] that exists. The name is the render
/// surface's (`aoide.surfaces.lockscreen`), so a rice declares nothing: the
/// file is the whole opinion, and deleting it takes the opinion away.
pub fn lockscreen_image(strillix_dir: &Path) -> Option<PathBuf> {
    WALLPAPER_EXTS
        .iter()
        .map(|ext| strillix_dir.join(format!("lockscreen.{ext}")))
        .find(|p| p.is_file())
}

/// Resolve a `wallpaper set <path|name>` argument to an absolute wallpaper path.
///
/// An absolute `arg` is taken literally; anything else resolves against the
/// shared wallpaper library `rice/wallpapers/`. Does NOT check existence — the caller
/// (`handle_wallpaper_set`) does that so it can render its own "no such file"
/// error with the resolved path in it.
pub fn resolve_wallpaper_arg(arg: &str) -> PathBuf {
    let literal = PathBuf::from(arg);
    if literal.is_absolute() {
        literal
    } else {
        aoide_storage::fs::strillix_dir().join("wallpapers").join(arg)
    }
}

// ── The stage seam: the rice's own wallpaper, or the user's pick for it ──────────

/// `<stage>/wallpaper.json`.
pub fn staged_wallpaper_json() -> PathBuf {
    aoide_storage::fs::stage_dir().join("wallpaper.json")
}

fn read_staged() -> Option<serde_json::Value> {
    let raw = std::fs::read_to_string(staged_wallpaper_json()).ok()?;
    serde_json::from_str(&raw).ok()
}

/// The wallpaper path `stage/wallpaper.json` names, or `None` when the file is absent,
/// unreadable, or carries no non-empty `path`.
pub fn staged_path() -> Option<String> {
    let path = read_staged()?.get("path")?.as_str()?.to_string();
    (!path.is_empty()).then_some(path)
}

/// The rice the staged wallpaper was written for. `None` is a legacy file, written
/// before writers stamped it — it applies to whichever rice is staged.
pub fn staged_rice() -> Option<String> {
    read_staged()?
        .get(RICE_FIELD)?
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

/// Is the staged wallpaper the user's pick? Requires the marker AND an identity to
/// show — a path, or (for a scene) a workshop id. The same pair the shell's own
/// gate requires.
pub fn staged_is_pick() -> bool {
    let Some(doc) = read_staged() else {
        return false;
    };
    doc.get(PICK_FIELD).and_then(|v| v.as_bool()).unwrap_or(false) && staged_identity().is_some()
}

/// Stage a pick — what `wallpaper set` writes. `rice` is the rice staged at write
/// time; `None` (nothing staged) writes the legacy shape, which applies to
/// whichever rice loads. The kind follows the file: a video extension stages a
/// video pick, anything else a still. A still carries `position` when it was
/// forced to `tile` or `fill`, and `tile: true` when it tiles ([`pick_tiles`]);
/// a video carries neither.
pub fn stage_pick(path: &Path, rice: Option<&str>, position: Option<&str>) -> std::io::Result<PathBuf> {
    let kind = kind_for_path(path);
    let mut doc = serde_json::json!({ "path": path.to_string_lossy(), KIND_FIELD: kind, PICK_FIELD: true });
    if let Some(obj) = doc.as_object_mut() {
        if let Some(rice) = rice {
            obj.insert(RICE_FIELD.to_string(), serde_json::Value::String(rice.to_string()));
        }
        if let Some(forced) = position.filter(|p| kind == KIND_STATIC && [POSITION_TILE, POSITION_FILL].contains(p)) {
            obj.insert(POSITION_FIELD.to_string(), serde_json::Value::String(forced.to_string()));
        }
        if pick_tiles(path, position) {
            obj.insert(TILE_FIELD.to_string(), serde_json::Value::Bool(true));
        }
    }
    write_wallpaper(doc)
}

/// Stage a Wallpaper Engine scene as a pick: the workshop id IS the identity, so
/// there is no path (CONTRACTS.md §4).
pub fn stage_scene(we_id: &str, rice: Option<&str>) -> std::io::Result<PathBuf> {
    write_wallpaper(match rice {
        Some(rice) => serde_json::json!({
            KIND_FIELD: KIND_WE, WE_ID_FIELD: we_id, PICK_FIELD: true, RICE_FIELD: rice
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
pub fn stage_provider_pick(kind: &str, identity: &str, rice: Option<&str>) -> std::io::Result<PathBuf> {
    let mut doc = if kind == KIND_WE {
        serde_json::json!({ KIND_FIELD: KIND_WE, WE_ID_FIELD: identity })
    } else {
        serde_json::json!({ KIND_FIELD: kind, "path": identity })
    };
    if let Some(obj) = doc.as_object_mut() {
        obj.insert(PICK_FIELD.to_string(), serde_json::Value::Bool(true));
        if let Some(rice) = rice {
            obj.insert(RICE_FIELD.to_string(), serde_json::Value::String(rice.to_string()));
        }
    }
    write_wallpaper(doc)
}

/// Stage `rice`'s own wallpaper as a default — what `rice stage` writes.
pub fn stage_default(path: &Path, rice: &str) -> std::io::Result<PathBuf> {
    write_wallpaper(serde_json::json!({ "path": path.to_string_lossy(), RICE_FIELD: rice }))
}

/// Remove `stage/wallpaper.json`.
pub fn clear_staged() -> std::io::Result<WallpaperWrite> {
    match std::fs::remove_file(staged_wallpaper_json()) {
        Ok(()) => Ok(WallpaperWrite::Removed),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(WallpaperWrite::Absent),
        Err(e) => Err(e),
    }
}

/// What a default write did to `stage/wallpaper.json`.
pub enum WallpaperWrite {
    /// The rice's own wallpaper, written as a default.
    Written(PathBuf),
    /// A `wallpaper.json` that no longer belonged to this rice was removed.
    Removed,
    /// Nothing derivable, and nothing there to remove.
    Absent,
}

impl WallpaperWrite {
    /// The word the command envelopes report as `wallpaperStage`.
    pub fn as_str(&self) -> &'static str {
        match self {
            WallpaperWrite::Written(_) => "default",
            WallpaperWrite::Removed => "cleared",
            WallpaperWrite::Absent => "none",
        }
    }
}

/// [`stage_for_rice`]'s answer.
pub enum WallpaperStage {
    /// A pick for THIS rice was standing: left byte-identical.
    PickKept,
    /// The rice's own default was (re)established.
    Wrote(WallpaperWrite),
}

/// `rice`'s own default: its derivable wallpaper, else no `wallpaper.json` at all.
/// Keeps no pick — a caller that wants one preserved asks [`stage_for_rice`].
pub fn stage_rice_default(rice: &str) -> std::io::Result<WallpaperWrite> {
    match derive_wallpaper(rice) {
        Some(path) => {
            stage_default(&path, rice)?;
            Ok(WallpaperWrite::Written(path))
        }
        None => clear_staged(),
    }
}

/// `rice stage <rice>`'s wallpaper half (CONTRACTS.md §4). A wallpaper carries its
/// rice: a pick the user made for THIS rice is what shows, so re-staging that
/// rice — a bare `rice stage`, a `rice mode` re-pin, a rebuild — leaves it
/// alone. Any other wallpaper standing (another rice's pick or default, nothing at
/// all) is replaced by this rice's own default.
pub fn stage_for_rice(rice: &str) -> std::io::Result<WallpaperStage> {
    if staged_is_pick() && staged_rice().as_deref() == Some(rice) {
        return Ok(WallpaperStage::PickKept);
    }
    stage_rice_default(rice).map(WallpaperStage::Wrote)
}

/// `stage/wallpaper.json`'s own format version (CONTRACTS.md §4). Stamped by
/// [`write_wallpaper`], the one writer, so every shape it carries is versioned.
pub const STAGE_WALLPAPER_VERSION: &str = "1";

fn write_wallpaper(mut value: serde_json::Value) -> std::io::Result<PathBuf> {
    let dst = staged_wallpaper_json();
    if let Some(obj) = value.as_object_mut() {
        obj.insert("schemaVersion".into(), serde_json::json!(STAGE_WALLPAPER_VERSION));
    }
    let body = serde_json::to_string_pretty(&value).unwrap_or_default() + "\n";
    aoide_storage::fs::atomic_write(&dst, &body)?;
    Ok(dst)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_test_support::*;

    #[test]
    fn resolve_wallpaper_arg_takes_an_absolute_path_literally() {
        let resolved = resolve_wallpaper_arg("/tmp/somewhere/wallpaper.png");
        assert_eq!(resolved, PathBuf::from("/tmp/somewhere/wallpaper.png"));
    }

    #[test]
    fn resolve_wallpaper_arg_resolves_a_bare_name_against_the_wallpapers_library() {
        // Shares `aoide_test_support::env_lock()` with every other env-touching
        // test in the crate (rice.rs, mode.rs, draft.rs) — this used to lock a
        // separate crate-local mutex, so its `set_var("AOIDE_STAGE_DIR", …)`
        // could race a rice.rs/mode.rs test holding the OTHER lock and clobber
        // its env mid-flight.
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        std::env::set_var("AOIDE_STAGE_DIR", "/tmp/aoide-wallpaper-test/stage");

        let resolved = resolve_wallpaper_arg("sonata.webp");
        assert_eq!(
            resolved,
            PathBuf::from("/tmp/aoide-wallpaper-test/wallpapers/sonata.webp")
        );
    }

    #[test]
    fn derive_wallpaper_finds_the_first_matching_extension_or_none() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        let root =
            std::env::temp_dir().join(format!("strillix-rice-derive-wallpaper-{}", std::process::id()));
        let stage = root.join("stage");
        let wallpapers = root.join("wallpapers");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::create_dir_all(&wallpapers).unwrap();
        std::fs::write(wallpapers.join("dusk.png"), b"\x89PNG stub").unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        assert_eq!(derive_wallpaper("dusk"), Some(wallpapers.join("dusk.png")));
        assert_eq!(derive_wallpaper("nope"), None);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn derive_wallpaper_prefers_the_rices_own_desktop_picture_over_the_shared_library() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        let root = std::env::temp_dir().join(format!("strillix-rice-own-wallpaper-{}", std::process::id()));
        let stage = root.join("stage");
        let own = root.join("rices").join("barcarolle").join("wallpaper");
        let wallpapers = root.join("wallpapers");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::create_dir_all(&own).unwrap();
        std::fs::create_dir_all(&wallpapers).unwrap();
        std::fs::write(wallpapers.join("barcarolle.png"), b"\x89PNG shared").unwrap();
        // The logon picture is the greeter's, never the desktop default.
        std::fs::write(own.join("barcarolle-logon.jpg"), b"logon").unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        // Only the logon picture in the rice's wallpaper/: the shared library wins.
        assert_eq!(derive_wallpaper("barcarolle"), Some(wallpapers.join("barcarolle.png")));

        std::fs::write(own.join("barcarolle-desktop.jpg"), b"desktop").unwrap();
        assert_eq!(derive_wallpaper("barcarolle"), Some(own.join("barcarolle-desktop.jpg")));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn lockscreen_image_is_the_first_of_the_wallpaper_extensions_in_the_rice_folder() {
        let root = unique_tmp("lockscreen-image");
        let rice = root.join("barcarolle");
        std::fs::create_dir_all(&rice).unwrap();
        assert_eq!(lockscreen_image(&rice), None);

        std::fs::write(rice.join("wallpaper.png"), b"").unwrap();
        std::fs::create_dir(rice.join("lockscreen.webp")).unwrap();
        assert_eq!(lockscreen_image(&rice), None, "another name, or a directory, is no picture");

        std::fs::write(rice.join("lockscreen.jpeg"), b"").unwrap();
        assert_eq!(lockscreen_image(&rice), Some(rice.join("lockscreen.jpeg")));
        std::fs::write(rice.join("lockscreen.jpg"), b"").unwrap();
        assert_eq!(lockscreen_image(&rice), Some(rice.join("lockscreen.jpg")));
        std::fs::write(rice.join("lockscreen.png"), b"").unwrap();
        assert_eq!(lockscreen_image(&rice), Some(rice.join("lockscreen.png")));
        std::fs::remove_dir(rice.join("lockscreen.webp")).unwrap();
        std::fs::write(rice.join("lockscreen.webp"), b"").unwrap();
        assert_eq!(lockscreen_image(&rice), Some(rice.join("lockscreen.webp")));

        let _ = std::fs::remove_dir_all(&root);
    }

    // ── The stage seam: a pick vs a rice default (CONTRACTS.md §4) ───────────

    /// A pick is marked; the rice's own default is NOT. This is the whole
    /// discriminator AoideWallpaper.qml's board gate rides.
    #[test]
    fn a_pick_is_marked_and_a_default_is_not() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        let root = unique_tmp("wallpaper-pick-field");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        stage_pick(Path::new("/tmp/user-choice.png"), Some("dusk"), None).unwrap();
        assert!(staged_is_pick());
        assert_eq!(staged_path().as_deref(), Some("/tmp/user-choice.png"));
        assert_eq!(staged_rice().as_deref(), Some("dusk"));

        stage_default(Path::new("/tmp/rice-default.png"), "dusk").unwrap();
        assert!(!staged_is_pick(), "a rice default carries no pick field");
        assert_eq!(staged_path().as_deref(), Some("/tmp/rice-default.png"));
        assert_eq!(staged_rice().as_deref(), Some("dusk"));

        // A pick with no rice to attribute it to (nothing was staged) keeps
        // the whole path requirement: the marker needs a path to mean anything.
        stage_pick(Path::new("/tmp/user-choice.png"), None, None).unwrap();
        assert!(staged_is_pick());
        assert_eq!(staged_rice(), None);
        std::fs::write(
            staged_wallpaper_json(),
            "{\"path\":\"\",\"pick\":true,\"rice\":\"dusk\"}\n",
        )
        .unwrap();
        assert!(!staged_is_pick(), "the marker alone is not a pick");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Back-compat: a file with no `rice` field is legacy and applies to
    /// whichever rice is staged — but a legacy PICK is not kept by a re-stage,
    /// because keeping one requires knowing which rice it was made for.
    #[test]
    fn a_legacy_wallpaper_json_without_the_rice_field_is_never_kept() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        let root = unique_tmp("wallpaper-v0-field");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        std::fs::write(staged_wallpaper_json(), "{\"path\":\"/tmp/old.png\"}\n").unwrap();
        assert!(!staged_is_pick());
        assert_eq!(staged_path().as_deref(), Some("/tmp/old.png"));
        assert_eq!(staged_rice(), None);

        // …and an explicit `false` (or garbage) is not a pick either.
        std::fs::write(staged_wallpaper_json(), "{\"path\":\"/tmp/old.png\",\"pick\":false}").unwrap();
        assert!(!staged_is_pick());
        std::fs::write(staged_wallpaper_json(), "{not json").unwrap();
        assert!(!staged_is_pick(), "a torn file is never a pick");
        assert_eq!(staged_rice(), None);

        std::fs::write(
            staged_wallpaper_json(),
            "{\"path\":\"/tmp/old.png\",\"pick\":true}\n",
        )
        .unwrap();
        assert!(staged_is_pick());
        assert!(matches!(
            stage_for_rice("dusk").unwrap(),
            WallpaperStage::Wrote(WallpaperWrite::Removed)
        ));

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The ruling, in one test: re-staging the pick's OWN rice keeps it; any
    /// other rice replaces it with that rice's own default.
    #[test]
    fn restaging_the_same_rice_keeps_a_pick_and_a_rice_switch_drops_it() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        let root = unique_tmp("wallpaper-rice-switch");
        let stage = root.join("stage");
        let wallpapers = root.join("wallpapers");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::create_dir_all(&wallpapers).unwrap();
        std::fs::write(wallpapers.join("dusk.png"), b"\x89PNG stub").unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        stage_pick(Path::new("/tmp/user-choice.png"), Some("cadenza"), None).unwrap();
        assert!(matches!(stage_for_rice("cadenza").unwrap(), WallpaperStage::PickKept));
        assert!(staged_is_pick(), "the pick's own rice is staged");
        assert_eq!(staged_path().as_deref(), Some("/tmp/user-choice.png"));

        // A different rice takes its own wallpaper, as a default.
        match stage_for_rice("dusk").unwrap() {
            WallpaperStage::Wrote(WallpaperWrite::Written(path)) => {
                assert!(path.ends_with("wallpapers/dusk.png"), "{path:?}");
            }
            _ => panic!("a switch stages the new rice's own default"),
        }
        assert!(!staged_is_pick(), "the new rice's default is not a pick");
        assert_eq!(staged_rice().as_deref(), Some("dusk"));
        assert!(staged_path().unwrap().ends_with("wallpapers/dusk.png"));

        // A rice with nothing derivable leaves no wallpaper at all, so the
        // previous rice's wallpaper cannot stand over it.
        stage_pick(Path::new("/tmp/user-choice.png"), Some("cadenza"), None).unwrap();
        assert!(matches!(
            stage_for_rice("nocturne").unwrap(),
            WallpaperStage::Wrote(WallpaperWrite::Removed)
        ));
        assert!(!staged_wallpaper_json().exists());
        assert_eq!(staged_path(), None);

        // Nothing staged and nothing derivable: idempotent, not an error.
        assert!(matches!(
            stage_for_rice("nocturne").unwrap(),
            WallpaperStage::Wrote(WallpaperWrite::Absent)
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
    /// own rice).
    #[test]
    fn a_scene_pick_is_marked_with_an_id_and_no_path() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        let root = unique_tmp("wallpaper-scene");
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
            stage_for_rice("cadenza").unwrap(),
            WallpaperStage::PickKept
        ));

        // A still stages the path as its identity, with the kind the file
        // implies.
        stage_pick(Path::new("/tmp/clip.mp4"), Some("cadenza"), None).unwrap();
        assert_eq!(staged_kind(), KIND_VIDEO);
        assert_eq!(
            staged_identity(),
            Some((KIND_VIDEO.to_string(), "/tmp/clip.mp4".to_string()))
        );

        // A scene entry with an EMPTY id names nothing: not a pick.
        std::fs::write(
            staged_wallpaper_json(),
            "{\"kind\":\"we\",\"weId\":\"\",\"pick\":true}\n",
        )
        .unwrap();
        assert!(!staged_is_pick());
        assert_eq!(staged_identity(), None);

        // An unknown kind reads as a still, the shape a path-only pick always
        // had (additive field).
        std::fs::write(
            staged_wallpaper_json(),
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

    // ── How a still is laid out: position and tile (CONTRACTS.md §4) ────────

    /// Automatic tiles a still whose own file name carries `_tile`, in any
    /// case; `tile` and `fill` force the answer; a video never tiles.
    #[test]
    fn a_still_tiles_by_its_name_or_by_force_and_a_video_never() {
        for (path, position, tiles) in [
            ("/x/brick_tile.png", None, true),
            ("/x/Brick_TILE.PNG", Some(POSITION_AUTO), true),
            ("/x/plain.png", None, false),
            ("/x/pattern_tiles/plain.png", None, false),
            ("/x/tile.png", None, false),
            ("/x/plain.png", Some(POSITION_TILE), true),
            ("/x/brick_tile.png", Some(POSITION_FILL), false),
            ("/x/loop_tile.mp4", None, false),
            ("/x/loop.mp4", Some(POSITION_TILE), false),
        ] {
            assert_eq!(pick_tiles(Path::new(path), position), tiles, "{path} {position:?}");
        }
    }

    /// `position` is written only when forced to tile or fill, `tile` only when
    /// the pick tiles, and a video carries neither. Re-staging the pick's own
    /// rice leaves both exactly as they were.
    #[test]
    fn a_pick_carries_position_only_when_forced_and_tile_only_when_it_tiles() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        let root = unique_tmp("wallpaper-position");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        let staged = || -> serde_json::Value {
            serde_json::from_str(&std::fs::read_to_string(staged_wallpaper_json()).unwrap()).unwrap()
        };

        for (path, position, written, tile) in [
            ("/x/plain.png", None, None, None),
            ("/x/plain.png", Some(POSITION_AUTO), None, None),
            ("/x/brick_tile.png", None, None, Some(true)),
            ("/x/plain.png", Some(POSITION_TILE), Some(POSITION_TILE), Some(true)),
            ("/x/brick_tile.png", Some(POSITION_FILL), Some(POSITION_FILL), None),
            ("/x/loop_tile.mp4", Some(POSITION_TILE), None, None),
        ] {
            stage_pick(Path::new(path), Some("cadenza"), position).unwrap();
            let doc = staged();
            assert_eq!(doc.get(POSITION_FIELD).and_then(|v| v.as_str()), written, "{path} {position:?}: {doc}");
            assert_eq!(doc.get(TILE_FIELD).and_then(|v| v.as_bool()), tile, "{path} {position:?}: {doc}");
        }

        stage_pick(Path::new("/x/plain.png"), Some("cadenza"), Some(POSITION_TILE)).unwrap();
        let before = std::fs::read_to_string(staged_wallpaper_json()).unwrap();
        assert!(matches!(stage_for_rice("cadenza").unwrap(), WallpaperStage::PickKept));
        assert_eq!(std::fs::read_to_string(staged_wallpaper_json()).unwrap(), before, "pick-kept is verbatim");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// What a provider reports is stamped exactly like a pick the user made —
    /// same marker, same `rice` — off the kind the CALLER named.
    #[test]
    fn a_provider_pick_is_stamped_like_any_other() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        let root = unique_tmp("wallpaper-engine-pick");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        stage_provider_pick(KIND_VIDEO, "/tmp/clip.mp4", Some("cadenza")).unwrap();
        assert!(staged_is_pick());
        assert_eq!(staged_rice().as_deref(), Some("cadenza"));
        assert_eq!(staged_kind(), KIND_VIDEO);
        assert_eq!(staged_path().as_deref(), Some("/tmp/clip.mp4"));

        stage_provider_pick(KIND_WE, "456", None).unwrap();
        assert!(staged_is_pick(), "no rice staged: the legacy shape still applies");
        assert_eq!(staged_rice(), None);
        assert_eq!(staged_we_id().as_deref(), Some("456"));
        assert_eq!(staged_path(), None);

        let _ = std::fs::remove_dir_all(&root);
    }
}
