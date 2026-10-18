//! `onboard` — the first-boot install-script flow (docs/architecture/
//! ONBOARD.md), core's shell-only half.
//!
//! CLI-only (ONBOARD.md's flow diagram: "Door::Cli only, from a checkout") —
//! an interactive install flow that shells out to a child process makes no
//! sense over MCP/A2A/the daemon, the same reasoning `events tail` already
//! carries for its own foreground-follow shape. From a checkout only
//! (decision 10): the clone IS what gets registered, and the skill link
//! `hooks install` performs needs the repo (the package never ships the
//! skill dir). Registers the clone, seeds the rices, asks which
//! harnesses to wire and runs the already-registered `hooks install` for
//! each, probes for `strillix` and delegates the nix half to `strillix onboard` as a
//! child process when it resolves (core never speaks nix itself — root
//! `AGENTS.md`'s core/strillix split), then prints the same `aoide guide` output
//! the stub's schema summary always promised.
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
use aoide_protocol::{aoide_home, choose_many, interactive, Door};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The note prefix `register_clone` writes on a failed project registration
/// and `handle_onboard` reads back to refuse an unconditional ok — one
/// literal, so neither side can drift from the other.
const PROJECT_FAILED_PREFIX: &str = "project registration failed: ";

pub fn register(r: &mut Registry) {
    r.insert(cmd!(
        path: ["onboard"],
        summary: "First-boot flow: register the clone, seed rices, wire harness hooks, delegate the nix half to `strillix onboard` when strillix is available, print the guide.",
        args: [],
        flags: [
            flag!("harness", "string", "Comma-separated harnesses to wire (claude,kimi,pi,eidolon); given at all, skips the interactive ask and wires exactly this list (default: ask on a tty, else every harness found on PATH)."),
            flag!("yes", "bool", "Skip every interactive prompt end to end, including the strillix delegate step: use --harness if given, else every harness found on PATH."),
            flag!("out", "string", "Forwarded to `strillix onboard` as its own --out (default: strillix's own default, ./aoide.nix); omitted unless given here."),
        ],
        gated: false,
        implemented: true,
        handler: handle_onboard,
        brief: "First boot: register the clone, wire harness hooks, print the guide.",
        doors: &[Door::Cli],
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

    let mut changed: Vec<String> = Vec::new();

    say(&format!("onboard: registering the clone ({})...", root.display()));
    let (clone_changed, clone_notes) = register_clone(&root);
    let project_registered = !clone_notes.iter().any(|n| n.starts_with(PROJECT_FAILED_PREFIX));
    for n in &clone_notes {
        say(&format!("  {n}"));
    }
    changed.extend(clone_changed);

    say("onboard: seeding the rices...");
    let (seeded, seed_note) = seed_rices(&root);
    say(&format!("  {seed_note}"));
    if seeded {
        changed.push(seed_note.clone());
    }

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

    say("onboard: probing for strillix...");
    let strillix_note = probe_and_delegate_strillix(inv);
    say(&format!("  {strillix_note}"));

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
        format!("onboard: clone registered, {} harness(es) wired -- {strillix_note}", harnesses.len())
    } else {
        format!(
            "onboard: project registration failed (see notes above), {} harness(es) wired -- {strillix_note}",
            harnesses.len()
        )
    };
    let data = json!({
        "root": root.display().to_string(),
        "harnesses": harnesses,
        "hooks": hook_reports,
        "strillix": strillix_note,
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

/// "register the clone" (ONBOARD.md's stub-summary contract, decision 1):
/// two shell-only, idempotent acts the wiki already documents as onboard's
/// job. `project.add`'s own schema example (`project add aoide
/// ~/Aoide`) is the existing "register X as known" mechanism this reuses --
/// but onboarding runs before any daemon exists to dispatch to, so this
/// calls `register_bootstrap_project` (the onboarding-only bootstrap entry,
/// `conduct/src/graph/manage.rs`) directly rather than `project_add`, which
/// is daemon-owned and would error with no daemon running; Rice-Anatomy.md/
/// Rice-Vocabulary.md/Clone-and-Run.md all separately state "onboard links
/// `~/rice` -> `~/Aoide/rice`", so the second act is the symlink they
/// already document, made real. Never a clobber: an existing correct link
/// is a no-op, anything else at either path is left alone with a note. A
/// failed registration is never silent: the note is prefixed
/// `"project registration failed: "` so `handle_onboard` can tell success
/// from failure without inventing a third return value.
fn register_clone(root: &Path) -> (Vec<String>, Vec<String>) {
    let mut changed = Vec::new();
    let mut notes = Vec::new();

    let out = crate::conduct::graph::register_bootstrap_project(
        "aoide",
        &root.display().to_string(),
        false,
    );
    notes.push(if out.status == Status::Ok {
        out.message.clone()
    } else {
        format!("{PROJECT_FAILED_PREFIX}{}", out.message)
    });
    changed.extend(out.changed);

    let home_rice = aoide_home().join("rice");
    let root_rice = root.join("rice");
    match std::fs::symlink_metadata(&home_rice) {
        Ok(meta) if meta.file_type().is_symlink() => {
            // The exact-path compare catches a symlink correctly aimed at
            // `root_rice` whose target doesn't (yet) exist -- canonicalize
            // fails on a dangling target, which would otherwise misreport a
            // correctly-targeted link as "elsewhere" just because the read
            // happened before `root/rice` existed. The canonicalize fallback
            // still catches an equivalent but differently-spelled target
            // (relative vs absolute, a symlink chain) once it resolves.
            let same_target = std::fs::read_link(&home_rice).map(|t| t == root_rice).unwrap_or(false);
            let resolves = same_target
                || matches!(
                    (std::fs::canonicalize(&home_rice), std::fs::canonicalize(&root_rice)),
                    (Ok(l), Ok(s)) if l == s
                );
            if resolves {
                notes.push(format!("~/rice already links to {}", root_rice.display()));
            } else {
                let target = std::fs::read_link(&home_rice)
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| "<unreadable>".to_string());
                notes.push(format!(
                    "{} is already a symlink to {target}, not to {} -- leaving it (remove it yourself to relink)",
                    home_rice.display(),
                    root_rice.display()
                ));
            }
        }
        Ok(_) => notes.push(format!(
            "{} already exists and is not a symlink -- leaving it (move it aside to let onboard link it)",
            home_rice.display()
        )),
        Err(_) => match aoide_storage::fs::link_dir(&root_rice, &home_rice) {
            Ok(()) => {
                notes.push(format!("linked {} -> {}", home_rice.display(), root_rice.display()));
                changed.push(format!("~/rice -> {}", root_rice.display()));
            }
            Err(e) => notes.push(format!("could not link {}: {e}", home_rice.display())),
        },
    }

    (changed, notes)
}

/// "seed rices" (ONBOARD.md's stub-summary contract, decision 1):
/// `rices/` travels committed with the clone itself
/// (`learnings.md`/`update-playbook.md` are already there on any fresh
/// checkout) — the one root file Rice-Anatomy.md documents as part of the
/// rices's cross-cutting trio that a fresh clone still lacks is
/// `preferences.md`. Idempotent: only ever created, never overwritten.
fn seed_rices(root: &Path) -> (bool, String) {
    let path = root.join("rices/preferences.md");
    if path.exists() {
        return (false, format!("{} already present", path.display()));
    }
    let body = "\
# Rices Preferences

Cross-cutting preferences that apply across rices, not any one rice's
`design/` folder -- the sibling of `learnings.md`. Sparse today; the agent
reads this before every ricing iteration and appends after declare/reject
decisions, the same write-back discipline `learnings.md` already holds.
";
    match std::fs::write(&path, body) {
        Ok(()) => (true, format!("seeded {}", path.display())),
        Err(e) => (false, format!("could not seed {}: {e}", path.display())),
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

/// ONBOARD.md decision 3: "strillix enabled" = `rice_bin()` resolves. The env
/// tier is trusted unconditionally (`rice_bin`'s own contract) and the
/// sibling tier is only ever RETURNED after `rice_bin` already ran its own
/// `exists()` check -- both show up as a path containing `/`. Only the bare
/// name (`"strillix"`, tier 3, deliberately left by `rice_bin` for
/// `Command::spawn` to resolve at exec time) needs a PATH probe of our own.
fn strillix_bin_if_resolved() -> Option<String> {
    let bin = aoide_protocol::bin::rice_bin();
    let resolved = bin.contains('/') || aoide_protocol::bin::on_path(&bin);
    resolved.then_some(bin)
}

/// Steps 2-3 of the flow (ONBOARD.md): probe for strillix, and when it
/// resolves, delegate the whole nix half to `strillix onboard` as a child
/// process with inherited stdio so its own prompts/output reach this same
/// terminal directly (`--harness` is deliberately NOT forwarded -- harness
/// wiring is core's own job, decision 7, and strillix's half has no use for it).
/// A spawn failure (e.g. `ENOENT`) OR a nonzero exit both fall to the same
/// tolerant note rather than a hard onboard failure -- either could mean the
/// resolved strillix binary predates this aoide and genuinely lacks `onboard`,
/// OR a real failure inside `strillix onboard` itself; the note names the exit
/// code (or spawn error) and states plainly that the desktop half was
/// skipped, without guessing which case it was. If strillix isn't found at all,
/// the returned line names no nix-shaped detail whatsoever (ONBOARD.md
/// decision 3's "nothing nix-shaped is ever spoken").
fn probe_and_delegate_strillix(inv: &Invocation) -> String {
    let Some(bin) = strillix_bin_if_resolved() else {
        return "strillix: not found (no STRILLIX_BIN, no sibling binary, not on PATH) -- skipping the desktop half; core setup is complete".to_string();
    };

    let mut cmd = Command::new(&bin);
    cmd.arg("onboard");
    if let Some(out) = inv.flags.get("out") {
        cmd.arg("--out").arg(out);
    }
    if inv.flag_present("yes") {
        cmd.arg("--yes");
    }
    cmd.stdin(Stdio::inherit()).stdout(Stdio::inherit()).stderr(Stdio::inherit());

    match cmd.status() {
        Ok(status) if status.success() => format!("strillix onboard: done ({bin})"),
        Ok(status) => format!(
            "strillix onboard failed (exited {}) -- the resolved strillix binary may be older than this aoide and lack the `onboard` subcommand, or the command itself failed; skipping the desktop half",
            status.code().map(|c| c.to_string()).unwrap_or_else(|| "via signal".to_string())
        ),
        Err(e) => format!(
            "strillix onboard could not be run ({e}) -- the resolved strillix binary may be older than this aoide and lack the `onboard` subcommand, or spawning it failed; skipping the desktop half"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_test_support::{env_lock, unique_tmp, EnvSaver};

    // ── register_clone: the ~/rice symlink's never-clobber contract ─────

    #[test]
    fn register_clone_links_rice_when_absent_and_is_idempotent_on_rerun() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvSaver::capture(&["HOME", "AOIDE_STAGE_DIR"]);
        let root = unique_tmp("onboard-clone-fresh");
        std::fs::create_dir_all(root.join("rice")).unwrap();
        let home = unique_tmp("onboard-clone-fresh-home");
        std::env::set_var("HOME", &home);
        std::env::set_var("AOIDE_STAGE_DIR", home.join("Aoide/rice/stage"));
        let home_rice = home.join("rice");

        let (changed, notes) = register_clone(&root);
        let meta = std::fs::symlink_metadata(&home_rice).unwrap();
        assert!(meta.file_type().is_symlink(), "~/rice was not created as a symlink");
        assert_eq!(
            std::fs::canonicalize(&home_rice).unwrap(),
            std::fs::canonicalize(root.join("rice")).unwrap()
        );
        assert!(notes.iter().any(|n| n.contains("linked") && n.contains("rice")), "notes: {notes:?}");
        assert!(changed.iter().any(|c| c.starts_with("~/rice ->")), "changed: {changed:?}");

        // Re-run: no-op, reported present, never re-listed as changed.
        let (changed2, notes2) = register_clone(&root);
        assert!(notes2.iter().any(|n| n.contains("already links to")), "notes2: {notes2:?}");
        assert!(
            !changed2.iter().any(|c| c.starts_with("~/rice ->")),
            "a second run must not re-report the symlink as changed: {changed2:?}"
        );

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// GATED on Unix with its reason: the fixture creates a symlink to a second
    /// directory, and native Windows needs `SeCreateSymbolicLinkPrivilege`
    /// (developer mode or an elevated token) for that call at all —
    /// `aoide_storage::fs::link_dir` surfaces the refusal, so the conflict this
    /// test stages cannot be built there. No native twin.
    #[cfg(unix)]
    #[test]
    fn register_clone_leaves_a_wrong_target_symlink_alone_with_a_note() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvSaver::capture(&["HOME", "AOIDE_STAGE_DIR"]);
        let root = unique_tmp("onboard-clone-wrong");
        std::fs::create_dir_all(root.join("rice")).unwrap();
        let home = unique_tmp("onboard-clone-wrong-home");
        std::env::set_var("HOME", &home);
        std::env::set_var("AOIDE_STAGE_DIR", home.join("Aoide/rice/stage"));
        let home_rice = home.join("rice");
        let elsewhere = unique_tmp("onboard-clone-wrong-elsewhere");
        std::os::unix::fs::symlink(&elsewhere, &home_rice).unwrap();

        let (changed, notes) = register_clone(&root);
        assert!(
            notes.iter().any(|n| n.contains("is already a symlink to") && n.contains(&elsewhere.display().to_string())),
            "notes: {notes:?}"
        );
        assert!(!changed.iter().any(|c| c.starts_with("~/rice ->")));
        assert_eq!(std::fs::read_link(&home_rice).unwrap(), elsewhere, "the wrong link must survive untouched");

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&elsewhere);
    }

    #[test]
    fn register_clone_leaves_a_regular_file_alone_with_a_note() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvSaver::capture(&["HOME", "AOIDE_STAGE_DIR"]);
        let root = unique_tmp("onboard-clone-file");
        std::fs::create_dir_all(root.join("rice")).unwrap();
        let home = unique_tmp("onboard-clone-file-home");
        std::env::set_var("HOME", &home);
        std::env::set_var("AOIDE_STAGE_DIR", home.join("Aoide/rice/stage"));
        let home_rice = home.join("rice");
        std::fs::write(&home_rice, "not a symlink").unwrap();

        let (changed, notes) = register_clone(&root);
        assert!(
            notes.iter().any(|n| n.contains("already exists and is not a symlink")),
            "notes: {notes:?}"
        );
        assert!(!changed.iter().any(|c| c.starts_with("~/rice ->")));
        assert_eq!(std::fs::read_to_string(&home_rice).unwrap(), "not a symlink", "the file must survive untouched");

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// GATED on Unix with its reason: the fixture creates a symlink (and
    /// deliberately a DANGLING one), which native Windows refuses without
    /// `SeCreateSymbolicLinkPrivilege` — see
    /// `register_clone_leaves_a_wrong_target_symlink_alone_with_a_note`'s own
    /// gate. No native twin.
    #[cfg(unix)]
    #[test]
    fn register_clone_reports_a_dangling_but_correctly_targeted_link_as_already_linked() {
        // The cosmetic fix: a symlink aimed at the RIGHT path whose target
        // doesn't exist yet must not be misreported as "elsewhere" just
        // because `canonicalize` can't resolve a dangling target.
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvSaver::capture(&["HOME", "AOIDE_STAGE_DIR"]);
        let root = unique_tmp("onboard-clone-dangling");
        // Deliberately no `root/rice` directory yet -- the link below points
        // at a path that does not exist on disk.
        let home = unique_tmp("onboard-clone-dangling-home");
        std::env::set_var("HOME", &home);
        std::env::set_var("AOIDE_STAGE_DIR", home.join("Aoide/rice/stage"));
        let home_rice = home.join("rice");
        std::os::unix::fs::symlink(root.join("rice"), &home_rice).unwrap();

        let (_changed, notes) = register_clone(&root);
        assert!(notes.iter().any(|n| n.contains("already links to")), "notes: {notes:?}");

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&home);
    }

    // ── register_clone: registers locally, no daemon required ────────────

    #[test]
    fn register_clone_registers_the_project_with_no_daemon_reachable() {
        // The 18e5dd5 regression this pins: `project add` became
        // daemon-owned (`manage.rs`'s `local_daemon`), but onboarding runs
        // before any daemon exists -- the documented first-run path is
        // `git clone … && cd ~/Aoide && aoide onboard`, no daemon step.
        // `register_clone` must land the clone in `projects.json` itself,
        // even with `AOIDE_DAEMON_SOCKET` pointed at a dead path under a
        // stage nothing else touches -- never a silent no-op because
        // nothing answered.
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvSaver::capture(&["HOME", "AOIDE_STAGE_DIR", "AOIDE_DAEMON_SOCKET"]);
        let root = unique_tmp("onboard-register-clone-no-daemon-root");
        let home = unique_tmp("onboard-register-clone-no-daemon-home");
        let stage = home.join("Aoide/rice/stage");
        std::env::set_var("HOME", &home);
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("AOIDE_DAEMON_SOCKET", stage.join("no-such-daemon.sock"));

        let (_changed, notes) = register_clone(&root);
        assert!(
            !notes.iter().any(|n| n.starts_with(PROJECT_FAILED_PREFIX)),
            "notes: {notes:?}"
        );

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

    // ── seed_rices: create-once, never overwrite ──────────────────────

    #[test]
    fn seed_rices_creates_preferences_once_then_leaves_it_untouched() {
        let root = unique_tmp("onboard-seed");
        std::fs::create_dir_all(root.join("rices")).unwrap();
        let path = root.join("rices/preferences.md");

        let (seeded, note) = seed_rices(&root);
        assert!(seeded);
        assert!(note.contains("seeded"), "note: {note}");
        let first = std::fs::read_to_string(&path).unwrap();
        assert!(first.contains("Rices Preferences"));

        let (seeded2, note2) = seed_rices(&root);
        assert!(!seeded2);
        assert!(note2.contains("already present"), "note2: {note2}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), first, "a second run must never overwrite");

        let _ = std::fs::remove_dir_all(&root);
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
}
