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
//!    died, or a pid hand-edited into `sessions.json`, names a process the seal
//!    was never minted over. A `done` record is judged by its pid and seal like
//!    any other host's, never by its state alone;
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
//! **Died mid-turn** is the line the child's own hooks cannot write: a turn that
//! never reached `stopped`/`idle`. Two things say it. The first is the wrap's own
//! fact: when the wrap that hosted the harness saw its child exit while the phase
//! was still open, it wrote `phase: "done"` and a `death` (the tool in flight and
//! the exit status) onto the child's `hooks.json` record
//! (`session_store::record_hook_deaths`), before it ended its own record. A wrap
//! that was itself sent SIGTERM, SIGHUP or SIGINT forwards the signal and writes
//! the same fact, with that signal as its status. The tick claims the fact once,
//! with its status, and clears it, so a record without a `death` says nothing
//! more. The second is the tick's inference, for a wrap that died without writing
//! (SIGKILL): the child drops out of the gather, and `hook::gone` reads its last
//! open phase (`working`/`awaiting`) against its harness, which is gone (the
//! first `hookAncestry` pid, the process the hook door walked to, or the
//! record's own `pid` where it stamps none, probed with the reaper's own
//! `pid_is_alive`). The inferred line carries no status, since no one saw the
//! exit. A live wrap is the one that tells: while its host runs, sealed or not, a
//! dead harness under it is not a death yet, and the tick infers one only once
//! the host is gone. It is the hook source's twin of the trace
//! source's `died mid-turn` (an eidolon record dropped with its turn open), and
//! says that the turn did not end, not why. A `done` phase with no fact is a
//! session ending by its own hook and says nothing.
//!
//! A child the tick can no longer route cannot be asked for its recipient, so
//! the cursor carries the route: the host wrap and the spawner resolved while
//! the child was live (both sealed then, the only way a route is written), the
//! harness it was seen alive as (pid and start time), and the tag and tool in
//! flight. Its rules, the orphan case included, are stated once in
//! `CONTRACTS.md` under `state/stage/pingback.json`; `hook::gone` is their code.
//!
//! `pingback.json` is unsealed data: a same-uid writer can fabricate a route and
//! so a died line to any sealed, receptive session. Every value read from it is
//! cleaned again where it is rendered. The trust model is stated once, in
//! `CONTRACTS.md` under `state/stage/pingback.json`.
//!
//! The cursor (`pingback.json`'s `hook` field) holds the canonical phase and
//! `updatedAt` last examined, two latches (`silent`, a silence line was sent for
//! this stamp, and `died`, the open turn's death was said or the gone path said
//! it), and the `route`. The latches are separate because a silence before a
//! crash must not hide the crash. A restamp re-arms both. The first examination
//! of a child is a baseline: it records what the child is and says nothing, except
//! a recorded death under a day old, which is said. A recorded death is said
//! whatever phase the tick last saw, and no latch gates it: the fact on
//! `hooks.json` is the unclaimed state, and the claim clears it once it is said.

use super::{clean, died_mid_turn, harness_tag, is_false, tracks, ChildClaim, CursorEntry, CursorFile, EXIT_CODE_MAX};
use std::collections::HashSet;
use crate::graph::model::{canonical_state, harness_pid, HookDeath, HookRecord, SessionRecord};
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
/// The highest signal number a Linux process can die by (`SIGRTMAX`). A fact
/// naming anything else has its signal dropped, not printed.
const SIGNAL_MAX: i32 = 64;

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
    /// The harness at the sighting, when it was alive: a reaped record whose
    /// harness still runs under this start time is not a death.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    harness: Option<Harness>,
}

/// A harness process as one sighting read it: its pid and the start time
/// `/proc` gave it. A pid reused since then has another start time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Harness {
    pid: u32,
    start: u64,
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
    harness: Option<Harness>,
    /// The wrap's fact, present only on a `done` record that still carries one.
    death: Option<HookDeath>,
}

#[derive(Debug, PartialEq, Eq)]
enum HookEvent {
    Awaiting,
    Silent { mins: i64, last: String },
    Settled,
    Died { last: String, status: Option<String> },
}

/// What a child whose record has left the roster comes to.
enum Gone {
    Say { claim: ChildClaim, recorded: bool },
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
    let to = spawner_of(host, &kid.session_id, roster, sealed)?;
    Some((host.session_id.clone(), to))
}

/// Steps 2 and 3 of [`route_of`]: the spawner of a conducted `host` wrap,
/// over the edge `host`'s registration attested, resolved to the conducted
/// session that can be written to. Shared with the screen source
/// (`prompt.rs`), whose child IS the wrap and so has no hook child to climb
/// from. Never `kid_id` itself and never `host`.
pub(super) fn spawner_of(
    host: &SessionRecord,
    kid_id: &str,
    roster: &[SessionRecord],
    sealed: &impl Fn(&SessionRecord) -> bool,
) -> Option<String> {
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
    (target.session_id != kid_id && target.session_id != host.session_id)
        .then(|| target.session_id.clone())
}

fn pid_dead(rec: &SessionRecord) -> bool {
    harness_pid(rec).is_some_and(|pid| !aoide_storage::fs::pid_is_alive(pid))
}

/// The harness as this sighting finds it, when it is alive: its pid and the
/// start time `/proc` gives it now. The route keeps it, so a later pass can tell
/// the same process from a pid that has been reused.
fn live_harness(rec: &SessionRecord) -> Option<Harness> {
    let pid = harness_pid(rec)?;
    let start = aoide_storage::attest::pid_starttime(i32::try_from(pid).ok()?)?;
    Some(Harness { pid, start })
}

/// Is a route's harness still the process it was seen as: the same pid, still
/// running under the same start time.
fn harness_still_alive(h: &Harness) -> bool {
    i32::try_from(h.pid).ok().and_then(aoide_storage::attest::pid_starttime) == Some(h.start)
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

/// A host wrap that is gone: its record absent, or its pid dead. A `done` record
/// whose pid still runs is not gone, and is judged by its seal like any live
/// host; a wrap marks itself `done` just before it exits, so the real case has
/// its pid dead by the time its child is judged. A pid reused by another process
/// is alive but fails the seal, which re-derives the start time: kept, not said.
fn host_gone(roster: &[SessionRecord], id: &str) -> bool {
    roster
        .iter()
        .find(|r| r.session_id == id)
        .map_or(true, |w| w.pid.is_some_and(|pid| !aoide_storage::fs::pid_is_alive(pid)))
}

/// A phase the turn is still open in.
fn is_open(phase: &str) -> bool {
    matches!(phase, "working" | "awaiting")
}

/// The exit status a death fact names, as its line shows it: `exit <code>`,
/// `signal <n>` or `timeout`. The fact is read back from `hooks.json`, which any
/// same-uid writer may edit, so a value outside what a real process can end with
/// is dropped, never printed.
fn status_of(death: &HookDeath) -> Option<String> {
    match death.outcome.as_deref()? {
        "exit" => death.exit_code.filter(|c| (0..=EXIT_CODE_MAX).contains(c)).map(|c| format!("exit {c}")),
        "signal" => death.signal.filter(|s| (1..=SIGNAL_MAX).contains(s)).map(|s| format!("signal {s}")),
        "timeout" => Some("timeout".to_string()),
        _ => None,
    }
}

/// The death a turn is said to have ended in: the tool the wrap recorded in
/// flight (else the cached `last`), and the status the fact names, if any. With
/// no fact the line is the inferred one, which carries no status.
fn died_of(last: &str, death: Option<&HookDeath>) -> HookEvent {
    let tool = death
        .and_then(|d| d.last_tool.clone())
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| last.to_string());
    HookEvent::Died { last: tool, status: death.and_then(status_of) }
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
            Some(HookChild {
                id: rec.session_id.clone(),
                tag: harness_tag(&rec.agent, rec.petname.as_deref(), &rec.session_id),
                host,
                to,
                phase: canonical_state(&hook.phase).to_string(),
                at: hook.updated_at.clone(),
                at_secs: parse_iso_utc(&hook.updated_at),
                last: rec.activity.as_deref().filter(|a| !a.is_empty()).map_or_else(|| "working".to_string(), clean),
                harness: live_harness(rec),
                death: recorded_death(hook).cloned(),
            })
        })
        .collect()
}

/// The death the wrap recorded on a hook record: a `done` phase that still
/// carries its `death` fact. The claim clears the fact once it is said, so a
/// fact still present is unclaimed.
fn recorded_death(hook: &HookRecord) -> Option<&HookDeath> {
    if canonical_state(&hook.phase) == "done" {
        hook.death.as_ref()
    } else {
        None
    }
}

/// The claim section's hook half — run ONLY inside the stage lock, over the
/// cursor the eidolon half just built: advance every gathered child's `hook`
/// cursor in `next` and return the lines now owed, and the children whose
/// recorded death those lines say (the caller clears each one's fact with
/// [`clear_said`]). Every other entry in `old` that holds a hook half is judged
/// by [`gone`]: said of, kept, or dropped, and the first two of those leave its
/// half in `next` only for a keep.
pub(super) fn claim(
    roster: &[SessionRecord],
    hooks: &[HookRecord],
    sealed: &impl Fn(&SessionRecord) -> bool,
    old: &CursorFile,
    next: &mut CursorFile,
    now_ms: i64,
) -> (Vec<ChildClaim>, Vec<String>) {
    let mut lines = Vec::new();
    let mut said = Vec::new();
    let children = gather(roster, hooks, sealed);
    for child in &children {
        let entry = next.entry(child.id.clone()).or_default();
        let (event, cursor) = decide(child, entry.hook.as_ref(), now_ms);
        entry.hook = Some(cursor);
        if let Some(event) = event {
            if matches!(event, HookEvent::Died { .. }) {
                said.push(child.id.clone());
            }
            lines.push(ChildClaim { parent: child.to.clone(), line: render(&child.tag, &event) });
        }
    }
    for (id, prev) in old {
        let Some(hook) = prev.hook.as_ref() else { continue };
        if children.iter().any(|c| c.id == *id) {
            continue;
        }
        match gone(hook, id, roster, hooks, sealed, now_ms) {
            Gone::Say { claim, recorded } => {
                if recorded {
                    said.push(id.clone());
                }
                lines.push(claim);
                next.entry(id.clone()).or_default().hook = Some(HookCursor { died: true, ..hook.clone() });
            }
            Gone::Keep => next.entry(id.clone()).or_default().hook = Some(hook.clone()),
            Gone::Drop => unhook(next, id),
        }
    }
    (lines, said)
}

/// Clears the death fact of each child the claim said, so its record reads as an
/// ordinary `done` one. The record outlives `pingback.json`: a cursor deleted
/// after its line went out finds no fact left to say again.
pub(super) fn clear_said(hooks: &mut [HookRecord], said: &[String]) {
    for hook in hooks.iter_mut().filter(|h| said.contains(&h.session_id)) {
        hook.death = None;
    }
}

/// The deaths the reaper and every prune must spare until the claim has said
/// them: a recorded death under a day old that the tick can still say. It can
/// say one it gathers now (a tracked child whose route resolves), or one it says
/// over a route cached while the child was live. A record with neither has no
/// line to wait for, so nothing holds it.
pub(crate) fn unclaimed_deaths(roster: &[SessionRecord], hooks: &[HookRecord], now_ms: i64) -> HashSet<String> {
    unclaimed_in(roster, hooks, &super::read_cursor(), now_ms)
}

fn unclaimed_in(roster: &[SessionRecord], hooks: &[HookRecord], cursor: &CursorFile, now_ms: i64) -> HashSet<String> {
    hooks
        .iter()
        .filter(|h| recorded_death(h).is_some())
        .filter(|h| parse_iso_utc(&h.updated_at).is_some_and(|at| now_ms - at * 1000 < STALE_MS))
        .filter(|h| {
            let cached = cursor
                .get(&h.session_id)
                .and_then(|e| e.hook.as_ref())
                .is_some_and(|c| c.route.is_some());
            let gathered = roster.iter().find(|r| r.session_id == h.session_id).is_some_and(|rec| {
                tracked(rec) && route_of(rec, roster, &|_: &SessionRecord| true).is_some()
            });
            cached || gathered
        })
        .map(|h| h.session_id.clone())
        .collect()
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
/// A recorded death is the death itself, whatever phase the tick last saw: it is
/// said while its stamp is under a day old, and the claim clears it. No latch
/// gates it, so a cursor lost or latched before the claim's write cannot hide it.
/// The first examination of a child is a BASELINE: it records what the child is
/// and says nothing else, so a restart never announces everything that was
/// already true. `settled` needs the cursor to have SEEN the child
/// `working`/`awaiting`, and a turn that starts and ends between two ticks is not
/// heard. A dead harness under a sealed host is not a death here: the host
/// records it, or it dies and [`gone`] judges it.
fn decide(child: &HookChild, entry: Option<&HookCursor>, now_ms: i64) -> (Option<HookEvent>, HookCursor) {
    let route = Some(Route {
        host: child.host.clone(),
        to: child.to.clone(),
        tag: child.tag.clone(),
        last: child.last.clone(),
        harness: child.harness.clone(),
    });
    let quiet_ms = child.at_secs.map(|at| now_ms - at * 1000);
    let recorded = child.death.is_some() && quiet_ms.is_some_and(|q| q < STALE_MS);
    let Some(entry) = entry else {
        let baseline = HookCursor {
            phase: child.phase.clone(),
            at: child.at.clone(),
            silent: false,
            died: recorded,
            route,
        };
        let event = recorded.then(|| died_of(&child.last, child.death.as_ref()));
        return (event, baseline);
    };
    let fresh_stamp = entry.at != child.at || entry.phase != child.phase;
    let mut silent = !fresh_stamp && entry.silent;
    let mut died = !fresh_stamp && entry.died;
    let event = if recorded {
        died = true;
        Some(died_of(&child.last, child.death.as_ref()))
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
/// record is still on the roster with its harness alive is the roster's
/// business: kept while it is tracked, dropped once it is not, so a live child
/// that merely lost its edge is never said of. Anything else is judged on the
/// route cached while the child was live, in the order of the body: no hook
/// record, or a stamp a day old, drops the entry. A recorded death is then said
/// over that route, whatever phase the entry last saw. Without a recorded death
/// the entry must still be an open turn, not latched, with its record still open:
/// a route whose harness still runs under the start time it was seen with is the
/// orphan, and is kept, and so is a route whose host wrap is not gone, since a
/// live host records the death itself. Either way the route must be distinct, its
/// spawner must verify (checked here, inside the stage lock), and the host wrap
/// must be gone or still verify. Only then is the child said of; any other case is
/// kept, until a drop rule applies.
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
    let Some(hook) = hooks.iter().find(|h| h.session_id == id) else { return Gone::Drop };
    let recent = parse_iso_utc(&hook.updated_at).is_some_and(|at| now_ms - at * 1000 < STALE_MS);
    let fact = recorded_death(hook);
    let open = is_open(canonical_state(&hook.phase));
    if !recent || (fact.is_none() && (prev.died || !is_open(&prev.phase) || !open)) {
        return Gone::Drop;
    }
    let Some(route) = prev.route.as_ref() else {
        return Gone::Drop;
    };
    if fact.is_none() && (route.harness.as_ref().is_some_and(harness_still_alive) || !host_gone(roster, &route.host)) {
        return Gone::Keep;
    }
    let distinct = route.host != id && route.to != id && route.to != route.host;
    let host_ok = host_gone(roster, &route.host) || wrap_now(roster, &route.host, sealed);
    if !(distinct && host_ok && wrap_now(roster, &route.to, sealed)) {
        return Gone::Keep;
    }
    Gone::Say {
        claim: ChildClaim {
            parent: route.to.clone(),
            line: render(&route.tag, &died_of(&route.last, fact)),
        },
        recorded: fact.is_some(),
    }
}

/// One event as its line. Every value is cleaned again here, not only where the
/// cursor wrote it: a route is read back from `pingback.json`, which any same-uid
/// writer can edit.
fn render(tag: &str, event: &HookEvent) -> String {
    let tag = clean(tag);
    match event {
        HookEvent::Awaiting => format!("{tag} awaiting"),
        HookEvent::Silent { mins, last } => format!("{tag} silent {mins} min · last: {}", clean(last)),
        HookEvent::Settled => format!("{tag} settled"),
        HookEvent::Died { last, status } => {
            let status = status.as_deref().map_or(String::new(), |s| format!(" ({s})"));
            died_mid_turn(&tag, &format!("{status} · last: {}", clean(last)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::model::{hooks_path, load_stage, sessions_path, write_stage, HooksFile, SessionsFile, STAGE_GRAPH_VERSION};
    use crate::graph::testutil::{built_aoide_bin, session, unique_stage, EnvVars};

    const AT: &str = "2026-10-07T10:00:00Z";
    /// A pid no process can hold (past every kernel's `pid_max`): a dead harness.
    const DEAD: i32 = i32::MAX;

    fn at_ms() -> i64 {
        parse_iso_utc(AT).unwrap() * 1000
    }

    /// The lines a claim owes. Tests that need the said set call `super::claim`.
    fn claim(
        roster: &[SessionRecord],
        hooks: &[HookRecord],
        sealed: &impl Fn(&SessionRecord) -> bool,
        old: &CursorFile,
        next: &mut CursorFile,
        now_ms: i64,
    ) -> Vec<ChildClaim> {
        super::claim(roster, hooks, sealed, old, next, now_ms).0
    }

    fn route() -> Route {
        Route {
            host: "wrap".into(),
            to: "spawner".into(),
            tag: "[claude fond-aspen]".into(),
            last: "working".into(),
            harness: None,
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
            harness: None,
            death: None,
        }
    }

    /// The child as the wrap leaves it: `done`, with the fact it recorded.
    fn died(at: &str, fact: HookDeath) -> HookChild {
        HookChild { death: Some(fact), ..child("done", at) }
    }

    /// Whether the cursor holds a claimed death for `id` (the latch, kept one pass).
    fn latched(cursor: &CursorFile, id: &str) -> bool {
        cursor.get(id).and_then(|e| e.hook.as_ref()).is_some_and(|h| h.died)
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
    fn the_baseline_never_latches_a_death_the_host_has_not_recorded() {
        let (event, base) = decide(&child("working", AT), None, at_ms());
        assert_eq!(event, None);
        assert_eq!(base, cursor("working", AT, false), "no latch: nothing was recorded");
        assert_eq!(decide(&child("working", AT), Some(&base), at_ms() + 60_000).0, None);
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
    fn a_recorded_death_over_an_open_turn_is_died_mid_turn_once() {
        let (_, base) = decide(&child("working", AT), None, at_ms());
        let mut kid = died("2026-10-07T10:01:00Z", died_fact("signal"));
        let (event, next) = decide(&kid, Some(&base), at_ms() + 60_000);
        assert_eq!(line(event).as_deref(), Some("[claude fond-aspen] died mid-turn · last: Bash"));
        assert!(next.died);
        // The claim clears the fact; the record then reads as an ordinary `done` one.
        kid.death = None;
        assert_eq!(decide(&kid, Some(&next), at_ms() + 60 * 60_000).0, None, "once");
    }

    #[test]
    fn the_died_line_names_the_tool_in_flight() {
        let mut kid = died(AT, HookDeath { outcome: Some("exit".into()), exit_code: Some(2), ..Default::default() });
        kid.last = "Bash".into();
        let (event, _) = decide(&kid, Some(&cursor("working", AT, false)), at_ms() + 1_000);
        assert_eq!(line(event).as_deref(), Some("[claude fond-aspen] died mid-turn (exit 2) · last: Bash"));
    }

    #[test]
    fn a_death_over_an_awaiting_turn_is_died_mid_turn_too() {
        let seen = cursor("awaiting", AT, false);
        let (event, next) = decide(&died(AT, died_fact("signal")), Some(&seen), at_ms() + 1_000);
        assert_eq!(line(event).as_deref(), Some("[claude fond-aspen] died mid-turn · last: Bash"));
        assert!(next.died);
    }

    #[test]
    fn a_silence_before_the_death_does_not_hide_the_death() {
        let seen = cursor("working", AT, false);
        let (silenced, quiet) = decide(&child("working", AT), Some(&seen), at_ms() + SILENCE_MS);
        assert!(matches!(silenced, Some(HookEvent::Silent { .. })));
        let (event, _) = decide(&died("2026-10-07T10:02:00Z", died_fact("signal")), Some(&quiet), at_ms() + SILENCE_MS + 60_000);
        assert_eq!(line(event).as_deref(), Some("[claude fond-aspen] died mid-turn · last: Bash"));
    }

    #[test]
    fn a_clean_stop_is_settled_and_a_stop_after_it_says_nothing() {
        let seen = cursor("working", AT, false);
        let (event, next) = decide(&child("stopped", AT), Some(&seen), at_ms());
        assert_eq!(line(event).as_deref(), Some("[claude fond-aspen] settled"));
        assert_eq!(decide(&child("stopped", AT), Some(&next), at_ms() + 60_000).0, None);
    }

    #[test]
    fn a_session_end_is_a_clean_end_not_a_death() {
        let seen = cursor("working", AT, false);
        assert_eq!(decide(&child("done", AT), Some(&seen), at_ms()).0, None);
    }

    /// A recorded death is said whatever phase the tick last saw: a turn that
    /// ended (`stopped`, `idle`, or a session `done`) between two ticks, and a
    /// cursor that had moved on, never hide the wrap's fact.
    #[test]
    fn a_recorded_death_is_said_whatever_phase_the_tick_last_saw() {
        let kid = died("2026-10-07T10:01:00Z", died_fact("signal"));
        for seen in ["stopped", "idle", "done", "working", "awaiting"] {
            let entry = cursor(seen, "2026-10-07T09:59:00Z", false);
            assert_eq!(
                line(decide(&kid, Some(&entry), at_ms() + 60_000).0).as_deref(),
                Some("[claude fond-aspen] died mid-turn · last: Bash"),
                "last seen {seen}"
            );
        }
    }

    /// A death the claim said is cleared from its record, so a cursor lost after
    /// the line went out finds no fact to say again.
    #[test]
    fn a_said_death_is_not_said_again_when_the_cursor_is_lost() {
        let roster = spawned_shape();
        let sealed = |_: &SessionRecord| true;
        let recorded = [HookRecord { death: Some(died_fact("signal")), ..hook_rec("K", "done", "2026-10-07T10:01:00Z") }];
        let mut next = CursorFile::new();
        let (lines, said) = super::claim(&roster, &recorded, &sealed, &CursorFile::new(), &mut next, at_ms() + 60_000);
        assert_eq!(lines.len(), 1, "{lines:?}");
        let mut hooks = recorded.to_vec();
        clear_said(&mut hooks, &said);
        // pingback.json deleted: an empty cursor over the record as the claim left it.
        let mut again = CursorFile::new();
        let (lines, _) = super::claim(&roster, &hooks, &sealed, &CursorFile::new(), &mut again, at_ms() + 72_000);
        assert!(lines.is_empty(), "{lines:?}");
    }

    /// The probe's order, with the host alive throughout: the tick first sees the
    /// harness dead under a live sealed host, which is not a death yet. The host's
    /// own record then says it, once; the claim clears it, and nothing follows.
    #[test]
    fn a_dead_harness_under_a_live_host_is_said_once_when_the_host_records_it() {
        let mut roster = spawned_shape();
        harness_died(&mut roster);
        let sealed = |_: &SessionRecord| true;
        let working = [hook_rec("K", "working", AT)];
        let mut base = CursorFile::new();
        assert!(claim(&roster, &working, &sealed, &CursorFile::new(), &mut base, at_ms()).is_empty());
        let mut seen = CursorFile::new();
        let (lines, _) = super::claim(&roster, &working, &sealed, &base, &mut seen, at_ms() + 60_000);
        assert!(lines.is_empty(), "the live host has not recorded it yet: {lines:?}");

        let fact = HookDeath { last_tool: Some("Edit".into()), outcome: Some("exit".into()), exit_code: Some(1), ..Default::default() };
        let recorded = [HookRecord { death: Some(fact), ..hook_rec("K", "done", "2026-10-07T10:01:00Z") }];
        let mut said = CursorFile::new();
        let (lines, ids) = super::claim(&roster, &recorded, &sealed, &seen, &mut said, at_ms() + 120_000);
        assert_eq!(
            lines,
            vec![ChildClaim { parent: "P".into(), line: "[claude fond-aspen] died mid-turn (exit 1) · last: Edit".into() }]
        );
        assert_eq!(ids, vec!["K".to_string()]);

        let cleared = [hook_rec("K", "done", "2026-10-07T10:01:00Z")];
        let mut after = CursorFile::new();
        assert!(claim(&roster, &cleared, &sealed, &said, &mut after, at_ms() + 180_000).is_empty());
    }

    /// Which recorded deaths the reaper and every prune must hold: only those the
    /// tick can still say, over a route it gathers now or one cached while the
    /// child was live. A record with neither, or one a day old, is not held.
    #[test]
    fn a_death_is_held_only_while_the_tick_can_still_say_it() {
        let roster = spawned_shape();
        let held = [HookRecord { death: Some(died_fact("signal")), ..hook_rec("K", "done", AT) }];
        let none = CursorFile::new();
        assert_eq!(unclaimed_in(&roster, &held, &none, at_ms()), HashSet::from(["K".to_string()]), "gathered: held");

        let mut untracked = roster.clone();
        untracked[2].parent_session_id = None;
        assert!(unclaimed_in(&untracked, &held, &none, at_ms()).is_empty(), "no route will ever gather it");

        let gone: Vec<SessionRecord> = roster.iter().filter(|r| r.session_id != "K").cloned().collect();
        assert!(unclaimed_in(&gone, &held, &none, at_ms()).is_empty(), "gone with no route cached");
        let mut cached = CursorFile::new();
        cached.insert("K".into(), super::super::CursorEntry { hook: Some(cursor("working", AT, false)), ..Default::default() });
        assert_eq!(unclaimed_in(&gone, &held, &cached, at_ms()), HashSet::from(["K".to_string()]), "gone, a route cached");

        assert!(unclaimed_in(&roster, &held, &none, at_ms() + STALE_MS).is_empty(), "a day old");
        let plain = [hook_rec("K", "done", AT)];
        assert!(unclaimed_in(&roster, &plain, &none, at_ms()).is_empty(), "no fact, nothing to hold");
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

    /// A real process standing in for a child's harness: the sighting reads its
    /// pid as the harness, and [`die`] makes it genuinely dead before a gone pass.
    fn sleeper(roster: &mut [SessionRecord]) -> std::process::Child {
        let child = std::process::Command::new("sleep").arg("60").spawn().unwrap();
        roster[2].hook_ancestry = vec![child.id() as i32, std::process::id() as i32, 1];
        child
    }

    fn die(mut child: std::process::Child) {
        child.kill().unwrap();
        child.wait().unwrap();
    }

    #[test]
    fn a_child_whose_record_leaves_while_working_is_died_mid_turn_once_to_its_spawner() {
        let mut roster = spawned_shape();
        let harness = sleeper(&mut roster);
        let hooks = [hook_rec("K", "working", AT)];
        let sealed = |_: &SessionRecord| true;
        let mut next = CursorFile::new();
        assert!(claim(&roster, &hooks, &sealed, &CursorFile::new(), &mut next, at_ms()).is_empty());

        // The harness dies, then its record leaves the roster; its hook file still
        // says working. The host still runs, and a live host records the death
        // itself: the tick infers nothing and keeps the entry.
        die(harness);
        let mut gone_roster: Vec<SessionRecord> = roster.iter().filter(|r| r.session_id != "K").cloned().collect();
        let mut kept = CursorFile::new();
        assert!(claim(&gone_roster, &hooks, &sealed, &next, &mut kept, at_ms() + 30_000).is_empty());
        assert_eq!(kept["K"].hook, next["K"].hook, "withheld while the host runs");

        // The host ends without having found the child to record: now it is inferred.
        wrap_exited(&mut gone_roster);
        let old = kept.clone();
        let mut after = CursorFile::new();
        let lines = claim(&gone_roster, &hooks, &sealed, &old, &mut after, at_ms() + 60_000);
        assert_eq!(
            lines,
            vec![ChildClaim { parent: "P".into(), line: "[claude fond-aspen] died mid-turn · last: working".into() }],
            "one line, no settled"
        );
        assert!(latched(&after, "K"), "the claim stays latched for one pass");
        let mut again = CursorFile::new();
        assert!(claim(&gone_roster, &hooks, &sealed, &after, &mut again, at_ms() + 72_000).is_empty());
    }

    /// The orphan: its wrap died without writing (SIGKILL), its record left the
    /// roster, and the harness it was seen alive as still runs under that start
    /// time. Its entry is kept while the harness lives, said of once if it dies
    /// with the turn open, and dropped silently otherwise.
    #[test]
    fn an_orphan_whose_harness_still_runs_is_kept_and_said_of_once_when_it_dies_open() {
        let mut roster = spawned_shape();
        let harness = sleeper(&mut roster);
        let hooks = [hook_rec("K", "working", AT)];
        let sealed = |_: &SessionRecord| true;
        let mut next = CursorFile::new();
        claim(&roster, &hooks, &sealed, &CursorFile::new(), &mut next, at_ms());
        let mut gone_roster: Vec<SessionRecord> = roster.iter().filter(|r| r.session_id != "K").cloned().collect();
        wrap_exited(&mut gone_roster);

        let mut kept = CursorFile::new();
        assert!(claim(&gone_roster, &hooks, &sealed, &next, &mut kept, at_ms() + 60_000).is_empty());
        assert!(kept.contains_key("K"), "a reaped record whose harness runs keeps its entry");

        die(harness);
        let mut after = CursorFile::new();
        let lines = claim(&gone_roster, &hooks, &sealed, &kept, &mut after, at_ms() + 120_000);
        assert_eq!(
            lines,
            vec![ChildClaim { parent: "P".into(), line: "[claude fond-aspen] died mid-turn · last: working".into() }],
            "said of once, when the harness dies with the turn open"
        );
        assert!(latched(&after, "K"), "the claim stays latched for one pass");
        let mut again = CursorFile::new();
        assert!(claim(&gone_roster, &hooks, &sealed, &after, &mut again, at_ms() + 180_000).is_empty());
    }

    #[test]
    fn an_orphan_whose_turn_closes_while_its_harness_runs_is_dropped_silently() {
        let mut roster = spawned_shape();
        let harness = sleeper(&mut roster);
        let hooks = [hook_rec("K", "working", AT)];
        let sealed = |_: &SessionRecord| true;
        let mut next = CursorFile::new();
        claim(&roster, &hooks, &sealed, &CursorFile::new(), &mut next, at_ms());
        let gone_roster: Vec<SessionRecord> = roster.iter().filter(|r| r.session_id != "K").cloned().collect();
        let mut kept = CursorFile::new();
        claim(&gone_roster, &hooks, &sealed, &next, &mut kept, at_ms() + 60_000);
        assert!(kept.contains_key("K"));

        let stopped = [hook_rec("K", "stopped", "2026-10-07T10:01:00Z")];
        let mut after = CursorFile::new();
        let lines = claim(&gone_roster, &stopped, &sealed, &kept, &mut after, at_ms() + 120_000);
        assert!(lines.is_empty(), "a closed turn is not a death: {lines:?}");
        assert!(!after.contains_key("K"), "dropped");
        die(harness);
    }

    #[test]
    fn an_orphan_is_dropped_once_its_turn_is_a_day_old_even_if_its_harness_later_dies() {
        let mut roster = spawned_shape();
        let harness = sleeper(&mut roster);
        let hooks = [hook_rec("K", "working", AT)];
        let sealed = |_: &SessionRecord| true;
        let mut next = CursorFile::new();
        claim(&roster, &hooks, &sealed, &CursorFile::new(), &mut next, at_ms());
        let gone_roster: Vec<SessionRecord> = roster.iter().filter(|r| r.session_id != "K").cloned().collect();
        let mut kept = CursorFile::new();
        claim(&gone_roster, &hooks, &sealed, &next, &mut kept, at_ms() + 60_000);

        let mut aged = CursorFile::new();
        assert!(claim(&gone_roster, &hooks, &sealed, &kept, &mut aged, at_ms() + STALE_MS).is_empty());
        assert!(!aged.contains_key("K"), "a day old: dropped");
        die(harness);
        let mut later = CursorFile::new();
        assert!(claim(&gone_roster, &hooks, &sealed, &aged, &mut later, at_ms() + STALE_MS + 60_000).is_empty());
    }

    #[test]
    fn a_harness_pid_reused_since_the_sighting_is_a_dead_harness() {
        let mut gone_roster: Vec<SessionRecord> = spawned_shape().into_iter().filter(|r| r.session_id != "K").collect();
        wrap_exited(&mut gone_roster);
        let sealed = |_: &SessionRecord| true;
        let me = std::process::id();
        let start = aoide_storage::attest::pid_starttime(me as i32).unwrap();
        for (seen, said) in [(start, 0), (start + 1, 1)] {
            let mut prev = cursor("working", AT, false);
            prev.route = Some(Route {
                host: "W".into(),
                to: "P".into(),
                harness: Some(Harness { pid: me, start: seen }),
                ..route()
            });
            let mut old = CursorFile::new();
            old.insert("K".into(), super::super::CursorEntry { hook: Some(prev), ..Default::default() });
            let mut next = CursorFile::new();
            let hooks = [hook_rec("K", "working", AT)];
            let lines = claim(&gone_roster, &hooks, &sealed, &old, &mut next, at_ms() + 60_000);
            assert_eq!(lines.len(), said, "the same start time is alive; another one is a reused pid");
        }
    }

    /// A recorded death over a cached route whose host still runs but reads
    /// `done`: the host is judged by its seal, never by its state.
    #[test]
    fn a_done_host_whose_pid_still_runs_is_judged_by_its_seal_not_by_its_state() {
        let mut roster = spawned_shape();
        let harness = sleeper(&mut roster);
        let sealed = |_: &SessionRecord| true;
        let mut next = CursorFile::new();
        claim(&roster, &[hook_rec("K", "working", AT)], &sealed, &CursorFile::new(), &mut next, at_ms());
        die(harness);
        // The host still runs (its pid is this process) but says `done`.
        let mut gone_roster: Vec<SessionRecord> = roster.iter().filter(|r| r.session_id != "K").cloned().collect();
        gone_roster[1].state = "done".into();
        let recorded = [HookRecord { death: Some(died_fact("exit")), ..hook_rec("K", "done", "2026-10-07T10:01:00Z") }];

        let mut said = CursorFile::new();
        let lines = claim(&gone_roster, &recorded, &sealed, &next, &mut said, at_ms() + 60_000);
        assert_eq!(lines.len(), 1, "a done host that verifies is said of: {lines:?}");

        let unsealed_host = |r: &SessionRecord| r.session_id != "W";
        let mut kept = CursorFile::new();
        assert!(claim(&gone_roster, &recorded, &unsealed_host, &next, &mut kept, at_ms() + 60_000).is_empty());
        assert!(kept.contains_key("K"), "a done host that does not verify is kept, not said");
    }

    #[test]
    fn a_forged_route_is_cleaned_when_it_is_rendered() {
        // A cursor entry a same-uid writer fabricated: no record, a host that
        // names nothing, and controls in its tag and tool.
        let gone_roster: Vec<SessionRecord> = spawned_shape().into_iter().filter(|r| r.session_id != "K").collect();
        let sealed = |_: &SessionRecord| true;
        let mut prev = cursor("working", AT, false);
        prev.route = Some(Route {
            host: "nowhere".into(),
            to: "P".into(),
            tag: "[claude fond-aspen]\r\nEVIL".into(),
            last: "Bash\n\u{1b}[2J".into(),
            harness: None,
        });
        let mut old = CursorFile::new();
        old.insert("K".into(), super::super::CursorEntry { hook: Some(prev), ..Default::default() });
        let mut next = CursorFile::new();
        let lines = claim(&gone_roster, &[hook_rec("K", "working", AT)], &sealed, &old, &mut next, at_ms() + 60_000);
        assert_eq!(lines.len(), 1, "{lines:?}");
        let line = &lines[0].line;
        assert!(line.starts_with("[claude fond-aspen]") && line.contains(" died mid-turn · last: "), "{line:?}");
        assert!(!line.chars().any(char::is_control), "no CR, LF or control character: {line:?}");
    }

    #[test]
    fn a_spawned_child_whose_wrap_exited_with_its_harness_is_died_mid_turn_once_to_its_spawner() {
        let mut roster = spawned_shape();
        let harness = sleeper(&mut roster);
        let hooks = [hook_rec("K", "working", AT)];
        let mut old = CursorFile::new();
        claim(&roster, &hooks, &(|_: &SessionRecord| true), &CursorFile::new(), &mut old, at_ms());
        die(harness);

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
        assert!(latched(&after, "K"), "the claim stays latched for one pass");
        let mut again = CursorFile::new();
        assert!(claim(&exited, &hooks, &only_spawner, &after, &mut again, at_ms() + 72_000).is_empty());
    }

    #[test]
    fn a_child_whose_live_wrap_no_longer_verifies_is_withheld_and_said_once_its_wrap_is_gone() {
        let mut roster = spawned_shape();
        let harness = sleeper(&mut roster);
        let hooks = [hook_rec("K", "working", AT)];
        let mut old = CursorFile::new();
        claim(&roster, &hooks, &(|_: &SessionRecord| true), &CursorFile::new(), &mut old, at_ms());
        die(harness);

        // The harness is gone while its wrap is still up, but the wrap's seal fails.
        let mut dead = roster.clone();
        harness_died(&mut dead);
        let unsealed_wrap = |r: &SessionRecord| r.session_id != "W";
        let mut next = old.clone();
        let lines = claim(&dead, &hooks, &unsealed_wrap, &old, &mut next, at_ms() + 60_000);
        assert!(lines.is_empty(), "a live host that does not verify: nothing said, {lines:?}");
        assert_eq!(next["K"].hook, old["K"].hook, "withheld, not forgotten");

        // The seal verifies again while the wrap is up: the live host is the one
        // that tells, so the tick says nothing for a child it still routes.
        let mut later = next.clone();
        let lines = claim(&dead, &hooks, &(|_: &SessionRecord| true), &next, &mut later, at_ms() + 72_000);
        assert!(lines.is_empty(), "a verified live host tells, not the tick: {lines:?}");

        // The wrap exits: the child is judged by its cached route, and said once.
        let mut exited = dead.clone();
        wrap_exited(&mut exited);
        let mut said = CursorFile::new();
        let lines = claim(&exited, &hooks, &unsealed_wrap, &later, &mut said, at_ms() + 84_000);
        assert_eq!(
            lines,
            vec![ChildClaim { parent: "P".into(), line: "[claude fond-aspen] died mid-turn · last: working".into() }],
        );
    }

    #[test]
    fn a_child_whose_spawner_does_not_verify_is_kept_until_its_turn_is_a_day_old() {
        let mut roster = spawned_shape();
        let harness = sleeper(&mut roster);
        let hooks = [hook_rec("K", "working", AT)];
        let mut old = CursorFile::new();
        claim(&roster, &hooks, &(|_: &SessionRecord| true), &CursorFile::new(), &mut old, at_ms());
        die(harness);
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
        let mut roster = spawned_shape();
        let harness = sleeper(&mut roster);
        let hooks = [hook_rec("K", "working", AT)];
        let mut old = CursorFile::new();
        claim(&roster, &hooks, &(|_: &SessionRecord| true), &CursorFile::new(), &mut old, at_ms());
        die(harness);

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

    /// A death nobody recorded is inferred only once the host is gone, since a
    /// live host records the death itself, whether its seal verifies or not; and
    /// then only to a spawner that verifies.
    #[test]
    fn a_gone_child_is_inferred_dead_only_once_its_host_is_gone_and_its_spawner_verifies() {
        let mut roster = spawned_shape();
        let harness = sleeper(&mut roster);
        let hooks = [hook_rec("K", "working", AT)];
        let mut next = CursorFile::new();
        claim(&roster, &hooks, &(|_: &SessionRecord| true), &CursorFile::new(), &mut next, at_ms());
        die(harness);
        let gone_roster: Vec<SessionRecord> = roster.iter().filter(|r| r.session_id != "K").cloned().collect();

        for host_seal in [true, false] {
            let sealed = |r: &SessionRecord| host_seal || r.session_id != "W";
            let mut kept = CursorFile::new();
            assert!(claim(&gone_roster, &hooks, &sealed, &next, &mut kept, at_ms() + 60_000).is_empty(), "seal {host_seal}");
            assert_eq!(kept["K"].hook, next["K"].hook, "withheld, not forgotten (seal {host_seal})");
        }

        let mut exited = gone_roster.clone();
        wrap_exited(&mut exited);
        let unsealed_spawner = |r: &SessionRecord| r.session_id != "P";
        let mut kept = CursorFile::new();
        assert!(claim(&exited, &hooks, &unsealed_spawner, &next, &mut kept, at_ms() + 72_000).is_empty());
        assert!(kept.contains_key("K"), "a spawner that does not verify: kept");

        let mut said = CursorFile::new();
        let lines = claim(&exited, &hooks, &(|_: &SessionRecord| true), &kept, &mut said, at_ms() + 84_000);
        assert_eq!(lines.len(), 1, "{lines:?}");
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

    fn died_fact(outcome: &str) -> HookDeath {
        HookDeath { last_tool: Some("Bash".into()), outcome: Some(outcome.into()), ..Default::default() }
    }

    /// The wrap's own fact is claimed once, from an open turn, with the tool it
    /// was in and the status it recorded.
    #[test]
    fn a_wrap_recorded_death_is_claimed_once_with_its_tool_and_exit_status() {
        let (_, base) = decide(&child("working", AT), None, at_ms());
        let mut kid = died("2026-10-07T10:01:00Z", HookDeath { signal: Some(9), ..died_fact("signal") });
        let (event, next) = decide(&kid, Some(&base), at_ms() + 60_000);
        assert_eq!(line(event).as_deref(), Some("[claude fond-aspen] died mid-turn (signal 9) · last: Bash"));
        assert!(next.died);
        assert_eq!(next.phase, "done");
        kid.death = None;
        assert_eq!(decide(&kid, Some(&next), at_ms() + 120_000).0, None, "once: the claim cleared the fact");
    }

    /// The status a fact names is printed only where a real process could have
    /// ended with it, and the tool is cleaned like every other value read back.
    #[test]
    fn a_death_status_outside_what_a_process_can_end_with_is_not_printed() {
        let status = |death: HookDeath| status_of(&death);
        assert_eq!(status(HookDeath { exit_code: Some(0), ..died_fact("exit") }).as_deref(), Some("exit 0"));
        assert_eq!(status(HookDeath { exit_code: Some(256), ..died_fact("exit") }), None);
        assert_eq!(status(HookDeath { exit_code: Some(-1), ..died_fact("exit") }), None);
        assert_eq!(status(HookDeath { signal: Some(9), ..died_fact("signal") }).as_deref(), Some("signal 9"));
        assert_eq!(status(HookDeath { signal: Some(0), ..died_fact("signal") }), None);
        assert_eq!(status(HookDeath { signal: Some(65), ..died_fact("signal") }), None);
        assert_eq!(status(died_fact("timeout")).as_deref(), Some("timeout"));
        assert_eq!(status(died_fact("bogus")), None);
        assert_eq!(status(HookDeath { exit_code: Some(3), ..Default::default() }), None, "no outcome, no status");

        let mut kid = died("2026-10-07T10:01:00Z", HookDeath::default());
        kid.death = Some(HookDeath { last_tool: Some("Bash\r\u{1b}[2J".into()), exit_code: Some(999), ..died_fact("exit") });
        let line = line(decide(&kid, Some(&cursor("working", AT, false)), at_ms() + 1_000).0).unwrap();
        assert_eq!(line, "[claude fond-aspen] died mid-turn · last: Bash[2J", "{line:?}");
        assert!(!line.chars().any(char::is_control), "{line:?}");
    }

    /// A wrap's fact about a child the tick no longer gathers (its wrap has
    /// exited) is said over the cached route, with the fact's own tool and status.
    #[test]
    fn a_wrap_recorded_death_is_said_over_the_cached_route_once_the_wrap_is_gone() {
        let mut roster = spawned_shape();
        let harness = sleeper(&mut roster);
        let sealed = |_: &SessionRecord| true;
        let mut next = CursorFile::new();
        claim(&roster, &[hook_rec("K", "working", AT)], &sealed, &CursorFile::new(), &mut next, at_ms());
        die(harness);

        let mut exited = roster.clone();
        wrap_exited(&mut exited);
        let fact = HookDeath { exit_code: Some(3), last_tool: Some("Edit".into()), ..died_fact("exit") };
        let recorded = HookRecord { death: Some(fact), ..hook_rec("K", "done", "2026-10-07T10:01:00Z") };
        let only_spawner = |r: &SessionRecord| r.session_id == "P";
        let mut after = CursorFile::new();
        let lines = claim(&exited, &[recorded.clone()], &only_spawner, &next, &mut after, at_ms() + 60_000);
        assert_eq!(
            lines,
            vec![ChildClaim { parent: "P".into(), line: "[claude fond-aspen] died mid-turn (exit 3) · last: Edit".into() }]
        );
        assert!(latched(&after, "K"));

        let mut kept = CursorFile::new();
        let unsealed_spawner = |r: &SessionRecord| r.session_id != "P";
        assert!(claim(&exited, &[recorded], &unsealed_spawner, &next, &mut kept, at_ms() + 60_000).is_empty());
        assert!(kept.contains_key("K"), "a fact whose spawner does not verify is kept, not said");
    }

    /// The wrap's death is `done` with its fact, so every reader that folds
    /// `hooks.json` (presence, the graph, the widgets) reads the process as over,
    /// and not as an idle or online session.
    #[test]
    fn a_recorded_death_reads_as_done_through_the_merged_reader() {
        let sessions = vec![session("kid", "/w", "working", AT, None)];
        let hooks = [HookRecord { death: Some(died_fact("signal")), ..hook_rec("kid", "done", AT) }];
        let merged = crate::graph::merged_sessions(&sessions, &hooks);
        assert_eq!(merged[0].state, "done");
        assert_eq!(crate::graph::who::session_presence(&merged[0].state), "done");
    }
    #[cfg(unix)]
    fn e2e_stage(tag: &str) -> (std::path::PathBuf, EnvVars, std::sync::MutexGuard<'static, ()>) {
        let guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_SESSION_ID"]);
        let root = unique_stage(tag);
        std::env::set_var("AOIDE_STAGE_DIR", root.join("stage"));
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::set_var("AOIDE_AUDIT_LOG", root.join("log"));
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        std::env::remove_var("AOIDE_SESSION_ID");
        (root, env, guard)
    }

    #[cfg(unix)]
    fn now_ms() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64)
    }

    /// One real tick over what is on disk: the roster and `hooks.json` as they are,
    /// the seal as a daemon with the wrap's pid alive would answer it, and the
    /// cursor carried from the previous tick. The next cursor starts as the old
    /// one, as the daemon's claim seeds it (`claim_locked` carries every tracked
    /// entry over); every entry here has a hook half, which the claim judges
    /// whether its child is still tracked or not.
    #[cfg(unix)]
    fn tick(cursor: &mut CursorFile) -> Vec<ChildClaim> {
        tick_at(cursor, now_ms())
    }

    /// [`tick`] at a given instant, for a line that waits on the clock.
    #[cfg(unix)]
    fn tick_at(cursor: &mut CursorFile, now_ms: i64) -> Vec<ChildClaim> {
        let roster: Vec<SessionRecord> = load_stage::<SessionsFile>(&sessions_path()).unwrap().sessions;
        let mut hooks: HooksFile = load_stage(&hooks_path()).unwrap_or_default();
        let sealed = |r: &SessionRecord| {
            r.session_id == SPAWNER || (r.session_id == WRAP && r.pid.is_some_and(aoide_storage::fs::pid_is_alive))
        };
        let mut next = cursor.clone();
        let (lines, said) = super::claim(&roster, &hooks.hooks, &sealed, &cursor.clone(), &mut next, now_ms);
        std::fs::write(super::super::pingback_path(), serde_json::to_string(&next).unwrap()).unwrap();
        if !said.is_empty() {
            clear_said(&mut hooks.hooks, &said);
            write_stage(&hooks_path(), &hooks).unwrap();
        }
        *cursor = next;
        lines
    }

    #[cfg(unix)]
    const SPAWNER: &str = "spawner-kid";
    #[cfg(unix)]
    const WRAP: &str = "wrap-kid";
    #[cfg(unix)]
    const HOOK: &str = "hook-kid";

    /// The fake harness: `before` runs first (the phase it reports through the
    /// real `aoide session phase` door), then it announces its pid and waits for
    /// `GO`, and `after` runs once it is released.
    #[cfg(unix)]
    fn fake_harness(before: &str, after: &str) -> String {
        format!("{before}\necho $$ > \"$PIDFILE\"\nwhile [ ! -f \"$GO\" ]; do sleep 0.05; done\n{after}\n")
    }

    /// The harness as a shell host runs it: `before` runs in the wrap's own child,
    /// a shell, and the harness is that shell's child. It announces its pid, waits
    /// for `GO` and kills itself, while the shell outlives it, so the wrap is
    /// still up once the harness is gone. The shell ends with the wrap's signal.
    #[cfg(unix)]
    fn shell_host(before: &str) -> String {
        format!("{before}\nsh -c 'echo $$ > \"$PIDFILE\"; while [ ! -f \"$GO\" ]; do sleep 0.05; done; kill -9 $$'\nexec sleep 60\n")
    }

    /// Stage the spawner and the hook child, run a real `aoide conduct` over the
    /// fake harness with the spawner as its parent, and stamp the wrap's own pid
    /// and the harness pid onto the hook child's ancestry, as the hook door would.
    /// Returns the wrap and the harness pid once the wrap is registered and its
    /// spawner attested.
    #[cfg(unix)]
    fn start_wrap(root: &std::path::Path, script: &str) -> (std::process::Child, i32) {
        // Started now: a spawner older than this boot is a pre-boot ghost to the reaper.
        let mut spawner = session(SPAWNER, "/spawner", "working", &aoide_storage::time::now_iso_utc(), None);
        spawner.conductable = Some(true);
        spawner.exempt = true;
        spawner.pid = Some(std::process::id());
        let mut hook = session(HOOK, "/w", "working", AT, Some(WRAP));
        hook.kind = Some("agent".into());
        hook.petname = Some("fond-aspen".into());
        hook.activity = Some("Bash".into());
        let sessions = SessionsFile { schema_version: STAGE_GRAPH_VERSION.into(), sessions: vec![spawner, hook] };
        write_stage(&sessions_path(), &sessions).unwrap();

        let script_path = root.join("harness.sh");
        std::fs::write(&script_path, script).unwrap();
        let pidfile = root.join("harness.pid");
        let child = std::process::Command::new(built_aoide_bin())
            .args(["conduct", "--headless", "--id", WRAP, "--agent", "claude", "--parent", SPAWNER, "--", "sh"])
            .arg(&script_path)
            .env("AOIDE", built_aoide_bin())
            .env("PIDFILE", &pidfile)
            .env("GO", root.join("go"))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("aoide conduct starts");

        let harness = poll_until(|| std::fs::read_to_string(&pidfile).ok()?.trim().parse::<i32>().ok())
            .expect("the fake harness announces its pid");
        let wrap_pid = child.id() as i32;
        poll_until(|| {
            let f: SessionsFile = load_stage(&sessions_path()).ok()?;
            let w = f.sessions.iter().find(|s| s.session_id == WRAP)?;
            (w.conductable == Some(true) && w.attested_spawner.as_deref() == Some(SPAWNER)).then_some(())
        })
        .expect("the wrap registers, and its spawner is attested");
        aoide_storage::fs::with_stage_lock(|| {
            let mut f: SessionsFile = load_stage(&sessions_path()).unwrap();
            let k = f.sessions.iter_mut().find(|s| s.session_id == HOOK).unwrap();
            k.hook_ancestry = vec![harness, wrap_pid];
            write_stage(&sessions_path(), &f).unwrap();
        });
        (child, harness)
    }

    /// Poll `probe` for up to ten seconds; its first answer, or `None`.
    #[cfg(unix)]
    fn poll_until<T>(mut probe: impl FnMut() -> Option<T>) -> Option<T> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            if let Some(v) = probe() {
                return Some(v);
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        None
    }

    #[cfg(unix)]
    fn wait_exit(wrap: &mut std::process::Child) -> std::process::ExitStatus {
        poll_until(|| wrap.try_wait().ok().flatten()).expect("the wrap ends once its child does")
    }

    #[cfg(unix)]
    fn hook_record() -> HookRecord {
        let hooks: HooksFile = load_stage(&hooks_path()).unwrap();
        hooks.hooks.into_iter().find(|h| h.session_id == HOOK).expect("the hook child has a record")
    }

    /// The live lines over a real wrap, each said once to the spawner: a `working`
    /// turn quiet for 12 minutes is `silent`, entering a prompt is `awaiting`, and
    /// stopping after it is `settled`. A harness that then exits says nothing more.
    #[cfg(unix)]
    #[test]
    fn a_live_child_is_heard_silent_then_awaiting_then_settled_once_each() {
        let (root, _env, _guard) = e2e_stage("dmt-live");
        let phase = |p: &str| format!("\"$AOIDE\" session phase --id {HOOK} --phase {p}");
        let gate = |n: u8| format!("while [ ! -f \"$GO.{n}\" ]; do sleep 0.05; done");
        let after = [phase("awaiting"), gate(2), phase("stopped"), gate(3)].join("\n");
        let (mut wrap, _) = start_wrap(&root, &fake_harness(&phase("working"), &after));
        let mut cursor = CursorFile::new();
        assert!(tick(&mut cursor).is_empty(), "the first examination is a baseline");
        let heard = |line: &str| vec![ChildClaim { parent: SPAWNER.into(), line: line.into() }];

        let quiet = now_ms() + SILENCE_MS;
        assert_eq!(tick_at(&mut cursor, quiet), heard("[claude fond-aspen] silent 12 min · last: Bash"));
        assert!(tick_at(&mut cursor, quiet + 60_000).is_empty(), "once per stamp");

        std::fs::write(root.join("go"), "").unwrap();
        poll_until(|| (hook_record().phase == "awaiting").then_some(())).expect("the harness asks");
        assert_eq!(tick(&mut cursor), heard("[claude fond-aspen] awaiting"));

        std::fs::write(root.join("go.2"), "").unwrap();
        poll_until(|| (hook_record().phase == "stopped").then_some(())).expect("the harness stops");
        assert_eq!(tick(&mut cursor), heard("[claude fond-aspen] settled"));
        assert!(tick(&mut cursor).is_empty(), "settled once");

        std::fs::write(root.join("go.3"), "").unwrap();
        wait_exit(&mut wrap);
        assert!(tick(&mut cursor).is_empty(), "a closed turn's exit says nothing");
    }

    /// A real wrap whose harness dies by signal 9 mid-turn: the wrap writes the
    /// fact itself, and the tick says the death once, with the signal, to the spawner.
    #[cfg(unix)]
    #[test]
    fn a_wrap_whose_child_dies_mid_turn_records_the_death_and_its_spawner_hears_it_once() {
        let (root, _env, _guard) = e2e_stage("dmt-death");
        let before = format!("\"$AOIDE\" session phase --id {HOOK} --phase working");
        let (mut wrap, _) = start_wrap(&root, &fake_harness(&before, "kill -9 $$"));
        let mut cursor = CursorFile::new();
        assert!(tick(&mut cursor).is_empty(), "the first examination is a baseline");

        std::fs::write(root.join("go"), "").unwrap();
        wait_exit(&mut wrap);
        let rec = hook_record();
        assert_eq!(rec.phase, "done");
        assert_eq!(
            rec.death,
            Some(HookDeath { last_tool: Some("Bash".into()), outcome: Some("signal".into()), signal: Some(9), ..Default::default() }),
            "the fact names the tool in flight and the signal"
        );

        assert_eq!(
            tick(&mut cursor),
            vec![ChildClaim {
                parent: SPAWNER.into(),
                line: "[claude fond-aspen] died mid-turn (signal 9) · last: Bash".into(),
            }]
        );
        assert!(tick(&mut cursor).is_empty(), "at most once");
    }

    /// A wrap whose child exits after a closed phase writes nothing, and nothing is
    /// said: the phase closed first, so there was no death to record.
    #[cfg(unix)]
    #[test]
    fn a_wrap_whose_child_exits_after_a_closed_phase_writes_nothing_and_says_nothing() {
        let (root, _env, _guard) = e2e_stage("dmt-closed");
        let before = format!("\"$AOIDE\" session phase --id {HOOK} --phase stopped");
        let (mut wrap, _) = start_wrap(&root, &fake_harness(&before, "kill -9 $$"));
        let mut cursor = CursorFile::new();
        assert!(tick(&mut cursor).is_empty());
        let closed = hook_record();

        std::fs::write(root.join("go"), "").unwrap();
        wait_exit(&mut wrap);
        assert_eq!(hook_record(), closed, "the wrap left the closed phase exactly as it was");
        assert!(tick(&mut cursor).is_empty(), "no line");
    }

    /// The tick's own inference still says a death for a wrap that died without
    /// writing anything: here the wrap is SIGKILLed, and the harness with it.
    #[cfg(unix)]
    #[test]
    fn a_wrap_killed_without_writing_is_still_said_of_by_the_tick() {
        let (root, _env, _guard) = e2e_stage("dmt-sigkill");
        let before = format!("\"$AOIDE\" session phase --id {HOOK} --phase working");
        let (mut wrap, harness) = start_wrap(&root, &fake_harness(&before, ""));
        let mut cursor = CursorFile::new();
        assert!(tick(&mut cursor).is_empty());

        wrap.kill().unwrap();
        wrap.wait().unwrap();
        std::process::Command::new("kill").args(["-9", &harness.to_string()]).status().unwrap();
        assert!(
            poll_until(|| harness_gone(harness as u32).then_some(())).is_some(),
            "the harness is gone"
        );
        assert_eq!(hook_record().phase, "working", "nothing was written for it");

        assert_eq!(
            tick(&mut cursor),
            vec![ChildClaim { parent: SPAWNER.into(), line: "[claude fond-aspen] died mid-turn · last: Bash".into() }],
            "said once, with no status: nobody saw the exit"
        );
        assert!(tick(&mut cursor).is_empty());
    }

    /// Gone for the tick's purposes: no such pid, or a zombie an init has not
    /// reaped yet (`pid_is_alive` counts a zombie as alive).
    #[cfg(unix)]
    fn harness_gone(pid: u32) -> bool {
        let state = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok().and_then(|stat| {
            stat.rsplit(')').next().and_then(|rest| rest.trim_start().chars().next())
        });
        !aoide_storage::fs::pid_is_alive(pid) || state == Some('Z')
    }

    #[cfg(unix)]
    fn child_state() -> String {
        let sessions: SessionsFile = load_stage(&sessions_path()).unwrap();
        sessions.sessions.into_iter().find(|s| s.session_id == HOOK).expect("the hook child is on the roster").state
    }

    /// The reaper runs its pass over a wrap's death before the tick reads it: the
    /// fact survives the pass, is held until the tick claims it, and is said once,
    /// with its signal, however many passes follow.
    #[cfg(unix)]
    #[test]
    fn a_reap_pass_leaves_a_wrap_recorded_death_to_be_said_once_with_its_signal() {
        let (root, _env, _guard) = e2e_stage("dmt-reap");
        let before = format!("\"$AOIDE\" session phase --id {HOOK} --phase working");
        let (mut wrap, _) = start_wrap(&root, &fake_harness(&before, "kill -9 $$"));
        let mut cursor = CursorFile::new();
        assert!(tick(&mut cursor).is_empty());

        std::fs::write(root.join("go"), "").unwrap();
        wait_exit(&mut wrap);
        assert_eq!(hook_record().phase, "done");
        assert_eq!(child_state(), "done", "the wrap marks its dead child done");

        crate::reap::reap(&crate::graph::testutil::invocation(&["session", "reap"], &[]));
        let hooks: HooksFile = load_stage(&hooks_path()).unwrap();
        assert_eq!(hook_record().death.and_then(|d| d.signal), Some(9), "the reaper leaves the fact");
        assert_eq!(crate::graph::held_deaths(&load_stage::<SessionsFile>(&sessions_path()).unwrap().sessions, &hooks.hooks), HashSet::from([HOOK.to_string()]), "held until said");
        assert_eq!(
            tick(&mut cursor),
            vec![ChildClaim {
                parent: SPAWNER.into(),
                line: "[claude fond-aspen] died mid-turn (signal 9) · last: Bash".into(),
            }]
        );
        let hooks: HooksFile = load_stage(&hooks_path()).unwrap();
        assert!(crate::graph::held_deaths(&load_stage::<SessionsFile>(&sessions_path()).unwrap().sessions, &hooks.hooks).is_empty(), "claimed, so no longer held");

        crate::reap::reap(&crate::graph::testutil::invocation(&["session", "reap"], &[]));
        assert!(tick(&mut cursor).is_empty(), "said once, and still once after a later pass");
    }

    /// A clean end the reaper or a prune took with its session, while its harness
    /// still runs: the entry is dropped with the record, and the harness's later
    /// exit says nothing.
    #[cfg(unix)]
    #[test]
    fn a_pruned_clean_end_whose_harness_later_exits_is_not_said_of() {
        let (root, _env, _guard) = e2e_stage("dmt-pruned");
        let before = format!("\"$AOIDE\" session phase --id {HOOK} --phase working");
        let (mut wrap, harness) = start_wrap(&root, &fake_harness(&before, ""));
        let mut cursor = CursorFile::new();
        assert!(tick(&mut cursor).is_empty());
        aoide_storage::fs::with_stage_lock(|| {
            let mut sessions: SessionsFile = load_stage(&sessions_path()).unwrap();
            sessions.sessions.retain(|s| s.session_id != HOOK);
            write_stage(&sessions_path(), &sessions).unwrap();
            let mut hooks: HooksFile = load_stage(&hooks_path()).unwrap();
            hooks.hooks.retain(|h| h.session_id != HOOK);
            write_stage(&hooks_path(), &hooks).unwrap();
        });
        assert!(tick(&mut cursor).is_empty(), "no record, no line");
        assert!(!cursor.contains_key(HOOK), "an absent record is a clean end: the entry is dropped");

        std::process::Command::new("kill").args(["-9", &harness.to_string()]).status().unwrap();
        assert!(poll_until(|| harness_gone(harness as u32).then_some(())).is_some(), "the harness is gone");
        wait_exit(&mut wrap);
        assert!(tick(&mut cursor).is_empty(), "no death is said, whatever the harness did later");
    }

    /// A harness that dies under a wrap that outlives it, and whose child record
    /// then leaves the roster: nothing is inferred while the host runs, because a
    /// live host records the death itself. The host then ends with no child left
    /// to record, and only then is the death inferred, once, with no status.
    #[cfg(unix)]
    #[test]
    fn a_dead_harness_whose_record_left_is_not_inferred_while_its_host_runs() {
        let (root, _env, _guard) = e2e_stage("dmt-live-host");
        let before = format!("\"$AOIDE\" session phase --id {HOOK} --phase working");
        let (mut wrap, harness) = start_wrap(&root, &shell_host(&before));
        let mut cursor = CursorFile::new();
        assert!(tick(&mut cursor).is_empty(), "the first examination is a baseline");

        std::fs::write(root.join("go"), "").unwrap();
        assert!(poll_until(|| harness_gone(harness as u32).then_some(())).is_some(), "the harness is gone");
        aoide_storage::fs::with_stage_lock(|| {
            let mut sessions: SessionsFile = load_stage(&sessions_path()).unwrap();
            sessions.sessions.retain(|s| s.session_id != HOOK);
            write_stage(&sessions_path(), &sessions).unwrap();
        });
        assert!(wrap.try_wait().unwrap().is_none(), "the host still runs");
        assert!(tick(&mut cursor).is_empty(), "no inference while a live host can record it");

        unsafe { libc::kill(wrap.id() as libc::pid_t, libc::SIGTERM) };
        wait_exit(&mut wrap);
        assert_eq!(hook_record().death, None, "the host found no child to record");
        assert_eq!(
            tick(&mut cursor),
            vec![ChildClaim { parent: SPAWNER.into(), line: "[claude fond-aspen] died mid-turn · last: Bash".into() }]
        );
        assert!(tick(&mut cursor).is_empty(), "once");
    }

    /// The reaper runs between a shell-hosted harness's death and its wrap's end:
    /// the child's record names the dead harness, but its wrap still runs with
    /// the turn open, so the reaper leaves the child to the wrap. The wrap then
    /// records the death, and the spawner hears it once.
    #[cfg(unix)]
    #[test]
    fn a_reap_while_the_wrap_runs_leaves_the_death_to_the_wrap_and_it_is_said_once() {
        let (root, _env, _guard) = e2e_stage("dmt-reap-live");
        let before = format!("\"$AOIDE\" session phase --id {HOOK} --phase working");
        let (mut wrap, harness) = start_wrap(&root, &shell_host(&before));
        let mut cursor = CursorFile::new();
        assert!(tick(&mut cursor).is_empty(), "the first examination is a baseline");
        // The record's own pid names the harness, as a hook-registered claude's does.
        aoide_storage::fs::with_stage_lock(|| {
            let mut f: SessionsFile = load_stage(&sessions_path()).unwrap();
            f.sessions.iter_mut().find(|s| s.session_id == HOOK).unwrap().pid = Some(harness as u32);
            write_stage(&sessions_path(), &f).unwrap();
        });

        std::fs::write(root.join("go"), "").unwrap();
        assert!(poll_until(|| harness_gone(harness as u32).then_some(())).is_some(), "the harness is gone");
        crate::reap::reap(&crate::graph::testutil::invocation(&["session", "reap"], &[]));
        assert!(wrap.try_wait().unwrap().is_none(), "the wrap still runs");
        assert_eq!(child_state(), "working", "the reaper leaves a live wrap's open child to it");
        assert_eq!(hook_record().phase, "working");

        unsafe { libc::kill(wrap.id() as libc::pid_t, libc::SIGTERM) };
        wait_exit(&mut wrap);
        assert_eq!(
            tick(&mut cursor),
            vec![ChildClaim {
                parent: SPAWNER.into(),
                line: "[claude fond-aspen] died mid-turn (signal 15) · last: Bash".into(),
            }]
        );
        assert!(tick(&mut cursor).is_empty(), "once");
    }

    /// An end signal sent to the wrap itself is forwarded to its harness, and the
    /// harness's end is the wrap's own death fact, with the signal it ended by.
    #[cfg(unix)]
    #[test]
    fn a_sigterm_to_the_wrap_is_forwarded_and_recorded_as_a_signal_death() {
        let (root, _env, _guard) = e2e_stage("dmt-term");
        let before = format!("\"$AOIDE\" session phase --id {HOOK} --phase working");
        let (mut wrap, _) = start_wrap(&root, &fake_harness(&before, ""));
        let mut cursor = CursorFile::new();
        assert!(tick(&mut cursor).is_empty());

        unsafe { libc::kill(wrap.id() as libc::pid_t, libc::SIGTERM) };
        let status = wait_exit(&mut wrap);
        assert_eq!(std::os::unix::process::ExitStatusExt::signal(&status), Some(libc::SIGTERM), "the wrap exits by the signal");
        assert_eq!(hook_record().phase, "done");
        assert_eq!(
            hook_record().death,
            Some(HookDeath { last_tool: Some("Bash".into()), outcome: Some("signal".into()), signal: Some(15), ..Default::default() }),
        );
        assert_eq!(
            tick(&mut cursor),
            vec![ChildClaim {
                parent: SPAWNER.into(),
                line: "[claude fond-aspen] died mid-turn (signal 15) · last: Bash".into(),
            }]
        );
    }

    /// A baseline that first sees a recent death says it; the same phase a day old
    /// is history and says nothing.
    #[test]
    fn a_death_the_baseline_first_sees_is_said_while_it_is_recent() {
        let (event, cursor) = decide(&died(AT, died_fact("signal")), None, at_ms() + 60_000);
        assert!(event.is_some(), "a recent death is announced");
        assert!(cursor.died, "and latched");
    }

    #[test]
    fn a_death_the_baseline_first_sees_a_day_old_is_silent() {
        let (event, cursor) = decide(&died(AT, died_fact("signal")), None, at_ms() + STALE_MS + 1);
        assert!(event.is_none(), "a day-old death says nothing");
        assert!(!cursor.died);
    }
}
