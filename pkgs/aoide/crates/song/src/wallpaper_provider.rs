//! The wallpaper provider's bridge — who paints a song's wallpaper here, and
//! what that provider should be showing right now.
//!
//! The shell's own `aoide-wallpaper` layer is the DEFAULT provider and needs no
//! call: it watches `stage/cover.json` itself. An external provider does, so
//! this module is the one place Aoide asks — and tells — it. Posture copied from
//! `live.rs`, the sibling seam: best-effort, never fatal, never a `Result` the
//! caller has to handle, and never a process under this crate's own test build
//! (the same `cfg!(test)` gate `live::apply_live` carries, for the same reason:
//! a test that reached a real provider would repaint the operator's desktop).
//!
//! The host's choice is the fact `aoide.wallpaper.provider`, which the `lyra`
//! lane publishes as `song/stage/wallpaper-provider` — one name, in the runtime
//! root, so the CLI, the QML and this module read the same word. Absent (a host
//! that never activated the lane) means the shell's own layer.
//!
//! TWO DOORS, one wait between them. `sync()` makes ONE attempt and is what
//! every interactive writer calls — a `cover set` must not stall for five
//! seconds because a daemon is down. `sync_waiting()` holds the reachable wait
//! and is what `lyra cover sync` calls, because that is the door a unit's
//! `ExecStartPost` runs while the daemon is still binding its socket: there, a
//! refusal is a race worth waiting out on every output.

use crate::cover;
use serde_json::Value;
use std::process::Command;
use std::time::{Duration, Instant};

/// The provider that paints picks on its own layer-shell surface.
pub const PROVIDER_SKWD_WALL: &str = "skwd-wall";

/// The provider the shell itself paints with, and the answer when nothing says
/// otherwise.
pub const PROVIDER_QUICKSHELL: &str = "quickshell";

/// The file the `lyra` lane publishes the active provider's name in, inside the
/// stage dir.
pub const PROVIDER_FILE: &str = "wallpaper-provider";

/// The env var the provider's own lane exports: the store path of the 1x1 fully
/// transparent PNG a pick-less external provider is applied with.
pub const STANDIN_ENV: &str = "AOIDE_SKWD_WALL_STANDIN";

/// The client that drives the external provider.
const HELM: &str = "skwd-helm";

/// Every output, always: a per-output apply is what the provider's own no-op
/// guard does NOT cover, so an Aoide-side apply to one output can start an
/// unbounded loop (measured 278 applies in 2 s). One wildcard target, one apply.
const ALL_OUTPUTS: &str = "*";

/// How long `sync_waiting` keeps trying a provider that is not there yet, and
/// how often it looks.
const REACHABLE_WAIT: Duration = Duration::from_secs(5);
const REACHABLE_POLL: Duration = Duration::from_millis(250);

/// The staged provider: the file the lane publishes, else the shell's own layer.
pub fn provider() -> String {
    std::fs::read_to_string(aoide_storage::fs::stage_dir().join(PROVIDER_FILE))
        .ok()
        .map(|raw| raw.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| PROVIDER_QUICKSHELL.to_string())
}

/// The pick that APPLIES right now: the staged cover's own `(kind, identity)`
/// when it is marked a pick AND stamped for the song staged right now — or
/// stamped for NO song, the legacy shape, which applies wherever it lands
/// (`cover::staged_song`'s own rule). Anything else is `None`, which is the read
/// the shell's own layer makes before it draws the song's default instead.
pub fn applying_pick() -> Option<(String, String)> {
    if !cover::staged_is_pick() {
        return None;
    }
    if let Some(stamped) = cover::staged_song() {
        if Some(stamped) != staged_song() {
            return None;
        }
    }
    cover::staged_identity()
}

/// The song the stage file names.
fn staged_song() -> Option<String> {
    let raw = std::fs::read_to_string(aoide_storage::fs::stage_dir().join("livery.json")).ok()?;
    let doc: Value = serde_json::from_str(&raw).ok()?;
    doc.get("song")?.as_str().map(str::to_string)
}

/// The step-aside image's store path, or `None` on a host whose lane did not
/// export one.
pub fn standin_png() -> Option<String> {
    std::env::var(STANDIN_ENV)
        .ok()
        .filter(|path| !path.is_empty())
}

/// What a provider should be showing for one staged state — the whole decision,
/// pure, so the truth table is a unit test rather than a live desktop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// The shell's own layer paints: there is no call to make.
    Nothing,
    /// The external provider paints this pick.
    Apply { kind: String, identity: String },
    /// The external provider paints nothing, on every output.
    StandAside,
}

/// The decision: an external provider shows the staged pick when one applies and
/// nothing at all otherwise (which is what makes a song switch step aside for
/// free — the pick stops applying). Every other provider stands down.
pub fn action(provider: &str, pick: Option<(String, String)>) -> Action {
    if provider != PROVIDER_SKWD_WALL {
        return Action::Nothing;
    }
    match pick {
        Some((kind, identity)) => Action::Apply { kind, identity },
        None => Action::StandAside,
    }
}

/// The target `skwd-helm apply` is given for one pick: the path for a still or a
/// video, the `we:<id>` library key for a scene (a bare id is not a key the
/// provider resolves).
fn apply_target(kind: &str, identity: &str) -> String {
    if kind == cover::KIND_WE {
        cover::scene_token(identity)
    } else {
        identity.to_string()
    }
}

/// How one `skwd-helm` call went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Helm {
    Ok,
    /// It ran and said the provider is not there (exit 3, the documented
    /// unreachable code).
    Unreachable,
    /// It ran and answered: a refusal nothing here can do anything about.
    Refused,
    /// It did not run at all — no `skwd-helm` on `PATH`. An answer too.
    Missing,
}

fn classify(out: &std::process::Output) -> Helm {
    if out.status.success() {
        Helm::Ok
    } else if out.status.code() == Some(3) {
        Helm::Unreachable
    } else {
        Helm::Refused
    }
}

fn attempt(args: &[&str]) -> Helm {
    Command::new(HELM)
        .args(args)
        .output()
        .map(|out| classify(&out))
        .unwrap_or(Helm::Missing)
}

/// One step of the wait loop: is another attempt due? Only an UNREACHABLE
/// provider is worth asking again — a refusal and a missing client are answers,
/// and the timeout is the caller's.
fn retry_unreachable(outcome: Helm, wait: bool, expired: bool) -> bool {
    wait && !expired && outcome == Helm::Unreachable
}

fn helm(args: &[&str], wait: bool) -> Helm {
    let deadline = Instant::now() + REACHABLE_WAIT;
    loop {
        let outcome = attempt(args);
        if !retry_unreachable(outcome, wait, Instant::now() >= deadline) {
            return outcome;
        }
        std::thread::sleep(REACHABLE_POLL);
    }
}

fn apply(target: &str, wait: bool) -> Helm {
    helm(&["apply", target, "-o", ALL_OUTPUTS], wait)
}

/// Make the external provider show nothing. The clean primitive is the
/// provider's own `clear` verb, which the release Aoide first shipped against
/// does not have: `clear` is tried first and any refusal falls back to the
/// stand-in image — the two answers are indistinguishable from here, and a
/// `clear` that landed leaves nothing for the image to do. One function, so
/// dropping the stand-in once `clear` is everywhere is one change.
fn step_aside(wait: bool) -> &'static str {
    match helm(&["clear", "-o", ALL_OUTPUTS], wait) {
        Helm::Ok => "stepped aside (cleared)",
        // No client at all: the stand-in would fail the same way, so say it once.
        Helm::Missing => "best-effort: skwd-helm is not on PATH",
        _ => match standin_png() {
            Some(png) => match apply(&png, wait) {
                Helm::Ok => "stepped aside (stand-in image)",
                Helm::Refused => "best-effort: the provider refused the step-aside image",
                Helm::Unreachable => "best-effort: skwd-helm unreachable",
                Helm::Missing => "best-effort: skwd-helm is not on PATH",
            },
            None => "best-effort: no step-aside image (AOIDE_SKWD_WALL_STANDIN unset)",
        },
    }
}

/// Make the provider agree with the stage files. Returns a short status string
/// for the caller's outcome envelope; NEVER a `Result` — a failed or absent
/// provider must not fail a stage, a pick or a revert (the stage files are
/// already the source of truth for the shell's half of the picture). ONE attempt.
pub fn sync() -> &'static str {
    sync_with(false)
}

/// [`sync`], holding the reachable wait.
pub fn sync_waiting() -> &'static str {
    sync_with(true)
}

fn sync_with(wait: bool) -> &'static str {
    match action(&provider(), applying_pick()) {
        Action::Nothing => "not the wallpaper provider (the shell's own layer paints)",
        Action::StandAside => {
            if cfg!(test) {
                return "skipped (this crate's own test build: no live skwd-helm)";
            }
            step_aside(wait)
        }
        Action::Apply { kind, identity } => {
            if cfg!(test) {
                return "skipped (this crate's own test build: no live skwd-helm)";
            }
            match apply(&apply_target(&kind, &identity), wait) {
                Helm::Ok => "applied the staged pick",
                Helm::Refused => "best-effort: the provider refused the staged pick",
                Helm::Unreachable => "best-effort: skwd-helm unreachable",
                Helm::Missing => "best-effort: skwd-helm is not on PATH",
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_test_support::*;
    use std::os::unix::process::ExitStatusExt;

    fn output(code: i32) -> std::process::Output {
        std::process::Output {
            status: std::process::ExitStatus::from_raw(code << 8),
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }

    fn stage(root: &std::path::Path) -> std::path::PathBuf {
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        stage
    }

    fn stage_song(stage: &std::path::Path, song: &str) {
        std::fs::write(stage.join("livery.json"), format!(r#"{{"song":"{song}"}}"#)).unwrap();
    }

    /// The whole decision, as a truth table: every provider, with and without a
    /// pick that applies. The shell's layer is never called; the external
    /// provider shows the pick when one applies and nothing when none does.
    #[test]
    fn the_action_is_the_provider_crossed_with_a_pick_that_applies() {
        let pick = || Some((cover::KIND_VIDEO.to_string(), "/tmp/clip.mp4".to_string()));

        assert_eq!(action(PROVIDER_QUICKSHELL, pick()), Action::Nothing);
        assert_eq!(action(PROVIDER_QUICKSHELL, None), Action::Nothing);
        assert_eq!(action("", pick()), Action::Nothing, "an unknown provider is no provider");

        assert_eq!(
            action(PROVIDER_SKWD_WALL, pick()),
            Action::Apply {
                kind: cover::KIND_VIDEO.to_string(),
                identity: "/tmp/clip.mp4".to_string(),
            }
        );
        assert_eq!(action(PROVIDER_SKWD_WALL, None), Action::StandAside);
    }

    /// A scene's target is the library key the provider resolves, never the bare
    /// id (`skwd-helm apply we:123`; a bare `123` is exit 2).
    #[test]
    fn a_scene_is_applied_by_its_key_and_a_file_by_its_path() {
        assert_eq!(apply_target(cover::KIND_WE, "123"), "we:123");
        assert_eq!(apply_target(cover::KIND_STATIC, "/tmp/a.png"), "/tmp/a.png");
        assert_eq!(apply_target(cover::KIND_VIDEO, "/tmp/a.mp4"), "/tmp/a.mp4");
    }

    /// The provider's own exit codes: 0 is done, 3 is "not there yet", anything
    /// else is an answer (2 not found, 4 bad args, 6 invalid params — the ones
    /// its `--help` documents).
    #[test]
    fn the_providers_exit_codes_classify_as_answers_or_a_missing_provider() {
        assert_eq!(classify(&output(0)), Helm::Ok);
        assert_eq!(classify(&output(3)), Helm::Unreachable);
        for answer in [1, 2, 4, 5, 6] {
            assert_eq!(classify(&output(answer)), Helm::Refused, "exit {answer}");
        }
    }

    /// Who waits, and on what: only the `cover sync` door, and only for a
    /// provider that is not there yet — a refusal and a missing client are
    /// answers, and the deadline ends the wait either way.
    #[test]
    fn only_the_sync_door_waits_and_only_on_an_unreachable_provider() {
        for outcome in [Helm::Ok, Helm::Refused, Helm::Missing] {
            for wait in [false, true] {
                for expired in [false, true] {
                    assert!(
                        !retry_unreachable(outcome, wait, expired),
                        "{outcome:?} wait={wait} expired={expired}"
                    );
                }
            }
        }
        assert!(retry_unreachable(Helm::Unreachable, true, false));
        assert!(!retry_unreachable(Helm::Unreachable, false, false), "a writer makes one attempt");
        assert!(!retry_unreachable(Helm::Unreachable, true, true), "the wait is bounded");
    }

    #[test]
    fn the_provider_is_the_published_name_and_quickshell_when_absent() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        let root = unique_tmp("provider-name");
        let stage = stage(&root);
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        assert_eq!(provider(), PROVIDER_QUICKSHELL, "absent means the shell's own layer");
        std::fs::write(stage.join(PROVIDER_FILE), "skwd-wall\n").unwrap();
        assert_eq!(provider(), PROVIDER_SKWD_WALL);
        std::fs::write(stage.join(PROVIDER_FILE), "   \n").unwrap();
        assert_eq!(provider(), PROVIDER_QUICKSHELL, "a blank word is not a provider");
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// A pick applies only while it is marked AND stamped for the song staged
    /// right now — the read the shell's own layer makes, from the other side —
    /// except that a cover stamped for NO song is the legacy shape and applies
    /// wherever it lands (`cover::staged_song`).
    #[test]
    fn a_pick_applies_for_its_own_staged_song_and_for_the_legacy_shape() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        let root = unique_tmp("provider-applying-pick");
        let stage = stage(&root);
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        stage_song(&stage, "cadenza");

        cover::stage_pick(std::path::Path::new("/tmp/clip.mp4"), Some("cadenza")).unwrap();
        assert_eq!(
            applying_pick(),
            Some((cover::KIND_VIDEO.to_string(), "/tmp/clip.mp4".to_string()))
        );

        // A cover stamped for another song stops applying, which is what makes a
        // switch step aside with no extra call.
        std::fs::write(
            stage.join("cover.json"),
            r#"{"path":"/tmp/clip.mp4","kind":"video","pick":true,"song":"dusk"}"#,
        )
        .unwrap();
        assert_eq!(applying_pick(), None);

        // The legacy shape — a pick with no `song` field — applies here too.
        std::fs::write(
            stage.join("cover.json"),
            r#"{"path":"/tmp/old.png","pick":true}"#,
        )
        .unwrap();
        assert_eq!(
            applying_pick(),
            Some((cover::KIND_STATIC.to_string(), "/tmp/old.png".to_string()))
        );

        // The song's own default is never a pick, and neither is a marker with
        // nothing named.
        cover::stage_default(std::path::Path::new("/tmp/song.png"), "cadenza").unwrap();
        assert_eq!(applying_pick(), None);
        std::fs::write(stage.join("cover.json"), r#"{"pick":true,"song":"cadenza"}"#).unwrap();
        assert_eq!(applying_pick(), None);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_standin_is_read_from_the_environment_alone() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&[STANDIN_ENV]);

        std::env::remove_var(STANDIN_ENV);
        assert_eq!(standin_png(), None);
        std::env::set_var(STANDIN_ENV, "/nix/store/x-standin.png");
        assert_eq!(standin_png().as_deref(), Some("/nix/store/x-standin.png"));
        std::env::set_var(STANDIN_ENV, "");
        assert_eq!(standin_png(), None, "an empty export is not a path");
    }

    /// Both doors are inert in this crate's own test build: they report the
    /// decision and run no client (`live::apply_live`'s gate). Which door WAITS,
    /// and on what, is `retry_unreachable`'s truth table — the loop itself cannot
    /// be observed here without running the client.
    #[test]
    fn both_doors_are_inert_in_this_crates_own_test_build() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        let root = unique_tmp("provider-sync-test-build");
        let stage = stage(&root);
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        assert!(sync().starts_with("not the wallpaper provider"));
        assert!(sync_waiting().starts_with("not the wallpaper provider"));

        std::fs::write(stage.join(PROVIDER_FILE), PROVIDER_SKWD_WALL).unwrap();
        stage_song(&stage, "cadenza");
        cover::stage_scene("123", Some("cadenza")).unwrap();
        for status in [sync(), sync_waiting()] {
            assert_eq!(status, "skipped (this crate's own test build: no live skwd-helm)");
        }
        std::fs::remove_dir_all(&root).unwrap();
    }
}
