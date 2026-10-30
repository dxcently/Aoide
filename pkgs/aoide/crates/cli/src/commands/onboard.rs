//! `onboard` — the first-boot flow (docs/architecture/ONBOARD.md), core's half.
//!
//! CLI-only (ONBOARD.md's flow diagram: "Door::Cli only, from a checkout") —
//! an interactive install flow that shells out to a child process makes no
//! sense over MCP/A2A/the daemon, the same reasoning `events tail` already
//! carries for its own foreground-follow shape. From a checkout only
//! (decision 10): the clone IS what gets registered, and the skill link
//! `hooks install` performs needs the repo (the package never ships the
//! skill dir). Registers the clone, asks which harnesses to wire and runs the
//! already-registered `hooks install` for each, then prints the same
//! `aoide guide` output the stub's schema summary always promised. The
//! desktop half (rice seed, `~/rice` link, nix) is not this command's to run.
//!
//! Root-coupled like `meta`/`stubs`/`infra` (this crate's own README/
//! AGENTS): it reads the fully-assembled registry twice — to call the
//! already-registered `hooks.install` handler directly (never reimplemented,
//! crates/AGENTS.md's "no cross-crate copying") and to render the closing
//! guide — neither reachable without depending on this crate, so onboard
//! lives here rather than in a domain crate.

use crate::dispatch::Invocation;
use crate::output::{Outcome, Status};
use crate::registry::{cmd, flag, Registry};
use aoide_protocol::agents::{agent_profile, known_agents, on_path};
use aoide_protocol::{choose_many, interactive, Door};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub fn register(r: &mut Registry) {
    r.insert(cmd!(
        path: ["onboard"],
        summary: "First-boot flow: register the clone, wire harness hooks, print the guide. The desktop half is not part of this command.",
        args: [],
        flags: [
            flag!("harness", "string", "Comma-separated harnesses to wire (claude,kimi,pi,eidolon); given at all, skips the interactive ask and wires exactly this list (default: ask on a tty, else every harness found on PATH)."),
            flag!("yes", "bool", "Skip every interactive prompt end to end: use --harness if given, else every harness found on PATH."),
        ],
        gated: false,
        implemented: true,
        handler: handle_onboard,
        brief: "First boot: register the clone, wire harness hooks, print the guide.",
        doors: &[Door::Cli],
        kind: "none", verb: "none",
    ));
}

fn handle_onboard(inv: &Invocation) -> Outcome {
    let cmd = "onboard";
    let quiet = inv.flag_present("json");
    let say = |s: &str| {
        if !quiet {
            println!("{s}");
        }
    };

    let Some(root) = checkout_root() else {
        return Outcome::error(
            cmd,
            "aoide onboard must run from inside an Aoide checkout (no .claude/skills/aoide/SKILL.md found walking up from the cwd) -- clone the repo (`git clone <upstream> ~/Aoide`) and run onboard from inside it",
        )
        .with_data(json!({ "reason": "no-checkout" }));
    };

    say(&format!("onboard: registering the clone ({})...", root.display()));
    let (project_registered, mut changed, project_note) = register_project(&root);
    say(&format!("  {project_note}"));

    let harnesses = match resolve_harnesses(inv) {
        Ok(h) => h,
        Err(out) => return out,
    };
    let mut hook_reports: Vec<serde_json::Value> = Vec::new();
    if harnesses.is_empty() {
        say("onboard: no harnesses selected -- nothing to wire");
    } else {
        say(&format!("onboard: wiring hooks for {}...", harnesses.join(", ")));
        for h in &harnesses {
            let out = run_hooks_install(h, inv.door);
            say(&format!("  {}", out.message));
            hook_reports.push(json!({
                "agent": h,
                "ok": out.status == Status::Ok,
                "message": out.message,
            }));
            changed.extend(out.changed.clone());
        }
    }

    if !quiet {
        println!("{}", crate::guide::render(crate::dispatch::registry()));
        // Last, deliberately: the node line is what the operator walks away
        // with. It is the machine's own identity for someone else's charter
        // (P-CHARTER) — public keys only, and the age binding is minted here
        // on first call if it does not exist yet.
        match aoide_storage::charter::node_line() {
            Ok(line) => println!(
                "onboard: this machine's node line — paste it under `[nodes]` in a charter:\n  {line}"
            ),
            Err(e) => println!("onboard: node line unavailable: {e}"),
        }
    }

    let message = if project_registered {
        format!(
            "onboard: core half complete -- clone registered, {} harness(es) wired. The desktop half (rice seed, ~/rice link, nix half) is not part of this command.",
            harnesses.len()
        )
    } else {
        format!(
            "onboard: core half incomplete -- project registration failed (see notes above), {} harness(es) wired.",
            harnesses.len()
        )
    };
    let data = json!({
        "root": root.display().to_string(),
        "harnesses": harnesses,
        "hooks": hook_reports,
        "projectRegistered": project_registered,
    });
    if project_registered {
        Outcome::ok(cmd, message).changed(changed).with_data(data)
    } else {
        Outcome::error(cmd, message).changed(changed).with_data(data)
    }
}

/// The invoking checkout's root: `hooks::skill_source()`'s own walk-up (the
/// only repo-root detector in the tree, ONBOARD.md decision 10) returns
/// `<root>/.claude/skills`; two `.parent()` calls strip `skills`/`.claude`
/// back to `<root>`.
fn checkout_root() -> Option<PathBuf> {
    let skills_dir = crate::conduct::commands::hooks::skill_source()?;
    skills_dir.parent()?.parent().map(Path::to_path_buf)
}

/// "register the clone" (ONBOARD.md's stub-summary contract, decision 1): the
/// shell-only, idempotent act onboard owns. `project.add`'s own schema example
/// (`project add aoide ~/Aoide`) is the existing "register X as known"
/// mechanism this reuses -- but onboarding runs before any daemon exists to
/// dispatch to, so this calls `register_bootstrap_project` (the
/// onboarding-only bootstrap entry, `conduct/src/graph/manage.rs`) directly
/// rather than `project_add`, which is daemon-owned and would error with no
/// daemon running. A failed registration is never silent: its note carries the
/// `project registration failed: ` prefix and the flag is false, so
/// `handle_onboard` never reports an unconditional ok.
fn register_project(root: &Path) -> (bool, Vec<String>, String) {
    let out = crate::conduct::graph::register_bootstrap_project("aoide", &root.display().to_string(), false);
    if out.status == Status::Ok {
        (true, out.changed, out.message)
    } else {
        (false, out.changed, format!("project registration failed: {}", out.message))
    }
}

/// Which harnesses to wire. `--harness` (comma-separated -- the repo's
/// established stand-in for a repeatable flag over this registry's flat
/// `BTreeMap<String,String>` flag model, `events tail --class`'s own
/// convention) skips the ask outright and wires exactly that list;
/// otherwise `--yes` or a non-tty invocation silently takes every harness
/// found on PATH; otherwise the interactive multi-select (ONBOARD.md
/// decision 7) asks, preselected by the same PATH probe.
fn resolve_harnesses(inv: &Invocation) -> Result<Vec<&'static str>, Outcome> {
    if let Some(raw) = inv.flags.get("harness") {
        let names: Vec<&str> = raw.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
        let mut chosen = Vec::new();
        for name in names {
            match known_agents().iter().find(|k| **k == name) {
                Some(k) => chosen.push(*k),
                None => {
                    return Err(Outcome::usage(
                        "onboard",
                        format!("unknown harness `{name}` (known: {})", known_agents().join(", ")),
                    ))
                }
            }
        }
        return Ok(chosen);
    }

    let known = known_agents();
    let on_path_idx: Vec<usize> = (0..known.len())
        .filter(|&i| agent_profile(known[i]).is_some_and(on_path))
        .collect();

    let non_interactive = inv.flag_present("yes") || !interactive(inv.door);
    if non_interactive {
        return Ok(on_path_idx.iter().map(|&i| known[i]).collect());
    }

    let rows: Vec<String> = known.iter().map(|n| n.to_string()).collect();
    match choose_many("which harnesses should onboard wire?", &rows, &on_path_idx) {
        Some(sel) => Ok(sel.into_iter().filter_map(|i| known.get(i).copied()).collect()),
        None => Ok(Vec::new()),
    }
}

/// Call the already-registered `hooks install <agent>` handler directly off
/// the assembled registry -- never reimplemented (crates/AGENTS.md's "no
/// cross-crate copying"), the exact entry point a plain `aoide hooks install
/// <agent>` invocation would reach (including its skill-symlink pass).
fn run_hooks_install(agent: &str, door: Door) -> Outcome {
    let path = vec!["hooks".to_string(), "install".to_string()];
    match crate::dispatch::registry().get(&path) {
        Some(entry) => entry.invoke(
            "aoide",
            &Invocation {
                path,
                args: vec![agent.to_string()],
                flags: BTreeMap::new(),
                door,
            },
        ),
        None => Outcome::error("onboard", "hooks.install is not registered (unexpected)"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_test_support::{env_lock, unique_tmp, EnvSaver};

    /// Every variable an onboard run reads a path or a PATH from. The tests
    /// that run the handler save and restore all of them.
    const ONBOARD_ENV: &[&str] = &[
        "HOME",
        "AOIDE_USER",
        "AOIDE_ROOT",
        "AOIDE_STAGE_DIR",
        "AOIDE_DAEMON_SOCKET",
        "PATH",
    ];

    // ── register_project: registers locally, no daemon required ──────────

    #[test]
    fn register_project_registers_the_clone_with_no_daemon_reachable() {
        // The 18e5dd5 regression this pins: `project add` became
        // daemon-owned (`manage.rs`'s `local_daemon`), but onboarding runs
        // before any daemon exists -- the documented first-run path is
        // `git clone … && cd ~/Aoide && aoide onboard`, no daemon step.
        // `register_project` must land the clone in `projects.json` itself,
        // even with `AOIDE_DAEMON_SOCKET` pointed at a dead path under a
        // stage nothing else touches -- never a silent no-op because
        // nothing answered.
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvSaver::capture(&["HOME", "AOIDE_STAGE_DIR", "AOIDE_DAEMON_SOCKET"]);
        let root = unique_tmp("onboard-register-project-no-daemon-root");
        let home = unique_tmp("onboard-register-project-no-daemon-home");
        let stage = home.join(".aoide/stage");
        std::env::set_var("HOME", &home);
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("AOIDE_DAEMON_SOCKET", stage.join("no-such-daemon.sock"));

        let (registered, _changed, note) = register_project(&root);
        assert!(registered, "note: {note}");

        let raw = std::fs::read_to_string(stage.join("projects.json"))
            .expect("projects.json written locally with no daemon reachable");
        let doc: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let projects = doc["projects"].as_array().unwrap();
        let clone_path = root.display().to_string();
        let p = projects
            .iter()
            .find(|p| p["name"] == "aoide")
            .expect("the clone registered as project `aoide`");
        assert_eq!(p["path"], clone_path);
        assert_eq!(p["roots"][0], clone_path);

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&home);
    }

    // ── resolve_harnesses: --harness parsing ─────────────────────────────

    fn onboard_inv(flags: &[(&str, &str)]) -> Invocation {
        Invocation {
            path: vec!["onboard".to_string()],
            args: vec![],
            flags: flags.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            door: Door::Cli,
        }
    }

    #[test]
    fn resolve_harnesses_from_explicit_flag_preserves_given_order() {
        let inv = onboard_inv(&[("harness", "kimi,claude")]);
        assert_eq!(resolve_harnesses(&inv).unwrap(), vec!["kimi", "claude"]);
    }

    #[test]
    fn resolve_harnesses_trims_whitespace_and_drops_empty_tokens() {
        let inv = onboard_inv(&[("harness", " claude ,, kimi ")]);
        assert_eq!(resolve_harnesses(&inv).unwrap(), vec!["claude", "kimi"]);
    }

    #[test]
    fn resolve_harnesses_rejects_an_unknown_name() {
        let inv = onboard_inv(&[("harness", "claude,bogus")]);
        let err = resolve_harnesses(&inv).unwrap_err();
        assert_eq!(err.status, Status::Usage);
        assert!(err.message.contains("unknown harness `bogus`"), "message: {}", err.message);
        assert!(err.message.contains("claude, kimi, pi"), "message: {}", err.message);
    }

    // ── checkout_root: the from-a-checkout detector ──────────────────────

    #[test]
    fn checkout_root_finds_the_repo_root_by_walking_up_to_the_skill_marker() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let root = unique_tmp("onboard-checkout-found");
        std::fs::create_dir_all(root.join(".claude/skills/aoide")).unwrap();
        std::fs::write(root.join(".claude/skills/aoide/SKILL.md"), "").unwrap();
        let nested = root.join("pkgs/aoide/crates/cli");
        std::fs::create_dir_all(&nested).unwrap();

        let saved_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&nested).unwrap();
        let found = checkout_root();
        std::env::set_current_dir(&saved_cwd).unwrap();

        assert_eq!(
            found.map(|p| std::fs::canonicalize(p).unwrap()),
            Some(std::fs::canonicalize(&root).unwrap())
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn checkout_root_is_none_outside_a_checkout() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let root = unique_tmp("onboard-checkout-missing");

        let saved_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();
        let found = checkout_root();
        std::env::set_current_dir(&saved_cwd).unwrap();

        assert!(found.is_none(), "found a checkout root where there is none: {found:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    // ── the core half, run for real on a private home ────────────────────

    /// A throwaway checkout (the `.claude/skills/aoide/SKILL.md` marker is all
    /// `checkout_root` reads, and `rice/` and `rices/` are there so the
    /// desktop half would have something to link and seed) beside a private
    /// home that every write lands in.
    fn private_checkout(tag: &str) -> (PathBuf, PathBuf) {
        let checkout = unique_tmp(&format!("{tag}-checkout"));
        std::fs::create_dir_all(checkout.join(".claude/skills/aoide")).unwrap();
        std::fs::write(checkout.join(".claude/skills/aoide/SKILL.md"), "").unwrap();
        std::fs::create_dir_all(checkout.join("rice")).unwrap();
        std::fs::create_dir_all(checkout.join("rices")).unwrap();
        (checkout, unique_tmp(&format!("{tag}-home")))
    }

    /// The real handler, as `aoide onboard --yes --json`, run from `checkout`
    /// with HOME, the stage and the daemon socket under `home` and `path` as
    /// the whole PATH. The caller holds `env_lock` and an `EnvSaver` over
    /// `ONBOARD_ENV`.
    fn onboard_yes_from(checkout: &Path, home: &Path, path: &Path) -> Outcome {
        let stage = home.join(".aoide/stage");
        std::env::set_var("HOME", home);
        std::env::remove_var("AOIDE_USER");
        std::env::set_var("AOIDE_ROOT", home.join(".aoide"));
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("AOIDE_DAEMON_SOCKET", stage.join("no-such-daemon.sock"));
        std::env::set_var("PATH", path);
        let saved_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(checkout).unwrap();
        let out = handle_onboard(&onboard_inv(&[("yes", ""), ("json", "")]));
        std::env::set_current_dir(saved_cwd).unwrap();
        out
    }

    #[test]
    fn onboard_yes_with_no_strillix_on_path_completes_the_core_half_and_says_so() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvSaver::capture(ONBOARD_ENV);
        let (checkout, home) = private_checkout("onboard-no-strillix");
        let empty_path = home.join("empty-path");
        std::fs::create_dir_all(&empty_path).unwrap();

        let out = onboard_yes_from(&checkout, &home, &empty_path);

        assert_eq!(out.status, Status::Ok, "message: {}", out.message);
        assert!(out.message.contains("core half complete"), "message: {}", out.message);
        assert_eq!(out.data.as_ref().map(|d| &d["projectRegistered"]), Some(&json!(true)));
        assert!(
            std::fs::symlink_metadata(home.join("rice")).is_err(),
            "the core half linked ~/rice, which is the desktop half's job"
        );
        assert!(
            !checkout.join("rices/preferences.md").exists(),
            "the core half seeded rices/, which is the desktop half's job"
        );
        let raw = std::fs::read_to_string(home.join(".aoide/stage/projects.json"))
            .expect("the clone registered on the private stage");
        let doc: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert!(
            doc["projects"].as_array().unwrap().iter().any(|p| p["name"] == "aoide"),
            "no `aoide` project: {raw}"
        );

        let _ = std::fs::remove_dir_all(&checkout);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// GATED on Unix with its reason: the stand-in `strillix` is a `#!/bin/sh`
    /// script, which Windows will not run as a PATH program. It proves core
    /// never runs a strillix that resolves on PATH.
    #[cfg(unix)]
    #[test]
    fn onboard_never_runs_a_strillix_that_resolves() {
        use std::os::unix::fs::PermissionsExt;
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvSaver::capture(ONBOARD_ENV);
        let (checkout, home) = private_checkout("onboard-stub-strillix");
        let bin = home.join("stub-bin");
        std::fs::create_dir_all(&bin).unwrap();
        let stub = bin.join("strillix");
        let ran = home.join("strillix-ran");
        std::fs::write(&stub, format!("#!/bin/sh\necho ran >> '{}'\n", ran.display())).unwrap();
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();

        let out = onboard_yes_from(&checkout, &home, &bin);

        assert_eq!(out.status, Status::Ok, "message: {}", out.message);
        assert!(!ran.exists(), "core ran a strillix that resolved");

        let _ = std::fs::remove_dir_all(&checkout);
        let _ = std::fs::remove_dir_all(&home);
    }
}
