//! The ping-back's second source: **a hook-reporting child**. The first source
//! reads an eidolon child's trace; this one reads what every other harness
//! already publishes — its `hooks.json` phase and the stamp of the last hook
//! event — and speaks for a child whose harness keeps no trace of its own
//! (claude today, any agent whose hooks write `hooks.json`).
//!
//! It is the same ping-back, not a second one. The line is raw-injected by the
//! one [`deliver`](super::deliver) (never a send: no gate, no pending entry),
//! under the resident daemon alone, with the same skip rules (a shell, a gone,
//! not-conductable or done recipient), and its at-most-once claim is written
//! into the SAME `pingback.json` inside the SAME stage-lock section as the
//! eidolon source's — in the entry's `hook` field, so the two sources never
//! read each other's cursor.
//!
//! Three lines, one per event:
//!
//! ```text
//! [claude <petname>] awaiting
//! [claude <petname>] silent 12 min · last: <the tool in flight, else "working">
//! [claude <petname>] settled
//! ```
//!
//! **Who hears it: the SPAWNER, and only over an attested edge.** A hook
//! child's `parentSessionId` is its OWN host wrap (the conducted wrap whose
//! harness it is), so the line must never go there: a headless host would
//! take it as its own next prompt and an interactive one would have it typed
//! into the user's composer. [`recipient`] resolves it in three steps, each a
//! kernel fact rather than a caller's word:
//!
//! 1. the child's host wrap is its `parentSessionId` record, a conducted wrap
//!    whose pid is in the child's `hookAncestry` (stamped by the hook door
//!    from the hook process's own peer credentials — `session start` cannot
//!    write it, so a forged `--parent` edge has none) AND whose daemon seal
//!    verifies. The seal is what makes the pid a fact: it re-derives the pid's
//!    `/proc` start time, so a pid reused by another process after the wrap
//!    died (its record lingers `done`, and nothing here checks state), or a
//!    pid hand-edited into `sessions.json`, names a process the seal was never
//!    minted over;
//! 2. the spawner is that wrap's `parentSessionId`, honoured only while it
//!    equals the wrap's `attestedSpawner` — the parent `conduct` registration
//!    saw in its own `/proc` ancestry (`window::spawner_is_attested`). A
//!    detached spawn that registers only after its spawner returned has lost
//!    that ancestry and gets no stamp: it is silent, never guessed;
//! 3. a spawner that is itself a conducted wrap (sealed) hears it directly; a
//!    hook-registered spawner (the usual case: an agent that ran `aoide
//!    spawn`) is resolved to its own sealed host wrap by step 1.
//!
//! Anything else — an unattested edge, a spawner that resolves back to the
//! child or its host — has no recipient and the child is not tracked.
//!
//! A hook record carries only `phase` and `updatedAt`, so the cursor remembers
//! the phase it last examined: `awaiting` is heard on ENTERING that phase,
//! `settled` on leaving `working`/`awaiting` for `stopped`/`idle`, and
//! `silent` when a `working` phase's stamp is [`SILENCE_MS`] old — once per
//! stamp, so any new hook event re-arms it. The first examination of a child
//! is a baseline only.

use super::{clean, is_false, tracks, ChildClaim, CursorFile};
use crate::graph::model::{canonical_state, HookRecord, SessionRecord};
use aoide_protocol::agents::agent_profile;
use aoide_storage::time::parse_iso_utc;
use serde::{Deserialize, Serialize};

/// How long a `working` phase may go without a hook event before the child is
/// called silent.
const SILENCE_MS: i64 = 12 * 60 * 1000;
/// Past this a `working` stamp is a dead record, not a quiet child: a crashed
/// harness leaves `working` behind for days, and "silent 4000 min" is noise.
const STALE_MS: i64 = 24 * 60 * 60 * 1000;

/// The hook source's half of a cursor entry (`pingback.json`'s `hook` field):
/// the canonical phase and the raw `updatedAt` last examined, and the latch
/// that a silence line was sent for that `updatedAt`.
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
    recipient: String,
    phase: String,
    at: String,
    at_secs: Option<i64>,
    activity: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
enum HookEvent {
    Awaiting,
    Silent { mins: i64, last: String },
    Settled,
}

/// Is this roster record one the hook source may speak for? A local-parented,
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

/// The conducted wrap `rec` runs under: its `parentSessionId` record, when
/// that is a conducted wrap with a pid the hook door saw in `rec`'s own
/// ancestry and a daemon seal over that pid (`sealed`: `verify_seal_over`
/// against the live daemon key, fail-closed). The kernel's and the daemon's
/// word, not the record's.
fn host_wrap<'a>(
    rec: &SessionRecord,
    roster: &'a [SessionRecord],
    sealed: &impl Fn(&SessionRecord) -> bool,
) -> Option<&'a SessionRecord> {
    let parent = rec.parent_session_id.as_deref()?;
    let wrap = roster.iter().find(|r| r.session_id == parent)?;
    let pid = i32::try_from(wrap.pid?).ok()?;
    (wrap.conductable == Some(true) && rec.hook_ancestry.contains(&pid) && sealed(wrap)).then_some(wrap)
}

/// Who hears `kid`: the conducted session that SPAWNED it, over an edge the
/// kernel attested (the module doc's three steps). Never the child, never its
/// own host wrap.
fn recipient(
    kid: &SessionRecord,
    roster: &[SessionRecord],
    sealed: &impl Fn(&SessionRecord) -> bool,
) -> Option<String> {
    let host = host_wrap(kid, roster, sealed)?;
    let spawner_id = host.parent_session_id.as_deref()?;
    if host.attested_spawner.as_deref() != Some(spawner_id) {
        return None;
    }
    let spawner = roster.iter().find(|r| r.session_id == spawner_id)?;
    let target = if spawner.conductable == Some(true) && sealed(spawner) {
        spawner
    } else {
        host_wrap(spawner, roster, sealed)?
    };
    (target.session_id != kid.session_id && target.session_id != host.session_id)
        .then(|| target.session_id.clone())
}

/// The tracked children that have a hook record and a recipient. A child with
/// either missing has nothing to say or nobody to say it to, and keeps
/// whatever cursor it had.
pub(super) fn gather(
    roster: &[SessionRecord],
    hooks: &[HookRecord],
    sealed: &impl Fn(&SessionRecord) -> bool,
) -> Vec<HookChild> {
    roster
        .iter()
        .filter(|rec| tracked(rec))
        .filter_map(|rec| {
            let hook = hooks.iter().find(|h| h.session_id == rec.session_id)?;
            let name = rec.petname.as_deref().filter(|p| !p.trim().is_empty()).unwrap_or(&rec.session_id);
            Some(HookChild {
                id: rec.session_id.clone(),
                tag: format!("[{} {}]", rec.agent, clean(name)),
                recipient: recipient(rec, roster, sealed)?,
                phase: canonical_state(&hook.phase).to_string(),
                at: hook.updated_at.clone(),
                at_secs: parse_iso_utc(&hook.updated_at),
                activity: rec.activity.clone().filter(|a| !a.is_empty()),
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
            lines.push(ChildClaim { parent: child.recipient.clone(), line: render(&child.tag, &event) });
        }
    }
    lines
}

/// One child's whole decision: the event to publish (or none) and its next
/// cursor. Pure, so every row is a table test.
///
/// The first examination of a child — a fresh roster entry, or a cursor that
/// was lost — is a BASELINE: it records what the child is and says nothing, so
/// a restart never announces everything that was already true. `settled` then
/// needs the cursor to have SEEN the child `working`/`awaiting`, and a turn
/// that starts and ends between two ticks is not heard.
fn decide(child: &HookChild, entry: Option<&HookCursor>, now_ms: i64) -> (Option<HookEvent>, HookCursor) {
    let Some(entry) = entry else {
        let baseline = HookCursor { phase: child.phase.clone(), at: child.at.clone(), silent: false };
        return (None, baseline);
    };
    let fresh_stamp = entry.at != child.at || entry.phase != child.phase;
    let mut silent = !fresh_stamp && entry.silent;
    let quiet_ms = child.at_secs.map(|at| now_ms - at * 1000);

    let event = match child.phase.as_str() {
        "awaiting" if entry.phase != "awaiting" => Some(HookEvent::Awaiting),
        "stopped" | "idle" if matches!(entry.phase.as_str(), "working" | "awaiting") => Some(HookEvent::Settled),
        "working" if !silent && quiet_ms.is_some_and(|q| (SILENCE_MS..STALE_MS).contains(&q)) => {
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

fn render(tag: &str, event: &HookEvent) -> String {
    match event {
        HookEvent::Awaiting => format!("{tag} awaiting"),
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
            recipient: "spawner".into(),
            phase: phase.into(),
            at: at.into(),
            at_secs: parse_iso_utc(at),
            activity: None,
        }
    }

    fn cursor(phase: &str, at: &str, silent: bool) -> HookCursor {
        HookCursor { phase: phase.into(), at: at.into(), silent }
    }

    fn line(event: Option<HookEvent>) -> Option<String> {
        event.map(|e| render("[claude fond-aspen]", &e))
    }

    #[test]
    fn the_first_examination_of_a_child_is_a_baseline_and_says_nothing() {
        // Whatever it already is: awaiting, stale working, stopped.
        for (phase, age) in [("awaiting", 0), ("working", 3_600_000), ("stopped", 0), ("working", 0)] {
            let (event, next) = decide(&child(phase, AT), None, at_ms() + age);
            assert_eq!(event, None, "{phase}");
            assert_eq!(next, cursor(phase, AT, false));
        }
        // The pass after the baseline decides normally.
        let (_, base) = decide(&child("working", AT), None, at_ms());
        assert!(decide(&child("awaiting", "2026-10-07T10:01:00Z"), Some(&base), at_ms() + 60_000).0.is_some());
    }

    #[test]
    fn entering_awaiting_is_one_bare_line_and_a_restamp_of_it_is_not_another() {
        let working = cursor("working", "2026-10-07T09:59:00Z", false);
        let (event, next) = decide(&child("awaiting", AT), Some(&working), at_ms());
        assert_eq!(line(event).as_deref(), Some("[claude fond-aspen] awaiting"));
        assert_eq!(next, cursor("awaiting", AT, false));

        // Same phase, a newer stamp (a second Notification): still awaiting.
        let again = child("awaiting", "2026-10-07T10:01:00Z");
        assert_eq!(decide(&again, Some(&next), at_ms() + 61_000).0, None);
        // Same stamp, a later tick: nothing either.
        assert_eq!(decide(&child("awaiting", AT), Some(&next), at_ms() + 60_000).0, None);
        // Out and back in is a second entry.
        let (_, worked) = decide(&child("working", "2026-10-07T10:02:00Z"), Some(&next), at_ms() + 120_000);
        let back = child("awaiting", "2026-10-07T10:03:00Z");
        assert!(decide(&back, Some(&worked), at_ms() + 180_000).0.is_some());
    }

    #[test]
    fn stopping_after_work_is_settled_once() {
        for live in ["working", "awaiting"] {
            let seen = cursor(live, "2026-10-07T09:50:00Z", false);
            let (event, next) = decide(&child("stopped", AT), Some(&seen), at_ms());
            assert_eq!(line(event).as_deref(), Some("[claude fond-aspen] settled"), "{live}");
            // The same stop on the next tick, and the hour-later decay to idle.
            assert_eq!(decide(&child("stopped", AT), Some(&next), at_ms() + 12_000).0, None);
            assert_eq!(decide(&child("idle", "2026-10-07T11:00:00Z"), Some(&next), at_ms() + 3_600_000).0, None);
        }
        // working -> idle (a harness that writes idle itself) settles too.
        assert!(decide(&child("idle", AT), Some(&cursor("working", "2026-10-07T09:50:00Z", false)), at_ms()).0.is_some());
        // Seen stopped, stays stopped: nothing.
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
    fn a_stamp_a_day_old_is_a_dead_record_not_a_silence() {
        let seen = cursor("working", AT, false);
        let kid = child("working", AT);
        assert!(decide(&kid, Some(&seen), at_ms() + STALE_MS - 1).0.is_some());
        assert_eq!(decide(&kid, Some(&seen), at_ms() + STALE_MS).0, None);
        assert_eq!(decide(&kid, Some(&seen), at_ms() + 4000 * 60_000).0, None, "no absurd minutes, ever");
    }

    #[test]
    fn silence_names_the_tool_in_flight_and_only_belongs_to_a_working_phase() {
        let mut kid = child("working", AT);
        kid.activity = Some("Bash".into());
        let (event, _) = decide(&kid, Some(&cursor("working", AT, false)), at_ms() + 20 * 60_000);
        assert_eq!(line(event).as_deref(), Some("[claude fond-aspen] silent 20 min · last: Bash"));
        for phase in ["awaiting", "stopped", "idle", "done"] {
            let quiet = child(phase, AT);
            let (event, _) = decide(&quiet, Some(&cursor(phase, AT, false)), at_ms() + 3_600_000);
            assert_eq!(event, None, "{phase}");
        }
        // A stamp that does not parse can never be called silent.
        let garbled = cursor("working", "garbled", false);
        assert_eq!(decide(&child("working", "garbled"), Some(&garbled), at_ms() + 3_600_000).0, None);
    }

    /// The REAL two-record shape a spawned claude leaves: a spawner `P` (a
    /// conducted wrap), the wrap `W` that hosts the child (its parent `P`, the
    /// edge the kernel attested at registration), and the hook-registered
    /// child `K` whose own `parentSessionId` is `W` and whose hook ancestry
    /// holds `W`'s pid.
    fn spawned_shape() -> Vec<SessionRecord> {
        let mut p = session("P", "/w", "working", AT, None);
        p.conductable = Some(true);
        p.pid = Some(50);
        let mut w = session("W", "/w", "working", AT, Some("P"));
        w.conductable = Some(true);
        w.pid = Some(100);
        w.attested_spawner = Some("P".into());
        let mut k = session("K", "/w", "working", AT, Some("W"));
        k.kind = Some("agent".into());
        k.petname = Some("fond-aspen".into());
        k.hook_ancestry = vec![999, 100, 1];
        vec![p, w, k]
    }

    fn who(roster: &[SessionRecord], id: &str) -> Option<String> {
        who_sealed(roster, id, &|_| true)
    }

    fn who_sealed(
        roster: &[SessionRecord],
        id: &str,
        sealed: &impl Fn(&SessionRecord) -> bool,
    ) -> Option<String> {
        recipient(roster.iter().find(|r| r.session_id == id).unwrap(), roster, sealed)
    }

    #[test]
    fn the_spawner_hears_a_child_and_its_own_host_wrap_never_does() {
        let roster = spawned_shape();
        assert_eq!(who(&roster, "K").as_deref(), Some("P"), "the spawner, not the host wrap W");

        // A hook-registered spawner (an agent that ran `aoide spawn`) is
        // resolved to the wrap hosting IT: P's own hook record `PK`, under
        // wrap `P`, is what W's attested parent names.
        let mut roster = spawned_shape();
        let mut pk = session("PK", "/w", "working", AT, Some("P"));
        pk.kind = Some("agent".into());
        pk.hook_ancestry = vec![60, 50];
        roster[1].parent_session_id = Some("PK".into());
        roster[1].attested_spawner = Some("PK".into());
        roster.push(pk);
        assert_eq!(who(&roster, "K").as_deref(), Some("P"));
    }

    #[test]
    fn a_child_is_never_its_own_recipient_under_any_shape() {
        // Its own parent edge pointing at itself.
        let mut roster = spawned_shape();
        roster[2].parent_session_id = Some("K".into());
        assert_eq!(who(&roster, "K"), None);
        // Its host wrap naming the child as the spawner.
        let mut roster = spawned_shape();
        roster[1].parent_session_id = Some("K".into());
        roster[1].attested_spawner = Some("K".into());
        assert_eq!(who(&roster, "K"), None);
        // The host wrap as its own spawner.
        let mut roster = spawned_shape();
        roster[1].parent_session_id = Some("W".into());
        roster[1].attested_spawner = Some("W".into());
        assert_eq!(who(&roster, "K"), None);
        // A spawner that is another hook record living under the SAME wrap.
        let mut roster = spawned_shape();
        let mut sibling = session("S", "/w", "working", AT, Some("W"));
        sibling.kind = Some("agent".into());
        sibling.hook_ancestry = vec![998, 100];
        roster[1].parent_session_id = Some("S".into());
        roster[1].attested_spawner = Some("S".into());
        roster.push(sibling);
        assert_eq!(who(&roster, "K"), None, "it resolves back to the host wrap");
        // A host wrap with no spawner at all: a user's own terminal.
        let mut roster = spawned_shape();
        roster[1].parent_session_id = None;
        assert_eq!(who(&roster, "K"), None);
    }

    #[test]
    fn a_forged_or_unattested_edge_has_no_recipient() {
        // `session start --id forged --agent claude --parent victim`: a record
        // the hook door never stamped, naming a conducted victim directly.
        let mut roster = spawned_shape();
        let mut forged = session("forged", "/w", "working", AT, Some("P"));
        forged.kind = Some("agent".into());
        roster.push(forged);
        assert_eq!(who(&roster, "forged"), None, "no hook ancestry, no host wrap");

        // …or naming the real host wrap, still with no evidence it runs there.
        let mut roster = spawned_shape();
        roster[2].hook_ancestry.clear();
        assert_eq!(who(&roster, "K"), None);

        // …or a re-parented real child: the hook ancestry holds W's pid, not P's.
        let mut roster = spawned_shape();
        roster[2].parent_session_id = Some("P".into());
        assert_eq!(who(&roster, "K"), None, "P is a wrap, but not the one K runs under");

        // The wrap's spawner edge was never kernel-confirmed (an explicit
        // `--parent` or env the kernel did not back)…
        let mut roster = spawned_shape();
        roster[1].attested_spawner = None;
        assert_eq!(who(&roster, "K"), None);
        // …or was rewritten by a later `session start --parent` after the stamp.
        let mut roster = spawned_shape();
        roster[1].parent_session_id = Some("victim".into());
        assert_eq!(who(&roster, "K"), None);

        // A host record that is not a conducted wrap is no host.
        let mut roster = spawned_shape();
        roster[1].conductable = None;
        assert_eq!(who(&roster, "K"), None);
    }

    #[test]
    fn a_wrap_whose_pid_was_rewritten_or_whose_seal_fails_yields_no_recipient() {
        // The seal re-derives the pid's start time, so a pid reused by
        // another process after the wrap died (or a pid hand-edited into
        // `sessions.json`) fails it. Modelled as a verifier that refuses the
        // one record whose pid moved.
        let mut roster = spawned_shape();
        roster[1].pid = Some(777);
        roster[2].hook_ancestry = vec![999, 777, 1];
        let honest = |r: &SessionRecord| r.pid != Some(777);
        assert_eq!(who_sealed(&roster, "K", &honest), None, "the host wrap fails its seal");
        assert_eq!(who_sealed(&roster, "K", &|_| true).as_deref(), Some("P"), "…and only the seal stops it");

        // An unsealed host, an unsealed conducted spawner, each on their own.
        let roster = spawned_shape();
        assert_eq!(who_sealed(&roster, "K", &|r: &SessionRecord| r.session_id != "W"), None);
        assert_eq!(who_sealed(&roster, "K", &|r: &SessionRecord| r.session_id != "P"), None);

        // A hook-registered spawner is only as good as ITS host wrap's seal.
        let mut roster = spawned_shape();
        let mut pk = session("PK", "/w", "working", AT, Some("P"));
        pk.kind = Some("agent".into());
        pk.hook_ancestry = vec![60, 50];
        roster[1].parent_session_id = Some("PK".into());
        roster[1].attested_spawner = Some("PK".into());
        roster.push(pk);
        assert_eq!(who_sealed(&roster, "K", &|_| true).as_deref(), Some("P"));
        assert_eq!(who_sealed(&roster, "K", &|r: &SessionRecord| r.session_id != "P"), None);
    }

    #[test]
    fn gather_keeps_only_children_with_a_hook_record_and_a_recipient() {
        let roster = spawned_shape();
        let hook = |phase: &str| HookRecord {
            session_id: "K".into(),
            phase: phase.into(),
            updated_at: AT.into(),
            ..Default::default()
        };
        let kids = gather(&roster, &[hook("running")], &|_| true);
        assert_eq!(kids.len(), 1, "W and P are not hook children");
        assert_eq!((kids[0].phase.as_str(), kids[0].tag.as_str()), ("working", "[claude fond-aspen]"));
        assert_eq!(kids[0].recipient, "P");
        assert!(gather(&roster, &[], &|_| true).is_empty(), "no hook record, nothing to say");
        let mut orphan = roster.clone();
        orphan[1].attested_spawner = None;
        assert!(gather(&orphan, &[hook("working")], &|_| true).is_empty(), "no attested recipient, nothing to claim");
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
        // The first claim is the baseline: the cursor lands, nothing is owed.
        let kids = [child("working", AT)];
        assert!(claim(&kids, &mut next, at_ms()).is_empty());
        let kids = [child("awaiting", "2026-10-07T10:01:00Z")];
        let lines = claim(&kids, &mut next, at_ms() + 60_000);
        assert_eq!(lines.len(), 1);
        assert_eq!(
            (lines[0].parent.as_str(), lines[0].line.as_str()),
            ("spawner", "[claude fond-aspen] awaiting")
        );
        assert_eq!(next["kid"].seen.as_deref(), Some("9"), "the trace half is untouched");
        assert_eq!(next["kid"].hook, Some(cursor("awaiting", "2026-10-07T10:01:00Z", false)));
        assert!(claim(&kids, &mut next, at_ms() + 72_000).is_empty(), "at most once");
    }
}
