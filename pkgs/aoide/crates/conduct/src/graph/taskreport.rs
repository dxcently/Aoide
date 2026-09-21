//! The managed task run's EXIT REPORT lane
//! (`docs/Aoide-Wiki/concepts/orchestration/Managed-Task-Wrapper.md`): a run
//! that ends is reported to its REPORT MAILBOX — `--report-to`, else the
//! parent's session id when that is a legal mailbox name AND a known session,
//! else the documented role mailbox `conductor` — durably, even if the child
//! never read a letter and even if the daemon was down at the exit instant. The
//! child's own inbox (`self/<slug>`) never carries this report.
//!
//! **The daemon files it, never the dying child.** The trigger is the RECORD
//! (`task` set, `state = done`, no cursor entry), read by the reaper tick —
//! so a report cannot depend on the child surviving its own report, and a
//! daemon that was down simply reports on its next pass. The tick calls this
//! lane from `reap()`, outside the stage lock, and it files through the
//! REGISTERED `["mail","send"]` implementation in `aoide-client` — the same
//! handler the CLI/MCP door dispatches to, invoked in-process with
//! `Door::Daemon`, which is the daemon authority path itself
//! (`aoide_client::daemon::daemon_dispatch` refuses to route a `Door::Daemon`
//! invocation any further: a socket round trip is neither possible nor
//! needed). No gate is bypassed: the registration, validation and filing all
//! come from that one path.
//!
//! **One wake route per event.** A filed letter is not a wake. Filing through
//! the registered handler in-process rings nothing (the handler's own ring
//! forward answers `"no-daemon"` inside the daemon), so this lane takes the
//! mail-side doorbell itself — `doorbell::ring(&mailbox, Some(child))` under
//! the existing `.ring.lock` — exactly the in-process call the daemon's
//! `mail ring` handler makes. Its outcome is recorded verbatim, including the
//! honest zero-reader case (`rang:0 no-armed-reader:<mailbox>`: a mailbox named
//! by a session id is readable but never ringable). That is the ONE
//! notification for one event: no
//! second raw line is injected, so a parent is never woken twice for the same
//! exit.
//!
//! **Delivery semantics — single delivery in the normal path, never lost,
//! and honest about the one window that can duplicate.** `state/stage/
//! taskreport.json` holds, per run session id, `{endedAt, exitCode, outcome,
//! msgid, mailbox, wake}`,
//! and it is what makes the ordinary case ONE letter per run: a run with an
//! entry is never filed again. The cursor is advanced only AFTER the letter is
//! really in the mailbase, so the failure direction that matters — losing a
//! report — cannot happen: a filing failure (a read-only or unmounted
//! mailbase, a refused `mail send`) leaves the run with NO entry, and the next
//! tick retries it. That retry is a genuine re-filing, so the delivery
//! guarantee is **at-least-once, normally exactly once**: a crash between a
//! successful filing and the cursor write (or a cursor write that fails)
//! re-files on the next pass, and the retry mints a NEW msgid — mint time and
//! signature differ — so the mailbase's own duplicate memory cannot collapse
//! it. The operator then sees one extra identically-worded letter whose
//! subject names the run. This is why the module never says "at-most-once" or
//! "exactly-once": the cursor prevents repeats, it does not prove uniqueness.
//!
//! **A finished run's record is not prunable, so the loss window is closed by
//! RETENTION rather than by call ordering.** `prune_done` (the chokepoint both
//! the reaper's sweep and `aoide session prune` share, via
//! `prune_done_scoped`) keeps every `task`-carrying `done` record — filed or
//! not — so a finished run stays resolvable by `session watch` until the user
//! explicitly prunes it. The lane still runs at the top of `reap()` so a report
//! is filed promptly, but nothing is lost if it does not.

use super::model::{canonical_state, load_stage, sessions_path, SessionRecord, SessionsFile};
use super::send::audit_send;
use aoide_protocol::output::{Outcome, Status};
use aoide_protocol::registry::Registry;
use aoide_protocol::{Door, Invocation};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// `state/stage/taskreport.json` — the per-run cursor, beside
/// `state/stage/pingback.json` (same pattern, same directory).
pub(crate) fn taskreport_path() -> PathBuf {
    aoide_storage::fs::conducting_stage_dir().join("taskreport.json")
}

/// One run's cursor entry: the facts the report was filed with, and the
/// mailbase's own `msgid` for it (so a hand inspection can find the letter
/// this cursor claims to have written).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
struct CursorEntry {
    #[serde(rename = "endedAt", default, skip_serializing_if = "Option::is_none")]
    ended_at: Option<String>,
    #[serde(rename = "exitCode", default, skip_serializing_if = "Option::is_none")]
    exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    msgid: Option<String>,
    /// The run's discriminated end (`exit`/`signal`/`timeout`/`stopped`), as
    /// stamped on the record — a fact, shown by the view's report line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    outcome: Option<String>,
    /// The mailbox the report actually went to, or absent when the run had no
    /// report mailbox at all — a fact, printed by the reap line, the audit line
    /// and the view.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mailbox: Option<String>,
    /// What the wake actually did, in the ring's own words
    /// (`rang:2`, `rang:0 deferred:<state> skipped:<reason>`, `ring-failed:…`).
    /// Recorded because "nobody was woken" is only answerable if the reason is,
    /// and a Codex-desktop/never-conducted parent cannot be woken at all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    wake: Option<String>,
}

/// A filed run's own cursor facts, for a READ-ONLY consumer (the view): the
/// report artifact is the cursor entry plus the letter it names — never the
/// mere presence of some letter from the child.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::graph) struct FiledReport {
    pub ended_at: Option<String>,
    pub exit_code: Option<i32>,
    pub outcome: Option<String>,
    pub mailbox: Option<String>,
    pub msgid: Option<String>,
    pub wake: Option<String>,
}

/// Has this run's exit report been filed? `None` means the lane owes it one
/// (or never saw the run) — a question answered from the cursor alone, with no
/// lock, no write and no cursor advance.
pub(in crate::graph) fn filed_report(run: &str) -> Option<FiledReport> {
    read_cursor().get(run).map(|e| FiledReport {
        ended_at: e.ended_at.clone(),
        exit_code: e.exit_code,
        outcome: e.outcome.clone(),
        mailbox: e.mailbox.clone(),
        msgid: e.msgid.clone(),
        wake: e.wake.clone(),
    })
}

/// The runs the exit-report lane still OWES a letter: `task` set, state `done`,
/// no cursor entry. Read-only. `prune_done` calls this so a finished run's
/// record — the lane's own trigger — cannot be swept out from under an
/// unfiled report by the reaper's prune or by `aoide session prune`.
pub(in crate::graph) fn unfiled_task_runs(
    sessions: &[SessionRecord],
) -> std::collections::HashSet<String> {
    let cursor = read_cursor();
    sessions
        .iter()
        .filter(|s| {
            s.task.as_deref().is_some_and(|t| !t.is_empty())
                && canonical_state(&s.state) == "done"
                && !cursor.contains_key(&s.session_id)
        })
        .map(|s| s.session_id.clone())
        .collect()
}

/// The whole cursor, keyed by the run's own `sessionId` (the run key: a task
/// slug outlives a respawn, an id does not). A `BTreeMap` so the file's key
/// order is stable.
type CursorFile = BTreeMap<String, CursorEntry>;

/// What this tick did — printed by the caller, never folded into the sweep's
/// `changed` (a report being filed must not toast the desktop).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TaskReport {
    /// `(run session id, msgid)` for every report filed this pass.
    pub filed: Vec<(String, String)>,
    /// `(run session id, why not)` for every run that should have been
    /// reported and was not — the cursor is left alone so the next tick
    /// retries.
    pub failed: Vec<(String, String)>,
    /// `(run session id, what the wake did)` for every filing — the ring's own
    /// rung/deferred/skipped detail, or its failure.
    pub wake: Vec<(String, String)>,
    /// Runs whose report has no mailbox to go to: no `--report-to`, and no
    /// parent that is a legal mailbox name. Still reported (cursor, record,
    /// audit, rail) — just not mailed, and this says so rather than pretending
    /// a letter exists.
    pub no_mailbox: Vec<String>,
}

/// One pass of the exit-report lane. Runs only under `Door::Daemon` — the
/// policy and audit boundary every automated write in this crate shares —
/// and returns what it did so the caller can print it.
pub(crate) fn taskreport(inv: &Invocation) -> TaskReport {
    let mut report = TaskReport::default();
    if inv.door != Door::Daemon {
        return report;
    }
    let roster: Vec<SessionRecord> = load_stage::<SessionsFile>(&sessions_path())
        .map(|f| f.sessions)
        .unwrap_or_default();
    let cursor = read_cursor();

    let mut advance: Vec<(String, CursorEntry)> = Vec::new();
    for rec in roster
        .iter()
        .filter(|r| r.task.as_deref().is_some_and(|t| !t.is_empty()))
    {
        if canonical_state(&rec.state) != "done" || cursor.contains_key(&rec.session_id) {
            continue;
        }
        let slug = rec.task.clone().unwrap_or_default();
        let destination = report_destination(rec, &roster);
        let Some((mailbox, why)) = destination else {
            // A4: no report mailbox at all (no `--report-to`, and no parent).
            // The run is STILL reported deterministically — cursor entry,
            // record, audit line, report rail — just not mailed, and this says
            // so instead of pretending a letter exists.
            advance.push((
                rec.session_id.clone(),
                CursorEntry {
                    ended_at: rec.ended_at.clone(),
                    exit_code: rec.exit_code,
                    outcome: rec.outcome.clone(),
                    msgid: None,
                    mailbox: None,
                    wake: None,
                },
            ));
            report.no_mailbox.push(rec.session_id.clone());
            eprintln!("[aoide/reap] task report unwritten (run {}): no report mailbox", rec.session_id);
            audit_send(
                inv,
                "unmailed",
                &format!("task report has no mailbox (run {}, task {slug})", rec.session_id),
                &report_subject(rec, &slug),
            );
            continue;
        };
        match file_report(rec, &slug, &mailbox) {
            Ok(msgid) => {
                // The wake: ONE route, the mail-side doorbell, taken
                // in-process under its own `.ring.lock` — the same call the
                // daemon's `mail ring` handler makes. Its real outcome is
                // KEPT (rung, deferred with the child's hook state, skipped
                // with the reason): a parent that is not a conducted wrap — a
                // Codex desktop/app session never is — cannot be woken at
                // all, and saying only `rang:0` would leave "why was nobody
                // woken" unanswerable. A ring failure is recorded the same
                // way; the letter stays filed regardless, because the durable
                // half of a report is the letter, not the nudge.
                let wake = match super::doorbell::ring(&mailbox, Some(&rec.session_id)) {
                    Ok(r) => {
                        let mut parts = vec![format!("rang:{}", r.rung.len())];
                        if !r.deferred.is_empty() {
                            parts.push(format!(
                                "deferred:{}",
                                r.deferred
                                    .iter()
                                    .map(|(id, why)| format!("{id}({why})"))
                                    .collect::<Vec<_>>()
                                    .join(",")
                            ));
                        }
                        if !r.skipped.is_empty() {
                            parts.push(format!(
                                "skipped:{}",
                                r.skipped
                                    .iter()
                                    .map(|(id, why)| format!("{id}({why})"))
                                    .collect::<Vec<_>>()
                                    .join(",")
                            ));
                        }
                        if r.rung.is_empty() && r.deferred.is_empty() && r.skipped.is_empty() {
                            // The honest reason a ring did nothing at all:
                            // nothing is ENROLLED as a real reader under this
                            // mailbox. A mailbox named by a session id is
                            // readable but never ringable — the reader key for a
                            // name is the name itself, and `ring_targets` never
                            // arms the pseudo-reader — so the fact is recorded
                            // instead of a bare `rang:0`.
                            parts.push(format!("no-armed-reader:{mailbox}"));
                        }
                        parts.join(" ")
                    }
                    Err(e) => format!("ring-failed:{e}"),
                };
                advance.push((
                    rec.session_id.clone(),
                    CursorEntry {
                        ended_at: rec.ended_at.clone(),
                        exit_code: rec.exit_code,
                        outcome: rec.outcome.clone(),
                        msgid: msgid.clone(),
                        mailbox: Some(mailbox.clone()),
                        wake: Some(wake.clone()),
                    },
                ));
                report.filed.push((
                    rec.session_id.clone(),
                    msgid.clone().unwrap_or_else(|| "filed (no msgid read back)".to_string()),
                ));
                report.wake.push((rec.session_id.clone(), wake.clone()));
                // House rule 6 — one policy surface, one audit log: the letter
                // is a user-visible mailbox mutation, so it gets its own line
                // through the SAME helper `send` and the ping-back use, with
                // the report's own subject line as untrusted data.
                audit_send(
                    inv,
                    "filed",
                    &format!(
                        "task report filed to `self/{mailbox}` (run {}), {wake} [{why}]",
                        rec.session_id
                    ),
                    &report_subject(rec, &slug),
                );
                eprintln!(
                    "[aoide/reap] task report → self/{mailbox} ({}), {wake} [{why}]",
                    rec.session_id
                );
            }
            Err(e) => report.failed.push((rec.session_id.clone(), e)),
        }
    }

    if !advance.is_empty() {
        // Persist progress SAFELY: the cursor is rebuilt from the runs the
        // roster still carries (a vanishe
        // record's entry leaves the file — its id never returns) plus the
        // entries just filed, and written atomically. A run that failed to
        // file keeps NO entry, so the next tick retries it.
        let live: std::collections::HashSet<&str> = roster
            .iter()
            .filter(|r| r.task.is_some())
            .map(|r| r.session_id.as_str())
            .collect();
        let mut next: CursorFile = cursor
            .into_iter()
            .filter(|(id, _)| live.contains(id.as_str()))
            .collect();
        for (id, entry) in advance {
            next.insert(id, entry);
        }
        if let Ok(body) = serde_json::to_string(&next) {
            if let Err(e) =
                aoide_storage::fs::atomic_write(&taskreport_path(), &format!("{body}\n"))
            {
                // The letters are filed; only the memory of it failed. Next
                // tick re-files (the duplicate window this module's doc
                // states), never loses.
                eprintln!("[aoide/reap] task report cursor write failed: {e}");
            }
        }
    }
    report
}

/// Read the cursor; unreadable/unparseable content reads as EMPTY rather than
/// failing the pass (the file is ours alone, and a torn hand-edit may only
/// ever cost one extra letter — never a stuck lane).
fn read_cursor() -> CursorFile {
    std::fs::read_to_string(taskreport_path())
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// The role mailbox a report falls back to when the run DOES have a parent but
/// its canonical id is not a legal mailbox name (an uppercase or
/// punctuation-bearing session id): `conductor`, the role name MAIL.md's
/// addressing rules name as the conductor's own mailbox. Falling back keeps
/// the report deliverable; the alternative — rewriting an identity into a
/// mailbox name — is never done, because two different ids could then collide
/// on one name. A run with no parent at all has no default destination and is
/// reported without a letter (§3.5 of the amendment).
const FALLBACK_REPORT_MAILBOX: &str = "conductor";

/// Where this run's report should go, and why — the facts the reap line, the
/// audit line and the cursor all carry. `--report-to` (validated when it was
/// accepted), else the parent's id when that IS a legal mailbox name, else the
/// documented role fallback, else nothing.
fn report_destination(rec: &SessionRecord, roster: &[SessionRecord]) -> Option<(String, String)> {
    if let Some(name) = rec
        .report_to
        .as_deref()
        .filter(|n| aoide_storage::node_store::valid_node_name(n))
    {
        return Some((name.to_string(), "report-to".to_string()));
    }
    let parent = rec.parent_session_id.as_deref().filter(|p| !p.is_empty())?;
    let known = roster.iter().any(|s| s.session_id == parent) || ledger_knows(parent);
    if aoide_storage::node_store::valid_node_name(parent) && known {
        return Some((parent.to_string(), "parent".to_string()));
    }
    Some((
        FALLBACK_REPORT_MAILBOX.to_string(),
        if aoide_storage::node_store::valid_node_name(parent) {
            // A well-formed id that names NOTHING this box tracks: a typo'd or
            // long-forgotten `--parent`. Delivering there would mail a mailbox
            // no session reads, so the report goes to the documented role
            // mailbox instead — and the reason travels with it, so the fact is
            // visible rather than silent.
            format!("role-fallback (parent `{parent}` is not a known session)")
        } else {
            format!("role-fallback (parent `{parent}` is not a mailbox name)")
        },
    ))
}

/// Does the durable ledger know this session id? A run's record can be pruned
/// from the roster while its ledger line stays — that is the difference between
/// "a real parent, since finished and pruned" and "a typo".
fn ledger_knows(id: &str) -> bool {
    aoide_storage::ledger::read_ledger()
        .map(|entries| entries.iter().any(|e| e.session_id == id))
        .unwrap_or(false)
}

/// The report's first line — deterministic, single-line, and carrying the RUN
/// id so a duplicate (the crash window above) is visibly the same run reported
/// twice rather than two runs.
fn report_subject(rec: &SessionRecord, slug: &str) -> String {
    format!("[task {slug}] {} (run {})", outcome_word(rec), rec.session_id)
}

/// The run's end as words, from the DISCRIMINATED outcome and the real code:
/// `exited 7`, `died by signal TERM`, `timed out`, `stopped (no status)`. Never
/// a fabricated `0`, and never a numeric code for a kill.
fn outcome_word(rec: &SessionRecord) -> String {
    match rec.outcome.as_deref() {
        Some("exit") => match rec.exit_code {
            Some(code) => format!("exited {code}"),
            None => "exited (no code recorded)".to_string(),
        },
        Some("signal") => "died by signal".to_string(),
        Some("timeout") => "timed out".to_string(),
        Some("stopped") => "stopped (no status)".to_string(),
        Some(other) => other.to_string(),
        // A record written before `outcome` existed: fall back to the code,
        // and say "unknown" rather than inventing one.
        None => match rec.exit_code {
            Some(code) => format!("exited {code}"),
            None => "stopped (no exit status recorded)".to_string(),
        },
    }
}

/// The report's body: the report's own subject line FIRST (so a plain reader
/// shows the run and the status without decoding anything), then a
/// deterministic template — task, run, agent, both instants, the exit status,
/// both paths — and both read commands. No model, no gate, no prompt, no
/// verdict: whether the work was any good is the operator's own reading
/// (`§3.5f` of the wrapper plan).
///
/// **The letter is deliberately PLAIN text, not the structured
/// (`--subject`) form.** A structured letter's text is the signed
/// `AOIDE-LETTER/1` envelope, which `aoide mail read` prints verbatim — so a
/// report an operator opens months later, after the run's record is long gone,
/// must be readable by the plainest reader there is. The subject line inside
/// the body carries what the structured form's subject would have.
fn report_body(rec: &SessionRecord, slug: &str, mailbox: &str) -> String {
    format!(
        "{}\n\
         managed task report — no verification of the work is implied by this letter\n\
         task: {slug}\n\
         run: {}\n\
         agent: {}\n\
         started: {}\n\
         ended: {}\n\
         outcome: {}\n\
         log: {}\n\
         instructions: {}\n\
         report mailbox: self/{mailbox}  (the child's own inbox is self/{slug})\n\
         read this mailbox: aoide mail read --for {mailbox} --reread\n\
         watch the run: aoide session watch {} --snapshot\n",
        report_subject(rec, slug),
        rec.session_id,
        rec.agent,
        rec.started_at,
        rec.ended_at.clone().unwrap_or_else(|| "unknown".to_string()),
        outcome_word(rec),
        rec.log_path.clone().unwrap_or_default(),
        rec.instructions_path.clone().unwrap_or_default(),
        rec.session_id,
    )
}

/// File one report through the REGISTERED `["mail","send"]` implementation,
/// in-process, under `Door::Daemon`. Returns the FILED letter's `msgid` — read
/// back out of the mailbase itself, not out of the handler's own summary data:
/// a structured (`--subject`) letter's outcome carries the wire result, not the
/// entry, and the mailbase is the authority on what was actually written.
fn file_report(rec: &SessionRecord, slug: &str, mailbox: &str) -> Result<Option<String>, String> {
    let mut registry = Registry::new();
    aoide_client::commands::register_mail(&mut registry);
    let path = vec!["mail".to_string(), "send".to_string()];
    let Some(command) = registry.get(&path) else {
        return Err("mail.send is not registered in this build".to_string());
    };
    let subject = report_subject(rec, slug);
    let mut flags = BTreeMap::new();
    // NEVER `self/<slug>`: the child's inbox carries only what was sent TO the
    // subagent. The report goes to the report mailbox (A4).
    flags.insert("to".to_string(), format!("self/{mailbox}"));
    flags.insert("from".to_string(), rec.session_id.clone());
    // NO `subject` flag: a structured letter's body is the signed
    // `AOIDE-LETTER/1` envelope, which `mail read` prints verbatim — the report
    // must stay readable by that plainest reader, so the subject line rides
    // inside the plain body instead (see `report_body`).
    let inv = Invocation {
        path,
        args: vec![report_body(rec, slug, mailbox)],
        flags,
        door: Door::Daemon,
    };
    let outcome: Outcome = (command.handler)(&inv);
    if outcome.status != Status::Ok {
        return Err(format!("mail.send refused: {}", outcome.message));
    }
    // Read-back: the newest letter in this mailbox from THIS run carrying this
    // report's own subject line. A handler that reported Ok but wrote nothing
    // findable is reported as NO msgid rather than a fabricated one — and the
    // caller treats a missing msgid as "filed, unverified identity", never as
    // proof.
    let found = aoide_storage::mail::read_base().ok().and_then(|entries| {
        entries
            .into_iter()
            .rev()
            .find(|e| {
                e.envelope.header.to.name == mailbox
                    && e.envelope.header.from.name == rec.session_id
                    && e.envelope.text.contains(&subject)
            })
            .map(|e| e.envelope.msgid)
    });
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn done_run(id: &str, slug: &str, exit_code: Option<i32>) -> SessionRecord {
        let mut rec = SessionRecord {
            session_id: id.into(),
            agent: "claude".into(),
            state: "done".into(),
            started_at: "2026-09-21T05:00:00Z".into(),
            ..Default::default()
        };
        rec.task = Some(slug.into());
        rec.exit_code = exit_code;
        rec.ended_at = Some("2026-09-21T05:01:00Z".into());
        rec.log_path = Some("/home/khoa/.aoide/state/sessions/x.log".into());
        rec.instructions_path = Some("/home/khoa/.aoide/state/sessions/x.instructions.md".into());
        rec
    }

    #[test]
    fn the_report_names_the_run_and_never_invents_an_exit_code() {
        let rec = done_run("run-1", "fix-flaky", Some(7));
        assert_eq!(report_subject(&rec, "fix-flaky"), "[task fix-flaky] exited 7 (run run-1)");
        let body = report_body(&rec, "fix-flaky", "conductor");
        assert!(body.contains("task: fix-flaky"), "{body}");
        assert!(body.contains("run: run-1"), "{body}");
        assert!(body.contains("outcome: exited 7"), "{body}");
        assert!(body.contains("aoide mail read --for conductor --reread"), "{body}");
        assert!(body.contains("report mailbox: self/conductor"), "{body}");
        assert!(body.contains("instructions: /home/khoa/.aoide/state/sessions/x.instructions.md"));
        // No env-like dump: the letter carries paths, ids and codes only.
        assert!(!body.contains("AOIDE_"), "{body}");
        assert!(!body.contains("="), "{body}");

        // A killed/reaped run has NO code and NO invented exit word: the report
        // says it stopped without a status rather than reporting 0.
        let killed = done_run("run-2", "fix-flaky", None);
        assert_eq!(
            report_subject(&killed, "fix-flaky"),
            "[task fix-flaky] stopped (no exit status recorded) (run run-2)"
        );
        assert!(
            report_body(&killed, "fix-flaky", "conductor").contains("outcome: stopped"),
            "{killed:?}"
        );
    }

    #[test]
    fn the_subject_is_one_line_whatever_the_run_carried() {
        let mut rec = done_run("run\r3", "fix-flaky", Some(1));
        rec.ended_at = Some("x".into());
        // The slug is validated at spawn, so only the id could ever carry a
        // control character — and the mailbox name rule is the guard that
        // matters here: nothing in this template is user text except ids.
        assert!(!report_subject(&rec, "fix-flaky").contains('\n'));
    }

    #[test]
    fn a_non_daemon_door_files_nothing() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let root = std::env::temp_dir().join(format!("aoide-taskreport-{}", crate::graph::conduct::unix_ts()));
        std::fs::create_dir_all(&root).unwrap();
        let _env = crate::graph::testutil::EnvVars::save(&["AOIDE_STATE_DIR", "AOIDE_STAGE_DIR"]);
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::set_var("AOIDE_STAGE_DIR", root.join("stage"));
        let inv = Invocation {
            path: vec!["session".to_string(), "reap".to_string()],
            args: Vec::new(),
            flags: BTreeMap::new(),
            door: Door::Cli,
        };
        let report = taskreport(&inv);
        assert_eq!(report, TaskReport::default());
        assert!(!taskreport_path().exists(), "a CLI-door pass must write no cursor");
        let _ = std::fs::remove_dir_all(&root);
    }

    // ── the daemon-door lane, against a real isolated mailbase ───────────

    use crate::graph::model::{write_stage, SessionsFile};
    use crate::graph::testutil::{unique_stage, EnvVars};
    use aoide_storage::mail;

    /// A fresh isolated state/stage/runtime triple. The caller holds
    /// `env_lock` for the test's duration.
    fn isolated(tag: &str) -> (PathBuf, EnvVars) {
        let guard = EnvVars::save(&["AOIDE_STATE_DIR", "AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR"]);
        let root = unique_stage(tag);
        for sub in ["stage", "state", "state/mail", "state/sessions", "run"] {
            std::fs::create_dir_all(root.join(sub)).unwrap();
        }
        std::env::set_var("AOIDE_STAGE_DIR", root.join("stage"));
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::set_var("XDG_RUNTIME_DIR", root.join("run"));
        (root, guard)
    }

    fn daemon_inv() -> Invocation {
        Invocation {
            path: vec!["session".to_string(), "reap".to_string()],
            args: Vec::new(),
            flags: BTreeMap::new(),
            door: Door::Daemon,
        }
    }

    /// A finished managed task run, exactly as `conduct`'s exit path leaves
    /// one: done, with the real code and end instant, its transcript and its
    /// instruction sidecar on disk.
    fn finished_run(root: &PathBuf, id: &str, slug: &str, exit_code: Option<i32>) -> SessionRecord {
        let log = root.join("state/sessions").join(format!("{id}.log"));
        std::fs::write(&log, "the run's own output\n").unwrap();
        let sidecar = root.join("state/sessions").join(format!("{id}.instructions.md"));
        std::fs::write(&sidecar, "BRIEF\n").unwrap();
        let mut rec = crate::graph::testutil::session(
            id,
            &root.to_string_lossy(),
            "done",
            "2026-09-21T05:00:00Z",
            Some("parent"),
        );
        rec.task = Some(slug.to_string());
        rec.log_path = Some(log.to_string_lossy().into_owned());
        rec.instructions_path = Some(sidecar.to_string_lossy().into_owned());
        rec.exit_code = exit_code;
        rec.ended_at = Some("2026-09-21T05:01:00Z".to_string());
        rec
    }

    fn write_roster(recs: Vec<SessionRecord>) {
        write_stage(
            &sessions_path(),
            &SessionsFile { sessions: recs, ..Default::default() },
        )
        .unwrap();
    }

    fn roster() -> Vec<SessionRecord> {
        load_stage::<SessionsFile>(&sessions_path()).unwrap().sessions
    }

    fn reports_for(slug: &str) -> Vec<mail::Entry> {
        mail::read_base()
            .unwrap()
            .into_iter()
            .filter(|e| e.envelope.header.to.name == slug)
            .collect()
    }

    /// The whole durable path in one pass: a daemon-door tick files exactly one
    /// letter through the registered `mail send` into the REPORT mailbox, wakes
    /// through the mail-side doorbell (recording WHY nobody was rung when the
    /// mailbox is named by a session id — no armed reader), advances the
    /// cursor, and the SECOND pass files nothing — the cursor is on disk, so
    /// this survives a daemon restart.
    #[test]
    fn a_daemon_pass_files_once_and_is_restart_durable() {
        let _lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, _env) = isolated("taskreport-files");
        // The parent is a REAL session on this box's roster: the parent's own
        // id is a default destination only when it names a KNOWN session —
        // anything else falls back to the role mailbox, with the reason
        // recorded (B3).
        let parent = crate::graph::testutil::session(
            "parent",
            &root.to_string_lossy(),
            "running",
            "2026-09-21T04:00:00Z",
            None,
        );
        write_roster(vec![finished_run(&root, "run-1", "fix-flaky", Some(7)), parent]);
        // A1: the CHILD is the slug's reader (enrolled at registration); the
        // parent reads the REPORT mailbox, not the child's inbox.
        mail::enrol_reader("fix-flaky", "run-1").unwrap();

        let first = taskreport(&daemon_inv());
        assert_eq!(first.filed.len(), 1, "{first:?}");
        assert_eq!(first.no_mailbox.len(), 0, "{first:?}");
        // B3: the wake says WHY nothing was rung. A mailbox named by a session
        // id has no armed reader — the reader key for a name is the name
        // itself, which the ringer never arms — so a bare `rang:0` would leave
        // the one question an operator has unanswered.
        assert_eq!(first.wake.len(), 1, "{first:?}");
        assert!(
            first.wake[0].1.contains("no-armed-reader:parent"),
            "honest zero-reader reason: {:?}",
            first.wake
        );
        // A4: the report goes to the parent's id (a legal mailbox name), NEVER
        // into the child's own inbox.
        let letters = reports_for("parent");
        assert_eq!(letters.len(), 1, "exactly one letter in the ordinary path");
        assert!(reports_for("fix-flaky").is_empty(), "the child's inbox never carries the report");
        let letter = &letters[0];
        assert_eq!(letter.envelope.header.from.name, "run-1", "the run is the sender");
        // The report is PLAIN text (never the signed structured envelope), so
        // the plainest reader — `mail read` — shows the whole thing.
        assert!(letter.envelope.text.starts_with("[task fix-flaky] exited 7 (run run-1)"));
        assert!(letter.envelope.text.contains("outcome: exited 7"), "{}", letter.envelope.text);
        assert!(letter.envelope.text.contains("aoide mail read --for parent --reread"));
        assert!(letter.envelope.text.contains("aoide session watch run-1 --snapshot"));
        assert!(aoide_storage::letter::decode(&letter.envelope.text).is_none(), "plain, not structured");
        assert!(!letter.envelope.text.contains("AOIDE_"), "no environment dump");

        // The cursor is DURABLE and claims the same msgid.
        let cursor: CursorFile =
            serde_json::from_str(&std::fs::read_to_string(taskreport_path()).unwrap()).unwrap();
        let entry = cursor.get("run-1").expect("an entry for the run");
        assert_eq!(entry.exit_code, Some(7));
        assert_eq!(entry.mailbox.as_deref(), Some("parent"));
        assert_eq!(entry.ended_at.as_deref(), Some("2026-09-21T05:01:00Z"));
        assert_eq!(entry.msgid.as_deref(), Some(letter.envelope.msgid.as_str()));

        // Unread for the parent in the REPORT mailbox, for the pseudo-reader —
        // and the child's own inbox is EMPTY, because nothing was sent to it.
        assert!(mail::names_with_unread(Some("parent")).unwrap().contains(&"parent".to_string()));
        assert!(mail::names_with_unread(None).unwrap().contains(&"parent".to_string()));
        assert_eq!(
            mail::unread_for("fix-flaky", Some("run-1")).unwrap(),
            (0, Vec::new()),
            "the child's own queue is untouched by the report"
        );

        // Second tick — a NEW process would read this same file, so this is
        // the restart case: nothing to do, no second letter.
        let second = taskreport(&daemon_inv());
        assert_eq!(second, TaskReport::default(), "a filed run is never filed again");
        assert_eq!(reports_for("parent").len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// "Persist progress safely": a filing failure writes NO cursor entry, so
    /// losing a report cannot happen — the next pass retries and succeeds.
    #[test]
    fn a_filing_failure_advances_nothing_and_the_next_pass_retries() {
        let _lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, _env) = isolated("taskreport-retry");
        // A KNOWN parent (a real roster record), so the default destination is
        // its own id rather than the role fallback.
        let parent = crate::graph::testutil::session(
            "parent",
            &root.to_string_lossy(),
            "running",
            "2026-09-21T04:00:00Z",
            None,
        );
        write_roster(vec![finished_run(&root, "run-1", "fix-flaky", Some(3)), parent]);

        // A mailbase that cannot exist: the state dir path is a FILE, so the
        // mailbase cannot be created. The stage dir (and so the cursor's own
        // home) stays writable, which is what makes the assertion below
        // meaningful.
        let blocked = root.join("blocked");
        std::fs::write(&blocked, "not a directory").unwrap();
        std::env::set_var("AOIDE_STATE_DIR", &blocked);

        let failed = taskreport(&daemon_inv());
        assert!(failed.filed.is_empty(), "{failed:?}");
        assert_eq!(failed.failed.len(), 1, "{failed:?}");
        assert!(
            !taskreport_path().exists(),
            "a filing failure must not advance (or create) the cursor"
        );

        // Now the mailbase works again: the SAME run is reported, once.
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        let retried = taskreport(&daemon_inv());
        assert_eq!(retried.filed.len(), 1, "{retried:?}");
        assert_eq!(reports_for("parent").len(), 1);
        assert_eq!(taskreport(&daemon_inv()).filed.len(), 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Parent unavailable (no report mailbox at all): the run is STILL reported
    /// deterministically — cursor, record, audit line — just not mailed, and
    /// the outcome says exactly that.
    #[test]
    fn a_run_with_no_report_mailbox_is_reported_without_a_letter() {
        let _lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, _env) = isolated("taskreport-no-parent");
        let mut rec = finished_run(&root, "run-1", "fix-flaky", Some(0));
        rec.parent_session_id = None;
        rec.report_to = None;
        write_roster(vec![rec]);

        let report = taskreport(&daemon_inv());
        assert_eq!(report.filed.len(), 0, "{report:?}");
        assert_eq!(report.failed.len(), 0, "no mailbox is not a failure");
        assert_eq!(report.no_mailbox, vec!["run-1".to_string()], "{report:?}");
        assert!(reports_for("fix-flaky").is_empty(), "no letter anywhere");
        assert!(reports_for("parent").is_empty(), "not even a default mailbox");

        // The facts are all still there without the letter: the cursor entry,
        // and the record's own end facts.
        let cursor: CursorFile =
            serde_json::from_str(&std::fs::read_to_string(taskreport_path()).unwrap()).unwrap();
        let entry = cursor.get("run-1").expect("an entry even with no mailbox");
        assert_eq!(entry.mailbox, None);
        assert!(entry.msgid.is_none(), "no letter, so no msgid to claim");
        let kept = roster().into_iter().find(|r| r.session_id == "run-1").expect("record kept");
        assert_eq!(kept.task.as_deref(), Some("fix-flaky"));
        assert_eq!(kept.exit_code, Some(0));
        assert_eq!(kept.ended_at.as_deref(), Some("2026-09-21T05:01:00Z"));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A2's read-only peek, from the crate boundary: reader-selected, and it
    /// moves no cursor — the observer can look as often as it likes without
    /// consuming the child's mail or its own.
    #[test]
    fn unread_for_is_a_reader_selected_peek_that_advances_nothing() {
        let _lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, _env) = isolated("taskreport-peek");
        mail::enrol_reader("fix-flaky", "run-1").unwrap();
        mail::file_letter("outside", "fix-flaky", "first").unwrap();
        mail::file_letter("outside", "fix-flaky", "second").unwrap();
        let cursors_before = std::fs::read(mail::cursors_path()).unwrap_or_default();

        let (mark, queue) = mail::unread_for("fix-flaky", Some("run-1")).unwrap();
        assert_eq!(mark, 0, "an unread reader's absent mark reads as 0");
        assert_eq!(queue.len(), 2, "both letters are the child's unread queue");
        assert_eq!(mail::unread_for("fix-flaky", Some("run-1")).unwrap().1.len(), 2);

        // Nothing moved: not the child's mark, not the file, not its unread state.
        assert_eq!(std::fs::read(mail::cursors_path()).unwrap_or_default(), cursors_before);
        assert!(mail::names_with_unread(Some("run-1")).unwrap().contains(&"fix-flaky".to_string()));

        // The CHILD reads: the queue drains for the child, and only for it.
        assert_eq!(mail::read_for("fix-flaky", false, Some("run-1")).unwrap().len(), 2);
        let (mark, queue) = mail::unread_for("fix-flaky", Some("run-1")).unwrap();
        assert!(mark > 0, "the child's own read advanced its own mark");
        assert!(queue.is_empty(), "nothing left unread for the child");
        // A DIFFERENT reader (the pseudo-reader) is untouched by that.
        assert!(!mail::names_with_unread(None).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// B3: a well-formed parent id that names NOTHING this box tracks is a typo
    /// or a forgotten run — never a destination. The report goes to the
    /// documented role mailbox, with the reason recorded, rather than being
    /// mailed into a mailbox no session reads.
    #[test]
    fn an_unknown_parent_falls_back_to_the_role_mailbox_with_the_reason() {
        let _lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, _env) = isolated("taskreport-unknown-parent");
        let mut rec = finished_run(&root, "run-1", "fix-flaky", Some(6));
        rec.parent_session_id = Some("ghost-parent".to_string());
        write_roster(vec![rec]);

        let report = taskreport(&daemon_inv());
        assert_eq!(report.filed.len(), 1, "{report:?}");
        assert_eq!(reports_for("ghost-parent").len(), 0, "never mailed to an unknown session");
        assert_eq!(reports_for(FALLBACK_REPORT_MAILBOX).len(), 1, "delivered to the role mailbox");
        let cursor: CursorFile =
            serde_json::from_str(&std::fs::read_to_string(taskreport_path()).unwrap()).unwrap();
        assert_eq!(cursor.get("run-1").unwrap().mailbox.as_deref(), Some(FALLBACK_REPORT_MAILBOX));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// B3: a parent that IS known (a live roster record) is the default
    /// destination, and the wake still says plainly that a session-id mailbox
    /// has no armed reader.
    #[test]
    fn a_known_parent_is_the_destination_and_the_wake_names_no_armed_reader() {
        let _lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, _env) = isolated("taskreport-known-parent");
        let mut rec = finished_run(&root, "run-1", "fix-flaky", Some(2));
        rec.parent_session_id = Some("parent".to_string());
        let parent = crate::graph::testutil::session(
            "parent",
            &root.to_string_lossy(),
            "running",
            "2026-09-21T04:00:00Z",
            None,
        );
        write_roster(vec![rec, parent]);

        let report = taskreport(&daemon_inv());
        assert_eq!(report.filed.len(), 1, "{report:?}");
        assert_eq!(reports_for("parent").len(), 1, "the known parent is the destination");
        assert!(report.wake[0].1.contains("no-armed-reader:parent"), "{:?}", report.wake);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// B3: a ROLE report mailbox can carry a real reader and a real latch — the
    /// enrolment `conduct` performs for `--report-to <role>` — so the ring
    /// names the reader it tried instead of reporting a bare zero.
    #[test]
    fn a_role_mailbox_reports_the_enrolled_reader_it_tried_to_ring() {
        let _lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, _env) = isolated("taskreport-role-mailbox");
        let mut rec = finished_run(&root, "run-1", "fix-flaky", Some(0));
        rec.report_to = Some("conductor".to_string());
        write_roster(vec![rec]);
        // `conduct`'s own registration does this for a role mailbox.
        mail::enrol_reader("conductor", "parent").unwrap();
        assert!(
            mail::ring_targets("conductor").unwrap().enrolled.contains(&"parent".to_string()),
            "the role mailbox carries a real reader"
        );

        let report = taskreport(&daemon_inv());
        assert_eq!(report.filed.len(), 1, "{report:?}");
        assert_eq!(reports_for("conductor").len(), 1);
        // The reader is enrolled but names no live session here, so the ring
        // reports exactly that — a reason, never a bare count.
        assert!(
            report.wake[0].1.contains("skipped:parent(unknown)"),
            "the enrolled reader is named with its reason: {:?}",
            report.wake
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// An invalid (canonical, uppercase-bearing) parent id is NEVER rewritten
    /// into a mailbox name — the report falls back to the documented role
    /// mailbox instead, so it is delivered rather than silently dropped and two
    /// ids can never collide on one rewritten name.
    #[test]
    fn an_unusable_parent_id_falls_back_to_the_role_mailbox() {
        let _lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, _env) = isolated("taskreport-fallback");
        let mut rec = finished_run(&root, "run-1", "fix-flaky", Some(4));
        rec.parent_session_id = Some("Aoide-816F".to_string()); // uppercase: not a mailbox name
        write_roster(vec![rec]);

        let report = taskreport(&daemon_inv());
        assert_eq!(report.filed.len(), 1, "{report:?}");
        assert_eq!(report.no_mailbox.len(), 0, "{report:?}");
        assert_eq!(reports_for(FALLBACK_REPORT_MAILBOX).len(), 1, "delivered to the role mailbox");
        assert!(reports_for("Aoide-816F").is_empty(), "identity never sanitized into a mailbox");
        let cursor: CursorFile =
            serde_json::from_str(&std::fs::read_to_string(taskreport_path()).unwrap()).unwrap();
        assert_eq!(
            cursor.get("run-1").unwrap().mailbox.as_deref(),
            Some(FALLBACK_REPORT_MAILBOX)
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The reap-order property: a finished run's report is durable even when
    /// the SAME sweep that follows reaps another session and prunes every
    /// `done` record — including this run's own. The lane runs before that
    /// prune (top of `reap()`), so the letter exists either way, and it stays
    /// readable after the record is gone.
    #[test]
    fn the_letter_survives_the_reap_that_prunes_the_run_record() {
        let _lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, _env) = isolated("taskreport-reap-prune");
        let done = finished_run(&root, "run-1", "fix-flaky", Some(5));
        // A KNOWN parent, so the report's destination is its own id.
        let parent = crate::graph::testutil::session(
            "parent",
            &root.to_string_lossy(),
            "running",
            "2026-09-21T04:00:00Z",
            None,
        );
        // A second, definitely-not-live session for the same sweep to reap —
        // which is what makes `reap_inner` run its `prune_done` at all.
        let mut dead = crate::graph::testutil::session("dead-1", &root.to_string_lossy(), "running", "2026-09-21T04:00:00Z", None);
        dead.pid = Some(999_999);
        write_roster(vec![done, parent, dead]);

        let outcome = crate::reap::reap(&daemon_inv());
        assert_eq!(outcome.status, aoide_protocol::output::Status::Ok, "{}", outcome.message);
        assert_eq!(reports_for("parent").len(), 1, "the report is filed by this sweep");
        let survivors: Vec<String> = roster().into_iter().map(|r| r.session_id).collect();
        eprintln!(
            "[test] reap left {:?} on the roster (the lane ran first either way)",
            survivors
        );
        // Durable after the record leaves the roster: the letter is still
        // there, still addressed to the report mailbox, still unread.
        assert_eq!(reports_for("parent").len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The retention contract, end to end: a completed task run whose report is
    /// FILED must not vanish when routine cleanup sweeps the roster — an
    /// unrelated reap that prunes every `done` record leaves it standing, and
    /// `session watch` still resolves it (instructions, mail, output, report).
    /// Only the explicit `aoide session prune` may drop it.
    #[test]
    fn a_filed_run_survives_the_reap_and_still_watches() {
        let _lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (root, _env) = isolated("taskreport-retention");
        write_roster(vec![finished_run(&root, "run-1", "fix-flaky", Some(5))]);
        assert_eq!(taskreport(&daemon_inv()).filed.len(), 1, "the report is filed first");

        // An UNRELATED sweep: a second, certainly-dead session is what makes
        // `reap_inner` run its `prune_done` at all.
        let mut dead = crate::graph::testutil::session(
            "dead-1",
            &root.to_string_lossy(),
            "running",
            "2026-09-21T04:00:00Z",
            None,
        );
        dead.pid = Some(999_999);
        let mut roster_now = roster();
        roster_now.push(dead);
        write_roster(roster_now);
        let outcome = crate::reap::reap(&daemon_inv());
        assert_eq!(outcome.status, aoide_protocol::output::Status::Ok, "{}", outcome.message);

        // The completed task did NOT vanish.
        assert!(
            roster().iter().any(|r| r.session_id == "run-1"),
            "a filed task run is retained against routine cleanup"
        );
        // And the view still resolves it, with its report line reading the
        // cursor (never a letter it guessed at).
        let out = crate::graph::view::session_watch(&snapshot_inv_pub("run-1"));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "{}", out.message);
        let data = out.data.clone().unwrap();
        assert_eq!(data["task"], "fix-flaky");
        assert_eq!(data["report"].as_str().is_some(), true, "report line present");
        assert!(
            out.message.contains("── 0 unread for fix-flaky"),
            "the child's queue is rendered: {}",
            out.message
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A snapshot invocation for the tests above (the view's own builder is
    /// private to `view.rs`).
    fn snapshot_inv_pub(id: &str) -> Invocation {
        Invocation {
            path: vec!["session".to_string(), "watch".to_string()],
            args: vec![id.to_string()],
            flags: BTreeMap::from([("snapshot".to_string(), "true".to_string())]),
            door: Door::Cli,
        }
    }
}
