//! Shared test seams for the `rice` family's command tests.
//!
//! Two things every staging test needs and none of them should spell for
//! itself:
//!
//!   * **the songbook gate's test-only fixture** — `rice stage` now evaluates
//!     `crate::widgets::plan_stage` (the §7.5 gate) before it writes anything,
//!     and the gate wants a shipped templates dir or a real `nix-instantiate`.
//!     A test about staging MECHANICS (notes, covers, drafts, takes, mode) has
//!     no business standing either up, so [`ensure_default_songbook_fixture`]
//!     points `widgets`'s seam (`AOIDE_SONGBOOK_EVAL_FIXTURE`) at an empty
//!     payload. A test that IS about the gate unsets the seam deliberately and
//!     drives the real path.
//!   * **a stand-in for the shipped generator** — [`nix_instantiate_recorder`],
//!     an executable named `nix-instantiate` that records its own argv and
//!     prints a payload, plus [`generator_payload`] to build the payload a
//!     correct generator would answer with for a fixture songbook. That is what
//!     lets §7.5's case 2 be exercised hermetically: the real generator lives in
//!     a store path this test binary never has.

use std::path::Path;

/// Points `widgets`'s songbook seam at an empty `{ "manifest": {}, "registry":
/// {} }` payload, unless the caller already pointed it at its own.
///
/// The file lives OUTSIDE every test's own root: a test that forgets to capture
/// `AOIDE_SONGBOOK_EVAL_FIXTURE` in its `EnvSaver` list then leaves a path that
/// still exists — and still means the same empty payload — rather than a
/// dangling one the next test would choke on.
pub(crate) fn ensure_default_songbook_fixture() {
    if std::fs::remove_file(real_path_marker()).is_ok() {
        return;
    }
    if std::env::var(crate::widgets::SONGBOOK_EVAL_FIXTURE_VAR).is_ok() {
        return;
    }
    let path = std::env::temp_dir().join("aoide-songbook-eval-fixture-default.json");
    if !path.is_file() {
        std::fs::write(&path, r#"{"manifest":{},"registry":{}}"#).unwrap();
    }
    std::env::set_var(crate::widgets::SONGBOOK_EVAL_FIXTURE_VAR, &path);
}

/// Marks THIS test as one that wants the REAL songbook path — §7.5's gate, not
/// the empty seam default — so the next [`ensure_default_songbook_fixture`]
/// stands down. One-shot and a FILE rather than an env var: the next staging
/// root consumes it, so it cannot leak into another test (which is what an env
/// var here would do, and what would silently disable the gate for every test
/// that ran afterwards).
pub(crate) fn require_the_real_songbook_path() {
    std::fs::write(real_path_marker(), "").unwrap();
}

fn real_path_marker() -> std::path::PathBuf {
    std::env::temp_dir().join("aoide-songbook-gate-real-path.marker")
}

/// Writes an executable `nix-instantiate` into `<root>/fake-bin` that appends
/// its own argv to `<root>/nix-instantiate-argv.txt` (one line per call) and
/// prints `payload` — the §9 argv, recorded and answered without nix.
///
/// Returns the directory to prepend to `PATH`. Callers must hold
/// `aoide_test_support::env_lock()` and capture `PATH` in their own `EnvSaver`
/// list, the same discipline every other process-wide mutation here follows.
pub(crate) fn nix_instantiate_recorder(root: &Path, payload: &str) -> std::path::PathBuf {
    let bin = root.join("fake-bin");
    std::fs::create_dir_all(&bin).unwrap();
    let log = root.join("nix-instantiate-argv.txt");
    let payload_path = root.join("nix-instantiate-payload.json");
    std::fs::write(&payload_path, payload).unwrap();
    let script = bin.join("nix-instantiate");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> {log}\ncat {payload}\n",
            log = log.display(),
            payload = payload_path.display(),
        ),
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script, perms).unwrap();
    }
    bin
}

/// Prepends `dir` to `PATH` (see [`nix_instantiate_recorder`]).
pub(crate) fn prepend_path(dir: &Path) {
    let joined = match std::env::var("PATH") {
        Ok(existing) if !existing.is_empty() => format!("{}:{existing}", dir.display()),
        _ => dir.display().to_string(),
    };
    std::env::set_var("PATH", joined);
}

/// A readable digest of every file under each root, sorted — the §9(d) "byte
/// unchanged" proof. Content, paths and (via the entry separator) the file set
/// are all in it; what is deliberately NOT in it is anything a stage is allowed
/// to touch, because a stage is not allowed to touch anything.
pub(crate) fn tree_snapshot(roots: &[&Path]) -> String {
    let mut lines: Vec<String> = Vec::new();
    for root in roots {
        if !root.exists() {
            lines.push(format!("{}\t<absent>", root.display()));
            continue;
        }
        lines.push(format!("{}\t<root>", root.display()));
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    let bytes = std::fs::read(&path).unwrap_or_default();
                    lines.push(format!(
                        "{}\t{}\t{:?}",
                        path.display(),
                        bytes.len(),
                        String::from_utf8_lossy(&bytes)
                    ));
                }
            }
        }
    }
    lines.sort();
    lines.join("\n")
}

/// The `{ manifest, registry, packages }` JSON a correct generator answers with
/// for `songbook_root`'s songs — the SHELF-LESS shape (`rice compose` never
/// writes a `_widgets/` shelf), which is the shape every fixture songbook in
/// these tests has: manifest from a `widgets/*.qml` scan, registry from the
/// song's `livery.json` `.widgets // {}`, and an empty package list.
pub(crate) fn generator_payload(songbook_root: &Path) -> String {
    let mut manifest = serde_json::Map::new();
    let mut registry = serde_json::Map::new();
    let mut packages = serde_json::Map::new();
    let entries = std::fs::read_dir(songbook_root)
        .expect("fixture songbook root must exist");
    let mut songs: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    songs.sort();
    for song in songs {
        let dir = songbook_root.join(&song);
        let slots: Vec<String> = {
            let mut slots: Vec<String> = std::fs::read_dir(dir.join("widgets"))
                .map(|entries| {
                    entries
                        .filter_map(|e| e.ok())
                        .filter(|e| e.path().is_file())
                        .filter_map(|e| e.file_name().to_str().map(str::to_string))
                        .filter(|name| {
                            name.ends_with(".qml")
                                && name
                                    .as_bytes()
                                    .first()
                                    .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
                        })
                        .map(|name| name.trim_end_matches(".qml").to_string())
                        .collect()
                })
                .unwrap_or_default();
            slots.sort();
            slots
        };
        if !slots.is_empty() {
            let mut entry = serde_json::Map::new();
            for slot in &slots {
                entry.insert(
                    slot.clone(),
                    serde_json::json!({ "owner": song, "file": format!("{slot}.qml") }),
                );
            }
            manifest.insert(song.clone(), serde_json::Value::Object(entry));
        }
        let widgets = std::fs::read_to_string(dir.join("livery.json"))
            .ok()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
            .and_then(|parsed| parsed.get("widgets").cloned())
            .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));
        registry.insert(song.clone(), widgets);
        packages.insert(song, serde_json::json!([]));
    }
    serde_json::json!({
        "manifest": serde_json::Value::Object(manifest),
        "registry": serde_json::Value::Object(registry),
        "packages": serde_json::Value::Object(packages),
    })
    .to_string()
}
