//! Registry-walk assertions: each takes ONE binary's registry and checks
//! every command against what the registry declares about it. `aoide-cli` and
//! `aoide-lyra` each call all of them from a test, so the two binaries are
//! held to the same rules and a command added tomorrow is covered the day it
//! is registered. Each walk asserts what a command DECLARES; one that declares
//! nothing passes, so a rule tightens exactly as commands migrate onto it.

use aoide_protocol::door::{list_description, parse, BRIEF_MAX};
use aoide_protocol::help;
use aoide_protocol::style::{Style, Term};
use aoide_protocol::registry::{Command, Registry};
use aoide_protocol::{Door, Invocation};
use aoide_protocol::output::{Outcome, Status};
use std::cell::Cell;
use std::collections::BTreeMap;

thread_local! {
    static REACHED: Cell<bool> = const { Cell::new(false) };
}

fn sentinel(_: &Invocation) -> Outcome {
    REACHED.with(|r| r.set(true));
    Outcome::ok("sentinel", "reached")
}

/// The registry with every handler swapped for a sentinel, so a walk can tell
/// "refused before the handler" from "the handler ran".
fn with_sentinels(registry: &Registry) -> Registry {
    let mut out = Registry::new();
    for c in registry.commands() {
        let mut c = c.clone();
        c.handler = sentinel;
        out.insert(c);
    }
    out
}

/// Split a shell-ish example into words: whitespace-separated, single and
/// double quotes grouped, stopping at the first pipe, redirect or list
/// operator (an example is a command line, and only its first command is ours).
fn words(example: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    for ch in example.chars() {
        match (quote, ch) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => cur.push(c),
            (None, '"' | '\'') => {
                quote = Some(ch);
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    started = false;
                }
            }
            (None, c) => cur.push(c),
        }
    }
    if started || !cur.is_empty() {
        out.push(cur);
    }
    let stop = out.iter().position(|w| {
        matches!(w.as_str(), "|" | "||" | "&&" | ";" | "<" | ">" | ">>")
            || w.strip_prefix(|c: char| c.is_ascii_digit()).is_some_and(|r| r.starts_with('>'))
    });
    out.truncate(stop.unwrap_or(out.len()));
    out
}

fn argv(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}

fn filled(c: &Command) -> Invocation {
    let required_args = c.args.iter().filter(|a| a.required).count();
    let mut flags = BTreeMap::new();
    for f in c.flags.iter().filter(|f| f.required && f.default.is_empty()) {
        flags.insert(f.name.to_string(), sample(f));
    }
    for group in c.one_of {
        if !group.iter().any(|n| flags.contains_key(*n)) {
            let first = c.flags.iter().find(|f| f.name == group[0]).expect("one_of names a declared flag");
            flags.insert(first.name.to_string(), sample(first));
        }
    }
    Invocation {
        path: c.path.iter().map(|s| s.to_string()).collect(),
        args: (0..required_args).map(|i| format!("arg{i}")).collect(),
        flags,
        door: Door::Cli,
    }
}

fn sample(f: &aoide_protocol::registry::Flag) -> String {
    if !f.takes_value() {
        "true".into()
    } else if let Some(v) = f.values.first() {
        v.to_string()
    } else {
        "x".into()
    }
}

fn run_sentinel(c: &Command, bin: &str, inv: &Invocation) -> (Outcome, bool) {
    REACHED.with(|r| r.set(false));
    let mut probe = c.clone();
    probe.handler = sentinel;
    let out = probe.invoke(bin, inv);
    (out, REACHED.with(Cell::get))
}

/// Every example starts with its own command path and parses (flags known,
/// arguments present, values allowed) against the registry it is shown in.
pub fn every_example_parses(bin: &str, registry: &Registry) {
    let sentinels = with_sentinels(registry);
    for c in registry.commands() {
        for ex in c.examples {
            let w = words(ex);
            assert!(
                w.len() >= c.path.len() && w[..c.path.len()].iter().zip(c.path).all(|(a, b)| a == b),
                "`{}` example does not start with its own path: {ex}",
                c.dotted()
            );
            if let Err(o) = parse(&w, Door::Cli, bin, &sentinels) {
                panic!("`{}` example does not parse: {ex}\n{}", c.dotted(), o.message);
            }
        }
    }
}

/// Everything a command declares required is refused before its handler runs,
/// by name, at exit 2 — and the same invocation with the item present reaches
/// the handler.
pub fn required_is_enforced(bin: &str, registry: &Registry) {
    for c in registry.commands().filter(|c| c.implemented) {
        let full = filled(c);
        let (out, reached) = run_sentinel(c, bin, &full);
        assert!(reached && out.status == Status::Ok, "`{}`: a satisfied invocation was refused: {}", c.dotted(), out.message);

        let required_args: Vec<_> = c.args.iter().filter(|a| a.required).collect();
        for (i, a) in required_args.iter().enumerate() {
            let mut inv = full.clone();
            inv.args.truncate(i);
            expect_refusal(c, bin, &inv, &format!("<{}>", a.name));
        }
        for f in c.flags.iter().filter(|f| f.required && f.default.is_empty()) {
            let mut inv = full.clone();
            inv.flags.remove(f.name);
            expect_refusal(c, bin, &inv, &format!("--{}", f.name));
        }
        for group in c.one_of {
            let mut inv = full.clone();
            for n in *group {
                inv.flags.remove(*n);
            }
            expect_refusal(c, bin, &inv, &format!("--{}", group[0]));
        }
        for f in c.flags {
            for other in f.conflicts {
                let mut inv = full.clone();
                let o = c.flags.iter().find(|x| x.name == *other).expect("conflicts names a declared flag");
                inv.flags.insert(f.name.to_string(), sample(f));
                inv.flags.insert(other.to_string(), sample(o));
                expect_refusal(c, bin, &inv, &format!("--{}", f.name));
            }
        }
        for f in c.flags.iter().filter(|f| !f.values.is_empty()) {
            let mut inv = full.clone();
            inv.flags.insert(f.name.to_string(), "no-such-value-zz".into());
            expect_refusal(c, bin, &inv, &format!("--{}", f.name));
            for v in f.values {
                let mut ok = full.clone();
                ok.flags.insert(f.name.to_string(), v.to_string());
                let (out, reached) = run_sentinel(c, bin, &ok);
                assert!(reached, "`{}`: --{} {v} is declared allowed but was refused: {}", c.dotted(), f.name, out.message);
            }
        }
    }
}

fn expect_refusal(c: &Command, bin: &str, inv: &Invocation, names: &str) {
    let (out, reached) = run_sentinel(c, bin, inv);
    assert!(!reached, "`{}`: the handler ran without {names}", c.dotted());
    assert_eq!(out.status, Status::Usage, "`{}`: a missing {names} is a usage error: {}", c.dotted(), out.message);
    assert_eq!(out.render(false).1, 2, "`{}`: exit code", c.dotted());
    assert!(out.message.contains(names), "`{}`: the refusal must name {names}: {}", c.dotted(), out.message);
}

/// A declared default is applied when the flag is absent, left alone when
/// given, and every declaration is coherent (a default is an allowed value,
/// names in `conflicts`/`one_of` exist, a boolean has no default).
pub fn defaults_applied(bin: &str, registry: &Registry) {
    for c in registry.commands() {
        let names: Vec<&str> = c.flags.iter().map(|f| f.name).collect();
        for group in c.one_of {
            for n in *group {
                assert!(names.contains(n), "`{}`: one_of names unknown flag --{n}", c.dotted());
            }
        }
        for f in c.flags {
            for other in f.conflicts {
                assert!(names.contains(other), "`{}`: --{} conflicts with unknown flag --{other}", c.dotted(), f.name);
            }
            if f.default.is_empty() {
                continue;
            }
            assert!(f.takes_value(), "`{}`: boolean --{} cannot have a default", c.dotted(), f.name);
            assert!(
                f.values.is_empty() || f.values.contains(&f.default),
                "`{}`: default of --{} is not among its values",
                c.dotted(),
                f.name
            );
            let mut inv = filled(c);
            inv.flags.remove(f.name);
            let (_, reached) = run_sentinel(c, bin, &inv);
            assert!(reached, "`{}`: omitting defaulted --{} was refused", c.dotted(), f.name);
            let mut checked = inv.clone();
            c.check(bin, &mut checked).unwrap_or_else(|r| panic!("`{}`: {}", c.dotted(), r.what));
            assert_eq!(checked.flags.get(f.name).map(String::as_str), Some(f.default), "`{}`: --{} default not applied", c.dotted(), f.name);

            let given = if f.values.len() > 1 { f.values.iter().find(|v| **v != f.default).unwrap() } else { "given" };
            if f.values.is_empty() || f.values.contains(&given) {
                let mut inv = filled(c);
                inv.flags.insert(f.name.to_string(), given.to_string());
                let mut checked = inv.clone();
                c.check(bin, &mut checked).unwrap_or_else(|r| panic!("`{}`: {}", c.dotted(), r.what));
                assert_eq!(checked.flags.get(f.name).map(String::as_str), Some(given), "`{}`: a given --{} was overwritten", c.dotted(), f.name);
            }
        }
    }
}

/// A `brief` fits a list line, and so does the description a list shows when
/// a command declares none.
pub fn brief_fits(registry: &Registry) {
    for c in registry.commands() {
        assert!(c.brief.chars().count() <= BRIEF_MAX, "`{}`: brief is {} chars (max {BRIEF_MAX})", c.dotted(), c.brief.chars().count());
        let shown = list_description(c);
        assert!(shown.chars().count() <= BRIEF_MAX + 1, "`{}`: list description is too long: {shown}", c.dotted());
    }
}

/// Every head that lists itself sits in a section (a stub head lists itself
/// last, automatically), and a group of several commands says in one line what
/// it is — its layout blurb, else its bare command's brief.
pub fn every_head_is_sectioned(registry: &Registry) {
    let layout = registry.layout();
    let mut heads: Vec<&str> = registry.commands().filter(|c| !c.internal).map(|c| c.path[0]).collect();
    heads.dedup();
    heads.sort();
    heads.dedup();
    for head in heads {
        let mine: Vec<&Command> = registry.commands().filter(|c| c.path[0] == head && !c.internal).collect();
        if mine.iter().all(|c| !c.implemented) {
            continue;
        }
        assert!(mine.iter().all(|c| !c.section.is_empty()), "head `{head}` has no section: list it in the layout");
        if mine.len() > 1 {
            let blurb = layout.sections.iter().flat_map(|(_, hs)| hs.iter()).find(|(h, _)| *h == head).map(|(_, b)| *b).unwrap_or_default();
            let bare = mine.iter().any(|c| c.path.len() == 1 && !c.brief.is_empty());
            assert!(!blurb.is_empty() || bare, "group `{head}` has {} commands and no one-line blurb in the layout", mine.len());
        }
    }
}

/// Every listing — the overview, each group's page, the unknown-name lists —
/// fits the width, and a wrapped line hangs under its column rather than
/// starting at column 0.
pub fn listings_fit_and_hang(bin: &str, registry: &Registry) {
    for width in [60, 80, 105, 120] {
        let t = Term { style: Style::OFF, width };
        let over = help::overview(registry, bin, &t);
        let paragraphs: Vec<&str> = over.split("\n\n").collect();
        for body in &paragraphs[1..paragraphs.len() - 1] {
            for line in body.lines().skip(1) {
                assert!(line.starts_with("  "), "{bin} overview at {width}: `{line}` is at column 0");
            }
        }
        let mut heads: Vec<&str> = registry.commands().map(|c| c.path[0]).collect();
        heads.dedup();
        heads.sort();
        heads.dedup();
        let mut pages = vec![over, help::choices(&["zzqq".to_string()], registry, &t)];
        for head in heads {
            if let Some(page) = help::group(&[head.to_string()], registry, bin, &t) {
                for line in page.lines().skip_while(|l| !l.starts_with("commands:")).skip(1).take_while(|l| !l.is_empty()) {
                    assert!(line.starts_with("  "), "{bin} {head} at {width}: `{line}` is at column 0");
                }
                pages.push(page);
            }
        }
        for page in pages {
            for line in page.lines() {
                let longest_word = line.split(' ').map(|w| w.chars().count()).max().unwrap_or(0);
                assert!(line.chars().count() <= width.max(longest_word + 4), "{bin} at {width}: line is too wide: `{line}`");
            }
        }
    }
}

fn render(o: &Outcome) -> String {
    o.render(false).0
}

/// An unknown command, flag or value is refused (exit 2) with a close match
/// when there is one, and the valid choices when there is not.
pub fn suggestion_or_list(bin: &str, registry: &Registry) {
    let sentinels = with_sentinels(registry);
    for c in registry.commands() {
        let head = c.path[0];
        if head.chars().count() >= 4 && c.path.len() == 1 {
            let typo = format!("{head}x");
            let err = parse(&argv(&[&typo]), Door::Cli, bin, &sentinels).expect_err("a typo is not a command");
            assert_eq!(err.status, Status::Usage);
            assert!(render(&err).contains(&format!("{bin} {head}")), "`{typo}` should suggest `{head}`: {}", err.message);
        }
        for f in c.flags.iter().filter(|f| f.name != "json" && f.name.chars().count() >= 4) {
            let mut chars: Vec<char> = f.name.chars().collect();
            let n = chars.len();
            chars.swap(n - 2, n - 1);
            let typo: String = chars.into_iter().collect();
            if c.flags.iter().any(|g| g.name == typo) || typo == f.name {
                continue;
            }
            let mut args: Vec<String> = c.path.iter().map(|s| s.to_string()).collect();
            args.push(format!("--{typo}"));
            let err = parse(&args, Door::Cli, bin, &sentinels).expect_err("an unknown flag is refused");
            assert_eq!(err.status, Status::Usage);
            assert!(render(&err).contains(&format!("--{}", f.name)), "`--{typo}` on `{}` should suggest --{}: {}", c.dotted(), f.name, err.message);
        }
        for f in c.flags.iter().filter(|f| !f.values.is_empty()) {
            let mut inv = filled(c);
            inv.flags.insert(f.name.to_string(), "zzzz-no-such-value".into());
            let (out, _) = run_sentinel(c, bin, &inv);
            assert_eq!(out.status, Status::Usage);
            assert!(f.values.iter().all(|v| render(&out).contains(v)), "`{}`: the refusal must list the accepted values: {}", c.dotted(), render(&out));
        }
    }
    let err = parse(&argv(&["zzqq-nothing"]), Door::Cli, bin, &sentinels).expect_err("garbage is not a command");
    let first = registry.commands().next().map(|c| c.path[0]).expect("a registry has a command");
    assert!(render(&err).contains(first), "garbage should list the valid commands: {}", err.message);
    if let Some(c) = registry.commands().find(|c| c.flags.iter().any(|f| f.name != "json")) {
        let mut args: Vec<String> = c.path.iter().map(|s| s.to_string()).collect();
        args.push("--zzqq-nothing".into());
        let err = parse(&args, Door::Cli, bin, &sentinels).expect_err("an unknown flag is refused");
        assert!(render(&err).contains("it takes"), "garbage flag should list the flags: {}", err.message);
    }
}
