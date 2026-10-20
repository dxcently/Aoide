//! The rice runtime's paths: the rice stage, the declared twin, the rice tree,
//! its drafts and the machine's rices. Strillix owns these roots, so
//! `aoide-storage` names none of them.
//!
//! `$STRILLIX_ROOT` (default `<home>/.strillix`) carries the rice stage
//! (`stage/`), the declared twin (`declared/`), the rice tree (`rices/`,
//! `wallpapers/`) and the rice drafts. `$AOIDE_STAGE_DIR` is the conducting
//! stage's override and never relocates any of them. The live QML and element
//! trees (`run/`) stay under `$AOIDE_ROOT`.

use aoide_storage::fs::{flake_root, root};

/// The rice runtime root: `$STRILLIX_ROOT` (absolute-path-wins, the same
/// discipline as [`root`]), default `<home>/.strillix`. Rice state (the
/// stage, the declared twin, the machine's rices, the wallpaper library)
/// hangs off it; aoide's own state never does. Every rice path, [`stage_dir`]
/// included, derives from it.
pub fn strillix_root() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("STRILLIX_ROOT") {
        let p = std::path::PathBuf::from(&dir);
        if p.is_absolute() {
            return p;
        }
    }
    aoide_protocol::aoide_home().join(".strillix")
}

/// The rice stage: `$STRILLIX_ROOT/stage/` (default `~/.strillix/stage/`). Every
/// rice stage reader and writer routes through here, and the rice stage lock
/// (`strillix-rice`'s `lock`) guards this directory.
pub fn stage_dir() -> std::path::PathBuf {
    strillix_root().join("stage")
}

/// The desktop's audit log: `$STRILLIX_ROOT/log`. Every strillix dispatch, bridge and schema
/// line lands here; core keeps `$AOIDE_ROOT/log`.
pub fn audit_log() -> std::path::PathBuf {
    strillix_root().join("log")
}

/// The live-deployed QML tree the desktop shell reads: `$AOIDE_ROOT/run/qml/`
/// (default `~/.aoide/run/qml/`), a sibling of `state/`. Never under the rice
/// stage.
pub fn run_qml_dir() -> std::path::PathBuf {
    root().join("run").join("qml")
}

/// The live-deployed element config tree non-QML rice targets (waybar,
/// dunst, ...) read from: `$AOIDE_ROOT/run/elements/` (default
/// `~/.aoide/run/elements/`), a sibling of [`run_qml_dir`] (ELEMENTS.md,
/// "Runtime layout").
pub fn run_elements_dir() -> std::path::PathBuf {
    root().join("run").join("elements")
}

/// The host's own config file — the NixOS module holding this machine's
/// `habit.rice.declared` line, which `strillix rice declare <rice> --host`
/// rewrites. `$AOIDE_HOST_CONFIG` (the core option `aoide.hostConfig`,
/// exported beside `$AOIDE_FLAKE_ROOT` on the same units and login shells) when
/// it is absolute; otherwise the option's own default spelled in code,
/// `<flake_root>/hosts/<hostname>/default.nix`, so unset == set-to-default.
/// `None` when neither names a file: no override and no usable hostname (one
/// carrying a `/` or spelling `.`/`..` is never joined into a path).
pub fn host_config() -> Option<std::path::PathBuf> {
    if let Ok(file) = std::env::var("AOIDE_HOST_CONFIG") {
        let p = std::path::PathBuf::from(&file);
        if p.is_absolute() {
            return Some(p);
        }
    }
    let host = aoide_storage::display::os_hostname()?;
    if host.contains('/') || host == "." || host == ".." {
        return None;
    }
    Some(flake_root().join("hosts").join(host).join("default.nix"))
}

/// The strillix shell SOURCE, as a path relative to [`flake_root`]: the QML tree,
/// the resolved icons and the preview fixture sets a checkout carries — the
/// same directory `pkgs/strillix-shell` builds its `share/strillix/{qml,icons,
/// preview}` from, so the path a checkout is READ from and the path the
/// package is BUILT from cannot drift apart.
///
/// Named ONCE, here. Every site that CONSTRUCTS the shell path joins the const
/// onto [`flake_root`] (`strillix`'s `commands::preview` stages the tree into
/// an isolated preview root; `strillix-rice`'s `compose` interpolates the const
/// into the scaffold doc that points a new rice's author at `qml/slots.md`) —
/// prose and doc comments, and that scaffold text, may name the literal
/// `"pkgs/strillix-shell"`, which nothing resolves. A second checkout-relative
/// tree earns its own const beside this one rather than a literal at the call
/// site — the rename this exists to survive is exactly the one that leaves a
/// string behind in a crate nobody thought to grep.
pub const STRILLIX_SHELL_SRC: &str = "pkgs/strillix-shell";

/// Pure tier logic for [`rice_templates_dir`] — the same two-tier shape
/// `aoide_protocol::bin`'s sibling-binary resolver uses (env override, then
/// a sibling of `current_exe()`'s directory gated on its OWN existence
/// check), applied to a directory instead of an executable, so the tier
/// decision is testable without a real `current_exe()`/filesystem probe.
/// `exe_dir` is `current_exe()`'s parent (`None` if that call failed);
/// `sibling_is_dir` stands in for the real `Path::is_dir()` check
/// [`rice_templates_dir`] performs.
fn resolve_rice_templates_dir(
    env_value: Option<&str>,
    exe_dir: Option<&std::path::Path>,
    sibling_is_dir: bool,
) -> Option<std::path::PathBuf> {
    if let Some(dir) = env_value {
        let p = std::path::PathBuf::from(dir);
        if p.is_absolute() {
            return Some(p);
        }
    }
    let dir = exe_dir?;
    if sibling_is_dir {
        Some(dir.join("..").join("share").join("strillix").join("rices"))
    } else {
        None
    }
}

/// The shipped SCORE TEMPLATES dir (L-C3, strillix-carrier lane, task #107):
/// `<templates>/<rice>/theme.json` is what `rice compose --from <rice>`
/// (`strillix-rice`'s `commands::rice`) falls back to reading when
/// [`rice_dir`] has nothing yet for that rice — the ordinary case on a
/// repo-less host, which has no `$AOIDE_FLAKE_ROOT` checkout to have
/// composed FROM in the first place. `<templates>` also carries prebaked
/// `manifest.json`/`registry.json` (`strillix-rice::widgets`'s own fallback,
/// baked at nix build time by `lib/rices.nix` — the SAME generator
/// [`flake_root`]'s `nix eval` path calls at runtime on a checkout host) for
/// the same repo-less case.
///
/// Two tiers, tried in order, absolute-path-wins discipline matching every
/// other override in this file:
///   1. `$STRILLIX_TEMPLATES` — trusted unconditionally once it resolves
///      to an absolute path, no existence check (same as [`root`]/
///      [`flake_root`]). The tier a nix unit sets, wired onto every unit
///      that already carries `$AOIDE_ROOT`/`$AOIDE_FLAKE_ROOT`
///      (`modules/nucleus/{aoided,shellbridge,secrets,melete-adapter}.nix`)
///      — a store-path boundary sibling resolution cannot cross.
///   2. the sibling of `current_exe()`'s directory,
///      `<exe_dir>/../share/strillix/rices` — ONLY if that directory
///      actually exists (mirrors `aoide_protocol::bin`'s own tier-2 rule:
///      handing back a path that isn't there only moves the failure
///      downstream for no benefit). What a non-nix tarball install
///      (`bin/strillix` + `share/strillix/rices/` sitting beside it) uses; on a
///      NixOS host this tier is never actually reached, since tier 1 is
///      always set there.
///
/// `None` when neither resolves — this function has no opinion on error
/// text (same as every other pure resolver here); the caller turns that
/// into a taught error naming both locations it checked.
pub fn rice_templates_dir() -> Option<std::path::PathBuf> {
    let env_value = std::env::var("STRILLIX_TEMPLATES").ok();
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(std::path::Path::to_path_buf));
    let sibling_is_dir = exe_dir
        .as_deref()
        .map(|dir| dir.join("..").join("share").join("strillix").join("rices"))
        .is_some_and(|p| p.is_dir());
    resolve_rice_templates_dir(env_value.as_deref(), exe_dir.as_deref(), sibling_is_dir)
}

/// `$STRILLIX_TEMPLATES`'s parent — the shipped `share/strillix` dir. Beside
/// the templates sit the two build-time payloads a repo-less machine reads to
/// do its own work without a checkout: `nix/manifest.nix` (§9's offline
/// rices generator, evaluated by `strillix-rice::widgets`) and
/// `aoide-options.json` (`strillix onboard`'s option set). Named ONCE here, for
/// the same reason [`STRILLIX_SHELL_SRC`] is: two crates derive it from
/// [`rice_templates_dir`], and a second derivation of the same parent is a
/// rename waiting to leave one of them behind.
///
/// `None` exactly when [`rice_templates_dir`] is `None` — there is no
/// templates dir to sit beside.
pub fn strillix_share_dir() -> Option<std::path::PathBuf> {
    rice_templates_dir().and_then(|templates| {
        templates
            .parent()
            .map(std::path::Path::to_path_buf)
    })
}

/// The committed-rice directory: `<rice>/rices/<name>/`.
///
/// Under [`strillix_root`]: a test that pins `STRILLIX_ROOT` gets an isolated
/// rice tree.
pub fn rice_dir(name: &str) -> std::path::PathBuf {
    strillix_root().join("rices").join(name)
}

/// The machine's rices ROOT: `<strillix>/rices/`, the directory a rice is
/// looked up BY NAME in ([`rice_dir`]) and the argument the offline
/// generator is pointed at (§9). A sibling of [`rice_dir`] rather than a
/// second path spelled at the call site, for the same one-seam reason.
pub fn rices_root() -> std::path::PathBuf {
    strillix_root().join("rices")
}

/// The committed-rice notes file: `<rice>/rices/<name>/theme.json`.
pub fn rice_notes(name: &str) -> std::path::PathBuf {
    rice_dir(name).join("theme.json")
}

/// The DECLARED rice's notes — the declared twin, `<rice>/declared/theme.json`
/// (CONTRACTS.md §4).
///
/// Same content as the active rice's committed `theme.json` with the venue's
/// `aoide.theme.override` applied (a plain file, never a symlink), published
/// by the strillix lane's activation seed — the file's own `"rice"` field
/// says WHICH rice that was. Read for the declarative re-pin
/// (`rice mode declarative`), which restores declared truth, and by
/// `declared_rice()` to learn WHICH rice is declared; staging derives from the
/// runtime rices instead ([`declared_venue`]). Absent on a host that never
/// activated the lane (or before its first activation); readers then fall back
/// to the committed rices for that rice. Rooted at [`strillix_root`] like every
/// other rice-tree path.
pub fn declared_notes() -> std::path::PathBuf {
    strillix_root().join("declared").join("theme.json")
}

/// What `strillix rice refresh` last wrote into the machine's copy of built-in rice
/// `name` — `<rice>/declared/rices/<name>.json`, `{schemaVersion, rice,
/// source, files: {relpath: sha256}}`. Beside [`declared_notes`] and outside
/// the rice folder on purpose: the folder-equality gate, `rice declare`,
/// `rice list`, `rice history` and the widget sync never see it.
pub fn rice_record(name: &str) -> std::path::PathBuf {
    strillix_root().join("declared").join("rices").join(format!("{name}.json"))
}

/// The venue's recolour of the declared rice — `<rice>/declared/venue.json`
/// (CONTRACTS.md §4): the slots `aoide.theme.override` changes, tier by tier,
/// published by the strillix lane's activation seed (`{}` when the host sets no
/// override). Staging the declared rice overlays it on the runtime rices's
/// copy. Absent on a host that never activated the lane; readers then overlay
/// nothing.
pub fn declared_venue() -> std::path::PathBuf {
    strillix_root().join("declared").join("venue.json")
}

/// The state link the compositor sources for the rice:
/// `<home>/.local/state/strillix/rice.conf` (CONTRACTS.md §4), `<home>` being
/// [`aoide_protocol::aoide_home`], the one home source [`root`] and
/// [`strillix_root`] default through. Not `$XDG_STATE_HOME`: the renderer's
/// Nix activation hardcodes that path, so the two halves name one file. Its
/// target is the declared twin ([`declared_rice_conf`]) or a staged
/// `hyprland.conf` (`rice stage`).
pub fn rice_conf_link() -> std::path::PathBuf {
    aoide_protocol::aoide_home()
        .join(".local")
        .join("state")
        .join("strillix")
        .join("rice.conf")
}

/// The declared twin of the rice: `$STRILLIX_ROOT/declared/rice.conf`, the
/// file the renderer's own Nix output is linked to. Under [`strillix_root`]: the
/// renderer writes under `strillix.root`.
pub fn declared_rice_conf() -> std::path::PathBuf {
    strillix_root().join("declared").join("rice.conf")
}

/// The staging-owned empty rice, `<stage>/empty-hyprland.conf`: the target a
/// stage links to when its rice ships no `hyprland.conf`, so the link never
/// keeps the previous rice's binds.
pub fn empty_rice_conf() -> std::path::PathBuf {
    stage_dir().join("empty-hyprland.conf")
}

/// A committed rice's drafts root: `<rice>/rices/<name>/drafts/` —
/// durable scratch for `rice draft save`, gitignored and outside `stage/` (a
/// draft is NOT the live stage, and NOT committed truth; that distinction is
/// the entire point of the feature). Nested under the rice it varies, not a
/// flat top-level dir: a draft is fundamentally a variation of an ALREADY
/// COMPOSED rice, so it belongs inside that rice's own directory, not a
/// separate global namespace.
pub fn rice_drafts_dir(rice: &str) -> std::path::PathBuf {
    rice_dir(rice).join("drafts")
}

/// One named draft's directory: `<rice>/rices/<name>/drafts/<draft>/` —
/// holds a snapshot of `stage/theme.json` (always) and `stage/wallpaper.json`
/// (when the stage had one) at the moment `rice draft save <draft>` was run.
pub fn draft_dir(rice: &str, draft: &str) -> std::path::PathBuf {
    rice_drafts_dir(rice).join(draft)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_test_support::{pin_roots, EnvSaver, MigrationEnvGuard};

    /// A scratch path that IS absolute on THIS host, as a `String` for the
    /// env-override APIs (see `aoide_storage`'s own copy of this helper).
    fn scratch(name: &str) -> String {
        std::env::temp_dir().join(name).to_string_lossy().to_string()
    }

    #[test]
    fn the_rice_stage_lives_under_strillix_root_and_ignores_the_conducting_override() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let root = std::path::PathBuf::from(scratch("strillix-paths-stage"));
        let _env = pin_roots(&root);
        assert_eq!(stage_dir(), root.join("stage"));

        // The conducting stage's override names that stage and nothing else.
        std::env::set_var("AOIDE_STAGE_DIR", scratch("strillix-paths-conducting"));
        assert_eq!(stage_dir(), root.join("stage"));
    }

    #[test]
    fn the_rice_tree_composes_off_strillix_root() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let root = std::path::PathBuf::from(scratch("strillix-paths-tree"));
        let _env = pin_roots(&root);
        assert_eq!(strillix_root(), root);
        assert_eq!(rices_root(), root.join("rices"));
        assert_eq!(rice_dir("moonlight"), root.join("rices").join("moonlight"));
        assert_eq!(rice_notes("moonlight"), root.join("rices").join("moonlight").join("theme.json"));
        assert_eq!(declared_notes(), root.join("declared").join("theme.json"));
        assert_eq!(declared_venue(), root.join("declared").join("venue.json"));
        assert_eq!(rice_record("moonlight"), root.join("declared").join("rices").join("moonlight.json"));
        assert_eq!(declared_rice_conf(), root.join("declared").join("rice.conf"));
        assert_eq!(empty_rice_conf(), root.join("stage").join("empty-hyprland.conf"));
    }

    #[test]
    fn drafts_nest_under_the_rice_they_vary() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let root = std::path::PathBuf::from(scratch("strillix-paths-drafts"));
        let _env = pin_roots(&root);
        assert_eq!(rice_drafts_dir("moonlight"), root.join("rices").join("moonlight").join("drafts"));
        assert_eq!(
            draft_dir("moonlight", "dim"),
            root.join("rices").join("moonlight").join("drafts").join("dim")
        );
    }

    #[test]
    fn run_trees_hang_off_aoide_root_and_never_the_rice_stage() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let aoide = std::path::PathBuf::from(scratch("strillix-paths-run-aoide"));
        let strillix = std::path::PathBuf::from(scratch("strillix-paths-run-strillix"));
        let _env = EnvSaver::capture(&["AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_STAGE_DIR"]);
        std::env::set_var("AOIDE_ROOT", &aoide);
        std::env::set_var("STRILLIX_ROOT", &strillix);
        std::env::remove_var("AOIDE_STAGE_DIR");
        assert_eq!(run_qml_dir(), aoide.join("run").join("qml"));
        assert_eq!(run_elements_dir(), aoide.join("run").join("elements"));
        assert_eq!(stage_dir(), strillix.join("stage"));
    }

    #[test]
    fn host_config_honors_an_absolute_override() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvSaver::capture(&["AOIDE_HOST_CONFIG"]);
        std::env::set_var("AOIDE_HOST_CONFIG", scratch("strillix-paths-host.nix"));
        assert_eq!(host_config(), Some(std::path::PathBuf::from(scratch("strillix-paths-host.nix"))));
    }

    #[test]
    fn rice_templates_tiers_resolve_in_order() {
        // Pure tier logic — no real env/filesystem, mirroring
        // `aoide_protocol::bin::tiers_resolve_in_order`'s table shape for
        // the same two-tier resolver applied to a directory.
        // The fixtures are HOST-absolute (`scratch`'s doc): a `/opt/...` or
        // `/usr/bin` literal is not absolute on Windows, so the env tier the
        // case is about would be ignored and the sibling tier would answer
        // instead — the assertion would then be measuring the wrong tier.
        let env_dir = scratch("opt/custom/rices");
        let exe_dir = scratch("usr/bin");
        let sibling = format!("{exe_dir}/../share/strillix/rices");
        let cases: &[(&str, Option<&str>, Option<&str>, bool, Option<&str>)] = &[
            (
                "env wins even when a sibling dir exists",
                Some(env_dir.as_str()),
                Some(exe_dir.as_str()),
                true,
                Some(env_dir.as_str()),
            ),
            (
                "env wins over the no-sibling case too",
                Some(env_dir.as_str()),
                None,
                false,
                Some(env_dir.as_str()),
            ),
            (
                "a relative env value is ignored, falls through to the sibling",
                Some("relative/rices"),
                Some(exe_dir.as_str()),
                true,
                Some(sibling.as_str()),
            ),
            (
                "sibling used only when it actually exists",
                None,
                Some(exe_dir.as_str()),
                true,
                Some(sibling.as_str()),
            ),
            (
                "sibling absent (not a dir) resolves to nothing",
                None,
                Some(exe_dir.as_str()),
                false,
                None,
            ),
            ("no exe dir and no env resolves to nothing", None, None, false, None),
        ];

        for (label, env_value, exe_dir, sibling_is_dir, expect) in cases {
            let got = resolve_rice_templates_dir(*env_value, exe_dir.map(std::path::Path::new), *sibling_is_dir);
            assert_eq!(got, expect.map(std::path::PathBuf::from), "{label}");
        }
    }

    #[test]
    fn rice_templates_dir_honors_an_absolute_env_override() {
        // The impure public wrapper: an absolute `$STRILLIX_TEMPLATES`
        // wins outright regardless of whatever `current_exe()`'s own
        // sibling resolves to in this test binary (which never has a real
        // `share/strillix/rices` beside it).
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("STRILLIX_TEMPLATES").ok();

        std::env::set_var("STRILLIX_TEMPLATES", scratch("strillix-rice-templates-test"));
        assert_eq!(
            rice_templates_dir(),
            Some(std::path::PathBuf::from(scratch("strillix-rice-templates-test")))
        );

        // Empty/relative values are ignored, same discipline as every other
        // override in this file — falls through to the sibling tier, which
        // resolves to `None` for this test binary.
        std::env::set_var("STRILLIX_TEMPLATES", "");
        assert_eq!(rice_templates_dir(), None);
        std::env::set_var("STRILLIX_TEMPLATES", "relative/templates");
        assert_eq!(rice_templates_dir(), None);

        std::env::remove_var("STRILLIX_TEMPLATES");
        assert_eq!(rice_templates_dir(), None);

        match saved {
            Some(v) => std::env::set_var("STRILLIX_TEMPLATES", v),
            None => std::env::remove_var("STRILLIX_TEMPLATES"),
        }
    }

    /// Both rice roots default through the one home source the aoide root
    /// does: a faked `$HOME` with neither override set puts both under it.
    #[test]
    fn strillix_root_defaults_under_the_same_home_as_root() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let home = std::env::temp_dir().join(format!("aoide-strillix-default-home-{}", std::process::id()));
        std::env::set_var("HOME", &home);

        assert_eq!(root(), home.join(".aoide"));
        assert_eq!(strillix_root(), home.join(".strillix"));
        assert_eq!(strillix_root(), home.join(".strillix"));
    }

    #[test]
    fn rice_conf_seams_name_home_for_the_link_and_the_root_for_the_twin() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = aoide_test_support::EnvSaver::capture(&["HOME", "AOIDE_ROOT", "STRILLIX_ROOT", "STRILLIX_ROOT", "AOIDE_STAGE_DIR"]);
        let home = scratch("aoide-test-rice-home");
        let root = scratch("aoide-test-rice-root");
        std::env::set_var("HOME", &home);
        std::env::set_var("AOIDE_ROOT", &root);
        std::env::set_var("STRILLIX_ROOT", &root);
        std::env::set_var("STRILLIX_ROOT", &root);
        std::env::remove_var("AOIDE_STAGE_DIR");

        assert_eq!(
            rice_conf_link(),
            std::path::PathBuf::from(&home).join(".local/state/strillix/rice.conf"),
        );
        assert_eq!(
            declared_rice_conf(),
            std::path::PathBuf::from(&root).join("declared/rice.conf"),
        );
        assert_eq!(
            empty_rice_conf(),
            std::path::PathBuf::from(&root).join("stage/empty-hyprland.conf"),
        );
    }
}
