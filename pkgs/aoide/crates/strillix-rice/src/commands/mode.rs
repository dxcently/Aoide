//! `rice mode {status,stage,declarative,draft}` — the staging/declarative/
//! draft mode toggle around `stage/mode.json` (concepts/Self-Ricing's
//! mode-toggle extension, khoa 2026-08-14; three-way `RiceMode` + symlink
//! draft routing, khoa 2026-08-14).
//!
//! Three modes, not two:
//!
//! - **`declarative`** — the safe default (no marker file at all reads as
//!   declarative — [`crate::mode::load_mode_marker`]). The direct
//!   CLI entrypoints for `rice stage` and `wallpaper set` refuse (their own
//!   `handle_rice_stage_entry`/`handle_wallpaper_set_entry` guards,
//!   `commands/rice.rs` and `commands/wallpaper.rs`) — nothing else in this
//!   codebase writes `stage/theme.json`/`stage/wallpaper.json`, so refusing
//!   those two entrypoints is sufficient to guarantee nothing drifts while
//!   locked; no background reconciler process exists (`aoided` is still a
//!   one-shot skeleton, `crates/server/src/daemon.rs`) and none is needed
//!   for that guarantee to hold. ONE exemption, and it is not a drift:
//!   `wallpaper set --from-skwd` records what an external wallpaper provider is
//!   already showing, and refusing it would leave the stage file describing
//!   something the screen is not (CONTRACTS.md §4).
//! - **`staging`** — `rice stage`/`wallpaper set` write live, always meaning
//!   plain declared content.
//! - **`draft`** — `stage/theme.json` is a SYMLINK into a saved
//!   `rices/<rice>/drafts/<name>/theme.json` ([`handle_mode_draft`]).
//!   Every writer of the stage file — `rice stage`, a hand-edit,
//!   Quickshell's own FileView reload — transparently lands in the draft,
//!   because [`aoide_storage::fs::atomic_write`] resolves and writes through
//!   a symlink at its destination rather than replacing it. This is
//!   ROUTING, not guessing: `rice stage`/`rice mode stage` carry ZERO
//!   draft-awareness and never auto-prefer or auto-detect one — reaching a
//!   draft at all only ever happens through `rice mode draft <name>`.
//!
//! `stage`/`declarative` both run [`super::rice::plan_rice_stage`] and
//! [`super::rice::apply_rice_stage`] directly (guard-free, `pub(crate)`) when a
//! rice name is given — the SAME side effects a bare `rice stage <name>` has —
//! so `declarative <name>` can
//! re-pin `stage/theme.json` to that rice's declared notes and lock it in
//! one step, even from the default (unmarked) declarative state, without
//! tripping its own guard (the marker isn't flipped until AFTER the write
//! succeeds). `declarative` plans with `repin` set
//! ([`super::rice::plan_rice_stage`]): a lock is not a rice
//! switch, so it leaves `stage/wallpaper.json` exactly as it stands.
//!
//! `rice mode stage` never leaves a bare flag-flip: with no name it resolves
//! "the current rice" off the existing `stage/theme.json`'s own `"rice"`
//! field ([`current_staged_rice`]) and stages that, so unlocking staging
//! always ALSO enables hot loading of whatever is presently active — only a
//! genuinely fresh box with no stage file yet falls back to a no-op unlock.
//! [`handle_mode_declarative`]'s own no-arg form resolves differently, and
//! deliberately: it goes to the DECLARED rice
//! ([`super::rice::declared_rice`], off `rice/declared/theme.json`) when the
//! lane has published one, and only then falls back to the same current-rice
//! rule above — "back to declarative" must mean the venue's declared rice, not
//! whichever rice happens to be staged.
//!
//! **Leaving `Draft` mode:** both [`handle_mode_stage`] and
//! [`handle_mode_declarative`] call [`teardown_draft_symlink`] — removing
//! any routing symlink currently at `stage/theme.json` — BEFORE they write
//! plain declared content, so that write lands in a real file rather than
//! transparently through into whatever draft the symlink still pointed at.
//! Neither leaves a dangling symlink behind when transitioning out of
//! `Draft`. The teardown comes AFTER the template seed and
//! [`super::rice::plan_rice_stage`], so a rice that cannot be staged fails
//! with the symlink and the `draft` marker exactly as they were. `mode.json`
//! is written last: a watcher of `stage/theme.json` re-arms on it
//! (CONTRACTS.md §4).

use aoide_protocol::Invocation;
use aoide_protocol::output::{Outcome, Status};
use aoide_protocol::registry::{arg, cmd, Registry};
use crate::mode::{load_mode_marker, mode_marker_path, save_mode_marker, ModeMarker, RiceMode};
use serde_json::{json, Value};
use aoide_protocol::Door;

pub fn register(r: &mut Registry) {
    r.insert(cmd!(
        path: ["rice", "mode", "status"],
        summary: "Report the current rice mode (staging, declarative, or draft) and, in staging/draft, which rice (and draft) it's pointed at.",
        args: [],
        flags: [],
        gated: false,
        implemented: true,
        handler: handle_mode_status,
        doors: &[Door::Cli, Door::Daemon, Door::Mcp],
    ));
    r.insert(cmd!(
        path: ["rice", "mode", "stage"],
        summary: "Unlock staging and enable hot loading now — ALWAYS plain declared content: stages <name> if given, else re-stages whatever rice is currently live. Leaves `Draft` mode (tearing down its routing symlink) if currently in it. `rice stage`/`wallpaper set` write live again.",
        args: [arg!("name", "string", false, "Rice to stage immediately; defaults to the currently staged rice.")],
        flags: [],
        gated: false,
        implemented: true,
        handler: handle_mode_stage,
        examples: [
            "rice mode stage moonlight",
            "rice mode stage",
        ],
    ));
    r.insert(cmd!(
        path: ["rice", "mode", "declarative"],
        summary: "Lock staging: `rice stage`/`wallpaper set` refuse until unlocked again. Optional <name> re-pins stage/theme.json to that rice's declared notes first. Leaves `Draft` mode (tearing down its routing symlink) if currently in it.",
        args: [arg!("name", "string", false, "Rice to pin stage/theme.json to before locking; omit to pin the declared rice (the currently staged one on a host with no declared twin).")],
        flags: [],
        gated: false,
        implemented: true,
        handler: handle_mode_declarative,
    ));
    r.insert(cmd!(
        path: ["rice", "mode", "draft"],
        summary: "Route stage/theme.json into rices/<rice>/drafts/<name>/theme.json via a symlink — every future write (rice stage, a hand-edit) lands directly in the draft. Forks the draft from the current stage first if it doesn't exist yet, and mints a baseline entry of it (unless it equals the head) so the first edit can be undone. Refuses while `rice mode declarative` is locked.",
        args: [arg!("name", "string", true, "Draft name to route the stage into (forked from the current stage if new).")],
        flags: [],
        gated: false,
        implemented: true,
        handler: handle_mode_draft,
        examples: ["rice mode draft neon-night"],
    ));
}

/// `pub(crate)`, not private: `commands/reload.rs` reuses this SAME mapping
/// to report which arm `strillix reload`'s dispatch took, rather than growing a
/// second `RiceMode -> &str` match elsewhere.
pub(crate) fn mode_word(m: RiceMode) -> &'static str {
    match m {
        RiceMode::Staging => "staging",
        RiceMode::Declarative => "declarative",
        RiceMode::Draft => "draft",
    }
}

/// `rice mode status` — read `stage/mode.json` via `aoide-storage` and
/// report it verbatim (absent file reports the `declarative` default, the
/// same tolerate-missing discipline every stage-file marker in this
/// codebase follows — [`crate::mode::load_mode_marker`]).
fn handle_mode_status(_inv: &Invocation) -> Outcome {
    let m = load_mode_marker();
    Outcome::ok("rice.mode.status", format!("rice mode: {}", mode_word(m.mode))).with_data(json!({
        "mode": mode_word(m.mode),
        "rice": m.rice,
        "draft": m.draft,
        "since": if m.since.is_empty() { None } else { Some(m.since) },
    }))
}

/// Read the rice `stage/theme.json` is CURRENTLY carrying, straight off its
/// own `"rice"` field — the same field both `rice stage` (below) and the nix
/// activation's `home.activation.aoideSeedStage` reseed script
/// (the strillix lane's activation) write on every stage, every activation.
/// This is how "the current rice" is knowable at all: the Rust
/// side has no nix evaluation access, so the stage file's own breadcrumb is
/// the only source of truth for "what is this host actually performing right
/// now" — reused here rather than re-deriving it some other way. Reads
/// transparently through a `Draft`-mode symlink too (`std::fs::read_to_string`
/// always follows symlinks), same as any other reader.
///
/// `pub(crate)`, not private: `commands/draft.rs` reuses this SAME resolution
/// to know which rice's `rices/<rice>/drafts/` `rice draft drop <name>`
/// should nest under, and [`handle_mode_draft`] (below) uses it to know which
/// rice to fork a new draft from — no adapter, no second implementation of
/// "what is the current rice".
pub(crate) fn current_staged_rice() -> Option<String> {
    let path = crate::paths::stage_dir().join("theme.json");
    let raw = std::fs::read_to_string(path).ok()?;
    let parsed: Value = serde_json::from_str(&raw).ok()?;
    let rice = parsed.get("rice")?.as_str()?.to_string();
    // The stage file is writable outside this CLI (a hand-edit, a rogue
    // writer) and every caller of this function joins the result straight
    // into a path (`draft_dir(rice, name)` in `draft.rs`/`mode.rs`) — same
    // traversal class `rice compose`/`rice stage` guard on the WRITE side;
    // this is the same guard on the READ side, so a poisoned `"rice"` field
    // can never resolve to anything at all rather than resolving to
    // something unsafe.
    crate::compose::valid_rice_name(&rice).then_some(rice)
}

/// Remove any `Draft`-mode routing symlink currently at `stage/theme.json`,
/// so a caller about to write plain declared content there
/// ([`super::rice::handle_rice_stage`], via `atomic_write`) creates a REAL
/// file rather than writing straight through into whatever draft the
/// symlink still points at. A no-op when `stage/theme.json` is absent or
/// already a plain file.
///
/// [`handle_mode_stage`] and [`handle_mode_declarative`] both call this
/// AFTER resolving which rice is currently active (that resolution itself
/// reads through the symlink via [`current_staged_rice`], so tearing it down
/// first would blind a bare no-`<name>` call) but BEFORE writing anything —
/// leaving `Draft` mode must never leave a dangling symlink behind.
fn teardown_draft_symlink() -> std::io::Result<()> {
    let stage_theme = crate::paths::stage_dir().join("theme.json");
    match std::fs::symlink_metadata(&stage_theme) {
        Ok(meta) if meta.file_type().is_symlink() => std::fs::remove_file(&stage_theme),
        _ => Ok(()),
    }
}

/// `rice mode stage [<name>]` — unlock staging writers AND enable hot
/// loading immediately, never a bare flag-flip. With `<name>`, stages that
/// rice; with no name, resolves "the current rice" off the existing
/// `stage/theme.json`'s own `"rice"` field ([`current_staged_rice`]) and
/// stages THAT — so entering staging mode always leaves a live, hot-loadable
/// rice behind it, not just an unlocked marker with a stale/absent stage
/// file. Only when no rice can be resolved at all (a genuinely fresh box
/// that has never activated or staged) does this fall back to a bare unlock.
///
/// ALWAYS means plain declared content — zero draft-awareness. If currently
/// in `Draft` mode, this doubles as how you LEAVE it:
/// [`teardown_draft_symlink`] removes the routing symlink, once the seed and
/// the stage plan have accepted the rice and before the declared-content
/// write, so that write lands in a real file rather than transparently
/// through into whatever draft the symlink still pointed at.
fn handle_mode_stage(inv: &Invocation) -> Outcome {
    let explicit_name = inv.args.first().cloned();
    let existing = load_mode_marker();
    // Resolution order for a bare (no-arg) call: an explicit name always
    // wins; otherwise prefer the remembered `staging_rice` — the durable
    // "what was I staging" memory that a declarative lock leaves untouched —
    // over `current_staged_rice()`'s file-read fallback, which reflects
    // whatever's CURRENTLY active and is unreliable here once a declarative
    // round-trip has overwritten it. `current_staged_rice()` remains the
    // final fallback only for the cold-start case where `staging_rice` has
    // never been set (a fresh `mode.json`, or one predating this field).
    let resolved_name = explicit_name
        .clone()
        .or_else(|| existing.staging_rice.clone())
        .or_else(current_staged_rice);

    let mut changed: Vec<String> = Vec::new();
    // Seed a first-stage runtime rices entry from the shipped template
    // BEFORE the plan below ever reads `rice_dir(name)` (task #41) —
    // this is the OTHER staging entry point `rice stage`'s own
    // `handle_rice_stage_entry` needs the same seed at (see
    // `commands::rice::seed_rices_from_templates`'s doc): a fresh host's
    // FIRST real interaction with a shipped-but-never-composed rice is
    // `rice mode stage <name>` (this command unlocks AND stages in one
    // call — `rice stage` alone refuses while still locked), so this is the
    // hook point that actually matters for that case, not an optional
    // extra. No-op whenever the rices already has anything for the
    // rice, or nothing is shipped for it either.
    //
    // Seed and plan come BEFORE the draft routing is torn down: a name that
    // cannot be staged must leave the shell where it was, not routed nowhere.
    let mut seeded_from: Option<std::path::PathBuf> = None;
    let mut plan = None;
    if let Some(name) = &resolved_name {
        seeded_from = match super::rice::seed_rices_from_templates(name) {
            Ok(Some(source)) => {
                changed.push(crate::paths::rice_dir(name).to_string_lossy().into_owned());
                Some(source)
            }
            Ok(None) => None,
            Err(e) => {
                return Outcome::error("rice.mode.stage", e)
                    .with_data(json!({ "reason": "seed-failed" }));
            }
        };
        let stage_inv = Invocation {
            path: vec!["rice".to_string(), "stage".to_string()],
            args: vec![name.clone()],
            flags: inv.flags.clone(),
            door: inv.door,
        };
        plan = match super::rice::plan_rice_stage(&stage_inv, false) {
            Ok(plan) => Some(plan),
            Err(mut refusal) => {
                refusal.command = "rice.mode.stage".to_string();
                return refusal;
            }
        };
    }

    if let Err(e) = teardown_draft_symlink() {
        return Outcome::error("rice.mode.stage", format!("failed to clear draft routing: {e}"))
            .with_data(json!({ "reason": "symlink-teardown-failed" }));
    }

    let mut data = json!({});
    if let Some(plan) = plan {
        let mut staged = super::rice::apply_rice_stage(plan);
        if staged.status != Status::Ok {
            staged.command = "rice.mode.stage".to_string();
            return staged;
        }
        data = staged.data.take().unwrap_or(data);
        changed.extend(staged.changed);
    }

    // `handle_rice_stage` above (the guard-free, marker-blind sibling of
    // `handle_rice_stage_entry`) never touches `stage/mode.json` itself, so
    // `existing` — loaded before that write, above — is still current; no
    // need to re-read it.
    let marker = ModeMarker {
        mode: RiceMode::Staging,
        rice: resolved_name.clone().or(existing.rice.clone()),
        draft: None,
        // Remember whatever rice staging mode is now on, carrying the old
        // value forward in the (normally unreachable) case this call somehow
        // resolves to nothing at all — same defensive `.or(existing...)`
        // pattern `rice` above already follows.
        staging_rice: resolved_name.clone().or(existing.staging_rice.clone()),
        since: aoide_storage::time::now_iso_utc(),
    };
    if let Err(e) = save_mode_marker(&marker) {
        return Outcome::error("rice.mode.stage", format!("failed to write mode marker: {e}"))
            .changed(changed)
            .with_data(json!({ "reason": "marker-write-failed" }));
    }
    changed.push(mode_marker_path().to_string_lossy().into_owned());

    data["mode"] = json!("staging");
    data["rice"] = json!(marker.rice);
    data["seeded"] = json!(seeded_from.as_ref().map(|p| p.to_string_lossy().into_owned()));

    // Never silent (task #41's outcome contract).
    let seed_note = match &seeded_from {
        Some(source) => format!(" — seeded rices from shipped template at {}", source.display()),
        None => String::new(),
    };

    Outcome::ok(
        "rice.mode.stage",
        match &resolved_name {
            Some(n) if explicit_name.is_some() => {
                format!("staging mode unlocked — staged `{n}` live{seed_note}")
            }
            Some(n) => format!(
                "staging mode unlocked — re-staged the current rice `{n}` live{seed_note}"
            ),
            None => "staging mode unlocked — no current rice to stage (no stage/theme.json yet)".to_string(),
        },
    )
    .changed(changed)
    .with_data(data)
}

/// `rice mode declarative [<name>]` — lock staging writers. With `<name>`,
/// re-pins `stage/theme.json` to that rice's committed notes FIRST (via
/// `handle_rice_stage`, guard-free), then writes the marker.
///
/// With NO name, this mirrors [`handle_mode_stage`]'s own no-arg auto-resolve
/// pattern rather than freezing the stage as-is: it resolves "the declared
/// rice" off the lane's declared twin
/// (`rice/declared/theme.json`, [`super::rice::declared_rice`],
/// CONTRACTS.md §4) — the rice the VENUE declares for this host — and only
/// when that twin is absent (a host that never activated the lane) falls
/// back to [`current_staged_rice`], the rice `stage/theme.json` is currently
/// carrying. It re-pins from the resolved rice's notes, then locks — so `rice
/// mode declarative` with no name discards whatever unsaved live edits sat in
/// the stage, same as the `<name>` path always has (`rice draft save` first
/// to keep them, hence the success message below). Only when no rice can be
/// resolved at all (a genuinely fresh box with no stage file yet) does this
/// fall back to a bare lock with nothing to re-pin — the one case where
/// nothing is discarded, because there was nothing live to discard.
///
/// If currently in `Draft` mode, this is also how you leave it: the SAME
/// [`teardown_draft_symlink`] call [`handle_mode_stage`] makes, after the
/// same plan, so the re-pin write below lands in a real file, never through a
/// stale symlink, and a rice that cannot be re-pinned leaves the draft routed.
///
/// A resolvable rice whose re-pin fails returns that error and does NOT flip
/// the marker — same failure handling the `<name>` path already had.
fn handle_mode_declarative(inv: &Invocation) -> Outcome {
    let explicit_name = inv.args.first().cloned();
    let resolved_name = explicit_name
        .or_else(super::rice::declared_rice)
        .or_else(current_staged_rice);

    // The plan comes BEFORE the draft routing is torn down: a name that cannot
    // be re-pinned must leave the shell where it was, not routed nowhere.
    let plan = match &resolved_name {
        Some(name) => {
            let stage_inv = Invocation {
                path: vec!["rice".to_string(), "stage".to_string()],
                args: vec![name.clone()],
                flags: inv.flags.clone(),
                door: inv.door,
            };
            match super::rice::plan_rice_stage(&stage_inv, true) {
                Ok(plan) => Some(plan),
                Err(mut refusal) => {
                    refusal.command = "rice.mode.declarative".to_string();
                    return refusal;
                }
            }
        }
        None => None,
    };

    if let Err(e) = teardown_draft_symlink() {
        return Outcome::error("rice.mode.declarative", format!("failed to clear draft routing: {e}"))
            .with_data(json!({ "reason": "symlink-teardown-failed" }));
    }

    let mut changed: Vec<String> = Vec::new();
    let mut data = json!({});
    if let Some(plan) = plan {
        let mut staged = super::rice::apply_rice_stage(plan);
        if staged.status != Status::Ok {
            staged.command = "rice.mode.declarative".to_string();
            return staged;
        }
        data = staged.data.take().unwrap_or(data);
        changed = staged.changed;
    }

    let existing = load_mode_marker();
    let marker = ModeMarker {
        mode: RiceMode::Declarative,
        rice: resolved_name.clone().or(existing.rice),
        draft: None,
        // Carried forward UNCHANGED, never cleared or overwritten here — this
        // is the crux of the staging-memory fix: `rice` above legitimately
        // gets overwritten to reflect what's now actually active, but
        // `staging_rice` must survive a declarative lock so a later bare
        // `rice mode stage` can still find its way back to it.
        staging_rice: existing.staging_rice,
        since: aoide_storage::time::now_iso_utc(),
    };
    if let Err(e) = save_mode_marker(&marker) {
        return Outcome::error("rice.mode.declarative", format!("failed to write mode marker: {e}"))
            .changed(changed)
            .with_data(json!({ "reason": "marker-write-failed" }));
    }
    changed.push(mode_marker_path().to_string_lossy().into_owned());

    data["mode"] = json!("declarative");
    data["rice"] = json!(marker.rice);

    Outcome::ok(
        "rice.mode.declarative",
        match &resolved_name {
            Some(n) => format!(
                "declarative mode locked — re-pinned to `{n}`'s committed notes \
                 (unsaved live edits discarded — `rice draft save` first to keep them); \
                 rice stage/wallpaper set refuse until unlocked"
            ),
            None => "declarative mode locked — rice stage/wallpaper set refuse until unlocked".to_string(),
        },
    )
    .changed(changed)
    .with_data(data)
}

/// Point `stage/theme.json` at `draft_theme`, removing whatever sits there
/// first (a real declared file, or a link routed to a different draft) —
/// [`teardown_draft_symlink`]'s inverse. [`handle_mode_draft`] routes with it,
/// and `strillix reload`'s draft arm re-routes with it after an activation seed
/// renamed a declared file over the link. The error is the outcome message,
/// naming the step that failed.
pub(crate) fn route_stage_to_draft(draft_theme: &std::path::Path) -> Result<(), String> {
    let stage_theme = crate::paths::stage_dir().join("theme.json");
    if std::fs::symlink_metadata(&stage_theme).is_ok() {
        std::fs::remove_file(&stage_theme)
            .map_err(|e| format!("failed to clear stage/theme.json before routing: {e}"))?;
    }
    if let Some(parent) = stage_theme.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("failed to prepare the stage dir: {e}"))?;
    }
    std::os::unix::fs::symlink(draft_theme, &stage_theme)
        .map_err(|e| format!("failed to route stage/theme.json to the draft: {e}"))
}

/// `rice mode draft <name>` — enter DRAFT mode: point `stage/theme.json`
/// at a symlink into `rices/<rice>/drafts/<name>/theme.json`, where
/// `rice` is resolved the same way `rice mode stage`'s no-arg form does
/// ([`current_staged_rice`] — no separate `<rice>` arg). Forks the draft
/// from whatever's CURRENTLY in the stage first if it doesn't exist yet,
/// reusing [`super::draft::fork_stage_into`] (the SAME write `rice draft
/// save`'s own handler performs, not a reimplementation) — so entering a
/// brand-new draft name is also how you create one.
///
/// Every future writer of `stage/theme.json` — `rice stage`, a hand-edit,
/// Quickshell's own FileView reload — transparently lands in the draft file
/// from here on, because [`aoide_storage::fs::atomic_write`] resolves and
/// writes through a symlink at its destination rather than replacing it.
/// Zero draft-awareness anywhere else is what makes this simple. (`wallpaper
/// set`/`stage/wallpaper.json` are NOT part of this routing — only
/// `theme.json` is symlinked.)
///
/// [`route_stage_to_draft`] removes whatever currently sits at
/// `stage/theme.json` (a real declared file, or an old symlink routed to a
/// DIFFERENT draft) before creating the new symlink. Refuses with the same
/// `declarative-mode-locked` shape every other write path uses.
fn handle_mode_draft(inv: &Invocation) -> Outcome {
    let existing = load_mode_marker();
    if existing.mode == RiceMode::Declarative {
        return Outcome::error(
            "rice.mode.draft",
            "declarative mode is locked — run `strillix rice mode stage` to unlock hot-loading first",
        )
        .with_data(json!({ "reason": "declarative-mode-locked" }));
    }

    let name = match inv.args.first() {
        Some(n) => n.clone(),
        None => {
            return Outcome::usage(
                "rice.mode.draft",
                "usage: strillix rice mode draft <name> [--json]",
            )
            .with_data(json!({ "reason": "missing-name" }));
        }
    };
    if !crate::compose::valid_rice_name(&name) {
        return Outcome::error(
            "rice.mode.draft",
            format!(
                "`{name}` is not a valid draft name: must match `^[a-z0-9][a-z0-9-]*$` \
                 (lowercase letters, digits, hyphens; no leading hyphen, no `/`, no `..`)"
            ),
        )
        .with_data(json!({ "reason": "invalid-name", "name": name }));
    }

    let rice = match current_staged_rice() {
        Some(s) => s,
        None => {
            return Outcome::error(
                "rice.mode.draft",
                "no rice is currently staged — nothing to fork a draft from \
                 (stage a rice first: `strillix rice stage <name>` or `strillix rice mode stage <name>`)",
            )
            .with_data(json!({ "reason": "no-resolvable-rice" }));
        }
    };

    let mut changed: Vec<String> = Vec::new();
    let draft_theme = crate::paths::draft_dir(&rice, &name).join("theme.json");
    if !draft_theme.is_file() {
        let mut forked = super::draft::fork_stage_into(&rice, &name);
        if forked.status != Status::Ok {
            forked.command = "rice.mode.draft".to_string();
            return forked;
        }
        changed.append(&mut forked.changed);
    }

    if let Err(e) = route_stage_to_draft(&draft_theme) {
        return Outcome::error("rice.mode.draft", e)
            .changed(changed)
            .with_data(json!({ "reason": "symlink-setup-failed" }));
    }
    let stage_theme = crate::paths::stage_dir().join("theme.json");
    changed.push(stage_theme.to_string_lossy().into_owned());

    let marker = ModeMarker {
        mode: RiceMode::Draft,
        rice: Some(rice.clone()),
        draft: Some(name.clone()),
        // Out of scope for this command (entering `Draft`, not `Staging`) —
        // carry the existing "what was I staging" memory forward unchanged.
        staging_rice: existing.staging_rice,
        since: aoide_storage::time::now_iso_utc(),
    };
    if let Err(e) = save_mode_marker(&marker) {
        return Outcome::error("rice.mode.draft", format!("failed to write mode marker: {e}"))
            .changed(changed)
            .with_data(json!({ "reason": "marker-write-failed" }));
    }
    changed.push(mode_marker_path().to_string_lossy().into_owned());

    // The baseline: a take of the draft as entered, so the first edit has
    // something to go back to (`rice back --entry 1`). After the marker, which
    // `history::resolve_scope` reads; deduped against the head, so re-entering an
    // unchanged draft mints nothing; never fatal, the route is already made.
    let mut out = Outcome::ok(
        "rice.mode.draft",
        format!("draft mode routed — stage/theme.json now points at `{rice}`'s draft `{name}`"),
    )
    .changed(changed)
    .with_data(json!({ "mode": "draft", "rice": rice, "draft": name }));
    super::history::attach_entry(&mut out, &marker, super::history::snapshot_if_identical_to_head("rice.mode.draft", "enter"));
    out
}

// ── Tests (rice mode status/stage/declarative/draft) ──────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use aoide_test_support::*;
    use std::path::PathBuf;

    #[test]
    fn current_staged_rice_rejects_a_hand_edited_path_traversal_rice_field() {
        // `current_staged_rice()`'s result is joined straight into
        // `draft_dir(rice, name)` by both `draft.rs` and `handle_mode_draft`
        // below — a stage/theme.json hand-edited (or written by some other
        // process) with a traversal `"rice"` must resolve to nothing at all,
        // same as a missing/absent one, rather than resolving to something
        // a caller then treats as a safe directory component.
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let stage = unique_tmp("current-staged-rice-traversal").join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::write(
            stage.join("theme.json"),
            r##"{"schemaVersion":"0","rice":"../../evil","palette":{"bg":"#000"}}"##,
        )
        .unwrap();
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        assert_eq!(current_staged_rice(), None);
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn status_reports_the_declarative_default_with_no_marker_file() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let stage = unique_tmp("mode-status-default").join("stage");
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        let out = handle_mode_status(&inv(&["rice", "mode", "status"], &[]));
        assert_eq!(out.status, Status::Ok);
        let data = out.data.unwrap();
        assert_eq!(data["mode"], "declarative");
        assert!(data["rice"].is_null());
        assert!(data["since"].is_null());
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn stage_with_no_name_just_unlocks() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let stage = unique_tmp("mode-stage-bare").join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        let out = handle_mode_stage(&inv(&["rice", "mode", "stage"], &[]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let marker = load_mode_marker();
        assert_eq!(marker.mode, RiceMode::Staging);
        assert_eq!(marker.rice, None);
        assert!(out.changed.iter().any(|c| c.ends_with("stage/mode.json")));
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn stage_with_a_name_stages_it_and_unlocks() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("mode-stage-named");
        let stage = root.join("stage");
        let rice = root.join("rices").join("moonlight");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::create_dir_all(&rice).unwrap();
        std::fs::write(rice.join("theme.json"), VALID_NOTES).unwrap();
        std::fs::write(rice.join("lockscreen.png"), b"").unwrap();
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        let out = handle_mode_stage(&inv(&["rice", "mode", "stage"], &["moonlight"]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let marker = load_mode_marker();
        assert_eq!(marker.mode, RiceMode::Staging);
        assert_eq!(marker.rice, Some("moonlight".to_string()));
        assert!(out.changed.iter().any(|c| c.ends_with("stage/theme.json")));
        assert!(out.changed.iter().any(|c| c.ends_with("stage/mode.json")));
        assert!(stage.join("theme.json").is_file());
        // The rice ships a lock picture, so staging it dresses the lock.
        let lock = std::fs::read_to_string(stage.join("hyprlock.conf")).unwrap();
        let picture = rice.join("lockscreen.png");
        assert!(lock.lines().any(|l| l == format!("$aoide_lock_image = {}", picture.display())), "{lock}");
        assert!(out.changed.iter().any(|c| c.ends_with("stage/hyprlock.conf")));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The realistic fresh-host case task #41 exists for: `rice mode stage
    /// <name>` unlocks AND stages in one call, so it — not bare `rice
    /// stage`, which refuses while still locked — is what a host actually
    /// runs FIRST for a shipped-but-never-composed rice. No pre-existing
    /// runtime rices entry here at all, only a shipped template.
    #[test]
    fn stage_with_a_name_seeds_the_runtime_rices_from_the_shipped_template_on_first_stage() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "STRILLIX_TEMPLATES"]);
        let root = unique_tmp("mode-stage-seed");
        let stage = root.join("stage");
        let templates = root.join("templates");
        let templated_rice = templates.join("moonlight");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::create_dir_all(&templated_rice).unwrap();
        std::fs::write(templated_rice.join("theme.json"), VALID_NOTES).unwrap();
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);
        std::env::set_var("STRILLIX_TEMPLATES", &templates);

        let out = handle_mode_stage(&inv(&["rice", "mode", "stage"], &["moonlight"]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert!(
            out.message.contains("seeded rices"),
            "never silent: {}",
            out.message
        );
        assert_eq!(out.data.unwrap()["seeded"], json!(templated_rice.to_string_lossy()));

        let marker = load_mode_marker();
        assert_eq!(marker.mode, RiceMode::Staging);
        assert_eq!(marker.rice, Some("moonlight".to_string()));
        assert_eq!(
            std::fs::read_to_string(crate::paths::rice_dir("moonlight").join("theme.json"))
                .unwrap(),
            VALID_NOTES,
            "the shipped template landed in the runtime rices"
        );
        assert!(stage.join("theme.json").is_file());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn stage_with_no_name_resolves_and_restages_the_current_rice() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("mode-stage-resolve-current");
        let stage = root.join("stage");
        let rice = root.join("rices").join("moonlight");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::create_dir_all(&rice).unwrap();
        std::fs::write(rice.join("theme.json"), VALID_NOTES).unwrap();
        // The stage file ALREADY carries a "rice" breadcrumb — written by a
        // prior `rice stage`/`rice mode stage` call or the nix activation's
        // reseed script — even though the caller passes no name this time.
        std::fs::write(
            stage.join("theme.json"),
            r##"{"schemaVersion":"0","rice":"moonlight","palette":{"bg":"#000"}}"##,
        )
        .unwrap();
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        let out = handle_mode_stage(&inv(&["rice", "mode", "stage"], &[]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert!(
            out.message.contains("re-staged the current rice"),
            "message names the auto-resolve path: {}",
            out.message
        );
        let marker = load_mode_marker();
        assert_eq!(marker.mode, RiceMode::Staging);
        assert_eq!(marker.rice, Some("moonlight".to_string()));
        assert!(out.changed.iter().any(|c| c.ends_with("stage/theme.json")));
        assert!(out.changed.iter().any(|c| c.ends_with("stage/mode.json")));
        // The re-stage pulled FRESH content from rices/moonlight (which
        // has an "accent" the seeded stage stub lacked) — proves it actually
        // re-derived, not just reused the breadcrumb as a no-op.
        let restaged = std::fs::read_to_string(stage.join("theme.json")).unwrap();
        assert!(restaged.contains("\"accent\""));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn stage_with_an_unknown_rice_name_fails_and_never_unlocks() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let stage = unique_tmp("mode-stage-badname").join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        let out = handle_mode_stage(&inv(&["rice", "mode", "stage"], &["nope"]));
        assert_eq!(out.status, Status::Error);
        assert_eq!(out.command, "rice.mode.stage");
        // The mode marker must never flip on a failed stage — still the
        // default declarative, not left half-unlocked.
        assert_eq!(load_mode_marker().mode, RiceMode::Declarative);
    }

    #[test]
    fn declarative_with_no_name_but_a_resolvable_current_rice_repins_then_locks() {
        // The behavior change (khoa 2026-08-14): bare `rice mode declarative`
        // no longer freezes the stage as-is — it mirrors `rice mode stage`'s
        // own no-arg auto-resolve and re-pins from the COMMITTED rices
        // notes before locking.
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("mode-declarative-autoresolve");
        let stage = root.join("stage");
        let rice = root.join("rices").join("moonlight");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::create_dir_all(&rice).unwrap();
        std::fs::write(rice.join("theme.json"), VALID_NOTES).unwrap();
        // The stage already carries a "rice" breadcrumb but STALE content (no
        // "accent" field, which VALID_NOTES has) — proves the lock actually
        // RE-PINS from the committed rices rather than freezing the stale
        // stage in place.
        std::fs::write(
            stage.join("theme.json"),
            r##"{"schemaVersion":"0","rice":"moonlight","palette":{"bg":"#000"}}"##,
        )
        .unwrap();
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        let out = handle_mode_declarative(&inv(&["rice", "mode", "declarative"], &[]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert!(
            out.message.contains("unsaved live edits discarded"),
            "the behavior change is self-documented in the CLI output: {}",
            out.message
        );
        let marker = load_mode_marker();
        assert_eq!(marker.mode, RiceMode::Declarative);
        assert_eq!(marker.rice, Some("moonlight".to_string()));
        assert_eq!(marker.draft, None);
        let repinned = std::fs::read_to_string(stage.join("theme.json")).unwrap();
        assert!(
            repinned.contains("\"accent\""),
            "re-pinned from the committed rices, not frozen as-is: {repinned}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A declared twin that carries no `geometry.terminalOpacity` re-pins to a
    /// terminal file with no `background_opacity` line: the host's bake shows.
    #[test]
    fn declarative_repin_of_a_twin_with_no_geometry_writes_no_opacity_line() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("mode-declarative-no-geometry");
        let stage = root.join("stage");
        let declared = root.join("declared");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::create_dir_all(root.join("rices").join("sonata")).unwrap();
        std::fs::create_dir_all(&declared).unwrap();
        std::fs::write(root.join("rices").join("sonata").join("theme.json"), VALID_NOTES)
            .unwrap();
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);
        std::fs::write(
            declared.join("theme.json"),
            r##"{"palette":{"accent":"#ebbcba","bg":"#0b1021","fg":"#c8d3f5","urgent":"#ff757f"},"schemaVersion":"0","rice":"sonata"}"##,
        )
        .unwrap();

        let out = handle_mode_declarative(&inv(&["rice", "mode", "declarative"], &["sonata"]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let terminal = std::fs::read_to_string(stage.join("terminal-colors.conf")).unwrap();
        assert!(terminal.lines().any(|l| l == "background #0b1021"), "{terminal}");
        assert!(!terminal.contains("background_opacity"), "{terminal}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The declared twin (CONTRACTS.md §4) outranks the stage file's own
    /// breadcrumb: with a DIFFERENT rice currently staged, bare `rice mode
    /// declarative` must land back on the rice the VENUE declares — the rice
    /// `rice/declared/theme.json` names — not on whatever happens to be
    /// staged. `staging_rice` still survives the lock untouched.
    #[test]
    fn declarative_with_no_name_repins_the_declared_rice() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("mode-declarative-declared-rice");
        let stage = root.join("stage");
        let sonata = root.join("rices").join("sonata");
        let nocturne = root.join("rices").join("nocturne");
        let declared = root.join("declared");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::create_dir_all(&sonata).unwrap();
        std::fs::create_dir_all(&nocturne).unwrap();
        std::fs::create_dir_all(&declared).unwrap();
        std::fs::write(sonata.join("theme.json"), VALID_NOTES).unwrap();
        std::fs::write(sonata.join("lockscreen.png"), b"").unwrap();
        std::fs::write(nocturne.join("theme.json"), VALID_NOTES).unwrap();
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);
        // The lane's activation seed published the DECLARED twin for sonata…
        std::fs::write(
            declared.join("theme.json"),
            r##"{"geometry":{"terminalOpacity":0.7},"palette":{"accent":"#ebbcba","bg":"#0b1021","fg":"#c8d3f5","urgent":"#ff757f"},"schemaVersion":"0","rice":"sonata"}"##,
        )
        .unwrap();
        // …while the stage is currently performing nocturne, and a prior
        // staging session's own memory says `etude`.
        std::fs::write(
            stage.join("theme.json"),
            r##"{"schemaVersion":"0","rice":"nocturne","palette":{"bg":"#000"}}"##,
        )
        .unwrap();
        save_mode_marker(&ModeMarker {
            mode: RiceMode::Staging,
            rice: Some("nocturne".to_string()),
            draft: None,
            staging_rice: Some("etude".to_string()),
            since: aoide_storage::time::now_iso_utc(),
        })
        .unwrap();

        let out = handle_mode_declarative(&inv(&["rice", "mode", "declarative"], &[]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let repinned: Value =
            serde_json::from_str(&std::fs::read_to_string(stage.join("theme.json")).unwrap())
                .unwrap();
        assert_eq!(
            repinned["rice"], "sonata",
            "re-pinned to the DECLARED rice, not the currently staged nocturne"
        );
        assert_eq!(
            repinned["palette"]["accent"], "#ebbcba",
            "and derived from the declared twin, venue recolour included"
        );
        // Leaving staging restores the terminals the same live way: the
        // staged colour file is rewritten from the DECLARED twin (its bg and
        // accent, not the committed sonata's or the staged nocturne's).
        let terminal = std::fs::read_to_string(stage.join("terminal-colors.conf")).unwrap();
        assert!(terminal.lines().any(|l| l == "background #0b1021"), "{terminal}");
        assert!(terminal.lines().any(|l| l == "cursor #c8d3f5"), "{terminal}");
        assert!(terminal.lines().any(|l| l == "color4 #ebbcba"), "{terminal}");
        assert!(terminal.lines().any(|l| l == "background_opacity 0.7"), "{terminal}");
        assert!(out.changed.iter().any(|c| c.ends_with("stage/terminal-colors.conf")));
        // And the lock the same way: the declared twin's colours, the declared
        // rice's own picture from the runtime rices.
        let lock = std::fs::read_to_string(stage.join("hyprlock.conf")).unwrap();
        let picture = sonata.join("lockscreen.png");
        assert!(lock.lines().any(|l| l == format!("$aoide_lock_image = {}", picture.display())), "{lock}");
        assert!(lock.lines().any(|l| l == "$aoide_lock_color = rgb(0b1021)"), "{lock}");
        assert!(lock.lines().any(|l| l == "$aoide_lock_field_check = rgb(ebbcba)"), "{lock}");
        assert!(out.changed.iter().any(|c| c.ends_with("stage/hyprlock.conf")));
        let marker = load_mode_marker();
        assert_eq!(marker.mode, RiceMode::Declarative);
        assert_eq!(marker.rice, Some("sonata".to_string()));
        assert_eq!(marker.draft, None);
        assert_eq!(
            marker.staging_rice,
            Some("etude".to_string()),
            "the declarative lock still never touches the staging memory"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn declarative_with_no_name_and_no_resolvable_rice_is_still_a_bare_lock() {
        // The unchanged fallback: a genuinely fresh box with no stage file
        // yet has nothing to resolve, so this is still a no-op freeze, not an
        // error and not a fabricated re-pin.
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let stage = unique_tmp("mode-declarative-bare").join("stage");
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        let out = handle_mode_declarative(&inv(&["rice", "mode", "declarative"], &[]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert!(
            !out.message.contains("discarded"),
            "nothing was live to discard: {}",
            out.message
        );
        let marker = load_mode_marker();
        assert_eq!(marker.mode, RiceMode::Declarative);
        assert_eq!(marker.rice, None);
        assert!(!stage.join("theme.json").exists(), "nothing staged out of nowhere");
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn declarative_with_a_name_repins_then_locks_even_from_the_default_state() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("mode-declarative-named");
        let stage = root.join("stage");
        let rice = root.join("rices").join("moonlight");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::create_dir_all(&rice).unwrap();
        std::fs::write(rice.join("theme.json"), VALID_NOTES).unwrap();
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        // Starting from the unmarked (default-declarative) state — the
        // re-pin write must NOT trip its own not-yet-written lock.
        assert_eq!(load_mode_marker().mode, RiceMode::Declarative);

        let out = handle_mode_declarative(&inv(&["rice", "mode", "declarative"], &["moonlight"]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let marker = load_mode_marker();
        assert_eq!(marker.mode, RiceMode::Declarative);
        assert_eq!(marker.rice, Some("moonlight".to_string()));
        assert!(stage.join("theme.json").is_file());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn stage_then_declarative_round_trip_toggles_cleanly() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let stage = unique_tmp("mode-roundtrip").join("stage");
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        handle_mode_stage(&inv(&["rice", "mode", "stage"], &[]));
        assert_eq!(load_mode_marker().mode, RiceMode::Staging);

        handle_mode_declarative(&inv(&["rice", "mode", "declarative"], &[]));
        assert_eq!(load_mode_marker().mode, RiceMode::Declarative);

        let _ = std::fs::remove_dir_all(&stage);
    }

    // ── `staging_rice` survives a declarative lock (khoa 2026-08-17) ──────
    //
    // The regression this whole field exists to fix: `current_staged_rice()`
    // reads `stage/theme.json`'s own `"rice"` field, which `handle_rice_stage`
    // overwrites on EVERY stage — including the re-pin a declarative lock
    // performs. So staging `etude`, then locking declarative on `sonata`,
    // used to permanently lose the memory that `etude` was ever staged: a
    // later bare `rice mode stage` (no name — the CLI default, which the RICE
    // menu never sends) would resolve back to `sonata`, never `etude`. `staging_rice` is
    // a separate, declarative-immune memory of "what was I staging" that a
    // bare `rice mode stage` now prefers over the unreliable file-read.

    #[test]
    fn staging_rice_survives_a_declarative_lock_and_a_bare_stage_resolves_back_to_it() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("mode-staging-rice-survives-lock");
        let stage = root.join("stage");
        let sonata = root.join("rices").join("sonata");
        let etude = root.join("rices").join("etude");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::create_dir_all(&sonata).unwrap();
        std::fs::create_dir_all(&etude).unwrap();
        std::fs::write(sonata.join("theme.json"), VALID_NOTES).unwrap();
        std::fs::write(etude.join("theme.json"), VALID_NOTES).unwrap();
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        // Stage `etude` explicitly.
        let out = handle_mode_stage(&inv(&["rice", "mode", "stage"], &["etude"]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let marker = load_mode_marker();
        assert_eq!(marker.rice, Some("etude".to_string()));
        assert_eq!(marker.staging_rice, Some("etude".to_string()));

        // Lock declarative on `sonata` — `rice` flips, `staging_rice` must
        // NOT: this is the actual regression, asserted explicitly.
        let out = handle_mode_declarative(&inv(&["rice", "mode", "declarative"], &["sonata"]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let marker = load_mode_marker();
        assert_eq!(marker.mode, RiceMode::Declarative);
        assert_eq!(marker.rice, Some("sonata".to_string()));
        assert_eq!(
            marker.staging_rice,
            Some("etude".to_string()),
            "declarative locking must not erase the staging memory"
        );

        // A bare `rice mode stage` (no name — the CLI default, which the RICE
        // menu never sends) must resolve back to `etude`, NOT `sonata` and NOT whatever
        // `current_staged_rice()`/`stage/theme.json` currently says.
        let out = handle_mode_stage(&inv(&["rice", "mode", "stage"], &[]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert!(
            out.message.contains("etude"),
            "bare stage resolved to the remembered staging_rice, not the just-locked rice: {}",
            out.message
        );
        let marker = load_mode_marker();
        assert_eq!(marker.mode, RiceMode::Staging);
        assert_eq!(marker.rice, Some("etude".to_string()));
        assert_eq!(marker.staging_rice, Some("etude".to_string()));

        let _ = std::fs::remove_dir_all(&root);
    }

    // ── A wallpaper pick across a mode change and a rebuild ──────────────────

    /// `<root>/{stage,rices/<rice>/theme.json}` for each named rice, with
    /// `AOIDE_STAGE_DIR` pointed at the stage and the rices gate fixtured.
    /// Callers own `remove_dir_all(&root)`.
    fn pick_tmp(tag: &str, rices: &[&str]) -> (PathBuf, PathBuf) {
        let root = unique_tmp(tag);
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        for rice in rices {
            let dir = root.join("rices").join(rice);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("theme.json"), VALID_NOTES).unwrap();
        }
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);
        (root, stage)
    }

    /// The rebuild case: the activation seed rewrites the staged `rice` to the
    /// declared one and leaves `wallpaper.json` alone, so a pick must survive being
    /// reseeded away and back — and must not show over a different rice.
    #[test]
    fn a_pick_survives_a_reseed_to_the_declared_rice_and_goes_for_another() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let (root, stage) = pick_tmp("mode-reseed-pick", &["cadenza", "sonata"]);

        handle_mode_stage(&inv(&["rice", "mode", "stage"], &["cadenza"]));
        crate::wallpaper::stage_pick(std::path::Path::new("/tmp/chosen.png"), Some("cadenza"), None)
            .unwrap();
        let staged_pick = std::fs::read_to_string(stage.join("wallpaper.json")).unwrap();

        let mut reseeded: Value = serde_json::from_str(
            &std::fs::read_to_string(stage.join("theme.json")).unwrap(),
        )
        .unwrap();
        reseeded["rice"] = json!("sonata");
        std::fs::write(stage.join("theme.json"), serde_json::to_string(&reseeded).unwrap())
            .unwrap();

        let out = handle_mode_stage(&inv(&["rice", "mode", "stage"], &[]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert_eq!(current_staged_rice().as_deref(), Some("cadenza"));
        assert!(crate::wallpaper::staged_is_pick(), "the pick's own rice is back");
        assert_eq!(
            std::fs::read_to_string(stage.join("wallpaper.json")).unwrap(),
            staged_pick
        );

        let switched = handle_mode_stage(&inv(&["rice", "mode", "stage"], &["sonata"]));
        assert_eq!(switched.status, Status::Ok, "{:?}", switched.data);
        assert!(
            !stage.join("wallpaper.json").exists(),
            "a pick for cadenza is not shown over sonata"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Stage `cadenza`, pick a wallpaper, fork a Draft, then leave Draft mode
    /// with a bare `rice mode stage` (the no-name CLI call).
    #[test]
    fn leaving_draft_mode_by_stage_keeps_a_pick_made_for_the_same_rice() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let (root, stage) = pick_tmp("mode-draft-pick-stage", &["cadenza"]);

        handle_mode_stage(&inv(&["rice", "mode", "stage"], &["cadenza"]));
        assert_eq!(current_staged_rice().as_deref(), Some("cadenza"));
        crate::wallpaper::stage_pick(std::path::Path::new("/tmp/chosen.png"), Some("cadenza"), None).unwrap();
        let staged_pick = std::fs::read_to_string(stage.join("wallpaper.json")).unwrap();
        assert!(crate::wallpaper::staged_is_pick());

        // Into a draft, then out of it again with the same rice.
        let drafted = handle_mode_draft(&inv(&["rice", "mode", "draft"], &["neon-night"]));
        assert_eq!(drafted.status, Status::Ok, "{:?}", drafted.data);
        assert_eq!(load_mode_marker().mode, RiceMode::Draft);
        assert!(std::fs::symlink_metadata(stage.join("theme.json"))
            .unwrap()
            .file_type()
            .is_symlink());

        let out = handle_mode_stage(&inv(&["rice", "mode", "stage"], &[]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert_eq!(load_mode_marker().mode, RiceMode::Staging);
        assert_eq!(current_staged_rice().as_deref(), Some("cadenza"));
        assert!(
            crate::wallpaper::staged_is_pick(),
            "a pick for the re-staged rice survives leaving Draft"
        );
        assert_eq!(
            std::fs::read_to_string(stage.join("wallpaper.json")).unwrap(),
            staged_pick,
            "byte-identical — nothing rewrote it"
        );
        assert!(
            !out.changed.iter().any(|c| c.ends_with("wallpaper.json")),
            "no wallpaper write on the picked path: {:?}",
            out.changed
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The declarative lock is not a rice switch: locking and unlocking back to
    /// the pick's own rice leaves the pick exactly where it was.
    #[test]
    fn a_declarative_lock_and_unlock_leave_a_pick_untouched() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let (root, stage) = pick_tmp("mode-declarative-pick", &["cadenza", "sonata"]);

        handle_mode_stage(&inv(&["rice", "mode", "stage"], &["cadenza"]));
        crate::wallpaper::stage_pick(std::path::Path::new("/tmp/chosen.png"), Some("cadenza"), None)
            .unwrap();
        let staged_pick = std::fs::read_to_string(stage.join("wallpaper.json")).unwrap();

        let locked = handle_mode_declarative(&inv(&["rice", "mode", "declarative"], &[]));
        assert_eq!(locked.status, Status::Ok, "{:?}", locked.data);
        assert_eq!(load_mode_marker().mode, RiceMode::Declarative);
        assert_eq!(
            std::fs::read_to_string(stage.join("wallpaper.json")).unwrap(),
            staged_pick,
            "the lock's re-pin does not touch wallpaper.json"
        );

        let out = handle_mode_stage(&inv(&["rice", "mode", "stage"], &[]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert_eq!(current_staged_rice().as_deref(), Some("cadenza"));
        assert!(crate::wallpaper::staged_is_pick());
        assert_eq!(std::fs::read_to_string(stage.join("wallpaper.json")).unwrap(), staged_pick);

        // Even a lock that re-pins to a DIFFERENT rice leaves the pick alone:
        // it is hidden read-side, not dropped.
        let locked_other =
            handle_mode_declarative(&inv(&["rice", "mode", "declarative"], &["sonata"]));
        assert_eq!(locked_other.status, Status::Ok, "{:?}", locked_other.data);
        assert_eq!(current_staged_rice().as_deref(), Some("sonata"));
        assert_eq!(std::fs::read_to_string(stage.join("wallpaper.json")).unwrap(), staged_pick);

        let back = handle_mode_stage(&inv(&["rice", "mode", "stage"], &[]));
        assert_eq!(back.status, Status::Ok, "{:?}", back.data);
        assert!(crate::wallpaper::staged_is_pick());

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The same regression through `rice mode declarative`.
    #[test]
    fn leaving_draft_mode_by_declarative_keeps_a_pick_made_for_the_same_rice() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let (root, stage) = pick_tmp("mode-draft-pick-declarative", &["cadenza"]);

        handle_mode_stage(&inv(&["rice", "mode", "stage"], &["cadenza"]));
        crate::wallpaper::stage_pick(std::path::Path::new("/tmp/chosen.png"), Some("cadenza"), None).unwrap();
        let staged_pick = std::fs::read_to_string(stage.join("wallpaper.json")).unwrap();

        let drafted = handle_mode_draft(&inv(&["rice", "mode", "draft"], &["neon-night"]));
        assert_eq!(drafted.status, Status::Ok, "{:?}", drafted.data);

        let out = handle_mode_declarative(&inv(&["rice", "mode", "declarative"], &[]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let marker = load_mode_marker();
        assert_eq!(marker.mode, RiceMode::Declarative);
        assert_eq!(marker.rice, Some("cadenza".to_string()));
        assert!(crate::wallpaper::staged_is_pick(), "the pick survives the re-pin");
        assert_eq!(
            std::fs::read_to_string(stage.join("wallpaper.json")).unwrap(),
            staged_pick
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// …and a mode change that re-stages a DIFFERENT rice is a rice switch, so
    /// the pick is dropped (here: cleared outright, because the new rice
    /// derives no wallpaper of its own).
    #[test]
    fn leaving_draft_mode_by_staging_a_different_rice_drops_the_pick() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let (root, stage) = pick_tmp("mode-draft-pick-switch", &["cadenza", "etude"]);

        handle_mode_stage(&inv(&["rice", "mode", "stage"], &["cadenza"]));
        crate::wallpaper::stage_pick(std::path::Path::new("/tmp/chosen.png"), Some("cadenza"), None).unwrap();
        let drafted = handle_mode_draft(&inv(&["rice", "mode", "draft"], &["neon-night"]));
        assert_eq!(drafted.status, Status::Ok, "{:?}", drafted.data);

        let out = handle_mode_stage(&inv(&["rice", "mode", "stage"], &["etude"]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert_eq!(current_staged_rice().as_deref(), Some("etude"));
        assert!(
            !stage.join("wallpaper.json").exists(),
            "a rice switch drops the pick — and with it the previous rice's wallpaper"
        );
        assert!(!crate::wallpaper::staged_is_pick());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn bare_stage_on_a_fresh_mode_json_still_falls_back_to_current_staged_rice() {
        // Cold-start case: a `mode.json` that has never carried a
        // `staging_rice` (a fresh box, or one predating this field) must
        // still resolve a bare `rice mode stage` off `current_staged_rice()`
        // — the old behavior — rather than failing to resolve at all.
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("mode-staging-rice-cold-start");
        let stage = root.join("stage");
        let rice = root.join("rices").join("moonlight");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::create_dir_all(&rice).unwrap();
        std::fs::write(rice.join("theme.json"), VALID_NOTES).unwrap();
        // The stage file already carries a "rice" breadcrumb, as if seeded by
        // the nix activation reseed script — but no mode.json exists yet, so
        // `staging_rice` has never been set.
        std::fs::write(
            stage.join("theme.json"),
            r##"{"schemaVersion":"0","rice":"moonlight","palette":{"bg":"#000"}}"##,
        )
        .unwrap();
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);
        assert_eq!(load_mode_marker(), ModeMarker::default(), "no mode.json written yet");

        let out = handle_mode_stage(&inv(&["rice", "mode", "stage"], &[]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let marker = load_mode_marker();
        assert_eq!(marker.rice, Some("moonlight".to_string()));
        assert_eq!(marker.staging_rice, Some("moonlight".to_string()));

        let _ = std::fs::remove_dir_all(&root);
    }

    // ── rice mode draft ───────────────────────────────────────────────────

    #[test]
    fn mode_draft_forks_a_new_draft_and_routes_the_symlink() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("mode-draft-fork");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        let live_notes = r##"{"schemaVersion":"0","rice":"sonata","palette":{"bg":"#live"}}"##;
        std::fs::write(stage.join("theme.json"), live_notes).unwrap();
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        save_mode_marker(&ModeMarker {
            mode: RiceMode::Staging,
            rice: Some("sonata".to_string()),
            ..Default::default()
        })
        .unwrap();

        let out = handle_mode_draft(&inv(&["rice", "mode", "draft"], &["neon-night"]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);

        let draft_theme = root.join("rices").join("sonata").join("drafts").join("neon-night").join("theme.json");
        assert_eq!(std::fs::read_to_string(&draft_theme).unwrap(), live_notes, "forked from the live stage");
        assert!(
            std::fs::symlink_metadata(stage.join("theme.json")).unwrap().file_type().is_symlink(),
            "stage/theme.json is now a symlink"
        );
        assert_eq!(std::fs::read_link(stage.join("theme.json")).unwrap(), draft_theme);
        assert_eq!(std::fs::read_to_string(stage.join("theme.json")).unwrap(), live_notes, "reads through fine");

        let marker = load_mode_marker();
        assert_eq!(marker.mode, RiceMode::Draft);
        assert_eq!(marker.rice, Some("sonata".to_string()));
        assert_eq!(marker.draft, Some("neon-night".to_string()));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn mode_draft_routes_to_an_existing_draft_without_reforking() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("mode-draft-existing");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::write(
            stage.join("theme.json"),
            r##"{"schemaVersion":"0","rice":"sonata","palette":{"bg":"#live"}}"##,
        )
        .unwrap();
        let existing_dir = root.join("rices").join("sonata").join("drafts").join("neon-night");
        std::fs::create_dir_all(&existing_dir).unwrap();
        let existing_notes = r##"{"schemaVersion":"0","rice":"sonata","palette":{"bg":"#already-saved"}}"##;
        std::fs::write(existing_dir.join("theme.json"), existing_notes).unwrap();
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        save_mode_marker(&ModeMarker {
            mode: RiceMode::Staging,
            rice: Some("sonata".to_string()),
            ..Default::default()
        })
        .unwrap();

        let out = handle_mode_draft(&inv(&["rice", "mode", "draft"], &["neon-night"]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        // The EXISTING draft content is untouched (not overwritten by the
        // live stage's own content) — routing, not re-forking.
        assert_eq!(std::fs::read_to_string(&existing_dir.join("theme.json")).unwrap(), existing_notes);
        assert_eq!(std::fs::read_to_string(stage.join("theme.json")).unwrap(), existing_notes);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn mode_draft_replaces_a_symlink_routed_to_a_different_draft() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("mode-draft-reroute");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::write(
            stage.join("theme.json"),
            r##"{"schemaVersion":"0","rice":"sonata","palette":{"bg":"#seed"}}"##,
        )
        .unwrap();
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);
        save_mode_marker(&ModeMarker {
            mode: RiceMode::Staging,
            rice: Some("sonata".to_string()),
            ..Default::default()
        })
        .unwrap();

        handle_mode_draft(&inv(&["rice", "mode", "draft"], &["amber-dusk"]));
        assert_eq!(load_mode_marker().draft, Some("amber-dusk".to_string()));

        let out = handle_mode_draft(&inv(&["rice", "mode", "draft"], &["neon-night"]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let marker = load_mode_marker();
        assert_eq!(marker.draft, Some("neon-night".to_string()));
        let neon_theme = root.join("rices").join("sonata").join("drafts").join("neon-night").join("theme.json");
        assert_eq!(std::fs::read_link(stage.join("theme.json")).unwrap(), neon_theme);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn mode_draft_refuses_while_declarative_locked() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let stage = unique_tmp("mode-draft-locked").join("stage");
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        // No marker file at all IS declarative (the safe default).
        let out = handle_mode_draft(&inv(&["rice", "mode", "draft"], &["neon-night"]));
        assert_eq!(out.status, Status::Error);
        assert_eq!(out.data.unwrap()["reason"], "declarative-mode-locked");
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn mode_draft_missing_name_is_usage() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let stage = unique_tmp("mode-draft-noname").join("stage");
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);
        save_mode_marker(&ModeMarker { mode: RiceMode::Staging, ..Default::default() }).unwrap();

        let out = handle_mode_draft(&inv(&["rice", "mode", "draft"], &[]));
        assert_eq!(out.status, Status::Usage);
        assert_eq!(out.data.unwrap()["reason"], "missing-name");
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn mode_draft_with_no_resolvable_rice_errors() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let stage = unique_tmp("mode-draft-norice").join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);
        save_mode_marker(&ModeMarker { mode: RiceMode::Staging, ..Default::default() }).unwrap();

        let out = handle_mode_draft(&inv(&["rice", "mode", "draft"], &["neon-night"]));
        assert_eq!(out.status, Status::Error);
        assert_eq!(out.data.unwrap()["reason"], "no-resolvable-rice");
    }

    // ── leaving Draft mode tears the symlink down ─────────────────────────

    #[test]
    fn mode_stage_tears_down_an_active_draft_symlink() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("mode-stage-teardown");
        let stage = root.join("stage");
        let rice = root.join("rices").join("sonata");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::create_dir_all(&rice).unwrap();
        std::fs::write(rice.join("theme.json"), VALID_NOTES).unwrap();
        // current_staged_rice() reads THIS file's own "rice" field, not the
        // marker's — handle_mode_draft needs it to resolve what to fork from.
        std::fs::write(
            stage.join("theme.json"),
            r##"{"schemaVersion":"0","rice":"sonata","palette":{"bg":"#seed"}}"##,
        )
        .unwrap();
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);
        save_mode_marker(&ModeMarker { mode: RiceMode::Staging, rice: Some("sonata".to_string()), ..Default::default() }).unwrap();

        handle_mode_draft(&inv(&["rice", "mode", "draft"], &["neon-night"]));
        assert!(std::fs::symlink_metadata(stage.join("theme.json")).unwrap().file_type().is_symlink());

        let out = handle_mode_stage(&inv(&["rice", "mode", "stage"], &["sonata"]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert!(
            !std::fs::symlink_metadata(stage.join("theme.json")).unwrap().file_type().is_symlink(),
            "the routing symlink must be gone — a real file now"
        );
        let marker = load_mode_marker();
        assert_eq!(marker.mode, RiceMode::Staging);
        assert_eq!(marker.draft, None);
        // The draft file itself is untouched — only the routing is torn down.
        let draft_theme = root.join("rices").join("sonata").join("drafts").join("neon-night").join("theme.json");
        assert!(draft_theme.is_file());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn mode_declarative_tears_down_an_active_draft_symlink() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("mode-declarative-teardown");
        let stage = root.join("stage");
        let rice = root.join("rices").join("sonata");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::create_dir_all(&rice).unwrap();
        std::fs::write(rice.join("theme.json"), VALID_NOTES).unwrap();
        // current_staged_rice() reads THIS file's own "rice" field, not the
        // marker's — handle_mode_draft needs it to resolve what to fork from.
        std::fs::write(
            stage.join("theme.json"),
            r##"{"schemaVersion":"0","rice":"sonata","palette":{"bg":"#seed"}}"##,
        )
        .unwrap();
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);
        save_mode_marker(&ModeMarker { mode: RiceMode::Staging, rice: Some("sonata".to_string()), ..Default::default() }).unwrap();

        handle_mode_draft(&inv(&["rice", "mode", "draft"], &["neon-night"]));
        assert!(std::fs::symlink_metadata(stage.join("theme.json")).unwrap().file_type().is_symlink());

        let out = handle_mode_declarative(&inv(&["rice", "mode", "declarative"], &[]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert!(
            !std::fs::symlink_metadata(stage.join("theme.json")).unwrap().file_type().is_symlink(),
            "the routing symlink must be gone — a real file now"
        );
        let marker = load_mode_marker();
        assert_eq!(marker.mode, RiceMode::Declarative);
        assert_eq!(marker.rice, Some("sonata".to_string()));
        assert_eq!(marker.draft, None);
        let staged = std::fs::read_to_string(stage.join("theme.json")).unwrap();
        assert!(staged.contains("\"accent\""), "re-pinned from the committed rices: {staged}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// `sonata` committed and staged, mode `Staging`. Callers own
    /// `remove_dir_all(&root)`.
    fn staged_sonata(tag: &str) -> (PathBuf, PathBuf) {
        let root = unique_tmp(tag);
        let stage = root.join("stage");
        let rice = root.join("rices").join("sonata");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::create_dir_all(&rice).unwrap();
        std::fs::write(rice.join("theme.json"), VALID_NOTES).unwrap();
        std::fs::write(
            stage.join("theme.json"),
            r##"{"schemaVersion":"0","rice":"sonata","palette":{"bg":"#seed"}}"##,
        )
        .unwrap();
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);
        save_mode_marker(&ModeMarker { mode: RiceMode::Staging, rice: Some("sonata".to_string()), ..Default::default() }).unwrap();
        (root, stage)
    }

    /// [`staged_sonata`], then routed into the draft `neon-night`.
    fn drafted_stage(tag: &str) -> (PathBuf, PathBuf) {
        let (root, stage) = staged_sonata(tag);
        let out = handle_mode_draft(&inv(&["rice", "mode", "draft"], &["neon-night"]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        (root, stage)
    }

    fn assert_still_drafted(stage: &std::path::Path) {
        assert!(
            std::fs::symlink_metadata(stage.join("theme.json")).unwrap().file_type().is_symlink(),
            "the routing symlink must survive a refused transition"
        );
        let marker = load_mode_marker();
        assert_eq!(marker.mode, RiceMode::Draft);
        assert_eq!(marker.draft, Some("neon-night".to_string()));
    }

    #[test]
    fn mode_stage_of_an_unknown_rice_leaves_the_draft_routed() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let (root, stage) = drafted_stage("mode-stage-bad-name-from-draft");

        let out = handle_mode_stage(&inv(&["rice", "mode", "stage"], &["nosuchrice"]));
        assert_eq!(out.status, Status::Error);
        assert_eq!(out.command, "rice.mode.stage");
        assert_still_drafted(&stage);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn mode_declarative_of_an_unknown_rice_leaves_the_draft_routed() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let (root, stage) = drafted_stage("mode-declarative-bad-name-from-draft");

        let out = handle_mode_declarative(&inv(&["rice", "mode", "declarative"], &["nosuchrice"]));
        assert_eq!(out.status, Status::Error);
        assert_eq!(out.command, "rice.mode.declarative");
        assert_still_drafted(&stage);
        let _ = std::fs::remove_dir_all(&root);
    }

    // ── the baseline take on draft entry ──────────────────────────────────

    #[test]
    fn entering_a_fresh_draft_mints_take_one_with_no_parent() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, stage) = staged_sonata("mode-draft-baseline-fresh");

        let out = handle_mode_draft(&inv(&["rice", "mode", "draft"], &["neon-night"]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert_eq!(out.data.as_ref().unwrap()["entry"], 1);
        assert!(out.changed.iter().any(|c| c.ends_with("neon-night/history/0001.json")), "{:?}", out.changed);

        let take = crate::history::load_history("sonata", Some("neon-night"), 1).unwrap();
        assert_eq!(take.parent, None);
        assert_eq!(take.cause, "enter");
        assert_eq!(take.theme["palette"]["bg"], "#seed");
        assert_eq!(std::fs::read_to_string(stage.join("theme.json")).unwrap(), std::fs::read_to_string(
            crate::paths::draft_dir("sonata", "neon-night").join("theme.json")).unwrap());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn re_entering_an_unchanged_draft_mints_nothing() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, _stage) = drafted_stage("mode-draft-baseline-reenter");

        let out = handle_mode_draft(&inv(&["rice", "mode", "draft"], &["neon-night"]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert!(out.data.as_ref().unwrap()["entry"].is_null());
        assert_eq!(crate::history::list_history("sonata", Some("neon-night")).len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_draft_saved_by_rice_draft_save_gets_its_baseline_on_entry() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, _stage) = staged_sonata("mode-draft-baseline-saved");
        let saved = super::super::draft::fork_stage_into("sonata", "neon-night");
        assert_eq!(saved.status, Status::Ok, "{:?}", saved.data);
        assert!(crate::history::list_history("sonata", Some("neon-night")).is_empty());

        let out = handle_mode_draft(&inv(&["rice", "mode", "draft"], &["neon-night"]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert_eq!(out.data.as_ref().unwrap()["entry"], 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rice_back_to_the_baseline_restores_the_forked_draft_after_a_reload() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, stage) = drafted_stage("mode-draft-baseline-back");
        let read = || -> Value { serde_json::from_str(&std::fs::read_to_string(stage.join("theme.json")).unwrap()).unwrap() };
        let forked = read();

        std::fs::write(
            stage.join("theme.json"),
            r##"{"schemaVersion":"0","rice":"sonata","palette":{"bg":"#edited"}}"##,
        )
        .unwrap();
        let reloaded = super::super::history::snapshot_if_identical_to_head("reload", "reload").unwrap().unwrap();
        assert_eq!((reloaded.entry, reloaded.parent), (2, Some(1)));

        let mut back = inv(&["rice", "back"], &[]);
        back.flags.insert("entry".to_string(), "1".to_string());
        let out = super::super::history::handle_rice_back(&back);
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert_eq!(read(), forked);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rice_stage_in_staging_mode_is_the_rice_a_bare_mode_stage_returns_to() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let (root, _stage) = pick_tmp("mode-rice-stage-records-staging-rice", &["nocturne", "cadenza", "sonata"]);

        let out = handle_mode_stage(&inv(&["rice", "mode", "stage"], &["nocturne"]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let out = super::super::rice::handle_rice_stage_entry(&inv(&["rice", "stage"], &["cadenza"]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let marker = load_mode_marker();
        assert_eq!((marker.rice.as_deref(), marker.staging_rice.as_deref()), (Some("cadenza"), Some("cadenza")));

        let out = handle_mode_declarative(&inv(&["rice", "mode", "declarative"], &["sonata"]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let out = handle_mode_stage(&inv(&["rice", "mode", "stage"], &[]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let marker = load_mode_marker();
        assert_eq!((marker.rice.as_deref(), marker.staging_rice.as_deref()), (Some("cadenza"), Some("cadenza")));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn stage_and_declarative_keep_the_inner_stage_data_and_add_their_own_keys() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let (root, _stage) = pick_tmp("mode-envelope-keeps-inner-data", &["sonata"]);

        let staged = handle_mode_stage(&inv(&["rice", "mode", "stage"], &["sonata"])).data.unwrap();
        for key in ["hyprctl", "terminal", "reload", "widgets", "registry", "mode", "rice", "seeded"] {
            assert!(staged.get(key).is_some(), "stage data lacks `{key}`: {staged}");
        }
        assert_eq!((staged["mode"].as_str(), staged["rice"].as_str()), (Some("staging"), Some("sonata")));

        let locked = handle_mode_declarative(&inv(&["rice", "mode", "declarative"], &["sonata"])).data.unwrap();
        for key in ["hyprctl", "terminal", "reload", "widgets", "registry", "mode", "rice"] {
            assert!(locked.get(key).is_some(), "declarative data lacks `{key}`: {locked}");
        }
        assert_eq!(locked["mode"], "declarative");
        let _ = std::fs::remove_dir_all(&root);
    }
}
