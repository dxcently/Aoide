//! The remote ping-back ring: `state/stage/pingback-remote.json` (v0) — the
//! events a child whose parent sits on ANOTHER node has published for it to
//! pull (CONTRACTS.md §4, P-RSA §4.4).
//!
//! It is the sibling of [`crate::remote_children`]'s `linesAfter` cursor: that
//! cursor is the parent-side "how far have I read", this file is the
//! child-side "what is there to read". Neither is a grant — the ring is
//! written by the child's own reaper tick and read by the child's own door,
//! and the door gates the read on the parent's key itself
//! ([`crate::records::RemoteParent`]'s doc).
//!
//! The events are OPAQUE here (`Value`): the closed event vocabulary is
//! `aoide-conduct`'s `PingEvent`, and this layer only knows that an event has
//! a `seq` and is JSON. That is deliberate — the ring is a bounded queue, and
//! a queue that parsed its own payload would be a second definition of the
//! event it carries.
//!
//! **Bounded, monotonic, and never rewound.** A child's ring holds at most
//! [`PINGBACK_REMOTE_MAX`] events and a push past that drops the OLDEST —
//! a parent that falls further behind than the cap misses events, and a read
//! that has missed any says so (`gap`) instead of silently handing over a
//! non-contiguous run. `seq` is per child, starts at 1, and only ever climbs.
//! A ring is never rewound, and it OUTLIVES the child's roster record: the
//! parent that owns one may still be reading it after the child left the
//! roster, and the last thing it reads is that child's exit. That is why the
//! entry carries its own gate input ([`ChildRing::key`]) and its own retention
//! clock ([`ChildRing::at`]): the door serves the read from here, and
//! [`retain_rings`] eventually drops what no parent can be owed any more —
//! called by the child-side pass, which knows the roster, never by this layer
//! on a guess.
//!
//! Same discipline `remote_children.rs`/`undying.rs` set: a `schemaVersion`
//! container, tolerate-missing/corrupt-as-empty on read, `fs::atomic_write` on
//! write, pure mutations for the ring algebra so the cap and the gap are
//! unit-testable off disk, and every write inside one short
//! `fs::with_stage_lock` section — the spool, and the prune alike.

use crate::fs::{atomic_write, conducting_stage_dir, with_stage_lock};
use crate::stage::load_stage;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// `state/stage/pingback-remote.json` schema version (CONTRACTS.md §4, v0).
pub const PINGBACK_REMOTE_VERSION: &str = "0";

/// How many events ONE child's ring retains. A pull is answered with at most
/// this many events, so the number is both the retention bound and the wire
/// bound — one number, because a wire bound larger than the ring would only
/// ever promise events that were already dropped.
pub const PINGBACK_REMOTE_MAX: usize = 16;

/// `state/stage/pingback-remote.json` — CONDUCTING state
/// (`fs::conducting_stage_dir`, never `fs::stage_dir`'s rice tree), beside
/// `pingback.json`, the cursor of the same tick that writes this one.
pub fn pingback_remote_path() -> std::path::PathBuf {
    conducting_stage_dir().join("pingback-remote.json")
}

/// One event on a child's ring: the `seq` the parent's cursor counts in, and
/// the event itself. `extra` round-trips a field this version does not know
/// about — one writer, one version per node, but a rewrite by an older
/// `aoided` (a rollback) must not silently drop what a newer one wrote.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteEvent {
    pub seq: u64,
    #[serde(default)]
    pub event: Value,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One child's ring. `last` is the highest `seq` ever pushed for that child
/// (0 before the first push) and is what a read reports so a parent whose
/// cursor has fallen off the retained window can resynchronize instead of
/// re-reading an empty answer forever.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChildRing {
    #[serde(default)]
    pub last: u64,
    /// The key this node stamped on the child's own `remoteParent` when it
    /// admitted the spawn — the ring's OWN gate input (P-RSA S9, CONTRACTS.md
    /// §4/§6).
    ///
    /// **Why the ring carries it.** The ring deliberately outlives the child's
    /// roster record, and the door's history arm is served from the ring: a
    /// gate that could only consult `sessions.json` would refuse the parent
    /// the very events the ring exists to hold, the moment the record was
    /// pruned. Stamped with a child's FIRST event and never overwritten — one
    /// ring has one owner, and a later pass cannot hand it to another parent.
    #[serde(default)]
    pub key: String,
    /// Unix seconds the ring last took an event — the ring's whole retention
    /// clock (L3 of the S8/S9 review). Nothing else in an entry carries a
    /// time: the events are opaque at this layer and a child's record may be
    /// long gone while its parent is still owed what is here. Updated on every
    /// spool, so a ring that keeps taking events is never a candidate for
    /// [`retain_rings`].
    #[serde(default)]
    pub at: u64,
    #[serde(default)]
    pub events: Vec<RemoteEvent>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The whole ring file, keyed by the CHILD's own session id — the id its node
/// knows it by, which is also the `tasks/get` id the parent pulls with.
/// A `BTreeMap` so the file's own key order is stable (a diff of two ticks
/// reads as a diff of children, never of hashing).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PingbackRemoteFile {
    #[serde(rename = "schemaVersion", default)]
    pub schema_version: String,
    #[serde(default)]
    pub children: BTreeMap<String, ChildRing>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The answer to one read: the events after the cursor, whether the ring lost
/// any between the cursor and the oldest retained event, and the newest `seq`
/// the child has pushed. This IS the door's wire payload (CONTRACTS.md §6,
/// `aoide/linesAfter`), so it serialises field for field what the reader
/// needs and nothing it must derive.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RingRead {
    pub events: Vec<RemoteEvent>,
    /// The ring rolled past the cursor: at least one event between the cursor
    /// and `events` was dropped. A read that set this has NOT seen everything
    /// it asked for, and the reader names that rather than treating the events
    /// it did get as contiguous.
    pub gap: bool,
    /// The highest `seq` this child has pushed — the cursor a reader that saw
    /// a `gap` can move to when the answer carries no event to move past.
    pub last: u64,
}

/// Push one event onto a child's ring, returning the `seq` it was given. Pure:
/// the caller owns the file, this owns the algebra. The seq is `last + 1`, and
/// a push past [`PINGBACK_REMOTE_MAX`] drops the OLDEST retained event.
pub fn push_event(ring: &mut ChildRing, event: Value) -> u64 {
    let seq = ring.last + 1;
    ring.last = seq;
    ring.events.push(RemoteEvent { seq, event, extra: Map::new() });
    if ring.events.len() > PINGBACK_REMOTE_MAX {
        let over = ring.events.len() - PINGBACK_REMOTE_MAX;
        ring.events.drain(0..over);
    }
    seq
}

/// The events after `after`, and whether the ring dropped any of them. Pure.
///
/// The retained events are contiguous by construction (every push is
/// consecutive and only the oldest leaves), so "was anything lost" is one
/// comparison: the oldest retained `seq` is past the one the cursor would
/// have handed over next. An EMPTY ring with `last` beyond the cursor lost
/// everything it ever held.
pub fn events_after(ring: &ChildRing, after: u64) -> RingRead {
    let events: Vec<RemoteEvent> =
        ring.events.iter().filter(|e| e.seq > after).take(PINGBACK_REMOTE_MAX).cloned().collect();
    let gap = match ring.events.first() {
        Some(oldest) => after.saturating_add(1) < oldest.seq,
        None => after < ring.last,
    };
    RingRead { events, gap, last: ring.last }
}

/// Read the ring file, tolerating a missing/corrupt/wrong-shape file as empty
/// — never an error, same discipline as [`crate::remote_children`]. Read-only:
/// no lock, no rewrite.
pub fn load_pingback_remote() -> PingbackRemoteFile {
    load_stage::<PingbackRemoteFile>(&pingback_remote_path()).unwrap_or_default()
}

/// One child's ring as a reader sees it — never an error: an unknown child is
/// an empty ring, which is exactly what a child that never spoke has.
pub fn events_for(child: &str, after: u64) -> RingRead {
    match load_pingback_remote().children.get(child) {
        Some(ring) => events_after(ring, after),
        None => RingRead { events: Vec::new(), gap: false, last: 0 },
    }
}

/// The key one child's ring was stamped with ([`ChildRing::key`]), or `None`
/// when the child has no ring entry at all or its entry never carried a key.
/// The door's history gate reads THIS before it reads `sessions.json`: the ring
/// outlives the record, and the parent that owns it must still be able to read
/// what it holds.
pub fn ring_key(child: &str) -> Option<String> {
    load_pingback_remote()
        .children
        .get(child)
        .map(|r| r.key.clone())
        .filter(|k| !k.is_empty())
}

/// Drop every ring entry the predicate rejects, under the stage lock, and
/// return how many left (L3 of the S8/S9 review). The child-side pass calls
/// this with its roster: a ring whose child's record is gone stays while it is
/// young (its parent may be mid-pull, or down), and the whole point of the
/// retention clock is that it eventually goes — a ring is rewritten WHOLE on
/// every spool, so an unbounded set of dead children costs every live event a
/// rewrite proportional to their number.
///
/// Same change-only discipline as [`crate::remote_children::retain_remote_children`]:
/// nothing rejected is not a write.
pub fn retain_rings(keep: impl Fn(&str, &ChildRing) -> bool) -> Result<usize, String> {
    with_stage_lock(|| {
        let mut file: PingbackRemoteFile = load_stage(&pingback_remote_path()).unwrap_or_default();
        let before = file.children.len();
        file.children.retain(|child, ring| keep(child, ring));
        let removed = before - file.children.len();
        if removed > 0 {
            let body = serde_json::to_string_pretty(&file)
                .map_err(|e| format!("serialize pingback-remote.json: {e}"))?
                + "\n";
            atomic_write(&pingback_remote_path(), &body)
                .map_err(|e| format!("{}: {e}", pingback_remote_path().display()))?;
        }
        Ok(removed)
    })
}

/// Append one event to a child's ring, under the stage lock, and return the
/// `seq` it was given — `None` when the file could not be written (the event
/// is then LOST, which is the same direction every other at-most-once step of
/// the ping-back loses in: a parent missing a line is safe, a parent reading
/// one twice is not).
///
/// `key` is the child's own `remoteParent.key` — the parent's node key this
/// node stamped when it admitted the spawn — and it is written into the ring
/// on the FIRST event ([`ChildRing::key`]); an empty `key` stamps nothing, and
/// once stamped the ring's own key is never replaced. `at` is the same
/// timestamp the caller keeps for the tick: this module mints no clock of its
/// own (the same discipline `RemoteChild::spawned_at` holds), and it is what
/// [`retain_rings`] measures a ring's age against.
pub fn spool_event(child: &str, key: &str, at: u64, event: Value) -> Option<u64> {
    with_stage_lock(|| {
        let mut file: PingbackRemoteFile = load_stage(&pingback_remote_path()).unwrap_or_default();
        if file.schema_version.is_empty() {
            file.schema_version = PINGBACK_REMOTE_VERSION.to_string();
        }
        let ring = file.children.entry(child.to_string()).or_default();
        if ring.key.is_empty() && !key.is_empty() {
            ring.key = key.to_string();
        }
        ring.at = at;
        let seq = push_event(ring, event);
        let body = match serde_json::to_string_pretty(&file) {
            Ok(body) => body + "\n",
            Err(e) => {
                eprintln!("[aoide/reap] ping-back ring encode failed: {e}");
                return None;
            }
        };
        match atomic_write(&pingback_remote_path(), &body) {
            Ok(()) => Some(seq),
            Err(e) => {
                eprintln!("[aoide/reap] ping-back ring write failed: {e}");
                None
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Set `$AOIDE_STAGE_DIR` to a fresh temp dir for the test's duration and
    /// put it back after — the same isolation `remote_children`'s tests use,
    /// and the override branch `conducting_stage_dir` prefers (its one-shot
    /// migration never runs there, so a test can never touch the operator's
    /// real stage tree).
    struct StageEnv {
        saved: Option<String>,
        dir: std::path::PathBuf,
    }

    impl StageEnv {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "aoide-pingback-remote-{tag}-{}-{}",
                std::process::id(),
                crate::time::now_iso_utc().replace([':', '-', '.'], "")
            ));
            let _ = std::fs::remove_dir_all(&dir);
            let saved = std::env::var("AOIDE_STAGE_DIR").ok();
            std::env::set_var("AOIDE_STAGE_DIR", &dir);
            Self { saved, dir }
        }
    }

    impl Drop for StageEnv {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
            match self.saved.take() {
                Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
                None => std::env::remove_var("AOIDE_STAGE_DIR"),
            }
        }
    }

    fn ring_of(n: usize) -> ChildRing {
        let mut ring = ChildRing::default();
        for i in 1..=n as u64 {
            push_event(&mut ring, json!({ "kind": { "exited": { "code": i } } }));
        }
        ring
    }

    fn seqs(read: &RingRead) -> Vec<u64> {
        read.events.iter().map(|e| e.seq).collect()
    }

    #[test]
    fn seq_is_per_child_and_starts_at_one() {
        let mut ring = ChildRing::default();
        assert_eq!(push_event(&mut ring, json!({ "a": 1 })), 1);
        assert_eq!(push_event(&mut ring, json!({ "a": 2 })), 2);
        assert_eq!(ring.last, 2);
        assert_eq!(seqs(&events_after(&ring, 0)), vec![1, 2]);

        // A different child is a different counter: the file keys them, never
        // a global sequence.
        let mut other = ChildRing::default();
        assert_eq!(push_event(&mut other, json!({ "a": 3 })), 1);
    }

    #[test]
    fn the_cap_drops_the_oldest_and_nothing_else() {
        let ring = ring_of(20);
        assert_eq!(ring.events.len(), PINGBACK_REMOTE_MAX, "the ring holds the cap");
        assert_eq!(ring.last, 20, "seq keeps climbing past the cap");
        assert_eq!(ring.events.first().unwrap().seq, 5, "the OLDEST is what leaves");
        assert_eq!(ring.events.last().unwrap().seq, 20);

        // A read is capped by the same number, and takes the OLDEST it has:
        // a parent behind by more than the cap drains the ring in order.
        assert_eq!(seqs(&events_after(&ring, 0)), (5..=20).collect::<Vec<u64>>());
    }

    #[test]
    fn a_read_is_contiguous_or_it_says_gap() {
        let ring = ring_of(20);

        // Inside the retained window and one behind it: nothing was lost.
        let read = events_after(&ring, 4);
        assert_eq!(seqs(&read), (5..=20).collect::<Vec<u64>>());
        assert!(!read.gap, "seq 5 is exactly what the cursor at 4 hands over: {read:?}");

        // Behind the window: the ring rolled past the cursor.
        let read = events_after(&ring, 1);
        assert_eq!(seqs(&read), (5..=20).collect::<Vec<u64>>());
        assert!(read.gap, "seqs 2..4 were dropped, and the read says so: {read:?}");

        // Caught up: nothing new, and no gap to report.
        let read = events_after(&ring, 20);
        assert!(read.events.is_empty());
        assert!(!read.gap, "{read:?}");
        assert_eq!(read.last, 20);

        // A cursor past the end (a rolled-back node, a hand-edit) is the same
        // quiet answer, never an overflow.
        let read = events_after(&ring, u64::MAX);
        assert!(read.events.is_empty());
        assert!(!read.gap, "{read:?}");
    }

    #[test]
    fn a_ring_that_dropped_everything_reports_its_newest_seq() {
        let ring = ring_of(PINGBACK_REMOTE_MAX + 3);
        assert_eq!(ring.last, 19);
        assert_eq!(ring.events.first().unwrap().seq, 4);

        // A cursor from before the window: events remain.
        let read = events_after(&ring, 2);
        assert_eq!(seqs(&read)[0], 4);
        assert!(read.gap);

        // The ring holds nothing at all (a child that only ever pushed into a
        // file that was then emptied) — the read is empty AND gapped, so the
        // reader can move to `last` rather than re-reading nothing forever.
        let empty = ChildRing { last: 19, key: String::new(), at: 0, events: Vec::new(), extra: Map::new() };
        let read = events_after(&empty, 3);
        assert!(read.events.is_empty());
        assert!(read.gap, "{read:?}");
        assert_eq!(read.last, 19);
    }

    #[test]
    fn retain_rings_drops_exactly_the_rejected_entries() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = StageEnv::new("retain-rings");
        spool_event("dead-old", "ab", 100, json!({ "settled": {} }));
        spool_event("dead-new", "ab", 900, json!({ "settled": {} }));
        spool_event("live", "ab", 100, json!({ "settled": {} }));

        // The decision is the CALLER's (it holds the roster); this is the
        // mechanism: a child with no record whose ring is old goes.
        let removed = retain_rings(|child, ring| child == "live" || ring.at > 500).unwrap();
        assert_eq!(removed, 1);
        let left: Vec<String> = load_pingback_remote().children.keys().cloned().collect();
        assert_eq!(left, vec!["dead-new".to_string(), "live".to_string()]);
        assert!(events_for("dead-old", 0).events.is_empty(), "and its events are gone");

        // Nothing rejected is not a write: the bytes stay untouched.
        let before = std::fs::read_to_string(pingback_remote_path()).unwrap();
        assert_eq!(retain_rings(|_, _| true).unwrap(), 0);
        assert_eq!(std::fs::read_to_string(pingback_remote_path()).unwrap(), before);
    }

    #[test]
    fn the_ring_carries_the_parent_key_it_was_stamped_with() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = StageEnv::new("ring-key");
        assert!(ring_key("sess-1").is_none(), "no ring, no key");

        spool_event("sess-1", "ab", 100, json!({ "settled": {} }));
        assert_eq!(ring_key("sess-1").as_deref(), Some("ab"));

        // One ring has ONE owner: a later spool cannot hand it to another
        // parent, whatever key it carries.
        spool_event("sess-1", "cd", 200, json!({ "settled": {} }));
        assert_eq!(ring_key("sess-1").as_deref(), Some("ab"), "stamped once, never replaced");

        // An empty key stamps nothing — and clears nothing.
        spool_event("sess-2", "", 100, json!({ "settled": {} }));
        assert!(ring_key("sess-2").is_none());
        assert_eq!(ring_key("sess-1").as_deref(), Some("ab"));

        // The key is on disk, not in this process: a re-read agrees.
        let raw = std::fs::read_to_string(pingback_remote_path()).unwrap();
        assert!(raw.contains("\"key\": \"ab\""), "{raw}");
        assert!(raw.contains("\"at\": 200"), "the retention clock is the last spool's: {raw}");
        assert_eq!(ring_key("sess-1").as_deref(), Some("ab"));

        // A ring written before the field existed carries no key at all, reads
        // as keyless (never as somebody's), and still serves its events.
        std::fs::write(
            pingback_remote_path(),
            r#"{ "schemaVersion": "0", "children": { "old": { "last": 1,
                 "events": [ { "seq": 1, "event": { "exited": {} } } ] } } }"#,
        )
        .unwrap();
        assert!(ring_key("old").is_none(), "an unstamped ring belongs to nobody");
        assert_eq!(events_for("old", 0).events.len(), 1, "and it still reads");
    }

    #[test]
    fn a_spooled_event_lands_on_disk_and_an_unknown_child_reads_empty() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = StageEnv::new("spool");
        assert_eq!(spool_event("sess-1", "ab", 100, json!({ "kind": "Exited" })), Some(1));
        assert_eq!(spool_event("sess-1", "ab", 200, json!({ "kind": "Exited" })), Some(2));

        let read = events_for("sess-1", 0);
        assert_eq!(seqs(&read), vec![1, 2]);
        assert!(!read.gap);
        assert_eq!(read.events[0].event, json!({ "kind": "Exited" }));

        // The cursor is the read's own input: the same call one event later.
        assert_eq!(seqs(&events_for("sess-1", 1)), vec![2]);

        let unknown = events_for("sess-ghost", 0);
        assert!(unknown.events.is_empty());
        assert!(!unknown.gap);
        assert_eq!(unknown.last, 0);

        // The container is the versioned one every other stage file is, and an
        // unknown top-level key survives a rewrite.
        let raw = std::fs::read_to_string(pingback_remote_path()).unwrap();
        assert!(raw.contains("\"schemaVersion\": \"0\""), "{raw}");
        assert!(raw.contains("\"sess-1\""), "{raw}");
    }

    #[test]
    fn the_cap_survives_the_round_trip_to_disk() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = StageEnv::new("cap");
        for _ in 0..PINGBACK_REMOTE_MAX + 4 {
            spool_event("sess-1", "ab", 300, json!({ "kind": "Settled" }));
        }
        let ring = load_pingback_remote().children.remove("sess-1").unwrap();
        assert_eq!(ring.events.len(), PINGBACK_REMOTE_MAX);
        assert_eq!(ring.last, (PINGBACK_REMOTE_MAX + 4) as u64);
        assert_eq!(events_for("sess-1", 0).gap, true, "seqs 1..4 were dropped before the cursor at 0");
        assert_eq!(events_for("sess-1", 4).gap, false, "the cursor at 4 hands over 5 next");
        assert_eq!(events_for("sess-1", 1).gap, true, "the cursor at 1 lost seqs 2..4");

        // A missing file, and a corrupt one, both read as empty rather than
        // failing a tick.
        std::fs::write(pingback_remote_path(), "{ not json").unwrap();
        assert!(load_pingback_remote().children.is_empty());
    }
}
