//! The self-registering command registry (CONTRACTS.md §3, v0).
//!
//! Every command in the tree is described here ONCE, alongside the handler
//! that runs it. The CLI dispatcher (`dispatch.rs`), the `schema --json`
//! emitter, the MCP tool list, and the A2A AgentCard (`a2a.rs`) all derive
//! from this single [`Registry`] — the "three doors, one schema" contract
//! (concepts/Agent-Interface). Nothing else in the crate enumerates commands.
//!
//! Each command group lives in its DOMAIN crate's `commands` module (Phase 9
//! restructure, docs/architecture/PACKAGE-LAYOUT.md) and contributes its
//! entries via a `register(&mut Registry)` function; the root package's
//! `commands/mod.rs::all()` assembles them (in the order that reproduces the
//! historical `schema.rs` table order byte-for-byte — `schema --json` and the
//! MCP tool list must never reorder). Nothing outside those `register()`
//! functions enumerates commands.

use crate::invocation::Invocation;
use crate::output::{Fix, Kind, Outcome, Refusal};
use crate::suggest::{closest, lead};
use serde::Serialize;

/// Contract versions (CONTRACTS.md "Versioning").
pub const SCHEMA_VERSION: &str = "0";
/// The Aoide RELEASE version (prebeta `0.0.X`, root README.md's
/// "Versioning" section) — derives from THIS crate's own Cargo.toml, which
/// itself inherits `pkgs/aoide/Cargo.toml`'s `[workspace.package].version`
/// (versioning start, 2026-08-22). Never a second hardcoded literal: a
/// release bump is one edit to the workspace manifest, not a search for
/// every place this string was repeated.
pub const AOIDE_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Stage-file format version (CONTRACTS.md §4).
pub const STAGE_NOTES_VERSION: &str = "0";

/// A positional argument of a command.
#[derive(Debug, Clone, Serialize)]
pub struct Arg {
    pub name: &'static str,
    #[serde(rename = "type")]
    pub ty: &'static str,
    pub required: bool,
    pub description: &'static str,
}

/// A `--flag` of a command.
///
/// Everything past `description` is the declarative half of validation
/// ([`Command::check`]): additive (CONTRACTS.md §3), each field skipped when
/// at its default so a flag that declares none serializes byte-identical to
/// before the fields existed. Declare them with named arms on `flag!`
/// (`flag!("key", "string", "…", value: "key", required: true)`).
#[derive(Debug, Clone, Serialize)]
pub struct Flag {
    pub name: &'static str,
    #[serde(rename = "type")]
    pub ty: &'static str,
    pub description: &'static str,
    /// Placeholder shown for a valued flag (`--key <key>`); empty falls back
    /// to the type name (`value` for a string).
    #[serde(skip_serializing_if = "is_blank")]
    pub value: &'static str,
    /// The invocation is refused when this flag is absent (and has no `default`).
    #[serde(skip_serializing_if = "is_false")]
    pub required: bool,
    /// Applied by `check` when the flag is absent, so a handler reads it as given.
    #[serde(skip_serializing_if = "is_blank")]
    pub default: &'static str,
    /// The closed set of accepted values; empty means any value.
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    pub values: &'static [&'static str],
    /// Flags that may not be given together with this one.
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    pub conflicts: &'static [&'static str],
}

impl Flag {
    /// All-default fields; `flag!` spreads it under the fields a call names.
    pub const NONE: Flag = Flag {
        name: "",
        ty: "",
        description: "",
        value: "",
        required: false,
        default: "",
        values: &[],
        conflicts: &[],
    };

    /// The `<placeholder>` a valued flag shows in a synopsis: its `value`,
    /// else its type (`string` reads as plain `value`).
    pub fn placeholder(&self) -> &'static str {
        match (self.value, self.ty) {
            ("", "string") => "value",
            ("", ty) => ty,
            (value, _) => value,
        }
    }

    pub fn takes_value(&self) -> bool {
        self.ty != "bool" && self.ty != "boolean"
    }
}

/// One command (a leaf in the command tree).
#[derive(Debug, Clone, Serialize)]
pub struct Command {
    /// The invocation path, e.g. `["rice", "gen"]`.
    pub path: &'static [&'static str],
    pub summary: &'static str,
    pub args: &'static [Arg],
    pub flags: &'static [Flag],
    /// Routes through the user rebuild gate (CONTRACTS.md §3).
    pub gated: bool,
    /// Actually mutates the live system? (walking skeleton: many are stubs).
    /// Additive field (CONTRACTS.md §3): serialized so a discovery consumer —
    /// the A2A AgentCard (CONTRACTS.md §6) is the first one — can filter to
    /// only the commands that are live, without a second command inventory.
    pub implemented: bool,
    /// Hook-plumbing, not an operator command — a harness's own hook payload
    /// drives it (`session start/phase/end/hook`), never a human typing it
    /// directly. Additive field (CONTRACTS.md §3), same discipline as
    /// `implemented`/`examples` before it: skipped when `false` so an
    /// ordinary command's schema stays byte-identical to before this field
    /// existed. `guide.rs`'s human listing skips `internal` commands; the
    /// schema/MCP/A2A doors still enumerate them — this hides noise from a
    /// person, not capability from a consumer.
    #[serde(skip_serializing_if = "is_false")]
    pub internal: bool,
    #[serde(rename = "exitCodes", serialize_with = "exit_codes")]
    pub exit_codes: (),
    /// Invocation examples shown by `<cmd> --help` (the human door only —
    /// MCP/A2A consumers get them through `schema --json` when present).
    /// Additive field (CONTRACTS.md §3), like `implemented` before it:
    /// skipped when empty so a command without examples serializes
    /// byte-identical to before this field existed.
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    pub examples: &'static [&'static str],
    /// Groups of flag names of which at least one must be given
    /// (`[["id", "to"]]`: `--id` or `--to`). Additive, skipped when empty.
    #[serde(rename = "oneOf", skip_serializing_if = "<[_]>::is_empty")]
    pub one_of: &'static [&'static [&'static str]],
    /// A one-line summary (72 chars at most) for lists; empty falls back to
    /// the first sentence of `summary`. Additive, skipped when empty.
    #[serde(skip_serializing_if = "is_blank")]
    pub brief: &'static str,
    /// The topic section this command's head is listed under, set once per
    /// head by [`Registry::arrange`] (never written on a `cmd!`). Additive,
    /// skipped when the head is unassigned (a stub head lists itself last).
    #[serde(skip_serializing_if = "is_blank")]
    pub section: &'static str,
    /// The handler `dispatch()` calls when `implemented` is true. Unused
    /// (never invoked) for stub commands — see `commands/stubs.rs`.
    #[serde(skip)]
    pub handler: fn(&Invocation) -> Outcome,
    /// Reserved for future conditional availability (e.g. env-gated
    /// commands); not yet consulted by `dispatch()`. Defaults to `|| true`.
    #[serde(skip)]
    pub available: fn() -> bool,
}

fn blank_handler(_: &Invocation) -> Outcome {
    Outcome::ok("", "")
}

impl Command {
    /// Placeholder values `cmd!` spreads under the fields a call names.
    pub const BLANK: Command = Command {
        path: &[],
        summary: "",
        args: &[],
        flags: &[],
        gated: false,
        implemented: true,
        internal: false,
        exit_codes: (),
        examples: &[],
        one_of: &[],
        brief: "",
        section: "",
        handler: blank_handler,
        available: || true,
    };

    /// The dotted path used as an MCP tool name, e.g. `rice.lint`.
    pub fn dotted(&self) -> String {
        self.path.join(".")
    }

    fn flag(&self, name: &str) -> Option<&Flag> {
        self.flags.iter().find(|f| f.name == name)
    }

    /// `--name <placeholder>`, or bare `--name` for a boolean.
    fn flag_form(&self, name: &str) -> String {
        match self.flag(name) {
            Some(f) if f.takes_value() => format!("--{name} <{}>", f.placeholder()),
            _ => format!("--{name}"),
        }
    }

    /// The invocation line as typed: positionals, required flags, and each
    /// `one_of` group as `(--a <v> | --b <v>)`. With `options`, the optional
    /// flags collapse to `[options]` and `[--json]` closes the line.
    pub fn synopsis(&self, bin: &str, options: bool) -> String {
        let mut s = format!("{bin} {}", self.path.join(" "));
        for a in self.args {
            if a.required {
                s.push_str(&format!(" <{}>", a.name));
            } else {
                s.push_str(&format!(" [<{}>]", a.name));
            }
        }
        for f in self.flags.iter().filter(|f| f.required && f.default.is_empty()) {
            s.push(' ');
            s.push_str(&self.flag_form(f.name));
        }
        for group in self.one_of {
            let forms: Vec<String> = group.iter().map(|n| self.flag_form(n)).collect();
            s.push_str(&format!(" ({})", forms.join(" | ")));
        }
        if options {
            let optional = self.flags.iter().any(|f| {
                f.name != "json"
                    && !(f.required && f.default.is_empty())
                    && !self.one_of.iter().any(|g| g.contains(&f.name))
            });
            if optional {
                s.push_str(" [options]");
            }
            s.push_str(" [--json]");
        }
        s
    }

    /// The runnable shape of this command for a fix: what was already given
    /// stays, what is missing shows as its placeholder.
    fn fix_line(&self, bin: &str, inv: &Invocation) -> String {
        let mut s = format!("{bin} {}", self.path.join(" "));
        let mut given = inv.args.iter();
        for a in self.args {
            match given.next() {
                Some(v) => s.push_str(&format!(" {v}")),
                None if a.required => s.push_str(&format!(" <{}>", a.name)),
                None => {}
            }
        }
        for f in self.flags.iter().filter(|f| f.required && f.default.is_empty()) {
            if !inv.flags.contains_key(f.name) {
                s.push(' ');
                s.push_str(&self.flag_form(f.name));
            }
        }
        for group in self.one_of {
            if !group.iter().any(|n| inv.flags.contains_key(*n)) {
                s.push(' ');
                s.push_str(&self.flag_form(group[0]));
            }
        }
        s
    }

    /// Run this command's declared checks against `inv`, applying defaults
    /// first: required positionals, required flags, `one_of`, `conflicts`,
    /// then `values` (with a did-you-mean). Every refusal is a usage error
    /// (exit 2) naming the item. A command that declares none of the new
    /// fields and no required positional passes untouched. A stub is never
    /// checked: it answers 64 whatever it was given.
    pub fn check(&self, bin: &str, inv: &mut Invocation) -> Result<(), Refusal> {
        if !self.implemented {
            return Ok(());
        }
        for f in self.flags.iter().filter(|f| !f.default.is_empty() && f.takes_value()) {
            inv.flags.entry(f.name.to_string()).or_insert_with(|| f.default.to_string());
        }
        let usage = |what: String, why: String, fix: String| Refusal::new(Kind::Usage, what, why, Fix::Run(fix));
        let me = format!("{bin} {}", self.path.join(" "));

        let missing = self.args.iter().filter(|a| a.required).nth(inv.args.len());
        if let Some(a) = missing {
            return Err(usage(
                format!("`{me}` needs <{}>", a.name),
                lead(a.description),
                self.fix_line(bin, inv),
            ));
        }
        if let Some(f) = self.flags.iter().find(|f| f.required && !inv.flags.contains_key(f.name)) {
            return Err(usage(
                format!("`{me}` needs {}", self.flag_form(f.name)),
                lead(f.description),
                self.fix_line(bin, inv),
            ));
        }
        for group in self.one_of {
            if !group.iter().any(|n| inv.flags.contains_key(*n)) {
                let names: Vec<String> = group.iter().map(|n| format!("--{n}")).collect();
                let parts: Vec<String> = group
                    .iter()
                    .map(|n| match self.flag(n) {
                        Some(f) => format!("--{n} ({})", lead(f.description)),
                        None => format!("--{n}"),
                    })
                    .collect();
                return Err(usage(
                    format!("`{me}` needs one of {}", names.join(" or ")),
                    format!("none was given; the choices are {}", parts.join(", ")),
                    self.fix_line(bin, inv),
                ));
            }
        }
        for f in self.flags.iter().filter(|f| inv.flags.contains_key(f.name)) {
            if let Some(other) = f.conflicts.iter().find(|c| inv.flags.contains_key(**c)) {
                return Err(usage(
                    format!("`{me}` takes --{} or --{other}, not both", f.name),
                    "they are alternatives for the same thing".to_string(),
                    format!("{me} …  (drop --{} or --{other})", f.name),
                ));
            }
        }
        for f in self.flags.iter().filter(|f| !f.values.is_empty()) {
            let Some(v) = inv.flags.get(f.name) else { continue };
            if f.values.contains(&v.as_str()) {
                continue;
            }
            let near = closest(v, f.values.iter().copied(), 1);
            let accepted = f.values.join(", ");
            let (why, pick) = match near.first() {
                Some(n) => (format!("did you mean `{n}`? the accepted values are {accepted}"), *n),
                None => (format!("the accepted values are {accepted}"), f.values[0]),
            };
            let mut fixed = inv.clone();
            fixed.flags.insert(f.name.to_string(), pick.to_string());
            return Err(usage(
                format!("`{me}` does not accept `--{} {v}`", f.name),
                why,
                format!("{} --{} {pick}", self.fix_line(bin, &fixed), f.name),
            ));
        }
        Ok(())
    }

    /// Check, then run the handler: the one entry every door uses.
    pub fn invoke(&self, bin: &str, inv: &Invocation) -> Outcome {
        let mut inv = inv.clone();
        match self.check(bin, &mut inv) {
            Ok(()) => (self.handler)(&inv),
            Err(r) => r.into_outcome(inv.dotted()),
        }
    }
}

/// The whole `schema --json` document.
#[derive(Debug, Serialize)]
pub struct Schema {
    #[serde(rename = "schemaVersion")]
    pub schema_version: &'static str,
    pub aoide: &'static str,
    #[serde(rename = "stageNotesVersion")]
    pub stage_notes_version: &'static str,
    pub commands: Vec<Command>,
    /// External subcommands (task #138): every `<bin_name>-<name>` executable
    /// found on `PATH` at the moment `schema` ran, name-only — never a second
    /// command inventory. **Additive**, same discipline as `implemented`/
    /// `examples`/`internal` above: omitted entirely when no plugin is
    /// installed, so a host with none emits a schema byte-identical to
    /// before this field existed. An external command never becomes a
    /// [`Command`] (CONTRACTS.md §3's "never gated" sentence covers why) —
    /// this is the ONLY place it appears in this document.
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    pub external: Vec<ExternalCommand>,
    /// The listing order: sections in order, each with its heads in order.
    /// **Additive**, omitted when the registry was never arranged.
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    pub sections: Vec<SchemaSection>,
}

/// One section of the listing order in [`Schema::sections`].
#[derive(Debug, Clone, Serialize)]
pub struct SchemaSection {
    pub name: &'static str,
    pub heads: Vec<SchemaHead>,
}

/// One head of a [`SchemaSection`]; `brief` is the group's one-liner.
#[derive(Debug, Clone, Serialize)]
pub struct SchemaHead {
    pub name: &'static str,
    #[serde(skip_serializing_if = "is_blank")]
    pub brief: &'static str,
}

/// How a binary lists its commands: a tagline, then topic sections in order,
/// each naming its heads in order with the one-line brief of a multi-command
/// head (a single-command head lists its own command's brief). A stub head
/// is left out; it lists itself last, under "Not yet implemented".
#[derive(Debug, Clone, Copy)]
pub struct Layout {
    pub tagline: &'static str,
    pub sections: &'static [(&'static str, &'static [(&'static str, &'static str)])],
}

impl Layout {
    pub const EMPTY: Layout = Layout { tagline: "", sections: &[] };
}

/// One entry in [`Schema::external`] — a name, the resolved command spelling,
/// and where it lives. No summary, no args/flags: aoide cannot make that
/// contract for a foreign binary (`<name> --help` answers for itself).
#[derive(Debug, Clone, Serialize)]
pub struct ExternalCommand {
    pub name: String,
    pub command: String,
    pub path: String,
}

/// `serde(skip_serializing_if)` predicate for `internal` — skip the key
/// entirely when false, so a non-internal command's schema is byte-identical
/// to before the field existed.
fn is_false(b: &bool) -> bool {
    !*b
}

fn is_blank(s: &&str) -> bool {
    s.is_empty()
}

/// The canonical exit-code map, identical for every command (CONTRACTS.md §3).
fn exit_codes<S: serde::Serializer>(_: &(), s: S) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeMap;
    let mut m = s.serialize_map(Some(4))?;
    m.serialize_entry("0", "ok")?;
    m.serialize_entry("1", "error")?;
    m.serialize_entry("2", "usage")?;
    m.serialize_entry("64", "not-implemented")?;
    m.end()
}

// ── The `--json` flag every command carries (the contract) ──────────────────
pub const JSON_FLAG: Flag = Flag {
    name: "json",
    ty: "bool",
    description: "Structured I/O — emit a machine-readable JSON envelope.",
    ..Flag::NONE
};

/// The command registry: every known command, in registration order.
/// Iteration order MUST match the historical `schema.rs` table order —
/// `schema --json` and the MCP tool list are byte-sensitive to it.
#[derive(Default)]
pub struct Registry {
    entries: Vec<Command>,
    layout: Layout,
}

impl Default for Layout {
    fn default() -> Self {
        Layout::EMPTY
    }
}

impl Registry {
    pub fn new() -> Self {
        Registry { entries: Vec::new(), layout: Layout::EMPTY }
    }

    /// Append one command. Panics on a duplicate path — a self-registering
    /// registry must never silently shadow an earlier entry.
    pub fn insert(&mut self, cmd: Command) {
        assert!(
            self.get(&cmd.path.iter().map(|s| s.to_string()).collect::<Vec<_>>())
                .is_none(),
            "duplicate command path: {}",
            cmd.dotted()
        );
        self.entries.push(cmd);
    }

    /// Adopt the listing `layout`: every command of a listed head takes its
    /// section. Panics on a head no command has or one listed twice, so a
    /// layout can never name what is not there.
    pub fn arrange(&mut self, layout: Layout) {
        for (section, heads) in layout.sections {
            for (head, _) in *heads {
                let mut found = false;
                for c in self.entries.iter_mut().filter(|c| c.path[0] == *head) {
                    assert!(c.section.is_empty(), "head `{head}` is listed in two sections");
                    c.section = section;
                    found = true;
                }
                assert!(found, "layout lists `{head}`, which no command has");
            }
        }
        self.layout = layout;
    }

    pub fn layout(&self) -> Layout {
        self.layout
    }

    /// Look up the command entry for a parsed invocation path.
    pub fn get(&self, path: &[String]) -> Option<&Command> {
        self.entries
            .iter()
            .find(|c| c.path.len() == path.len() && c.path.iter().zip(path).all(|(a, b)| *a == b))
    }

    /// Every registered command, in registration order.
    pub fn commands(&self) -> impl Iterator<Item = &Command> {
        self.entries.iter()
    }

    /// Build the full `schema --json` document from this registry.
    /// `bin_name` names the invoking binary (`"aoide"`/`"lyra"`) — it drives
    /// `external`'s own `PATH` probe (`crate::bin::discover_external`), the
    /// same parameter every usage/did-you-mean string in `door.rs` already
    /// takes.
    pub fn schema(&self, bin_name: &str) -> Schema {
        let external = crate::bin::discover_external(bin_name)
            .into_iter()
            .map(|(name, path)| ExternalCommand {
                command: format!("{bin_name}-{name}"),
                path: path.to_string_lossy().into_owned(),
                name,
            })
            .collect();
        Schema {
            schema_version: SCHEMA_VERSION,
            aoide: AOIDE_VERSION,
            stage_notes_version: STAGE_NOTES_VERSION,
            commands: self.entries.clone(),
            external,
            sections: self
                .layout
                .sections
                .iter()
                .map(|(name, heads)| SchemaSection {
                    name,
                    heads: heads.iter().map(|(name, brief)| SchemaHead { name, brief }).collect(),
                })
                .collect(),
        }
    }
}

/// Small helper: a leaf command whose only flag is `--json`, wired to a
/// handler fn. Every command group's `register()` uses this to build its
/// `Command` entries — metadata copied verbatim from the pre-registry
/// `schema.rs` table.
///
/// Moved from the root package's `src/registry.rs` (Phase 9 restructure,
/// docs/architecture/PACKAGE-LAYOUT.md) so every domain crate's
/// `commands::register()` can describe its own commands; the `$crate::registry::*`
/// expansions resolve identically inside THIS crate, which owns the types.
#[macro_export]
macro_rules! cmd {
    // The examples-carrying arm is listed FIRST: macro arms are tried in
    // order, so the more specific matcher must precede the general one below
    // — a call site that passes `examples:` lands here, everything else falls
    // through to the no-examples arm (which defaults `examples: &[]`).
    // Both arms take trailing named extras (`brief: "…"`, `one_of: &[…]`):
    // any `Command` field not otherwise set comes from `Command::BLANK`.
    (
        path: [$($seg:literal),*],
        summary: $summary:literal,
        args: [$($arg:expr),* $(,)?],
        flags: [$($flag:expr),* $(,)?],
        gated: $gated:expr,
        implemented: $impl:expr,
        handler: $handler:expr,
        examples: [$($ex:literal),* $(,)?]
        $(, $extra:ident : $val:expr)* $(,)?
    ) => {
        $crate::registry::Command {
            path: &[$($seg),*],
            summary: $summary,
            args: &[$($arg),*],
            flags: &[$crate::registry::JSON_FLAG, $($flag),*],
            gated: $gated,
            implemented: $impl,
            examples: &[$($ex),*],
            handler: $handler,
            $($extra: $val,)*
            ..$crate::registry::Command::BLANK
        }
    };
    (
        path: [$($seg:literal),*],
        summary: $summary:literal,
        args: [$($arg:expr),* $(,)?],
        flags: [$($flag:expr),* $(,)?],
        gated: $gated:expr,
        implemented: $impl:expr,
        handler: $handler:expr
        $(, $extra:ident : $val:expr)* $(,)?
    ) => {
        $crate::registry::Command {
            path: &[$($seg),*],
            summary: $summary,
            args: &[$($arg),*],
            flags: &[$crate::registry::JSON_FLAG, $($flag),*],
            gated: $gated,
            implemented: $impl,
            handler: $handler,
            $($extra: $val,)*
            ..$crate::registry::Command::BLANK
        }
    };
}

#[macro_export]
macro_rules! arg {
    ($name:literal, $ty:literal, $req:expr, $desc:literal) => {
        $crate::registry::Arg {
            name: $name,
            ty: $ty,
            required: $req,
            description: $desc,
        }
    };
}

#[macro_export]
macro_rules! flag {
    // Named extras (`value:`, `required:`, `default:`, `values:`,
    // `conflicts:`) are `Flag` fields; the 3-argument form names none.
    ($name:literal, $ty:literal, $desc:literal $(, $extra:ident : $val:expr)* $(,)?) => {
        $crate::registry::Flag {
            name: $name,
            ty: $ty,
            description: $desc,
            $($extra: $val,)*
            ..$crate::registry::Flag::NONE
        }
    };
}

// Re-export at this module's path too, so a domain crate's
// `use aoide_protocol::registry::{arg, cmd, flag, Registry};` reads exactly
// like the root package's historical `use crate::registry::{…}`.
pub use crate::{arg, cmd, flag};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::Door;
    use crate::output::Status;
    use std::collections::BTreeMap;

    fn noop(_: &Invocation) -> Outcome {
        Outcome::ok("noop", "ran")
    }

    fn fixture() -> Command {
        cmd!(
            path: ["thing", "add"],
            summary: "Add a thing.",
            args: [arg!("name", "string", true, "The thing's name.")],
            flags: [
                flag!("backend", "string", "Where it lives.", value: "backend", default: "age"),
                flag!("key", "string", "Its key.", value: "key", required: true),
                flag!("id", "string", "By id.", conflicts: &["to"]),
                flag!("to", "string", "By name.", conflicts: &["id"]),
                flag!("what", "string", "Which part.", values: &["screen", "widget"])
            ],
            gated: false,
            implemented: true,
            handler: noop,
            one_of: &[&["id", "to"]],
            brief: "Add a thing."
        )
    }

    fn inv(args: &[&str], flags: &[(&str, &str)]) -> Invocation {
        Invocation {
            path: vec!["thing".into(), "add".into()],
            args: args.iter().map(|s| s.to_string()).collect(),
            flags: flags.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<BTreeMap<_, _>>(),
            door: Door::Cli,
        }
    }

    fn refusal(args: &[&str], flags: &[(&str, &str)]) -> (String, String, String) {
        let r = fixture().check("aoide", &mut inv(args, flags)).unwrap_err();
        let o = r.into_outcome("thing.add");
        assert_eq!(o.status, Status::Usage);
        let text = o.render(false).0;
        let mut lines = text.lines();
        (lines.next().unwrap().to_string(), lines.next().unwrap_or_default().to_string(), lines.next().unwrap_or_default().to_string())
    }

    #[test]
    fn a_missing_positional_is_named_and_the_fix_keeps_what_was_given() {
        let (head, why, fix) = refusal(&[], &[("key", "k"), ("id", "s")]);
        assert_eq!(head, "[usage] thing.add: `aoide thing add` needs <name>");
        assert_eq!(why, "  why: the thing's name");
        assert_eq!(fix, "  fix: aoide thing add <name>");
    }

    #[test]
    fn a_missing_required_flag_is_named_with_its_placeholder() {
        let (head, _, fix) = refusal(&["x"], &[("id", "s")]);
        assert_eq!(head, "[usage] thing.add: `aoide thing add` needs --key <key>");
        assert_eq!(fix, "  fix: aoide thing add x --key <key>");
    }

    #[test]
    fn a_one_of_group_with_no_member_lists_the_choices() {
        let (head, why, fix) = refusal(&["x"], &[("key", "k")]);
        assert_eq!(head, "[usage] thing.add: `aoide thing add` needs one of --id or --to");
        assert_eq!(why, "  why: none was given; the choices are --id (by id), --to (by name)");
        assert_eq!(fix, "  fix: aoide thing add x --id <value>");
    }

    #[test]
    fn conflicting_flags_are_refused_together() {
        let (head, _, _) = refusal(&["x"], &[("key", "k"), ("id", "a"), ("to", "b")]);
        assert_eq!(head, "[usage] thing.add: `aoide thing add` takes --id or --to, not both");
    }

    #[test]
    fn a_value_outside_the_set_gets_a_did_you_mean_or_the_list() {
        let (head, why, fix) = refusal(&["x"], &[("key", "k"), ("id", "a"), ("what", "wdget")]);
        assert_eq!(head, "[usage] thing.add: `aoide thing add` does not accept `--what wdget`");
        assert_eq!(why, "  why: did you mean `widget`? the accepted values are screen, widget");
        assert!(fix.ends_with("--what widget"), "{fix}");
        let (_, why, _) = refusal(&["x"], &[("key", "k"), ("id", "a"), ("what", "nonsense")]);
        assert_eq!(why, "  why: the accepted values are screen, widget");
    }

    #[test]
    fn defaults_apply_and_a_satisfied_invocation_passes() {
        let mut i = inv(&["x"], &[("key", "k"), ("id", "a")]);
        fixture().check("aoide", &mut i).unwrap();
        assert_eq!(i.flags.get("backend").map(String::as_str), Some("age"));
        let mut given = inv(&["x"], &[("key", "k"), ("id", "a"), ("backend", "pass")]);
        fixture().check("aoide", &mut given).unwrap();
        assert_eq!(given.flags.get("backend").map(String::as_str), Some("pass"));
    }

    #[test]
    fn a_command_declaring_nothing_new_and_no_required_arg_is_untouched() {
        let c = cmd!(path: ["plain"], summary: "Plain.", args: [], flags: [], gated: false, implemented: true, handler: noop);
        let mut i = inv(&[], &[]);
        c.check("aoide", &mut i).unwrap();
        assert!(i.flags.is_empty());
        let v = serde_json::to_value(&c).unwrap();
        for k in ["oneOf", "brief"] {
            assert!(v.get(k).is_none(), "{k}");
        }
        let f = serde_json::to_value(&flag!("x", "string", "d")).unwrap();
        assert_eq!(f, serde_json::json!({ "name": "x", "type": "string", "description": "d" }));
    }

    #[test]
    fn a_stub_is_never_checked() {
        let mut c = fixture();
        c.implemented = false;
        c.check("aoide", &mut inv(&[], &[])).unwrap();
    }

    #[test]
    fn the_synopsis_shows_required_flags_and_choices() {
        assert_eq!(
            fixture().synopsis("aoide", true),
            "aoide thing add <name> --key <key> (--id <value> | --to <value>) [options] [--json]"
        );
    }

    #[test]
    fn the_layout_is_additive_data_a_registry_without_one_serializes_as_before() {
        let mut r = Registry::new();
        r.insert(cmd!(path: ["a"], summary: "A.", args: [], flags: [], gated: false, implemented: true, handler: noop));
        r.insert(cmd!(path: ["b", "go"], summary: "B.", args: [], flags: [], gated: false, implemented: true, handler: noop));
        let before = serde_json::to_value(r.schema("aoide")).unwrap();
        assert!(before.get("sections").is_none());
        assert!(before["commands"].as_array().unwrap().iter().all(|c| c.get("section").is_none()));

        r.arrange(Layout { tagline: "t", sections: &[("First", &[("b", "The b group")]), ("Second", &[("a", "")])] });
        let after = serde_json::to_value(r.schema("aoide")).unwrap();
        assert_eq!(after["commands"][0]["section"], "Second");
        assert_eq!(after["commands"][1]["section"], "First");
        assert_eq!(
            after["sections"],
            serde_json::json!([
                { "name": "First", "heads": [{ "name": "b", "brief": "The b group" }] },
                { "name": "Second", "heads": [{ "name": "a" }] }
            ])
        );
    }

    #[test]
    #[should_panic(expected = "which no command has")]
    fn a_layout_naming_a_head_nobody_registers_is_refused() {
        let mut r = Registry::new();
        r.arrange(Layout { tagline: "", sections: &[("S", &[("ghost", "")])] });
    }
}
