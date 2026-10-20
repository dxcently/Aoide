//! `wallpaper set` — the live wallpaper write-path (rice/wallpapers/).

use aoide_protocol::Invocation;
use aoide_protocol::output::{Fix, Kind, Outcome};
use aoide_protocol::registry::{arg, cmd, flag, Registry};
use serde_json::{json, Value};

pub fn register(r: &mut Registry) {
    r.insert(cmd!(
        path: ["wallpaper", "set"],
        summary: "Set the live wallpaper: stage stage/wallpaper.json (hot-swap) from a wallpaper path or a bare name in rice/wallpapers/ — a PICK, which survives re-staging the same rice. A still is cropped to fill the screen, or tiled at 1:1 when its file name carries `_tile` or `--position tile` forces it. `--clear` drops it and returns to the active rice's own default. Refuses while `rice mode declarative` is locked, except for `--from-skwd`, which records what an external provider is already showing.",
        args: [arg!("path", "string", false, "Absolute wallpaper path, a bare filename resolved against rice/wallpapers/, or a `we:<id>` Wallpaper Engine scene token. Omitted with `--clear`.")],
        flags: [
            flag!("clear", "bool", "Drop the pick: return the active rice to its own default (its derivable wallpaper staged as a rice default, else no stage/wallpaper.json at all — the baked/palette fallback and the rice's own live board)."),
            flag!("from-skwd", "bool", "RECORD ONLY: the skwd-wall provider reports what it now shows, and this writes it into stage/wallpaper.json without applying anything back (the hook's own door). Ignored when the host's provider is not skwd-wall, and when the identity is the step-aside image."),
            flag!("kind", "string", "With `--from-skwd`: the provider's own `%type%` — static, video or we (a scene, whose identity is the workshop id)."),
            flag!("position", "string", "How a still is laid on the screen: auto (the default: tiled at 1:1 when the file name carries `_tile`, else cropped to fill), tile, or fill. Not for a video or a `we:<id>` scene.", value: "position", values: &crate::wallpaper::POSITIONS, conflicts: &["clear", "from-skwd"]),
        ],
        gated: false,
        implemented: true,
        handler: handle_wallpaper_set_entry,
    ));
    r.insert(cmd!(
        path: ["wallpaper", "sync"],
        summary: "Make the wallpaper provider agree with the stage files: an external provider is given the staged pick while one applies, and is told to show nothing otherwise (its own `clear` verb where the release has one, the transparent step-aside image otherwise); the shell's own layer needs no call. Best-effort, never fatal — the repair door for a dropped pick or a restarted provider.",
        args: [],
        flags: [],
        gated: false,
        implemented: true,
        handler: handle_wallpaper_sync,
    ));
}

/// `wallpaper sync` — the bridge's one write to an EXTERNAL wallpaper provider, and
/// the door a provider's own unit calls in `ExecStartPost` (CONTRACTS.md §4).
/// Pure decision in [`crate::wallpaper_provider::action`]; this is its envelope.
/// It is the ONLY caller that holds the reachable wait, because it is the only
/// one whose provider may still be starting up.
fn handle_wallpaper_sync(_inv: &Invocation) -> Outcome {
    let provider = crate::wallpaper_provider::provider();
    let status = crate::wallpaper_provider::sync_waiting();
    Outcome::ok("wallpaper.sync", format!("wallpaper provider `{provider}`: {status}"))
        .with_data(json!({
            "provider": provider,
            "status": status,
            "pick": crate::wallpaper_provider::applying_pick(),
        }))
}

/// `wallpaper set` registry entrypoint — refuses while `rice mode declarative`
/// is locked, same guard as `rice stage`'s own entrypoint
/// (`commands/rice.rs::handle_rice_stage_entry`, khoa 2026-08-14), with the
/// RECORD door exempt (CONTRACTS.md §4). The pure
/// write logic stays in [`handle_wallpaper_set`] guard-free.
fn handle_wallpaper_set_entry(inv: &Invocation) -> Outcome {
    let mode_marker = crate::mode::load_mode_marker();
    // EXEMPT: `--from-skwd` RECORDS what an external provider is already
    // showing. The lock exists so nix stays the only writer of the DECLARED
    // state; refusing the record would leave `stage/wallpaper.json` describing
    // something the screen is not showing (CONTRACTS.md §4). Every other
    // spelling of this command — a plain `wallpaper set`, `--clear` — stays refused.
    if !inv.flag_present("from-skwd")
        && mode_marker.mode == crate::mode::RiceMode::Declarative
    {
        return Outcome::error(
            "wallpaper.set",
            "declarative mode is locked — run `aoide rice mode stage` to unlock hot-loading first",
        )
        .with_data(json!({ "reason": "declarative-mode-locked" }));
    }
    let mut out = handle_wallpaper_set(inv);

    // Auto-take (A3) — the same hook `rice stage` has; `recorded_nothing` keeps
    // the RECORD door's no-ops out of it.
    let recorded_nothing = out
        .data
        .as_ref()
        .and_then(|data| data.get("recorded"))
        .and_then(Value::as_bool)
        == Some(false);
    if out.status == aoide_protocol::output::Status::Ok
        && !recorded_nothing
        && mode_marker.mode == crate::mode::RiceMode::Draft
    {
        super::history::attach_entry(&mut out, &mode_marker, super::history::snapshot("wallpaper.set", "wallpaper-set").map(Some));
    }
    out
}

/// `wallpaper set <path>` — the live wallpaper write path.
///
/// Resolves `<path>` (absolute, or a bare name under `rice/wallpapers/`) and stages
/// it as a pick for the rice staged right now (CONTRACTS.md §4): what the user
/// chose shows, the rice's own board does not draw over it, and re-staging that
/// rice leaves it alone. Nothing is committed; the baked `AOIDE_WALLPAPER`
/// remains the boot fallback.
fn handle_wallpaper_set(inv: &Invocation) -> Outcome {
    // Opposite intents, refused together rather than silently picking one (and
    // never a way past the declarative lock: the record door's exemption covers
    // a RECORD, not a clear in disguise).
    if inv.flag_present("clear") && inv.flag_present("from-skwd") {
        return Outcome::usage(
            "wallpaper.set",
            "usage: `--clear` drops a pick, `--from-skwd` records one — pass one of them",
        )
        .with_data(json!({ "reason": "clear-with-from-skwd" }));
    }
    if inv.flag_present("clear") {
        return handle_wallpaper_clear(inv);
    }
    if inv.flag_present("from-skwd") {
        return handle_wallpaper_from_skwd(inv);
    }
    let arg = match inv.args.first() {
        Some(a) => a.clone(),
        None => {
            return Outcome::usage(
                "wallpaper.set",
                "usage: aoide wallpaper set <path|name|we:<id>> [--json] · aoide wallpaper set --clear [--json]",
            )
            .with_data(json!({ "reason": "missing-path" }));
        }
    };

    let rice = super::mode::current_staged_rice();
    let position = inv.flags.get("position").map(String::as_str);

    // A scene token names the provider's own library entry: there is no file to
    // stat, and the id is taken as it was given (`we:123` is the key the provider
    // applies) — validated only for the shape the token promises.
    if let Some(we_id) = crate::wallpaper::parse_scene_token(&arg) {
        if position.is_some() {
            return position_refusal(&arg, "a Wallpaper Engine scene");
        }
        return match crate::wallpaper::stage_scene(we_id, rice.as_deref()) {
            Ok(dst) => Outcome::ok(
                "wallpaper.set",
                format!("wallpaper set to the Wallpaper Engine scene {we_id} — stage/wallpaper.json live for hot-swap"),
            )
            .changed(vec![dst.to_string_lossy().into_owned()])
            .with_data(json!({
                "wallpaperPick": crate::wallpaper::scene_token(we_id),
                "kind": crate::wallpaper::KIND_WE,
                "weId": we_id,
                "wallpaperJson": dst.to_string_lossy(),
                "pick": true,
                "rice": rice,
                "wallpaper": crate::wallpaper_provider::sync(),
                "seam": "AoideWallpaper.qml FileView-watches stage/wallpaper.json and hot-swaps live",
            })),
            Err(e) => Outcome::error("wallpaper.set", format!("failed to stage wallpaper.json: {e}"))
                .with_data(json!({
                    "reason": "stage-write-failed",
                    "target": crate::wallpaper::staged_wallpaper_json().to_string_lossy(),
                })),
        };
    }

    // Absolute path → literal; anything else → the shared wallpapers/ library.
    let resolved = crate::wallpaper::resolve_wallpaper_arg(&arg);
    let kind = crate::wallpaper::kind_for_path(&resolved);
    if position.is_some() && kind == crate::wallpaper::KIND_VIDEO {
        return position_refusal(&arg, "a video");
    }

    if !resolved.is_file() {
        return Outcome::error(
            "wallpaper.set",
            format!("no wallpaper at {}: not a file", resolved.display()),
        )
        .with_data(json!({
            "reason": "wallpaper-not-found",
            "arg": arg,
            "resolved": resolved.to_string_lossy(),
        }));
    }

    let wallpaper_dst = match crate::wallpaper::stage_pick(&resolved, rice.as_deref(), position) {
        Ok(dst) => dst,
        Err(e) => {
            return Outcome::error("wallpaper.set", format!("failed to stage wallpaper.json: {e}"))
                .with_data(json!({
                    "reason": "stage-write-failed",
                    "target": crate::wallpaper::staged_wallpaper_json().to_string_lossy(),
                }));
        }
    };

    let tile = crate::wallpaper::pick_tiles(&resolved, position);
    let laid = if tile { ", tiled at 1:1" } else { "" };
    Outcome::ok(
        "wallpaper.set",
        match &rice {
            Some(rice) => format!(
                "wallpaper set to {}{laid} for `{rice}` — stage/wallpaper.json live for hot-swap",
                resolved.display()
            ),
            None => format!(
                "wallpaper set to {}{laid} — stage/wallpaper.json live for hot-swap",
                resolved.display()
            ),
        },
    )
    .changed(vec![wallpaper_dst.to_string_lossy().into_owned()])
    .with_data(json!({
        "wallpaperPick": resolved.to_string_lossy(),
        "wallpaperJson": wallpaper_dst.to_string_lossy(),
        "kind": kind,
        "position": position.unwrap_or(crate::wallpaper::POSITION_AUTO),
        "tile": tile,
        "pick": true,
        "rice": rice,
        "wallpaper": crate::wallpaper_provider::sync(),
        "seam": "AoideWallpaper.qml FileView-watches stage/wallpaper.json and hot-swaps live",
    }))
}

/// `--position` lays out a still. A video or a scene is played by whatever
/// paints it, so the flag can never apply to one.
fn position_refusal(arg: &str, what: &str) -> Outcome {
    Outcome::refuse(
        "wallpaper.set",
        Kind::Usage,
        format!("`--position` lays out a still, and `{arg}` is {what}"),
        "only a still picture is tiled or cropped to fill; a video or a scene is laid out by what plays it",
        Fix::Run(format!("strillix wallpaper set {arg}")),
    )
    .with_fields(json!({ "reason": "position-not-a-still" }))
}

/// `wallpaper set --from-skwd --kind <static|video|we> <path|id>` — the RECORD
/// direction of the provider bridge: an external provider says what it now
/// shows, and this writes it into `stage/wallpaper.json` for the staged rice.
///
/// Record-only, deliberately: nothing here is applied back. A provider that
/// re-applied what it was just told would be told again by its own
/// post-processing hook, which is an unbounded loop (~140 applies/s measured);
/// the plain `wallpaper set` is the door that APPLIES, and this one is the door that
/// hears. The other half of the loop breaker is the identity check below.
///
/// Ignored — exit 0 with `recorded: false`, so a provider's hook is never an
/// error — when
///   * the host's provider is not this one (the shell's own layer paints; a
///     report about a surface nothing is looking at is not a pick),
///   * the identity is this host's step-aside image (the provider showed
///     *nothing*, which is not something to record),
///   * the staged pick already names this identity for this rice: Aoide applied
///     it, the provider's hook came back, and the second report is already true.
fn handle_wallpaper_from_skwd(inv: &Invocation) -> Outcome {
    const USAGE: &str = "usage: aoide wallpaper set --from-skwd --kind <static|video|we> <path|id> [--json]";
    let Some(kind) = inv.flags.get("kind").cloned() else {
        return Outcome::usage("wallpaper.set", USAGE)
            .with_data(json!({ "reason": "missing-kind" }));
    };
    if !crate::wallpaper::KINDS.contains(&kind.as_str()) {
        return Outcome::usage("wallpaper.set", USAGE).with_data(json!({
            "reason": "unknown-kind",
            "kind": kind,
            "kinds": crate::wallpaper::KINDS,
        }));
    }
    let identity = match inv.args.first().map(|a| a.trim()) {
        Some(a) if !a.is_empty() => a.to_string(),
        _ => {
            return Outcome::usage("wallpaper.set", USAGE)
                .with_data(json!({ "reason": "missing-identity" }));
        }
    };
    // A provider reports a scene as its workshop id, never as the `we:<id>` key
    // the CLI spells elsewhere: taking a token here would record `we:123` AS the
    // id, which no later apply could resolve.
    if crate::wallpaper::parse_scene_token(&identity).is_some() {
        return Outcome::usage("wallpaper.set", USAGE).with_data(json!({
            "reason": "scene-token-not-an-id",
            "identity": identity,
            "hint": "pass the bare workshop id with `--kind we`",
        }));
    }

    let rice = super::mode::current_staged_rice();

    let provider = crate::wallpaper_provider::provider();
    if provider != crate::wallpaper_provider::PROVIDER_SKWD_WALL {
        return Outcome::ok(
            "wallpaper.set",
            format!("not recorded — `{provider}` paints this host's wallpaper, not the reporting provider"),
        )
        .with_data(json!({
            "recorded": false,
            "ignored": "provider",
            "provider": provider,
            "rice": rice,
        }));
    }

    let standin = crate::wallpaper_provider::standin_png();
    if kind != crate::wallpaper::KIND_WE && standin.as_deref() == Some(identity.as_str()) {
        return Outcome::ok(
            "wallpaper.set",
            format!("not recorded — {identity} is the provider's step-aside image, not a pick"),
        )
        .with_data(json!({
            "recorded": false,
            "ignored": "standin",
            "provider": provider,
            "rice": rice,
        }));
    }

    let staged = crate::wallpaper::staged_identity();
    if staged.as_ref() == Some(&(kind.clone(), identity.clone()))
        && crate::wallpaper::staged_is_pick()
        && crate::wallpaper::staged_rice() == rice
    {
        return Outcome::ok(
            "wallpaper.set",
            format!("unchanged — the staged pick is already {kind} {identity}"),
        )
        .with_data(json!({
            "recorded": false,
            "unchanged": true,
            "kind": kind,
            "identity": identity,
            "provider": provider,
            "rice": rice,
        }));
    }

    match crate::wallpaper::stage_provider_pick(&kind, &identity, rice.as_deref()) {
        Ok(dst) => Outcome::ok(
            "wallpaper.set",
            match &rice {
                Some(rice) => {
                    format!("recorded {kind} {identity} as `{rice}`'s pick — stage/wallpaper.json live for hot-swap")
                }
                None => format!("recorded {kind} {identity} — stage/wallpaper.json live for hot-swap"),
            },
        )
        .changed(vec![dst.to_string_lossy().into_owned()])
        .with_data(json!({
            "wallpaperPick": if kind == crate::wallpaper::KIND_WE { crate::wallpaper::scene_token(&identity) } else { identity.clone() },
            "wallpaperJson": dst.to_string_lossy(),
            "kind": kind,
            "identity": identity,
            "pick": true,
            "recorded": true,
            "provider": provider,
            "rice": rice,
            "seam": "the provider's postProcessing hook reports what it applied; nothing is applied back (see `strillix wallpaper sync`)",
        })),
        Err(e) => Outcome::error("wallpaper.set", format!("failed to stage wallpaper.json: {e}"))
            .with_data(json!({
                "reason": "stage-write-failed",
                "target": crate::wallpaper::staged_wallpaper_json().to_string_lossy(),
            }))
    }
}

/// `wallpaper set --clear` — drop the pick and give the staged rice back its own
/// default. With no rice staged there is no default to derive, so the clear is
/// a removal.
fn handle_wallpaper_clear(inv: &Invocation) -> Outcome {
    if !inv.args.is_empty() {
        return Outcome::usage(
            "wallpaper.set",
            "usage: aoide wallpaper set <path|name> [--json] · aoide wallpaper set --clear [--json]",
        )
        .with_data(json!({ "reason": "clear-takes-no-path" }));
    }

    let rice = super::mode::current_staged_rice();
    let dropped_pick =
        crate::wallpaper::staged_is_pick() && crate::wallpaper::staged_rice() == rice;
    let wallpaper_dst = crate::wallpaper::staged_wallpaper_json();
    let staged = match &rice {
        Some(rice) => crate::wallpaper::stage_rice_default(rice),
        None => crate::wallpaper::clear_staged(),
    };
    let staged = match staged {
        Ok(staged) => staged,
        Err(e) => {
            return Outcome::error("wallpaper.set", format!("failed to clear wallpaper.json: {e}"))
                .with_data(json!({
                    "reason": "clear-failed",
                    "target": wallpaper_dst.to_string_lossy(),
                }));
        }
    };
    let stage_word = staged.as_str();
    match staged {
        crate::wallpaper::WallpaperWrite::Written(path) => Outcome::ok(
            "wallpaper.set",
            format!(
                "pick dropped — {} is back to its own wallpaper {}",
                rice.as_deref().unwrap_or("the rice"),
                path.display()
            ),
        )
        .changed(vec![wallpaper_dst.to_string_lossy().into_owned()])
        .with_data(json!({
            "cleared": false,
            "droppedPick": dropped_pick,
            "wallpaperPick": path.to_string_lossy(),
            "wallpaperJson": wallpaper_dst.to_string_lossy(),
            "wallpaperStage": stage_word,
            "wallpaper": crate::wallpaper_provider::sync(),
        })),
        write => {
            let removed = matches!(write, crate::wallpaper::WallpaperWrite::Removed);
            let whose = match &rice {
                Some(rice) => format!("`{rice}` has no derivable wallpaper"),
                None => "no rice is staged".to_string(),
            };
            let note = if dropped_pick {
                "pick dropped"
            } else if removed {
                "no pick for this rice to drop"
            } else {
                "no pick staged"
            };
            let mut out = Outcome::ok(
                "wallpaper.set",
                format!("{note} — {whose}; the rice's own default (the baked/palette fallback, \
                         and its own live board) shows again"),
            )
            .with_data(json!({
                "cleared": removed,
                "droppedPick": dropped_pick,
                "wallpaperPick": Value::Null,
                "wallpaperJson": wallpaper_dst.to_string_lossy(),
                "wallpaperStage": stage_word,
                "wallpaper": crate::wallpaper_provider::sync(),
            }));
            if removed {
                out = out.changed(vec![wallpaper_dst.to_string_lossy().into_owned()]);
            }
            out
        }
    }
}

// ── Tests (wallpaper set) ────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use aoide_test_support::*;
    use aoide_protocol::output::Status;

    #[test]
    fn wallpaper_set_stages_an_absolute_path() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("wallpaper-abs");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        let img = root.join("elsewhere.png");
        std::fs::write(&img, b"\x89PNG stub").unwrap();
        aoide_test_support::pin_stage(&stage);

        let out = handle_wallpaper_set(&inv(&["wallpaper", "set"], &[img.to_str().unwrap()]));
        assert_eq!(out.status, Status::Ok);
        // wallpaper.json landed in the stage and points at the absolute path.
        let wallpaper = std::fs::read_to_string(stage.join("wallpaper.json")).unwrap();
        assert!(wallpaper.contains("elsewhere.png"), "wallpaper.json points at the file: {wallpaper}");
        assert!(wallpaper.ends_with("\n"), "trailing newline mirrors rice stage");
        assert!(
            wallpaper.contains("\"pick\": true"),
            "`wallpaper set` stages a PICK, not a rice default: {wallpaper}"
        );
        assert!(out.changed.iter().any(|c| c.ends_with("wallpaper.json")));
        let data = out.data.unwrap();
        assert_eq!(data["wallpaperPick"].as_str().unwrap(), img.to_string_lossy());
        assert_eq!(data["pick"], true);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn wallpaper_set_resolves_a_bare_name_against_the_wallpapers_library() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("wallpaper-name");
        let stage = root.join("stage");
        let wallpapers = root.join("wallpapers");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::create_dir_all(&wallpapers).unwrap();
        std::fs::write(wallpapers.join("sonata.webp"), b"RIFF stub").unwrap();
        aoide_test_support::pin_stage(&stage);

        let out = handle_wallpaper_set(&inv(&["wallpaper", "set"], &["sonata.webp"]));
        assert_eq!(out.status, Status::Ok);
        let staged = out.data.unwrap()["wallpaperPick"].as_str().unwrap().to_string();
        assert!(
            staged.ends_with("wallpapers/sonata.webp"),
            "bare name resolved under the shared wallpapers library: {staged}"
        );
        let wallpaper = std::fs::read_to_string(stage.join("wallpaper.json")).unwrap();
        assert!(wallpaper.contains("wallpapers/sonata.webp"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn wallpaper_set_missing_file_is_error_exit_1() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("wallpaper-missing");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        aoide_test_support::pin_stage(&stage);

        let out = handle_wallpaper_set(&inv(&["wallpaper", "set"], &["nope.png"]));
        assert_eq!(out.status, Status::Error);
        assert_eq!(out.render(false).1, aoide_protocol::output::exit::ERROR);
        assert_eq!(out.data.unwrap()["reason"], "wallpaper-not-found");
        // Nothing was staged for a missing file.
        assert!(!stage.join("wallpaper.json").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn wallpaper_set_missing_arg_is_usage_exit_2() {
        let out = handle_wallpaper_set(&inv(&["wallpaper", "set"], &[]));
        assert_eq!(out.status, Status::Usage);
        assert_eq!(out.render(false).1, aoide_protocol::output::exit::USAGE);
    }

    // ── wallpaper set --clear: drop the pick, back to the rice's default ─────────

    /// `wallpaper set --clear` returns the ACTIVE rice to its own default. With a
    /// derivable wallpaper that means the rice's wallpaper comes back (as a default,
    /// never as a pick); with none it means no `wallpaper.json` at all (the baked
    /// /palette fallback, plus the rice's own live board).
    #[test]
    fn clear_drops_the_pick_and_restores_the_rices_own_default() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("wallpaper-clear");
        let stage = root.join("stage");
        let wallpapers = root.join("wallpapers");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::create_dir_all(&wallpapers).unwrap();
        std::fs::write(wallpapers.join("dusk.png"), b"\x89PNG stub").unwrap();
        aoide_test_support::pin_stage(&stage);

        // What `rice stage dusk` leaves: the rice, and dusk's own wallpaper as a
        // DEFAULT.
        std::fs::write(
            stage.join("theme.json"),
            r#"{"schemaVersion":"0","rice":"dusk"}"#,
        )
        .unwrap();
        crate::wallpaper::stage_default(&wallpapers.join("dusk.png"), "dusk").unwrap();
        assert!(!crate::wallpaper::staged_is_pick());

        // The user picks something else…
        let img = root.join("chosen.png");
        std::fs::write(&img, b"\x89PNG stub").unwrap();
        let set = handle_wallpaper_set(&inv(&["wallpaper", "set"], &[img.to_str().unwrap()]));
        assert_eq!(set.status, Status::Ok);
        assert!(crate::wallpaper::staged_is_pick());

        // …and clears it: dusk's own wallpaper is back, unmarked.
        let cleared = handle_wallpaper_set(&clear_inv(&[]));
        assert_eq!(cleared.status, Status::Ok, "{:?}", cleared.data);
        let data = cleared.data.unwrap();
        assert_eq!(data["wallpaperStage"], "default");
        assert_eq!(data["cleared"], false);
        assert!(data["wallpaperPick"].as_str().unwrap().ends_with("wallpapers/dusk.png"));
        assert!(!crate::wallpaper::staged_is_pick(), "the restored wallpaper is a default");
        assert!(cleared.changed.iter().any(|c| c.ends_with("wallpaper.json")));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A rice with NO derivable wallpaper: the clear removes the file, and a second
    /// clear is a quiet no-op rather than an error.
    #[test]
    fn clear_removes_the_file_when_the_rice_derives_none_and_is_idempotent() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("wallpaper-clear-none");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        aoide_test_support::pin_stage(&stage);

        std::fs::write(
            stage.join("theme.json"),
            r#"{"schemaVersion":"0","rice":"moonlight"}"#,
        )
        .unwrap();
        crate::wallpaper::stage_pick(std::path::Path::new("/tmp/old-choice.png"), Some("moonlight"), None)
            .unwrap();

        let cleared = handle_wallpaper_set(&clear_inv(&[]));
        assert_eq!(cleared.status, Status::Ok);
        let data = cleared.data.unwrap();
        assert_eq!(data["cleared"], true);
        assert!(data["wallpaperPick"].is_null());
        assert!(!stage.join("wallpaper.json").exists());
        assert!(cleared.changed.iter().any(|c| c.ends_with("wallpaper.json")));

        // Nothing staged any more → a second clear says so, exit 0, no change.
        let again = handle_wallpaper_set(&clear_inv(&[]));
        assert_eq!(again.status, Status::Ok);
        assert_eq!(again.data.unwrap()["cleared"], false);
        assert!(again.changed.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// With no stage file at all there is no rice to derive a default for —
    /// the clear is just the removal (the baked/palette fallback shows).
    #[test]
    fn clear_with_nothing_staged_only_removes() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("wallpaper-clear-norice");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        aoide_test_support::pin_stage(&stage);

        crate::wallpaper::stage_pick(std::path::Path::new("/tmp/old-choice.png"), None, None).unwrap();
        let out = handle_wallpaper_set(&clear_inv(&[]));
        assert_eq!(out.status, Status::Ok);
        assert_eq!(out.data.unwrap()["cleared"], true);
        assert!(!stage.join("wallpaper.json").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// `--clear` and a path are opposite intents — refused, never half-honoured.
    #[test]
    fn clear_with_a_path_is_usage_exit_2() {
        let out = handle_wallpaper_set(&clear_inv(&["sonata.webp"]));
        assert_eq!(out.status, Status::Usage);
        assert_eq!(out.render(false).1, aoide_protocol::output::exit::USAGE);
        assert_eq!(out.data.unwrap()["reason"], "clear-takes-no-path");
    }

    /// The declarative lock covers the clear too: it is a stage write like any
    /// other, and the entrypoint guard is where that policy lives.
    #[test]
    fn clear_entry_refuses_while_declarative_mode_is_locked() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("wallpaper-clear-locked");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        aoide_test_support::pin_stage(&stage);
        crate::wallpaper::stage_pick(std::path::Path::new("/tmp/old-choice.png"), None, None).unwrap();

        let out = handle_wallpaper_set_entry(&clear_inv(&[]));
        assert_eq!(out.status, Status::Error);
        assert_eq!(out.data.unwrap()["reason"], "declarative-mode-locked");
        assert!(stage.join("wallpaper.json").is_file(), "nothing cleared while locked");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The RECORD door's no-ops are not writes: a report that changed nothing
    /// mints no take, so a running provider cannot fill the take tree with the
    /// same non-event.
    #[test]
    fn from_skwd_no_op_mints_no_take_in_draft_mode() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SKWD_WALL_STANDIN"]);
        let root = unique_tmp("wallpaper-from-skwd-draft-noop");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::write(stage.join("theme.json"), VALID_NOTES).unwrap();
        std::fs::write(stage.join("wallpaper-provider"), "skwd-wall\n").unwrap();
        aoide_test_support::pin_stage(&stage);
        std::env::remove_var("AOIDE_SKWD_WALL_STANDIN");

        crate::mode::save_mode_marker(&crate::mode::ModeMarker {
            mode: crate::mode::RiceMode::Draft,
            rice: Some("moonlight".to_string()),
            draft: Some("neon-night".to_string()),
            ..Default::default()
        })
        .unwrap();

        let record = set_inv(
            &["/tmp/library/clip.mp4"],
            &[("from-skwd", "true"), ("kind", "video")],
        );
        let first = handle_wallpaper_set_entry(&record);
        assert_eq!(first.status, Status::Ok, "{:?}", first.data);
        assert_eq!(first.data.as_ref().unwrap()["recorded"], true);
        assert_eq!(first.data.as_ref().unwrap()["entry"], 1);

        // The same report again: recorded nothing, so nothing is minted.
        let again = handle_wallpaper_set_entry(&record);
        assert_eq!(again.status, Status::Ok, "{:?}", again.data);
        let data = again.data.unwrap();
        assert_eq!(data["recorded"], false);
        assert_eq!(data["unchanged"], true);
        assert!(data.get("entry").is_none(), "a no-op mints no take: {data}");
        assert_eq!(
            crate::history::list_history("moonlight", Some("neon-night")).len(),
            1,
            "the take tree records writes only"
        );

        // An ignored report is no write either.
        std::env::set_var("AOIDE_SKWD_WALL_STANDIN", "/nix/store/x-standin.png");
        let ignored = handle_wallpaper_set_entry(&set_inv(
            &["/nix/store/x-standin.png"],
            &[("from-skwd", "true"), ("kind", "static")],
        ));
        assert_eq!(ignored.status, Status::Ok);
        assert_eq!(ignored.data.unwrap()["ignored"], "standin");
        assert_eq!(
            crate::history::list_history("moonlight", Some("neon-night")).len(),
            1
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// `wallpaper set --clear`, as the registry spells it.
    fn clear_inv(args: &[&str]) -> Invocation {
        let mut i = inv(&["wallpaper", "set"], args);
        i.flags.insert("clear".into(), "true".into());
        i
    }

    // ── wallpaper set: the declarative-mode write guard (khoa 2026-08-14) ────────

    /// The lock's exemption covers a RECORD, not a clear in disguise: `--clear`
    /// with `--from-skwd` is a usage refusal, and the pick stands untouched.
    #[test]
    fn clear_with_from_skwd_is_a_usage_refusal_and_keeps_the_pick() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SKWD_WALL_STANDIN"]);
        let root = unique_tmp("wallpaper-clear-from-skwd");
        let stage = root.join("stage");
        stage_theme(&stage, Some("skwd-wall"));
        aoide_test_support::pin_stage(&stage);
        std::env::remove_var("AOIDE_SKWD_WALL_STANDIN");

        // A recorded pick first, then the locked combination.
        let recorded = handle_wallpaper_set_entry(&set_inv(
            &["/tmp/library/a.png"],
            &[("from-skwd", "true"), ("kind", "static")],
        ));
        assert_eq!(recorded.status, Status::Ok, "{:?}", recorded.data);
        let before = std::fs::read_to_string(stage.join("wallpaper.json")).unwrap();

        let both = set_inv(&[], &[("clear", "true"), ("from-skwd", "true")]);
        let out = handle_wallpaper_set_entry(&both);
        assert_eq!(out.status, Status::Usage, "{:?}", out.data);
        assert_eq!(
            out.render(false).1,
            aoide_protocol::output::exit::USAGE
        );
        assert_eq!(out.data.unwrap()["reason"], "clear-with-from-skwd");
        assert_eq!(
            std::fs::read_to_string(stage.join("wallpaper.json")).unwrap(),
            before,
            "the refusal wrote nothing"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The provider reports a scene as its workshop id; a `we:<id>` token here
    /// would be recorded AS the id and could never be applied later.
    #[test]
    fn from_skwd_refuses_a_scene_token_as_its_identity() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SKWD_WALL_STANDIN"]);
        let root = unique_tmp("wallpaper-from-skwd-token");
        let stage = root.join("stage");
        stage_theme(&stage, Some("skwd-wall"));
        aoide_test_support::pin_stage(&stage);
        std::env::remove_var("AOIDE_SKWD_WALL_STANDIN");

        let out = handle_wallpaper_set(&set_inv(
            &["we:123"],
            &[("from-skwd", "true"), ("kind", "we")],
        ));
        assert_eq!(out.status, Status::Usage);
        assert_eq!(out.data.unwrap()["reason"], "scene-token-not-an-id");
        assert!(!stage.join("wallpaper.json").exists(), "nothing recorded");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The record door is EXEMPT from the declarative lock (CONTRACTS.md §4):
    /// what an external provider is already showing gets recorded even while nix
    /// owns the declared state — refusing would leave `stage/wallpaper.json`
    /// describing something the screen is not showing. Every other spelling of
    /// this command stays refused.
    #[test]
    fn from_skwd_is_exempt_from_the_declarative_lock() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SKWD_WALL_STANDIN"]);
        let root = unique_tmp("wallpaper-from-skwd-locked");
        let stage = root.join("stage");
        stage_theme(&stage, Some("skwd-wall"));
        aoide_test_support::pin_stage(&stage);
        std::env::remove_var("AOIDE_SKWD_WALL_STANDIN");

        // No marker file at all reads as `declarative` — the locked default.
        assert_eq!(
            crate::mode::load_mode_marker().mode,
            crate::mode::RiceMode::Declarative
        );

        let recorded = handle_wallpaper_set_entry(&set_inv(
            &["/tmp/library/clip.mp4"],
            &[("from-skwd", "true"), ("kind", "video")],
        ));
        assert_eq!(recorded.status, Status::Ok, "{:?}", recorded.data);
        let data = recorded.data.unwrap();
        assert_eq!(data["recorded"], true);
        assert_eq!(data["provider"], "skwd-wall");
        let wallpaper = std::fs::read_to_string(stage.join("wallpaper.json")).unwrap();
        assert!(wallpaper.contains("\"kind\": \"video\""), "{wallpaper}");

        // The lock still holds for the pick-maker: a plain set, and a clear.
        let img = root.join("elsewhere.png");
        std::fs::write(&img, b"\x89PNG stub").unwrap();
        let plain = handle_wallpaper_set_entry(&inv(&["wallpaper", "set"], &[img.to_str().unwrap()]));
        assert_eq!(plain.status, Status::Error);
        assert_eq!(plain.data.unwrap()["reason"], "declarative-mode-locked");

        let cleared = handle_wallpaper_set_entry(&clear_inv(&[]));
        assert_eq!(cleared.status, Status::Error);
        assert_eq!(cleared.data.unwrap()["reason"], "declarative-mode-locked");
        assert_eq!(
            std::fs::read_to_string(stage.join("wallpaper.json")).unwrap(),
            wallpaper,
            "the refusal left the recorded pick exactly as it stood"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn wallpaper_set_entry_refuses_while_declarative_mode_is_locked() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("wallpaper-entry-locked");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        let img = root.join("elsewhere.png");
        std::fs::write(&img, b"\x89PNG stub").unwrap();
        aoide_test_support::pin_stage(&stage);

        let out = handle_wallpaper_set_entry(&inv(&["wallpaper", "set"], &[img.to_str().unwrap()]));
        assert_eq!(out.status, Status::Error);
        assert_eq!(out.data.unwrap()["reason"], "declarative-mode-locked");
        assert!(!stage.join("wallpaper.json").exists(), "nothing staged while locked");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn wallpaper_set_entry_allows_writes_once_staging_mode_is_unlocked() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("wallpaper-entry-unlocked");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        let img = root.join("elsewhere.png");
        std::fs::write(&img, b"\x89PNG stub").unwrap();
        aoide_test_support::pin_stage(&stage);

        crate::mode::save_mode_marker(&crate::mode::ModeMarker {
            mode: crate::mode::RiceMode::Staging,
            ..Default::default()
        })
        .unwrap();

        let out = handle_wallpaper_set_entry(&inv(&["wallpaper", "set"], &[img.to_str().unwrap()]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert!(stage.join("wallpaper.json").is_file());
        let _ = std::fs::remove_dir_all(&root);
    }

    // ── wallpaper set: the auto-take hook (phase A3) ─────────────────────────

    #[test]
    fn wallpaper_set_entry_in_draft_mode_mints_an_auto_take_on_a_real_change() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("wallpaper-autotake-fires");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::write(stage.join("theme.json"), VALID_NOTES).unwrap();
        let img = root.join("elsewhere.png");
        std::fs::write(&img, b"\x89PNG stub").unwrap();
        aoide_test_support::pin_stage(&stage);

        crate::mode::save_mode_marker(&crate::mode::ModeMarker {
            mode: crate::mode::RiceMode::Draft,
            rice: Some("moonlight".to_string()),
            draft: Some("neon-night".to_string()),
            ..Default::default()
        })
        .unwrap();

        let out = handle_wallpaper_set_entry(&inv(&["wallpaper", "set"], &[img.to_str().unwrap()]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert_eq!(out.data.as_ref().unwrap()["entry"], 1);
        assert!(out.changed.iter().any(|c| c.ends_with("history/0001.json")));
        assert!(out.changed.iter().any(|c| c.ends_with("history/head.json")));

        let record = crate::history::load_history("moonlight", Some("neon-night"), 1).unwrap();
        assert_eq!(record.cause, "wallpaper-set");
        assert!(record.wallpaper.is_some(), "the wallpaper that was just set is carried on the take");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn wallpaper_set_entry_in_draft_mode_mints_again_on_a_content_identical_reset() {
        // The pinned invariant (orchestrator correction over this step's own
        // earlier draft): a take records EVERY write, not just the ones that
        // changed something. Setting the SAME wallpaper twice in a row produces
        // byte-identical content to what take 1 already holds — it must
        // STILL mint a second take. Suppressing on no drift would make the
        // take tree an incomplete record of write events; the resulting
        // duplicate-take noise is `rice history prune`'s problem (phase A9),
        // not this hook's.
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("wallpaper-autotake-repeat");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::write(stage.join("theme.json"), VALID_NOTES).unwrap();
        let img = root.join("elsewhere.png");
        std::fs::write(&img, b"\x89PNG stub").unwrap();
        aoide_test_support::pin_stage(&stage);

        crate::mode::save_mode_marker(&crate::mode::ModeMarker {
            mode: crate::mode::RiceMode::Draft,
            rice: Some("moonlight".to_string()),
            draft: Some("neon-night".to_string()),
            ..Default::default()
        })
        .unwrap();

        let first = handle_wallpaper_set_entry(&inv(&["wallpaper", "set"], &[img.to_str().unwrap()]));
        assert_eq!(first.status, Status::Ok, "{:?}", first.data);
        assert_eq!(first.data.unwrap()["entry"], 1);

        let second = handle_wallpaper_set_entry(&inv(&["wallpaper", "set"], &[img.to_str().unwrap()]));
        assert_eq!(second.status, Status::Ok, "{:?}", second.data);
        assert_eq!(
            second.data.unwrap()["entry"], 2,
            "a content-identical wallpaper reset still mints its own take"
        );
        assert!(second.changed.iter().any(|c| c.ends_with("history/0002.json")));

        let record = crate::history::load_history("moonlight", Some("neon-night"), 2).unwrap();
        assert_eq!(record.parent, Some(1), "the second take hangs off the first");
        assert_eq!(
            crate::history::list_history("moonlight", Some("neon-night")).len(),
            2,
            "both writes are on record, even though their content is identical"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    // ── The pick's kind, and the provider's RECORD door (CONTRACTS.md §4) ────

    fn set_inv(args: &[&str], flags: &[(&str, &str)]) -> Invocation {
        let mut i = inv(&["wallpaper", "set"], args);
        for (k, v) in flags {
            i.flags.insert((*k).into(), (*v).into());
        }
        i
    }

    /// `rice stage`'s theme for the staged rice, plus — when `provider` is
    /// given — the provider name the `strillix` lane publishes beside it. Absent
    /// means the shell's own layer, which is what a host with no provider
    /// selected reports.
    fn stage_theme(stage: &std::path::Path, provider: Option<&str>) {
        std::fs::create_dir_all(stage).unwrap();
        std::fs::write(
            stage.join("theme.json"),
            r#"{"schemaVersion":"0","rice":"cadenza"}"#,
        )
        .unwrap();
        match provider {
            Some(provider) => {
                std::fs::write(stage.join("wallpaper-provider"), format!("{provider}\n")).unwrap();
            }
            None => {
                let _ = std::fs::remove_file(stage.join("wallpaper-provider"));
            }
        }
    }

    /// A `we:<id>` token is a scene, and a scene is a pick with no file to stat.
    #[test]
    fn wallpaper_set_stages_a_scene_token_as_a_scene_pick() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("wallpaper-scene-token");
        let stage = root.join("stage");
        stage_theme(&stage, None);
        aoide_test_support::pin_stage(&stage);

        let out = handle_wallpaper_set(&inv(&["wallpaper", "set"], &["we:123"]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let data = out.data.unwrap();
        assert_eq!(data["kind"], "we");
        assert_eq!(data["weId"], "123");
        assert_eq!(data["pick"], true);
        let wallpaper = std::fs::read_to_string(stage.join("wallpaper.json")).unwrap();
        assert!(wallpaper.contains("\"weId\": \"123\""), "{wallpaper}");
        assert!(!wallpaper.contains("path"), "a scene has no path: {wallpaper}");

        // An empty id is not a scene token (and not a file either).
        let out = handle_wallpaper_set(&inv(&["wallpaper", "set"], &["we:"]));
        assert_eq!(out.status, Status::Error);
        assert_eq!(out.data.unwrap()["reason"], "wallpaper-not-found");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A video file stages a VIDEO pick — kind by extension, the same rule the
    /// provider itself applies to the path.
    #[test]
    fn wallpaper_set_takes_the_kind_from_a_video_extension() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("wallpaper-video-kind");
        let stage = root.join("stage");
        stage_theme(&stage, None);
        let clip = root.join("clip.mp4");
        std::fs::write(&clip, b"\x00\x00\x00\x18ftypmp42").unwrap();
        aoide_test_support::pin_stage(&stage);

        let out = handle_wallpaper_set(&inv(&["wallpaper", "set"], &[clip.to_str().unwrap()]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert_eq!(out.data.unwrap()["kind"], "video");

        let out = handle_wallpaper_set(&inv(&["wallpaper", "set"], &["we:7"]));
        assert_eq!(out.status, Status::Ok);
        assert_eq!(out.data.unwrap()["kind"], "we");
        let _ = std::fs::remove_dir_all(&root);
    }

    // ── wallpaper set --position: how a still is laid out (CONTRACTS.md §4) ─────

    /// `--position tile` forces a tile, `fill` forbids one, and with no
    /// position a `_tile` file name tiles; the envelope says which.
    #[test]
    fn wallpaper_set_position_forces_or_follows_the_file_name() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("wallpaper-position-set");
        let stage = root.join("stage");
        stage_theme(&stage, None);
        aoide_test_support::pin_stage(&stage);
        let plain = root.join("plain.png");
        let brick = root.join("Brick_Tile.png");
        std::fs::write(&plain, b"\x89PNG stub").unwrap();
        std::fs::write(&brick, b"\x89PNG stub").unwrap();
        let staged = || -> Value {
            serde_json::from_str(&std::fs::read_to_string(stage.join("wallpaper.json")).unwrap()).unwrap()
        };

        let forced = handle_wallpaper_set(&set_inv(&[plain.to_str().unwrap()], &[("position", "tile")]));
        assert_eq!(forced.status, Status::Ok, "{:?}", forced.data);
        let data = forced.data.unwrap();
        assert_eq!((data["position"].as_str(), data["tile"].as_bool()), (Some("tile"), Some(true)));
        assert_eq!((staged()["position"].as_str(), staged()["tile"].as_bool()), (Some("tile"), Some(true)));

        let auto = handle_wallpaper_set(&inv(&["wallpaper", "set"], &[brick.to_str().unwrap()]));
        assert_eq!(auto.status, Status::Ok, "{:?}", auto.data);
        assert_eq!(auto.data.unwrap()["position"], "auto");
        assert!(staged().get("position").is_none(), "auto writes no position: {}", staged());
        assert_eq!(staged()["tile"], true, "a `_tile` name tiles");

        let fill = handle_wallpaper_set(&set_inv(&[brick.to_str().unwrap()], &[("position", "fill")]));
        assert_eq!(fill.status, Status::Ok, "{:?}", fill.data);
        assert_eq!(fill.data.unwrap()["tile"], false);
        assert_eq!(staged()["position"], "fill");
        assert!(staged().get("tile").is_none(), "a fill carries no tile: {}", staged());

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A scene or a video is never laid out by `--position`: a usage refusal at
    /// exit 2 that writes nothing, whether or not the file exists.
    #[test]
    fn wallpaper_set_position_refuses_a_scene_and_a_video() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("wallpaper-position-refused");
        let stage = root.join("stage");
        stage_theme(&stage, None);
        aoide_test_support::pin_stage(&stage);
        let clip = root.join("loop_tile.mp4");
        std::fs::write(&clip, b"\x00\x00\x00\x18ftypmp42").unwrap();

        for arg in ["we:123", clip.to_str().unwrap(), "/nowhere/clip.webm"] {
            let out = handle_wallpaper_set(&set_inv(&[arg], &[("position", "fill")]));
            assert_eq!(out.status, Status::Usage, "{arg}: {:?}", out.data);
            assert_eq!(out.render(false).1, aoide_protocol::output::exit::USAGE);
            let data = out.data.unwrap();
            assert_eq!(data["reason"], "position-not-a-still", "{arg}");
            assert_eq!(data["refusal"]["fix"]["run"], format!("strillix wallpaper set {arg}"), "{data}");
            assert!(!stage.join("wallpaper.json").exists(), "{arg}: nothing staged");
        }

        // Without the flag the same video is an ordinary pick, and never tiles.
        let out = handle_wallpaper_set(&inv(&["wallpaper", "set"], &[clip.to_str().unwrap()]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert_eq!(out.data.unwrap()["tile"], false);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The registry refuses `--position` beside `--clear` or `--from-skwd`, and
    /// any word but auto, tile or fill, before the handler writes anything.
    #[test]
    fn wallpaper_set_position_is_declared_against_clear_from_skwd_and_other_words() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("wallpaper-position-declared");
        let stage = root.join("stage");
        stage_theme(&stage, None);
        aoide_test_support::pin_stage(&stage);
        crate::mode::save_mode_marker(&crate::mode::ModeMarker {
            mode: crate::mode::RiceMode::Staging,
            ..Default::default()
        })
        .unwrap();
        let img = root.join("plain.png");
        std::fs::write(&img, b"\x89PNG stub").unwrap();
        let mut r = Registry::new();
        register(&mut r);
        let set = r.get(&["wallpaper".to_string(), "set".to_string()]).unwrap();
        let path = img.to_str().unwrap();

        for (args, flags) in [
            (vec![], vec![("clear", "true"), ("position", "tile")]),
            (vec![path], vec![("from-skwd", "true"), ("kind", "static"), ("position", "tile")]),
            (vec![path], vec![("position", "center")]),
        ] {
            let out = set.invoke("strillix", &set_inv(&args, &flags));
            assert_eq!(out.status, Status::Usage, "{flags:?}: {:?}", out.data);
            assert!(!stage.join("wallpaper.json").exists(), "{flags:?}: nothing staged");
        }

        // The argv the Desktop Pictures popup runs: `strillix wallpaper set <path> --position <word>`.
        let argv: Vec<String> = ["wallpaper", "set", path, "--position", "tile"].iter().map(|w| w.to_string()).collect();
        let (parsed, _) = aoide_protocol::door::parse(&argv, aoide_protocol::Door::Cli, "strillix", &r)
            .unwrap_or_else(|o| panic!("{}", o.message));
        assert_eq!((parsed.args.as_slice(), parsed.flags.get("position").map(String::as_str)), ([path.to_string()].as_slice(), Some("tile")));
        let ok = set.invoke("strillix", &parsed);
        assert_eq!(ok.status, Status::Ok, "{:?}", ok.data);
        assert_eq!(ok.data.unwrap()["tile"], true);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The provider's own report is RECORDED for the staged rice — and the
    /// envelope says so.
    #[test]
    fn from_skwd_records_the_providers_pick_for_the_staged_rice() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SKWD_WALL_STANDIN"]);
        let root = unique_tmp("wallpaper-from-skwd");
        let stage = root.join("stage");
        stage_theme(&stage, Some("skwd-wall"));
        aoide_test_support::pin_stage(&stage);
        std::env::remove_var("AOIDE_SKWD_WALL_STANDIN");

        let out = handle_wallpaper_set(&set_inv(
            &["/tmp/library/clip.mp4"],
            &[("from-skwd", "true"), ("kind", "video")],
        ));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let data = out.data.unwrap();
        assert_eq!(data["recorded"], true);
        assert_eq!(data["kind"], "video");
        assert_eq!(data["provider"], "skwd-wall");
        assert_eq!(data["rice"], "cadenza");
        let wallpaper = std::fs::read_to_string(stage.join("wallpaper.json")).unwrap();
        assert!(wallpaper.contains("\"kind\": \"video\""), "{wallpaper}");
        assert!(wallpaper.contains("\"pick\": true"), "{wallpaper}");
        assert!(wallpaper.contains("\"rice\": \"cadenza\""), "{wallpaper}");

        // A scene records the id and no path.
        let out = handle_wallpaper_set(&set_inv(&["123"], &[("from-skwd", "true"), ("kind", "we")]));
        assert_eq!(out.status, Status::Ok);
        assert_eq!(out.data.unwrap()["recorded"], true);
        let wallpaper = std::fs::read_to_string(stage.join("wallpaper.json")).unwrap();
        assert!(wallpaper.contains("\"weId\": \"123\""), "{wallpaper}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The shell's own layer paints this rice: the report is ignored, and
    /// nothing is written.
    #[test]
    fn from_skwd_is_ignored_when_the_shell_paints() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SKWD_WALL_STANDIN"]);
        let root = unique_tmp("wallpaper-from-skwd-shell");
        let stage = root.join("stage");
        stage_theme(&stage, None);
        aoide_test_support::pin_stage(&stage);
        std::env::remove_var("AOIDE_SKWD_WALL_STANDIN");

        let out = handle_wallpaper_set(&set_inv(
            &["/tmp/library/a.png"],
            &[("from-skwd", "true"), ("kind", "static")],
        ));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let data = out.data.unwrap();
        assert_eq!(data["recorded"], false);
        assert_eq!(data["ignored"], "provider");
        assert_eq!(data["provider"], "quickshell");
        assert!(!stage.join("wallpaper.json").exists(), "nothing recorded");

        // …and the same with no rice staged at all.
        std::fs::remove_file(stage.join("theme.json")).unwrap();
        let out = handle_wallpaper_set(&set_inv(
            &["/tmp/library/a.png"],
            &[("from-skwd", "true"), ("kind", "static")],
        ));
        assert_eq!(out.data.unwrap()["ignored"], "provider");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The step-aside image is not a pick: with the provider up and the stand-in
    /// applied, the report is dropped — that is what keeps "the provider steps aside"
    /// from being recorded as "the user picked a transparent 1x1".
    #[test]
    fn from_skwd_ignores_the_standin_image() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SKWD_WALL_STANDIN"]);
        let root = unique_tmp("wallpaper-from-skwd-standin");
        let stage = root.join("stage");
        stage_theme(&stage, Some("skwd-wall"));
        aoide_test_support::pin_stage(&stage);
        std::env::set_var("AOIDE_SKWD_WALL_STANDIN", "/nix/store/x-standin.png");

        let out = handle_wallpaper_set(&set_inv(
            &["/nix/store/x-standin.png"],
            &[("from-skwd", "true"), ("kind", "static")],
        ));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let data = out.data.unwrap();
        assert_eq!(data["recorded"], false);
        assert_eq!(data["ignored"], "standin");
        assert!(!stage.join("wallpaper.json").exists(), "nothing recorded");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The loop breaker: a report that says what the stage already holds writes
    /// nothing, byte for byte — the provider's hook can fire as often as the
    /// provider applies and the file never moves after the first one.
    #[test]
    fn from_skwd_is_a_no_op_when_the_pick_already_matches() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SKWD_WALL_STANDIN"]);
        let root = unique_tmp("wallpaper-from-skwd-noop");
        let stage = root.join("stage");
        stage_theme(&stage, Some("skwd-wall"));
        aoide_test_support::pin_stage(&stage);
        std::env::remove_var("AOIDE_SKWD_WALL_STANDIN");

        let first = handle_wallpaper_set(&set_inv(
            &["/tmp/library/a.png"],
            &[("from-skwd", "true"), ("kind", "static")],
        ));
        assert_eq!(first.data.unwrap()["recorded"], true);
        let before = std::fs::read_to_string(stage.join("wallpaper.json")).unwrap();

        let again = handle_wallpaper_set(&set_inv(
            &["/tmp/library/a.png"],
            &[("from-skwd", "true"), ("kind", "static")],
        ));
        assert_eq!(again.status, Status::Ok, "{:?}", again.data);
        let data = again.data.unwrap();
        assert_eq!(data["recorded"], false);
        assert_eq!(data["unchanged"], true);
        assert!(again.changed.is_empty(), "a no-op reports no changed path");
        assert_eq!(
            std::fs::read_to_string(stage.join("wallpaper.json")).unwrap(),
            before,
            "the file did not move"
        );

        // A DIFFERENT kind for the same identity is a different pick: recorded.
        let moved = handle_wallpaper_set(&set_inv(
            &["/tmp/library/a.png"],
            &[("from-skwd", "true"), ("kind", "video")],
        ));
        assert_eq!(moved.data.unwrap()["recorded"], true);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The flag's own contract: the three kinds, and an identity, or a usage
    /// refusal — a provider hook that spelled its arguments wrong must be a
    /// loud failure and never a silent record of nothing.
    #[test]
    fn from_skwd_requires_a_known_kind_and_an_identity() {
        let missing_kind = handle_wallpaper_set(&set_inv(&["/tmp/a.png"], &[("from-skwd", "true")]));
        assert_eq!(missing_kind.status, Status::Usage);
        assert_eq!(missing_kind.data.unwrap()["reason"], "missing-kind");

        let unknown =
            handle_wallpaper_set(&set_inv(&["/tmp/a.png"], &[("from-skwd", "true"), ("kind", "still")]));
        assert_eq!(unknown.status, Status::Usage);
        assert_eq!(unknown.data.unwrap()["reason"], "unknown-kind");

        let no_identity =
            handle_wallpaper_set(&set_inv(&[], &[("from-skwd", "true"), ("kind", "static")]));
        assert_eq!(no_identity.status, Status::Usage);
        assert_eq!(no_identity.data.unwrap()["reason"], "missing-identity");

        let blank =
            handle_wallpaper_set(&set_inv(&["   "], &[("from-skwd", "true"), ("kind", "static")]));
        assert_eq!(blank.status, Status::Usage);
    }
}
