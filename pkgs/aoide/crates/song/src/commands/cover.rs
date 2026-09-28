//! `cover set` — the live wallpaper write-path (song/covers/).

use aoide_protocol::Invocation;
use aoide_protocol::output::Outcome;
use aoide_protocol::registry::{arg, cmd, flag, Registry};
use serde_json::{json, Value};

pub fn register(r: &mut Registry) {
    r.insert(cmd!(
        path: ["cover", "set"],
        summary: "Set the live wallpaper: stage stage/cover.json (hot-swap) from a cover path or a bare name in song/covers/ — a PICK, which survives re-staging the same song. `--clear` drops it and returns to the active song's own default. Refuses while `rice mode declarative` is locked.",
        args: [arg!("path", "string", false, "Absolute cover path, or a bare filename resolved against song/covers/. Omitted with `--clear`.")],
        flags: [flag!("clear", "bool", "Drop the pick: return the active song to its own default (its derivable cover staged as a song default, else no stage/cover.json at all — the baked/palette fallback and the song's own live board).")],
        gated: false,
        implemented: true,
        handler: handle_cover_set_entry,
    ));
}

/// `cover set` registry entrypoint — refuses while `rice mode declarative`
/// is locked, same guard as `rice stage`'s own entrypoint
/// (`commands/rice.rs::handle_rice_stage_entry`, khoa 2026-08-14). The pure
/// write logic stays in [`handle_cover_set`] guard-free.
fn handle_cover_set_entry(inv: &Invocation) -> Outcome {
    let mode_marker = aoide_storage::mode::load_mode_marker();
    if mode_marker.mode == aoide_storage::mode::RiceMode::Declarative {
        return Outcome::error(
            "cover.set",
            "declarative mode is locked — run `aoide rice mode stage` to unlock hot-loading first",
        )
        .with_data(json!({ "reason": "declarative-mode-locked" }));
    }
    let mut out = handle_cover_set(inv);

    // Auto-take (phase A3) — same hook, same posture, same rationale as
    // `rice stage`'s own in `commands/rice.rs::handle_rice_stage_entry`; see
    // that function's doc comment for the full write-up (Draft-mode-only
    // gate off `mode_marker` read before the write; unconditional
    // `snapshot` — not the drift-checking core — because a take records
    // every write and the resulting noise is pruning's problem, not
    // write-time suppression's; the non-fatal `"take"/"takeError"`
    // reporting posture). `cause` is `"cover-set"` and `cmd` is
    // `"cover.set"` — its own dotted name, not `rice.stage`'s, so a
    // refusal names the command that actually ran.
    if out.status == aoide_protocol::output::Status::Ok
        && mode_marker.mode == aoide_storage::mode::RiceMode::Draft
    {
        match super::take::snapshot("cover.set", "cover-set") {
            Ok(record) => {
                if let (Some(song), Some(draft)) = (&mode_marker.song, &mode_marker.draft) {
                    out.changed.push(
                        aoide_storage::takes::take_path(song, Some(draft), record.take)
                            .to_string_lossy()
                            .into_owned(),
                    );
                    out.changed.push(
                        aoide_storage::takes::head_path(song, Some(draft))
                            .to_string_lossy()
                            .into_owned(),
                    );
                }
                if let Some(Value::Object(map)) = &mut out.data {
                    map.insert("take".to_string(), json!(record.take));
                }
            }
            Err(err) => {
                if let Some(Value::Object(map)) = &mut out.data {
                    map.insert("take".to_string(), Value::Null);
                    map.insert("takeError".to_string(), json!(err.message));
                }
            }
        }
    }
    out
}

/// `cover set <path>` — the live wallpaper write path.
///
/// Resolves `<path>` (absolute, or a bare name under `song/covers/`) and stages
/// it as a pick for the song staged right now (CONTRACTS.md §4): what the user
/// chose shows, the song's own board does not draw over it, and re-staging that
/// song leaves it alone. Nothing is committed; the baked `AOIDE_WALLPAPER`
/// remains the boot fallback.
fn handle_cover_set(inv: &Invocation) -> Outcome {
    if inv.flag_present("clear") {
        return handle_cover_clear(inv);
    }
    let arg = match inv.args.first() {
        Some(a) => a.clone(),
        None => {
            return Outcome::usage(
                "cover.set",
                "usage: aoide cover set <path|name> [--json] · aoide cover set --clear [--json]",
            )
            .with_data(json!({ "reason": "missing-path" }));
        }
    };

    // Absolute path → literal; anything else → the shared covers/ library.
    let resolved = crate::cover::resolve_cover_arg(&arg);

    if !resolved.is_file() {
        return Outcome::error(
            "cover.set",
            format!("no cover at {}: not a file", resolved.display()),
        )
        .with_data(json!({
            "reason": "cover-not-found",
            "arg": arg,
            "resolved": resolved.to_string_lossy(),
        }));
    }

    let song = super::mode::current_staged_song();
    let cover_dst = match crate::cover::stage_pick(&resolved, song.as_deref()) {
        Ok(dst) => dst,
        Err(e) => {
            return Outcome::error("cover.set", format!("failed to stage cover.json: {e}"))
                .with_data(json!({
                    "reason": "stage-write-failed",
                    "target": crate::cover::staged_cover_json().to_string_lossy(),
                }));
        }
    };

    Outcome::ok(
        "cover.set",
        match &song {
            Some(song) => format!(
                "wallpaper set to {} for `{song}` — stage/cover.json live for hot-swap",
                resolved.display()
            ),
            None => format!(
                "wallpaper set to {} — stage/cover.json live for hot-swap",
                resolved.display()
            ),
        },
    )
    .changed(vec![cover_dst.to_string_lossy().into_owned()])
    .with_data(json!({
        "cover": resolved.to_string_lossy(),
        "coverJson": cover_dst.to_string_lossy(),
        "pick": true,
        "song": song,
        "seam": "AoideWallpaper.qml FileView-watches stage/cover.json and hot-swaps live",
    }))
}

/// `cover set --clear` — drop the pick and give the staged song back its own
/// default. With no song staged there is no default to derive, so the clear is
/// a removal.
fn handle_cover_clear(inv: &Invocation) -> Outcome {
    if !inv.args.is_empty() {
        return Outcome::usage(
            "cover.set",
            "usage: aoide cover set <path|name> [--json] · aoide cover set --clear [--json]",
        )
        .with_data(json!({ "reason": "clear-takes-no-path" }));
    }

    let song = super::mode::current_staged_song();
    let dropped_pick =
        crate::cover::staged_is_pick() && crate::cover::staged_song() == song;
    let cover_dst = crate::cover::staged_cover_json();
    let staged = match &song {
        Some(song) => crate::cover::stage_song_default(song),
        None => crate::cover::clear_staged(),
    };
    let staged = match staged {
        Ok(staged) => staged,
        Err(e) => {
            return Outcome::error("cover.set", format!("failed to clear cover.json: {e}"))
                .with_data(json!({
                    "reason": "clear-failed",
                    "target": cover_dst.to_string_lossy(),
                }));
        }
    };
    let stage_word = staged.as_str();
    match staged {
        crate::cover::CoverWrite::Written(path) => Outcome::ok(
            "cover.set",
            format!(
                "pick dropped — {} is back to its own cover {}",
                song.as_deref().unwrap_or("the song"),
                path.display()
            ),
        )
        .changed(vec![cover_dst.to_string_lossy().into_owned()])
        .with_data(json!({
            "cleared": false,
            "droppedPick": dropped_pick,
            "cover": path.to_string_lossy(),
            "coverJson": cover_dst.to_string_lossy(),
            "coverStage": stage_word,
        })),
        write => {
            let removed = matches!(write, crate::cover::CoverWrite::Removed);
            let whose = match &song {
                Some(song) => format!("`{song}` has no derivable cover"),
                None => "no song is staged".to_string(),
            };
            let note = if dropped_pick {
                "pick dropped"
            } else if removed {
                "no pick for this song to drop"
            } else {
                "no pick staged"
            };
            let mut out = Outcome::ok(
                "cover.set",
                format!("{note} — {whose}; the song's own default (the baked/palette fallback, \
                         and its own live board) shows again"),
            )
            .with_data(json!({
                "cleared": removed,
                "droppedPick": dropped_pick,
                "cover": Value::Null,
                "coverJson": cover_dst.to_string_lossy(),
                "coverStage": stage_word,
            }));
            if removed {
                out = out.changed(vec![cover_dst.to_string_lossy().into_owned()]);
            }
            out
        }
    }
}

// ── Tests (cover set) ────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use aoide_test_support::*;
    use aoide_protocol::output::Status;

    #[test]
    fn cover_set_stages_an_absolute_path() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        let root = unique_tmp("cover-abs");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        let img = root.join("elsewhere.png");
        std::fs::write(&img, b"\x89PNG stub").unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        let out = handle_cover_set(&inv(&["cover", "set"], &[img.to_str().unwrap()]));
        assert_eq!(out.status, Status::Ok);
        // cover.json landed in the stage and points at the absolute path.
        let cover = std::fs::read_to_string(stage.join("cover.json")).unwrap();
        assert!(cover.contains("elsewhere.png"), "cover.json points at the file: {cover}");
        assert!(cover.ends_with("\n"), "trailing newline mirrors rice stage");
        assert!(
            cover.contains("\"pick\": true"),
            "`cover set` stages a PICK, not a song default: {cover}"
        );
        assert!(out.changed.iter().any(|c| c.ends_with("cover.json")));
        let data = out.data.unwrap();
        assert_eq!(data["cover"].as_str().unwrap(), img.to_string_lossy());
        assert_eq!(data["pick"], true);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn cover_set_resolves_a_bare_name_against_the_covers_library() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        let root = unique_tmp("cover-name");
        let stage = root.join("stage");
        let covers = root.join("covers");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::create_dir_all(&covers).unwrap();
        std::fs::write(covers.join("sonata.webp"), b"RIFF stub").unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        let out = handle_cover_set(&inv(&["cover", "set"], &["sonata.webp"]));
        assert_eq!(out.status, Status::Ok);
        let staged = out.data.unwrap()["cover"].as_str().unwrap().to_string();
        assert!(
            staged.ends_with("covers/sonata.webp"),
            "bare name resolved under the shared covers library: {staged}"
        );
        let cover = std::fs::read_to_string(stage.join("cover.json")).unwrap();
        assert!(cover.contains("covers/sonata.webp"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn cover_set_missing_file_is_error_exit_1() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        let root = unique_tmp("cover-missing");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        let out = handle_cover_set(&inv(&["cover", "set"], &["nope.png"]));
        assert_eq!(out.status, Status::Error);
        assert_eq!(out.render(false).1, aoide_protocol::output::exit::ERROR);
        assert_eq!(out.data.unwrap()["reason"], "cover-not-found");
        // Nothing was staged for a missing file.
        assert!(!stage.join("cover.json").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn cover_set_missing_arg_is_usage_exit_2() {
        let out = handle_cover_set(&inv(&["cover", "set"], &[]));
        assert_eq!(out.status, Status::Usage);
        assert_eq!(out.render(false).1, aoide_protocol::output::exit::USAGE);
    }

    // ── cover set --clear: drop the pick, back to the song's default ─────────

    /// `cover set --clear` returns the ACTIVE song to its own default. With a
    /// derivable cover that means the song's cover comes back (as a default,
    /// never as a pick); with none it means no `cover.json` at all (the baked
    /// /palette fallback, plus the song's own live board).
    #[test]
    fn clear_drops_the_pick_and_restores_the_songs_own_default() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        let root = unique_tmp("cover-clear");
        let stage = root.join("stage");
        let covers = root.join("covers");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::create_dir_all(&covers).unwrap();
        std::fs::write(covers.join("dusk.png"), b"\x89PNG stub").unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        // What `rice stage dusk` leaves: the song, and dusk's own cover as a
        // DEFAULT.
        std::fs::write(
            stage.join("livery.json"),
            r#"{"schemaVersion":"0","song":"dusk"}"#,
        )
        .unwrap();
        crate::cover::stage_default(&covers.join("dusk.png"), "dusk").unwrap();
        assert!(!crate::cover::staged_is_pick());

        // The user picks something else…
        let img = root.join("chosen.png");
        std::fs::write(&img, b"\x89PNG stub").unwrap();
        let set = handle_cover_set(&inv(&["cover", "set"], &[img.to_str().unwrap()]));
        assert_eq!(set.status, Status::Ok);
        assert!(crate::cover::staged_is_pick());

        // …and clears it: dusk's own cover is back, unmarked.
        let cleared = handle_cover_set(&clear_inv(&[]));
        assert_eq!(cleared.status, Status::Ok, "{:?}", cleared.data);
        let data = cleared.data.unwrap();
        assert_eq!(data["coverStage"], "default");
        assert_eq!(data["cleared"], false);
        assert!(data["cover"].as_str().unwrap().ends_with("covers/dusk.png"));
        assert!(!crate::cover::staged_is_pick(), "the restored cover is a default");
        assert!(cleared.changed.iter().any(|c| c.ends_with("cover.json")));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A song with NO derivable cover: the clear removes the file, and a second
    /// clear is a quiet no-op rather than an error.
    #[test]
    fn clear_removes_the_file_when_the_song_derives_none_and_is_idempotent() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        let root = unique_tmp("cover-clear-none");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        std::fs::write(
            stage.join("livery.json"),
            r#"{"schemaVersion":"0","song":"moonlight"}"#,
        )
        .unwrap();
        crate::cover::stage_pick(std::path::Path::new("/tmp/old-choice.png"), Some("moonlight"))
            .unwrap();

        let cleared = handle_cover_set(&clear_inv(&[]));
        assert_eq!(cleared.status, Status::Ok);
        let data = cleared.data.unwrap();
        assert_eq!(data["cleared"], true);
        assert!(data["cover"].is_null());
        assert!(!stage.join("cover.json").exists());
        assert!(cleared.changed.iter().any(|c| c.ends_with("cover.json")));

        // Nothing staged any more → a second clear says so, exit 0, no change.
        let again = handle_cover_set(&clear_inv(&[]));
        assert_eq!(again.status, Status::Ok);
        assert_eq!(again.data.unwrap()["cleared"], false);
        assert!(again.changed.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// With no stage file at all there is no song to derive a default for —
    /// the clear is just the removal (the baked/palette fallback shows).
    #[test]
    fn clear_with_nothing_staged_only_removes() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        let root = unique_tmp("cover-clear-nosong");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        crate::cover::stage_pick(std::path::Path::new("/tmp/old-choice.png"), None).unwrap();
        let out = handle_cover_set(&clear_inv(&[]));
        assert_eq!(out.status, Status::Ok);
        assert_eq!(out.data.unwrap()["cleared"], true);
        assert!(!stage.join("cover.json").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// `--clear` and a path are opposite intents — refused, never half-honoured.
    #[test]
    fn clear_with_a_path_is_usage_exit_2() {
        let out = handle_cover_set(&clear_inv(&["sonata.webp"]));
        assert_eq!(out.status, Status::Usage);
        assert_eq!(out.render(false).1, aoide_protocol::output::exit::USAGE);
        assert_eq!(out.data.unwrap()["reason"], "clear-takes-no-path");
    }

    /// The declarative lock covers the clear too: it is a stage write like any
    /// other, and the entrypoint guard is where that policy lives.
    #[test]
    fn clear_entry_refuses_while_declarative_mode_is_locked() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        let root = unique_tmp("cover-clear-locked");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        crate::cover::stage_pick(std::path::Path::new("/tmp/old-choice.png"), None).unwrap();

        let out = handle_cover_set_entry(&clear_inv(&[]));
        assert_eq!(out.status, Status::Error);
        assert_eq!(out.data.unwrap()["reason"], "declarative-mode-locked");
        assert!(stage.join("cover.json").is_file(), "nothing cleared while locked");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// `cover set --clear`, as the registry spells it.
    fn clear_inv(args: &[&str]) -> Invocation {
        let mut i = inv(&["cover", "set"], args);
        i.flags.insert("clear".into(), "true".into());
        i
    }

    // ── cover set: the declarative-mode write guard (khoa 2026-08-14) ────────

    #[test]
    fn cover_set_entry_refuses_while_declarative_mode_is_locked() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        let root = unique_tmp("cover-entry-locked");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        let img = root.join("elsewhere.png");
        std::fs::write(&img, b"\x89PNG stub").unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        let out = handle_cover_set_entry(&inv(&["cover", "set"], &[img.to_str().unwrap()]));
        assert_eq!(out.status, Status::Error);
        assert_eq!(out.data.unwrap()["reason"], "declarative-mode-locked");
        assert!(!stage.join("cover.json").exists(), "nothing staged while locked");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn cover_set_entry_allows_writes_once_staging_mode_is_unlocked() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        let root = unique_tmp("cover-entry-unlocked");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        let img = root.join("elsewhere.png");
        std::fs::write(&img, b"\x89PNG stub").unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        aoide_storage::mode::save_mode_marker(&aoide_storage::mode::ModeMarker {
            mode: aoide_storage::mode::RiceMode::Staging,
            ..Default::default()
        })
        .unwrap();

        let out = handle_cover_set_entry(&inv(&["cover", "set"], &[img.to_str().unwrap()]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert!(stage.join("cover.json").is_file());
        let _ = std::fs::remove_dir_all(&root);
    }

    // ── cover set: the auto-take hook (phase A3) ─────────────────────────

    #[test]
    fn cover_set_entry_in_draft_mode_mints_an_auto_take_on_a_real_change() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        let root = unique_tmp("cover-autotake-fires");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::write(stage.join("livery.json"), VALID_NOTES).unwrap();
        let img = root.join("elsewhere.png");
        std::fs::write(&img, b"\x89PNG stub").unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        aoide_storage::mode::save_mode_marker(&aoide_storage::mode::ModeMarker {
            mode: aoide_storage::mode::RiceMode::Draft,
            song: Some("moonlight".to_string()),
            draft: Some("neon-night".to_string()),
            ..Default::default()
        })
        .unwrap();

        let out = handle_cover_set_entry(&inv(&["cover", "set"], &[img.to_str().unwrap()]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert_eq!(out.data.as_ref().unwrap()["take"], 1);
        assert!(out.changed.iter().any(|c| c.ends_with("takes/0001.json")));
        assert!(out.changed.iter().any(|c| c.ends_with("takes/head.json")));

        let record = aoide_storage::takes::load_take("moonlight", Some("neon-night"), 1).unwrap();
        assert_eq!(record.cause, "cover-set");
        assert!(record.cover.is_some(), "the cover that was just set is carried on the take");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn cover_set_entry_in_draft_mode_mints_again_on_a_content_identical_reset() {
        // The pinned invariant (orchestrator correction over this step's own
        // earlier draft): a take records EVERY write, not just the ones that
        // changed something. Setting the SAME cover twice in a row produces
        // byte-identical content to what take 1 already holds — it must
        // STILL mint a second take. Suppressing on no drift would make the
        // take tree an incomplete record of write events; the resulting
        // duplicate-take noise is `rice take prune`'s problem (phase A9),
        // not this hook's.
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        let root = unique_tmp("cover-autotake-repeat");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::write(stage.join("livery.json"), VALID_NOTES).unwrap();
        let img = root.join("elsewhere.png");
        std::fs::write(&img, b"\x89PNG stub").unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        aoide_storage::mode::save_mode_marker(&aoide_storage::mode::ModeMarker {
            mode: aoide_storage::mode::RiceMode::Draft,
            song: Some("moonlight".to_string()),
            draft: Some("neon-night".to_string()),
            ..Default::default()
        })
        .unwrap();

        let first = handle_cover_set_entry(&inv(&["cover", "set"], &[img.to_str().unwrap()]));
        assert_eq!(first.status, Status::Ok, "{:?}", first.data);
        assert_eq!(first.data.unwrap()["take"], 1);

        let second = handle_cover_set_entry(&inv(&["cover", "set"], &[img.to_str().unwrap()]));
        assert_eq!(second.status, Status::Ok, "{:?}", second.data);
        assert_eq!(
            second.data.unwrap()["take"], 2,
            "a content-identical cover reset still mints its own take"
        );
        assert!(second.changed.iter().any(|c| c.ends_with("takes/0002.json")));

        let record = aoide_storage::takes::load_take("moonlight", Some("neon-night"), 2).unwrap();
        assert_eq!(record.parent, Some(1), "the second take hangs off the first");
        assert_eq!(
            aoide_storage::takes::list_takes("moonlight", Some("neon-night")).len(),
            2,
            "both writes are on record, even though their content is identical"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
