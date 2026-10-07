//! `do` — one sentence in, the one `aoide` command it means out, printed and
//! never run (docs/architecture/AOIDE-VV-JEV.md, "VV — `aoide do`").
//!
//! A classifier selects from a closed set and writes a row; it never acts. The
//! chain is: [`client`] shells out to `verba-volantia dispatch` and returns its
//! verdict, [`decide`] applies the fail-closed table to it, and the bound
//! command is checked by the registry's own `Command::check` before it is
//! printed, so what comes out is a line that parses. The closed set and its
//! slot names are [`kit`]'s, read off the same registry, so the intent the
//! classifier answers with and the command it names are one table read both
//! ways.
//!
//! Root-coupled like `meta` and `onboard`: it needs the fully assembled
//! registry, which no domain crate can reach. `do kit` is the registry-to-
//! training-spec half of the same table; it ships as a subcommand so it gets
//! `--help`, examples and the registry walks like every other command.

mod client;
pub mod kit;

use crate::dispatch::Invocation;
use crate::output::{io_cause, Fix, Kind, Outcome, Refusal};
use crate::registry::{arg, cmd, flag, Command, Registry};
use client::Kit;
use kit::Slot;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::Path;

const COMMAND: &str = "do";
const SHOWN: usize = 3;
const GEN: &str = "verba-volantia gen --spec templates.json --data data/aoide";
const TRAIN: &str =
    "verba-volantia train --data data/aoide --out weights/aoide --seed 1 --epochs 60 --batch 256 --bucket-window 16 --smooth 0.1";

pub fn register(r: &mut Registry) {
    r.insert(cmd!(
        path: ["do"],
        summary: "Turn a sentence into the one aoide command it means. The command is printed, copy-pasteable and checked against the registry, and never run: a classifier suggests, you decide. It abstains with the nearest commands when it is unsure. Needs the verba-volantia binary and a trained kit (`aoide do kit --help`).",
        args: [arg!("utterance", "string", true, "What you want done, in your own words; several words need no quotes.")],
        flags: [],
        gated: false,
        implemented: true,
        handler: handle_do,
        examples: ["do show me the trace for session abc123", "do \"send that session a follow-up\" --json"],
        brief: "Turn a sentence into the one aoide command it means; prints, never runs."
    ));
    r.insert(cmd!(
        path: ["do", "kit"],
        summary: "Emit the classifier's training spec (verba-volantia's templates.json) from this binary's own registry: one intent per command, one slot per argument or value flag, a few phrasings each. Train it on a GPU box with `verba-volantia gen --spec templates.json --data data/aoide`, then `verba-volantia train --data data/aoide --out weights/aoide --seed 1 --epochs 60 --batch 256 --bucket-window 16 --smooth 0.1`, then copy weights/aoide into the kit directory (`[verba] weightsDir`, default $AOIDE_ROOT/verba/aoide).",
        args: [],
        flags: [flag!("out", "string", "Write the spec to this file instead of printing it, and print the commands that train it.", value: "file")],
        gated: false,
        implemented: true,
        handler: handle_kit,
        examples: ["do kit", "do kit --out templates.json"],
        brief: "Emit the classifier's training spec from the registry."
    ));
}

fn refuse(r: Refusal) -> Outcome {
    r.into_outcome(COMMAND)
}

fn handle_do(inv: &Invocation) -> Outcome {
    let utterance = inv.args.join(" ").split_whitespace().collect::<Vec<_>>().join(" ");
    let verdict = match Kit::configured().and_then(|kit| client::classify(&kit, &utterance)) {
        Ok(v) => v,
        Err(r) => return refuse(r),
    };
    decide(crate::dispatch::registry(), &utterance, &verdict).unwrap_or_else(|o| o)
}

/// The fail-closed table: a verdict becomes a command only when nothing in it
/// argues against dispatch. `Ok` is the command; `Err` is the taught refusal.
fn decide(r: &Registry, utterance: &str, verdict: &Value) -> Result<Outcome, Outcome> {
    let intent = verdict["intent"].as_str().unwrap_or_default();
    let margin = verdict["margin"].as_f64().unwrap_or_default();
    let said = |what: &str, why: String, fix: Fix| {
        Outcome::refuse(COMMAND, Kind::Refused, format!("`{utterance}` {what}"), why, fix)
            .with_fields(json!({ "candidates": candidates(r, verdict), "verdict": verdict }))
    };
    let rephrase = Fix::None("pick a candidate above, or say it with more of the command's own words");

    if verdict["accept"].as_bool() != Some(true) {
        let why = match verdict["threshold"].as_f64() {
            Some(t) => format!("the classifier's margin {margin:.2} is under the {t:.2} this kit was calibrated to, so it abstains"),
            None => "this kit carries no calibrated threshold, so the classifier abstains on everything".to_string(),
        };
        return Err(said("did not resolve to one command", why, rephrase));
    }
    let conflicts: Vec<&str> = verdict["conflicts"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    if !conflicts.is_empty() {
        let why = format!("the classifier claimed {} twice in one sentence, so it contradicts itself", conflicts.join(", "));
        return Err(said("did not resolve to one command", why, rephrase));
    }
    if let Some(rest) = verdict["trailing_editorial_text"].as_str() {
        let why = format!("part of the sentence (`{rest}`) is addressed to nothing, so the intent is not trustworthy");
        return Err(said("did not resolve to one command", why, rephrase));
    }
    if intent == kit::NONE {
        let why = "the classifier is confident this is not one of the commands aoide has".to_string();
        return Err(said("is not something aoide does", why, Fix::Run("aoide guide".into())));
    }
    let Some(c) = kit::resolve(r, intent) else {
        return Err(Outcome::refuse(
            COMMAND,
            Kind::Failed,
            format!("the kit answered `{intent}`, which is not an aoide command"),
            "the kit was trained from a different command set than this binary registers",
            Fix::Run("aoide do kit --help".into()),
        )
        .with_fields(json!({ "verdict": verdict })));
    };
    let slots = verdict["slots"].as_object().cloned().unwrap_or_default();
    let bound = bind(c, &slots).map_err(|why| {
        Outcome::refuse(
            COMMAND,
            Kind::Failed,
            format!("`aoide {}` was understood but its values cannot be placed", c.path.join(" ")),
            why,
            Fix::Run("aoide do kit --help".into()),
        )
        .with_fields(json!({ "verdict": verdict }))
    })?;
    let mut probe = bound.clone();
    if let Err(r) = c.check("aoide", &mut probe) {
        let fix = keep_given(r.fix, &bound);
        return Err(Outcome::refuse(
            COMMAND,
            Kind::Refused,
            format!("`{utterance}` means `aoide {}`, but not all of it was said", c.path.join(" ")),
            format!("{}: {}", r.what, r.why),
            fix.clone(),
        )
        .with_fields(json!({ "intent": intent, "slots": slots, "command": run_line(&fix), "verdict": verdict })));
    }
    let line = bound.command_line("aoide", &[], false);
    if let Err(why) = reads_back(r, &line, &probe) {
        return Err(Outcome::refuse(
            COMMAND,
            Kind::Refused,
            format!("`{utterance}` means `aoide {}`, but its values would not read back as said", c.path.join(" ")),
            why,
            Fix::None("say the values without leading dashes, or type the command yourself"),
        )
        .with_fields(json!({ "intent": intent, "slots": slots, "verdict": verdict })));
    }
    Ok(Outcome::ok(COMMAND, line.clone()).with_data(json!({
        "command": line,
        "intent": intent,
        "slots": slots,
        "margin": margin,
        "threshold": verdict["threshold"],
        "verdict": verdict,
    })))
}

/// The registry's fix line shows what is still missing; the flags the
/// classifier did fill belong on it too, or the line forgets what was said.
fn keep_given(fix: Fix, bound: &Invocation) -> Fix {
    let Fix::Run(mut line) = fix else { return fix };
    let plain = |v: &str| !v.starts_with('-') && aoide_protocol::invocation::shell_word(v) == v;
    if !bound.args.iter().chain(bound.flags.values()).all(|v| plain(v)) {
        return Fix::None("say the values plainly (no leading dash, spaces or shell characters), or type the command yourself");
    }
    for (name, value) in bound.flags.iter().filter(|(name, _)| name.as_str() != "json") {
        if !line.split_whitespace().any(|w| w.strip_prefix("--") == Some(name.as_str())) {
            line.push_str(&format!(" --{name} {}", aoide_protocol::invocation::shell_word(value)));
        }
    }
    Fix::Run(line)
}

/// The printed line, typed at a shell and parsed by the door, must be exactly
/// the invocation the registry checked: a value that reads as a flag (`--yes`)
/// would otherwise ride into a copy-pasted line the registry never saw.
fn reads_back(r: &Registry, line: &str, checked: &Invocation) -> Result<(), String> {
    let argv = shell_words(line).ok_or("the line is not one shell command")?;
    let (typed, _) = aoide_protocol::door::parse(&argv[1..], aoide_protocol::Door::Cli, "aoide", r)
        .map_err(|_| "typed at a shell, it is read as a different command".to_string())?;
    if typed.path == checked.path && typed.args == checked.args && typed.flags == checked.flags {
        Ok(())
    } else {
        Err("typed at a shell, its arguments and flags are read differently than they were bound".into())
    }
}

/// POSIX words of a line `Invocation::command_line` wrote: bare words and
/// single-quoted runs (`'\''` for a quote).
fn shell_words(line: &str) -> Option<Vec<String>> {
    let (mut words, mut cur, mut started, mut quoted) = (Vec::new(), String::new(), false, false);
    let mut chars = line.chars();
    while let Some(ch) = chars.next() {
        match (quoted, ch) {
            (true, '\'') => quoted = false,
            (true, c) => cur.push(c),
            (false, '\'') => {
                quoted = true;
                started = true;
            }
            (false, '\\') => cur.push(chars.next().filter(|c| *c == '\'')?),
            (false, c) if c.is_whitespace() => {
                if started || !cur.is_empty() {
                    words.push(std::mem::take(&mut cur));
                    started = false;
                }
            }
            (false, c) => cur.push(c),
        }
    }
    if quoted {
        return None;
    }
    if started || !cur.is_empty() {
        words.push(cur);
    }
    Some(words)
}

fn run_line(fix: &Fix) -> Value {
    match fix {
        Fix::Run(line) => json!(line),
        _ => Value::Null,
    }
}

/// The classifier's slots as the command's own arguments and flags, in the
/// order the command takes them. A positional given after one left blank has
/// nowhere to go, and is refused rather than moved.
fn bind(c: &Command, slots: &Map<String, Value>) -> Result<Invocation, String> {
    let table = kit::slots(c);
    let mut args: Vec<Option<String>> = vec![None; c.args.len()];
    let mut flags = BTreeMap::new();
    for (param, value) in slots {
        let Some(text) = value.as_str() else {
            return Err(format!("`{param}` came back as {value}, and every slot is one piece of text"));
        };
        let Some((_, slot)) = table.iter().find(|(id, _)| id == param) else {
            return Err(format!("`{param}` is not a slot of this command"));
        };
        if text.chars().any(char::is_control) {
            return Err(format!("`{param}` holds a control character or a line break, which no command takes"));
        }
        match (slot, text.trim()) {
            (_, "") => {}
            (Slot::Arg(i), text) => args[*i] = Some(text.to_string()),
            (Slot::Flag(name), text) => {
                flags.insert(name.to_string(), text.to_string());
            }
        }
    }
    let said = args.iter().take_while(|a| a.is_some()).count();
    if args[said..].iter().any(Option::is_some) {
        let later = args.iter().rposition(Option::is_some).unwrap_or(said);
        return Err(format!("it filled <{}> but left <{}>, which comes first, empty", c.args[later].name, c.args[said].name));
    }
    Ok(Invocation {
        path: c.path.iter().map(|s| s.to_string()).collect(),
        args: args.into_iter().flatten().collect(),
        flags,
        door: aoide_protocol::Door::Cli,
    })
}

/// The runner-up commands as full lines, best first, each with its score. Only
/// the slots the candidate also has are carried over; the rest show as the
/// placeholder the registry's own fix would show.
fn candidates(r: &Registry, verdict: &Value) -> Vec<String> {
    let slots = verdict["slots"].as_object().cloned().unwrap_or_default();
    let ranked = verdict["candidates"].as_array().into_iter().flatten();
    ranked
        .filter_map(|cand| Some((kit::resolve(r, cand["intent"].as_str()?)?, cand["score"].as_f64().unwrap_or_default())))
        .take(SHOWN)
        .map(|(c, score)| {
            let own: Map<String, Value> = slots.iter().filter(|(k, _)| kit::slots(c).iter().any(|(id, _)| id == *k)).map(|(k, v)| (k.clone(), v.clone())).collect();
            let line = match bind(c, &own) {
                Ok(inv) => {
                    let mut probe = inv.clone();
                    match c.check("aoide", &mut probe) {
                        Ok(()) => inv.command_line("aoide", &[], false),
                        Err(refusal) => match keep_given(refusal.fix, &inv) {
                            Fix::Run(line) => line,
                            _ => c.synopsis("aoide", false),
                        },
                    }
                }
                Err(_) => c.synopsis("aoide", false),
            };
            format!("{score:.2}  {line}")
        })
        .collect()
}

fn handle_kit(inv: &Invocation) -> Outcome {
    let spec = kit::spec(crate::dispatch::registry());
    let (intents, templates) = kit::counts(&spec);
    let text = serde_json::to_string_pretty(&spec).unwrap_or_default();
    let command = "do.kit";
    let Some(out) = inv.flags.get("out").map(|o| o.trim()).filter(|o| !o.is_empty()) else {
        let message = if inv.flag_present("json") { format!("{intents} intents, {templates} templates") } else { text };
        return Outcome::ok(command, message).with_data(json!({ "spec": spec, "intents": intents, "templates": templates }));
    };
    if let Err(e) = std::fs::write(out, format!("{text}\n")) {
        let cause = io_cause("write", Path::new(out), &e);
        return Outcome::refuse(command, Kind::Failed, format!("the spec was not written to `{out}`"), cause.why, Fix::Run("aoide do kit --out templates.json".into()))
            .with_detail(cause.detail);
    }
    let dir = Kit::configured().map(|k| k.dir.display().to_string()).unwrap_or_else(|_| "$AOIDE_ROOT/verba/aoide".into());
    let steps = [GEN.to_string(), TRAIN.to_string(), format!("mkdir -p {dir} && cp -r weights/aoide/. {dir}")];
    let message = format!("wrote {intents} intents and {templates} templates to {out}\nthen, where the GPU is:\n  {}", steps.join("\n  "));
    Outcome::ok(command, message).with_data(json!({ "path": out, "intents": intents, "templates": templates, "steps": steps }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_test_support::{env_lock, unique_tmp, EnvSaver};

    fn registry() -> &'static Registry {
        crate::dispatch::registry()
    }

    fn verdict(intent: &str, slots: Value, accept: bool, margin: f64) -> Value {
        json!({
            "utterance": "u", "delex": "u", "intent": intent, "intent_prob": 0.9, "margin": margin, "threshold": 0.64,
            "accept": accept, "slots": slots, "conflicts": [], "trailing_editorial_text": null,
            "candidates": [{"intent": intent, "score": 0.8}, {"intent": "session_watch", "score": 0.1}, {"intent": "none", "score": 0.05}],
        })
    }

    fn rendered(o: &Outcome) -> String {
        o.render(false).0
    }

    // ── the decision table, on verdicts alone ───────────────────────────────

    #[test]
    fn an_accepted_verdict_prints_the_bound_command_and_it_parses() {
        let v = verdict("session_trace", json!({ "id": "abc 123" }), true, 0.79);
        let o = decide(registry(), "trace it", &v).unwrap();
        assert_eq!(o.message, "aoide session trace 'abc 123'");
        assert_eq!(o.data.as_ref().unwrap()["intent"], "session_trace");
        let argv: Vec<String> = vec!["session".into(), "trace".into(), "abc 123".into()];
        let (inv, _) = aoide_protocol::door::parse(&argv, aoide_protocol::Door::Cli, "aoide", registry()).unwrap();
        assert_eq!(inv.path, ["session", "trace"]);
    }

    #[test]
    fn a_value_flag_slot_lands_as_a_flag_and_a_default_is_not_printed() {
        let v = verdict("send", json!({ "id": "s1", "text": "hello there" }), true, 0.8);
        let o = decide(registry(), "tell s1 hello there", &v).unwrap();
        assert_eq!(o.message, "aoide send 'hello there' --id s1", "{}", o.message);
    }

    #[test]
    fn a_value_that_would_read_as_a_flag_is_refused_not_printed() {
        let o = decide(registry(), "u", &verdict("session_trace", json!({ "id": "abc", "tail": "--follow" }), true, 0.8)).unwrap_err();
        assert_eq!(o.status.exit_code(), 1);
        let text = rendered(&o);
        assert!(text.contains("would not read back as said"), "{text}");
        assert!(!text.contains("fix: aoide"), "no runnable line is offered: {text}");
        let v = verdict("secrets_status", json!({ "name": "--yes" }), true, 0.8);
        assert!(decide(registry(), "u", &v).is_err());
        let v = verdict("session_trace", json!({ "id": "--yes" }), true, 0.8);
        assert!(decide(registry(), "u", &v).unwrap().message.contains("-- --yes"), "a positional goes after `--` and stays one value");
    }

    #[test]
    fn a_fix_line_never_carries_a_value_that_would_read_as_a_flag() {
        let v = verdict("session_trace", json!({ "tail": "--follow" }), true, 0.8);
        let o = decide(registry(), "u", &v).unwrap_err();
        let text = rendered(&o);
        assert!(text.contains("fix: none: say the values plainly") && !text.contains("--follow"), "{text}");
        assert_eq!(o.data.unwrap()["command"], Value::Null);
        let v = verdict("session_trace", json!({ "tail": "--follow" }), false, 0.1);
        assert!(!rendered(&decide(registry(), "u", &v).unwrap_err()).contains("--follow"), "candidates carry no such line either");
    }

    #[test]
    fn a_name_that_reads_as_a_flag_is_pinned_as_a_positional() {
        let v = verdict("node_pull", json!({ "name": "--yes" }), true, 0.8);
        assert_eq!(decide(registry(), "u", &v).unwrap().message, "aoide node pull -- --yes");
    }

    #[test]
    fn quoted_values_survive_the_round_trip() {
        let v = verdict("session_trace", json!({ "id": "it's a; $(x) `y`" }), true, 0.8);
        let o = decide(registry(), "u", &v).unwrap();
        assert_eq!(shell_words(&o.message).unwrap()[3], "it's a; $(x) `y`");
    }

    #[test]
    fn control_characters_and_line_breaks_in_a_value_are_refused() {
        for bad in ["a\u{1b}[31mb", "a\nb", "a\u{7}"] {
            let v = verdict("session_trace", json!({ "id": bad }), true, 0.8);
            assert!(rendered(&decide(registry(), "u", &v).unwrap_err()).contains("control character"), "{bad:?}");
        }
    }

    #[test]
    fn a_denied_intent_is_not_a_command_even_when_accepted() {
        let o = decide(registry(), "u", &verdict("secrets_put", json!({ "name": "db" }), true, 0.99)).unwrap_err();
        assert!(rendered(&o).contains("`secrets_put`, which is not an aoide command"));
    }

    #[test]
    fn a_missing_required_slot_prints_the_command_with_the_slot_named_and_refuses() {
        let o = decide(registry(), "show the trace", &verdict("session_trace", json!({}), true, 0.8)).unwrap_err();
        assert_eq!(o.status, crate::output::Status::Error, "a valid ask the world could not finish is exit 1");
        let text = rendered(&o);
        assert!(text.contains("means `aoide session trace`, but not all of it was said"), "{text}");
        assert!(text.contains("needs <id>"), "{text}");
        assert!(text.contains("fix: aoide session trace <id>"), "{text}");
        assert_eq!(o.data.unwrap()["command"], "aoide session trace <id>");
    }

    #[test]
    fn a_flag_that_was_said_stays_on_the_line_that_names_what_was_not() {
        let o = decide(registry(), "tail it", &verdict("session_trace", json!({ "tail": "20" }), true, 0.8)).unwrap_err();
        assert!(rendered(&o).contains("fix: aoide session trace <id> --tail 20"), "{}", rendered(&o));
        let v = verdict("session_trace", json!({ "tail": "20" }), false, 0.1);
        assert!(rendered(&decide(registry(), "u", &v).unwrap_err()).contains("0.80  aoide session trace <id> --tail 20"));
    }

    #[test]
    fn an_abstention_lists_the_nearest_commands_in_full_and_dispatches_nothing() {
        let v = verdict("session_trace", json!({ "id": "abc" }), false, 0.2);
        let o = decide(registry(), "do the thing with abc", &v).unwrap_err();
        assert_eq!(o.status.exit_code(), 1);
        let text = rendered(&o);
        assert!(text.contains("under the 0.64 this kit was calibrated to"), "{text}");
        assert!(text.contains("candidates:\n    0.80  aoide session trace abc\n    0.10  aoide session watch abc"), "{text}");
        assert!(!text.contains("aoide none"), "the `none` class is not a command to offer: {text}");
        assert!(o.data.unwrap()["verdict"]["intent"] == "session_trace");
    }

    #[test]
    fn a_self_contradicting_or_half_addressed_verdict_is_not_dispatched_even_when_accepted() {
        let mut v = verdict("session_trace", json!({ "id": "abc" }), true, 0.9);
        v["conflicts"] = json!(["id"]);
        assert!(rendered(&decide(registry(), "u", &v).unwrap_err()).contains("claimed id twice"));
        let mut v = verdict("session_trace", json!({ "id": "abc" }), true, 0.9);
        v["trailing_editorial_text"] = json!("and make it pretty");
        assert!(rendered(&decide(registry(), "u", &v).unwrap_err()).contains("`and make it pretty`) is addressed to nothing"));
    }

    #[test]
    fn a_null_accept_fails_closed_and_none_is_not_ours() {
        let mut v = verdict("session_trace", json!({ "id": "abc" }), true, 0.9);
        v["accept"] = Value::Null;
        v["threshold"] = Value::Null;
        assert!(rendered(&decide(registry(), "u", &v).unwrap_err()).contains("no calibrated threshold"));
        let o = decide(registry(), "order a pizza", &verdict("none", json!({}), true, 0.9)).unwrap_err();
        assert!(rendered(&o).contains("is not something aoide does"));
    }

    #[test]
    fn an_intent_or_slot_the_registry_does_not_know_is_a_kit_bug_never_a_command() {
        let o = decide(registry(), "u", &verdict("rice_compose", json!({}), true, 0.9)).unwrap_err();
        assert_eq!(o.status.exit_code(), 1);
        assert!(rendered(&o).contains("`rice_compose`, which is not an aoide command"));
        let o = decide(registry(), "u", &verdict("session_trace", json!({ "bogus": "x" }), true, 0.9)).unwrap_err();
        assert!(rendered(&o).contains("`bogus` is not a slot of this command"));
        let o = decide(registry(), "u", &verdict("session_trace", json!({ "id": ["a", "b"] }), true, 0.9)).unwrap_err();
        assert!(rendered(&o).contains("one piece of text"));
    }

    #[test]
    fn a_value_the_registry_refuses_comes_back_as_the_commands_own_fix() {
        let v = verdict("node_advertise", json!({ "state": "maybe" }), true, 0.9);
        let o = decide(registry(), "u", &v).unwrap_err();
        let text = rendered(&o);
        assert!(text.contains("does not accept `maybe` for <state>"), "{text}");
        assert!(text.contains("fix: aoide node advertise on"), "{text}");
    }

    #[cfg(unix)]
    mod shell_out {
        use super::*;
        use std::os::unix::fs::PermissionsExt;
        use std::path::PathBuf;

        // ── the shell-out, against a fake verba-volantia ────────────────────────

        struct Rig {
            _lock: std::sync::MutexGuard<'static, ()>,
            _env: EnvSaver,
            root: PathBuf,
        }

        /// An isolated root whose `config.toml` points `[verba]` at `binary` (and a
        /// kit that exists), with `body` as the fake's script.
        fn boxed(tag: &str, body: Option<&str>) -> Rig {
            let lock = env_lock().lock().unwrap_or_else(|e| e.into_inner());
            let env = EnvSaver::capture(&["AOIDE_ROOT", "AOIDE_CONFIG", "AOIDE_AUDIT_LOG"]);
            let root = unique_tmp(tag);
            std::env::set_var("AOIDE_ROOT", &root);
            std::env::remove_var("AOIDE_CONFIG");
            let kit = root.join("verba").join("aoide");
            std::fs::create_dir_all(&kit).unwrap();
            for f in ["meta.json", "model.safetensors"] {
                std::fs::write(kit.join(f), "{}").unwrap();
            }
            let binary = match body {
                Some(body) => {
                    let path = root.join("verba-volantia");
                    std::fs::write(&path, format!("#!/bin/sh\nread -r line\n{body}\n")).unwrap();
                    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
                    path.display().to_string()
                }
                None => root.join("absent").join("verba-volantia").display().to_string(),
            };
            std::fs::write(root.join("config.toml"), format!("[verba]\nbinary = \"{binary}\"\n")).unwrap();
            Rig { _lock: lock, _env: env, root }
        }

        fn say(utterance: &str) -> Outcome {
            let mut inv = aoide_test_support::inv(&["do"], &[utterance]);
            inv.flags.insert("json".into(), "true".into());
            handle_do(&inv)
        }

        fn canned(v: &Value) -> String {
            format!("printf '%s\\n' '{v}'")
        }

        #[test]
        fn the_fake_binary_accepts_and_the_command_is_printed_not_run() {
            let _b = boxed("do-accept", Some(&canned(&verdict("session_trace", json!({ "id": "abc123" }), true, 0.8))));
            let o = say("show the trace for abc123");
            assert_eq!(o.message, "aoide session trace abc123");
            assert_eq!(o.status, crate::output::Status::Ok);
        }

        #[test]
        fn the_fake_binary_receives_the_utterance_on_stdin_and_the_kit_on_out() {
            let ok = canned(&verdict("session_trace", json!({ "id": "x" }), true, 0.8));
            let b = boxed("do-wire", Some(&format!("printf '%s\\n%s\\n' \"$line\" \"$*\" > \"$(dirname \"$0\")/seen\"\n{ok}")));
            let _ = say("  show   me\nthe trace ");
            let seen = std::fs::read_to_string(b.root.join("seen")).unwrap();
            assert_eq!(seen, format!("show me the trace\ndispatch --out {}\n", b.root.join("verba/aoide").display()));
        }

        #[test]
        fn the_fake_binary_abstains_and_the_candidates_come_back_as_commands() {
            let _b = boxed("do-reject", Some(&canned(&verdict("session_trace", json!({ "id": "abc" }), false, 0.1))));
            let o = say("hmm");
            assert_eq!(o.status.exit_code(), 1);
            let text = rendered(&o);
            assert!(text.contains("did not resolve to one command") && text.contains("0.80  aoide session trace abc"), "{text}");
        }

        #[test]
        fn the_fake_binary_misses_a_slot_and_the_command_is_shown_with_it_named() {
            let _b = boxed("do-slot", Some(&canned(&verdict("session_trace", json!({}), true, 0.8))));
            let text = rendered(&say("trace please"));
            assert!(text.contains("fix: aoide session trace <id>"), "{text}");
        }

        #[test]
        fn a_missing_binary_is_one_taught_refusal_that_says_how_to_install_it() {
            let _b = boxed("do-nobinary", None);
            let o = say("anything");
            assert_eq!(o.status.exit_code(), 1);
            let text = rendered(&o);
            assert!(text.contains("is not installed"), "{text}");
            assert!(text.contains("github.com/noah427/verba-volantia"), "{text}");
            assert!(text.contains("fix: aoide config set verba.binary /path/to/verba-volantia"), "{text}");
        }

        #[test]
        fn a_binary_found_on_path_by_its_bare_name_is_used_and_a_missing_kit_says_where_it_goes() {
            let b = boxed("do-path", Some("exit 0"));
            std::fs::remove_file(b.root.join("config.toml")).unwrap();
            let _path = EnvSaver::capture(&["PATH"]);
            std::env::set_var("PATH", &b.root);
            std::fs::remove_file(b.root.join("verba/aoide/model.safetensors")).unwrap();
            let text = rendered(&say("anything"));
            assert!(text.contains(&format!("no trained kit in `{}`", b.root.join("verba/aoide").display())), "{text}");
            assert!(text.contains("model.safetensors not there"), "{text}");
            assert!(text.contains("fix: aoide do kit --help"), "{text}");
        }

        #[test]
        fn a_malformed_verdict_a_silent_child_and_a_failing_child_are_each_one_refusal() {
            let _b = boxed("do-garbled", Some("echo 'this is not json'; echo '{\"nope\":1}'"));
            assert!(rendered(&say("x")).contains("answer cannot be read"));
            drop(_b);
            let _b = boxed("do-silent", Some("exit 0"));
            assert!(rendered(&say("x")).contains("printed no verdict"));
            drop(_b);
            let _b = boxed("do-fails", Some("echo 'Error: weights/meta.json: train first' >&2; exit 1"));
            let text = rendered(&say("x"));
            assert!(text.contains("gave no answer") && text.contains("it exited with an error: Error: weights/meta.json: train first"), "{text}");
        }

        #[test]
        fn a_verdict_from_a_child_that_then_fails_is_not_accepted() {
            let ok = canned(&verdict("session_trace", json!({ "id": "a" }), true, 0.8));
            let _b = boxed("do-exit3", Some(&format!("{ok}\nexit 3")));
            let text = rendered(&say("x"));
            assert!(text.contains("gave no answer") && !text.contains("fix: aoide session"), "{text}");
        }

        #[test]
        fn noise_before_the_verdict_is_ignored() {
            let _b = boxed("do-noise", Some(&format!("echo 'device: cuda:0'\n{}", canned(&verdict("session_trace", json!({ "id": "a" }), true, 0.8)))));
            assert_eq!(say("x").message, "aoide session trace a");
        }

        // ── the kit command ─────────────────────────────────────────────────────

        #[test]
        fn the_kit_prints_the_spec_and_writing_it_names_the_commands_that_train_it() {
            let b = boxed("do-kit", None);
            let mut inv = aoide_test_support::inv(&["do", "kit"], &[]);
            let o = handle_kit(&inv);
            let spec: Value = serde_json::from_str(&o.message).expect("text mode prints the spec itself");
            assert!(spec["functions"].as_array().unwrap().iter().any(|f| f["name"] == "session_trace"));
            assert!(!spec["functions"].as_array().unwrap().iter().any(|f| f["name"] == "do" || f["name"] == "do_kit"));

            let file = b.root.join("templates.json");
            inv.flags.insert("out".into(), file.display().to_string());
            let o = handle_kit(&inv);
            assert_eq!(serde_json::from_str::<Value>(&std::fs::read_to_string(&file).unwrap()).unwrap(), spec);
            assert!(o.message.contains(GEN) && o.message.contains(TRAIN), "{}", o.message);
            assert!(o.message.contains(&format!("mkdir -p {}", b.root.join("verba/aoide").display())), "{}", o.message);
            for c in ["do", "do.kit"] {
                let summary = registry().commands().find(|x| x.dotted() == c).unwrap().summary;
                if c == "do.kit" {
                    assert!(summary.contains(GEN) && summary.contains(TRAIN), "--help must carry the exact commands");
                }
            }
        }

        #[test]
        fn an_unwritable_out_is_a_taught_refusal() {
            let b = boxed("do-kit-bad", None);
            let mut inv = aoide_test_support::inv(&["do", "kit"], &[]);
            inv.flags.insert("out".into(), b.root.join("nope/templates.json").display().to_string());
            let o = handle_kit(&inv);
            assert_eq!(o.status.exit_code(), 1);
            assert!(rendered(&o).contains("was not found") || rendered(&o).contains("cannot write"), "{}", rendered(&o));
        }

    }
}
