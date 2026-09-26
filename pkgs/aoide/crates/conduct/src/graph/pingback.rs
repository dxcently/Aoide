//! The ping-back (P-EIDOLON slice E5b, `docs/architecture/EIDOLON-TRACE.md`
//! "Second slice"): **a parent hears the children it spawned.** Every
//! `agent:"eidolon"` record Aoide enrolled from outside carries a TRACE
//! (`<stem>.jsonl` beside its `.eid`, named by its presence `meta.json`), and
//! this module is the reaper tick's own reader of it: the highest-priority
//! new event on a child since the last pass becomes ONE line, and that line is
//! delivered to the child's `parentSessionId`.
//!
//! **The daemon's own line, never a `send`.** The send door attests the SENDER
//! from the running process's `/proc` ancestry, so inside the resident daemon
//! the attested sender is the daemon — never the child — which is exactly why
//! the reciprocal rule cannot be `sender_is_parent` made symmetric
//! (`conduct/AGENTS.md` forbids threading a peercred pid into `send.rs` for
//! it). This module therefore takes the DOORBELL's path instead
//! (`graph/doorbell.rs`): raw injection into the target's own transport — one
//! write + close over a live Claude Code channel socket, else
//! [`write_delivery`] plus the target's own submit keystroke for a headless
//! wrap — with no gate, no `pending.json` entry, no provenance prefix, no
//! title rename (the line starts with `[`, which
//! [`names_the_node`](super::send::names_the_node) would otherwise read as a
//! task label), and no mailbase receipt. Every other door forwards this exact
//! thing through the daemon instead, because the daemon IS the policy and
//! audit boundary here: [`pingback`] executes only under `Door::Daemon` and
//! returns silently for every other door (the daemon's own tick will do it).
//!
//! **Post-lock, after the eidolon sync.** The reaper holds `.stage.lock`
//! across `reap_inner`; this module runs entirely AFTER that lock is released
//! (`reap.rs`'s post-lock collector block, the same side `sync_codex_app_
//! threads`/`refresh_live_agents` sit on), never inside it — the claim section
//! takes the stage lock briefly for its own read-decide-write, and the socket
//! write that follows happens with no lock held at all. The socket write is
//! bounded ([`super::doorbell::connect_for_ring`]'s own 2s timeout, the one
//! transport-timeout authority in this crate) so a wedged parent can never
//! park the daemon's tick.
//!
//! **At-most-once, by a per-child cursor claimed BEFORE the delivery.**
//! `state/stage/pingback.json` holds, per child session id, the last trace
//! record id examined (`seen`) and the record id a silence line was already
//! sent for (`silentAt`). The claim section reads that file, decides, and
//! writes the ADVANCED cursor with a temp-then-rename write, all inside one
//! short critical section; the delivery happens after. A crash between the two
//! loses a line — the safe direction, since the other direction is a duplicate
//! in a parent's composer. A child whose record is gone drops out of the file
//! on the same pass.
//!
//! **The event and the line are two things.** [`choose_event`] decides WHICH
//! [`PingEvent`] a child's new records amount to; [`render_line`] turns that
//! event into the one local line and needs nothing but the event and the tag.
//! Everything the line shows that is not the event — the tag — is applied by
//! whoever renders, so an event names no node and carries no presentation. The
//! same split is what lets a child whose parent sits on ANOTHER node publish
//! the event instead of a line: `remoteParent` on the record means the claim
//! SPOOLS the event to `state/stage/pingback-remote.json` (whose parent pulls it
//! over the A2A door, CONTRACTS.md §4/§6) and this module never calls
//! [`deliver`] for it. That is also why every string in a `PingEvent` is
//! already cleaned HERE, at the sender: the far renderer is another node's
//! code, and the text it renders must be safe before it leaves.
//!
//! **Never a shell parent.** A line submitted into a bare shell would RUN as a
//! command, so a target whose `agent` is `""`/`"shell"` (or names no
//! registered harness profile) is skipped and counted, never injected into.
//! Every child-authored fragment of a line — the quoted say, prompt and stop
//! reason, and the unquoted tool label — is untrusted model output (house
//! rule 4): one line, every unsafe character stripped (control, and the
//! Unicode `Cf` marks that would reorder it invisibly — the same
//! [`super::common::is_unsafe`] set `clean_line` strips), clipped to
//! [`SAY_MAX`] with `…`; the quoted ones are never allowed to start with `/`
//! or `!`.

use super::common::{clean_line, clip_flat, strip_unsafe};
use super::conduct::channel_socket_path;
use super::doorbell::{connect_for_ring, write_channel};
use super::eidolon::{eidolon_state_from_trace, DroppedEidolon};
use super::model::{canonical_state, load_stage, sessions_path, SessionRecord, SessionsFile};
use super::permit::profile_for_agent;
use super::send::{audit_send, write_delivery, SUBMIT_KEYSTROKE_DELAY};
use super::trace::{one_line_clip, tool_result_summary};
use aoide_client::node::FrameReadError;
use aoide_protocol::agents::{agent_profile, eidolon_trace_record, TraceRecord};
use aoide_protocol::{Door, Invocation};
use aoide_storage::pingback_remote::RingRead;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

/// How much of a quoted `say`/prompt reaches the parent's line — one short
/// phrase, never a paragraph in somebody else's composer.
const SAY_MAX: usize = 80;
/// How long a turn may sit with no new record before the child is called
/// silent (the brief's ten minutes).
const SILENCE_MS: i64 = 10 * 60 * 1000;
/// How many `ToolResult{is_error:true}` records IN A ROW it takes to say
/// "failing" — below this, tool errors ride an existing line at most.
const ERROR_RUN: u64 = 3;

/// What one child's new records amount to — the decision, split from the line
/// it renders as (`render_line`) so the SAME decision can be spooled to a
/// remote parent's ring instead of delivered here (CONTRACTS.md §4, P-RSA
/// S8).
///
/// Closed, and every field already CLEANED: each string passed `clean`/
/// `str_field`/`numeric_field` on the way in, so a renderer — including
/// another node's — never sanitizes again, and an event kind nobody knows is
/// simply not deserializable. Numbers stay numbers (`calls`, `mins`) rather
/// than pre-rendered segments, so the presentation lives in exactly one place,
/// [`render_line`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PingEvent {
    /// The turn ended by itself: `stop` is the harness's own `stop_reason`
    /// (`None` when it wrote none), and `errors` counts the
    /// `ToolResult{is_error:true}` records among the new ones.
    Settled {
        stop: Option<String>,
        calls: Option<u64>,
        mins: Option<i64>,
        say: Option<String>,
        errors: u64,
    },
    /// The turn was cancelled outright.
    Cancelled { calls: Option<u64>, mins: Option<i64>, say: Option<String>, errors: u64 },
    /// The process went away with its turn still open.
    DiedMidTurn { calls: Option<u64>, say: Option<String>, errors: u64 },
    /// A prompt is open and nothing has answered it.
    Asking { prompt: String, errors: u64 },
    /// The harness told the child to wrap up: exactly one of the two bounds is
    /// present, as the record's own kind decides.
    WrappingUp { calls_left: Option<String>, secs_left: Option<String>, errors: u64 },
    /// A run of tool errors ending the new records.
    Failing { run: u64, tool: Option<String> },
    /// An open turn, ten minutes quiet. `mins` is whole minutes; `last` is
    /// what the child last did or said, which the line quotes one way and not
    /// the other.
    Silent { mins: i64, last: Option<Last> },
    /// The child ENDED — claimed once per child, from the record's own `done`
    /// plus its `outcome`/`exitCode`, and only for a child whose parent is on
    /// another node (Q5's ruled default: a local parent hears the run's own
    /// report, never this). It is the one event every agent kind publishes,
    /// which is why a remote child of ANY harness spools one when it ends.
    Exited { code: Option<i32>, outcome: Option<String> },
}

/// What a `Silent` event's trailing `last:` names: the child SAID something
/// (the line quotes it) or it DID something (the tool label, bare). Kept
/// apart at the event level rather than baked into a quoted string, so the
/// renderer — on either node — is the one that decides the quotes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Last {
    Said(String),
    Did(String),
}

/// `state/stage/pingback.json` — the per-child cursor. One map, keyed by the
/// child's own native session id (the eidolon presence id, verbatim).
pub(crate) fn pingback_path() -> PathBuf {
    aoide_storage::fs::conducting_stage_dir().join("pingback.json")
}

/// One child's cursor entry: `seen` is the last trace record id examined
/// (delivered or merely passed over), `silentAt` the record id a silence line
/// was already sent for. `silentAt` is re-armed (absent) by any new record, so
/// a child that speaks and goes quiet again gets its next silence line.
/// `exited` is the one-shot latch for [`PingEvent::Exited`] — claimed once per
/// child, and it must be a latch of its own because a child that keeps no trace
/// (the non-eidolon harness `exited` exists for) has no `seen` to advance past
/// the end of its own record.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct CursorEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    seen: Option<String>,
    #[serde(rename = "silentAt", default, skip_serializing_if = "Option::is_none")]
    silent_at: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    exited: bool,
}

/// The `serde` skip for a latch: a `false` field is never written, so every
/// cursor entry without an exit in it stays byte-identical to what it was
/// before this field existed.
fn is_false(b: &bool) -> bool {
    !*b
}

/// The whole cursor file. A `BTreeMap` so the file's own key order is stable
/// (a diff of two ticks reads as a diff of children, never of hashing).
type CursorFile = BTreeMap<String, CursorEntry>;

/// What [`pingback`] did this tick — the report the reaper prints from and its
/// own tests assert against. Never folded into the sweep's `changed` vec: a
/// parent being told something must not toast the desktop every twelve
/// seconds (`refresh_live_agents`' own rule, one collector over).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PingbackReport {
    /// `(parent id, the line delivered)` per delivery.
    pub delivered: Vec<(String, String)>,
    /// `(target id, why not)` for every candidate that was claimed but not
    /// delivered — `no-parent-record`, `shell-parent`, `not-conductable`,
    /// `parent-done`, `interactive-composer`, `write-failed` — and, on the
    /// parent's own side, for every ledger row a pull could not read:
    /// `unknown-node`, `pull-failed` (the four above appear there too, judged
    /// before the far node is asked anything).
    pub skipped: Vec<(String, String)>,
}

/// One child this tick, already gathered: its roster facts plus the trace tail
/// READ OUTSIDE the claim section (a 1 MiB read per child has no business
/// inside the stage lock), so the critical section only reads the cursor,
/// decides, and writes it.
struct Child {
    id: String,
    petname: Option<String>,
    agent: String,
    parent: String,
    /// The trace's own tail lines, or `None` when there is no readable trace
    /// for this child right now (then nothing is examined, and any existing
    /// cursor entry is left exactly as it was).
    lines: Option<Vec<String>>,
    /// `sync_eidolon_sessions` dropped this child's presence record on THIS
    /// tick — the "died mid-turn" row, and the reason its cursor entry leaves
    /// the file on this same pass.
    dropped_mid_turn: bool,
    /// The record carries `remoteParent`: this child's parent is on ANOTHER
    /// node, so its events spool to the ring instead of being delivered here
    /// (CONTRACTS.md §4, P-RSA S8). `remote_key` is that stamp's own key — the
    /// parent's node key, and the ring entry's gate input (H2 of the S8/S9
    /// review): the ring outlives this record, so the key must ride with the
    /// event, not with the record the far door may no longer find.
    remote: bool,
    remote_key: String,
    /// The record's own end facts, present only once it says `done` — the one
    /// signal every agent kind publishes, and the whole input of
    /// [`PingEvent::Exited`].
    ended: Option<Ended>,
}

/// A roster record's own end: its `outcome` (a closed set: `exit`, `signal`,
/// `timeout`, `stopped`) and the `exitCode` that is present only beside
/// `exit`. Both absent is a real end too — an ordinary session that simply
/// stopped — so the event carries the two as they are rather than inventing a
/// status for it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Ended {
    code: Option<i32>,
    outcome: Option<String>,
}

/// One decided delivery: which parent, and the one line to write.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ChildClaim {
    parent: String,
    line: String,
}

/// One decided event for a child whose parent is elsewhere: the child whose
/// ring it goes on (its own session id — the id the parent pulls with), and
/// the event. Never a delivery: this node's `deliver` is only ever the LOCAL
/// parent's transport, and the far parent's own daemon does its own.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SpoolClaim {
    child: String,
    /// The key the child's own `remoteParent` was stamped with — written into
    /// the ring entry on its first event ([`aoide_storage::pingback_remote::
    /// spool_event`]), and the door's only gate input once the record is gone.
    key: String,
    event: PingEvent,
}

/// What one tick's claim section decided: the lines to deliver here, and the
/// events to spool for a parent on another node.
#[derive(Debug, Clone, Default)]
struct Claimed {
    lines: Vec<ChildClaim>,
    spool: Vec<SpoolClaim>,
}

/// Is this roster record one the ping-back tracks? Every `agent:"eidolon"`
/// record (the trace rows, exactly as before) and every child stamped
/// `remoteParent` — the latter whatever its harness, because its own exit is
/// the one event it can always publish (P-RSA S8). What the predicate decides
/// is also what the cursor file RETAINS an entry for, so a remote child's
/// `exited` latch survives the tick that claimed it.
fn tracks(rec: &SessionRecord) -> bool {
    rec.agent == "eidolon" || rec.remote_parent.is_some()
}

/// A record's end facts, `None` until its state folds to `done`. The fold is
/// [`canonical_state`] — the same one the roster and the delivery skips use —
/// so "ended" means the same thing here as everywhere else.
fn ended_of(rec: &SessionRecord) -> Option<Ended> {
    (canonical_state(&rec.state) == "done")
        .then(|| Ended { code: rec.exit_code, outcome: rec.outcome.clone() })
}

/// The ping-back's whole pass, run from `reap.rs`'s post-lock collector block
/// AFTER `sync_eidolon_sessions()` (so the roster it reads is the one that
/// sync just reconciled) — and only under `Door::Daemon`, the policy and
/// audit boundary for an automated line. Returns the tick's report; the
/// caller folds NOTHING into `outcome.changed`.
pub(crate) fn pingback(inv: &Invocation, dropped: &[DroppedEidolon]) -> PingbackReport {
    let mut report = PingbackReport::default();
    if inv.door != Door::Daemon {
        return report;
    }
    let roster: Vec<SessionRecord> = load_stage::<SessionsFile>(&sessions_path())
        .map(|f| f.sessions)
        .unwrap_or_default();
    let now_ms = (super::conduct::unix_ts() as i64) * 1000;

    // ── gather (no lock, no socket) ─────────────────────────────────────
    let mut children: Vec<Child> = Vec::new();
    let mut tracked_ids: HashSet<String> = HashSet::new();
    for rec in roster.iter().filter(|r| tracks(r)) {
        tracked_ids.insert(rec.session_id.clone());
        let remote = rec.remote_parent.is_some();
        // A child with no local parent has nobody HERE to tell — but a remote
        // child's parent is told over its ring, so that one is gathered all
        // the same.
        let parent = rec.parent_session_id.clone().filter(|p| !p.is_empty()).unwrap_or_default();
        if parent.is_empty() && !remote {
            continue;
        }
        children.push(Child {
            id: rec.session_id.clone(),
            petname: rec.petname.clone(),
            agent: rec.agent.clone(),
            parent,
            lines: locate_trace(&rec.agent, &rec.session_id, &rec.cwd, rec.log_path.as_deref()),
            dropped_mid_turn: false,
            remote,
            remote_key: rec.remote_parent.as_ref().map(|rp| rp.key.clone()).unwrap_or_default(),
            ended: ended_of(rec),
        });
    }
    // Two producers can name the same child on one pass — the eidolon sync's
    // own drop set and the roster exit `reap_inner` reports (H1 of the S8/S9
    // review) — and one child has ONE exit. The first producer to name it owns
    // it; the second is not a second decision over the same child.
    let mut gathered: HashSet<String> = children.iter().map(|c| c.id.clone()).collect();
    for drop in dropped {
        if !gathered.insert(drop.session_id.clone()) {
            continue;
        }
        let remote = drop.remote;
        let parent = drop.parent_session_id.clone().filter(|p| !p.is_empty()).unwrap_or_default();
        // The same rule as the roster above, plus the drop's own: a child the
        // sync just removed whose parent is on another node still owes that
        // parent its exit, and this pass is the last one that will ever decide
        // over it.
        if parent.is_empty() && !remote {
            continue;
        }
        children.push(Child {
            id: drop.session_id.clone(),
            petname: drop.petname.clone(),
            agent: drop.agent.clone(),
            parent,
            lines: drop
                .trace
                .as_deref()
                .and_then(|path| read_trace(&drop.agent, path)),
            dropped_mid_turn: true,
            remote,
            // A dropped child's key comes off the record it was built from,
            // like every other end fact on this path (H2 of the S8/S9 review):
            // its ring entry is stamped with it, so the far parent can still
            // read the exit after this record is gone.
            remote_key: drop.remote_key.clone().unwrap_or_default(),
            // No record is left to say `done`: the sync's own drop IS the
            // observation that this child ended.
            ended: remote.then(|| Ended { code: drop.exit_code, outcome: drop.outcome.clone() }),
        });
    }

    // ── claim: one short critical section, read → decide → write ────────
    // The advanced cursor is written INSIDE the lock, before it is released:
    // the claim IS the write, so no second tick can decide over the old
    // cursor between this tick's decision and its file.
    let claims = aoide_storage::fs::with_stage_lock(|| {
        let (claims, old_cursor, new_cursor) = claim_locked(&children, &tracked_ids, now_ms);
        if new_cursor != old_cursor {
            match serde_json::to_string(&new_cursor) {
                Ok(body) => {
                    if let Err(e) = aoide_storage::fs::atomic_write(&pingback_path(), &format!("{body}\n")) {
                        eprintln!("[aoide/reap] ping-back cursor write failed: {e}");
                    }
                }
                Err(e) => eprintln!("[aoide/reap] ping-back cursor encode failed: {e}"),
            }
        }
        claims
    });

    // ── deliver here, spool there (no lock held) ────────────────────────
    for claim in claims.lines {
        match deliver(&claim.line, &claim.parent, &roster, inv, "autogate-child") {
            Ok(()) => {
                eprintln!("[aoide/reap] ping-back → {}: {}", claim.parent, claim.line);
                report.delivered.push((claim.parent, claim.line));
            }
            Err(reason) => report.skipped.push((claim.parent, reason)),
        }
    }
    // A remote child's parent pulls its events off this node's door, so this
    // tick's job ends at the ring: the claim was written, the event lands
    // after — a crash in between loses an event, the same direction the
    // delivery above loses a line.
    for claim in claims.spool {
        match serde_json::to_value(&claim.event) {
            Ok(event) => match aoide_storage::pingback_remote::spool_event(&claim.child, &claim.key, event) {
                Some(seq) => eprintln!(
                    "[aoide/reap] ping-back spooled seq {seq} for {} (parent on another node)",
                    claim.child
                ),
                None => report.skipped.push((claim.child, "spool-failed".to_string())),
            },
            Err(e) => {
                eprintln!("[aoide/reap] ping-back event encode failed: {e}");
                report.skipped.push((claim.child, "encode-failed".to_string()));
            }
        }
    }
    report
}

/// The claim section's whole body — run ONLY inside the stage lock.
///
/// Entry lifetime: an entry survives only while its child is still a roster
/// record this module tracks ([`tracks`] — so a child whose record is gone,
/// dropped by the sync above or reaped as stale, leaves the file on this same
/// pass), and a child that yielded nothing to examine keeps whatever it had.
/// A remote child whose record is still there keeps its entry even when it has
/// no trace at all: that entry IS its `exited` latch.
fn claim_locked(
    children: &[Child],
    tracked_ids: &HashSet<String>,
    now_ms: i64,
) -> (Claimed, CursorFile, CursorFile) {
    let old = read_cursor();
    let mut next: CursorFile = BTreeMap::new();
    for (id, entry) in &old {
        if tracked_ids.contains(id) {
            next.insert(id.clone(), entry.clone());
        }
    }

    let mut claimed = Claimed::default();
    for child in children {
        let entry = next.get(&child.id).cloned().unwrap_or_default();
        let (event, updated) = decide(child, &entry, now_ms);
        let mut events: Vec<PingEvent> = event.into_iter().collect();
        // A child the sync DROPPED is never decided again — its record is gone
        // from the roster, so its cursor entry leaves the file on this same
        // pass — which means the exit `decide` would otherwise owe on a LATER
        // tick has to be claimed here, after the trace row the drop could
        // still produce. Local dropped children are skipped: their parent is
        // told by the run's own report, exactly as for a live one. The exit is
        // pushed only when `decide` did not already answer with one — a drop
        // whose window holds no readable record reaches `exit_event` through
        // `decide` itself, and pushing a second here would put TWO exits on a
        // ring that is supposed to close with exactly one.
        let already_exited = events.iter().any(|e| matches!(e, PingEvent::Exited { .. }));
        if child.dropped_mid_turn && child.remote && !already_exited {
            if let Some(exit) = exit_event(child, entry.exited) {
                events.push(exit);
            }
        }
        for event in events {
            // Where an event GOES is the child's own record's business: a
            // remote parent has no local transport, and this node's `deliver`
            // is that transport.
            if child.remote {
                claimed.spool.push(SpoolClaim {
                    child: child.id.clone(),
                    key: child.remote_key.clone(),
                    event,
                });
            } else {
                let tag = child_tag(child.petname.as_deref(), &child.id);
                claimed.lines.push(ChildClaim { parent: child.parent.clone(), line: render_line(&tag, &event) });
            }
        }
        // A child whose record is gone — the `died mid-turn` row, whose only
        // input is the sync's own dropped set — leaves the file on this same
        // pass: an entry naming a session nothing is tracking would only ever
        // re-decide a stale window.
        if (updated.seen.is_some() || updated.exited) && tracked_ids.contains(&child.id) {
            next.insert(child.id.clone(), updated);
        }
    }
    (claimed, old, next)
}

/// Read the cursor file; any unreadable/unparseable content reads as an EMPTY
/// cursor rather than failing the pass (the file is ours alone, and a torn
/// hand-edit may only ever cost one extra line — never a stuck sweep).
fn read_cursor() -> CursorFile {
    std::fs::read_to_string(pingback_path())
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// One child's whole decision: the event to publish (or none) and its own next
/// cursor entry. Pure over the already-read tail, so every row of the line
/// grammar is a table test.
fn decide(child: &Child, entry: &CursorEntry, now_ms: i64) -> (Option<PingEvent>, CursorEntry) {
    let lines: &[String] = child.lines.as_deref().unwrap_or(&[]);
    let tail: Vec<TraceRecord> = lines.iter().filter_map(|l| eidolon_trace_record(l)).collect();
    let (event, silent_at) = if tail.is_empty() {
        // No readable record: nothing is examined and any existing cursor
        // entry is left exactly as it was.
        (None, entry.silent_at.clone())
    } else {
        choose_event(
            child,
            lines,
            &tail,
            split_new(&tail, entry.seen.as_deref()),
            entry.silent_at.as_deref(),
            now_ms,
        )
    };
    // The child's own end is the LAST thing a parent hears, never the first:
    // an event the tail still holds is delivered (or spooled) first, and the
    // exit — which a remote parent's pull stops at — follows on a later tick.
    let (event, exited) = match (event, exit_event(child, entry.exited)) {
        (Some(event), _) => (Some(event), entry.exited),
        (None, Some(event)) => (Some(event), true),
        (None, None) => (None, entry.exited),
    };
    (
        event,
        CursorEntry { seen: tail.last().map(|r| r.id.clone()).or_else(|| entry.seen.clone()), silent_at, exited },
    )
}

/// The `Exited` event a child still owes, given whether its cursor has already
/// claimed one: only for a child whose parent is on another node (Q5's ruled
/// default — a local parent hears the run's own report instead), only once its
/// record says `done`, and only until the cursor's own latch has claimed it. A
/// record that stays on the roster after its run ended would otherwise
/// re-decide the same exit every twelve seconds, which is exactly what the
/// latch is for.
fn exit_event(child: &Child, claimed: bool) -> Option<PingEvent> {
    if !child.remote || claimed {
        return None;
    }
    let ended = child.ended.as_ref()?;
    Some(PingEvent::Exited { code: ended.code, outcome: ended.outcome.clone() })
}

/// The records after `seen` IN THE TAIL — everything in the tail when `seen`
/// is absent or no longer in the window (a tail that scrolled past it, a fresh
/// child): that is treated as new ONCE, and nothing is ever said about what
/// the window lost.
fn split_new<'a>(tail: &'a [TraceRecord], seen: Option<&str>) -> &'a [TraceRecord] {
    let Some(seen) = seen.filter(|s| !s.is_empty() && *s != "?") else {
        return tail;
    };
    match tail.iter().position(|r| r.id == seen) {
        Some(i) => &tail[i + 1..],
        None => tail,
    }
}

/// Choose the highest-priority new event, returning it beside the silence
/// latch to persist. The table is the design doc's — `exited` (P-RSA S8) is
/// the one row that comes from the RECORD rather than the trace, so it sits
/// after every trace row and takes the place of a silence line for a child
/// that has ended:
///
/// | priority | event | line |
/// |---|---|---|
/// | 1 | `TurnSettled` / `Cancelled` / dropped mid-turn | `settled …` / `cancelled …` / `died mid-turn …` |
/// | 2 | `AskUser{answer:null}` | `asking: "…"` |
/// | 3 | `TurnBudget{calls_left}` / `TurnDeadline{secs_left}` | `wrapping up · <n> calls left` |
/// | 4 | three or more `ToolResult{is_error:true}` in a row, ending the new records | `failing · <k> tool errors in a row · last: <tool label>` |
/// | 5 | the record says `done` (remote child only, latched) | `exited · exit <code>` / `exited · <outcome>` |
/// | 6 | no new record, turn open, ten minutes quiet | `silent <M> min · last: …` |
///
/// A `ToolResult{is_error:true}` among the new records rides any priority-1..3
/// line as ` · <k> tool errors`; tool errors alone below three in a row are
/// never a line of their own.
fn choose_event(
    child: &Child,
    lines: &[String],
    tail: &[TraceRecord],
    new: &[TraceRecord],
    silent_at: Option<&str>,
    now_ms: i64,
) -> (Option<PingEvent>, Option<String>) {
    let errors = new.iter().filter(|r| is_error(r)).count() as u64;
    let last_id = tail.last().map(|r| r.id.clone());

    // Priority 1: the turn's own end — settled or cancelled, whichever new
    // record is LAST (the most recent truth wins).
    if let Some(rec) = new.iter().rev().find(|r| r.kind == "TurnSettled" || r.kind == "Cancelled") {
        let calls = calls_in_turn(tail);
        let mins = mins_in_turn(tail);
        let say = say_of(&child.agent, lines);
        let event = if rec.kind == "TurnSettled" {
            let stop = str_field(rec.payload.as_ref(), "stop_reason");
            PingEvent::Settled { stop: (!stop.is_empty()).then_some(stop), calls, mins, say, errors }
        } else {
            PingEvent::Cancelled { calls, mins, say, errors }
        };
        return (Some(event), None);
    }

    // Priority 1, the third shape: the process went away with the turn still
    // open (its trace says so) — the one row that needs the sync's own
    // additive return.
    if child.dropped_mid_turn && turn_open(lines) {
        let event = PingEvent::DiedMidTurn {
            calls: calls_in_turn(tail),
            say: say_of(&child.agent, lines),
            errors,
        };
        return (Some(event), None);
    }

    // Priority 2: a prompt is open and nothing has answered it.
    if let Some(rec) = new
        .iter()
        .rev()
        .find(|r| r.kind == "AskUser" && !answered(r))
    {
        let prompt = str_field(rec.payload.as_ref(), "prompt");
        return (Some(PingEvent::Asking { prompt, errors }), None);
    }

    // Priority 3: the harness told it to wrap up.
    if let Some(rec) = new
        .iter()
        .rev()
        .find(|r| r.kind == "TurnBudget" || r.kind == "TurnDeadline")
    {
        let (calls_left, secs_left) = if rec.kind == "TurnBudget" {
            (numeric_field(rec.payload.as_ref(), "calls_left"), None)
        } else {
            (None, numeric_field(rec.payload.as_ref(), "secs_left"))
        };
        return (Some(PingEvent::WrappingUp { calls_left, secs_left, errors }), None);
    }

    // Priority 4: a run of tool errors, ending the new records.
    let run = trailing_error_run(new);
    if run >= ERROR_RUN {
        return (Some(PingEvent::Failing { run, tool: last_tool_label(tail) }), None);
    }

    // Priority 5: silence on an open turn — latched, one line per silence,
    // re-armed by any new record (every arm above returns `None` for the
    // latch, which IS the re-arm).
    if new.is_empty() && turn_open(lines) {
        if let (Some(last_id), Some(ts)) = (last_id, tail.last().and_then(|r| r.ts_ms)) {
            let quiet_ms = now_ms - ts;
            if quiet_ms >= SILENCE_MS && silent_at != Some(last_id.as_str()) {
                let last = last_tool_or_say(child, tail, lines);
                return (Some(PingEvent::Silent { mins: quiet_ms / 60_000, last }), Some(last_id));
            }
        }
    }

    (None, silent_at.map(str::to_string))
}

// ── the line ────────────────────────────────────────────────────────────

/// The event as ONE line. `tag` is the renderer's own — `[eidolon <petname>]`
/// here, the parent's ledger tag on the node that pulled the event — so an
/// event never names a box and this function needs nothing but the event.
/// Every segment is rebuilt from the event's own values, in the one order the
/// table above sets; nothing is parsed back out of a string.
fn render_line(tag: &str, event: &PingEvent) -> String {
    match event {
        PingEvent::Settled { stop, calls, mins, say, errors } => {
            let mut line = match stop {
                Some(stop) => format!("{tag} settled {}", quote(stop)),
                None => format!("{tag} settled"),
            };
            line.push_str(&turn_tail(*calls, *mins, say, *errors));
            line
        }
        PingEvent::Cancelled { calls, mins, say, errors } => {
            let mut line = format!("{tag} cancelled");
            line.push_str(&turn_tail(*calls, *mins, say, *errors));
            line
        }
        PingEvent::DiedMidTurn { calls, say, errors } => {
            let mut line = format!("{tag} died mid-turn");
            line.push_str(&turn_tail(*calls, None, say, *errors));
            line
        }
        PingEvent::Asking { prompt, errors } => {
            format!("{tag} asking: \"{}\"{}", quote(prompt), errors_note(*errors))
        }
        PingEvent::WrappingUp { calls_left, secs_left, errors } => {
            let mut line = format!("{tag} wrapping up");
            if let Some(left) = calls_left {
                line.push_str(&format!(" · {left} calls left"));
            }
            if let Some(left) = secs_left {
                line.push_str(&format!(" · {left} s left"));
            }
            line.push_str(&errors_note(*errors));
            line
        }
        PingEvent::Failing { run, tool } => {
            let mut line = format!("{tag} failing · {run} tool errors in a row");
            if let Some(label) = tool {
                line.push_str(&format!(" · last: {label}"));
            }
            line
        }
        PingEvent::Silent { mins, last } => {
            let mut line = format!("{tag} silent {mins} min");
            match last {
                Some(Last::Said(say)) => line.push_str(&format!(" · last: \"{}\"", quote(say))),
                Some(Last::Did(label)) => line.push_str(&format!(" · last: {label}")),
                None => {}
            }
            line
        }
        PingEvent::Exited { code, outcome } => {
            let mut line = format!("{tag} exited");
            if let Some(code) = code {
                line.push_str(&format!(" · exit {code}"));
            } else if let Some(outcome) = outcome.as_ref().filter(|o| !o.is_empty()) {
                line.push_str(&format!(" · {outcome}"));
            }
            line
        }
    }
}

/// The trailing segments a turn's-end line shares, in the one order they have
/// always had: the call count, the elapsed minutes, the last thing said, then
/// the tool errors riding along.
fn turn_tail(calls: Option<u64>, mins: Option<i64>, say: &Option<String>, errors: u64) -> String {
    let mut tail = String::new();
    if let Some(calls) = calls {
        tail.push_str(&format!(" · {calls} calls"));
    }
    if let Some(mins) = mins {
        tail.push_str(&format!(" · {mins} min"));
    }
    if let Some(say) = say {
        tail.push_str(&format!(" · last: \"{}\"", quote(say)));
    }
    tail.push_str(&errors_note(errors));
    tail
}

/// ` · <k> tool errors`, or nothing at all — a run of tool errors rides a
/// higher-priority line this way, and says nothing when there is none.
fn errors_note(k: u64) -> String {
    if k == 0 {
        String::new()
    } else {
        format!(" · {k} tool errors")
    }
}

// ── gathering ───────────────────────────────────────────────────────────

/// The trace path for a live child, through the harness CAPABILITY
/// (`TranscriptSpec::locate` — the same locator the reaper's transcript
/// refresh and `session trace` call; never `if agent == "eidolon"`), then
/// through `TranscriptSpec::trace`, the ONE trace reader. `None` for a
/// harness that keeps no trace and for a presence naming none. `log_path` is
/// the record's own journal, the locator's hint for a presence that a clean
/// exit already removed.
fn locate_trace(agent: &str, id: &str, cwd: &str, log_path: Option<&str>) -> Option<Vec<String>> {
    let profile = agent_profile(agent)?;
    let path = (profile.transcript.locate)(id, Some(cwd), log_path)?;
    read_trace(agent, &path)
}

/// Read `path` as one harness's trace tail. A trace that is not there, or a
/// path that is not a trace at all, is `None` — never an error that would void
/// this child's whole pass.
fn read_trace(agent: &str, path: &Path) -> Option<Vec<String>> {
    let read = agent_profile(agent)?.transcript.trace?;
    read(path)
}

// ── rendering ───────────────────────────────────────────────────────────

/// `[eidolon <petname>]` — the line's own tag. A record always carries a
/// petname (minted at insert), so the id fallback is for a hand-edited roster,
/// never the ordinary path.
fn child_tag(petname: Option<&str>, id: &str) -> String {
    match petname.filter(|p| !p.trim().is_empty()) {
        Some(name) => format!("[eidolon {name}]"),
        None => format!("[eidolon {id}]"),
    }
}

/// Untrusted child-authored text as ONE safe line (house rule 4): every
/// character [`super::common::is_unsafe`] refuses is stripped — control
/// characters (`\r` is an Enter at a headless parent's PTY, and whatever
/// follows it would start a fresh composer line) AND the Unicode `Cf` marks
/// (bidi overrides, zero-width joiners) that would otherwise reorder or hide
/// the line — whitespace flattened, clipped to [`SAY_MAX`] with `…`. EVERY
/// fragment the child wrote passes through here — the say, the prompt, the
/// stop reason, and the tool label alike — never only the ones the grammar
/// puts in quotes.
fn clean(s: &str) -> String {
    one_line_clip(&strip_unsafe(s), SAY_MAX)
}

/// [`clean`] as a quoted phrase: additionally never allowed to start with `/`
/// or `!` (which would read as a command or a shell escape at a parent's
/// prompt) — a leading space is the guard.
fn quote(s: &str) -> String {
    let clipped = clean(s);
    if clipped.starts_with('/') || clipped.starts_with('!') {
        format!(" {clipped}")
    } else {
        clipped
    }
}

/// A payload string field, [`clean`]ed; empty when absent/blank.
fn str_field(payload: Option<&Value>, key: &str) -> String {
    payload
        .and_then(|p| p.get(key))
        .and_then(Value::as_str)
        .map(clean)
        .unwrap_or_default()
}

/// A payload numeric field, whichever of the two spellings the journal used
/// (the sample contract writes numbers; nothing promises it stays one).
fn numeric_field(payload: Option<&Value>, key: &str) -> Option<String> {
    match payload?.get(key)? {
        Value::Number(n) => Some(n.to_string()),
        Value::String(s) if !s.trim().is_empty() => Some(one_line_clip(s, SAY_MAX)),
        _ => None,
    }
}

fn is_error(rec: &TraceRecord) -> bool {
    rec.kind == "ToolResult"
        && rec
            .payload
            .as_ref()
            .and_then(|p| p.get("is_error"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
}

fn answered(rec: &TraceRecord) -> bool {
    rec.payload
        .as_ref()
        .and_then(|p| p.get("answer"))
        .map(|a| !a.is_null())
        .unwrap_or(false)
}

fn tool_uses(rec: &TraceRecord) -> u64 {
    rec.payload
        .as_ref()
        .and_then(|p| p.get("content"))
        .and_then(Value::as_array)
        .map(|blocks| {
            blocks
                .iter()
                .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_use"))
                .count() as u64
        })
        .unwrap_or(0)
}

/// The index of the turn's own opening record — the LAST `UserMessage`/
/// `ExternalMessage` in the tail (a steer opens a turn as surely as a first
/// prompt does).
fn turn_open_index(tail: &[TraceRecord]) -> Option<usize> {
    tail.iter()
        .rposition(|r| r.kind == "UserMessage" || r.kind == "ExternalMessage")
}

/// How many `tool_use` blocks the turn has run — since the turn's opening
/// record, when that record is in the tail; `None` (segment omitted) when the
/// window has lost it.
fn calls_in_turn(tail: &[TraceRecord]) -> Option<u64> {
    let start = turn_open_index(tail)?;
    Some(tail[start + 1..].iter().map(tool_uses).sum())
}

/// How many whole minutes the turn has run, from that same opening record's
/// `ts_ms` to the tail's last record, when both carry one.
fn mins_in_turn(tail: &[TraceRecord]) -> Option<i64> {
    let start = turn_open_index(tail)?;
    let from = tail.get(start)?.ts_ms?;
    let to = tail.last()?.ts_ms?;
    Some((to - from).max(0) / 60_000)
}

/// The turn is still OPEN — the same fold the state rule uses
/// (`eidolon_state_from_trace`, the reconciler's own function): `working` or
/// `awaiting`, never a settled/cancelled end, never "unknown" (no evidence is
/// not evidence of an open turn).
fn turn_open(lines: &[String]) -> bool {
    matches!(eidolon_state_from_trace(lines), Some("working") | Some("awaiting"))
}

/// The child's latest words, through its own harness CAPABILITY
/// (`TranscriptSpec::say` — the very function that fills the roster's `say`
/// field), never a second reader of an assistant record.
fn say_of(agent: &str, lines: &[String]) -> Option<String> {
    let profile = agent_profile(agent)?;
    (profile.transcript.say)(lines, false).filter(|s| !s.trim().is_empty())
}

/// `<tool label>` — the last `ToolResult`'s own rendered line, through
/// `trace.rs`'s `tool_result_summary` (the renderer `session trace` shows for
/// that record; never a second formatter for it), then [`clean`]ed: a tool
/// result's first line is the least trusted text in the trace (a file the
/// child read, a page it fetched).
fn last_tool_label(tail: &[TraceRecord]) -> Option<String> {
    tail.iter()
        .rev()
        .find(|r| r.kind == "ToolResult")
        .map(|r| clean(&tool_result_summary(r.payload.as_ref(), if is_error(r) { "! " } else { "" })))
        .filter(|label| !label.is_empty())
}

/// What the child last DID or SAID — whichever of the two the tail holds most
/// recently, for the silence line. The two are told apart at the event level
/// ([`Last`]) rather than pre-formatted, so the quotes around a say are the
/// renderer's business on either node.
fn last_tool_or_say(child: &Child, tail: &[TraceRecord], lines: &[String]) -> Option<Last> {
    let tool = tail.iter().rposition(|r| r.kind == "ToolResult");
    let spoke = tail.iter().rposition(|r| r.kind == "AssistantMessage");
    match (tool, spoke) {
        (Some(t), Some(s)) if s > t => say_of(&child.agent, lines).map(Last::Said),
        (Some(_), _) => last_tool_label(tail).map(Last::Did),
        (None, Some(_)) => say_of(&child.agent, lines).map(Last::Said),
        (None, None) => None,
    }
}

/// How many `ToolResult{is_error:true}` records END the new records.
fn trailing_error_run(new: &[TraceRecord]) -> u64 {
    let mut run = 0;
    for rec in new.iter().rev() {
        if is_error(rec) {
            run += 1;
        } else {
            break;
        }
    }
    run
}

// ── delivery (the doorbell's way) ────────────────────────────────────────

/// Deliver ONE line to `parent`, raw — the doorbell's own transport selection
/// (P-M5c-3): a live Claude Code channel socket outranks the control-socket
/// PTY for ANY wrap and needs no submit keystroke; otherwise a HEADLESS wrap
/// takes [`write_delivery`] plus the wrap's own profile submit key; an
/// interactive wrap with no channel is skipped (`interactive-composer`) — the
/// same ban a ring holds, since this is likewise a raw keystroke with no
/// human's intent behind it. Every rejection is a NAMED skip, counted in the
/// report, never a silent drop.
///
/// `gate` is the audit label this lane's delivery carries — `autogate-child`
/// for a line the local claim decided, `autogate-child remote` for one a pull
/// brought back from another node (the two are one rule, and the log says
/// which wire it came in on).
fn deliver(
    line: &str,
    parent: &str,
    roster: &[SessionRecord],
    inv: &Invocation,
    gate: &str,
) -> Result<(), String> {
    let Some(rec) = roster.iter().find(|s| s.session_id == parent) else {
        return Err("no-parent-record".to_string());
    };
    if let Some(reason) = unreceptive(rec) {
        return Err(reason.to_string());
    }

    let payload = format!("{line}\n");
    let channel = connect_for_ring(channel_socket_path(parent))
        .ok()
        .map(|stream| write_channel(stream, payload.as_bytes()));

    let wrote = match channel {
        Some(result) => result,
        None if rec.headless => {
            let socket = rec.socket.as_deref().unwrap_or_default();
            let profile = profile_for_agent(&rec.agent);
            (|| -> std::io::Result<()> {
                let mut stream = connect_for_ring(socket)?;
                write_delivery(
                    &mut stream,
                    payload.as_bytes(),
                    true,
                    profile.submit_key,
                    SUBMIT_KEYSTROKE_DELAY,
                )
            })()
        }
        None => return Err("interactive-composer".to_string()),
    };

    match wrote {
        Ok(()) => {
            // One audit line per delivery, through the SAME helper `send`
            // uses, with this lane's own gate label. The line itself (which
            // carries quoted model output) rides as `untrusted_data`, never as
            // the audit message.
            audit_send(inv, "delivered", &format!("delivered to `{parent}` ({gate})"), line);
            Ok(())
        }
        Err(_) => Err("write-failed".to_string()),
    }
}

/// Why a record cannot take a line at all, named once so a caller that means
/// to WRITE to it (the pull, which would otherwise spend a remote request on a
/// target that can never receive) and the delivery itself cannot disagree
/// about who is a recipient. `deliver`'s own "no-parent-record" is not here:
/// there is no record to judge in that case.
///
/// Never a shell parent: a line reaching a bare shell's input is a COMMAND
/// LINE, and it would run. A record naming no registered harness profile is
/// the same shape `profile_for_agent` itself falls back for.
fn unreceptive(rec: &SessionRecord) -> Option<&'static str> {
    if matches!(rec.agent.as_str(), "" | "shell") || agent_profile(&rec.agent).is_none() {
        return Some("shell-parent");
    }
    if !super::doc::is_conductable_now(rec) {
        return Some("not-conductable");
    }
    if canonical_state(&rec.state) == "done" {
        return Some("parent-done");
    }
    None
}

// ── the pull (the parent's own side) ─────────────────────────────────────

/// `pingback_pull`: for every child THIS node spawned on another node, ask
/// that node's door for the events the child published for it, render them
/// here, and deliver them to the parent through the same [`deliver`] the local
/// lane uses (P-RSA S9, CONTRACTS.md §4/§6, the lane brief's §4.4).
///
/// **A pull, because the far node cannot push.** The spawn proved one
/// direction of reachability (this node dials the child's, with a signature
/// and a via the pairing ceremony committed to); nothing proves the reverse.
/// So the parent's own node — the policy and audit boundary this module
/// already sits on — decides and delivers, and the child's node never writes
/// into anyone's composer.
///
/// **The ledger is the whole candidate set.** [`aoide_storage::remote_children`]
/// holds one row per spawn, keyed by the child's identity, with the cursor of
/// what has already been delivered. Only a row whose `parentSessionId` is a
/// LIVE local session (and [`unreceptive`] admits) is pulled, and never again
/// once its row is [`drained`](aoide_storage::remote_children::RemoteChild::drained).
///
/// **The tunnel key is the PARENT's session id**, so the ssh forward this
/// reuses is the parent's own and `close_all_for_session` closes it with the
/// session. The daemon holds no forward of its own — a standing
/// `pid-<aoided pid>` forward is exactly what PAIRING.md's Transport section
/// forbids.
///
/// **At-most-once.** The cursor advances BEFORE any line is delivered, so a
/// crash (or a failed write, or a skipped target) between the two loses a line
/// rather than repeating one — the direction the whole lane loses in, and the
/// same one the child-side claim holds. A failed pull is quiet: no line, no
/// retry storm, and the next tick asks again from the same cursor (the brief
/// names no backoff, so there is none — one pull per tick, per child).
pub(crate) fn pingback_pull(inv: &Invocation) -> PingbackReport {
    pingback_pull_with(inv, fetch_history)
}

/// [`pingback_pull`]'s body, parameterised over its one effect — the wire fetch
/// — so the whole pass (the candidate filter, the re-validation, the cursor,
/// the gap marker, the delivery) is provable with no node, no socket and no
/// wire. The same split `view.rs`'s `watch_remote_with` holds over its frame
/// fetch.
fn pingback_pull_with(
    inv: &Invocation,
    fetch: impl Fn(&aoide_storage::node_store::Node, &str, u64, &str) -> Result<RingRead, FrameReadError>,
) -> PingbackReport {
    let mut report = PingbackReport::default();
    if inv.door != Door::Daemon {
        return report;
    }
    let ledger = aoide_storage::remote_children::load_remote_children();
    if ledger.is_empty() {
        return report;
    }
    let roster: Vec<SessionRecord> = load_stage::<SessionsFile>(&sessions_path())
        .map(|f| f.sessions)
        .unwrap_or_default();
    let nodes = aoide_storage::node_store::load_nodes();

    for entry in ledger.iter().filter(|e| !e.drained) {
        let parent = entry.parent_session_id.as_str();
        // The target is judged BEFORE the far node is asked for anything: a
        // session that can never receive a line must not cost a request, and
        // — since the cursor advances before delivery — must never consume
        // the events it would not have shown.
        let verdict = match roster.iter().find(|s| s.session_id == parent) {
            None => Some("no-parent-record"),
            Some(rec) => unreceptive(rec),
        };
        if let Some(reason) = verdict {
            report.skipped.push((parent.to_string(), reason.to_string()));
            continue;
        }
        // By KEY, never by label: `node` is the display name known at spawn
        // time, and a rename must not break a pull while a re-pair must not
        // silently dial a node the child does not live on.
        let Some(node) = node_of(&nodes, entry) else {
            report.skipped.push((parent.to_string(), "unknown-node".to_string()));
            continue;
        };
        let read = match fetch(node, &entry.session_id, entry.lines_after, parent) {
            Ok(read) => read,
            Err(e) => {
                let why = clean_line(&e.message);
                // A PERMANENT answer: the far node holds neither a record nor a
                // ring for this child, so it is gone for good and every later
                // tick would ask the same question forever — unbounded audit
                // growth and one wasted request per tick. The row is latched
                // exactly as a drained exit latches it. A refusal is NOT this
                // (a wrong key may be a re-pair the operator can fix), and
                // neither is a transport failure: both stay retryable.
                if e.code == Some(aoide_protocol::wire::a2a::TASK_NOT_FOUND_CODE) {
                    eprintln!(
                        "[aoide/reap] remote ping-back child `{}`/{} is gone; latched and never pulled again",
                        node.name, entry.session_id
                    );
                    audit_pull(
                        inv,
                        "child-gone",
                        &format!(
                            "remote ping-back child `{}`/{} is gone; row latched",
                            node.name, entry.session_id
                        ),
                    );
                    if let Err(e) = aoide_storage::remote_children::mark_drained(&entry.key, &entry.session_id) {
                        eprintln!(
                            "[aoide/reap] remote ping-back drain latch failed for `{}`/{}: {e}",
                            node.name, entry.session_id
                        );
                    }
                    report.skipped.push((parent.to_string(), "child-gone".to_string()));
                    continue;
                }
                eprintln!(
                    "[aoide/reap] remote ping-back pull from `{}`/{} failed: {why}",
                    node.name, entry.session_id
                );
                audit_pull(inv, "pull-failed", &format!(
                    "remote ping-back pull from `{}`/{} failed: {why}",
                    node.name, entry.session_id
                ));
                report.skipped.push((parent.to_string(), "pull-failed".to_string()));
                continue;
            }
        };

        // The cursor advances FIRST, and the claim is ATOMIC with the read the
        // events are then filtered against (M2): the value this pass may
        // deliver from is whatever the ledger holds at claim time, so a second
        // pass that fetched the same window — the daemon's loop and a
        // `session reap` re-entering through a connection thread overlap by
        // design — delivers nothing, rather than everything twice.
        //
        // It advances to whatever the answer CARRIED: the newest event in it,
        // or — for a `gap` the ring cannot hand over an event for — the newest
        // `seq` the child ever pushed. `last` is trusted only where the ring's
        // own doc says it may be, i.e. under that `gap`: without one, an
        // honest ring cannot report `last` beyond the events it sent, and
        // believing a peer's number there would drive the cursor to a value no
        // event can ever exceed — the child silent forever, the row never
        // latched (M1).
        let next = read
            .events
            .last()
            .map(|e| e.seq)
            .unwrap_or(if read.gap { read.last } else { entry.lines_after });
        let from = match aoide_storage::remote_children::claim_lines_after(
            &entry.key,
            &entry.session_id,
            next,
        ) {
            Ok(Some(from)) => from,
            // The row left the ledger between this pass's read and its claim:
            // there is nothing to deliver against, and nothing to say.
            Ok(None) => continue,
            // The advance could not be written, so nothing may be delivered:
            // with the cursor where it was, the same events would come back
            // next tick and the line would land twice. A lost line is this
            // lane's safe direction.
            Err(e) => {
                eprintln!(
                    "[aoide/reap] remote ping-back cursor write failed for `{}`/{}: {e}",
                    node.name, entry.session_id
                );
                audit_pull(
                    inv,
                    "cursor-failed",
                    &format!("remote ping-back cursor write failed for `{}`/{}", node.name, entry.session_id),
                );
                report.skipped.push((parent.to_string(), "cursor-failed".to_string()));
                continue;
            }
        };

        let tag = pull_tag(&node.name, &entry.session_id);
        let mut lines: Vec<String> = Vec::new();
        let mut exited = false;
        for carried in &read.events {
            let Some(event) = ping_event_of(&carried.event) else {
                continue;
            };
            // The exit is read off the WHOLE answer, not only the events this
            // pass still owes: a ring keeps its closing event forever, and a
            // pass that finds it already claimed (an overlapping pass took it)
            // is exactly the pass that should latch the row.
            exited |= matches!(event, PingEvent::Exited { .. });
            if carried.seq <= from {
                continue;
            }
            lines.push(render_line(&tag, &event));
        }
        // One honest marker, and only when the claim left a hole to name: a
        // `gap` whose events are all already claimed costs the parent nothing.
        let missed = gap_missed(&read, from);
        if read.gap && missed > 0 {
            lines.insert(0, gap_line(&tag, missed));
        }
        for line in lines {
            match deliver(&line, parent, &roster, inv, "autogate-child remote") {
                Ok(()) => {
                    eprintln!("[aoide/reap] ping-back (remote) → {parent}: {line}");
                    report.delivered.push((parent.to_string(), line));
                }
                Err(reason) => report.skipped.push((parent.to_string(), reason)),
            }
        }
        // The child is over and its last event has been drained: nothing else
        // will ever be on its ring, so stop asking. Only a DRAINED exit stops
        // it — a pull that failed above never reaches this line.
        if exited {
            if let Err(e) = aoide_storage::remote_children::mark_drained(&entry.key, &entry.session_id) {
                eprintln!(
                    "[aoide/reap] remote ping-back drain latch failed for `{}`/{}: {e}",
                    node.name, entry.session_id
                );
            }
        }
    }
    report
}

/// The wire fetch a pull performs: the ring read's JSON, deserialized into the
/// ONE type both ends of this wire share (`RingRead` serializes field for field
/// what the door's `history` message carries). A body that parses as JSON-RPC
/// but not as a ring read is a failure with no code — the far side is a peer,
/// and a peer answering a shape this version cannot read is named as that,
/// never rendered as an empty ring (an empty ring is a real answer).
///
/// The failure is a [`FrameReadError`] and never a `String` (H2 of the S8/S9
/// review): the door's own CODE is what tells this side a permanent answer
/// (`TASK_NOT_FOUND_CODE` — no record and no ring, the child is gone) from a
/// transient one, and a flattened message would throw exactly that away.
fn fetch_history(
    node: &aoide_storage::node_store::Node,
    id: &str,
    after: u64,
    tunnel_key: &str,
) -> Result<RingRead, FrameReadError> {
    let value = aoide_client::commands::task_history_on_node(node, id, after, tunnel_key)?;
    serde_json::from_value::<RingRead>(value).map_err(|e| FrameReadError {
        code: None,
        message: format!("the far node's ping-back read did not parse: {e}"),
    })
}

/// Which registered node holds this row's child — by the row's KEY, which is
/// the far node's verifying pubkey, and never by its label (`current_node_name`
/// has the display half of this same rule).
fn node_of<'a>(
    nodes: &'a [aoide_storage::node_store::Node],
    entry: &aoide_storage::remote_children::RemoteChild,
) -> Option<&'a aoide_storage::node_store::Node> {
    if entry.key.is_empty() {
        return None;
    }
    nodes
        .iter()
        .find(|n| n.pubkey.as_deref().is_some_and(|k| k.eq_ignore_ascii_case(&entry.key)))
}

/// The tag a PULLED line carries: `[<node>/<child>]`. The local lane's tag
/// names the harness and the child (`child_tag`); this one names the BOX and
/// the child, because that is what the parent cannot otherwise know — the far
/// node supplies neither (its events carry no tag at all). Both halves are
/// re-cleaned: the label is this node's own, but the child's id came back in
/// the far node's spawn ack and is peer text all the way (house rule 4).
fn pull_tag(node: &str, child: &str) -> String {
    format!("[{}/{}]", clean_line(node), clean_line(child))
}

/// How many events the ring lost between the cursor this pull asked from and
/// the oldest event it got back. Exact, from the ring's own contiguity: every
/// push is consecutive and only the oldest leaves, so the missing run is
/// `cursor+1 ..= oldest.seq` — or, for a `gap` whose answer carries no event
/// at all, the whole `cursor+1 ..= last`.
fn gap_missed(read: &RingRead, cursor: u64) -> u64 {
    match read.events.first() {
        Some(oldest) => oldest.seq.saturating_sub(cursor.saturating_add(1)),
        None => read.last.saturating_sub(cursor),
    }
}

/// The one line a `gap` is worth: an honest marker, never a fabricated event.
/// It is built HERE, from numbers this node already holds — nothing in it
/// comes from the far node's text — so a rolled ring costs the parent one line
/// that tells the truth about what it did not get.
fn gap_line(tag: &str, missed: u64) -> String {
    let noun = if missed == 1 { "event" } else { "events" };
    format!("{tag} ping-back gap · {missed} {noun} lost before this point")
}

/// A peer-published event, re-validated: the CLOSED vocabulary decides (an
/// unknown kind does not deserialize — `None`, dropped, never guessed at), and
/// every string it carries is re-cleaned with [`clean_line`] and clipped to
/// [`SAY_MAX`] before the local renderer touches it.
///
/// **Why clean again, when the sender already did.** The event crossed a
/// wire: this node renders it into a parent's composer, and no peer's bytes
/// reach that line on another node's word for its own sanitizing. The rule
/// applied is the SAME one the sender uses ([`reclean`] reaches the identical
/// [`strip_unsafe`]/[`SAY_MAX`] pair), which is what makes the second pass
/// cheap to trust — re-cleaning an already-clean field changes nothing, and a
/// field a compromised or older peer sent raw does not get through.
fn ping_event_of(event: &Value) -> Option<PingEvent> {
    let event: PingEvent = serde_json::from_value(event.clone()).ok()?;
    Some(reclamp(event))
}

/// [`ping_event_of`]'s re-cleaning half: every string field of a closed event,
/// through [`reclean`]. Numbers and the enumeration itself are already typed —
/// there is nothing to clean in a `u64` — so this is exactly the string set,
/// named field by field rather than walked, because the enum is CLOSED and a
/// new variant must be given a decision here (the compiler says so at the
/// `match`, which is the point).
fn reclamp(event: PingEvent) -> PingEvent {
    let one = |s: Option<String>| s.map(|s| reclean(&s));
    match event {
        PingEvent::Settled { stop, calls, mins, say, errors } => {
            PingEvent::Settled { stop: one(stop), calls, mins, say: one(say), errors }
        }
        PingEvent::Cancelled { calls, mins, say, errors } => {
            PingEvent::Cancelled { calls, mins, say: one(say), errors }
        }
        PingEvent::DiedMidTurn { calls, say, errors } => {
            PingEvent::DiedMidTurn { calls, say: one(say), errors }
        }
        PingEvent::Asking { prompt, errors } => PingEvent::Asking { prompt: reclean(&prompt), errors },
        PingEvent::WrappingUp { calls_left, secs_left, errors } => {
            PingEvent::WrappingUp { calls_left: one(calls_left), secs_left: one(secs_left), errors }
        }
        PingEvent::Failing { run, tool } => PingEvent::Failing { run, tool: one(tool) },
        PingEvent::Silent { mins, last } => {
            let last = last.map(|last| match last {
                Last::Said(say) => Last::Said(reclean(&say)),
                Last::Did(label) => Last::Did(reclean(&label)),
            });
            PingEvent::Silent { mins, last }
        }
        PingEvent::Exited { code, outcome } => PingEvent::Exited { code, outcome: one(outcome) },
    }
}

/// A peer-sent string, made safe for the line it becomes: [`strip_unsafe`]'s
/// own strip (control characters and every Unicode `Cf`) clipped to this
/// module's [`SAY_MAX`], and never free to start with `/` or `!` — the guard
/// [`quote`] puts on a local fragment, applied here as well because a BARE
/// segment (a tool label, a wrap-up bound, an exit outcome) is never quoted by
/// [`render_line`]. Prepending a space to a QUOTED field is a no-op: `quote`
/// cleans first, and cleaning trims.
fn reclean(s: &str) -> String {
    let cleaned = clip_flat(&strip_unsafe(s), SAY_MAX);
    if cleaned.starts_with('/') || cleaned.starts_with('!') {
        format!(" {cleaned}")
    } else {
        cleaned
    }
}

/// One audit line per pull failure, in `audit_resurrect`'s shape (this pass
/// answers to the reaper's own tick, so that is the command it logs under).
/// The reason can carry a peer's own bytes, so the caller [`clean_line`]s it
/// before it gets here — the audit log is not a place for a node's raw text
/// either, and the message bound is `append_audit`'s own.
fn audit_pull(inv: &Invocation, status: &str, message: &str) {
    let _ = aoide_protocol::append_audit(
        &aoide_protocol::audit_log_path(inv),
        &aoide_protocol::AuditRecord {
            ts: super::conduct::unix_ts(),
            door: inv.door,
            class: aoide_protocol::EventClass::Audit,
            command: "session.reap".to_string(),
            status: status.to_string(),
            message: message.to_string(),
            untrusted_data: None,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::conduct::conduct_socket_path;
    use crate::graph::model::{load_stage, sessions_path, write_stage, SessionsFile};
    use crate::graph::session_store::{do_session_start, stamp_headless};
    use crate::graph::testutil::*;
    use std::io::Read as _;
    use std::os::unix::net::UnixListener;

    // The design doc's own sample lines, verbatim (EIDOLON-TRACE.md's "Line"
    // block) — the same fixtures `protocol/agents.rs`, `graph/eidolon.rs` and
    // `graph/trace.rs` pin their own halves against.
    const START: &str = r#"{"id":0,"parent":null,"ts_ms":1789603005561,"kind":{"SessionStart":{"model":"ollama:deepseek-v4.1-flash","cwd":"/home/khoa/Aoide","system":null}}}"#;
    const USER: &str = r##"{"id":1,"parent":0,"ts_ms":1789603005570,"kind":{"UserMessage":{"role":"user","content":[{"type":"text","text":"# Brief A: …"}]}}}"##;
    const ASSISTANT: &str = r#"{"id":2,"parent":1,"ts_ms":1789603009102,"kind":{"AssistantMessage":{"role":"assistant","content":[{"type":"thinking","thinking":"…","signature":"…"},{"type":"text","text":"Let me read the slot catalog first."},{"type":"tool_use","id":"call_8vr43zri","name":"read","input":"{\"path\":\"a\"}"}]}}}"#;
    const RESULT: &str = r#"{"id":3,"parent":2,"ts_ms":1789603009140,"kind":{"ToolResult":{"tool_use_id":"call_8vr43zri","content":"     1\t# Per-song widget slots\nmore","is_error":false}}}"#;
    const RESULT_ERR: &str = r#"{"id":4,"parent":3,"ts_ms":1789603009200,"kind":{"ToolResult":{"tool_use_id":"call_9","content":"ENOENT: no such file\nmore","is_error":true}}}"#;
    const SETTLED: &str = r#"{"id":131,"parent":130,"ts_ms":1789606421000,"kind":{"TurnSettled":{"stop_reason":"end_turn","usage":{"input_tokens":9570000,"output_tokens":71900}}}}"#;
    const CANCELLED: &str = r#"{"id":77,"parent":76,"ts_ms":1789626990000,"kind":"Cancelled"}"#;
    const ASK: &str = r#"{"id":40,"parent":39,"ts_ms":1789626500000,"kind":{"AskUser":{"call_id":"call_x","prompt":"Overwrite?","answer":null}}}"#;
    const BUDGET: &str = r#"{"id":120,"parent":119,"ts_ms":1789606380000,"kind":{"TurnBudget":{"calls_left":8}}}"#;
    const DEADLINE: &str = r#"{"id":121,"parent":120,"ts_ms":1789606380500,"kind":{"TurnDeadline":{"secs_left":30}}}"#;
    const EXTERNAL: &str = r#"{"id":55,"parent":54,"ts_ms":1789626700000,"kind":{"ExternalMessage":{"from":"orchestrator","channel":null,"text":"STOP: write the report now"}}}"#;

    fn lines(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    /// A child as the gather would hand it over: petname `brave-otter`, agent
    /// `eidolon`, parent `wrap-1`, and the given trace body — a LOCAL child
    /// (its parent is on this node), still running.
    fn child_of(body: &[&str]) -> Child {
        Child {
            id: "user-0001".to_string(),
            petname: Some("brave-otter".to_string()),
            agent: "eidolon".to_string(),
            parent: "wrap-1".to_string(),
            lines: Some(lines(body)),
            dropped_mid_turn: false,
            remote: false,
            remote_key: String::new(),
            ended: None,
        }
    }

    /// [`super::decide`], with the one line its event renders as — the tag is
    /// the child's own, exactly as the claim section builds it. The tests read
    /// LINES (that is the grammar they pin), so the seam underneath becoming
    /// an event changes nothing they assert.
    fn decide(child: &Child, entry: &CursorEntry, now: i64) -> (Option<String>, CursorEntry) {
        let (event, entry) = super::decide(child, entry, now);
        let line = event.map(|e| render_line(&child_tag(child.petname.as_deref(), &child.id), &e));
        (line, entry)
    }

    /// [`super::decide`]'s event itself, for the tests that are about the
    /// event (its split from the line, its wire shape).
    fn event_of(body: &[&str], seen_id: Option<&str>) -> Option<PingEvent> {
        let child = child_of(body);
        let entry = CursorEntry { seen: seen_id.map(str::to_string), ..Default::default() };
        super::decide(&child, &entry, last_ts(body)).0
    }

    /// Decide one body with `seen` already past `seen_id` — the pure seam the
    /// whole line grammar is tested through. "Now" is anchored to the tail's
    /// OWN last record, so the silence row never fires by accident: a test
    /// that wants silence asks for it explicitly through [`plan_at`].
    fn plan(body: &[&str], seen_id: Option<&str>) -> (Option<String>, CursorEntry) {
        plan_at(body, seen_id, last_ts(body))
    }

    fn plan_at(body: &[&str], seen_id: Option<&str>, now: i64) -> (Option<String>, CursorEntry) {
        let child = child_of(body);
        let entry = CursorEntry { seen: seen_id.map(str::to_string), ..Default::default() };
        decide(&child, &entry, now)
    }

    /// The tail's own last readable `ts_ms` — the anchor every non-silence
    /// expectation is measured from.
    fn last_ts(body: &[&str]) -> i64 {
        lines(body)
            .iter()
            .filter_map(|l| eidolon_trace_record(l))
            .filter_map(|r| r.ts_ms)
            .next_back()
            .unwrap_or(0)
    }

    fn now_ms() -> i64 {
        (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()) as i64
    }

    // ── the line grammar, pure ───────────────────────────────────────────

    #[test]
    fn every_row_of_the_table_renders_its_own_one_line() {
        // TurnSettled → settled, with calls/minutes since the opening prompt
        // and the last thing said.
        let (line, entry) = plan(&[USER, ASSISTANT, RESULT, SETTLED], None);
        let line = line.expect("a settled turn is a line");
        assert!(line.starts_with("[eidolon brave-otter] settled end_turn"), "{line}");
        assert!(line.contains("1 calls"), "one tool_use since the prompt: {line}");
        assert!(line.contains("56 min"), "prompt 1789603005570 → settle 1789606421000: {line}");
        assert!(line.contains("last: \"Let me read the slot catalog first.\""), "{line}");
        assert_eq!(entry.seen.as_deref(), Some("131"), "the cursor advances to the tail's last id");
        assert_eq!(entry.silent_at, None);

        // Cancelled → its own line, same trailing segments.
        let (line, _) = plan(&[USER, ASSISTANT, CANCELLED], None);
        let line = line.expect("a cancel is a line");
        assert!(line.starts_with("[eidolon brave-otter] cancelled"), "{line}");
        assert!(line.contains("1 calls"), "{line}");
        assert!(line.contains("last: \"Let me read the slot catalog first.\""), "{line}");

        // AskUser{answer:null} → asking.
        let (line, _) = plan(&[USER, ASK], None);
        assert_eq!(line.unwrap(), "[eidolon brave-otter] asking: \"Overwrite?\"");

        // TurnBudget / TurnDeadline → wrapping up.
        let (line, _) = plan(&[USER, BUDGET], None);
        assert_eq!(line.clone().unwrap(), "[eidolon brave-otter] wrapping up · 8 calls left");
        let (line, _) = plan(&[USER, DEADLINE], None);
        assert_eq!(line.unwrap(), "[eidolon brave-otter] wrapping up · 30 s left");

        // Three errors in a row, ending the new records → failing.
        let (line, _) = plan(&[USER, RESULT_ERR, RESULT_ERR, RESULT_ERR], None);
        let line = line.expect("three in a row is a line");
        assert!(line.starts_with("[eidolon brave-otter] failing · 3 tool errors in a row"), "{line}");
        assert!(line.contains("last: ! ENOENT: no such file"), "the rendered tool result: {line}");

        // Silence on an open turn → silent, with the latch set.
        let now = now_ms();
        let old = format!(
            r#"{{"id":7,"parent":6,"ts_ms":{},"kind":{{"AssistantMessage":{{"content":[{{"type":"text","text":"thinking it over"}}]}}}}}}"#,
            now - 11 * 60 * 1000
        );
        let (line, entry) = plan_at(&[USER, old.as_str()], Some("7"), now);
        let line = line.expect("ten quiet minutes is a line");
        assert!(line.starts_with("[eidolon brave-otter] silent 11 min"), "{line}");
        assert!(line.contains("last: \"thinking it over\""), "{line}");
        assert_eq!(entry.silent_at.as_deref(), Some("7"), "the silence latch is set");
    }

    #[test]
    fn a_dropped_child_with_an_open_turn_reads_died_mid_turn() {
        let mut child = child_of(&[USER, ASSISTANT]);
        child.dropped_mid_turn = true;
        let (line, entry) = decide(&child, &CursorEntry::default(), now_ms());
        let line = line.expect("a death with the turn open is a line");
        assert!(line.starts_with("[eidolon brave-otter] died mid-turn"), "{line}");
        assert!(line.contains("1 calls"), "{line}");
        assert!(line.contains("last: \"Let me read the slot catalog first.\""), "{line}");
        assert_eq!(entry.seen.as_deref(), Some("2"));

        // ...but a child whose trace says the turn ENDED is not "mid-turn",
        // and its new settled record is already the priority-1 line.
        let mut closed = child_of(&[USER, SETTLED]);
        closed.dropped_mid_turn = true;
        let (line, _) = decide(&closed, &CursorEntry::default(), now_ms());
        assert!(line.unwrap().contains("settled"), "a settled death is not a mid-turn death");
        let mut silent_death = child_of(&[SETTLED]);
        silent_death.dropped_mid_turn = true;
        let (line, _) = decide(
            &silent_death,
            &CursorEntry { seen: Some("131".to_string()), ..Default::default() },
            now_ms(),
        );
        assert_eq!(line, None, "a closed turn's death is not a line");
    }

    #[test]
    fn the_highest_priority_new_event_wins_and_the_last_of_a_tie() {
        // Several events new at once: settled (1) beats asking (2), wrapping
        // (3), the error run (4) and silence (5).
        let (line, _) = plan(&[USER, BUDGET, ASK, RESULT_ERR, SETTLED], None);
        assert!(line.unwrap().starts_with("[eidolon brave-otter] settled"), "priority 1 wins");

        // asking beats wrapping up.
        let (line, _) = plan(&[USER, ASK, BUDGET], None);
        assert!(line.unwrap().contains("asking: \"Overwrite?\""));

        // wrapped up beats a failing run.
        let (line, _) = plan(&[USER, BUDGET, RESULT_ERR, RESULT_ERR, RESULT_ERR], None);
        assert!(line.unwrap().contains("wrapping up"));

        // Two priority-1 records new in the same tick: the LAST one is the
        // turn's most recent truth.
        let (line, _) = plan(&[USER, CANCELLED, SETTLED], None);
        assert!(line.unwrap().contains("settled"), "the last priority-1 record wins");

        // A turn already seen as settled, then cancelled: the cancel is the
        // new event.
        let (line, _) = plan(&[USER, SETTLED, CANCELLED], Some("131"));
        assert!(line.unwrap().contains("cancelled"));
    }

    #[test]
    fn tool_errors_ride_a_higher_line_and_never_fire_alone_below_three() {
        // One error among the new records rides the settled line.
        let (line, _) = plan(&[USER, ASSISTANT, RESULT_ERR, SETTLED], None);
        assert!(line.unwrap().ends_with(" · 1 tool errors"), "the error rides along");

        // Two errors ride an asking line the same way.
        let (line, _) = plan(&[USER, RESULT, RESULT_ERR, RESULT_ERR, ASK], None);
        let line = line.unwrap();
        assert!(line.contains("asking: \"Overwrite?\""), "{line}");
        assert!(line.ends_with(" · 2 tool errors"), "{line}");

        // Below three in a row, with nothing else new: NOT a line.
        assert_eq!(plan(&[USER, RESULT_ERR, RESULT_ERR, ASSISTANT], Some("2")).0, None);
        assert_eq!(plan(&[USER, RESULT_ERR], Some("1")).0, None, "one error alone is not a line");

        // A run broken by another record is not a run.
        assert_eq!(plan(&[USER, RESULT_ERR, RESULT_ERR, RESULT_ERR, RESULT_ERR, RESULT, ASSISTANT], Some("2")).0, None);
    }

    #[test]
    fn quoted_text_is_one_line_clipped_and_never_starts_a_command() {
        // 200 chars of quoted output clips to SAY_MAX with an ellipsis.
        let long = format!(
            r#"{{"id":5,"parent":4,"ts_ms":1000,"kind":{{"AskUser":{{"prompt":"{}","answer":null}}}}}}"#,
            "x".repeat(200)
        );
        let (line, _) = plan(&[USER, long.as_str()], Some("1"));
        let line = line.unwrap();
        assert!(line.contains('…'), "clipped: {line}");
        let quoted = line.split('"').nth(1).unwrap();
        assert_eq!(quoted.chars().count(), SAY_MAX, "exactly SAY_MAX chars inside the quotes");

        // A leading `/` or `!` is a command/escape at a parent's prompt — the
        // quote is prefixed with a space instead.
        for raw in ["/compact keep only records", "!rm -rf /tmp/x"] {
            let escaped = raw.replace('"', "'");
            let json = format!(
                r#"{{"id":5,"parent":4,"ts_ms":1000,"kind":{{"AskUser":{{"prompt":"{escaped}","answer":null}}}}}}"#
            );
            let (line, _) = plan(&[USER, json.as_str()], Some("1"));
            let line = line.unwrap();
            assert!(line.contains(&format!("\" {raw}\"")), "leading space guards it: {line}");
        }

        // Control characters (an escape sequence an agent printed) never reach
        // the parent's composer.
        let noisy = r#"{"id":5,"parent":4,"ts_ms":1000,"kind":{"AskUser":{"prompt":"a\u001b[31mb\u0007c","answer":null}}}"#;
        let (line, _) = plan(&[USER, noisy], Some("1"));
        let line = line.unwrap();
        assert!(!line.contains('\u{1b}') && !line.contains('\u{7}'), "{line:?}");
        assert!(line.contains("a[31mbc"), "{line:?}");
    }

    #[test]
    fn a_bidi_or_zero_width_mark_in_a_local_line_is_stripped() {
        // `is_control` does NOT cover Unicode `Cf`: a bidi override (U+202E)
        // or a zero-width space (U+200B) in a child's own text survives a
        // control-only filter and reorders what the parent reads — the exact
        // trick that turned a prompt of `/compact` into the local line's
        // quoted say. Both ride a LOCAL trace here: no wire involved.
        let hostile = r#"{"id":5,"parent":4,"ts_ms":1000,"kind":{"AskUser":{"prompt":"\u202egpj.exe\u200b --version","answer":null}}}"#;
        let (line, _) = plan(&[USER, hostile], Some("1"));
        let line = line.unwrap();
        assert_eq!(line, "[eidolon brave-otter] asking: \"gpj.exe --version\"", "{line:?}");
        assert!(
            !line.chars().any(super::super::common::is_unsafe),
            "no unsafe character survives a local line either: {line:?}"
        );

        // The same for a BARE segment the renderer never quotes, and for the
        // stop reason a Settled line quotes.
        let label = r#"{"id":9,"parent":8,"ts_ms":1789603009300,"kind":{"ToolResult":{"tool_use_id":"call_p","content":"ok\u200f!rm -rf /tmp/x","is_error":true}}}"#;
        let (line, _) = plan(&[USER, ASSISTANT, RESULT_ERR, RESULT_ERR, label], Some("2"));
        let line = line.unwrap();
        assert!(line.contains("last: ! ok!rm -rf /tmp/x"), "the mark is gone, the text stays: {line:?}");
        assert!(!line.chars().any(|c| c == '\u{200f}' || c == '\u{202e}'), "{line:?}");
    }

    #[test]
    fn a_tool_label_is_cleaned_like_every_other_child_authored_fragment() {
        // A tool result's first line is the least trusted text in the trace
        // (a file the child `cat`ed, a page it fetched). On the failing and
        // the silence lines it rides as `last: <label>` — and a `\r` inside
        // it is an Enter at a headless parent's PTY, so `!rm …` after it
        // would be a shell escape at the start of a fresh composer line.
        let poison = r#"{"id":9,"parent":8,"ts_ms":1789603009300,"kind":{"ToolResult":{"tool_use_id":"call_p","content":"ok\r!rm -rf /tmp/x\u0007 boom\u001b[0m","is_error":true}}}"#;
        let (line, _) = plan(&[USER, ASSISTANT, RESULT_ERR, RESULT_ERR, poison], Some("2"));
        let line = line.unwrap();
        assert!(line.contains("failing · 3 tool errors in a row · last: ! ok!rm -rf /tmp/x boom[0m"), "{line:?}");
        assert!(!line.chars().any(char::is_control), "{line:?}");

        // The same label on the silence line.
        let old = format!(
            r#"{{"id":9,"parent":8,"ts_ms":{},"kind":{{"ToolResult":{{"tool_use_id":"call_p","content":"ok\r!rm -rf /tmp/x\u0007 boom\u001b[0m","is_error":false}}}}}}"#,
            now_ms() - 20 * 60 * 1000
        );
        let (line, _) = decide(
            &child_of(&[USER, old.as_str()]),
            &CursorEntry { seen: Some("9".into()), ..Default::default() },
            now_ms(),
        );
        let line = line.unwrap();
        assert!(line.contains("silent 20 min · last: ok!rm -rf /tmp/x boom[0m"), "{line:?}");
        assert!(!line.chars().any(char::is_control), "{line:?}");
    }

    #[test]
    fn a_steer_opens_the_frame_a_bare_start_never_decides_anything() {
        // An external steer is a turn's opening record just as a first prompt
        // is, so the calls segment counts from it.
        let (line, _) = plan(&[START, EXTERNAL, ASSISTANT, SETTLED], Some("0"));
        let line = line.unwrap();
        assert!(line.contains("1 calls"), "counted from the steer: {line}");
        assert!(line.contains("last: \"Let me read the slot catalog first.\""), "{line}");

        // A tail holding only a `SessionStart` is not an event of its own.
        assert_eq!(plan(&[START], Some("0")).0, None);
    }

    #[test]
    fn a_cursor_outside_the_tail_treats_the_whole_tail_as_new_once() {
        // `seen` names a record the window no longer holds: everything in the
        // tail is new ONCE, and nothing is said about what was lost.
        let (line, entry) = plan(&[USER, ASSISTANT, SETTLED], Some("9999"));
        assert!(line.unwrap().contains("settled"), "the window's whole content is new once");
        assert_eq!(entry.seen.as_deref(), Some("131"), "the cursor catches up to the tail");

        // The very NEXT pass over the same tail has nothing new at all.
        let (line, entry) = plan(&[USER, ASSISTANT, SETTLED], entry.seen.as_deref());
        assert_eq!(line, None);
        assert_eq!(entry.seen.as_deref(), Some("131"));

        // A child never tracked at all (no cursor entry) is the same shape.
        let (line, _) = plan(&[USER, SETTLED], None);
        assert!(line.unwrap().contains("settled"));
    }

    #[test]
    fn the_silence_line_fires_once_and_re_arms_on_a_new_record() {
        let old = format!(
            r#"{{"id":7,"parent":6,"ts_ms":{},"kind":{{"AssistantMessage":{{"content":[{{"type":"text","text":"quiet"}}]}}}}}}"#,
            now_ms() - 20 * 60 * 1000
        );
        let child = child_of(&[USER, old.as_str()]);
        let seen = "7";

        // First quiet pass: the line, and the latch.
        let (line, entry) = decide(&child, &CursorEntry { seen: Some(seen.into()), ..Default::default() }, now_ms());
        assert!(line.unwrap().contains("silent 20 min"));
        assert_eq!(entry.silent_at.as_deref(), Some("7"));

        // Second quiet pass, same tail, latch set: nothing.
        let (line, entry2) = decide(
            &child,
            &CursorEntry { seen: Some(seen.into()), silent_at: entry.silent_at.clone(), ..Default::default() },
            now_ms(),
        );
        assert_eq!(line, None, "one line per silence");
        assert_eq!(entry2.silent_at.as_deref(), Some("7"), "the latch survives");

        // A NEW record re-arms it: the next settle returns the latch to
        // absent, so a later silence has a line of its own.
        let revived = child_of(&[USER, old.as_str(), SETTLED]);
        let (line, entry3) = decide(
            &revived,
            &CursorEntry { seen: Some(seen.into()), silent_at: Some("7".into()), ..Default::default() },
            now_ms(),
        );
        assert!(line.unwrap().contains("settled"));
        assert_eq!(entry3.silent_at, None, "a new record re-arms the latch");
        assert_eq!(entry3.seen.as_deref(), Some("131"));

        // A quiet turn that has NOT reached ten minutes is nothing.
        let fresh = format!(
            r#"{{"id":8,"parent":7,"ts_ms":{},"kind":{{"AssistantMessage":{{"content":[{{"type":"text","text":"still going"}}]}}}}}}"#,
            now_ms() - 60 * 1000
        );
        let (line, _) = decide(
            &child_of(&[USER, fresh.as_str()]),
            &CursorEntry { seen: Some("8".into()), ..Default::default() },
            now_ms(),
        );
        assert_eq!(line, None, "one quiet minute is not silence");
    }

    // ── the cursor on disk, and the delivery, end to end ─────────────────

    /// Isolate every env var this module's writers read, the same shape
    /// `doorbell.rs`'s own `setup` uses.
    fn setup(tag: &str) -> PathBuf {
        let root = unique_stage(tag);
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        std::env::set_var("AOIDE_AUDIT_LOG", root.join("log"));
        std::env::remove_var("AOIDE_SESSION_ID");
        std::env::remove_var("AOIDE_CONDUCT_AUTOGATE");
        root
    }

    fn daemon_inv() -> Invocation {
        Invocation {
            path: vec!["session".to_string(), "reap".to_string()],
            args: Vec::new(),
            flags: std::collections::BTreeMap::new(),
            door: Door::Daemon,
        }
    }

    /// A live eidolon child on the roster: its presence `meta.json` (the
    /// locator's own source), its trace file beside it, and the record whose
    /// `parentSessionId` names `parent`.
    fn child_fixture(root: &Path, id: &str, parent: &str, body: &[&str]) {
        let sessions = root.join("sessions");
        std::fs::create_dir_all(&sessions).unwrap();
        let trace = sessions.join(format!("{id}.jsonl"));
        std::fs::write(&trace, format!("{}\n", body.join("\n"))).unwrap();

        let presence = root.join("eidolon").join(id);
        std::fs::create_dir_all(&presence).unwrap();
        std::fs::write(
            presence.join("meta.json"),
            format!(
                r#"{{"id":"{id}","pid":4242,"log":"{id}.eid","cwd":"/w","model":"ollama:x","title":"t","busy":false,"trace":"{}"}}"#,
                trace.display()
            ),
        )
        .unwrap();

        let mut file: SessionsFile = load_stage(&sessions_path()).unwrap_or_default();
        let mut rec = session(id, "/w", "working", "2026-09-12T00:00:00Z", Some(parent));
        rec.agent = "eidolon".to_string();
        rec.petname = Some("brave-otter".to_string());
        rec.kind = Some("agent".to_string());
        file.sessions.push(rec);
        file.schema_version = "0".to_string();
        write_stage(&sessions_path(), &file).unwrap();
    }

    /// Register `id` as a HEADLESS conducted parent wrap with a bound control
    /// socket — the shape a delivered ping-back may actually reach.
    fn headless_parent(id: &str, agent: &str) -> UnixListener {
        let socket = conduct_socket_path(id);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let listener = UnixListener::bind(&socket).unwrap();
        do_session_start(
            id,
            Some(agent),
            Some("/w"),
            None,
            None,
            Some(true),
            Some(socket.to_str().unwrap()),
            None,
            None,
        );
        stamp_headless(id);
        listener
    }

    fn read_all(listener: UnixListener) -> Vec<u8> {
        let (mut conn, _) = listener.accept().unwrap();
        let mut buf = Vec::new();
        let _ = conn.read_to_end(&mut buf);
        buf
    }

    /// A `TurnSettled` record with an arbitrary id — the shape a test uses to
    /// give one child a SECOND turn, so a later pass has something new to
    /// decide.
    fn settle(id: &str) -> String {
        format!(
            r#"{{"id":{id},"parent":{id},"ts_ms":{},"kind":{{"TurnSettled":{{"stop_reason":"end_turn","usage":{{}}}}}}}}"#,
            now_ms()
        )
    }

    /// Rewrite a fixture child's trace body — one more turn on the same child,
    /// the way an eidolon appends to its own `.jsonl`.
    fn advance_trace(root: &Path, id: &str, body: &[&str]) {
        let trace = root.join("sessions").join(format!("{id}.jsonl"));
        std::fs::write(&trace, format!("{}\n", body.join("\n"))).unwrap();
    }

    /// A `remoteParent` stamp as the A2A door writes one: the key that
    /// verified the spawn, the parent's node, and the parent's own session id
    /// on that node.
    fn remote_stamp() -> aoide_storage::records::RemoteParent {
        aoide_storage::records::RemoteParent {
            node: "sakaki".to_string(),
            key: "aa".repeat(32),
            session_id: "conduct-17991-1790312541".to_string(),
            extra: Default::default(),
        }
    }

    /// Stamp a roster child `remoteParent` and clear its local parent edge —
    /// exactly the shape the door leaves behind (`stamp_spawn_provenance`
    /// writes the stamp and never a `parentSessionId`, which is local-only).
    /// Nothing else about the record moves.
    fn make_remote(id: &str) {
        let mut file: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec = file.sessions.iter_mut().find(|r| r.session_id == id).unwrap();
        rec.remote_parent = Some(remote_stamp());
        rec.parent_session_id = None;
        write_stage(&sessions_path(), &file).unwrap();
    }

    /// A roster child whose parent is on another node, of any harness: the
    /// non-eidolon shape has no trace at all and exists here for `Exited`.
    fn remote_child_record(id: &str, agent: &str, state: &str) {
        let mut file: SessionsFile = load_stage(&sessions_path()).unwrap_or_default();
        let mut rec = session(id, "/w", state, "2026-09-12T00:00:00Z", None);
        rec.agent = agent.to_string();
        rec.remote_parent = Some(remote_stamp());
        file.sessions.push(rec);
        file.schema_version = "0".to_string();
        write_stage(&sessions_path(), &file).unwrap();
    }

    /// Rewrite one roster record's own end, as the child's exit path and the
    /// reaper stamp it.
    fn end_record(id: &str, state: &str, code: Option<i32>, outcome: Option<&str>) {
        let mut file: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec = file.sessions.iter_mut().find(|r| r.session_id == id).unwrap();
        rec.state = state.to_string();
        rec.exit_code = code;
        rec.outcome = outcome.map(str::to_string);
        write_stage(&sessions_path(), &file).unwrap();
    }

    #[test]
    fn a_delivered_line_reaches_a_headless_parent_and_the_cursor_stops_a_second_pass() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_SESSION_ID",
            "AOIDE_CONDUCT_AUTOGATE",
        ]);
        let root = setup("pingback-deliver-once");
        let listener = headless_parent("wrap-1", "claude");
        child_fixture(&root, "user-0001", "wrap-1", &[USER, ASSISTANT, SETTLED]);

        let acc = std::thread::spawn(move || read_all(listener));
        let report = pingback(&daemon_inv(), &[]);
        let bytes = acc.join().unwrap();

        let text = String::from_utf8_lossy(&bytes);
        assert!(
            text.starts_with("[eidolon brave-otter] settled end_turn"),
            "the parent hears its child: {text:?}"
        );
        assert!(!text.contains("\n["), "exactly one line: {text:?}");
        assert!(!text.contains("from "), "no provenance prefix: {text:?}");
        assert!(text.ends_with("\n\r"), "the line, then the target's own submit key: {bytes:?}");
        assert_eq!(report.delivered.len(), 1, "{report:?}");
        assert!(report.skipped.is_empty(), "{report:?}");

        // The claim landed BEFORE the delivery: a second pass over the same
        // tail has nothing left to say, and writes no socket.
        let cursor: CursorFile = serde_json::from_str(&std::fs::read_to_string(pingback_path()).unwrap()).unwrap();
        assert_eq!(
            cursor.get("user-0001").and_then(|e| e.seen.clone()).as_deref(),
            Some("131")
        );
        let again = pingback(&daemon_inv(), &[]);
        assert_eq!(again, PingbackReport::default(), "at-most-once: the second pass is silent");

        // One audit line, under this lane's own gate label.
        let log = std::fs::read_to_string(root.join("log")).unwrap();
        assert!(log.contains("autogate-child"), "{log}");
        assert!(log.contains("delivered to `wrap-1`"), "{log}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_line_goes_out_over_a_live_channel_socket_with_no_submit_keystroke() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_SESSION_ID",
            "AOIDE_CONDUCT_AUTOGATE",
        ]);
        let root = setup("pingback-channel");
        // An interactive parent (never `stamp_headless`) — the channel is what
        // makes it reachable at all, and it takes the line with no keystroke.
        let socket = conduct_socket_path("wrap-1");
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let pty = UnixListener::bind(&socket).unwrap();
        do_session_start(
            "wrap-1",
            Some("claude"),
            Some("/w"),
            None,
            None,
            Some(true),
            Some(socket.to_str().unwrap()),
            None,
            None,
        );
        let channel = channel_socket_path("wrap-1");
        std::fs::create_dir_all(channel.parent().unwrap()).unwrap();
        let channel_listener = UnixListener::bind(&channel).unwrap();

        child_fixture(&root, "user-0001", "wrap-1", &[USER, ASK]);
        let acc = std::thread::spawn(move || read_all(channel_listener));
        let report = pingback(&daemon_inv(), &[]);
        let bytes = acc.join().unwrap();

        assert_eq!(String::from_utf8_lossy(&bytes), "[eidolon brave-otter] asking: \"Overwrite?\"\n");
        assert_eq!(report.delivered.len(), 1, "{report:?}");
        pty.set_nonblocking(true).unwrap();
        assert!(
            pty.accept().is_err(),
            "the channel wins: the PTY is never written to"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_delivered_line_is_never_folded_into_the_sweep_and_other_doors_skip_it() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_SESSION_ID",
        ]);
        let root = setup("pingback-door-gate");
        let listener = headless_parent("wrap-1", "claude");
        child_fixture(&root, "user-0001", "wrap-1", &[USER, SETTLED]);

        // Every other door forwards through the daemon instead — nothing is
        // delivered, and the cursor is not even claimed.
        let mut inv = daemon_inv();
        inv.door = Door::Cli;
        let report = pingback(&inv, &[]);
        assert_eq!(report, PingbackReport::default());
        listener.set_nonblocking(true).unwrap();
        assert!(listener.accept().is_err(), "a non-daemon door writes nothing");
        assert!(!pingback_path().exists(), "and claims nothing");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_shell_parent_is_skipped_and_never_written_to() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_SESSION_ID",
        ]);
        let root = setup("pingback-shell-parent");
        // A conducted shell: conductable, a real socket, and its agent is
        // `shell` — a line submitted here would RUN as a command.
        let listener = headless_parent("wrap-1", "shell");
        child_fixture(&root, "user-0001", "wrap-1", &[USER, SETTLED]);

        let report = pingback(&daemon_inv(), &[]);
        listener.set_nonblocking(true).unwrap();
        assert!(listener.accept().is_err(), "a bare shell is never injected into");
        assert_eq!(report.skipped, vec![("wrap-1".to_string(), "shell-parent".to_string())]);
        assert!(report.delivered.is_empty());
        // The line is lost, not replayed: the claim ran first, which is the
        // safe direction (a duplicate in a composer is the other one).
        let cursor: CursorFile = serde_json::from_str(&std::fs::read_to_string(pingback_path()).unwrap()).unwrap();
        assert_eq!(cursor.get("user-0001").and_then(|e| e.seen.clone()).as_deref(), Some("131"));
        assert_eq!(pingback(&daemon_inv(), &[]), PingbackReport::default());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_parent_that_is_gone_not_conductable_or_done_is_a_named_skip() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_SESSION_ID",
        ]);
        let root = setup("pingback-parent-skips");
        child_fixture(&root, "user-0001", "wrap-gone", &[USER, SETTLED]);
        // No record for `wrap-gone` at all.
        let report = pingback(&daemon_inv(), &[]);
        assert_eq!(report.skipped, vec![("wrap-gone".to_string(), "no-parent-record".to_string())]);

        // A recorded parent whose control socket file was never bound.
        let socket = conduct_socket_path("wrap-dead");
        do_session_start(
            "wrap-dead",
            Some("claude"),
            Some("/w"),
            None,
            None,
            Some(true),
            Some(socket.to_str().unwrap()),
            None,
            None,
        );
        let mut file: SessionsFile = load_stage(&sessions_path()).unwrap();
        file.sessions
            .iter_mut()
            .find(|s| s.session_id == "user-0001")
            .unwrap()
            .parent_session_id = Some("wrap-dead".to_string());
        write_stage(&sessions_path(), &file).unwrap();
        // A FRESH settled record, so the cursor is not already past the tail:
        // every pass below has something new to decide, and the skip reason is
        // what the assertion is actually about.
        advance_trace(&root, "user-0001", &[USER, &settle("200")]);
        let report = pingback(&daemon_inv(), &[]);
        assert_eq!(report.skipped, vec![("wrap-dead".to_string(), "not-conductable".to_string())]);

        // Same parent, socket bound, but the record already ended.
        let listener = headless_parent("wrap-dead", "claude");
        let mut file: SessionsFile = load_stage(&sessions_path()).unwrap();
        file.sessions.iter_mut().find(|s| s.session_id == "wrap-dead").unwrap().state = "done".to_string();
        write_stage(&sessions_path(), &file).unwrap();
        advance_trace(&root, "user-0001", &[USER, &settle("201")]);
        let report = pingback(&daemon_inv(), &[]);
        assert_eq!(report.skipped, vec![("wrap-dead".to_string(), "parent-done".to_string())]);
        listener.set_nonblocking(true).unwrap();
        assert!(listener.accept().is_err());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_dropped_child_reports_died_and_leaves_the_cursor_file() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_SESSION_ID",
        ]);
        let root = setup("pingback-dropped");
        let listener = headless_parent("wrap-1", "claude");
        child_fixture(&root, "user-0001", "wrap-1", &[USER, ASSISTANT]);

        // The child's own record was just dropped by the sync that ran before
        // this pass (`reap`'s order): the roster no longer holds it, and the
        // dropped set names it with the trace path its record carried.
        let trace = root.join("sessions").join("user-0001.jsonl");
        let mut file: SessionsFile = load_stage(&sessions_path()).unwrap();
        file.sessions.retain(|s| s.session_id != "user-0001");
        write_stage(&sessions_path(), &file).unwrap();

        // A stale entry the file already carried for it (the child was on the
        // roster on an earlier tick) — it must be GONE after this pass.
        std::fs::write(
            pingback_path(),
            r#"{"user-0001":{"seen":"2"},"user-other":{"seen":"9"}}"#,
        )
        .unwrap();

        let acc = std::thread::spawn(move || read_all(listener));
        let report = pingback(
            &daemon_inv(),
            &[DroppedEidolon {
                session_id: "user-0001".to_string(),
                petname: Some("brave-otter".to_string()),
                parent_session_id: Some("wrap-1".to_string()),
                agent: "eidolon".to_string(),
                trace: Some(trace),
                remote: false,
                remote_key: None,
                exit_code: None,
                outcome: None,
            }],
        );
        let bytes = acc.join().unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.starts_with("[eidolon brave-otter] died mid-turn"), "{text:?}");
        assert_eq!(report.delivered.len(), 1, "{report:?}");

        // The entry leaves the file on the SAME pass — nothing lingers for a
        // child that is no longer on the roster, and every id whose eidolon
        // record is gone goes with it.
        let cursor: CursorFile = serde_json::from_str(&std::fs::read_to_string(pingback_path()).unwrap()).unwrap();
        assert!(cursor.is_empty(), "every entry without a roster record is gone: {cursor:?}");

        let _ = std::fs::remove_dir_all(&root);
    }

    // ── P-RSA S8: a remote parent's events ──────────────────────────────

    #[test]
    fn a_remote_parent_child_spools_its_events_and_never_delivers() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_SESSION_ID",
            "AOIDE_CONDUCT_AUTOGATE",
        ]);
        let root = setup("pingback-remote-spool");
        // A live, reachable LOCAL parent — a delivery WOULD land here, which is
        // what makes "never delivers" a real claim rather than an accident of
        // an unreachable target.
        let listener = headless_parent("wrap-1", "claude");
        child_fixture(&root, "user-0001", "wrap-1", &[USER, ASSISTANT, SETTLED]);
        make_remote("user-0001");

        let report = pingback(&daemon_inv(), &[]);
        assert!(report.delivered.is_empty(), "a remote parent is never delivered to from here: {report:?}");
        assert!(report.skipped.is_empty(), "{report:?}");
        listener.set_nonblocking(true).unwrap();
        assert!(listener.accept().is_err(), "not one byte was written to the local parent");

        // The event took the line's place on the ring, whole: the same
        // decision the local parent would have read as a line.
        let read = aoide_storage::pingback_remote::events_for("user-0001", 0);
        assert_eq!(read.events.len(), 1, "{read:?}");
        assert_eq!(read.events[0].seq, 1);
        assert!(!read.gap, "a fresh cursor sees every event the child published");
        let event = &read.events[0].event;
        assert_eq!(event["settled"]["stop"], serde_json::json!("end_turn"), "{event}");
        assert_eq!(event["settled"]["say"], serde_json::json!("Let me read the slot catalog first."), "{event}");
        assert_eq!(event["settled"]["calls"], serde_json::json!(1));

        // The cursor is the same at-most-once cursor: a second pass over the
        // same tail publishes nothing, and the ring is not appended to.
        let cursor: CursorFile = serde_json::from_str(&std::fs::read_to_string(pingback_path()).unwrap()).unwrap();
        assert_eq!(cursor.get("user-0001").and_then(|e| e.seen.clone()).as_deref(), Some("131"));
        let again = pingback(&daemon_inv(), &[]);
        assert_eq!(again, PingbackReport::default(), "at-most-once: the second pass is silent");
        assert_eq!(aoide_storage::pingback_remote::events_for("user-0001", 0).events.len(), 1);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_exited_event_fires_once_when_a_remote_child_ends() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_SESSION_ID",
            "AOIDE_CONDUCT_AUTOGATE",
        ]);
        let root = setup("pingback-remote-exit");
        // A non-eidolon harness: no trace to read, ever. Its exit is the one
        // event this module can still publish for it.
        remote_child_record("a2a-4411-1790", "a2a", "working");
        // A LOCAL child that ends the same way — nobody spools for it (Q5's
        // ruled default: the local parent hears the run's own report).
        remote_child_record("local-sess", "claude", "done");
        let mut file: SessionsFile = load_stage(&sessions_path()).unwrap();
        file.sessions.iter_mut().find(|r| r.session_id == "local-sess").unwrap().remote_parent = None;
        write_stage(&sessions_path(), &file).unwrap();

        let report = pingback(&daemon_inv(), &[]);
        assert!(
            aoide_storage::pingback_remote::events_for("a2a-4411-1790", 0).events.is_empty(),
            "a running child owes nothing yet"
        );
        assert_eq!(report, PingbackReport::default(), "{report:?}");

        end_record("a2a-4411-1790", "done", Some(7), Some("exit"));
        end_record("local-sess", "done", Some(7), Some("exit"));
        let report = pingback(&daemon_inv(), &[]);
        assert!(report.delivered.is_empty(), "{report:?}");
        let read = aoide_storage::pingback_remote::events_for("a2a-4411-1790", 0);
        assert_eq!(read.events.len(), 1, "{read:?}");
        assert_eq!(
            read.events[0].event,
            serde_json::json!({ "exited": { "code": 7, "outcome": "exit" } })
        );
        assert!(
            aoide_storage::pingback_remote::events_for("local-sess", 0).events.is_empty(),
            "a local child's end is its report's business, never an event"
        );

        // ONCE. The record stays on the roster after its run ended, so the
        // latch — not the roster's own lifetime — is what stops the repeat.
        let again = pingback(&daemon_inv(), &[]);
        assert_eq!(again, PingbackReport::default(), "{again:?}");
        assert_eq!(
            aoide_storage::pingback_remote::events_for("a2a-4411-1790", 0).events.len(),
            1,
            "exited is claimed once per child"
        );
        let cursor: CursorFile = serde_json::from_str(&std::fs::read_to_string(pingback_path()).unwrap()).unwrap();
        let entry = cursor.get("a2a-4411-1790").expect("the child keeps its entry");
        assert!(entry.exited, "the latch is what was written: {entry:?}");
        assert_eq!(entry.seen, None, "a child with no trace has no `seen` to hold it");

        // The line the puller will render for it, from the event alone.
        assert_eq!(
            render_line("[sakaki/conduct-17991]", &PingEvent::Exited { code: Some(7), outcome: Some("exit".into()) }),
            "[sakaki/conduct-17991] exited · exit 7"
        );
        assert_eq!(
            render_line(
                "[sakaki/conduct-17991]",
                &PingEvent::Exited { code: None, outcome: Some("signal".into()) }
            ),
            "[sakaki/conduct-17991] exited · signal"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_spooled_event_is_already_clean_and_still_renders_through_the_guard() {
        // The child authored BOTH fragments: a prompt that would read as a
        // command at a prompt, with a control character in it.
        let hostile = "{\"id\":9,\"parent\":8,\"ts_ms\":1000,\"kind\":{\"AskUser\":{\"prompt\":\"/compact\\u0007 now\\r!rm -rf /\",\"answer\":null}}}";
        let event = event_of(&[USER, hostile], Some("1")).expect("an open prompt is an event");
        let json = serde_json::to_value(&event).unwrap();

        // The sender cleaned: no control character reaches the ring at all, so
        // no receiver has to trust the far node for that.
        let text = serde_json::to_string(&json).unwrap();
        assert!(!text.chars().any(char::is_control), "{text:?}");
        assert!(
            json["asking"]["prompt"].as_str().unwrap().starts_with('/'),
            "cleaning is not the guard — the leading `/` is still there to be neutralized: {json}"
        );

        // ...and the renderer neutralizes it, exactly as the local line always
        // has: a leading space turns a command into text.
        let line = render_line("[eidolon brave-otter]", &event);
        assert_eq!(line, "[eidolon brave-otter] asking: \" /compact now!rm -rf /\"", "{line:?}");
        assert!(!line.chars().any(char::is_control), "{line:?}");
    }

    #[test]
    fn a_dropped_remote_child_ends_its_ring_with_one_exited() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_SESSION_ID",
        ]);
        let root = setup("pingback-dropped-remote");
        // A remote eidolon child, mid-turn, whose presence the sync just
        // removed: its record is gone, so THIS is the only pass that can ever
        // tell its parent anything again.
        child_fixture(&root, "user-0001", "wrap-1", &[USER, ASSISTANT]);
        make_remote("user-0001");
        let trace = root.join("sessions").join("user-0001.jsonl");
        let mut file: SessionsFile = load_stage(&sessions_path()).unwrap();
        file.sessions.retain(|s| s.session_id != "user-0001");
        write_stage(&sessions_path(), &file).unwrap();

        let report = pingback(
            &daemon_inv(),
            &[DroppedEidolon {
                session_id: "user-0001".to_string(),
                petname: Some("brave-otter".to_string()),
                parent_session_id: None,
                agent: "eidolon".to_string(),
                trace: Some(trace),
                remote: true,
                remote_key: Some("aa".repeat(32)),
                exit_code: Some(7),
                outcome: Some("exit".to_string()),
            }],
        );
        assert!(report.delivered.is_empty(), "{report:?}");

        // Two events, in that order: the row the drop could still produce, then
        // the exit that closes the ring.
        let read = aoide_storage::pingback_remote::events_for("user-0001", 0);
        assert_eq!(read.events.len(), 2, "{read:?}");
        assert_eq!(read.events[0].seq, 1);
        assert!(
            read.events[0].event.get("died_mid_turn").is_some(),
            "the turn was open when the process went away: {read:?}"
        );
        assert_eq!(read.events[1].seq, 2);
        assert_eq!(
            read.events[1].event,
            serde_json::json!({ "exited": { "code": 7, "outcome": "exit" } }),
            "the ring ENDS with one exit, from the record's own end facts"
        );

        // Exactly one: no later tick can decide this child again (its record is
        // gone and its cursor entry left with it), and a second pass with the
        // same dropped set is what proves the ring is not appended to twice.
        let again = pingback(&daemon_inv(), &[]);
        assert_eq!(again, PingbackReport::default(), "{again:?}");
        assert_eq!(aoide_storage::pingback_remote::events_for("user-0001", 0).events.len(), 2);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_child_that_says_nothing_new_keeps_its_place_and_never_rewrites_the_file() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_SESSION_ID",
        ]);
        let root = setup("pingback-quiet");
        let _listener = headless_parent("wrap-1", "claude");
        child_fixture(&root, "user-0001", "wrap-1", &[USER, SETTLED]);

        // A trace whose window holds no readable record at all: nothing is
        // examined, so no cursor entry is ever created for it.
        child_fixture(&root, "user-empty", "wrap-1", &[]);
        let report = pingback(&daemon_inv(), &[]);
        assert_eq!(report.delivered.len(), 1, "{report:?}");
        let cursor: CursorFile = serde_json::from_str(&std::fs::read_to_string(pingback_path()).unwrap()).unwrap();
        assert_eq!(cursor.len(), 1, "only the child with records has an entry: {cursor:?}");
        assert!(cursor.contains_key("user-0001"));

        // A second pass over the same quiet tail: the file is byte-identical
        // (change-only), so the daemon's tick writes nothing at all.
        let before = std::fs::read_to_string(pingback_path()).unwrap();
        let report = pingback(&daemon_inv(), &[]);
        assert_eq!(report, PingbackReport::default());
        assert_eq!(std::fs::read_to_string(pingback_path()).unwrap(), before);

        let _ = std::fs::remove_dir_all(&root);
    }

    // ── P-RSA S9: the parent pulls its remote children ──────────────────

    /// The far node's own key — the ledger row's `key`, and the pubkey the
    /// registry resolves it by.
    fn remote_key() -> String {
        "ab".repeat(32)
    }

    /// Register the far node this node spawned onto, under the key the ledger
    /// row carries. A registered node is what a pull resolves to before it
    /// dials anything; the fetch closure never reaches the wire in these
    /// tests.
    fn register_far_node(name: &str) {
        aoide_storage::node_store::save_nodes(&[aoide_storage::node_store::Node {
            name: name.into(),
            url: "http://nodeb:8710/".into(),
            autogate: false,
            token_file: None,
            bearer_secret: None,
            hub: false,
            pubkey: Some(remote_key()),
            verified: true,
            allows: vec!["read".into()],
            via: None,
            added_at: "2026-09-25T00:00:00Z".into(),
        }])
        .unwrap();
    }

    /// One ledger row: `child` spawned from `parent` on the far node.
    fn ledger_row(parent: &str, child: &str) {
        aoide_storage::remote_children::append_remote_child(
            &aoide_storage::remote_children::RemoteChild {
                parent_session_id: parent.into(),
                node: "nodeb".into(),
                key: remote_key(),
                session_id: child.into(),
                spawned_at: "2026-09-25T00:00:00Z".into(),
                lines_after: 0,
                drained: false,
                extra: Default::default(),
            },
        )
        .unwrap();
    }

    /// The ledger's cursor for one child, as the node that pulled it left it.
    fn cursor_of(child: &str) -> u64 {
        aoide_storage::remote_children::load_remote_children()
            .into_iter()
            .find(|c| c.session_id == child)
            .map(|c| c.lines_after)
            .unwrap_or(0)
    }

    fn drained_of(child: &str) -> bool {
        aoide_storage::remote_children::load_remote_children()
            .into_iter()
            .any(|c| c.session_id == child && c.drained)
    }

    /// A ring read of `(seq, event)`s, contiguous and gap-free.
    fn read_of(events: &[(u64, serde_json::Value)]) -> RingRead {
        let events = events
            .iter()
            .map(|(seq, event)| aoide_storage::pingback_remote::RemoteEvent {
                seq: *seq,
                event: event.clone(),
                extra: Default::default(),
            })
            .collect::<Vec<_>>();
        RingRead { last: events.last().map(|e| e.seq).unwrap_or(0), events, gap: false }
    }

    /// A settled event, as a peer would publish one.
    fn settled(say: &str) -> serde_json::Value {
        serde_json::json!({
            "settled": { "stop": "end_turn", "calls": 1, "mins": 2, "say": say, "errors": 0 }
        })
    }

    fn exited() -> serde_json::Value {
        serde_json::json!({ "exited": { "code": 7, "outcome": "exit" } })
    }

    #[test]
    fn a_pulled_line_is_rendered_locally_and_delivered_to_the_parent() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_SESSION_ID",
        ]);
        let root = setup("pingback-pull-line");
        register_far_node("nodeb");
        let listener = headless_parent("wrap-1", "claude");
        ledger_row("wrap-1", "a2a-4411-1790");

        let acc = std::thread::spawn(move || read_all(listener));
        let report = pingback_pull_with(&daemon_inv(), move |node, id, after, key| {
            assert_eq!(node.name, "nodeb", "the row resolves to the node its key names");
            assert_eq!(id, "a2a-4411-1790");
            assert_eq!(after, 0, "the cursor the ledger holds is what the far node is asked from");
            assert_eq!(key, "wrap-1", "the TUNNEL KEY is the parent session id");
            Ok(read_of(&[(1, settled("let me read the catalog first"))]))
        });
        let bytes = acc.join().unwrap();

        // The tag names the BOX and the child; the event supplies only the rest.
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            text.starts_with("[nodeb/a2a-4411-1790] settled end_turn"),
            "a pulled line renders with the ledger's own tag: {text:?}"
        );
        assert!(!text.contains("\n["), "exactly one line: {text:?}");
        assert_eq!(report.delivered.len(), 1, "{report:?}");
        assert!(report.skipped.is_empty(), "{report:?}");
        assert_eq!(cursor_of("a2a-4411-1790"), 1, "the cursor advanced past the delivered event");

        // A remote delivery says which wire it came in on.
        let log = std::fs::read_to_string(root.join("log")).unwrap();
        assert!(log.contains("autogate-child remote"), "{log}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_unknown_event_kind_is_dropped_and_the_ring_still_drains() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_SESSION_ID",
        ]);
        let root = setup("pingback-pull-unknown");
        register_far_node("nodeb");
        let _listener = headless_parent("wrap-1", "claude");
        ledger_row("wrap-1", "a2a-4411-1790");

        let report = pingback_pull_with(&daemon_inv(), move |_, _, _, _| {
            // A kind this version does not know, a field of the wrong type, and
            // one honest event beside them. The ring is the SAME version as
            // this node, so a shape that does not deserialize is not an event
            // to guess at.
            Ok(read_of(&[
                (1, serde_json::json!({ "reticulated": { "splines": 4 } })),
                (2, serde_json::json!({ "settled": { "stop": "end_turn", "calls": "many" } })),
                (3, settled("done")),
            ]))
        });

        assert_eq!(report.delivered.len(), 1, "only the readable event becomes a line: {report:?}");
        assert!(report.delivered[0].1.contains("settled end_turn"), "{report:?}");
        assert!(
            !report.delivered[0].1.contains("reticulated"),
            "an unknown kind is never rendered: {report:?}"
        );
        // The cursor is not held back by what was dropped: a kind nobody can
        // read must not pin the ring forever.
        assert_eq!(cursor_of("a2a-4411-1790"), 3);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_pulled_event_is_re_cleaned_and_its_quoted_field_neutralized() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_SESSION_ID",
        ]);
        let root = setup("pingback-pull-hostile");
        register_far_node("nodeb");
        let _listener = headless_parent("wrap-1", "claude");
        ledger_row("wrap-1", "a2a-4411-1790");

        let report = pingback_pull_with(&daemon_inv(), move |_, _, _, _| {
            Ok(read_of(&[
                // A quoted field that would read as a command, an ESC/colour
                // sequence, and a bidi override — the sender's own cleaning
                // (`clean`) strips control characters only, so a peer can hand
                // over both of these.
                (1, serde_json::json!({ "asking": {
                    "prompt": "/compact\u{1b}[31m\u{202e} gpj.exe \u{7}now", "errors": 0 } })),
                // A BARE segment (never quoted by the renderer) leading with a
                // shell escape.
                (2, serde_json::json!({ "failing": { "run": 3, "tool": "!rm -rf /" } })),
            ]))
        });

        let lines: Vec<&str> = report.delivered.iter().map(|(_, l)| l.as_str()).collect();
        assert_eq!(lines.len(), 2, "{report:?}");

        let asking = lines[0];
        assert!(asking.starts_with("[nodeb/a2a-4411-1790] asking:"), "{asking:?}");
        assert!(asking.contains("\" /compact"), "the leading `/` is neutralized: {asking:?}");
        assert!(!asking.contains('\u{1b}'), "no escape sequence survives: {asking:?}");
        assert!(!asking.contains('\u{202e}'), "no bidi override survives: {asking:?}");
        assert!(!asking.contains('\u{7}'), "no control character survives: {asking:?}");

        let failing = lines[1];
        assert!(failing.contains("last: !rm -rf /") || failing.contains("last:  !rm -rf /"),
            "a bare segment is cleaned and its leading `!` neutralized: {failing:?}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_shell_parent_is_never_pulled_for() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_SESSION_ID",
        ]);
        let root = setup("pingback-pull-shell");
        register_far_node("nodeb");
        // A bare shell on the roster, with a live socket — the shape a
        // deliverable target has, minus the harness.
        let socket = conduct_socket_path("shell-1");
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let listener = UnixListener::bind(&socket).unwrap();
        do_session_start("shell-1", Some("shell"), Some("/w"), None, None, Some(false),
            Some(socket.to_str().unwrap()), None, None);
        ledger_row("shell-1", "a2a-4411-1790");

        let report = pingback_pull_with(&daemon_inv(), |_, _, _, _| {
            panic!("a shell parent is judged before the far node is asked anything")
        });

        assert_eq!(report.skipped, vec![("shell-1".to_string(), "shell-parent".to_string())], "{report:?}");
        assert!(report.delivered.is_empty(), "{report:?}");
        listener.set_nonblocking(true).unwrap();
        assert!(listener.accept().is_err(), "not one byte was written to a shell's input");
        assert_eq!(cursor_of("a2a-4411-1790"), 0, "and nothing was consumed on its behalf");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn no_pull_happens_when_the_parent_is_done() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_SESSION_ID",
        ]);
        let root = setup("pingback-pull-done-parent");
        register_far_node("nodeb");
        let _listener = headless_parent("wrap-1", "claude");
        ledger_row("wrap-1", "a2a-4411-1790");
        end_record("wrap-1", "done", Some(0), Some("exit"));

        let report = pingback_pull_with(&daemon_inv(), |_, _, _, _| {
            panic!("a done parent is never pulled for")
        });
        assert_eq!(report.skipped, vec![("wrap-1".to_string(), "parent-done".to_string())], "{report:?}");
        assert_eq!(cursor_of("a2a-4411-1790"), 0);

        // A parent that is not even on the roster is the same shape: nothing to
        // deliver to, so nothing is asked for and nothing is consumed.
        let mut file: SessionsFile = load_stage(&sessions_path()).unwrap();
        file.sessions.retain(|s| s.session_id != "wrap-1");
        write_stage(&sessions_path(), &file).unwrap();
        let report = pingback_pull_with(&daemon_inv(), |_, _, _, _| {
            panic!("a parent with no record is never pulled for")
        });
        assert_eq!(report.skipped, vec![("wrap-1".to_string(), "no-parent-record".to_string())]);
        assert_eq!(cursor_of("a2a-4411-1790"), 0);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_cursor_advances_even_when_the_line_cannot_be_written() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_SESSION_ID",
        ]);
        let root = setup("pingback-pull-write-fails");
        register_far_node("nodeb");
        let listener = headless_parent("wrap-1", "claude");
        ledger_row("wrap-1", "a2a-4411-1790");
        // The session stays conductable (its socket path exists) but nothing is
        // listening on it: `deliver` reaches the headless branch and the write
        // fails. This is the at-most-once direction the whole lane loses in.
        drop(listener);

        let report = pingback_pull_with(&daemon_inv(), move |_, _, _, _| {
            Ok(read_of(&[(1, settled("read the catalog")), (2, settled("and then this"))]))
        });

        assert!(report.delivered.is_empty(), "nothing landed: {report:?}");
        assert_eq!(
            report.skipped,
            vec![
                ("wrap-1".to_string(), "write-failed".to_string()),
                ("wrap-1".to_string(), "write-failed".to_string())
            ],
            "{report:?}"
        );
        assert_eq!(cursor_of("a2a-4411-1790"), 2, "the cursor moved BEFORE the delivery, and stays moved");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_failed_pull_leaves_the_cursor_alone_and_the_pass_survives_it() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_SESSION_ID",
        ]);
        let root = setup("pingback-pull-failed");
        register_far_node("nodeb");
        ledger_row("wrap-1", "dead-child");
        let listener = headless_parent("wrap-1", "claude");
        // A second row behind the failing one: a pull failure must not end the
        // pass, and the tick after it asks again from the same cursor.
        ledger_row("wrap-1", "a2a-4411-1790");
        aoide_storage::remote_children::advance_lines_after(&remote_key(), "dead-child", 4).unwrap();

        let calls = std::cell::Cell::new(0);
        let acc = std::thread::spawn(move || read_all(listener));
        let report = pingback_pull_with(&daemon_inv(), |_, id, after, _| {
            calls.set(calls.get() + 1);
            if id == "dead-child" {
                assert_eq!(after, 4, "the retry starts from the cursor the ledger holds");
                // A peer's own bytes: hostile ones included, and a transport
                // failure carries no code — retryable, never a latch.
                return Err(FrameReadError {
                    code: None,
                    message: "the far node refused\u{1b}[31m the read\u{202e}".to_string(),
                });
            }
            Ok(read_of(&[(1, settled("still here"))]))
        });
        let bytes = acc.join().unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert_eq!(calls.get(), 2, "the failing child did not stop the next one");

        assert_eq!(cursor_of("dead-child"), 4, "a failed pull leaves the cursor exactly where it was");
        assert_eq!(cursor_of("a2a-4411-1790"), 1, "the healthy child still moved");
        assert!(text.contains("[nodeb/a2a-4411-1790] settled end_turn"), "{text:?}");
        assert!(
            report.skipped.contains(&("wrap-1".to_string(), "pull-failed".to_string())),
            "{report:?}"
        );

        // The failure is audited, and the peer's own words are cleaned before
        // they land in the log (house rule 4 applies to the log too).
        let log = std::fs::read_to_string(root.join("log")).unwrap();
        assert!(log.contains("pull-failed"), "{log}");
        assert!(!log.contains('\u{1b}') && !log.contains('\u{202e}'), "{log}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_overlapping_pass_owns_nothing_and_a_lying_last_moves_nothing() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_SESSION_ID",
        ]);
        let root = setup("pingback-pull-overlap");
        register_far_node("nodeb");
        let _listener = headless_parent("wrap-1", "claude");
        ledger_row("wrap-1", "a2a-4411-1790");

        // M1: no gap, no events, and a `last` no ring could justify. `last` is
        // the resync a GAP needs, and nothing else — believing it here would
        // drive the cursor past every seq the child can ever push, silencing
        // it forever and making the row undrainable.
        let report = pingback_pull_with(&daemon_inv(), |_, _, after, _| {
            assert_eq!(after, 0);
            Ok(RingRead { events: Vec::new(), gap: false, last: u64::MAX })
        });
        assert_eq!(report, PingbackReport::default(), "{report:?}");
        assert_eq!(cursor_of("a2a-4411-1790"), 0, "a peer's own `last` is not a cursor");

        // M2: a pass that fetched the same window as one that has since
        // delivered it. The overlap is staged where it really happens — the
        // claim section — by advancing the ledger inside the fetch, and the
        // delivery owes nothing: the events are already spoken for.
        let report = pingback_pull_with(&daemon_inv(), |_, _, after, _| {
            assert_eq!(after, 0, "this pass fetched BEFORE the other one claimed");
            aoide_storage::remote_children::advance_lines_after(&remote_key(), "a2a-4411-1790", 2)
                .unwrap();
            Ok(read_of(&[(1, settled("first")), (2, settled("second"))]))
        });
        assert_eq!(report, PingbackReport::default(), "no duplicate lines: {report:?}");
        assert_eq!(cursor_of("a2a-4411-1790"), 2);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_gone_child_latches_the_row_on_the_permanent_not_found() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_SESSION_ID",
        ]);
        let root = setup("pingback-pull-child-gone");
        register_far_node("nodeb");
        let _listener = headless_parent("wrap-1", "claude");
        ledger_row("wrap-1", "a2a-4411-1790");

        // The far door's "no record and no ring" answer, with the code it
        // carries: the child is gone for good.
        let calls = std::cell::Cell::new(0);
        let report = pingback_pull_with(&daemon_inv(), |_, _, _, _| {
            calls.set(calls.get() + 1);
            Err(FrameReadError {
                code: Some(aoide_protocol::wire::a2a::TASK_NOT_FOUND_CODE),
                message: "task not found".to_string(),
            })
        });
        assert_eq!(calls.get(), 1);
        assert_eq!(report.skipped, vec![("wrap-1".to_string(), "child-gone".to_string())], "{report:?}");
        assert!(drained_of("a2a-4411-1790"), "a child that is gone is never asked about again");

        // The latch is what stops the forever-retry: no second request, ever.
        let report = pingback_pull_with(&daemon_inv(), |_, _, _, _| {
            panic!("a latched row is never pulled again")
        });
        assert_eq!(report, PingbackReport::default(), "{report:?}");

        // And it is audited for what it was — distinct from a transient
        // failure, which never latches.
        let log = std::fs::read_to_string(root.join("log")).unwrap();
        assert!(log.contains("child-gone"), "{log}");
        assert!(log.contains("row latched"), "{log}");

        // A REFUSAL is not a gone child: the key may be fixable (a re-pair),
        // so the row stays and the next tick retries.
        ledger_row("wrap-1", "a2a-4411-9999");
        let report = pingback_pull_with(&daemon_inv(), |_, _, _, _| {
            Err(FrameReadError {
                code: Some(aoide_protocol::wire::a2a::OUTPUT_READ_REFUSED_CODE),
                message: "output read refused".to_string(),
            })
        });
        assert_eq!(
            report.skipped,
            vec![("wrap-1".to_string(), "pull-failed".to_string())],
            "a refusal is retryable: {report:?}"
        );
        assert!(!drained_of("a2a-4411-9999"), "a refusal must never latch the row");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_gap_delivers_one_honest_marker_and_never_a_fabricated_event() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_SESSION_ID",
        ]);
        let root = setup("pingback-pull-gap");
        register_far_node("nodeb");
        let _listener = headless_parent("wrap-1", "claude");
        ledger_row("wrap-1", "a2a-4411-1790");
        // The parent is four events behind and the ring only kept back to 5.
        aoide_storage::remote_children::advance_lines_after(&remote_key(), "a2a-4411-1790", 4).unwrap();

        let report = pingback_pull_with(&daemon_inv(), move |_, _, after, _| {
            assert_eq!(after, 4);
            Ok(RingRead {
                events: vec![aoide_storage::pingback_remote::RemoteEvent {
                    seq: 9,
                    event: settled("last thing said"),
                    extra: Default::default(),
                }],
                gap: true,
                last: 9,
            })
        });

        let lines: Vec<&str> = report.delivered.iter().map(|(_, l)| l.as_str()).collect();
        assert_eq!(lines.len(), 2, "the marker, then the one event that survived: {report:?}");
        assert_eq!(
            lines[0], "[nodeb/a2a-4411-1790] ping-back gap · 4 events lost before this point",
            "the marker is arithmetic, not a guess"
        );
        assert!(lines[1].contains("settled end_turn"), "{:?}", lines[1]);
        assert_eq!(cursor_of("a2a-4411-1790"), 9);

        // A gap whose answer carries NO event still moves the cursor to `last`
        // — otherwise the same gap would be re-reported every tick forever.
        let report = pingback_pull_with(&daemon_inv(), move |_, _, after, _| {
            assert_eq!(after, 9);
            Ok(RingRead { events: Vec::new(), gap: true, last: 12 })
        });
        let lines: Vec<&str> = report.delivered.iter().map(|(_, l)| l.as_str()).collect();
        assert_eq!(lines, vec!["[nodeb/a2a-4411-1790] ping-back gap · 3 events lost before this point"]);
        assert_eq!(cursor_of("a2a-4411-1790"), 12);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_pull_stops_once_an_exited_has_been_drained() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_SESSION_ID",
        ]);
        let root = setup("pingback-pull-drained");
        register_far_node("nodeb");
        let _listener = headless_parent("wrap-1", "claude");
        ledger_row("wrap-1", "a2a-4411-1790");

        let calls = std::cell::Cell::new(0);
        let report = pingback_pull_with(&daemon_inv(), |_, _, _, _| {
            calls.set(calls.get() + 1);
            Ok(read_of(&[(1, settled("one last thing")), (2, exited())]))
        });
        assert_eq!(calls.get(), 1);
        assert_eq!(report.delivered.len(), 2, "{report:?}");
        assert!(drained_of("a2a-4411-1790"), "the exit was drained, so the row is latched");
        assert_eq!(cursor_of("a2a-4411-1790"), 2);

        // The NEXT tick does not ask again: a ring is never pruned and a child
        // that has left the roster never pushes, so this is forever.
        let report = pingback_pull_with(&daemon_inv(), |_, _, _, _| {
            panic!("a drained child is never pulled again")
        });
        assert_eq!(report, PingbackReport::default(), "{report:?}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_pull_with_no_ledger_row_asks_nothing() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_SESSION_ID",
        ]);
        let root = setup("pingback-pull-nobody");
        register_far_node("nodeb");
        let report = pingback_pull_with(&daemon_inv(), |_, _, _, _| panic!("no rows, no calls"));
        assert_eq!(report, PingbackReport::default());

        // The door gate: only the daemon's own tick pulls (the same boundary
        // `pingback` holds).
        ledger_row("wrap-1", "a2a-4411-1790");
        let mut cli = daemon_inv();
        cli.door = Door::Cli;
        let report = pingback_pull_with(&cli, |_, _, _, _| panic!("a CLI door never pulls"));
        assert_eq!(report, PingbackReport::default());

        // An unresolved node is named, never dialed at a guess.
        let _listener = headless_parent("wrap-1", "claude");
        aoide_storage::node_store::save_nodes(&[]).unwrap();
        let report = pingback_pull_with(&daemon_inv(), |_, _, _, _| panic!("an unknown node is never dialed"));
        assert_eq!(report.skipped, vec![("wrap-1".to_string(), "unknown-node".to_string())]);

        let _ = std::fs::remove_dir_all(&root);
    }
}
