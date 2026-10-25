//! `aoided`'s own tick-loop producers (P-D3, `docs/architecture/AOIDED.md`'s
//! "L1 — the event bus" section): the secrets-feed mirror ("ONE BUS: the
//! secrets mirror") and the #69 hand-edit watcher. Both are plain,
//! bounded, side-effect-scoped structs whose `tick`/`sweep` methods
//! [`crate::daemon::run_loop`] calls once per tick — no threads, no
//! sleeping, no signal handling live in here, the same "thin wrapper
//! touches the real clock/loop, a pure/bounded core does the actual work"
//! split `aoide_protocol::feed`'s own `Follower`/`FeedWriter` already hold
//! (a caller loops; these two structs never loop themselves).
//!
//! ## The secrets mirror
//!
//! [`SecretsMirror`] tails the secrets broker's OWN events feed — its
//! location resolved via `aoide_secrets::socket::socket_path`/`events_path`
//! (this crate already depends on `aoide-secrets` for an unrelated reason,
//! the A2A door's inbound bearer resolve in `a2a.rs`; those two path
//! functions are plain, wire-type-free `PathBuf` resolvers, so importing
//! them is the "no cross-crate copying" convention
//! (`pkgs/aoide/crates/AGENTS.md`) working as intended rather than a
//! boundary this producer needs to route around) — with a
//! [`aoide_protocol::feed::Follower`] and re-publishes every line matching
//! one of the crate's five notable outcomes — `released`/`parked`/
//! `completed`/`dismissed`/`expired` (`aoide-secrets`'s `broker::
//! emit_notify` module doc names these as the crate's ONLY five) — onto
//! the daemon's own feed as a name-only mirror record. **What this module
//! DOES deliberately avoid is `aoide-secrets`'s wire/record TYPES for the
//! parsing half**: each line is parsed as a bare `serde_json::Value`,
//! reading ONLY the four known field names ([`SECRETS_MIRROR_FIELDS`]) —
//! never a typed struct, and never the parsed object wholesale. This is
//! what makes "never copy an unknown field" a STRUCTURAL property of
//! [`mirror_secrets_line`] rather than a discipline someone could quietly
//! break by widening a shared struct: there is no struct to widen. The
//! path resolution carries no such risk (a `PathBuf` has no unknown-field
//! surface to leak), so it is reused rather than re-derived.
//!
//! ## The hand-edit watcher (#69)
//!
//! [`HandEditWatcher`] holds a small (mtime, len) baseline per watched
//! stage file. [`HandEditWatcher::sweep`] returns, once each and in roster
//! order, the files whose on-disk state no longer matches their baseline,
//! and re-baselines them, so a genuine change is reported exactly once.
//!
//! aoided knows its own writes at the rename. [`HandEditWatcher::registered`]
//! registers the watcher with storage's write seam for its roster, so every
//! atomic write to a roster file runs its rename inside
//! [`WriteWatch::around_write`]: the file is observed, the rename runs, and the
//! new state is folded, all under the watcher's lock. A sweep takes the same
//! lock, so it never sees one of aoided's writes between its rename and its
//! fold. Any thread or crate may make the write; no writer calls the watcher.
//! A change aoided did not make keeps its hand-edit line, and so does a foreign
//! write that one of aoided's writes then replaces. The one residual: a foreign
//! write that lands between the watcher's check and its fold, around the rename
//! of aoided's own write, is folded, with one state line and no hand-edit line.
//!
//! ## The state cue
//!
//! The same watcher raises a `class:"state"` line whenever a watched file's
//! `(mtime, len)` moves, whoever moved it: a sweep's observation, or the fold
//! of an atomic write. Its `kind` is `changed`, and its `payload.files`
//! names the file by base name, as the hand-edit line does. Each file gets at
//! most one such line per [`STATE_WINDOW`]. A change that lands inside a file's
//! window owes one more line, written by the first observation after the window
//! closes, so a burst cannot flood the feed and no change is lost. The hand-edit
//! line is unchanged; the state line sits beside it. A desktop view re-reads on
//! the state line (`docs/architecture/AOIDED.md`'s "The first producer" section).

use aoide_protocol::feed::{FeedWriter, Follower};
use aoide_protocol::{audit::now_secs, EventClass};
use aoide_storage::fs::{watch_writes, WriteWatch};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, Once};
use std::time::{Duration, Instant, SystemTime};

// ── the secrets-feed mirror ──────────────────────────────────────────────
//
// The broker's own socket/events-feed path resolution lives ONLY in
// `aoide_secrets::socket::{socket_path, events_path}` — both plain,
// wire-type-free `PathBuf` resolvers this crate already has a dependency
// edge to reach (module doc). `crate::daemon::run_loop` calls them
// directly when constructing a `SecretsMirror`; nothing in this module
// re-derives that resolution.

/// The five broker outcomes this mirror recognizes — `aoide-secrets`'s
/// `broker::emit_notify` module doc's own list, restated here since this
/// module never imports that crate's code to read it from.
const SECRETS_MIRROR_KINDS: [&str; 5] = ["released", "parked", "completed", "dismissed", "expired"];

/// The ONLY field names ever copied out of a parsed secrets-feed line and
/// into a mirror record's `payload` — `id`/`secret`/`consumer`/
/// `timeoutSecs`, `docs/architecture/AOIDED.md`'s "ONE BUS" section's own
/// list. A field not named here can never ride the mirror, structurally —
/// this is the array [`mirror_secrets_line`] loops over instead of ever
/// cloning the parsed object's whole map.
const SECRETS_MIRROR_FIELDS: [&str; 4] = ["id", "secret", "consumer", "timeoutSecs"];

/// Parse one secrets-broker events-feed LINE and build the daemon's own
/// name-only mirror record, or `None` when the line doesn't parse as JSON,
/// carries no recognized `event` field, or names an event outside the five
/// known kinds. Copies fields BY NAME only ([`SECRETS_MIRROR_FIELDS`]) —
/// never the parsed object wholesale — so an unknown extra field on the
/// source line is structurally impossible to mirror (module doc, and the
/// phase's own test requirement).
fn mirror_secrets_line(line: &str) -> Option<Value> {
    let parsed: Value = serde_json::from_str(line).ok()?;
    let event = parsed.get("event").and_then(Value::as_str)?;
    if !SECRETS_MIRROR_KINDS.contains(&event) {
        return None;
    }
    let mut payload = serde_json::Map::new();
    for field in SECRETS_MIRROR_FIELDS {
        if let Some(v) = parsed.get(field) {
            payload.insert(field.to_string(), v.clone());
        }
    }
    Some(json!({
        "v": 0,
        "ts": now_secs(),
        "class": serde_json::to_value(EventClass::Secret).unwrap_or_else(|_| json!("secret")),
        "kind": event,
        "source": "secrets-mirror",
        "payload": Value::Object(payload),
    }))
}

/// Tails the secrets broker's own events feed and re-publishes every
/// recognized line onto the daemon's feed via a caller-supplied
/// [`FeedWriter`]. One instance per daemon process, constructed once at
/// [`crate::daemon::run_loop`] startup and ticked every daemon tick.
pub struct SecretsMirror {
    events_path: PathBuf,
    follower: Option<Follower>,
}

impl SecretsMirror {
    /// `events_path` is resolved ONCE by the caller (this crate's own
    /// "resolve once, pass as a parameter" discipline,
    /// `crate::daemon`'s module doc) — typically `aoide_secrets::socket::
    /// events_path(&aoide_secrets::socket::socket_path())`'s result
    /// (`crate::daemon::run_loop`'s own construction site), though a test
    /// passes an arbitrary tempdir path directly.
    pub fn new(events_path: PathBuf) -> Self {
        Self { events_path, follower: None }
    }

    /// One tick: read every complete NEW line since the last tick and
    /// append a mirror record for each one [`mirror_secrets_line`]
    /// recognizes. The broker being absent, or its events file not
    /// existing yet, is a QUIET skip — [`Follower::open_at_end`] simply
    /// fails and this tick tries again next time (module doc's "ONE BUS"
    /// section: "Broker absent/file missing = quiet skip, retry next
    /// tick").
    pub fn tick(&mut self, feed: &FeedWriter) {
        if self.follower.is_none() {
            self.follower = Follower::open_at_end(&self.events_path).ok();
        }
        let Some(f) = self.follower.as_mut() else { return };
        match f.poll() {
            Ok(lines) => {
                for line in lines {
                    if let Some(mirrored) = mirror_secrets_line(&line) {
                        feed.append(&mirrored);
                    }
                }
            }
            // A read error past the initial open (e.g. the file vanished
            // mid-poll) drops the follower — the next tick's `is_none()`
            // check above retries opening it fresh, the same recovery
            // `stream_subscribe` already gives a dropped subscriber.
            Err(_) => self.follower = None,
        }
    }
}

// ── the #69 hand-edit watcher ────────────────────────────────────────────

/// One file's `(mtime, len)`, or `None` when the file doesn't exist.
type FileStat = Option<(SystemTime, u64)>;

fn stat(path: &Path) -> FileStat {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

fn base_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string_lossy().to_string())
}

/// The window one watched file's `state` cue is coalesced over: at most one
/// line per file inside it (module doc's "The state cue").
const STATE_WINDOW: Duration = Duration::from_secs(1);

/// One watched file's `state`-cue bookkeeping.
#[derive(Default)]
struct StateCue {
    /// When this file's last `state` line was written.
    last: Option<Instant>,
    /// A change landed inside the window `last` opened, and its line is owed.
    owed: bool,
}

impl StateCue {
    fn window_open(&self, now: Instant) -> bool {
        self.last.is_some_and(|t| now.saturating_duration_since(t) < STATE_WINDOW)
    }

    /// A change was observed at `now`. `true`: write its line now. `false`:
    /// the window is open, so the line is owed to the first sweep after it.
    fn changed(&mut self, now: Instant) -> bool {
        if self.window_open(now) {
            self.owed = true;
            return false;
        }
        self.last = Some(now);
        self.owed = false;
        true
    }

    /// `true` once a line is owed and its window has closed: write it now.
    fn take_owed(&mut self, now: Instant) -> bool {
        if !self.owed || self.window_open(now) {
            return false;
        }
        self.last = Some(now);
        self.owed = false;
        true
    }
}

/// One watched file: its roster path, its baseline, its state cue, and whether
/// an observed hand edit is still owed to the next sweep.
struct Watched {
    path: PathBuf,
    baseline: FileStat,
    cue: StateCue,
    owed_edit: bool,
}

/// The `state` line for one watched file (module doc's "The state cue"), in
/// the same envelope as the feed's other lines, with the file's base name in
/// `payload.files`.
fn state_line(path: &Path) -> Value {
    json!({
        "v": 0,
        "ts": now_secs(),
        "class": serde_json::to_value(EventClass::State).unwrap_or_else(|_| json!("state")),
        "kind": "changed",
        "source": "aoided",
        "payload": {"files": [base_name(path)]},
    })
}

/// A tick-driven stat sweep over a fixed roster of broker-owned files
/// (`docs/architecture/AOIDED.md`'s "The first producer: the hand-edit
/// watcher (#69)" section) — [`crate::daemon::stage_roster`] supplies the
/// six real stage-file paths in production; a test passes an arbitrary
/// list of tempfiles instead. It also writes the `state` cue to `feed`.
///
/// It is also the [`WriteWatch`] for its roster once registered
/// ([`HandEditWatcher::registered`]), so a shared instance takes `&self`
/// throughout and keeps its state behind its own lock.
pub struct HandEditWatcher {
    watched: Mutex<Vec<Watched>>,
    feed: FeedWriter,
}

/// Set by the first poisoned-lock recovery in the process, so it says so once.
static POISON_NOTED: Once = Once::new();

/// Observes one watched file at `now`: a stat that moved owes a hand edit and
/// raises its state cue (or owes that line, inside the window). The baseline
/// folds to the new stat, and an owed state line whose window has closed is
/// written.
fn observe(feed: &FeedWriter, w: &mut Watched, now: Instant) {
    let current = stat(&w.path);
    if current != w.baseline {
        w.owed_edit = true;
        if w.cue.changed(now) {
            feed.append(&state_line(&w.path));
        }
    }
    w.baseline = current;
    if w.cue.take_owed(now) {
        feed.append(&state_line(&w.path));
    }
}

impl HandEditWatcher {
    /// The baseline is seeded from each file's CURRENT on-disk state —
    /// nothing fires for a pre-existing file's own history at daemon
    /// startup, only a change the watcher itself observes across two
    /// ticks. `feed` is the daemon's own events feed. This constructor does
    /// not register the watcher: a unit test drives it directly, and the
    /// daemon uses [`Self::registered`].
    pub fn new(files: Vec<PathBuf>, feed: FeedWriter) -> Self {
        let watched = files
            .into_iter()
            .map(|path| Watched { baseline: stat(&path), path, cue: StateCue::default(), owed_edit: false })
            .collect();
        Self { watched: Mutex::new(watched), feed }
    }

    /// The daemon's watcher: [`Self::new`], shared, and registered with
    /// storage's write seam for `files`. The registration lasts as long as the
    /// returned `Arc`.
    pub fn registered(files: Vec<PathBuf>, feed: FeedWriter) -> Arc<Self> {
        let watcher = Arc::new(Self::new(files.clone(), feed));
        let shared: Arc<dyn WriteWatch> = watcher.clone();
        watch_writes(files, Arc::downgrade(&shared));
        watcher
    }

    /// The state, recovering a poisoned lock. The state is a baseline and cues:
    /// a holder that panicked may leave one write unfolded, which the next
    /// observation reports as a hand edit. The first recovery says so once.
    fn state(&self) -> MutexGuard<'_, Vec<Watched>> {
        self.watched.lock().unwrap_or_else(|poisoned| {
            POISON_NOTED.call_once(|| {
                eprintln!("[aoided] the hand-edit watcher's lock was poisoned by a panic; recovering its state");
            });
            poisoned.into_inner()
        })
    }

    /// The write seam's hook for one roster file (`WriteWatch::around_write`):
    /// observes `path` as a sweep does, runs `rename`, then folds the file's
    /// new state with its state cue, all under the lock. A path outside the
    /// roster runs `rename` alone.
    fn around_write_at(
        &self,
        path: &Path,
        now: Instant,
        rename: &mut dyn FnMut() -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        let mut watched = self.state();
        let Some(index) = watched.iter().position(|w| w.path == path) else {
            drop(watched);
            return rename();
        };
        let w = &mut watched[index];
        observe(&self.feed, w, now);
        let result = rename();
        let current = stat(&w.path);
        if current != w.baseline && w.cue.changed(now) {
            self.feed.append(&state_line(&w.path));
        }
        w.baseline = current;
        result
    }

    /// One sweep: observes every roster file, then returns the base filenames
    /// (never the full path — `docs/architecture/AOIDED.md`'s own example
    /// payload, `{"file":"sessions.json"}`) of every file owed a hand edit, in
    /// roster order, each once. A file created, deleted or modified is owed
    /// when its `(mtime, len)` moves off its baseline, so the SAME change is
    /// never reported twice.
    pub fn sweep(&self) -> Vec<String> {
        self.sweep_at(Instant::now())
    }

    fn sweep_at(&self, now: Instant) -> Vec<String> {
        let mut watched = self.state();
        for w in watched.iter_mut() {
            observe(&self.feed, w, now);
        }
        watched
            .iter_mut()
            .filter_map(|w| std::mem::take(&mut w.owed_edit).then(|| base_name(&w.path)))
            .collect()
    }
}

impl WriteWatch for HandEditWatcher {
    fn around_write(&self, path: &Path, rename: &mut dyn FnMut() -> std::io::Result<()>) -> std::io::Result<()> {
        self.around_write_at(path, Instant::now(), rename)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn short_tmp(tag: &str) -> PathBuf {
        let nanos =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().subsec_nanos();
        PathBuf::from(format!("/tmp/av-producers-{tag}-{}-{nanos}", std::process::id()))
    }

    // ── mirror_secrets_line: the five known shapes ──────────────────────

    #[test]
    fn mirrors_released_by_name_only() {
        let line = r#"{"event":"released","secret":"totp","consumer":"melete"}"#;
        let m = mirror_secrets_line(line).expect("released must mirror");
        assert_eq!(m["class"], "secret");
        assert_eq!(m["source"], "secrets-mirror");
        assert_eq!(m["kind"], "released");
        assert_eq!(m["payload"]["secret"], "totp");
        assert_eq!(m["payload"]["consumer"], "melete");
        assert!(m["payload"].get("id").is_none(), "{m}");
    }

    #[test]
    fn mirrors_parked_including_id_and_timeout() {
        let line = r#"{"event":"parked","id":"a3f1-2","secret":"totp","consumer":"melete","timeoutSecs":300}"#;
        let m = mirror_secrets_line(line).expect("parked must mirror");
        assert_eq!(m["kind"], "parked");
        assert_eq!(m["payload"]["id"], "a3f1-2");
        assert_eq!(m["payload"]["secret"], "totp");
        assert_eq!(m["payload"]["consumer"], "melete");
        assert_eq!(m["payload"]["timeoutSecs"], 300);
    }

    #[test]
    fn mirrors_completed() {
        let line = r#"{"event":"completed","id":"9","secret":"db","consumer":"agent"}"#;
        let m = mirror_secrets_line(line).expect("completed must mirror");
        assert_eq!(m["kind"], "completed");
        assert_eq!(m["payload"]["id"], "9");
    }

    #[test]
    fn mirrors_dismissed() {
        let line = r#"{"event":"dismissed","id":"9","secret":"db"}"#;
        let m = mirror_secrets_line(line).expect("dismissed must mirror");
        assert_eq!(m["kind"], "dismissed");
        assert!(m["payload"].get("consumer").is_none(), "{m}");
    }

    #[test]
    fn mirrors_expired() {
        let line = r#"{"event":"expired","id":"9","secret":"db","consumer":"agent"}"#;
        let m = mirror_secrets_line(line).expect("expired must mirror");
        assert_eq!(m["kind"], "expired");
    }

    /// The phase entry's own hard requirement: an unknown extra field on
    /// the source line is structurally impossible to mirror.
    #[test]
    fn unknown_field_never_rides_the_mirror() {
        let line = r#"{"event":"released","secret":"totp","consumer":"melete","value":"super-secret-value","extra":"whatever"}"#;
        let m = mirror_secrets_line(line).expect("released must still mirror");
        let payload = m["payload"].as_object().expect("payload is an object");
        assert_eq!(payload.len(), 2, "only secret+consumer expected: {payload:?}");
        assert!(!payload.contains_key("value"), "{payload:?}");
        assert!(!payload.contains_key("extra"), "{payload:?}");
    }

    #[test]
    fn unrecognized_event_kind_does_not_mirror() {
        assert!(mirror_secrets_line(r#"{"event":"age-identity-minted"}"#).is_none());
        assert!(mirror_secrets_line(r#"{"event":"something-new","secret":"x"}"#).is_none());
    }

    #[test]
    fn malformed_line_does_not_mirror() {
        assert!(mirror_secrets_line("not json at all").is_none());
        assert!(mirror_secrets_line(r#"{"no_event_field":true}"#).is_none());
    }

    // ── SecretsMirror::tick ──────────────────────────────────────────────

    #[test]
    fn secrets_mirror_forwards_matching_lines_onto_the_daemon_feed() {
        let secrets_events = short_tmp("secrets-events").with_extension("jsonl");
        let daemon_events = short_tmp("daemon-events").with_extension("jsonl");
        // Pre-existing HISTORY on the broker's feed, written before this
        // mirror ever starts — `Follower::open_at_end` must never replay
        // it (matches `secrets watch`'s own choice, and every other
        // Follower consumer in this workstream).
        std::fs::write(&secrets_events, b"{\"event\":\"released\",\"secret\":\"pre-existing\",\"consumer\":\"m\"}\n").unwrap();

        let mut mirror = SecretsMirror::new(secrets_events.clone());
        let feed = FeedWriter::new(daemon_events.clone(), 1024 * 1024, 0o600);

        mirror.tick(&feed);
        assert!(!daemon_events.exists() || std::fs::read_to_string(&daemon_events).unwrap().is_empty(), "history before the mirror started must never replay");

        let mut f = std::fs::OpenOptions::new().append(true).open(&secrets_events).unwrap();
        use std::io::Write;
        writeln!(f, r#"{{"event":"released","secret":"t","consumer":"m"}}"#).unwrap();
        writeln!(f, r#"{{"event":"age-identity-minted"}}"#).unwrap(); // never mirrored
        mirror.tick(&feed);

        let contents = std::fs::read_to_string(&daemon_events).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 1, "only the recognized event mirrors: {contents}");
        let mirrored: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(mirrored["kind"], "released");
        assert_eq!(mirrored["source"], "secrets-mirror");

        std::fs::remove_file(&secrets_events).ok();
        std::fs::remove_file(&daemon_events).ok();
    }

    /// Broker absent/file missing: a quiet skip, never a panic, ready to
    /// pick up the moment the file appears on a later tick.
    #[test]
    fn secrets_mirror_quietly_skips_when_the_broker_feed_is_absent() {
        let secrets_events = short_tmp("missing-broker").with_extension("jsonl");
        let daemon_events = short_tmp("missing-broker-daemon").with_extension("jsonl");
        let mut mirror = SecretsMirror::new(secrets_events.clone());
        let feed = FeedWriter::new(daemon_events.clone(), 1024 * 1024, 0o600);

        mirror.tick(&feed); // must not panic
        assert!(!daemon_events.exists(), "nothing should have been appended");

        // The broker "starts" later — the (empty) file appears, and this
        // tick is what successfully opens the follower (module doc's own
        // "retry next tick" contract) — AT its current end, so a line
        // written only AFTER this point is what the next tick must see.
        std::fs::write(&secrets_events, b"").unwrap();
        mirror.tick(&feed);

        let mut f = std::fs::OpenOptions::new().append(true).open(&secrets_events).unwrap();
        use std::io::Write;
        writeln!(f, r#"{{"event":"released","secret":"t","consumer":"m"}}"#).unwrap();
        mirror.tick(&feed);

        let contents = std::fs::read_to_string(&daemon_events).unwrap();
        assert!(contents.contains("\"kind\":\"released\""), "{contents}");

        std::fs::remove_file(&secrets_events).ok();
        std::fs::remove_file(&daemon_events).ok();
    }

    // ── HandEditWatcher ───────────────────────────────────────────────────

    /// A feed over a fresh scratch events file, and that file's path: a
    /// watcher test reads back the `state` lines it raised from the path.
    fn scratch_feed(tag: &str) -> (FeedWriter, PathBuf) {
        let events = short_tmp(&format!("feed-{tag}")).with_extension("jsonl");
        (FeedWriter::new(events.clone(), 1024 * 1024, 0o600), events)
    }

    /// Every `class:"state"` line in `events`, parsed. A missing file has none.
    fn state_lines(events: &Path) -> Vec<Value> {
        std::fs::read_to_string(events)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .filter(|v| v["class"] == "state")
            .collect()
    }

    /// A foreign write that lands before aoided's own write to the same file is
    /// observed at that write's rename, so it stays an owed hand edit: the next
    /// sweep returns the file once, and the sweep after that returns nothing.
    #[test]
    fn a_foreign_write_before_an_own_write_is_returned_once_by_the_next_sweep() {
        let path = short_tmp("foreign-then-own");
        std::fs::write(&path, b"{}").unwrap();
        let (feed, _events) = scratch_feed("foreign-then-own");
        let w = HandEditWatcher::new(vec![path.clone()], feed);
        let t0 = Instant::now();

        std::fs::write(&path, br#"{"out":"of band"}"#).unwrap();
        w.around_write_at(&path, t0 + Duration::from_millis(200), &mut || {
            std::fs::write(&path, br#"{"dispatch":"wrote"}"#)
        })
        .unwrap();

        assert_eq!(w.sweep_at(t0 + Duration::from_millis(300)), vec![base_name(&path)]);
        assert!(w.sweep_at(t0 + Duration::from_millis(400)).is_empty());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn hand_edit_watcher_fires_on_an_out_of_band_mtime_change() {
        let path = short_tmp("hand-edit");
        std::fs::write(&path, b"{}").unwrap();
        let (feed, events) = scratch_feed("hand-edit");
        let w = HandEditWatcher::new(vec![path.clone()], feed);

        // Nothing changed since construction — no false positive on startup.
        assert_eq!(w.sweep(), Vec::<String>::new());

        // An OUT-OF-BAND edit (no write through the seam made it).
        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(&path, b"{\"hand\":\"edited\"}").unwrap();
        let fired = w.sweep();
        assert_eq!(fired, vec![path.file_name().unwrap().to_string_lossy().to_string()]);

        // Re-baselined: sweeping again with no further change is quiet.
        assert_eq!(w.sweep(), Vec::<String>::new());

        std::fs::remove_file(&path).ok();
        std::fs::remove_file(&events).ok();
    }

    /// The daemon's own write, made inside the write seam, must NOT fire on
    /// the next sweep.
    #[test]
    fn hand_edit_watcher_does_not_fire_on_the_daemons_own_write() {
        let path = short_tmp("own-write");
        std::fs::write(&path, b"{}").unwrap();
        let (feed, events) = scratch_feed("own-write");
        let w = HandEditWatcher::new(vec![path.clone()], feed);
        assert_eq!(w.sweep(), Vec::<String>::new());

        std::thread::sleep(std::time::Duration::from_millis(10));
        w.around_write_at(&path, Instant::now(), &mut || std::fs::write(&path, b"{\"daemon\":\"wrote-this\"}")).unwrap();
        assert_eq!(w.sweep(), Vec::<String>::new(), "the daemon's own write must not be reported as a hand edit");

        // A LATER out-of-band edit still fires normally: the seam folds only
        // its own transition, not the file forever.
        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(&path, b"{\"hand\":\"edited-after\"}").unwrap();
        assert_eq!(w.sweep().len(), 1);

        std::fs::remove_file(&path).ok();
        std::fs::remove_file(&events).ok();
    }

    #[test]
    fn hand_edit_watcher_reports_a_deleted_file_once() {
        let path = short_tmp("deleted");
        std::fs::write(&path, b"{}").unwrap();
        let (feed, events) = scratch_feed("deleted");
        let w = HandEditWatcher::new(vec![path.clone()], feed);
        assert_eq!(w.sweep(), Vec::<String>::new());

        std::fs::remove_file(&path).unwrap();
        assert_eq!(w.sweep().len(), 1, "a deletion is itself a change worth reporting");
        assert_eq!(w.sweep(), Vec::<String>::new(), "re-baselined to 'absent' — no repeat");

        std::fs::remove_file(&events).ok();
    }

    #[test]
    fn hand_edit_watcher_sweeps_multiple_files_independently() {
        let a = short_tmp("multi-a");
        let b = short_tmp("multi-b");
        std::fs::write(&a, b"{}").unwrap();
        std::fs::write(&b, b"{}").unwrap();
        let (feed, events) = scratch_feed("multi");
        let w = HandEditWatcher::new(vec![a.clone(), b.clone()], feed);
        assert_eq!(w.sweep(), Vec::<String>::new());

        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(&b, b"{\"changed\":true}").unwrap();
        let fired = w.sweep();
        assert_eq!(fired, vec![b.file_name().unwrap().to_string_lossy().to_string()]);

        std::fs::remove_file(&a).ok();
        std::fs::remove_file(&b).ok();
        std::fs::remove_file(&events).ok();
    }

    /// An out-of-band write is reported as a hand edit (the sweep's return,
    /// which the tick turns into its own line) and raises one `state` line
    /// beside it.
    #[test]
    fn an_out_of_band_write_raises_one_state_line_beside_its_hand_edit() {
        let path = short_tmp("state-hand");
        std::fs::write(&path, b"{}").unwrap();
        let (feed, events) = scratch_feed("state-hand");
        let w = HandEditWatcher::new(vec![path.clone()], feed);
        let t0 = Instant::now();
        assert_eq!(w.sweep_at(t0), Vec::<String>::new());
        assert!(state_lines(&events).is_empty(), "nothing moved, so nothing is cued");

        std::fs::write(&path, b"{\"hand\":\"edited\"}").unwrap();
        assert_eq!(w.sweep_at(t0), vec![base_name(&path)]);
        let state = state_lines(&events);
        assert_eq!(state.len(), 1, "{state:?}");
        assert_eq!(state[0]["kind"], "changed");
        assert_eq!(state[0]["source"], "aoided");
        assert_eq!(state[0]["payload"]["files"], json!([base_name(&path)]));

        std::fs::remove_file(&path).ok();
        std::fs::remove_file(&events).ok();
    }

    /// The daemon's own write raises one `state` line like any other write,
    /// and is never reported back as a hand edit.
    #[test]
    fn a_daemon_own_write_raises_one_state_line_and_no_hand_edit() {
        let path = short_tmp("state-own");
        std::fs::write(&path, b"{}").unwrap();
        let (feed, events) = scratch_feed("state-own");
        let w = HandEditWatcher::new(vec![path.clone()], feed);
        let t0 = Instant::now();
        assert_eq!(w.sweep_at(t0), Vec::<String>::new());

        w.around_write_at(&path, t0, &mut || std::fs::write(&path, b"{\"daemon\":\"wrote-this\"}")).unwrap();
        assert_eq!(w.sweep_at(t0), Vec::<String>::new(), "the daemon's own write must not be reported as a hand edit");
        assert_eq!(state_lines(&events).len(), 1, "{:?}", state_lines(&events));

        std::fs::remove_file(&path).ok();
        std::fs::remove_file(&events).ok();
    }

    /// Writes inside one window raise one line in that window. The change
    /// that landed after that line is written once the window has closed,
    /// and nothing more follows without another change.
    #[test]
    fn a_burst_within_one_second_raises_one_line_then_one_more_once_it_closes() {
        let path = short_tmp("state-burst");
        std::fs::write(&path, b"0").unwrap();
        let (feed, events) = scratch_feed("state-burst");
        let w = HandEditWatcher::new(vec![path.clone()], feed);
        let t0 = Instant::now();
        assert_eq!(w.sweep_at(t0), Vec::<String>::new());

        // Each write changes the length, so each one moves the stat. The first
        // two are aoided's own writes, made inside the seam; the third is
        // foreign.
        w.around_write_at(&path, t0, &mut || std::fs::write(&path, b"01")).unwrap();
        w.around_write_at(&path, t0 + Duration::from_millis(200), &mut || std::fs::write(&path, b"012")).unwrap();
        std::fs::write(&path, b"0123").unwrap();
        assert_eq!(w.sweep_at(t0 + Duration::from_millis(400)), vec![base_name(&path)]);
        assert_eq!(state_lines(&events).len(), 1, "the burst is one line inside its second");

        assert_eq!(w.sweep_at(t0 + Duration::from_millis(1000)), Vec::<String>::new());
        assert_eq!(state_lines(&events).len(), 2, "the change after the first line is owed, not lost");

        assert_eq!(w.sweep_at(t0 + Duration::from_millis(2500)), Vec::<String>::new());
        assert_eq!(state_lines(&events).len(), 2, "nothing is owed without a change");

        std::fs::remove_file(&path).ok();
        std::fs::remove_file(&events).ok();
    }

    /// The cue belongs to a roster file: a path outside the roster raises
    /// nothing, and a roster file's own change still raises its line.
    #[test]
    fn a_path_outside_the_roster_raises_no_state_line() {
        let watched = short_tmp("state-watched");
        let outside = short_tmp("state-outside");
        std::fs::write(&watched, b"{}").unwrap();
        std::fs::write(&outside, b"{}").unwrap();
        let (feed, events) = scratch_feed("state-roster");
        let w = HandEditWatcher::new(vec![watched.clone()], feed);
        let t0 = Instant::now();
        assert_eq!(w.sweep_at(t0), Vec::<String>::new());

        w.around_write_at(&outside, t0, &mut || std::fs::write(&outside, b"{\"x\":1}")).unwrap();
        assert!(state_lines(&events).is_empty(), "{:?}", state_lines(&events));

        std::fs::write(&watched, b"{\"x\":1}").unwrap();
        assert_eq!(w.sweep_at(t0), vec![base_name(&watched)]);
        assert_eq!(state_lines(&events).len(), 1);

        std::fs::remove_file(&watched).ok();
        std::fs::remove_file(&outside).ok();
        std::fs::remove_file(&events).ok();
    }

    /// A sweep started inside the rename waits for the fold, so once the write
    /// returns it has nothing to report, and the own write raised one state
    /// line.
    #[test]
    fn no_sweep_runs_inside_a_rename() {
        let path = short_tmp("no-sweep-in-rename");
        std::fs::write(&path, b"{}").unwrap();
        let (feed, events) = scratch_feed("no-sweep-in-rename");
        let w = Arc::new(HandEditWatcher::new(vec![path.clone()], feed));
        let t0 = Instant::now();
        let mut swept = None;

        w.around_write_at(&path, t0, &mut || {
            std::fs::write(&path, b"{\"own\":\"write\"}")?;
            let sweeper = Arc::clone(&w);
            swept = Some(std::thread::spawn(move || sweeper.sweep_at(t0)));
            Ok(())
        })
        .unwrap();

        let returned = swept.expect("the rename started a sweep").join().unwrap();
        assert!(returned.is_empty(), "the sweep waited for the fold, so it sees no change: {returned:?}");
        assert_eq!(state_lines(&events).len(), 1, "{:?}", state_lines(&events));
        std::fs::remove_file(&path).ok();
        std::fs::remove_file(&events).ok();
    }

    /// A registered watcher observes its roster's atomic writes for as long as
    /// one `Arc` of it lives, and never after its last one drops.
    #[test]
    fn a_registered_watcher_observes_writes_until_its_last_arc_drops() {
        let path = short_tmp("registered");
        std::fs::write(&path, b"{}").unwrap();
        let (feed, events) = scratch_feed("registered");
        let watcher = HandEditWatcher::registered(vec![path.clone()], feed);

        aoide_storage::fs::atomic_write(&path, "{\"own\":1}").unwrap();
        assert_eq!(state_lines(&events).len(), 1, "{:?}", state_lines(&events));

        drop(watcher);
        aoide_storage::fs::atomic_write(&path, "{\"own\":\"after the drop\"}").unwrap();
        assert_eq!(state_lines(&events).len(), 1, "a dropped watcher observes nothing: {:?}", state_lines(&events));
        std::fs::remove_file(&path).ok();
        std::fs::remove_file(&events).ok();
    }
}
