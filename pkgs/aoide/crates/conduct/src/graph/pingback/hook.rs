//! The ping-back's second source: **a hook-reporting child**. The first source
//! reads an eidolon child's trace; this one reads what every other harness
//! already publishes — its `hooks.json` phase and the stamp of the last hook
//! event — and speaks for a child whose harness keeps no trace of its own
//! (claude today, any agent whose hooks write `hooks.json`).
//!
//! It is the same ping-back, not a second one. The line is raw-injected by the
//! one [`deliver`](super::deliver) (never a send: no gate, no pending entry),
//! under the resident daemon alone, to the child's local `parentSessionId`,
//! with the same skip rules (a shell, a gone, not-conductable or done parent),
//! and its at-most-once claim is written into the SAME `pingback.json` inside
//! the SAME stage-lock section as the eidolon source's — in the entry's `hook`
//! field, so the two sources never read each other's cursor.
//!
//! Three lines, one per event:
//!
//! ```text
//! [claude <petname>] awaiting · <what it waits on, when the record names it>
//! [claude <petname>] silent 12 min · last: <the tool in flight, else "working">
//! [claude <petname>] settled
//! ```
//!
//! A hook record carries only `phase` and `updatedAt`, so the cursor remembers
//! the phase it last examined: `awaiting` is heard on ENTERING that phase,
//! `settled` on leaving `working`/`awaiting` for `stopped`/`idle`, and
//! `silent` when a `working` phase's stamp is [`SILENCE_MS`] old — once per
//! stamp, so any new hook event re-arms it.

use super::{clean, is_false, tracks, ChildClaim, CursorFile};
use crate::graph::model::{canonical_state, HookRecord, SessionRecord};
use aoide_protocol::agents::agent_profile;
use aoide_storage::time::parse_iso_utc;
use serde::{Deserialize, Serialize};

/// How long a `working` phase may go without a hook event before the child is
/// called silent.
const SILENCE_MS: i64 = 12 * 60 * 1000;

/// The hook source's half of a cursor entry (`pingback.json`'s `hook` field):
/// the canonical phase and the raw `updatedAt` last examined, and the latch
/// that a silence line was sent for that stamp.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct HookCursor {
    phase: String,
    at: String,
    #[serde(default, skip_serializing_if = "is_false")]
    silent: bool,
}

/// One hook-reporting child, gathered: the line's tag, who hears it, and the
/// facts the decision reads.
pub(super) struct HookChild {
    id: String,
    tag: String,
    parent: String,
    phase: String,
    at: String,
    at_secs: Option<i64>,
    activity: Option<String>,
    tool: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
enum HookEvent {
    Awaiting { on: Option<String> },
    Silent { mins: i64, last: String },
    Settled,
}

/// Is this roster record one the hook source speaks for? A local-parented,
/// registered harness that is not an eidolon (its trace is the other source),
/// not a remote-parented child (its events ride a ring), and not a shell, a
/// sub-agent node or an app thread. Also what the cursor RETAINS an entry for.
pub(super) fn tracked(rec: &SessionRecord) -> bool {
    !tracks(rec)
        && !rec.shell
        && rec.parent_session_id.as_deref().is_some_and(|p| !p.is_empty())
        && !matches!(rec.kind.as_deref(), Some("shell" | "subagent" | "app"))
        && agent_profile(&rec.agent).is_some()
}

/// The tracked children that have a hook record. A child with none has nothing
/// to say yet, and keeps whatever cursor it had.
pub(super) fn gather(roster: &[SessionRecord], hooks: &[HookRecord]) -> Vec<HookChild> {
    roster
        .iter()
        .filter(|rec| tracked(rec))
        .filter_map(|rec| {
            let hook = hooks.iter().find(|h| h.session_id == rec.session_id)?;
            let name = rec.petname.as_deref().filter(|p| !p.trim().is_empty()).unwrap_or(&rec.session_id);
            Some(HookChild {
                id: rec.session_id.clone(),
                tag: format!("[{} {}]", rec.agent, clean(name)),
                parent: rec.parent_session_id.clone().unwrap_or_default(),
                phase: canonical_state(&hook.phase).to_string(),
                at: hook.updated_at.clone(),
                at_secs: parse_iso_utc(&hook.updated_at),
                activity: rec.activity.clone().filter(|a| !a.is_empty()),
                tool: rec.tool.clone(),
            })
        })
        .collect()
}

/// The claim section's hook half — run ONLY inside the stage lock, over the
/// cursor the eidolon half just built: advance every child's `hook` cursor in
/// `next` and return the lines now owed.
pub(super) fn claim(children: &[HookChild], next: &mut CursorFile, now_ms: i64) -> Vec<ChildClaim> {
    let mut lines = Vec::new();
    for child in children {
        let entry = next.entry(child.id.clone()).or_default();
        let (event, cursor) = decide(child, entry.hook.as_ref(), now_ms);
        entry.hook = Some(cursor);
        if let Some(event) = event {
            lines.push(ChildClaim { parent: child.parent.clone(), line: render(&child.tag, &event) });
        }
    }
    lines
}

/// One child's whole decision: the event to publish (or none) and its next
/// cursor. Pure, so every row is a table test.
///
/// A child first seen is judged by what it IS — `awaiting` and a stale
/// `working` are standing facts a parent still wants — but `settled` needs the
/// cursor to have SEEN the child live, so a child found already stopped says
/// nothing. A turn that starts and ends between two ticks is not heard.
fn decide(child: &HookChild, entry: Option<&HookCursor>, now_ms: i64) -> (Option<HookEvent>, HookCursor) {
    let before = entry.map(|e| e.phase.as_str());
    let fresh_stamp = entry.is_none_or(|e| e.at != child.at || e.phase != child.phase);
    let mut silent = !fresh_stamp && entry.is_some_and(|e| e.silent);
    let quiet_ms = child.at_secs.map(|at| now_ms - at * 1000);

    let event = match child.phase.as_str() {
        "awaiting" if before != Some("awaiting") => {
            Some(HookEvent::Awaiting { on: waiting_on(child.activity.as_deref(), child.tool.as_deref()) })
        }
        "stopped" | "idle" if matches!(before, Some("working" | "awaiting")) => Some(HookEvent::Settled),
        "working" if !silent && quiet_ms.is_some_and(|q| q >= SILENCE_MS) => {
            silent = true;
            Some(HookEvent::Silent {
                mins: quiet_ms.unwrap_or_default() / 60_000,
                last: child.activity.as_deref().map_or_else(|| "working".to_string(), clean),
            })
        }
        _ => None,
    };
    (event, HookCursor { phase: child.phase.clone(), at: child.at.clone(), silent })
}

/// What an awaiting child waits on: the tool in flight (`activity`, hook-set
/// and live), widened to the transcript's `Bash: cargo test` label only when
/// that label names the same tool — the label is refreshed at other hook
/// boundaries and can be a previous call's.
fn waiting_on(activity: Option<&str>, tool: Option<&str>) -> Option<String> {
    let activity = activity?;
    let label = tool.filter(|t| *t == activity || t.strip_prefix(activity).is_some_and(|r| r.starts_with(':')));
    Some(clean(label.unwrap_or(activity)))
}

fn render(tag: &str, event: &HookEvent) -> String {
    match event {
        HookEvent::Awaiting { on: Some(on) } => format!("{tag} awaiting · {on}"),
        HookEvent::Awaiting { on: None } => format!("{tag} awaiting"),
        HookEvent::Silent { mins, last } => format!("{tag} silent {mins} min · last: {last}"),
        HookEvent::Settled => format!("{tag} settled"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::testutil::session;

    const AT: &str = "2026-10-07T10:00:00Z";

    fn at_ms() -> i64 {
        parse_iso_utc(AT).unwrap() * 1000
    }

    fn child(phase: &str, at: &str) -> HookChild {
        HookChild {
            id: "kid".into(),
            tag: "[claude fond-aspen]".into(),
            parent: "wrap-1".into(),
            phase: phase.into(),
            at: at.into(),
            at_secs: parse_iso_utc(at),
            activity: None,
            tool: None,
        }
    }

    fn cursor(phase: &str, at: &str, silent: bool) -> HookCursor {
        HookCursor { phase: phase.into(), at: at.into(), silent }
    }

    fn line(event: Option<HookEvent>) -> Option<String> {
        event.map(|e| render("[claude fond-aspen]", &e))
    }

    #[test]
    fn entering_awaiting_is_one_line_and_a_restamp_of_it_is_not_another() {
        let mut kid = child("awaiting", AT);
        kid.activity = Some("Bash".into());
        kid.tool = Some("Bash: cargo test".into());
        let (event, next) = decide(&kid, Some(&cursor("working", "2026-10-07T09:59:00Z", false)), at_ms());
        assert_eq!(line(event).as_deref(), Some("[claude fond-aspen] awaiting · Bash: cargo test"));
        assert_eq!(next, cursor("awaiting", AT, false));

        // Same phase, a newer stamp (a second Notification): still awaiting.
        let again = child("awaiting", "2026-10-07T10:01:00Z");
        assert_eq!(decide(&again, Some(&next), at_ms() + 61_000).0, None);
        // Same stamp, a later tick: nothing either.
        assert_eq!(decide(&kid, Some(&next), at_ms() + 60_000).0, None);
        // Out and back in is a second entry.
        let (_, worked) = decide(&child("working", "2026-10-07T10:02:00Z"), Some(&next), at_ms() + 120_000);
        let back = child("awaiting", "2026-10-07T10:03:00Z");
        assert!(decide(&back, Some(&worked), at_ms() + 180_000).0.is_some());
    }

    #[test]
    fn awaiting_names_only_what_the_record_carries() {
        // Nothing in flight: the bare line.
        let (event, _) = decide(&child("awaiting", AT), None, at_ms());
        assert_eq!(line(event).as_deref(), Some("[claude fond-aspen] awaiting"));

        // A stale transcript label for a DIFFERENT tool is not the answer.
        let mut kid = child("awaiting", AT);
        kid.activity = Some("Bash".into());
        kid.tool = Some("Agent: Lane H".into());
        assert_eq!(line(decide(&kid, None, at_ms()).0).as_deref(), Some("[claude fond-aspen] awaiting · Bash"));

        // Model-authored text is cleaned: one line, no control characters.
        kid.activity = Some("Bash".into());
        kid.tool = Some("Bash: echo hi\r\n!rm -rf".into());
        let text = line(decide(&kid, None, at_ms()).0).unwrap();
        assert!(!text.contains(['\r', '\n']), "{text:?}");
    }

    #[test]
    fn stopping_after_work_is_settled_once_and_a_child_found_stopped_says_nothing() {
        for live in ["working", "awaiting"] {
            let (event, next) = decide(&child("stopped", AT), Some(&cursor(live, "2026-10-07T09:50:00Z", false)), at_ms());
            assert_eq!(line(event).as_deref(), Some("[claude fond-aspen] settled"), "{live}");
            // The same stop on the next tick, and the hour-later decay to idle.
            assert_eq!(decide(&child("stopped", AT), Some(&next), at_ms() + 12_000).0, None);
            assert_eq!(decide(&child("idle", "2026-10-07T11:00:00Z"), Some(&next), at_ms() + 3_600_000).0, None);
        }
        // working -> idle (a harness that writes idle itself) settles too.
        assert!(decide(&child("idle", AT), Some(&cursor("working", "2026-10-07T09:50:00Z", false)), at_ms()).0.is_some());
        // Never seen live: baseline only.
        assert_eq!(decide(&child("stopped", AT), None, at_ms()).0, None);
        assert_eq!(decide(&child("idle", AT), Some(&cursor("stopped", AT, false)), at_ms()).0, None);
        // The session ending is not a settle.
        assert_eq!(decide(&child("done", AT), Some(&cursor("working", AT, false)), at_ms()).0, None);
    }

    #[test]
    fn silence_fires_at_twelve_minutes_exactly_once_per_stamp() {
        let kid = child("working", AT);
        let seen = cursor("working", AT, false);
        // One millisecond short of the threshold is not silence; the threshold is.
        assert_eq!(decide(&kid, Some(&seen), at_ms() + SILENCE_MS - 1).0, None);
        let (event, next) = decide(&kid, Some(&seen), at_ms() + SILENCE_MS);
        assert_eq!(line(event).as_deref(), Some("[claude fond-aspen] silent 12 min · last: working"));
        assert!(next.silent);
        // Later ticks of the same silence say nothing more.
        assert_eq!(decide(&kid, Some(&next), at_ms() + 30 * 60_000).0, None);
        // A new hook event re-arms it: the next 12 quiet minutes speak again.
        let moved = child("working", "2026-10-07T10:30:00Z");
        let (event, rearmed) = decide(&moved, Some(&next), at_ms() + 31 * 60_000);
        assert_eq!(event, None);
        assert!(!rearmed.silent);
        let (event, _) = decide(&moved, Some(&rearmed), at_ms() + 30 * 60_000 + SILENCE_MS + 60_000);
        assert_eq!(line(event).as_deref(), Some("[claude fond-aspen] silent 13 min · last: working"));
    }

    #[test]
    fn silence_names_the_tool_in_flight_and_only_belongs_to_a_working_phase() {
        let mut kid = child("working", AT);
        kid.activity = Some("Bash".into());
        let (event, _) = decide(&kid, None, at_ms() + 20 * 60_000);
        assert_eq!(line(event).as_deref(), Some("[claude fond-aspen] silent 20 min · last: Bash"));
        for phase in ["awaiting", "stopped", "idle", "done"] {
            let quiet = child(phase, AT);
            let (event, _) = decide(&quiet, Some(&cursor(phase, AT, false)), at_ms() + 3_600_000);
            assert_eq!(event, None, "{phase}");
        }
        // A stamp that does not parse can never be called silent.
        assert_eq!(decide(&child("working", "garbled"), None, at_ms() + 3_600_000).0, None);
    }

    #[test]
    fn a_hook_record_names_its_phase_through_the_canonical_fold() {
        let mut rec = session("kid", "/w", "working", AT, Some("wrap-1"));
        rec.petname = Some("fond-aspen".into());
        let hook = |phase: &str| HookRecord { session_id: "kid".into(), phase: phase.into(), updated_at: AT.into(), ..Default::default() };
        let kids = gather(&[rec.clone()], &[hook("running")]);
        assert_eq!((kids[0].phase.as_str(), kids[0].tag.as_str()), ("working", "[claude fond-aspen]"));
        assert!(gather(&[rec], &[]).is_empty(), "no hook record, nothing to say");
    }

    #[test]
    fn only_a_local_parented_harness_that_is_not_an_eidolon_is_tracked() {
        let base = || session("kid", "/w", "working", AT, Some("wrap-1"));
        assert!(tracked(&base()));
        assert!(tracked(&SessionRecord { agent: "kimi".into(), ..base() }));
        assert!(!tracked(&SessionRecord { agent: "eidolon".into(), ..base() }), "the trace source's");
        assert!(!tracked(&SessionRecord { agent: "shell".into(), ..base() }));
        assert!(!tracked(&SessionRecord { agent: "nothing".into(), ..base() }));
        assert!(!tracked(&SessionRecord { shell: true, ..base() }));
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
    }

    #[test]
    fn claim_advances_the_cursor_beside_an_entry_it_does_not_own() {
        let mut next = CursorFile::new();
        next.insert("kid".into(), super::super::CursorEntry { seen: Some("9".into()), ..Default::default() });
        let kids = [child("awaiting", AT)];
        let lines = claim(&kids, &mut next, at_ms());
        assert_eq!(lines.len(), 1);
        assert_eq!((lines[0].parent.as_str(), lines[0].line.as_str()), ("wrap-1", "[claude fond-aspen] awaiting"));
        assert_eq!(next["kid"].seen.as_deref(), Some("9"), "the trace half is untouched");
        assert_eq!(next["kid"].hook, Some(cursor("awaiting", AT, false)));
        assert!(claim(&kids, &mut next, at_ms() + 12_000).is_empty(), "at most once");
    }
}
