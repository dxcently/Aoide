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
//! Four lines, one per event:
//!
//! ```text
//! [claude <petname>] awaiting
//! [claude <petname>] silent 12 min · last: <the tool in flight, else "working">
//! [claude <petname>] settled
//! [claude <petname>] died mid-turn · last: <the tool in flight, else "working">
//! ```
//!
//! **Who hears it: the SPAWNER, and only over an attested edge.** A hook
//! child's `parentSessionId` is its OWN host wrap (the conducted wrap whose
//! harness it is), so the line must never go there: a headless host would
//! take it as its own next prompt and an interactive one would have it typed
//! into the user's composer. [`route_of`] resolves it in three steps, each a
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
//! **Died mid-turn** is the line the child's own hooks cannot write: a turn
//! that never reached `stopped`/`idle`. It is claimed when the child's last
//! open phase (`working`/`awaiting`) is still the one `hooks.json` holds and
//! its harness is gone — the first `hookAncestry` pid (the process the hook
//! door walked to; the record's own `pid` where it stamps none), probed with
//! the reaper's own `pid_is_alive` — or when the record has left the roster
//! with that phase still open. It is the hook source's twin of the trace
//! source's `died mid-turn` (an eidolon record dropped with its turn open), and
//! says that the turn did not end, not why. A `done` phase is a session ending
//! by its own hook and says nothing.
//!
//! A child the tick can no longer route — its record has left the roster, or
//! its harness is dead and a wrap on its route has stopped verifying — cannot be
//! asked for its recipient, so the cursor carries the route: the host wrap and
//! the spawner resolved while the child was live (both sealed then, the only
//! way a route is written), and the tag and tool in flight it was last seen
//! under. Such a child is said of over that route when its host wrap is gone
//! (absent, `done`, or its pid dead) or still verifies, and its spawner verifies
//! now. A live host that does not verify keeps the entry until it verifies again
//! or the turn is a day old.
//!
//! The cursor (`pingback.json`'s `hook` field) holds the canonical phase and
//! `updatedAt` last examined, two latches — `silent` (a silence line was sent
//! for this stamp) and `died` (the death was announced, or was already true
//! when first seen) — and the `route`. The latches are separate because a
//! silence before a crash must not hide the crash. A restamp re-arms both. The
//! first examination of a child is a baseline: it records what the child is and
//! says nothing, so a child first seen already dead while open is latched
//! `died` and never announces its own death.

use super::{clean, is_false, tracks, ChildClaim, CursorEntry, CursorFile};
use crate::graph::model::{canonical_state, HookRecord, SessionRecord};
use aoide_protocol::agents::agent_profile;
use aoide_storage::time::parse_iso_utc;
use serde::{Deserialize, Serialize};

/// How long a `working` phase may go without a hook event before the child is
/// called silent.
const SILENCE_MS: i64 = 12 * 60 * 1000;
/// Past this a `working` stamp is a dead record, not a quiet child: a crashed
/// harness leaves `working` behind for days, and "silent 4000 min" is noise.
/// The same cut ends a death's claim: a stamp this old is not news.
const STALE_MS: i64 = 24 * 60 * 60 * 1000;

/// The hook source's half of a cursor entry (`pingback.json`'s `hook` field):
/// the canonical phase and the raw `updatedAt` last examined, the two latches
/// for the open turn at that stamp, and the route a line about the child takes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct HookCursor {
    phase: String,
    at: String,
    #[serde(default, skip_serializing_if = "is_false")]
    silent: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    died: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    route: Option<Route>,
}

/// Where a line about one hook child goes, as last resolved while the child was
/// live: the host wrap whose seal carried the edge, the spawner it is said to,
/// the tag it is said under, and the tool in flight at that sighting. Kept so a
/// child whose record has left the roster can still be told of its death.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Route {
    host: String,
    to: String,
    tag: String,
    last: String,
}

/// One hook-reporting child, gathered: its line's tag, its route, and the facts
/// the decision reads.
pub(super) struct HookChild {
    id: String,
    tag: String,
    host: String,
    to: String,
    phase: String,
    at: String,
    at_secs: Option<i64>,
    last: String,
    pid_dead: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum HookEvent {
    Awaiting,
    Silent { mins: i64, last: String },
    Settled,
    Died { last: String },
}

/// What a child whose record has left the roster comes to.
enum Gone {
    Say(ChildClaim),
    Keep,
    Drop,
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

/// Who hears `kid`, and through which host: `(host, spawner)`, the spawner
/// being the conducted session that SPAWNED it, over an edge the kernel
/// attested (the module doc's three steps). Never the child, never its own
/// host wrap.
fn route_of(
    kid: &SessionRecord,
    roster: &[SessionRecord],
    sealed: &impl Fn(&SessionRecord) -> bool,
) -> Option<(String, String)> {
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
        .then(|| (host.session_id.clone(), target.session_id.clone()))
}

/// The process a hook child's liveness is read off: the harness its hook door
/// walked to (the first `hookAncestry` pid — the door itself is never stored),
/// or the record's own `pid` where it stamps none.
fn harness_pid(rec: &SessionRecord) -> Option<u32> {
    rec.hook_ancestry
        .first()
        .and_then(|pid| u32::try_from(*pid).ok())
        .filter(|pid| *pid > 0)
        .or(rec.pid)
}

fn pid_dead(rec: &SessionRecord) -> bool {
    harness_pid(rec).is_some_and(|pid| !aoide_storage::fs::pid_is_alive(pid))
}

/// A wrap a route still names, verified NOW: a conducted record whose seal
/// verifies, which re-derives its pid's start time and so proves that process
/// is alive and is the one the seal was minted over.
fn wrap_now(roster: &[SessionRecord], id: &str, sealed: &impl Fn(&SessionRecord) -> bool) -> bool {
    roster
        .iter()
        .find(|r| r.session_id == id)
        .is_some_and(|r| r.conductable == Some(true) && sealed(r))
}

/// A host wrap a route may pass WITHOUT a live seal, because nothing live is
/// left to verify: its record absent, `done`, or its pid gone. The route was
/// sealed on both ends when it was written.
fn host_gone(roster: &[SessionRecord], id: &str) -> bool {
    roster.iter().find(|r| r.session_id == id).map_or(true, |w| {
        canonical_state(&w.state) == "done" || w.pid.is_some_and(|pid| !aoide_storage::fs::pid_is_alive(pid))
    })
}

/// A phase the turn is still open in.
fn is_open(phase: &str) -> bool {
    matches!(phase, "working" | "awaiting")
}

/// The tracked children that have a hook record and a live route. A child
/// with either missing has nothing to say or nobody to say it to, and keeps
/// whatever cursor it had.
fn gather(roster: &[SessionRecord], hooks: &[HookRecord], sealed: &impl Fn(&SessionRecord) -> bool) -> Vec<HookChild> {
    roster
        .iter()
        .filter(|rec| tracked(rec))
        .filter_map(|rec| {
            let hook = hooks.iter().find(|h| h.session_id == rec.session_id)?;
            let (host, to) = route_of(rec, roster, sealed)?;
            let name = rec.petname.as_deref().filter(|p| !p.trim().is_empty()).unwrap_or(&rec.session_id);
            Some(HookChild {
                id: rec.session_id.clone(),
                tag: format!("[{} {}]", rec.agent, clean(name)),
                host,
                to,
                phase: canonical_state(&hook.phase).to_string(),
                at: hook.updated_at.clone(),
                at_secs: parse_iso_utc(&hook.updated_at),
                last: rec.activity.as_deref().filter(|a| !a.is_empty()).map_or_else(|| "working".to_string(), clean),
                pid_dead: pid_dead(rec),
            })
        })
        .collect()
}

/// The claim section's hook half — run ONLY inside the stage lock, over the
/// cursor the eidolon half just built: advance every gathered child's `hook`
/// cursor in `next` and return the lines now owed. Every other entry in `old`
/// that holds a hook half is judged by [`gone`]: said of, kept, or dropped, and
/// the first two of those leave its half in `next` only for a keep.
pub(super) fn claim(
    roster: &[SessionRecord],
    hooks: &[HookRecord],
    sealed: &impl Fn(&SessionRecord) -> bool,
    old: &CursorFile,
    next: &mut CursorFile,
    now_ms: i64,
) -> Vec<ChildClaim> {
    let mut lines = Vec::new();
    let children = gather(roster, hooks, sealed);
    for child in &children {
        let entry = next.entry(child.id.clone()).or_default();
        let (event, cursor) = decide(child, entry.hook.as_ref(), now_ms);
        entry.hook = Some(cursor);
        if let Some(event) = event {
            lines.push(ChildClaim { parent: child.to.clone(), line: render(&child.tag, &event) });
        }
    }
    for (id, prev) in old {
        let Some(hook) = prev.hook.as_ref() else { continue };
        if children.iter().any(|c| c.id == *id) {
            continue;
        }
        match gone(hook, id, roster, hooks, sealed, now_ms) {
            Gone::Say(claim) => {
                lines.push(claim);
                unhook(next, id);
            }
            Gone::Keep => next.entry(id.clone()).or_default().hook = Some(hook.clone()),
            Gone::Drop => unhook(next, id),
        }
    }
    lines
}

/// Clears a child's hook half from `next`, and the entry with it once nothing
/// else in it is left.
fn unhook(next: &mut CursorFile, id: &str) {
    let Some(entry) = next.get_mut(id) else { return };
    entry.hook = None;
    if *entry == CursorEntry::default() {
        next.remove(id);
    }
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
    let route = Some(Route {
        host: child.host.clone(),
        to: child.to.clone(),
        tag: child.tag.clone(),
        last: child.last.clone(),
    });
    let open = is_open(&child.phase);
    let Some(entry) = entry else {
        let baseline = HookCursor {
            phase: child.phase.clone(),
            at: child.at.clone(),
            silent: false,
            died: child.pid_dead && open,
            route,
        };
        return (None, baseline);
    };
    let fresh_stamp = entry.at != child.at || entry.phase != child.phase;
    let mut silent = !fresh_stamp && entry.silent;
    let mut died = !fresh_stamp && entry.died;
    let quiet_ms = child.at_secs.map(|at| now_ms - at * 1000);

    let dies = child.pid_dead
        && open
        && is_open(&entry.phase)
        && !died
        && quiet_ms.is_some_and(|q| q < STALE_MS);
    let event = if dies {
        died = true;
        Some(HookEvent::Died { last: child.last.clone() })
    } else {
        match child.phase.as_str() {
            "awaiting" if entry.phase != "awaiting" => Some(HookEvent::Awaiting),
            "stopped" | "idle" if is_open(&entry.phase) => Some(HookEvent::Settled),
            "working" if !silent && !died && quiet_ms.is_some_and(|q| (SILENCE_MS..STALE_MS).contains(&q)) => {
                silent = true;
                Some(HookEvent::Silent {
                    mins: quiet_ms.unwrap_or_default() / 60_000,
                    last: child.last.clone(),
                })
            }
            _ => None,
        }
    };
    (event, HookCursor { phase: child.phase.clone(), at: child.at.clone(), silent, died, route })
}

/// The fate of an entry whose child was not gathered this tick. A child whose
/// record is still there with its harness alive is the roster's business: kept
/// while it is tracked and dropped once it is not, so a live child that merely
/// lost its edge is never said of. Anything else is judged on the route cached
/// while the child was live and on what verifies now: its open turn must be
/// recent, unannounced, and still open in `hooks.json`; its host wrap must be
/// gone or still verify; its spawner must verify. Only then is it said of.
fn gone(
    prev: &HookCursor,
    id: &str,
    roster: &[SessionRecord],
    hooks: &[HookRecord],
    sealed: &impl Fn(&SessionRecord) -> bool,
    now_ms: i64,
) -> Gone {
    if let Some(rec) = roster.iter().find(|r| r.session_id == id).filter(|r| !pid_dead(r)) {
        return if tracked(rec) { Gone::Keep } else { Gone::Drop };
    }
    let recent = parse_iso_utc(&prev.at).is_some_and(|at| now_ms - at * 1000 < STALE_MS);
    let phase_now = hooks.iter().find(|h| h.session_id == id).map(|h| canonical_state(&h.phase));
    if prev.died || !is_open(&prev.phase) || !recent || phase_now.is_some_and(|p| !is_open(p)) {
        return Gone::Drop;
    }
    let Some(route) = prev.route.as_ref() else {
        return Gone::Drop;
    };
    let distinct = route.host != id && route.to != id && route.to != route.host;
    let host_ok = host_gone(roster, &route.host) || wrap_now(roster, &route.host, sealed);
    if distinct && host_ok && wrap_now(roster, &route.to, sealed) {
        Gone::Say(ChildClaim {
            parent: route.to.clone(),
            line: render(&route.tag, &HookEvent::Died { last: route.last.clone() }),
        })
    } else {
        Gone::Keep
    }
}

fn render(tag: &str, event: &HookEvent) -> String {
    match event {
        HookEvent::Awaiting => format!("{tag} awaiting"),
        HookEvent::Silent { mins, last } => format!("{tag} silent {mins} min · last: {last}"),
        HookEvent::Settled => format!("{tag} settled"),
        HookEvent::Died { last } => format!("{tag} died mid-turn · last: {last}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::testutil::session;

    const AT: &str = "2026-10-07T10:00:00Z";
    /// A pid no process can hold (past every kernel's `pid_max`): a dead harness.
    const DEAD: i32 = i32::MAX;

    fn at_ms() -> i64 {
        parse_iso_utc(AT).unwrap() * 1000
    }

    fn route() -> Route {
        Route {
            host: "wrap".into(),
            to: "spawner".into(),
            tag: "[claude fond-aspen]".into(),
            last: "working".into(),
        }
    }

    fn child(phase: &str, at: &str) -> HookChild {
        HookChild {
            id: "kid".into(),
            tag: "[claude fond-aspen]".into(),
            host: "wrap".into(),
            to: "spawner".into(),
            phase: phase.into(),
            at: at.into(),
            at_secs: parse_iso_utc(at),
            last: "working".into(),
            pid_dead: false,
        }
    }

    fn dead(phase: &str, at: &str) -> HookChild {
        HookChild { pid_dead: true, ..child(phase, at) }
    }

    fn cursor(phase: &str, at: &str, silent: bool) -> HookCursor {
        HookCursor { phase: phase.into(), at: at.into(), silent, died: false, route: Some(route()) }
    }

    fn line(event: Option<HookEvent>) -> Option<String> {
        event.map(|e| render("[claude fond-aspen]", &e))
    }

    fn hook_rec(id: &str, phase: &str, at: &str) -> HookRecord {
        HookRecord { session_id: id.into(), phase: phase.into(), updated_at: at.into(), ..Default::default() }
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
    fn a_child_first_seen_already_dead_while_open_is_latched_and_never_announced() {
        let (event, base) = decide(&dead("working", AT), None, at_ms());
        assert_eq!(event, None);
        assert_eq!(base, HookCursor { died: true, ..cursor("working", AT, false) });
        assert_eq!(decide(&dead("working", AT), Some(&base), at_ms() + 60_000).0, None);
        // Dead but stopped: nothing was open, so there is nothing to latch.
        let (_, stopped) = decide(&dead("stopped", AT), None, at_ms());
        assert!(!stopped.died);
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
        kid.last = "Bash".into();
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

    #[test]
    fn a_working_child_whose_harness_dies_is_died_mid_turn_once() {
        let (_, base) = decide(&child("working", AT), None, at_ms());
        let (event, next) = decide(&dead("working", AT), Some(&base), at_ms() + 60_000);
        assert_eq!(line(event).as_deref(), Some("[claude fond-aspen] died mid-turn · last: working"));
        assert!(next.died);
        // Once: nothing more, and no silence line for the same dead turn either.
        assert_eq!(decide(&dead("working", AT), Some(&next), at_ms() + 60 * 60_000).0, None);
    }

    #[test]
    fn the_died_line_names_the_tool_in_flight() {
        let mut kid = dead("working", AT);
        kid.last = "Bash".into();
        let (event, _) = decide(&kid, Some(&cursor("working", AT, false)), at_ms() + 1_000);
        assert_eq!(line(event).as_deref(), Some("[claude fond-aspen] died mid-turn · last: Bash"));
    }

    #[test]
    fn an_awaiting_child_whose_harness_dies_is_died_mid_turn() {
        let seen = cursor("awaiting", AT, false);
        let (event, next) = decide(&dead("awaiting", AT), Some(&seen), at_ms() + 1_000);
        assert_eq!(line(event).as_deref(), Some("[claude fond-aspen] died mid-turn · last: working"));
        assert!(next.died);
    }

    #[test]
    fn a_silence_before_the_death_does_not_hide_the_death() {
        let seen = cursor("working", AT, false);
        let (silenced, quiet) = decide(&child("working", AT), Some(&seen), at_ms() + SILENCE_MS);
        assert!(matches!(silenced, Some(HookEvent::Silent { .. })));
        let (event, _) = decide(&dead("working", AT), Some(&quiet), at_ms() + SILENCE_MS + 60_000);
        assert_eq!(line(event).as_deref(), Some("[claude fond-aspen] died mid-turn · last: working"));
    }

    #[test]
    fn a_clean_stop_is_settled_and_a_death_after_it_says_nothing() {
        let seen = cursor("working", AT, false);
        let (event, next) = decide(&dead("stopped", AT), Some(&seen), at_ms());
        assert_eq!(line(event).as_deref(), Some("[claude fond-aspen] settled"));
        assert_eq!(decide(&dead("stopped", AT), Some(&next), at_ms() + 60_000).0, None);
    }

    #[test]
    fn a_session_end_is_a_clean_end_not_a_death() {
        let seen = cursor("working", AT, false);
        assert_eq!(decide(&dead("done", AT), Some(&seen), at_ms()).0, None);
    }

    /// The real two-record shape a spawned claude leaves: a spawner `P` (a
    /// conducted wrap), the wrap `W` that hosts the child (its parent `P`, the
    /// edge the kernel attested at registration), and the hook-registered
    /// child `K` whose own `parentSessionId` is `W` and whose hook ancestry
    /// holds `W`'s pid. `W` and `K`'s harness are this process, so both are
    /// alive for the whole test; a test that needs them dead says so.
    fn spawned_shape() -> Vec<SessionRecord> {
        let me = std::process::id();
        let mut p = session("P", "/w", "working", AT, None);
        p.conductable = Some(true);
        p.pid = Some(50);
        let mut w = session("W", "/w", "working", AT, Some("P"));
        w.conductable = Some(true);
        w.pid = Some(me);
        w.attested_spawner = Some("P".into());
        let mut k = session("K", "/w", "working", AT, Some("W"));
        k.kind = Some("agent".into());
        k.petname = Some("fond-aspen".into());
        k.hook_ancestry = vec![me as i32, me as i32, 1];
        vec![p, w, k]
    }

    /// The child's harness is gone: its hook ancestry's first pid names nothing.
    fn harness_died(roster: &mut [SessionRecord]) {
        roster[2].hook_ancestry[0] = DEAD;
    }

    /// The host wrap exited with its harness: `done`, its pid gone.
    fn wrap_exited(roster: &mut [SessionRecord]) {
        roster[1].state = "done".into();
        roster[1].pid = Some(DEAD as u32);
    }

    fn who(roster: &[SessionRecord], id: &str) -> Option<String> {
        who_sealed(roster, id, &|_| true)
    }

    fn who_sealed(
        roster: &[SessionRecord],
        id: &str,
        sealed: &impl Fn(&SessionRecord) -> bool,
    ) -> Option<String> {
        route_of(roster.iter().find(|r| r.session_id == id).unwrap(), roster, sealed).map(|(_, to)| to)
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
    fn gather_keeps_only_children_with_a_hook_record_and_a_live_route() {
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
        assert_eq!((kids[0].host.as_str(), kids[0].to.as_str()), ("W", "P"));
        assert!(gather(&roster, &[], &|_| true).is_empty(), "no hook record, nothing to say");
        let mut orphan = roster.clone();
        orphan[1].attested_spawner = None;
        assert!(gather(&orphan, &[hook("working")], &|_| true).is_empty(), "no attested recipient, nothing to claim");
    }

    #[test]
    fn the_tool_in_flight_is_read_clean() {
        let mut roster = spawned_shape();
        roster[2].activity = Some("Bash\r".into());
        let kids = gather(&roster, &[hook_rec("K", "working", AT)], &|_| true);
        assert_eq!(kids[0].last, "Bash", "a control character never reaches a parent's composer");
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
        let roster = spawned_shape();
        let sealed = |_: &SessionRecord| true;
        let mut next = CursorFile::new();
        next.insert("K".into(), super::super::CursorEntry { seen: Some("9".into()), ..Default::default() });
        // The first claim is the baseline: the cursor lands, nothing is owed.
        let hooks = [hook_rec("K", "working", AT)];
        assert!(claim(&roster, &hooks, &sealed, &next.clone(), &mut next, at_ms()).is_empty());
        let hooks = [hook_rec("K", "awaiting", "2026-10-07T10:01:00Z")];
        let lines = claim(&roster, &hooks, &sealed, &next.clone(), &mut next, at_ms() + 60_000);
        assert_eq!(
            lines,
            vec![ChildClaim { parent: "P".into(), line: "[claude fond-aspen] awaiting".into() }]
        );
        assert_eq!(next["K"].seen.as_deref(), Some("9"), "the trace half is untouched");
        assert_eq!(next["K"].hook.as_ref().map(|h| h.phase.as_str()), Some("awaiting"));
        assert!(
            claim(&roster, &hooks, &sealed, &next.clone(), &mut next, at_ms() + 72_000).is_empty(),
            "at most once"
        );
    }

    #[test]
    fn a_child_whose_record_leaves_while_working_is_died_mid_turn_once_to_its_spawner() {
        let roster = spawned_shape();
        let hooks = [hook_rec("K", "working", AT)];
        let sealed = |_: &SessionRecord| true;
        let mut next = CursorFile::new();
        assert!(claim(&roster, &hooks, &sealed, &CursorFile::new(), &mut next, at_ms()).is_empty());

        // The record is reaped; its hook file still says working.
        let gone_roster: Vec<SessionRecord> = roster.iter().filter(|r| r.session_id != "K").cloned().collect();
        let old = next.clone();
        let mut after = CursorFile::new();
        let lines = claim(&gone_roster, &hooks, &sealed, &old, &mut after, at_ms() + 60_000);
        assert_eq!(
            lines,
            vec![ChildClaim { parent: "P".into(), line: "[claude fond-aspen] died mid-turn · last: working".into() }],
            "one line, no settled"
        );
        assert!(!after.contains_key("K"), "the entry leaves with its child");
        let mut again = CursorFile::new();
        assert!(claim(&gone_roster, &hooks, &sealed, &after, &mut again, at_ms() + 72_000).is_empty());
    }

    #[test]
    fn a_gone_child_whose_hook_file_says_it_ended_is_not_said_of() {
        let roster = spawned_shape();
        let sealed = |_: &SessionRecord| true;
        let mut next = CursorFile::new();
        claim(&roster, &[hook_rec("K", "working", AT)], &sealed, &CursorFile::new(), &mut next, at_ms());
        let gone_roster: Vec<SessionRecord> = roster.iter().filter(|r| r.session_id != "K").cloned().collect();
        let mut after = CursorFile::new();
        let lines = claim(
            &gone_roster,
            &[hook_rec("K", "done", AT)],
            &sealed,
            &next,
            &mut after,
            at_ms() + 60_000,
        );
        assert!(lines.is_empty(), "a session end is a clean end");
        assert!(!after.contains_key("K"));
    }

    #[test]
    fn a_gone_child_is_said_of_only_while_its_host_verifies_or_is_gone_and_its_spawner_verifies() {
        let roster = spawned_shape();
        let hooks = [hook_rec("K", "working", AT)];
        let mut next = CursorFile::new();
        claim(&roster, &hooks, &(|_: &SessionRecord| true), &CursorFile::new(), &mut next, at_ms());
        let gone_roster: Vec<SessionRecord> = roster.iter().filter(|r| r.session_id != "K").cloned().collect();

        // The live host no longer verifies: nothing is said, and the entry is
        // kept, so a later tick with the seal back can still say it.
        let unsealed_host = |r: &SessionRecord| r.session_id != "W";
        let mut kept = CursorFile::new();
        assert!(claim(&gone_roster, &hooks, &unsealed_host, &next, &mut kept, at_ms() + 60_000).is_empty());
        assert!(kept.contains_key("K"), "withheld, not forgotten");

        let mut said = CursorFile::new();
        let lines = claim(&gone_roster, &hooks, &(|_: &SessionRecord| true), &kept, &mut said, at_ms() + 72_000);
        assert_eq!(lines.len(), 1, "{lines:?}");
    }

    #[test]
    fn a_spawned_child_whose_wrap_exited_with_its_harness_is_died_mid_turn_once_to_its_spawner() {
        let roster = spawned_shape();
        let hooks = [hook_rec("K", "working", AT)];
        let mut old = CursorFile::new();
        claim(&roster, &hooks, &(|_: &SessionRecord| true), &CursorFile::new(), &mut old, at_ms());

        // The wrap and its claude both exited: only the spawner still verifies.
        let mut exited = roster.clone();
        wrap_exited(&mut exited);
        harness_died(&mut exited);
        let only_spawner = |r: &SessionRecord| r.session_id == "P";
        let mut after = old.clone();
        let lines = claim(&exited, &hooks, &only_spawner, &old, &mut after, at_ms() + 60_000);
        assert_eq!(
            lines,
            vec![ChildClaim { parent: "P".into(), line: "[claude fond-aspen] died mid-turn · last: working".into() }],
            "one line, to the sealed spawner"
        );
        assert!(!after.contains_key("K"), "the entry leaves once its death is said");
        let mut again = CursorFile::new();
        assert!(claim(&exited, &hooks, &only_spawner, &after, &mut again, at_ms() + 72_000).is_empty());
    }

    #[test]
    fn a_child_whose_live_wrap_no_longer_verifies_is_withheld_and_said_once_it_verifies_again() {
        let roster = spawned_shape();
        let hooks = [hook_rec("K", "working", AT)];
        let mut old = CursorFile::new();
        claim(&roster, &hooks, &(|_: &SessionRecord| true), &CursorFile::new(), &mut old, at_ms());

        // The harness dies while its wrap is still up, but the wrap's seal fails.
        let mut dead = roster.clone();
        harness_died(&mut dead);
        let unsealed_wrap = |r: &SessionRecord| r.session_id != "W";
        let mut next = old.clone();
        let lines = claim(&dead, &hooks, &unsealed_wrap, &old, &mut next, at_ms() + 60_000);
        assert!(lines.is_empty(), "a live host that does not verify: nothing said, {lines:?}");
        assert_eq!(next["K"].hook, old["K"].hook, "withheld, not forgotten");

        let mut later = next.clone();
        let lines = claim(&dead, &hooks, &(|_: &SessionRecord| true), &next, &mut later, at_ms() + 72_000);
        assert_eq!(
            lines,
            vec![ChildClaim { parent: "P".into(), line: "[claude fond-aspen] died mid-turn · last: working".into() }],
        );
    }

    #[test]
    fn a_child_whose_spawner_does_not_verify_is_kept_until_its_turn_is_a_day_old() {
        let roster = spawned_shape();
        let hooks = [hook_rec("K", "working", AT)];
        let mut old = CursorFile::new();
        claim(&roster, &hooks, &(|_: &SessionRecord| true), &CursorFile::new(), &mut old, at_ms());
        let mut dead = roster.clone();
        harness_died(&mut dead);

        let unsealed_spawner = |r: &SessionRecord| r.session_id != "P";
        let mut kept = CursorFile::new();
        assert!(claim(&dead, &hooks, &unsealed_spawner, &old, &mut kept, at_ms() + 60_000).is_empty());
        assert!(kept.contains_key("K"), "withheld, not forgotten");

        let mut aged = CursorFile::new();
        assert!(claim(&dead, &hooks, &unsealed_spawner, &kept, &mut aged, at_ms() + STALE_MS).is_empty());
        assert!(!aged.contains_key("K"), "a day old: a dead record, not a death to announce now");
    }

    #[test]
    fn a_clean_exit_in_the_spawn_shape_is_never_died() {
        let roster = spawned_shape();
        let hooks = [hook_rec("K", "working", AT)];
        let mut old = CursorFile::new();
        claim(&roster, &hooks, &(|_: &SessionRecord| true), &CursorFile::new(), &mut old, at_ms());

        // The turn ended on its own hook (stopped) or the session ended (done),
        // and then the wrap and the harness exited.
        let mut ended = roster.clone();
        wrap_exited(&mut ended);
        harness_died(&mut ended);
        ended[2].state = "done".into();
        let only_spawner = |r: &SessionRecord| r.session_id == "P";
        for phase in ["stopped", "done"] {
            let mut after = CursorFile::new();
            let lines = claim(&ended, &[hook_rec("K", phase, AT)], &only_spawner, &old, &mut after, at_ms() + 60_000);
            assert!(lines.is_empty(), "{phase}: {lines:?}");
            assert!(!after.contains_key("K"), "{phase}");
        }
    }

    #[test]
    fn a_route_is_cached_only_while_both_of_its_wraps_verify() {
        let roster = spawned_shape();
        let hooks = [hook_rec("K", "working", AT)];
        let mut next = CursorFile::new();
        claim(&roster, &hooks, &(|r: &SessionRecord| r.session_id != "W"), &CursorFile::new(), &mut next, at_ms());
        assert!(!next.contains_key("K"), "the host does not verify: no route to cache");
        let mut next = CursorFile::new();
        claim(&roster, &hooks, &(|r: &SessionRecord| r.session_id != "P"), &CursorFile::new(), &mut next, at_ms());
        assert!(!next.contains_key("K"), "the spawner does not verify: no route to cache");
    }

    #[test]
    fn a_gone_child_with_no_route_or_already_announced_is_dropped() {
        let gone_roster: Vec<SessionRecord> = Vec::new();
        let sealed = |_: &SessionRecord| true;
        let no_route = HookCursor { route: None, ..cursor("working", AT, false) };
        let announced = HookCursor { died: true, ..cursor("working", AT, false) };
        for prev in [no_route, announced] {
            let mut old = CursorFile::new();
            old.insert("K".into(), super::super::CursorEntry { hook: Some(prev), ..Default::default() });
            let mut next = CursorFile::new();
            assert!(claim(&gone_roster, &[], &sealed, &old, &mut next, at_ms() + 60_000).is_empty());
            assert!(next.is_empty());
        }
    }

    #[test]
    fn a_gone_child_whose_open_turn_is_a_day_old_is_dropped() {
        let sealed = |_: &SessionRecord| true;
        let mut old = CursorFile::new();
        old.insert(
            "K".into(),
            super::super::CursorEntry { hook: Some(cursor("working", AT, false)), ..Default::default() },
        );
        let mut next = CursorFile::new();
        let lines = claim(&[], &[], &sealed, &old, &mut next, at_ms() + STALE_MS);
        assert!(lines.is_empty());
        assert!(next.is_empty(), "a dead record, not a death to announce now");
    }

    #[test]
    fn a_live_child_that_merely_lost_its_edge_is_not_said_died() {
        // K is still on the roster, its harness alive, but its parent edge was
        // cleared: it is no longer tracked, and nothing says it died.
        let mut roster = spawned_shape();
        roster[2].parent_session_id = None;
        let sealed = |_: &SessionRecord| true;
        let mut old = CursorFile::new();
        old.insert(
            "K".into(),
            super::super::CursorEntry { hook: Some(cursor("working", AT, false)), ..Default::default() },
        );
        let mut next = CursorFile::new();
        let lines = claim(&roster, &[hook_rec("K", "working", AT)], &sealed, &old, &mut next, at_ms() + 60_000);
        assert!(lines.is_empty());
        assert!(!next.contains_key("K"));
    }
}
