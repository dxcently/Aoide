//! `strillix component <slot> <verb> [arg]` — the one door a cross-component call
//! goes through. The shellbridge's `component` command execs it, and a
//! terminal can run it directly.
//!
//! One ordered rule against `run/qml/components.json` (CONTRACTS.md §4). It
//! never reads the catalogue, and it never reaches into another component:
//!
//! ```text
//! components.json[<slot>]
//!   absent ............................ refused: no provider fills <slot>
//!   provider == "strillix" .............. quickshell ipc call component act <slot> <verb> <arg>
//!   actions.<verb> is an argv ......... spawn that argv with <arg> appended, no shell, detached
//!   otherwise ......................... refused: provider <p> declares no <verb>
//! ```
//!
//! The shell answers `act` with `true` when the slot's live item ran the verb,
//! and `false` when the item is not loaded or does not expose the verb. A
//! void answer would look the same whether the verb ran or not, so the reply
//! is a value.

use crate::commands::lock::{ipc_argv, run_ipc};
use crate::dispatch::Invocation;
use crate::output::{Fix, Kind, Outcome, Refusal};
use crate::registry::{arg, cmd, Registry};
use aoide_protocol::Door;
use serde_json::{json, Value};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The provider the shell itself answers: its own QML item, not an argv.
const SHELL_PROVIDER: &str = "strillix";

/// How long the shell has to answer one `act`, the lock command's budget.
const SHELL_ANSWER: Duration = Duration::from_secs(5);

/// How a `(slot, verb)` pair is answered, once [`resolve`] has picked it.
#[derive(Debug, PartialEq, Eq)]
pub enum Route {
    /// The shell's own `component` IPC target runs the verb.
    Shell,
    /// The provider's argv for this verb, run with the arg appended.
    Argv(Vec<String>),
}

pub fn register(r: &mut Registry) {
    r.insert(cmd!(
        path: ["component"],
        summary: "Ask the component that fills a shell slot to do one of its verbs. The provider is read from run/qml/components.json: the strillix shell answers through its own IPC target, and any other provider runs the argv it declares for the verb. A slot no provider fills, or a verb the provider does not declare, is refused with the fix.",
        args: [
            arg!("slot", "string", true, "The component slot: bar, launcher, powermenu, notifications, wallpaper, dock, calendar, switcher, lock, greeter, or a consumer's own."),
            arg!("verb", "string", true, "What the provider answers: toggle, show, hide, openTab or openPictures for strillix; any verb a non-strillix provider declares in components.json."),
            arg!("arg", "string", false, "The verb's one argument, when it takes one: a screen name, a power action, a tab."),
        ],
        flags: [],
        gated: false,
        implemented: true,
        handler: handle_component,
        examples: ["component powermenu toggle", "component calendar toggle DP-1", "component dock openTab emblems"],
        brief: "Ask the component that fills a slot to act.",
        doors: &[Door::Cli],
    ));
}

fn handle_component(inv: &Invocation) -> Outcome {
    let cmd = "component";
    let slot = inv.args[0].as_str();
    let verb = inv.args[1].as_str();
    let arg = inv.args.get(2).map(String::as_str).filter(|a| !a.is_empty());
    let shell_qml = strillix_rice::paths::run_qml_dir().join("shell.qml");
    match resolve(read_components().as_ref(), slot, verb) {
        Ok(Route::Shell) => shell_act(cmd, &shell_qml, slot, verb, arg),
        Ok(Route::Argv(argv)) => spawn_argv(cmd, &argv, arg),
        Err(refusal) => refusal.into_outcome(cmd),
    }
}

/// `run/qml/components.json`, or `None` when it is missing or unreadable.
fn read_components() -> Option<Value> {
    let text = std::fs::read_to_string(strillix_rice::paths::run_qml_dir().join("components.json")).ok()?;
    serde_json::from_str(&text).ok()
}

/// Pick the route for `(slot, verb)` from the published components. Pure:
/// the one place the resolution order above is written down.
pub fn resolve(components: Option<&Value>, slot: &str, verb: &str) -> Result<Route, Refusal> {
    let Some(entry) = components.and_then(|c| c.get(slot)) else {
        let why = match components {
            Some(_) => format!("the host did not select the {slot} component"),
            None => "run/qml/components.json is missing or unreadable".to_string(),
        };
        return Err(Refusal::new(
            Kind::Refused,
            format!("no provider fills {slot}"),
            why,
            Fix::Set(format!("habit.dendrites.{slot} = {{ enable = true; provider = \"strillix\"; }}")),
        ));
    };
    let provider = entry.get("provider").and_then(Value::as_str).unwrap_or("");
    if provider == SHELL_PROVIDER {
        return Ok(Route::Shell);
    }
    let argv = entry
        .get("actions")
        .and_then(|a| a.get(verb))
        .and_then(Value::as_array)
        .and_then(|a| a.iter().map(Value::as_str).map(|s| s.map(str::to_string)).collect::<Option<Vec<_>>>())
        .filter(|a| !a.is_empty());
    argv.map(Route::Argv).ok_or_else(|| {
        Refusal::new(
            Kind::Refused,
            format!("provider {provider} declares no {verb}"),
            format!("{slot} is filled by {provider}, and its actions name no argv for {verb}"),
            Fix::Set(format!("add actions.{verb} to the {slot} provider")),
        )
    })
}

/// The shell's `component.act` call as an argv, the arg `""` when absent.
pub fn shell_argv(shell_qml: &Path, slot: &str, verb: &str, arg: Option<&str>) -> Vec<String> {
    ipc_argv(shell_qml, "component", &["act", slot, verb, arg.unwrap_or("")])
}

fn shell_act(cmd: &str, shell_qml: &Path, slot: &str, verb: &str, arg: Option<&str>) -> Outcome {
    let fix = Fix::Run("systemctl --user status aoide-quickshell.service".to_string());
    match run_ipc(&shell_argv(shell_qml, slot, verb, arg), Instant::now() + SHELL_ANSWER) {
        Ok(answer) if answer == "true" => {
            Outcome::ok(cmd, format!("the shell ran {verb} on {slot}")).with_data(json!({ "route": "shell", "slot": slot, "verb": verb }))
        }
        Ok(answer) if answer == "false" => Outcome::refuse(
            cmd,
            Kind::Refused,
            format!("the shell has no live {slot} to {verb}"),
            format!("the {slot} item is not loaded, or it exposes no {verb}"),
            fix,
        )
        .with_fields(json!({ "reason": "item-not-loaded" })),
        Ok(answer) => Outcome::refuse(
            cmd,
            Kind::Failed,
            format!("the shell did not take {verb} on {slot}"),
            answer,
            fix,
        ),
        Err(why) => Outcome::refuse(cmd, Kind::Failed, format!("the shell did not take {verb} on {slot}"), why, fix),
    }
}

/// Start the provider's argv detached: no shell, and no wait, since a
/// provider's own window outlives this command.
fn spawn_argv(cmd: &str, argv: &[String], arg: Option<&str>) -> Outcome {
    let started = Command::new(&argv[0])
        .args(&argv[1..])
        .args(arg)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    match started {
        Ok(child) => Outcome::ok(cmd, format!("started {}", argv[0])).with_data(json!({ "route": "argv", "pid": child.id() })),
        Err(e) => Outcome::refuse(
            cmd,
            Kind::Failed,
            format!("could not start {}", argv[0]),
            e.to_string(),
            Fix::Run(argv[0].clone()),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn components() -> Value {
        json!({
            "powermenu": { "provider": "strillix", "actions": {} },
            "launcher": { "provider": "rofi", "actions": { "open": ["rofi", "-show", "drun"] } },
            "dock": { "provider": "waybar", "actions": { "toggle": [] } }
        })
    }

    #[test]
    fn a_strillix_slot_resolves_to_the_shell_route() {
        assert_eq!(resolve(Some(&components()), "powermenu", "toggle"), Ok(Route::Shell));
    }

    #[test]
    fn a_strillix_slot_builds_the_exact_ipc_argv() {
        let argv = shell_argv(Path::new("/run/qml/shell.qml"), "powermenu", "show", Some("lock"));
        assert_eq!(
            argv,
            ["quickshell", "-p", "/run/qml/shell.qml", "ipc", "call", "component", "act", "powermenu", "show", "lock"]
        );
        let bare = shell_argv(Path::new("/run/qml/shell.qml"), "dock", "toggle", None);
        assert_eq!(bare.last().map(String::as_str), Some(""), "an absent arg is the empty string");
    }

    #[test]
    fn a_declared_argv_resolves_to_its_argv() {
        assert_eq!(
            resolve(Some(&components()), "launcher", "open"),
            Ok(Route::Argv(vec!["rofi".into(), "-show".into(), "drun".into()]))
        );
    }

    #[test]
    fn an_absent_slot_is_a_taught_refusal_with_exit_one() {
        let refusal = resolve(Some(&components()), "lock", "lock").unwrap_err();
        assert_eq!(refusal.what, "no provider fills lock");
        assert_eq!(refusal.why, "the host did not select the lock component");
        assert_eq!(refusal.into_outcome("component").status.exit_code(), 1);
    }

    #[test]
    fn a_missing_components_file_refuses_as_unreadable() {
        let refusal = resolve(None, "bar", "toggle").unwrap_err();
        assert_eq!(refusal.what, "no provider fills bar");
        assert!(refusal.why.contains("missing or unreadable"), "{}", refusal.why);
    }

    #[test]
    fn a_verb_the_provider_does_not_declare_is_refused() {
        let refusal = resolve(Some(&components()), "dock", "openTab").unwrap_err();
        assert_eq!(refusal.what, "provider waybar declares no openTab");
        assert!(refusal.why.contains("no argv for openTab"), "{}", refusal.why);
        let empty = resolve(Some(&components()), "dock", "toggle").unwrap_err();
        assert_eq!(empty.what, "provider waybar declares no toggle", "an empty argv is no argv");
    }

    /// A stub provider on PATH records that it ran: the argv is spawned by
    /// name, the arg is appended, and no shell is in between.
    #[test]
    fn a_declared_argv_runs_detached_with_the_arg_appended() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _saved = aoide_test_support::EnvSaver::capture(&["PATH"]);
        let dir = aoide_test_support::unique_tmp("component-argv");
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let ran = dir.join("stub.args");
        let shim = bin.join("aoide-component-stub");
        std::fs::write(&shim, format!("#!/bin/sh\nprintf '%s ' \"$@\" > '{}'\n", ran.display())).unwrap();
        std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("PATH", &bin);

        let argv = vec!["aoide-component-stub".to_string(), "--open".to_string()];
        let out = spawn_argv("component", &argv, Some("DP-1"));
        assert_eq!(out.status.exit_code(), 0, "{}", out.message);

        let deadline = Instant::now() + Duration::from_secs(5);
        while !ran.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(std::fs::read_to_string(&ran).unwrap_or_default(), "--open DP-1 ");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_provider_binary_is_a_failure_that_names_it() {
        let argv = vec!["aoide-no-such-provider-binary".to_string()];
        let out = spawn_argv("component", &argv, None);
        assert_eq!(out.status.exit_code(), 1);
        assert!(out.message.contains("could not start aoide-no-such-provider-binary"), "{}", out.message);
    }
}
