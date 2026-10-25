//! `resurrect --project <x>` — revive a project's most recently-ended
//! resumable session off the durable ledger (P-D8, `docs/architecture/
//! AOIDED.md`'s "L5 — harness summoning" section). The same command core
//! also backs the daemon's own boot-time auto-resume trigger
//! (`aoide-server`'s `daemon.rs`, called in-process the same way
//! `run_internal_reap` calls `crate::reap::reap_and_announce`).
//!
//! Selection: resolve `--project <x>` against `projects.json` by exact name,
//! read every ledger line whose `cwd` anchors to it (the SAME longest-
//! path-prefix rule bare `graph`'s `anchor_for` uses — reused, never
//! re-derived), then pick candidates. `--all` widens to every anchored
//! entry; `--id` narrows to one specific ledger `sessionId`; bare (neither
//! flag) resumes the project's WHOLE undying set (`state/undying.json`,
//! durable-sessions plan P-C4) — every anchored entry currently marked
//! durable, minus any id already alive in `sessions.json`, deduped by
//! `sessionId` keeping the newest `endedAt` (an append-only ledger can hold
//! more than one exit for the same undying id once it has been resurrected
//! and exited again). `--all` and `--id` are unchanged escapes: both widen
//! or narrow past the undying set regardless of the mark. Each candidate is
//! filtered through its harness's `AgentProfile.resume_args`
//! (`aoide_protocol::agents`). `ById` resumes by the harness's own session
//! id. `ByLog` (eidolon) resumes by the journal its ledger line's `logPath`
//! names, and skips a line whose `logPath` names no journal ([`journal_of`]).
//! `None` (an unregistered agent, or a harness whose resume argv has never
//! been verified) skips that candidate with a taught message naming the
//! harness, never a guessed invocation.
//!
//! A resurrected session is ALWAYS a fresh `sessionId` — ids are never
//! recycled — spawned via the windowed path ([`super::spawn::session_spawn`]
//! with `--windowed --cwd <the ledger entry's own cwd>`, P-D7/P-D8) so it
//! reopens in a real terminal, in its original project directory. On success
//! the new record is stamped `resumedFrom` (`stamp_resumed_from`), naming the
//! ledger entry's own `sessionId` — `build_graph` projects that as a
//! `resumed` edge beside `spawned`/`anchors` (CONTRACTS.md §4). If the old id
//! was undying (`state/undying.json`, durable-sessions plan P-C3), the mark
//! transfers onto the new id in the same step — never left on the now-dead
//! old id, which would double-resurrect on the next sweep. A by-journal
//! revival (`ByLog`) is the exception: its mark stays on the ledger id, and
//! the new id gets none. The journal is the session and the terminal is only
//! its window, so the next sweep must reopen that same journal, which the
//! ledger id names.
//!
//! Never a hard `Outcome::error` over a per-candidate spawn failure (a
//! headless host has no `$AOIDE_TERMINAL`/display — `session_spawn`'s own
//! taught error): every candidate's outcome is folded into
//! `resurrected`/`skipped`/`failed` and the command itself stays `Ok`, so a
//! `--all` batch keeps going past one bad candidate and the daemon's
//! boot-time trigger degrades gracefully (log the skip, never crash the
//! tick) instead of treating a headless box as a command failure. Only
//! genuine USAGE problems (`--project` missing, an unknown project name, an
//! `--id` that names no anchored ledger entry) are `Outcome::usage`/`error`.
//!
//! **Candidate resolution, two arms (P-C6, durable-sessions plan):**
//! [`resolve_candidate`] tries the harness arm FIRST —
//! `AgentProfile.resume_args` off `aoide_protocol::agents` — and only when
//! that yields nothing does it try the TERMINAL arm: a ledger entry carrying
//! a `restore` snapshot (P-C5) is a conducted shell, not a harness, so it
//! reopens as `[<login shell>, "-l"]` — the exact argv `kitty.nix`'s own
//! wrapper execs — rather than a `--resume <id>` nobody could ever verify. A
//! candidate with neither hits the pre-existing taught skip. A `ByLog`
//! harness never reaches the terminal arm: a line whose journal cannot be
//! resumed is skipped with its own reason, `restore` or not.
//!
//! **One engine per session.** A candidate is never launched while its
//! session is live, or while a launch of it is in flight. [`claim_resume`]
//! runs the block ([`block_verdict`]) under the stage lock and, when the
//! candidate is free, writes its claim (`state/resuming.json`, see
//! `aoide_storage::resuming`) before the spawn. A claim is in flight while
//! its new id has a session that is not `done`, or while it has none and is
//! younger than [`RESUME_CLAIM_BOUND_SECS`]. A launch that fails drops its
//! own claim. A dry run runs the same block on the same reads and writes
//! nothing.
//!
//! **Post-spawn delivery into a resurrected terminal (decision 8):** once a
//! terminal candidate's spawn actually registers — and its harness is READY to
//! take a turn ([`super::spawn::wait_ready`], the same gate `spawn --prompt`
//! passes; a target that never becomes ready is reported `not-ready` and typed
//! at by nothing) — its `restore` snapshot decides what — if anything — lands
//! in the new pty, through [`send_restore`] (the gated send), never a direct
//! socket write. A restore is not a send a caller makes: it rests on the door's
//! `--yes` alone, so an MCP door parks it even when its server is attested, and
//! the revived session's spawn is the harness's own resume line, launched
//! without the MCP spawn gate:
//! - **not idle, with a foreground `argv`** — the session was demonstrably
//!   RUNNING something when it left. Its command is TYPED at the prompt, never
//!   submitted: the human who reads the line presses Enter. Nothing runs at
//!   boot on its own. EXCEPT when [`restore_argv_refusal`] refuses the argv: the
//!   refusals, and why each one exists, are listed once in CONTRACTS.md's
//!   restore paragraph. The cwd still restores; nothing is delivered.
//! - **idle** — nothing is delivered. A terminal reopened at its own cwd is
//!   already the correct, complete answer. Nothing typed in the old session
//!   is carried into the new prompt, so nothing can fire itself at boot.
//!
//! **Bare-manifest mode (U2, command-defrag lane U).** `resurrect` with
//! NONE of `--project`/`--all`/`--id` given walks UP from cwd
//! (`aoide_storage::manifest::walk_up`) for the nearest `.aoide/
//! project.json` and, if found, revives THAT manifest's specs directly —
//! [`resurrect_from_manifest`] — instead of the flag-mode selection above.
//! The manifest is SELF-SUFFICIENT: no `projects.json` registration is
//! read or required. Not found — genuinely bare, no flag given either —
//! the command falls through to the ordinary `--project`-required check,
//! whose usage error then names both misses (no manifest above cwd, no
//! flag given). `--project`/`--all`/`--id` are UNCHANGED escapes that
//! ignore the manifest entirely — mutually exclusive with bare-manifest
//! mode by construction, since any one of them present routes straight to
//! the pre-existing flag-mode path AND skips the manifest walk altogether,
//! so a flag-mode invocation missing `--project` (`--id X` alone, say)
//! gets `require_flag`'s own ORIGINAL, accurate usage error — never the
//! both-misses wording, which would lie twice over (a flag WAS given; no
//! walk was ever attempted). A review fix (U2 round 1) closed exactly this
//! bug: the both-misses message used to fire unconditionally on any
//! `require_flag` failure.
//!
//! Each manifest spec (`{host, dir, agent, command?}`,
//! `aoide_storage::manifest::SessionSpec`) resolves independently, same
//! per-candidate isolation the flag-mode loop already holds: a spec whose
//! `host` is not this host's own name
//! (`aoide_storage::display::local_host_name`) is [`summon_remote`]'s job
//! (U4, command-defrag lane U) — summoned through the node door rather than
//! skipped, see that function's own doc for the local refusal shapes, the
//! reused signed-spawn wire, and the cwd limitation this phase lands with.
//! A local spec's `dir` resolves through `aoide_storage::manifest::
//! resolve_spec_dir` (the containment guard — a `..`-laden `dir` is
//! refused, never silently resolved outside the project root).
//!
//! **The enrichment rule (the User's design decision, U2): the manifest
//! decides WHAT exists; the ledger decides HOW.** The newest entry in THIS
//! HOST's own `state/session-ledger.jsonl` whose `cwd`/`agent` match the
//! resolved `dir`/the spec's `agent` (host is implicit — the ledger is
//! host-local state, never synced, and only a same-host spec reaches this
//! match at all) is revived through the exact SAME [`resolve_candidate`]/
//! [`resurrect_one`] path `--id` drives — its harness resume args or
//! terminal restore snapshot, exactly as if the operator had named that
//! ledger entry directly. No match — the ordinary case for a spec this
//! host has never actually run, e.g. straight off a fresh checkout — falls
//! to a CLEAN windowed spawn instead ([`clean_spawn_from_spec`]): the
//! spec's own `command` when given, else the agent's registered
//! `AgentProfile::launch` default (`aoide_protocol::agents::agent_profile`)
//! — the SAME windowed [`session_spawn`] path every other resurrect
//! candidate spawns through, never a forked launch mechanism. An agent
//! with neither a `command` nor a registered profile is a taught
//! `failed[]` entry, never a guessed argv. A spec whose `host` names a
//! DIFFERENT box is [`summon_remote`]'s job (U4) rather than this loop's
//! own local match — see that function's doc. Every row of the outcome —
//! both buckets [`resurrect_one`] can push into as well as this loop's own
//! `summoned-remote`/`failed`/`clean-spawned` rows — carries a
//! `disposition` key, stamped after the fact where `resurrect_one` itself
//! doesn't know it is being called from manifest mode.
//!
//! **Manifest-revived sessions are marked undying (orchestrator design
//! ruling, U2 review round 1) — LOCAL revivals only.** [`summon_remote`]'s
//! own rows never reach this: the resurrected id lives on the NODE, and
//! `state/undying.json` is host-local runtime state naming ids that live on
//! THIS host (the same reasoning U3's picker already holds toward a node
//! row's mark — it writes a manifest spec, never touches `undying.json`
//! for an id it doesn't own). Once a spawn from EITHER local path actually
//! lands a row in `resurrected` (which only happens past `Status::Ok`, the
//! same gate `resurrect_one`'s own pre-existing undying TRANSFER block
//! reads off, never `registered` — a live terminal registering is a fact
//! this crate's own tests never exercise end-to-end, `spawn.rs`'s own
//! module doc draws that exact line), [`mark_manifest_revival_undying`]
//! marks that new id undying directly: one `load_undying`/`set_undying`/
//! `save_undying`, right here in [`resurrect_from_manifest`] — conceptually
//! the same idea `aoide spawn --undying` marks a fresh spawn with, but its
//! own separate call, not a flag threaded into the shared `spawn`
//! invocation. The manifest spec IS the durable declaration of what should
//! exist, so marking its revived session undying means a LATER bare
//! `resurrect --project <name>` (or the daemon's boot sweep) finds it in
//! the undying set without re-walking or re-consulting the manifest —
//! flag-mode and manifest-mode revival converge on ONE durable set instead
//! of tracking two independent notions of "what this project wants
//! running." This is deliberately unconditional, unlike flag-mode's own
//! undying TRANSFER a few paragraphs up (which only ever marks a new id
//! when the OLD ledger id it replaces was already undying): there is no
//! ordinary-revive case to protect here, every manifest-mode spawn already
//! came from an explicit, operator-authored declaration.
//!
//! **Dry run (`--dry-run`).** The flag takes the selection the launch would
//! take and answers with its rows instead of launching them: flag mode through
//! [`dry_run_listing`], bare mode through [`dry_run_manifest`]. Each row's
//! verdict comes from the helper its launch uses (`enrichment`, `clean_argv`,
//! `summon_check`, `resolve_candidate`), so the listing and the launch agree.
//! From an unattested caller on a door that cannot approve, a dry run gets what
//! the call would give that caller, minus the launch: in bare mode the bare
//! call's refusal; with `--id` or `--all` and no `--project`, the usage error
//! the call gives; with `--project`, the `dry-run-not-held` refusal, since that
//! call would hold and show the caller nothing. Nothing is held, spawned or
//! sent on any door. The handler writes three things and no others: the audit
//! line (status `dry-run`, at the points the real call audits); the one-time
//! `carry.json` rename an undying read performs
//! (`aoide_storage::undying::load_undying`), on a host that still holds the
//! legacy file; and the one-time conducting-stage migration a stage read
//! performs (`aoide_storage::fs::conducting_stage_dir`), on a host that still
//! holds the legacy stage files. Two writes come with any call, dry or real,
//! outside the handler: the door's audit line, and the start-up migration both
//! core binaries run (`aoide_storage::fs::migrate_root_once`). It locks
//! `state/` owner-only when it exists and renames old peer state files. When
//! the legacy `~/Aoide` root is not `$AOIDE_ROOT`, it moves that root's state
//! unless `AOIDE_STATE_DIR` is set, and its log unless `AOIDE_AUDIT_LOG` is set
//! and not empty.

use super::common::{is_unsafe, require_flag, stage_error};
use super::conduct::{program_is_a_shell, program_is_an_interactive_shell, runs_a_privilege_program};
use super::model::{load_stage, projects_path, sessions_path, ProjectsFile, SessionsFile};
use super::send::{deliver_local_with, real_attested_sender, PendingSpawn};
use super::session_store::{stamp_origin, stamp_resumed_from};
use super::spawn::{hold_whole, launch_session, shell_join, Ready};
use aoide_protocol::agents::{agent_profile, ResumeArgs};
use aoide_protocol::output::{Fix, Kind, Outcome, Status};
use aoide_protocol::{Door, Invocation};
use aoide_storage::attest::is_node_origin;
use aoide_storage::records::RestoreSnapshot;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;

/// Mint a fresh session id for a resurrected session — never the ledger
/// entry's own id (ids are never recycled, `docs/architecture/AOIDED.md`'s
/// invariant list, item 5). Same `<command>-<pid>-<unixts>` shape `spawn`
/// mints with (`spawn.rs::unix_ts`, reused rather than re-derived).
fn mint_resurrected_id() -> String {
    format!("resurrect-{}-{}", std::process::id(), super::conduct::unix_ts())
}

/// One selected, resumable ledger candidate, resolved down to what
/// [`resurrect_one`] needs — the harness profile lookup, `harnessSessionId`
/// fallback and journal check already done, so the spawn/skip decision below
/// never re-derives them.
struct Candidate {
    entry: aoide_storage::ledger::LedgerEntry,
    /// The argv the revival runs, or the taught line that skips it.
    resume_argv: Result<Vec<String>, String>,
    /// A by-journal revival: the undying mark stays on the ledger id.
    by_log: bool,
}

fn resolve_candidate(entry: aoide_storage::ledger::LedgerEntry) -> Candidate {
    match agent_profile(&entry.agent).and_then(|p| p.resume_args) {
        Some(ResumeArgs::ById(f)) => {
            let harness_id = entry.harness_session_id.as_deref().unwrap_or(&entry.session_id);
            let resume_argv = Ok(f(harness_id));
            Candidate { entry, resume_argv, by_log: false }
        }
        // No terminal arm after a journal: a journal line that cannot name a
        // journal is skipped, never reopened as a shell.
        Some(ResumeArgs::ByLog(f)) => {
            let resume_argv = journal_of(&entry).map(|log| f(&log));
            Candidate { entry, resume_argv, by_log: true }
        }
        // Harness arm found nothing — try the TERMINAL arm (P-C6, durable-
        // sessions plan). `agent == "shell"` is never a harness — it has no
        // session id to `--resume` and no profile row (`agents.rs`'s own module
        // doc refuses a guessed `SHELL_PROFILE` for exactly this reason) — what
        // it carries instead is a `restore` snapshot (P-C5). `Some(restore)` is
        // the whole test for "is this even a terminal candidate"; a `restore`
        // -less entry (predating P-C5, or a harness this box has never
        // verified) takes the pre-existing taught skip.
        None => {
            let resume_argv = if entry.restore.is_some() {
                Ok(vec![login_shell(), "-l".to_string()])
            } else {
                Err(unresumed_reason(&entry.agent))
            };
            Candidate { entry, resume_argv, by_log: false }
        }
    }
}

/// The journal a by-journal revival reopens: its ledger line's `logPath`, when
/// that is an absolute, plain path to a regular file outside the session logs
/// dir. Anything else is the candidate's skip reason. The path itself stays out
/// of the reason: it is ledger data.
fn journal_of(entry: &aoide_storage::ledger::LedgerEntry) -> Result<String, String> {
    let who = format!("session {} ({})", entry.session_id, entry.agent);
    let skip = |why: &str| Err(format!("{who} {why} — skipped rather than guessed"));
    let Some(log) = entry.log_path.as_deref() else {
        return skip("has no logPath, so its journal is unknown");
    };
    if !Path::new(log).is_absolute() || log.chars().any(is_unsafe) {
        return skip("has a logPath that is not an absolute, plain path");
    }
    if aoide_storage::fs::inside_session_logs(log) {
        return skip("has a logPath inside the session logs dir, which holds terminal transcripts, not journals");
    }
    if std::fs::symlink_metadata(log).is_ok_and(|m| m.file_type().is_symlink()) {
        return skip("has a logPath that is a symbolic link");
    }
    if !Path::new(log).is_file() {
        return skip("has a logPath that names no regular file");
    }
    Ok(log.to_string())
}

/// How long a claim is in flight with no record to show for its new id. A
/// spawn's registration budget is 3 s (`spawn::REGISTRATION_BUDGET`), and a
/// terminal can take longer than that to register its session, so a claim
/// younger than this with nothing registered is still a launch in progress.
const RESUME_CLAIM_BOUND_SECS: i64 = 60;

/// Why the block refuses a candidate: the live session that holds it, or the
/// launch that claims it and how old that claim is. `by_journal` is set when
/// the claim is another session's, on the candidate's journal.
#[derive(Debug, PartialEq, Eq)]
enum Blocked {
    Live { live_id: String },
    InFlight { new_id: String, age_secs: i64, by_journal: bool },
}

/// A candidate the claim step or a dry run refuses: the taught line for its
/// row, and the id that line names, as the row key (`liveSessionId` or
/// `claimedBy`).
#[derive(Debug, PartialEq, Eq)]
struct Refusal {
    reason: String,
    named: Option<(&'static str, String)>,
}

/// A journal path as the block compares it: spelled as the ledger, presence or
/// roster names it, and resolved once to the file it names, when it resolves.
/// The block compares these and never reads the filesystem; [`Journal::of`] is
/// the one place a path is resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Journal {
    spelled: String,
    resolved: Option<String>,
}

impl Journal {
    fn of(path: &str) -> Journal {
        Journal {
            spelled: path.to_string(),
            resolved: std::fs::canonicalize(path).ok().map(|p| p.to_string_lossy().into_owned()),
        }
    }

    /// Whether two journals name one file: equal as spelled, or both resolved to
    /// the same file, so a symlink to a journal is that journal.
    fn names_the_file_of(&self, other: &Journal) -> bool {
        self.spelled == other.spelled
            || matches!((&self.resolved, &other.resolved), (Some(a), Some(b)) if a == b)
    }

    /// Whether a claim's stored journal names this journal: as spelled, or as
    /// the file it resolves to.
    fn is_named_by(&self, stored: &str) -> bool {
        self.spelled == stored || self.resolved.as_deref() == Some(stored)
    }
}

/// How far ahead of the clock a claim may be stamped and still count. Past this
/// the stamp is a clock step or a hand edit, and the claim blocks nothing.
const CLAIM_CLOCK_SKEW_SECS: i64 = 5;

/// A claim's verdict against the roster: its new id and its age while it is in
/// flight, which is while its new id has a session that is not `done`, or has
/// none yet and the claim is younger than [`RESUME_CLAIM_BOUND_SECS`]. A claim
/// stamped more than [`CLAIM_CLOCK_SKEW_SECS`] ahead of `now` is stale; within
/// that skew its age is 0. Anything else is stale and blocks nothing.
fn claim_in_flight(
    claim: &aoide_storage::resuming::Claim,
    sessions: &[aoide_storage::records::SessionRecord],
    now: i64,
) -> Option<(String, i64)> {
    let raw_age = now - aoide_storage::time::parse_iso_utc(&claim.at).unwrap_or(now);
    if raw_age < -CLAIM_CLOCK_SKEW_SECS {
        return None;
    }
    let age_secs = raw_age.max(0);
    let open = sessions.iter().any(|s| s.session_id == claim.new_id && s.state != "done");
    let seen = sessions.iter().any(|s| s.session_id == claim.new_id);
    (open || (!seen && age_secs < RESUME_CLAIM_BOUND_SECS)).then(|| (claim.new_id.clone(), age_secs))
}

/// The block decision for a candidate, from the roster, the claims, the open
/// journals and the clock. `journal` is the candidate's own journal, for a
/// by-journal candidate only, and `roster_journals` lines up with `sessions`. A
/// live session is named before a launch in flight, so a running session reads as
/// running. A claim blocks when it names the candidate's session or its
/// journal. It compares what [`Journal`] holds and does no I/O.
fn block_verdict(
    entry: &aoide_storage::ledger::LedgerEntry,
    journal: Option<&Journal>,
    sessions: &[aoide_storage::records::SessionRecord],
    roster_journals: &[Option<Journal>],
    claims: &[aoide_storage::resuming::Claim],
    presence: &[(String, Journal)],
    now: i64,
) -> Option<Blocked> {
    let on_journal = |log: &Journal| journal.is_some_and(|j| j.names_the_file_of(log));
    let live = sessions.iter().zip(roster_journals).find(|(s, log)| {
        s.state != "done"
            && (s.session_id == entry.session_id
                || s.resumed_from.as_deref() == Some(entry.session_id.as_str())
                || log.as_ref().is_some_and(|l| on_journal(l)))
    });
    if let Some((s, _)) = live {
        return Some(Blocked::Live { live_id: s.session_id.clone() });
    }
    if let Some((id, _)) = presence.iter().find(|(_, log)| on_journal(log)) {
        return Some(Blocked::Live { live_id: id.clone() });
    }
    claims
        .iter()
        .filter(|c| {
            c.session_id == entry.session_id
                || journal.is_some_and(|j| c.journal.as_deref().is_some_and(|stored| j.is_named_by(stored)))
        })
        .find_map(|c| {
            claim_in_flight(c, sessions, now).map(|(new_id, age_secs)| Blocked::InFlight {
                new_id,
                age_secs,
                by_journal: c.session_id != entry.session_id,
            })
        })
}

/// The candidate's own journal, read once for the block, for a by-journal
/// candidate only.
fn candidate_journal(entry: &aoide_storage::ledger::LedgerEntry, by_log: bool) -> Option<Journal> {
    entry.log_path.as_deref().filter(|_| by_log).map(Journal::of)
}

/// Each roster record's `logPath`, read once for the block, lined up with
/// `sessions`.
fn roster_journals(sessions: &[aoide_storage::records::SessionRecord]) -> Vec<Option<Journal>> {
    sessions.iter().map(|s| s.log_path.as_deref().map(Journal::of)).collect()
}

/// The open journals a by-journal candidate is checked against, read from the
/// presence scan outside any lock, each one resolved once. A scan that cannot
/// finish refuses the candidate, since Aoide cannot then tell whether the journal
/// is open. A candidate that is not by-journal reads nothing.
fn presence_journals(
    entry: &aoide_storage::ledger::LedgerEntry,
    by_log: bool,
) -> Result<Vec<(String, Journal)>, Refusal> {
    if !by_log {
        return Ok(Vec::new());
    }
    match super::eidolon::eidolon_presence_sessions() {
        super::eidolon::PresenceScan::Observed(live) => {
            Ok(live.into_iter().map(|s| (s.id, Journal::of(&s.log))).collect())
        }
        super::eidolon::PresenceScan::Unknown(why) => Err(refused(
            entry,
            format!(
                "cannot be checked: the presence scan is incomplete ({why:?}), so Aoide cannot tell whether its journal is open — skipped rather than guessed"
            ),
        )),
    }
}

/// The roster and the claims, read under the stage lock by the claim step and
/// without it by a dry run. An unreadable file is an error the candidate refuses
/// on.
fn read_roster_and_claims() -> Result<(Vec<aoide_storage::records::SessionRecord>, Vec<aoide_storage::resuming::Claim>), String> {
    let roster: SessionsFile = load_stage(&sessions_path())?;
    Ok((roster.sessions, aoide_storage::resuming::load_claims()?))
}

/// The refusal for a candidate: `session <id> (<agent>)`, then why it is skipped.
fn refused(entry: &aoide_storage::ledger::LedgerEntry, why: String) -> Refusal {
    Refusal { reason: format!("session {} ({}) {why}", entry.session_id, entry.agent), named: None }
}

/// The refusal for a blocked candidate, naming the live session or the claim.
fn blocked_refusal(entry: &aoide_storage::ledger::LedgerEntry, blocked: Blocked) -> Refusal {
    match blocked {
        Blocked::Live { live_id } => Refusal {
            named: Some(("liveSessionId", live_id.clone())),
            ..refused(entry, format!("is live as {live_id}, so it is not resumed again — skipped"))
        },
        Blocked::InFlight { new_id, age_secs, by_journal } => {
            let held = if by_journal { "shares its journal with" } else { "has" };
            Refusal {
                named: Some(("claimedBy", new_id.clone())),
                ..refused(
                    entry,
                    format!("{held} a launch in flight as {new_id} (claimed {age_secs}s ago), so it is not resumed again — skipped"),
                )
            }
        }
    }
}

/// What a dry run shows for a candidate: its skip reason, or the verdict of the
/// block it would meet at launch. Takes no lock and writes nothing.
fn dry_verdict(c: &Candidate) -> Result<(), Refusal> {
    match &c.resume_argv {
        Err(reason) => Err(Refusal { reason: reason.clone(), named: None }),
        Ok(_) => dry_gate(&c.entry, c.by_log),
    }
}

fn dry_gate(entry: &aoide_storage::ledger::LedgerEntry, by_log: bool) -> Result<(), Refusal> {
    let presence = presence_journals(entry, by_log)?;
    let candidate = candidate_journal(entry, by_log);
    let (sessions, claims) = read_roster_and_claims().map_err(|e| {
        refused(entry, format!("cannot be checked: {e} — skipped rather than launched unchecked"))
    })?;
    let roster = roster_journals(&sessions);
    let now = super::conduct::unix_ts() as i64;
    block_verdict(entry, candidate.as_ref(), &sessions, &roster, &claims, &presence, now)
        .map_or(Ok(()), |b| Err(blocked_refusal(entry, b)))
}

/// The claim step, run before a launch: under the stage lock, the block and, when
/// the candidate is free, its claim written in place of any stale one. No spawn
/// runs under the lock.
fn claim_resume(
    entry: &aoide_storage::ledger::LedgerEntry,
    by_log: bool,
    new_id: &str,
) -> Result<(), Refusal> {
    let presence = presence_journals(entry, by_log)?;
    let candidate = candidate_journal(entry, by_log);
    let now = super::conduct::unix_ts() as i64;
    let claimed = aoide_storage::fs::try_stage_lock(|| -> Result<(), Refusal> {
        let (sessions, mut claims) = read_roster_and_claims().map_err(|e| {
            refused(entry, format!("cannot be checked: {e} — skipped rather than launched unchecked"))
        })?;
        let roster = roster_journals(&sessions);
        if let Some(blocked) = block_verdict(entry, candidate.as_ref(), &sessions, &roster, &claims, &presence, now) {
            return Err(blocked_refusal(entry, blocked));
        }
        claims.retain(|c| claim_in_flight(c, &sessions, now).is_some());
        aoide_storage::resuming::set_claim(
            &mut claims,
            aoide_storage::resuming::Claim {
                session_id: entry.session_id.clone(),
                new_id: new_id.to_string(),
                at: aoide_storage::time::iso_utc_from_epoch(now),
                journal: candidate.as_ref().map(|j| j.resolved.clone().unwrap_or_else(|| j.spelled.clone())),
            },
        );
        aoide_storage::resuming::save_claims(&claims).map_err(|e| {
            refused(entry, format!("cannot be claimed: {e} — skipped rather than launched unclaimed"))
        })
    });
    claimed.unwrap_or_else(|e| {
        Err(refused(
            entry,
            format!("cannot be claimed: the stage lock could not be taken ({e}) — skipped rather than launched unclaimed"),
        ))
    })
}

/// A failed launch drops its own claim under the stage lock, and only while the
/// claim still names its new id. If the file cannot be rewritten the claim stays,
/// and it is stale once [`RESUME_CLAIM_BOUND_SECS`] passes with nothing
/// registered under its id.
fn release_resume_claim(session_id: &str, new_id: &str) {
    let _ = aoide_storage::fs::try_stage_lock(|| {
        let Ok(mut claims) = aoide_storage::resuming::load_claims() else {
            return;
        };
        if aoide_storage::resuming::remove_claim(&mut claims, session_id, new_id) {
            let _ = aoide_storage::resuming::save_claims(&claims);
        }
    });
}

/// The `skipped` row a refused candidate leaves. Its named id is the key the
/// refusal names: `liveSessionId` or `claimedBy`.
fn refusal_row(entry: &aoide_storage::ledger::LedgerEntry, refusal: Refusal) -> serde_json::Value {
    let mut row = json!({ "sessionId": entry.session_id, "agent": entry.agent, "reason": refusal.reason });
    if let Some((key, id)) = refusal.named {
        row[key] = json!(id);
    }
    row
}

/// The login shell a resurrected terminal candidate re-opens with `-l` —
/// reconstructing exactly what `modules/dendrites/kitty.nix`'s own wrapper
/// execs (`:70,72`, `<login_shell> -l`), same three-step resolution order
/// (`kitty.nix:49-55`): `$SHELL` if set and executable, else this user's own
/// stored shell if executable, else the host's conventional fallback
/// (`/bin/sh`; `%COMSPEC%` on native Windows, which is the one command
/// interpreter that OS guarantees is present). Not a pure function —
/// it reads the environment, the user's own shell record, and the filesystem —
/// so it stays a thin, unmocked helper the same way `spawn.rs`'s own
/// `terminal_template`/`require_display` do; nothing downstream needs to
/// know WHY a shell was chosen, only which one.
fn login_shell() -> String {
    if let Ok(shell) = std::env::var("SHELL") {
        if is_executable_file(&shell) {
            return shell;
        }
    }
    if let Some(shell) = stored_login_shell() {
        if is_executable_file(&shell) {
            return shell;
        }
    }
    #[cfg(unix)]
    {
        "/bin/sh".to_string()
    }
    #[cfg(windows)]
    {
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
    }
}

/// Is `path` a program THIS host would run? One question, one answer per host,
/// because the two hosts disagree on what makes a file launchable at all.
/// Unix: a regular file carrying an execute bit. Native Windows: no mode bits
/// exist — a file is launchable by its EXTENSION (`%PATHEXT%`'s own set,
/// folded, which is exactly what `CreateProcess` acts on), so the extension IS
/// the answer there. Same contract either way: `true` only for a path this
/// process could actually exec, `false` for anything else (including a path
/// that does not exist).
fn is_executable_file(path: &str) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(windows)]
    {
        let path = std::path::Path::new(path);
        let launchable = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "exe" | "bat" | "cmd" | "com"));
        launchable && std::fs::metadata(path).map(|m| m.is_file()).unwrap_or(false)
    }
}

/// This user's own stored shell — the shell field of the passwd entry whose
/// uid is this process's, read through `getent` exactly as `kitty.nix`'s
/// `getent passwd "$(id -u)" | cut -d: -f7` reads it. Native Windows has no
/// passwd database and no uid to look up: the same fact lives in the
/// environment the OS itself maintains, and the caller above falls through to
/// `%COMSPEC%` when it is absent, so this arm answers `None` by NAME rather
/// than inventing a path (`None` is already the caller's "ask the next step").
#[cfg(unix)]
fn stored_login_shell() -> Option<String> {
    let uid = unsafe { libc::getuid() };
    let out = std::process::Command::new("getent").arg("passwd").arg(uid.to_string()).output().ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()?.trim_end().split(':').nth(6).map(str::to_string)
}

#[cfg(windows)]
fn stored_login_shell() -> Option<String> {
    None
}

/// The longest line a restore re-types, in bytes. A longer recorded argv is
/// refused, never clipped: a clipped line is wrong text, not a short one.
const RESTORE_LINE_MAX: usize = 4096;

/// Why a recorded foreground argv is never re-typed, or `None` when it may be.
/// Pure. The refusals are listed once, in CONTRACTS.md's restore paragraph. The
/// reason returned names the one that fired, and the `restore refused` line
/// carries it.
///
/// Reading argv is not proof of what will run: a same-uid process can rewrite
/// its own `/proc` cmdline, so these refusals judge the typed text, not the
/// process.
fn restore_argv_refusal(argv: &[String]) -> Option<&'static str> {
    if argv.first().is_some_and(|a| a.contains('=')) {
        return Some("an assignment prefix");
    }
    if argv.iter().any(|w| w.contains('\u{fffd}') || w.chars().any(is_unsafe)) {
        return Some("a control or invisible character");
    }
    if shell_join(argv).len() > RESTORE_LINE_MAX {
        return Some("a line over 4096 bytes");
    }
    if runs_a_privilege_program(argv) {
        return Some("a privilege program");
    }
    if program_is_a_shell(argv) && !program_is_an_interactive_shell(argv) {
        return Some("a shell-form argv");
    }
    None
}

/// The refusal a restore's recorded foreground gets, or `None`. An idle session
/// has no foreground argv to refuse, so `None` for it.
pub(in crate::graph) fn restore_refused_because(restore: &RestoreSnapshot) -> Option<&'static str> {
    if restore.idle {
        return None;
    }
    restore.argv.as_deref().and_then(restore_argv_refusal)
}

/// Type a restore into the revived session through the one gated injection
/// door. A restore is the session's own earlier bytes going back to its own
/// prompt, so what authorises it is the door's `--yes` and nothing else: it
/// names no sender, even to a caller the kernel can attest. A door that cannot
/// approve parks it, whoever is calling, and a parked restore is reported as
/// [`Landed::Held`], never as typed.
fn send_restore(inv: &Invocation, new_id: &str) -> Result<Landed, String> {
    let out = deliver_local_with(inv, new_id, |_| None);
    if out.status != Status::Ok {
        return Err(out.message);
    }
    if out.data.as_ref().and_then(|d| d.get("state")).and_then(serde_json::Value::as_str) == Some("pending") {
        return Ok(Landed::Held);
    }
    Ok(Landed::Typed)
}

/// Where a restore's line went once its send returned: typed into the pty, or
/// parked in the pending queue for a human to approve.
#[derive(Debug, PartialEq, Eq)]
enum Landed {
    Typed,
    Held,
}

/// What, if anything, to deliver into a freshly-resurrected terminal once it
/// registers — pure, no process, no socket, so this decision and its exact
/// `Invocation` flag map are directly unit-testable apart from
/// `resurrect_one`'s real spawn. Only a session that was running a command
/// when it left gets anything: that command is TYPED and never submitted, so
/// no flag map here carries `submit`. An idle one is restored by cwd alone.
///
/// A busy session's recorded argv is typed (`shell_join`-quoted) unless
/// [`restore_argv_refusal`] refuses it. Why never submitted: an argv is
/// attacker-choosable (a same-uid process can rewrite its own `/proc`
/// cmdline), and reading it cannot prove what it will do, so the human who
/// reads the line presses Enter. A control byte in it, `\r` included, would
/// act as that Enter, which is why the refusals stay.
fn restore_delivery(door: aoide_protocol::Door, new_id: &str, restore: &RestoreSnapshot) -> Option<Invocation> {
    if restore.idle {
        return None;
    }
    let argv = restore.argv.as_ref()?;
    if restore_argv_refusal(argv).is_some() {
        return None;
    }
    // `grep "a b" f` is typed as `grep 'a b' f`: the words re-parse to the
    // recorded argv, for an argv that passed `restore_argv_refusal`.
    let line = shell_join(argv);
    let mut flags = BTreeMap::new();
    flags.insert("id".to_string(), new_id.to_string());
    // Attributed to the target ITSELF: the line must reach its prompt verbatim,
    // and send's self-attribution rule delivers it that way. A provenance
    // prefix would turn it into a shell syntax error (live P-C7 finding).
    flags.insert("from".to_string(), new_id.to_string());
    flags.insert("yes".to_string(), "true".to_string());
    Some(Invocation {
        path: vec!["send".to_string()],
        args: vec![line],
        flags,
        door,
    })
}

/// How one resurrected terminal's restore ended, which is the `restoreDelivery`
/// of its row in `resurrected[]` ([`Restore::label`] is the whole vocabulary).
#[derive(Debug, PartialEq, Eq)]
enum Restore {
    /// The spawn never registered, so there was no live pty to deliver into.
    Unregistered,
    /// [`restore_refused_because`] refused the recorded foreground argv.
    Refused(&'static str),
    /// Nothing to deliver: an idle session, or an entry with no `restore`.
    Nothing,
    /// The target never became ready, so nothing was typed.
    NotReady,
    /// The send into the new pty failed.
    Failed(String),
    /// The command was typed, never submitted. `verified` is whether the
    /// readiness was a fact or a settled guess.
    Typed { verified: bool },
    /// The door could not approve the restore, so it waits in the pending queue
    /// for a human. Nothing was typed.
    Held,
}

impl Restore {
    fn label(&self) -> String {
        match self {
            Restore::Unregistered => "skipped-unregistered".to_string(),
            Restore::Refused(why) => format!("refused: {why}"),
            Restore::Nothing => "none".to_string(),
            Restore::NotReady => "not-ready".to_string(),
            Restore::Failed(why) => format!("failed: {why}"),
            Restore::Typed { verified: true } => "command".to_string(),
            Restore::Typed { verified: false } => "command-unverified".to_string(),
            Restore::Held => "held-pending".to_string(),
        }
    }
}

/// The restore step of one resurrected terminal, in the order its effects may
/// run: nothing is waited on or sent for an unregistered, refused or empty
/// restore, and nothing is sent to a target that is not ready. The wait and the
/// send are injected, so the whole decision is a table test.
fn restore_step(
    registered: bool,
    restore: Option<&RestoreSnapshot>,
    door: aoide_protocol::Door,
    new_id: &str,
    wait_ready: impl FnOnce() -> Ready,
    send: impl FnOnce(&Invocation) -> Result<Landed, String>,
) -> Restore {
    if !registered {
        return Restore::Unregistered;
    }
    if let Some(why) = restore.and_then(restore_refused_because) {
        return Restore::Refused(why);
    }
    let Some(inv) = restore.and_then(|r| restore_delivery(door, new_id, r)) else {
        return Restore::Nothing;
    };
    let ready = wait_ready();
    if ready == Ready::NotReady {
        return Restore::NotReady;
    }
    match send(&inv) {
        Ok(Landed::Typed) => Restore::Typed { verified: ready == Ready::Verified },
        Ok(Landed::Held) => Restore::Held,
        Err(why) => Restore::Failed(why),
    }
}

/// What `resurrect_one` does with a ledger entry's `origin` field when
/// reviving it (LANE IDENTITY P-ID0, G6 MUST-FIX). `Carry` for anything
/// EXCEPT a `node:*` shape — a `node:*` value gets `RefusedNode` instead of
/// being silently dropped, because `state/session-ledger.jsonl` is an
/// UNSEALED append-only file (sealing is P-ID1/P-ID2's job, not this
/// phase): a same-uid process can append a line claiming `origin:"node:X"`
/// and then run this ungated LOCAL `aoide resurrect`, which has no door and
/// no seal behind it. `node:*` is door-authenticated identity — only
/// `a2a::do_spawn`'s `stamp_spawn_provenance` may mint it (`stamp_origin`'s own
/// doc names the two legitimate STAMP callers) — so a local resurrect
/// re-minting that authority off an unsealed file would be exactly the
/// forgery this lane closes, not a fix for it. Pure and directly
/// unit-testable, so the refusal is provable without a real spawn.
#[derive(Debug, PartialEq, Eq)]
enum OriginCarry<'a> {
    Carry(&'a str),
    RefusedNode(&'a str),
    Nothing,
}

fn origin_to_carry(ledger_origin: Option<&str>) -> OriginCarry<'_> {
    match ledger_origin {
        Some(origin) if is_node_origin(origin) => OriginCarry::RefusedNode(origin),
        Some(origin) => OriginCarry::Carry(origin),
        None => OriginCarry::Nothing,
    }
}

/// The reason a candidate whose harness has no verified resume argv is skipped,
/// read by the launch and by a dry run alike: a taught line, never a guessed
/// invocation.
fn unresumed_reason(agent: &str) -> String {
    format!("harness `{agent}` has no verified resume argv — skipped rather than typing a guessed invocation")
}

/// Spawn one candidate via the windowed path and fold the outcome into
/// `resurrected`/`skipped`/`failed`. Never returns an error — every failure
/// mode is data, per the module doc.
fn resurrect_one(
    door: aoide_protocol::Door,
    c: Candidate,
    resurrected: &mut Vec<serde_json::Value>,
    skipped: &mut Vec<serde_json::Value>,
    failed: &mut Vec<serde_json::Value>,
    changed: &mut Vec<String>,
) {
    let argv = match c.resume_argv {
        Ok(argv) => argv,
        Err(reason) => {
            skipped.push(json!({
                "sessionId": c.entry.session_id,
                "agent": c.entry.agent,
                "reason": reason,
            }));
            return;
        }
    };
    let new_id = mint_resurrected_id();
    if let Err(refusal) = claim_resume(&c.entry, c.by_log, &new_id) {
        skipped.push(refusal_row(&c.entry, refusal));
        return;
    }
    let mut flags = BTreeMap::new();
    flags.insert("agent".to_string(), c.entry.agent.clone());
    flags.insert("id".to_string(), new_id.clone());
    flags.insert("windowed".to_string(), "true".to_string());
    if !c.entry.cwd.is_empty() {
        flags.insert("cwd".to_string(), c.entry.cwd.clone());
    }
    let spawn_inv = Invocation {
        path: vec!["spawn".to_string()],
        args: argv,
        flags,
        door,
    };
    // This launch's start instant, taken before the spawn exists — the
    // readiness wait below reads only a harness `SessionStart` stamped at or
    // after it, so a leftover record from the session this one is replacing
    // (the ledger's own old id, or a reused `--id`) cannot pass for the new
    // one's hello.
    let launch_at = super::session_store::now_iso_utc();
    let out = launch_session(&spawn_inv, None);
    if out.status != aoide_protocol::output::Status::Ok {
        release_resume_claim(&c.entry.session_id, &new_id);
        failed.push(json!({
            "sessionId": c.entry.session_id,
            "agent": c.entry.agent,
            "reason": out.message,
        }));
        return;
    }
    let registered = out
        .data
        .as_ref()
        .and_then(|d| d.get("registered"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    // Safe no-op if the record has not landed yet (an honest `registered:
    // false` on a slow terminal open) — never a second wait loop here;
    // `session_spawn` already spent its own registration budget.
    stamp_resumed_from(&new_id, &c.entry.session_id);
    if let Some(project) = &c.entry.project { super::actions::assign_project(&new_id, Some(project)); }

    // Carry the ledger entry's own `origin` forward onto the revived record
    // (LANE IDENTITY P-ID0, G6) — LOCAL-CLASS ONLY. `ledger_session_exit`
    // writes `origin` on every exit, but nothing read it back until now, so
    // a revived LOCAL session silently became origin-less. A `node:*` value
    // is refused here on purpose, not carried: `state/session-ledger.jsonl`
    // is an UNSEALED append-only file (the P-ID1 seal covers live session
    // records, never this ledger)
    // — a same-uid process can append a line claiming `origin:"node:X"` and
    // then run this ungated LOCAL `aoide resurrect`, which has no door and
    // no seal behind it. `node:*` is door-authenticated identity (only
    // `a2a::do_spawn`'s `stamp_spawn_provenance` may mint it, per `stamp_origin`'s
    // own doc); a local resurrect re-minting that authority off an unsealed
    // file would be exactly the forgery this lane closes, not a fix for it.
    // Change-only/no-op-safe exactly like `stamp_resumed_from` above for the
    // carried case (an unknown id or empty origin is a silent no-op);
    // absent on the ledger entry (a locally-registered session never had
    // one) means nothing to carry, same as before.
    match origin_to_carry(c.entry.origin.as_deref()) {
        OriginCarry::Carry(origin) => stamp_origin(&new_id, origin),
        OriginCarry::RefusedNode(origin) => eprintln!(
            "aoide resurrect: reviving `{}` as LOCAL-class — its ledger origin `{origin}` cannot be trusted from an unsealed ledger and is not carried forward (P-ID1's sealed credential closes this)",
            c.entry.session_id
        ),
        OriginCarry::Nothing => {}
    }

    // Undying transfer (P-C3, durable-sessions plan): if the OLD id was
    // durable, move the mark onto the fresh one rather than leaving it
    // behind — a mark left on a ledger id would double-resurrect on the next
    // sweep once P-C4 selects off the undying set. A no-op when the old id
    // was never undying at all (this resurrect did not originate from the
    // undying set), so an ordinary `--all`/`--id` revive never starts
    // marking sessions undying that nobody marked. A by-journal revival
    // (`ByLog`) moves nothing: its mark stays on the ledger id, the module
    // doc says why.
    //
    // Both mutations land in ONE in-memory vector before the SINGLE
    // `save_undying` write below — mark the new id BEFORE dropping the old
    // one, so a crash between the two in-memory edits and the write is
    // impossible, and a crash right before the write leaves the OLD id
    // still undying (retry-safe) rather than neither (silent loss). Also
    // idempotent: re-running this on an already-transferred pair finds the
    // old id no longer undying and writes nothing.
    let mut undying = aoide_storage::undying::load_undying();
    if !c.by_log && aoide_storage::undying::is_undying(&undying, &c.entry.session_id) {
        aoide_storage::undying::set_undying(&mut undying, &new_id, true);
        aoide_storage::undying::set_undying(&mut undying, &c.entry.session_id, false);
        let _ = aoide_storage::undying::save_undying(&undying);
    }

    // Post-spawn restore delivery (P-C6, durable-sessions plan) — only for a
    // TERMINAL candidate (a `restore` snapshot present) whose spawn actually
    // registered AND became READY: an unregistered session has no live pty to
    // deliver into, and a registered-but-unstarted one has a pty whose harness
    // is still coming up, where the same early-typing defect `spawn --prompt`
    // just fixed would lose the line. `restore_step` runs the wait and the send
    // in order and names the outcome; `restore_delivery` is pure and decides the
    // whole shape: it is `Some` only for a busy session's command.
    let restore = restore_step(
        registered,
        c.entry.restore.as_ref(),
        door,
        &new_id,
        || super::spawn::wait_ready(&c.entry.agent, &new_id, &launch_at, super::spawn::READY_BUDGET),
        |inv| send_restore(inv, &new_id),
    );
    if let Restore::Refused(why) = &restore {
        changed.push(format!("session {new_id}: restore refused — {why}"));
    }

    changed.push(format!(
        "session {new_id}: resurrected from {} ({}){}",
        c.entry.session_id,
        c.entry.agent,
        if registered { "" } else { " (not yet registered)" }
    ));
    if let Restore::Typed { .. } = restore {
        changed.push(format!("session {new_id}: restore {}", restore.label()));
    }
    if restore == Restore::NotReady {
        changed.push(format!(
            "session {new_id}: restore not injected — `{}` was not ready",
            c.entry.agent
        ));
    }
    if restore == Restore::Held {
        changed.push(format!("session {new_id}: restore held pending approval"));
    }
    resurrected.push(json!({
        "sessionId": new_id,
        "resumedFrom": c.entry.session_id,
        "agent": c.entry.agent,
        "registered": registered,
        "restoreDelivery": restore.label(),
    }));
}

/// The bare form's picks (no `--all`/`--id`, decision 6 of the durable-sessions
/// plan): the project's WHOLE undying set, not just its single most recent
/// entry, as positions in `anchored`, keyed by `sessionId` so they come out in
/// the order the bare launch revives them. Three steps, in order:
///
/// 1. keep only entries whose `sessionId` is in `state/undying.json`
///    (`aoide_storage::undying::is_undying`);
/// 2. drop any id that is already alive (non-`done`) in `sessions.json` —
///    the daemon's old `has_live` skip moves HERE, per-id instead of
///    per-project, so one live terminal no longer suppresses the rest of a
///    multi-session undying set (`server/src/daemon.rs`'s
///    `run_boot_auto_resume`, which now calls this unconditionally);
/// 3. dedup by `sessionId`, keeping the entry with the latest `endedAt` — a
///    undying id that was resurrected and exited again appears twice in the
///    append-only ledger.
fn undying_picks(anchored: &[aoide_storage::ledger::LedgerEntry]) -> BTreeMap<String, usize> {
    let undying = aoide_storage::undying::load_undying();
    let sessions: SessionsFile = load_stage(&sessions_path()).unwrap_or_default();
    let live: std::collections::HashSet<&str> = sessions
        .sessions
        .iter()
        .filter(|s| s.state != "done")
        .map(|s| s.session_id.as_str())
        .collect();

    let mut newest: BTreeMap<String, usize> = BTreeMap::new();
    for (i, e) in anchored.iter().enumerate() {
        if !aoide_storage::undying::is_undying(&undying, &e.session_id) {
            continue;
        }
        if live.contains(e.session_id.as_str()) {
            continue;
        }
        let ended = aoide_storage::time::parse_iso_utc(&e.ended_at).unwrap_or(0);
        let keep = match newest.get(&e.session_id) {
            Some(&j) => ended > aoide_storage::time::parse_iso_utc(&anchored[j].ended_at).unwrap_or(0),
            None => true,
        };
        if keep {
            newest.insert(e.session_id.clone(), i);
        }
    }
    newest
}

/// The bare form's launch: the entries [`undying_picks`] names.
fn undying_selection(anchored: Vec<aoide_storage::ledger::LedgerEntry>) -> Vec<aoide_storage::ledger::LedgerEntry> {
    let picks = undying_picks(&anchored);
    picks.values().map(|&i| anchored[i].clone()).collect()
}

/// One audit line per `resurrect` invocation, carrying the decision's own
/// counts/message — the boot-sweep postmortem's own finding (gate-6): an
/// early empty-selection return must audit exactly like a full run does,
/// never silently skip it. Mirrors `send.rs`'s `audit_send`/`pending.rs`'s
/// `audit_pending` shape, minus `untrusted_data` (a resurrect decision
/// carries no forwarded text to wrap). Deliberately NOT called from a pure
/// usage/stage-file miss (a missing flag, an unreadable `projects.json`/
/// ledger) — the same posture `audit_send` already holds toward its own
/// `require_flag`/`stage_error` early exits; this covers every point past
/// that where `resurrect` has actually made — or attempted — a revival
/// decision.
fn audit_resurrect(inv: &Invocation, status: &str, message: &str) {
    // A dry run audits under its own status whatever it decided, so the log
    // tells a listing from a launch.
    let status = if inv.flag_present("dry-run") { "dry-run" } else { status };
    let log = aoide_protocol::audit_log_path(inv);
    let _ = aoide_protocol::append_audit(
        &log,
        &aoide_protocol::AuditRecord {
            ts: aoide_protocol::audit::now_secs(),
            door: inv.door,
            class: aoide_protocol::EventClass::Audit,
            command: "resurrect".to_string(),
            status: status.to_string(),
            message: message.to_string(),
            untrusted_data: None,
        },
    );
}

/// Is the caller of a door that cannot approve a session? Only the MCP door
/// can be: its server walks its own ancestry to a sealed session. The A2A door
/// runs inside aoided with no sealed ancestor, so nothing is asked of it.
fn caller_is_attested(
    inv: &Invocation,
    resolve_caller: impl Fn(&[super::model::SessionRecord]) -> Option<String>,
) -> bool {
    inv.door == Door::Mcp && resolve_caller(&load_stage::<SessionsFile>(&sessions_path()).unwrap_or_default().sessions).is_some()
}

/// `aoide resurrect [--project <name> [--all | --id <ledgerSessionId>]]`.
///
/// Bare (none of the three flags): try the manifest first
/// ([`resurrect_from_manifest`], U2's own module-doc paragraph) — found, it
/// owns the whole outcome; not found, falls through to the flag-mode path
/// below, whose `require_flag` miss now teaches both misses at once. Any of
/// `--project`/`--all`/`--id` present routes straight past the manifest
/// check to the pre-existing flag-mode selection, unchanged.
///
/// A door that cannot approve launches only as an attested caller, the way
/// `spawn` does ([`session_resurrect_with`]): an MCP server under a sealed
/// session revives (its restore bytes are still held, see [`send_restore`]), and
/// every other caller has a `--project` call held whole in `pending.json`
/// for a human to approve, with nothing launched. Bare mode is refused there,
/// and `--id` or `--all` without `--project` is a usage error. `--dry-run`
/// lists the same selection instead, and launches, holds and sends nothing
/// ([`dry_run_listing`], [`dry_run_manifest`]). From a caller that cannot
/// approve, a dry run gets what the call gives without the launch: bare mode
/// its refusal, `--id` or `--all` without `--project` the usage error, and
/// `--project` the `dry-run-not-held` refusal.
pub fn session_resurrect(inv: &Invocation) -> Outcome {
    session_resurrect_with(inv, real_attested_sender)
}

/// [`session_resurrect`] with the sender-identity resolver injected, so the
/// door's decision is testable without a live daemon.
///
/// An unattested caller on a door that cannot approve gets `--project …` held
/// whole, and its dry run is refused instead, since the call would hold it and
/// show that caller nothing. With `--id` or `--all` and no `--project`, a dry
/// run gets the usage error the call gives, since that call holds nothing.
/// Bare mode is refused there instead of held, dry or not: it reads the
/// working directory's manifest and launches the manifest's own commands, which
/// an approver replaying from another directory would neither see in the held
/// line nor find in the same place.
fn session_resurrect_with(
    inv: &Invocation,
    resolve_caller: impl Fn(&[super::model::SessionRecord]) -> Option<String>,
) -> Outcome {
    let cmd = "resurrect";
    let flag_mode = inv.flags.contains_key("project") || inv.flags.contains_key("id") || inv.flag_present("all");
    let dry = inv.flag_present("dry-run");
    if !inv.door.can_approve() && !caller_is_attested(inv, resolve_caller) {
        if !flag_mode {
            let out = Outcome::refuse(
                cmd,
                Kind::Refused,
                "resurrect not held: bare mode reads this process's working directory",
                "a held command is replayed by the human who approves it, from their directory, and a \
                 manifest's own commands would launch without being shown to them",
                Fix::Run("aoide resurrect --project <name>".to_string()),
            )
            .with_fields(json!({ "reason": "bare-resurrect-not-held" }));
            audit_resurrect(inv, "refused", &out.message);
            return out;
        }
        if dry && inv.flags.contains_key("project") {
            let out = Outcome::refuse(
                cmd,
                Kind::Refused,
                "resurrect dry run not shown: this door cannot approve, and the call \
                 would wait for a person",
                "a dry run shows what its caller gets without an approval, and from this \
                 door the call holds and shows that caller nothing",
                Fix::Run("aoide resurrect --project <name>".to_string()),
            )
            .with_fields(json!({ "reason": "dry-run-not-held" }));
            audit_resurrect(inv, "refused", &out.message);
            return out;
        }
        if let Some(project) = inv.flags.get("project") {
            let held = PendingSpawn {
                path: vec![cmd.to_string()],
                args: inv.args.clone(),
                flags: inv.flags.clone(),
                shown: held_plan(inv, project),
            };
            return hold_whole(inv, &format!("{cmd}:{project}"), held, "");
        }
    }
    if !flag_mode {
        let cwd = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
        if let Some((root, manifest)) = aoide_storage::manifest::walk_up(&cwd) {
            return if dry {
                dry_run_manifest(inv, &root, &manifest)
            } else {
                resurrect_from_manifest(inv, &root, &manifest)
            };
        }
    }

    let name = match require_flag(inv, "project") {
        Ok(v) => v,
        Err(o) => {
            // The manifest-miss wording is honest ONLY when a manifest walk
            // was actually attempted — that happened above iff `!flag_mode`.
            // `flag_mode == true` here means one of `--all`/`--id` was given
            // without `--project` (no walk was ever tried, and one of the
            // three flags WAS given) — `require_flag`'s own original error
            // is the true one in that case (review fix, U2 round 1: this
            // branch used to return the manifest-miss message unconditionally,
            // which lied on both counts for e.g. a bare `--id` invocation).
            if flag_mode {
                return o;
            }
            let cwd = std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "?".to_string());
            return Outcome::usage(
                cmd,
                format!(
                    "no .aoide/project.json above `{cwd}` and no --project/--all/--id given — \
                     run inside a project with a manifest, or pass --project <name>"
                ),
            );
        }
    };

    let selection = match select_candidates(inv, &name) {
        Ok(selection) => selection,
        Err(o) => return o,
    };
    if dry {
        return dry_run_listing(inv, &selection);
    }
    launch_selection(inv, &selection)
}

/// The candidates a flag-mode resurrect names, resolved against the project,
/// the ledger and the undying set as they stand: the sessions it revives.
pub(in crate::graph) struct Selection {
    name: String,
    selected: Vec<aoide_storage::ledger::LedgerEntry>,
    /// `picked[i]` is whether the bare `--project` form revives `selected[i]`;
    /// only a dry run reads it.
    picked: Vec<bool>,
}

/// Resolve `--project <name>` with `--id` or `--all` (or the undying set) to its
/// candidates. A refusal is the outcome the command gives, audited here.
fn select_candidates(inv: &Invocation, name: &str) -> Result<Selection, Outcome> {
    let cmd = "resurrect";
    let projects: ProjectsFile = match load_stage(&projects_path()) {
        Ok(f) => f,
        Err(e) => return Err(stage_error(cmd, e)),
    };
    let Some(target_idx) = projects.projects.iter().position(|p| p.name == name) else {
        let out = Outcome::error(cmd, format!("no project named `{name}` — register it first with `project add`"))
            .with_data(json!({ "reason": "unknown-project", "project": name }));
        audit_resurrect(inv, "error", &out.message);
        return Err(out);
    };

    let ledger = match aoide_storage::ledger::read_ledger() {
        Ok(v) => v,
        Err(e) => return Err(stage_error(cmd, e.to_string())),
    };
    let anchored: Vec<aoide_storage::ledger::LedgerEntry> = ledger
        .into_iter()
        .filter(|e| e.project.as_ref().map_or_else(|| super::model::anchor_for(&e.cwd, &projects.projects) == Some(target_idx), |p| p == &projects.projects[target_idx].name))
        .collect();

    // The bare form's picks by position in `anchored`. On an `--all` or `--id`
    // call only a dry run reads the undying set here. A real call reads it per
    // revival in `resurrect_one`, and rewrites it there when a mark moves.
    let picks = if inv.flag_present("dry-run") { undying_picks(&anchored) } else { BTreeMap::new() };
    let picked_at = |i: usize| picks.values().any(|&j| j == i);
    let (selected, picked): (Vec<aoide_storage::ledger::LedgerEntry>, Vec<bool>) = if let Some(id) = inv.flags.get("id") {
        match anchored.iter().rposition(|e| &e.session_id == id) {
            Some(i) => (vec![anchored[i].clone()], vec![picked_at(i)]),
            None => {
                let out = Outcome::error(
                    cmd,
                    format!("no ledger entry `{id}` anchored to project `{name}`"),
                )
                .with_data(json!({ "reason": "unknown-ledger-id", "project": name, "id": id }));
                audit_resurrect(inv, "error", &out.message);
                return Err(out);
            }
        }
    } else if inv.flag_present("all") {
        let mut order: Vec<usize> = (0..anchored.len()).collect();
        order.sort_by(|&a, &b| {
            let ea = aoide_storage::time::parse_iso_utc(&anchored[a].ended_at).unwrap_or(0);
            let eb = aoide_storage::time::parse_iso_utc(&anchored[b].ended_at).unwrap_or(0);
            eb.cmp(&ea)
        });
        (
            order.iter().map(|&i| anchored[i].clone()).collect(),
            order.iter().map(|&i| picked_at(i)).collect(),
        )
    } else {
        let selected = undying_selection(anchored);
        let picked = vec![true; selected.len()];
        (selected, picked)
    };
    Ok(Selection { name: name.to_string(), selected, picked })
}

/// What a held resurrect would do, one line per candidate, in the words the
/// approver reads: the session it revives and exactly what its restore would
/// type into the new terminal. Approval replays only a selection whose plan
/// equals the one shown ([`approved_selection`]), so nothing is typed that the
/// approver was not shown.
pub(in crate::graph) fn plan_lines(selection: &Selection) -> Vec<String> {
    if selection.selected.is_empty() {
        return vec![format!("nothing to revive for project `{}`", selection.name)];
    }
    selection
        .selected
        .iter()
        .map(|e| format!("{} ({}): {}", e.session_id, e.agent, restore_text(e.restore.as_ref())))
        .collect()
}

/// The plan line's account of one restore: the text [`restore_delivery`] would
/// type, or why nothing is typed. Must agree with `restore_delivery`.
fn restore_text(restore: Option<&RestoreSnapshot>) -> String {
    let Some(restore) = restore else {
        return "no restore recorded, nothing typed".to_string();
    };
    if restore.idle {
        return "idle, so only the cwd restores".to_string();
    }
    if let Some(why) = restore_refused_because(restore) {
        return format!("refused ({why}), nothing typed");
    }
    match restore.argv.as_deref() {
        Some(argv) => format!("types `{}`, never submitted", shell_join(argv)),
        None => "no command recorded, nothing typed".to_string(),
    }
}

/// The plan a held resurrect carries: what [`select_candidates`] resolves now.
/// A selection that cannot be made is held as that refusal's message, so the
/// approver reads the same thing a launch would answer.
pub(in crate::graph) fn held_plan(inv: &Invocation, name: &str) -> Vec<String> {
    match select_candidates(inv, name) {
        Ok(selection) => plan_lines(&selection),
        Err(o) => vec![format!("no candidates: {}", o.message)],
    }
}

/// The selection an approval of a held resurrect may launch: the one `launch`
/// names, provided its plan still equals `shown`, the plan the entry was held
/// with. A plan that moved is refused, so the launch can never revive, or type,
/// anything the approver was not shown.
pub(in crate::graph) fn approved_selection(launch: &Invocation, shown: &[String]) -> Result<Selection, Outcome> {
    let name = launch.flags.get("project").map(String::as_str).unwrap_or_default();
    let now = select_candidates(launch, name);
    let lines = match &now {
        Ok(selection) => plan_lines(selection),
        Err(o) => vec![format!("no candidates: {}", o.message)],
    };
    if lines != shown {
        let out = Outcome::refuse(
            "resurrect",
            Kind::Refused,
            format!("held resurrect of `{name}` not launched: its candidates changed since it was shown"),
            "what it would revive, and what each restore would type, is decided at approval, and it differs from the plan the entry was held with",
            Fix::None("deny this entry and hold the resurrect again to see the current plan"),
        )
        .with_fields(json!({ "reason": "plan-changed", "shown": shown, "now": lines }));
        audit_resurrect(launch, "refused", &out.message);
        return Err(out);
    }
    now
}

/// Revive every candidate of `selection`, as `inv`'s door. The shared launch
/// of a direct resurrect and an approved held one.
pub(in crate::graph) fn launch_selection(inv: &Invocation, selection: &Selection) -> Outcome {
    let cmd = "resurrect";
    let name = selection.name.as_str();
    if selection.selected.is_empty() {
        let bare = inv.flags.get("id").is_none() && !inv.flag_present("all");
        let msg = if bare {
            format!("undying set is empty for project `{name}` — nothing to resurrect")
        } else {
            format!("no resumable session found for project `{name}`")
        };
        audit_resurrect(inv, "ok", &msg);
        return Outcome::ok(cmd, msg)
            .with_data(json!({ "project": name, "resurrected": [], "skipped": [], "failed": [] }));
    }

    let mut resurrected: Vec<serde_json::Value> = Vec::new();
    let mut skipped: Vec<serde_json::Value> = Vec::new();
    let mut failed: Vec<serde_json::Value> = Vec::new();
    let mut changed: Vec<String> = Vec::new();
    for e in selection.selected.iter().cloned() {
        resurrect_one(inv.door, resolve_candidate(e), &mut resurrected, &mut skipped, &mut failed, &mut changed);
    }

    let out = Outcome::ok(
        cmd,
        format!(
            "project `{name}`: resurrected {}, skipped {}, failed {}",
            resurrected.len(),
            skipped.len(),
            failed.len(),
        ),
    )
    .changed(changed)
    .with_data(json!({
        "project": name,
        "resurrected": resurrected,
        "skipped": skipped,
        "failed": failed,
        "sessionsFile": sessions_path().to_string_lossy(),
    }));
    audit_resurrect(inv, "ok", &out.message);
    out
}

/// A flag-mode dry run: one row per candidate the launch would act on, in the
/// order it acts on them, each with the disposition `resurrect_one` would reach
/// (a candidate with no resume argv is skipped there too). The message is the
/// plan `approved_selection` compares, each line spelled as the pending view
/// spells a held plan line (`visible_held`); the data and the stored plan stay
/// raw. Nothing is spawned, sent or held.
fn dry_run_listing(inv: &Invocation, selection: &Selection) -> Outcome {
    let name = selection.name.as_str();
    let candidates: Vec<serde_json::Value> = selection
        .selected
        .iter()
        .zip(&selection.picked)
        .map(|(e, &picked)| {
            let mut row = json!({
                "sessionId": e.session_id,
                "agent": e.agent,
                "project": name,
                "cwd": e.cwd,
                "endedAt": e.ended_at,
                "restoreText": restore_text(e.restore.as_ref()),
                "pickedByBare": picked,
                "disposition": "revived-from-ledger",
            });
            if let Err(refusal) = dry_verdict(&resolve_candidate(e.clone())) {
                row["disposition"] = json!("skipped");
                row["reason"] = json!(refusal.reason);
                if let Some((key, id)) = refusal.named {
                    row[key] = json!(id);
                }
            }
            row
        })
        .collect();
    let message = plan_lines(selection)
        .iter()
        .map(|l| super::visible_held(l))
        .collect::<Vec<_>>()
        .join("\n");
    audit_resurrect(inv, "dry-run", &message);
    Outcome::ok("resurrect", message).with_data(json!({
        "project": name,
        "dryRun": true,
        "candidates": candidates,
    }))
}

/// The enriching ledger entry for a manifest spec: the newest entry whose cwd
/// and agent match the spec's resolved dir and agent, the module doc's
/// enrichment rule. `None` is the clean-spawn case.
fn enrichment<'a>(
    ledger: &'a [aoide_storage::ledger::LedgerEntry],
    dir: &str,
    agent: &str,
) -> Option<&'a aoide_storage::ledger::LedgerEntry> {
    ledger
        .iter()
        .filter(|e| e.cwd == dir && e.agent == agent)
        .max_by_key(|e| aoide_storage::time::parse_iso_utc(&e.ended_at).unwrap_or(0))
}

/// Mark a manifest-revived session's OWN new id undying (orchestrator
/// design ruling, U2 review round 1) — `entry` is a `resurrected[]` row
/// (either shape: `resurrect_one`'s own, or `clean_spawn_from_spec`'s),
/// read back for its `sessionId` rather than threading one down through
/// another parameter. One `load_undying`/`set_undying`/`save_undying`,
/// gated on nothing but the row already being IN `resurrected` — which by
/// construction only happens once the underlying spawn reached
/// `Status::Ok` (never on `registered`: this crate's own tests never open
/// a real terminal end-to-end, the same line `spawn.rs`'s module doc
/// draws, so gating on live registration would make this unconditionally
/// untestable here). The rationale for marking unconditionally rather than
/// only transferring a PRE-existing mark (`resurrect_one`'s own transfer
/// block, a few paragraphs up, untouched by this function): the manifest
/// spec IS the durable declaration of what should exist, so its revived
/// session belongs in the undying set regardless of whether the ledger
/// entry that enriched it (if any) happened to be marked — a later bare
/// `resurrect --project <name>` or the daemon's boot sweep then finds it
/// without ever re-walking or re-consulting the manifest.
fn mark_manifest_revival_undying(entry: &serde_json::Value) {
    let Some(id) = entry.get("sessionId").and_then(serde_json::Value::as_str) else {
        return;
    };
    let mut undying = aoide_storage::undying::load_undying();
    aoide_storage::undying::set_undying(&mut undying, id, true);
    let _ = aoide_storage::undying::save_undying(&undying);
}

/// U2's bare-manifest mode: `resurrect` with no `--project`/`--all`/`--id`
/// found `.aoide/project.json` walking up from cwd — revive its specs
/// directly. See this module's own doc for the enrichment rule and the
/// remote-skip/containment-guard steps; this function is the loop that
/// applies them per spec, one failure never aborting the rest (same
/// posture the flag-mode candidate loop above already holds).
fn resurrect_from_manifest(
    inv: &Invocation,
    root: &Path,
    manifest: &aoide_storage::manifest::Manifest,
) -> Outcome {
    let cmd = "resurrect";
    let this_host = aoide_storage::display::local_host_name();
    let ledger = match aoide_storage::ledger::read_ledger() {
        Ok(v) => v,
        Err(e) => return stage_error(cmd, e.to_string()),
    };

    let mut resurrected: Vec<serde_json::Value> = Vec::new();
    let mut skipped: Vec<serde_json::Value> = Vec::new();
    let mut failed: Vec<serde_json::Value> = Vec::new();
    let mut changed: Vec<String> = Vec::new();

    for spec in &manifest.sessions {
        if spec.host != this_host {
            summon_remote(spec, &mut resurrected, &mut failed, &mut changed);
            continue;
        }

        let dir = match aoide_storage::manifest::resolve_spec_dir(root, &spec.dir) {
            Ok(p) => p,
            Err(e) => {
                failed.push(json!({
                    "host": spec.host, "dir": spec.dir, "agent": spec.agent,
                    "disposition": "failed", "reason": e,
                }));
                continue;
            }
        };
        let dir_str = dir.to_string_lossy().into_owned();

        // Enrichment (the module doc's own rule): the newest ledger entry
        // whose cwd/agent match this spec — host is implicit, a spec whose
        // host didn't match this one already skipped above, and the ledger
        // itself is host-local state that is never synced.
        let matched = enrichment(&ledger, &dir_str, &spec.agent);

        match matched {
            Some(entry) => {
                // `resurrect_one` is the flag-mode function, reused
                // VERBATIM — it pushes into exactly ONE of the three
                // buckets per call, none of its own pushes carrying a
                // `disposition` key (that's a manifest-mode-only concept).
                // Track each bucket's length so whichever one grew gets
                // stamped after the fact — every row this loop's own
                // outcome carries MUST have a `disposition`, so a consumer
                // filtering by it never silently drops a row that fell
                // through `resurrect_one`'s own skip/fail shapes (review
                // fix, U2 round 1).
                let (before_r, before_s, before_f) = (resurrected.len(), skipped.len(), failed.len());
                resurrect_one(inv.door, resolve_candidate(entry.clone()), &mut resurrected, &mut skipped, &mut failed, &mut changed);
                if resurrected.len() > before_r {
                    if let Some(last) = resurrected.last_mut() {
                        last["disposition"] = json!("revived-from-ledger");
                    }
                    mark_manifest_revival_undying(&resurrected[resurrected.len() - 1]);
                } else if skipped.len() > before_s {
                    if let Some(last) = skipped.last_mut() {
                        last["disposition"] = json!("skipped");
                    }
                } else if failed.len() > before_f {
                    if let Some(last) = failed.last_mut() {
                        last["disposition"] = json!("failed");
                    }
                }
            }
            None => {
                let before_r = resurrected.len();
                clean_spawn_from_spec(inv.door, spec, &dir_str, &mut resurrected, &mut failed, &mut changed);
                if resurrected.len() > before_r {
                    mark_manifest_revival_undying(&resurrected[resurrected.len() - 1]);
                }
            }
        }
    }

    let out = Outcome::ok(
        cmd,
        format!(
            "manifest at `{}`: resurrected {}, skipped {}, failed {}",
            root.display(),
            resurrected.len(),
            skipped.len(),
            failed.len(),
        ),
    )
    .changed(changed)
    .with_data(json!({
        "manifestRoot": root.to_string_lossy(),
        "resurrected": resurrected,
        "skipped": skipped,
        "failed": failed,
        "sessionsFile": sessions_path().to_string_lossy(),
    }));
    audit_resurrect(inv, "ok", &out.message);
    out
}

/// A manifest spec whose `host` names a DIFFERENT box: summoned through the
/// node door rather than skipped (U4, command-defrag lane U — landing the
/// U2/U3 module doc's own "remote summoning is a later phase" note).
/// `spec.host` resolves against `state/nodes.json` the exact same way U3's
/// picker WRITES it (`{host: <node name>, dir, agent}`, `CONTRACTS.md`'s
/// `.aoide/project.json` section) — a node NICKNAME, not a literal DNS/OS
/// hostname. Three local refusals, all landing in `failed[]` — never
/// `skipped[]`, since this spec was tried and refused, not given up on —
/// before the wire is ever touched:
/// - no node named `spec.host` at all — taught, names `node add`;
/// - a registered but UNVERIFIED node — the same local-only refusal
///   `aoide-client::commands::handle_node_spawn` already holds (an
///   unsigned request can never satisfy the remote door's `Signature`-rung
///   spawn gate, P-P4/PAIRING.md decision 6): refused HERE rather than
///   earning a doomed round trip;
/// - neither a `command` nor a registered `AgentProfile::launch` default
///   for `spec.agent` ([`summon_text`]) — the same taught gap
///   [`clean_spawn_from_spec`] refuses locally, mirrored here since there
///   is no argv to build a prompt from either.
///
/// Past those three, this reuses [`aoide_client::commands::spawn_on_node`]
/// VERBATIM — the identical signed spawn-shaped `message/send`
/// (`context_id: None`) `aoide node spawn` drives (the `conduct` → `client`
/// edge this crate's `Cargo.toml` already documents for the roster core's
/// own live node probe (`who.rs`, reached via bare `session`/`--hosts`),
/// extended to this tenant) — never a re-implementation of the wire, never a shell-out
/// to the `aoide` CLI. No confirm prompt: unlike `node spawn`'s interactive
/// `--yes` gate, a manifest spec IS the operator's own standing
/// declaration — the identical posture U2's local clean-spawn already
/// takes toward a spec's own `command`, never re-asked at revival time.
/// **Which AGENT actually runs is the NODE's own configured
/// `aoide.a2a.spawnAgent`, never chosen here** — [`summon_text`]'s result
/// only ever becomes that agent's first turn (an A2A door holds it for a human
/// and never types it), the same
/// security model `handle_node_spawn`'s own doc states; `spec.agent` is
/// informational on the remote leg, unlike the local leg where it picks
/// the actual harness. Every remaining refusal — unreachable node, the
/// remote door's own gate/autogate/allow-set refusal — surfaces VERBATIM
/// into `failed[]` as `spawn_on_node`'s own `Err` text; per-spec isolation
/// holds exactly as every other row in this loop already does.
///
/// **The cwd limitation (design note, U4).** The spawn wire carries NO
/// working-directory field at all — `decide_send_action`/`do_spawn`
/// (`aoide-server::a2a`) take only a prompt and the pre-configured
/// `spawn_agent` executable, nothing else — so `spec.dir` cannot be pushed
/// onto the node through this call; it is not silently dropped so much as
/// never representable on this wire version. A spec wanting a specific
/// directory on the node must say so inside its own `command`
/// (`git -C <absolute path on the node> …`) — an honest limitation, never
/// a guessed `--cwd` the wire has nowhere to carry. Adding a wire field is
/// a LATER phase's job: the fleet's doors run older binaries this phase
/// must stay compatible with, so the wire itself is never touched here.
fn summon_remote(
    spec: &aoide_storage::manifest::SessionSpec,
    resurrected: &mut Vec<serde_json::Value>,
    failed: &mut Vec<serde_json::Value>,
    changed: &mut Vec<String>,
) {
    let (node, text) = match summon_check(spec) {
        Ok(checked) => checked,
        Err(reason) => {
            failed.push(json!({
                "host": spec.host, "dir": spec.dir, "agent": spec.agent,
                "disposition": "failed", "reason": reason,
            }));
            return;
        }
    };

    match aoide_client::commands::spawn_on_node(&node, &text) {
        Ok(resp) => {
            let session_id = resp
                .get("result")
                .and_then(|r| r.get("id"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_string();
            changed.push(format!(
                "session {session_id}: summoned on node `{}` (manifest spec, agent {})",
                spec.host, spec.agent
            ));
            resurrected.push(json!({
                "sessionId": session_id,
                "host": spec.host,
                "agent": spec.agent,
                "disposition": "summoned-remote",
                "response": resp,
            }));
        }
        Err(e) => {
            failed.push(json!({
                "host": spec.host, "dir": spec.dir, "agent": spec.agent,
                "disposition": "failed",
                "reason": format!("summoning on node `{}`: {e}", spec.host),
            }));
        }
    }
}

/// The local pre-wire verdict on a remote spec, the refusals `summon_remote`
/// gives before the wire is touched: the node to summon on and the text to
/// send, or the reason the spec cannot be sent.
fn summon_check(
    spec: &aoide_storage::manifest::SessionSpec,
) -> Result<(aoide_storage::node_store::Node, String), String> {
    let nodes = aoide_storage::node_store::load_nodes();
    let node = match nodes.iter().find(|p| p.name == spec.host) {
        Some(p) if p.verified => p.clone(),
        Some(_) => {
            return Err(format!(
                "node `{}` is registered but not paired — summoning requires a signed \
                 request from a VERIFIED node; pair first with `aoide pair \
                 <url> --name {}`",
                spec.host, spec.host
            ))
        }
        None => {
            return Err(format!(
                "host `{}` is not a registered node — `aoide node add {} <url>` (then pair it) first",
                spec.host, spec.host
            ))
        }
    };
    let text = summon_text(spec).ok_or_else(|| {
        format!(
            "no `command` given and no registered default launch for agent `{}` — add a `command` to the spec",
            spec.agent
        )
    })?;
    Ok((node, text))
}

/// The text a remote summon injects as the node's newly spawned session's
/// first turn — the spec's own `command` VERBATIM when given (unlike
/// [`clean_spawn_from_spec`]'s LOCAL argv, this never whitespace-splits it:
/// there is no argv on this wire, only one prompt string, so splitting and
/// rejoining would only risk collapsing whitespace the operator wrote on
/// purpose), else the agent's registered
/// [`aoide_protocol::agents::AgentProfile::launch`] default joined back
/// into one line (the same fallback U2's local clean-spawn already
/// applies, mirrored here since the wire wants a string, not a `Vec`).
/// `None` when neither exists — the caller turns that into a taught
/// `failed[]` entry, never a guessed prompt.
fn summon_text(spec: &aoide_storage::manifest::SessionSpec) -> Option<String> {
    // A whitespace-only command is no command — treated as absent so the
    // agent default (or the taught None) applies instead of summoning a
    // blank prompt: the string-form twin of clean_spawn's empty-argv guard.
    if let Some(command) = spec.command.as_deref().filter(|c| !c.trim().is_empty()) {
        return Some(command.to_string());
    }
    let profile = agent_profile(&spec.agent)?;
    if profile.launch.is_empty() {
        return None;
    }
    Some(profile.launch.join(" "))
}

/// The argv a clean spawn of a manifest spec launches: its `command` split on
/// whitespace, else the agent's registered default launch. A spec with neither,
/// or whose `command` splits to nothing, is refused with the reason.
fn clean_argv(spec: &aoide_storage::manifest::SessionSpec) -> Result<Vec<String>, String> {
    let argv: Vec<String> = match &spec.command {
        Some(command) => command.split_whitespace().map(str::to_string).collect(),
        None => match agent_profile(&spec.agent) {
            Some(p) if !p.launch.is_empty() => p.launch.iter().map(|s| s.to_string()).collect(),
            _ => {
                return Err(format!(
                    "no `command` given and no registered default launch for agent `{}` — add a `command` to the spec",
                    spec.agent
                ))
            }
        },
    };
    if argv.is_empty() {
        return Err("spec's `command` is empty after whitespace-splitting".to_string());
    }
    Ok(argv)
}

/// A manifest spec with no enriching ledger match: clean-spawn it windowed
/// — the spec's own `command` when given (whitespace-split into argv; no
/// shell-quote awareness, the same naive tokenizing `spawn.rs`'s own
/// `build_terminal_argv` already uses for `$AOIDE_TERMINAL`), else the
/// agent's registered [`aoide_protocol::agents::AgentProfile::launch`]
/// default. An agent with neither is a `failed[]` entry naming the gap,
/// never a guessed invocation. Reuses [`session_spawn`]'s own windowed
/// path — never a forked launch mechanism.
fn clean_spawn_from_spec(
    door: aoide_protocol::Door,
    spec: &aoide_storage::manifest::SessionSpec,
    dir: &str,
    resurrected: &mut Vec<serde_json::Value>,
    failed: &mut Vec<serde_json::Value>,
    changed: &mut Vec<String>,
) {
    let argv = match clean_argv(spec) {
        Ok(argv) => argv,
        Err(reason) => {
            failed.push(json!({
                "host": spec.host, "dir": spec.dir, "agent": spec.agent,
                "disposition": "failed", "reason": reason,
            }));
            return;
        }
    };

    let mut flags = BTreeMap::new();
    flags.insert("agent".to_string(), spec.agent.clone());
    flags.insert("windowed".to_string(), "true".to_string());
    flags.insert("cwd".to_string(), dir.to_string());
    let out = launch_session(&Invocation { path: vec!["spawn".to_string()], args: argv, flags, door }, None);
    if out.status != Status::Ok {
        failed.push(json!({
            "host": spec.host, "dir": spec.dir, "agent": spec.agent,
            "disposition": "failed", "reason": out.message,
        }));
        return;
    }
    let session_id = out
        .data
        .as_ref()
        .and_then(|d| d.get("sessionId"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string();
    let registered = out
        .data
        .as_ref()
        .and_then(|d| d.get("registered"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    changed.push(format!(
        "session {session_id}: clean-spawned from manifest spec ({}){}",
        spec.agent,
        if registered { "" } else { " (not yet registered)" }
    ));
    resurrected.push(json!({
        "sessionId": session_id,
        "agent": spec.agent,
        "registered": registered,
        "disposition": "clean-spawned",
    }));
}

/// One manifest spec's dry-run row and its plan line. Each verdict comes from
/// the helper the launch uses for that spec: `summon_check` for a non-local
/// host; `resolve_spec_dir`, `enrichment`, `resolve_candidate` and `clean_argv`
/// for a local one. Nothing is spawned or sent.
fn plan_spec(
    root: &Path,
    spec: &aoide_storage::manifest::SessionSpec,
    this_host: &str,
    ledger: &[aoide_storage::ledger::LedgerEntry],
) -> (serde_json::Value, String) {
    let label = if spec.host == this_host {
        format!("{} ({})", spec.dir, spec.agent)
    } else {
        format!("{} {} ({})", spec.host, spec.dir, spec.agent)
    };
    let mut row = json!({ "host": spec.host, "dir": spec.dir, "agent": spec.agent, "command": spec.command });
    if spec.host != this_host {
        return match summon_check(spec) {
            Ok((_, text)) => {
                row["disposition"] = json!("summoned-remote");
                row["text"] = json!(text);
                (row, format!("{label}: summon `{text}`"))
            }
            Err(reason) => not_launched(row, &label, "failed", reason),
        };
    }
    let dir = match aoide_storage::manifest::resolve_spec_dir(root, &spec.dir) {
        Ok(dir) => dir,
        Err(reason) => return not_launched(row, &label, "failed", reason),
    };
    let dir_str = dir.to_string_lossy().into_owned();
    row["cwd"] = json!(dir_str);
    if let Some(entry) = enrichment(ledger, &dir_str, &spec.agent) {
        row["resumedFrom"] = json!(entry.session_id);
        if let Err(refusal) = dry_verdict(&resolve_candidate(entry.clone())) {
            if let Some((key, id)) = refusal.named {
                row[key] = json!(id);
            }
            return not_launched(row, &label, "skipped", refusal.reason);
        }
        let restore = restore_text(entry.restore.as_ref());
        row["restoreText"] = json!(restore);
        row["disposition"] = json!("revived-from-ledger");
        return (row, format!("{label}: revive {}, {restore}", entry.session_id));
    }
    match clean_argv(spec) {
        Ok(argv) => {
            row["disposition"] = json!("clean-spawned");
            row["argv"] = json!(argv);
            (row, format!("{label}: clean spawn `{}`", shell_join(&argv)))
        }
        Err(reason) => not_launched(row, &label, "failed", reason),
    }
}

/// A dry-run row the launch would fail or skip, with its reason and plan line.
fn not_launched(
    mut row: serde_json::Value,
    label: &str,
    disposition: &str,
    reason: String,
) -> (serde_json::Value, String) {
    row["disposition"] = json!(disposition);
    row["reason"] = json!(reason);
    (row, format!("{label}: {disposition}, {reason}"))
}

/// `resurrect` with no flag, as a dry run: one row per manifest spec, each
/// decided as `resurrect_from_manifest` decides it (see [`plan_spec`]). The
/// header and each line are spelled as [`dry_run_listing`]'s are. Nothing is
/// spawned, sent or held.
fn dry_run_manifest(inv: &Invocation, root: &Path, manifest: &aoide_storage::manifest::Manifest) -> Outcome {
    let cmd = "resurrect";
    let this_host = aoide_storage::display::local_host_name();
    let ledger = match aoide_storage::ledger::read_ledger() {
        Ok(v) => v,
        Err(e) => return stage_error(cmd, e.to_string()),
    };
    let (candidates, lines): (Vec<serde_json::Value>, Vec<String>) = manifest
        .sessions
        .iter()
        .map(|spec| plan_spec(root, spec, &this_host, &ledger))
        .unzip();
    let message = std::iter::once(format!("manifest at `{}`", root.display()))
        .chain(lines)
        .map(|l| super::visible_held(&l))
        .collect::<Vec<_>>()
        .join("\n");
    audit_resurrect(inv, "dry-run", &message);
    Outcome::ok(cmd, message).with_data(json!({
        "manifestRoot": root.to_string_lossy(),
        "dryRun": true,
        "candidates": candidates,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::testutil::*;
    use super::super::model::write_stage;

    fn set_ledger(entries: &[aoide_storage::ledger::LedgerEntry]) {
        for e in entries {
            aoide_storage::ledger::append_ledger_entry(e).unwrap();
        }
    }

    /// A node registered via the legacy `node add` escape, never paired —
    /// same `verified: false` shape `send.rs`'s own `test_node` fixture
    /// uses, named here for the summon tests' own local-refusal case.
    fn unpaired_node(name: &str) -> aoide_storage::node_store::Node {
        aoide_storage::node_store::Node {
            name: name.to_string(),
            url: "http://127.0.0.1:9/".to_string(),
            token_file: None,
            bearer_secret: None,
            hub: false,
            pubkey: None,
            verified: false,
            grants: aoide_storage::node_store::Grants::new(),
            narrowed: aoide_storage::node_store::Grants::new(),
            via: None,
            added_at: "2026-08-27T00:00:00Z".to_string(),
        }
    }

    /// A `verified: true` node at the given `url` — enough for
    /// `summon_remote`'s local gate to pass and `spawn_on_node`'s own
    /// signing to proceed (signing only needs THIS instance's own identity,
    /// never the node's `pubkey` — `sign_headers_for_node`'s own doc), so
    /// no real pairing ceremony is needed to exercise the wire.
    fn verified_node(name: &str, url: &str) -> aoide_storage::node_store::Node {
        aoide_storage::node_store::Node {
            verified: true,
            url: url.to_string(),
            ..unpaired_node(name)
        }
    }

    /// Count audit-log lines whose `command` tag is `"resurrect"` — the
    /// EXACT check the headline invariant needs ("exactly one line per
    /// invocation", review round 1), not a `contains()` scan that would
    /// also pass on a log carrying two lines, or a stray substring inside
    /// some OTHER command's own message.
    fn count_resurrect_audit_lines(log_path: &std::path::Path) -> usize {
        std::fs::read_to_string(log_path)
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .filter(|v| v.get("command").and_then(serde_json::Value::as_str) == Some("resurrect"))
            .count()
    }

    fn ledger_entry(
        session_id: &str,
        agent: &str,
        cwd: &str,
        ended_at: &str,
    ) -> aoide_storage::ledger::LedgerEntry {
        aoide_storage::ledger::LedgerEntry {
            v: 0,
            session_id: session_id.to_string(),
            enduring_agent_id: None,
            project: None,
            agent: agent.to_string(),
            harness_session_id: Some(session_id.to_string()),
            cwd: cwd.to_string(),
            title: None,
            petname: None,
            started_at: "2026-08-20T00:00:00Z".to_string(),
            ended_at: ended_at.to_string(),
            resumed_from: None,
            origin: None,
            restore: None,
            log_path: None,
        }
    }

    /// Same fixture as `ledger_entry`, with an explicit `restore` block —
    /// the terminal-arm tests need to control it directly rather than
    /// always getting `None`.
    fn ledger_entry_with_restore(
        session_id: &str,
        agent: &str,
        cwd: &str,
        ended_at: &str,
        restore: Option<RestoreSnapshot>,
    ) -> aoide_storage::ledger::LedgerEntry {
        aoide_storage::ledger::LedgerEntry {
            restore,
            log_path: None,
            ..ledger_entry(session_id, agent, cwd, ended_at)
        }
    }

    /// `ledger_entry` carrying a `logPath`, built through its JSON line so the
    /// fixture has the shape `ledger_session_exit` writes.
    fn ledger_entry_with_log(
        session_id: &str,
        agent: &str,
        cwd: &str,
        log_path: Option<&str>,
    ) -> aoide_storage::ledger::LedgerEntry {
        let mut line =
            serde_json::to_value(ledger_entry(session_id, agent, cwd, "2026-08-20T01:00:00Z")).unwrap();
        line["logPath"] = log_path.map_or(serde_json::Value::Null, |p| json!(p));
        serde_json::from_value(line).unwrap()
    }

    /// Common env scaffolding every test below needs: an isolated stage +
    /// state dir, a registered project anchored at that dir. Returns the
    /// project's own absolute path (also the anchor every ledger fixture
    /// entry's `cwd` should use).
    fn setup(tag: &str) -> (std::path::PathBuf, String) {
        let root = unique_stage(tag);
        let stage = root.join("stage");
        let state = root.join("state");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("AOIDE_STATE_DIR", &state);
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        std::env::set_var("AOIDE_AUDIT_LOG", root.join("log"));
        let proj_path = root.to_str().unwrap().to_string();
        // `project add` is DAEMON-OWNED now (`manage.rs`'s `local_daemon`):
        // a `Door::Cli` invocation would try to forward to `aoided`, which
        // nothing here is running — stamp `Door::Daemon` to exercise the
        // local path directly, same as `manage.rs`'s own handler tests.
        let mut inv = invocation(&["project", "add"], &["proj", &proj_path]);
        inv.door = aoide_protocol::Door::Daemon;
        let out = crate::graph::project_add(&inv);
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        (root, proj_path)
    }

    // A full windowed-spawn SUCCESS (a real `conduct` registering a real
    // agent under a real-if-nulled pty) is deliberately NOT exercised here —
    // `spawn.rs`'s own module doc draws this exact line: "the LIVE gate (a
    // real terminal opening under the compositor) is the orchestrator's and
    // the User's, never this crate's tests." What IS this crate's job —
    // that a resurrected session always mints a FRESH id, and that
    // `resumedFrom` actually lands on the record once one exists — is
    // covered directly below and in `session_store.rs`'s own
    // `stamp_resumed_from` tests, with no process ever spawned.

    #[test]
    fn mint_resurrected_id_never_reuses_the_ledger_id() {
        // Same `<command>-<pid>-<unixts>` shape (and the same second-granularity
        // caveat) as `spawn`'s/`conduct`'s own id minting — this only
        // asserts what P-D8 actually needs: it is never the OLD ledger id.
        for old_id in ["ledger-old-1", "resurrect-1-1"] {
            let minted = mint_resurrected_id();
            assert_ne!(minted, old_id, "a resurrected session must never reuse the ledger id");
            assert!(minted.starts_with("resurrect-"), "id: {minted}");
        }
    }

    /// The terminal arm (P-C6): a `restore`-bearing `shell` entry has no
    /// harness profile at all (`agent_profile("shell")` is `None`, same as
    /// any unregistered name) — the harness arm skips it exactly like
    /// `a_no_resume_args_harness_is_skipped_with_a_taught_message` proves for
    /// an unknown harness, but the terminal arm picks it up right after,
    /// resolving `[<login shell>, "-l"]` rather than leaving it skipped.
    #[test]
    fn resolve_candidate_terminal_arm_resolves_a_restore_bearing_shell_entry_the_harness_arm_skips() {
        let entry = ledger_entry_with_restore(
            "ledger-terminal",
            "shell",
            "/home/khoa/Aoide",
            "2026-08-20T01:00:00Z",
            Some(RestoreSnapshot { cwd: Some("/home/khoa/Aoide".into()), idle: true, argv: None }),
        );
        let candidate = resolve_candidate(entry);
        let argv = candidate.resume_argv.expect("the terminal arm must resolve a restore-bearing shell entry");
        assert_eq!(argv.len(), 2, "argv: {argv:?}");
        assert_eq!(argv[1], "-l", "argv: {argv:?}");
        assert!(!argv[0].is_empty(), "the login shell must not resolve to an empty string");
    }

    /// A `shell` entry with NO `restore` block (predating P-C5, or a harness
    /// this box has never verified) must still hit the pre-existing taught
    /// skip — the terminal arm's whole gate is `Some(restore)`, never bare
    /// `agent == "shell"`.
    #[test]
    fn resolve_candidate_restore_less_shell_entry_still_hits_the_taught_skip() {
        let entry = ledger_entry_with_restore("ledger-no-restore", "shell", "/home/khoa/Aoide", "2026-08-20T01:00:00Z", None);
        let candidate = resolve_candidate(entry);
        assert_eq!(
            candidate.resume_argv.unwrap_err(),
            "harness `shell` has no verified resume argv — skipped rather than typing a guessed invocation",
            "a restore-less shell entry must still be skipped with the taught line"
        );
    }

    /// R1: a busy session's recorded argv is TYPED and never submitted. The
    /// flag map carries `--yes` and no `submit`, and the bytes the real send
    /// writer puts on the pty are the line itself, with no submit key after it.
    #[cfg(unix)]
    #[test]
    fn a_busy_argv_is_typed_and_never_submitted() {
        let restore = busy_restore(&["nvim", "notes.md"]);
        let inv = restore_delivery(aoide_protocol::Door::Daemon, "new-id", &restore)
            .expect("a busy argv that passes the refusals is typed");
        assert_eq!(inv.flags.get("yes").map(String::as_str), Some("true"));
        assert!(!inv.flag_present("submit"), "a restore never submits: {:?}", inv.flags);
        assert_eq!(inv.flags.get("from").map(String::as_str), Some("new-id"));
        assert_eq!(inv.args, vec!["nvim notes.md".to_string()]);

        let (mut tx, mut rx) = std::os::unix::net::UnixStream::pair().expect("a socket pair");
        crate::graph::send::write_delivery(
            &mut tx,
            inv.args[0].as_bytes(),
            inv.flag_present("submit"),
            "\r",
            std::time::Duration::ZERO,
        )
        .expect("the line is written");
        drop(tx);
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut rx, &mut bytes).expect("the socket reads to its end");
        assert_eq!(bytes, b"nvim notes.md", "the line is typed, and no submit key follows it");
    }

    /// R1 at the prompt: a busy argv typed into `bash -i` on a pty sits there
    /// unrun until a human presses Enter. The sentinel is the result of
    /// `expr 6 '*' 7`, which the typed text does not contain.
    #[cfg(unix)]
    #[test]
    fn a_busy_argv_waits_at_the_prompt_until_a_human_presses_enter() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let restore = busy_restore(&["expr", "6", "*", "7"]);
        let line = restore_delivery(aoide_protocol::Door::Daemon, "new-id", &restore)
            .expect("a busy argv that passes the refusals is typed")
            .args[0]
            .clone();
        let args: Vec<String> = ["--norc", "--noprofile", "-i"].iter().map(|s| s.to_string()).collect();
        let (mut child, mut pty) = crate::graph::pty::spawn_on_pty("bash", &args, "resurrect-r1", None, None)
            .expect("bash starts on a pty");
        pty.write(line.as_bytes());
        let before = read_pty_until(&mut pty, std::time::Duration::from_millis(1500), "42");
        pty.write(b"\r");
        let after = read_pty_until(&mut pty, std::time::Duration::from_secs(10), "42");
        child.kill();
        let _ = child.wait_for(std::time::Duration::from_secs(3));
        pty.close();
        assert!(!before.contains("42"), "the typed line ran before Enter: {before:?}");
        assert!(after.contains("42"), "Enter did not run the typed line: {after:?}");
    }

    /// Reads what a pty prints for up to `budget`, stopping early at `stop`.
    #[cfg(unix)]
    fn read_pty_until(pty: &mut crate::graph::pty::Pty, budget: std::time::Duration, stop: &str) -> String {
        let mut out = Vec::new();
        let mut buf = [0u8; 8192];
        let deadline = std::time::Instant::now() + budget;
        while std::time::Instant::now() < deadline && !String::from_utf8_lossy(&out).contains(stop) {
            while pty.has_output() {
                match pty.read(&mut buf) {
                    Ok(n) if n > 0 => out.extend_from_slice(&buf[..n]),
                    _ => break,
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    /// An idle restore constructs no send at all — a bare cwd restore is the
    /// whole answer. Nothing typed by the old session is ever replayed.
    #[test]
    fn an_idle_restore_delivers_nothing_it_restores_the_cwd_alone() {
        let restore = RestoreSnapshot { cwd: Some("/home/khoa/Aoide".into()), idle: true, argv: None };
        assert!(restore_delivery(aoide_protocol::Door::Daemon, "new-id", &restore).is_none());
    }

    /// The `restoreDelivery` vocabulary and the order of its effects, one row
    /// per label: nothing is waited on or sent for an unregistered, refused or
    /// empty restore, and nothing is sent to a target that is not ready.
    #[test]
    fn a_restore_step_is_labelled_and_runs_its_effects_only_when_due() {
        use std::cell::Cell;
        let idle = RestoreSnapshot { cwd: Some("/p".into()), idle: true, argv: None };
        let busy = busy_restore(&["nvim", "notes.md"]);
        let sudo = busy_restore(&["sudo", "reboot"]);
        let ok = || Ok(Landed::Typed);
        // (registered, restore, readiness, send result, label, waited, sent)
        let rows: Vec<(bool, Option<&RestoreSnapshot>, Ready, Result<Landed, String>, &str, bool, bool)> = vec![
            (false, Some(&busy), Ready::Verified, ok(), "skipped-unregistered", false, false),
            (true, Some(&sudo), Ready::Verified, ok(), "refused: a privilege program", false, false),
            (true, Some(&idle), Ready::Verified, ok(), "none", false, false),
            (true, None, Ready::Verified, ok(), "none", false, false),
            (true, Some(&busy), Ready::NotReady, ok(), "not-ready", true, false),
            (true, Some(&busy), Ready::Verified, Err("no socket".into()), "failed: no socket", true, true),
            (true, Some(&busy), Ready::Verified, ok(), "command", true, true),
            (true, Some(&busy), Ready::Unverified, ok(), "command-unverified", true, true),
            (true, Some(&busy), Ready::Verified, Ok(Landed::Held), "held-pending", true, true),
        ];
        for (registered, restore, ready, sent, label, waited, did_send) in rows {
            let (waits, sends) = (Cell::new(false), Cell::new(false));
            let step = restore_step(
                registered,
                restore,
                aoide_protocol::Door::Daemon,
                "new-id",
                || {
                    waits.set(true);
                    ready
                },
                |inv| {
                    assert_eq!(inv.args, vec!["nvim notes.md".to_string()]);
                    sends.set(true);
                    sent
                },
            );
            assert_eq!(step.label(), label);
            assert_eq!((waits.get(), sends.get()), (waited, did_send), "effects for {label}");
        }
    }

    /// The orchestrator ruling (open knob 5): a recorded foreground of
    /// `sudo …` re-execs nothing and delivers nothing — only the cwd
    /// restores. Covers both the privilege check directly and the delivery
    /// decision that consults it.
    #[test]
    fn sudo_foreground_is_never_re_typed() {
        assert!(runs_a_privilege_program(&["sudo".to_string(), "reboot".to_string()]));
        assert!(runs_a_privilege_program(&["/usr/bin/sudo".to_string(), "-i".to_string()]), "basename match must see through a full path");
        assert!(!runs_a_privilege_program(&["sudo-ish".to_string()]), "must match the exact basename, nothing cleverer");
        assert!(!runs_a_privilege_program(&[]));

        let restore = RestoreSnapshot {
            cwd: Some("/home/khoa/Aoide".into()),
            idle: false,
            argv: Some(vec!["sudo".to_string(), "systemctl".to_string(), "restart".to_string(), "aoided".to_string()]),
        };
        assert!(
            restore_delivery(aoide_protocol::Door::Daemon, "new-id", &restore).is_none(),
            "a recorded sudo foreground must never be re-exec'd"
        );
    }

    /// A recorded shell-form argv (`sh -c …`, `bash -lc …`) is never re-typed.
    /// Its command sits inside the quotes, where no program check can see a
    /// `sudo`, and the join that flattens it would mangle it. Its session gets
    /// its cwd only, as an idle one does.
    #[test]
    fn a_recorded_shell_command_string_is_never_re_typed() {
        for argv in [
            vec!["sh", "-c", "sudo apt install x"],
            vec!["sh", "-c", "sleep 30; true"],
            vec!["bash", "-lc", "make"],
        ] {
            let restore = RestoreSnapshot {
                cwd: Some("/home/khoa/Aoide".into()),
                idle: false,
                argv: Some(argv.iter().map(|s| s.to_string()).collect()),
            };
            assert!(
                restore_delivery(aoide_protocol::Door::Daemon, "new-id", &restore).is_none(),
                "{argv:?} must not be re-typed"
            );
        }
    }

    fn busy_restore(argv: &[&str]) -> RestoreSnapshot {
        RestoreSnapshot {
            cwd: Some("/home/khoa/Aoide".into()),
            idle: false,
            argv: Some(argv.iter().map(|s| s.to_string()).collect()),
        }
    }

    /// F1: a control byte, an invisible format mark, U+FFFD (a lossy decode of
    /// non-UTF-8 bytes) or a newline in any word is never re-typed. Quoting
    /// does not contain them: readline reads `^A` as an edit key, and a newline
    /// submits the line early.
    #[test]
    fn a_control_or_invisible_byte_in_a_recorded_argv_is_never_re_typed() {
        for word in ["\u{1}echo OUT", "a\u{1b}b", "a\u{7f}b", "a\u{202e}b", "a\u{fffd}b", "a\nb", "a\rb", "a\tb"] {
            let restore = busy_restore(&["echo", word]);
            assert!(
                restore_delivery(aoide_protocol::Door::Daemon, "new-id", &restore).is_none(),
                "{word:?} must not be re-typed"
            );
        }
    }

    /// F1: the re-typed line is capped at 4096 bytes, and refused above it.
    #[test]
    fn a_re_typed_line_is_capped_at_4096_bytes() {
        let fits = "x".repeat(4091);
        let over = "x".repeat(4092);
        let line = |word: &str| {
            restore_delivery(aoide_protocol::Door::Daemon, "new-id", &busy_restore(&["echo", word]))
                .map(|inv| inv.args[0].len())
        };
        assert_eq!(line(&fits), Some(4096), "a line of exactly 4096 bytes is typed");
        assert_eq!(line(&over), None, "a line of 4097 bytes is refused");
    }

    /// F1 at the keystroke level: `bash --norc --noprofile -i` on a real pty,
    /// fed what `restore_delivery` would type for the hostile argv. The
    /// refusal means nothing is typed, so the payload never runs; the canary
    /// line proves the shell read input after the (absent) argv line.
    #[cfg(unix)]
    #[test]
    fn a_hostile_argv_never_reaches_an_interactive_shell_as_keystrokes() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let restore = busy_restore(&["echo", "\u{1}echo OUT$((6*7))ZZ #"]);
        let typed = restore_delivery(aoide_protocol::Door::Daemon, "new-id", &restore).map(|inv| inv.args[0].clone());
        assert!(typed.is_none(), "the hostile argv must be refused, not typed");

        let args: Vec<String> = ["--norc", "--noprofile", "-i"].iter().map(|s| s.to_string()).collect();
        let (mut child, mut pty) = crate::graph::pty::spawn_on_pty("bash", &args, "resurrect-f1", None, None)
            .expect("bash starts on a pty");
        if let Some(line) = typed {
            pty.write(format!("{line}\r").as_bytes());
        }
        pty.write(b"echo CANARY$((1+1))\r");
        let mut out = Vec::new();
        let mut buf = [0u8; 8192];
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < deadline && !String::from_utf8_lossy(&out).contains("CANARY2") {
            while pty.has_output() {
                match pty.read(&mut buf) {
                    Ok(n) if n > 0 => out.extend_from_slice(&buf[..n]),
                    _ => break,
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        child.kill();
        let _ = child.wait_for(std::time::Duration::from_secs(3));
        pty.close();
        let text = String::from_utf8_lossy(&out).into_owned();
        assert!(text.contains("CANARY2"), "the shell never read the canary line: {text:?}");
        assert!(!text.contains("OUT42ZZ"), "the recorded argv ran as keystrokes: {text:?}");
    }

    /// F2: a leading `NAME=value` word is a shell assignment, and it must not
    /// hide the `sudo` or the shell behind it from the refusals.
    #[test]
    fn an_assignment_prefix_never_hides_a_privilege_or_a_shell_form() {
        for argv in [
            vec!["A=1", "sudo", "-n", "true"],
            vec!["A=1", "B=2", "bash", "-c", "x"],
            vec!["env", "A=1", "sudo", "x"],
        ] {
            let restore = busy_restore(&argv);
            assert!(
                restore_delivery(aoide_protocol::Door::Daemon, "new-id", &restore).is_none(),
                "{argv:?} must not be re-typed"
            );
        }
    }

    /// F6: a privilege program is never re-typed, bare or wrapped: `doas`,
    /// `su`,
    /// `pkexec` and `run0` ask for a password on the pty, and an interactive
    /// `doas bash` or `su` would hand a root shell to an unattended line.
    /// `doas apt x` is refused by the shell-form test already; it is pinned
    /// here so the refusal cannot quietly go.
    #[test]
    fn a_privilege_program_is_never_re_typed() {
        for argv in [
            vec!["doas", "bash"],
            vec!["doas", "-u", "root", "zsh"],
            vec!["su"],
            vec!["su", "bash"],
            vec!["su", "-l", "khoa"],
            vec!["pkexec", "x"],
            vec!["run0", "x"],
            vec!["doas", "apt", "x"],
            vec!["env", "pkexec", "x"],
            vec!["nice", "su", "-l"],
            vec!["sudoedit", "x"],
            vec!["runuser", "-u", "root", "bash"],
            vec!["setpriv", "--reuid=0", "bash"],
            vec!["chroot", "/x", "sh"],
            vec!["sg", "wheel", "x"],
            vec!["newgrp", "wheel"],
            vec!["dzdo", "x"],
            vec!["ksu", "x"],
            vec!["busybox", "su", "-l", "khoa"],
        ] {
            let restore = busy_restore(&argv);
            assert!(
                restore_delivery(aoide_protocol::Door::Daemon, "new-id", &restore).is_none(),
                "{argv:?} must not be re-typed"
            );
        }
    }

    /// The re-exec line re-parses to the recorded argv: a word with a space, a
    /// quote, a glob or a `$` stays one word, and a plain word stays bare. It
    /// is checked through `sh -c`, which is the POSIX re-parse only. An
    /// interactive shell's readline reads control bytes as edit keys, which
    /// this test does not cover; `a_hostile_argv_never_reaches_an_interactive_
    /// shell_as_keystrokes` covers that, and the refusal keeps such an argv out.
    #[cfg(unix)]
    #[test]
    fn a_re_exec_line_re_parses_to_the_recorded_argv() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let argv: Vec<String> = ["grep", "a b", "f", "it's", "x*y", "$HOME", ""].iter().map(|s| s.to_string()).collect();
        let restore = RestoreSnapshot {
            cwd: Some("/home/khoa/Aoide".into()),
            idle: false,
            argv: Some(argv.clone()),
        };
        let inv = restore_delivery(aoide_protocol::Door::Daemon, "new-id", &restore)
            .expect("a working restore with argv must construct a send");
        let line = &inv.args[0];
        assert_eq!(line, "grep 'a b' f 'it'\\''s' 'x*y' '$HOME' ''");
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("set -- {line}; for a in \"$@\"; do printf '[%s]\\n' \"$a\"; done"))
            .output()
            .expect("sh runs");
        let words: Vec<String> = String::from_utf8(out.stdout)
            .unwrap()
            .lines()
            .map(|l| l[1..l.len() - 1].to_string())
            .collect();
        assert_eq!(words, argv, "the typed line must re-parse to the recorded argv");
    }

    #[test]
    fn a_no_resume_args_harness_is_skipped_with_a_taught_message() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG"]);
        let (root, proj_path) = setup("resurrect-skip");

        set_ledger(&[ledger_entry(
            "ledger-unknown-harness",
            "no-such-harness",
            &proj_path,
            "2026-08-20T01:00:00Z",
        )]);
        // Bare selection is undying-set-driven (P-C4) — mark the entry so it
        // is even a candidate; the point of this test is the harness skip,
        // not the selection width.
        let mut undying = Vec::new();
        aoide_storage::undying::set_undying(&mut undying, "ledger-unknown-harness", true);
        aoide_storage::undying::save_undying(&undying).unwrap();

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "proj")]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let data = out.data.as_ref().unwrap();
        assert_eq!(data["resurrected"].as_array().unwrap().len(), 0);
        let skipped = data["skipped"].as_array().unwrap();
        assert_eq!(skipped.len(), 1, "data: {data}");
        assert!(
            skipped[0]["reason"].as_str().unwrap().contains("no-such-harness"),
            "the skip reason must name the harness: {}",
            skipped[0]["reason"]
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_windowed_spawn_failure_degrades_gracefully_instead_of_erroring_the_command() {
        // The headless-host case (P-D8's decided trigger): no $AOIDE_TERMINAL
        // set. `session_spawn`'s own taught error fires — this proves it is
        // folded into `failed`, never turned into a hard `Outcome::error`, so
        // the daemon's boot-time trigger can call this in a loop without ever
        // treating a display-less box as a failure worth crashing a tick over.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_TERMINAL",
            "WAYLAND_DISPLAY",
            "DISPLAY",
        ]);
        let (root, proj_path) = setup("resurrect-headless");
        std::env::remove_var("AOIDE_TERMINAL");
        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::remove_var("DISPLAY");

        set_ledger(&[ledger_entry("ledger-old-2", "claude", &proj_path, "2026-08-20T01:00:00Z")]);
        // Bare selection is undying-set-driven (P-C4) — mark the entry so it
        // is even a candidate; the point of this test is the failure
        // handling, not the selection width.
        let mut undying = Vec::new();
        aoide_storage::undying::set_undying(&mut undying, "ledger-old-2", true);
        aoide_storage::undying::save_undying(&undying).unwrap();

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "proj")]));
        assert_eq!(
            out.status,
            aoide_protocol::output::Status::Ok,
            "a per-candidate spawn failure must degrade gracefully, never error the command: {}",
            out.message
        );
        let data = out.data.as_ref().unwrap();
        assert_eq!(data["resurrected"].as_array().unwrap().len(), 0);
        let failed = data["failed"].as_array().unwrap();
        assert_eq!(failed.len(), 1, "data: {data}");
        assert!(
            failed[0]["reason"].as_str().unwrap().contains("AOIDE_TERMINAL"),
            "reason: {}",
            failed[0]["reason"]
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A successful resurrect of a UNDYING old id transfers the mark: the
    /// new id ends up undying, the old id does not, and an unrelated undying
    /// id already in the set is left exactly as it was (P-C3, durable-
    /// sessions plan). `AOIDE_TERMINAL=true` is enough to make the windowed
    /// spawn itself succeed (`Status::Ok`) without a real terminal — `true`
    /// exits 0 the instant it's exec'd; the point of this test is the undying
    /// transfer, not registration, which `resurrect_one` never gates it on.
    /// GATED on Unix with its reason: the revived session is `conduct
    /// --headless`, whose fixture is a POSIX child (`sh -c`, a `#!/bin/sh` shim
    /// and a mode) that `CreateProcess` cannot run (see
    /// `a_node_origin_ledger_entry_never_derails_an_ordinary_resurrect`'s gate).
    #[cfg(unix)]
    #[test]
    fn a_successful_resurrect_transfers_the_undying_mark_from_old_to_new() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_TERMINAL",
            "WAYLAND_DISPLAY",
            "DISPLAY",
        ]);
        let (root, proj_path) = setup("resurrect-undying-transfer");
        std::env::set_var("AOIDE_TERMINAL", "true");
        std::env::set_var("WAYLAND_DISPLAY", "wayland-0");

        set_ledger(&[ledger_entry("ledger-undying", "claude", &proj_path, "2026-08-20T01:00:00Z")]);

        let mut undying = Vec::new();
        aoide_storage::undying::set_undying(&mut undying, "ledger-undying", true);
        aoide_storage::undying::set_undying(&mut undying, "unrelated-id", true);
        aoide_storage::undying::save_undying(&undying).unwrap();

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "proj")]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let data = out.data.as_ref().unwrap();
        let resurrected = data["resurrected"].as_array().unwrap();
        assert_eq!(resurrected.len(), 1, "data: {data}");
        let new_id = resurrected[0]["sessionId"].as_str().unwrap().to_string();

        let undying = aoide_storage::undying::load_undying();
        assert!(aoide_storage::undying::is_undying(&undying, &new_id), "the new id must be undying");
        assert!(
            !aoide_storage::undying::is_undying(&undying, "ledger-undying"),
            "the old id must no longer be undying"
        );
        assert!(
            aoide_storage::undying::is_undying(&undying, "unrelated-id"),
            "an unrelated undying id must be left untouched"
        );
        assert_eq!(undying.len(), 2, "exactly one id moves — the set's size is unchanged");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// G6 (LANE IDENTITY P-ID0), data-integrity half: `resurrect_one` reads
    /// `c.entry.origin` straight off the resolved candidate — this pins
    /// that `resolve_candidate` (the harness-arm/terminal-arm dispatcher
    /// every mode routes through) carries the ledger entry's `origin` into
    /// the `Candidate` unmodified, the exact value `origin_to_carry` (and
    /// hence the carry-forward line at the bottom of `resurrect_one`)
    /// consumes. Pure, no env, no spawn.
    #[test]
    fn resolve_candidate_preserves_the_ledger_entrys_origin() {
        let entry = aoide_storage::ledger::LedgerEntry {
            origin: Some("node:yomi-strix".to_string()),
            log_path: None,
            ..ledger_entry("ledger-node-origin", "claude", "/home/khoa/Aoide", "2026-08-20T01:00:00Z")
        };
        let candidate = resolve_candidate(entry);
        assert_eq!(candidate.entry.origin.as_deref(), Some("node:yomi-strix"));
    }

    /// G6 MUST-FIX (LANE IDENTITY P-ID0, review round 1): THE real
    /// assertion — a `node:*` ledger origin is REFUSED, never carried,
    /// because `state/session-ledger.jsonl` is unsealed and a local
    /// resurrect has no door behind it to authenticate that shape. A
    /// non-node (local-class) origin still carries forward normally, and
    /// no ledger origin at all carries nothing — `origin_to_carry` is pure,
    /// so this is provable without a spawn, a registered record, or any
    /// env at all.
    #[test]
    fn origin_to_carry_refuses_a_node_shape_and_carries_everything_else() {
        assert_eq!(origin_to_carry(Some("node:yomi-strix")), OriginCarry::RefusedNode("node:yomi-strix"));
        assert_eq!(origin_to_carry(Some("local")), OriginCarry::Carry("local"));
        assert_eq!(origin_to_carry(None), OriginCarry::Nothing);
    }

    /// G6 (LANE IDENTITY P-ID0), wiring half: a node-origin ledger entry
    /// must not derail an otherwise-ordinary resurrect (the taught refusal
    /// eprintln fires, but the invocation still succeeds cleanly) —
    /// `resurrect_one`'s `origin_to_carry` match sits right after
    /// `stamp_resumed_from`, on the SAME `AOIDE_TERMINAL=true` fixture that
    /// never actually registers a record (see the undying-transfer test
    /// above), so this exercises the `RefusedNode` arm specifically rather
    /// than only proving it in isolation. The refusal DECISION itself is
    /// the real assertion above; whether a carried (non-node) value LANDS
    /// on a real record is `stamp_origin`'s own contract, proven directly
    /// in `session_store.rs`'s `stamp_origin_lands_the_field_and_never_
    /// restages_graph_json` — a real registered windowed spawn is the live
    /// gate's job, per this module's own doc (top of file), never this
    /// crate's.
    /// GATED on Unix with its reason: a resurrect revives the session by
    /// spawning `conduct --headless`, whose fixture here is a POSIX `sh -c`
    /// child (a shebang script and a mode), which `CreateProcess` cannot run.
    /// The detached, NON-PTY spawn is native, and its cover
    /// is `spawn`'s four green tests, named in their own gate.
    #[cfg(unix)]
    #[test]
    fn a_node_origin_ledger_entry_never_derails_an_ordinary_resurrect() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_TERMINAL",
            "WAYLAND_DISPLAY",
            "DISPLAY",
        ]);
        let (root, proj_path) = setup("resurrect-origin-carry");
        std::env::set_var("AOIDE_TERMINAL", "true");
        std::env::set_var("WAYLAND_DISPLAY", "wayland-0");

        let entry = aoide_storage::ledger::LedgerEntry {
            origin: Some("node:yomi-strix".to_string()),
            log_path: None,
            ..ledger_entry("ledger-node-origin", "claude", &proj_path, "2026-08-20T01:00:00Z")
        };
        set_ledger(&[entry]);

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "proj"), ("all", "true")]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let data = out.data.as_ref().unwrap();
        let resurrected = data["resurrected"].as_array().unwrap();
        assert_eq!(resurrected.len(), 1, "data: {data}");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The failure half of the same rule: a spawn that never even reaches
    /// `Status::Ok` (the headless-host taught error, no `$AOIDE_TERMINAL`)
    /// must leave the old id undying, so the next sweep retries it.
    #[test]
    fn a_failed_resurrect_leaves_the_old_id_undying() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_TERMINAL",
            "WAYLAND_DISPLAY",
            "DISPLAY",
        ]);
        let (root, proj_path) = setup("resurrect-undying-failed");
        std::env::remove_var("AOIDE_TERMINAL");
        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::remove_var("DISPLAY");

        set_ledger(&[ledger_entry("ledger-undying-fail", "claude", &proj_path, "2026-08-20T01:00:00Z")]);

        let mut undying = Vec::new();
        aoide_storage::undying::set_undying(&mut undying, "ledger-undying-fail", true);
        aoide_storage::undying::save_undying(&undying).unwrap();

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "proj")]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        assert_eq!(out.data.as_ref().unwrap()["failed"].as_array().unwrap().len(), 1);

        let undying = aoide_storage::undying::load_undying();
        assert!(
            aoide_storage::undying::is_undying(&undying, "ledger-undying-fail"),
            "a failed resurrect must leave the old id undying so the next sweep retries it"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Re-running a resurrect against the same (append-only, so still
    /// selectable) ledger entry after it has already transferred must not
    /// mark the SECOND new id undying or touch the set again — the transfer step
    /// only fires when the old id is currently undying, and by the second
    /// call it no longer is.
    /// GATED on Unix with its reason: the revived session is `conduct
    /// --headless`, whose fixture is a POSIX child (`sh -c`, a `#!/bin/sh` shim
    /// and a mode) that `CreateProcess` cannot run (see
    /// `a_node_origin_ledger_entry_never_derails_an_ordinary_resurrect`'s gate).
    #[cfg(unix)]
    #[test]
    fn transfer_is_idempotent_when_the_pair_has_already_transferred() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_TERMINAL",
            "WAYLAND_DISPLAY",
            "DISPLAY",
        ]);
        let (root, proj_path) = setup("resurrect-undying-idempotent");
        std::env::set_var("AOIDE_TERMINAL", "true");
        std::env::set_var("WAYLAND_DISPLAY", "wayland-0");

        set_ledger(&[ledger_entry("ledger-idem", "claude", &proj_path, "2026-08-20T01:00:00Z")]);

        let mut undying = Vec::new();
        aoide_storage::undying::set_undying(&mut undying, "ledger-idem", true);
        aoide_storage::undying::save_undying(&undying).unwrap();

        let first = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "proj")]));
        assert_eq!(first.status, aoide_protocol::output::Status::Ok, "msg: {}", first.message);
        let first_new_id =
            first.data.as_ref().unwrap()["resurrected"][0]["sessionId"].as_str().unwrap().to_string();
        let after_first = aoide_storage::undying::load_undying();
        assert!(aoide_storage::undying::is_undying(&after_first, &first_new_id));
        assert!(!aoide_storage::undying::is_undying(&after_first, "ledger-idem"));

        // Bare selection no longer re-picks `ledger-idem` (P-C4): its mark
        // already moved to `first_new_id` above. `--id` is the unchanged
        // escape that narrows to one entry regardless of the mark (the
        // append-only ledger still holds the line), so it is what re-drives
        // the same candidate a second time here — the point of THIS test is
        // `resurrect_one`'s transfer idempotency, not bare-mode selection.
        // The first revival has ended by now: its claim is stale, so the
        // re-drive may launch. While that session is open, it is refused.
        let mut roster = SessionsFile::default();
        roster.sessions.push(aoide_storage::records::SessionRecord {
            session_id: first_new_id.clone(),
            state: "done".into(),
            ..Default::default()
        });
        write_stage(&sessions_path(), &roster).unwrap();
        let second = session_resurrect(&flag_invocation(
            &["resurrect"],
            &[("project", "proj"), ("id", "ledger-idem")],
        ));
        assert_eq!(second.status, aoide_protocol::output::Status::Ok, "msg: {}", second.message);
        let second_new_id =
            second.data.as_ref().unwrap()["resurrected"][0]["sessionId"].as_str().unwrap().to_string();

        let after_second = aoide_storage::undying::load_undying();
        assert!(
            !aoide_storage::undying::is_undying(&after_second, &second_new_id),
            "the old id was no longer undying, so nothing transfers to the second new id"
        );
        assert_eq!(
            after_second, after_first,
            "a re-run transfer on an already-transferred pair changes nothing"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn no_ledger_history_for_the_project_is_an_ok_no_op() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG"]);
        let (root, _proj_path) = setup("resurrect-empty");

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "proj")]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        assert_eq!(out.data.as_ref().unwrap()["resurrected"].as_array().unwrap().len(), 0);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn unknown_project_is_an_error() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG"]);
        let (root, _proj_path) = setup("resurrect-unknown-project");

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "nope")]));
        assert_eq!(out.status, aoide_protocol::output::Status::Error);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_project_flag_is_a_usage_error() {
        // Bare mode (no flags at all) now tries a manifest walk-up FIRST
        // (U2) — chdir into a fresh, manifest-free scratch dir so this
        // stays deterministic regardless of where `cargo test` happens to
        // run from, rather than depending on the real ambient cwd's own
        // ancestry having no `.aoide/project.json` (every real ancestor of
        // a fresh temp dir is guaranteed manifest-free, the same
        // assumption `aoide_storage::manifest`'s own walk-up-none test
        // already leans on).
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let scratch = unique_stage("resurrect-no-manifest-no-flags");
        let _cwd = CwdGuard::enter(&scratch);

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[]));
        assert_eq!(out.status, aoide_protocol::output::Status::Usage);
        assert!(
            out.message.contains("--project") && out.message.contains("project.json"),
            "the taught error must name both misses: {}",
            out.message
        );

        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// The review-round-1 fix: `--id` given WITHOUT `--project` is
    /// flag-mode (one of the three flags is present), so it must NEVER
    /// attempt a manifest walk and must NEVER get the bare-mode's
    /// both-misses wording — only `require_flag`'s own, accurate
    /// `--project`-missing usage error, exactly as it was before U2 ever
    /// touched this function. No cwd/env setup needed at all: proving this
    /// doesn't even reach the manifest-walk branch is the whole point.
    #[test]
    fn id_without_project_is_the_ordinary_missing_flag_error_not_the_manifest_message() {
        let out = session_resurrect(&flag_invocation(&["resurrect"], &[("id", "some-ledger-id")]));
        assert_eq!(out.status, aoide_protocol::output::Status::Usage);
        assert!(out.message.contains("--project"), "message: {}", out.message);
        assert!(
            !out.message.contains("project.json"),
            "an --id-given invocation must get the ORDINARY missing-flag error, never the \
             manifest-miss wording (which would lie: a flag WAS given, no walk was attempted): {}",
            out.message
        );
    }

    /// Same fix, the `--all` half.
    #[test]
    fn all_without_project_is_the_ordinary_missing_flag_error_not_the_manifest_message() {
        let out = session_resurrect(&flag_invocation(&["resurrect"], &[("all", "true")]));
        assert_eq!(out.status, aoide_protocol::output::Status::Usage);
        assert!(out.message.contains("--project"), "message: {}", out.message);
        assert!(!out.message.contains("project.json"), "message: {}", out.message);
    }

    /// `--all` and `--id` are unchanged escapes (P-C4's own scope line): both
    /// widen or narrow past the undying set regardless of the mark — neither
    /// entry below is ever undying, and both still resolve.
    #[test]
    fn all_widens_to_every_anchored_entry_and_id_narrows_to_one_regardless_of_the_mark() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG"]);
        let (root, proj_path) = setup("resurrect-all-id");

        // Two resumable-shaped entries (unresolved harness, so both land in
        // `skipped` rather than needing a real spawn) — proves the SELECTION
        // width, independent of the spawn mechanics already covered above.
        // Neither is undying: --all and --id must not care.
        set_ledger(&[
            ledger_entry("ledger-a", "no-such-harness", &proj_path, "2026-08-20T01:00:00Z"),
            ledger_entry("ledger-b", "no-such-harness", &proj_path, "2026-08-20T02:00:00Z"),
        ]);

        // --all: both, undying or not.
        let out = session_resurrect(&flag_invocation(
            &["resurrect"],
            &[("project", "proj"), ("all", "true")],
        ));
        assert_eq!(out.data.as_ref().unwrap()["skipped"].as_array().unwrap().len(), 2);

        // --id: exactly the named one, not undying and not the newest.
        let out = session_resurrect(&flag_invocation(
            &["resurrect"],
            &[("project", "proj"), ("id", "ledger-a")],
        ));
        let skipped = out.data.as_ref().unwrap()["skipped"].as_array().unwrap().clone();
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0]["sessionId"], "ledger-a");

        let _ = std::fs::remove_dir_all(&root);
    }

    // ── P-C4: bare selection drives off the undying set ─────────────────────

    /// The headline case: three undying, two not undying, all five anchored to
    /// the same project — bare `--project` resurrects exactly the three.
    #[test]
    fn bare_default_resurrects_exactly_the_undying_set() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG"]);
        let (root, proj_path) = setup("resurrect-undying-set");

        set_ledger(&[
            ledger_entry("undying-1", "no-such-harness", &proj_path, "2026-08-20T01:00:00Z"),
            ledger_entry("undying-2", "no-such-harness", &proj_path, "2026-08-20T02:00:00Z"),
            ledger_entry("undying-3", "no-such-harness", &proj_path, "2026-08-20T03:00:00Z"),
            ledger_entry("not-undying-1", "no-such-harness", &proj_path, "2026-08-20T04:00:00Z"),
            ledger_entry("not-undying-2", "no-such-harness", &proj_path, "2026-08-20T05:00:00Z"),
        ]);
        let mut undying = Vec::new();
        for id in ["undying-1", "undying-2", "undying-3"] {
            aoide_storage::undying::set_undying(&mut undying, id, true);
        }
        aoide_storage::undying::save_undying(&undying).unwrap();

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "proj")]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let skipped = out.data.as_ref().unwrap()["skipped"].as_array().unwrap().clone();
        let mut ids: Vec<&str> = skipped.iter().map(|s| s["sessionId"].as_str().unwrap()).collect();
        ids.sort();
        assert_eq!(ids, vec!["undying-1", "undying-2", "undying-3"], "skipped: {skipped:?}");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A undying id still live in `sessions.json` (non-`done`) is excluded —
    /// the daemon's old per-project `has_live` skip moved down to here,
    /// per-id (P-C4's own scope line).
    #[test]
    fn bare_default_excludes_an_undying_id_still_live_in_the_roster() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG"]);
        let (root, proj_path) = setup("resurrect-undying-live");

        set_ledger(&[
            ledger_entry("undying-alive", "no-such-harness", &proj_path, "2026-08-20T01:00:00Z"),
            ledger_entry("undying-dead", "no-such-harness", &proj_path, "2026-08-20T02:00:00Z"),
        ]);
        let mut undying = Vec::new();
        aoide_storage::undying::set_undying(&mut undying, "undying-alive", true);
        aoide_storage::undying::set_undying(&mut undying, "undying-dead", true);
        aoide_storage::undying::save_undying(&undying).unwrap();

        // `undying-alive` is still in the roster, non-`done`.
        write_stage(
            &sessions_path(),
            &SessionsFile {
                schema_version: "0".into(),
                sessions: vec![session("undying-alive", &proj_path, "working", "2026-08-20T01:00:00Z", None)],
            },
        )
        .unwrap();

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "proj")]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let skipped = out.data.as_ref().unwrap()["skipped"].as_array().unwrap().clone();
        assert_eq!(skipped.len(), 1, "skipped: {skipped:?}");
        assert_eq!(skipped[0]["sessionId"], "undying-dead");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// An empty undying set is an `ok` no-op with an honest message — never
    /// silently treated as "nothing to do" without saying why.
    #[test]
    fn bare_default_is_an_ok_no_op_when_the_undying_set_is_empty() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG"]);
        let (root, proj_path) = setup("resurrect-undying-empty");

        // An anchored entry exists, but nothing is undying.
        set_ledger(&[ledger_entry("not-undying-only", "no-such-harness", &proj_path, "2026-08-20T01:00:00Z")]);

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "proj")]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        assert_eq!(out.data.as_ref().unwrap()["resurrected"].as_array().unwrap().len(), 0);
        assert!(
            out.message.contains("undying set is empty"),
            "message must say WHY, not just no-op silently: {}",
            out.message
        );

        // The gate-6 fix (U2): an empty-selection early return must still
        // write the ONE audit line every resurrect invocation gets, never
        // silently skip it because nothing was selected — exactly one line,
        // never zero, never two (review round 1: `contains()` alone would
        // also pass on a duplicated line).
        let log = root.join("log");
        assert_eq!(count_resurrect_audit_lines(&log), 1, "log: {}", std::fs::read_to_string(&log).unwrap_or_default());
        let raw = std::fs::read_to_string(&log).unwrap_or_default();
        assert!(raw.contains("undying set is empty"), "the one line must carry the real message: {raw}");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A undying id that exited, was resurrected, and exited again appears
    /// twice in the append-only ledger — the dedup keeps the newest
    /// `endedAt`, so only one candidate is ever selected.
    #[test]
    fn bare_default_dedups_a_repeated_undying_id_keeping_the_newest_ended_at() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG"]);
        let (root, proj_path) = setup("resurrect-undying-dedup");

        // Same sessionId, two ledger lines (append-only, both legal): an
        // earlier exit and a later re-exit.
        set_ledger(&[
            ledger_entry("repeated-id", "no-such-harness", &proj_path, "2026-08-20T01:00:00Z"),
            ledger_entry("repeated-id", "no-such-harness", &proj_path, "2026-08-20T09:00:00Z"),
        ]);
        let mut undying = Vec::new();
        aoide_storage::undying::set_undying(&mut undying, "repeated-id", true);
        aoide_storage::undying::save_undying(&undying).unwrap();

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "proj")]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let skipped = out.data.as_ref().unwrap()["skipped"].as_array().unwrap().clone();
        assert_eq!(skipped.len(), 1, "the repeated id must be deduped to one candidate: {skipped:?}");

        let _ = std::fs::remove_dir_all(&root);
    }

    // ── U2: bare-manifest mode ───────────────────────────────────────────

    /// Restores the real process cwd on drop, even if the test body panics
    /// mid-assertion — without this, a panicking test would leave every
    /// LATER test in this same process running from the wrong directory
    /// (tests share one OS process; `--test-threads=1` plus `env_lock`
    /// serializes access, but only a `Drop` guard protects against a panic
    /// skipping the restore).
    struct CwdGuard {
        prev: std::path::PathBuf,
    }
    impl CwdGuard {
        fn enter(dir: &std::path::Path) -> Self {
            let prev = std::env::current_dir().expect("current_dir must resolve in a test");
            std::env::set_current_dir(dir).expect("chdir into the scratch dir must succeed");
            CwdGuard { prev }
        }
    }
    impl Drop for CwdGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.prev);
        }
    }

    /// Build a manifest at `root` (also this test's `AOIDE_STATE_DIR`/
    /// `AOIDE_STAGE_DIR` root — no collision, `.aoide/` sits beside, never
    /// inside, `state/`/`stage/`) with the given specs, chdir into
    /// `root/work` (proving the walk actually climbs, not just checks
    /// cwd itself), and return `(root, CwdGuard)` — the guard must outlive
    /// the call that exercises `session_resurrect`.
    fn setup_manifest(tag: &str, specs: Vec<aoide_storage::manifest::SessionSpec>) -> (std::path::PathBuf, CwdGuard) {
        let root = unique_stage(tag);
        let stage = root.join("stage");
        let state = root.join("state");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("AOIDE_STATE_DIR", &state);
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        std::env::set_var("AOIDE_AUDIT_LOG", root.join("log"));

        aoide_storage::manifest::save_manifest(
            &root,
            &aoide_storage::manifest::Manifest { version: aoide_storage::manifest::MANIFEST_VERSION, sessions: specs },
        )
        .unwrap();

        let work = root.join("work");
        std::fs::create_dir_all(&work).unwrap();
        let guard = CwdGuard::enter(&work);
        (root, guard)
    }

    fn manifest_spec(host: &str, dir: &str, agent: &str, command: Option<&str>) -> aoide_storage::manifest::SessionSpec {
        aoide_storage::manifest::SessionSpec {
            host: host.to_string(),
            dir: dir.to_string(),
            agent: agent.to_string(),
            command: command.map(str::to_string),
        }
    }

    /// The headline case: bare `resurrect`, no flags, cwd nested under a
    /// project with a manifest but no matching ledger history — the spec
    /// clean-spawns (windowed, `AOIDE_TERMINAL=true` for a real-but-inert
    /// child, same fixture `a_successful_resurrect_transfers_the_undying_
    /// mark_from_old_to_new` already uses) rather than needing any
    /// `projects.json` registration at all.
    /// GATED on Unix with its reason: the revived session is `conduct
    /// --headless`, whose fixture is a POSIX child (`sh -c`, a `#!/bin/sh` shim
    /// and a mode) that `CreateProcess` cannot run (see
    /// `a_node_origin_ledger_entry_never_derails_an_ordinary_resurrect`'s gate).
    #[cfg(unix)]
    #[test]
    fn bare_mode_finds_the_manifest_and_clean_spawns_an_unmatched_spec() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG",
            "AOIDE_TERMINAL", "WAYLAND_DISPLAY", "DISPLAY",
        ]);
        let this_host = aoide_storage::display::local_host_name();
        let (root, cwd) = setup_manifest(
            "resurrect-manifest-clean-spawn",
            vec![manifest_spec(&this_host, ".", "claude", None)],
        );
        std::env::set_var("AOIDE_TERMINAL", "true");
        std::env::set_var("WAYLAND_DISPLAY", "wayland-0");

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[]));
        drop(cwd);
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let data = out.data.as_ref().unwrap();
        assert_eq!(data["manifestRoot"], root.to_string_lossy().to_string());
        let resurrected = data["resurrected"].as_array().unwrap();
        assert_eq!(resurrected.len(), 1, "data: {data}");
        assert_eq!(resurrected[0]["disposition"], "clean-spawned");
        assert_eq!(resurrected[0]["agent"], "claude");

        // Bare-manifest mode audits too, EXACTLY once, same as flag mode.
        let log = root.join("log");
        assert_eq!(
            count_resurrect_audit_lines(&log), 1,
            "log: {}", std::fs::read_to_string(&log).unwrap_or_default()
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A spec whose `command` is given wins over the agent's registered
    /// default — proven indirectly: an agent with NO registered profile
    /// (`no-such-harness`) would otherwise fail with no default launch, but
    /// a `command` on the spec still clean-spawns it.
    /// GATED on Unix with its reason: the revived session is `conduct
    /// --headless`, whose fixture is a POSIX child (`sh -c`, a `#!/bin/sh` shim
    /// and a mode) that `CreateProcess` cannot run (see
    /// `a_node_origin_ledger_entry_never_derails_an_ordinary_resurrect`'s gate).
    #[cfg(unix)]
    #[test]
    fn bare_mode_clean_spawn_prefers_the_specs_own_command_over_a_default_launch() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG",
            "AOIDE_TERMINAL", "WAYLAND_DISPLAY", "DISPLAY",
        ]);
        let this_host = aoide_storage::display::local_host_name();
        let (root, cwd) = setup_manifest(
            "resurrect-manifest-own-command",
            vec![manifest_spec(&this_host, ".", "no-such-harness", Some("watch -n1 true"))],
        );
        std::env::set_var("AOIDE_TERMINAL", "true");
        std::env::set_var("WAYLAND_DISPLAY", "wayland-0");

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[]));
        drop(cwd);
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let data = out.data.as_ref().unwrap();
        assert_eq!(data["failed"].as_array().unwrap().len(), 0, "data: {data}");
        let resurrected = data["resurrected"].as_array().unwrap();
        assert_eq!(resurrected.len(), 1, "data: {data}");
        assert_eq!(resurrected[0]["disposition"], "clean-spawned");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// An agent with neither a `command` nor a registered profile is a
    /// taught `failed[]` entry, never a guessed argv.
    #[test]
    fn bare_mode_clean_spawn_fails_taught_when_neither_command_nor_default_launch_exists() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG"]);
        let this_host = aoide_storage::display::local_host_name();
        let (root, cwd) = setup_manifest(
            "resurrect-manifest-no-default",
            vec![manifest_spec(&this_host, ".", "no-such-harness", None)],
        );

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[]));
        drop(cwd);
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let data = out.data.as_ref().unwrap();
        let failed = data["failed"].as_array().unwrap();
        assert_eq!(failed.len(), 1, "data: {data}");
        assert_eq!(failed[0]["disposition"], "failed");
        assert!(
            failed[0]["reason"].as_str().unwrap().contains("no-such-harness"),
            "reason must name the agent: {}",
            failed[0]["reason"]
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// U4: a spec whose `host` is not this host's own name is SUMMONED, not
    /// skipped — but "no node named that host at all" is the first local
    /// refusal `summon_remote` holds, before the wire is ever touched. Lands
    /// in `failed[]` (never `skipped[]` — this spec was tried and refused),
    /// taught to name `node add`.
    #[test]
    fn bare_mode_remote_summon_fails_taught_against_an_unregistered_host() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG"]);
        let (root, cwd) = setup_manifest(
            "resurrect-manifest-remote-unknown",
            vec![manifest_spec("some-other-host", ".", "claude", None)],
        );

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[]));
        drop(cwd);
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let data = out.data.as_ref().unwrap();
        assert_eq!(data["skipped"].as_array().unwrap().len(), 0, "an unknown-host remote spec is FAILED, not skipped: {data}");
        let failed = data["failed"].as_array().unwrap();
        assert_eq!(failed.len(), 1, "data: {data}");
        assert_eq!(failed[0]["disposition"], "failed");
        assert_eq!(failed[0]["host"], "some-other-host");
        assert!(
            failed[0]["reason"].as_str().unwrap().contains("node add"),
            "reason must teach `node add`: {}",
            failed[0]["reason"]
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The second local refusal: a node registered via the legacy `node
    /// add` escape but never paired (`verified: false`) can never satisfy
    /// the remote door's `Signature`-rung spawn gate — refused LOCALLY,
    /// same posture `aoide-client::commands::handle_node_spawn` already
    /// holds toward its own CLI callers.
    #[test]
    fn bare_mode_remote_summon_fails_taught_against_an_unverified_node() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG"]);
        let (root, cwd) = setup_manifest(
            "resurrect-manifest-remote-unverified",
            vec![manifest_spec("sakaki", ".", "claude", None)],
        );
        aoide_storage::node_store::save_nodes(&[unpaired_node("sakaki")]).unwrap();

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[]));
        drop(cwd);
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let data = out.data.as_ref().unwrap();
        let failed = data["failed"].as_array().unwrap();
        assert_eq!(failed.len(), 1, "data: {data}");
        assert_eq!(failed[0]["disposition"], "failed");
        assert!(
            failed[0]["reason"].as_str().unwrap().contains("aoide pair"),
            "reason must teach the pairing ceremony: {}",
            failed[0]["reason"]
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The third local refusal: a verified node, but nothing to summon it
    /// WITH — no `command` and no registered default launch for the spec's
    /// agent. Proven with a genuinely unreachable node URL (port 9, the
    /// same discard-port fixture `send.rs`'s own remote-delivery tests use)
    /// to prove the wire is never even touched — the refusal must fire
    /// before `spawn_on_node` gets a chance to fail for a DIFFERENT reason.
    #[test]
    fn bare_mode_remote_summon_fails_taught_with_nothing_to_summon() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG"]);
        let (root, cwd) = setup_manifest(
            "resurrect-manifest-remote-no-command",
            vec![manifest_spec("sakaki", ".", "no-such-harness", None)],
        );
        aoide_storage::node_store::save_nodes(&[verified_node("sakaki", "http://127.0.0.1:9/")]).unwrap();

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[]));
        drop(cwd);
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let data = out.data.as_ref().unwrap();
        let failed = data["failed"].as_array().unwrap();
        assert_eq!(failed.len(), 1, "data: {data}");
        assert!(
            failed[0]["reason"].as_str().unwrap().contains("no-such-harness"),
            "reason must name the agent: {}",
            failed[0]["reason"]
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A verified, reachable-address node with nothing listening at all
    /// (port 9, same discard-port fixture `send.rs`'s remote-delivery
    /// tests already rely on) — `spawn_on_node`'s own connection failure
    /// surfaces VERBATIM into `failed[]`, never turned into a hard
    /// `Outcome::error` (per-spec isolation holds even past the local
    /// refusals).
    #[test]
    fn bare_mode_remote_summon_fails_taught_when_the_node_is_unreachable() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG"]);
        let (root, cwd) = setup_manifest(
            "resurrect-manifest-remote-unreachable",
            vec![manifest_spec("sakaki", ".", "claude", Some("git -C /home/khoa/Aoide pull --ff-only"))],
        );
        aoide_storage::node_store::save_nodes(&[verified_node("sakaki", "http://127.0.0.1:9/")]).unwrap();

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[]));
        drop(cwd);
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let data = out.data.as_ref().unwrap();
        assert_eq!(data["resurrected"].as_array().unwrap().len(), 0, "data: {data}");
        let failed = data["failed"].as_array().unwrap();
        assert_eq!(failed.len(), 1, "the closed loopback port refuses the POST: {data}");
        assert_eq!(failed[0]["disposition"], "failed");
        assert_eq!(failed[0]["host"], "sakaki");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Local and remote specs in ONE manifest resolve independently — a
    /// remote spec refused locally (unknown host) never poisons a sibling
    /// LOCAL spec's own clean-spawn in the same invocation, the same
    /// per-spec isolation every other row in this loop already holds.
    /// GATED on Unix with its reason: the revived session is `conduct
    /// --headless`, whose fixture is a POSIX child (`sh -c`, a `#!/bin/sh` shim
    /// and a mode) that `CreateProcess` cannot run (see
    /// `a_node_origin_ledger_entry_never_derails_an_ordinary_resurrect`'s gate).
    #[cfg(unix)]
    #[test]
    fn bare_mode_local_and_remote_specs_isolate_in_one_manifest() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG",
            "AOIDE_TERMINAL", "WAYLAND_DISPLAY", "DISPLAY",
        ]);
        let this_host = aoide_storage::display::local_host_name();
        let (root, cwd) = setup_manifest(
            "resurrect-manifest-mixed",
            vec![
                manifest_spec("some-other-host", ".", "claude", None),
                manifest_spec(&this_host, ".", "claude", None),
            ],
        );
        std::env::set_var("AOIDE_TERMINAL", "true");
        std::env::set_var("WAYLAND_DISPLAY", "wayland-0");

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[]));
        drop(cwd);
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let data = out.data.as_ref().unwrap();
        let failed = data["failed"].as_array().unwrap();
        assert_eq!(failed.len(), 1, "data: {data}");
        assert_eq!(failed[0]["host"], "some-other-host");
        let resurrected = data["resurrected"].as_array().unwrap();
        assert_eq!(resurrected.len(), 1, "the LOCAL spec must still clean-spawn: {data}");
        assert_eq!(resurrected[0]["disposition"], "clean-spawned");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// [`summon_text`] pinned directly, no I/O: a spec's own `command`
    /// wins verbatim (never whitespace-split — there is no argv on the
    /// remote wire, only one prompt string), a `command`-less spec falls to
    /// the agent's registered default launch joined back into one line, and
    /// an agent with neither yields `None` rather than a guessed prompt.
    #[test]
    fn summon_text_prefers_the_specs_own_command_verbatim_over_a_default_launch() {
        let spec = manifest_spec("sakaki", ".", "claude", Some("echo  two  spaces"));
        assert_eq!(summon_text(&spec).as_deref(), Some("echo  two  spaces"));
    }

    #[test]
    fn summon_text_falls_to_the_agents_default_launch_joined_into_one_line() {
        let spec = manifest_spec("sakaki", ".", "claude", None);
        let text = summon_text(&spec).expect("a registered agent must fall to its default launch");
        assert!(!text.is_empty());
        assert!(!text.contains('\u{0}'), "sanity: a real command string");
    }

    #[test]
    fn summon_text_is_none_for_an_unregistered_agent_with_no_command() {
        let spec = manifest_spec("sakaki", ".", "no-such-harness", None);
        assert!(summon_text(&spec).is_none());
    }

    /// A `dir` that normalizes outside the project root is rejected into
    /// `failed[]`, never resolved to some path outside the project the
    /// manifest lives in.
    #[test]
    fn bare_mode_rejects_a_dir_that_escapes_the_project_root() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG"]);
        let this_host = aoide_storage::display::local_host_name();
        let (root, cwd) = setup_manifest(
            "resurrect-manifest-escape",
            vec![manifest_spec(&this_host, "../../etc", "claude", None)],
        );

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[]));
        drop(cwd);
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let data = out.data.as_ref().unwrap();
        let failed = data["failed"].as_array().unwrap();
        assert_eq!(failed.len(), 1, "data: {data}");
        assert_eq!(failed[0]["disposition"], "failed");
        assert!(failed[0]["reason"].as_str().unwrap().contains("escapes"), "reason: {}", failed[0]["reason"]);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// One spec's containment rejection never aborts a sibling spec's own
    /// resolution — the same per-candidate isolation the flag-mode loop
    /// already holds, now proven at the per-SPEC level.
    /// GATED on Unix with its reason: the revived session is `conduct
    /// --headless`, whose fixture is a POSIX child (`sh -c`, a `#!/bin/sh` shim
    /// and a mode) that `CreateProcess` cannot run (see
    /// `a_node_origin_ledger_entry_never_derails_an_ordinary_resurrect`'s gate).
    #[cfg(unix)]
    #[test]
    fn bare_mode_one_failing_spec_never_aborts_the_rest() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG",
            "AOIDE_TERMINAL", "WAYLAND_DISPLAY", "DISPLAY",
        ]);
        let this_host = aoide_storage::display::local_host_name();
        let (root, cwd) = setup_manifest(
            "resurrect-manifest-isolation",
            vec![
                manifest_spec(&this_host, "../escape", "claude", None),
                manifest_spec(&this_host, ".", "claude", None),
            ],
        );
        std::env::set_var("AOIDE_TERMINAL", "true");
        std::env::set_var("WAYLAND_DISPLAY", "wayland-0");

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[]));
        drop(cwd);
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let data = out.data.as_ref().unwrap();
        assert_eq!(data["failed"].as_array().unwrap().len(), 1, "data: {data}");
        assert_eq!(data["resurrected"].as_array().unwrap().len(), 1, "data: {data}");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The enrichment rule: a matching ledger entry revives through the
    /// SAME `resolve_candidate`/`resurrect_one` path `--id` drives, and
    /// when more than one entry matches, the NEWEST `endedAt` wins — proven
    /// via the harness-skip taught message (cheap: no windowed spawn), same
    /// as the flag-mode `--id`/`--all` tests above prove selection width
    /// without needing a real spawn either.
    #[test]
    fn bare_mode_enrichment_picks_the_newest_matching_ledger_entry() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG"]);
        let this_host = aoide_storage::display::local_host_name();
        let (root, cwd) = setup_manifest(
            "resurrect-manifest-enrich-newest",
            vec![manifest_spec(&this_host, ".", "no-such-harness", None)],
        );
        let root_str = root.to_str().unwrap().to_string();
        set_ledger(&[
            ledger_entry("ledger-older", "no-such-harness", &root_str, "2026-08-20T01:00:00Z"),
            ledger_entry("ledger-newer", "no-such-harness", &root_str, "2026-08-20T09:00:00Z"),
        ]);

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[]));
        drop(cwd);
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let data = out.data.as_ref().unwrap();
        let skipped = data["skipped"].as_array().unwrap();
        assert_eq!(skipped.len(), 1, "exactly one ledger entry must be selected for enrichment: {data}");
        assert_eq!(skipped[0]["sessionId"], "ledger-newer", "the NEWEST matching entry must win: {data}");
        // Review round 1: a row `resurrect_one` itself pushed into `skipped`
        // (no `disposition` of its own — that's a manifest-mode concept)
        // must still carry one once it lands in THIS loop's own outcome,
        // so a consumer filtering by `disposition` never drops it.
        assert_eq!(skipped[0]["disposition"], "skipped", "data: {data}");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The same uniform-disposition fix, the `failed` bucket half: a
    /// MATCHED entry (terminal arm resolves — a `restore` snapshot is
    /// present) whose spawn then fails outright (no `$AOIDE_TERMINAL`, the
    /// same headless taught error `a_windowed_spawn_failure_degrades_
    /// gracefully_instead_of_erroring_the_command` proves in flag mode)
    /// must land in `failed` with `disposition: "failed"`, not a bare
    /// `resurrect_one`-shaped row missing the key entirely.
    #[test]
    fn bare_mode_enrichment_spawn_failure_gets_a_failed_disposition_too() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG",
            "AOIDE_TERMINAL", "WAYLAND_DISPLAY", "DISPLAY",
        ]);
        let this_host = aoide_storage::display::local_host_name();
        let (root, cwd) = setup_manifest(
            "resurrect-manifest-enrich-failed",
            vec![manifest_spec(&this_host, ".", "shell", None)],
        );
        let root_str = root.to_str().unwrap().to_string();
        set_ledger(&[ledger_entry_with_restore(
            "ledger-shell-fail",
            "shell",
            &root_str,
            "2026-08-20T01:00:00Z",
            Some(RestoreSnapshot { cwd: Some(root_str.clone()), idle: true, argv: None }),
        )]);
        std::env::remove_var("AOIDE_TERMINAL");
        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::remove_var("DISPLAY");

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[]));
        drop(cwd);
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let data = out.data.as_ref().unwrap();
        let failed = data["failed"].as_array().unwrap();
        assert_eq!(failed.len(), 1, "data: {data}");
        assert_eq!(failed[0]["disposition"], "failed", "data: {data}");
        assert_eq!(data["resurrected"].as_array().unwrap().len(), 0);

        // A failed spawn never reaches the undying mark either — nothing
        // to mark, the session never came into being.
        let undying = aoide_storage::undying::load_undying();
        assert!(!aoide_storage::undying::is_undying(&undying, "ledger-shell-fail"));

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The enrichment path is not just "some ledger match" — it reuses
    /// `resolve_candidate`/`resurrect_one` verbatim, so a MATCHED entry with
    /// a `restore` snapshot resolves through the terminal arm exactly like
    /// `--id` would, and the resulting `resurrected` entry carries
    /// `disposition: "revived-from-ledger"` (never `"clean-spawned"`,
    /// proving enrichment — not a fresh launch — is what actually fired).
    /// GATED on Unix with its reason: the revived session is `conduct
    /// --headless`, whose fixture is a POSIX child (`sh -c`, a `#!/bin/sh` shim
    /// and a mode) that `CreateProcess` cannot run (see
    /// `a_node_origin_ledger_entry_never_derails_an_ordinary_resurrect`'s gate).
    #[cfg(unix)]
    #[test]
    fn bare_mode_enrichment_revives_a_matched_restore_bearing_entry_via_the_terminal_arm() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG",
            "AOIDE_TERMINAL", "WAYLAND_DISPLAY", "DISPLAY",
        ]);
        let this_host = aoide_storage::display::local_host_name();
        let (root, cwd) = setup_manifest(
            "resurrect-manifest-enrich-terminal",
            vec![manifest_spec(&this_host, ".", "shell", None)],
        );
        let root_str = root.to_str().unwrap().to_string();
        set_ledger(&[ledger_entry_with_restore(
            "ledger-shell",
            "shell",
            &root_str,
            "2026-08-20T01:00:00Z",
            Some(RestoreSnapshot { cwd: Some(root_str.clone()), idle: true, argv: None }),
        )]);
        std::env::set_var("AOIDE_TERMINAL", "true");
        std::env::set_var("WAYLAND_DISPLAY", "wayland-0");

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[]));
        drop(cwd);
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let data = out.data.as_ref().unwrap();
        let resurrected = data["resurrected"].as_array().unwrap();
        assert_eq!(resurrected.len(), 1, "data: {data}");
        assert_eq!(resurrected[0]["disposition"], "revived-from-ledger");
        assert_eq!(resurrected[0]["resumedFrom"], "ledger-shell");

        // Design ruling (U2 review round 1): a manifest-enriched revival
        // marks its NEW session id undying, gated on Status::Ok alone
        // (this spawn reached it — "true" launches successfully even
        // though it never registers).
        let new_id = resurrected[0]["sessionId"].as_str().unwrap().to_string();
        let undying = aoide_storage::undying::load_undying();
        assert!(
            aoide_storage::undying::is_undying(&undying, &new_id),
            "a manifest-enriched revival must mark its new session undying"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The design ruling's clean-spawn half: an UNMATCHED spec (no ledger
    /// enrichment at all — the ordinary fresh-checkout case) still marks
    /// its freshly clean-spawned session undying, same as the enriched
    /// path above.
    /// GATED on Unix with its reason: the revived session is `conduct
    /// --headless`, whose fixture is a POSIX child (`sh -c`, a `#!/bin/sh` shim
    /// and a mode) that `CreateProcess` cannot run (see
    /// `a_node_origin_ledger_entry_never_derails_an_ordinary_resurrect`'s gate).
    #[cfg(unix)]
    #[test]
    fn bare_mode_clean_spawn_marks_the_new_session_undying() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG",
            "AOIDE_TERMINAL", "WAYLAND_DISPLAY", "DISPLAY",
        ]);
        let this_host = aoide_storage::display::local_host_name();
        let (root, cwd) = setup_manifest(
            "resurrect-manifest-clean-spawn-undying",
            vec![manifest_spec(&this_host, ".", "claude", None)],
        );
        std::env::set_var("AOIDE_TERMINAL", "true");
        std::env::set_var("WAYLAND_DISPLAY", "wayland-0");

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[]));
        drop(cwd);
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let data = out.data.as_ref().unwrap();
        let resurrected = data["resurrected"].as_array().unwrap();
        assert_eq!(resurrected.len(), 1, "data: {data}");
        assert_eq!(resurrected[0]["disposition"], "clean-spawned");
        let new_id = resurrected[0]["sessionId"].as_str().unwrap().to_string();

        let undying = aoide_storage::undying::load_undying();
        assert!(
            aoide_storage::undying::is_undying(&undying, &new_id),
            "a manifest clean-spawn must mark its new session undying"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Finding 5, resurrect: a restore an MCP door parks reports `held-pending`
    /// and its entry is in pending.json. It is never reported as preloaded. The
    /// target is registered and conductable on a bound socket, so the held
    /// word can only come from the door (an unreachable target would answer
    /// `failed`, not `held-pending`), and the socket must see nothing.
    #[cfg(unix)]
    #[test]
    fn a_restore_over_an_mcp_door_is_held_pending_and_never_typed() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_CONDUCT_AUTOGATE",
            "AOIDE_SESSION_ID",
        ]);
        let (root, _proj_path) = setup("resurrect-mcp-restore");

        let id = "rs-target";
        let socket = crate::graph::conduct::conduct_socket_path(id);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        crate::graph::session_store::do_session_start(
            id,
            Some("claude"),
            Some("/w"),
            None,
            None,
            Some(true),
            Some(socket.to_str().unwrap()),
            None,
            None,
        );

        let restore = busy_restore(&["echo", "hello"]);
        let inv = restore_delivery(aoide_protocol::Door::Mcp, id, &restore)
            .expect("a busy restore constructs a send");
        assert_eq!(send_restore(&inv, id), Ok(Landed::Held), "a restore over MCP is parked");

        listener.set_nonblocking(true).unwrap();
        assert!(
            matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock),
            "a restore over an MCP door typed into the session"
        );
        let raw: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(crate::graph::send::pending_path()).unwrap()).unwrap();
        let held = raw["pending"].as_array().expect("pending.json holds the restore");
        assert_eq!(held.len(), 1, "pending: {raw}");
        assert_eq!(held[0]["sessionId"], id);
        assert_eq!(held[0]["text"], "echo hello");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A restore is not a send, so attestation does not buy it: the revived
    /// session is parented by the sealed session this process stands for, which
    /// is exactly the relationship that delivers an ordinary MCP send, and the
    /// restore is still held. The control is the ordinary send of the same bytes
    /// through the same door, which the real resolver delivers, so the fixture
    /// is shown to be attested rather than assumed.
    #[cfg(unix)]
    #[test]
    fn an_attested_mcp_callers_restore_is_held_pending_though_its_send_would_deliver() {
        use crate::graph::send::session_send;
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_CONDUCT_AUTOGATE",
            "AOIDE_SESSION_ID",
            "AOIDE_DAEMON_SOCKET",
        ]);
        let (root, _proj_path) = setup("resurrect-mcp-attested");
        let kp = aoide_storage::identity::mint_ephemeral().unwrap();
        std::env::set_var("AOIDE_DAEMON_SOCKET", fake_seal_daemon(&kp.info().pubkey_hex));
        crate::graph::session_store::do_session_start(
            "orch", Some("claude"), Some("/w"), None, None, None, None, None, Some(std::process::id()),
        );
        seal_record("orch", &kp);

        let id = "rs-attested";
        let socket = crate::graph::conduct::conduct_socket_path(id);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        crate::graph::session_store::do_session_start(
            id, Some("claude"), Some("/w"), None, Some("orch"), Some(true), Some(socket.to_str().unwrap()), None, None,
        );
        crate::graph::session_store::stamp_attested_spawner(id, Some("orch"));
        let restore = busy_restore(&["echo", "hello"]);
        let inv = restore_delivery(aoide_protocol::Door::Mcp, id, &restore).unwrap();

        assert_eq!(send_restore(&inv, id), Ok(Landed::Held), "a restore over MCP is parked");
        assert!(
            matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock),
            "an attested MCP caller's restore typed into the session"
        );

        listener.set_nonblocking(false).unwrap();
        let acc = aoide_test_support::expect_delivery(listener, "the same bytes as an ordinary attested send");
        let sent = session_send(&inv);
        let _ = acc.join().unwrap();
        assert_eq!(sent.data.as_ref().unwrap()["gate"], "autogate-parent", "the control: {}", sent.message);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A terminal that records it was opened: `AOIDE_TERMINAL` for a windowed
    /// launch, a script that touches `marker`, so a test proves by the file
    /// whether anything was launched.
    #[cfg(unix)]
    fn marker_terminal(root: &std::path::Path) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let marker = root.join("terminal-ran");
        let script = root.join("fake-terminal.sh");
        std::fs::write(&script, format!("#!/bin/sh\ntouch {}\n", marker.display())).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("AOIDE_TERMINAL", &script);
        std::env::set_var("WAYLAND_DISPLAY", "wayland-0");
        marker
    }

    /// `wait_for` waits this long for a positive control's terminal, and
    /// `no_marker_within_the_launch_wait` watches this long for a dry run's. A
    /// launch that starts later is not caught.
    #[cfg(unix)]
    const LAUNCH_WAIT: std::time::Duration = std::time::Duration::from_millis(5000);

    #[cfg(unix)]
    fn wait_for(path: &std::path::Path) -> bool {
        let deadline = std::time::Instant::now() + LAUNCH_WAIT;
        while std::time::Instant::now() < deadline && !path.exists() {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        path.exists()
    }

    /// The negative side of `wait_for`: no terminal may open within
    /// `LAUNCH_WAIT`. It fails the moment the marker appears, so a launch in
    /// that time is what it catches.
    #[cfg(unix)]
    fn no_marker_within_the_launch_wait(marker: &std::path::Path) {
        let deadline = std::time::Instant::now() + LAUNCH_WAIT;
        while std::time::Instant::now() < deadline {
            assert!(!marker.exists(), "a dry run opened a terminal");
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(!marker.exists(), "a dry run opened a terminal");
    }

    #[cfg(unix)]
    const RESURRECT_ENV: &[&str] = &[
        "AOIDE_STAGE_DIR",
        "AOIDE_STATE_DIR",
        "XDG_RUNTIME_DIR",
        "AOIDE_AUDIT_LOG",
        "AOIDE_TERMINAL",
        "WAYLAND_DISPLAY",
        "DISPLAY",
        "AOIDE_DAEMON_SOCKET",
        "AOIDE_SESSION_ID",
    ];

    #[cfg(unix)]
    fn held_entries() -> Vec<crate::graph::send::PendingSend> {
        load_stage::<crate::graph::send::PendingFile>(&crate::graph::send::pending_path())
            .map(|f| f.pending)
            .unwrap_or_default()
    }

    /// Resurrect launches harness processes, so a door that cannot approve gets
    /// it the way it gets a spawn: with no attested caller the whole command is
    /// held in `pending.json` and nothing launches, whether the caller is an
    /// MCP server with no sealed ancestor (the production resolver, no daemon)
    /// or the A2A door, which is never asked who it is even when a resolver
    /// would name a session. The entry replays `resurrect`, shows as that
    /// command, and approving it from the CLI is what revives the session.
    #[cfg(unix)]
    #[test]
    fn an_unattested_resurrect_is_held_whole_and_launches_nothing_until_a_human_approves() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(RESURRECT_ENV);
        let (root, proj_path) = setup("resurrect-mcp-held");
        std::env::remove_var("AOIDE_DAEMON_SOCKET");
        std::env::remove_var("AOIDE_SESSION_ID");
        let marker = marker_terminal(&root);
        set_ledger(&[ledger_entry("ledger-old-held", "claude", &proj_path, "2026-08-20T01:00:00Z")]);
        let mut undying = Vec::new();
        aoide_storage::undying::set_undying(&mut undying, "ledger-old-held", true);
        aoide_storage::undying::save_undying(&undying).unwrap();

        let mut mcp = flag_invocation(&["resurrect"], &[("project", "proj")]);
        mcp.door = aoide_protocol::Door::Mcp;
        let out = session_resurrect(&mcp);
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "{}", out.message);
        let data = out.data.as_ref().unwrap();
        assert_eq!(data["state"], "pending", "{data}");
        assert_eq!(data["launched"], false);

        let mut a2a = flag_invocation(&["resurrect"], &[("project", "proj"), ("all", "true")]);
        a2a.door = aoide_protocol::Door::A2a;
        let out = session_resurrect_with(&a2a, |_| Some("orch".to_string()));
        assert_eq!(out.data.as_ref().unwrap()["state"], "pending", "A2A is held though a resolver would attest: {}", out.message);

        let held = held_entries();
        assert_eq!(held.len(), 2, "{held:?}");
        let spawn = held[0].spawn.as_ref().expect("a held command, not a held send");
        assert_eq!(spawn.path, ["resurrect"]);
        assert_eq!(spawn.flags.get("project").map(String::as_str), Some("proj"));
        assert_eq!(held[0].from.as_deref(), Some("mcp"));
        assert_eq!(held[0].text, "aoide resurrect --project=proj", "the approver is shown the command");
        assert_eq!(held[1].text, "aoide resurrect --all --project=proj");

        std::thread::sleep(std::time::Duration::from_millis(600));
        assert!(!marker.exists(), "the held resurrect opened a terminal");
        let roster: SessionsFile = load_stage(&sessions_path()).unwrap_or_default();
        assert!(roster.sessions.is_empty(), "no session was registered: {:?}", roster.sessions);

        let listed = crate::graph::pending::pending_list(&flag_invocation(&["session", "pending", "list"], &[]));
        assert!(listed.message.contains("aoide resurrect --project=proj"), "{}", listed.message);

        let approve = crate::graph::pending::pending_approve(&Invocation {
            path: vec!["session".into(), "pending".into(), "approve".into()],
            args: vec!["0".into()],
            flags: crate::graph::pending::expect_at(0),
            door: aoide_protocol::Door::Cli,
        });
        assert_eq!(approve.status, aoide_protocol::output::Status::Ok, "{}", approve.message);
        assert!(approve.message.contains("resurrected"), "{}", approve.message);
        assert_eq!(approve.data.as_ref().unwrap()["launched"], true);
        assert!(wait_for(&marker), "approving the held resurrect revived the session");
        assert_eq!(held_entries().len(), 1, "only the approved entry left the queue");
        let _ = std::fs::remove_dir_all(&root);
    }
    /// A held resurrect replays the plan its approver was shown. If the ledger
    /// gained a candidate since the hold, approve refuses it before the entry
    /// leaves the queue, and nothing is revived. Before the binding, approve
    /// re-selected at approve time and revived whatever the ledger held then.
    #[cfg(unix)]
    #[test]
    fn a_held_resurrect_refuses_to_launch_a_plan_its_approver_was_not_shown() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(RESURRECT_ENV);
        let (root, proj_path) = setup("resurrect-plan-moved");
        std::env::remove_var("AOIDE_DAEMON_SOCKET");
        std::env::remove_var("AOIDE_SESSION_ID");
        let marker = marker_terminal(&root);
        set_ledger(&[ledger_entry("ledger-old-plan", "claude", &proj_path, "2026-08-20T01:00:00Z")]);
        let mut undying = Vec::new();
        aoide_storage::undying::set_undying(&mut undying, "ledger-old-plan", true);
        aoide_storage::undying::save_undying(&undying).unwrap();

        let mut mcp = flag_invocation(&["resurrect"], &[("project", "proj")]);
        mcp.door = aoide_protocol::Door::Mcp;
        let out = session_resurrect(&mcp);
        assert_eq!(out.data.as_ref().unwrap()["state"], "pending", "{}", out.message);
        assert_eq!(held_entries()[0].text, "aoide resurrect --project=proj");

        set_ledger(&[
            ledger_entry("ledger-old-plan", "claude", &proj_path, "2026-08-20T01:00:00Z"),
            ledger_entry("ledger-new-plan", "claude", &proj_path, "2026-08-21T01:00:00Z"),
        ]);
        aoide_storage::undying::set_undying(&mut undying, "ledger-new-plan", true);
        aoide_storage::undying::save_undying(&undying).unwrap();

        let approve = crate::graph::pending::pending_approve(&Invocation {
            path: vec!["session".into(), "pending".into(), "approve".into()],
            args: vec!["0".into()],
            flags: crate::graph::pending::expect_at(0),
            door: aoide_protocol::Door::Cli,
        });
        assert_eq!(approve.status, aoide_protocol::output::Status::Error, "{}", approve.message);
        assert_eq!(held_entries().len(), 1, "the refused entry stays held");
        std::thread::sleep(std::time::Duration::from_millis(600));
        assert!(!marker.exists(), "the candidate nobody was shown did not revive");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A held resurrect that revives nothing when approved says so: the entry
    /// has left the queue (approval consumes it), and the answer is not success.
    /// Before the change the answer was `launched: true` for an empty revival.
    #[cfg(unix)]
    #[test]
    fn approving_a_resurrect_that_revives_nothing_is_not_reported_as_launched() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(RESURRECT_ENV);
        let (root, _proj_path) = setup("resurrect-nothing-revived");
        std::env::remove_var("AOIDE_DAEMON_SOCKET");
        std::env::remove_var("AOIDE_SESSION_ID");
        set_ledger(&[]);

        let mut mcp = flag_invocation(&["resurrect"], &[("project", "proj")]);
        mcp.door = aoide_protocol::Door::Mcp;
        let out = session_resurrect(&mcp);
        assert_eq!(out.data.as_ref().unwrap()["state"], "pending", "{}", out.message);

        let approve = crate::graph::pending::pending_approve(&Invocation {
            path: vec!["session".into(), "pending".into(), "approve".into()],
            args: vec!["0".into()],
            flags: crate::graph::pending::expect_at(0),
            door: aoide_protocol::Door::Cli,
        });
        assert_eq!(approve.status, aoide_protocol::output::Status::Error, "{}", approve.message);
        let data = approve.data.as_ref().unwrap();
        assert_eq!(data["launched"], false, "{data}");
        assert_eq!(data["reason"], "nothing-revived", "{data}");
        assert!(held_entries().is_empty(), "the answered entry left the queue");
        let _ = std::fs::remove_dir_all(&root);
    }


    /// An attested MCP caller keeps today's behaviour: the resurrect launches,
    /// nothing is held, and the restore bytes it would type are still held (the
    /// restore tests above pin that half through the same door).
    #[cfg(unix)]
    #[test]
    fn an_attested_mcp_resurrect_launches_and_holds_nothing() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(RESURRECT_ENV);
        let (root, proj_path) = setup("resurrect-mcp-attested-launch");
        std::env::remove_var("AOIDE_DAEMON_SOCKET");
        std::env::remove_var("AOIDE_SESSION_ID");
        let marker = marker_terminal(&root);
        set_ledger(&[ledger_entry("ledger-old-att", "claude", &proj_path, "2026-08-20T01:00:00Z")]);
        let mut undying = Vec::new();
        aoide_storage::undying::set_undying(&mut undying, "ledger-old-att", true);
        aoide_storage::undying::save_undying(&undying).unwrap();

        let mut mcp = flag_invocation(&["resurrect"], &[("project", "proj")]);
        mcp.door = aoide_protocol::Door::Mcp;
        let out = session_resurrect_with(&mcp, |_| Some("orch".to_string()));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "{}", out.message);
        assert_eq!(out.data.as_ref().unwrap()["resurrected"].as_array().unwrap().len(), 1, "{}", out.message);
        assert!(wait_for(&marker), "an attested caller's resurrect launched");
        assert!(held_entries().is_empty(), "and nothing was held");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Bare mode reads the manifest of this process's working directory and runs
    /// its commands, so a held bare resurrect would be replayed from another
    /// directory and show the approver nothing of what it launches. An
    /// unattested caller on a door that cannot approve is refused with a taught
    /// line, nothing is held, nothing launches; the same bare call from the CLI
    /// door, and from an attested MCP server, still clean-spawns.
    #[cfg(unix)]
    #[test]
    fn a_bare_resurrect_from_an_unattested_door_is_refused_with_a_teaching_line() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(RESURRECT_ENV);
        let this_host = aoide_storage::display::local_host_name();
        let (root, cwd) = setup_manifest(
            "resurrect-mcp-bare",
            vec![manifest_spec(&this_host, ".", "claude", Some("echo from-the-manifest"))],
        );
        std::env::remove_var("AOIDE_DAEMON_SOCKET");
        std::env::remove_var("AOIDE_SESSION_ID");
        let marker = marker_terminal(&root);

        let mut mcp = flag_invocation(&["resurrect"], &[]);
        mcp.door = aoide_protocol::Door::Mcp;
        let out = session_resurrect_with(&mcp, |_| None);
        assert_eq!(out.status, aoide_protocol::output::Status::Error, "{}", out.message);
        let data = out.data.as_ref().unwrap();
        assert_eq!(data["reason"], "bare-resurrect-not-held");
        assert!(data["refusal"]["why"].is_string(), "taught why: {data}");
        assert!(
            data["refusal"]["fix"]["run"].as_str().unwrap().contains("--project"),
            "the fix names flag mode: {data}"
        );
        std::thread::sleep(std::time::Duration::from_millis(600));
        assert!(!marker.exists(), "nothing launched");
        assert!(held_entries().is_empty(), "nothing was held");
        assert_eq!(count_resurrect_audit_lines(&root.join("log")), 1, "the refusal is audited once");

        let attested = session_resurrect_with(&mcp, |_| Some("orch".to_string()));
        assert_eq!(attested.status, aoide_protocol::output::Status::Ok, "{}", attested.message);
        assert_eq!(attested.data.as_ref().unwrap()["resurrected"].as_array().unwrap().len(), 1, "{}", attested.message);
        assert!(wait_for(&marker), "an attested server's bare resurrect launched the manifest session");
        drop(cwd);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A queue file is not trusted to name a command: an entry whose `spawn`
    /// path is neither `spawn` nor `resurrect` is malformed, approve refuses it,
    /// and an entry written before `path` existed is a spawn.
    #[test]
    fn a_held_entry_names_only_a_command_an_approval_may_launch() {
        let known: PendingSpawn = serde_json::from_value(json!({ "args": ["sh"], "flags": {} })).unwrap();
        assert_eq!(known.path, ["spawn"], "an entry from before `path` is a spawn");
        assert!(known.is_known());
        for bad in [json!(["conduct"]), json!(["spawn", "x"]), json!([])] {
            let entry: PendingSpawn = serde_json::from_value(json!({ "path": bad, "args": ["sh"], "flags": {} })).unwrap();
            assert!(!entry.is_known(), "{entry:?}");
        }
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG"]);
        let root = unique_stage("resurrect-held-unknown");
        std::fs::create_dir_all(root.join("stage")).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", root.join("stage"));
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        std::env::set_var("AOIDE_AUDIT_LOG", root.join("log"));
        write_stage(
            &crate::graph::send::pending_path(),
            &json!({ "schemaVersion": "0", "pending": [{
                "sessionId": "x", "text": "t", "from": "mcp",
                "spawn": { "path": ["conduct"], "args": ["sh"], "flags": {} }
            }]}),
        )
        .unwrap();
        let approve = crate::graph::pending::pending_approve(&Invocation {
            path: vec!["session".into(), "pending".into(), "approve".into()],
            args: vec!["0".into()],
            flags: crate::graph::pending::expect_at(0),
            door: aoide_protocol::Door::Cli,
        });
        assert_eq!(approve.status, aoide_protocol::output::Status::Error, "{}", approve.message);
        let raw: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(crate::graph::send::pending_path()).unwrap()).unwrap();
        assert_eq!(raw["pending"].as_array().unwrap().len(), 1, "the entry was not taken");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The `candidates` rows a dry run lists.
    fn candidates(out: &Outcome) -> Vec<serde_json::Value> {
        out.data.as_ref().unwrap()["candidates"].as_array().unwrap().clone()
    }

    fn session_ids(rows: &[serde_json::Value]) -> Vec<&str> {
        rows.iter().map(|r| r["sessionId"].as_str().unwrap()).collect()
    }

    /// Every regular file under `dir` with its bytes, keyed by path. The audit
    /// log is left out: a call is allowed to append to it, nothing else.
    fn snapshot(dir: &std::path::Path) -> std::collections::BTreeMap<std::path::PathBuf, Vec<u8>> {
        fn walk(root: &std::path::Path, dir: &std::path::Path, out: &mut std::collections::BTreeMap<std::path::PathBuf, Vec<u8>>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                let kind = entry.file_type().unwrap();
                if kind.is_dir() {
                    walk(root, &path, out);
                } else if kind.is_file() && path != root.join("log") {
                    out.insert(path.clone(), std::fs::read(&path).unwrap());
                }
            }
        }
        let mut out = std::collections::BTreeMap::new();
        walk(dir, dir, &mut out);
        out
    }

    /// The `status` of every audit line `resurrect` wrote, in file order.
    fn resurrect_audit_statuses(log_path: &std::path::Path) -> Vec<String> {
        std::fs::read_to_string(log_path)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .filter(|v| v.get("command").and_then(serde_json::Value::as_str) == Some("resurrect"))
            .map(|v| v.get("status").and_then(serde_json::Value::as_str).unwrap_or_default().to_string())
            .collect()
    }

    /// The bare `--project P` dry run lists the undying set the bare form
    /// revives: one row per undying id, an id alive now left out, each row
    /// marked as one the bare form picks.
    #[test]
    fn dry_run_bare_form_lists_the_undying_set_with_live_ids_left_out() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG"]);
        let (root, proj_path) = setup("resurrect-dry-bare");

        set_ledger(&[
            ledger_entry("undying-a", "claude", &proj_path, "2026-08-20T01:00:00Z"),
            ledger_entry("undying-live", "claude", &proj_path, "2026-08-20T02:00:00Z"),
            ledger_entry("plain", "claude", &proj_path, "2026-08-20T03:00:00Z"),
        ]);
        let mut undying = Vec::new();
        aoide_storage::undying::set_undying(&mut undying, "undying-a", true);
        aoide_storage::undying::set_undying(&mut undying, "undying-live", true);
        aoide_storage::undying::save_undying(&undying).unwrap();
        write_stage(
            &sessions_path(),
            &SessionsFile {
                schema_version: "0".into(),
                sessions: vec![session("undying-live", &proj_path, "working", "2026-08-20T02:00:00Z", None)],
            },
        )
        .unwrap();

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "proj"), ("dry-run", "true")]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let rows = candidates(&out);
        assert_eq!(session_ids(&rows), vec!["undying-a"], "{rows:?}");
        assert_eq!(rows[0]["pickedByBare"], true, "{rows:?}");
        assert_eq!(rows[0]["agent"], "claude", "{rows:?}");
        assert_eq!(rows[0]["project"], "proj", "{rows:?}");
        assert_eq!(rows[0]["cwd"], proj_path.as_str(), "{rows:?}");
        assert_eq!(rows[0]["endedAt"], "2026-08-20T01:00:00Z", "{rows:?}");
        assert!(rows[0]["restoreText"].is_string(), "{rows:?}");
        assert!(out.message.contains("undying-a"), "{}", out.message);
        assert!(!out.message.contains("undying-live"), "{}", out.message);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// `--all` lists every anchored entry of the project, newest first, and
    /// sets `pickedByBare` on exactly the rows the bare form revives: the
    /// newest row of a duplicated undying id, and no row of an id alive now.
    /// Another project's entries never appear.
    #[test]
    fn dry_run_all_lists_every_anchored_entry_newest_first_and_marks_only_the_bare_picks() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG"]);
        let (root, proj_path) = setup("resurrect-dry-all");

        set_ledger(&[
            ledger_entry("old", "claude", &proj_path, "2026-08-20T01:00:00Z"),
            ledger_entry("dup", "claude", &proj_path, "2026-08-20T03:00:00Z"),
            ledger_entry("dup", "claude", &proj_path, "2026-08-20T04:00:00Z"),
            ledger_entry("plain", "claude", &proj_path, "2026-08-20T05:00:00Z"),
            ledger_entry("live", "claude", &proj_path, "2026-08-20T06:00:00Z"),
            ledger_entry("elsewhere", "claude", "/not/under/the/project", "2026-08-20T07:00:00Z"),
        ]);
        let mut undying = Vec::new();
        for id in ["old", "dup", "live"] {
            aoide_storage::undying::set_undying(&mut undying, id, true);
        }
        aoide_storage::undying::save_undying(&undying).unwrap();
        write_stage(
            &sessions_path(),
            &SessionsFile {
                schema_version: "0".into(),
                sessions: vec![session("live", &proj_path, "working", "2026-08-20T06:00:00Z", None)],
            },
        )
        .unwrap();

        let out = session_resurrect(&flag_invocation(
            &["resurrect"],
            &[("project", "proj"), ("all", "true"), ("dry-run", "true")],
        ));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let rows = candidates(&out);
        let got: Vec<(&str, &str, bool)> = rows
            .iter()
            .map(|r| {
                (
                    r["sessionId"].as_str().unwrap(),
                    r["endedAt"].as_str().unwrap(),
                    r["pickedByBare"].as_bool().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            got,
            vec![
                ("live", "2026-08-20T06:00:00Z", false),
                ("plain", "2026-08-20T05:00:00Z", false),
                ("dup", "2026-08-20T04:00:00Z", true),
                ("dup", "2026-08-20T03:00:00Z", false),
                ("old", "2026-08-20T01:00:00Z", true),
            ],
            "{rows:?}"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The listing is the launch: `--all` lists exactly the anchored entries
    /// `--all` would launch, and the message is the plan `approved_selection`
    /// compares, so approving the plan the dry run showed is accepted.
    #[test]
    fn dry_run_all_lists_exactly_the_anchored_entries_and_approval_accepts_the_plan_shown() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG"]);
        let (root, proj_path) = setup("resurrect-dry-plan");
        let other = unique_stage("resurrect-dry-plan-other");
        let other_str = other.to_str().unwrap().to_string();
        let mut add = invocation(&["project", "add"], &["other", &other_str]);
        add.door = aoide_protocol::Door::Daemon;
        let added = crate::graph::project_add(&add);
        assert_eq!(added.status, aoide_protocol::output::Status::Ok, "msg: {}", added.message);

        set_ledger(&[
            ledger_entry("p-old", "claude", &proj_path, "2026-08-20T01:00:00Z"),
            ledger_entry("p-new", "claude", &proj_path, "2026-08-20T02:00:00Z"),
            ledger_entry("o-1", "claude", &other_str, "2026-08-20T03:00:00Z"),
        ]);

        let out = session_resurrect(&flag_invocation(
            &["resurrect"],
            &[("project", "proj"), ("all", "true"), ("dry-run", "true")],
        ));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let rows = candidates(&out);
        assert_eq!(session_ids(&rows), vec!["p-new", "p-old"], "{rows:?}");

        let launch = flag_invocation(&["resurrect"], &[("project", "proj"), ("all", "true")]);
        let shown = match select_candidates(&launch, "proj") {
            Ok(selection) => plan_lines(&selection),
            Err(o) => panic!("the real --all selection refused: {}", o.message),
        };
        assert_eq!(out.message, shown.join("\n"));
        assert!(approved_selection(&launch, &shown).is_ok(), "approval refused the plan the dry run showed");

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&other);
    }

    /// `--id S` lists one row, marked only when the bare form would revive
    /// that row: an undying id is picked, an ordinary one is not.
    #[test]
    fn dry_run_id_lists_one_row_and_marks_it_only_when_the_bare_form_would_revive_it() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG"]);
        let (root, proj_path) = setup("resurrect-dry-id");

        set_ledger(&[
            ledger_entry("u", "claude", &proj_path, "2026-08-20T01:00:00Z"),
            ledger_entry("n", "claude", &proj_path, "2026-08-20T02:00:00Z"),
        ]);
        let mut undying = Vec::new();
        aoide_storage::undying::set_undying(&mut undying, "u", true);
        aoide_storage::undying::save_undying(&undying).unwrap();

        let picked = session_resurrect(&flag_invocation(
            &["resurrect"],
            &[("project", "proj"), ("id", "u"), ("dry-run", "true")],
        ));
        assert_eq!(picked.status, aoide_protocol::output::Status::Ok, "msg: {}", picked.message);
        let rows = candidates(&picked);
        assert_eq!(session_ids(&rows), vec!["u"], "{rows:?}");
        assert_eq!(rows[0]["pickedByBare"], true, "{rows:?}");

        let plain = session_resurrect(&flag_invocation(
            &["resurrect"],
            &[("project", "proj"), ("id", "n"), ("dry-run", "true")],
        ));
        assert_eq!(plain.status, aoide_protocol::output::Status::Ok, "msg: {}", plain.message);
        let rows = candidates(&plain);
        assert_eq!(session_ids(&rows), vec!["n"], "{rows:?}");
        assert_eq!(rows[0]["pickedByBare"], false, "{rows:?}");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A dry run in every flag mode launches nothing (the recording terminal
    /// never runs), holds nothing, writes no state file (the audit log aside),
    /// and writes exactly one audit line per call, marked `dry-run`.
    #[cfg(unix)]
    #[test]
    fn dry_run_launches_nothing_holds_nothing_and_writes_no_state() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(RESURRECT_ENV);
        let (root, proj_path) = setup("resurrect-dry-zero");
        std::env::remove_var("AOIDE_DAEMON_SOCKET");
        std::env::remove_var("AOIDE_SESSION_ID");
        let marker = marker_terminal(&root);
        set_ledger(&[
            ledger_entry("u", "claude", &proj_path, "2026-08-20T01:00:00Z"),
            ledger_entry("p", "claude", &proj_path, "2026-08-20T02:00:00Z"),
        ]);
        let mut undying = Vec::new();
        aoide_storage::undying::set_undying(&mut undying, "u", true);
        aoide_storage::undying::save_undying(&undying).unwrap();
        let before = snapshot(&root);

        for flags in [
            vec![("project", "proj"), ("dry-run", "true")],
            vec![("project", "proj"), ("all", "true"), ("dry-run", "true")],
            vec![("project", "proj"), ("id", "p"), ("dry-run", "true")],
        ] {
            let out = session_resurrect(&flag_invocation(&["resurrect"], &flags));
            assert_eq!(out.status, aoide_protocol::output::Status::Ok, "{flags:?}: {}", out.message);
        }
        no_marker_within_the_launch_wait(&marker);
        assert!(held_entries().is_empty(), "a dry run queued a hold: {:?}", held_entries());
        assert_eq!(snapshot(&root), before, "a dry run wrote state");
        assert_eq!(resurrect_audit_statuses(&root.join("log")), vec!["dry-run".to_string(); 3]);

        // The positive control: the same fixture's real call launches within
        // `LAUNCH_WAIT`, so the check above would have caught such a launch.
        let launched = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "proj"), ("id", "p")]));
        assert_eq!(launched.status, aoide_protocol::output::Status::Ok, "msg: {}", launched.message);
        assert!(wait_for(&marker), "the real call launched on this fixture");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// `resurrect` opens only to the CLI and daemon doors, so a door that
    /// cannot approve is reached only here, by calling the handler directly.
    /// The real call holds the whole command; a dry run with `--project` is
    /// refused instead, holding nothing, launching nothing and showing the
    /// caller no ledger field.
    #[cfg(unix)]
    #[test]
    fn dry_run_on_a_door_that_cannot_approve_holds_nothing_and_launches_nothing() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(RESURRECT_ENV);
        let (root, proj_path) = setup("resurrect-dry-mcp");
        std::env::remove_var("AOIDE_DAEMON_SOCKET");
        std::env::remove_var("AOIDE_SESSION_ID");
        let marker = marker_terminal(&root);
        set_ledger(&[ledger_entry("u", "claude", &proj_path, "2026-08-20T01:00:00Z")]);
        let mut undying = Vec::new();
        aoide_storage::undying::set_undying(&mut undying, "u", true);
        aoide_storage::undying::save_undying(&undying).unwrap();
        let before = snapshot(&root);

        for flags in [
            vec![("project", "proj"), ("dry-run", "true")],
            vec![("project", "proj"), ("all", "true"), ("dry-run", "true")],
            vec![("project", "proj"), ("id", "u"), ("dry-run", "true")],
        ] {
            let mut dry = flag_invocation(&["resurrect"], &flags);
            dry.door = Door::Mcp;
            let out = session_resurrect_with(&dry, |_| None::<String>);
            assert_eq!(out.status, aoide_protocol::output::Status::Error, "{flags:?}: {}", out.message);
            assert_eq!(out.data.as_ref().unwrap()["reason"], "dry-run-not-held", "{flags:?}");
            assert_eq!(out.data.as_ref().unwrap()["refusal"]["kind"], serde_json::json!(Kind::Refused), "{flags:?}");
            let shown = serde_json::to_string(&out).unwrap();
            for key in ["sessionId", "cwd", "restoreText", "agent"] {
                assert!(!shown.contains(&format!("\"{key}\"")), "{flags:?} shows `{key}`: {shown}");
            }
        }
        assert_eq!(resurrect_audit_statuses(&root.join("log")), vec!["dry-run".to_string(); 3]);
        assert!(held_entries().is_empty(), "a dry run queued a hold: {:?}", held_entries());
        assert_eq!(snapshot(&root), before, "a dry run wrote state");
        no_marker_within_the_launch_wait(&marker);

        let mut real = flag_invocation(&["resurrect"], &[("project", "proj")]);
        real.door = Door::Mcp;
        let held = session_resurrect_with(&real, |_| None::<String>);
        assert_eq!(held.data.as_ref().unwrap()["state"], "pending", "{}", held.message);
        assert_eq!(held_entries().len(), 1, "the same call without the flag is the one hold");

        // The positive control: the CLI door's real call launches the undying
        // set on this fixture within `LAUNCH_WAIT`, so the check above would
        // have caught such a launch.
        let launched = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "proj")]));
        assert_eq!(launched.status, aoide_protocol::output::Status::Ok, "msg: {}", launched.message);
        assert!(wait_for(&marker), "the real call launched on this fixture");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A dry run without `--project` from a door that cannot approve is the
    /// real call's own usage error: the same status, message and data, with no
    /// hold or launch before it. `--id` and `--all` are the two forms.
    #[cfg(unix)]
    #[test]
    fn a_projectless_dry_run_gets_the_real_calls_outcome_and_holds_nothing() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(RESURRECT_ENV);
        let (root, proj_path) = setup("resurrect-projectless-mcp");
        std::env::remove_var("AOIDE_DAEMON_SOCKET");
        std::env::remove_var("AOIDE_SESSION_ID");
        let marker = marker_terminal(&root);
        set_ledger(&[ledger_entry("u", "claude", &proj_path, "2026-08-20T01:00:00Z")]);
        let mut undying = Vec::new();
        aoide_storage::undying::set_undying(&mut undying, "u", true);
        aoide_storage::undying::save_undying(&undying).unwrap();
        let before = snapshot(&root);

        for flags in [vec![("id", "u")], vec![("all", "true")]] {
            let mut real = flag_invocation(&["resurrect"], &flags);
            real.door = Door::Mcp;
            let real_out = session_resurrect_with(&real, |_| None::<String>);
            assert!(real_out.message.contains("missing required flag --project"), "{flags:?}: {}", real_out.message);

            let mut dry_flags = flags.clone();
            dry_flags.push(("dry-run", "true"));
            let mut dry = flag_invocation(&["resurrect"], &dry_flags);
            dry.door = Door::Mcp;
            let dry_out = session_resurrect_with(&dry, |_| None::<String>);
            assert_eq!(dry_out.status, real_out.status, "{flags:?}");
            assert_eq!(dry_out.message, real_out.message, "{flags:?}");
            assert_eq!(dry_out.data, real_out.data, "{flags:?}");
        }
        assert!(held_entries().is_empty(), "a dry run queued a hold: {:?}", held_entries());
        assert_eq!(snapshot(&root), before, "a dry run wrote state");
        no_marker_within_the_launch_wait(&marker);

        // The positive control: the CLI door's real call launches the undying
        // set on this fixture within `LAUNCH_WAIT`, so the check above would
        // have caught such a launch.
        let launched = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "proj"), ("all", "true")]));
        assert_eq!(launched.status, aoide_protocol::output::Status::Ok, "msg: {}", launched.message);
        assert!(wait_for(&marker), "the real call launched on this fixture");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The bare form is refused from an unattested caller on a door that cannot
    /// approve, dry or not: a bare dry run gets the refusal the real call gets,
    /// writes one audit line marked `dry-run`, and launches and holds nothing.
    /// The same fixture's CLI-door call launches within `LAUNCH_WAIT`, so the
    /// check below would catch such a launch.
    #[cfg(unix)]
    #[test]
    fn a_bare_dry_run_from_an_unattested_door_gets_the_real_refusal_and_launches_nothing() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(RESURRECT_ENV);
        let this_host = aoide_storage::display::local_host_name();
        let (root, cwd) = setup_manifest(
            "resurrect-dry-bare-refusal",
            vec![manifest_spec(&this_host, ".", "claude", Some("echo from-the-manifest"))],
        );
        std::env::remove_var("AOIDE_DAEMON_SOCKET");
        std::env::remove_var("AOIDE_SESSION_ID");
        let marker = marker_terminal(&root);

        let mut real = flag_invocation(&["resurrect"], &[]);
        real.door = Door::Mcp;
        let refused = session_resurrect_with(&real, |_| None::<String>);
        assert_eq!(refused.status, aoide_protocol::output::Status::Error, "{}", refused.message);
        assert_eq!(refused.data.as_ref().unwrap()["reason"], "bare-resurrect-not-held");

        let mut dry = flag_invocation(&["resurrect"], &[("dry-run", "true")]);
        dry.door = Door::Mcp;
        let answer = session_resurrect_with(&dry, |_| None::<String>);
        assert_eq!(answer.status, refused.status, "{}", answer.message);
        assert_eq!(answer.message, refused.message);
        assert_eq!(answer.data, refused.data, "the dry run gives the real call's refusal");
        assert_eq!(
            resurrect_audit_statuses(&root.join("log")),
            vec!["refused".to_string(), "dry-run".to_string()],
            "the dry run writes one audit line, marked dry-run"
        );
        assert!(held_entries().is_empty(), "nothing was held");

        no_marker_within_the_launch_wait(&marker);

        let launched = session_resurrect(&flag_invocation(&["resurrect"], &[]));
        assert_eq!(launched.status, aoide_protocol::output::Status::Ok, "msg: {}", launched.message);
        assert!(wait_for(&marker), "the real bare call launched on this fixture");
        drop(cwd);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Bare mode's dry run lists each spec with the decision its launch would
    /// make: a clean spawn, a ledger revival, a refusal, or a remote summon
    /// whose local pre-wire check passes. A remote spec is never sent: the
    /// node's port takes no connection and nothing launches. From a door that
    /// cannot approve, a bare dry run is refused instead, as the real call is
    /// (`a_bare_dry_run_from_an_unattested_door_gets_the_real_refusal_and_launches_nothing`).
    #[cfg(unix)]
    #[test]
    fn dry_run_bare_mode_lists_the_manifest_specs_and_launches_nothing() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(RESURRECT_ENV);
        let host = aoide_storage::display::local_host_name();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let (root, cwd) = setup_manifest(
            "resurrect-dry-manifest",
            vec![
                manifest_spec(&host, ".", "claude", Some("claude --resume-me")),
                manifest_spec(&host, "sub", "claude", None),
                manifest_spec(&host, ".", "no-such-harness", None),
                manifest_spec("nowhere", ".", "claude", Some("true")),
                manifest_spec("sakaki", ".", "claude", Some("git -C /home/khoa/Aoide pull --ff-only")),
            ],
        );
        std::env::remove_var("AOIDE_DAEMON_SOCKET");
        std::env::remove_var("AOIDE_SESSION_ID");
        let marker = marker_terminal(&root);
        std::fs::create_dir_all(root.join("sub")).unwrap();
        let sub_str = root.join("sub").to_str().unwrap().to_string();
        set_ledger(&[ledger_entry("sub-old", "claude", &sub_str, "2026-08-20T01:00:00Z")]);
        aoide_storage::node_store::save_nodes(&[verified_node("sakaki", &url)]).unwrap();
        let before = snapshot(&root);

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[("dry-run", "true")]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let rows = candidates(&out);
        let dispositions: Vec<&str> = rows.iter().map(|r| r["disposition"].as_str().unwrap()).collect();
        assert_eq!(
            dispositions,
            vec!["clean-spawned", "revived-from-ledger", "failed", "failed", "summoned-remote"],
            "{rows:?}"
        );
        assert_eq!(rows[0]["argv"], serde_json::json!(["claude", "--resume-me"]), "{rows:?}");
        assert_eq!(rows[1]["resumedFrom"], "sub-old", "{rows:?}");
        assert!(rows[2]["reason"].as_str().unwrap().contains("no `command` given"), "{rows:?}");
        assert!(rows[3]["reason"].as_str().unwrap().contains("not a registered node"), "{rows:?}");
        assert_eq!(rows[4]["text"], "git -C /home/khoa/Aoide pull --ff-only", "{rows:?}");
        assert!(listener.accept().is_err(), "a dry run connected to the node");

        assert!(held_entries().is_empty(), "a dry run queued a hold: {:?}", held_entries());
        assert_eq!(resurrect_audit_statuses(&root.join("log")), vec!["dry-run".to_string()], "one audit line");

        no_marker_within_the_launch_wait(&marker);
        assert_eq!(snapshot(&root), before, "a dry run wrote state");

        // The positive control: the same fixture's real call launches within
        // `LAUNCH_WAIT`, so the check above would have caught such a launch.
        // The node is dropped first:
        // a summon to a listening port that never answers would hang.
        drop(listener);
        let launched = session_resurrect(&flag_invocation(&["resurrect"], &[]));
        assert_eq!(launched.status, aoide_protocol::output::Status::Ok, "msg: {}", launched.message);
        assert!(wait_for(&marker), "the real bare call launched on this fixture");
        drop(cwd);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A dry run of a flag mode refuses an unknown project or an unknown id
    /// with the refusal the real call gives. Its audit line is written where
    /// the real call's is, marked `dry-run`.
    #[test]
    fn dry_run_refuses_an_unknown_project_or_id_as_the_real_call_does() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG"]);
        let (root, _proj_path) = setup("resurrect-dry-refusals");

        for flags in [vec![("project", "nope")], vec![("project", "proj"), ("id", "no-such-id")]] {
            let real = session_resurrect(&flag_invocation(&["resurrect"], &flags));
            let mut dry_flags = flags.clone();
            dry_flags.push(("dry-run", "true"));
            let dry = session_resurrect(&flag_invocation(&["resurrect"], &dry_flags));
            assert_eq!(dry.status, real.status, "{flags:?}");
            assert_eq!(dry.message, real.message, "{flags:?}");
            assert_eq!(dry.data, real.data, "{flags:?}");
        }
        let statuses = resurrect_audit_statuses(&root.join("log"));
        assert_eq!(statuses.iter().filter(|s| s.as_str() == "error").count(), 2, "{statuses:?}");
        assert_eq!(statuses.iter().filter(|s| s.as_str() == "dry-run").count(), 2, "{statuses:?}");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The dry run's bare-mode rows agree with the real launch for local
    /// specs: the same spec gets the same disposition, so the listing is what
    /// the launch does. (A remote spec's wire outcome is not predicted, so it
    /// is not in this comparison.)
    #[cfg(unix)]
    #[test]
    fn dry_run_manifest_dispositions_match_the_real_launch_for_local_specs() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(RESURRECT_ENV);
        let host = aoide_storage::display::local_host_name();
        let (root, cwd) = setup_manifest(
            "resurrect-dry-parity",
            vec![
                manifest_spec(&host, ".", "claude", Some("claude --resume-me")),
                manifest_spec(&host, "sub", "claude", None),
                manifest_spec(&host, ".", "no-such-harness", None),
            ],
        );
        let _marker = marker_terminal(&root);
        std::fs::create_dir_all(root.join("sub")).unwrap();
        let sub_str = root.join("sub").to_str().unwrap().to_string();
        set_ledger(&[ledger_entry("sub-old", "claude", &sub_str, "2026-08-20T01:00:00Z")]);

        let dry = session_resurrect(&flag_invocation(&["resurrect"], &[("dry-run", "true")]));
        assert_eq!(dry.status, aoide_protocol::output::Status::Ok, "msg: {}", dry.message);
        let mut dry_dispositions: Vec<String> = candidates(&dry)
            .iter()
            .map(|r| r["disposition"].as_str().unwrap().to_string())
            .collect();

        let real = session_resurrect(&flag_invocation(&["resurrect"], &[]));
        assert_eq!(real.status, aoide_protocol::output::Status::Ok, "msg: {}", real.message);
        let data = real.data.as_ref().unwrap();
        let mut real_dispositions: Vec<String> = ["resurrected", "skipped", "failed"]
            .iter()
            .flat_map(|bucket| data[*bucket].as_array().unwrap().iter())
            .map(|r| r["disposition"].as_str().unwrap().to_string())
            .collect();

        dry_dispositions.sort();
        real_dispositions.sort();
        assert_eq!(dry_dispositions, vec!["clean-spawned", "failed", "revived-from-ledger"]);
        assert_eq!(dry_dispositions, real_dispositions);

        drop(cwd);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A ledger line's ids and agent are an agent's own text. A dry run prints
    /// them spelled, as the pending view spells a held plan line, and its data
    /// keeps them raw. The held plan is the raw plan, which approval accepts.
    #[test]
    fn dry_run_spells_a_hostile_ledger_entry_and_keeps_the_plan_raw() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG"]);
        let (root, proj_path) = setup("resurrect-dry-hostile-flag");
        let id = "s\u{1b}\r\u{202e}x";
        let agent = "a\u{1b}\r\u{202e}";
        set_ledger(&[ledger_entry(id, agent, &proj_path, "2026-08-20T01:00:00Z")]);

        let dry = session_resurrect(&flag_invocation(
            &["resurrect"],
            &[("project", "proj"), ("all", "true"), ("dry-run", "true")],
        ));
        assert_eq!(dry.status, aoide_protocol::output::Status::Ok, "msg: {}", dry.message);
        assert_eq!(dry.message, "s\\x1b\\r\\u{202e}x (a\\x1b\\r\\u{202e}): no restore recorded, nothing typed");
        let rows = candidates(&dry);
        assert_eq!(rows[0]["sessionId"], id);
        assert_eq!(rows[0]["agent"], agent);
        let log = std::fs::read_to_string(root.join("log")).unwrap();
        let audit: serde_json::Value = serde_json::from_str(log.lines().last().unwrap()).unwrap();
        assert_eq!(
            audit["message"].as_str().unwrap(),
            aoide_protocol::untrusted::escape_for_log(&dry.message),
            "the audit line carries the message the dry run answers"
        );

        let launch = flag_invocation(&["resurrect"], &[("project", "proj"), ("all", "true")]);
        let shown = held_plan(&launch, "proj");
        let raw = match select_candidates(&launch, "proj") {
            Ok(selection) => plan_lines(&selection),
            Err(o) => panic!("the real --all selection refused: {}", o.message),
        };
        assert_eq!(shown, raw, "the held plan is the raw plan");
        assert_eq!(shown, vec![format!("{id} ({agent}): no restore recorded, nothing typed")]);
        assert!(approved_selection(&launch, &shown).is_ok(), "approval accepts the plan it was held with");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A manifest's own text is as untrusted as a ledger's. A spec whose dir
    /// and command hold ESC, CR and U+202E shows spelled in the bare dry run's
    /// message, and its data keeps the raw strings.
    #[cfg(unix)]
    #[test]
    fn dry_run_bare_mode_spells_a_hostile_spec_and_keeps_its_data_raw() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(RESURRECT_ENV);
        let this_host = aoide_storage::display::local_host_name();
        let dir = "d\u{1b}\r\u{202e}";
        let command = "tool\u{1b}x \u{202e}y\r";
        let (root, cwd) = setup_manifest(
            "resurrect-dry-hostile-bare",
            vec![manifest_spec(&this_host, dir, "claude", Some(command))],
        );
        std::env::remove_var("AOIDE_DAEMON_SOCKET");
        std::env::remove_var("AOIDE_SESSION_ID");

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[("dry-run", "true")]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let rows = candidates(&out);
        assert_eq!(rows[0]["dir"], dir);
        assert_eq!(rows[0]["command"], command);
        assert_eq!(rows[0]["argv"], serde_json::json!(["tool\u{1b}x", "\u{202e}y"]));
        let lines: Vec<&str> = out.message.split('\n').collect();
        assert_eq!(lines.len(), 2, "a header line, then one line per spec: {}", out.message);
        assert!(lines[0].starts_with("manifest at `"), "{}", out.message);
        assert_eq!(lines[1], "d\\x1b\\r\\u{202e} (claude): clean spawn `'tool\\x1bx' '\\u{202e}y'`");
        assert!(
            out.message.chars().all(|c| c == '\n' || (' '..='~').contains(&c)),
            "a raw control or format character in the message: {:?}",
            out.message
        );

        drop(cwd);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// An attested MCP caller is not refused, dry or not: its bare dry run
    /// lists the manifest's specs and its flag-mode dry run lists the ledger
    /// rows. Neither launches. The ledger row anchors the spec's directory, so
    /// the bare row is the revival the launch would make.
    #[cfg(unix)]
    #[test]
    fn an_attested_mcp_dry_run_lists_and_launches_nothing() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(RESURRECT_ENV);
        let this_host = aoide_storage::display::local_host_name();
        let (root, cwd) = setup_manifest(
            "resurrect-dry-attested",
            vec![manifest_spec(&this_host, ".", "claude", Some("echo from-the-manifest"))],
        );
        std::env::remove_var("AOIDE_DAEMON_SOCKET");
        std::env::remove_var("AOIDE_SESSION_ID");
        let marker = marker_terminal(&root);
        let root_str = root.to_str().unwrap().to_string();
        let mut add = invocation(&["project", "add"], &["proj", &root_str]);
        add.door = Door::Daemon;
        assert_eq!(crate::graph::project_add(&add).status, aoide_protocol::output::Status::Ok);
        set_ledger(&[ledger_entry("u", "claude", &root_str, "2026-08-20T01:00:00Z")]);

        let mut bare = flag_invocation(&["resurrect"], &[("dry-run", "true")]);
        bare.door = Door::Mcp;
        let listed = session_resurrect_with(&bare, |_| Some("sess".to_string()));
        assert_eq!(listed.status, aoide_protocol::output::Status::Ok, "msg: {}", listed.message);
        assert_eq!(candidates(&listed)[0]["disposition"], "revived-from-ledger", "{}", listed.message);

        let mut flag = flag_invocation(&["resurrect"], &[("project", "proj"), ("all", "true"), ("dry-run", "true")]);
        flag.door = Door::Mcp;
        let rows = session_resurrect_with(&flag, |_| Some("sess".to_string()));
        assert_eq!(rows.status, aoide_protocol::output::Status::Ok, "msg: {}", rows.message);
        assert_eq!(session_ids(&candidates(&rows)), vec!["u"]);

        assert_eq!(resurrect_audit_statuses(&root.join("log")), vec!["dry-run".to_string(); 2]);
        assert!(held_entries().is_empty(), "a dry run queued a hold: {:?}", held_entries());
        no_marker_within_the_launch_wait(&marker);

        // The positive control: the CLI door's real call launches the anchored
        // ledger rows on this fixture within `LAUNCH_WAIT`, so the check above
        // would have caught such a launch.
        let launched = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "proj"), ("all", "true")]));
        assert_eq!(launched.status, aoide_protocol::output::Status::Ok, "msg: {}", launched.message);
        assert!(wait_for(&marker), "the real call launched on this fixture");
        drop(cwd);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A manifest root's own directory name is as untrusted as a spec's text.
    /// A root named with ESC, CR and U+202E shows spelled in the header line of
    /// a bare dry run's message, and `data.manifestRoot` keeps the raw path.
    #[cfg(unix)]
    #[test]
    fn dry_run_spells_a_hostile_manifest_root_name_and_keeps_its_data_raw() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(RESURRECT_ENV);
        let this_host = aoide_storage::display::local_host_name();
        let specs = || vec![manifest_spec(&this_host, ".", "claude", Some("echo from-the-manifest"))];
        let (root, cwd) = setup_manifest("resurrect-dry-hostile-root", specs());
        let hostile = root.join("r\u{1b}\r\u{202e}t");
        std::fs::create_dir_all(&hostile).unwrap();
        aoide_storage::manifest::save_manifest(
            &hostile,
            &aoide_storage::manifest::Manifest { version: aoide_storage::manifest::MANIFEST_VERSION, sessions: specs() },
        )
        .unwrap();
        let hostile_cwd = CwdGuard::enter(&hostile);
        std::env::remove_var("AOIDE_DAEMON_SOCKET");
        std::env::remove_var("AOIDE_SESSION_ID");

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[("dry-run", "true")]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let lines: Vec<&str> = out.message.split('\n').collect();
        assert_eq!(lines.len(), 2, "a header line, then one line per spec: {}", out.message);
        assert!(lines[0].contains("r\\x1b\\r\\u{202e}t"), "the header spells the root name: {}", lines[0]);
        assert!(
            out.message.chars().all(|c| c == '\n' || (' '..='~').contains(&c)),
            "a raw control or format character in the message: {:?}",
            out.message
        );
        assert_eq!(out.data.as_ref().unwrap()["manifestRoot"], hostile.to_str().unwrap());

        drop(hostile_cwd);
        drop(cwd);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A dry run writes nothing to `pending.json`: a seeded file is unchanged
    /// byte for byte; the real call on the same door adds exactly one entry.
    #[cfg(unix)]
    #[test]
    fn a_dry_run_leaves_a_seeded_pending_file_byte_for_byte_and_a_real_hold_adds_one() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(RESURRECT_ENV);
        let (root, proj_path) = setup("resurrect-dry-pending");
        std::env::remove_var("AOIDE_DAEMON_SOCKET");
        std::env::remove_var("AOIDE_SESSION_ID");
        set_ledger(&[ledger_entry("u", "claude", &proj_path, "2026-08-20T01:00:00Z")]);
        let mut undying = Vec::new();
        aoide_storage::undying::set_undying(&mut undying, "u", true);
        aoide_storage::undying::save_undying(&undying).unwrap();

        let mut seed = flag_invocation(&["resurrect"], &[("project", "proj"), ("all", "true")]);
        seed.door = Door::Mcp;
        let seeded = session_resurrect_with(&seed, |_| None::<String>);
        assert_eq!(seeded.data.as_ref().unwrap()["state"], "pending", "{}", seeded.message);
        assert_eq!(held_entries().len(), 1, "the seed is one held entry");
        let before = std::fs::read(crate::graph::send::pending_path()).unwrap();

        let mut dry = flag_invocation(&["resurrect"], &[("project", "proj"), ("dry-run", "true")]);
        dry.door = Door::Mcp;
        let out = session_resurrect_with(&dry, |_| None::<String>);
        assert_eq!(out.data.as_ref().unwrap()["reason"], "dry-run-not-held", "{}", out.message);
        assert_eq!(std::fs::read(crate::graph::send::pending_path()).unwrap(), before, "a dry run changed pending.json");

        let mut real = flag_invocation(&["resurrect"], &[("project", "proj")]);
        real.door = Door::Mcp;
        let held = session_resurrect_with(&real, |_| None::<String>);
        assert_eq!(held.data.as_ref().unwrap()["state"], "pending", "{}", held.message);
        assert_eq!(held_entries().len(), 2, "the real call adds exactly one held entry");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A recording `$AOIDE_TERMINAL`: each launch appends a `RUN` line, then one
    /// line per argv word, so a test reads back exactly what the terminal was
    /// handed. Unix only, like every launch test here.
    #[cfg(unix)]
    fn recording_terminal(root: &std::path::Path) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let rec = root.join("terminal.rec");
        let script = root.join("record-terminal.sh");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nprintf 'RUN\\n' >> '{rec}'\nfor a in \"$@\"; do printf '%s\\n' \"$a\" >> '{rec}'; done\n",
                rec = rec.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("AOIDE_TERMINAL", &script);
        std::env::set_var("WAYLAND_DISPLAY", "wayland-0");
        rec
    }

    /// Every launch the recording terminal saw, as its argv words in order.
    #[cfg(unix)]
    fn recorded_launches(rec: &std::path::Path) -> Vec<Vec<String>> {
        let raw = std::fs::read_to_string(rec).unwrap_or_default();
        let mut runs: Vec<Vec<String>> = Vec::new();
        for line in raw.lines() {
            if line == "RUN" {
                runs.push(Vec::new());
            } else if let Some(run) = runs.last_mut() {
                run.push(line.to_string());
            }
        }
        runs
    }

    /// An ended eidolon session with a journal named by its ledger line is
    /// resumed once, as `eidolon tui --session <journal>`.
    #[cfg(unix)]
    #[test]
    fn an_ended_eidolon_session_resumes_from_the_journal_its_ledger_line_names() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(RESURRECT_ENV);
        let (root, proj_path) = setup("resurrect-by-log");
        let rec = recording_terminal(&root);
        let journal = root.join("s.eid");
        std::fs::write(&journal, "").unwrap();
        let journal = journal.to_string_lossy().into_owned();
        set_ledger(&[ledger_entry_with_log("ledger-eid", "eidolon", &proj_path, Some(journal.as_str()))]);

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "proj"), ("id", "ledger-eid")]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let launches = recorded_launches(&rec);
        assert_eq!(launches.len(), 1, "one launch; launches: {launches:?}, data: {:?}", out.data);
        let argv = &launches[0];
        let tail: Vec<&str> = argv[argv.len() - 4..].iter().map(String::as_str).collect();
        assert_eq!(tail, ["eidolon", "tui", "--session", journal.as_str()]);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The journal path reaches the terminal as one argv word, even with a
    /// space and a quote in it.
    #[cfg(unix)]
    #[test]
    fn a_journal_path_with_a_space_and_a_quote_stays_one_argv_word() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(RESURRECT_ENV);
        let (root, proj_path) = setup("resurrect-by-log-awkward");
        let rec = recording_terminal(&root);
        let journal = root.join("my journal's.eid");
        std::fs::write(&journal, "").unwrap();
        let journal = journal.to_string_lossy().into_owned();
        set_ledger(&[ledger_entry_with_log("ledger-eid", "eidolon", &proj_path, Some(journal.as_str()))]);

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "proj"), ("id", "ledger-eid")]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let launches = recorded_launches(&rec);
        assert_eq!(launches.len(), 1, "one launch; launches: {launches:?}");
        let argv = &launches[0];
        assert_eq!(argv.last().unwrap(), &journal, "argv: {argv:?}");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A by-journal candidate whose ledger `logPath` is skipped by `log_path`'s
    /// value launches nothing, and both its launch and a dry run give one
    /// reason, which names the entry and the cause.
    #[cfg(unix)]
    fn assert_by_log_skip(
        tag: &str,
        log_path: impl FnOnce(&std::path::Path) -> Option<String>,
        cause: &str,
    ) {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(RESURRECT_ENV);
        let (root, proj_path) = setup(tag);
        let rec = recording_terminal(&root);
        let log_path = log_path(&root);
        set_ledger(&[ledger_entry_with_log("ledger-eid", "eidolon", &proj_path, log_path.as_deref())]);

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "proj"), ("id", "ledger-eid")]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let skipped = out.data.as_ref().unwrap()["skipped"].as_array().unwrap().clone();
        assert_eq!(skipped.len(), 1, "data: {:?}", out.data);
        let reason = skipped[0]["reason"].as_str().unwrap().to_string();
        assert!(reason.contains("ledger-eid") && reason.contains(cause), "reason: {reason}");

        let dry = session_resurrect(&flag_invocation(
            &["resurrect"],
            &[("project", "proj"), ("id", "ledger-eid"), ("dry-run", "true")],
        ));
        assert_eq!(dry.status, aoide_protocol::output::Status::Ok, "msg: {}", dry.message);
        let row = &dry.data.as_ref().unwrap()["candidates"][0];
        assert_eq!(row["disposition"], "skipped", "dry-run row: {row}");
        assert_eq!(row["reason"].as_str().unwrap(), reason, "the dry run must give the launch's reason");

        assert!(recorded_launches(&rec).is_empty(), "a skipped candidate launched a terminal");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn a_by_log_entry_without_a_log_path_is_skipped_with_its_reason() {
        assert_by_log_skip("resurrect-no-log-path", |_| None, "has no logPath");
    }

    #[cfg(unix)]
    #[test]
    fn a_by_log_entry_with_a_relative_log_path_is_skipped_with_its_reason() {
        assert_by_log_skip(
            "resurrect-relative-log",
            |_| Some("relative/s.eid".to_string()),
            "not an absolute, plain path",
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_by_log_entry_whose_log_path_holds_an_escape_is_skipped_with_its_reason() {
        assert_by_log_skip(
            "resurrect-esc-log",
            |_| Some("/nonexistent-root/s\u{1b}.eid".to_string()),
            "not an absolute, plain path",
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_by_log_entry_whose_log_path_is_a_terminal_transcript_is_skipped_with_its_reason() {
        assert_by_log_skip(
            "resurrect-transcript-log",
            |root| {
                let dir = root.join("state").join("sessions");
                std::fs::create_dir_all(&dir).unwrap();
                let transcript = dir.join("ledger-eid.log");
                std::fs::write(&transcript, "transcript").unwrap();
                Some(transcript.to_string_lossy().into_owned())
            },
            "session logs dir",
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_by_log_entry_whose_log_path_names_no_file_is_skipped_with_its_reason() {
        assert_by_log_skip(
            "resurrect-missing-log",
            |root| Some(root.join("missing.eid").to_string_lossy().into_owned()),
            "names no regular file",
        );
    }

    /// The journal is the session, so a by-journal revival keeps the undying
    /// mark on the ledger id, and its new id gets none.
    #[cfg(unix)]
    #[test]
    fn an_undying_by_log_entry_keeps_its_mark_and_its_revival_gets_none() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(RESURRECT_ENV);
        let (root, proj_path) = setup("resurrect-by-log-undying");
        let _rec = recording_terminal(&root);
        let journal = root.join("s.eid");
        std::fs::write(&journal, "").unwrap();
        let journal = journal.to_string_lossy().into_owned();
        set_ledger(&[ledger_entry_with_log("ledger-eid", "eidolon", &proj_path, Some(journal.as_str()))]);
        let mut undying = Vec::new();
        aoide_storage::undying::set_undying(&mut undying, "ledger-eid", true);
        aoide_storage::undying::save_undying(&undying).unwrap();

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "proj"), ("id", "ledger-eid")]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let resurrected = out.data.as_ref().unwrap()["resurrected"].as_array().unwrap().clone();
        assert_eq!(resurrected.len(), 1, "data: {:?}", out.data);
        let new_id = resurrected[0]["sessionId"].as_str().unwrap().to_string();

        let undying = aoide_storage::undying::load_undying();
        assert!(aoide_storage::undying::is_undying(&undying, "ledger-eid"), "the journal is the session: its mark stays");
        assert!(!aoide_storage::undying::is_undying(&undying, &new_id), "the new id must get no mark");

        let _ = std::fs::remove_dir_all(&root);
    }

    // -- One engine per session (ruling 165) --

    const NOW: i64 = 1_800_000_000;

    fn claim_taken(session_id: &str, new_id: &str, age_secs: i64) -> aoide_storage::resuming::Claim {
        aoide_storage::resuming::Claim {
            session_id: session_id.to_string(),
            new_id: new_id.to_string(),
            at: aoide_storage::time::iso_utc_from_epoch(NOW - age_secs),
            journal: None,
        }
    }

    fn roster_record(id: &str, state: &str) -> aoide_storage::records::SessionRecord {
        aoide_storage::records::SessionRecord {
            session_id: id.to_string(),
            state: state.to_string(),
            ..Default::default()
        }
    }

    /// The block as these tests state it: the candidate's journal and the open
    /// journals resolved once through [`Journal::of`], which resolves nothing for a
    /// path that does not exist.
    fn blocked(
        entry: &aoide_storage::ledger::LedgerEntry,
        by_log: bool,
        sessions: &[aoide_storage::records::SessionRecord],
        claims: &[aoide_storage::resuming::Claim],
        presence: &[(String, String)],
        now: i64,
    ) -> Option<Blocked> {
        let candidate = candidate_journal(entry, by_log);
        let roster = roster_journals(sessions);
        let presence: Vec<(String, Journal)> =
            presence.iter().map(|(id, log)| (id.clone(), Journal::of(log))).collect();
        block_verdict(entry, candidate.as_ref(), sessions, &roster, claims, &presence, now)
    }

    #[test]
    fn a_live_record_under_the_candidates_own_id_blocks_it() {
        let entry = ledger_entry("ledger-claude", "claude", "/w", "2026-08-20T01:00:00Z");
        let sessions = [roster_record("ledger-claude", "running")];
        assert_eq!(
            blocked(&entry, false, &sessions, &[], &[], NOW),
            Some(Blocked::Live { live_id: "ledger-claude".into() })
        );
    }

    #[test]
    fn a_live_resume_of_the_candidate_blocks_it() {
        let entry = ledger_entry("ledger-claude", "claude", "/w", "2026-08-20T01:00:00Z");
        let mut resumed = roster_record("resurrect-9-1", "idle");
        resumed.resumed_from = Some("ledger-claude".into());
        assert_eq!(
            blocked(&entry, false, &[resumed], &[], &[], NOW),
            Some(Blocked::Live { live_id: "resurrect-9-1".into() })
        );
    }

    #[test]
    fn a_live_record_on_the_candidates_journal_blocks_a_by_log_candidate() {
        let entry = ledger_entry_with_log("ledger-eid", "eidolon", "/w", Some("/r/s.eid"));
        let mut live = roster_record("eid-live", "running");
        live.log_path = Some("/r/s.eid".into());
        assert_eq!(
            blocked(&entry, true, &[live], &[], &[], NOW),
            Some(Blocked::Live { live_id: "eid-live".into() })
        );
    }

    #[test]
    fn an_open_journal_of_the_presence_scan_blocks_a_by_log_candidate() {
        let entry = ledger_entry_with_log("ledger-eid", "eidolon", "/w", Some("/r/s.eid"));
        let journals = [("eid-open".to_string(), "/r/s.eid".to_string())];
        assert_eq!(
            blocked(&entry, true, &[], &[], &journals, NOW),
            Some(Blocked::Live { live_id: "eid-open".into() })
        );
    }

    #[test]
    fn a_claim_whose_new_id_is_live_is_in_flight() {
        let entry = ledger_entry("ledger-claude", "claude", "/w", "2026-08-20T01:00:00Z");
        let claims = [claim_taken("ledger-claude", "resurrect-9-2", 5)];
        let sessions = [roster_record("resurrect-9-2", "running")];
        assert_eq!(
            blocked(&entry, false, &sessions, &claims, &[], NOW),
            Some(Blocked::InFlight { new_id: "resurrect-9-2".into(), age_secs: 5, by_journal: false })
        );
    }

    #[test]
    fn a_young_claim_with_an_unregistered_new_id_is_in_flight() {
        let entry = ledger_entry("ledger-claude", "claude", "/w", "2026-08-20T01:00:00Z");
        let claims = [claim_taken("ledger-claude", "resurrect-9-3", 5)];
        assert_eq!(
            blocked(&entry, false, &[], &claims, &[], NOW),
            Some(Blocked::InFlight { new_id: "resurrect-9-3".into(), age_secs: 5, by_journal: false })
        );
    }

    #[test]
    fn an_old_claim_with_an_unregistered_new_id_is_stale_and_blocks_nothing() {
        let entry = ledger_entry("ledger-claude", "claude", "/w", "2026-08-20T01:00:00Z");
        let claims = [claim_taken("ledger-claude", "resurrect-9-4", 3600)];
        assert_eq!(blocked(&entry, false, &[], &claims, &[], NOW), None);
    }

    #[test]
    fn a_claim_whose_new_id_is_done_is_stale_and_blocks_nothing() {
        let entry = ledger_entry("ledger-claude", "claude", "/w", "2026-08-20T01:00:00Z");
        let claims = [claim_taken("ledger-claude", "resurrect-9-5", 5)];
        let sessions = [roster_record("resurrect-9-5", "done")];
        assert_eq!(blocked(&entry, false, &sessions, &claims, &[], NOW), None);
    }

    #[test]
    fn a_candidate_with_nothing_against_it_is_free() {
        let entry = ledger_entry("ledger-claude", "claude", "/w", "2026-08-20T01:00:00Z");
        assert_eq!(blocked(&entry, false, &[], &[], &[], NOW), None);
    }

    #[test]
    fn a_claim_stamped_three_seconds_ahead_blocks_and_its_age_reads_0s() {
        let entry = ledger_entry("ledger-claude", "claude", "/w", "2026-08-20T01:00:00Z");
        let claims = [claim_taken("ledger-claude", "resurrect-9-7", -3)];
        assert_eq!(
            blocked(&entry, false, &[], &claims, &[], NOW),
            Some(Blocked::InFlight { new_id: "resurrect-9-7".into(), age_secs: 0, by_journal: false })
        );
    }

    #[test]
    fn a_claim_stamped_an_hour_ahead_blocks_nothing() {
        let entry = ledger_entry("ledger-claude", "claude", "/w", "2026-08-20T01:00:00Z");
        let claims = [claim_taken("ledger-claude", "resurrect-9-8", -3600)];
        assert_eq!(blocked(&entry, false, &[], &claims, &[], NOW), None);
    }

    #[test]
    fn a_journal_that_resolves_to_the_candidates_file_blocks_it_though_spelled_apart() {
        let entry = ledger_entry_with_log("ledger-eid", "eidolon", "/w", Some("/r/s.eid"));
        let candidate = Journal { spelled: "/r/s.eid".into(), resolved: Some("/real/s.eid".into()) };
        let presence = [(
            "eid-open".to_string(),
            Journal { spelled: "/r/link.eid".into(), resolved: Some("/real/s.eid".into()) },
        )];
        assert_eq!(
            block_verdict(&entry, Some(&candidate), &[], &[], &[], &presence, NOW),
            Some(Blocked::Live { live_id: "eid-open".into() })
        );
    }

    #[test]
    fn a_roster_log_that_resolves_to_the_candidates_file_blocks_it_though_spelled_apart() {
        let entry = ledger_entry_with_log("ledger-eid", "eidolon", "/w", Some("/r/s.eid"));
        let candidate = Journal { spelled: "/r/s.eid".into(), resolved: Some("/real/s.eid".into()) };
        let sessions = [aoide_storage::records::SessionRecord {
            session_id: "eid-roster".into(),
            state: "running".into(),
            log_path: Some("/r/link.eid".into()),
            ..Default::default()
        }];
        let roster = [Some(Journal { spelled: "/r/link.eid".into(), resolved: Some("/real/s.eid".into()) })];
        assert_eq!(
            block_verdict(&entry, Some(&candidate), &sessions, &roster, &[], &[], NOW),
            Some(Blocked::Live { live_id: "eid-roster".into() })
        );
    }

    /// A recording terminal that also copies the claims file at each launch
    /// into `claims-at-launch`, so a test can read what the launch saw.
    #[cfg(unix)]
    fn recording_terminal_with_claims(root: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let rec = root.join("terminal.rec");
        let dump = root.join("claims-at-launch");
        let claims = aoide_storage::resuming::resuming_path();
        let script = root.join("record-terminal.sh");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nprintf 'RUN\\n' >> '{rec}'\nfor a in \"$@\"; do printf '%s\\n' \"$a\" >> '{rec}'; done\ncat '{claims}' > '{dump}' 2>/dev/null || printf 'absent\\n' > '{dump}'\n",
                rec = rec.display(),
                claims = claims.display(),
                dump = dump.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("AOIDE_TERMINAL", &script);
        std::env::set_var("WAYLAND_DISPLAY", "wayland-0");
        (rec, dump)
    }

    /// Two `aoide resurrect --project P --id X` processes started together: one
    /// launches X, and the other is refused as in flight, or as live.
    #[cfg(unix)]
    #[test]
    fn two_resurrects_started_together_launch_once() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(RESURRECT_ENV);
        let (root, proj_path) = setup("resurrect-two-procs");
        let (rec, dump) = recording_terminal_with_claims(&root);
        set_ledger(&[ledger_entry("ledger-claude", "claude", &proj_path, "2026-08-20T01:00:00Z")]);

        let bin = built_aoide_bin();
        let run = || {
            std::process::Command::new(&bin)
                .args(["resurrect", "--project", "proj", "--id", "ledger-claude", "--json"])
                .env_remove("AOIDE_SESSION_ID")
                .env_remove("AOIDE_DAEMON_SOCKET")
                .output()
                .unwrap()
        };
        let (first, second) = std::thread::scope(|s| {
            let a = s.spawn(|| run());
            let b = s.spawn(|| run());
            (a.join().unwrap(), b.join().unwrap())
        });

        let launches = recorded_launches(&rec);
        assert_eq!(launches.len(), 1, "exactly one launch; launches: {launches:?}");
        let new_id = launches[0]
            .iter()
            .find_map(|w| w.strip_prefix("--id="))
            .expect("the launch names its new id")
            .to_string();
        let at_launch = std::fs::read_to_string(&dump).unwrap_or_default();
        assert!(
            at_launch.contains(&new_id) && at_launch.contains("ledger-claude"),
            "the claim is written before the launch it guards; at launch: {at_launch}"
        );
        let texts: Vec<String> = [first, second]
            .iter()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .collect();
        assert!(
            texts.iter().any(|t| t.contains("in flight") || t.contains("is live")),
            "the other run must be refused; stdout: {texts:?}"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A launch that fails leaves no claim behind, so a later resurrect of the
    /// same session is not blocked by one.
    #[cfg(unix)]
    #[test]
    fn a_failed_launch_leaves_no_claim_and_a_later_resurrect_is_not_blocked() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(RESURRECT_ENV);
        let (root, proj_path) = setup("resurrect-failed-claim");
        set_ledger(&[ledger_entry("ledger-claude", "claude", &proj_path, "2026-08-20T01:00:00Z")]);
        std::env::remove_var("AOIDE_TERMINAL");

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "proj"), ("id", "ledger-claude")]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let failed = out.data.as_ref().unwrap()["failed"].as_array().unwrap().len();
        assert_eq!(failed, 1, "data: {:?}", out.data);
        let claims = aoide_storage::resuming::load_claims().unwrap();
        assert!(claims.iter().all(|c| c.session_id != "ledger-claude"), "a failed launch drops its claim: {claims:?}");

        let (rec, _dump) = recording_terminal_with_claims(&root);
        let out = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "proj"), ("id", "ledger-claude")]));
        assert_eq!(recorded_launches(&rec).len(), 1, "a later resurrect launches; data: {:?}", out.data);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A by-journal candidate whose presence scan cannot say which journals are
    /// open is refused, and nothing launches.
    /// It listens so its meta is read: nothing listening means skipped unread.
    #[cfg(unix)]
    #[test]
    fn a_by_log_candidate_with_an_unknown_presence_scan_is_skipped_and_nothing_launches() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(RESURRECT_ENV);
        let (root, proj_path) = setup("resurrect-presence-unknown");
        let journal = root.join("s.eid");
        std::fs::write(&journal, "").unwrap();
        let presence = root.join("eidolon").join("garbage");
        std::fs::create_dir_all(&presence).unwrap();
        std::fs::write(presence.join("meta.json"), "{ not json").unwrap();
        let _held_open = std::os::unix::net::UnixListener::bind(presence.join("sock")).unwrap();
        let journal = journal.to_string_lossy().into_owned();
        set_ledger(&[ledger_entry_with_log("ledger-eid", "eidolon", &proj_path, Some(journal.as_str()))]);
        let (rec, _dump) = recording_terminal_with_claims(&root);

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "proj"), ("id", "ledger-eid")]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let skipped = out.data.as_ref().unwrap()["skipped"].as_array().unwrap().clone();
        assert_eq!(skipped.len(), 1, "data: {:?}", out.data);
        let reason = skipped[0]["reason"].as_str().unwrap();
        assert!(reason.contains("presence"), "reason: {reason}");
        assert!(recorded_launches(&rec).is_empty(), "an unknown presence scan launched a terminal");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A by-journal candidate whose presence socket accepts and never answers is
    /// still open: the presence's pid is this process, which the real `ps` lists,
    /// so the session is live and nothing launches.
    #[cfg(unix)]
    #[test]
    fn a_by_log_candidate_whose_presence_is_silent_is_skipped_as_live() {
        use std::os::unix::net::UnixListener;
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(RESURRECT_ENV);
        let (root, proj_path) = setup("resurrect-presence-silent");
        let journal = root.join("s.eid");
        std::fs::write(&journal, "").unwrap();
        let journal = journal.to_string_lossy().into_owned();
        let presence = root.join("eidolon").join("user-silent");
        std::fs::create_dir_all(&presence).unwrap();
        std::fs::write(
            presence.join("meta.json"),
            serde_json::json!({
                "id": "user-silent", "pid": std::process::id(), "log": journal,
                "cwd": proj_path, "repo": null, "model": "m", "started_ms": 0,
                "title": "t", "busy": false
            })
            .to_string(),
        )
        .unwrap();
        let _held_open = UnixListener::bind(presence.join("sock")).unwrap();
        set_ledger(&[ledger_entry_with_log("ledger-eid", "eidolon", &proj_path, Some(journal.as_str()))]);
        let (rec, _dump) = recording_terminal_with_claims(&root);

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "proj"), ("id", "ledger-eid")]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let skipped = out.data.as_ref().unwrap()["skipped"].as_array().unwrap().clone();
        assert_eq!(skipped.len(), 1, "data: {:?}", out.data);
        let reason = skipped[0]["reason"].as_str().unwrap();
        assert!(reason.contains("is live as user-silent"), "the live sentence names the presence id: {reason}");
        assert!(recorded_launches(&rec).is_empty(), "a silent, pid-live presence launched a terminal");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A claim stamped an hour ahead of the clock is stale: it blocks nothing,
    /// the claim step drops it, and the candidate launches once.
    #[cfg(unix)]
    #[test]
    fn a_claim_stamped_an_hour_ahead_does_not_block_and_the_candidate_launches_once() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(RESURRECT_ENV);
        let (root, proj_path) = setup("resurrect-claim-future");
        set_ledger(&[ledger_entry("ledger-claude", "claude", &proj_path, "2026-08-20T01:00:00Z")]);
        let future = aoide_storage::time::iso_utc_from_epoch(super::super::conduct::unix_ts() as i64 + 3600);
        aoide_storage::resuming::save_claims(&[aoide_storage::resuming::Claim {
            session_id: "ledger-claude".into(),
            new_id: "resurrect-future-1".into(),
            at: future,
            journal: None,
        }])
        .unwrap();
        let (rec, _dump) = recording_terminal_with_claims(&root);

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "proj"), ("id", "ledger-claude")]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        assert_eq!(recorded_launches(&rec).len(), 1, "data: {:?}", out.data);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Two ledger lines for one id: the older names no journal, the newer names
    /// one. `--id` takes the newest line, so it launches once, by its journal.
    #[cfg(unix)]
    #[test]
    fn an_id_with_two_ledger_lines_launches_once_by_the_newest_journal() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(RESURRECT_ENV);
        let (root, proj_path) = setup("resurrect-id-newest");
        let journal = root.join("s.eid");
        std::fs::write(&journal, "").unwrap();
        let journal = journal.to_string_lossy().into_owned();
        set_ledger(&[
            ledger_entry_with_log("ledger-eid", "eidolon", &proj_path, None),
            ledger_entry_with_log("ledger-eid", "eidolon", &proj_path, Some(journal.as_str())),
        ]);
        let (rec, _dump) = recording_terminal_with_claims(&root);

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "proj"), ("id", "ledger-eid")]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let launches = recorded_launches(&rec);
        assert_eq!(launches.len(), 1, "data: {:?}", out.data);
        assert!(launches[0].contains(&journal), "launched by the newest line's journal: {:?}", launches[0]);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A `logPath` that is a link to a regular journal is skipped by its own
    /// sentence: the check reads the path without following the link.
    #[cfg(unix)]
    #[test]
    fn a_log_path_that_is_a_link_to_a_journal_is_skipped_and_nothing_launches() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(RESURRECT_ENV);
        let (root, proj_path) = setup("resurrect-journal-link");
        let journal = root.join("s.eid");
        std::fs::write(&journal, "").unwrap();
        let link = root.join("link.eid");
        std::os::unix::fs::symlink(&journal, &link).unwrap();
        let link = link.to_string_lossy().into_owned();
        set_ledger(&[ledger_entry_with_log("ledger-eid", "eidolon", &proj_path, Some(link.as_str()))]);
        let (rec, _dump) = recording_terminal_with_claims(&root);

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "proj"), ("id", "ledger-eid")]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let skipped = out.data.as_ref().unwrap()["skipped"].as_array().unwrap().clone();
        assert_eq!(skipped.len(), 1, "data: {:?}", out.data);
        let reason = skipped[0]["reason"].as_str().unwrap();
        assert!(reason.contains("has a logPath that is a symbolic link"), "reason: {reason}");
        assert!(recorded_launches(&rec).is_empty(), "a link to a journal launched a terminal");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A quoted `'{cmd}'` template joins the launch into one shell line. A
    /// journal path holding `$(id)`, a backtick, a `'` and a space must reach
    /// the recorder as one argv word, byte for byte.
    #[cfg(unix)]
    #[test]
    fn a_quoted_terminal_template_hands_a_hostile_journal_path_on_as_one_word() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(RESURRECT_ENV);
        let (root, proj_path) = setup("resurrect-hostile-path");
        let journal = root.join("it's $(id) `x` s.eid");
        std::fs::write(&journal, "").unwrap();
        let journal = journal.to_string_lossy().into_owned();
        set_ledger(&[ledger_entry_with_log("ledger-eid", "eidolon", &proj_path, Some(journal.as_str()))]);
        // The recorder takes the joined line as its one argument and splits it
        // the way a shell would, so each word it prints is what the terminal runs.
        let rec = root.join("terminal.rec");
        let script = root.join("record-terminal.sh");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nprintf 'RUN\\n' >> '{rec}'\neval \"set -- $1\"\nfor a in \"$@\"; do printf '%s\\n' \"$a\" >> '{rec}'; done\n",
                rec = rec.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("AOIDE_TERMINAL", format!("{} '{{cmd}}'", script.display()));
        std::env::set_var("WAYLAND_DISPLAY", "wayland-0");

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "proj"), ("id", "ledger-eid")]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let launches = recorded_launches(&rec);
        assert_eq!(launches.len(), 1, "data: {:?}", out.data);
        assert_eq!(
            launches[0].last().map(String::as_str),
            Some(journal.as_str()),
            "the journal reaches the recorder as one word: {:?}",
            launches[0]
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A conducting stage lock that cannot be taken refuses the candidate: the
    /// claim cannot be written under the lock, so nothing launches.
    #[cfg(unix)]
    #[test]
    fn a_stage_lock_that_cannot_be_taken_skips_the_candidate_and_nothing_launches() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(RESURRECT_ENV);
        let (root, proj_path) = setup("resurrect-lock-unopenable");
        set_ledger(&[ledger_entry("ledger-claude", "claude", &proj_path, "2026-08-20T01:00:00Z")]);
        // `setup`'s `project add` has already taken the stage lock, leaving
        // the lock file in place; a directory at its path makes the open fail.
        let lock = root.join("stage").join(".stage.lock");
        std::fs::remove_file(&lock).unwrap();
        std::fs::create_dir(&lock).unwrap();
        let (rec, _dump) = recording_terminal_with_claims(&root);

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "proj"), ("id", "ledger-claude")]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        let skipped = out.data.as_ref().unwrap()["skipped"].as_array().unwrap().clone();
        assert_eq!(skipped.len(), 1, "data: {:?}", out.data);
        let reason = skipped[0]["reason"].as_str().unwrap();
        assert!(reason.contains("stage lock"), "reason: {reason}");
        assert!(recorded_launches(&rec).is_empty(), "an untaken stage lock launched a terminal");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A dry run reports a fresh claim as in flight, and leaves the claims file
    /// exactly as it found it, whether present or absent.
    #[cfg(unix)]
    #[test]
    fn a_dry_run_against_a_fresh_claim_shows_in_flight_and_leaves_the_claims_file_alone() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(RESURRECT_ENV);
        let (root, proj_path) = setup("resurrect-dry-claim");
        set_ledger(&[ledger_entry("ledger-claude", "claude", &proj_path, "2026-08-20T01:00:00Z")]);
        let (rec, _dump) = recording_terminal_with_claims(&root);
        let path = aoide_storage::resuming::resuming_path();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let bytes = format!(
            r#"{{"v":1,"claims":[{{"sessionId":"ledger-claude","newId":"resurrect-held-1","at":"{}"}}]}}"#,
            aoide_storage::time::now_iso_utc()
        );
        std::fs::write(&path, &bytes).unwrap();

        let dry = || {
            session_resurrect(&flag_invocation(
                &["resurrect"],
                &[("project", "proj"), ("id", "ledger-claude"), ("dry-run", "true")],
            ))
        };
        let out = dry();
        let row = out.data.as_ref().unwrap()["candidates"][0].clone();
        assert_eq!(row["disposition"], "skipped", "row: {row}");
        assert!(row["reason"].as_str().unwrap().contains("in flight"), "row: {row}");
        assert_eq!(row["claimedBy"], "resurrect-held-1", "row: {row}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), bytes, "a dry run must not touch the claims file");

        std::fs::remove_file(&path).unwrap();
        let _ = dry();
        assert!(!path.exists(), "a dry run must not create the claims file");
        assert!(recorded_launches(&rec).is_empty(), "a dry run launched a terminal");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// An ended eidolon ledger line for `session_id`, anchored at `cwd`, whose
    /// journal is `log_path`, with its own `ended_at`.
    fn eidolon_line_ended(session_id: &str, cwd: &str, ended_at: &str, log_path: &str) -> aoide_storage::ledger::LedgerEntry {
        let mut line = serde_json::to_value(ledger_entry(session_id, "eidolon", cwd, ended_at)).unwrap();
        line["logPath"] = json!(log_path);
        serde_json::from_value(line).unwrap()
    }

    /// A claim for another session on the candidate's journal blocks it while
    /// the claim is in flight, and the block is by journal.
    #[test]
    fn a_claim_for_another_id_on_the_same_journal_blocks() {
        let entry = ledger_entry_with_log("ledger-b", "eidolon", "/w", Some("/r/s.eid"));
        let candidate = Journal { spelled: "/r/s.eid".into(), resolved: Some("/real/s.eid".into()) };
        let mut claim = claim_taken("ledger-a", "resurrect-a-1", 5);
        claim.journal = Some("/real/s.eid".into());
        assert_eq!(
            block_verdict(&entry, Some(&candidate), &[], &[], &[claim], &[], NOW),
            Some(Blocked::InFlight { new_id: "resurrect-a-1".into(), age_secs: 5, by_journal: true })
        );
    }

    /// The same claim blocks nothing once it is stale.
    #[test]
    fn a_stale_claim_for_another_id_on_the_same_journal_blocks_nothing() {
        let entry = ledger_entry_with_log("ledger-b", "eidolon", "/w", Some("/r/s.eid"));
        let candidate = Journal { spelled: "/r/s.eid".into(), resolved: Some("/real/s.eid".into()) };
        let mut claim = claim_taken("ledger-a", "resurrect-a-1", 3600);
        claim.journal = Some("/real/s.eid".into());
        assert_eq!(block_verdict(&entry, Some(&candidate), &[], &[], &[claim], &[], NOW), None);
    }

    /// A claim with no journal blocks only the session it names.
    #[test]
    fn a_claim_without_a_journal_blocks_only_its_own_id() {
        let entry = ledger_entry_with_log("ledger-b", "eidolon", "/w", Some("/r/s.eid"));
        let candidate = Journal { spelled: "/r/s.eid".into(), resolved: Some("/real/s.eid".into()) };
        let other = claim_taken("ledger-a", "resurrect-a-1", 5);
        assert_eq!(block_verdict(&entry, Some(&candidate), &[], &[], &[other], &[], NOW), None);
        let own = claim_taken("ledger-b", "resurrect-b-1", 5);
        assert_eq!(
            block_verdict(&entry, Some(&candidate), &[], &[], &[own], &[], NOW),
            Some(Blocked::InFlight { new_id: "resurrect-b-1".into(), age_secs: 5, by_journal: false })
        );
    }

    /// `--all` over two ended eidolon lines with distinct ids and one journal:
    /// the newer line launches, and the older is skipped, its journal held by
    /// the newer line's launch.
    #[cfg(unix)]
    #[test]
    fn an_all_run_launches_one_of_two_lines_that_share_a_journal() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(RESURRECT_ENV);
        let (root, proj_path) = setup("resurrect-all-shared-journal");
        let journal = root.join("s.eid");
        std::fs::write(&journal, "").unwrap();
        let journal = journal.to_string_lossy().into_owned();
        set_ledger(&[
            eidolon_line_ended("ledger-old", &proj_path, "2026-08-19T01:00:00Z", &journal),
            eidolon_line_ended("ledger-new", &proj_path, "2026-08-20T01:00:00Z", &journal),
        ]);
        let (rec, _dump) = recording_terminal_with_claims(&root);

        let out = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "proj"), ("all", "true")]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);
        assert_eq!(recorded_launches(&rec).len(), 1, "one launch; message: {}", out.message);
        let claimed = aoide_storage::resuming::load_claims()
            .unwrap()
            .into_iter()
            .find(|c| c.session_id == "ledger-new")
            .expect("the newer line holds the claim");
        let data = out.data.as_ref().expect("the run reports its rows");
        let skipped = data["skipped"].as_array().expect("the older line is skipped");
        assert_eq!(skipped.len(), 1, "data: {data}");
        assert_eq!(skipped[0]["sessionId"], "ledger-old");
        assert_eq!(skipped[0]["claimedBy"], claimed.new_id.as_str());
        let reason = skipped[0]["reason"].as_str().unwrap_or_default();
        assert!(reason.contains("shares its journal with a launch in flight as"), "reason: {reason}");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// `--id` of one line launches it. `--id` of the other line, on the same
    /// journal, is refused, and its skipped row names the first launch's claim.
    #[cfg(unix)]
    #[test]
    fn a_second_id_on_a_journal_a_launch_holds_is_refused_across_calls() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(RESURRECT_ENV);
        let (root, proj_path) = setup("resurrect-second-id-journal");
        let journal = root.join("s.eid");
        std::fs::write(&journal, "").unwrap();
        let journal = journal.to_string_lossy().into_owned();
        set_ledger(&[
            eidolon_line_ended("ledger-old", &proj_path, "2026-08-19T01:00:00Z", &journal),
            eidolon_line_ended("ledger-new", &proj_path, "2026-08-20T01:00:00Z", &journal),
        ]);
        let (rec, _dump) = recording_terminal_with_claims(&root);

        let first = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "proj"), ("id", "ledger-new")]));
        assert_eq!(first.status, aoide_protocol::output::Status::Ok, "msg: {}", first.message);
        assert_eq!(recorded_launches(&rec).len(), 1, "the first id launches; message: {}", first.message);
        let claimed = aoide_storage::resuming::load_claims()
            .unwrap()
            .into_iter()
            .find(|c| c.session_id == "ledger-new")
            .expect("the first launch holds a claim")
            .new_id;

        let second = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "proj"), ("id", "ledger-old")]));
        assert_eq!(recorded_launches(&rec).len(), 1, "the other line launches nothing; message: {}", second.message);
        let data = second.data.as_ref().expect("the refusal is a skipped row");
        let skipped = data["skipped"].as_array().expect("the skipped rows");
        assert_eq!(skipped.len(), 1, "data: {data}");
        assert_eq!(skipped[0]["claimedBy"], claimed.as_str());
        let reason = skipped[0]["reason"].as_str().unwrap_or_default();
        assert!(reason.contains("shares its journal with a launch in flight as"), "reason: {reason}");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A by-journal launch's claim names its journal, resolved; an id launch's
    /// claim names none.
    #[cfg(unix)]
    #[test]
    fn a_by_journal_claim_names_its_journal_and_an_id_claim_names_none() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(RESURRECT_ENV);
        let (root, proj_path) = setup("resurrect-claim-journal");
        let journal_path = root.join("s.eid");
        std::fs::write(&journal_path, "").unwrap();
        let resolved = std::fs::canonicalize(&journal_path).unwrap().to_string_lossy().into_owned();
        let journal = journal_path.to_string_lossy().into_owned();
        set_ledger(&[
            eidolon_line_ended("ledger-eid", &proj_path, "2026-08-20T01:00:00Z", &journal),
            ledger_entry("ledger-claude", "claude", &proj_path, "2026-08-20T02:00:00Z"),
        ]);
        let (rec, _dump) = recording_terminal_with_claims(&root);

        let by_journal = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "proj"), ("id", "ledger-eid")]));
        assert_eq!(by_journal.status, aoide_protocol::output::Status::Ok, "msg: {}", by_journal.message);
        let by_id = session_resurrect(&flag_invocation(&["resurrect"], &[("project", "proj"), ("id", "ledger-claude")]));
        assert_eq!(by_id.status, aoide_protocol::output::Status::Ok, "msg: {}", by_id.message);
        assert_eq!(recorded_launches(&rec).len(), 2, "both launch; message: {}", by_id.message);

        let raw: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(aoide_storage::resuming::resuming_path()).unwrap()).unwrap();
        let claims = raw["claims"].as_array().expect("the claims array");
        let eid = claims.iter().find(|c| c["sessionId"] == "ledger-eid").expect("the by-journal claim");
        assert_eq!(eid["journal"], resolved.as_str(), "claim: {eid}");
        let claude = claims.iter().find(|c| c["sessionId"] == "ledger-claude").expect("the id claim");
        assert!(claude.get("journal").is_none(), "an id launch's claim has no journal key: {claude}");

        let _ = std::fs::remove_dir_all(&root);
    }
}
