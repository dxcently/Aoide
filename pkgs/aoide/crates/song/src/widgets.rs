//! Per-song widget QML sync — the runtime-tree half of `rice stage`
//! (concepts/song/Self-Ricing.md "Staging vs Declarative Mode"). `rice
//! stage` already hot-reloads a song's palette/notes via
//! `song/stage/livery.json`; this module carries a song's widget QML
//! BODIES (`song/songbook/<song>/widgets/*.qml`) into the live runtime tree
//! (`run/qml/songs/<song>/`) too, so Quickshell's own file-watcher picks up
//! an edit to an EXISTING widget file live, no rebuild needed. `manifest.json`
//! is a hot-reloaded `FileView` on the QML side (`StagingEngine.qml`), not
//! read once at startup — a brand-new slot still needs THIS module's own
//! manifest regeneration (below) to appear in it, but no service restart.
//!
//! **The generator is SHIPPED, and it runs offline.** `manifest.json` and
//! `registry.json` are not derived by scanning a song's own `widgets/`
//! directory: a scan cannot answer "who owns this slot" once a composition
//! borrows another song's body — only `composeSong` (`lib/song.nix`) resolves
//! ownership, and it runs in the nix evaluator, nowhere else. So both files are
//! regenerated WHOLE, for every song this machine's songbook holds, by one
//! plain `nix-instantiate --eval --strict --json` over `share/lyra/nix/
//! manifest.nix` — a file SHIPPED by `pkgs/lyra-songbook`, a thin entry over
//! copies of `lib/songbook.nix`/`lib/song.nix`, pointed at this machine's own
//! `$AOIDE_ROOT/song/songbook`. No flake, no network, no checkout, no
//! `--impure`: `flake_root()` is not read here at all, and the flake's
//! `#songbookManifest` output is a build-time convenience, never a runtime
//! dependency. [`plan_stage`] is that shell-out and §7.5's gate in one
//! decision, invoked once from the manifest path
//! ([`sync_song_widgets`]) and once from the registry path
//! ([`sync_song_registry`]) — see each function's own doc.
//!
//! This also fixes a real, silent hazard the old per-entry writer left
//! behind: it preserved every OTHER song's entry verbatim on each write, so
//! a manifest.json written by nix (owner-map shape) that this module then
//! patched would leave every UNTOUCHED song's entry in owner-map shape but
//! silently rewrite the STAGED song's own entry back to the old bare-list
//! shape — a shape `StagingEngine.qml`'s `has()` cannot look up, so every
//! widget for that one song would render nothing, no error anywhere.
//! Whole-file regeneration can't reproduce that failure mode: every song's
//! entry, staged or not, comes from one eval of one songbook.
//!
//! Mirrors the nix build's own per-song widget carry (the lyra lane's
//! `quickshellConfig` derivation) for the BODY copy: the WHOLE `widgets/` tree
//! is copied unfiltered (helper components, asset subdirs, `.gitkeep`,
//! everything), while the manifest only ever lists top-level lowercase-kebab
//! `.qml` files as slots.
//!
//! **§7.5's three cases, and a refusal stages NOTHING.** [`plan_stage`] is the
//! whole policy, and it runs in the CALLER before that caller's first write:
//!
//!   1. **Built in, with no differing machine copy** — the shipped templates'
//!      baked `manifest.json`/`registry.json` are the answer and `nix` is never
//!      invoked. "No differing copy" compares CONTENT over what the SEED ships;
//!      the machine's own runtime dirs (`takes/`, `drafts/` —
//!      [`MACHINE_RUNTIME_DIRS`]) are not differences in the song, so taking a
//!      snapshot does not disable staging on a host with no nix.
//!   2. **Otherwise, on a host with `nix-instantiate`** — the shipped generator
//!      ([`eval_generator`]) over this machine's songbook, whose answer WINS
//!      over the baked baseline for any song both carry: a machine owns its
//!      songbook, so the machine copy is what gets staged.
//!   3. **Otherwise** — refused with [`refusal_no_nix`] or
//!      [`refusal_rebuild_needed`], and nothing is staged.
//!
//! One decision and one evaluation per stage: the callers (`rice stage`,
//! `rice mode stage`, `rice back`, `reload`) hand the same [`StageSongbook`] to
//! both syncs rather than asking twice. The widget BODY carry follows the same
//! ownership the manifest records — the staged song AND every LENDER its entry
//! names ([`widget_owners`]), because a borrowed slot resolves to
//! `songs/<owner>/<file>`.

use std::path::Path;

/// A successful widget sync — including the clean no-op "nothing to do"
/// cases (no `widgets/` dir, no deployed runtime tree).
pub struct WidgetSyncOk {
    /// `run/qml`-rooted files actually (re)written, absolute paths.
    pub changed: Vec<String>,
    /// This song's LOCAL slot names, sorted — a scan of the widgets/ dir
    /// just copied (never the just-regenerated manifest.json): informational
    /// only, for the `Outcome`'s own `data.slots`/message, independent of
    /// what nix's committed-tree eval says this song owns. A song being
    /// staged from an uncommitted/relocated songbook (tests; a brand-new
    /// song not yet `git add`ed) can carry local widget files nix's eval of
    /// the real committed tree has never seen — this field still reports
    /// them; `manifest.json` itself does not until nix does.
    pub slots: Vec<String>,
    /// Whether any widget BODY file (not `manifest.json`) was (re)written —
    /// the trigger `rice stage`'s caller uses to decide whether a
    /// Quickshell IPC reload is worth it (dynamically
    /// `Qt.createComponent`-loaded widget bodies have no file watcher;
    /// `manifest.json` does, via `StagingEngine.qml`'s own `FileView`, so a
    /// manifest-only regeneration needs no IPC nudge).
    pub bodies_changed: bool,
    /// Human summary for the caller's `Outcome` message/data.
    pub note: String,
}

/// A widget sync failure — an IO error partway through the copy, or the
/// `nix eval` shell-out failing/producing something unusable. Fatal to the
/// caller in both cases: a torn widgets copy is worse than refusing the
/// call, and a manifest/registry write built on a NIX EVAL FAILURE is
/// exactly the silently-wrong-file class this module exists to prevent —
/// better to leave the last-good file in place and surface nix's own
/// message than guess.
pub struct WidgetSyncErr {
    pub error: String,
    pub target: String,
}

/// A successful widget-TYPE registry sync — including the clean no-op
/// "nothing to do" case (no deployed `run/qml` runtime tree).
pub struct RegistrySyncOk {
    /// `run/qml`-rooted files actually (re)written, absolute paths — 0 or 1
    /// entries (`registry.json`'s own path), same shape as
    /// [`WidgetSyncOk::changed`].
    pub changed: Vec<String>,
    /// This song's freshly-regenerated registry entry (`{}` when the
    /// committed tree declares nothing for it), straight from
    /// [`plan_stage`]'s output — never the pre-regeneration file.
    pub widgets: serde_json::Value,
    /// Human summary for the caller's `Outcome` message/data.
    pub note: String,
}

/// Test-only input seam: when set, [`plan_stage`] reads its `{
/// manifest, registry }` payload from the JSON FILE at this path instead of
/// evaluating the shipped generator. Exists so the widgets-sync-MECHANICS tests
/// (body copy, manifest whole-regen/shape-healing, registry idempotency —
/// `commands/rice.rs`'s `mod tests`) can run inside the `aoide` package
/// derivation's sandboxed `checkPhase`, which has `HOME=/homeless-shelter`,
/// no shipped templates dir, no network, and no usable `nix` binary. Those
/// tests exercise THIS crate's regeneration/write logic, not the real
/// generator's output shape, so a fixture is a strictly better input for
/// them: hermetic, and no longer coupled to a songbook's current slot counts.
/// Tests that deliberately assert the REAL behaviour (the §7.5 cases and the
/// generator's argv, `commands/rice.rs`'s own gate tests) unset this and drive
/// the real path, and the two nix CHECKS (`checks.generator-offline`,
/// `checks.generator-relocatable`) cover the generator itself.
///
/// Opt-in only, read once per call, and validated exactly like the generator's
/// own payload (never a default/empty value on missing/malformed input) — this
/// is an alternate SOURCE for the same validated shape, not a fallback that
/// lets a broken/missing `nix-instantiate` proceed quietly. `#[cfg(test)]`
/// on both this constant and its read in [`plan_stage`] compiles the seam
/// out of every non-test build: a deployed binary contains no read of this
/// variable, so its eval can never be redirected through the environment.
/// All setters live in this crate's own `mod tests` +
/// `commands::test_support`, the same compilation unit, so the plain
/// `cfg(test)` gate reaches them.
#[cfg(test)]
pub(crate) const SONGBOOK_EVAL_FIXTURE_VAR: &str = "AOIDE_SONGBOOK_EVAL_FIXTURE";

/// §7.5's staging gate AND the manifest/registry a stage will write — one
/// decision, one evaluation, in that order, so that nothing is ever staged on
/// a refusal and the songbook is never computed twice for one `rice stage`.
///
/// Three cases, in this order:
///
///   1. **Built in, with no differing machine copy** — the SHIPPED
///      `manifest.json`/`registry.json` are the whole answer
///      ([`baseline_songbook`]), and `nix` is never invoked: the templates are
///      the build that put this song on the machine, so a host with no nix can
///      stage what it already carries.
///   2. **Any other stageable song** — one plain
///      `nix-instantiate --eval --strict --json` over the SHIPPED generator
///      (`share/lyra/nix/manifest.nix`) pointed at the MACHINE's songbook
///      (`$AOIDE_ROOT/song/songbook`). A FILE eval: no flake, no network, no
///      checkout, no `--impure`, and no argv naming `$AOIDE_FLAKE_ROOT` or a
///      `#<flake output>`. The generated manifest WINS over the shipped
///      baseline for any song both carry — a machine owns its songbook, so the
///      machine copy is what gets staged — and the song's own needs are
///      checked against what this system installed BEFORE anything is written.
///   3. **Refused** — and the caller stages nothing at all: no livery, no
///      widget body, no manifest. Two taught refusals
///      ([`refusal_no_nix`], [`refusal_rebuild_needed`]).
///
/// A host with no `nix` makes case 2 impossible, which is exactly the first
/// refusal's shape: only what was built in can be staged. The check IS the
/// spawn (`ErrorKind::NotFound`), not a `which` probe — one answer, no second
/// source of truth about the host's PATH.
///
/// Returns the WHOLE `{ manifest, registry }` payload; callers pick the half
/// they need.
///
/// [`SONGBOOK_EVAL_FIXTURE_VAR`] replaces the whole of the above for tests —
/// see that constant's own doc.
pub fn plan_stage(name: &str) -> Result<StageSongbook, WidgetSyncErr> {
    #[cfg(test)]
    if let Ok(path) = std::env::var(SONGBOOK_EVAL_FIXTURE_VAR) {
        let bytes = std::fs::read(&path).map_err(|e| WidgetSyncErr {
            error: format!(
                "failed to read {SONGBOOK_EVAL_FIXTURE_VAR} fixture at {path}: {e}"
            ),
            target: path.clone(),
        })?;
        return parse_songbook_eval(&bytes, &path);
    }

    let Some(templates) = aoide_storage::fs::song_templates_dir() else {
        return Err(WidgetSyncErr {
            error: format!(
                "no shipped song templates dir found: $AOIDE_SONG_TEMPLATES is unset (or not \
                 absolute) and no `share/lyra/songbook` sits beside this binary, so there is \
                 nothing to stage `{name}` from"
            ),
            target: "AOIDE_SONG_TEMPLATES".to_string(),
        });
    };

    let builtin = read_builtin(&templates)?;
    let built_in = builtin.songs.iter().any(|song| song == name);
    let machine_dir = aoide_storage::fs::songbook_dir(name);
    let machine = machine_dir.is_dir();

    // §7.5's own precondition: `<n>` must exist in the machine's songbook, in
    // the built-in set, or both.
    if !built_in && !machine {
        return Err(WidgetSyncErr {
            error: format!(
                "no song `{name}`: it is not built into this system and {} does not exist",
                machine_dir.display()
            ),
            target: machine_dir.to_string_lossy().into_owned(),
        });
    }

    // Case 1: built in, and the machine has no DIFFERING copy — absent counts,
    // and so does byte-identical (a machine copy is seeded FROM the shipped
    // folder, so an untouched seeded song is the same song, and refusing to
    // see that would push every nix-less host's built-in songs down a road it
    // cannot travel).
    if built_in && !machine_copy_differs(name, &templates) {
        return baseline_songbook(name, &templates);
    }

    // Case 2: the shipped generator over the machine's songbook.
    let generated = eval_generator(name, &templates)?;
    let missing: Vec<String> = generated
        .packages
        .iter()
        .filter(|pkg| !builtin.packages.contains(pkg))
        .cloned()
        .collect();
    if !missing.is_empty() {
        return Err(WidgetSyncErr {
            error: refusal_rebuild_needed(name, &missing),
            target: generated.target,
        });
    }

    // The shipped baseline carries the BUILT-IN songs' entries (`manifest.json`
    // baked over exactly the shipped set); this system's own generated answer
    // overwrites it for every song the machine's songbook holds. A song in
    // neither is simply absent — the prune is automatic, never an immortal
    // stale key.
    let mut manifest = read_baked(&templates, "manifest.json")?;
    let mut registry = read_baked(&templates, "registry.json")?;
    if let (Some(manifest_obj), Some(gen_obj)) =
        (manifest.as_object_mut(), generated.manifest.as_object())
    {
        for (song, entry) in gen_obj {
            manifest_obj.insert(song.clone(), entry.clone());
        }
    }
    if let (Some(registry_obj), Some(gen_obj)) =
        (registry.as_object_mut(), generated.registry.as_object())
    {
        for (song, entry) in gen_obj {
            registry_obj.insert(song.clone(), entry.clone());
        }
    }
    Ok(StageSongbook { manifest, registry })
}

/// One `nix-instantiate --eval --strict --json` over the SHIPPED generator,
/// pointed at the machine's songbook — §7.5 case 2.
///
/// `--strict` because a shelf's `composeSong` guards `throw`: without forcing
/// the whole tree an unevaluated song would serialize as garbage instead of
/// failing. The generator path and the songbook argument both come from
/// [`aoide_storage::fs`] — the templates dir this binary resolves, and
/// `$AOIDE_ROOT/song/songbook` — so nothing here can name a checkout path.
///
/// The spawn error IS the no-nix answer: `ErrorKind::NotFound` becomes
/// [`refusal_no_nix`], every other spawn failure its own message. A non-zero
/// exit or unparseable output is a real failure and reported with nix's own
/// message — never a default/empty songbook, which a caller could mistake for
/// "the songbook is genuinely empty" and write out.
fn eval_generator(name: &str, templates: &Path) -> Result<Generated, WidgetSyncErr> {
    let manifest_nix = templates.join("..").join("nix").join("manifest.nix");
    let songbook = aoide_storage::fs::songbook_root();
    let expr = format!(
        "import {} {{ songbook = {}; }}",
        nix_string(&manifest_nix.to_string_lossy()),
        nix_string(&songbook.to_string_lossy()),
    );
    let target = manifest_nix.to_string_lossy().into_owned();

    let output = match std::process::Command::new("nix-instantiate")
        .args(["--eval", "--strict", "--json", "--expr", &expr])
        .output()
    {
        Ok(output) => output,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(WidgetSyncErr {
                error: refusal_no_nix(name),
                target: "nix-instantiate".to_string(),
            });
        }
        Err(e) => {
            return Err(WidgetSyncErr {
                error: format!("failed to run `nix-instantiate`: {e}"),
                target,
            });
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(WidgetSyncErr {
            error: format!(
                "the shipped songbook generator failed over {} — no manifest.json/registry.json \
                 written:\n{}",
                songbook.display(),
                stderr.trim()
            ),
            target,
        });
    }

    let parsed: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| WidgetSyncErr {
            error: format!("the shipped songbook generator's output is not valid JSON: {e}"),
            target: target.clone(),
        })?;
    let manifest = parsed
        .get("manifest")
        .filter(|v| v.is_object())
        .cloned()
        .ok_or_else(|| WidgetSyncErr {
            error: "the shipped songbook generator's output has no `manifest` object".to_string(),
            target: target.clone(),
        })?;
    let registry = parsed
        .get("registry")
        .filter(|v| v.is_object())
        .cloned()
        .ok_or_else(|| WidgetSyncErr {
            error: "the shipped songbook generator's output has no `registry` object".to_string(),
            target: target.clone(),
        })?;
    // `packages` is the §7.5 needs-check's whole input, so its absence cannot
    // pass silently as "needs nothing": a templates package older than the
    // generator that emits it is a real mismatch, and this says so.
    let packages = parsed
        .get("packages")
        .filter(|v| v.is_object())
        .and_then(|v| v.get(name))
        .and_then(|v| v.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|e| e.as_str().map(str::to_string))
                .collect::<Vec<String>>()
        })
        .ok_or_else(|| WidgetSyncErr {
            error: "the shipped songbook generator's output carries no `packages` entry for this \
                    song — the templates package predates it; rebuild this system"
                .to_string(),
            target: target.clone(),
        })?;

    Ok(Generated {
        manifest,
        registry,
        packages,
        target,
    })
}

/// [`eval_generator`]'s parsed answer: the two files plus the package names the
/// staged song needs (its borrow-closure included — `nix/manifest.nix`).
struct Generated {
    manifest: serde_json::Value,
    registry: serde_json::Value,
    packages: Vec<String>,
    target: String,
}

/// A nix string literal for the one place a path has to survive being spliced
/// into an `--expr` argument. `\`, `"` and `${` are the three sequences that
/// would otherwise end the literal or start an interpolation.
fn nix_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '$' if chars.clone().next() == Some('{') => out.push_str("\\$"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// §7.5 case 3, first form: nothing is built in for `name` and this host has no
/// `nix` to build one from the machine's songbook.
fn refusal_no_nix(name: &str) -> String {
    format!(
        "{name} is not built into this system and this host has no nix; \
         add it to habit.song.available and rebuild"
    )
}

/// §7.5 case 3, second form: the machine's copy of `name` declares packages
/// this system did not install, so staging it would render widgets that shell
/// out to executables that are not there.
fn refusal_rebuild_needed(name: &str, missing: &[String]) -> String {
    format!(
        "rebuild needed: {name} needs {} (not in this system); \
         add \"{name}\" to habit.song.available in hosts/<host> and rebuild",
        missing.join(", ")
    )
}

/// §7.5 case 1 — `name` is built in and the machine has no differing copy, so
/// the SHIPPED, prebaked `manifest.json`/`registry.json` from `templates` are
/// the answer and no `nix` is invoked. Three layers, in order, each
/// overwriting the last:
///
///   1. **Baseline**: the baked manifest/registry, computed at templates-build
///      time by the SAME `lib/songbook.nix` generator the machine path
///      evaluates (`pkgs/lyra-songbook`). Authoritative for every shipped,
///      read-only song.
///   2. **Overlay**: [`overlay_surviving_entries`] copies every entry from
///      the EXISTING on-disk `run_qml/songs/{manifest,registry}.json` whose
///      song still has a directory in the host songbook on top of the
///      baseline. Without it, staging a built-in song after having staged a
///      MACHINE song would silently drop the machine song's entry (it is in
///      neither the frozen baseline nor the built-in song's own scan) —
///      `StagingEngine.qml` would then resolve its widgets against a different
///      song's slot, no error anywhere. A song whose songbook directory was
///      since removed is NOT overlaid — its entry is pruned rather than kept
///      immortal.
///   3. **Patch**: `name`'s own entry, from a fresh, nix-free scan of its
///      ACTUAL songbook directory ([`scan_own_entry`]) — always wins over both
///      the baseline and the overlay, so a local edit to a seeded song is
///      picked up. Skipped for a song with no songbook directory (nothing to
///      scan, and an empty patch would delete the baked entry) and for one
///      with a `_widgets/` shelf (the scan is the shelf-less shape only —
///      `rice compose` never writes a shelf — and the baked entry already has
///      its borrowed slots resolved by `composeSong`).
///
/// Net effect: the SHIPPED songbook's entries, with this machine's own
/// previously-staged songs preserved and the staged song self-healed.
fn baseline_songbook(name: &str, templates: &Path) -> Result<StageSongbook, WidgetSyncErr> {
    let run_qml = aoide_storage::fs::run_qml_dir();
    let manifest_path = templates.join("manifest.json");
    let registry_path = templates.join("registry.json");
    if !manifest_path.is_file() || !registry_path.is_file() {
        return Err(WidgetSyncErr {
            error: format!(
                "the shipped templates dir at {} has no baked manifest.json/registry.json — \
                 every stage of a built-in song reads them; set $AOIDE_SONG_TEMPLATES to a \
                 directory shipping both",
                templates.display()
            ),
            target: templates.to_string_lossy().into_owned(),
        });
    }

    // A `_widgets/` shelf (borrowed/composed widget ownership) resolves in
    // `composeSong`, in the nix evaluator — and for a built-in song that is
    // exactly what the BAKED baseline already carries. The layer-3 scan below
    // can only express the shelf-less shape (`rice compose` never writes a
    // shelf), so a shelf song takes the baseline and the overlay as they are:
    // patching it with a scan would DROP every borrowed slot it has.
    let shelf_dir = aoide_storage::fs::songbook_dir(name).join("_widgets");

    let manifest_bytes = std::fs::read(&manifest_path).map_err(|e| WidgetSyncErr {
        error: format!(
            "failed to read shipped templates manifest.json at {}: {e}",
            manifest_path.display()
        ),
        target: manifest_path.to_string_lossy().into_owned(),
    })?;
    let registry_bytes = std::fs::read(&registry_path).map_err(|e| WidgetSyncErr {
        error: format!(
            "failed to read shipped templates registry.json at {}: {e}",
            registry_path.display()
        ),
        target: registry_path.to_string_lossy().into_owned(),
    })?;

    let mut manifest: serde_json::Value =
        serde_json::from_slice(&manifest_bytes).map_err(|e| WidgetSyncErr {
            error: format!("shipped templates manifest.json is not valid JSON: {e}"),
            target: manifest_path.to_string_lossy().into_owned(),
        })?;
    let mut registry: serde_json::Value =
        serde_json::from_slice(&registry_bytes).map_err(|e| WidgetSyncErr {
            error: format!("shipped templates registry.json is not valid JSON: {e}"),
            target: registry_path.to_string_lossy().into_owned(),
        })?;
    if !manifest.is_object() || !registry.is_object() {
        return Err(WidgetSyncErr {
            error: "shipped templates manifest.json/registry.json must both be JSON objects \
                    keyed by song name — refusing to write a malformed manifest.json/\
                    registry.json"
                .to_string(),
            target: templates.to_string_lossy().into_owned(),
        });
    }

    // Layer 2: overlay the EXISTING on-disk entries for every song that
    // still has a directory in the host songbook — see this function's own
    // doc for why (composed songs live outside the frozen baseline).
    overlay_surviving_entries(&mut manifest, &run_qml.join("songs").join("manifest.json"));
    overlay_surviving_entries(&mut registry, &run_qml.join("songs").join("registry.json"));

    // Layer 3: patch — `name`'s own entry always wins over both the
    // baseline and the overlay — but only when there IS a songbook directory
    // to scan. A shipped song re-pinned from the declared twin
    // (`song/declared/livery.json`, by `rice mode declarative` — the one door
    // that still reads it) on a host whose runtime songbook never seeded it
    // has nothing local to scan, and its baked baseline entry is the truth;
    // an empty patch here deleted sonata from osaka's manifest and blanked
    // every surface.
    if aoide_storage::fs::songbook_dir(name).is_dir() && !shelf_dir.is_dir() {
        let (own_manifest, own_registry) = scan_own_entry(name)?;
        let manifest_obj = manifest.as_object_mut().expect("checked is_object above");
        // Only songs with at least one slot appear in manifest.json (the same
        // asymmetry `lib/songbook.nix`'s own comment documents) — an empty
        // scan removes any stale entry for `name` rather than writing `{}`.
        match own_manifest.as_object() {
            Some(m) if !m.is_empty() => {
                manifest_obj.insert(name.to_string(), own_manifest);
            }
            _ => {
                manifest_obj.remove(name);
            }
        }
        let registry_obj = registry.as_object_mut().expect("checked is_object above");
        // registry.json keeps EVERY committed song, `{}` when it declares
        // nothing — always inserted, never conditionally removed.
        registry_obj.insert(name.to_string(), own_registry);
    }

    Ok(StageSongbook { manifest, registry })
}

/// Layer 2 of [`baseline_songbook`]'s merge: copy every entry
/// from the EXISTING on-disk file at `existing_path` into `target` (already
/// validated as a JSON object by the caller) — but ONLY for a song that
/// still has a directory under [`aoide_storage::fs::songbook_dir`] in the
/// host songbook. A song whose directory was since removed is silently
/// skipped, which is the prune: its entry has nowhere to survive from (not
/// in the frozen baseline, not in the on-disk overlay), so it simply isn't
/// present in the merged result — never an immortal stale key.
///
/// A missing or corrupt `existing_path` is treated as "nothing to overlay"
/// (the same "tolerate as empty" posture `undying::load_undying`/
/// `node_store`'s own loaders hold for their files) — this is a best-effort
/// preservation layer over what's already on disk, not a durable store of
/// its own; the baseline and the layer-3 patch are what makes every call
/// correct regardless of what this step finds.
fn overlay_surviving_entries(target: &mut serde_json::Value, existing_path: &Path) {
    let Ok(raw) = std::fs::read_to_string(existing_path) else {
        return;
    };
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return;
    };
    let Some(existing_obj) = parsed.as_object() else {
        return;
    };
    let target_obj = target
        .as_object_mut()
        .expect("baseline_songbook already validated `target` is an object");
    for (song, entry) in existing_obj {
        if aoide_storage::fs::songbook_dir(song).is_dir() {
            target_obj.insert(song.clone(), entry.clone());
        }
    }
}

/// `name`'s own manifest/registry entry, computed directly from its
/// committed songbook directory with no nix involved — the same "no
/// `_widgets/` shelf" formula `lib/songbook.nix`'s `songMeta` uses per song
/// (owner is always the song itself, `file` is always `<slot>.qml`; the
/// registry falls back to `livery.json`'s `.widgets // {}`). `rice compose`
/// never writes a `_widgets/` shelf, so this is the ONLY shape a freshly
/// composed song can ever have — [`baseline_songbook`] checks for
/// a shelf and refuses before ever calling this.
fn scan_own_entry(name: &str) -> Result<(serde_json::Value, serde_json::Value), WidgetSyncErr> {
    let song_dir = aoide_storage::fs::songbook_dir(name);

    let widgets_dir = song_dir.join("widgets");
    let manifest_entry = if widgets_dir.is_dir() {
        let slots = scan_slot_names(&widgets_dir)?;
        let mut m = serde_json::Map::new();
        for slot in slots {
            m.insert(
                slot.clone(),
                serde_json::json!({ "owner": name, "file": format!("{slot}.qml") }),
            );
        }
        serde_json::Value::Object(m)
    } else {
        serde_json::Value::Object(serde_json::Map::new())
    };

    let livery_path = song_dir.join("livery.json");
    let registry_entry = if livery_path.is_file() {
        let raw = std::fs::read_to_string(&livery_path).map_err(|e| WidgetSyncErr {
            error: e.to_string(),
            target: livery_path.to_string_lossy().into_owned(),
        })?;
        let parsed: serde_json::Value = serde_json::from_str(&raw).map_err(|e| WidgetSyncErr {
            error: format!("{}'s livery.json is not valid JSON: {e}", song_dir.display()),
            target: livery_path.to_string_lossy().into_owned(),
        })?;
        parsed
            .get("widgets")
            .cloned()
            .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()))
    } else {
        serde_json::Value::Object(serde_json::Map::new())
    };

    Ok((manifest_entry, registry_entry))
}

/// [`plan_stage`]'s answer: the whole songbook's manifest and registry, keyed
/// by song name, already merged for this stage. Opaque to callers — they hand
/// the same value to [`sync_song_widgets`] and [`sync_song_registry`] so one
/// `rice stage` evaluates the songbook once, and write what the gate approved.
pub struct StageSongbook {
    manifest: serde_json::Value,
    registry: serde_json::Value,
}

/// The `builtin.json` `pkgs/lyra-songbook` bakes beside the song folders:
/// `{ declared, songs, packages }`, the host's built-in set as lyra recorded
/// it (`modules/dendrites/lyra`'s override). Read for §7.5's two questions —
/// is this song built in, and does it need a package this system lacks.
pub(crate) struct BuiltIn {
    pub(crate) songs: Vec<String>,
    packages: Vec<String>,
}

/// `share/lyra/songbook/builtin.json`, read from `templates`. A MISSING file
/// means nothing is built in, and that is the truthful reading rather than a
/// hole: the only templates instance without one is the flake's own
/// `packages.lyra-songbook` (the whole songbook, no selection), and a host
/// running that installed no song's packages. Every stage then falls to case 2
/// with an empty `packages` baseline, which refuses anything that declares a
/// need. A PRESENT but unparseable file is a broken package and says so.
pub(crate) fn read_builtin(templates: &Path) -> Result<BuiltIn, WidgetSyncErr> {
    let path = templates.join("builtin.json");
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Ok(BuiltIn {
            songs: Vec::new(),
            packages: Vec::new(),
        });
    };
    let parsed: serde_json::Value = serde_json::from_str(&raw).map_err(|e| WidgetSyncErr {
        error: format!("shipped templates builtin.json is not valid JSON: {e}"),
        target: path.to_string_lossy().into_owned(),
    })?;
    let strings = |key: &str| -> Vec<String> {
        parsed
            .get(key)
            .and_then(|v| v.as_array())
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|e| e.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    Ok(BuiltIn {
        songs: strings("songs"),
        packages: strings("packages"),
    })
}

/// One baked `{ manifest, registry }` half from `templates` — the same
/// validation [`parse_songbook_eval`] applies to an evaluated payload, so a
/// malformed baked file cannot be written out as if it were the songbook.
fn read_baked(templates: &Path, file: &str) -> Result<serde_json::Value, WidgetSyncErr> {
    let path = templates.join(file);
    let bytes = std::fs::read(&path).map_err(|e| WidgetSyncErr {
        error: format!("failed to read the shipped {file} at {}: {e}", path.display()),
        target: path.to_string_lossy().into_owned(),
    })?;
    let parsed: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| WidgetSyncErr {
        error: format!("the shipped {file} at {} is not valid JSON: {e}", path.display()),
        target: path.to_string_lossy().into_owned(),
    })?;
    if !parsed.is_object() {
        return Err(WidgetSyncErr {
            error: format!("the shipped {file} must be a JSON object keyed by song name"),
            target: path.to_string_lossy().into_owned(),
        });
    }
    Ok(parsed)
}

/// The TOP-LEVEL directory names of a MACHINE's song folder that the seed
/// NEVER ships — its runtime scratch, each named where it is written:
///
///   `takes/`     the take store (`aoide-storage::takes` — `rice take`,
///                `rice back`'s drift snapshot, `cover set`'s archive)
///   `drafts/`    `aoide-storage::fs::drafts_dir` — `rice draft save`
///
/// Names, matched at the song folder's root only: `widgets/takes/` is a song's
/// own widget directory, and a difference there is a difference in the song.
///
/// `elements/` is deliberately NOT here, although `rice element seed` writes
/// into it: that directory is SONG-AUTHORED — `elements::seed_song` READS
/// `<song>/elements/*/element.json` from the machine's song folder as its
/// input — so a song that ships one has content there and a difference is a
/// real difference. Only a name the seed never ships AND that changes not one
/// staged byte belongs in this list.
///
/// Two consumers, each for its own reason:
///
///   - [`machine_copy_differs`] ignores these: a snapshot is not a difference
///     in the SONG. Comparing them would let `rice take` — an ordinary,
///     reversible operation — turn a built-in song into a "differing" machine
///     copy, and on a host with no nix that makes the song unstageable while
///     reporting `is not built into this system`, which is false on its face.
///   - `lyra rice declare` leaves them behind: the checkout carries the song,
///     not the machine's undo history and scratch.
///
/// The seed ships none of them, so every name here is a machine's own scratch
/// rather than content; a third runtime writer belongs in this list, and a
/// song that ever SHIPS one of these names needs this list revisited.
pub const MACHINE_RUNTIME_DIRS: &[&str] = &["takes", "drafts"];

/// §7.5 case 1's "no differing machine copy": the machine's folder for `name`
/// is absent, or matches the shipped one everywhere the SEED puts content
/// ([`MACHINE_RUNTIME_DIRS`] excepted).
///
/// CONTENT only, never permissions: the shipped copy comes out of the nix store
/// (0444/0555) while a machine copy was `cp -r`ed into `$AOIDE_ROOT`
/// (0644/0755), so a mode comparison would call every seeded song "differing"
/// and push a host with no nix down the generator path it cannot take. A
/// missing shipped folder counts as differing — there is nothing to be equal
/// to.
fn machine_copy_differs(name: &str, templates: &Path) -> bool {
    let machine = aoide_storage::fs::songbook_dir(name);
    if !machine.is_dir() {
        return false;
    }
    !trees_equal(&machine, &templates.join(name), MACHINE_RUNTIME_DIRS)
}

/// Recursive content equality of two directories: same relative file set, same
/// bytes — `skip` names excepted at THIS level on both sides, never below it
/// (callers pass [`MACHINE_RUNTIME_DIRS`], whose names are top-level only).
/// Directories are compared by what they hold, not by their mtimes, and
/// symlinks are followed (a song folder holds none).
pub(crate) fn trees_equal(a: &Path, b: &Path, skip: &[&str]) -> bool {
    let (Ok(a_entries), Ok(b_entries)) = (std::fs::read_dir(a), std::fs::read_dir(b)) else {
        return false;
    };
    let names = |entries: std::fs::ReadDir| -> Vec<String> {
        let mut names: Vec<String> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| !skip.contains(&name.as_str()))
            .collect();
        names.sort();
        names
    };
    let (a_names, b_names) = (names(a_entries), names(b_entries));
    if a_names != b_names {
        return false;
    }
    for name in a_names {
        let (a_path, b_path) = (a.join(&name), b.join(&name));
        let (a_dir, b_dir) = (a_path.is_dir(), b_path.is_dir());
        if a_dir != b_dir {
            return false;
        }
        if a_dir {
            if !trees_equal(&a_path, &b_path, &[]) {
                return false;
            }
        } else if std::fs::read(&a_path).ok() != std::fs::read(&b_path).ok() {
            return false;
        }
    }
    true
}

/// Shared validation for the test-only fixture seam's payload — must be
/// `{ manifest, registry }` with both fields objects, never a default/empty
/// value on partial/garbled input, which a caller could mistake for "the
/// songbook is genuinely empty" and write out.
#[cfg(test)]
fn parse_songbook_eval(bytes: &[u8], target: &str) -> Result<StageSongbook, WidgetSyncErr> {
    let parsed: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| WidgetSyncErr {
        error: format!("songbook eval output isn't valid JSON: {e}"),
        target: target.to_string(),
    })?;

    let manifest = parsed
        .get("manifest")
        .filter(|v| v.is_object())
        .cloned()
        .ok_or_else(|| WidgetSyncErr {
            error: "songbook eval output has no `manifest` object — refusing to write a malformed manifest.json"
                .to_string(),
            target: target.to_string(),
        })?;
    let registry = parsed
        .get("registry")
        .filter(|v| v.is_object())
        .cloned()
        .ok_or_else(|| WidgetSyncErr {
            error: "songbook eval output has no `registry` object — refusing to write a malformed registry.json"
                .to_string(),
            target: target.to_string(),
        })?;

    Ok(StageSongbook { manifest, registry })
}

/// Capture `<song>/songbook/<name>/widgets/` as `{ "<relative path>":
/// "<utf-8 content>" }` — the take store's own widget-body payload (`lyra
/// reload` design, settled 2026-08-31: "Takes gain WIDGET BODIES in both
/// modes", closing the gap that widget QML was never snapshotted). Reads
/// the SONGBOOK tree, the same source [`sync_song_widgets`] itself copies
/// from — not the deployed `run/qml` copy — because widget bodies are
/// song-scoped git substrate (this module's own header), the same source of
/// truth a take is meant to remember. An absent `widgets/` dir captures as
/// `{}` (no widgets), the same clean-skip [`sync_song_widgets`] uses for a
/// missing local tree. Unfiltered, recursive, matching
/// [`copy_tree_atomic`]'s own walk (helper components, asset subdirs,
/// `.gitkeep`, everything) — a take is a full-content snapshot, not a
/// filtered one. A non-UTF-8 file is a hard error: QML source is always
/// text, so an unreadable file here means something is already wrong, not
/// something to silently skip.
pub fn snapshot_widget_bodies(song: &str) -> Result<serde_json::Value, WidgetSyncErr> {
    let src = aoide_storage::fs::songbook_dir(song).join("widgets");
    let mut map = serde_json::Map::new();
    if src.is_dir() {
        capture_tree(&src, &src, &mut map)?;
    }
    Ok(serde_json::Value::Object(map))
}

/// [`snapshot_widget_bodies`]'s own recursive walk: `root` stays fixed
/// across the recursion (every captured key is relative to it), `dir` is
/// the directory currently being read.
fn capture_tree(
    root: &Path,
    dir: &Path,
    out: &mut serde_json::Map<String, serde_json::Value>,
) -> Result<(), WidgetSyncErr> {
    let entries = std::fs::read_dir(dir).map_err(|e| WidgetSyncErr {
        error: e.to_string(),
        target: dir.to_string_lossy().into_owned(),
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| WidgetSyncErr {
            error: e.to_string(),
            target: dir.to_string_lossy().into_owned(),
        })?;
        let file_type = entry.file_type().map_err(|e| WidgetSyncErr {
            error: e.to_string(),
            target: entry.path().to_string_lossy().into_owned(),
        })?;
        let path = entry.path();
        if file_type.is_dir() {
            capture_tree(root, &path, out)?;
            continue;
        }
        let bytes = std::fs::read(&path).map_err(|e| WidgetSyncErr {
            error: e.to_string(),
            target: path.to_string_lossy().into_owned(),
        })?;
        let text = String::from_utf8(bytes).map_err(|e| WidgetSyncErr {
            error: format!("{} is not valid UTF-8, cannot snapshot: {e}", path.display()),
            target: path.to_string_lossy().into_owned(),
        })?;
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        out.insert(rel, serde_json::Value::String(text));
    }
    Ok(())
}

/// The songs whose `widgets/` this stage must carry into `run/qml/songs/`:
/// `name`, plus every owner its manifest entry names.
///
/// A borrowed slot's BODY lives in its owner's `widgets/` (`lib/song.nix`'s
/// `composeSong`; CONTRACTS.md §5) and `StagingEngine.qml` resolves a slot to
/// `songs/<owner>/<file>` — so carrying only the staged song's own folder
/// leaves a borrower's lender absent from `run/qml` and the borrowed slot
/// renders NOTHING, with no error anywhere. The set is read off the entry
/// [`plan_stage`] already resolved (the generator's own answer for a
/// machine-authored composition, the baked baseline's for a built-in song)
/// rather than recomputed here: one source of ownership, in the place that
/// resolved it.
fn widget_owners(songbook: &StageSongbook, name: &str) -> Vec<String> {
    let mut owners = vec![name.to_string()];
    if let Some(entry) = songbook.manifest.get(name).and_then(|entry| entry.as_object()) {
        for slot in entry.values() {
            if let Some(owner) = slot.get("owner").and_then(|owner| owner.as_str()) {
                if !owners.iter().any(|seen| seen == owner) {
                    owners.push(owner.to_string());
                }
            }
        }
    }
    owners
}

/// Sync `<song>/songbook/<name>/widgets/` into `run/qml/songs/<name>/` and
/// regenerate `manifest.json` WHOLE (every song this machine's songbook
/// holds, from the one [`plan_stage`] answer) — the live-desktop half of
/// `rice stage`.
///
/// Clean-skips (`Ok`, empty `changed`) only when no `run/qml` runtime tree
/// is deployed at all (no `nixos-rebuild switch` yet) — there is nowhere to
/// write. Unlike the pre-C4 version, a MISSING `widgets/` dir for `name`
/// does NOT skip the manifest regeneration: the manifest is a whole-songbook
/// artifact, not a per-song one, so it stays current (and self-heals any
/// other song's stale/malformed entry) on every `rice stage` call once a
/// runtime tree exists, regardless of whether the ACTIVE song has bodies to
/// carry.
///
/// Pre-existing scope limit, now WIDENED (S9 review, M4): the staged song's
/// own bodies are carried, AND so are its LENDERS' — every owner its manifest
/// entry names ([`widget_owners`]), because a borrowed slot resolves to
/// `songs/<owner>/<file>` and would otherwise render nothing, silently. A
/// lender with no `widgets/` in the songbook is an error naming it, not a
/// skip. Still out of scope: a lender whose bodies were EDITED without
/// staging the lender (`rice stage <borrower>` carries the lender's bodies as
/// they are on disk, which is the same freshness every other body copy has).
pub fn sync_song_widgets(
    name: &str,
    songbook: &StageSongbook,
) -> Result<WidgetSyncOk, WidgetSyncErr> {
    let run_qml = aoide_storage::fs::run_qml_dir();
    if !run_qml.is_dir() {
        return Ok(WidgetSyncOk {
            changed: vec![],
            slots: vec![],
            bodies_changed: false,
            note: "no run/qml runtime tree deployed; widgets not synced".into(),
        });
    }

    let src = aoide_storage::fs::songbook_dir(name).join("widgets");
    let mut changed: Vec<String> = Vec::new();
    let local_slots = if src.is_dir() {
        let dst = run_qml.join("songs").join(name);
        copy_tree_atomic(&src, &dst, &[], &mut changed)?;
        scan_slot_names(&src)?
    } else {
        Vec::new()
    };

    // The runtime half of the borrow closure: every LENDER the staged song's
    // entry names must have its bodies under `run/qml/songs/<owner>/` too, or
    // the borrowed slot resolves to nothing (see [`widget_owners`]).
    let mut lenders: Vec<String> = Vec::new();
    for owner in widget_owners(songbook, name) {
        if owner == name {
            continue;
        }
        let lender_src = aoide_storage::fs::songbook_dir(&owner).join("widgets");
        if !lender_src.is_dir() {
            return Err(WidgetSyncErr {
                error: format!(
                    "`{name}` borrows a slot owned by `{owner}`, whose widget bodies are not in \
                     the songbook at {} — a borrowed slot cannot render without its lender's \
                     bodies",
                    lender_src.display()
                ),
                target: lender_src.to_string_lossy().into_owned(),
            });
        }
        copy_tree_atomic(&lender_src, &run_qml.join("songs").join(&owner), &[], &mut changed)?;
        lenders.push(owner);
    }

    let body_file_count = changed.len();
    let bodies_changed = body_file_count > 0;

    regenerate_manifest(songbook, &run_qml, &mut changed)?;

    let note = if !bodies_changed {
        "widget bodies already current".to_string()
    } else if lenders.is_empty() {
        format!("synced {body_file_count} widget file(s) into run/qml/songs/{name}")
    } else {
        format!(
            "synced {body_file_count} widget file(s) into run/qml/songs/{name} (borrowed bodies \
             carried from {})",
            lenders.join(", ")
        )
    };
    Ok(WidgetSyncOk { changed, slots: local_slots, bodies_changed, note })
}

/// Recursively copy `src` into `dst` — every file and subdir, including
/// `.gitkeep` and uppercase helper components, matching the nix build's
/// `cp -r "$d/widgets/."` — except the entries of `src` itself named in `skip`
/// (a directory, a file or a symlink alike; nothing below `src` is skipped).
/// A destination file whose bytes already match the source is left untouched
/// (not written, not counted in `changed`): Quickshell would otherwise
/// reload/reinstantiate every widget on every palette-only `rice stage` call,
/// causing visible flicker/lost widget state. Public because `lyra rice
/// declare` copies a song folder through it, skipping [`MACHINE_RUNTIME_DIRS`].
pub fn copy_tree_atomic(
    src: &Path,
    dst: &Path,
    skip: &[&str],
    changed: &mut Vec<String>,
) -> Result<(), WidgetSyncErr> {
    std::fs::create_dir_all(dst).map_err(|e| WidgetSyncErr {
        error: e.to_string(),
        target: dst.to_string_lossy().into_owned(),
    })?;
    let entries = std::fs::read_dir(src).map_err(|e| WidgetSyncErr {
        error: e.to_string(),
        target: src.to_string_lossy().into_owned(),
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| WidgetSyncErr {
            error: e.to_string(),
            target: src.to_string_lossy().into_owned(),
        })?;
        if skip.iter().any(|name| entry.file_name() == *name) {
            continue;
        }
        let file_type = entry.file_type().map_err(|e| WidgetSyncErr {
            error: e.to_string(),
            target: entry.path().to_string_lossy().into_owned(),
        })?;
        let dst_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_tree_atomic(&entry.path(), &dst_path, &[], changed)?;
            continue;
        }
        let bytes = std::fs::read(entry.path()).map_err(|e| WidgetSyncErr {
            error: e.to_string(),
            target: entry.path().to_string_lossy().into_owned(),
        })?;
        if std::fs::read(&dst_path).map(|existing| existing == bytes).unwrap_or(false) {
            continue; // byte-identical — skip, deliberately not counted as changed.
        }
        aoide_storage::fs::atomic_write_bytes(&dst_path, &bytes).map_err(|e| WidgetSyncErr {
            error: e.to_string(),
            target: dst_path.to_string_lossy().into_owned(),
        })?;
        changed.push(dst_path.to_string_lossy().into_owned());
    }
    Ok(())
}

/// `name`'s top-level slot files in `src` — informational only (see
/// [`WidgetSyncOk::slots`]'s doc), same rule the lyra lane's manifest
/// generation uses: a FILE (not a dir), not `.gitkeep`, ending `.qml`, first
/// byte
/// ascii-lowercase or an ascii digit.
fn scan_slot_names(src: &Path) -> Result<Vec<String>, WidgetSyncErr> {
    let entries = std::fs::read_dir(src).map_err(|e| WidgetSyncErr {
        error: e.to_string(),
        target: src.to_string_lossy().into_owned(),
    })?;
    let mut slots: Vec<String> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| WidgetSyncErr {
            error: e.to_string(),
            target: src.to_string_lossy().into_owned(),
        })?;
        let file_type = entry.file_type().map_err(|e| WidgetSyncErr {
            error: e.to_string(),
            target: entry.path().to_string_lossy().into_owned(),
        })?;
        if !file_type.is_file() {
            continue;
        }
        let file_name = entry.file_name();
        let Some(base) = file_name.to_str() else {
            continue;
        };
        if base == ".gitkeep" {
            continue;
        }
        let Some(stem) = base.strip_suffix(".qml") else {
            continue;
        };
        let Some(&first) = base.as_bytes().first() else {
            continue;
        };
        if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
            continue;
        }
        slots.push(stem.to_string());
    }
    slots.sort();
    slots.dedup();
    Ok(slots)
}

/// Regenerate `run_qml/songs/manifest.json` WHOLE from the songbook
/// [`plan_stage`] resolved — every song this machine's songbook holds, its
/// owner-map entry written outright, replacing the file. On
/// a checkout host the eval is total: a stale or malformed entry for ANY
/// song, not just the one being staged, self-heals on every call. On a
/// repo-less host [`baseline_songbook`]'s three-layer merge
/// self-heals the STAGED song's own entry on every call and preserves every
/// other still-live song's entry from the file this write is about to
/// replace (see that function's own doc).
fn regenerate_manifest(
    songbook: &StageSongbook,
    run_qml: &Path,
    changed: &mut Vec<String>,
) -> Result<(), WidgetSyncErr> {
    let eval = songbook;
    let manifest_path = run_qml.join("songs").join("manifest.json");
    let body = serde_json::to_string_pretty(&eval.manifest).unwrap_or_default() + "\n";
    let existing = std::fs::read_to_string(&manifest_path).ok();
    if existing.as_deref() != Some(body.as_str()) {
        aoide_storage::fs::atomic_write(&manifest_path, &body).map_err(|e| WidgetSyncErr {
            error: e.to_string(),
            target: manifest_path.to_string_lossy().into_owned(),
        })?;
        changed.push(manifest_path.to_string_lossy().into_owned());
    }
    Ok(())
}

/// Regenerate `run/qml/songs/registry.json` WHOLE from the songbook
/// [`plan_stage`] resolved — the second writer of the one generator's answer
/// (see the module doc's "one decision and one evaluation"). Same posture as [`regenerate_manifest`]: a
/// checkout host's eval is total (every committed song's registry entry
/// comes from THIS eval, every time); a repo-less host's merge self-heals
/// the staged song and preserves every other still-live song's entry from
/// the file this write is about to replace.
///
/// Clean-skips (`Ok`, empty `changed`) when no `run/qml` runtime tree is
/// deployed at all — mirrors [`sync_song_widgets`]'s own not-yet-switched
/// early return (`registry.json` lives under the same tree).
pub fn sync_song_registry(
    name: &str,
    songbook: &StageSongbook,
) -> Result<RegistrySyncOk, WidgetSyncErr> {
    let run_qml = aoide_storage::fs::run_qml_dir();
    if !run_qml.is_dir() {
        return Ok(RegistrySyncOk {
            changed: vec![],
            widgets: serde_json::json!({}),
            note: "no run/qml runtime tree deployed; registry not synced".into(),
        });
    }

    let eval = songbook;
    let registry_path = run_qml.join("songs").join("registry.json");
    let body = serde_json::to_string_pretty(&eval.registry).unwrap_or_default() + "\n";
    let existing = std::fs::read_to_string(&registry_path).ok();
    let differs = existing.as_deref() != Some(body.as_str());
    let mut changed: Vec<String> = Vec::new();
    if differs {
        aoide_storage::fs::atomic_write(&registry_path, &body).map_err(|e| WidgetSyncErr {
            error: e.to_string(),
            target: registry_path.to_string_lossy().into_owned(),
        })?;
        changed.push(registry_path.to_string_lossy().into_owned());
    }

    let widgets = eval.registry.get(name).cloned().unwrap_or_else(|| serde_json::json!({}));

    let note = if differs {
        format!("synced `{name}`'s widget-type registry entry")
    } else {
        "widget-type registry entry already current".to_string()
    };
    Ok(RegistrySyncOk { changed, widgets, note })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_test_support::{env_lock, unique_tmp, EnvSaver};

    fn write(path: &Path, body: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    #[test]
    fn machine_runtime_dirs_are_scratch_at_the_top_level_only() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        let root = unique_tmp("widgets-machine-copy");
        std::env::set_var("AOIDE_STAGE_DIR", root.join("song").join("stage"));
        let templates = root.join("templates");
        let machine = aoide_storage::fs::songbook_dir("dusk");
        for dir in [templates.join("dusk"), machine.clone()] {
            write(&dir.join("livery.json"), "{}");
            write(&dir.join("widgets").join("takes").join("Bar.qml"), "// bar\n");
        }
        assert!(!machine_copy_differs("dusk", &templates));

        write(&machine.join("takes").join("0001.json"), "{}");
        write(&machine.join("drafts").join("neon").join("livery.json"), "{}");
        assert!(
            !machine_copy_differs("dusk", &templates),
            "a take and a draft at the song's top level are not a difference in the song"
        );

        write(&machine.join("widgets").join("takes").join("Bar.qml"), "// edited\n");
        assert!(
            machine_copy_differs("dusk", &templates),
            "widgets/takes/ is the song's own widget directory, so an edit there is a difference"
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}
