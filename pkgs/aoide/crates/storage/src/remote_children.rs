//! The remote-children ledger: `state/stage/remote-children.json` (v0) — the
//! children THIS node spawned on OTHER nodes over their A2A door, one entry
//! per child (CONTRACTS.md §4).
//!
//! It is the caller-side mirror of [`crate::records::RemoteParent`]: the
//! child's own node records who its parent is, and this file records, on the
//! parent's own node, which children that parent asked for. Neither is a
//! grant — both are same-uid attribution, gated only by the key comparison
//! the door makes against the verifying node ([`crate::records::RemoteParent`]'s
//! doc; the `origin` precedent in CONTRACTS.md §4).
//!
//! Keyed by CHILD, and the parent's own record is never touched: that
//! record's writer is its own wrap, so a second writer would be exactly the
//! shape this whole field split exists to avoid. An entry leaves the file when
//! the parent leaves the roster — `aoide-conduct`'s
//! `graph/doc.rs::drop_remote_child_rows`, called by both roster-exit paths
//! once their `sessions.json` write has landed, via
//! [`retain_remote_children`]).
//!
//! Same discipline `undying.rs`/`node_store.rs` set: a `schemaVersion`
//! container, tolerate-missing/corrupt-as-empty on read, `fs::atomic_write`
//! on write, pure mutations for the CRUD so the shape is unit-testable off
//! disk. Every mutation runs inside one short `fs::with_stage_lock` section
//! (a read-modify-write of one file, same as every other stage mutator), and
//! [`append_remote_child`] is idempotent on the child's identity
//! `(key, sessionId)` — a re-acked spawn never doubles an entry.

use crate::fs::{atomic_write, conducting_stage_dir, with_stage_lock};
use crate::stage::load_stage;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// `state/stage/remote-children.json` schema version (CONTRACTS.md §4, v0).
pub const REMOTE_CHILDREN_VERSION: &str = "0";

/// The longest claimed session id a caller may present — CONTRACTS.md §6's
/// `aoide/from` bound. A `String` this long is already past every id this
/// repo mints (`conduct-<pid>-<epoch>`).
pub const CLAIMED_SESSION_ID_MAX: usize = 128;

/// One child this node spawned elsewhere. `parentSessionId` is this node's
/// OWN local parent (the session that asked for the spawn), so the same child
/// stays attributable after a restart; `key`/`sessionId` identify the child,
/// and `node` is the display label known at spawn time. `extra` round-trips
/// any field this version does not know about — one writer, one version per
/// node, but a rewrite by an older `aoided` (a rollback) must not silently
/// drop a field a newer one wrote, the tolerance [`crate::records::RemoteParent`]
/// and `SessionRecord` hold.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteChild {
    #[serde(rename = "parentSessionId")]
    pub parent_session_id: String,
    #[serde(default)]
    pub node: String,
    #[serde(default)]
    pub key: String,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "spawnedAt", default)]
    pub spawned_at: String,
    /// The remote ping-back pull cursor: the highest event `seq` this node
    /// has already delivered to `parentSessionId` (CONTRACTS.md §4). Advanced
    /// by [`advance_lines_after`] BEFORE the line is delivered, so a crash
    /// between the two loses a line rather than duplicating one — the same
    /// at-most-once direction `pingback.json`'s cursor holds.
    #[serde(rename = "linesAfter", default)]
    pub lines_after: u64,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The `state/stage/remote-children.json` container — `extra` for the same
/// reason [`RemoteChild::extra`] exists, one level up: an unknown top-level
/// key survives a rewrite by this version.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RemoteChildrenFile {
    #[serde(rename = "schemaVersion", default)]
    pub schema_version: String,
    #[serde(default)]
    pub children: Vec<RemoteChild>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The ledger path: `state/stage/remote-children.json` — CONDUCTING state
/// (`fs::conducting_stage_dir`, never `fs::stage_dir`'s rice tree), beside
/// `pingback.json`/`taskreport.json`, the two other cursors the daemon tick
/// keeps.
pub fn remote_children_path() -> std::path::PathBuf {
    conducting_stage_dir().join("remote-children.json")
}

/// Two entries naming the same child. The child's identity is the pair the
/// door verifies — the node key that owns it and the session id under that
/// key — never the label, and never the parent (one child on a far node is
/// spawned by one parent).
fn same_child(a: &RemoteChild, b: &RemoteChild) -> bool {
    a.key == b.key && a.session_id == b.session_id
}

/// Is `claim` a legal `aoide/from` claim (CONTRACTS.md §6)? 1..=128 bytes of
/// `[A-Za-z0-9._:-]`, no `/`. Pure, shared by the client (which refuses its
/// own unruly claim before signing it) and the door (which refuses one a
/// caller signed anyway) — the same predicate on both sides, never a second
/// spelling.
pub fn valid_claimed_session_id(claim: &str) -> bool {
    !claim.is_empty()
        && claim.len() <= CLAIMED_SESSION_ID_MAX
        && claim
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-'))
}

/// Read the ledger, tolerating a missing/corrupt/wrong-shape file as empty —
/// never an error, same discipline as `load_undying`. Read-only: no lock, no
/// rewrite.
pub fn load_remote_children() -> Vec<RemoteChild> {
    load_stage::<RemoteChildrenFile>(&remote_children_path())
        .map(|f| f.children)
        .unwrap_or_default()
}

/// Append one child, under the stage lock; `Ok(false)` if that child is
/// already recorded (idempotent on [`same_child`], so a re-acked spawn is a
/// no-op rather than a duplicate row). The caller supplies `spawned_at`
/// filled in — this module mints no timestamp of its own.
pub fn append_remote_child(child: &RemoteChild) -> Result<bool, String> {
    with_stage_lock(|| {
        let mut file: RemoteChildrenFile = load_stage(&remote_children_path()).unwrap_or_default();
        if file.children.iter().any(|c| same_child(c, child)) {
            return Ok(false);
        }
        file.children.push(child.clone());
        save(&mut file)?;
        Ok(true)
    })
}

/// Drop every entry the predicate rejects, under the stage lock; returns how
/// many were removed. The roster-exit paths use this to clear the children of a
/// parent that has left the roster.
pub fn retain_remote_children(keep: impl Fn(&RemoteChild) -> bool) -> Result<usize, String> {
    with_stage_lock(|| {
        let mut file: RemoteChildrenFile = load_stage(&remote_children_path()).unwrap_or_default();
        let before = file.children.len();
        file.children.retain(|c| keep(c));
        let removed = before - file.children.len();
        if removed > 0 {
            save(&mut file)?;
        }
        Ok(removed)
    })
}

/// Advance one child's pull cursor to `seq`, under the stage lock. FORWARD
/// ONLY: a `seq` at or below the stored cursor is a no-op, so a replayed pull
/// can never rewind the cursor and re-deliver a line. A no-op for an unknown
/// child (nothing to advance).
pub fn advance_lines_after(key: &str, session_id: &str, seq: u64) -> Result<bool, String> {
    with_stage_lock(|| {
        let mut file: RemoteChildrenFile = load_stage(&remote_children_path()).unwrap_or_default();
        let Some(entry) = file
            .children
            .iter_mut()
            .find(|c| c.key == key && c.session_id == session_id)
        else {
            return Ok(false);
        };
        if seq <= entry.lines_after {
            return Ok(false);
        }
        entry.lines_after = seq;
        save(&mut file)?;
        Ok(true)
    })
}

/// Write the container back, stamping the version. The file always carries a
/// non-empty `children` by the time this is called.
fn save(file: &mut RemoteChildrenFile) -> Result<(), String> {
    if file.schema_version.is_empty() {
        file.schema_version = REMOTE_CHILDREN_VERSION.to_string();
    }
    let body = serde_json::to_string_pretty(file)
        .map_err(|e| format!("serialize remote-children.json: {e}"))?
        + "\n";
    atomic_write(&remote_children_path(), &body)
        .map_err(|e| format!("{}: {e}", remote_children_path().display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Set `$AOIDE_STAGE_DIR` to a fresh temp dir for the test's duration and
    /// put it back after — the same isolation `node_store`'s temp-state tests
    /// use. `conducting_stage_dir` prefers that override, and its one-shot
    /// migration never runs on the override branch, so a test can never touch
    /// the operator's real stage tree.
    struct StageEnv {
        saved: Option<String>,
        dir: std::path::PathBuf,
    }

    impl StageEnv {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "aoide-remote-children-{tag}-{}-{}",
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

    fn child(parent: &str, node: &str, key: &str, id: &str) -> RemoteChild {
        RemoteChild {
            parent_session_id: parent.to_string(),
            node: node.to_string(),
            key: key.to_string(),
            session_id: id.to_string(),
            spawned_at: "2026-09-25T00:00:00Z".to_string(),
            lines_after: 0,
            extra: Default::default(),
        }
    }

    #[test]
    fn unknown_fields_survive_a_ledger_rewrite() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = StageEnv::new("extra");
        std::fs::create_dir_all(conducting_stage_dir()).unwrap();
        // A ledger written by a LATER version: one unknown top-level key, one
        // unknown key inside a child.
        std::fs::write(
            remote_children_path(),
            r#"{ "schemaVersion": "0", "futureTop": "kept",
                 "children": [ { "parentSessionId": "par1", "node": "nodeb", "key": "aa",
                                 "sessionId": "c1", "spawnedAt": "2026-09-25T00:00:00Z",
                                 "linesAfter": 0, "futureCursor": 3 } ] }"#,
        )
        .unwrap();

        // The rewrite under test is a real one: the cursor advance re-reads
        // the whole file and writes it back through `save`.
        assert_eq!(advance_lines_after("aa", "c1", 5).unwrap(), true);
        let raw = std::fs::read_to_string(remote_children_path()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["futureTop"], "kept", "an unknown TOP-LEVEL key survives the rewrite: {raw}");
        assert_eq!(v["children"][0]["futureCursor"], 3, "so does an unknown CHILD key: {raw}");
        assert_eq!(v["children"][0]["linesAfter"], 5, "and the known field still moved: {raw}");

        // The typed read agrees: exactly one child, its `extra` carrying the
        // unknown key and nothing fabricated in it.
        let mut children = load_remote_children();
        assert_eq!(children.len(), 1);
        let c = children.remove(0);
        assert_eq!(c.extra.get("futureCursor"), Some(&serde_json::json!(3)));
        assert_eq!(c.extra.len(), 1, "the known fields stay named fields, never extra entries");
    }

    #[test]
    fn append_is_idempotent_on_the_verified_child_identity() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = StageEnv::new("append");
        let c = child("par1", "nodeb", "aa", "a2a-1");

        assert_eq!(append_remote_child(&c).unwrap(), true, "first append lands");
        assert_eq!(append_remote_child(&c).unwrap(), false, "the same child is a no-op");
        assert_eq!(load_remote_children().len(), 1);

        // The identity is (key, sessionId) — a second child under the SAME
        // key, or the same session id under a DIFFERENT key, both append.
        assert_eq!(append_remote_child(&child("par1", "nodeb", "aa", "a2a-2")).unwrap(), true);
        assert_eq!(append_remote_child(&child("par2", "nodec", "bb", "a2a-1")).unwrap(), true);
        assert_eq!(load_remote_children().len(), 3);

        // The label or the spawning parent differing does not make a new
        // child: `node` is display-only, and one child has one parent.
        assert_eq!(append_remote_child(&child("par9", "nodeb-relabelled", "aa", "a2a-1")).unwrap(), false);
        assert_eq!(load_remote_children().len(), 3);

        let raw = std::fs::read_to_string(remote_children_path()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["schemaVersion"], "0");
        assert_eq!(v["children"][0]["parentSessionId"], "par1");
        assert_eq!(v["children"][0]["sessionId"], "a2a-1");
        assert_eq!(v["children"][0]["spawnedAt"], "2026-09-25T00:00:00Z");
        assert_eq!(v["children"][0]["linesAfter"], 0);
    }

    #[test]
    fn a_missing_or_corrupt_ledger_reads_as_empty() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = StageEnv::new("tolerant");
        assert!(load_remote_children().is_empty(), "a missing file is an empty ledger");

        std::fs::create_dir_all(conducting_stage_dir()).unwrap();
        std::fs::write(remote_children_path(), "{ not json").unwrap();
        assert!(load_remote_children().is_empty(), "a corrupt file is an empty ledger");

        // And a corrupt file does not block the next append: the read in the
        // locked section degrades the same way, so a spawn still records.
        assert_eq!(append_remote_child(&child("p", "n", "k", "s")).unwrap(), true);
        assert_eq!(load_remote_children().len(), 1);
    }

    #[test]
    fn retain_drops_exactly_the_rejected_rows() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = StageEnv::new("retain");
        append_remote_child(&child("par1", "nodeb", "aa", "c1")).unwrap();
        append_remote_child(&child("par2", "nodeb", "aa", "c2")).unwrap();

        let removed = retain_remote_children(|c| c.parent_session_id == "par1").unwrap();
        assert_eq!(removed, 1);
        let left = load_remote_children();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].session_id, "c1");

        // Nothing rejected is not a write: the file's bytes are untouched.
        let before = std::fs::read_to_string(remote_children_path()).unwrap();
        assert_eq!(retain_remote_children(|_| true).unwrap(), 0);
        assert_eq!(std::fs::read_to_string(remote_children_path()).unwrap(), before);
    }

    #[test]
    fn advance_lines_after_is_forward_only_and_ignores_an_unknown_child() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = StageEnv::new("advance");
        append_remote_child(&child("par1", "nodeb", "aa", "c1")).unwrap();

        assert_eq!(advance_lines_after("aa", "c1", 7).unwrap(), true);
        assert_eq!(load_remote_children()[0].lines_after, 7);
        assert_eq!(advance_lines_after("aa", "c1", 3).unwrap(), false, "a replayed pull never rewinds");
        assert_eq!(advance_lines_after("aa", "c1", 7).unwrap(), false, "the same seq is not a write");
        assert_eq!(load_remote_children()[0].lines_after, 7);
        assert_eq!(advance_lines_after("aa", "nope", 9).unwrap(), false);
        assert_eq!(advance_lines_after("bb", "c1", 9).unwrap(), false, "the key is half the identity");
        assert_eq!(load_remote_children()[0].lines_after, 7);
    }

    #[test]
    fn valid_claimed_session_id_admits_exactly_the_contract_shape() {
        assert!(valid_claimed_session_id("a2a-4411-1790312541"));
        assert!(valid_claimed_session_id("conduct-17991-1790312541"));
        assert!(valid_claimed_session_id("A.b_c:d-e"));
        assert!(valid_claimed_session_id(&"a".repeat(CLAIMED_SESSION_ID_MAX)));

        assert!(!valid_claimed_session_id(""), "empty");
        assert!(!valid_claimed_session_id(&"a".repeat(CLAIMED_SESSION_ID_MAX + 1)), "129 bytes");
        assert!(!valid_claimed_session_id("nodeb/C"), "a slash would read as a node-qualified id");
        assert!(!valid_claimed_session_id("a b"), "space");
        assert!(!valid_claimed_session_id("a\nb"), "newline");
        assert!(!valid_claimed_session_id("a\u{7}b"), "control char");
        assert!(!valid_claimed_session_id("naïve"), "non-ASCII");
        assert!(!valid_claimed_session_id("../etc"), "traversal shape");
    }
}
