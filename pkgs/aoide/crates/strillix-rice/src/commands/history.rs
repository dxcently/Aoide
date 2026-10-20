//! `rice history` — the explicit snapshot command, and the two mutator cores every
//! later entry command (`rice back`, `rice history mark`, the auto-entry hooks in
//! `rice stage`/`wallpaper set`) is built on
//! (`references/fleshing-out-aoide-ricing.md` §5.2, phase A; the
//! branch-from-any-mark ask this whole feature exists to answer).
//!
//! **Locking discipline (advisor verdict, defect D2) — read this before
//! adding a caller.** [`snapshot_unlocked`] and [`snapshot_if_drifted_unlocked`]
//! never take `crate::lock::with_stage_lock` themselves — allocating a
//! entry number is a read-max/write-max+1 race exactly like the lock exists
//! for, and closing it is the CALLER-held boundary's job.
//! `with_stage_lock` nests for the thread that already holds it
//! (`aoide-storage/src/fs.rs`), so a core that locked internally would not
//! deadlock inside a caller's hold — it would simply move the atomic unit off
//! the caller and onto each core. `rice back` — which must
//! snapshot-if-drifted, write the revert,
//! AND advance the head cursor as one atomic unit — therefore owns one hold
//! around the whole body, and a caller that wrapped only part of its body
//! would silently get several sections instead of one.
//! [`snapshot`] is the ONLY locked entrypoint here — a single
//! `with_stage_lock` around [`snapshot_unlocked`], fine for any caller whose
//! write isn't already folded into someone else's locked mutator. That
//! covers this file's own `rice history` handler AND `rice stage`/`wallpaper
//! set`'s auto-entry hooks (phase A3, `commands/rice.rs`/`commands/wallpaper.rs`)
//! — both call [`snapshot`] plainly, unconditionally, on every successful
//! Draft-mode write, and fold the result into their outcome with
//! [`attach_entry`]. [`snapshot_if_drifted_unlocked`] carries NO locked
//! counterpart at all, deliberately not reintroduced, because it has
//! exactly ONE sanctioned caller: `rice back`, which must
//! snapshot-if-drifted, write the revert, AND advance the head cursor as
//! ONE atomic unit — the drift check has to fold into that SAME single
//! lock, not a second acquire, so it stays `_unlocked` and `rice back`
//! wraps `with_stage_lock` around its own whole body itself. The auto-entry
//! hooks are NOT a second caller of the drift check: an entry records every
//! write, not just the ones that changed something
//! (`crate::history`' own module doc: "minted on every rehearsal
//! write") — a content-identical restage still mints, and the resulting
//! noise is `rice history prune`'s problem (phase A9, §7.1), not write-time
//! suppression's. The drift check's whole reason to exist is different: A5
//! calls it because a revert is ABOUT TO OVERWRITE the stage and must
//! preserve unrecorded edits before destroying them. A3's hooks run AFTER a
//! write has already landed — nothing is about to be destroyed — so that
//! rationale never applied to them, and reusing the drift core there was
//! this file's own earlier mistake, corrected before landing. They record
//! what a write left, so the state BEFORE the first write of a draft has no
//! entry of its own: `rice mode draft` mints that baseline on entry through
//! [`snapshot_if_identical_to_head`] (cause `enter`), which is what makes the
//! first edit undoable.
//!
//! Everything else about the model — the parent pointer, the flat monotone
//! counter, the head cursor — lives in `crate::history`; this module
//! is CLI + orchestration on top of that pure store.
//!
//! **Command-name threading (phase A3, advisor-flagged).** [`snapshot_unlocked`],
//! [`snapshot`], and [`snapshot_if_drifted_unlocked`] all take a `cmd: &str`
//! first argument — the SAME dotted command name [`resolve_draft`] already
//! threads through, not the entry-store's own `cause` vocabulary. This was
//! harmless while `rice history` (cmd `"rice.history"`) was the only caller: every
//! refusal `Outcome` these cores build could safely hardcode that string.
//! Phase A3 adds a SECOND caller — the auto-entry hooks in `rice stage`/
//! `wallpaper set` — so a refusal bubbling out of an auto-entry must report the
//! command that actually invoked it (`"rice.stage"`, `"wallpaper.set"`), not
//! `"rice.history"`. Every caller passes its own dotted name straight through,
//! exactly like [`resolve_scope(cmd)`] already did and following the
//! `no_resolvable_rice(cmd)` precedent in `draft.rs`.

use aoide_protocol::Invocation;
use aoide_protocol::output::Outcome;
use aoide_protocol::pick;
use aoide_protocol::registry::{arg, cmd, flag, Registry};
use aoide_storage::fs as shellbridge;
use crate::mode::{self, RiceMode};
use crate::history::{self, HistoryRecord};
use aoide_storage::time::{now_iso_utc, parse_iso_utc};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use aoide_protocol::Door;

pub fn register(r: &mut Registry) {
    r.insert(cmd!(
        path: ["rice", "history"],
        summary: "Snapshot the routed draft's current theme+wallpaper as a new entry, hanging off the current head. Draft mode only — entries live inside the draft.",
        args: [],
        flags: [],
        gated: false,
        implemented: true,
        handler: handle_rice_entry,
        doors: &[Door::Cli, Door::Daemon, Door::Mcp],
    ));
    r.insert(cmd!(
        path: ["rice", "history", "list"],
        summary: "List every entry in the routed draft as a tree: number, parent, mark, cause, and the current head. Always the whole tree (fork 9 killed the partial/`--all` view — there is only one view now).",
        args: [],
        flags: [],
        gated: false,
        implemented: true,
        handler: handle_rice_entry_list,
        doors: &[Door::Cli, Door::Daemon, Door::Mcp],
    ));
    r.insert(cmd!(
        path: ["rice", "history", "mark"],
        summary: "Stamp a rehearsal-style letter (A-Z) on an entry via one atomic write of history/marks.json — never an entry-record rewrite. A letter already in use MOVES to the new entry (a normal correction, not an error). Defaults to the current head when --entry is omitted.",
        args: [arg!("letter", "string", true, "A single letter A-Z to stamp.")],
        flags: [flag!("entry", "int", "Entry number to mark; defaults to the current head.")],
        gated: false,
        implemented: true,
        handler: handle_rice_entry_mark,
        doors: &[Door::Cli, Door::Daemon, Door::Mcp],
    ));
    r.insert(cmd!(
        path: ["rice", "history", "diff"],
        summary: "Key-wise diff between a base entry's theme and the routed draft's CURRENT staged theme (added/removed/changed paths, never a text diff — key reordering is not a change). Base defaults to the nearest mark on the head's ancestry (falling back to the head's own parent, or an `ok` 'nothing to diff against' when there is neither); --entry N or --mark <letter> override it explicitly.",
        args: [],
        flags: [
            flag!("entry", "int", "Entry number to diff against, overriding the default base."),
            flag!("mark", "string", "Mark letter to diff against, resolved through history/marks.json."),
        ],
        gated: false,
        implemented: true,
        handler: handle_rice_entry_diff,
        doors: &[Door::Cli, Door::Daemon, Door::Mcp],
    ));
    r.insert(cmd!(
        path: ["rice", "history", "prune"],
        summary: "Prune entries so the store does not grow without bound: --older-than <Nd|Nh>, --keep <N>, and --all-but-marks select candidates from everything except the head and its whole ancestry, which are never selectable by any flag combination — re-checked at prune time against the live head, not just when the candidates were chosen. Given together, selector flags combine as AND (an entry must satisfy every one given). Marked entries are protected unless --force; only then does their letter also leave history/marks.json. Surviving children of a pruned entry re-parent to its parent, so ancestry keeps resolving. A bare invocation (no selector) opens a multi-select picker on a real CLI tty; off a tty it prints a dry run and changes nothing.",
        args: [],
        flags: [
            flag!("older-than", "string", "Prune entries older than this: `<N>d` (days) or `<N>h` (hours), e.g. `7d` or `12h`."),
            flag!("keep", "int", "Keep only the newest N entries outside the head's ancestry (the head and its ancestry always survive on top of this); the rest become candidates."),
            flag!("all-but-marks", "bool", "Select the whole eligible pool as candidates (the head/ancestry rail still applies; marked entries still need --force)."),
            flag!("force", "bool", "Also include marked entries in the candidate set; their letters are dropped from history/marks.json when pruned."),
        ],
        gated: false,
        implemented: true,
        handler: handle_rice_entry_prune,
    ));
    r.insert(cmd!(
        path: ["rice", "back"],
        summary: "Revert the routed draft's live stage to an earlier entry (--entry N or --mark <letter>) and move the head cursor there — the NEXT write hangs off it, branching implicitly with no branch command. unrecorded drift on the stage is snapshotted first so nothing is destroyed. Draft mode only. A bare `rice back` (neither flag) opens a numbered picker defaulting to the head's parent when run on a real CLI tty; off a tty (an agent door, or a script) it refuses with a usage error instead — flags/--json always bypass the picker either way.",
        args: [],
        flags: [
            flag!("entry", "int", "Entry number to revert to."),
            flag!("mark", "string", "Mark letter to revert to (resolved through history/marks.json)."),
        ],
        gated: false,
        implemented: true,
        handler: handle_rice_back,
    ));
}

/// The shared "must be staged or routed into a draft" guard every entry
/// command starts with: entries live inside `rices/<rice>/history/` when
/// staged directly, `rices/<rice>/drafts/<draft>/history/` when routed into
/// a draft ([`crate::history::history_dir`]) — the staging-mode reach
/// `strillix reload`'s design (settled 2026-08-31) extends `rice history`/`rice
/// back` to, so nothing about them is resolvable outside `Staging`/`Draft`
/// at all; `Declarative` has nothing unlocked to snapshot. `mode.json`'s
/// `draft` field is `Some` **iff** `mode == Draft` (`crate::mode`'s
/// own module doc records the invariant) — the `None` returned here for
/// `Staging` is that same fact, not a separate guess. `cmd` is the caller's
/// own dotted command name (`rice.history`, `rice.back`, …) so the refusal's
/// `Outcome` carries the command that actually issued it, not this shared
/// helper's.
pub(crate) fn resolve_scope(cmd: &str) -> Result<(String, Option<String>), Outcome> {
    let marker = mode::load_mode_marker();
    match (marker.mode, marker.rice, marker.draft) {
        (RiceMode::Draft, Some(rice), Some(draft)) => Ok((rice, Some(draft))),
        (RiceMode::Staging, Some(rice), _) => Ok((rice, None)),
        _ => Err(Outcome::error(
            cmd,
            "no rice currently staged or drafted — entries only exist while unlocked \
             (`strillix rice mode stage` or `strillix rice mode draft <name>` first)",
        )
        .with_data(json!({ "reason": "not-staged-or-drafted" }))),
    }
}

/// Render a `(rice, draft)` scope for a message: `<rice>/<draft>` when
/// routed into a draft, bare `<rice>` when staged directly (the staging-mode
/// entry/back reach, `strillix reload` design, settled 2026-08-31) — the single
/// formatting rule every entry/back/prune message in this file uses instead
/// of interpolating `draft` (an `Option<&str>`, with no `Display` of its
/// own) directly.
fn scope_label(rice: &str, draft: Option<&str>) -> String {
    match draft {
        Some(d) => format!("{rice}/{d}"),
        None => rice.to_string(),
    }
}

/// One comparison payload for [`diff_theme`] — folds theme+wallpaper+widgets
/// into a single `Value` so `strillix reload`'s dedupe-against-head
/// ([`snapshot_if_identical_to_head_unlocked`]) is ONE `diff_theme` call
/// over the whole captured content, never three separate comparisons (the
/// User's own framing, 2026-08-31: "reuses history diff's own key-wise
/// machinery, never a text diff, never a second differ").
fn entry_content_value(theme: &Value, wallpaper: &Option<Value>, widgets: &Value) -> Value {
    json!({ "theme": theme, "wallpaperPick": wallpaper, "widgets": widgets })
}

/// Read the routed draft's CURRENT `theme.json` (required — an absent or
/// unparseable stage theme is refused, there is nothing to snapshot) and
/// `wallpaper.json` (optional — absent simply means no wallpaper, not an error).
/// Both are read straight off [`crate::paths::stage_dir`], not the
/// draft directory itself: while `Draft` mode is routed, `stage/theme.json`
/// IS the draft's live content (a symlink `aoide_storage::fs::atomic_write`
/// resolves transparently, and a plain `read_to_string` follows the same
/// way) — reading the stage is reading the draft, with zero extra
/// symlink-awareness needed here.
fn read_staged_content(cmd: &str) -> Result<(Value, Option<Value>), Outcome> {
    let stage = crate::paths::stage_dir();
    let theme_path = stage.join("theme.json");
    let raw = std::fs::read_to_string(&theme_path).map_err(|e| {
        Outcome::error(
            cmd,
            format!("nothing staged to snapshot: cannot read {} ({e})", theme_path.display()),
        )
        .with_data(json!({
            "reason": "no-staged-theme",
            "expected": theme_path.to_string_lossy(),
        }))
    })?;
    let theme: Value = serde_json::from_str(&raw).map_err(|e| {
        Outcome::error(cmd, format!("staged theme.json is not valid JSON: {e}")).with_data(json!({
            "reason": "invalid-json",
            "theme": theme_path.to_string_lossy(),
        }))
    })?;

    let wallpaper_path = stage.join("wallpaper.json");
    let wallpaper: Option<Value> = if wallpaper_path.is_file() {
        std::fs::read_to_string(&wallpaper_path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
    } else {
        None
    };

    Ok((theme, wallpaper))
}

/// The unlocked snapshot core (see the module doc for why it is unlocked):
/// read the routed draft's current content, allocate the next entry number,
/// stamp it as a child of whatever the head cursor currently names, write
/// the record, and advance the head to it. `cmd` is the caller's own dotted
/// command name (`"rice.history"`, `"rice.stage"`, `"wallpaper.set"`, …), threaded
/// into every `Outcome` this builds so a refusal reports who actually asked
/// — see the module doc's "command-name threading" note. `cause` is the
/// entry-store's own event vocabulary (`"explicit"`, `"stage"`, `"wallpaper-set"`,
/// `"drift"` — distinct from an `Outcome` reason string,
/// `crate::history`' `HistoryRecord::cause` doc explains why).
pub(crate) fn snapshot_unlocked(cmd: &str, cause: &str) -> Result<HistoryRecord, Outcome> {
    let (rice, draft) = resolve_scope(cmd)?;
    let (theme, wallpaper) = read_staged_content(cmd)?;
    let widgets = crate::widgets::snapshot_widget_bodies(&rice).map_err(|e| {
        Outcome::error(cmd, format!("failed to snapshot widget bodies: {}", e.error))
            .with_data(json!({ "reason": "widget-snapshot-failed", "target": e.target }))
    })?;

    let entry = history::next_history_number(&rice, draft.as_deref());
    let record = HistoryRecord {
        schema_version: String::new(),
        entry,
        parent: history::load_head(&rice, draft.as_deref()),
        at: aoide_storage::time::now_iso_utc(),
        session_id: std::env::var("AOIDE_SESSION_ID").ok(),
        cause: cause.to_string(),
        theme,
        wallpaper,
        widgets,
    };

    history::save_history(&rice, draft.as_deref(), &record).map_err(|e| {
        Outcome::error(cmd, format!("failed to write entry {entry}: {e}"))
            .with_data(json!({ "reason": "write-failed" }))
    })?;
    history::save_head(&rice, draft.as_deref(), entry).map_err(|e| {
        Outcome::error(cmd, format!("failed to advance the head cursor: {e}"))
            .with_data(json!({ "reason": "write-failed" }))
    })?;

    Ok(record)
}

/// `rice history`'s locked entrypoint: exactly ONE `with_stage_lock` around
/// [`snapshot_unlocked`]'s whole read-allocate-write. Safe to call from any
/// site that is not itself already inside a locked mutator — see the module
/// doc for the rule and who must NOT call this. `cmd`/`cause` pass straight
/// through to [`snapshot_unlocked`].
pub(crate) fn snapshot(cmd: &str, cause: &str) -> Result<HistoryRecord, Outcome> {
    crate::lock::with_stage_lock(|| snapshot_unlocked(cmd, cause))
}

/// The unlocked drift-check core: mint an entry only when the routed draft's
/// CURRENT content differs from the head entry's own payload — comparing
/// parsed [`Value`]s, never raw bytes, so a document that merely
/// re-serialized (different key order, different whitespace) never reads as
/// drift. An absent head — an empty store, or a stale `head.json`
/// `crate::history::load_head` couldn't resolve at all — has no
/// existing entry to compare against, so it counts as drift unconditionally:
/// there is nothing on record yet, and the first content a store ever sees
/// is always worth capturing. `Ok(None)` is the no-op case; `Ok(Some(_))` is
/// the minted entry; `Err` is any of [`snapshot_unlocked`]'s own failures
/// (not routed, nothing staged, a write failure). `cmd` threads through the
/// same way as [`snapshot_unlocked`]'s own — see the module doc.
///
/// Widgets are deliberately OUT of this comparison: this core's one
/// sanctioned caller, [`back_unlocked`], never overwrites widget bodies on
/// revert (they stay git's substrate — see that function's own doc), so a
/// widget-only edit is never something a revert is about to destroy, and
/// there is nothing here for the drift check's "preserve what's about to be
/// overwritten" rationale to protect.
pub(crate) fn snapshot_if_drifted_unlocked(cmd: &str, cause: &str) -> Result<Option<HistoryRecord>, Outcome> {
    let (rice, draft) = resolve_scope(cmd)?;

    let head_entry =
        history::load_head(&rice, draft.as_deref()).and_then(|n| history::load_history(&rice, draft.as_deref(), n));
    if let Some(head_entry) = &head_entry {
        let (theme, wallpaper) = read_staged_content(cmd)?;
        if theme == head_entry.theme && wallpaper == head_entry.wallpaper {
            return Ok(None);
        }
    }

    snapshot_unlocked(cmd, cause).map(Some)
}

/// The dedupe core (staging/draft dispatch beat 1, `strillix reload` design
/// settled by the User 2026-08-31), shared by `strillix reload` and the draft
/// entry baseline (`rice mode draft`): mints an entry of the
/// currently staged theme+wallpaper+widget bodies UNLESS it would be key-wise
/// IDENTICAL to the current head — the "dedupe against head" rule the User
/// settled alongside the command itself, so an agent hammering `strillix reload`
/// with no intervening edit leaves one entry, not one per call.
///
/// Reuses [`diff_theme`] ([`entry_content_value`]'s single merged payload,
/// one call) rather than [`snapshot_if_drifted_unlocked`]'s own `==`
/// comparison — that core predates widget bodies and only ever compared
/// theme+wallpaper; this one folds all three fields the way the User's own
/// framing asks for ("reuses history diff's own key-wise machinery, never a
/// text diff, never a second differ"). `Ok(None)` is the dedupe no-op — the
/// caller still runs its sync/reload beats regardless, only the TAKE is
/// skipped. `cmd`/`cause` thread through like every other core in this file.
pub(crate) fn snapshot_if_identical_to_head_unlocked(cmd: &str, cause: &str) -> Result<Option<HistoryRecord>, Outcome> {
    let (rice, draft) = resolve_scope(cmd)?;

    let head_entry =
        history::load_head(&rice, draft.as_deref()).and_then(|n| history::load_history(&rice, draft.as_deref(), n));
    if let Some(head_entry) = &head_entry {
        let (theme, wallpaper) = read_staged_content(cmd)?;
        let widgets = crate::widgets::snapshot_widget_bodies(&rice).map_err(|e| {
            Outcome::error(cmd, format!("failed to snapshot widget bodies: {}", e.error))
                .with_data(json!({ "reason": "widget-snapshot-failed", "target": e.target }))
        })?;
        let before = entry_content_value(&head_entry.theme, &head_entry.wallpaper, &head_entry.widgets);
        let after = entry_content_value(&theme, &wallpaper, &widgets);
        if diff_theme(&before, &after).is_empty() {
            return Ok(None);
        }
    }

    snapshot_unlocked(cmd, cause).map(Some)
}

/// The dedupe core's locked entrypoint: exactly ONE `with_stage_lock` around
/// [`snapshot_if_identical_to_head_unlocked`]'s whole read-compare-write —
/// same shape as [`snapshot`], reused because neither `strillix reload` nor
/// `rice mode draft` is itself already inside a locked mutator.
pub(crate) fn snapshot_if_identical_to_head(cmd: &str, cause: &str) -> Result<Option<HistoryRecord>, Outcome> {
    crate::lock::with_stage_lock(|| snapshot_if_identical_to_head_unlocked(cmd, cause))
}

/// Fold the entry a write minted into that write's outcome, the one shape
/// `rice stage`, `wallpaper set` and `rice mode draft` share: the entry and head
/// files join `changed` (when `marker` names the rice and draft they live
/// under) and `data.entry` is the bare entry number, `null` when the dedupe core
/// minted nothing. A failed mint is never fatal — it reads `data.entry` null
/// and `data.entryError`, because the write it records already landed.
/// (`strillix reload` keeps its own `{entry, deduped}` object.)
pub(crate) fn attach_entry(out: &mut Outcome, marker: &mode::ModeMarker, minted: Result<Option<HistoryRecord>, Outcome>) {
    let entry = match minted {
        Ok(Some(record)) => {
            if let (Some(rice), Some(draft)) = (&marker.rice, &marker.draft) {
                out.changed.push(history::history_path(rice, Some(draft), record.entry).to_string_lossy().into_owned());
                out.changed.push(history::head_path(rice, Some(draft)).to_string_lossy().into_owned());
            }
            json!(record.entry)
        }
        Ok(None) => Value::Null,
        Err(err) => {
            if let Some(Value::Object(map)) = &mut out.data {
                map.insert("entryError".to_string(), json!(err.message));
            }
            Value::Null
        }
    };
    if let Some(Value::Object(map)) = &mut out.data {
        map.insert("entry".to_string(), entry);
    }
}

/// `rice history` — the explicit snapshot command (cause `"explicit"`). A bare
/// mint of whatever is currently staged in the routed draft; no selection,
/// no comparison, no revert — `rice back` is where reverting
/// and branching actually happen. This handler's own write is not folded
/// into anything else, so [`snapshot`]'s single lock is exactly right.
fn handle_rice_entry(_inv: &Invocation) -> Outcome {
    let (rice, draft) = match resolve_scope("rice.history") {
        Ok(v) => v,
        Err(o) => return o,
    };

    match snapshot("rice.history", "explicit") {
        Ok(record) => {
            let entry_file = history::history_path(&rice, draft.as_deref(), record.entry);
            let head_file = history::head_path(&rice, draft.as_deref());
            let message = match record.parent {
                Some(parent) => format!("entry {:04} minted — from entry {parent:04}", record.entry),
                None => format!("entry {:04} minted — the draft's first entry", record.entry),
            };
            Outcome::ok("rice.history", message)
                .changed(vec![
                    entry_file.to_string_lossy().into_owned(),
                    head_file.to_string_lossy().into_owned(),
                ])
                .with_data(json!({
                    "entry": record.entry,
                    "parent": record.parent,
                    "at": record.at,
                    "cause": record.cause,
                    "sessionId": record.session_id,
                }))
        }
        Err(o) => o,
    }
}

// ── `rice history list` — the tree (phase A6) ──────────────────────────────────

/// Every mark letter currently naming `entry` — usually zero or one, but nothing in
/// [`crate::history::save_marks`] forbids two letters landing on the same entry, so this
/// returns however many actually do, sorted (the map iterates by letter already, since it's a
/// `BTreeMap`). Shared between [`render_node`] and [`handle_rice_entry_list`]'s `--json` builder
/// so the two never compute this differently.
fn mark_letters_for(marks: &BTreeMap<String, u32>, entry: u32) -> Vec<String> {
    marks.iter().filter(|(_, v)| **v == entry).map(|(k, _)| k.clone()).collect()
}

/// Depth-first ASCII rendering of the entry tree's BODY — no header line (no rice/draft name to
/// print one with; the caller, [`handle_rice_entry_list`], prepends that) — so this stays a
/// **pure** function of exactly what an entry store already knows, unit-testable with zero
/// terminal and zero clock. Children are walked in ascending number order
/// ([`crate::history::children`], already sorted — reused, not reimplemented). A straight
/// single-child descent draws no branch glyph at all; only an actual fork (2+ children sharing a
/// parent) draws `├─`/`└─`, with a `│` continuation column threaded down the non-last branch —
/// the shape `references/fleshing-out-aoide-ricing.md` §3.2's mockup draws by hand. The head
/// entry is marked `← head`; a mark letter (or letters, see [`mark_letters_for`]) is bracketed
/// after the row. `marks` is the raw letter→entry map [`crate::history::load_marks`]
/// returns — a stale entry naming an entry absent from `entries` is simply never looked up (this
/// walks `entries` itself outward from its roots, never `marks`' own keys), so it is skipped for
/// free, exactly the self-healing read the advisor verdict (D6) requires, with no extra
/// filtering code anywhere in this function.
///
/// No `at`-based "2h ago" fuzzing (the mockup's cosmetic choice): a relative clock reading would
/// make this function impure and its own tests non-deterministic, which is the entire reason it
/// takes no `now` argument. The raw ISO-8601 `at` timestamp is printed verbatim instead.
///
/// **Orphans are swept in, never dropped.** `crate::history` is deliberately tolerant of
/// partial state (`list_history` survives a half-written record, `load_head` falls back on a stale
/// pointer) — a `parent` naming an entry number that no longer exists on disk is exactly that kind
/// of state, and `rice history list` is the command someone uses to FIND an entry to revert to, so a real
/// record that exists on disk must never render as nothing. The first pass walks outward from the
/// `parent == None` roots via [`crate::history::children`]; a second pass then sweeps every
/// entry number, ascending, that the first pass never reached and renders each as its own
/// top-level entry, labeled `detached` — this covers both a dangling parent pointer AND a pure
/// parent cycle with no root at all (two entries each naming the other), which the first pass can
/// never reach either. `visited` is threaded through both passes and is what makes the sweep safe
/// against that cycle case: [`render_node`] checks it before rendering anything, so once the
/// cycle's entry point renders once, walking back into it a second time is a no-op instead of
/// unbounded recursion.
pub fn render_tree(entries: &[HistoryRecord], head: Option<u32>, marks: &BTreeMap<String, u32>) -> String {
    let mut roots: Vec<u32> = entries.iter().filter(|t| t.parent.is_none()).map(|t| t.entry).collect();
    roots.sort_unstable();

    let mut visited: BTreeSet<u32> = BTreeSet::new();
    let mut out = String::new();
    for r in roots {
        render_node(entries, head, marks, r, "", "", None, &mut visited, &mut out);
    }

    let mut numbers: Vec<u32> = entries.iter().map(|t| t.entry).collect();
    numbers.sort_unstable();
    for n in numbers {
        if visited.contains(&n) {
            continue;
        }
        let note = detached_note(entries, n);
        render_node(entries, head, marks, n, "", "", Some(&note), &mut visited, &mut out);
    }
    out
}

/// The label a swept-in [`render_tree`] entry carries, describing WHY it never had a real root to
/// hang off: its own `parent` names an entry absent from the store, or (the cycle case) `parent`
/// names an entry that itself, transitively, leads back here. Either way the caller only needs to
/// know it's detached and why, not walk the ancestry again — this is a one-shot diagnostic, not
/// part of the tree walk itself.
fn detached_note(entries: &[HistoryRecord], n: u32) -> String {
    match entries.iter().find(|t| t.entry == n).and_then(|t| t.parent) {
        Some(p) if entries.iter().any(|t| t.entry == p) => "detached (cyclic ancestry)".to_string(),
        Some(p) => format!("detached (parent {p:04} missing)"),
        None => "detached".to_string(),
    }
}

/// One row plus its subtree — see [`render_tree`]'s doc for the glyph rule and the sweep/`visited`
/// mechanism. `connector` is `""` (root, a straight single-child continuation, or a swept-in
/// detached entry), `"├─"` (a non-last fork branch), or `"└─"` (the last fork branch); `prefix` is
/// everything already laid down by ancestors. `detached` is `Some(note)` only for a sweep's own
/// entry call — every recursive call this function makes for a child passes `None`, so the label
/// marks exactly the entry that had no real root, never the entries hanging off it. Returns
/// immediately, without rendering, if `n` is already in `visited` — this is what stops the sweep
/// from spinning forever the instant it walks into a parent cycle — or if `n` isn't actually in
/// `entries` (defensive; every real caller only ever passes numbers [`crate::history::children`]
/// itself just returned, or a number [`render_tree`]'s own sweep just confirmed is present).
fn render_node(
    entries: &[HistoryRecord],
    head: Option<u32>,
    marks: &BTreeMap<String, u32>,
    n: u32,
    prefix: &str,
    connector: &str,
    detached: Option<&str>,
    visited: &mut BTreeSet<u32>,
    out: &mut String,
) {
    if visited.contains(&n) {
        return;
    }
    let Some(rec) = entries.iter().find(|t| t.entry == n) else { return };
    visited.insert(n);

    let letters = mark_letters_for(marks, n);
    let mark_part = if letters.is_empty() { String::new() } else { format!(" [{}]", letters.join(",")) };
    let head_part = if head == Some(n) { " \u{2190} head" } else { "" };
    let detached_part = detached.map(|note| format!(" {note}")).unwrap_or_default();

    out.push_str(prefix);
    out.push_str(connector);
    out.push_str(&format!(
        "{:04}  {}  {}{mark_part}{head_part}{detached_part}\n",
        rec.entry, rec.at, rec.cause
    ));

    let next_prefix = match connector {
        "├─" => format!("{prefix}\u{2502} "),
        "└─" => format!("{prefix}  "),
        _ => prefix.to_string(),
    };

    let kids = history::children(entries, n);
    match kids.len() {
        0 => {}
        1 => render_node(entries, head, marks, kids[0], &next_prefix, "", None, visited, out),
        _ => {
            let last = kids.len() - 1;
            for (i, k) in kids.iter().enumerate() {
                let c = if i == last { "└─" } else { "├─" };
                render_node(entries, head, marks, *k, &next_prefix, c, None, visited, out);
            }
        }
    }
}

/// `rice history list [--json]` — the tree, always in full. Fork 9 decided "whole tree by default"
/// over an ancestry-only view, which killed the plan's originally-reserved `--all` flag outright
/// (advisor verdict D5a): there is only one view now, so no flag selects it. Refuses outside
/// `Draft` mode via [`resolve_draft`], the same guard every other entry command opens with — entries
/// live inside a routed draft's `history/`, nowhere else. An empty store (`Draft` mode entered,
/// nothing ever entered yet) is `ok` with an empty `entries` array, never an error — the `rice draft
/// list` precedent (`draft.rs::handle_draft_list`, no drafts found is `ok` too).
fn handle_rice_entry_list(_inv: &Invocation) -> Outcome {
    let (rice, draft) = match resolve_scope("rice.history.list") {
        Ok(v) => v,
        Err(o) => return o,
    };

    let recs = history::list_history(&rice, draft.as_deref());
    let head = history::load_head(&rice, draft.as_deref());
    let marks = history::load_marks(&rice, draft.as_deref());

    let head_label = head.map(|n| format!("{n:04}")).unwrap_or_else(|| "-".to_string());
    let scope = scope_label(&rice, draft.as_deref());
    let mut message = format!("{} entry(s) for {scope} \u{2014} head \u{2192} {head_label}", recs.len());
    let tree = render_tree(&recs, head, &marks);
    if !tree.is_empty() {
        message.push_str("\n\n");
        message.push_str(tree.trim_end());
    }

    let entries: Vec<Value> = recs
        .iter()
        .map(|t| {
            let letters = mark_letters_for(&marks, t.entry);
            let mark_value = if letters.is_empty() { Value::Null } else { Value::String(letters.join(",")) };
            json!({
                "entry": t.entry,
                "parent": t.parent,
                "mark": mark_value,
                "at": t.at,
                "sessionId": t.session_id,
                "cause": t.cause,
            })
        })
        .collect();

    Outcome::ok("rice.history.list", message)
        .with_data(json!({ "rice": rice, "draft": draft, "head": head, "history": entries }))
}

/// A single letter `A`-`Z` — [`handle_rice_entry_mark`]'s whole validation of
/// its positional arg. Upper-case-only and single-character, on purpose:
/// rehearsal marks in an actual score are always capitals, and the plan's
/// own vocabulary never speaks of a "lowercase mark" or a multi-letter one.
/// Rejected outright rather than silently normalized (`.to_uppercase()`)
/// so a typo (`rice history mark a`) surfaces as an error instead of quietly
/// landing on the letter the caller didn't mean to type.
fn valid_mark_letter(s: &str) -> bool {
    let mut chars = s.chars();
    matches!((chars.next(), chars.next()), (Some(c), None) if c.is_ascii_uppercase())
}

/// The unlocked mark core (locking discipline: see the module doc's
/// "Locking discipline" section — this is this file's OTHER
/// read-modify-write besides the snapshot cores, and gets the identical
/// treatment). Stamping a mark is `history::load_marks` → mutate one entry →
/// `history::save_marks`: a read-then-write of `history/marks.json`, racy the
/// same way entry-number allocation is if two `rice history mark` calls (or a
/// mark racing a prune) interleave unlocked — so it never locks itself,
/// and [`mark`] below is the one locked entrypoint.
///
/// `target` is already resolved by the caller (`--entry N`, parsed, or the
/// current head) — this function's own job is only to confirm `target`
/// names an entry that actually exists (`entry-not-found` if not), stamp the
/// letter, and report whether it moved. Returns `(target, moved, previous)`
/// where `previous` is whatever entry the letter named before this call, if
/// any — `None` for a fresh stamp, `Some(old)` for a move (`moved` is false
/// when `previous == Some(target)`: re-stamping a letter onto the entry it
/// already names is a no-op affirmation, not a move).
///
/// Entry files are **never** rewritten here — the whole reason marks live in
/// `history/marks.json` rather than a field on `HistoryRecord` (advisor verdict,
/// fork 4 / D6, recorded in the module doc): a mark stamp or move is this
/// ONE `save_marks` call, full stop.
pub(crate) fn mark_unlocked(cmd: &str, letter: &str, target: u32) -> Result<(u32, bool, Option<u32>), Outcome> {
    let (rice, draft) = resolve_scope(cmd)?;

    if history::load_history(&rice, draft.as_deref(), target).is_none() {
        return Err(Outcome::error(cmd, format!("entry {target:04} does not exist — nothing to mark"))
            .with_data(json!({ "reason": "entry-not-found", "entry": target })));
    }

    let mut marks = history::load_marks(&rice, draft.as_deref());
    let previous = marks.get(letter).copied();
    let moved = previous.is_some_and(|p| p != target);
    marks.insert(letter.to_string(), target);
    history::save_marks(&rice, draft.as_deref(), &marks).map_err(|e| {
        Outcome::error(cmd, format!("failed to write marks.json: {e}"))
            .with_data(json!({ "reason": "write-failed" }))
    })?;

    Ok((target, moved, previous))
}

/// `rice history mark`'s locked entrypoint: exactly ONE `with_stage_lock`
/// around [`mark_unlocked`]'s whole read-mutate-write. `cmd`/`letter`/
/// `target` pass straight through.
pub(crate) fn mark(cmd: &str, letter: &str, target: u32) -> Result<(u32, bool, Option<u32>), Outcome> {
    crate::lock::with_stage_lock(|| mark_unlocked(cmd, letter, target))
}

/// `rice history mark <letter> [--entry N]` — the phase-A standalone command
/// (`references/fleshing-out-aoide-ricing.md` §5.2/§10: phase B's `rice
/// score` `mark` step CALLS this later; it does not reimplement it, and
/// this handler is not itself part of that state machine).
///
/// `--entry N` names the target explicitly; omitted, it defaults to the
/// CURRENT HEAD (`crate::history::load_head`) — "mark where I am
/// right now" is the common case. An empty store (no head at all, nothing
/// ever entered) reports `entry-not-found`: from the caller's point of view
/// "no head to default to" and "the named entry doesn't exist" are the same
/// fact, so they share the one reason string rather than inventing a
/// second for what is really the same failure.
///
/// Dual-entrance per the project's rule: flags/`--json` only, no prompting,
/// no stdin read ever — the interactive picker belongs to `rice back`'s
/// bare-tty branch alone, never to this explicit command.
fn handle_rice_entry_mark(inv: &Invocation) -> Outcome {
    let letter = match inv.args.first() {
        Some(l) => l.clone(),
        None => {
            return Outcome::usage(
                "rice.history.mark",
                "usage: strillix rice history mark <letter A-Z> [--entry N] [--json]",
            )
            .with_data(json!({ "reason": "missing-mark" }));
        }
    };
    if !valid_mark_letter(&letter) {
        return Outcome::error(
            "rice.history.mark",
            format!("`{letter}` is not a valid mark: must be a single letter A-Z"),
        )
        .with_data(json!({ "reason": "invalid-mark", "mark": letter }));
    }

    let (rice, draft) = match resolve_scope("rice.history.mark") {
        Ok(v) => v,
        Err(o) => return o,
    };

    let target = match inv.flags.get("entry") {
        Some(raw) => match raw.parse::<u32>() {
            Ok(n) => n,
            Err(_) => {
                return Outcome::usage(
                    "rice.history.mark",
                    format!("`--entry {raw}` is not a valid entry number"),
                )
                .with_data(json!({ "reason": "invalid-entry", "entry": raw }));
            }
        },
        None => match history::load_head(&rice, draft.as_deref()) {
            Some(h) => h,
            None => {
                return Outcome::error(
                    "rice.history.mark",
                    "no entries exist yet for this draft — nothing to mark (`strillix rice history` first)",
                )
                .with_data(json!({ "reason": "entry-not-found" }));
            }
        },
    };

    match mark("rice.history.mark", &letter, target) {
        Ok((entry, moved, previous)) => {
            let marks_file = history::marks_path(&rice, draft.as_deref()).to_string_lossy().into_owned();
            let message = if moved {
                format!(
                    "mark {letter} moved from entry {:04} to entry {entry:04}",
                    previous.expect("moved implies a previous entry")
                )
            } else {
                format!("mark {letter} stamped on entry {entry:04}")
            };
            Outcome::ok("rice.history.mark", message)
                .changed(vec![marks_file])
                .with_data(json!({
                    "mark": letter,
                    "entry": entry,
                    "moved": moved,
                    "from": previous,
                }))
        }
        Err(o) => o,
    }
}

// ── `rice history diff` — key-wise theme diff (phase A7) ─────────────────────

/// Pure key-wise diff between two theme documents, walking dotted paths through nested
/// objects. Returns one entry per path where the two documents differ: `(dotted_path,
/// old_value, new_value)`, where `None` on either side means the key is absent there — added
/// when `old` is `None`, removed when `new` is `None`, changed when both are `Some` but differ.
///
/// Deliberately a VALUE walk, never a text differ (advisor verdict, fork 7): themes are
/// shallow key/value trees, and a text diff of pretty-printed JSON reports key REORDERING as a
/// change — `serde_json::to_string_pretty` makes no promise about key order surviving a
/// round-trip, so a text diff would be a false-positive generator for reviewer agents flagging
/// changes nobody made. Comparing parsed [`Value`]s side by side, keyed by name rather than by
/// line, is immune to that by construction: two documents with the same keys in different orders
/// produce an empty diff here, exactly the same "compare values, not bytes" discipline
/// [`snapshot_if_drifted_unlocked`]'s own `theme == head_entry.theme` check already relies on
/// above in this file.
///
/// An equal value at any level (an `a == b` bailout before the match) recurses no further, which
/// is also what keeps whole untouched subtrees out of the output even when they are nested deep.
/// A key present as an object on BOTH sides recurses into it, appending its own child keys to the
/// path with `.`; anything else (a scalar, an array, or a type change — an object on one side and
/// something else on the other) is a single leaf entry at its own path, `old`/`new` holding the
/// two whole values verbatim. Arrays are compared as opaque values, never walked element by
/// element — nothing in the plan or the shallow-tree theme shape this exists to serve asks for
/// array-index diffing.
pub fn diff_theme(a: &Value, b: &Value) -> Vec<(String, Option<Value>, Option<Value>)> {
    let mut out = Vec::new();
    diff_at(String::new(), a, b, &mut out);
    out
}

/// [`diff_theme`]'s own recursive walk. `prefix` is the dotted path built up by every ancestor
/// call so far (empty at the root). Bails out immediately when `a == b` — the check that makes
/// key reordering a no-op, since `serde_json::Value`'s own `PartialEq` for an object compares
/// keys and values, never insertion order.
fn diff_at(prefix: String, a: &Value, b: &Value, out: &mut Vec<(String, Option<Value>, Option<Value>)>) {
    if a == b {
        return;
    }
    match (a, b) {
        (Value::Object(am), Value::Object(bm)) => {
            let mut keys: BTreeSet<&String> = am.keys().collect();
            keys.extend(bm.keys());
            for k in keys {
                let path = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
                match (am.get(k), bm.get(k)) {
                    (Some(av), Some(bv)) => diff_at(path, av, bv, out),
                    (Some(av), None) => out.push((path, Some(av.clone()), None)),
                    (None, Some(bv)) => out.push((path, None, Some(bv.clone()))),
                    (None, None) => unreachable!("k came from am's or bm's own keys"),
                }
            }
        }
        _ => out.push((prefix, Some(a.clone()), Some(b.clone()))),
    }
}

/// Compact single-line rendering of one side of a diff entry for [`handle_rice_entry_diff`]'s text
/// message — `serde_json::to_string` (not the pretty printer), so a nested value stays on one row
/// per diff line instead of spilling the report across many.
fn render_diff_value(v: &Value) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "?".to_string())
}

/// Resolve `rice history diff`'s comparison base. `--entry N` names it directly (`entry-not-found` if
/// absent); `--mark X` resolves through `history/marks.json` exactly like [`back_unlocked`]'s own
/// resolution (`mark-not-found` for an unstamped letter, or one naming an entry that no longer
/// exists — the map's self-healing read, D6). With neither flag, the default is the NEAREST MARK
/// ON THE HEAD'S ANCESTRY (plan §3.3): walk [`crate::history::ancestry`] from the head
/// itself — its own first element — back toward the root, and stop at the first entry carrying any
/// mark letter. Checking the head first is deliberate: a head that was just marked diffs against
/// itself, and "what changed since the last mark" is correctly empty for un-staged content in
/// that case. When nothing in the WHOLE ancestry carries a mark, the fallback is the head's own
/// PARENT, unmarked (`anc[1]`, since `anc[0]` is the head itself) — and `Ok(None)` ("nothing to
/// diff against") only when the head has no parent either (the store's own unmarked root, or an
/// empty store with no head at all).
///
/// A GLOBAL-MAX letter — the highest mark stamped anywhere in the draft, on ANY branch — is
/// deliberately NOT what this resolves to: under a tree, a higher letter can sit on a branch the
/// head never descends from, and comparing against it would report changes that were never made
/// on the head's own line at all. Only the ancestry walk decides; `marked` below is consulted as
/// a plain membership set, never sorted or compared by letter.
fn resolve_base(
    cmd: &str,
    rice: &str,
    draft: Option<&str>,
    entry_flag: Option<u32>,
    mark_flag: Option<String>,
    head: Option<u32>,
) -> Result<Option<u32>, Outcome> {
    if let Some(n) = entry_flag {
        return if history::load_history(rice, draft, n).is_some() {
            Ok(Some(n))
        } else {
            Err(Outcome::error(cmd, format!("entry {n:04} does not exist — nothing to diff against"))
                .with_data(json!({ "reason": "entry-not-found", "entry": n })))
        };
    }
    if let Some(letter) = mark_flag {
        let marks = history::load_marks(rice, draft);
        return match marks.get(&letter).copied() {
            Some(n) if history::load_history(rice, draft, n).is_some() => Ok(Some(n)),
            _ => Err(Outcome::error(cmd, format!("mark `{letter}` is not stamped on any entry"))
                .with_data(json!({ "reason": "mark-not-found", "mark": letter }))),
        };
    }

    let Some(head) = head else {
        return Ok(None); // an empty store — nothing minted yet, nothing to diff against.
    };
    let all = history::list_history(rice, draft);
    let marks = history::load_marks(rice, draft);
    let marked: BTreeSet<u32> = marks.values().copied().collect();
    let anc = history::ancestry(&all, head); // [head, parent, grandparent, ..., root]
    if let Some(&nearest) = anc.iter().find(|n| marked.contains(n)) {
        return Ok(Some(nearest));
    }
    Ok(anc.get(1).copied()) // no mark anywhere on the ancestry — the head's own parent, if any.
}

/// `rice history diff [--entry N | --mark <letter>] [--json]` — the reviewer's view (plan §3.3):
/// "what changed since the last mark". Read-only, so unlike every mutator above in this file it
/// takes NO lock at all — nothing here writes, and [`resolve_base`]/[`read_staged_content`] are
/// both plain reads of state some other locked mutator already made durable.
///
/// Flags are syntax-validated first, same order [`handle_rice_back`] uses and for the same
/// reason: a malformed `--entry`/`--mark` is a usage error regardless of draft state. Then
/// [`resolve_draft`] gates on Draft mode, [`resolve_base`] picks the comparison base (see its own
/// doc for the ancestral-mark default), and — only once a real base is in hand — the routed
/// draft's CURRENT staged content is read via [`read_staged_content`], the same seam every other
/// entry command in this file reads through: comparing against the stage is comparing against the
/// live draft, exactly like [`snapshot_if_drifted_unlocked`]'s own drift check does.
fn handle_rice_entry_diff(inv: &Invocation) -> Outcome {
    let entry_flag = match inv.flags.get("entry") {
        Some(raw) => match raw.parse::<u32>() {
            Ok(n) => Some(n),
            Err(_) => {
                return Outcome::usage("rice.history.diff", format!("`--entry {raw}` is not a valid entry number"))
                    .with_data(json!({ "reason": "invalid-entry", "entry": raw }));
            }
        },
        None => None,
    };
    let mark_flag = match inv.flags.get("mark") {
        Some(letter) => {
            if !valid_mark_letter(letter) {
                return Outcome::error(
                    "rice.history.diff",
                    format!("`{letter}` is not a valid mark: must be a single letter A-Z"),
                )
                .with_data(json!({ "reason": "invalid-mark", "mark": letter }));
            }
            Some(letter.clone())
        }
        None => None,
    };

    let (rice, draft) = match resolve_scope("rice.history.diff") {
        Ok(v) => v,
        Err(o) => return o,
    };

    let head = history::load_head(&rice, draft.as_deref());
    let base = match resolve_base("rice.history.diff", &rice, draft.as_deref(), entry_flag, mark_flag, head) {
        Ok(v) => v,
        Err(o) => return o,
    };

    let Some(base) = base else {
        return Outcome::ok(
            "rice.history.diff",
            "nothing to diff against — the head has no marked ancestor and no parent \
             (the draft's first entry, never marked)",
        )
        .with_data(json!({ "base": Value::Null, "diff": [] }));
    };

    let (staged, _wallpaper) = match read_staged_content("rice.history.diff") {
        Ok(v) => v,
        Err(o) => return o,
    };
    let base_record = history::load_history(&rice, draft.as_deref(), base).expect("resolve_base only returns an existing entry");

    let diff = diff_theme(&base_record.theme, &staged);
    let message = if diff.is_empty() {
        format!("no change since entry {base:04}")
    } else {
        let mut lines = vec![format!("diff since entry {base:04} ({} key(s) changed):", diff.len())];
        for (path, old, new) in &diff {
            let line = match (old, new) {
                (None, Some(n)) => format!("  + {path}: {}", render_diff_value(n)),
                (Some(o), None) => format!("  - {path}: {}", render_diff_value(o)),
                (Some(o), Some(n)) => format!("  ~ {path}: {} -> {}", render_diff_value(o), render_diff_value(n)),
                (None, None) => unreachable!("diff_theme never emits a (None, None) entry"),
            };
            lines.push(line);
        }
        lines.join("\n")
    };

    let entries: Vec<Value> = diff
        .iter()
        .map(|(path, old, new)| json!({ "path": path, "old": old, "new": new }))
        .collect();

    Outcome::ok("rice.history.diff", message).with_data(json!({ "base": base, "diff": entries }))
}

// ── `rice history prune` — the pressure valve (phase A9, §7.1) ────────────────
//
// **Design note, since the plan leaves the exact combination un-spelled-out:**
// every OTHER command in this file draws a hard line between "a flag was given"
// (act immediately, no prompt, no stdin) and "no flag was given" (the tty
// picker, or off a tty, a refusal/report) — `rice back`'s `--entry`/`--mark`
// vs. its bare dual entrance is the precedent this mirrors exactly. Prune
// follows the identical split: `--older-than`/`--keep`/`--all-but-marks`
// are agent-facing and act at once once parsed; the "dry-run shape ... with
// a confirm" the plan describes IS the bare (no selector) path — on a tty
// the multi-select picker's own act of choosing rows *is* the confirm (the
// same shape `rice back`'s picker already uses, no separate y/n prompt —
// this repo has none anywhere and takes no new dependency to add one); off
// a tty it degrades to printing the same candidate report and touching
// nothing, per the plan's own explicit non-tty rule. When more than one
// selector flag is given they combine as AND (an entry must satisfy every
// given criterion to be a candidate) — the conservative reading, and the
// only one consistent with `--all-but-marks` (which selects the *whole*
// eligible pool) composing sensibly with a narrower `--older-than`/`--keep`
// alongside it rather than fighting it.

/// Parse `--older-than <Nd|Nh>`: a positive integer immediately followed by
/// exactly one unit letter, `d` (days) or `h` (hours) — `"7d"`, `"12h"`.
/// Returns the threshold in seconds. Anything else (empty, no digits, a
/// zero count, a third unit, trailing junk) is `None`, which the caller
/// turns into a usage error rather than silently rounding or defaulting —
/// the same "malformed flag is a usage error before anything else runs"
/// discipline `handle_rice_back`'s own `--entry`/`--mark` parsing uses.
fn parse_older_than(raw: &str) -> Option<u64> {
    if raw.len() < 2 {
        return None;
    }
    let (digits, unit) = raw.split_at(raw.len() - 1);
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let n: u64 = digits.parse().ok()?;
    if n == 0 {
        return None;
    }
    match unit {
        "d" => Some(n * 86400),
        "h" => Some(n * 3600),
        _ => None,
    }
}

/// UTC wall-clock now as Unix epoch seconds — reuses
/// [`aoide_storage::time::now_iso_utc`]/[`parse_iso_utc`] (the format/parse
/// pair `aoide-storage` already exports) rather than growing a third
/// almost-identical epoch helper in this crate for one comparison.
fn now_epoch() -> i64 {
    parse_iso_utc(&now_iso_utc()).unwrap_or(0)
}

/// Everything a prune decision needs, computed off one unlocked read of the
/// store (the same read-then-decide shape [`resolve_base`]/[`entry_lane_rows`]
/// already use elsewhere in this file — nothing here writes).
struct PrunePlan {
    /// The head and its whole ancestry (`crate::history::ancestry`,
    /// walked from the head) — every one of these is excluded BEFORE any
    /// selector runs, and no flag, including `--force`, can put one back in.
    /// This is the never-prune-the-ground-you-are-standing-on rail.
    protected_ancestry: BTreeSet<u32>,
    /// The entries a selector actually picked — `--older-than`/`--keep`/
    /// `--all-but-marks` narrowing the eligible pool (all entries minus
    /// `protected_ancestry`), already filtered to drop a marked entry unless
    /// `force` was set. This is the real candidate set any confirm/selection
    /// acts on.
    candidates: Vec<HistoryRecord>,
    /// The subset withheld from `candidates` purely because it carries a
    /// mark and `force` was not given — reported so a dry run/report can say
    /// how many are being protected and why.
    protected_by_mark: Vec<HistoryRecord>,
}

/// Build a [`PrunePlan`] for the routed draft. `older_than`/`keep` are
/// already-parsed selector values (`None` when their flag was absent);
/// `all_but_marks` selects the whole eligible pool (still subject to the
/// ancestry rail and the mark filter) — see the module banner above for why
/// multiple given selectors combine as AND. `force` decides whether a
/// marked entry can survive into `candidates` at all.
fn plan_prune(rice: &str, draft: Option<&str>, older_than: Option<u64>, keep: Option<usize>, force: bool) -> PrunePlan {
    let all = history::list_history(rice, draft);
    let head = history::load_head(rice, draft);
    let marks = history::load_marks(rice, draft);
    let marked: BTreeSet<u32> = marks.values().copied().collect();

    let protected_ancestry: BTreeSet<u32> =
        head.map(|h| history::ancestry(&all, h).into_iter().collect()).unwrap_or_default();

    let mut pool: Vec<HistoryRecord> = all.iter().filter(|t| !protected_ancestry.contains(&t.entry)).cloned().collect();
    pool.sort_by_key(|t| t.entry); // ascending — oldest first, newest last.

    if let Some(n) = keep {
        // The newest `n` (by entry number — the store's own monotone mint
        // order) survive; truncating to the front drops them from the tail,
        // leaving the "beyond keep" pool as candidates. `n >= pool.len()`
        // saturates to 0, a no-op truncate — nothing is a candidate then.
        let cut = pool.len().saturating_sub(n);
        pool.truncate(cut);
    }
    if let Some(threshold) = older_than {
        let now = now_epoch();
        pool.retain(|t| match parse_iso_utc(&t.at) {
            Some(at) => (now - at).max(0) as u64 >= threshold,
            None => false, // an unreadable `at` is never "old enough" — fail safe.
        });
    }

    let (protected_by_mark, candidates): (Vec<HistoryRecord>, Vec<HistoryRecord>) =
        pool.iter().cloned().partition(|t| !force && marked.contains(&t.entry));

    PrunePlan { protected_ancestry, candidates, protected_by_mark }
}

/// The result of an actual prune write, assembled inside the lock so nothing
/// here re-reads state the write already changed underneath it (mirrors
/// [`BackResult`]'s own discipline).
struct PruneResult {
    pruned: Vec<u32>,
    /// `(entry, old_parent, new_parent)` for every SURVIVING entry the splice
    /// actually re-parented.
    reparented: Vec<(u32, Option<u32>, Option<u32>)>,
    dropped_marks: Vec<String>,
    changed: Vec<String>,
    /// Candidates the CALLER handed in (a [`PrunePlan`] computed unlocked,
    /// possibly seconds or minutes earlier — the tty picker blocks on human
    /// input with no lock held in between) that turned out to be the head or
    /// on its ancestry by the time this actually ran, and were silently
    /// dropped rather than pruned — see [`prune_unlocked`]'s own doc for why
    /// a stale plan can disagree with the live store here.
    skipped_now_protected: Vec<u32>,
}

/// The unlocked prune core: given an already-decided candidate set (from a
/// [`PrunePlan`], or hand-picked via the tty picker), splice each one out
/// with [`crate::history::reparent`], persist every surviving entry
/// whose `parent` the splice actually changed, delete every pruned entry's
/// own file, and rewrite `history/marks.json` in the SAME pass to drop any
/// letter naming an entry this call just removed (advisor verdict D6 — the
/// prune is exactly the place that knows).
///
/// **The head-and-ancestry rail is re-checked HERE, against a FRESH read,
/// not trusted from `doomed`.** [`plan_prune`] computes its own
/// `protected_ancestry` unlocked, and the caller that turns a plan into this
/// call's `doomed` list can be arbitrarily far removed in time from this
/// function actually running — most concretely, [`prune_picker`] sits
/// blocked on [`pick::choose_many`] (human input, no lock held) between
/// computing the plan and calling this. A second door can move the head in
/// that window (e.g. another agent's `rice back --entry 4` while a human is
/// still staring at a picker that offered entry 4 as a candidate), which
/// would make a plan-time-only check breach "never prune the head or its
/// ancestry, under ANY flag combination" — the ancestry it checked against
/// is no longer the live one. So: `before`/`head` are read fresh right here,
/// `protected_now` is derived from THAT read, and any element of `doomed`
/// it contains is stripped before anything is spliced or deleted — the same
/// fail-safe-against-a-race posture `history::load_head`'s stale-pointer
/// fallback and [`plan_prune`]'s unreadable-`at` skip already use elsewhere
/// in this feature, rather than trusting a decision that may already be
/// wrong.
///
/// The (now-filtered) `doomed` is folded one at a time through `reparent`,
/// and this is order-independent by construction:
/// [`crate::history::reparent`] always looks up the CURRENT (possibly
/// already-spliced) parent of the entry being removed, so an entry whose own
/// parent was ALSO pruned in this same pass still lands its surviving
/// children on the nearest ancestor that makes it through the whole pass,
/// however `doomed` happens to be ordered.
fn prune_unlocked(rice: &str, draft: Option<&str>, doomed: &[u32]) -> Result<PruneResult, String> {
    let before = history::list_history(rice, draft);
    let head = history::load_head(rice, draft);
    let protected_now: BTreeSet<u32> = head.map(|h| history::ancestry(&before, h).into_iter().collect()).unwrap_or_default();

    let mut skipped_now_protected = Vec::new();
    let doomed: Vec<u32> = doomed
        .iter()
        .copied()
        .filter(|n| {
            if protected_now.contains(n) {
                skipped_now_protected.push(*n);
                false
            } else {
                true
            }
        })
        .collect();

    let mut after = before.clone();
    for &n in &doomed {
        after = history::reparent(&after, n);
    }

    // Persist every splice: a surviving entry whose `parent` differs between
    // `before` and `after` must be rewritten so `ancestry` resolves
    // correctly the next time anything reads it off disk, not just in this
    // call's own memory.
    let mut reparented = Vec::new();
    let mut changed = Vec::new();
    for rec in &after {
        let old_parent = before.iter().find(|t| t.entry == rec.entry).and_then(|t| t.parent);
        if old_parent != rec.parent {
            history::save_history(rice, draft, rec).map_err(|e| format!("failed to re-parent entry {}: {e}", rec.entry))?;
            changed.push(history::history_path(rice, draft, rec.entry).to_string_lossy().into_owned());
            reparented.push((rec.entry, old_parent, rec.parent));
        }
    }

    for &n in &doomed {
        let path = history::history_path(rice, draft, n);
        if path.is_file() {
            std::fs::remove_file(&path).map_err(|e| format!("failed to remove entry {n}: {e}"))?;
            changed.push(path.to_string_lossy().into_owned());
        }
    }

    let doomed_set: BTreeSet<u32> = doomed.iter().copied().collect();
    let mut marks = history::load_marks(rice, draft);
    let mut dropped_marks = Vec::new();
    marks.retain(|letter, entry| {
        if doomed_set.contains(entry) {
            dropped_marks.push(letter.clone());
            false
        } else {
            true
        }
    });
    if !dropped_marks.is_empty() {
        history::save_marks(rice, draft, &marks).map_err(|e| format!("failed to write marks.json: {e}"))?;
        changed.push(history::marks_path(rice, draft).to_string_lossy().into_owned());
    }

    Ok(PruneResult { pruned: doomed, reparented, dropped_marks, changed, skipped_now_protected })
}

/// `rice history prune`'s locked entrypoint: exactly ONE `with_stage_lock`
/// around [`prune_unlocked`]'s whole splice-persist-delete-remark body — the
/// crate's one-hold-per-mutator rule (module doc's "Locking discipline"
/// section), the same shape every other mutator in this file uses.
fn prune(rice: &str, draft: Option<&str>, doomed: &[u32]) -> Result<PruneResult, String> {
    crate::lock::with_stage_lock(|| prune_unlocked(rice, draft, doomed))
}

/// One picker row for a prune candidate — same fields as
/// [`entry_lane_rows`]'s tree rows, minus the tree glyphs (a flat candidate
/// list has no branches to draw): number, timestamp, cause, and any mark
/// letters (reusing [`mark_letters_for`]) so a marked-and-`--force`d
/// candidate is still labeled as one right before the User prunes it.
fn prune_row(rec: &HistoryRecord, marks: &BTreeMap<String, u32>) -> String {
    let letters = mark_letters_for(marks, rec.entry);
    let mark_part = if letters.is_empty() { String::new() } else { format!(" [{}]", letters.join(",")) };
    format!("{:04}  {}  {}{mark_part}", rec.entry, rec.at, rec.cause)
}

/// The dry-run report: `Status::Ok`, `changed: []`, nothing written — the
/// shape both the non-tty bare path and an empty-selection tty path (nothing
/// left to pick) return. `Ok`, not `Usage`: nothing was asked for that this
/// refuses, this is the informational default the plan's own "prints the
/// dry run and changes nothing" describes.
fn dry_run_outcome(rice: &str, draft: Option<&str>, plan: &PrunePlan) -> Outcome {
    let scope = scope_label(rice, draft);
    let numbers: Vec<u32> = plan.candidates.iter().map(|t| t.entry).collect();
    let message = if numbers.is_empty() {
        format!(
            "nothing prunable for {scope} right now \
             (pass --older-than/--keep/--all-but-marks, or run on a tty for the picker)"
        )
    } else {
        format!(
            "would prune {} entry(s) for {scope}: {} \
             (dry run — pass a selector flag to act, --force to also include marked entries, \
             or run on a tty to pick)",
            numbers.len(),
            numbers.iter().map(|n| format!("{n:04}")).collect::<Vec<_>>().join(", ")
        )
    };
    Outcome::ok("rice.history.prune", message).with_data(json!({
        "dryRun": true,
        "candidates": numbers,
        "protectedByMark": plan.protected_by_mark.iter().map(|t| t.entry).collect::<Vec<_>>(),
        "protectedAncestry": plan.protected_ancestry.iter().copied().collect::<Vec<_>>(),
    }))
}

/// Render a completed [`PruneResult`] into `rice history prune`'s success
/// [`Outcome`] — shared by the flag-driven path and the tty picker's success
/// arm, mirroring how [`render_back_outcome`] is shared by `rice back`'s two
/// entrances.
fn render_prune_outcome(rice: &str, draft: Option<&str>, result: PruneResult, plan: &PrunePlan) -> Outcome {
    let scope = scope_label(rice, draft);
    let reparent_note = if result.reparented.is_empty() {
        String::new()
    } else {
        format!(
            "; re-parented {}",
            result.reparented.iter().map(|(t, _, _)| format!("{t:04}")).collect::<Vec<_>>().join(", ")
        )
    };
    // A candidate the plan picked can still lose the race against a head
    // move that happened between planning and this call actually running
    // (see `prune_unlocked`'s own doc) — surfaced in the message, not just
    // buried in `data`, since it means fewer entries went than the caller
    // asked for.
    let skipped_note = if result.skipped_now_protected.is_empty() {
        String::new()
    } else {
        format!(
            "; {} skipped (became the head or its ancestor before the prune ran): {}",
            result.skipped_now_protected.len(),
            result.skipped_now_protected.iter().map(|n| format!("{n:04}")).collect::<Vec<_>>().join(", ")
        )
    };
    let message = format!(
        "pruned {} entry(s) for {scope}: {}{reparent_note}{skipped_note}",
        result.pruned.len(),
        result.pruned.iter().map(|n| format!("{n:04}")).collect::<Vec<_>>().join(", ")
    );
    Outcome::ok("rice.history.prune", message).changed(result.changed).with_data(json!({
        "pruned": result.pruned,
        "reparented": result.reparented.iter().map(|(t, old, new)| json!({ "entry": t, "from": old, "to": new })).collect::<Vec<_>>(),
        "droppedMarks": result.dropped_marks,
        "protectedByMark": plan.protected_by_mark.iter().map(|t| t.entry).collect::<Vec<_>>(),
        "skippedNowProtected": result.skipped_now_protected,
    }))
}

/// Act on a [`PrunePlan`] whose candidates were selected by a flag
/// (`--older-than`/`--keep`/`--all-but-marks`) — executes immediately, no
/// prompt, no stdin, exactly like every other flag-driven command in this file.
/// An empty candidate set is still `Ok` ("nothing to prune"), never an
/// error — the same "idempotent no-op is success" shape
/// [`snapshot_if_drifted_unlocked`]'s no-drift case uses.
fn execute_prune(rice: &str, draft: Option<&str>, plan: &PrunePlan) -> Outcome {
    let doomed: Vec<u32> = plan.candidates.iter().map(|t| t.entry).collect();
    if doomed.is_empty() {
        let scope = scope_label(rice, draft);
        return Outcome::ok("rice.history.prune", format!("nothing to prune for {scope}")).with_data(json!({
            "pruned": Vec::<u32>::new(),
            "protectedByMark": plan.protected_by_mark.iter().map(|t| t.entry).collect::<Vec<_>>(),
        }));
    }
    match prune(rice, draft, &doomed) {
        Ok(result) => render_prune_outcome(rice, draft, result, plan),
        Err(e) => Outcome::error("rice.history.prune", format!("prune failed: {e}")).with_data(json!({ "reason": "write-failed" })),
    }
}

/// Bare `rice history prune` on a real CLI tty — reached only from
/// [`handle_rice_entry_prune`], only once [`pick::interactive`] has already
/// said yes. Rows come from `plan.candidates` (the mark filter and the
/// ancestry rail already applied — a marked entry is never even offered
/// unless `--force` was also passed, and the head/its ancestry are never
/// rows at all). An empty candidate pool skips the picker outright and
/// returns the same dry-run report the non-tty path would. Choosing rows
/// IS the confirm (see the module banner) — there is no separate y/n
/// prompt.
fn prune_picker(rice: &str, draft: Option<&str>, plan: &PrunePlan) -> Outcome {
    if plan.candidates.is_empty() {
        return dry_run_outcome(rice, draft, plan);
    }
    let marks = history::load_marks(rice, draft);
    let rows: Vec<String> = plan.candidates.iter().map(|t| prune_row(t, &marks)).collect();
    let prompt = format!("prune which entry(s) for {}?", scope_label(rice, draft));
    match pick::choose_many(&prompt, &rows, &[]) {
        Some(indices) => {
            let doomed: Vec<u32> = indices.iter().filter_map(|&i| plan.candidates.get(i)).map(|t| t.entry).collect();
            match prune(rice, draft, &doomed) {
                Ok(result) => render_prune_outcome(rice, draft, result, plan),
                Err(e) => Outcome::error("rice.history.prune", format!("prune failed: {e}"))
                    .with_data(json!({ "reason": "write-failed" })),
            }
        }
        None => {
            Outcome::usage("rice.history.prune", "no entries selected — aborted").with_data(json!({ "reason": "no-selection" }))
        }
    }
}

/// `rice history prune [--older-than <Nd|Nh>] [--keep <N>] [--all-but-marks]
/// [--force] [--json]` — the pressure valve (phase A9, §7.1). Flags are
/// syntax-validated first (a malformed `--older-than`/`--keep` is a usage
/// error before the draft is even resolved, mirroring every other command's
/// flag-first-then-mode-check order in this file), then [`resolve_draft`]
/// gates on Draft mode, then [`plan_prune`] computes the candidate set once.
/// A selector flag present (`older-than`/`keep`/`all-but-marks`) acts at
/// once via [`execute_prune`]; with none given, [`pick::interactive`] routes
/// a real CLI tty to [`prune_picker`] and everything else (an agent door, or
/// a `Cli` invocation off a tty) to [`dry_run_outcome`] — see the module
/// banner for why the split lands exactly here.
fn handle_rice_entry_prune(inv: &Invocation) -> Outcome {
    let older_than = match inv.flags.get("older-than") {
        Some(raw) => match parse_older_than(raw) {
            Some(secs) => Some(secs),
            None => {
                return Outcome::usage(
                    "rice.history.prune",
                    format!("`--older-than {raw}` is not valid — expected `<N>d` or `<N>h`, e.g. `7d` or `12h`"),
                )
                .with_data(json!({ "reason": "invalid-older-than", "olderThan": raw }));
            }
        },
        None => None,
    };
    let keep = match inv.flags.get("keep") {
        Some(raw) => match raw.parse::<usize>() {
            Ok(n) => Some(n),
            Err(_) => {
                return Outcome::usage("rice.history.prune", format!("`--keep {raw}` is not a valid count"))
                    .with_data(json!({ "reason": "invalid-keep", "keep": raw }));
            }
        },
        None => None,
    };
    let all_but_marks = inv.flag_present("all-but-marks");
    let force = inv.flag_present("force");
    let selector_given = older_than.is_some() || keep.is_some() || all_but_marks;

    let (rice, draft) = match resolve_scope("rice.history.prune") {
        Ok(v) => v,
        Err(o) => return o,
    };

    let plan = plan_prune(&rice, draft.as_deref(), older_than, keep, force);

    if selector_given {
        return execute_prune(&rice, draft.as_deref(), &plan);
    }
    if pick::interactive(inv.door) {
        return prune_picker(&rice, draft.as_deref(), &plan);
    }
    dry_run_outcome(&rice, draft.as_deref(), &plan)
}

// ── `rice back` — the revert that lands the branch-from-any-mark ask ───────
//
// Everything below is ONE mutator: [`back_unlocked`] does drift-snapshot →
// theme/wallpaper write-back → compositor apply → registry re-sync → head
// advance as a single `with_stage_lock` body (which nests for the thread that
// already holds it — `aoide-storage/src/fs.rs`), which is why it
// calls ONLY the `_unlocked` cores above, never [`snapshot`]/[`mark`]: those
// own a hold of their own, and this function's whole point is that the unit is
// ONE body, not a wrapper around four separately-serialised ones.

/// Everything [`handle_rice_back`] needs to build its `Outcome`, assembled
/// INSIDE the lock so nothing here re-reads state the write already changed
/// underneath it.
struct BackResult {
    /// The head cursor's value BEFORE this call touched anything — captured
    /// before the drift snapshot runs, so it names where the caller was
    /// actually standing, not an intermediate value the drift mint produced.
    from: Option<u32>,
    to: u32,
    /// The mark letter used to select the target, if `--mark` was given —
    /// `None` for a `--entry` selection, even if that entry happens to carry a
    /// letter (the report says how the caller ASKED, not what the entry owns).
    mark: Option<String>,
    /// The entry the pre-overwrite drift snapshot minted, if content on the
    /// stage differed from the head entry it was about to clobber.
    drifted: Option<u32>,
    changed: Vec<String>,
    registry_note: String,
    hyprctl_status: &'static str,
    /// The terminal colour push's envelope data
    /// (`super::rice::stage_terminal_colors`).
    terminal: serde_json::Value,
    /// The lock-screen dress write's envelope data
    /// (`super::rice::stage_lockscreen`).
    lock_screen: serde_json::Value,
}

/// The unlocked revert core — see the section banner above for the shape.
/// `cmd` threads through to every `Outcome` built here (module doc's
/// command-name-threading note); `entry_flag`/`mark_flag` are already
/// syntax-validated by [`handle_rice_back`] (a parse/letter-shape failure is
/// a usage error surfaced before this ever runs, and before the lock is even
/// entered) — this function's job is resolving WHICH one names a real entry and
/// then acting on it.
fn back_unlocked(cmd: &str, entry_flag: Option<u32>, mark_flag: Option<String>) -> Result<BackResult, Outcome> {
    let (rice, draft) = resolve_scope(cmd)?;

    // D7: refuse before touching anything if the DRAFT's routing symlink is
    // gone, dangling, or was ever replaced by a plain file. `atomic_write`'s
    // symlink transparency (`aoide-storage/src/fs.rs`) is the ENTIRE
    // mechanism a Draft-mode revert rides on — the write-back below carries
    // zero symlink-awareness of its own, exactly like `rice stage`'s.
    // Without this check, a broken routing symlink would make the write
    // below land in a plain `stage/theme.json` instead of the draft file:
    // the draft itself untouched, the entry store and the live stage
    // silently disagreeing about which draft is "current". `symlink_metadata`
    // never follows the link, so this answers "is `stage/theme.json` ITSELF
    // a symlink" without caring whether its target exists.
    //
    // Staging-mode revert (`draft.is_none()`, the staging-mode reach `strillix
    // reload`'s design extends `rice back` to) has no symlink to check at
    // all — `stage/theme.json` is a plain file there, exactly like `rice
    // stage`'s own write, so this whole rail is Draft-only.
    let stage = crate::paths::stage_dir();
    let theme_path = stage.join("theme.json");
    if let Some(draft_name) = draft.as_deref() {
        let routed = std::fs::symlink_metadata(&theme_path)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false);
        if !routed {
            return Err(Outcome::error(
                cmd,
                format!(
                    "draft routing is broken: {} is not a symlink — refusing to revert rather than \
                     silently writing a plain stage file that leaves the draft untouched \
                     (`strillix rice mode draft {draft_name}` re-routes it)",
                    theme_path.display()
                ),
            )
            .with_data(json!({ "reason": "routing-broken", "expected": theme_path.to_string_lossy() })));
        }
    }

    // Resolve the target: `--entry N` names it directly; `--mark X` resolves
    // through `history/marks.json`. A map entry naming an entry that no longer
    // exists (a hand-edited marks file, or — once pruning lands — an entry
    // pruned without also rewriting the map) reads exactly like an
    // unstamped letter from here: both are "that mark doesn't name anything
    // real", so they share `mark-not-found` rather than inventing a second
    // reason for the same practical failure (mirrors `mark_unlocked`'s own
    // entry-not-found/no-head collapse just above).
    let (target, mark_used) = if let Some(n) = entry_flag {
        if history::load_history(&rice, draft.as_deref(), n).is_none() {
            return Err(Outcome::error(cmd, format!("entry {n:04} does not exist — nothing to revert to"))
                .with_data(json!({ "reason": "entry-not-found", "entry": n })));
        }
        (n, None)
    } else {
        let letter = mark_flag.expect("handle_rice_back guarantees entry_flag or mark_flag is Some");
        let marks = history::load_marks(&rice, draft.as_deref());
        match marks.get(&letter).copied() {
            Some(n) if history::load_history(&rice, draft.as_deref(), n).is_some() => (n, Some(letter)),
            _ => {
                return Err(
                    Outcome::error(cmd, format!("mark `{letter}` is not stamped on any entry"))
                        .with_data(json!({ "reason": "mark-not-found", "mark": letter })),
                );
            }
        }
    };
    let target_record =
        history::load_history(&rice, draft.as_deref(), target).expect("existence just confirmed above");

    // §7.5's gate, at the HEAD of the back path — above the drift snapshot
    // below, which is itself a write (it mints an entry). A refusal here leaves
    // the desktop exactly where it was: no drift entry, no reverted theme, no
    // geometry re-applied to the compositor, no wallpaper rewrite, no cursor move.
    // The revert itself needs no nix; only the registry sync does — and
    // refusing the whole operation BEFORE its first write is what makes that
    // sync's refusal safe to inherit.
    let rices = crate::widgets::plan_stage(&rice).map_err(|e| {
        Outcome::error(cmd, e.error)
            .with_data(json!({ "reason": "stage-refused", "target": e.target }))
    })?;

    let head_before = history::load_head(&rice, draft.as_deref());

    // The rail that makes "nothing is destroyed" literally true even for
    // unrecorded hand-edits: preserve whatever is CURRENTLY on the stage
    // before the write below overwrites it, if it differs from the head
    // entry it's about to clobber. Folded into THIS lock, not a second
    // acquisition — see the module doc; this is `snapshot_if_drifted_unlocked`'s
    // one sanctioned caller.
    let drifted = snapshot_if_drifted_unlocked(cmd, "drift")?;

    let mut changed = Vec::new();
    if let Some(rec) = &drifted {
        changed.push(history::history_path(&rice, draft.as_deref(), rec.entry).to_string_lossy().into_owned());
        changed.push(history::head_path(&rice, draft.as_deref()).to_string_lossy().into_owned());
    }

    // The write-back IS the routing (plan §5.2): the identical `atomic_write`
    // seam `rice stage` writes (`commands/rice.rs::handle_rice_stage`),
    // symlink-transparent, so this lands in the draft file the same way
    // every other stage writer does. No new apply path — this reuses the
    // one that already exists.
    let theme_body = serde_json::to_string_pretty(&target_record.theme)
        .map_err(|e| {
            Outcome::error(cmd, format!("failed to serialize entry {target:04}'s theme: {e}"))
                .with_data(json!({ "reason": "write-failed" }))
        })?
        + "\n";
    shellbridge::atomic_write(&theme_path, &theme_body).map_err(|e| {
        Outcome::error(cmd, format!("failed to write {}: {e}", theme_path.display()))
            .with_data(json!({ "reason": "write-failed", "target": theme_path.to_string_lossy() }))
    })?;
    changed.push(theme_path.to_string_lossy().into_owned());

    // Best-effort compositor live-apply — the exact sequencing
    // `handle_rice_stage` uses (geometry/border keywords derived from the
    // SAME theme just written), never fatal: the stage-file write above is
    // already the source of truth for the hot-reload half.
    let hyprctl_status = crate::live::apply_live(&crate::live::geometry_keywords(&target_record.theme));
    // The terminal half off the same restored theme: the colour file every
    // new kitty window includes, pushed to the open ones — same best-effort
    // tier, through the one writer `rice stage` uses.
    let (terminal_changed, terminal) = super::rice::stage_terminal_colors(
        crate::live::terminal_colors(&target_record.theme).as_deref(),
    );
    changed.extend(terminal_changed);
    // And the lock screen's dress, off the same theme: the rice's own picture
    // from the runtime rices, or nothing, which clears the dress a later
    // staged rice left behind.
    let lock_image = crate::wallpaper::lockscreen_image(&crate::paths::rice_dir(&rice));
    let (lock_changed, lock_screen) = super::rice::stage_lockscreen(
        crate::live::lockscreen_fragment(&target_record.theme, lock_image.as_deref()),
        lock_image.as_deref(),
    );
    changed.extend(lock_changed);

    // Wallpaper restore is narrowed to the STAGE ONLY (advisor verdict D4):
    // `stage/wallpaper.json` is not symlink-routed the way `theme.json` is
    // (`storage/src/mode.rs`'s `handle_mode_draft` symlinks exactly one
    // file), so this writes the same seam `wallpaper set` writes
    // (`commands/wallpaper.rs::handle_wallpaper_set`), never the draft directory's
    // own `wallpaper.json` — that file is a `draft save`-time archive copy no
    // write path maintains and no read path consumes (draft commands are
    // save/list/drop only). An entry minted with NO wallpaper must not leave the
    // previous wallpaper lying on the stage — mirror `draft.rs`'s own
    // fork-time stale-wallpaper removal (`rice.draft.save`'s `wallpaper_src.is_file()`
    // else-branch) rather than leaving a wallpaper the target entry never had.
    let wallpaper_path = stage.join("wallpaper.json");
    match &target_record.wallpaper {
        Some(wallpaper_value) => {
            let wallpaper_body = serde_json::to_string_pretty(wallpaper_value).unwrap_or_default() + "\n";
            shellbridge::atomic_write(&wallpaper_path, &wallpaper_body).map_err(|e| {
                Outcome::error(cmd, format!("failed to write {}: {e}", wallpaper_path.display()))
                    .with_data(json!({ "reason": "write-failed", "target": wallpaper_path.to_string_lossy() }))
            })?;
            changed.push(wallpaper_path.to_string_lossy().into_owned());
        }
        None => {
            if wallpaper_path.exists() {
                let _ = std::fs::remove_file(&wallpaper_path);
                changed.push(wallpaper_path.to_string_lossy().into_owned());
            }
        }
    }

    // Registry re-sync, no IPC reload (advisor verdict D1). `registry.json`
    // is derived from the theme's `.widgets` key, so a reverted theme with
    // a different widget set would leave it stale without this — but every
    // file this whole function writes (theme, wallpaper, registry) is
    // FileView-watched by Quickshell exactly like `rice stage`'s own palette
    // tier, so there is nothing for a Quickshell IPC reload to do. The ONE
    // reload lane that exists anywhere in this codebase
    // (`ipc::quickshell_ipc_reload`, gated in `handle_rice_stage`) exists
    // ONLY for dynamically `Qt.createComponent`-loaded widget BODIES, which
    // a revert never touches by design (widget bodies are git's substrate,
    // §5.2) — so that call is never reached from here, deliberately, not by
    // omission. The gate that approves this rices ran at the HEAD of this
    // function, above the drift snapshot — that is what makes a refusal here
    // cost the operator nothing.
    let registry_sync = crate::widgets::sync_rice_registry(&rice, &rices).map_err(|e| {
        Outcome::error(cmd, format!("failed to sync widget-type registry: {}", e.error))
            .with_data(json!({ "reason": "registry-sync-failed", "target": e.target }))
    })?;
    changed.extend(registry_sync.changed.clone());

    // A revert is NOT an entry (advisor verdict, fork 8) — only the cursor
    // moves. This is the step where branching actually happens: the NEXT
    // snapshot parents off whatever `save_head` names here, not off
    // whatever the head happened to be a moment ago.
    history::save_head(&rice, draft.as_deref(), target).map_err(|e| {
        Outcome::error(cmd, format!("failed to advance the head cursor: {e}"))
            .with_data(json!({ "reason": "write-failed" }))
    })?;
    changed.push(history::head_path(&rice, draft.as_deref()).to_string_lossy().into_owned());

    Ok(BackResult {
        from: head_before,
        to: target,
        mark: mark_used,
        drifted: drifted.map(|r| r.entry),
        changed,
        registry_note: registry_sync.note,
        hyprctl_status,
        terminal,
        lock_screen,
    })
}

/// `rice back [--entry N | --mark <letter>]` — the command the branch-from-any-
/// mark ask lands on. Syntax-validates its flags BEFORE taking the lock (a
/// malformed `--entry`/`--mark` is a usage error regardless of draft state,
/// so there is no reason to acquire anything to report it), then wraps
/// [`back_unlocked`]'s whole read-drift-write-advance body in exactly ONE
/// `with_stage_lock` — see that function's own doc and the module doc's
/// locking-discipline section.
///
/// The bare (neither flag) form is phase A8's dual entrance (§7, advisor
/// verdict fork 6): [`pick::interactive`] decides whether this invocation is
/// a real CLI tty, and only then does [`handle_rice_back_picker`] get a
/// chance to open — every other bare invocation (an agent door, or a CLI
/// invocation piped/redirected/run under a test harness) hits the same
/// usage refusal this function has always returned — same `Status::Usage`,
/// same `reason: "no-selection"`, still no stdin read and still nothing
/// written. Only its wording moved when the picker landed, since the old
/// wording called the picker unbuilt.
pub(crate) fn handle_rice_back(inv: &Invocation) -> Outcome {
    let entry_flag = match inv.flags.get("entry") {
        Some(raw) => match raw.parse::<u32>() {
            Ok(n) => Some(n),
            Err(_) => {
                return Outcome::usage("rice.back", format!("`--entry {raw}` is not a valid entry number"))
                    .with_data(json!({ "reason": "invalid-entry", "entry": raw }));
            }
        },
        None => None,
    };
    let mark_flag = match inv.flags.get("mark") {
        Some(letter) => {
            if !valid_mark_letter(letter) {
                return Outcome::error(
                    "rice.back",
                    format!("`{letter}` is not a valid mark: must be a single letter A-Z"),
                )
                .with_data(json!({ "reason": "invalid-mark", "mark": letter }));
            }
            Some(letter.clone())
        }
        None => None,
    };

    // Dual entrance (§7, phase A8, advisor verdict fork 6): a bare `rice
    // back` on a real CLI tty opens the picker, head's-parent pre-selected
    // as row 1 so §5.2's one-step undo is a single Enter. Every other bare
    // invocation — any non-`Cli` door, or a `Cli` invocation off a tty
    // (piped, redirected, this very test harness) — never reaches the
    // picker at all and gets a usage refusal naming the two flags that work
    // without a terminal. The refusal's SHAPE is unchanged from before A8
    // (`Status::Usage`, `reason: "no-selection"`, no stdin read, nothing
    // written); its TEXT is not, because the old text promised the picker
    // was "a later step, not built yet" and this step is that later step.
    if entry_flag.is_none() && mark_flag.is_none() {
        if pick::interactive(inv.door) {
            return handle_rice_back_picker();
        }
        return Outcome::usage(
            "rice.back",
            "usage: strillix rice back --entry N | --mark <letter> [--json] \
             (a bare `rice back` opens a numbered picker on a tty; \
             this door is not one, so pass --entry or --mark)",
        )
        .with_data(json!({ "reason": "no-selection" }));
    }

    match crate::lock::with_stage_lock(|| back_unlocked("rice.back", entry_flag, mark_flag)) {
        Ok(result) => render_back_outcome(result),
        Err(o) => o,
    }
}

/// Render a completed [`BackResult`] into `rice back`'s success [`Outcome`]
/// — shared by both entrances (the `--entry`/`--mark` flags, and phase A8's
/// tty picker below) so "take the existing A5 path" (the plan's own words
/// for this step) means calling this ONE function from the picker's success
/// arm too, never a second copy of the same message-building logic.
fn render_back_outcome(result: BackResult) -> Outcome {
    let drift_note = result
        .drifted
        .map(|d| format!("; entry {d:04} preserved the unrecorded edit that was about to be overwritten"))
        .unwrap_or_default();
    let mark_note = result.mark.as_ref().map(|m| format!(" (mark {m})")).unwrap_or_default();
    let message = format!(
        "reverted to entry {:04}{mark_note} — head now {:04}{drift_note}",
        result.to, result.to
    );
    Outcome::ok("rice.back", message)
        .changed(result.changed)
        .with_data(json!({
            "from": result.from,
            "to": result.to,
            "mark": result.mark,
            "drifted": result.drifted,
            "wallpaper": crate::wallpaper_provider::sync(),
            "hyprctl": result.hyprctl_status,
            "terminal": result.terminal,
            "lockScreen": result.lock_screen,
            "registry": result.registry_note,
        }))
}

// ── `rice back`'s tty picker (phase A8, §7 dual entrance) ──────────────────

/// The row source for [`handle_rice_back_picker`]: reuse [`render_tree`]
/// (A6) rather than re-walking the store, splitting its multi-line output
/// back into `(entry, row)` pairs so the picker can report which entry number
/// a chosen ROW actually names. Safe to parse this way because every line
/// [`render_node`] emits starts its own entry number as the first run of
/// ASCII digits after the tree-glyph prefix (`"{:04}  {at}  {cause}..."`),
/// and none of the glyph characters the prefix can contain (` `, `│`, `├─`,
/// `└─`) are digits — there is nothing else on the line a digit scan could
/// mistake for the number. Order follows [`render_tree`]'s own depth-first
/// walk, so the picker's numbered rows read top-to-bottom exactly like
/// `rice history list`'s tree does.
fn entry_lane_rows(recs: &[HistoryRecord], head: Option<u32>, marks: &BTreeMap<String, u32>) -> Vec<(u32, String)> {
    render_tree(recs, head, marks)
        .lines()
        .filter_map(|line| {
            let start = line.find(|c: char| c.is_ascii_digit())?;
            let digits: String = line[start..].chars().take_while(|c| c.is_ascii_digit()).collect();
            let entry = digits.parse().ok()?;
            Some((entry, line.to_string()))
        })
        .collect()
}

/// Fork 6's own rule, pulled out of [`handle_rice_back_picker`] so it is
/// testable without a tty: the picker's default row is the HEAD's PARENT —
/// the one-step-undo row, so a bare Enter (fork 6) reverts exactly one step
/// back — but only when that parent is actually a row in `lane`. Two cases
/// fail safe to `None` rather than defaulting to something arbitrary, and
/// [`pick::choose`] treats `None` as "no default; a bare Enter aborts", not
/// as "pick row 0":
///   - the head is a root entry (`parent == None`) — there is nothing to
///     default to.
///   - the head's `parent` names an entry absent from `entries` entirely (a
///     dangling/hand-edited pointer — the same "detached" shape A6's
///     [`render_tree`] already renders rather than hides): `lane` has no
///     row for a number that was never really there, so
///     `lane.iter().position(...)` correctly finds nothing.
fn default_row_index(recs: &[HistoryRecord], head: Option<u32>, lane: &[(u32, String)]) -> Option<usize> {
    let head_parent = head.and_then(|h| recs.iter().find(|t| t.entry == h).and_then(|t| t.parent));
    head_parent.and_then(|p| lane.iter().position(|(n, _)| *n == p))
}

/// Bare `rice back` on a real CLI tty — reached only from
/// [`handle_rice_back`], only once [`pick::interactive`] has already said
/// yes. Builds the entries lane from [`entry_lane_rows`], defaults the picker
/// via [`default_row_index`] (fork 6: the one-step-undo row, so an empty
/// Enter reverts exactly one step back), runs [`pick::choose`], and — on a
/// selection — takes the existing `--entry` path via [`render_back_outcome`].
/// An abort (`q`, EOF, or two bad attempts) reports the same `no-selection`
/// reason [`handle_rice_back`]'s own non-tty refusal uses, since the
/// practical outcome is identical either way: nothing was picked, nothing
/// was written.
fn handle_rice_back_picker() -> Outcome {
    let (rice, draft) = match resolve_scope("rice.back") {
        Ok(v) => v,
        Err(o) => return o,
    };

    let recs = history::list_history(&rice, draft.as_deref());
    let head = history::load_head(&rice, draft.as_deref());
    let marks = history::load_marks(&rice, draft.as_deref());
    let lane = entry_lane_rows(&recs, head, &marks);

    let default_idx = default_row_index(&recs, head, &lane);
    let rows: Vec<String> = lane.iter().map(|(_, row)| row.clone()).collect();

    let prompt = format!("revert {} to which entry?", scope_label(&rice, draft.as_deref()));
    match pick::choose(&prompt, &rows, default_idx) {
        Some(idx) => {
            let target = lane[idx].0;
            match crate::lock::with_stage_lock(|| back_unlocked("rice.back", Some(target), None)) {
                Ok(result) => render_back_outcome(result),
                Err(o) => o,
            }
        }
        None => Outcome::usage("rice.back", "no entry selected — aborted")
            .with_data(json!({ "reason": "no-selection" })),
    }
}

// ── Tests (the snapshot cores + `rice history`) ────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use aoide_protocol::output::Status;
    use crate::mode::{save_mode_marker, ModeMarker};
    use aoide_test_support::*;

    /// A bare in-memory `HistoryRecord` for [`render_tree`]'s own tests — those exercise a pure
    /// function directly, off hand-built records, with no filesystem/env rig at all (unlike
    /// every other test in this file).
    fn rec(entry: u32, parent: Option<u32>) -> HistoryRecord {
        HistoryRecord {
            schema_version: String::new(),
            entry,
            parent,
            at: "2026-08-18T00:00:00Z".to_string(),
            session_id: None,
            cause: "stage".to_string(),
            theme: serde_json::json!({}),
            wallpaper: None,
            widgets: serde_json::json!({}),
        }
    }

    /// Route `AOIDE_STAGE_DIR` at a fresh tmp stage and mark `mode.json` as
    /// `Draft` for `sonata`/`neon-night` — every snapshot-core test needs
    /// this before it can pass [`resolve_draft`]. Callers still write
    /// `stage/theme.json` themselves (content varies per test) and own
    /// `remove_dir_all(&root)` at the end, matching every other command
    /// module's test rig in this crate.
    fn routed_draft(tag: &str) -> (std::path::PathBuf, String, String) {
        let root = unique_tmp(tag);
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);
        let rice = "sonata".to_string();
        let draft = "neon-night".to_string();
        save_mode_marker(&ModeMarker {
            mode: RiceMode::Draft,
            rice: Some(rice.clone()),
            draft: Some(draft.clone()),
            ..Default::default()
        })
        .unwrap();
        (root, rice, draft)
    }

    // ── snapshot: numbering + parent chain ──────────────────────────────

    #[test]
    fn first_snapshot_in_a_routed_draft_is_0001_with_no_parent() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, rice, draft) = routed_draft("entry-first");
        std::fs::write(crate::paths::stage_dir().join("theme.json"), VALID_NOTES).unwrap();

        let record = snapshot("rice.history", "explicit").unwrap();
        assert_eq!(record.entry, 1);
        assert_eq!(record.parent, None, "the very first entry has no parent");
        assert_eq!(record.cause, "explicit");
        assert_eq!(history::load_head(&rice, Some(&draft)), Some(1));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn second_snapshot_parents_off_the_first() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, rice, draft) = routed_draft("entry-second");
        std::fs::write(crate::paths::stage_dir().join("theme.json"), VALID_NOTES).unwrap();

        let first = snapshot("rice.history", "stage").unwrap();
        assert_eq!(first.entry, 1);

        std::fs::write(
            crate::paths::stage_dir().join("theme.json"),
            r##"{"schemaVersion":"0","palette":{"bg":"#111111"}}"##,
        )
        .unwrap();
        let second = snapshot("rice.history", "stage").unwrap();
        assert_eq!(second.entry, 2);
        assert_eq!(second.parent, Some(1), "hangs off the head at the time of the mint");
        assert_eq!(history::load_head(&rice, Some(&draft)), Some(2));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn snapshot_outside_staging_or_draft_mode_refuses() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp("entry-not-staged-or-drafted");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        // No marker at all IS declarative (the safe default) — refused:
        // nothing is unlocked, so there is nothing to snapshot.
        let err = snapshot("rice.history", "explicit").unwrap_err();
        assert_eq!(err.status, Status::Error);
        assert_eq!(err.data.clone().unwrap()["reason"], "not-staged-or-drafted");

        // Explicit `Declarative` refuses the same way as no marker at all.
        save_mode_marker(&ModeMarker { mode: RiceMode::Declarative, ..Default::default() }).unwrap();
        let err = snapshot("rice.history", "explicit").unwrap_err();
        assert_eq!(err.data.unwrap()["reason"], "not-staged-or-drafted");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The staging-mode entry/back reach itself (`strillix reload` design,
    /// settled 2026-08-31): `Staging` mode is NOT refused — `rice history`
    /// mints straight onto `rices/<rice>/history/`, sibling to `drafts/`,
    /// never nested under one that doesn't exist in this mode.
    #[test]
    fn snapshot_in_staging_mode_entries_off_the_rice_directly() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let root = unique_tmp("entry-staging-routing");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        save_mode_marker(&ModeMarker {
            mode: RiceMode::Staging,
            rice: Some("sonata".to_string()),
            ..Default::default()
        })
        .unwrap();
        std::fs::write(stage.join("theme.json"), VALID_NOTES).unwrap();

        let record = snapshot("rice.history", "explicit").unwrap();
        assert_eq!(record.entry, 1);
        assert_eq!(
            crate::history::load_history("sonata", None, 1).map(|r| r.entry),
            Some(1),
            "the entry lives directly under rices/sonata/history/"
        );
        assert!(
            !crate::paths::rice_drafts_dir("sonata").is_dir(),
            "no drafts/ directory was ever created for a staging-mode entry"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    // ── command-name threading: a refusal reports the ACTUAL caller ─────
    //
    // The known issue A3 exists to fix: before this, every refusal built
    // inside these cores hardcoded `"rice.history"`, harmless while `rice
    // entry` was the only caller. `rice stage`/`wallpaper set`'s auto-entry hooks
    // are a second caller — a refusal bubbling out of THEIR snapshot must
    // name `"rice.stage"`/`"wallpaper.set"`, not `"rice.history"`, or an agent
    // reading the error would think the wrong command failed.

    #[test]
    fn snapshot_unlocked_refusal_reports_the_invoking_command_not_rice_entry() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let stage = unique_tmp("entry-cmd-thread-snapshot").join("stage");
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        // No marker at all IS declarative — refused before this even reaches
        // the entry store, so the refusal must name the REAL caller.
        let err = snapshot_unlocked("rice.stage", "stage").unwrap_err();
        assert_eq!(err.command, "rice.stage", "not the hardcoded rice.history");
        assert_eq!(err.data.unwrap()["reason"], "not-staged-or-drafted");
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn snapshot_if_drifted_unlocked_refusal_reports_the_invoking_command_not_rice_entry() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let stage = unique_tmp("entry-cmd-thread-drift").join("stage");
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        let err = snapshot_if_drifted_unlocked("wallpaper.set", "wallpaper-set").unwrap_err();
        assert_eq!(err.command, "wallpaper.set", "not the hardcoded rice.history");
        assert_eq!(err.data.unwrap()["reason"], "not-staged-or-drafted");
        let _ = std::fs::remove_dir_all(&stage);
    }

    // ── snapshot_if_drifted_unlocked: no-op vs. mint ────────────────────
    //
    // Exercised directly against the unlocked core — it has no locked
    // wrapper (its only planned caller, `rice back`, folds the drift check
    // into its own single `with_stage_lock` acquisition alongside the
    // revert write and the head-cursor save, so a standalone locked wrapper
    // here would have no caller — YAGNI).

    #[test]
    fn snapshot_if_drifted_unlocked_is_a_noop_when_unchanged_and_mints_when_changed() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, rice, draft) = routed_draft("entry-drift");
        std::fs::write(crate::paths::stage_dir().join("theme.json"), VALID_NOTES).unwrap();

        let first = snapshot("rice.history", "stage").unwrap();
        assert_eq!(first.entry, 1);

        // Re-serialized but VALUE-identical content must not read as drift.
        let reparsed: Value = serde_json::from_str(VALID_NOTES).unwrap();
        let reserialized = serde_json::to_string_pretty(&reparsed).unwrap();
        std::fs::write(crate::paths::stage_dir().join("theme.json"), reserialized).unwrap();
        let noop = snapshot_if_drifted_unlocked("rice.history", "drift").unwrap();
        assert!(noop.is_none(), "byte-different, value-identical content is not drift");
        assert_eq!(history::list_history(&rice, Some(&draft)).len(), 1, "no new entry minted");

        // A genuine change mints an entry, parented off the head it drifted from.
        std::fs::write(
            crate::paths::stage_dir().join("theme.json"),
            r##"{"schemaVersion":"0","palette":{"bg":"#222222"}}"##,
        )
        .unwrap();
        let drifted = snapshot_if_drifted_unlocked("rice.history", "drift").unwrap();
        let drifted = drifted.expect("a real content change is drift");
        assert_eq!(drifted.entry, 2);
        assert_eq!(drifted.parent, Some(1));
        assert_eq!(drifted.cause, "drift");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn snapshot_if_drifted_unlocked_mints_unconditionally_on_an_empty_store() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, _rice, _draft) = routed_draft("entry-drift-empty");
        std::fs::write(crate::paths::stage_dir().join("theme.json"), VALID_NOTES).unwrap();

        // No head exists yet — there is nothing to compare against, so the
        // very first content is always worth capturing.
        let minted = snapshot_if_drifted_unlocked("rice.history", "drift").unwrap();
        let minted = minted.expect("an empty store has nothing to be identical to");
        assert_eq!(minted.entry, 1);
        assert_eq!(minted.parent, None);
        let _ = std::fs::remove_dir_all(&root);
    }

    // ── sessionId: carried when set, omitted when not ──────────────────

    #[test]
    fn entry_carries_session_id_when_set_and_omits_when_not() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        let (root, _rice, _draft) = routed_draft("entry-session");
        std::fs::write(crate::paths::stage_dir().join("theme.json"), VALID_NOTES).unwrap();

        std::env::remove_var("AOIDE_SESSION_ID");
        let no_session = snapshot("rice.history", "explicit").unwrap();
        assert_eq!(no_session.session_id, None);

        std::env::set_var("AOIDE_SESSION_ID", "sess-123");
        let with_session = snapshot("rice.history", "explicit").unwrap();
        assert_eq!(with_session.session_id, Some("sess-123".to_string()));
        let _ = std::fs::remove_dir_all(&root);
    }

    // ── `rice history` — the registry entrypoint ───────────────────────────

    #[test]
    fn rice_entry_handler_mints_and_reports_the_entry_and_head_files_changed() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, rice, draft) = routed_draft("entry-handler-ok");
        std::fs::write(crate::paths::stage_dir().join("theme.json"), VALID_NOTES).unwrap();

        let out = handle_rice_entry(&inv(&["rice", "history"], &[]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let data = out.data.unwrap();
        assert_eq!(data["entry"], 1);
        assert!(data["parent"].is_null());
        assert_eq!(data["cause"], "explicit");
        assert!(out.changed.iter().any(|c| c.ends_with("history/0001.json")));
        assert!(out.changed.iter().any(|c| c.ends_with("history/head.json")));
        assert_eq!(history::load_head(&rice, Some(&draft)), Some(1));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rice_entry_handler_refuses_outside_draft_mode() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let stage = unique_tmp("entry-handler-not-draft").join("stage");
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        let out = handle_rice_entry(&inv(&["rice", "history"], &[]));
        assert_eq!(out.status, Status::Error);
        assert_eq!(out.data.unwrap()["reason"], "not-staged-or-drafted");
        let _ = std::fs::remove_dir_all(&stage);
    }

    // ── `rice history mark` — the mark command (phase A4) ─────────────────────
    //
    // `inv()` (aoide_test_support) has no flags support, so a `--entry N`
    // invocation is built by hand, same pattern `graph/permit.rs`'s and
    // `rice.rs`'s own tests use for a hand-populated `Invocation`.

    fn inv_with_entry(letter: &str, entry: u32) -> Invocation {
        Invocation {
            path: vec!["rice".to_string(), "history".to_string(), "mark".to_string()],
            args: vec![letter.to_string()],
            flags: std::collections::BTreeMap::from([("entry".to_string(), entry.to_string())]),
            door: aoide_protocol::Door::Cli,
        }
    }

    #[test]
    fn mark_stamps_the_current_head_by_default() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, rice, draft) = routed_draft("mark-head-default");
        std::fs::write(crate::paths::stage_dir().join("theme.json"), VALID_NOTES).unwrap();
        let first = snapshot("rice.history", "explicit").unwrap();
        assert_eq!(first.entry, 1);

        let out = handle_rice_entry_mark(&inv(&["rice", "history", "mark"], &["A"]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let data = out.data.unwrap();
        assert_eq!(data["mark"], "A");
        assert_eq!(data["entry"], 1, "no --entry given, defaults to the head");
        assert_eq!(data["moved"], false, "a fresh letter is a stamp, not a move");
        assert!(data["from"].is_null());
        assert!(out.changed.iter().any(|c| c.ends_with("history/marks.json")));
        assert_eq!(history::load_marks(&rice, Some(&draft)).get("A"), Some(&1));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn mark_entry_flag_targets_a_specific_entry_not_the_head() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, rice, draft) = routed_draft("mark-entry-flag");
        std::fs::write(crate::paths::stage_dir().join("theme.json"), VALID_NOTES).unwrap();
        snapshot("rice.history", "explicit").unwrap(); // entry 1
        std::fs::write(
            crate::paths::stage_dir().join("theme.json"),
            r##"{"schemaVersion":"0","palette":{"bg":"#222222"}}"##,
        )
        .unwrap();
        let second = snapshot("rice.history", "explicit").unwrap(); // entry 2, now head
        assert_eq!(second.entry, 2);
        assert_eq!(history::load_head(&rice, Some(&draft)), Some(2));

        // Explicitly mark entry 1, even though the head has since moved to 2.
        let out = handle_rice_entry_mark(&inv_with_entry("A", 1));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert_eq!(out.data.unwrap()["entry"], 1);
        assert_eq!(history::load_marks(&rice, Some(&draft)).get("A"), Some(&1));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn mark_restamping_a_letter_moves_it_entry_files_stay_untouched_one_map_entry() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, rice, draft) = routed_draft("mark-move");
        std::fs::write(crate::paths::stage_dir().join("theme.json"), VALID_NOTES).unwrap();
        let first = snapshot("rice.history", "explicit").unwrap();
        std::fs::write(
            crate::paths::stage_dir().join("theme.json"),
            r##"{"schemaVersion":"0","palette":{"bg":"#222222"}}"##,
        )
        .unwrap();
        let second = snapshot("rice.history", "explicit").unwrap();
        assert_eq!((first.entry, second.entry), (1, 2));
        let entry_1_raw_before = std::fs::read_to_string(history::history_path(&rice, Some(&draft), 1)).unwrap();
        let entry_2_raw_before = std::fs::read_to_string(history::history_path(&rice, Some(&draft), 2)).unwrap();

        let stamped = handle_rice_entry_mark(&inv_with_entry("A", 1));
        assert_eq!(stamped.status, Status::Ok, "{:?}", stamped.data);

        let moved = handle_rice_entry_mark(&inv_with_entry("A", 2));
        assert_eq!(moved.status, Status::Ok, "{:?}", moved.data);
        let data = moved.data.unwrap();
        assert_eq!(data["moved"], true, "the letter already named entry 1 — this is a move");
        assert_eq!(data["from"], 1);
        assert_eq!(data["entry"], 2);

        // Old entry loses the letter, new entry has it — as one map, not a
        // per-entry field: exactly one entry, naming the new entry.
        let marks = history::load_marks(&rice, Some(&draft));
        assert_eq!(marks.len(), 1, "moving overwrote the entry, it did not duplicate it");
        assert_eq!(marks.get("A"), Some(&2));

        // D6/fork 4: marks live OUTSIDE the entry record — stamping or moving
        // a letter must never rewrite an NNNN.json.
        assert_eq!(
            std::fs::read_to_string(history::history_path(&rice, Some(&draft), 1)).unwrap(),
            entry_1_raw_before,
            "entry 1's own file is untouched by the mark ever moving off it"
        );
        assert_eq!(
            std::fs::read_to_string(history::history_path(&rice, Some(&draft), 2)).unwrap(),
            entry_2_raw_before,
            "entry 2's own file is untouched by the mark landing on it"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn mark_rejects_lowercase_multichar_and_non_letter_marks() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, _rice, _draft) = routed_draft("mark-invalid");
        std::fs::write(crate::paths::stage_dir().join("theme.json"), VALID_NOTES).unwrap();
        snapshot("rice.history", "explicit").unwrap();

        for bad in ["a", "AB", "1", "", "Å"] {
            let out = handle_rice_entry_mark(&inv(&["rice", "history", "mark"], &[bad]));
            assert_eq!(out.status, Status::Error, "`{bad}` should be rejected: {:?}", out.data);
            assert_eq!(out.data.unwrap()["reason"], "invalid-mark", "for input `{bad}`");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn mark_missing_letter_arg_is_a_usage_error() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let (root, _rice, _draft) = routed_draft("mark-missing-arg");

        let out = handle_rice_entry_mark(&inv(&["rice", "history", "mark"], &[]));
        assert_eq!(out.status, Status::Usage);
        assert_eq!(out.data.unwrap()["reason"], "missing-mark");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn mark_refuses_outside_draft_mode() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let stage = unique_tmp("mark-not-draft").join("stage");
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        let out = handle_rice_entry_mark(&inv(&["rice", "history", "mark"], &["A"]));
        assert_eq!(out.status, Status::Error);
        assert_eq!(out.data.unwrap()["reason"], "not-staged-or-drafted");
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn mark_on_a_nonexistent_entry_errors_entry_not_found() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, _rice, _draft) = routed_draft("mark-entry-missing");
        std::fs::write(crate::paths::stage_dir().join("theme.json"), VALID_NOTES).unwrap();
        snapshot("rice.history", "explicit").unwrap(); // only entry 1 exists

        let out = handle_rice_entry_mark(&inv_with_entry("A", 99));
        assert_eq!(out.status, Status::Error);
        assert_eq!(out.data.unwrap()["reason"], "entry-not-found");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn mark_with_no_entries_at_all_and_no_entry_flag_errors_entry_not_found() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let (root, _rice, _draft) = routed_draft("mark-empty-store");

        // Draft mode, but nothing has ever been entered — no head to default
        // to, which reads the same as "that entry doesn't exist".
        let out = handle_rice_entry_mark(&inv(&["rice", "history", "mark"], &["A"]));
        assert_eq!(out.status, Status::Error);
        assert_eq!(out.data.unwrap()["reason"], "entry-not-found");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn mark_invalid_entry_flag_is_a_usage_error() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, _rice, _draft) = routed_draft("mark-entry-flag-invalid");
        std::fs::write(crate::paths::stage_dir().join("theme.json"), VALID_NOTES).unwrap();
        snapshot("rice.history", "explicit").unwrap();

        let out = handle_rice_entry_mark(&Invocation {
            path: vec!["rice".to_string(), "history".to_string(), "mark".to_string()],
            args: vec!["A".to_string()],
            flags: std::collections::BTreeMap::from([("entry".to_string(), "not-a-number".to_string())]),
            door: aoide_protocol::Door::Cli,
        });
        assert_eq!(out.status, Status::Usage);
        assert_eq!(out.data.unwrap()["reason"], "invalid-entry");
        let _ = std::fs::remove_dir_all(&root);
    }

    // ── `rice back` — the revert that lands the branch-from-any-mark ask ───
    //
    // Unlike `routed_draft` above (a bare Draft-mode marker over a PLAIN
    // stage/theme.json — fine for the snapshot/mark cores, which never
    // check routing), every `rice back` test needs stage/theme.json to be a
    // REAL symlink into the draft file, because `back_unlocked`'s
    // routing-broken rail (D7) checks exactly that before writing anything.

    fn routed_draft_symlinked(tag: &str) -> (std::path::PathBuf, String, String, std::path::PathBuf) {
        let root = unique_tmp(tag);
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);
        let rice = "sonata".to_string();
        let draft = "neon-night".to_string();
        let draft_dir = crate::paths::draft_dir(&rice, &draft);
        std::fs::create_dir_all(&draft_dir).unwrap();
        let draft_theme = draft_dir.join("theme.json");
        std::fs::write(&draft_theme, "{}").unwrap();
        std::os::unix::fs::symlink(&draft_theme, stage.join("theme.json")).unwrap();
        save_mode_marker(&ModeMarker {
            mode: RiceMode::Draft,
            rice: Some(rice.clone()),
            draft: Some(draft.clone()),
            ..Default::default()
        })
        .unwrap();
        (root, rice, draft, draft_theme)
    }

    /// Writes new content THROUGH the routing symlink — plain `std::fs::write`
    /// (unlike `atomic_write`'s rename) follows a symlink transparently, so
    /// this lands in the draft file while leaving the symlink itself intact,
    /// exactly the property every test below relies on to keep re-editing
    /// "the draft" across multiple snapshots.
    fn write_theme(stage: &std::path::Path, bg: &str) {
        std::fs::write(
            stage.join("theme.json"),
            format!(r##"{{"schemaVersion":"0","palette":{{"bg":"{bg}"}}}}"##),
        )
        .unwrap();
    }

    fn inv_back(mark: Option<&str>, entry: Option<u32>) -> Invocation {
        let mut flags = std::collections::BTreeMap::new();
        if let Some(m) = mark {
            flags.insert("mark".to_string(), m.to_string());
        }
        if let Some(t) = entry {
            flags.insert("entry".to_string(), t.to_string());
        }
        Invocation {
            path: vec!["rice".to_string(), "back".to_string()],
            args: vec![],
            flags,
            door: aoide_protocol::Door::Cli,
        }
    }

    /// The acceptance test for the whole ask: revert to a mark, keep
    /// editing, and the entries after the mark survive as siblings of the new
    /// branch rather than being destroyed or renumbered.
    #[test]
    fn revert_to_a_mark_branches_and_leaves_the_old_line_intact() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, rice, draft, draft_theme) = routed_draft_symlinked("back-branch-acceptance");
        let stage = crate::paths::stage_dir();

        write_theme(&stage, "#111111");
        let entry1 = snapshot("rice.history", "stage").unwrap();
        assert_eq!((entry1.entry, entry1.parent), (1, None));

        write_theme(&stage, "#222222");
        let entry2 = snapshot("rice.history", "stage").unwrap();
        assert_eq!((entry2.entry, entry2.parent), (2, Some(1)));

        mark("rice.history.mark", "A", 2).unwrap();

        write_theme(&stage, "#333333");
        let entry3 = snapshot("rice.history", "stage").unwrap();
        assert_eq!((entry3.entry, entry3.parent), (3, Some(2)));
        assert_eq!(history::load_head(&rice, Some(&draft)), Some(3));

        // `rice back --mark A` — the stage exactly matches the head (entry 3)
        // it's about to overwrite, so nothing drifts.
        let out = handle_rice_back(&inv_back(Some("A"), None));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let data = out.data.clone().unwrap();
        assert_eq!(data["from"], 3, "head before the revert");
        assert_eq!(data["to"], 2);
        assert_eq!(data["mark"], "A");
        assert!(data["drifted"].is_null(), "stage matched the head exactly — nothing to preserve");
        assert!(
            data.get("reload").is_none(),
            "a revert never touches Quickshell IPC (D1) — no reload field at all, unlike `rice stage`'s outcome"
        );
        assert_eq!(history::load_head(&rice, Some(&draft)), Some(2), "head moved to the mark's entry");

        let restored: Value = serde_json::from_str(&std::fs::read_to_string(&draft_theme).unwrap()).unwrap();
        assert_eq!(restored, entry2.theme, "the draft file (through the symlink) now holds entry 2's content");

        // Entries 3 is completely untouched — nothing destroyed, nothing
        // renumbered.
        let entry3_after = history::load_history(&rice, Some(&draft), 3).unwrap();
        let stamped3 = HistoryRecord { schema_version: history::HISTORY_SCHEMA_VERSION.to_string(), ..entry3.clone() };
        assert_eq!(entry3_after, stamped3);

        // Branch out again: the next write hangs off the REVERTED head (2),
        // not off entry 3 — this is where branching actually happens.
        write_theme(&stage, "#444444");
        let entry4 = snapshot("rice.history", "stage").unwrap();
        assert_eq!((entry4.entry, entry4.parent), (4, Some(2)), "branches off the reverted head, not off entry 3");

        // Entry 3 is STILL there, still parented on 2 — a sibling of entry 4,
        // not overwritten and not renumbered.
        let entry3_final = history::load_history(&rice, Some(&draft), 3).unwrap();
        assert_eq!(entry3_final.parent, Some(2));
        assert_eq!(history::children(&history::list_history(&rice, Some(&draft)), 2), vec![3, 4], "two branches off the same mark");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn revert_by_entry_number_moves_head_and_restores_content() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, rice, draft, draft_theme) = routed_draft_symlinked("back-by-entry");
        let stage = crate::paths::stage_dir();

        write_theme(&stage, "#aaaaaa");
        let entry1 = snapshot("rice.history", "stage").unwrap();
        write_theme(&stage, "#bbbbbb");
        snapshot("rice.history", "stage").unwrap(); // entry 2, now head

        let out = handle_rice_back(&inv_back(None, Some(1)));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let data = out.data.unwrap();
        assert_eq!(data["to"], 1);
        assert!(data["mark"].is_null(), "a --entry selection reports no mark");
        assert_eq!(history::load_head(&rice, Some(&draft)), Some(1));

        let restored: Value = serde_json::from_str(&std::fs::read_to_string(&draft_theme).unwrap()).unwrap();
        assert_eq!(restored, entry1.theme);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// `rice back` redresses the lock off the entry it restores, the same
    /// writer `rice stage` uses: the target entry's colours with the rice's own
    /// picture, and nothing once the rice ships none.
    #[test]
    fn back_restores_the_lock_dress_off_the_target_entry() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, rice, _draft, _draft_theme) = routed_draft_symlinked("back-lock-dress");
        let stage = crate::paths::stage_dir();
        let picture = crate::paths::rice_dir(&rice).join("lockscreen.png");
        std::fs::write(&picture, b"").unwrap();
        let write_full = |bg: &str| {
            std::fs::write(
                stage.join("theme.json"),
                format!(
                    r##"{{"schemaVersion":"0","palette":{{"bg":"{bg}","fg":"#eeeeee","accent":"#00ff00","urgent":"#ff0000"}}}}"##
                ),
            )
            .unwrap();
        };

        write_full("#111111");
        snapshot("rice.history", "stage").unwrap();
        write_full("#222222");
        snapshot("rice.history", "stage").unwrap();

        let out = handle_rice_back(&inv_back(None, Some(1)));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let lock = std::fs::read_to_string(stage.join("hyprlock.conf")).unwrap();
        assert!(lock.lines().any(|l| l == "$aoide_lock_color = rgb(111111)"), "{lock}");
        assert!(lock.lines().any(|l| l == format!("$aoide_lock_image = {}", picture.display())), "{lock}");
        assert!(out.changed.iter().any(|c| c.ends_with("stage/hyprlock.conf")));
        assert_eq!(out.data.unwrap()["lockScreen"]["status"], "written");

        std::fs::remove_file(&picture).unwrap();
        let out = handle_rice_back(&inv_back(None, Some(2)));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert_eq!(
            std::fs::read_to_string(stage.join("hyprlock.conf")).unwrap(),
            crate::theme::emit::hyprlock::NO_DRESS
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn back_by_entry_reports_null_mark_even_when_the_target_entry_carries_one() {
        // Pins the "how was I asked" semantics [`BackResult::mark`]'s own
        // doc comment claims: the report reflects the SELECTOR the caller
        // used, not whatever the resolved entry happens to own. Both of this
        // file's other `--entry` tests target unmarked entries, so this was
        // otherwise asserted nowhere.
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, rice, draft, _draft_theme) = routed_draft_symlinked("back-entry-target-has-mark");
        let stage = crate::paths::stage_dir();

        write_theme(&stage, "#111111");
        snapshot("rice.history", "stage").unwrap(); // entry 1
        write_theme(&stage, "#222222");
        snapshot("rice.history", "stage").unwrap(); // entry 2, head
        mark("rice.history.mark", "A", 1).unwrap(); // entry 1 — the REVERT TARGET — carries a mark

        let out = handle_rice_back(&inv_back(None, Some(1)));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert!(
            out.data.unwrap()["mark"].is_null(),
            "selected via --entry, not --mark — the report says null even though entry 1 owns mark A"
        );
        assert_eq!(history::load_head(&rice, Some(&draft)), Some(1));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn back_snapshots_an_unentered_hand_edit_before_overwriting_it() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, rice, draft, draft_theme) = routed_draft_symlinked("back-drift-preserved");
        let stage = crate::paths::stage_dir();

        write_theme(&stage, "#111111");
        snapshot("rice.history", "stage").unwrap(); // entry 1, head

        // An unrecorded hand edit — nothing ever called `rice history`/`rice
        // stage` on this content, so it exists nowhere but the live stage.
        write_theme(&stage, "#hand-edited");

        let out = handle_rice_back(&inv_back(None, Some(1)));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let data = out.data.unwrap();
        let drifted_entry = data["drifted"].as_u64().expect("the hand edit must have minted an entry");
        assert_eq!(drifted_entry, 2, "the drift entry hangs off the head it preserved");

        // The unrecorded edit is recoverable: it is exactly what entry 2 holds.
        let preserved = history::load_history(&rice, Some(&draft), 2).unwrap();
        assert_eq!(preserved.cause, "drift");
        assert_eq!(preserved.parent, Some(1));
        assert_eq!(preserved.theme["palette"]["bg"], "#hand-edited");

        // And the revert itself still landed — entry 1's content is now live.
        let restored: Value = serde_json::from_str(&std::fs::read_to_string(&draft_theme).unwrap()).unwrap();
        assert_eq!(restored["palette"]["bg"], "#111111");
        assert_eq!(history::load_head(&rice, Some(&draft)), Some(1));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn back_mints_no_drift_entry_when_the_stage_already_matches_the_head() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, rice, draft, _draft_theme) = routed_draft_symlinked("back-no-drift");
        let stage = crate::paths::stage_dir();

        write_theme(&stage, "#111111");
        snapshot("rice.history", "stage").unwrap(); // entry 1, head — stage matches it exactly

        let out = handle_rice_back(&inv_back(None, Some(1)));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert!(out.data.unwrap()["drifted"].is_null(), "nothing differed from the head — nothing to preserve");
        assert_eq!(
            history::list_history(&rice, Some(&draft)).len(),
            1,
            "no phantom entry minted when there was nothing to drift-capture"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn back_to_the_same_target_twice_in_a_row_mints_no_new_entries_between_calls() {
        // Fork 8 ("a revert is not an entry") proven against the state a
        // REVERT ITSELF produces, not just against a stage nobody has
        // touched yet. `back_mints_no_drift_entry_when_the_stage_already_
        // matches_the_head` above only exercises the latter (nothing ever
        // ran `rice back` before the assertion); this calls `rice back`
        // twice at the SAME target and checks the entry count never grows
        // between the two calls — the second call's drift comparison is
        // against exactly what the first call's own write just produced.
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, rice, draft, _draft_theme) = routed_draft_symlinked("back-repeat-same-target");
        let stage = crate::paths::stage_dir();

        write_theme(&stage, "#111111");
        snapshot("rice.history", "stage").unwrap(); // entry 1
        write_theme(&stage, "#222222");
        snapshot("rice.history", "stage").unwrap(); // entry 2, head

        let first = handle_rice_back(&inv_back(None, Some(1)));
        assert_eq!(first.status, Status::Ok, "{:?}", first.data);
        let count_after_first = history::list_history(&rice, Some(&draft)).len();

        let second = handle_rice_back(&inv_back(None, Some(1)));
        assert_eq!(second.status, Status::Ok, "{:?}", second.data);
        assert!(
            second.data.as_ref().unwrap()["drifted"].is_null(),
            "the second revert to the same target drifts nothing — the stage already IS entry 1"
        );
        assert_eq!(
            history::list_history(&rice, Some(&draft)).len(),
            count_after_first,
            "two consecutive reverts to the same target mint zero entries between them"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn back_clears_a_stale_stage_wallpaper_when_the_target_entry_has_none() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, _rice, _draft, _draft_theme) = routed_draft_symlinked("back-wallpaper-clear");
        let stage = crate::paths::stage_dir();

        write_theme(&stage, "#111111");
        let entry1 = snapshot("rice.history", "stage").unwrap();
        assert!(entry1.wallpaper.is_none());

        // A wallpaper-set lands on top and gets entered — entry 2 carries a wallpaper.
        std::fs::write(stage.join("wallpaper.json"), r#"{"path":"/tmp/x.png"}"#).unwrap();
        let entry2 = snapshot("rice.history", "stage").unwrap();
        assert!(entry2.wallpaper.is_some());
        assert!(stage.join("wallpaper.json").is_file());

        // Revert to entry 1, which has no wallpaper — the stale stage wallpaper
        // (still sitting there from entry 2) must be cleared, not left
        // pointing at a wallpaper entry 1 never had.
        let out = handle_rice_back(&inv_back(None, Some(1)));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert!(out.data.unwrap()["drifted"].is_null(), "stage matched head (entry 2) exactly before the call");
        assert!(!stage.join("wallpaper.json").exists(), "target entry had no wallpaper — the stale stage wallpaper is cleared");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// An entry carries the wallpaper VALUE as-is (CONTRACTS.md §4's `rice back`:
    /// the identical `{"path": …}` seam, verbatim), so an entry minted from a
    /// user PICK must come back AS a pick — `"pick": true` and all, or a
    /// restored wallpaper would stop being the user's choice and the rice's
    /// live board would start drawing over it. And an entry minted with NO wallpaper
    /// must not leave one (of either kind) lying on the stage.
    #[test]
    fn back_round_trips_a_pick_as_a_pick() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, _rice, _draft, _draft_theme) = routed_draft_symlinked("back-wallpaper-pick");
        let stage = crate::paths::stage_dir();

        write_theme(&stage, "#111111");
        crate::wallpaper::stage_default(std::path::Path::new("/tmp/rice-wallpaper.png"), "sonata")
            .unwrap();
        let entry1 = snapshot("rice.history", "stage").unwrap();
        assert!(entry1.wallpaper.is_some());
        assert!(!crate::wallpaper::staged_is_pick());

        crate::wallpaper::stage_pick(
            std::path::Path::new("/tmp/chosen.png"),
            Some("sonata"),
            Some(crate::wallpaper::POSITION_TILE),
        )
        .unwrap();
        let entry2 = snapshot("rice.history", "stage").unwrap();
        assert!(entry2.wallpaper.is_some());

        let to_default = handle_rice_back(&inv_back(None, Some(1)));
        assert_eq!(to_default.status, Status::Ok, "{:?}", to_default.data);
        assert!(!crate::wallpaper::staged_is_pick(), "the default entry restored no pick");
        assert_eq!(crate::wallpaper::staged_path().as_deref(), Some("/tmp/rice-wallpaper.png"));

        let to_pick = handle_rice_back(&inv_back(None, Some(2)));
        assert_eq!(to_pick.status, Status::Ok, "{:?}", to_pick.data);
        assert!(crate::wallpaper::staged_is_pick(), "a restored pick is still a pick");
        assert_eq!(crate::wallpaper::staged_path().as_deref(), Some("/tmp/chosen.png"));
        let raw = std::fs::read_to_string(stage.join("wallpaper.json")).unwrap();
        assert!(raw.contains("\"pick\": true"), "verbatim: {raw}");
        assert!(raw.contains("\"position\": \"tile\""), "verbatim: {raw}");
        assert!(raw.contains("\"tile\": true"), "verbatim: {raw}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn back_refuses_when_the_routing_symlink_is_missing() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let root = unique_tmp("back-routing-broken");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);
        let rice = "sonata".to_string();
        let draft = "neon-night".to_string();
        // Draft mode claimed, but stage/theme.json is a PLAIN file, never
        // routed through `rice mode draft`'s symlink.
        std::fs::write(stage.join("theme.json"), VALID_NOTES).unwrap();
        save_mode_marker(&ModeMarker {
            mode: RiceMode::Draft,
            rice: Some(rice.clone()),
            draft: Some(draft.clone()),
            ..Default::default()
        })
        .unwrap();
        // TWO entries, so target (1) and the pre-existing head (falls back to
        // the max, 2) differ — this is what makes the closing head
        // assertion discriminate. With only one entry (both = 1), the
        // fallback in `load_head` lands on 1 whether or not `save_head` ever
        // ran, so that assertion would hold even if the routing-broken
        // refusal secretly still moved the cursor.
        std::fs::create_dir_all(crate::paths::draft_dir(&rice, &draft).join("history")).unwrap();
        let seed = |n: u32, parent: Option<u32>| HistoryRecord {
            schema_version: String::new(),
            entry: n,
            parent,
            at: "2026-08-18T00:00:00Z".to_string(),
            session_id: None,
            cause: "explicit".to_string(),
            theme: serde_json::json!({ "schemaVersion": "0" }),
            wallpaper: None,
            widgets: serde_json::json!({}),
        };
        history::save_history(&rice, Some(&draft), &seed(1, None)).unwrap();
        history::save_history(&rice, Some(&draft), &seed(2, Some(1))).unwrap();

        let out = handle_rice_back(&inv_back(None, Some(1)));
        assert_eq!(out.status, Status::Error);
        assert_eq!(out.data.unwrap()["reason"], "routing-broken");
        // Nothing written: the plain stage file is untouched and the head
        // cursor is exactly what it was before this call (the fallback max,
        // 2) — NOT moved to the target (1), which is what a bug that let
        // `save_head` run despite the refusal would produce.
        assert_eq!(
            std::fs::read_to_string(stage.join("theme.json")).unwrap(),
            VALID_NOTES,
            "a routing-broken refusal writes nothing"
        );
        assert_eq!(
            history::load_head(&rice, Some(&draft)),
            Some(2),
            "head stays at the pre-existing fallback (2), never claimed down to the target (1) by this call"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn back_with_both_entry_and_mark_given_entry_wins() {
        // No sibling command has two co-present selectors, so the reviewer
        // ruled a silent `--entry`-wins priority defensible but unpinned —
        // this test is the pin.
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, rice, draft, _draft_theme) = routed_draft_symlinked("back-both-flags-entry-wins");
        let stage = crate::paths::stage_dir();

        write_theme(&stage, "#111111");
        snapshot("rice.history", "stage").unwrap(); // entry 1
        write_theme(&stage, "#222222");
        snapshot("rice.history", "stage").unwrap(); // entry 2, head
        mark("rice.history.mark", "A", 2).unwrap(); // A names entry 2 — the OTHER selection

        let out = handle_rice_back(&inv_back(Some("A"), Some(1)));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let data = out.data.unwrap();
        assert_eq!(data["to"], 1, "--entry wins over a co-present --mark naming a different entry");
        assert!(data["mark"].is_null(), "the winning selector was --entry, so mark reports null");
        assert_eq!(history::load_head(&rice, Some(&draft)), Some(1));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn back_entry_not_found_and_mark_not_found() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, _rice, _draft, _draft_theme) = routed_draft_symlinked("back-not-found");
        let stage = crate::paths::stage_dir();
        write_theme(&stage, "#111111");
        snapshot("rice.history", "stage").unwrap(); // only entry 1 exists, no marks

        let missing_entry = handle_rice_back(&inv_back(None, Some(99)));
        assert_eq!(missing_entry.status, Status::Error);
        assert_eq!(missing_entry.data.unwrap()["reason"], "entry-not-found");

        let missing_mark = handle_rice_back(&inv_back(Some("Z"), None));
        assert_eq!(missing_mark.status, Status::Error);
        assert_eq!(missing_mark.data.unwrap()["reason"], "mark-not-found");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn back_refuses_before_its_first_write_when_the_gate_refuses() {
        // H1: the §7.5 gate runs at the HEAD of the back path — above the
        // drift snapshot, which is itself a write (it mints an entry). A refusal
        // must therefore cost the operator NOTHING: no drift entry, no reverted
        // theme, no geometry re-applied to the compositor, no wallpaper rewrite,
        // no cursor move. This is the case §9(d) never reached, because it
        // tests `rice stage`.
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&[
            "AOIDE_STAGE_DIR",
            "STRILLIX_TEMPLATES",
            "PATH",
            crate::widgets::RICES_EVAL_FIXTURE_VAR,
        ]);
        std::env::remove_var(crate::widgets::RICES_EVAL_FIXTURE_VAR);
        let _armed = crate::commands::test_support::require_the_real_rices_path();
        std::env::remove_var("STRILLIX_TEMPLATES");

        let root = unique_tmp("back-gate-refused");
        let stage = root.join("aoide").join("rice").join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        let templates = root.join("templates");
        std::fs::create_dir_all(&templates).unwrap();
        std::fs::write(templates.join("manifest.json"), r#"{"sonata":{}}"#).unwrap();
        std::fs::write(templates.join("registry.json"), r#"{"sonata":{}}"#).unwrap();
        // Built in, but its machine copy DIFFERS (the templates carry no
        // `sonata/` folder at all), so case 2 applies — and the generator
        // answers that this rice needs a package the system does not carry.
        std::fs::write(
            templates.join("builtin.json"),
            r#"{"declared":"sonata","rices":["sonata"],"packages":[]}"#,
        )
        .unwrap();
        crate::commands::test_support::nix_instantiate_recorder(
            &root,
            r#"{"manifest":{"sonata":{"bar":{"owner":"sonata","file":"bar.qml"}}},"registry":{"sonata":{}},"packages":{"sonata":["not-installed-pkg"]}}"#,
        );
        crate::commands::test_support::prepend_path(&root.join("fake-bin"));

        aoide_test_support::pin_stage(&stage);
        std::env::set_var("STRILLIX_TEMPLATES", &templates);
        std::env::remove_var(crate::widgets::RICES_EVAL_FIXTURE_VAR);

        std::fs::write(stage.join("theme.json"), VALID_NOTES).unwrap();
        let strillix_root = crate::paths::rice_dir("sonata");
        std::fs::create_dir_all(strillix_root.join("widgets")).unwrap();
        std::fs::write(strillix_root.join("theme.json"), VALID_NOTES).unwrap();
        std::fs::write(strillix_root.join("widgets").join("bar.qml"), "// bar\n").unwrap();
        // Unlocked staging mode: `rice back` refuses while declarative, and
        // entries only exist while unlocked — a mode refusal would mask the gate.
        save_mode_marker(&ModeMarker {
            mode: RiceMode::Staging,
            rice: Some("sonata".to_string()),
            ..Default::default()
        })
        .unwrap();
        history::save_history(
            "sonata",
            None,
            &HistoryRecord {
                schema_version: String::new(),
                entry: 1,
                parent: None,
                at: "2026-08-18T00:00:00Z".to_string(),
                session_id: None,
                cause: "explicit".to_string(),
                theme: serde_json::json!({ "schemaVersion": "0" }),
                wallpaper: None,
                widgets: serde_json::json!({}),
            },
        )
        .unwrap();

        // The stage lock file, pre-created: `with_stage_lock` creates it on the
        // way in (the call's own doing, before any policy runs), so it must
        // exist before the snapshot or it would read as a difference.
        std::fs::write(stage.join(".stage.lock"), "").unwrap();

        let head_before = history::load_head("sonata", None);
        let before = crate::commands::test_support::tree_snapshot(&[&stage, &strillix_root]);
        let out = handle_rice_back(&inv_back(None, Some(1)));

        assert_eq!(out.status, Status::Error, "{:?}", out.data);
        assert!(
            out.message.contains(
                "rebuild needed: sonata needs not-installed-pkg (not in this system)"
            ),
            "the taught refusal, not a half-revert: {}",
            out.message
        );
        assert_eq!(
            crate::commands::test_support::tree_snapshot(&[&stage, &strillix_root]),
            before,
            "a refused `rice back` writes nothing: stage/ and the rice dir are byte-unchanged"
        );
        assert_eq!(
            history::load_head("sonata", None),
            head_before,
            "and the head cursor never moved (it is `Some(1)` here — the max-entry \
             fallback for a rice with no `head.json`)"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn back_invalid_entry_and_mark_flags_are_rejected_before_anything_else() {
        // Malformed-flag shape — the bare (neither flag) form is covered by
        // `back_bare_invocation_refuses_without_reading_stdin_or_writing_anything`.
        let bad_entry = handle_rice_back(&Invocation {
            path: vec!["rice".to_string(), "back".to_string()],
            args: vec![],
            flags: std::collections::BTreeMap::from([("entry".to_string(), "nope".to_string())]),
            door: aoide_protocol::Door::Cli,
        });
        assert_eq!(bad_entry.status, Status::Usage);
        assert_eq!(bad_entry.data.unwrap()["reason"], "invalid-entry");

        let bad_mark = handle_rice_back(&inv_back(Some("ab"), None));
        assert_eq!(bad_mark.status, Status::Error);
        assert_eq!(bad_mark.data.unwrap()["reason"], "invalid-mark");
    }

    #[test]
    fn back_bare_invocation_refuses_without_reading_stdin_or_writing_anything() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, rice, draft, _draft_theme) = routed_draft_symlinked("back-bare-refuses");
        let stage = crate::paths::stage_dir();
        write_theme(&stage, "#111111");
        snapshot("rice.history", "stage").unwrap();

        // Snapshot the WHOLE store's observable state before the call, not
        // just the head cursor — the plan's requirement is that the STORE
        // is untouched, and a head-only assertion would miss a future
        // regression that (say) wrote marks.json without moving the head.
        let before_entry_count = history::list_history(&rice, Some(&draft)).len();
        let before_marks = history::load_marks(&rice, Some(&draft));
        let theme_path = stage.join("theme.json");
        let before_theme = std::fs::read(&theme_path).unwrap();
        let wallpaper_path = stage.join("wallpaper.json");
        let before_wallpaper = std::fs::read(&wallpaper_path).ok();

        // Neither --entry nor --mark: `pick::interactive` checks a REAL tty
        // (`std::io::IsTerminal`), and cargo test's own stdin/stdout are a
        // pipe/file, never a tty, so `inv.door == Cli` here still can't
        // reach phase A8's picker branch — this exercises the exact same
        // non-tty usage refusal it always has, verbatim.
        let out = handle_rice_back(&inv_back(None, None));
        assert_eq!(out.status, Status::Usage);
        assert_eq!(out.data.unwrap()["reason"], "no-selection");
        assert_eq!(history::load_head(&rice, Some(&draft)), Some(1), "the bare refusal moved nothing");
        assert_eq!(history::list_history(&rice, Some(&draft)).len(), before_entry_count, "no entry was minted or removed");
        assert_eq!(history::load_marks(&rice, Some(&draft)), before_marks, "marks.json is untouched");
        assert_eq!(std::fs::read(&theme_path).unwrap(), before_theme, "the stage theme is byte-identical");
        assert_eq!(std::fs::read(&wallpaper_path).ok(), before_wallpaper, "the stage wallpaper is byte-identical (still absent)");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A8's own assertion on the SAME non-tty behavior above: with the
    /// picker built, a bare invocation off a tty must still refuse rather
    /// than prompt, and must say WHY in terms of the door it actually got.
    /// Pins the wording, so a later step cannot quietly reintroduce a
    /// promise that the picker is unbuilt.
    #[test]
    fn back_bare_invocation_off_a_tty_names_the_picker_and_still_refuses() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, rice, draft, _draft_theme) = routed_draft_symlinked("back-bare-verbatim");
        let stage = crate::paths::stage_dir();
        write_theme(&stage, "#111111");
        snapshot("rice.history", "stage").unwrap();

        let out = handle_rice_back(&inv_back(None, None));
        assert_eq!(out.status, Status::Usage);
        assert_eq!(
            out.message,
            "usage: strillix rice back --entry N | --mark <letter> [--json] \
             (a bare `rice back` opens a numbered picker on a tty; \
             this door is not one, so pass --entry or --mark)"
        );
        assert_eq!(out.data.unwrap()["reason"], "no-selection");
        assert_eq!(history::load_head(&rice, Some(&draft)), Some(1), "writes nothing");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn back_refuses_outside_draft_mode() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let stage = unique_tmp("back-not-draft").join("stage");
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        let out = handle_rice_back(&inv_back(None, Some(1)));
        assert_eq!(out.status, Status::Error);
        assert_eq!(out.data.unwrap()["reason"], "not-staged-or-drafted");
        let _ = std::fs::remove_dir_all(&stage);
    }

    // ── `rice history list` — the tree (phase A6) ──────────────────────────

    #[test]
    fn render_tree_is_empty_for_an_empty_store() {
        assert_eq!(render_tree(&[], None, &BTreeMap::new()), "");
    }

    #[test]
    fn render_tree_draws_a_linear_store_with_no_branch_glyphs_marks_bracketed_head_arrowed() {
        let entries = vec![rec(1, None), rec(2, Some(1)), rec(3, Some(2))];
        let marks = BTreeMap::from([("A".to_string(), 1u32)]);

        let out = render_tree(&entries, Some(3), &marks);

        assert!(!out.contains("\u{251c}\u{2500}"), "a straight line draws no fork glyph: {out}");
        assert!(!out.contains("\u{2514}\u{2500}"), "a straight line draws no fork glyph: {out}");
        assert!(out.contains("[A]"), "the marked entry brackets its letter: {out}");
        assert!(out.contains("\u{2190} head"), "the head entry is arrowed: {out}");

        // Depth-first, ascending: 1 before 2 before 3.
        let pos = |needle: &str| out.find(needle).unwrap_or_else(|| panic!("{needle} missing from:\n{out}"));
        assert!(pos("0001") < pos("0002"));
        assert!(pos("0002") < pos("0003"));
        // The head arrow lands on entry 3's own line, not anywhere else.
        let head_line = out.lines().find(|l| l.contains("0003")).unwrap();
        assert!(head_line.contains("\u{2190} head"));
        assert!(!out.lines().find(|l| l.contains("0001")).unwrap().contains("\u{2190} head"));
    }

    #[test]
    fn render_tree_draws_both_children_of_a_fork_under_the_mark_with_branch_glyphs() {
        // 1 <- 9[A] <- 17
        //          \- 21 <- head
        let entries = vec![rec(1, None), rec(9, Some(1)), rec(17, Some(9)), rec(21, Some(9))];
        let marks = BTreeMap::from([("A".to_string(), 9u32)]);

        let out = render_tree(&entries, Some(21), &marks);

        assert!(out.contains("\u{251c}\u{2500}"), "a fork draws the non-last branch glyph: {out}");
        assert!(out.contains("\u{2514}\u{2500}"), "a fork draws the last branch glyph: {out}");
        assert!(out.contains("0017"), "the first child renders: {out}");
        assert!(out.contains("0021"), "the second child renders: {out}");
        let mark_line = out.lines().find(|l| l.contains("0009")).unwrap();
        assert!(mark_line.contains("[A]"), "both children hang off the marked entry: {out}");
        let head_line = out.lines().find(|l| l.contains("0021")).unwrap();
        assert!(head_line.contains("\u{2190} head"));
    }

    // ── entry_lane_rows: the picker's row source (phase A8) ─────────────────

    #[test]
    fn entry_lane_rows_pairs_each_line_with_its_entry_number_through_the_tree_glyphs() {
        // Same fork shape as the render_tree test just above, reused so this
        // proves the digit-scan survives real branch glyphs (`\u{251c}\u{2500}`/
        // `\u{2514}\u{2500}`), not just a straight unbranched line.
        // 1 <- 9[A] <- 17
        //          \- 21 <- head
        let entries = vec![rec(1, None), rec(9, Some(1)), rec(17, Some(9)), rec(21, Some(9))];
        let marks = BTreeMap::from([("A".to_string(), 9u32)]);

        let lane = entry_lane_rows(&entries, Some(21), &marks);

        let numbers: Vec<u32> = lane.iter().map(|(n, _)| *n).collect();
        assert_eq!(numbers, vec![1, 9, 17, 21], "depth-first order, matching render_tree's own walk");
        let (_, row_21) = lane.iter().find(|(n, _)| *n == 21).unwrap();
        assert!(row_21.contains("\u{2190} head"), "the row text is render_tree's own line, untouched: {row_21}");
        let (_, row_9) = lane.iter().find(|(n, _)| *n == 9).unwrap();
        assert!(row_9.contains("[A]"));
    }

    #[test]
    fn entry_lane_rows_is_empty_for_an_empty_store() {
        assert!(entry_lane_rows(&[], None, &BTreeMap::new()).is_empty());
    }

    // ── default_row_index: fork 6's own rule, off a tty ────────────────────

    #[test]
    fn default_row_index_targets_the_heads_parent_when_present() {
        let entries = vec![rec(1, None), rec(2, Some(1)), rec(3, Some(1))];
        let lane = entry_lane_rows(&entries, Some(3), &BTreeMap::new());
        assert_eq!(default_row_index(&entries, Some(3), &lane), Some(0), "row 0 is entry 1, the head's (3's) parent");
    }

    #[test]
    fn default_row_index_is_none_for_a_root_head_with_no_parent() {
        let entries = vec![rec(1, None)];
        let lane = entry_lane_rows(&entries, Some(1), &BTreeMap::new());
        assert_eq!(
            default_row_index(&entries, Some(1), &lane),
            None,
            "a root head has no parent to default to -- Enter must abort, not pick something arbitrary"
        );
    }

    #[test]
    fn default_row_index_is_none_when_the_heads_parent_is_not_in_the_store() {
        // Entry 9's own `parent` names 99, an entry absent from `entries` entirely -- the same
        // dangling-pointer shape A6's render_tree already renders as "detached" rather than
        // hiding. `lane` therefore has no row for 99, and the default must fail safe to None
        // instead of pointing at a row that does not correspond to what `parent` actually names.
        let entries = vec![rec(1, None), rec(9, Some(99))];
        let lane = entry_lane_rows(&entries, Some(9), &BTreeMap::new());
        assert_eq!(default_row_index(&entries, Some(9), &lane), None);
    }

    #[test]
    fn render_tree_skips_a_mark_naming_a_entry_that_no_longer_exists() {
        // Self-healing read (D6): a stale marks.json entry for a pruned/hand-removed entry must
        // never panic or otherwise surface — it is simply never looked up, because this walks
        // `entries`, not `marks`.
        let entries = vec![rec(1, None)];
        let marks = BTreeMap::from([("Z".to_string(), 99u32)]);
        let out = render_tree(&entries, Some(1), &marks);
        assert!(!out.contains("[Z]"), "a mark naming a nonexistent entry never renders: {out}");
        assert!(out.contains("0001"));
    }

    #[test]
    fn render_tree_renders_an_orphan_entry_marked_detached_instead_of_vanishing() {
        // Entry 5's parent (2) is not in the store at all — a crash mid-prune, or a hand edit.
        // `crate::history` is deliberately tolerant of exactly this shape; the tree view
        // must surface it, not silently drop an entry that still exists on disk.
        let entries = vec![rec(1, None), rec(5, Some(2))];
        let marks = BTreeMap::new();

        let out = render_tree(&entries, None, &marks);

        assert!(out.contains("0001"), "the real root still renders: {out}");
        let orphan_line = out.lines().find(|l| l.contains("0005")).unwrap_or_else(|| panic!("orphan entry 5 vanished: {out}"));
        assert!(orphan_line.contains("detached"), "the orphan is labeled, not silently a plain row: {orphan_line}");
        assert!(orphan_line.contains("0002"), "the label names the missing parent: {orphan_line}");
    }

    #[test]
    fn render_tree_an_orphan_that_is_also_head_still_gets_the_head_arrow() {
        let entries = vec![rec(1, None), rec(5, Some(2))];
        let marks = BTreeMap::new();

        let out = render_tree(&entries, Some(5), &marks);

        let orphan_line = out.lines().find(|l| l.contains("0005")).unwrap();
        assert!(orphan_line.contains("detached"), "still detached: {orphan_line}");
        assert!(orphan_line.contains("\u{2190} head"), "still arrowed as head: {orphan_line}");
    }

    #[test]
    fn render_tree_terminates_on_a_two_node_parent_cycle_rendering_each_entry_exactly_once() {
        // 1's parent is 2, 2's parent is 1 — a hand-edited/corrupted pair with no `None`-parented
        // root at all. Neither the roots pass nor a plain child walk can ever reach either one;
        // only the sweep finds them, and only the `visited` guard stops it recursing forever once
        // it does.
        let entries = vec![rec(1, Some(2)), rec(2, Some(1))];
        let marks = BTreeMap::new();

        let out = render_tree(&entries, None, &marks);

        let ones = out.lines().filter(|l| l.contains("0001")).count();
        let twos = out.lines().filter(|l| l.contains("0002")).count();
        assert_eq!(ones, 1, "entry 1 renders exactly once, not looping: {out}");
        assert_eq!(twos, 1, "entry 2 renders exactly once, not looping: {out}");
        assert!(out.contains("detached"), "the cycle's entry point is labeled detached: {out}");
    }

    #[test]
    fn rice_entry_list_handler_on_an_empty_store_is_ok_with_an_empty_list() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let (root, _rice, _draft) = routed_draft("entry-list-empty");

        let out = handle_rice_entry_list(&inv(&["rice", "history", "list"], &[]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let data = out.data.unwrap();
        assert_eq!(data["history"].as_array().unwrap().len(), 0);
        assert!(data["head"].is_null());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rice_entry_list_handler_refuses_outside_draft_mode() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let stage = unique_tmp("entry-list-not-draft").join("stage");
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        let out = handle_rice_entry_list(&inv(&["rice", "history", "list"], &[]));
        assert_eq!(out.status, Status::Error);
        assert_eq!(out.data.unwrap()["reason"], "not-staged-or-drafted");
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn rice_entry_list_json_shape_carries_parent_and_mark_for_a_branching_store() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, rice, draft) = routed_draft("entry-list-json-shape");
        std::fs::write(crate::paths::stage_dir().join("theme.json"), VALID_NOTES).unwrap();

        let entry1 = snapshot("rice.history", "explicit").unwrap(); // 1, no parent
        std::fs::write(
            crate::paths::stage_dir().join("theme.json"),
            r##"{"schemaVersion":"0","palette":{"bg":"#222222"}}"##,
        )
        .unwrap();
        let entry2 = snapshot("rice.history", "explicit").unwrap(); // 2, parent 1
        assert_eq!((entry1.entry, entry2.entry, entry2.parent), (1, 2, Some(1)));
        mark("rice.history.mark", "A", 1).unwrap();

        let out = handle_rice_entry_list(&inv(&["rice", "history", "list"], &[]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let data = out.data.unwrap();
        assert_eq!(data["rice"], rice);
        assert_eq!(data["draft"], draft);
        assert_eq!(data["head"], 2);

        let entries = data["history"].as_array().unwrap();
        assert_eq!(entries.len(), 2);
        let entry_1 = entries.iter().find(|e| e["entry"] == 1).unwrap();
        assert!(entry_1["parent"].is_null(), "the first entry carries no parent");
        assert_eq!(entry_1["mark"], "A");
        let entry_2 = entries.iter().find(|e| e["entry"] == 2).unwrap();
        assert_eq!(entry_2["parent"], 1, "the flat array carries the parent pointer");
        assert!(entry_2["mark"].is_null(), "an unmarked entry reports a null mark, not an absent key");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rice_entry_list_json_lists_an_orphan_entry_too_the_builder_walks_the_flat_list_not_the_tree() {
        // `handle_rice_entry_list`'s `--json` array is built straight off `history::list_history`
        // (a flat list), never off `render_tree`'s walk — so it was never exposed to the tree
        // walk's orphan defect, but this pins that fact so it can't regress silently if the
        // builder ever gets rewritten to reuse the tree instead.
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let (root, rice, draft) = routed_draft("entry-list-json-orphan");

        let seed = |n: u32, parent: Option<u32>| HistoryRecord {
            schema_version: String::new(),
            entry: n,
            parent,
            at: "2026-08-18T00:00:00Z".to_string(),
            session_id: None,
            cause: "explicit".to_string(),
            theme: serde_json::json!({}),
            wallpaper: None,
            widgets: serde_json::json!({}),
        };
        history::save_history(&rice, Some(&draft), &seed(1, None)).unwrap();
        history::save_history(&rice, Some(&draft), &seed(5, Some(2))).unwrap(); // parent 2 never existed

        let out = handle_rice_entry_list(&inv(&["rice", "history", "list"], &[]));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let entries = out.data.unwrap()["history"].as_array().unwrap().clone();
        assert_eq!(entries.len(), 2, "the orphan is still listed: {entries:?}");
        assert!(entries.iter().any(|e| e["entry"] == 5), "entry 5 is present even though its parent is gone");
        let _ = std::fs::remove_dir_all(&root);
    }

    // ── `rice history diff` — key-wise theme diff (phase A7) ──────────────

    #[test]
    fn diff_theme_detects_added_removed_and_changed_keys_at_depth() {
        let a = json!({
            "schemaVersion": "0",
            "palette": { "bg": "#111111", "fg": "#eeeeee" },
            "widgets": { "clock": { "enabled": true } },
        });
        let b = json!({
            "schemaVersion": "0",
            "palette": { "bg": "#222222" },
            "widgets": { "clock": { "enabled": true }, "battery": { "enabled": false } },
        });

        let diff = diff_theme(&a, &b);
        let find = |path: &str| diff.iter().find(|(p, _, _)| p == path);

        let changed = find("palette.bg").expect("changed key at depth found");
        assert_eq!(changed.1, Some(json!("#111111")));
        assert_eq!(changed.2, Some(json!("#222222")));

        let removed = find("palette.fg").expect("removed key at depth found");
        assert_eq!(removed.1, Some(json!("#eeeeee")));
        assert_eq!(removed.2, None);

        let added = find("widgets.battery").expect("added key at depth found");
        assert_eq!(added.1, None);
        assert_eq!(added.2, Some(json!({ "enabled": false })));

        assert!(find("widgets.clock").is_none(), "an unchanged subtree produces no entries");
        assert!(find("schemaVersion").is_none(), "an unchanged top-level scalar produces no entry");
        assert_eq!(diff.len(), 3, "exactly the three real differences, nothing else");
    }

    #[test]
    fn diff_theme_key_reordering_alone_produces_an_empty_diff() {
        // Fork 7's whole reason for a key-wise diff over a text diff: the same keys/values in a
        // different insertion order must NOT read as a change.
        let a = json!({ "schemaVersion": "0", "palette": { "bg": "#111111", "fg": "#222222" } });
        let b: Value =
            serde_json::from_str(r##"{"palette":{"fg":"#222222","bg":"#111111"},"schemaVersion":"0"}"##).unwrap();
        assert!(diff_theme(&a, &b).is_empty(), "reordered keys, identical values: no diff");
    }

    #[test]
    fn diff_theme_identical_documents_produce_an_empty_diff() {
        let a = json!({ "schemaVersion": "0", "palette": { "bg": "#111111" } });
        assert!(diff_theme(&a, &a.clone()).is_empty());
    }

    fn inv_diff(mark: Option<&str>, entry: Option<u32>) -> Invocation {
        let mut flags = std::collections::BTreeMap::new();
        if let Some(m) = mark {
            flags.insert("mark".to_string(), m.to_string());
        }
        if let Some(t) = entry {
            flags.insert("entry".to_string(), t.to_string());
        }
        Invocation {
            path: vec!["rice".to_string(), "history".to_string(), "diff".to_string()],
            args: vec![],
            flags,
            door: aoide_protocol::Door::Cli,
        }
    }

    /// THE branch-specific assertion the plan calls out as the one that matters most: the
    /// default base must be the nearest mark ON THE HEAD'S OWN ANCESTRY, never the highest
    /// letter stamped anywhere in the draft. The store below is built so those two answers
    /// differ — mark B is minted LATER and sorts higher than mark A, but B lands on a branch
    /// the head never descends from, while A sits directly on the head's own line. A
    /// highest-letter (or most-recently-stamped) implementation picks B/entry 4 here; the
    /// correct ancestry walk picks A/entry 2.
    #[test]
    fn diff_default_base_is_the_nearest_ancestral_mark_on_the_current_branch_not_the_highest_letter() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, rice, draft, _draft_theme) = routed_draft_symlinked("diff-nearest-ancestral-mark");
        let stage = crate::paths::stage_dir();

        // entry 1 — the root.
        write_theme(&stage, "#111111");
        snapshot("rice.history", "stage").unwrap();

        // entry 2 — mark A here. This IS on the branch the head ends up on.
        write_theme(&stage, "#222222");
        let entry2 = snapshot("rice.history", "stage").unwrap();
        assert_eq!(entry2.parent, Some(1));
        mark("rice.history.mark", "A", 2).unwrap();

        // Branch off the ROOT instead (a sibling of entry 2, not a descendant): revert to entry
        // 1, then take again. The counter is flat and monotone (never per-branch), so this
        // mints entry 3, not entry 4 — reverting alone never mints anything by itself.
        let back1 = handle_rice_back(&inv_back(None, Some(1)));
        assert_eq!(back1.status, Status::Ok, "{:?}", back1.data);
        write_theme(&stage, "#444444");
        let entry3 = snapshot("rice.history", "stage").unwrap();
        assert_eq!(entry3.parent, Some(1), "entry 3 hangs off the root, a sibling of entry 2");
        // Mark B here — mint order AND alphabet both put it "highest", but the head below
        // never descends from it.
        mark("rice.history.mark", "B", 3).unwrap();

        // Return to entry 2's branch and extend it — this is where the head ends up.
        let back2 = handle_rice_back(&inv_back(None, Some(2)));
        assert_eq!(back2.status, Status::Ok, "{:?}", back2.data);
        write_theme(&stage, "#333333");
        let entry4 = snapshot("rice.history", "stage").unwrap();
        assert_eq!(entry4.parent, Some(2));
        assert_eq!(history::load_head(&rice, Some(&draft)), Some(4));

        let out = handle_rice_entry_diff(&inv_diff(None, None));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let data = out.data.unwrap();
        assert_eq!(
            data["base"], 2,
            "the nearest mark ON THE HEAD'S ANCESTRY (A/entry 2), not the highest letter (B/entry 3)"
        );

        let diff = data["diff"].as_array().unwrap();
        assert_eq!(diff.len(), 1);
        assert_eq!(diff[0]["path"], "palette.bg");
        assert_eq!(diff[0]["old"], "#222222");
        assert_eq!(diff[0]["new"], "#333333");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Every OTHER default-base test in this file happens to put the nearest mark at ancestry
    /// depth 0 (the head itself) or depth 1 (its immediate parent) — nothing before this test
    /// empirically proved the walk goes any further than `anc.get(0)`/`anc.get(1)`. This store
    /// puts an UNMARKED two-entry gap between the head and the marked ancestor, so the mark sits
    /// at depth 3: a shallow implementation that only ever inspects the head and its immediate
    /// parent (falling back to the parent unconditionally, marked or not) lands on entry 3
    /// instead — this asserts the real answer, entry 1.
    #[test]
    fn diff_default_base_finds_a_mark_at_ancestry_depth_three_not_just_the_head_or_its_immediate_parent() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, _rice, _draft) = routed_draft("diff-depth-three-mark");
        std::fs::write(crate::paths::stage_dir().join("theme.json"), VALID_NOTES).unwrap();
        snapshot("rice.history", "stage").unwrap(); // entry 1 — the root, marked
        mark("rice.history.mark", "A", 1).unwrap();

        std::fs::write(
            crate::paths::stage_dir().join("theme.json"),
            r##"{"schemaVersion":"0","palette":{"bg":"#222222"}}"##,
        )
        .unwrap();
        snapshot("rice.history", "stage").unwrap(); // entry 2 — unmarked
        std::fs::write(
            crate::paths::stage_dir().join("theme.json"),
            r##"{"schemaVersion":"0","palette":{"bg":"#333333"}}"##,
        )
        .unwrap();
        snapshot("rice.history", "stage").unwrap(); // entry 3 — unmarked
        std::fs::write(
            crate::paths::stage_dir().join("theme.json"),
            r##"{"schemaVersion":"0","palette":{"bg":"#444444"}}"##,
        )
        .unwrap();
        snapshot("rice.history", "stage").unwrap(); // entry 4 — head, unmarked

        // ancestry(4) = [4, 3, 2, 1] — the mark sits at index 3, two whole unmarked entries (3
        // and 2) away from the head.
        let out = handle_rice_entry_diff(&inv_diff(None, None));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert_eq!(
            out.data.unwrap()["base"], 1,
            "the mark at ancestry depth 3 must still be found, not just depth 0 (head) or depth 1 (its parent)"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn diff_falls_back_to_the_heads_parent_when_nothing_in_the_ancestry_is_marked() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, _rice, _draft) = routed_draft("diff-fallback-parent");
        std::fs::write(crate::paths::stage_dir().join("theme.json"), VALID_NOTES).unwrap();
        snapshot("rice.history", "stage").unwrap(); // entry 1
        std::fs::write(
            crate::paths::stage_dir().join("theme.json"),
            r##"{"schemaVersion":"0","palette":{"bg":"#222222"}}"##,
        )
        .unwrap();
        snapshot("rice.history", "stage").unwrap(); // entry 2, head, unmarked

        let out = handle_rice_entry_diff(&inv_diff(None, None));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert_eq!(out.data.unwrap()["base"], 1, "no marks anywhere — falls back to the head's own parent");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn diff_on_an_entirely_empty_store_reports_nothing_to_diff_against() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let (root, _rice, _draft) = routed_draft("diff-empty-store");

        let out = handle_rice_entry_diff(&inv_diff(None, None));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let data = out.data.unwrap();
        assert!(data["base"].is_null());
        assert_eq!(data["diff"].as_array().unwrap().len(), 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn diff_with_a_single_unmarked_root_entry_reports_nothing_to_diff_against() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, _rice, _draft) = routed_draft("diff-root-only-unmarked");
        std::fs::write(crate::paths::stage_dir().join("theme.json"), VALID_NOTES).unwrap();
        snapshot("rice.history", "stage").unwrap(); // entry 1 — the only entry, no parent, no mark

        let out = handle_rice_entry_diff(&inv_diff(None, None));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let data = out.data.unwrap();
        assert!(data["base"].is_null(), "the root has no parent and no mark on its own ancestry");
        assert_eq!(data["diff"].as_array().unwrap().len(), 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn diff_identical_content_against_a_self_marked_head_is_an_empty_diff() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, _rice, _draft) = routed_draft("diff-identical-self-mark");
        std::fs::write(crate::paths::stage_dir().join("theme.json"), VALID_NOTES).unwrap();
        let entry1 = snapshot("rice.history", "stage").unwrap();
        mark("rice.history.mark", "A", entry1.entry).unwrap();

        // The head itself is marked and the stage has not changed since — base resolves to the
        // head, and comparing it to itself is an empty diff.
        let out = handle_rice_entry_diff(&inv_diff(None, None));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let data = out.data.unwrap();
        assert_eq!(data["base"], 1);
        assert_eq!(data["diff"].as_array().unwrap().len(), 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn diff_entry_flag_overrides_the_default_base() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, _rice, _draft) = routed_draft("diff-entry-flag");
        std::fs::write(crate::paths::stage_dir().join("theme.json"), VALID_NOTES).unwrap();
        snapshot("rice.history", "stage").unwrap(); // entry 1
        std::fs::write(
            crate::paths::stage_dir().join("theme.json"),
            r##"{"schemaVersion":"0","palette":{"bg":"#222222"}}"##,
        )
        .unwrap();
        snapshot("rice.history", "stage").unwrap(); // entry 2, head

        let out = handle_rice_entry_diff(&inv_diff(None, Some(1)));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert_eq!(out.data.unwrap()["base"], 1, "--entry overrides the default ancestral-mark resolution");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn diff_mark_flag_overrides_the_default_base() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, _rice, _draft) = routed_draft("diff-mark-flag");
        std::fs::write(crate::paths::stage_dir().join("theme.json"), VALID_NOTES).unwrap();
        snapshot("rice.history", "stage").unwrap(); // entry 1
        mark("rice.history.mark", "A", 1).unwrap();
        std::fs::write(
            crate::paths::stage_dir().join("theme.json"),
            r##"{"schemaVersion":"0","palette":{"bg":"#222222"}}"##,
        )
        .unwrap();
        snapshot("rice.history", "stage").unwrap(); // entry 2, head, unmarked

        let out = handle_rice_entry_diff(&inv_diff(Some("A"), None));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert_eq!(out.data.unwrap()["base"], 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// `history::load_marks`' own doc says it never filters a stale entry — that's the caller's
    /// job. This pins that [`resolve_base`] actually does that filtering for `--mark`, rather
    /// than trusting the map and falling through to some default base (or panicking on the
    /// `expect` a few lines later in [`handle_rice_entry_diff`]). Distinct from the existing
    /// `diff_entry_not_found_and_mark_not_found` test below, which only covers a letter that was
    /// NEVER stamped — a different branch than a letter whose entry existed and was then removed.
    #[test]
    fn diff_mark_flag_naming_a_entry_that_no_longer_exists_is_mark_not_found_not_a_silent_default() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, rice, draft) = routed_draft("diff-mark-dangling");
        std::fs::write(crate::paths::stage_dir().join("theme.json"), VALID_NOTES).unwrap();
        snapshot("rice.history", "stage").unwrap(); // entry 1
        std::fs::write(
            crate::paths::stage_dir().join("theme.json"),
            r##"{"schemaVersion":"0","palette":{"bg":"#222222"}}"##,
        )
        .unwrap();
        snapshot("rice.history", "stage").unwrap(); // entry 2, head
        mark("rice.history.mark", "A", 1).unwrap(); // A names entry 1

        // Entry 1's own file is removed out from under the map — `marks.json` still says A -> 1,
        // but entry 1 itself is gone (a hand edit, or pruning without a matching map rewrite).
        std::fs::remove_file(history::history_path(&rice, Some(&draft), 1)).unwrap();

        let out = handle_rice_entry_diff(&inv_diff(Some("A"), None));
        assert_eq!(out.status, Status::Error, "{:?}", out.data);
        assert_eq!(out.data.unwrap()["reason"], "mark-not-found");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Mirrors `back_with_both_entry_and_mark_given_entry_wins` — the same silent priority rule,
    /// pinned for `rice history diff` too, so a future reordering inside `resolve_base` can't flip
    /// it without a test noticing.
    #[test]
    fn diff_with_both_entry_and_mark_given_entry_wins() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, _rice, _draft) = routed_draft("diff-both-flags-entry-wins");
        std::fs::write(crate::paths::stage_dir().join("theme.json"), VALID_NOTES).unwrap();
        snapshot("rice.history", "stage").unwrap(); // entry 1
        std::fs::write(
            crate::paths::stage_dir().join("theme.json"),
            r##"{"schemaVersion":"0","palette":{"bg":"#222222"}}"##,
        )
        .unwrap();
        snapshot("rice.history", "stage").unwrap(); // entry 2, head
        mark("rice.history.mark", "A", 2).unwrap(); // A names entry 2 — the OTHER selection

        let out = handle_rice_entry_diff(&inv_diff(Some("A"), Some(1)));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert_eq!(out.data.unwrap()["base"], 1, "--entry wins over a co-present --mark naming a different entry");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn diff_entry_not_found_and_mark_not_found() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_SESSION_ID"]);
        std::env::remove_var("AOIDE_SESSION_ID");
        let (root, _rice, _draft) = routed_draft("diff-not-found");
        std::fs::write(crate::paths::stage_dir().join("theme.json"), VALID_NOTES).unwrap();
        snapshot("rice.history", "stage").unwrap(); // only entry 1 exists, no marks

        let missing_entry = handle_rice_entry_diff(&inv_diff(None, Some(99)));
        assert_eq!(missing_entry.status, Status::Error);
        assert_eq!(missing_entry.data.unwrap()["reason"], "entry-not-found");

        let missing_mark = handle_rice_entry_diff(&inv_diff(Some("Z"), None));
        assert_eq!(missing_mark.status, Status::Error);
        assert_eq!(missing_mark.data.unwrap()["reason"], "mark-not-found");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn diff_invalid_entry_and_mark_flags_are_rejected_before_anything_else() {
        let bad_entry = handle_rice_entry_diff(&Invocation {
            path: vec!["rice".to_string(), "history".to_string(), "diff".to_string()],
            args: vec![],
            flags: std::collections::BTreeMap::from([("entry".to_string(), "nope".to_string())]),
            door: aoide_protocol::Door::Cli,
        });
        assert_eq!(bad_entry.status, Status::Usage);
        assert_eq!(bad_entry.data.unwrap()["reason"], "invalid-entry");

        let bad_mark = handle_rice_entry_diff(&inv_diff(Some("ab"), None));
        assert_eq!(bad_mark.status, Status::Error);
        assert_eq!(bad_mark.data.unwrap()["reason"], "invalid-mark");
    }

    #[test]
    fn diff_refuses_outside_draft_mode() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let stage = unique_tmp("diff-not-draft").join("stage");
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        let out = handle_rice_entry_diff(&inv_diff(None, None));
        assert_eq!(out.status, Status::Error);
        assert_eq!(out.data.unwrap()["reason"], "not-staged-or-drafted");
        let _ = std::fs::remove_dir_all(&stage);
    }

    // ── `rice history prune` (phase A9, §7.1) ──────────────────────────────

    /// Seed one entry directly (bypassing `snapshot`) so a test can control
    /// `parent`/`at` exactly — the shape every prune test below needs to
    /// build a tree with specific ages and branch points without minting
    /// through the stage. Optionally stamps a mark letter onto it in the
    /// same call.
    fn seed(rice: &str, draft: Option<&str>, entry: u32, parent: Option<u32>, at: &str, mark: Option<char>) -> HistoryRecord {
        let record = HistoryRecord {
            schema_version: String::new(),
            entry,
            parent,
            at: at.to_string(),
            session_id: None,
            cause: "stage".to_string(),
            theme: serde_json::json!({}),
            wallpaper: None,
            widgets: serde_json::json!({}),
        };
        history::save_history(rice, draft, &record).unwrap();
        if let Some(c) = mark {
            let mut marks = history::load_marks(rice, draft);
            marks.insert(c.to_string(), entry);
            history::save_marks(rice, draft, &marks).unwrap();
        }
        record
    }

    fn inv_prune(older_than: Option<&str>, keep: Option<u32>, all_but_marks: bool, force: bool) -> Invocation {
        let mut flags = std::collections::BTreeMap::new();
        if let Some(v) = older_than {
            flags.insert("older-than".to_string(), v.to_string());
        }
        if let Some(k) = keep {
            flags.insert("keep".to_string(), k.to_string());
        }
        if all_but_marks {
            flags.insert("all-but-marks".to_string(), "true".to_string());
        }
        if force {
            flags.insert("force".to_string(), "true".to_string());
        }
        Invocation {
            path: vec!["rice".to_string(), "history".to_string(), "prune".to_string()],
            args: vec![],
            flags,
            door: aoide_protocol::Door::Cli,
        }
    }

    const OLD_AT: &str = "2020-01-01T00:00:00Z";

    // ── parse_older_than: pure, no fs ────────────────────────────────────

    #[test]
    fn parse_older_than_accepts_days_and_hours() {
        assert_eq!(parse_older_than("7d"), Some(7 * 86400));
        assert_eq!(parse_older_than("12h"), Some(12 * 3600));
        assert_eq!(parse_older_than("1d"), Some(86400));
    }

    #[test]
    fn parse_older_than_rejects_malformed_values() {
        assert_eq!(parse_older_than(""), None, "empty");
        assert_eq!(parse_older_than("d"), None, "no digits");
        assert_eq!(parse_older_than("7"), None, "no unit");
        assert_eq!(parse_older_than("0d"), None, "a zero count");
        assert_eq!(parse_older_than("7x"), None, "an unknown unit");
        assert_eq!(parse_older_than("-7d"), None, "a negative count");
    }

    // ── plan_prune: the ancestry rail holds at the planning layer itself ──

    #[test]
    fn plan_prune_never_lets_the_head_or_its_ancestry_into_candidates_even_with_force() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let (root, rice, draft) = routed_draft("prune-plan-ancestry-rail");
        seed(&rice, Some(&draft), 1, None, OLD_AT, None);
        seed(&rice, Some(&draft), 2, Some(1), OLD_AT, None);
        seed(&rice, Some(&draft), 3, Some(2), OLD_AT, None);
        seed(&rice, Some(&draft), 4, Some(1), OLD_AT, None); // a branch off root 1, not on head 3's line.
        history::save_head(&rice, Some(&draft), 3).unwrap();

        let plan = plan_prune(&rice, Some(&draft), None, None, true); // force=true changes nothing about the rail.
        assert_eq!(plan.protected_ancestry, BTreeSet::from([1, 2, 3]));
        let candidate_numbers: Vec<u32> = plan.candidates.iter().map(|t| t.entry).collect();
        assert_eq!(candidate_numbers, vec![4], "only the off-line branch is ever a candidate");
        let _ = std::fs::remove_dir_all(&root);
    }

    // ── the handler: dual entrance ───────────────────────────────────────

    #[test]
    fn prune_bare_non_tty_prints_the_dry_run_and_changes_nothing() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let (root, rice, draft) = routed_draft("prune-dry-run");
        seed(&rice, Some(&draft), 1, None, OLD_AT, None);
        seed(&rice, Some(&draft), 2, Some(1), OLD_AT, None);
        history::save_head(&rice, Some(&draft), 1).unwrap();

        let before_count = history::list_history(&rice, Some(&draft)).len();
        let before_marks = history::load_marks(&rice, Some(&draft));

        // No selector flag, and cargo test's stdin/stdout are never a real
        // tty, so `pick::interactive` reads false here exactly like
        // `rice back`'s own bare-non-tty test relies on.
        let out = handle_rice_entry_prune(&inv_prune(None, None, false, false));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let data = out.data.unwrap();
        assert_eq!(data["dryRun"], true);
        assert_eq!(data["candidates"], json!([2]));
        assert!(out.changed.is_empty(), "a dry run changes nothing");
        assert_eq!(history::list_history(&rice, Some(&draft)).len(), before_count, "no entry removed");
        assert_eq!(history::load_marks(&rice, Some(&draft)), before_marks, "marks.json untouched");
        assert!(history::load_history(&rice, Some(&draft), 2).is_some(), "entry 2 still on disk");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn prune_never_selects_the_head_or_its_ancestry_under_force_and_all_but_marks() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let (root, rice, draft) = routed_draft("prune-force-ancestry-rail");
        seed(&rice, Some(&draft), 1, None, OLD_AT, None);
        seed(&rice, Some(&draft), 2, Some(1), OLD_AT, None);
        seed(&rice, Some(&draft), 3, Some(2), OLD_AT, None); // head's line: 1 <- 2 <- 3
        seed(&rice, Some(&draft), 4, Some(1), OLD_AT, None); // off-line branch, off root
        seed(&rice, Some(&draft), 5, Some(2), OLD_AT, None); // off-line branch, off 2
        history::save_head(&rice, Some(&draft), 3).unwrap();

        let out = handle_rice_entry_prune(&inv_prune(None, None, true, true)); // --all-but-marks --force
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert_eq!(out.data.unwrap()["pruned"], json!([4, 5]));
        assert_eq!(history::load_head(&rice, Some(&draft)), Some(3), "head untouched");
        for n in [1u32, 2, 3] {
            assert!(history::load_history(&rice, Some(&draft), n).is_some(), "entry {n} on the head's ancestry survives --force");
        }
        assert!(history::load_history(&rice, Some(&draft), 4).is_none());
        assert!(history::load_history(&rice, Some(&draft), 5).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn prune_keep_n_keeps_the_newest_n_eligible_entries() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let (root, rice, draft) = routed_draft("prune-keep-n");
        seed(&rice, Some(&draft), 1, None, OLD_AT, None);
        for n in 2u32..=6 {
            seed(&rice, Some(&draft), n, Some(1), OLD_AT, None);
        }
        history::save_head(&rice, Some(&draft), 1).unwrap();

        let out = handle_rice_entry_prune(&inv_prune(None, Some(2), false, false));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert_eq!(out.data.unwrap()["pruned"], json!([2, 3, 4]), "the newest 2 (5, 6) are kept");
        assert!(history::load_history(&rice, Some(&draft), 5).is_some());
        assert!(history::load_history(&rice, Some(&draft), 6).is_some());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn prune_keep_n_exceeding_the_entry_count_prunes_nothing() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let (root, rice, draft) = routed_draft("prune-keep-exceeds");
        seed(&rice, Some(&draft), 1, None, OLD_AT, None);
        seed(&rice, Some(&draft), 2, Some(1), OLD_AT, None);
        history::save_head(&rice, Some(&draft), 1).unwrap();

        let out = handle_rice_entry_prune(&inv_prune(None, Some(100), false, false));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert_eq!(out.data.unwrap()["pruned"], json!([]));
        assert!(out.changed.is_empty());
        assert!(history::load_history(&rice, Some(&draft), 2).is_some());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn prune_older_than_selects_only_entries_past_the_threshold() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let (root, rice, draft) = routed_draft("prune-older-than");
        seed(&rice, Some(&draft), 1, None, OLD_AT, None);
        seed(&rice, Some(&draft), 2, Some(1), OLD_AT, None); // 2020 — well past any sane threshold.
        seed(&rice, Some(&draft), 3, Some(1), &now_iso_utc(), None); // just minted — never "old enough".
        history::save_head(&rice, Some(&draft), 1).unwrap();

        let out = handle_rice_entry_prune(&inv_prune(Some("1d"), None, false, false));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert_eq!(out.data.unwrap()["pruned"], json!([2]));
        assert!(history::load_history(&rice, Some(&draft), 3).is_some(), "too recent to be a candidate");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn prune_older_than_malformed_value_is_a_usage_error_that_changes_nothing() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let (root, rice, draft) = routed_draft("prune-older-than-bad");
        seed(&rice, Some(&draft), 1, None, OLD_AT, None);
        seed(&rice, Some(&draft), 2, Some(1), OLD_AT, None);
        history::save_head(&rice, Some(&draft), 1).unwrap();
        let before = history::list_history(&rice, Some(&draft)).len();

        let out = handle_rice_entry_prune(&inv_prune(Some("nonsense"), None, false, false));
        assert_eq!(out.status, Status::Usage);
        assert_eq!(out.data.unwrap()["reason"], "invalid-older-than");
        assert_eq!(history::list_history(&rice, Some(&draft)).len(), before, "nothing was pruned");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn prune_keep_malformed_value_is_a_usage_error() {
        let out = handle_rice_entry_prune(&Invocation {
            path: vec!["rice".to_string(), "history".to_string(), "prune".to_string()],
            args: vec![],
            flags: std::collections::BTreeMap::from([("keep".to_string(), "nope".to_string())]),
            door: aoide_protocol::Door::Cli,
        });
        assert_eq!(out.status, Status::Usage);
        assert_eq!(out.data.unwrap()["reason"], "invalid-keep");
    }

    #[test]
    fn prune_marks_survive_by_default_and_go_under_force_dropping_the_letter_from_the_map() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let (root, rice, draft) = routed_draft("prune-marks-protection");
        seed(&rice, Some(&draft), 1, None, OLD_AT, None);
        seed(&rice, Some(&draft), 2, Some(1), OLD_AT, Some('A'));
        seed(&rice, Some(&draft), 3, Some(1), OLD_AT, None);
        history::save_head(&rice, Some(&draft), 1).unwrap();

        // Without --force, the marked entry (2) is protected — only 3 goes.
        let out = handle_rice_entry_prune(&inv_prune(None, None, true, false));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert_eq!(out.data.unwrap()["pruned"], json!([3]));
        assert!(history::load_history(&rice, Some(&draft), 2).is_some(), "marked entry survives by default");
        assert_eq!(history::load_marks(&rice, Some(&draft)).get("A"), Some(&2));

        // With --force, the marked entry is now a candidate too, and its
        // letter leaves the map in the SAME pass.
        let out = handle_rice_entry_prune(&inv_prune(None, None, true, true));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert_eq!(out.data.unwrap()["pruned"], json!([2]));
        assert!(history::load_history(&rice, Some(&draft), 2).is_none());
        assert!(
            history::load_marks(&rice, Some(&draft)).get("A").is_none(),
            "the mark's letter no longer names a deleted entry"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    // ── the prune core: splice, cascade, orphans ─────────────────────────

    #[test]
    fn prune_reparents_surviving_children_and_ancestry_still_resolves() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let (root, rice, draft) = routed_draft("prune-splice");
        seed(&rice, Some(&draft), 1, None, OLD_AT, None);
        seed(&rice, Some(&draft), 2, Some(1), OLD_AT, None);
        seed(&rice, Some(&draft), 3, Some(2), OLD_AT, None);
        seed(&rice, Some(&draft), 4, Some(2), OLD_AT, None);
        history::save_head(&rice, Some(&draft), 1).unwrap();

        let result = prune(&rice, Some(&draft), &[2]).unwrap();
        assert_eq!(result.pruned, vec![2]);
        let mut reparented = result.reparented.clone();
        reparented.sort_by_key(|(t, _, _)| *t);
        assert_eq!(reparented, vec![(3, Some(2), Some(1)), (4, Some(2), Some(1))]);
        assert!(history::load_history(&rice, Some(&draft), 2).is_none());
        assert_eq!(history::load_history(&rice, Some(&draft), 3).unwrap().parent, Some(1));
        assert_eq!(history::load_history(&rice, Some(&draft), 4).unwrap().parent, Some(1));
        assert_eq!(history::ancestry(&history::list_history(&rice, Some(&draft)), 3), vec![3, 1]);
        assert_eq!(history::ancestry(&history::list_history(&rice, Some(&draft)), 4), vec![4, 1]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A splice target that is ITSELF pruned in the same pass — grandchildren
    /// must land on the nearest SURVIVING ancestor, and this must hold
    /// regardless of which order the doomed entries are folded in.
    #[test]
    fn prune_cascades_through_a_splice_target_that_is_also_pruned_in_the_same_pass() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);

        for (tag, order) in [("prune-cascade-fwd", [9u32, 17]), ("prune-cascade-rev", [17, 9])] {
            let (root, rice, draft) = routed_draft(tag);
            seed(&rice, Some(&draft), 1, None, OLD_AT, None);
            seed(&rice, Some(&draft), 9, Some(1), OLD_AT, None);
            seed(&rice, Some(&draft), 17, Some(9), OLD_AT, None);
            seed(&rice, Some(&draft), 25, Some(17), OLD_AT, None);
            history::save_head(&rice, Some(&draft), 1).unwrap();

            let result = prune(&rice, Some(&draft), &order).unwrap();
            let mut pruned = result.pruned.clone();
            pruned.sort_unstable();
            assert_eq!(pruned, vec![9, 17], "order {order:?}");
            assert_eq!(
                history::load_history(&rice, Some(&draft), 25).unwrap().parent,
                Some(1),
                "grandchild lands on the nearest surviving ancestor regardless of fold order {order:?}"
            );
            assert_eq!(history::ancestry(&history::list_history(&rice, Some(&draft)), 25), vec![25, 1]);
            let _ = std::fs::remove_dir_all(&root);
        }
    }

    #[test]
    fn prune_an_orphan_entry_is_prunable_and_does_not_corrupt_the_store() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let (root, rice, draft) = routed_draft("prune-orphan");
        seed(&rice, Some(&draft), 1, None, OLD_AT, None);
        seed(&rice, Some(&draft), 2, Some(999), OLD_AT, None); // dangling parent — A6 established this is reachable.
        history::save_head(&rice, Some(&draft), 1).unwrap();

        let result = prune(&rice, Some(&draft), &[2]).unwrap();
        assert_eq!(result.pruned, vec![2]);
        assert!(result.reparented.is_empty(), "the orphan has no children to splice");
        assert!(history::load_history(&rice, Some(&draft), 2).is_none());
        let remaining = history::list_history(&rice, Some(&draft));
        assert_eq!(remaining.iter().map(|t| t.entry).collect::<Vec<_>>(), vec![1]);
        assert_eq!(history::ancestry(&remaining, 1), vec![1], "the surviving root is untouched");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The rail's real defense-in-depth: `doomed` here plays the role of a
    /// PLAN computed unlocked, possibly well before this actually runs (the
    /// tty picker blocks on human input in between with no lock held — see
    /// `prune_unlocked`'s own doc). Simulate the race directly: hand `prune`
    /// a `doomed` list naming the CURRENT head, exactly as if another door
    /// had moved the head there after a plan was computed but before the
    /// human confirmed a selection that included it. The fresh re-check
    /// inside `prune_unlocked` must strip it rather than trust the list.
    #[test]
    fn prune_re_checks_the_ancestry_rail_against_a_fresh_read_and_skips_a_stale_candidate() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let (root, rice, draft) = routed_draft("prune-stale-plan-race");
        seed(&rice, Some(&draft), 1, None, OLD_AT, None);
        seed(&rice, Some(&draft), 2, Some(1), OLD_AT, None); // now the head — as if `rice back --entry 2` raced the plan.
        seed(&rice, Some(&draft), 3, Some(1), OLD_AT, None); // a genuine, still-off-ancestry candidate.
        history::save_head(&rice, Some(&draft), 2).unwrap();

        // A stale plan that offered both 2 and 3 as candidates before the
        // head moved to 2 — `prune_unlocked` must not trust it verbatim.
        let result = prune(&rice, Some(&draft), &[2, 3]).unwrap();
        assert_eq!(result.pruned, vec![3], "the now-protected candidate never reaches the doomed set");
        assert_eq!(result.skipped_now_protected, vec![2]);
        assert!(history::load_history(&rice, Some(&draft), 2).is_some(), "the entry that raced into being the head survives");
        assert_eq!(history::load_history(&rice, Some(&draft), 2).unwrap().parent, Some(1), "and is untouched, not just un-deleted");
        assert!(history::load_history(&rice, Some(&draft), 3).is_none());
        assert_eq!(history::load_head(&rice, Some(&draft)), Some(2), "the head itself is exactly where it raced to");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Same race, but the stale candidate is on the LIVE HEAD'S ANCESTRY
    /// rather than being the head itself — the rail re-check walks the
    /// whole ancestry fresh, not just a bare `head == n` comparison.
    #[test]
    fn prune_re_checks_the_whole_fresh_ancestry_not_just_the_bare_head() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let (root, rice, draft) = routed_draft("prune-stale-plan-ancestry-race");
        seed(&rice, Some(&draft), 1, None, OLD_AT, None);
        seed(&rice, Some(&draft), 2, Some(1), OLD_AT, None); // about to become the head's PARENT.
        seed(&rice, Some(&draft), 3, Some(2), OLD_AT, None); // the new head.
        history::save_head(&rice, Some(&draft), 3).unwrap();

        let result = prune(&rice, Some(&draft), &[2]).unwrap();
        assert_eq!(result.pruned, Vec::<u32>::new());
        assert_eq!(result.skipped_now_protected, vec![2]);
        assert!(history::load_history(&rice, Some(&draft), 2).is_some());
        assert_eq!(history::load_history(&rice, Some(&draft), 3).unwrap().parent, Some(2), "no splice ran — 2 was never touched");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// `changed` must list only paths this call actually wrote or deleted —
    /// a candidate the fresh rail re-check strips must not show up as
    /// "changed" when nothing about it changed at all.
    #[test]
    fn prune_changed_never_lists_a_path_the_rail_re_check_stripped() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let (root, rice, draft) = routed_draft("prune-changed-accuracy-stale");
        seed(&rice, Some(&draft), 1, None, OLD_AT, None);
        seed(&rice, Some(&draft), 2, Some(1), OLD_AT, None); // races into being the head.
        seed(&rice, Some(&draft), 3, Some(1), OLD_AT, None); // genuinely pruned.
        history::save_head(&rice, Some(&draft), 2).unwrap();

        let result = prune(&rice, Some(&draft), &[2, 3]).unwrap();
        let entry2_path = history::history_path(&rice, Some(&draft), 2).to_string_lossy().into_owned();
        assert!(!result.changed.contains(&entry2_path), "entry 2 was skipped, not touched — must not be reported as changed");
        assert_eq!(result.changed, vec![history::history_path(&rice, Some(&draft), 3).to_string_lossy().into_owned()]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The other way `changed` could over-report: a doomed number whose file
    /// was ALREADY gone by the time this ran (a second racing prune, or a
    /// hand removal) — `path.is_file()` is false, nothing is removed, and
    /// that must not be reported as a change either.
    #[test]
    fn prune_changed_never_lists_a_doomed_entry_whose_file_was_already_gone() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let (root, rice, draft) = routed_draft("prune-changed-accuracy-already-gone");
        seed(&rice, Some(&draft), 1, None, OLD_AT, None);
        seed(&rice, Some(&draft), 2, Some(1), OLD_AT, None);
        history::save_head(&rice, Some(&draft), 1).unwrap();
        std::fs::remove_file(history::history_path(&rice, Some(&draft), 2)).unwrap(); // simulate a concurrent removal.

        let result = prune(&rice, Some(&draft), &[2]).unwrap();
        assert!(result.changed.is_empty(), "nothing was actually removed by THIS call");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn prune_refuses_outside_draft_mode() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let stage = unique_tmp("prune-not-draft").join("stage");
        crate::commands::test_support::ensure_default_rices_fixture();
        aoide_test_support::pin_stage(&stage);

        let out = handle_rice_entry_prune(&inv_prune(None, None, true, false));
        assert_eq!(out.status, Status::Error);
        assert_eq!(out.data.unwrap()["reason"], "not-staged-or-drafted");
        let _ = std::fs::remove_dir_all(&stage);
    }
}
