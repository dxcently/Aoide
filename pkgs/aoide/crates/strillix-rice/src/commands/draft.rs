//! `rice draft {save,list,drop}` — durable scratch snapshots of the live
//! stage (concepts/Self-Ricing's draft extension, the User 2026-08-14; symlink
//! routing + `rice mode draft`, the User 2026-08-14).
//!
//! A draft is a saved snapshot living at
//! `rices/<rice>/drafts/<name>/theme.json` (+ `wallpaper.json` when the
//! stage had one) — nested under the rice it varies, not a flat top-level
//! dir: a draft is fundamentally a variation of an ALREADY COMPOSED rice, so
//! it belongs inside that rice's own directory, not a separate global
//! namespace. Gitignored, outside `stage/`, and never committed/declared
//! truth (that distinction from `rices/<name>/`'s own committed
//! files is the entire point of this feature; `lib/checks.nix`'s
//! `noRiceRead` bans the nested `drafts/` path at nix-eval time, same
//! treatment as `stage/`).
//!
//! **Reaching a draft is `rice mode draft <name>`'s job now**
//! (`commands/mode.rs::handle_mode_draft`), not this module's — it points
//! `stage/theme.json` at a SYMLINK into the draft's own file, so every
//! future writer (`rice stage`, `wallpaper set`, a hand-edit, Quickshell's own
//! FileView reload) transparently lands in the draft with zero
//! draft-awareness anywhere, because [`aoide_storage::fs::atomic_write`] is
//! symlink-transparent. This module owns only the drafts THEMSELVES:
//!
//! - **`rice draft save [<name>]`** — fork whatever's currently live into a
//!   NEW or updated draft snapshot, independent of switching modes (e.g.
//!   preserving a moment as a second draft while still working in a first
//!   one, or saving a snapshot while in plain `Staging` without ever
//!   entering `Draft` mode). Upserts. Never touches `mode.json` — it doesn't
//!   change what's currently routed/active, only what's saved. With no
//!   `<name>` it mints the lowest free `draft-<n>` for the staged rice
//!   ([`mint_draft_name`]) and returns it as `data.name`: the one place a
//!   draft name is minted, so a surface that offers "new draft" asks here
//!   instead of inventing a name of its own.
//!   [`fork_stage_into`] is the reusable core; `rice mode draft <name>`'s own
//!   "create the draft if it doesn't exist yet" step calls the SAME
//!   function rather than reimplementing the read/write.
//! - **`rice draft list [<rice>]`** — enumerate saved drafts.
//! - **`rice draft drop <name>`** — delete a draft outright. Refuses if it's
//!   the draft the CURRENT `Draft` mode is routed to (rather than silently
//!   also tearing down the symlink and falling back to `Staging` — a `drop`
//!   that quietly also changes your mode would be the more surprising
//!   choice; `rice mode stage`/`rice mode declarative` are how you leave
//!   `Draft` mode, explicitly, same as always).
//!
//! There used to be a fourth command, `rice draft stage <name>` (copy-based:
//! read the draft, atomic-write it into the stage as a one-shot snapshot).
//! It's gone — fully superseded by `rice mode draft`'s symlink routing, and
//! keeping both would be two spellings of "go live with this draft" (the
//! no-internal-aliases rule).
//!
//! `mode.json`'s `draft` field is `Some(name)` **if and only if**
//! `mode == Draft` — see `crates/storage/src/mode.rs` for the marker shape
//! and `commands/mode.rs` for how `handle_mode_draft`/`handle_mode_stage`/
//! `handle_mode_declarative` together keep that invariant true across every
//! mode transition.

use aoide_protocol::Invocation;
use aoide_protocol::output::Outcome;
use aoide_protocol::registry::{arg, cmd, Registry};
use aoide_storage::fs as shellbridge;
use crate::mode::{load_mode_marker, RiceMode};
use serde_json::{json, Value};
use aoide_protocol::Door;

pub fn register(r: &mut Registry) {
    r.insert(cmd!(
        path: ["rice", "draft", "save"],
        summary: "Fork the current stage (stage/theme.json, required; stage/wallpaper.json if present) into rices/<rice>/drafts/<name>/ — <rice> is the currently staged rice. With no <name>, mints the lowest free `draft-<n>` for it. Upserts; independent of mode (never switches modes).",
        args: [arg!("name", "string", false, "Draft name; omit it to mint the lowest free `draft-<n>` for the staged rice.")],
        flags: [],
        gated: false,
        implemented: true,
        handler: handle_draft_save,
        examples: ["rice draft save", "rice draft save neon-night"],
    ));
    r.insert(cmd!(
        path: ["rice", "draft", "list"],
        summary: "List saved drafts: name, rice, saved-at, and whether it's the currently-loaded draft (mode.json is in Draft mode, routed to it). No arg walks every rice's drafts/; <rice> scopes to just that rice.",
        args: [arg!("rice", "string", false, "Scope the listing to one rice's drafts; omit to list every rice's.")],
        flags: [],
        gated: false,
        implemented: true,
        handler: handle_draft_list,
        doors: &[Door::Cli, Door::Daemon, Door::Mcp],
    ));
    r.insert(cmd!(
        path: ["rice", "draft", "drop"],
        summary: "Delete a saved draft of the currently staged rice. Errors if the name doesn't exist (not idempotent), or if it's the draft `rice mode draft` currently has the stage routed to (switch modes first).",
        args: [arg!("name", "string", true, "Draft name to delete, from the current rice's drafts/.")],
        flags: [],
        gated: false,
        implemented: true,
        handler: handle_draft_drop,
    ));
}

/// The shared "no rice is currently staged, nothing to resolve a draft
/// directory under" error — [`handle_draft_drop`] has no other reason to
/// touch `stage/theme.json`, so it resolves the rice purely through
/// [`super::mode::current_staged_rice`] and reports this single reason on
/// failure (a fresh box, or a stage file with no `"rice"` breadcrumb, are
/// the only ways to hit it).
fn no_resolvable_rice(cmd: &str) -> Outcome {
    Outcome::error(
        cmd,
        "no rice is currently staged — nothing to resolve a draft directory under \
         (stage a rice first: `strillix rice stage <name>` or `strillix rice mode stage <name>`)",
    )
    .with_data(json!({ "reason": "no-resolvable-rice" }))
}

/// `rice draft save [<name>]` — fork the current stage into
/// `rices/<rice>/drafts/<name>/`, where `<rice>` is whatever
/// `stage/theme.json`'s own `"rice"` field names right now. `stage/theme.json`
/// is required (error if absent/unparseable/rice-less — there is nothing to
/// snapshot, or nowhere to nest it, without one). The rice resolves BEFORE the
/// name because a minted name is the lowest free one of THAT rice's drafts.
///
/// Reads + validates the stage itself (rather than delegating to
/// [`fork_stage_into`] for that part) so a missing file, invalid JSON, and a
/// valid-but-rice-less file each keep their own distinct, specific error
/// reason; the actual write is [`fork_stage_into`], shared with `rice mode
/// draft`'s fork-on-first-entry step.
///
/// Not gated by `rice mode declarative`, and never touches `mode.json` —
/// it only reads the stage (transparently through a `Draft`-mode symlink if
/// one is currently routed there) and writes outside it
/// (`rices/<rice>/drafts/`, never `stage/`), so it works in any mode
/// without changing what's currently active.
fn handle_draft_save(inv: &Invocation) -> Outcome {
    let theme_src = crate::paths::stage_dir().join("theme.json");
    let raw = match std::fs::read_to_string(&theme_src) {
        Ok(s) => s,
        Err(e) => {
            return Outcome::error(
                "rice.draft.save",
                format!("nothing staged to save: cannot read {} ({e})", theme_src.display()),
            )
            .with_data(json!({
                "reason": "no-staged-theme",
                "expected": theme_src.to_string_lossy(),
            }));
        }
    };
    let parsed: Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            return Outcome::error(
                "rice.draft.save",
                format!("staged theme.json is not valid JSON: {e}"),
            )
            .with_data(json!({ "reason": "invalid-json", "theme": theme_src.to_string_lossy() }));
        }
    };
    let rice = match parsed.get("rice").and_then(Value::as_str) {
        Some(s) => s.to_string(),
        None => {
            return Outcome::error(
                "rice.draft.save",
                "staged theme.json carries no `rice` field — nothing to nest this draft under \
                 (stage a named rice first: `strillix rice stage <name>`)",
            )
            .with_data(json!({ "reason": "no-resolvable-rice" }));
        }
    };

    let name = inv.args.first().cloned().unwrap_or_else(|| mint_draft_name(&rice));
    if !crate::compose::valid_rice_name(&name) {
        return Outcome::error(
            "rice.draft.save",
            format!(
                "`{name}` is not a valid draft name: must match `^[a-z0-9][a-z0-9-]*$` \
                 (lowercase letters, digits, hyphens; no leading hyphen, no `/`, no `..`)"
            ),
        )
        .with_data(json!({ "reason": "invalid-name", "name": name }));
    }

    fork_stage_into(&rice, &name)
}

/// `draft-<n>` for the lowest positive `n` no entry of `<rice>`'s `drafts/`
/// is already named. Any entry counts as taken, a directory with no
/// `theme.json` included, so a half-written draft is never overwritten; an
/// absent `drafts/` is empty. Candidates carry no leading zero, so `draft-01`,
/// `draft-0` and `draft-x` never collide with one.
fn mint_draft_name(rice: &str) -> String {
    let taken: Vec<String> = std::fs::read_dir(crate::paths::rice_drafts_dir(rice))
        .map(|read| {
            read.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    let mut n = 1;
    loop {
        let name = format!("draft-{n}");
        if !taken.contains(&name) {
            return name;
        }
        n += 1;
    }
}

/// The reusable core of a draft snapshot: atomic-write the CURRENT
/// `stage/theme.json` into `rices/<rice>/drafts/<name>/theme.json`
/// (reading it fresh — transparently through a `Draft`-mode symlink if one
/// is currently routed there, same as any other reader), mirroring
/// `stage/wallpaper.json` in if present, or removing a stale draft wallpaper if the
/// current stage no longer has one (a draft always mirrors the stage
/// exactly at save/fork time, never a merge of old + new).
///
/// `pub(crate)`, not private: [`handle_draft_save`] (above) and `rice mode
/// draft <name>`'s "create the draft if it doesn't exist yet" step
/// (`commands/mode.rs::handle_mode_draft`) both call this for the identical
/// read/write, rather than each reimplementing it. Takes `rice`/`name`
/// already resolved — callers own how they got there (`handle_draft_save`
/// derives it from the stage's own `"rice"` field with distinct error
/// reasons; `handle_mode_draft` already has it via
/// [`super::mode::current_staged_rice`]).
pub(crate) fn fork_stage_into(rice: &str, name: &str) -> Outcome {
    let stage = crate::paths::stage_dir();
    let theme_src = stage.join("theme.json");
    let raw = match std::fs::read_to_string(&theme_src) {
        Ok(s) => s,
        Err(e) => {
            return Outcome::error(
                "rice.draft.save",
                format!("nothing staged to fork: cannot read {} ({e})", theme_src.display()),
            )
            .with_data(json!({
                "reason": "no-staged-theme",
                "expected": theme_src.to_string_lossy(),
            }));
        }
    };
    if let Err(e) = serde_json::from_str::<Value>(&raw) {
        return Outcome::error(
            "rice.draft.save",
            format!("staged theme.json is not valid JSON: {e}"),
        )
        .with_data(json!({ "reason": "invalid-json", "theme": theme_src.to_string_lossy() }));
    }

    let dir = crate::paths::draft_dir(rice, name);
    let theme_dst = dir.join("theme.json");
    if let Err(e) = shellbridge::atomic_write(&theme_dst, &raw) {
        return Outcome::error(
            "rice.draft.save",
            format!("failed to write {}: {e}", theme_dst.display()),
        )
        .with_data(json!({ "reason": "write-failed", "target": theme_dst.to_string_lossy() }));
    }
    let mut changed: Vec<String> = vec![theme_dst.to_string_lossy().into_owned()];

    let wallpaper_src = stage.join("wallpaper.json");
    let wallpaper_dst = dir.join("wallpaper.json");
    let wallpaper_path = if wallpaper_src.is_file() {
        match std::fs::read_to_string(&wallpaper_src) {
            Ok(wallpaper_raw) => {
                if let Err(e) = shellbridge::atomic_write(&wallpaper_dst, &wallpaper_raw) {
                    return Outcome::error(
                        "rice.draft.save",
                        format!("failed to write {}: {e}", wallpaper_dst.display()),
                    )
                    .changed(changed)
                    .with_data(json!({ "reason": "write-failed", "target": wallpaper_dst.to_string_lossy() }));
                }
                changed.push(wallpaper_dst.to_string_lossy().into_owned());
                Some(wallpaper_dst.to_string_lossy().into_owned())
            }
            Err(e) => {
                return Outcome::error(
                    "rice.draft.save",
                    format!("failed to read staged wallpaper.json: {e}"),
                )
                .changed(changed)
                .with_data(json!({ "reason": "wallpaper-read-failed" }));
            }
        }
    } else {
        if wallpaper_dst.exists() {
            let _ = std::fs::remove_file(&wallpaper_dst);
        }
        None
    };

    Outcome::ok(
        "rice.draft.save",
        format!(
            "saved draft `{name}` for `{rice}` — {} file(s) snapshotted under {}",
            changed.len(),
            dir.display()
        ),
    )
    .changed(changed)
    .with_data(json!({
        "name": name,
        "rice": rice,
        "theme": theme_dst.to_string_lossy(),
        "wallpaperPick": wallpaper_path,
    }))
}

/// `rice draft list [<rice>]` — enumerate saved drafts. With `<rice>`, scans
/// only `rices/<rice>/drafts/*/theme.json`; with no arg, walks every
/// rice under `rices/` and reports drafts from all of them. Each entry
/// carries its rice (structural — the rices dir it was found under),
/// name, saved-at (the theme.json file's mtime), and whether it is the
/// currently-loaded draft — `mode.json`'s `rice`+`draft` pair, which is only
/// ever set together (`draft` is non-null iff `mode == Draft`). A directory
/// with no `theme.json` is not a draft and is silently skipped. An
/// absent/empty scope is `ok` with an empty list — never an error.
fn handle_draft_list(inv: &Invocation) -> Outcome {
    let scope = inv.args.first().cloned();
    let marker = load_mode_marker();

    let rices: Vec<String> = match &scope {
        Some(s) => vec![s.clone()],
        None => {
            let root = crate::paths::strillix_root().join("rices");
            std::fs::read_dir(&root)
                .map(|read| {
                    let mut names: Vec<String> = read
                        .flatten()
                        .filter(|e| e.path().is_dir())
                        .map(|e| e.file_name().to_string_lossy().into_owned())
                        .collect();
                    names.sort();
                    names
                })
                .unwrap_or_default()
        }
    };

    let mut entries: Vec<((String, String), Value)> = Vec::new();
    for rice in &rices {
        let dir = crate::paths::rice_drafts_dir(rice);
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let theme = path.join("theme.json");
            if !theme.is_file() {
                continue; // no theme.json — not a draft, silently skipped
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let saved_at = std::fs::metadata(&theme)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| aoide_storage::time::iso_utc_from_epoch(d.as_secs() as i64));
            let is_current =
                marker.rice.as_deref() == Some(rice.as_str()) && marker.draft.as_deref() == Some(name.as_str());
            entries.push((
                (rice.clone(), name.clone()),
                json!({
                    "rice": rice,
                    "name": name,
                    "savedAt": saved_at,
                    "current": is_current,
                }),
            ));
        }
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let drafts: Vec<Value> = entries.into_iter().map(|(_, v)| v).collect();

    Outcome::ok("rice.draft.list", format!("{} draft(s)", drafts.len()))
        .with_data(json!({ "drafts": drafts, "scope": scope }))
}

/// `rice draft drop <name>` — delete the CURRENTLY staged rice's
/// `rices/<rice>/drafts/<name>/` outright. Missing draft → error
/// `draft-not-found` (NOT idempotent-silent, unlike this codebase's usual
/// tolerate-missing marker discipline — a named drop of nothing is almost
/// certainly a typo the caller should see).
///
/// Refuses (`draft-is-live`) if it's the draft `rice mode draft` currently
/// has `stage/theme.json` symlinked into — chosen over the alternative
/// (drop it AND fall back to plain `Staging`) because a `drop` that also
/// silently changes your mode and re-pins the stage is the more surprising
/// behavior; `rice mode stage`/`rice mode declarative` are the explicit,
/// already-documented way to leave `Draft` mode first.
fn handle_draft_drop(inv: &Invocation) -> Outcome {
    let name = inv.args.first().cloned().unwrap_or_default();
    if !crate::compose::valid_rice_name(&name) {
        return Outcome::error(
            "rice.draft.drop",
            format!(
                "`{name}` is not a valid draft name: must match `^[a-z0-9][a-z0-9-]*$` \
                 (lowercase letters, digits, hyphens; no leading hyphen, no `/`, no `..`)"
            ),
        )
        .with_data(json!({ "reason": "invalid-name", "name": name }));
    }
    let rice = match super::mode::current_staged_rice() {
        Some(s) => s,
        None => return no_resolvable_rice("rice.draft.drop"),
    };

    let existing = load_mode_marker();
    if existing.mode == RiceMode::Draft
        && existing.rice.as_deref() == Some(rice.as_str())
        && existing.draft.as_deref() == Some(name.as_str())
    {
        return Outcome::error(
            "rice.draft.drop",
            format!(
                "cannot drop `{name}` — it's the live draft for `{rice}` \
                 (stage/theme.json is routed to it); switch modes first \
                 (`strillix rice mode stage` or `strillix rice mode declarative`), then drop it"
            ),
        )
        .with_data(json!({ "reason": "draft-is-live", "name": name, "rice": rice }));
    }

    let dir = crate::paths::draft_dir(&rice, &name);
    if !dir.is_dir() {
        return Outcome::error(
            "rice.draft.drop",
            format!("no draft `{name}` for `{rice}` at {}", dir.display()),
        )
        .with_data(json!({
            "reason": "draft-not-found",
            "name": name,
            "rice": rice,
            "expected": dir.to_string_lossy(),
        }));
    }
    if let Err(e) = std::fs::remove_dir_all(&dir) {
        return Outcome::error(
            "rice.draft.drop",
            format!("failed to remove {}: {e}", dir.display()),
        )
        .with_data(json!({ "reason": "remove-failed", "target": dir.to_string_lossy() }));
    }

    Outcome::ok("rice.draft.drop", format!("dropped draft `{name}` for `{rice}`"))
        .changed(vec![dir.to_string_lossy().into_owned()])
        .with_data(json!({ "name": name, "rice": rice }))
}

// ── Tests (rice draft save/list/drop) ────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use aoide_protocol::output::Status;
    use crate::mode::{save_mode_marker, ModeMarker};
    use aoide_test_support::*;

    /// Every test's stage carries a `"rice"` breadcrumb — `save`/`drop` both
    /// resolve their target rice off it now, so a bare `VALID_NOTES` (which
    /// has no `"rice"` field) needs this written first.
    fn stage_with_rice(stage: &std::path::Path, rice: &str) {
        std::fs::write(
            stage.join("theme.json"),
            format!(
                r##"{{"schemaVersion":"0","rice":"{rice}","palette":{{"bg":"#0b1021","fg":"#c8d3f5","accent":"#82aaff","urgent":"#ff757f"}}}}"##
            ),
        )
        .unwrap();
    }

    // ── rice draft save ──────────────────────────────────────────────────

    #[test]
    fn save_creates_the_draft_dir_nested_under_its_rice_and_files() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("draft-save-ok");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        stage_with_rice(&stage, "sonata");
        std::fs::write(stage.join("wallpaper.json"), r#"{"path":"x.png"}"#).unwrap();
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        let out = handle_draft_save(&inv(&["rice", "draft", "save"], &["neon-night"]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let dir = root.join("rices").join("sonata").join("drafts").join("neon-night");
        let staged = std::fs::read_to_string(stage.join("theme.json")).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("theme.json")).unwrap(), staged);
        assert_eq!(
            std::fs::read_to_string(dir.join("wallpaper.json")).unwrap(),
            r#"{"path":"x.png"}"#
        );
        assert!(out
            .changed
            .iter()
            .any(|c| c.ends_with("rices/sonata/drafts/neon-night/theme.json")));
        assert!(out
            .changed
            .iter()
            .any(|c| c.ends_with("rices/sonata/drafts/neon-night/wallpaper.json")));
        let data = out.data.unwrap();
        assert_eq!(data["name"], "neon-night");
        assert_eq!(data["rice"], "sonata");
        assert!(data["wallpaperPick"].as_str().unwrap().ends_with("wallpaper.json"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn save_without_a_staged_theme_errors() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let stage = unique_tmp("draft-save-nostage").join("stage");
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        let out = handle_draft_save(&inv(&["rice", "draft", "save"], &["neon-night"]));
        assert_eq!(out.status, Status::Error);
        assert_eq!(out.data.unwrap()["reason"], "no-staged-theme");
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn save_with_a_riceless_staged_theme_errors_no_resolvable_rice() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let stage = unique_tmp("draft-save-norice").join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        // VALID_NOTES parses fine but carries no "rice" field.
        std::fs::write(stage.join("theme.json"), VALID_NOTES).unwrap();
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        let out = handle_draft_save(&inv(&["rice", "draft", "save"], &["neon-night"]));
        assert_eq!(out.status, Status::Error);
        assert_eq!(out.data.unwrap()["reason"], "no-resolvable-rice");
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn save_upserts_an_existing_draft() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("draft-save-upsert");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        stage_with_rice(&stage, "sonata");
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        handle_draft_save(&inv(&["rice", "draft", "save"], &["neon-night"]));

        let updated = r##"{"schemaVersion":"0","rice":"sonata","palette":{"bg":"#111111"}}"##;
        std::fs::write(stage.join("theme.json"), updated).unwrap();
        let out = handle_draft_save(&inv(&["rice", "draft", "save"], &["neon-night"]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let dir = root.join("rices").join("sonata").join("drafts").join("neon-night");
        assert_eq!(
            std::fs::read_to_string(dir.join("theme.json")).unwrap(),
            updated,
            "re-saving overwrites, not appends/merges"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn save_clears_a_stale_wallpaper_when_the_current_stage_has_none() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("draft-save-stale-wallpaper");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        stage_with_rice(&stage, "sonata");
        std::fs::write(stage.join("wallpaper.json"), r#"{"path":"x.png"}"#).unwrap();
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        handle_draft_save(&inv(&["rice", "draft", "save"], &["neon-night"]));
        let dir = root.join("rices").join("sonata").join("drafts").join("neon-night");
        assert!(dir.join("wallpaper.json").is_file());

        std::fs::remove_file(stage.join("wallpaper.json")).unwrap();
        let out = handle_draft_save(&inv(&["rice", "draft", "save"], &["neon-night"]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert!(!dir.join("wallpaper.json").exists(), "stale wallpaper removed on re-save");
        assert!(out.data.unwrap()["wallpaperPick"].is_null());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn saving_one_draft_never_touches_another() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("draft-save-independent");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        stage_with_rice(&stage, "sonata");
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        handle_draft_save(&inv(&["rice", "draft", "save"], &["amber-dusk"]));
        let updated = r##"{"schemaVersion":"0","rice":"sonata","palette":{"bg":"#222222"}}"##;
        std::fs::write(stage.join("theme.json"), updated).unwrap();
        handle_draft_save(&inv(&["rice", "draft", "save"], &["neon-night"]));

        let drafts = root.join("rices").join("sonata").join("drafts");
        assert!(
            std::fs::read_to_string(drafts.join("amber-dusk").join("theme.json"))
                .unwrap()
                .contains("#0b1021"),
            "amber-dusk is untouched by saving neon-night"
        );
        assert_eq!(
            std::fs::read_to_string(drafts.join("neon-night").join("theme.json")).unwrap(),
            updated
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn drafts_of_two_different_rices_never_collide_even_with_the_same_name() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("draft-save-cross-rice");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        stage_with_rice(&stage, "sonata");
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);
        handle_draft_save(&inv(&["rice", "draft", "save"], &["neon-night"]));

        stage_with_rice(&stage, "moonlight");
        handle_draft_save(&inv(&["rice", "draft", "save"], &["neon-night"]));

        assert!(root
            .join("rices")
            .join("sonata")
            .join("drafts")
            .join("neon-night")
            .join("theme.json")
            .is_file());
        assert!(root
            .join("rices")
            .join("moonlight")
            .join("drafts")
            .join("neon-night")
            .join("theme.json")
            .is_file());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn save_reads_transparently_through_a_symlinked_stage() {
        // The "preserve a moment as a second draft while still working in a
        // first one" scenario `rice mode draft` enables: if `stage/theme.json`
        // is CURRENTLY a symlink (Draft mode routed into some other draft),
        // `rice draft save <new-name>` still reads the live content fine —
        // std::fs::read_to_string follows symlinks transparently, same as
        // any other reader — and forks it into a brand-new draft.
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("draft-save-through-symlink");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        let existing_dir = root.join("rices").join("sonata").join("drafts").join("neon-night");
        std::fs::create_dir_all(&existing_dir).unwrap();
        let routed_notes = r##"{"schemaVersion":"0","rice":"sonata","palette":{"bg":"#routed"}}"##;
        std::fs::write(existing_dir.join("theme.json"), routed_notes).unwrap();
        std::os::unix::fs::symlink(&existing_dir.join("theme.json"), stage.join("theme.json")).unwrap();
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        let out = handle_draft_save(&inv(&["rice", "draft", "save"], &["amber-dusk"]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let forked = root.join("rices").join("sonata").join("drafts").join("amber-dusk");
        assert_eq!(std::fs::read_to_string(forked.join("theme.json")).unwrap(), routed_notes);
        // The symlink itself, and what it routes to, are both untouched.
        assert!(std::fs::symlink_metadata(stage.join("theme.json")).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(existing_dir.join("theme.json")).unwrap(), routed_notes);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn save_with_no_name_mints_draft_1_for_a_rice_with_no_drafts() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("draft-mint-first");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        stage_with_rice(&stage, "sonata");
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        let out = handle_draft_save(&inv(&["rice", "draft", "save"], &[]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert_eq!(out.data.unwrap()["name"], "draft-1");
        let drafts = root.join("rices").join("sonata").join("drafts");
        assert!(drafts.join("draft-1").join("theme.json").is_file());

        let again = handle_draft_save(&inv(&["rice", "draft", "save"], &[]));
        assert_eq!(again.data.unwrap()["name"], "draft-2");
        assert!(drafts.join("draft-1").join("theme.json").is_file(), "the first mint is never overwritten");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn minting_fills_the_lowest_gap() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("draft-mint-gap");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        stage_with_rice(&stage, "sonata");
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);
        let drafts = root.join("rices").join("sonata").join("drafts");
        for taken in ["draft-1", "draft-3"] {
            std::fs::create_dir_all(drafts.join(taken)).unwrap();
        }

        let out = handle_draft_save(&inv(&["rice", "draft", "save"], &[]));
        assert_eq!(out.data.unwrap()["name"], "draft-2");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn minting_ignores_every_name_that_is_not_draft_n() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("draft-mint-ignores");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        stage_with_rice(&stage, "sonata");
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);
        let drafts = root.join("rices").join("sonata").join("drafts");
        for other in ["draft-0", "draft-01", "draft-x", "neon-night"] {
            std::fs::create_dir_all(drafts.join(other)).unwrap();
        }

        let out = handle_draft_save(&inv(&["rice", "draft", "save"], &[]));
        assert_eq!(out.data.unwrap()["name"], "draft-1");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn minting_counts_a_directory_with_no_theme_as_taken() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("draft-mint-halfwritten");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        stage_with_rice(&stage, "sonata");
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);
        let half = root.join("rices").join("sonata").join("drafts").join("draft-1");
        std::fs::create_dir_all(&half).unwrap();
        std::fs::write(half.join("wallpaper.json"), "{}").unwrap();

        let out = handle_draft_save(&inv(&["rice", "draft", "save"], &[]));
        assert_eq!(out.data.unwrap()["name"], "draft-2");
        assert!(!half.join("theme.json").exists(), "the half-written draft is left alone");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn minting_counts_only_the_staged_rices_drafts() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("draft-mint-per-rice");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        stage_with_rice(&stage, "sonata");
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);
        std::fs::create_dir_all(root.join("rices").join("moonlight").join("drafts").join("draft-1")).unwrap();

        let out = handle_draft_save(&inv(&["rice", "draft", "save"], &[]));
        assert_eq!(out.data.unwrap()["name"], "draft-1");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn save_with_no_name_and_no_staged_rice_refuses_and_creates_nothing() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("draft-mint-norice");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::write(stage.join("theme.json"), VALID_NOTES).unwrap();
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        let out = handle_draft_save(&inv(&["rice", "draft", "save"], &[]));
        assert_eq!(out.status, Status::Error);
        assert_eq!(out.data.unwrap()["reason"], "no-resolvable-rice");
        assert!(!root.join("rices").exists(), "nothing is created for a refused mint");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn save_returns_the_name_whether_it_was_given_or_minted() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("draft-mint-data-name");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        stage_with_rice(&stage, "sonata");
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        let given = handle_draft_save(&inv(&["rice", "draft", "save"], &["neon"])).data.unwrap();
        let minted = handle_draft_save(&inv(&["rice", "draft", "save"], &[])).data.unwrap();
        assert_eq!(given["name"], "neon");
        assert_eq!(minted["name"], "draft-1");
        assert_eq!(given["rice"], "sonata");
        assert_eq!(minted["rice"], "sonata");
        let _ = std::fs::remove_dir_all(&root);
    }

    // ── rice draft list ──────────────────────────────────────────────────

    #[test]
    fn list_with_no_rices_at_all_is_ok_with_an_empty_list() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let stage = unique_tmp("draft-list-absent").join("stage");
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        let out = handle_draft_list(&inv(&["rice", "draft", "list"], &[]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert!(out.data.unwrap()["drafts"].as_array().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn list_scoped_to_a_rice_with_no_drafts_is_ok_with_an_empty_list() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("draft-list-scoped-empty");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::create_dir_all(root.join("rices").join("sonata")).unwrap();
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        let out = handle_draft_list(&inv(&["rice", "draft", "list"], &["sonata"]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert!(out.data.unwrap()["drafts"].as_array().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn list_surfaces_multiple_drafts_of_one_rice_with_the_current_one_flagged() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("draft-list-multi");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        stage_with_rice(&stage, "sonata");
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        handle_draft_save(&inv(&["rice", "draft", "save"], &["amber-dusk"]));
        handle_draft_save(&inv(&["rice", "draft", "save"], &["neon-night"]));

        save_mode_marker(&ModeMarker {
            mode: RiceMode::Draft,
            rice: Some("sonata".to_string()),
            draft: Some("neon-night".to_string()),
            ..Default::default()
        })
        .unwrap();

        let out = handle_draft_list(&inv(&["rice", "draft", "list"], &["sonata"]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let drafts = out.data.unwrap()["drafts"].as_array().unwrap().clone();
        assert_eq!(drafts.len(), 2);
        let neon = drafts.iter().find(|d| d["name"] == "neon-night").unwrap();
        assert_eq!(neon["current"], true);
        assert_eq!(neon["rice"], "sonata");
        assert!(neon["savedAt"].is_string());
        let amber = drafts.iter().find(|d| d["name"] == "amber-dusk").unwrap();
        assert_eq!(amber["current"], false);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn list_with_no_arg_walks_every_rice_and_scoping_by_name_narrows_it() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("draft-list-allrices");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        stage_with_rice(&stage, "sonata");
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);
        handle_draft_save(&inv(&["rice", "draft", "save"], &["neon-night"]));

        stage_with_rice(&stage, "moonlight");
        handle_draft_save(&inv(&["rice", "draft", "save"], &["amber-dusk"]));

        let out_all = handle_draft_list(&inv(&["rice", "draft", "list"], &[]));
        assert_eq!(out_all.status, Status::Ok, "{:?}", out_all.data);
        let all = out_all.data.unwrap()["drafts"].as_array().unwrap().clone();
        assert_eq!(all.len(), 2, "both rices' drafts surface with no scope");
        assert!(all.iter().any(|d| d["rice"] == "sonata" && d["name"] == "neon-night"));
        assert!(all.iter().any(|d| d["rice"] == "moonlight" && d["name"] == "amber-dusk"));

        let out_scoped = handle_draft_list(&inv(&["rice", "draft", "list"], &["sonata"]));
        let scoped = out_scoped.data.unwrap()["drafts"].as_array().unwrap().clone();
        assert_eq!(scoped.len(), 1, "scoping to sonata excludes moonlight's draft");
        assert_eq!(scoped[0]["name"], "neon-night");
        let _ = std::fs::remove_dir_all(&root);
    }

    // ── rice draft drop ──────────────────────────────────────────────────

    #[test]
    fn drop_removes_a_draft() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("draft-drop-ok");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        stage_with_rice(&stage, "sonata");
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        handle_draft_save(&inv(&["rice", "draft", "save"], &["neon-night"]));
        let dir = root.join("rices").join("sonata").join("drafts").join("neon-night");
        assert!(dir.is_dir());

        let out = handle_draft_drop(&inv(&["rice", "draft", "drop"], &["neon-night"]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert!(!dir.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn drop_rejects_a_path_traversal_name_before_touching_the_filesystem() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("draft-drop-traversal");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        stage_with_rice(&stage, "sonata");
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        // `draft_dir(rice, name)` joins `name` straight into a path — a
        // traversal name must be rejected before it ever reaches
        // `remove_dir_all`, same guard `handle_draft_save` already applies.
        let out = handle_draft_drop(&inv(&["rice", "draft", "drop"], &["../../evil"]));
        assert_eq!(out.status, Status::Error);
        assert_eq!(out.data.unwrap()["reason"], "invalid-name");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn drop_with_an_empty_name_is_an_invalid_name_not_a_panic() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let stage = unique_tmp("draft-drop-empty").join("stage");
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        for args in [&[][..], &[""][..]] {
            let out = handle_draft_drop(&inv(&["rice", "draft", "drop"], args));
            assert_eq!(out.status, Status::Error);
            assert_eq!(out.data.unwrap()["reason"], "invalid-name");
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn drop_on_a_missing_draft_errors() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let stage = unique_tmp("draft-drop-missing").join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        stage_with_rice(&stage, "sonata");
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        let out = handle_draft_drop(&inv(&["rice", "draft", "drop"], &["nope"]));
        assert_eq!(out.status, Status::Error);
        assert_eq!(out.render(false).1, aoide_protocol::output::exit::ERROR);
        assert_eq!(out.data.unwrap()["reason"], "draft-not-found");
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn drop_with_no_resolvable_rice_errors() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let stage = unique_tmp("draft-drop-norice").join("stage");
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        let out = handle_draft_drop(&inv(&["rice", "draft", "drop"], &["neon-night"]));
        assert_eq!(out.status, Status::Error);
        assert_eq!(out.data.unwrap()["reason"], "no-resolvable-rice");
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn drop_of_the_live_draft_is_refused_not_silently_torn_down() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("draft-drop-live");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        stage_with_rice(&stage, "sonata");
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        handle_draft_save(&inv(&["rice", "draft", "save"], &["neon-night"]));
        save_mode_marker(&ModeMarker {
            mode: RiceMode::Draft,
            rice: Some("sonata".to_string()),
            draft: Some("neon-night".to_string()),
            ..Default::default()
        })
        .unwrap();

        let out = handle_draft_drop(&inv(&["rice", "draft", "drop"], &["neon-night"]));
        assert_eq!(out.status, Status::Error);
        assert_eq!(out.data.unwrap()["reason"], "draft-is-live");
        // Nothing was removed, and the marker is untouched.
        assert!(root
            .join("rices")
            .join("sonata")
            .join("drafts")
            .join("neon-night")
            .is_dir());
        let marker = load_mode_marker();
        assert_eq!(marker.mode, RiceMode::Draft);
        assert_eq!(marker.draft, Some("neon-night".to_string()));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn drop_of_a_different_draft_succeeds_while_another_is_live() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("draft-drop-other");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        stage_with_rice(&stage, "sonata");
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        handle_draft_save(&inv(&["rice", "draft", "save"], &["amber-dusk"]));
        handle_draft_save(&inv(&["rice", "draft", "save"], &["neon-night"]));
        save_mode_marker(&ModeMarker {
            mode: RiceMode::Draft,
            rice: Some("sonata".to_string()),
            draft: Some("neon-night".to_string()),
            ..Default::default()
        })
        .unwrap();

        let out = handle_draft_drop(&inv(&["rice", "draft", "drop"], &["amber-dusk"]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        // The live draft's marker is completely untouched by dropping a
        // DIFFERENT one.
        let marker = load_mode_marker();
        assert_eq!(marker.mode, RiceMode::Draft);
        assert_eq!(marker.draft, Some("neon-night".to_string()));
        let _ = std::fs::remove_dir_all(&root);
    }
}
