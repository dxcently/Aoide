//! The resume claims: `state/resuming.json` (v1). A resurrect that is about to
//! launch a ledger session under a fresh id writes one claim first, naming the
//! session and the new id, so a second launch of that session sees the claim
//! and refuses (one engine per session). CONTRACTS.md's
//! `state/resuming.json` section is the wire shape.

use serde::{Deserialize, Serialize};

/// `state/resuming.json` schema version.
pub const RESUMING_VERSION: u32 = 1;

/// One claim: `sessionId` is being resumed as `newId`, taken at `at` (ISO UTC).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Claim {
    pub session_id: String,
    pub new_id: String,
    pub at: String,
}

/// The claims file path: `state/resuming.json`, under the same state dir as
/// `undying.json`.
pub fn resuming_path() -> std::path::PathBuf {
    crate::fs::state_dir().join("resuming.json")
}

/// The file's container: `{"v":1,"claims":[...]}`.
#[derive(Serialize, Deserialize)]
struct ClaimsFile {
    v: u32,
    claims: Vec<Claim>,
}

/// The claims on disk. A missing file is no claims. A file that cannot be read
/// or parsed, is another version, or holds a claim whose `at` is not an ISO UTC
/// time is an error, and the caller refuses on it: a claim that cannot be read
/// never reads as no claim.
pub fn load_claims() -> Result<Vec<Claim>, String> {
    let path = resuming_path();
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let file: ClaimsFile = serde_json::from_str(&raw)
        .map_err(|e| format!("{}: unreadable claims file: {e}", path.display()))?;
    if file.v != RESUMING_VERSION {
        return Err(format!("{}: claims file version {} is not {RESUMING_VERSION}", path.display(), file.v));
    }
    if let Some(claim) = file.claims.iter().find(|c| crate::time::parse_iso_utc(&c.at).is_none()) {
        return Err(format!("{}: the claim for {} has an unparseable `at`", path.display(), claim.session_id));
    }
    Ok(file.claims)
}

/// Writes the claims atomically, owner-only.
pub fn save_claims(claims: &[Claim]) -> Result<(), String> {
    let body = serde_json::to_vec_pretty(&ClaimsFile { v: RESUMING_VERSION, claims: claims.to_vec() })
        .map_err(|e| format!("serialize resuming.json: {e}"))?;
    let path = resuming_path();
    crate::fs::atomic_write_private(&path, &body).map_err(|e| format!("{}: {e}", path.display()))
}

/// Records `claim` as its session's one claim, in place of any the session held.
pub fn set_claim(claims: &mut Vec<Claim>, claim: Claim) {
    claims.retain(|c| c.session_id != claim.session_id);
    claims.push(claim);
}

/// Drops the claim that names `session_id` resumed as `new_id`; a claim naming
/// another id stays. Returns whether one was dropped.
pub fn remove_claim(claims: &mut Vec<Claim>, session_id: &str, new_id: &str) -> bool {
    let before = claims.len();
    claims.retain(|c| !(c.session_id == session_id && c.new_id == new_id));
    claims.len() != before
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_temp_state_dir<T>(name: &str, f: impl FnOnce(&std::path::Path) -> T) -> T {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STATE_DIR").ok();
        let dir = std::env::temp_dir().join(format!("aoide-resuming-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::env::set_var("AOIDE_STATE_DIR", &dir);

        let out = f(&dir);

        let _ = std::fs::remove_dir_all(&dir);
        match saved {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
        out
    }

    fn claim(session_id: &str, new_id: &str, at: &str) -> Claim {
        Claim { session_id: session_id.into(), new_id: new_id.into(), at: at.into() }
    }

    #[test]
    fn a_missing_claims_file_is_no_claims() {
        with_temp_state_dir("missing", |_| {
            assert!(load_claims().unwrap().is_empty());
        });
    }

    #[test]
    fn claims_round_trip_through_the_file() {
        with_temp_state_dir("roundtrip", |_| {
            let claims = vec![claim("s1", "n1", "2026-10-25T10:00:00Z"), claim("s2", "n2", "2026-10-25T10:01:00Z")];
            save_claims(&claims).unwrap();
            assert_eq!(load_claims().unwrap(), claims);
        });
    }

    #[test]
    fn the_file_is_the_documented_shape_and_private() {
        with_temp_state_dir("shape", |_| {
            save_claims(&[claim("s1", "n1", "2026-10-25T10:00:00Z")]).unwrap();
            let path = resuming_path();
            let raw: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            assert_eq!(raw["v"], 1);
            assert_eq!(raw["claims"][0]["sessionId"], "s1");
            assert_eq!(raw["claims"][0]["newId"], "n1");
            assert_eq!(raw["claims"][0]["at"], "2026-10-25T10:00:00Z");
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "the claims file is written owner-only");
        });
    }

    #[test]
    fn an_unparseable_claims_file_is_an_error_not_no_claims() {
        with_temp_state_dir("garbage", |_| {
            std::fs::create_dir_all(resuming_path().parent().unwrap()).unwrap();
            std::fs::write(resuming_path(), "{ not json").unwrap();
            assert!(load_claims().is_err(), "a corrupt file must refuse, never read as no claims");
        });
    }

    #[test]
    fn a_claims_file_of_another_version_is_refused() {
        with_temp_state_dir("version", |_| {
            std::fs::create_dir_all(resuming_path().parent().unwrap()).unwrap();
            std::fs::write(resuming_path(), r#"{"v":2,"claims":[]}"#).unwrap();
            assert!(load_claims().is_err());
        });
    }

    #[test]
    fn a_claim_with_an_unparseable_at_refuses_the_whole_file() {
        with_temp_state_dir("bad-at", |_| {
            std::fs::create_dir_all(resuming_path().parent().unwrap()).unwrap();
            std::fs::write(
                resuming_path(),
                r#"{"v":1,"claims":[{"sessionId":"s1","newId":"n1","at":"yesterday"}]}"#,
            )
            .unwrap();
            assert!(load_claims().is_err());
        });
    }

    #[test]
    fn set_claim_keeps_one_claim_per_session() {
        let mut claims = vec![claim("s1", "old", "2026-10-25T10:00:00Z")];
        set_claim(&mut claims, claim("s1", "new", "2026-10-25T10:05:00Z"));
        assert_eq!(claims, vec![claim("s1", "new", "2026-10-25T10:05:00Z")]);
    }

    #[test]
    fn remove_claim_drops_only_the_claim_that_names_its_new_id() {
        let mut claims = vec![claim("s1", "mine", "2026-10-25T10:00:00Z"), claim("s2", "theirs", "2026-10-25T10:00:00Z")];
        assert!(!remove_claim(&mut claims, "s1", "someone-else"), "a claim naming another id stays");
        assert!(remove_claim(&mut claims, "s1", "mine"));
        assert_eq!(claims, vec![claim("s2", "theirs", "2026-10-25T10:00:00Z")]);
    }
}
