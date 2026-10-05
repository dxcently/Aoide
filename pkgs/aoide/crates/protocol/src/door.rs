//! The CLI arg-parser and the parse -> dispatch -> render run loop, shared by
//! every aoide binary (Phase 3 restructure, docs/architecture/PACKAGE-LAYOUT.md).
//!
//! Hand-rolled (no clap) to keep the offline cargo lock tiny and the build
//! pure. The command tree is read from a caller-supplied [`Registry`], so the
//! parser and the schema can never disagree about what commands exist.
//!
//! [`run`] drives the standard loop (parse, render a parse error uniformly,
//! else dispatch the matched command and render its [`Outcome`]) — but a
//! command tree always has a handful of commands that are NOT one-shot dispatch:
//! a raw-stdout tool, a long-running server launch, anything that needs to
//! bypass the `Outcome` envelope entirely. Those are per-binary — the core
//! `aoide` binary launches `mcp serve --stdio`/`a2a serve`/`conductor`,
//! lyra (P-A4) launches its own smaller set and never touches `a2a serve` or
//! `conductor` — so `run` takes a `special` hook: a closure given the parsed
//! [`Invocation`] and the `--json` flag, run AFTER a successful parse and
//! BEFORE the generic dispatch. `Some(code)` short-circuits with that exit
//! code; `None` falls through to the uniform dispatch+render path. This is
//! how one parser + one run loop serves binaries with different special-case
//! command sets without duplicating either.
//!
//! **Streams, color and width.** [`run`] is the only place output is styled:
//! it strips the global `--color` flag, resolves a `Style` for the stream the
//! outcome is written to (`style`), and hands `parse_with` a `Term` so every
//! listing it builds (`help`) follows the same order, width and palette. An
//! informational parse result (the overview, a group page, `--help`) prints on
//! stdout at exit 0; a refusal prints on stderr at its own code.
//!
//! **External subcommands (task #138, the git/cargo pattern).** Immediately
//! BEFORE `parse` runs, `run` probes raw argv for a fallthrough to an
//! executable `<bin_name>-<name>` on `PATH` — `aoide deploy` becomes
//! `aoide-deploy` the same way `git foo` becomes `git-foo`:
//!
//! ```text
//!   1. argv.first() absent          → parse   (bare `aoide`)
//!   2. name starts with '-'         → parse   (--help, -h, --json)
//!   3. name is a RESERVED HEAD      → parse   (a built-in always wins;
//!      reserved := every registered command's path[0] ∪ every ALIASES head)
//!   4. probe `<bin_name>-<name>` on PATH, executable
//!        miss  → parse                        (did-you-mean survives)
//!        hit   → audit, spawn argv[1..] verbatim, return the child's own
//!                exit code unchanged
//! ```
//!
//! This is why the probe reads RAW argv rather than hooking in after
//! `parse`: by the time `parse` has split flags into a `BTreeMap`, a
//! wrapped command's own flag order/repeats/`--f=v` vs `--f v` spelling is
//! already destroyed, and an external command must receive its argv
//! byte-for-byte. Step 3's reservation is a first-SEGMENT check, not a
//! full-path check, so `aoide graph vie` (a typo of the built-in `graph`
//! group) never probes `aoide-graph` — it lands in `parse`'s own
//! `unknown_command_outcome` with `did_you_mean` intact, same as before this
//! existed. The trust boundary is structural, not a guard: `run` is called
//! from exactly the two CLI entry points (`aoide-cli`'s `run_cli`, lyra's
//! `run_lyra`), so an external command is reachable from `Door::Cli` only —
//! no `Door` check is added here, because there is nothing else to check.
//! MCP/A2A/the aoided socket reach `dispatch()` directly, never `run`, and
//! `dispatch()` has no PATH-probing logic of its own (`crates/cli/src/
//! dispatch.rs`'s `an_unregistered_path_on_a_non_cli_door_is_still_unknown_
//! command` test is the tripwire: the probe must never migrate into
//! `dispatch()`'s own `None =>` arm). An external command never becomes a
//! [`Command`] — it is never gated, never enters the MCP tool list or the
//! A2A `AgentCard`, and the golden command-path snapshot never sees it
//! (CONTRACTS.md §3 states the "never gated" rule as permanent, by
//! construction, not merely by omission today).

use crate::audit::{audit, default_audit_log, Door, EventClass};
use crate::invocation::Invocation;
use crate::help::{self, Row};
use crate::output::{exit, Fix, Kind, Outcome, Status};
use crate::registry::{Command, Registry};
use crate::style::{self, Color, Stream, Style, Term};
use crate::suggest::closest;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::{Command as Process, Stdio};

pub use crate::help::{list_description, BRIEF_MAX};

/// CLI-only ergonomic shorthands for a canonical command path, resolved HERE
/// — before the greedy path match below — so a shorthand never becomes a
/// second registered command: the registry, `schema --json`, and every
/// golden snapshot see only the canonical path. Add a row here for a new
/// shorthand, never a second `register()` call for the same command.
const ALIASES: &[(&[&str], &[&str])] = &[(&["node", "rm"], &["node", "remove"])];

/// Rewrite a leading alias prefix of `positionals` to its canonical form, in
/// place. A no-op when no alias prefix matches (the common case).
fn resolve_aliases(positionals: &mut Vec<String>) {
    for (from, to) in ALIASES {
        if positionals.len() >= from.len() && positionals.iter().zip(*from).all(|(p, f)| p == f) {
            positionals.splice(0..from.len(), to.iter().map(|s| s.to_string()));
            return;
        }
    }
}

/// All known command paths (from the registry), longest-first for greedy match.
fn known_paths(registry: &Registry) -> Vec<Vec<String>> {
    let mut paths: Vec<Vec<String>> = registry
        .commands()
        .map(|c| c.path.iter().map(|s| s.to_string()).collect())
        .collect();
    paths.sort_by_key(|p| std::cmp::Reverse(p.len()));
    paths
}

/// Parse argv (excluding the program name) into an [`Invocation`].
///
/// `bin_name` is the invoking binary's name (`"aoide"` for core, `"lyra"`
/// for the graphical binary, P-A5 of the binary-split workstream) — every
/// usage/help/did-you-mean string below names it instead of a hardcoded
/// `"aoide"`, so lyra's own usage errors say `lyra`, not `aoide`.
///
/// Returns `Err(Outcome)` for a usage error (`--help`, unknown command) so the
/// caller can render it as JSON or text uniformly.
pub fn parse(argv: &[String], door: Door, bin_name: &str, registry: &Registry) -> Result<(Invocation, bool), Outcome> {
    parse_with(argv, door, bin_name, registry, &Term::plain())
}

/// [`parse`] writing its listings and refusals the way `t` says.
fn parse_with(
    argv: &[String],
    door: Door,
    bin_name: &str,
    registry: &Registry,
    t: &Term,
) -> Result<(Invocation, bool), Outcome> {
    // First split off flags anywhere; positionals keep order.
    let mut positionals: Vec<String> = Vec::new();
    let mut flags: BTreeMap<String, String> = BTreeMap::new();
    let mut json = false;
    let mut help = false;

    let mut i = 0;
    while i < argv.len() {
        let a = &argv[i];
        // A bare `--` ends flag parsing: everything after it is positional,
        // verbatim — the wrapped-command seam (`conduct -- codex --model x`
        // must not have the child's flags eaten as aoide's).
        if a == "--" {
            positionals.extend(argv[i + 1..].iter().cloned());
            break;
        }
        // `--help`/`-h` anywhere is a request for usage, never a command flag.
        if a == "--help" || a == "-h" {
            help = true;
            i += 1;
            continue;
        }
        if let Some(name) = a.strip_prefix("--") {
            // `--flag=value` or `--flag value` or bare boolean `--flag` —
            // whether the spaced form may consume a value is decided by the
            // registry's declared flag type (`declared_flag_kind`), below.
            if let Some((k, v)) = name.split_once('=') {
                if k == "json" {
                    json = true;
                }
                flags.insert(k.to_string(), v.to_string());
            } else if name == "json" {
                json = true;
                flags.insert("json".into(), "true".into());
            } else {
                // The registry decides whether this flag takes a value: a
                // declared-bool flag NEVER consumes the next token (#111 —
                // `node add --no-verify alice` used to swallow `alice` as
                // no-verify's value). Only a valued (or undeclared — rejected
                // by name later anyway) flag peeks ahead.
                match declared_flag_kind(name, &positionals, registry) {
                    DeclaredFlag::Bool => {
                        flags.insert(name.to_string(), "true".into());
                    }
                    DeclaredFlag::Mixed if i + 1 < argv.len() && !argv[i + 1].starts_with("--") => {
                        // Declared bool by one candidate command and valued by
                        // another, with a consumable token following: the token
                        // genuinely reads both ways. Same convention as the
                        // path-segment collision below (#48) — refuse loudly,
                        // never guess.
                        return Err(mixed_flag_outcome(name, &argv[i + 1], &positionals, bin_name));
                    }
                    DeclaredFlag::Mixed => {
                        // Nothing consumable follows — only the bare-boolean
                        // reading exists.
                        flags.insert(name.to_string(), "true".into());
                    }
                    DeclaredFlag::Valued | DeclaredFlag::Undeclared => {
                        // Peek: if the next token is a value (not a flag), consume it.
                        if i + 1 < argv.len() && !argv[i + 1].starts_with("--") {
                            if is_command_token(&argv[i + 1], &positionals, registry) {
                                // The token reads both ways: this flag's value, or the
                                // next segment of a command path still being spelled
                                // (`graph session --id start` vs `graph session start
                                // --id …`). Neither reading is safe to pick silently —
                                // refuse loudly, naming both.
                                return Err(ambiguous_flag_outcome(
                                    name,
                                    &argv[i + 1],
                                    &positionals,
                                    registry,
                                    bin_name,
                                ));
                            }
                            flags.insert(name.to_string(), argv[i + 1].clone());
                            i += 1;
                        } else {
                            flags.insert(name.to_string(), "true".into());
                        }
                    }
                }
            }
        } else {
            positionals.push(a.clone());
        }
        i += 1;
    }

    // `aoide help [cmd…]` is `aoide [cmd…] --help`, unless a registered
    // command already owns the word.
    if positionals.first().is_some_and(|p| p == "help") && !registry.commands().any(|c| c.path[0] == "help") {
        positionals.remove(0);
        help = true;
    }

    if positionals.is_empty() {
        return Err(help_outcome(bin_name, help::overview(registry, bin_name, t)));
    }

    resolve_aliases(&mut positionals);

    // `--help` on a group (`secrets`, `graph project`) lists the group, even
    // when a shorter registered command (`graph`) is a prefix of it.
    if help && registry.get(&positionals).is_none() {
        if let Some(message) = help::group(&positionals, registry, bin_name, t) {
            return Err(help_outcome(&positionals.join("."), message));
        }
    }

    // Greedy longest-prefix match of positionals against known command paths.
    let paths = known_paths(registry);
    let matched = paths
        .iter()
        .find(|p| p.len() <= positionals.len() && p.iter().zip(&positionals).all(|(a, b)| a == b))
        .cloned();

    let path = match matched {
        Some(p) => p,
        None => {
            // A `--help` on an unknown path still surfaces the command list
            // rather than a bare "unknown command".
            if help {
                return Err(help_outcome(&positionals.join("."), help::overview(registry, bin_name, t)));
            }
            return Err(unknown_command_outcome(&positionals, registry, bin_name, t));
        }
    };

    // `--help`/`-h` on a known subcommand → that subcommand's usage (exit 0).
    if help {
        return Err(help_outcome(&path.join("."), command_usage(&path, registry, bin_name, t)));
    }

    // Reject an unrecognised flag by name (exit 2) — never silently swallow it.
    let args = positionals[path.len()..].to_vec();

    if let Some(bad) = unknown_flag(&path, &flags, registry) {
        return Err(unknown_flag_outcome(&bad, &path, &args, &flags, registry, bin_name, t));
    }

    // A matched command that declares NO positional args of its own must not
    // silently swallow leftover positionals as ignored input. Without this,
    // a parent path that is ALSO a registered leaf (`graph` render, alongside
    // the longer `graph link`) would let an unregistered child like `graph
    // nonsense` resolve as the bare parent with a stray arg the handler just
    // ignores, instead of surfacing `nonsense` as the unrecognised segment it
    // is. Only fires when `c.args` is empty — commands that declare at least
    // one positional (`conduct`, `graph spawn`, `graph send`, …) intentionally
    // consume everything past their first required arg verbatim, and must
    // keep doing so.
    if !args.is_empty() {
        if let Some(c) = command_for(&path, registry) {
            if c.args.is_empty() {
                return Err(unknown_command_outcome(&positionals, registry, bin_name, t));
            }
        }
    }

    let mut inv = Invocation {
        path,
        args,
        flags,
        door,
    };
    if let Some(c) = command_for(&inv.path, registry) {
        if let Err(refusal) = c.check(bin_name, &mut inv) {
            return Err(refusal.into_outcome(inv.dotted()));
        }
    }
    Ok((inv, json))
}

/// The registry entry for a matched command path (name-for-name).
fn command_for<'a>(path: &[String], registry: &'a Registry) -> Option<&'a Command> {
    registry.get(path)
}

/// Heads only `aoide` registers, heads only `lyra` registers, and heads both
/// do (`cli`'s and `lyra`'s registry tests pin all three against the real
/// registries). A typed head the running binary lacks but the other owns
/// gets a "run it there" refusal instead of a did-you-mean.
pub const AOIDE_ONLY_HEADS: &[&str] = &[
    "a2a", "adapter", "conduct", "conductor", "config", "content", "context", "daemon", "events", "graph",
    "hooks", "identity", "mail", "make", "melete", "mesh", "node", "project", "resurrect", "send", "session",
    "soundcheck", "spawn", "update", "usage", "workspace",
];
pub const LYRA_ONLY_HEADS: &[&str] = &[
    "cover", "element", "herald", "icon", "livery", "preview", "quickshell", "reload", "rice", "screen",
    "shellbridge",
];
pub const SHARED_HEADS: &[&str] = &["guide", "mcp", "onboard", "pair", "schema", "secrets"];

/// The other binary's name when `head` belongs to it and not to this one.
fn other_binary(head: &str, bin_name: &str, registry: &Registry) -> Option<&'static str> {
    if registry.commands().any(|c| c.path[0] == head) {
        return None;
    }
    match bin_name {
        "aoide" if LYRA_ONLY_HEADS.contains(&head) => Some("lyra"),
        "lyra" if AOIDE_ONLY_HEADS.contains(&head) => Some("aoide"),
        _ => None,
    }
}

/// The answer to an unresolvable invocation. Four shapes, by intent:
///
/// * The input is exactly a GROUP (`secrets`, `graph project`) — the caller
///   asked what is there: its listing, informational (exit 0).
/// * The head belongs to the other binary (`aoide rice stage`): say so and
///   give the command to run there.
/// * A typo of a command, group or leaf — each typed segment matched against
///   the registered paths by edit distance (`nodee lst` → `node list`).
/// * Neither close nor a group: list what IS valid at the point it went wrong.
///
/// The refusals end with the `aoide --help` pointer.
fn unknown_command_outcome(positionals: &[String], registry: &Registry, bin_name: &str, t: &Term) -> Outcome {
    if let Some(listing) = help::group(positionals, registry, bin_name, t) {
        return help_outcome(&positionals.join("."), listing);
    }
    if let Some(other) = other_binary(&positionals[0], bin_name, registry) {
        return Outcome::refuse(
            positionals[0].clone(),
            Kind::Usage,
            format!("`{}` is {} command, not {} one", positionals[0], article_for(other), article_for(bin_name)),
            format!("{other} owns it; `{bin_name}` does not register it"),
            Fix::Run(format!("{other} {}", positionals.join(" "))),
        );
    }
    let mut message = format!("unknown command: `{}`", positionals.join(" "));
    let suggestions = did_you_mean(positionals, registry);
    if suggestions.is_empty() {
        message.push_str(&format!("\n\n{}", help::choices(positionals, registry, t)));
    } else {
        message.push_str("\n\ndid you mean:");
        for s in suggestions {
            message.push_str(&format!("\n  {}", t.style.suggest(&format!("{bin_name} {s}"))));
        }
    }
    Outcome::usage(
        positionals.join("."),
        format!("{message}\n\nrun '{bin_name} --help' for the full command list"),
    )
}

fn article_for(bin_name: &str) -> String {
    format!("{} {bin_name}", if bin_name.starts_with(['a', 'e', 'i', 'o', 'u']) { "an" } else { "a" })
}

/// The closest registered paths to a typo'd input, nearest first, at most
/// two. Each typed segment is matched against the same segment of a path by
/// edit distance, so a typo in a group (`nodee list`), a leaf (`node lst`) or
/// both (`nodee lst`) lands on the real path; segments typed past the path's
/// end (the arguments) are carried over verbatim. A path the input already
/// matches exactly is never a suggestion — that input is not a typo.
fn did_you_mean(positionals: &[String], registry: &Registry) -> Vec<String> {
    let mut scored: Vec<(usize, usize, String)> = Vec::new();
    for c in registry.commands().filter(|c| !c.internal) {
        let k = c.path.len().min(positionals.len());
        let mut total = 0;
        let mut ok = true;
        for (typed, real) in positionals.iter().zip(c.path) {
            if typed == real {
                continue;
            }
            if closest(typed, [*real], 1).is_empty() {
                ok = false;
                break;
            }
            total += crate::suggest::distance(&typed.to_lowercase(), &real.to_lowercase());
        }
        if !ok || total == 0 {
            continue;
        }
        let mut words: Vec<&str> = c.path[..k].to_vec();
        words.extend(positionals[k..].iter().map(String::as_str));
        let line = words.join(" ");
        if !scored.iter().any(|(_, _, l)| *l == line) {
            scored.push((total, help::rank(registry, c.path[0]), line));
        }
    }
    // Nearest first; a tie goes to the head listed first, then registration order.
    scored.sort_by_key(|(d, r, _)| (*d, *r));
    scored.into_iter().take(2).map(|(_, _, p)| p).collect()
}

/// Flags accepted for a command: the schema-declared ones (which already
/// include `--json`) plus `--audit-log`, which the dispatcher honours on every
/// command as the audit-log override.
fn allowed_flags(path: &[String], registry: &Registry) -> Vec<String> {
    let mut names: Vec<String> = vec!["json".into(), "audit-log".into()];
    if let Some(c) = command_for(path, registry) {
        names.extend(c.flags.iter().map(|f| f.name.to_string()));
    }
    names
}

/// The first flag present that the matched command does not accept, if any.
fn unknown_flag(path: &[String], flags: &BTreeMap<String, String>, registry: &Registry) -> Option<String> {
    let allowed = allowed_flags(path, registry);
    flags
        .keys()
        .find(|k| !allowed.iter().any(|a| a == *k))
        .cloned()
}

/// The refusal for a flag the command does not take: the close match if
/// there is one (with the value the user typed carried over), else the flags
/// it does take, and the one-line usage rather than the whole help block.
fn unknown_flag_outcome(
    bad: &str,
    path: &[String],
    args: &[String],
    flags: &BTreeMap<String, String>,
    registry: &Registry,
    bin_name: &str,
    t: &Term,
) -> Outcome {
    let me = format!("{bin_name} {}", path.join(" "));
    let Some(c) = command_for(path, registry) else {
        return unknown_command_outcome(path, registry, bin_name, t);
    };
    let mut taken: Vec<&str> = c.flags.iter().map(|f| f.name).collect();
    taken.push("audit-log");
    let usage = c.synopsis(bin_name, true);
    let near = closest(bad, taken.iter().copied(), 1);
    let (why, fix) = match near.first() {
        Some(n) => {
            let mut fix = me.clone();
            for a in args {
                fix.push_str(&format!(" {a}"));
            }
            let typed = flags.get(bad).map(String::as_str).unwrap_or("true");
            let takes_value = c.flags.iter().find(|f| f.name == *n).is_none_or(|f| f.takes_value());
            if takes_value && typed != "true" {
                fix.push_str(&format!(" --{n} {typed}"));
            } else {
                fix.push_str(&format!(" --{n}"));
            }
            (format!("did you mean `--{n}`? usage: {usage}"), fix)
        }
        None => {
            let names: Vec<String> = c.flags.iter().filter(|f| f.name != "json").map(|f| format!("--{}", f.name)).collect();
            let list = if names.is_empty() { "no flags besides --json".to_string() } else { names.join(", ") };
            (format!("it takes {list}; usage: {usage}"), format!("{me} --help"))
        }
    };
    Outcome::refuse(
        path.join("."),
        Kind::Usage,
        format!("unrecognized flag `--{bad}` for `{me}`"),
        why,
        Fix::Run(fix),
    )
}

/// An Ok-status outcome carrying a raw usage block. `run` prints an
/// Ok-status parse result to stdout and exits 0 — the `--help` path.
fn help_outcome(cmd: &str, message: String) -> Outcome {
    Outcome::ok(cmd, message)
}

/// The per-subcommand usage block printed for `--help`/`-h`, built from the
/// registry so it can never drift from the real arg/flag set.
fn command_usage(path: &[String], registry: &Registry, bin_name: &str, t: &Term) -> String {
    let Some(c) = command_for(path, registry) else {
        return help::overview(registry, bin_name, t);
    };
    let st = t.style;
    let summary = style::wrap(c.summary, t.width).join("\n");
    let mut s = format!("{}\n\n{summary}", help::usage_line(&c.synopsis(bin_name, true), t));
    if !c.implemented {
        s.push_str(&format!("\n\n{}", st.stub("(not yet implemented)")));
    }
    if !c.args.is_empty() {
        let rows: Vec<Row> = c
            .args
            .iter()
            .map(|a| {
                let mut notes = vec![if a.required { "required" } else { "optional" }.to_string()];
                if !a.values.is_empty() {
                    notes.push(format!("one of: {}", a.values.join(", ")));
                }
                if !a.required_after.is_empty() {
                    notes.push(format!("required after: {}", a.required_after.join(", ")));
                }
                Row {
                    note: format!("({})", notes.join("; ")),
                    ..Row::new(format!("<{}>", a.name), "", a.description)
                }
            })
            .collect();
        s.push_str(&format!("\n\n{}\n{}", st.heading("args:"), help::table(&rows, t)));
    }
    let rows: Vec<Row> = c
        .flags
        .iter()
        .map(|f| {
            let mut notes: Vec<String> = Vec::new();
            if f.required && f.default.is_empty() {
                notes.push("required".into());
            }
            if !f.values.is_empty() {
                notes.push(format!("one of: {}", f.values.join(", ")));
            }
            if !f.default.is_empty() {
                notes.push(format!("default: {}", f.default));
            }
            let args = if f.takes_value() { format!("<{}>", f.placeholder()) } else { String::new() };
            Row {
                note: if notes.is_empty() { String::new() } else { format!("({})", notes.join("; ")) },
                ..Row::new(format!("--{}", f.name), args, f.description)
            }
        })
        .collect();
    s.push_str(&format!("\n\n{}\n{}", st.heading("flags:"), help::table(&rows, t)));
    for group in c.one_of {
        let names: Vec<String> = group.iter().map(|n| format!("--{n}")).collect();
        s.push_str(&format!("\n\none of {} is required", names.join(" / ")));
    }
    let subs = help::children(path, registry, t);
    if !subs.is_empty() {
        s.push_str(&format!("\n\n{}\n{subs}", st.heading("subcommands:")));
    }
    if !c.examples.is_empty() {
        s.push_str(&format!("\n\n{}", st.heading("examples:")));
        for ex in c.examples {
            s.push_str(&format!("\n  {}", example_line(ex, bin_name, registry)));
        }
    }
    s
}

/// An example as typed at a shell: a command of this binary gets the binary's
/// name; anything already starting with it, or with another shell word
/// (`printf … | aoide …`), is shown verbatim.
fn example_line(example: &str, bin_name: &str, registry: &Registry) -> String {
    let first = example.split_whitespace().next().unwrap_or_default();
    if registry.commands().any(|c| c.path[0] == first) || ALIASES.iter().any(|(from, _)| from[0] == first) {
        format!("{bin_name} {example}")
    } else {
        example.to_string()
    }
}

/// What the registry declares `--name` to be at this point in the parse —
/// judged across every CANDIDATE command, i.e. every registered command whose
/// path agrees with the positionals collected so far on their common prefix
/// (covers both orderings: the flag before the path is complete, and the flag
/// after the full path with args already interleaved). Same position-aware
/// stance as [`is_command_token`], one struct field over: that helper reads
/// the candidates' PATHS, this one reads their flag specs.
enum DeclaredFlag {
    /// Every candidate that declares the flag declares it `"bool"` — it never
    /// takes a value token.
    Bool,
    /// Every candidate that declares it gives it a non-bool type — it may
    /// consume the next token as its value.
    Valued,
    /// Declared `"bool"` by one candidate and valued by another — with a
    /// consumable token following, the parse refuses loudly rather than
    /// guessing (the #48 convention).
    Mixed,
    /// No candidate declares it (an alias-spelled path mid-collection, a typo
    /// rejected by name after the path match, or the dispatcher-level
    /// `--audit-log` override) — the legacy peek-and-consume applies.
    /// Consequence for aliases: an alias-spelled invocation of a command
    /// whose declared BOOL flag precedes positionals falls into this arm
    /// and re-swallows the next token (the exact #111 shape) — harmless for
    /// today's one flagless alias (`node rm`), but any future alias for a
    /// bool-flagged command must resolve aliases BEFORE the flag loop or
    /// teach this judge the alias table.
    Undeclared,
}

/// Resolve `--name` against the candidate commands' flag specs — see
/// [`DeclaredFlag`] for the verdicts and the candidate definition.
fn declared_flag_kind(name: &str, prior: &[String], registry: &Registry) -> DeclaredFlag {
    let mut bool_seen = false;
    let mut valued_seen = false;
    for c in registry.commands() {
        let n = c.path.len().min(prior.len());
        if !c.path[..n].iter().zip(&prior[..n]).all(|(a, b)| *a == b) {
            continue;
        }
        if let Some(f) = c.flags.iter().find(|f| f.name == name) {
            if f.ty == "bool" {
                bool_seen = true;
            } else {
                valued_seen = true;
            }
        }
    }
    match (bool_seen, valued_seen) {
        (true, true) => DeclaredFlag::Mixed,
        (true, false) => DeclaredFlag::Bool,
        (false, true) => DeclaredFlag::Valued,
        (false, false) => DeclaredFlag::Undeclared,
    }
}

/// The usage error for a flag declared `"bool"` by one candidate command and
/// valued by another, with a consumable token following ([`DeclaredFlag::
/// Mixed`]) — the token reads both ways and the parser refuses to pick,
/// naming both spellings, same as [`ambiguous_flag_outcome`].
fn mixed_flag_outcome(flag: &str, value: &str, prior: &[String], bin_name: &str) -> Outcome {
    let at = if prior.is_empty() { String::new() } else { format!(" {}", prior.join(" ")) };
    Outcome::usage(
        prior.join("."),
        format!(
            "ambiguous: `--{flag}` is boolean for one `{bin_name}{at}` command and \
             takes a value for another — `{value}` could be its value or a positional\
             \n\ndid you mean:\
             \n  {bin_name} … --{flag}={value}  (`{value}` as the flag's value)\
             \n  {bin_name} … --{flag} -- {value}  (`{value}` as a positional, `--{flag}` bare)\
             \n\nspell the full command path before the flag to disambiguate; \
             run '{bin_name} --help' for the full command list"
        ),
    )
}

/// Is this token part of a command path (so a preceding `--flag` must not
/// silently consume it as a value)?
///
/// Flag-position-aware (khoa, 2026-08-20): `prior` is the positionals already
/// collected by the time the parser reaches this token — i.e. how much of a
/// command path has been built so far, interleaved with whatever flags came
/// before it. A token only continues a command path if some REGISTERED path
/// agrees with `prior` exactly up to `prior.len()` and has `tok` as its very
/// next segment. This is NOT a strict refinement of the old "does `tok`
/// match ANY command's first segment" check — the two compare the token at
/// different depths (any path's first segment vs. the segment after
/// `prior`), so for a non-empty `prior` each accepts tokens the other
/// rejects. What the position-aware form fixes is the first-segment
/// collision the old check suffered anywhere in argv: a value like `a2a` (a
/// real group's first segment) or `shell` broke `--agent a2a` /
/// `--agent shell` even with the command path fully spelled before the
/// flag. What it cannot fix is the converse ordering — a flag placed BEFORE
/// the path is complete, whose value matches the path's next segment
/// (`graph session --id start`): the token genuinely reads both ways, and
/// no yes/no answer here picks correctly. `parse` refuses that ordering
/// loudly ([`ambiguous_flag_outcome`]) instead of guessing.
fn is_command_token(tok: &str, prior: &[String], registry: &Registry) -> bool {
    registry.commands().any(|c| {
        c.path.len() > prior.len()
            && c.path[..prior.len()].iter().zip(prior).all(|(a, b)| *a == b)
            && c.path[prior.len()] == tok
    })
}

/// The usage error for a flag whose value collides with the next segment of
/// a command path still being spelled (`graph session --id start`, where
/// `graph session start` is a registered command). The token reads both ways
/// and the parser refuses to pick silently: name the flag, the ambiguous
/// value, and the unambiguous spelling(s).
fn ambiguous_flag_outcome(
    flag: &str,
    value: &str,
    prior: &[String],
    registry: &Registry,
    bin_name: &str,
) -> Outcome {
    // A registered command the value would continue — `is_command_token` just
    // matched one, so a witness always exists (any one makes the suggestion
    // concrete; the fallback is unreachable belt-and-braces).
    let full = registry
        .commands()
        .find(|c| {
            c.path.len() > prior.len()
                && c.path[..prior.len()].iter().zip(prior).all(|(a, b)| *a == b)
                && c.path[prior.len()] == value
        })
        .map(|c| c.path.join(" "))
        .unwrap_or_else(|| {
            prior.iter().map(String::as_str).chain([value]).collect::<Vec<_>>().join(" ")
        });
    let mut m = format!(
        "ambiguous: `--{flag} {value}` sits before the command path is complete — \
         `{value}` could be the flag's value or the next command-path segment (`{full}`)\
         \n\ndid you mean:\
         \n  {bin_name} {full} --{flag} <value>  (flags after the full command path)"
    );
    // Offer the `--flag=value` spelling only when `prior` is itself a
    // complete registered command — otherwise that spelling just errors too.
    if registry.get(prior).is_some() {
        m.push_str(&format!(
            "\n  {bin_name} {} --{flag}={value}  (`{value}` as the flag's value)",
            prior.join(" ")
        ));
    }
    m.push_str(&format!("\n\nrun '{bin_name} --help' for the full command list"));
    let mut continued = prior.to_vec();
    continued.push(value.to_string());
    Outcome::usage(continued.join("."), m)
}

/// Every first path segment a registered command begins with, plus every
/// [`ALIASES`] entry's own first segment — the set an external-command probe
/// must never shadow (module doc's step 3). A built-in always wins, and a
/// TYPO of a built-in head still falls through to `parse`'s own
/// `unknown_command_outcome`/did-you-mean rather than a silent PATH probe
/// pre-empting it. Today's one alias head (`node`) is already a registered
/// head on its own, so including `ALIASES` here is free insurance against a
/// future alias whose head is not itself a command.
fn reserved_heads(registry: &Registry) -> std::collections::HashSet<&'static str> {
    let mut heads: std::collections::HashSet<&'static str> = registry.commands().map(|c| c.path[0]).collect();
    heads.extend(ALIASES.iter().map(|(from, _)| from[0]));
    heads.insert("help");
    heads
}

/// Step 4 of the module doc's EXTERNAL PROBE: is `argv[0]` an eligible name
/// (present, not `-`-leading, not a reserved head), and if so, is
/// `<bin_name>-<name>` an executable file on `PATH`? A hit resolves to the
/// absolute path to spawn; anything else (ineligible, or nothing found)
/// returns `None` so `parse`'s ordinary path runs completely unchanged — this
/// is a filter in front of that path, never a replacement for it.
fn probe_external(argv: &[String], bin_name: &str, registry: &Registry) -> Option<PathBuf> {
    let name = argv.first()?;
    if name.starts_with('-') {
        return None;
    }
    if reserved_heads(registry).contains(name.as_str()) {
        return None;
    }
    crate::bin::resolve_executable_on_path(&format!("{bin_name}-{name}"))
}

/// Spawn a resolved external command with `args` verbatim and
/// `Stdio::inherit()` throughout (the `spawn_with_secret` precedent,
/// `crates/secrets/src/client.rs`), returning the CHILD's own exit code
/// unchanged — never aoide's own exit-code vocabulary. Audits ONE line at
/// LAUNCH (right after `spawn` succeeds, before `wait`) so a plugin that
/// never exits — a watcher, a TUI — still leaves a record, the same
/// audit-then-block shape `a2a serve` uses for the same reason. The
/// message carries the resolved path and the argument COUNT, never the
/// argument VALUES (`crates/secrets/src/client.rs`'s "never argv" rule) —
/// `aoide deploy --token abc` must never put `abc` in a world-readable log.
/// A spawn failure (bad binary, permission denied) is audited `"error"` and
/// exits [`exit::ERROR`], reason on stderr, same shape `session_conduct`
/// (`crates/conduct/src/graph/conduct.rs`) uses for its own spawn failure.
fn run_external(path: &std::path::Path, args: &[String], bin_name: &str, name: &str) -> i32 {
    let log = default_audit_log();
    let command = format!("external.{name}");
    let mut child = match Process::new(path)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            let _ = audit(&log, Door::Cli, EventClass::Audit, &command, "error", &format!("{}: {e}", path.display()));
            eprintln!("{bin_name} {name}: spawning `{}`: {e}", path.display());
            return exit::ERROR;
        }
    };
    let _ = audit(
        &log,
        Door::Cli,
        EventClass::Audit,
        &command,
        "ok",
        &format!("{} ({} arg{})", path.display(), args.len(), if args.len() == 1 { "" } else { "s" }),
    );
    match child.wait() {
        Ok(status) => status.code().unwrap_or(exit::ERROR),
        Err(_) => exit::ERROR,
    }
}

/// Exit code for a usage error surfaced during parsing.
pub const USAGE_EXIT: i32 = exit::USAGE;

/// Did argv contain `--json` anywhere? (used before full parse for errors).
fn wants_json(argv: &[String]) -> bool {
    argv.iter().any(|a| a == "--json" || a == "--json=true")
}

/// Run one invocation end-to-end: parse against `registry`, offer the result
/// to `special` first, else dispatch generically through `dispatch` and
/// render the [`Outcome`]. Returns the process exit code.
///
/// `special` is called with the parsed [`Invocation`] and the `--json` flag
/// AFTER a successful parse — see the module doc for why it exists.
/// `Some(code)` short-circuits `run` with that exit code (the special case
/// already did its own printing); `None` falls through to the uniform
/// dispatch+render path below, unchanged from every other command.
pub fn run(
    argv: &[String],
    door: Door,
    bin_name: &str,
    registry: &Registry,
    dispatch: fn(&Invocation) -> Outcome,
    special: impl FnOnce(&Invocation, bool) -> Option<i32>,
) -> i32 {
    if let Some(path) = probe_external(argv, bin_name, registry) {
        return run_external(&path, &argv[1..], bin_name, &argv[0]);
    }

    let json = wants_json(argv);
    let (rest, mode) = match style::take_color(argv) {
        Ok(v) => v,
        Err(bad) => {
            let refusal = Outcome::refuse(
                "color",
                Kind::Usage,
                format!("`--color {bad}` is not a color mode"),
                format!("the modes are {}", Color::VALUES.join(", ")),
                Fix::Run(format!("{bin_name} --color={}", Color::VALUES[0])),
            );
            return print(&refusal, json, Style::for_stream(Color::Auto, json, Stream::Err), false);
        }
    };
    let width = style::width();
    let for_stream = |stream| Term { style: Style::for_stream(mode, json, stream), width };

    let (inv, json) = match parse_with(&rest, door, bin_name, registry, &Term { style: Style::OFF, width }) {
        Ok(v) => v,
        Err(o) => {
            let t = for_stream(if o.status == Status::Ok { Stream::Out } else { Stream::Err });
            let o = if t.style == Style::OFF { o } else { parse_with(&rest, door, bin_name, registry, &t).err().unwrap_or(o) };
            return print(&o, json, t.style, true);
        }
    };

    if let Some(code) = special(&inv, json) {
        return code;
    }

    let outcome = dispatch(&inv);
    let stream = if outcome.status == Status::Ok { Stream::Out } else { Stream::Err };
    print(&outcome, json, Style::for_stream(mode, json, stream), false)
}

/// Write an outcome where its status says (ok on stdout, the rest on stderr)
/// and return its exit code. A parse result that is informational (`--help`,
/// a group's page) prints its message raw in text mode; `--json` always
/// prints the envelope.
fn print(o: &Outcome, json: bool, st: Style, raw_ok: bool) -> i32 {
    let (body, code) = o.render_styled(json, st);
    if code == exit::OK {
        println!("{}", if raw_ok && !json { &o.message } else { &body });
    } else {
        eprintln!("{body}");
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::Status;
    use crate::registry::{Arg, Flag, JSON_FLAG};

    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    fn noop(_inv: &Invocation) -> Outcome {
        Outcome::ok("noop", "ok")
    }

    /// Point `PATH` at a fresh, empty scratch dir for the duration of a
    /// PATH-touching test, returning the dir (for planting fake plugins) and
    /// the real `PATH` to restore afterward. Callers must hold
    /// `crate::bin::path_test_lock()` for the whole test — `std::env::
    /// set_var` is process-global (crates/AGENTS.md). Every existing
    /// `parse`/`run` test that never plants a plugin still needs this once
    /// the external probe exists: `usage_root` (root `--help`/bare argv) now
    /// reads the AMBIENT `PATH` for its own trailing section, so a bare
    /// `parse(&argv(&[]), ..)` must not depend on what happens to sit there.
    fn scoped_empty_path(tag: &str) -> (PathBuf, Option<std::ffi::OsString>) {
        let saved = std::env::var_os("PATH");
        let dir = std::env::temp_dir().join(format!("aoide_protocol_door_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("PATH", &dir);
        (dir, saved)
    }

    fn restore_path(dir: PathBuf, saved: Option<std::ffi::OsString>) {
        match saved {
            Some(v) => std::env::set_var("PATH", v),
            None => std::env::remove_var("PATH"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Write `contents` to `dir.join(name)` and make it a program this host
    /// will run.
    ///
    /// Unix: the execute bit, via `crate::bin::mark_executable` (the shared
    /// `PermissionsExt` dance). Windows: a spawnable suffix, because that —
    /// not any bit — is what both this crate's resolver and `Command`'s own
    /// name lookup ask for. So the file lands as `<name>.exe`, and the
    /// caller gets the path it must expect back. The two tests below that
    /// actually RUN their shim stay `cfg(unix)`: their body is a `#!/bin/sh`
    /// script, and a `.exe` this arm can write is never one a shell script
    /// could be.
    #[cfg(unix)]
    fn write_executable(dir: &std::path::Path, name: &str, contents: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, contents).unwrap();
        crate::bin::mark_executable(&path);
        path
    }

    #[cfg(windows)]
    fn write_executable(dir: &std::path::Path, name: &str, contents: &str) -> PathBuf {
        let path = dir.join(format!("{name}.exe"));
        std::fs::write(&path, contents).unwrap();
        path
    }

    /// A tiny hand-built registry standing in for a real crate's
    /// `commands::all()` — protocol cannot depend on the domain crates that
    /// register the real command tree, so these tests exercise the parser's
    /// own logic (greedy match, flags, `--help`) against a minimal tree
    /// rather than the golden 87/41 paths (those are covered by the cli/lyra
    /// crates' own tests, which call through to this same code).
    fn test_registry() -> Registry {
        let mut r = Registry::new();
        r.insert(Command {
            path: &["graph", "view"],
            summary: "View the project/session graph.",
            args: &[],
            flags: &[JSON_FLAG, Flag { name: "focus", ty: "string", description: "Focus a node.", ..Flag::NONE }],
            gated: false,
            implemented: true,
            internal: false,
            exit_codes: (),
            examples: &[],
            handler: noop,
            available: || true,
            ..Command::BLANK
        });
        // A bare parent path (`graph`) that is ALSO the prefix of a longer,
        // separately-registered sibling (`graph link`) — the R1 graph-prefix
        // cutover shape: `graph` renders, `graph link` records an edge, and
        // the two must coexist without either shadowing the other.
        r.insert(Command {
            path: &["graph"],
            summary: "Render the project/session graph.",
            args: &[],
            flags: &[JSON_FLAG],
            gated: false,
            implemented: true,
            internal: false,
            exit_codes: (),
            examples: &[],
            handler: noop,
            available: || true,
            ..Command::BLANK
        });
        r.insert(Command {
            path: &["graph", "link"],
            summary: "Record a spawned-by edge.",
            args: &[
                Arg { name: "child", ty: "string", required: true, description: "Child session id.", ..Arg::NONE },
                Arg { name: "parent", ty: "string", required: true, description: "Parent session id.", ..Arg::NONE },
            ],
            flags: &[JSON_FLAG],
            gated: false,
            implemented: true,
            internal: false,
            exit_codes: (),
            examples: &[],
            handler: noop,
            available: || true,
            ..Command::BLANK
        });
        r.insert(Command {
            path: &["graph", "project", "add"],
            summary: "Register a project anchor root.",
            args: &[Arg { name: "name", ty: "string", required: true, description: "Anchor name.", ..Arg::NONE }],
            flags: &[JSON_FLAG],
            gated: false,
            implemented: true,
            internal: false,
            exit_codes: (),
            examples: &[],
            handler: noop,
            available: || true,
            ..Command::BLANK
        });
        r.insert(Command {
            path: &["graph", "session", "start"],
            summary: "Register a running session.",
            args: &[],
            flags: &[JSON_FLAG, Flag { name: "id", ty: "string", description: "Session id.", ..Flag::NONE }],
            gated: false,
            implemented: true,
            internal: false,
            exit_codes: (),
            examples: &[],
            handler: noop,
            available: || true,
            ..Command::BLANK
        });
        r.insert(Command {
            path: &["node", "remove"],
            summary: "Deregister a node.",
            args: &[Arg { name: "name", ty: "string", required: true, description: "Node name.", ..Arg::NONE }],
            flags: &[JSON_FLAG],
            gated: false,
            implemented: true,
            internal: false,
            exit_codes: (),
            examples: &[],
            handler: noop,
            available: || true,
            ..Command::BLANK
        });
        // The #111 filed shape: a command with positional args plus a
        // declared-bool flag AND a declared-valued flag, mirroring the real
        // `node add <name> <url> [--no-verify] [--via …]`.
        r.insert(Command {
            path: &["node", "add"],
            summary: "Register a node.",
            args: &[
                Arg { name: "name", ty: "string", required: true, description: "Node name.", ..Arg::NONE },
                Arg { name: "url", ty: "string", required: true, description: "Node URL.", ..Arg::NONE },
            ],
            flags: &[
                JSON_FLAG,
                Flag { name: "no-verify", ty: "bool", description: "Skip the card fetch.", ..Flag::NONE },
                Flag { name: "via", ty: "string", description: "Tunnel spec.", ..Flag::NONE },
            ],
            gated: false,
            implemented: true,
            internal: false,
            exit_codes: (),
            examples: &[],
            handler: noop,
            available: || true,
            ..Command::BLANK
        });
        r
    }

    #[test]
    fn help_flag_prints_subcommand_usage_at_exit_zero() {
        let reg = test_registry();
        let err = parse(&argv(&["graph", "view", "--help"]), Door::Cli, "aoide", &reg).unwrap_err();
        assert_eq!(err.status, Status::Ok, "--help is informational, exit 0");
        assert_eq!(err.render(false).1, exit::OK);
        assert!(err.message.contains("usage: aoide graph view"));
        assert!(err.message.contains("--focus"), "lists the command's flags");
    }

    #[test]
    fn unknown_flag_is_a_usage_error_naming_the_offender() {
        let reg = test_registry();
        let err = parse(&argv(&["graph", "view", "--bogus"]), Door::Cli, "aoide", &reg).unwrap_err();
        assert_eq!(err.status, Status::Usage, "unknown flag → exit 2");
        assert_eq!(err.render(false).1, exit::USAGE);
        assert!(err.message.contains("--bogus"));
    }

    #[test]
    fn known_flags_still_parse() {
        let reg = test_registry();
        let (inv, _) = parse(&argv(&["graph", "view", "--focus", "session:x"]), Door::Cli, "aoide", &reg).unwrap();
        assert_eq!(inv.path, vec!["graph", "view"]);
        assert_eq!(inv.flags.get("focus").map(String::as_str), Some("session:x"));
    }

    /// Regression (#48): a flag placed BEFORE the final path segment, whose
    /// value collides with that segment's name. `graph session --id start`
    /// used to parse SILENTLY as path=`graph.session.start`,
    /// flags={id:"true"} — the value swallowed as a path segment, the flag
    /// mis-booleaned. The ordering is genuinely ambiguous, so it must be a
    /// loud usage error naming the flag, the value, and the unambiguous
    /// spelling.
    #[test]
    fn a_flag_before_the_full_path_whose_value_collides_with_a_segment_fails_loudly() {
        let reg = test_registry();
        let err = parse(&argv(&["graph", "session", "--id", "start"]), Door::Cli, "aoide", &reg).unwrap_err();
        assert_eq!(err.status, Status::Usage, "ambiguous ordering → exit 2");
        assert_eq!(err.render(false).1, exit::USAGE);
        assert!(err.message.contains("`--id start`"), "names the flag+value: {}", err.message);
        assert!(err.message.contains("`graph session start`"), "names the colliding command: {}", err.message);
        assert!(
            err.message.contains("aoide graph session start --id <value>"),
            "suggests the flags-after-path spelling: {}",
            err.message
        );
    }

    /// The same value AFTER the full command path is not ambiguous — nothing
    /// extends `graph session start`, so `--id start` binds normally.
    #[test]
    fn the_colliding_value_after_the_full_path_still_binds_as_a_flag_value() {
        let reg = test_registry();
        let (inv, _) = parse(&argv(&["graph", "session", "start", "--id", "start"]), Door::Cli, "aoide", &reg).unwrap();
        assert_eq!(inv.path, vec!["graph", "session", "start"]);
        assert_eq!(inv.flags.get("id").map(String::as_str), Some("start"));
    }

    /// The R1 graph-prefix cutover shape: a bare parent path (`graph`) that
    /// is ALSO a registered leaf, coexisting with a longer sibling (`graph
    /// link`) that extends the same first segment. Longest-match must prefer
    /// the 2-segment path over the 1-segment one when both are spelled out,
    /// and the bare path must still resolve to itself when nothing follows.
    #[test]
    fn a_bare_parent_path_and_a_longer_sibling_subcommand_coexist() {
        let reg = test_registry();

        let (inv, _) = parse(&argv(&["graph", "link", "c1", "p1"]), Door::Cli, "aoide", &reg).unwrap();
        assert_eq!(inv.path, vec!["graph", "link"], "the longer sibling wins over the bare parent");
        assert_eq!(inv.args, vec!["c1", "p1"]);

        let (inv, json) = parse(&argv(&["graph"]), Door::Cli, "aoide", &reg).unwrap();
        assert_eq!(inv.path, vec!["graph"]);
        assert!(inv.args.is_empty());
        assert!(!json);

        let (inv, json) = parse(&argv(&["graph", "--json"]), Door::Cli, "aoide", &reg).unwrap();
        assert_eq!(inv.path, vec!["graph"]);
        assert!(json);
    }

    /// The boundary this coexistence creates: an unregistered CHILD of the
    /// zero-arg bare parent must not silently resolve as the parent with a
    /// stray ignored positional — it must be a loud unknown-command error,
    /// same as any other typo. `graph` declares no positional args (unlike
    /// `conduct`/`graph spawn`/`graph send`, which intentionally consume
    /// everything after their first required arg), so this is the ONE case
    /// where leftover positionals must reject rather than pass through.
    #[test]
    fn an_unregistered_child_of_a_zero_arg_parent_is_a_loud_unknown_command() {
        let reg = test_registry();
        let err = parse(&argv(&["graph", "nonsense"]), Door::Cli, "aoide", &reg).unwrap_err();
        assert_eq!(
            err.status,
            Status::Usage,
            "must not silently resolve to the bare parent with a stray positional"
        );
        assert!(
            err.message.contains("unknown command: `graph nonsense`"),
            "{}",
            err.message
        );
    }

    /// A command that DOES declare a positional arg (`graph link`, two
    /// required args) is unaffected by the zero-arg overflow check above —
    /// its own declared args still bind normally, and a command like
    /// `conduct`/`graph spawn` that deliberately consumes MORE than its one
    /// declared arg (the wrapped command's own argv) must keep doing so. The
    /// zero-arg registry check must never fire for a command with `args`.
    #[test]
    fn a_command_declaring_positional_args_is_unaffected_by_the_zero_arg_check() {
        let reg = test_registry();
        let (inv, _) = parse(&argv(&["graph", "project", "add", "aoide", "extra"]), Door::Cli, "aoide", &reg).unwrap();
        assert_eq!(inv.path, vec!["graph", "project", "add"]);
        assert_eq!(inv.args, vec!["aoide", "extra"]);
    }

    /// `node rm <name>` is an ergonomic alias for `node remove <name>`,
    /// resolved at the parser level (`ALIASES`/`resolve_aliases`) — the
    /// invocation it produces must be byte-identical to typing the canonical
    /// path out, and the trailing arg (the node name) must survive the
    /// rewrite untouched.
    #[test]
    fn node_rm_is_a_parser_level_alias_for_node_remove() {
        let reg = test_registry();
        let (aliased, _) = parse(&argv(&["node", "rm", "alice"]), Door::Cli, "aoide", &reg).unwrap();
        let (canonical, _) = parse(&argv(&["node", "remove", "alice"]), Door::Cli, "aoide", &reg).unwrap();
        assert_eq!(aliased.path, vec!["node", "remove"], "the alias resolves to the CANONICAL path, never a `node.rm` path of its own");
        assert_eq!(aliased.path, canonical.path);
        assert_eq!(aliased.args, canonical.args);
        assert_eq!(aliased.args, vec!["alice"]);
    }

    /// Regression (#111, filed off the M3 review): a declared-BOOL flag given
    /// before the positional args swallowed the following token as its value —
    /// `node add --no-verify alice url` parsed as flags={no-verify:"alice"},
    /// args=["url"], silently dropping a positional. The registry declares the
    /// flag's type, so the parser must never let a bool consume a value token.
    #[test]
    fn a_bool_flag_before_positionals_never_swallows_the_next_token() {
        let reg = test_registry();
        let (inv, _) = parse(
            &argv(&["node", "add", "--no-verify", "alice", "http://h:7466"]),
            Door::Cli,
            "aoide",
            &reg,
        )
        .unwrap();
        assert_eq!(inv.path, vec!["node", "add"]);
        assert_eq!(inv.args, vec!["alice", "http://h:7466"], "both positionals survive");
        assert_eq!(inv.flags.get("no-verify").map(String::as_str), Some("true"));
    }

    /// The neighboring shapes around the #111 fix: a bool flag at the end and
    /// between positionals binds bare either way, with every positional kept.
    #[test]
    fn a_bool_flag_at_the_end_or_between_positionals_binds_bare() {
        let reg = test_registry();

        let (inv, _) = parse(
            &argv(&["node", "add", "alice", "http://h:7466", "--no-verify"]),
            Door::Cli,
            "aoide",
            &reg,
        )
        .unwrap();
        assert_eq!(inv.args, vec!["alice", "http://h:7466"]);
        assert_eq!(inv.flags.get("no-verify").map(String::as_str), Some("true"));

        let (inv, _) = parse(
            &argv(&["node", "add", "alice", "--no-verify", "http://h:7466"]),
            Door::Cli,
            "aoide",
            &reg,
        )
        .unwrap();
        assert_eq!(inv.args, vec!["alice", "http://h:7466"]);
        assert_eq!(inv.flags.get("no-verify").map(String::as_str), Some("true"));
    }

    /// A bool flag placed before the command path is even complete resolves
    /// through the same candidate-aware type lookup — the following token
    /// stays a path segment, never the flag's value.
    #[test]
    fn a_bool_flag_before_the_path_is_complete_leaves_the_segment_alone() {
        let reg = test_registry();
        let (inv, _) = parse(
            &argv(&["node", "--no-verify", "add", "alice", "http://h:7466"]),
            Door::Cli,
            "aoide",
            &reg,
        )
        .unwrap();
        assert_eq!(inv.path, vec!["node", "add"]);
        assert_eq!(inv.args, vec!["alice", "http://h:7466"]);
        assert_eq!(inv.flags.get("no-verify").map(String::as_str), Some("true"));
    }

    /// The converse must be untouched by the #111 fix: a genuinely VALUED
    /// flag before the positionals still consumes exactly its one value.
    #[test]
    fn a_valued_flag_before_positionals_still_consumes_its_value() {
        let reg = test_registry();
        let (inv, _) = parse(
            &argv(&["node", "add", "--via", "ssh://h:22", "alice", "http://h:7466"]),
            Door::Cli,
            "aoide",
            &reg,
        )
        .unwrap();
        assert_eq!(inv.args, vec!["alice", "http://h:7466"]);
        assert_eq!(inv.flags.get("via").map(String::as_str), Some("ssh://h:22"));
    }

    /// A flag declared `"bool"` by one candidate command and valued by a
    /// sibling, with a consumable token following, reads both ways — the
    /// parser refuses loudly (the #48 convention) instead of guessing, and
    /// names both unambiguous spellings.
    #[test]
    fn a_flag_declared_bool_and_valued_by_sibling_commands_fails_loudly() {
        let mut r = Registry::new();
        let mk = |path: &'static [&'static str], ty: &'static str| Command {
            path,
            summary: "Test.",
            args: &[Arg { name: "x", ty: "string", required: false, description: "X.", ..Arg::NONE }],
            flags: if ty == "bool" {
                &[JSON_FLAG, Flag { name: "force", ty: "bool", description: "F.", ..Flag::NONE }]
            } else {
                &[JSON_FLAG, Flag { name: "force", ty: "string", description: "F.", ..Flag::NONE }]
            },
            gated: false,
            implemented: true,
            internal: false,
            exit_codes: (),
            examples: &[],
            handler: noop,
            available: || true,
            ..Command::BLANK
        };
        r.insert(mk(&["thing", "one"], "bool"));
        r.insert(mk(&["thing", "two"], "string"));

        let err = parse(&argv(&["thing", "--force", "val"]), Door::Cli, "aoide", &r).unwrap_err();
        assert_eq!(err.status, Status::Usage, "mixed declaration + consumable token → exit 2");
        assert!(err.message.contains("--force"), "{}", err.message);
        assert!(err.message.contains("--force=val"), "offers the valued spelling: {}", err.message);

        // With nothing consumable following, only the bare reading exists.
        let (inv, _) = parse(&argv(&["thing", "one", "--force"]), Door::Cli, "aoide", &r).unwrap();
        assert_eq!(inv.flags.get("force").map(String::as_str), Some("true"));
    }

    #[test]
    fn a_typo_gets_a_did_you_mean_suggestion() {
        let reg = test_registry();
        let err = parse(&argv(&["graph", "vie"]), Door::Cli, "aoide", &reg).unwrap_err();
        assert_eq!(err.status, Status::Usage);
        assert!(err.message.contains("unknown command: `graph vie`"));
        assert!(err.message.contains("did you mean:\n  aoide graph view"));
    }

    /// `bin_name` is not cosmetic: a second binary (lyra, P-A5) must see its
    /// own name in every usage/help/did-you-mean string, never `aoide`'s.
    #[test]
    fn bin_name_names_the_invoking_binary_everywhere() {
        // Scoped: `usage_root` (the bare-argv branch below) now reads the
        // ambient `PATH` for its own trailing external section (task #138) —
        // pin it to empty so this test's `!contains("aoide")` assertion
        // never depends on what the real machine's PATH happens to hold.
        let _guard = crate::bin::path_test_lock().lock().unwrap();
        let (dir, saved) = scoped_empty_path("bin_name_everywhere");

        let reg = test_registry();

        let root = parse(&argv(&[]), Door::Cli, "lyra", &reg).unwrap_err();
        assert!(root.message.contains("usage: lyra <command>"), "{}", root.message);
        assert!(!root.message.contains("aoide"), "{}", root.message);

        let sub = parse(&argv(&["graph", "view", "--help"]), Door::Cli, "lyra", &reg).unwrap_err();
        assert!(sub.message.contains("usage: lyra graph view"), "{}", sub.message);

        let unknown = parse(&argv(&["graph", "vie"]), Door::Cli, "lyra", &reg).unwrap_err();
        assert!(
            unknown.message.contains("did you mean:\n  lyra graph view"),
            "{}",
            unknown.message
        );
        assert!(unknown.message.contains("run 'lyra --help'"), "{}", unknown.message);

        restore_path(dir, saved);
    }

    // ── external-command probe (task #138) ──────────────────────────────────

    /// A registered head must never probe `PATH`, no matter what sits
    /// there — the reservation (module doc's step 3) wins structurally, not
    /// by luck of what happens to be installed.
    #[test]
    fn a_registered_head_never_probes_path_even_with_a_matching_binary_present() {
        let _guard = crate::bin::path_test_lock().lock().unwrap();
        let (dir, saved) = scoped_empty_path("reserved_head");
        write_executable(&dir, "aoide-graph", "#!/bin/sh\nexit 0\n");

        let reg = test_registry();
        assert!(
            probe_external(&argv(&["graph", "vie"]), "aoide", &reg).is_none(),
            "`graph` is a registered head — it must never reach the PATH probe"
        );

        restore_path(dir, saved);
    }

    /// A typo of a registered head (not itself a reserved head) DOES reach
    /// the PATH probe, misses on an empty `PATH`, and falls through to the
    /// exact same taught did-you-mean error as before the probe existed —
    /// the probe is a filter in front of `parse`'s own path, never a
    /// replacement for it.
    #[test]
    fn a_typo_of_a_registered_head_falls_through_to_did_you_mean() {
        let _guard = crate::bin::path_test_lock().lock().unwrap();
        let (dir, saved) = scoped_empty_path("typo_head");

        let reg = test_registry();
        assert!(
            probe_external(&argv(&["grph", "view"]), "aoide", &reg).is_none(),
            "no `aoide-grph` exists on this scoped PATH"
        );
        let err = parse(&argv(&["grph", "view"]), Door::Cli, "aoide", &reg).unwrap_err();
        assert_eq!(err.status, Status::Usage);
        assert!(err.message.contains("unknown command: `grph view`"), "{}", err.message);
        assert!(err.message.contains("did you mean:\n  aoide graph view"), "{}", err.message);

        restore_path(dir, saved);
    }

    /// An eligible, non-reserved name with a matching executable on `PATH`
    /// resolves to that executable's absolute path.
    #[test]
    fn an_eligible_name_with_a_matching_binary_resolves_to_its_absolute_path() {
        let _guard = crate::bin::path_test_lock().lock().unwrap();
        let (dir, saved) = scoped_empty_path("eligible_hit");
        let plugin = write_executable(&dir, "aoide-deploy", "#!/bin/sh\nexit 0\n");

        let reg = test_registry();
        assert_eq!(probe_external(&argv(&["deploy", "--env", "prod"]), "aoide", &reg), Some(plugin));

        restore_path(dir, saved);
    }

    /// `--help`/`-h`-leading argv never probes (module doc's step 2), even
    /// when a same-named executable exists — flags are never mistaken for a
    /// plugin name.
    #[test]
    fn a_flag_leading_argv_never_probes_path() {
        let _guard = crate::bin::path_test_lock().lock().unwrap();
        let (dir, saved) = scoped_empty_path("flag_leading");
        write_executable(&dir, "aoide---help", "#!/bin/sh\nexit 0\n");

        let reg = test_registry();
        assert!(probe_external(&argv(&["--help"]), "aoide", &reg).is_none());
        assert!(probe_external(&argv(&[]), "aoide", &reg).is_none(), "bare argv never probes");

        restore_path(dir, saved);
    }

    /// End-to-end through `run`: a hit spawns the child with `argv[1..]`
    /// VERBATIM and returns the CHILD's own exit code unchanged, never
    /// aoide's own vocabulary.
    #[cfg(unix)]
    #[test]
    fn run_spawns_the_resolved_external_command_with_argv_verbatim_and_returns_its_exit_code() {
        let _guard = crate::bin::path_test_lock().lock().unwrap();
        let (dir, saved) = scoped_empty_path("run_spawn");
        write_executable(
            &dir,
            "aoide-deploy",
            "#!/bin/sh\n[ \"$1\" = \"--env\" ] && [ \"$2\" = \"prod\" ] && [ \"$3\" = \"x\" ] && exit 42\nexit 99\n",
        );

        let reg = test_registry();
        let code = run(&argv(&["deploy", "--env", "prod", "x"]), Door::Cli, "aoide", &reg, noop, |_inv, _json| None);
        assert_eq!(code, 42, "argv[1..] must reach the child verbatim");

        restore_path(dir, saved);
    }

    /// A spawn failure (nonexistent binary resolved a moment ago, then
    /// removed — the realistic TOCTOU shape) exits `exit::ERROR`, not a
    /// panic and not aoide's usage/not-implemented vocabulary.
    #[test]
    fn run_external_reports_a_spawn_failure_as_a_plain_error_exit() {
        // Scoped: the error arm still audits one line, and this test must
        // never touch the real machine's `~/.aoide/log`.
        let _guard = crate::bin::path_test_lock().lock().unwrap();
        let log = std::env::temp_dir().join(format!("aoide_protocol_door_spawn_failure_{}.log", std::process::id()));
        let _ = std::fs::remove_file(&log);
        let saved_log = std::env::var_os("AOIDE_AUDIT_LOG");
        std::env::set_var("AOIDE_AUDIT_LOG", &log);

        let path = std::path::PathBuf::from("/definitely/not/a/real/path/aoide-deploy");
        let code = run_external(&path, &[], "aoide", "deploy");
        assert_eq!(code, exit::ERROR);

        match saved_log {
            Some(v) => std::env::set_var("AOIDE_AUDIT_LOG", v),
            None => std::env::remove_var("AOIDE_AUDIT_LOG"),
        }
        let _ = std::fs::remove_file(&log);
    }

    /// The audit line lands at LAUNCH: resolved path + argument COUNT only —
    /// never an argument VALUE (`crates/secrets/src/client.rs`'s "never
    /// argv" rule). A secret-looking flag value must never reach the log.
    /// The Windows half of the spawning pair is missing on purpose, and the
    /// reason is the same one `write_executable`'s own doc gives: the shim
    /// these two tests spawn is a `#!/bin/sh` script. What they assert about
    /// `run` (argv verbatim, the child's exit code unchanged, the audit line's
    /// path-and-count shape) is platform-independent policy, and the
    /// resolution it rides on is covered natively by `bin.rs`'s
    /// `windows_*` tests; only the child these two need cannot exist here.
    #[cfg(unix)]
    #[test]
    fn run_audits_the_launch_with_path_and_arg_count_never_argument_values() {
        let _guard = crate::bin::path_test_lock().lock().unwrap();
        let (dir, saved) = scoped_empty_path("run_audit");
        let plugin = write_executable(&dir, "aoide-deploy", "#!/bin/sh\nexit 0\n");

        let log = dir.join("audit.log");
        let saved_log = std::env::var_os("AOIDE_AUDIT_LOG");
        std::env::set_var("AOIDE_AUDIT_LOG", &log);

        let reg = test_registry();
        let code = run(
            &argv(&["deploy", "--token", "super-secret-value"]),
            Door::Cli,
            "aoide",
            &reg,
            noop,
            |_inv, _json| None,
        );
        assert_eq!(code, 0);

        let contents = std::fs::read_to_string(&log).unwrap();
        assert!(contents.contains("\"command\":\"external.deploy\""), "{contents}");
        assert!(contents.contains(&plugin.display().to_string()), "{contents}");
        assert!(contents.contains("2 arg"), "carries the argument COUNT: {contents}");
        assert!(!contents.contains("super-secret-value"), "must never carry an argument VALUE: {contents}");

        match saved_log {
            Some(v) => std::env::set_var("AOIDE_AUDIT_LOG", v),
            None => std::env::remove_var("AOIDE_AUDIT_LOG"),
        }
        restore_path(dir, saved);
    }

    #[test]
    fn run_falls_through_to_dispatch_when_special_declines() {
        let reg = test_registry();
        let code = run(&argv(&["graph", "view"]), Door::Cli, "aoide", &reg, noop, |_inv, _json| None);
        assert_eq!(code, exit::OK);
    }

    #[test]
    fn run_short_circuits_when_special_claims_the_invocation() {
        let reg = test_registry();
        let code = run(&argv(&["graph", "view"]), Door::Cli, "aoide", &reg, noop, |_inv, _json| Some(exit::NOT_IMPLEMENTED));
        assert_eq!(code, exit::NOT_IMPLEMENTED);
    }

    // ── help + typo behaviour ───────────────────────────────────────────────

    #[test]
    fn help_is_an_alias_of_dash_dash_help() {
        let _guard = crate::bin::path_test_lock().lock().unwrap();
        let (dir, saved) = scoped_empty_path("help_alias");
        let reg = test_registry();
        let via_word = parse(&argv(&["help", "graph", "view"]), Door::Cli, "aoide", &reg).unwrap_err();
        let via_flag = parse(&argv(&["graph", "view", "--help"]), Door::Cli, "aoide", &reg).unwrap_err();
        assert_eq!(via_word.status, Status::Ok);
        assert_eq!(via_word.message, via_flag.message);
        let root = parse(&argv(&["help"]), Door::Cli, "aoide", &reg).unwrap_err();
        assert_eq!(root.status, Status::Ok);
        assert!(root.message.contains("usage: aoide <command>"), "{}", root.message);
        restore_path(dir, saved);
    }

    #[test]
    fn group_help_prints_the_group_not_the_root_list() {
        let reg = test_registry();
        let err = parse(&argv(&["graph", "project", "--help"]), Door::Cli, "aoide", &reg).unwrap_err();
        assert_eq!(err.status, Status::Ok);
        assert!(err.message.starts_with("usage: aoide graph project <command>"), "{}", err.message);
        assert!(err.message.contains("\n  add <name>"), "{}", err.message);
        assert!(!err.message.contains("node remove"), "only the group's own commands: {}", err.message);
    }

    #[test]
    fn a_command_that_is_also_a_group_lists_its_subcommands_under_its_own_usage() {
        let reg = test_registry();
        let err = parse(&argv(&["graph", "--help"]), Door::Cli, "aoide", &reg).unwrap_err();
        assert!(err.message.starts_with("usage: aoide graph"), "{}", err.message);
        assert!(err.message.contains("subcommands:\n  view"), "{}", err.message);
    }

    #[test]
    fn an_unknown_flag_gets_the_close_match_and_one_line_of_usage_not_the_help_block() {
        let reg = test_registry();
        let err = parse(&argv(&["graph", "view", "--focuss", "n1"]), Door::Cli, "aoide", &reg).unwrap_err();
        assert_eq!(err.status, Status::Usage);
        let text = err.render(false).0;
        assert!(text.contains("unrecognized flag `--focuss` for `aoide graph view`"), "{text}");
        assert!(text.contains("did you mean `--focus`?"), "{text}");
        assert!(text.contains("fix: aoide graph view --focus n1"), "{text}");
        assert!(!text.contains("flags:"), "no full help block: {text}");

        let far = parse(&argv(&["graph", "view", "--zzqqxx"]), Door::Cli, "aoide", &reg).unwrap_err();
        let text = far.render(false).0;
        assert!(text.contains("it takes --focus"), "{text}");
        assert!(text.contains("fix: aoide graph view --help"), "{text}");
    }

    #[test]
    fn a_typo_in_a_group_segment_lands_on_the_real_path() {
        let reg = test_registry();
        let err = parse(&argv(&["grap", "view"]), Door::Cli, "aoide", &reg).unwrap_err();
        assert!(err.message.contains("did you mean:\n  aoide graph view"), "{}", err.message);
        let both = parse(&argv(&["nodee", "remov", "alice"]), Door::Cli, "aoide", &reg).unwrap_err();
        assert!(both.message.contains("aoide node remove alice"), "{}", both.message);
        let group = parse(&argv(&["nodee"]), Door::Cli, "aoide", &reg).unwrap_err();
        assert!(group.message.contains("did you mean:\n  aoide node\n"), "{}", group.message);
    }

    #[test]
    fn nothing_close_lists_the_valid_choices_at_the_point_it_went_wrong() {
        let reg = test_registry();
        let deep = parse(&argv(&["graph", "nonsense", "extra"]), Door::Cli, "aoide", &reg).unwrap_err();
        assert!(deep.message.contains("`graph` has: view, link, project, session"), "{}", deep.message);
        let root = parse(&argv(&["zzqq"]), Door::Cli, "aoide", &reg).unwrap_err();
        assert!(root.message.contains("graph, node"), "{}", root.message);
    }

    #[test]
    fn bare_help_and_dash_dash_help_are_one_overview_at_exit_zero() {
        let _guard = crate::bin::path_test_lock().lock().unwrap();
        let (dir, saved) = scoped_empty_path("bare_overview");
        let reg = test_registry();
        let bare = parse(&argv(&[]), Door::Cli, "aoide", &reg).unwrap_err();
        let word = parse(&argv(&["help"]), Door::Cli, "aoide", &reg).unwrap_err();
        let flag = parse(&argv(&["--help"]), Door::Cli, "aoide", &reg).unwrap_err();
        for o in [&bare, &word, &flag] {
            assert_eq!((o.status, o.render(false).1), (Status::Ok, exit::OK));
        }
        assert_eq!(bare.message, word.message);
        assert_eq!(bare.message, flag.message);
        restore_path(dir, saved);
    }

    #[test]
    fn a_group_named_exactly_lists_itself_at_exit_zero_and_a_longer_miss_stays_a_usage_error() {
        let reg = test_registry();
        let group = parse(&argv(&["graph", "project"]), Door::Cli, "aoide", &reg).unwrap_err();
        assert_eq!((group.status, group.render(false).1), (Status::Ok, exit::OK));
        assert!(group.message.starts_with("usage: aoide graph project <command>"), "{}", group.message);
        let miss = parse(&argv(&["graph", "projet"]), Door::Cli, "aoide", &reg).unwrap_err();
        assert_eq!(miss.render(false).1, exit::USAGE);
    }

    #[test]
    fn suggestions_are_bold_only_when_the_term_is_styled() {
        let reg = test_registry();
        let styled = Term { style: Style::ON, width: 80 };
        let plain = parse_with(&argv(&["grap", "view"]), Door::Cli, "aoide", &reg, &Term { style: Style::OFF, width: 80 }).unwrap_err();
        let bold = parse_with(&argv(&["grap", "view"]), Door::Cli, "aoide", &reg, &styled).unwrap_err();
        assert!(bold.message.contains("\x1b[1maoide graph view\x1b[0m"), "{:?}", bold.message);
        assert!(!plain.message.contains('\x1b'));
    }

    #[test]
    fn a_tie_between_equally_near_commands_goes_to_the_head_listed_first() {
        let mut r = Registry::new();
        for path in [&["beta", "go"][..], &["alpha", "go"][..]] {
            r.insert(Command { path, summary: "Go.", flags: &[JSON_FLAG], ..Command::BLANK });
        }
        r.arrange(crate::registry::Layout { tagline: "", sections: &[("All", &[("alpha", ""), ("beta", "")])] });
        let tie = did_you_mean(&["alphx".to_string(), "go".to_string()], &r);
        assert_eq!(tie, vec!["alpha go"]);
    }

    #[test]
    fn a_head_the_other_binary_owns_says_where_to_run_it() {
        let reg = test_registry();
        let err = parse(&argv(&["rice", "stage", "dusk"]), Door::Cli, "aoide", &reg).unwrap_err();
        assert_eq!(err.status, Status::Usage);
        assert_eq!(err.command, "rice", "typed arguments never leak into the command id");
        let text = err.render(false).0;
        assert!(text.contains("`rice` is a lyra command, not an aoide one"), "{text}");
        assert!(text.contains("fix: lyra rice stage dusk"), "{text}");

        let back = parse(&argv(&["spawn", "x"]), Door::Cli, "lyra", &reg).unwrap_err();
        assert!(back.render(false).0.contains("`spawn` is an aoide command, not a lyra one"), "{}", back.message);
    }

    #[test]
    fn an_example_already_starting_with_a_shell_word_or_the_binary_is_not_prefixed() {
        let reg = test_registry();
        assert_eq!(example_line("graph view --focus x", "aoide", &reg), "aoide graph view --focus x");
        assert_eq!(example_line("printf %s x | aoide node remove a", "aoide", &reg), "printf %s x | aoide node remove a");
        assert_eq!(example_line("aoide node remove a", "aoide", &reg), "aoide node remove a");
    }

    #[test]
    fn a_stub_is_marked_in_every_list() {
        let mut r = Registry::new();
        r.insert(Command { path: &["later"], summary: "Not built yet.", implemented: false, flags: &[JSON_FLAG], ..Command::BLANK });
        r.insert(Command { path: &["mixed", "now"], summary: "Works.", flags: &[JSON_FLAG], ..Command::BLANK });
        r.insert(Command { path: &["mixed", "soon"], summary: "Will work.", implemented: false, flags: &[JSON_FLAG], ..Command::BLANK });
        let _guard = crate::bin::path_test_lock().lock().unwrap();
        let (dir, saved) = scoped_empty_path("stub_marker");
        let t = Term { style: Style::OFF, width: 80 };
        let root = help::overview(&r, "aoide", &t);
        assert!(root.contains("Not yet implemented\n  later  Not built yet."), "{root}");
        let group = help::group(&["mixed".to_string()], &r, "aoide", &t).unwrap();
        assert!(group.contains("soon  Will work. (not yet implemented)"), "{group}");
        restore_path(dir, saved);
    }

    #[test]
    fn a_brief_replaces_the_cut_summary_in_a_list() {
        let c = Command { summary: "Long. Longer.", brief: "Tiny.", ..Command::BLANK };
        assert_eq!(list_description(&c), "Tiny.");
        let d = Command { summary: "First sentence here. Second.", ..Command::BLANK };
        assert_eq!(list_description(&d), "First sentence here.");
    }
}

