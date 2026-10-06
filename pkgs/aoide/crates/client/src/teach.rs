//! The refusals the pair, node, mesh and mail commands share, said as taught
//! errors (`pkgs/aoide/crates/AGENTS.md`, "Taught error"): what was refused,
//! why, and a full command line that fixes it. A handler builds a one-off
//! refusal with `Outcome::refuse` directly; what lives here is only what more
//! than one command says.

use aoide_protocol::output::{Fix, Kind, Outcome};
use aoide_protocol::suggest::closest;
use aoide_protocol::Invocation;
use serde_json::json;
use std::path::Path;

/// Flags of these commands that take no value, for re-quoting a line.
const BOOLS: &[&str] = &[
    "all-names", "autogate", "clear", "hold", "no-verify", "popup", "reread", "refused", "replace", "yes",
];

/// Where in the invocation the mistyped name sits.
#[derive(Clone, Copy)]
pub(crate) enum Slot<'a> {
    Arg(usize),
    Flag(&'a str),
    /// The `<node>` half of a `--to <node>/<name>` address.
    ToNode,
}

/// The invocation as a line to paste.
pub(crate) fn line(inv: &Invocation) -> String {
    inv.command_line("aoide", BOOLS, false)
}

/// The invocation after `edit`, as a line to paste.
pub(crate) fn edited(inv: &Invocation, edit: impl FnOnce(&mut Invocation)) -> String {
    let mut fixed = inv.clone();
    edit(&mut fixed);
    line(&fixed)
}

/// The invocation with its first positional set to `with` (added when absent).
pub(crate) fn line_with_new_arg(inv: &Invocation, with: &str) -> String {
    edited(inv, |i| match i.args.first_mut() {
        Some(a) => *a = with.to_string(),
        None => i.args.push(with.to_string()),
    })
}

/// The invocation with the name in `slot` replaced by `with`, as a line to paste.
pub(crate) fn line_with(inv: &Invocation, slot: Slot, with: &str) -> String {
    let mut fixed = inv.clone();
    match slot {
        Slot::Arg(i) if i < fixed.args.len() => fixed.args[i] = with.to_string(),
        Slot::Arg(_) => {}
        Slot::Flag(name) => {
            fixed.flags.insert(name.to_string(), with.to_string());
        }
        Slot::ToNode => {
            let name = fixed.flags.get("to").and_then(|t| t.split_once('/')).map(|(_, n)| n.to_string());
            let value = match name {
                Some(n) => format!("{with}/{n}"),
                None => with.to_string(),
            };
            fixed.flags.insert("to".to_string(), value);
        }
    }
    line(&fixed)
}

/// A name the world holds nothing under (exit 1): the closest valid name when
/// one is a typo away, else the names there are, and a fix that runs.
///
/// `held` says what the list is ("registered nodes"), `list_line` is a command
/// that prints the valid names (the fix when nothing is close, since guessing
/// a name for a command that changes something would be wrong), and `none_yet`
/// is the fix when there is nothing to choose from.
pub(crate) fn unknown_name(
    inv: &Invocation,
    cmd: &str,
    noun: &str,
    slot: Slot,
    typed: &str,
    valid: &[String],
    held: &str,
    list_line: &str,
    none_yet: Fix,
) -> Outcome {
    let what = format!("no {noun} named `{typed}`");
    let list = if valid.len() > 6 {
        format!("{}, … ({} more)", valid[..6].join(", "), valid.len() - 6)
    } else {
        valid.join(", ")
    };
    let prefixed = valid.iter().find(|v| typed.chars().count() >= 4 && v.starts_with(typed)).map(String::as_str);
    let near = prefixed.or_else(|| closest(typed, valid.iter().map(String::as_str), 1).first().copied());
    let (why, fix) = match near.as_ref() {
        Some(near) => (
            format!("did you mean `{near}`? the {held} are {list}"),
            Fix::Run(line_with(inv, slot, near)),
        ),
        None if valid.is_empty() => (format!("there are no {held} yet"), none_yet),
        None => (format!("the {held} are {list}"), Fix::Run(list_line.to_string())),
    };
    Outcome::refuse(cmd, Kind::Refused, what, why, fix)
        .with_fields(json!({ "reason": format!("unknown-{}", noun.replace(' ', "-")), "name": typed }))
}

/// A store under `$AOIDE_ROOT` (`state/mail`, `state/outbox`) could not be read
/// or written (exit 1); `err` is the store's own text, kept in `detail`.
pub(crate) fn store_failed(cmd: &str, store: &str, err: &str) -> Outcome {
    Outcome::refuse(
        cmd,
        Kind::Failed,
        format!("could not use `{store}`"),
        "the store could not be read or written: its directory is not writable, the disk is full, or a file in it is damaged",
        Fix::Set(format!("write permission on `$AOIDE_ROOT/{store}`, or free disk space")),
    )
    .with_fields(json!({ "reason": "store-failed", "store": store }))
    .with_detail(err)
}

/// The node registry could not be written (exit 1).
pub(crate) fn registry_write(cmd: &str, path: &Path, err: &str) -> Outcome {
    Outcome::refuse(
        cmd,
        Kind::Failed,
        "could not write the node registry",
        format!("`{}` could not be written; its directory is not writable or the disk is full", path.display()),
        Fix::Set(format!("write permission on `{}`'s directory, or free disk space", path.display())),
    )
    .with_fields(json!({ "reason": "registry-write-failed" }))
    .with_detail(err)
}

/// `name` as the nickname grammar would have it: lowercase letters, digits and
/// single hyphens.
pub(crate) fn sanitize(name: &str) -> String {
    let mut fixed: String =
        name.to_lowercase().chars().map(|c| if c.is_ascii_lowercase() || c.is_ascii_digit() { c } else { '-' }).collect();
    while fixed.contains("--") {
        fixed = fixed.replace("--", "-");
    }
    fixed.trim_matches('-').to_string()
}

/// A name the nickname grammar forbids (exit 2): it can never be valid.
pub(crate) fn bad_nickname(inv: &Invocation, cmd: &str, noun: &str, slot: Slot, name: &str) -> Outcome {
    let fixed = sanitize(name);
    let fix = if aoide_storage::node_store::valid_node_name(&fixed) {
        Fix::Run(line_with(inv, slot, &fixed))
    } else {
        Fix::Set(format!("a {noun} of lowercase letters, digits and single hyphens, such as `sakaki`"))
    };
    Outcome::refuse(
        cmd,
        Kind::Usage,
        format!("`{name}` is not a valid {noun}"),
        "it is lowercase letters, digits and hyphens (`^[a-z0-9][a-z0-9-]*$`), with no leading hyphen, `/` or `..`, because it becomes a file name",
        fix,
    )
    .with_fields(json!({ "reason": "invalid-name", "name": name }))
}

/// The mesh could not be resolved (`resolve_mesh*` refused): a `--mesh`
/// that is no mesh name can never be valid (exit 2); no `--mesh` where `known`
/// holds several is the world's ambiguity (exit 1), fixed by naming one.
pub(crate) fn mesh_choice(inv: &Invocation, cmd: &str, known: &[String]) -> Outcome {
    if let Some(typed) = inv.flags.get("mesh") {
        return bad_nickname(inv, cmd, "mesh name", Slot::Flag("mesh"), typed)
            .with_fields(json!({ "reason": "invalid-mesh", "mesh": typed }));
    }
    let pick = known.first().map(String::as_str).unwrap_or("home");
    Outcome::refuse(
        cmd,
        Kind::Refused,
        format!("the mesh this acts in is ambiguous: this box knows {}", known.join(", ")),
        "a command acts in exactly one mesh, and guessing would address a grant you did not mean",
        Fix::Run(line_with(inv, Slot::Flag("mesh"), pick)),
    )
    .with_fields(json!({ "reason": "mesh-ambiguous", "meshes": known }))
}

/// A node that is registered but never paired, where the command needs a
/// paired one (exit 1).
pub(crate) fn not_paired(cmd: &str, name: &str, url: &str) -> Outcome {
    let fix = if url.starts_with("http") { format!("aoide pair {url} --name {name}") } else { format!("aoide pair {name}") };
    Outcome::refuse(
        cmd,
        Kind::Refused,
        format!("node `{name}` is registered but not paired"),
        "this command sends a signed request, and the far door honours only a node it has paired with (docs/architecture/PAIRING.md decision 6)",
        Fix::Run(fix),
    )
    .with_fields(json!({ "reason": "unpaired-node", "name": name }))
}

/// The answer to a prompt could not be read (exit 1); `--yes` is the scripted way.
pub(crate) fn prompt_failed(inv: &Invocation, cmd: &str, err: &str) -> Outcome {
    Outcome::refuse(
        cmd,
        Kind::Failed,
        "could not read your answer to the confirmation",
        "the prompt needs a terminal to read from, and this one did not give it",
        Fix::Run(edited(inv, |i| {
            i.flags.insert("yes".to_string(), "true".to_string());
        })),
    )
    .with_detail(err)
}

/// A command only a person at a terminal may run, reached over another door (exit 2).
pub(crate) fn cli_only(inv: &Invocation, cmd: &str, why: &str) -> Outcome {
    Outcome::refuse(cmd, Kind::Usage, format!("`aoide {}` is CLI-only", inv.path.join(" ")), why, Fix::Run(line(inv)))
}

/// A `--secs` that is not a positive whole number (exit 2).
pub(crate) fn bad_secs(inv: &Invocation, cmd: &str, default: u64) -> Outcome {
    let typed = inv.flags.get("secs").cloned().unwrap_or_default();
    Outcome::refuse(
        cmd,
        Kind::Usage,
        format!("`--secs {typed}` is not a positive whole number of seconds"),
        "a typo'd window must never quietly listen for the default one instead of what you asked",
        Fix::Run(edited(inv, |i| {
            i.flags.insert("secs".to_string(), default.to_string());
        })),
    )
}

/// One line on stderr before a long wait, so a person is not left at a silent
/// prompt. Never in `--json` mode, where stderr stays quiet for the machine.
pub(crate) fn progress(inv: &Invocation, note: &str) {
    if !inv.flag_present("json") {
        eprintln!("{note}");
    }
}

/// More positionals than the command takes (exit 2): the registry declares what
/// is required, and nothing stops a surplus word from being silently ignored.
pub(crate) fn extra_args(inv: &Invocation, cmd: &str, max: usize) -> Option<Outcome> {
    (inv.args.len() > max).then(|| {
        let kept = inv.args[..max].to_vec();
        Outcome::refuse(
            cmd,
            Kind::Usage,
            format!("`aoide {}` takes {max} argument(s), not {}", inv.path.join(" "), inv.args.len()),
            format!("`{}` would be ignored", inv.args[max]),
            Fix::Run(edited(inv, |i| i.args = kept)),
        )
    })
}

/// A far door that did not answer, or answered something unusable (exit 1).
pub(crate) fn far_door(cmd: &str, doing: &str, url: &str, cause: &str, reason: &str, rerun: &str) -> Outcome {
    Outcome::refuse(
        cmd,
        Kind::Failed,
        format!("{doing} at {url} failed"),
        "the far door did not answer in time or said something this client cannot read; its own words are in --json `detail`",
        Fix::Wait(format!("until `aoide a2a serve` answers on the far box, then run `{rerun}` again")),
    )
    .with_fields(json!({ "reason": reason, "url": url }))
    .with_detail(cause)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_protocol::Door;

    fn inv(path: &[&str], args: &[&str], flags: &[(&str, &str)]) -> Invocation {
        Invocation {
            path: path.iter().map(|s| s.to_string()).collect(),
            args: args.iter().map(|s| s.to_string()).collect(),
            flags: flags.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            door: Door::Cli,
        }
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn fix_of(o: &Outcome) -> String {
        let fix = &o.data.as_ref().unwrap()["refusal"]["fix"];
        fix.as_object().unwrap().values().next().unwrap().as_str().unwrap().to_string()
    }

    #[test]
    fn a_typo_suggests_the_near_node_and_the_fix_is_the_whole_corrected_line() {
        let i = inv(&["node", "pull"], &["sakak"], &[("mesh", "home")]);
        let o = unknown_name(&i, "node.pull", "node", Slot::Arg(0), "sakak", &names(&["sakaki", "yomi"]), "registered nodes", "aoide node status", Fix::None("x"));
        assert_eq!(o.status, aoide_protocol::output::Status::Error, "a name the world lacks is exit 1");
        assert!(o.message.contains("no node named `sakak`"), "{}", o.message);
        assert!(o.data.as_ref().unwrap()["refusal"]["why"].as_str().unwrap().contains("did you mean `sakaki`"));
        assert_eq!(fix_of(&o), "aoide node pull sakaki --mesh home");
    }

    #[test]
    fn a_name_nothing_is_near_lists_the_valid_names_and_points_at_the_list() {
        let i = inv(&["node", "remove"], &["zzzzzz"], &[]);
        let o = unknown_name(&i, "node.remove", "node", Slot::Arg(0), "zzzzzz", &names(&["sakaki", "yomi"]), "registered nodes", "aoide node status", Fix::None("x"));
        assert!(o.data.as_ref().unwrap()["refusal"]["why"].as_str().unwrap().contains("registered nodes are sakaki, yomi"));
        assert_eq!(fix_of(&o), "aoide node status", "a mutating command never guesses a name for the fix");
    }

    #[test]
    fn with_nothing_registered_the_refusal_says_so_and_names_the_first_step() {
        let i = inv(&["node", "pull"], &["a"], &[]);
        let o = unknown_name(&i, "node.pull", "node", Slot::Arg(0), "a", &[], "registered nodes", "aoide node status", Fix::Run("aoide pair".into()));
        assert!(o.data.as_ref().unwrap()["refusal"]["why"].as_str().unwrap().contains("no registered nodes yet"));
        assert_eq!(fix_of(&o), "aoide pair");
    }

    #[test]
    fn a_flag_slot_is_corrected_in_place() {
        let i = inv(&["mail", "read"], &[], &[("for", "conductr")]);
        let o = unknown_name(&i, "mail.read", "mailbox", Slot::Flag("for"), "conductr", &names(&["conductor"]), "mailboxes with mail", "aoide mail", Fix::None("x"));
        assert_eq!(fix_of(&o), "aoide mail read --for conductor");
    }

    #[test]
    fn a_bad_nickname_is_a_usage_error_with_the_sanitised_name_as_the_fix() {
        let i = inv(&["node", "add"], &["My Box", "poll"], &[]);
        let o = bad_nickname(&i, "node.add", "node nickname", Slot::Arg(0), "My Box");
        assert_eq!(o.status, aoide_protocol::output::Status::Usage);
        assert_eq!(fix_of(&o), "aoide node add my-box poll");
    }
}
