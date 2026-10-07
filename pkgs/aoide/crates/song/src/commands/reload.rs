//! `lyra reload` — the one mode-aware iteration command of the agent rice
//! loop (design settled by the User, 2026-08-31: edit → `lyra reload` →
//! look, no separate save, no separate sync, no tree-direction trap). Reads
//! `stage/mode.json` (the authority) and dispatches:
//!
//! - **declarative** — SHELL RELOAD ONLY, byte-for-byte the old `quickshell
//!   reload` command ([`shell_reload_only`]), which this command absorbs
//!   outright — hard cutover, no alias, the `node invite` precedent. Nothing
//!   is unlocked, so there is nothing to snapshot or sync.
//! - **staging** — 1. sync via [`super::rice::handle_rice_stage`]'s own body
//!   (the existing seam, never a copy: re-derives `stage/livery.json` from
//!   the committed songbook, syncs widget bodies + the widget-type registry
//!   into `run/qml`, best-effort hyprctl); 2. snapshot the now-synced stage
//!   + widget bodies as a take, deduped against the head
//!   (`commands/take.rs`'s `snapshot_if_identical_to_head`, hanging off
//!   `songbook/<song>/takes/`); 3. shell reload.
//! - **draft** — the same shape, but [`sync_draft_in_place`] stands in for
//!   `handle_rice_stage`: `stage/livery.json` is ALREADY the draft's own
//!   live content via its routing symlink (`commands/mode.rs`'s own doc —
//!   every writer, including a hand-edit, lands straight in the draft), so
//!   re-deriving it from the COMMITTED songbook the way `handle_rice_stage`
//!   does would silently clobber the very edits draft mode exists to hold,
//!   on every single reload. Draft's sync instead applies the SAME
//!   live-apply + widget/registry-sync tail (`crate::live`/`crate::widgets`,
//!   the identical primitives `handle_rice_stage` itself calls) against the
//!   CURRENT staged content, touching the content of `stage/livery.json` not
//!   at all. The take hangs off the routed draft
//!   (`songbook/<song>/drafts/<name>/takes/`, where takes already live).
//!   Widget bodies are SONG-scoped, not draft-scoped (a draft forks the dress
//!   — livery+cover — never the widgets), so a widget edit under draft mode
//!   mutates every draft's view alike. Before the sync the arm checks the
//!   routing itself: the activation seed RENAMES the declared file over
//!   `stage/livery.json` (the draft's own file is never written through), so
//!   a link that is not the one to the marked draft's `livery.json` is
//!   re-routed ([`super::mode::route_stage_to_draft`], the block `rice mode
//!   draft` routes with) and, after the sync, the SAME marker is saved again
//!   so `stage/mode.json` lands last — the shell's livery watch re-arms on a
//!   `mode.json` change, never on a swapped entry. A draft whose file is gone
//!   is refused by name and nothing is written.
//!
//! `lyra reload` is also what brings a staged or drafted song back after a
//! login or a switch: the activation lays declared state over the runtime
//! tree, and the lyra lane runs this command from its `aoide-rice-reload` unit
//! to put back whatever `stage/mode.json` names.
//!
//! **Sync runs BEFORE snapshot in both arms** — not the order the beats are
//! numbered in casual description, but load-bearing for dedupe-against-head:
//! `handle_rice_stage`'s own write is a pure, deterministic function of the
//! committed songbook (same input, same "song"-field-injected output, every
//! call), so a snapshot taken AFTER it settles into a STABLE value across
//! repeated no-op reloads — a snapshot taken BEFORE it would instead capture
//! that injection itself as "drift" on every single call, defeating dedupe
//! entirely for Staging. Still strictly before the shell reload (beat 3),
//! which is all `rice back`'s reversibility promise ever needed.
//!
//! Snapshot-before-reload makes every iteration reversible via `rice back`
//! for free — the agent loop's undo comes with the verb the agent already
//! runs.

use aoide_protocol::Invocation;
use aoide_protocol::output::{Fix, Kind, Outcome, Status};
use aoide_protocol::registry::{cmd, Registry};
use aoide_storage::mode::{self, RiceMode};
use serde_json::{json, Value};

pub fn register(r: &mut Registry) {
    r.insert(cmd!(
        path: ["reload"],
        summary: "The one mode-aware iteration command: reads the rice mode and reloads accordingly. Declarative shell-reloads only (byte for byte the old `quickshell reload`, which this absorbed). Staging/draft snapshot the current rice — deduped against the head take, so an unchanged reload mints nothing — sync it (`rice stage`'s own body), then shell-reload. Snapshot-before-reload makes every dress iteration revertible via `rice back` for free (widget bodies are captured in the take but revert via git, their own substrate).",
        args: [],
        flags: [],
        gated: false,
        implemented: true,
        handler: handle_reload,
        brief: "Reload the live rice, honoring the rice mode.",
    ));
}

fn handle_reload(inv: &Invocation) -> Outcome {
    let marker = mode::load_mode_marker();
    match marker.mode {
        RiceMode::Declarative => shell_reload_only(),
        RiceMode::Staging | RiceMode::Draft => reload_staging_or_draft(inv, marker),
    }
}

/// The declarative arm — byte-for-byte [`crate::commands::quickshell`]'s old
/// `handle_quickshell_reload`: best-effort, always `Outcome::ok` regardless
/// of whether the IPC call actually reached a live instance. Nothing is
/// unlocked to save in this mode, so this is the WHOLE arm — no sync, no
/// snapshot (the measure is closed).
fn shell_reload_only() -> Outcome {
    let status = crate::ipc::quickshell_ipc_reload();
    Outcome::ok("reload", status.message()).with_data(json!({ "status": status.tag() }))
}

/// The staging/draft arm: sync → snapshot (deduped) → shell reload — see
/// the module doc for why sync runs FIRST. Resolves "the current rice" off
/// the mode marker's own `song` field — populated in both `Staging` and
/// `Draft` (`aoide_storage::mode::ModeMarker`'s own doc) — never re-guesses
/// it a second way.
fn reload_staging_or_draft(inv: &Invocation, marker: mode::ModeMarker) -> Outcome {
    let Some(song) = marker.song.clone() else {
        return Outcome::error(
            "reload",
            "no rice currently staged or drafted — nothing to reload \
             (`aoide rice mode stage` or `aoide rice mode draft <name>` first)",
        )
        .with_data(json!({ "reason": "not-staged-or-drafted" }));
    };
    let draft = marker.draft.as_deref();

    // Beat "sync": Staging re-derives declared content from the committed
    // songbook via `handle_rice_stage`'s own body (the existing seam, never
    // a copy). Draft applies the same live-apply + widget/registry-sync
    // tail WITHOUT touching `stage/livery.json`'s content — see the module
    // doc for why that split is load-bearing, not cosmetic — after making
    // sure the file still routes to the draft.
    let (mut synced, rerouted) = match draft {
        None => (
            super::rice::handle_rice_stage(&Invocation {
                path: vec!["rice".to_string(), "stage".to_string()],
                args: vec![song.clone()],
                flags: inv.flags.clone(),
                door: inv.door,
            }),
            false,
        ),
        Some(draft) => match route_to_marked_draft(&song, draft) {
            Ok(rerouted) => (sync_draft_in_place(&song), rerouted),
            Err(refused) => return refused,
        },
    };
    // The entry's type changed, so the marker follows it — whether or not the
    // sync went on to succeed (CONTRACTS.md §4, `stage/mode.json`).
    if rerouted {
        if let Err(e) = mode::save_mode_marker(&marker) {
            return Outcome::error("reload", format!("failed to write mode marker: {e}"))
                .with_data(json!({ "reason": "marker-write-failed" }));
        }
    }
    if synced.status != Status::Ok {
        synced.command = "reload".to_string();
        return synced;
    }
    let mut changed = synced.changed.clone();
    if rerouted {
        changed.push(aoide_storage::fs::stage_dir().join("livery.json").to_string_lossy().into_owned());
        changed.push(mode::mode_marker_path().to_string_lossy().into_owned());
    }

    // Beat "snapshot": the now-synced stage + widget bodies, deduped against
    // the head — `commands/take.rs`'s own dedupe core (the User's own
    // settled rule, 2026-08-31), reused verbatim rather than reimplemented
    // here.
    let take_data = match super::take::snapshot_if_identical_to_head("reload", "reload") {
        Ok(Some(record)) => {
            changed.push(
                aoide_storage::takes::take_path(&song, draft, record.take)
                    .to_string_lossy()
                    .into_owned(),
            );
            changed.push(
                aoide_storage::takes::head_path(&song, draft)
                    .to_string_lossy()
                    .into_owned(),
            );
            json!({ "take": record.take, "deduped": false })
        }
        Ok(None) => json!({ "take": Value::Null, "deduped": true }),
        Err(err) => json!({ "take": Value::Null, "takeError": err.message }),
    };

    // Beat "reload": shell reload — unconditional, the same call the
    // declarative arm makes. Staging's own `handle_rice_stage` call above
    // already fires its OWN IPC reload internally, but only when widget
    // bodies changed (its bandwidth-saving optimization for its OTHER
    // callers, `rice stage <name>` run standalone — `commands/rice.rs`'s own
    // doc). `lyra reload` means "show me the current state now", so this
    // beat always runs regardless of what that internal call already
    // attempted — a second IPC reload is a harmless no-op, never a
    // correctness problem (`quickshell_ipc_reload` is idempotent by
    // construction, `ipc.rs`'s own doc).
    let status = crate::ipc::quickshell_ipc_reload();

    Outcome::ok(
        "reload",
        format!(
            "reloaded `{song}` ({}) — {}",
            super::mode::mode_word(marker.mode),
            status.message()
        ),
    )
    .changed(changed)
    .with_data(json!({
        "mode": super::mode::mode_word(marker.mode),
        "song": song,
        "draft": marker.draft,
        "take": take_data,
        "sync": synced.data,
        "reload": { "status": status.tag(), "message": status.message() },
    }))
}

/// Make `stage/livery.json` the link to the marked draft's `livery.json`, as
/// `rice mode draft` left it. `Ok(true)` when it had to be re-routed — the
/// activation seed renames a declared file over the link — `Ok(false)` when the
/// link was already right. A draft whose file is gone is refused by name
/// before anything is written, dangling link or not: re-routing to it would
/// leave a link to nothing.
fn route_to_marked_draft(song: &str, draft: &str) -> Result<bool, Outcome> {
    let want = aoide_storage::fs::draft_dir(song, draft).join("livery.json");
    if !want.is_file() {
        return Err(Outcome::refuse(
            "reload",
            Kind::Refused,
            format!("draft `{draft}` of `{song}` is gone"),
            format!("stage/mode.json routes to {}, which does not exist", want.display()),
            Fix::Run("lyra rice mode stage".to_string()),
        ));
    }
    let link = aoide_storage::fs::stage_dir().join("livery.json");
    if std::fs::read_link(&link).is_ok_and(|to| to == want) {
        return Ok(false);
    }
    super::mode::route_stage_to_draft(&want)
        .map(|()| true)
        .map_err(|e| Outcome::error("reload", e).with_data(json!({ "reason": "symlink-setup-failed" })))
}

/// Draft mode's own "sync" beat: apply hyprctl geometry/border keywords and
/// the terminal colours (both derived from the CURRENT staged livery —
/// already the draft's own content via its routing symlink) and sync
/// widget bodies + the widget-type registry into `run/qml` — the SAME primitives
/// [`super::rice::handle_rice_stage`] itself calls (`crate::live`/
/// `crate::widgets`), minus the "read the committed songbook and (re)write
/// `stage/livery.json`" step that function opens with. That step is
/// Staging-only: see this module's own doc for why reusing it here would
/// clobber the draft.
fn sync_draft_in_place(song: &str) -> Outcome {
    let stage = aoide_storage::fs::stage_dir();
    let livery_path = stage.join("livery.json");
    let raw = match std::fs::read_to_string(&livery_path) {
        Ok(s) => s,
        Err(e) => {
            return Outcome::error(
                "reload",
                format!("nothing staged to reload: cannot read {} ({e})", livery_path.display()),
            )
            .with_data(json!({ "reason": "no-staged-livery", "expected": livery_path.to_string_lossy() }));
        }
    };
    let parsed: Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            return Outcome::error("reload", format!("staged livery.json is not valid JSON: {e}"))
                .with_data(json!({ "reason": "invalid-json", "livery": livery_path.to_string_lossy() }));
        }
    };

    let hyprctl_status = crate::live::apply_live(&crate::live::geometry_keywords(&parsed));
    let (terminal_changed, terminal_data) =
        super::rice::stage_terminal_colors(crate::live::terminal_colors(&parsed).as_deref());

    // The §7.5 gate, once per reload: the same decision `rice stage` makes,
    // for the same reason — `sync_draft_in_place` writes into `stage/` and
    // `run/qml`, so it must refuse BEFORE either.
    let songbook = match crate::widgets::plan_stage(song) {
        Ok(songbook) => songbook,
        Err(e) => {
            return Outcome::error("reload", e.error)
                .with_data(json!({ "reason": "stage-refused", "target": e.target }));
        }
    };

    let widget_sync = match crate::widgets::sync_song_widgets(song, &songbook) {
        Ok(sync) => sync,
        Err(e) => {
            return Outcome::error("reload", format!("failed to sync widget bodies: {}", e.error))
                .with_data(json!({ "reason": "widget-sync-failed", "target": e.target }));
        }
    };
    let registry_sync = match crate::widgets::sync_song_registry(song, &songbook) {
        Ok(sync) => sync,
        Err(e) => {
            return Outcome::error("reload", format!("failed to sync widget-type registry: {}", e.error))
                .with_data(json!({ "reason": "registry-sync-failed", "target": e.target }));
        }
    };

    let mut changed = terminal_changed;
    changed.extend(widget_sync.changed.clone());
    changed.extend(registry_sync.changed.clone());

    Outcome::ok("reload", format!("synced `{song}`'s draft in place — {}", widget_sync.note))
        .changed(changed)
        .with_data(json!({
            "hyprctl": hyprctl_status,
            "terminal": terminal_data,
            "widgets": widget_sync.note,
            "registry": registry_sync.note,
        }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_storage::fs as shellbridge;
    use aoide_storage::mode::{save_mode_marker, ModeMarker};
    use aoide_test_support::*;
    use std::os::unix::fs::MetadataExt;
    use std::path::{Path, PathBuf};

    const VALID_NOTES: &str = r##"{"schemaVersion":"0","palette":{"bg":"#000000"}}"##;

    /// What the activation seed leaves at `stage/livery.json`: another song's
    /// declared document, as a plain file.
    const DECLARED_NOTES: &str = r##"{"schemaVersion":"0","palette":{"bg":"#222222"},"song":"nocturne"}"##;

    const ROUTED_NOTES: &str = r##"{"schemaVersion":"0","palette":{"bg":"#abcdef"},"song":"sonata"}"##;

    /// `<root>/aoide/song/stage`: the layout under which `run_qml_dir()` lands
    /// inside the test's own root as well.
    fn runtime_stage(tag: &str) -> (PathBuf, PathBuf) {
        let root = unique_tmp(tag);
        let stage = root.join("aoide").join("song").join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        (root, stage)
    }

    /// A drafted `sonata` whose draft holds [`ROUTED_NOTES`], routed by a real
    /// link, with the marker saved as `rice mode draft` would. Returns the
    /// draft's `livery.json`.
    fn route_a_draft(stage: &Path) -> PathBuf {
        let draft_livery = shellbridge::draft_dir("sonata", "neon-night").join("livery.json");
        std::fs::create_dir_all(draft_livery.parent().unwrap()).unwrap();
        std::fs::write(&draft_livery, ROUTED_NOTES).unwrap();
        std::os::unix::fs::symlink(&draft_livery, stage.join("livery.json")).unwrap();
        save_mode_marker(&ModeMarker {
            mode: RiceMode::Draft,
            song: Some("sonata".to_string()),
            draft: Some("neon-night".to_string()),
            ..Default::default()
        })
        .unwrap();
        draft_livery
    }

    /// The activation seed's write: `mv -f` of a temp file over the entry.
    fn rename_declared_over(stage: &Path) {
        let tmp = stage.join(".livery.json.seed");
        std::fs::write(&tmp, DECLARED_NOTES).unwrap();
        std::fs::rename(&tmp, stage.join("livery.json")).unwrap();
    }

    fn reload_inv() -> Invocation {
        aoide_test_support::inv(&["reload"], &[])
    }

    /// Declarative is the safe default (no marker file at all IS
    /// declarative) — always `Ok`, whatever the live IPC attempt actually
    /// resolved to (`not-running`/`failed`/`reloaded` are reported facts,
    /// same posture the old `quickshell reload` command had). Deliberately
    /// doesn't assert WHICH tag: that depends on whether
    /// `aoide-quickshell.service` happens to be live on the machine running
    /// this test.
    #[test]
    fn declarative_mode_is_shell_reload_only_and_always_ok() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        let root = unique_tmp("reload-declarative");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        crate::commands::test_support::ensure_default_songbook_fixture();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        let out = handle_reload(&reload_inv());
        assert_eq!(out.status, Status::Ok);
        assert_eq!(out.command, "reload");
        let data = out.data.unwrap();
        assert!(data["status"].is_string(), "{data:?}");
        assert!(data.get("mode").is_none(), "declarative arm carries no mode/song/take payload");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Mode dispatch is pure off `mode.json` — this is the "declarative
    /// refuses to snapshot/sync" half of that dispatch made concrete: no
    /// `songbook/<song>/takes/` directory is ever created for a declarative
    /// reload, because the declarative arm never calls the snapshot core at
    /// all.
    #[test]
    fn declarative_mode_never_mints_a_take() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        let root = unique_tmp("reload-declarative-no-take");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        crate::commands::test_support::ensure_default_songbook_fixture();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        save_mode_marker(&ModeMarker {
            mode: RiceMode::Declarative,
            song: Some("sonata".to_string()),
            ..Default::default()
        })
        .unwrap();

        handle_reload(&reload_inv());
        assert!(
            !aoide_storage::takes::takes_dir("sonata", None).is_dir(),
            "declarative reload must never create a takes/ directory"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Staging routing: a reload while staged mints its take under
    /// `songbook/<song>/takes/` — the sibling-of-`drafts/` root, never
    /// nested under a draft that doesn't exist in this mode — and syncs +
    /// reloads successfully.
    #[test]
    fn staging_mode_reload_takes_off_the_song_and_succeeds() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let root = unique_tmp("reload-staging-routing");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        crate::commands::test_support::ensure_default_songbook_fixture();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        let songbook = shellbridge::songbook_dir("sonata");
        std::fs::create_dir_all(&songbook).unwrap();
        std::fs::write(songbook.join("livery.json"), VALID_NOTES).unwrap();
        std::fs::write(stage.join("livery.json"), VALID_NOTES).unwrap();

        save_mode_marker(&ModeMarker {
            mode: RiceMode::Staging,
            song: Some("sonata".to_string()),
            staging_song: Some("sonata".to_string()),
            ..Default::default()
        })
        .unwrap();

        let out = handle_reload(&reload_inv());
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert_eq!(out.data.as_ref().unwrap()["take"]["deduped"], false);
        assert_eq!(out.data.as_ref().unwrap()["take"]["take"], 1);

        let record = aoide_storage::takes::load_take("sonata", None, 1)
            .expect("take 1 lives under songbook/sonata/takes/, not a draft");
        assert_eq!(record.cause, "reload");
        assert!(
            !shellbridge::song_drafts_dir("sonata").join("takes").is_dir(),
            "a staging-mode take must never be nested under drafts/"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Dedupe against head: a second reload with no intervening edit mints
    /// nothing, reusing `take diff`'s own key-wise machinery
    /// (`snapshot_if_identical_to_head`) rather than a text comparison — the
    /// User's own settled rule, 2026-08-31.
    #[test]
    fn staging_mode_reload_dedupes_against_an_unchanged_head() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let root = unique_tmp("reload-staging-dedupe");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        crate::commands::test_support::ensure_default_songbook_fixture();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        let songbook = shellbridge::songbook_dir("sonata");
        std::fs::create_dir_all(&songbook).unwrap();
        std::fs::write(songbook.join("livery.json"), VALID_NOTES).unwrap();
        std::fs::write(stage.join("livery.json"), VALID_NOTES).unwrap();

        save_mode_marker(&ModeMarker {
            mode: RiceMode::Staging,
            song: Some("sonata".to_string()),
            staging_song: Some("sonata".to_string()),
            ..Default::default()
        })
        .unwrap();

        let first = handle_reload(&reload_inv());
        assert_eq!(first.status, Status::Ok, "{:?}", first.data);
        assert_eq!(first.data.as_ref().unwrap()["take"]["take"], 1);

        let second = handle_reload(&reload_inv());
        assert_eq!(second.status, Status::Ok, "{:?}", second.data);
        assert_eq!(second.data.as_ref().unwrap()["take"]["deduped"], true, "{:?}", second.data);
        assert_eq!(
            aoide_storage::takes::list_takes("sonata", None).len(),
            1,
            "an unchanged second reload must not mint a second take"
        );

        // Changing the SONGBOOK (the real staging-mode edit target — sync
        // re-derives `stage/livery.json` from it every call, so editing the
        // stage directly would just be clobbered straight back by the next
        // sync) is real drift — the third reload must mint again, proving
        // the dedupe compares content, not "reload was called before".
        std::fs::write(
            songbook.join("livery.json"),
            r##"{"schemaVersion":"0","palette":{"bg":"#111111"}}"##,
        )
        .unwrap();
        let third = handle_reload(&reload_inv());
        assert_eq!(third.status, Status::Ok, "{:?}", third.data);
        assert_eq!(third.data.as_ref().unwrap()["take"]["deduped"], false, "{:?}", third.data);
        assert_eq!(aoide_storage::takes::list_takes("sonata", None).len(), 2);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Draft routing: a reload while drafted mints its take off the DRAFT,
    /// not the song root — the two scopes never collide.
    ///
    /// The regression this guards: an earlier version of this command's
    /// draft-mode sync called `handle_rice_stage` directly — the SAME
    /// function the staging arm uses, which reads the COMMITTED songbook
    /// and writes it into `stage/livery.json`. While routed into a draft,
    /// that path is a SYMLINK into the draft file (`commands/mode.rs`'s own
    /// doc), so `atomic_write`'s symlink transparency means that write would
    /// land straight in the draft — silently clobbering it back to plain
    /// declared content on every single reload. This test routes a REAL
    /// symlink (mirroring `rice mode draft`'s own mechanism) and gives the
    /// committed songbook DIFFERENT content from the draft, so a clobber
    /// would be caught immediately: the draft's own content must survive.
    #[test]
    fn draft_mode_reload_takes_off_the_draft_and_never_clobbers_it() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let root = unique_tmp("reload-draft-routing");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        crate::commands::test_support::ensure_default_songbook_fixture();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        let songbook = shellbridge::songbook_dir("sonata");
        std::fs::create_dir_all(&songbook).unwrap();
        // Deliberately DIFFERENT from the draft's own content below — the
        // committed truth a clobber would silently overwrite the draft with.
        std::fs::write(songbook.join("livery.json"), VALID_NOTES).unwrap();

        let draft_dir = shellbridge::draft_dir("sonata", "neon-night");
        std::fs::create_dir_all(&draft_dir).unwrap();
        const DRAFT_NOTES: &str = r##"{"schemaVersion":"0","palette":{"bg":"#abcdef"}}"##;
        std::fs::write(draft_dir.join("livery.json"), DRAFT_NOTES).unwrap();

        let stage_livery = stage.join("livery.json");
        let _ = std::fs::remove_file(&stage_livery);
        std::os::unix::fs::symlink(draft_dir.join("livery.json"), &stage_livery).unwrap();

        save_mode_marker(&ModeMarker {
            mode: RiceMode::Draft,
            song: Some("sonata".to_string()),
            draft: Some("neon-night".to_string()),
            ..Default::default()
        })
        .unwrap();

        let marker_inode = std::fs::metadata(mode::mode_marker_path()).unwrap().ino();
        let out = handle_reload(&reload_inv());
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert_eq!(out.data.as_ref().unwrap()["take"]["take"], 1);
        assert_eq!(
            std::fs::metadata(mode::mode_marker_path()).unwrap().ino(),
            marker_inode,
            "a link that was already right is not a swap: mode.json is not rewritten"
        );

        let record = aoide_storage::takes::load_take("sonata", Some("neon-night"), 1)
            .expect("the take lives under the draft");
        assert_eq!(
            record.livery,
            serde_json::from_str::<serde_json::Value>(DRAFT_NOTES).unwrap(),
            "the captured take is the DRAFT's content, not the songbook's"
        );
        assert!(
            aoide_storage::takes::list_takes("sonata", None).is_empty(),
            "and NOT under the song's own staging-mode takes/ root"
        );

        let after = std::fs::read_to_string(&stage_livery).unwrap();
        assert_eq!(
            after, DRAFT_NOTES,
            "reload must never overwrite the draft's own live content with the committed songbook"
        );
        assert!(
            std::fs::symlink_metadata(&stage_livery).unwrap().file_type().is_symlink(),
            "the routing symlink itself must survive a reload untouched"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The activation seed renames a declared file over the routing link —
    /// the draft's own file is untouched, the link is gone. A reload brings
    /// the link back to the marked draft and writes `mode.json` after it.
    #[test]
    fn draft_mode_reload_restores_the_link_the_seed_renamed_over() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, stage) = runtime_stage("reload-draft-heal");
        crate::commands::test_support::ensure_default_songbook_fixture();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        let draft_livery = route_a_draft(&stage);
        rename_declared_over(&stage);
        let stage_livery = stage.join("livery.json");
        assert!(
            !std::fs::symlink_metadata(&stage_livery).unwrap().file_type().is_symlink(),
            "the seed's rename replaced the link"
        );
        assert_eq!(std::fs::read_to_string(&draft_livery).unwrap(), ROUTED_NOTES, "and left the draft alone");
        let marker_inode = std::fs::metadata(mode::mode_marker_path()).unwrap().ino();

        let out = handle_reload(&reload_inv());
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);

        assert_eq!(std::fs::read_link(&stage_livery).unwrap(), draft_livery);
        assert_eq!(std::fs::read_to_string(&stage_livery).unwrap(), ROUTED_NOTES);
        assert_eq!(std::fs::read_to_string(&draft_livery).unwrap(), ROUTED_NOTES);
        let marker = std::fs::metadata(mode::mode_marker_path()).unwrap();
        assert_ne!(marker.ino(), marker_inode, "mode.json was rewritten");
        assert!(
            marker.modified().unwrap() >= std::fs::symlink_metadata(&stage_livery).unwrap().modified().unwrap(),
            "and after the link was made"
        );
        assert!(out.changed.iter().any(|c| c.ends_with("stage/livery.json")), "{:?}", out.changed);
        assert!(out.changed.iter().any(|c| c.ends_with("stage/mode.json")), "{:?}", out.changed);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A draft that is gone is refused by name, whatever sits at the entry —
    /// the declared file the seed left, or a link to the vanished file — and
    /// nothing is written: no link made, no entry touched, the marker kept.
    #[test]
    fn draft_mode_reload_refuses_a_vanished_draft_and_writes_nothing() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, stage) = runtime_stage("reload-draft-gone");
        crate::commands::test_support::ensure_default_songbook_fixture();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        let draft_livery = route_a_draft(&stage);
        std::fs::remove_dir_all(draft_livery.parent().unwrap()).unwrap();
        let stage_livery = stage.join("livery.json");
        let marker_before = std::fs::read(mode::mode_marker_path()).unwrap();

        rename_declared_over(&stage);
        let out = handle_reload(&reload_inv());
        assert_eq!(out.status, Status::Error, "{:?}", out.data);
        assert_eq!(out.status.exit_code(), 1);
        let refusal = &out.data.as_ref().unwrap()["refusal"];
        assert_eq!(refusal["kind"], "refused");
        assert!(refusal["what"].as_str().unwrap().contains("`neon-night` of `sonata`"), "{refusal}");
        assert!(refusal["why"].as_str().unwrap().contains(&draft_livery.display().to_string()), "{refusal}");
        assert_eq!(refusal["fix"]["run"], "lyra rice mode stage");
        assert_eq!(std::fs::read_to_string(&stage_livery).unwrap(), DECLARED_NOTES);
        assert!(
            !std::fs::symlink_metadata(&stage_livery).unwrap().file_type().is_symlink(),
            "no link to a file that does not exist"
        );
        assert_eq!(std::fs::read(mode::mode_marker_path()).unwrap(), marker_before);

        std::fs::remove_file(&stage_livery).unwrap();
        std::os::unix::fs::symlink(&draft_livery, &stage_livery).unwrap();
        let out = handle_reload(&reload_inv());
        assert_eq!(out.status, Status::Error, "{:?}", out.data);
        assert!(out.data.as_ref().unwrap()["refusal"]["what"].is_string(), "{:?}", out.data);
        assert_eq!(std::fs::read_link(&stage_livery).unwrap(), draft_livery);
        assert_eq!(std::fs::read(mode::mode_marker_path()).unwrap(), marker_before);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Declarative reloads nothing but the shell: whatever the seed laid at
    /// `stage/livery.json` stays as it is.
    #[test]
    fn declarative_mode_reload_leaves_the_stage_livery_alone() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        let (root, stage) = runtime_stage("reload-declarative-livery");
        crate::commands::test_support::ensure_default_songbook_fixture();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        rename_declared_over(&stage);
        save_mode_marker(&ModeMarker {
            mode: RiceMode::Declarative,
            song: Some("sonata".to_string()),
            ..Default::default()
        })
        .unwrap();

        let out = handle_reload(&reload_inv());
        assert_eq!(out.status, Status::Ok);
        let stage_livery = stage.join("livery.json");
        assert_eq!(std::fs::read_to_string(&stage_livery).unwrap(), DECLARED_NOTES);
        assert!(!std::fs::symlink_metadata(&stage_livery).unwrap().file_type().is_symlink());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// An activation lays the declared song over the runtime: the stage file
    /// names another song, `rsync --delete` drops the staged song's widget
    /// bodies, and `manifest.json` goes back to the baked one. A staging
    /// reload puts all three back.
    #[test]
    fn staging_mode_reload_restores_the_staged_song_after_an_activation() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_SESSION_ID",
            crate::widgets::SONGBOOK_EVAL_FIXTURE_VAR,
        ]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, stage) = runtime_stage("reload-staging-activation");
        let fixture = root.join("songbook-eval-fixture.json");
        std::fs::write(
            &fixture,
            r#"{"manifest":{"sonata":{"bar":{"owner":"sonata","file":"bar.qml"}}},"registry":{}}"#,
        )
        .unwrap();
        std::env::set_var(crate::widgets::SONGBOOK_EVAL_FIXTURE_VAR, &fixture);
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        let songbook = shellbridge::songbook_dir("sonata");
        std::fs::create_dir_all(songbook.join("widgets")).unwrap();
        std::fs::write(songbook.join("livery.json"), VALID_NOTES).unwrap();
        std::fs::write(songbook.join("widgets").join("bar.qml"), "// bar\n").unwrap();
        save_mode_marker(&ModeMarker {
            mode: RiceMode::Staging,
            song: Some("sonata".to_string()),
            staging_song: Some("sonata".to_string()),
            ..Default::default()
        })
        .unwrap();
        let stage_livery = stage.join("livery.json");
        let songs = shellbridge::run_qml_dir().join("songs");
        let staged_song = || -> Value {
            serde_json::from_str::<Value>(&std::fs::read_to_string(&stage_livery).unwrap()).unwrap()["song"].clone()
        };
        let manifest_has_sonata = || -> bool {
            serde_json::from_str::<Value>(&std::fs::read_to_string(songs.join("manifest.json")).unwrap())
                .unwrap()
                .get("sonata")
                .is_some()
        };

        let baked_manifest = r#"{"nocturne":{"bar":{"owner":"nocturne","file":"bar.qml"}}}"#;
        std::fs::create_dir_all(&songs).unwrap();
        std::fs::write(songs.join("manifest.json"), baked_manifest).unwrap();

        let first = handle_reload(&reload_inv());
        assert_eq!(first.status, Status::Ok, "{:?}", first.data);
        assert_eq!(staged_song(), "sonata");
        assert_eq!(std::fs::read_to_string(songs.join("sonata").join("bar.qml")).unwrap(), "// bar\n");
        assert!(manifest_has_sonata());

        rename_declared_over(&stage);
        std::fs::remove_dir_all(songs.join("sonata")).unwrap();
        std::fs::write(songs.join("manifest.json"), baked_manifest).unwrap();
        assert_eq!(staged_song(), "nocturne");
        assert!(!manifest_has_sonata());

        let second = handle_reload(&reload_inv());
        assert_eq!(second.status, Status::Ok, "{:?}", second.data);
        assert_eq!(staged_song(), "sonata");
        assert_eq!(std::fs::read_to_string(songs.join("sonata").join("bar.qml")).unwrap(), "// bar\n");
        assert!(manifest_has_sonata());
        assert!(!std::fs::symlink_metadata(&stage_livery).unwrap().file_type().is_symlink());
        let _ = std::fs::remove_dir_all(&root);
    }
}
