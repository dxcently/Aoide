//! The kit's closed set, read off the registry: one intent per command path,
//! one slot per positional argument and value-taking flag, a few phrasings per
//! intent, rendered as the `templates.json` `verba-volantia gen --spec` reads.
//!
//! Nothing here is authored against the command list. The registry is the
//! source of truth, so a command added, renamed or removed changes the spec on
//! the next `aoide do kit`, and [`resolve`] is the same table read the other
//! way: an intent the classifier returns that [`surface`] does not hold is a
//! kit bug, never a command.
//!
//! Template syntax and style are VV's (`src/datagen.rs`, `tools/
//! validate_templates.py`): lowercase `[a-z0-9 ]` words, `{param}` holes,
//! 2..=18 words, no duplicate bag of tokens.

use crate::registry::{Command, Registry};
use aoide_protocol::suggest::lead;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// The class for "confidently not one of ours".
pub const NONE: &str = "none";

const WRAPPERS: &[&str] =
    &["{}", "please {}", "can you {}", "{} for me", "go ahead and {}", "hey aoide {}", "i need you to {}", "{} right now"];

/// Requests that are not commands, each with a value it must not bind.
const DECOYS: &[&str] = &[
    "order a pizza for {x}",
    "what is the weather in {x}",
    "tell me a joke about {x}",
    "translate {x} into french",
    "play {x} on spotify",
    "send an email to {x}",
    "book a flight to {x}",
    "write a poem about {x}",
    "summarize the article {x}",
    "what is the capital of {x}",
    "set an alarm for {x}",
    "who won the game {x}",
];

const MIN_WORDS: usize = 2;
const MAX_WORDS: usize = 18;
const VERBS: &[&str] = &["run", "execute", "launch", "invoke", "call"];
const PREPOSITIONS: &[&str] = &["for", "on", "named"];

/// Where a slot's value lands on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Arg(usize),
    Flag(&'static str),
}

/// A command is in the closed set when a person can type it: implemented, not
/// hook plumbing, and not `do` itself.
pub fn surface(r: &Registry) -> impl Iterator<Item = &Command> {
    r.commands().filter(|c| c.implemented && !c.internal && c.path[0] != "do")
}

pub fn intent_id(c: &Command) -> String {
    c.path.iter().map(|s| ident(s)).collect::<Vec<_>>().join("_")
}

/// The command a classifier intent names.
pub fn resolve<'a>(r: &'a Registry, intent: &str) -> Option<&'a Command> {
    surface(r).find(|c| intent_id(c) == intent)
}

fn ident(s: &str) -> String {
    s.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()
}

/// The slots of `c` in command-line order: positionals, then value flags. A
/// flag that shares a name with an earlier slot is told apart by `_flag`.
pub fn slots(c: &Command) -> Vec<(String, Slot)> {
    let mut out: Vec<(String, Slot)> = c.args.iter().enumerate().map(|(i, a)| (ident(a.name), Slot::Arg(i))).collect();
    for f in c.flags.iter().filter(|f| f.name != "json" && f.takes_value()) {
        let mut id = ident(f.name);
        if out.iter().any(|(taken, _)| *taken == id) {
            id.push_str("_flag");
        }
        out.push((id, Slot::Flag(f.name)));
    }
    out
}

/// Slots a phrasing must carry: what the command cannot run without.
fn needed(c: &Command) -> Vec<String> {
    let all = slots(c);
    let id_of = |s: Slot| all.iter().find(|(_, x)| *x == s).map(|(id, _)| id.clone());
    let mut out: Vec<String> = c.args.iter().enumerate().filter(|(_, a)| a.required).filter_map(|(i, _)| id_of(Slot::Arg(i))).collect();
    for f in c.flags.iter().filter(|f| f.required && f.default.is_empty() && f.takes_value()) {
        out.extend(id_of(Slot::Flag(f.name)));
    }
    for group in c.one_of {
        let has = |n: &&str| c.flags.iter().any(|f| f.name == *n && f.required);
        if !group.iter().any(has) {
            out.extend(c.flags.iter().find(|f| f.name == group[0] && f.takes_value()).and_then(|f| id_of(Slot::Flag(f.name))));
        }
    }
    out
}

fn words(text: &str) -> String {
    text.to_lowercase().replace(['\'', '\u{2019}'], "").split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ")
}

fn holes(ids: &[String]) -> String {
    ids.iter().map(|id| format!(" {{{id}}}")).collect()
}

/// The raw phrasings of one command, before the style and duplicate filters.
fn frames(c: &Command) -> Vec<String> {
    let path: Vec<String> = c.path.iter().map(|s| words(s)).collect();
    let p = path.join(" ");
    let need = needed(c);
    let h = holes(&need);
    let mut out = vec![format!("{p}{h}"), format!("aoide {p}{h}"), format!("use aoide to {p}{h}"), format!("i want to {p}{h}"), format!("the {p} command{h}")];
    out.extend(VERBS.iter().map(|verb| format!("{verb} {p}{h}")));

    let first_clause = |s: &str| words(&lead(s)).split(' ').take(12).collect::<Vec<_>>().join(" ");
    let said = [first_clause(c.brief), first_clause(c.summary)];
    for text in said.iter().filter(|t| !t.is_empty()) {
        out.push(text.clone());
        if let Some(first) = need.first() {
            out.push(format!("{text} for {{{first}}}"));
            out.push(format!("{text}{}", holes(&need)));
        }
    }
    if let [first] = need.as_slice() {
        out.extend(PREPOSITIONS.iter().map(|prep| format!("{p} {prep} {{{first}}}")));
    }
    if let Some((leaf, rest)) = path.split_last() {
        if !rest.is_empty() {
            let noun = rest.iter().rev().cloned().collect::<Vec<_>>().join(" ");
            out.extend(["", "the ", "a ", "my "].iter().map(|det| format!("{leaf} {det}{noun}{h}")));
        }
    }

    let all = slots(c);
    let hole_of = |s: Slot| all.iter().find(|(_, x)| *x == s).map(|(id, _)| format!("{{{id}}}"));
    let in_group = |name: &str| c.one_of.iter().any(|g| g.contains(&name));
    let mut optional: Vec<_> = c.flags.iter().filter(|f| f.takes_value() && f.name != "json" && !f.required).collect();
    optional.sort_by_key(|f| !in_group(f.name));
    for f in optional.into_iter().filter(|f| !need.contains(&ident(f.name))).take(2) {
        let (Some(hole), cue) = (hole_of(Slot::Flag(f.name)), words(f.name)) else { continue };
        out.push(format!("{p}{h} with {cue} {hole}"));
        out.push(format!("{p}{h} {cue} {hole}"));
    }
    for example in c.examples {
        out.extend(example_frame(c, &p, example, &all));
    }
    out
}

/// An example's own shape as a template: its positionals become the argument
/// holes in order, each `--flag value` a cue word and the flag's hole.
fn example_frame(c: &Command, p: &str, example: &str, all: &[(String, Slot)]) -> Option<String> {
    let rest: Vec<&str> = example.split_whitespace().skip(c.path.len()).collect();
    let mut out = p.to_string();
    let (mut arg, mut i) = (0, 0);
    while i < rest.len() {
        match rest[i].strip_prefix("--") {
            Some(name) => {
                let flag = c.flags.iter().find(|f| f.name == name)?;
                i += 1;
                if flag.takes_value() {
                    let id = all.iter().find(|(_, s)| *s == Slot::Flag(flag.name))?.0.clone();
                    out.push_str(&format!(" {} {{{id}}}", words(name)));
                    i += 1;
                }
            }
            None => {
                out.push_str(&format!(" {{{}}}", all.iter().find(|(_, s)| *s == Slot::Arg(arg))?.0));
                arg += 1;
                i += 1;
            }
        }
    }
    Some(out)
}

/// The duplicate key `validate_templates.py` uses: the bag of words, and the
/// order of the holes.
fn key(t: &str) -> (Vec<String>, Vec<String>) {
    let mut bag: Vec<String> = t.split(' ').map(str::to_string).collect();
    bag.sort();
    (bag, t.split(' ').filter(|w| w.starts_with('{')).map(str::to_string).collect())
}

fn fits(t: &str) -> bool {
    (MIN_WORDS..=MAX_WORDS).contains(&t.split(' ').count())
}

/// Every command's phrasings, with any phrasing two commands share dropped
/// from both: an utterance with two labels teaches nothing.
fn phrasings(r: &Registry) -> Vec<(&Command, Vec<String>)> {
    let own: Vec<(&Command, Vec<String>)> = surface(r)
        .map(|c| {
            let mut seen = Vec::new();
            let kept = frames(c)
                .into_iter()
                .filter(|t| fits(t) && !t.is_empty())
                .filter(|t| {
                    let k = key(t);
                    !seen.contains(&k) && {
                        seen.push(k);
                        true
                    }
                })
                .collect();
            (c, kept)
        })
        .collect();
    let mut owners: BTreeMap<(Vec<String>, Vec<String>), usize> = BTreeMap::new();
    for (_, ts) in &own {
        for t in ts {
            *owners.entry(key(t)).or_default() += 1;
        }
    }
    own.into_iter().map(|(c, ts)| (c, ts.into_iter().filter(|t| owners[&key(t)] == 1).collect())).collect()
}

/// The whole `templates.json`, in registry order with `none` last.
pub fn spec(r: &Registry) -> Value {
    let mut functions: Vec<Value> = phrasings(r)
        .into_iter()
        .map(|(c, templates)| {
            let params: Vec<String> = slots(c).into_iter().map(|(id, _)| id).collect();
            json!({ "name": intent_id(c), "params": params, "templates": templates })
        })
        .collect();
    functions.push(json!({ "name": NONE, "params": Vec::<String>::new(), "templates": DECOYS }));
    json!({ "wrappers": WRAPPERS, "functions": functions })
}

/// Intents and templates in a spec, for the line that reports a write.
pub fn counts(spec: &Value) -> (usize, usize) {
    let functions = spec["functions"].as_array().map_or(&[][..], Vec::as_slice);
    (functions.len(), functions.iter().map(|f| f["templates"].as_array().map_or(0, Vec::len)).sum())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{arg, cmd, flag};
    use aoide_protocol::Invocation;

    fn noop(_: &Invocation) -> crate::output::Outcome {
        crate::output::Outcome::ok("x", "x")
    }

    fn sample() -> Registry {
        let mut r = Registry::new();
        r.insert(cmd!(
            path: ["session", "trace"],
            summary: "Show the trace of one session. Follows it live with --follow.",
            args: [arg!("id", "string", true, "The session id.")],
            flags: [flag!("tail", "int", "Last N lines.", value: "n"), flag!("follow", "bool", "Follow it live.")],
            gated: false,
            implemented: true,
            handler: noop,
            examples: ["session trace abc --tail 20"],
            brief: "Show one session's trace."
        ));
        r.insert(cmd!(path: ["node", "pull"], summary: "Pull a node.", args: [arg!("name", "string", true, "Node.")], flags: [flag!("name", "string", "Rename.")], gated: false, implemented: true, handler: noop));
        r.insert(cmd!(path: ["session", "start"], summary: "Hook.", args: [], flags: [], gated: false, implemented: true, handler: noop, internal: true));
        r.insert(cmd!(path: ["later"], summary: "Soon.", args: [], flags: [], gated: false, implemented: false, handler: noop));
        r.insert(cmd!(path: ["do"], summary: "Ask.", args: [], flags: [], gated: false, implemented: true, handler: noop));
        r
    }

    #[test]
    fn the_closed_set_is_what_a_person_can_type() {
        let r = sample();
        let ids: Vec<String> = surface(&r).map(intent_id).collect();
        assert_eq!(ids, ["session_trace", "node_pull"], "no plumbing, no stub, not `do` itself");
        assert!(resolve(&r, "session_trace").is_some());
        assert!(resolve(&r, "session_start").is_none() && resolve(&r, "none").is_none());
    }

    #[test]
    fn slots_are_args_then_value_flags_and_a_shared_name_is_told_apart() {
        let r = sample();
        let trace = surface(&r).next().unwrap();
        assert_eq!(slots(trace), [("id".to_string(), Slot::Arg(0)), ("tail".to_string(), Slot::Flag("tail"))]);
        let pull = surface(&r).nth(1).unwrap();
        assert_eq!(slots(pull), [("name".to_string(), Slot::Arg(0)), ("name_flag".to_string(), Slot::Flag("name"))]);
    }

    #[test]
    fn the_spec_is_vvs_shape_and_every_template_passes_vvs_style() {
        let spec = spec(&sample());
        assert_eq!(spec["wrappers"][0], "{}");
        let functions = spec["functions"].as_array().unwrap();
        assert_eq!(functions.last().unwrap()["name"], NONE);
        for f in functions {
            let params: Vec<&str> = f["params"].as_array().unwrap().iter().map(|p| p.as_str().unwrap()).collect();
            let allowed: Vec<&str> = if params.is_empty() { vec!["x", "y"] } else { params };
            for t in f["templates"].as_array().unwrap() {
                let t = t.as_str().unwrap();
                assert!(t.chars().all(|c| matches!(c, 'a'..='z' | '0'..='9' | ' ' | '{' | '}' | '_')), "style: {t}");
                assert!(fits(t), "length: {t}");
                for hole in t.split(' ').filter(|w| w.starts_with('{')) {
                    assert!(allowed.contains(&hole.trim_matches(|c| c == '{' || c == '}')), "{t}: undeclared hole");
                }
            }
        }
    }

    #[test]
    fn phrasings_come_from_the_command_its_brief_and_its_example() {
        let spec = spec(&sample());
        let trace: Vec<&str> = spec["functions"][0]["templates"].as_array().unwrap().iter().map(|t| t.as_str().unwrap()).collect();
        for want in [
            "session trace {id}",
            "show one sessions trace for {id}",
            "trace the session {id}",
            "session trace {id} with tail {tail}",
        ] {
            assert!(trace.contains(&want), "{want} missing from {trace:?}");
        }
        assert!(trace.iter().all(|t| !t.contains("follow")), "a bool flag is not a slot, so no phrasing carries one");
        assert_eq!(spec["functions"][0]["params"], json!(["id", "tail"]));
    }

    #[test]
    fn a_phrasing_two_commands_share_is_dropped_from_both() {
        let mut r = Registry::new();
        for path in [&["a", "go"][..], &["b", "go"][..]] {
            r.insert(Command { path: Box::leak(path.to_vec().into_boxed_slice()), summary: "Go now.", brief: "Go now.", handler: noop, ..Command::BLANK });
        }
        let spec = spec(&r);
        for f in spec["functions"].as_array().unwrap().iter().take(2) {
            assert!(!f["templates"].as_array().unwrap().iter().any(|t| t == "go now"), "{f}");
        }
    }

    #[test]
    fn the_real_registry_yields_unique_intents_and_enough_phrasings_for_vvs_split() {
        let r = crate::commands::all();
        let spec = spec(&r);
        let functions = spec["functions"].as_array().unwrap();
        let mut names: Vec<&str> = functions.iter().map(|f| f["name"].as_str().unwrap()).collect();
        let n = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), n, "two commands share an intent id");
        assert_eq!(n, surface(&r).count() + 1);
        for f in functions {
            for t in f["templates"].as_array().unwrap().iter().map(|t| t.as_str().unwrap()) {
                let holes: Vec<&str> = t.split(' ').filter(|w| w.starts_with('{')).collect();
                let mut unique = holes.clone();
                unique.sort();
                unique.dedup();
                assert_eq!(unique.len(), holes.len(), "{}: `{t}` binds one slot twice", f["name"]);
            }
            // VV holds out a quarter (min 2) as test and 15% (min 1) as dev.
            assert!(f["templates"].as_array().unwrap().len() >= 6, "{}: too few phrasings to split", f["name"]);
        }
        for c in surface(&r) {
            let ids: Vec<String> = slots(c).into_iter().map(|(id, _)| id).collect();
            let mut unique = ids.clone();
            unique.sort();
            unique.dedup();
            assert_eq!(unique.len(), ids.len(), "{}: slot ids collide", c.dotted());
            assert!(ids.iter().all(|id| !id.is_empty()), "{}", c.dotted());
        }
    }
}
