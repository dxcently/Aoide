//! Every place the CLI lists commands: the overview, a group's commands, the
//! name lists of an unknown-command refusal. One ordering (the registry's
//! [`Layout`], then stubs last), one table (sized to its content, wrapped with a
//! hanging indent) and one palette (`style`). A listing never decides any of
//! that for itself.

use crate::registry::{Command, Registry};
use crate::style::{wrap, Term};

/// The longest a list description runs; `Command::brief` is held to it.
pub const BRIEF_MAX: usize = 72;

pub const OTHER: &str = "Other";
pub const STUBS: &str = "Not yet implemented";

/// GNU-style terseness for list output: the first sentence of a command's
/// summary, cut at the last clause or word boundary inside the cap so it never
/// ends mid-word. The registry summaries are deliberate multi-sentence prose;
/// that prose still lives behind `aoide <cmd> --help` — the list is a list,
/// not the docs. A command that sets `brief` skips all of this.
fn short_desc(summary: &str) -> String {
    let end = summary
        .find(". ")
        .map(|i| i + 1) // keep the period
        .or_else(|| summary.find('\n'))
        .unwrap_or(summary.len());
    let s = summary[..end].trim_end();
    if s.chars().count() <= BRIEF_MAX {
        return s.to_string();
    }
    let prefix: String = s.chars().take(BRIEF_MAX).collect();
    let clause = prefix.rfind(['—', ';', ':', ',']).filter(|i| prefix[..*i].chars().count() >= BRIEF_MAX / 2);
    let cut = clause.or_else(|| prefix.rfind(char::is_whitespace)).unwrap_or(prefix.len());
    format!("{}…", prefix[..cut].trim_end())
}

/// What a list shows beside a command: its `brief`, else the first sentence
/// of its summary cut to fit.
pub fn list_description(c: &Command) -> String {
    if c.brief.is_empty() {
        short_desc(c.summary)
    } else {
        c.brief.to_string()
    }
}

/// `<req> [<opt>]`, the one builder for a command's positional signature.
fn signature(c: &Command) -> String {
    c.args
        .iter()
        .map(|a| if a.required { format!("<{}>", a.name) } else { format!("[<{}>]", a.name) })
        .collect::<Vec<_>>()
        .join(" ")
}

/// One line of a table: a name, its dim arguments, a note (`(required)`) and
/// the text that wraps under its own column.
pub struct Row {
    pub name: String,
    pub args: String,
    pub note: String,
    pub text: String,
    pub stub: bool,
}

impl Row {
    pub fn new(name: impl Into<String>, args: impl Into<String>, text: impl Into<String>) -> Row {
        Row { name: name.into(), args: args.into(), note: String::new(), text: text.into(), stub: false }
    }

    /// `c` as listed under the group at `below` segments: its path past the group.
    fn command(c: &Command, below: usize) -> Row {
        let mut text = list_description(c);
        if !c.implemented {
            text.push_str(" (not yet implemented)");
        }
        Row { stub: !c.implemented, ..Row::new(c.path[below..].join(" "), signature(c), text) }
    }

    fn left_len(&self) -> usize {
        self.name.chars().count() + if self.args.is_empty() { 0 } else { 1 + self.args.chars().count() }
    }
}

/// The name column for `rows`: as wide as the widest left cell, capped so the
/// text keeps most of the line.
fn column(rows: &[Row], t: &Term) -> usize {
    let cap = (t.width / 3).max(16);
    rows.iter().map(Row::left_len).max().unwrap_or(0).min(cap)
}

/// `rows` at `indent`, name cells padded to `col`, text wrapped to the width
/// and continued under its own column. A cell wider than `col` sits alone on
/// its line with the text hanging below.
fn render_rows(rows: &[Row], indent: usize, col: usize, t: &Term) -> Vec<String> {
    let st = t.style;
    let avail = t.width.saturating_sub(indent + col + 2).max(20);
    let pad = " ".repeat(indent);
    let hang = " ".repeat(indent + col + 2);
    let mut out = Vec::new();
    for r in rows {
        let name = if r.stub { st.stub(&r.name) } else { st.name(&r.name) };
        let left = if r.args.is_empty() { name } else { format!("{name} {}", st.args(&r.args)) };
        let full = [r.note.as_str(), r.text.as_str()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" ");
        let mut lines = wrap(&full, avail).into_iter().enumerate().map(|(i, l)| {
            if r.stub {
                return st.stub(&l);
            }
            match l.strip_prefix(&r.note).filter(|_| i == 0 && !r.note.is_empty()) {
                Some(rest) if r.note.starts_with("(required") => format!("{}{rest}", st.required(&r.note)),
                Some(rest) => format!("{}{rest}", st.args(&r.note)),
                None => l,
            }
        });
        let first = lines.next().unwrap_or_default();
        if r.left_len() <= col {
            let gap = " ".repeat(col - r.left_len());
            out.push(format!("{pad}{left}{gap}  {first}").trim_end().to_string());
        } else {
            out.push(format!("{pad}{left}"));
            if !first.is_empty() {
                out.push(format!("{hang}{first}"));
            }
        }
        out.extend(lines.map(|l| format!("{hang}{l}")));
    }
    out
}

/// `usage: <synopsis>`, wrapped with the continuation hanging under the synopsis.
pub fn usage_line(synopsis: &str, t: &Term) -> String {
    let hang = " ".repeat("usage: ".len());
    let lines = wrap(synopsis, t.width.saturating_sub(hang.len()));
    let rest: Vec<String> = lines.iter().skip(1).map(|l| format!("{hang}{l}")).collect();
    std::iter::once(format!("{} {}", t.style.heading("usage:"), lines[0])).chain(rest).collect::<Vec<_>>().join("\n")
}

/// A table of `rows` on its own, indented two columns.
pub fn table(rows: &[Row], t: &Term) -> String {
    render_rows(rows, 2, column(rows, t), t).join("\n")
}

/// Leaves in listing order: implemented before stubs, and the members of a
/// deeper subgroup (`mail outbox …`) kept together at the place their first
/// member was registered. Registration order otherwise.
fn ordered<'a>(cmds: Vec<&'a Command>) -> Vec<&'a Command> {
    let mut keyed: Vec<(bool, usize, &Command)> = cmds
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let cluster = if c.path.len() > 2 {
                cmds.iter().position(|o| o.path.starts_with(&c.path[..2])).unwrap_or(i)
            } else {
                i
            };
            (!c.implemented, cluster, *c)
        })
        .collect();
    keyed.sort_by_key(|(stub, cluster, _)| (*stub, *cluster));
    keyed.into_iter().map(|(_, _, c)| c).collect()
}

fn visible<'a>(registry: &'a Registry) -> impl Iterator<Item = &'a Command> {
    registry.commands().filter(|c| !c.internal)
}

/// Where `head` falls in the layout (unlisted heads and stubs after every
/// listed one); ties in a did-you-mean break by this.
pub fn rank(registry: &Registry, head: &str) -> usize {
    let layout = registry.layout();
    layout
        .sections
        .iter()
        .flat_map(|(_, heads)| heads.iter())
        .position(|(h, _)| *h == head)
        .unwrap_or(usize::MAX)
}

struct Head<'a> {
    name: &'static str,
    commands: Vec<&'a Command>,
    brief: &'static str,
}

impl Head<'_> {
    fn stub(&self) -> bool {
        self.commands.iter().all(|c| !c.implemented)
    }

    fn row(&self) -> Row {
        let [only] = self.commands.as_slice() else {
            let text = if !self.brief.is_empty() {
                self.brief.to_string()
            } else if let Some(bare) = self.commands.iter().find(|c| c.path.len() == 1) {
                list_description(bare)
            } else {
                let leaves: Vec<&str> = self.commands.iter().map(|c| c.path[c.path.len() - 1]).collect();
                leaves.join(", ")
            };
            let note = format!("({} commands)", self.commands.len());
            return Row { stub: self.stub(), note, ..Row::new(self.name, "", text) };
        };
        Row { stub: self.stub(), ..Row::new(only.path.join(" "), "", list_description(only)) }
    }
}

/// The heads in listing order, by section: the layout's sections in order,
/// then any unlisted head under "Other", then every all-stub head under
/// "Not yet implemented". Internal commands are not listed.
fn sections<'a>(registry: &'a Registry) -> Vec<(&'static str, Vec<Head<'a>>)> {
    let mut heads: Vec<Head<'a>> = Vec::new();
    for c in visible(registry) {
        match heads.iter_mut().find(|h| h.name == c.path[0]) {
            Some(h) => h.commands.push(c),
            None => heads.push(Head { name: c.path[0], commands: vec![c], brief: "" }),
        }
    }
    let mut out: Vec<(&'static str, Vec<Head<'a>>)> = Vec::new();
    for (section, listed) in registry.layout().sections {
        let mut members = Vec::new();
        for (name, brief) in *listed {
            if let Some(i) = heads.iter().position(|h| h.name == *name && !h.stub()) {
                let mut h = heads.remove(i);
                h.brief = brief;
                members.push(h);
            }
        }
        if !members.is_empty() {
            out.push((section, members));
        }
    }
    let (stubs, rest): (Vec<_>, Vec<_>) = heads.into_iter().partition(Head::stub);
    if !rest.is_empty() {
        out.push((OTHER, rest));
    }
    if !stubs.is_empty() {
        out.push((STUBS, stubs));
    }
    out
}

/// The listing's inventory for a prose consumer (`guide`): each section with
/// its heads as `(name, listed commands, of them stubs)`, in listing order.
pub fn inventory(registry: &Registry) -> Vec<(&'static str, Vec<(&'static str, usize, usize)>)> {
    sections(registry)
        .into_iter()
        .map(|(section, heads)| {
            let rows = heads
                .iter()
                .map(|h| (h.name, h.commands.len(), h.commands.iter().filter(|c| !c.implemented).count()))
                .collect();
            (section, rows)
        })
        .collect()
}

fn heading(t: &Term, section: &str) -> String {
    if section == STUBS {
        t.style.stub(section)
    } else {
        t.style.heading(section)
    }
}

/// What bare `aoide`, `aoide --help` and `aoide help` print: the tagline, one
/// line per head under its section, and the pointer to the rest.
pub fn overview(registry: &Registry, bin: &str, t: &Term) -> String {
    let st = t.style;
    let groups = sections(registry);
    let rows: Vec<Row> = groups.iter().flat_map(|(_, heads)| heads.iter().map(Head::row)).collect();
    let col = column(&rows, t);
    let tagline = registry.layout().tagline;
    let title = if tagline.is_empty() { bin.to_string() } else { format!("{bin} — {tagline}") };
    let mut out: Vec<String> = wrap(&title, t.width);
    out[0] = out[0].replacen(bin, &st.heading(bin), 1);
    out.push(usage_line(&format!("{bin} <command> [args] [--json]"), t));
    let mut at = 0;
    for (section, heads) in &groups {
        out.push(String::new());
        out.push(heading(t, section));
        out.extend(render_rows(&rows[at..at + heads.len()], 2, col, t));
        at += heads.len();
    }

    let external = crate::bin::discover_external(bin);
    if !external.is_empty() {
        let ext: Vec<Row> = external.iter().map(|(name, path)| Row::new(name.clone(), "", path.display().to_string())).collect();
        out.push(String::new());
        out.push(st.heading(&format!("external ({bin}-* on PATH, not part of this schema)")));
        out.push(table(&ext, t));
    }

    out.push(String::new());
    out.extend(wrap(
        &format!("{bin} <group> lists its commands · {bin} help <cmd> shows one · --json for machines"),
        t.width,
    ));
    out.join("\n")
}

/// The commands strictly below `path`, ordered, for a group's listing and a
/// command's `subcommands:`.
pub fn children(path: &[String], registry: &Registry, t: &Term) -> String {
    let kids: Vec<&Command> = visible(registry)
        .filter(|c| c.path.len() > path.len() && c.path.iter().zip(path).all(|(a, b)| *a == b))
        .collect();
    let rows: Vec<Row> = ordered(kids).into_iter().map(|c| Row::command(c, path.len())).collect();
    table(&rows, t)
}

/// A group's page (`aoide secrets`, `aoide help secrets`, `secrets --help`):
/// its blurb and commands. `None` when `path` is no group.
pub fn group(path: &[String], registry: &Registry, bin: &str, t: &Term) -> Option<String> {
    let listing = children(path, registry, t);
    if listing.is_empty() {
        return None;
    }
    let st = t.style;
    let me = path.join(" ");
    let blurb = registry
        .layout()
        .sections
        .iter()
        .flat_map(|(_, heads)| heads.iter())
        .find(|(h, _)| path.len() == 1 && *h == path[0])
        .map(|(_, b)| *b)
        .unwrap_or_default();
    let mut out = vec![usage_line(&format!("{bin} {me} <command> [args] [--json]"), t)];
    if !blurb.is_empty() {
        out.push(String::new());
        out.extend(wrap(blurb, t.width));
    }
    out.extend([String::new(), st.heading("commands:"), listing, String::new()]);
    out.extend(wrap(&format!("Run '{bin} {me} <command> --help' for args, flags, and examples."), t.width));
    Some(out.join("\n"))
}

/// Names as a wrapped comma list after `label` (shown as given, `plain_len`
/// columns wide, gap included), each name in the command color.
fn names(label: &str, plain_len: usize, names: &[&str], stub: bool, t: &Term) -> Vec<String> {
    let st = t.style;
    let hang = " ".repeat(plain_len);
    let paint = |n: &str| if stub { st.stub(n) } else { st.name(n) };
    wrap(&names.join(", "), t.width.saturating_sub(plain_len).max(20))
        .into_iter()
        .enumerate()
        .map(|(i, line)| {
            let painted = line
                .split(' ')
                .map(|w| match w.strip_suffix(',') {
                    Some(n) => format!("{},", paint(n)),
                    None => paint(w),
                })
                .collect::<Vec<_>>()
                .join(" ");
            if i == 0 {
                format!("{label}{painted}")
            } else {
                format!("{hang}{painted}")
            }
        })
        .collect()
}

/// What can be typed after `positionals`' longest valid prefix: at the root,
/// every head under its section; below it, the next segments in listing order.
pub fn choices(positionals: &[String], registry: &Registry, t: &Term) -> String {
    let st = t.style;
    let mut depth = 0;
    while depth < positionals.len()
        && registry
            .commands()
            .any(|c| c.path.len() > depth && c.path[..=depth].iter().zip(positionals).all(|(a, b)| a == b))
    {
        depth += 1;
    }
    if depth == 0 {
        let groups = sections(registry);
        let width = groups.iter().map(|(s, _)| s.chars().count()).max().unwrap_or(0);
        let mut out = vec![st.heading("commands:")];
        for (section, heads) in &groups {
            let list: Vec<&str> = heads.iter().map(|h| h.name).collect();
            let pad = " ".repeat(width - section.chars().count());
            let label = format!("  {}{pad}  ", heading(t, section));
            out.extend(names(&label, 4 + width, &list, *section == STUBS, t));
        }
        return out.join("\n");
    }
    let kids: Vec<&Command> = visible(registry)
        .filter(|c| c.path.len() > depth && c.path[..depth].iter().zip(positionals).all(|(a, b)| a == b))
        .collect();
    let mut next: Vec<&str> = Vec::new();
    for c in ordered(kids) {
        if !next.contains(&c.path[depth]) {
            next.push(c.path[depth]);
        }
    }
    let label = format!("`{}` has: ", positionals[..depth].join(" "));
    names(&label, label.chars().count(), &next, false, t).join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_desc_keeps_the_first_sentence_only() {
        assert_eq!(short_desc("One sentence. Two sentences."), "One sentence.");
        // A newline also ends the "sentence"; no dangling whitespace.
        assert_eq!(short_desc("First line\nsecond line"), "First line");
        // Short summaries pass through whole.
        assert_eq!(short_desc("Terse."), "Terse.");
    }

    #[test]
    fn short_desc_truncates_a_chatty_opener_at_a_word_boundary() {
        let long = "This opener runs on and on well past the cap without a single period to stop it anywhere at all.";
        let out = short_desc(long);
        assert!(out.ends_with('…'), "ellipsis marks the cut: {out}");
        assert!(out.chars().count() <= BRIEF_MAX + 1, "cap + ellipsis: {}", out.len());
        let body = out.trim_end_matches('…');
        assert!(long.starts_with(body), "never invents text: {out}");
        // Word-boundary cut: the next char in the source after the kept body
        // is whitespace (nothing half-swallowed).
        let next = long[body.len()..].chars().next();
        assert!(next.is_none_or(|ch| ch.is_whitespace()), "mid-word cut: {out}");
    }

    #[test]
    fn short_desc_is_multibyte_safe_at_the_cap() {
        // ▶ is 3 bytes — byte-naive truncation at the cap would panic; the
        // char-based cut must not.
        let s = format!("{} watch the ▶ marker glide past the truncation cap without a panic.", "x".repeat(50));
        let out = short_desc(&s);
        assert!(out.ends_with('…'));
        // And a short string containing ▶ passes through untouched.
        assert_eq!(short_desc("Highlight ▶ node."), "Highlight ▶ node.");
    }

    use crate::registry::{Layout, Registry};
    use crate::style::Style;

    fn noop(_: &crate::invocation::Invocation) -> crate::output::Outcome {
        crate::output::Outcome::ok("noop", "")
    }

    fn leaf(path: &'static [&'static str], summary: &'static str, brief: &'static str) -> Command {
        Command { path, summary, brief, handler: noop, ..Command::BLANK }
    }

    const LAYOUT: Layout = Layout {
        tagline: "run the shop",
        sections: &[
            ("Start here", &[("guide", "")]),
            ("Work", &[("job", "Jobs: start, stop, list and clean up after them"), ("ship", "")]),
        ],
    };

    fn shop() -> Registry {
        let mut r = Registry::new();
        r.insert(Command { implemented: false, ..leaf(&["later"], "Not built yet.", "") });
        r.insert(leaf(&["job", "start"], "Start a job.", "Start a job by name."));
        r.insert(Command { internal: true, ..leaf(&["job", "hook"], "Hook plumbing.", "") });
        r.insert(leaf(&["guide"], "Print the guide.", "Print the guide."));
        r.insert(leaf(&["job", "stop"], "Stop a job.", "Stop a running job."));
        r.insert(leaf(
            &["ship"],
            "Send the shop's goods to a customer, waiting for each parcel to be signed for.",
            "Send the shop's goods to a customer, waiting for each parcel to be signed for.",
        ));
        r.insert(Command { implemented: false, ..leaf(&["job", "soon"], "Will list.", "Will list jobs.") });
        r.insert(leaf(&["job", "queue", "add"], "Queue a job.", "Queue a job."));
        r.insert(leaf(&["job", "queue", "drop"], "Drop a queued job.", "Drop a queued job."));
        r.insert(leaf(&["job", "list"], "List jobs.", "List jobs."));
        r.arrange(LAYOUT);
        r
    }

    fn term(width: usize, on: bool) -> Term {
        Term { style: if on { Style::ON } else { Style::OFF }, width }
    }

    fn strip_ansi(s: &str) -> String {
        let mut out = String::new();
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                for e in chars.by_ref() {
                    if e == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    fn plain_overview(width: usize) -> String {
        let _guard = crate::bin::path_test_lock().lock().unwrap();
        let saved = std::env::var_os("PATH");
        std::env::set_var("PATH", std::env::temp_dir().join("aoide_help_no_such_dir"));
        let text = overview(&shop(), "shop", &term(width, false));
        match saved {
            Some(v) => std::env::set_var("PATH", v),
            None => std::env::remove_var("PATH"),
        }
        text
    }

    #[test]
    fn the_overview_at_120_columns_is_sectioned_one_line_per_head_with_stubs_last() {
        assert_eq!(
            plain_overview(120),
            "shop — run the shop
usage: shop <command> [args] [--json]

Start here
  guide  Print the guide.

Work
  job    (6 commands) Jobs: start, stop, list and clean up after them
  ship   Send the shop's goods to a customer, waiting for each parcel to be signed for.

Not yet implemented
  later  Not built yet.

shop <group> lists its commands · shop help <cmd> shows one · --json for machines"
        );
    }

    #[test]
    fn at_80_columns_a_long_summary_wraps_under_its_own_column() {
        let text = plain_overview(80);
        assert!(
            text.contains(
                "  ship   Send the shop's goods to a customer, waiting for each parcel to be
         signed for."
            ),
            "{text}"
        );
        assert!(text.lines().all(|l| l.chars().count() <= 80), "{text}");
    }

    #[test]
    fn a_group_lists_its_commands_in_order_clustered_with_stubs_last_and_nothing_internal() {
        let text = group(&["job".to_string()], &shop(), "shop", &term(80, false)).unwrap();
        assert_eq!(
            text,
            "usage: shop job <command> [args] [--json]

Jobs: start, stop, list and clean up after them

commands:
  start       Start a job by name.
  stop        Stop a running job.
  queue add   Queue a job.
  queue drop  Drop a queued job.
  list        List jobs.
  soon        Will list jobs. (not yet implemented)

Run 'shop job <command> --help' for args, flags, and examples."
        );
    }

    #[test]
    fn a_nested_group_lists_exactly_like_a_top_level_one() {
        let text = group(&["job".to_string(), "queue".to_string()], &shop(), "shop", &term(80, false)).unwrap();
        assert_eq!(
            text,
            "usage: shop job queue <command> [args] [--json]

commands:
  add   Queue a job.
  drop  Drop a queued job.

Run 'shop job queue <command> --help' for args, flags, and examples."
        );
        let on = group(&["job".to_string(), "queue".to_string()], &shop(), "shop", &term(80, true)).unwrap();
        assert_eq!(strip_ansi(&on), text, "color only adds escape codes");
        assert!(on.contains("\x1b[36madd\x1b[0m"), "{on:?}");
    }

    #[test]
    fn color_only_adds_escape_codes_the_text_is_otherwise_identical() {
        let on = group(&["job".to_string()], &shop(), "shop", &term(80, true)).unwrap();
        let off = group(&["job".to_string()], &shop(), "shop", &term(80, false)).unwrap();
        assert!(on.contains("\x1b[1musage:\x1b[0m") && on.contains("\x1b[36mstart\x1b[0m"), "{on:?}");
        assert!(on.contains("\x1b[2msoon\x1b[0m"), "a stub is dim: {on:?}");
        assert_eq!(strip_ansi(&on), off);
        assert!(!off.contains('\x1b'));
    }

    #[test]
    fn a_cell_wider_than_the_column_cap_sits_alone_and_its_text_hangs_below() {
        let mut r = Registry::new();
        r.insert(Command {
            args: &[
                crate::registry::Arg { name: "alpha", ty: "string", required: true, description: "", ..crate::registry::Arg::NONE },
                crate::registry::Arg { name: "bravo", ty: "string", required: true, description: "", ..crate::registry::Arg::NONE },
                crate::registry::Arg { name: "charlie", ty: "string", required: false, description: "", ..crate::registry::Arg::NONE },
            ],
            ..leaf(&["g", "long"], "x", "Takes many words to say.")
        });
        r.insert(leaf(&["g", "ab"], "x", "Short."));
        let text = children(&["g".to_string()], &r, &term(60, false));
        assert_eq!(
            text,
            "  long <alpha> <bravo> [<charlie>]
                        Takes many words to say.
  ab                    Short."
        );
    }

    #[test]
    fn unknown_names_list_by_section_and_wrap_with_a_hanging_indent() {
        let text = choices(&["zzz".to_string()], &shop(), &term(40, false));
        assert_eq!(
            text,
            "commands:
  Start here           guide
  Work                 job, ship
  Not yet implemented  later"
        );
        let deep = choices(&["job".to_string(), "zzz".to_string()], &shop(), &term(30, false));
        assert_eq!(deep, "`job` has: start, stop, queue,\n           list, soon");
    }

    #[test]
    fn a_head_missing_from_the_layout_lists_under_other() {
        let mut r = shop();
        r.insert(leaf(&["stray"], "Stray.", "Stray."));
        let text = overview(&r, "shop", &term(80, false));
        assert!(text.contains("\nOther\n  stray  Stray.\n"), "{text}");
    }

    #[test]
    fn the_columns_variable_sets_the_width_and_the_default_is_80() {
        let _guard = crate::bin::path_test_lock().lock().unwrap();
        let saved = std::env::var_os("COLUMNS");
        std::env::set_var("COLUMNS", "133");
        assert_eq!(crate::style::width(), 133);
        std::env::set_var("COLUMNS", "oops");
        assert!(crate::style::width() >= 40);
        match saved {
            Some(v) => std::env::set_var("COLUMNS", v),
            None => std::env::remove_var("COLUMNS"),
        }
    }
}
