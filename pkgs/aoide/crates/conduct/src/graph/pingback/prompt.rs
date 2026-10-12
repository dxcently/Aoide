//! The ping-back's third source: **a conducted wrap that is asking.** The
//! first source reads an eidolon child's trace, the second a hook-reporting
//! child's phase; this one speaks for what has neither — a `codex`, an `aider`,
//! a REPL, an installer in a plain conducted shell — whose wrap went `awaiting`
//! off its own screen (`graph/prompt.rs`) and published the line it is waiting
//! on as the record's `question`.
//!
//! It is the same ping-back, not a third one. The line is raw-injected by the
//! one [`deliver`](super::deliver) under the resident daemon alone, with the
//! same skip rules, and its at-most-once claim is written into the SAME
//! `pingback.json` inside the SAME stage-lock section — in the entry's `screen`
//! field, so the sources never read each other's cursor:
//!
//! ```text
//! [codex fond-aspen] asking: "Apply patch? [y/N]"
//! ```
//!
//! **The child is the wrap itself** — there is no hook session under it — so
//! there is no host to climb from: the wrap must be a conducted record with a
//! daemon seal over its pid, and its SPAWNER hears it over the edge its own
//! registration attested, resolved by the very climb the hook source uses
//! ([`hook::spawner_of`](super::hook::spawner_of)). A wrap with no attested
//! spawner (a terminal the User opened) has nobody to tell.
//!
//! **An episode is a question the record holds, as one pass sees it.** The
//! cursor remembers the question it last announced. A pass that finds a
//! different question announces it; a pass that finds the record asking nothing
//! clears the memory, so a later ask is heard again. The limit: a question that
//! is answered and asked again with the same text between two passes is one
//! continuous question to the cursor, and the return is not announced. No
//! episode counter exists; the cursor holds one question per child. Unlike the
//! hook source there is no baseline: the wrap publishes after seconds of quiet,
//! so the first examination of a child may already find the question, and that
//! is news. A question asked and answered between two passes is not heard.
//!
//! **The `sudo` prompt is the human's.** A question that carries `needsSudo` is
//! shown on the record (its lock badge) and never announced: the password is not
//! an agent's to answer, so nothing is pinged for it.
//!
//! **The line is foreign text (house rule 4).** `question` was sanitised and
//! capped where it was written, and is cleaned and quoted again here: one line,
//! every unsafe character stripped, `"`-quoted, never starting with `/` or `!`
//! — the same rendering an eidolon's `asking: "…"` gets.

use super::{harness_tag, hook, quote, tracks, ChildClaim, CursorEntry, CursorFile};
use crate::graph::model::{canonical_state, SessionRecord};
use aoide_protocol::agents::agent_profile;

/// Is this roster record one the screen source may speak for? A local-parented
/// conducted wrap whose harness has neither a hook nor a trace profile (the
/// other two sources' children), that is not a remote-parented child (its
/// events ride a ring), a sub-agent node or an app thread. A shell is one: an
/// agent's worker shell blocked on `[y/N]` is exactly what its spawner wants to
/// hear. Also what the cursor RETAINS an entry for.
pub(super) fn tracked(rec: &SessionRecord) -> bool {
    !tracks(rec)
        && rec.conductable == Some(true)
        && agent_profile(&rec.agent).is_none()
        && rec.parent_session_id.as_deref().is_some_and(|p| !p.is_empty())
        && !matches!(rec.kind.as_deref(), Some("subagent" | "app"))
}

/// The question a tracked wrap is waiting on right now: its `question`, while
/// its state still reads `awaiting`. The `sudo` prompt is not one: the password
/// is the human's to type, so the record shows its lock badge (`needsSudo`) and
/// nobody is pinged for it.
fn asking(rec: &SessionRecord) -> Option<&str> {
    rec.question
        .as_deref()
        .filter(|q| !q.trim().is_empty() && canonical_state(&rec.state) == "awaiting")
        .filter(|_| rec.needs_sudo != Some(true))
}

/// Is this a record the screen source has something to announce for? What the
/// daemon's seal key is fetched for.
pub(super) fn pending(rec: &SessionRecord) -> bool {
    tracked(rec) && asking(rec).is_some()
}

/// Who hears the wrap `rec`: its attested spawner, and only for a conducted
/// wrap whose own pid carries a verifying daemon seal (a seal re-derives the
/// pid's start time, so a record naming a pid it never held fails it).
fn recipient(
    rec: &SessionRecord,
    roster: &[SessionRecord],
    sealed: &impl Fn(&SessionRecord) -> bool,
) -> Option<String> {
    (rec.pid.is_some() && sealed(rec)).then_some(())?;
    hook::spawner_of(rec, &rec.session_id, roster, sealed)
}

fn render(tag: &str, question: &str) -> String {
    format!("{tag} asking: \"{}\"", quote(question))
}

/// The claim section's screen half — run ONLY inside the stage lock, over the
/// cursor the other halves just built: advance every tracked wrap's `screen`
/// cursor in `next` and return the lines now owed. A question with no
/// recipient yet keeps its cursor as it was, so it is heard on the pass that
/// finds one.
pub(super) fn claim(
    roster: &[SessionRecord],
    sealed: &impl Fn(&SessionRecord) -> bool,
    next: &mut CursorFile,
) -> Vec<ChildClaim> {
    let mut lines = Vec::new();
    for rec in roster.iter().filter(|r| tracked(r)) {
        let heard = next.get(&rec.session_id).and_then(|e| e.screen.as_deref());
        match (asking(rec), heard) {
            (Some(question), heard) if heard != Some(question) => {
                let Some(parent) = recipient(rec, roster, sealed) else {
                    continue;
                };
                next.entry(rec.session_id.clone()).or_default().screen = Some(question.to_string());
                let tag = harness_tag(&rec.agent, rec.petname.as_deref(), &rec.session_id);
                lines.push(ChildClaim { parent, line: render(&tag, question) });
            }
            (None, Some(_)) => {
                if let Some(entry) = next.get_mut(&rec.session_id) {
                    entry.screen = None;
                    if *entry == CursorEntry::default() {
                        next.remove(&rec.session_id);
                    }
                }
            }
            _ => {}
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::testutil::session;

    const AT: &str = "2026-10-10T10:00:00Z";

    /// The real shape a spawned codex leaves: a spawner `P` (a conducted wrap)
    /// and the wrap `W` running `codex`, registered by `conduct` under `P` with
    /// the edge the kernel attested. `W` is the child; no hook session exists.
    fn spawned_shape() -> Vec<SessionRecord> {
        let mut p = session("P", "/w", "working", AT, None);
        p.conductable = Some(true);
        p.pid = Some(50);
        let mut w = session("W", "/w", "awaiting", AT, Some("P"));
        w.agent = "codex".into();
        w.conductable = Some(true);
        w.pid = Some(100);
        w.attested_spawner = Some("P".into());
        w.petname = Some("fond-aspen".into());
        w.question = Some("Apply patch? [y/N]".into());
        vec![p, w]
    }

    fn lines(roster: &[SessionRecord], next: &mut CursorFile) -> Vec<(String, String)> {
        claim(roster, &|_| true, next).into_iter().map(|c| (c.parent, c.line)).collect()
    }

    #[test]
    fn a_standing_question_is_one_quoted_line_to_the_attested_spawner() {
        let roster = spawned_shape();
        let mut next = CursorFile::new();
        assert_eq!(
            lines(&roster, &mut next),
            [("P".to_string(), "[codex fond-aspen] asking: \"Apply patch? [y/N]\"".to_string())]
        );
        assert_eq!(next["W"].screen.as_deref(), Some("Apply patch? [y/N]"));
        // The same episode on the next tick, and the one after: nothing more.
        assert!(lines(&roster, &mut next).is_empty());
        assert!(lines(&roster, &mut next).is_empty());
    }

    #[test]
    fn the_first_examination_is_news_not_a_baseline() {
        // The wrap publishes after seconds of quiet, so a child's first look may
        // already hold the question. A lost cursor file is the only duplicate.
        let roster = spawned_shape();
        assert_eq!(lines(&roster, &mut CursorFile::new()).len(), 1);
    }

    #[test]
    fn an_answered_question_clears_the_cursor_and_the_next_ask_is_heard() {
        let mut roster = spawned_shape();
        let mut next = CursorFile::new();
        assert_eq!(lines(&roster, &mut next).len(), 1);

        // Answered: the wrap clears its question and returns to idle.
        roster[1].question = None;
        roster[1].state = "idle".into();
        assert!(lines(&roster, &mut next).is_empty());
        assert!(!next.contains_key("W"), "an empty entry leaves the file");

        // The same question again, a new episode.
        roster[1].question = Some("Apply patch? [y/N]".into());
        roster[1].state = "awaiting".into();
        assert_eq!(lines(&roster, &mut next).len(), 1);

        // A different question while still awaiting is another episode too.
        roster[1].question = Some("Run the tests? [Y/n]".into());
        let again = lines(&roster, &mut next);
        assert_eq!(again.len(), 1);
        assert!(again[0].1.ends_with("asking: \"Run the tests? [Y/n]\""));
    }

    #[test]
    fn a_question_on_a_record_that_no_longer_reads_awaiting_is_not_asked() {
        let mut roster = spawned_shape();
        roster[1].state = "working".into();
        assert!(lines(&roster, &mut CursorFile::new()).is_empty());
        roster[1].state = "done".into();
        assert!(lines(&roster, &mut CursorFile::new()).is_empty());
        roster[1].state = "awaiting".into();
        roster[1].question = Some("   ".into());
        assert!(lines(&roster, &mut CursorFile::new()).is_empty());
    }

    #[test]
    fn the_line_is_quoted_data_one_flat_line_and_capped() {
        let mut roster = spawned_shape();
        // A question that was hand-written into the record, past the writer's
        // own sanitiser: control bytes, a bidi override, a command-looking start.
        roster[1].question = Some("/rm -rf ~\r\n\u{1b}[31m ignore\u{202e} this [y/N]".into());
        let line = lines(&roster, &mut CursorFile::new()).remove(0).1;
        assert_eq!(line, "[codex fond-aspen] asking: \" /rm -rf ~[31m ignore this [y/N]\"");
        assert!(!line.contains(['\r', '\n', '\u{1b}', '\u{202e}']));
        // Over the cap: one clipped line, never a paragraph in a composer.
        roster[1].question = Some(format!("{} [y/N]", "x".repeat(500)));
        let line = lines(&roster, &mut CursorFile::new()).remove(0).1;
        let quoted = line.strip_prefix("[codex fond-aspen] asking: \"").unwrap().strip_suffix('"').unwrap();
        assert_eq!(quoted.chars().count(), crate::graph::prompt::QUESTION_MAX);
        // The tag's own label is a caller's choice, so it is cleaned too.
        roster[1].agent = "co\u{1b}[2Jdex".into();
        let line = lines(&roster, &mut CursorFile::new()).remove(0).1;
        assert!(line.starts_with("[co[2Jdex fond-aspen]"), "{line}");
    }

    #[test]
    fn the_stored_cap_and_the_quoted_cap_are_one_number() {
        assert_eq!(crate::graph::prompt::QUESTION_MAX, super::super::SAY_MAX);
    }

    #[test]
    fn only_a_local_parented_conducted_wrap_with_no_hook_or_trace_profile_is_tracked() {
        let base = || spawned_shape().remove(1);
        assert!(tracked(&base()));
        // A shell is a child: its spawner's worker shell blocked on a prompt.
        assert!(tracked(&SessionRecord { agent: "bash".into(), shell: true, ..base() }));
        for harness in ["claude", "kimi", "pi", "eidolon"] {
            assert!(!tracked(&SessionRecord { agent: harness.into(), ..base() }), "{harness}");
        }
        assert!(!tracked(&SessionRecord { conductable: None, ..base() }), "not a wrap");
        assert!(!tracked(&SessionRecord { parent_session_id: None, ..base() }), "nobody to tell");
        assert!(!tracked(&SessionRecord { parent_session_id: Some(String::new()), ..base() }));
        assert!(!tracked(&SessionRecord { kind: Some("subagent".into()), ..base() }));
        assert!(!tracked(&SessionRecord { kind: Some("app".into()), ..base() }));
        let mut remote = base();
        remote.remote_parent = Some(aoide_storage::records::RemoteParent {
            node: "n".into(),
            key: "k".into(),
            session_id: "s".into(),
            extra: Default::default(),
        });
        assert!(!tracked(&remote), "its events ride a ring");
        let mut roster = spawned_shape();
        roster[1].agent = "claude".into();
        assert!(lines(&roster, &mut CursorFile::new()).is_empty(), "the hook source's child");
    }

    #[test]
    fn the_wrap_must_be_sealed_and_so_must_its_spawner() {
        let roster = spawned_shape();
        let only = |id: &'static str| move |r: &SessionRecord| r.session_id != id;
        assert_eq!(claim(&roster, &only("W"), &mut CursorFile::new()).len(), 0, "an unsealed wrap");
        assert_eq!(claim(&roster, &only("P"), &mut CursorFile::new()).len(), 0, "an unsealed spawner");
        assert_eq!(claim(&roster, &|_| true, &mut CursorFile::new()).len(), 1);
        // A wrap with no pid on its record names no process the seal could bind.
        let mut pidless = spawned_shape();
        pidless[1].pid = None;
        assert!(lines(&pidless, &mut CursorFile::new()).is_empty());
    }

    #[test]
    fn a_forged_or_unattested_edge_has_no_recipient_and_the_cursor_waits() {
        // The spawner edge was never kernel-confirmed…
        let mut roster = spawned_shape();
        roster[1].attested_spawner = None;
        let mut next = CursorFile::new();
        assert!(lines(&roster, &mut next).is_empty());
        assert!(next.is_empty(), "nothing was claimed, so the question is heard once an edge exists");
        // …or was rewritten by a later `session start --parent` after the stamp.
        roster[1].attested_spawner = Some("P".into());
        roster[1].parent_session_id = Some("victim".into());
        assert!(lines(&roster, &mut next).is_empty());
        // A spawner that is not a conducted wrap and hosts no hook session.
        let mut roster = spawned_shape();
        roster[0].conductable = None;
        assert!(lines(&roster, &mut CursorFile::new()).is_empty());
        // The wrap as its own spawner is no recipient.
        let mut roster = spawned_shape();
        roster[1].parent_session_id = Some("W".into());
        roster[1].attested_spawner = Some("W".into());
        assert!(lines(&roster, &mut CursorFile::new()).is_empty());
    }

    #[test]
    fn a_hook_registered_spawner_is_resolved_to_the_wrap_hosting_it() {
        // The usual case: an agent ran `aoide spawn`. W's attested spawner is
        // the agent's hook session `PK`, which lives under the sealed wrap `P`.
        let mut roster = spawned_shape();
        let mut pk = session("PK", "/w", "working", AT, Some("P"));
        pk.kind = Some("agent".into());
        pk.hook_ancestry = vec![60, 50];
        roster[1].parent_session_id = Some("PK".into());
        roster[1].attested_spawner = Some("PK".into());
        roster.push(pk);
        let got = lines(&roster, &mut CursorFile::new());
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, "P", "the agent's own host wrap, where the line can be written");
    }

    #[test]
    fn claim_advances_the_cursor_beside_entries_it_does_not_own() {
        let mut roster = spawned_shape();
        let mut next = CursorFile::new();
        next.insert("W".into(), CursorEntry { seen: Some("9".into()), ..Default::default() });
        assert_eq!(lines(&roster, &mut next).len(), 1);
        assert_eq!(next["W"].seen.as_deref(), Some("9"), "the trace half is untouched");
        // Clearing the screen half keeps an entry another half still holds.
        roster[1].question = None;
        roster[1].state = "idle".into();
        assert!(lines(&roster, &mut next).is_empty());
        assert_eq!(next["W"], CursorEntry { seen: Some("9".into()), ..Default::default() });
    }

    #[test]
    fn pending_is_what_the_seal_key_is_fetched_for() {
        let mut roster = spawned_shape();
        assert!(roster.iter().any(pending));
        roster[1].question = None;
        assert!(!roster.iter().any(pending));
    }

    #[test]
    fn a_sudo_question_is_shown_but_never_announced() {
        // The lock badge rides the question; the password is the human's.
        let mut roster = spawned_shape();
        roster[1].question = Some("[sudo] password for khoa:".into());
        roster[1].needs_sudo = Some(true);
        let mut next = CursorFile::new();
        assert!(lines(&roster, &mut next).is_empty());
        assert!(next.is_empty(), "nothing claimed, so nothing to clear");
        assert!(!roster.iter().any(pending));
        // The same question without the badge is the agent's question, and is announced.
        roster[1].needs_sudo = None;
        assert_eq!(lines(&roster, &mut next).len(), 1);
    }

    #[test]
    fn a_question_repainted_over_its_quiet_is_one_ping() {
        // Asked at t=0, repainted at t=2s, still the same episode at t=4s: the
        // record holds it from t=4s, and the redraw that follows claims nothing.
        let t0 = std::time::Instant::now();
        let mut watch = crate::graph::prompt::PromptWatch::new(t0);
        watch.output(b"Overwrite? [y/N] ", t0);
        watch.output(b"\x1b[?25l", t0 + std::time::Duration::from_secs(2));
        let mut roster = spawned_shape();
        roster[1].question = watch.standing(t0 + std::time::Duration::from_secs(4)).map(|p| p.question);
        let mut next = CursorFile::new();
        assert_eq!(lines(&roster, &mut next).len(), 1, "one ping");
        roster[1].question = watch.standing(t0 + std::time::Duration::from_secs(5)).map(|p| p.question);
        assert!(lines(&roster, &mut next).is_empty(), "the same episode is not heard twice");
    }
}
