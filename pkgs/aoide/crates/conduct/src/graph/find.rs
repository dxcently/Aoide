//! The query core of `aoide session find` (`docs/architecture/SESSION-FIND.md`,
//! slice B3): the grammar, the matcher and the order, all pure. `split_line`
//! turns the conductor's line into the words a shell's argv would hold, `parse`
//! splits those words into terms and bare words, and `find` ranks a
//! [`FindIndex`] of plain rows. No file is read, no lock is taken, no node is
//! dialled and nothing is written; B5's `load_index` builds the index.
//!
//! Reachable from its own tests only until the command registers it (B5).

use aoide_protocol::state::canonical_state;
use aoide_protocol::suggest::closest;
use aoide_storage::addr::{resolve, LocalCandidate, Resolution};

/// The closed key set. A word whose text before its first colon is not one of
/// these is refused, so a typo can never silently match nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Key {
    Agent,
    Kind,
    State,
    Node,
    Project,
    Role,
    Mailbox,
    Cc,
    Parent,
}

impl Key {
    pub(crate) const ALL: [Key; 9] = [
        Key::Agent,
        Key::Kind,
        Key::State,
        Key::Node,
        Key::Project,
        Key::Role,
        Key::Mailbox,
        Key::Cc,
        Key::Parent,
    ];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Key::Agent => "agent",
            Key::Kind => "kind",
            Key::State => "state",
            Key::Node => "node",
            Key::Project => "project",
            Key::Role => "role",
            Key::Mailbox => "mailbox",
            Key::Cc => "cc",
            Key::Parent => "parent",
        }
    }
}

/// One `key:v1,v2` word: its key, its alternatives as typed, and the word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Term {
    pub(crate) word: String,
    pub(crate) key: Key,
    pub(crate) values: Vec<String>,
}

/// A parsed query. `words` are the non-empty words in the order given; each
/// word is either a term or a bare word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Query {
    pub(crate) words: Vec<String>,
    pub(crate) terms: Vec<Term>,
    pub(crate) bare: Vec<String>,
}

/// A refusal in the taught shape: what was refused, why, and the fix. B5 turns
/// it into the command's refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FindError {
    pub(crate) what: String,
    pub(crate) why: String,
    pub(crate) fix: String,
}

/// How a value answers a word. The order is the grade: `Exact` beats `Prefix`,
/// which beats `Substring`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Grade {
    Substring = 1,
    Prefix = 2,
    Exact = 3,
}

/// One session row as find sees it. A local row comes from this box's stage;
/// a cached row from a far node's `node-cache` file, where `state` is the
/// last-seen state. `None` on a cached row's `reads` or `cc` means the node
/// does not publish that fact, which is not the same as an empty list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Row {
    pub(crate) node: String,
    pub(crate) local: bool,
    pub(crate) session_id: String,
    pub(crate) petname: Option<String>,
    pub(crate) task: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) cwd: String,
    pub(crate) agent: String,
    pub(crate) kind: Option<String>,
    pub(crate) state: String,
    pub(crate) project: Option<String>,
    pub(crate) native_role: Option<String>,
    /// Projects this session leads.
    pub(crate) leads: Vec<String>,
    /// The parent, as `(node, session id)`. A local parent sits on this row's
    /// own node.
    pub(crate) parent: Option<Ident>,
    pub(crate) remote_parent: Option<Ident>,
    /// Mailboxes the session reads. `None` when the node does not publish them.
    pub(crate) reads: Option<Vec<String>>,
    /// Letters the session was cc'd on. `None` when the node does not publish them.
    pub(crate) cc: Option<Vec<Cc>>,
}

/// A session's identity: its node, then its session id.
pub(crate) type Ident = (String, String);

/// One letter the session was cc'd on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Cc {
    pub(crate) sender: String,
    pub(crate) to: Vec<String>,
    pub(crate) subject: String,
    pub(crate) thread_id: String,
}

/// Every row find can answer from, and the box it answers on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FindIndex {
    /// This box's own node name: what `node:self` means, and the host a
    /// `parent:` reference resolves on.
    pub(crate) host: String,
    /// Every registered node name, for a `node/<rest>` reference.
    pub(crate) nodes: Vec<String>,
    pub(crate) rows: Vec<Row>,
}

/// What one term or bare word matched on one row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Matched {
    pub(crate) term: String,
    pub(crate) field: &'static str,
    pub(crate) value: String,
    pub(crate) how: Grade,
}

/// A row that matched every term and bare word, with each one's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Hit<'a> {
    pub(crate) row: &'a Row,
    pub(crate) matched: Vec<Matched>,
}

/// The ranked hits, and how many rows the index held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Found<'a> {
    pub(crate) hits: Vec<Hit<'a>>,
    pub(crate) total: usize,
}

/// The conductor's query line as the words a shell would pass. Spaces separate
/// words. A `"…"` pair keeps its spaces and loses its quotes wherever it sits:
/// `cc:"pr 41"` is the one word `cc:pr 41`. An unclosed quote is refused.
pub(crate) fn split_line(line: &str) -> Result<Vec<String>, FindError> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut open: Option<usize> = None;
    for (col, c) in line.chars().enumerate() {
        match c {
            '"' if open.is_some() => open = None,
            '"' => open = Some(col),
            c if c.is_whitespace() && open.is_none() => {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
            }
            c => word.push(c),
        }
    }
    if let Some(col) = open {
        return Err(FindError {
            what: format!("unclosed quote in `{line}`"),
            why: format!("the quote at column {} never closes", col + 1),
            fix: "close it, for example `cc:\"pr 41\"`".to_string(),
        });
    }
    if !word.is_empty() {
        words.push(word);
    }
    Ok(words)
}

/// Words to a query. A word with a colon is a term: the text before its first
/// colon is the key, and the rest is comma-separated alternatives. Any other
/// word is a bare word. Whitespace-only words are no word.
pub(crate) fn parse(words: &[String]) -> Result<Query, FindError> {
    let words: Vec<String> = words.iter().filter(|w| !w.trim().is_empty()).cloned().collect();
    let mut terms = Vec::new();
    let mut bare = Vec::new();
    for word in &words {
        match word.split_once(':') {
            Some((key, values)) => terms.push(term(word, key, values)?),
            None => bare.push(word.clone()),
        }
    }
    Ok(Query { words, terms, bare })
}

fn term(word: &str, key: &str, values: &str) -> Result<Term, FindError> {
    let folded = key.trim().to_ascii_lowercase();
    let Some(key) = Key::ALL.into_iter().find(|k| k.name() == folded) else {
        let names = Key::ALL.map(Key::name);
        let fix = match closest(&folded, names, 1).first() {
            Some(near) => format!("did you mean `{near}:{values}`?"),
            None => format!("use one of the keys: {}", names.join(", ")),
        };
        return Err(FindError {
            what: format!("unknown key `{key}` in `{word}`"),
            why: format!("a term's key is one of: {}", names.join(", ")),
            fix,
        });
    };
    if values.trim().is_empty() {
        return Err(FindError {
            what: format!("`{word}` has no value"),
            why: "a term needs a value after its colon".to_string(),
            fix: format!("give it one, for example `{}:<value>`", key.name()),
        });
    }
    let values: Vec<String> = values.split(',').map(|v| v.trim().to_string()).collect();
    if values.iter().any(String::is_empty) {
        return Err(FindError {
            what: format!("`{word}` has an empty alternative"),
            why: "commas separate alternatives, so each one needs a value".to_string(),
            fix: format!("drop the extra comma, for example `{}:a,b`", key.name()),
        });
    }
    Ok(Term { word: word.to_string(), key, values })
}

/// `want` against `have`, both case-folded: the whole text, a prefix, a
/// substring, or nothing.
fn grade(want: &str, have: &str) -> Option<Grade> {
    let (want, have) = (want.to_lowercase(), have.to_lowercase());
    if have == want {
        Some(Grade::Exact)
    } else if have.starts_with(&want) {
        Some(Grade::Prefix)
    } else if have.contains(&want) {
        Some(Grade::Substring)
    } else {
        None
    }
}

/// Keep `candidate` when it beats the best so far. A tie keeps the earlier
/// field, so the field order the caller walks is the tie-break.
fn consider(best: &mut Option<Matched>, term: &str, field: &'static str, value: &str, how: Option<Grade>) {
    let Some(how) = how else { return };
    if best.as_ref().is_none_or(|b| how > b.how) {
        *best = Some(Matched { term: term.to_string(), field, value: value.to_string(), how });
    }
}

/// Every row find can answer from, as the resolver's candidates.
fn candidates<'a>(rows: impl Iterator<Item = &'a Row>) -> Vec<LocalCandidate<'a>> {
    rows.map(|r| LocalCandidate {
        session_id: r.session_id.as_str(),
        petname: r.petname.as_deref(),
        role: if r.parent.is_some() { "child" } else { "root" },
    })
    .collect()
}

fn ambiguous(reference: &str, ids: &[String]) -> FindError {
    FindError {
        what: format!("`parent:{reference}` names more than one session"),
        why: format!("candidates: {}", ids.join(", ")),
        fix: "name one by its full session id".to_string(),
    }
}

/// A `parent:` reference, resolved the way `send --to` resolves one. It is
/// case-sensitive, as the resolver is. `Ok(None)` names nothing, and matches no
/// row. A `node/<rest>` reference resolves inside that node's own cached rows,
/// and stays as typed when they do not hold it.
fn resolve_ref(index: &FindIndex, reference: &str) -> Result<Option<Ident>, FindError> {
    let locals = candidates(index.rows.iter().filter(|r| r.local));
    let nodes: Vec<&str> = index.nodes.iter().map(String::as_str).collect();
    match resolve(reference, &index.host, &locals, &nodes) {
        Resolution::Local(id) => Ok(Some((index.host.clone(), id))),
        Resolution::Remote { node, query } => {
            let theirs = candidates(index.rows.iter().filter(|r| !r.local && r.node == node));
            match resolve(&query, &node, &theirs, &[]) {
                Resolution::Local(id) => Ok(Some((node, id))),
                Resolution::Ambiguous(ids) => Err(ambiguous(reference, &ids)),
                _ => Ok(Some((node, query))),
            }
        }
        Resolution::Ambiguous(ids) => Err(ambiguous(reference, &ids)),
        Resolution::NotFound => Ok(None),
    }
}

/// The fields an exact-or-prefix key reads off a row. `mailbox:` reads this
/// box's rows only: a far node's mailboxes are not published here.
fn key_fields(row: &Row, key: Key) -> Vec<(&'static str, String)> {
    match key {
        Key::Agent => vec![("agent", row.agent.clone())],
        Key::Kind => row.kind.iter().map(|k| ("kind", k.clone())).collect(),
        Key::State => vec![("state", row.state.clone())],
        Key::Node => {
            let mut fields = vec![("node", row.node.clone())];
            if row.local {
                fields.push(("node", "self".to_string()));
            }
            fields
        }
        Key::Project => row.project.iter().map(|p| ("project", p.clone())).collect(),
        Key::Role => {
            let mut fields: Vec<(&'static str, String)> = Vec::new();
            fields.extend(row.task.iter().map(|t| ("task", t.clone())));
            fields.extend(row.native_role.iter().map(|n| ("native role", n.clone())));
            if !row.leads.is_empty() {
                fields.push(("lead", "lead".to_string()));
            }
            fields
        }
        Key::Mailbox if !row.local => Vec::new(),
        Key::Mailbox => {
            let mut fields: Vec<(&'static str, String)> = Vec::new();
            fields.extend(row.reads.iter().flatten().map(|n| ("reads", n.clone())));
            fields.extend(row.task.iter().map(|t| ("inbox", t.clone())));
            fields.extend(row.petname.iter().map(|p| ("petname", p.clone())));
            fields
        }
        Key::Cc | Key::Parent => Vec::new(),
    }
}

fn exact_or_prefix(row: &Row, term: &Term, key: Key) -> Option<Matched> {
    let mut best = None;
    for value in &term.values {
        for (field, text) in key_fields(row, key) {
            let how = grade(value, &text).filter(|g| *g != Grade::Substring);
            consider(&mut best, &term.word, field, &text, how);
        }
    }
    best
}

/// `cc:` searches the letters the session was cc'd on: the sender, each To
/// address and the subject take a substring; the thread id takes a prefix.
fn cc_match(row: &Row, term: &Term) -> Option<Matched> {
    let letters = row.cc.as_ref()?;
    let mut best = None;
    for value in &term.values {
        for letter in letters {
            let word = &term.word;
            consider(&mut best, word, "sender", &letter.sender, grade(value, &letter.sender));
            for to in &letter.to {
                consider(&mut best, word, "to", to, grade(value, to));
            }
            consider(&mut best, word, "subject", &letter.subject, grade(value, &letter.subject));
            let thread = grade(value, &letter.thread_id).filter(|g| *g != Grade::Substring);
            consider(&mut best, word, "thread", &letter.thread_id, thread);
        }
    }
    best
}

fn parent_match(row: &Row, term: &Term, idents: &[Option<Ident>]) -> Option<Matched> {
    for (value, ident) in term.values.iter().zip(idents) {
        let Some(ident) = ident else { continue };
        let field = if row.parent.as_ref() == Some(ident) {
            "parent"
        } else if row.remote_parent.as_ref() == Some(ident) {
            "remote parent"
        } else {
            continue;
        };
        return Some(Matched { term: term.word.clone(), field, value: value.clone(), how: Grade::Exact });
    }
    None
}

fn term_match(row: &Row, term: &Term, idents: &[Option<Ident>]) -> Option<Matched> {
    match term.key {
        Key::Cc => cc_match(row, term),
        Key::Parent => parent_match(row, term, idents),
        key => exact_or_prefix(row, term, key),
    }
}

/// A bare word's best field on a row. Its candidates are the petname, the
/// task, the title, the session id, then each cwd path component. A tie keeps
/// the earlier one.
fn bare_match(row: &Row, word: &str) -> Option<Matched> {
    let mut fields: Vec<(&'static str, &str)> = Vec::new();
    fields.extend(row.petname.as_deref().map(|p| ("petname", p)));
    fields.extend(row.task.as_deref().map(|t| ("task", t)));
    fields.extend(row.title.as_deref().map(|t| ("title", t)));
    fields.push(("id", row.session_id.as_str()));
    let mut best = None;
    for (field, text) in fields {
        consider(&mut best, word, field, text, grade(word, text));
    }
    for component in row.cwd.split('/').filter(|c| !c.is_empty()) {
        consider(&mut best, word, "cwd", component, grade(word, component));
    }
    best
}

/// The matched entries for every term and bare word, and the grade of each bare
/// word. `None` when the row fails any one of them.
fn answer(row: &Row, query: &Query, parents: &[Vec<Option<Ident>>]) -> Option<(Vec<Matched>, Vec<Grade>)> {
    let mut matched = Vec::new();
    for (term, idents) in query.terms.iter().zip(parents) {
        matched.push(term_match(row, term, idents)?);
    }
    let mut grades = Vec::new();
    for word in &query.bare {
        let m = bare_match(row, word)?;
        grades.push(m.how);
        matched.push(m);
    }
    Some((matched, grades))
}

/// The state's place in the urgency order: awaiting, working, idle, stopped,
/// then done.
fn urgency(state: &str) -> u8 {
    match canonical_state(state) {
        "awaiting" => 0,
        "working" => 1,
        "idle" => 2,
        "stopped" => 3,
        _ => 4,
    }
}

/// The name a row sorts under: its petname, else its session id.
fn label(row: &Row) -> &str {
    row.petname.as_deref().unwrap_or(&row.session_id)
}

struct Ranked<'a> {
    hit: Hit<'a>,
    weakest: Option<Grade>,
    sum: u32,
}

/// The four keys, in order: the weakest bare-word grade, then the sum of the
/// grades (both higher first); local before cached; the state's urgency; then
/// the label, node and session id. Terms only filter, so they never rank.
fn order(a: &Ranked<'_>, b: &Ranked<'_>) -> std::cmp::Ordering {
    b.weakest
        .cmp(&a.weakest)
        .then(b.sum.cmp(&a.sum))
        .then(b.hit.row.local.cmp(&a.hit.row.local))
        .then(urgency(&a.hit.row.state).cmp(&urgency(&b.hit.row.state)))
        .then_with(|| label(a.hit.row).cmp(label(b.hit.row)))
        .then_with(|| a.hit.row.node.cmp(&b.hit.row.node))
        .then_with(|| a.hit.row.session_id.cmp(&b.hit.row.session_id))
}

/// Rank the rows that answer `query`. A `done` row is left out unless `all`.
/// An ambiguous `parent:` reference is refused before any row is read.
pub(crate) fn find<'a>(index: &'a FindIndex, query: &Query, all: bool) -> Result<Found<'a>, FindError> {
    let parents = query
        .terms
        .iter()
        .map(|term| match term.key {
            Key::Parent => term.values.iter().map(|v| resolve_ref(index, v)).collect::<Result<Vec<_>, FindError>>(),
            _ => Ok(Vec::new()),
        })
        .collect::<Result<Vec<Vec<Option<Ident>>>, FindError>>()?;
    let mut ranked = Vec::new();
    for row in &index.rows {
        if !all && canonical_state(&row.state) == "done" {
            continue;
        }
        if let Some((matched, grades)) = answer(row, query, &parents) {
            ranked.push(Ranked {
                hit: Hit { row, matched },
                weakest: grades.iter().min().copied(),
                sum: grades.iter().map(|g| *g as u32).sum(),
            });
        }
    }
    ranked.sort_by(order);
    Ok(Found {
        hits: ranked.into_iter().map(|r| r.hit).collect(),
        total: index.rows.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A local row on this box (`osaka`), with mailboxes published and no cc.
    fn local(id: &str, petname: &str, state: &str) -> Row {
        Row {
            node: "osaka".into(),
            local: true,
            session_id: id.into(),
            petname: Some(petname.into()),
            task: None,
            title: None,
            cwd: "/home/k/aoide".into(),
            agent: "claude".into(),
            kind: Some("agent".into()),
            state: state.into(),
            project: None,
            native_role: None,
            leads: Vec::new(),
            parent: None,
            remote_parent: None,
            reads: Some(Vec::new()),
            cc: Some(Vec::new()),
        }
    }

    /// A cached row from `node`: its mailboxes and cc's are not published.
    fn cached(node: &str, id: &str, petname: &str, state: &str) -> Row {
        Row {
            node: node.into(),
            local: false,
            reads: None,
            cc: None,
            ..local(id, petname, state)
        }
    }

    fn index(rows: Vec<Row>) -> FindIndex {
        FindIndex { host: "osaka".into(), nodes: vec!["yomi-strix".into()], rows }
    }

    fn query(line: &str) -> Query {
        parse(&split_line(line).expect("the line splits")).expect("the query parses")
    }

    fn run(index: &FindIndex, line: &str) -> Vec<String> {
        let found = find(index, &query(line), false).expect("the query runs");
        found.hits.iter().map(|h| h.row.session_id.clone()).collect()
    }

    #[test]
    fn split_line_gives_the_words_a_shell_would_pass() {
        let cases: [(&str, &[&str]); 6] = [
            ("brave", &["brave"]),
            (r#"cc:"pr 41""#, &["cc:pr 41"]),
            ("agent:kimi   state:working", &["agent:kimi", "state:working"]),
            (r#""two words" x"#, &["two words", "x"]),
            (r#"a"b c"d"#, &["ab cd"]),
            (r#""""#, &[]),
        ];
        for (line, want) in cases {
            assert_eq!(split_line(line).unwrap(), want, "line {line:?}");
        }
    }

    #[test]
    fn an_unclosed_quote_is_refused_with_its_column() {
        let err = split_line(r#"cc:"pr 41"#).unwrap_err();
        assert!(err.what.contains("unclosed quote"), "{err:?}");
        assert!(err.why.contains("column 4"), "{err:?}");
    }

    #[test]
    fn a_term_splits_on_its_first_colon_and_its_values_on_commas() {
        let q = query(r#"state:working,awaiting cc:"Re: PR 41" brave"#);
        assert_eq!(q.terms[0].key, Key::State);
        assert_eq!(q.terms[0].values, ["working", "awaiting"]);
        assert_eq!(q.terms[1].key, Key::Cc);
        assert_eq!(q.terms[1].values, ["Re: PR 41"]);
        assert_eq!(q.bare, ["brave"], "a colon-less word is a bare word");
    }

    #[test]
    fn an_unknown_key_is_refused_with_the_nearest_key_named() {
        let err = parse(&["agnt:kimi".to_string()]).unwrap_err();
        assert!(err.what.contains("agnt"), "{err:?}");
        assert_eq!(err.fix, "did you mean `agent:kimi`?");
        let far = parse(&["zzzzzzzz:x".to_string()]).unwrap_err();
        assert!(far.fix.contains("use one of the keys"), "{far:?}");
    }

    #[test]
    fn an_empty_value_or_an_empty_alternative_is_refused() {
        for word in ["state:", "state: ", "state:a,,b", "state:a,"] {
            assert!(parse(&[word.to_string()]).is_err(), "{word:?} must be refused");
        }
    }

    #[test]
    fn agent_and_kind_match_exactly_or_by_prefix_and_fold_case() {
        let mut kimi = local("k1", "quiet-fern", "idle");
        kimi.agent = "kimi".into();
        let mut shell = local("sh1", "dune", "idle");
        shell.agent = "shell".into();
        shell.kind = Some("shell".into());
        let idx = index(vec![local("c1", "brave-otter", "working"), kimi, shell]);

        assert_eq!(run(&idx, "agent:CLAUDE"), ["c1"]);
        assert_eq!(run(&idx, "agent:kim"), ["k1"]);
        assert_eq!(run(&idx, "agent:claude,kimi"), ["c1", "k1"]);
        assert_eq!(run(&idx, "agent:aude"), Vec::<String>::new(), "a substring does not match");
        assert_eq!(run(&idx, "kind:sh"), ["sh1"]);
        assert_eq!(run(&idx, "kind:agent"), ["c1", "k1"]);
    }

    #[test]
    fn state_matches_exactly_or_by_prefix_and_never_by_substring() {
        let idx = index(vec![local("a", "alpha", "working"), local("b", "bravo", "idle")]);
        assert_eq!(run(&idx, "state:work"), ["a"]);
        assert_eq!(run(&idx, "state:king"), Vec::<String>::new());
        assert_eq!(run(&idx, "state:working,idle"), ["a", "b"]);
    }

    #[test]
    fn node_names_this_box_self_and_matches_other_nodes_by_name() {
        let idx = index(vec![local("a", "alpha", "idle"), cached("yomi-strix", "b", "bravo", "idle")]);
        assert_eq!(run(&idx, "node:self"), ["a"]);
        assert_eq!(run(&idx, "node:yomi"), ["b"]);
        assert_eq!(run(&idx, "node:osaka"), ["a"]);
        assert_eq!(run(&idx, "node:sakaki"), Vec::<String>::new());
    }

    #[test]
    fn project_matches_the_roster_bucket_by_prefix() {
        let mut tagged = local("a", "alpha", "idle");
        tagged.project = Some("aoide".into());
        let idx = index(vec![tagged, local("b", "bravo", "idle")]);
        assert_eq!(run(&idx, "project:aoi"), ["a"]);
        assert_eq!(run(&idx, "project:other"), Vec::<String>::new());
    }

    #[test]
    fn role_matches_the_task_the_native_role_or_a_project_lead() {
        let mut review = local("a", "alpha", "idle");
        review.task = Some("review-pr".into());
        let mut sub = local("b", "bravo", "idle");
        sub.native_role = Some("subagent".into());
        let mut lead = local("c", "charlie", "idle");
        lead.leads = vec!["aoide".into()];
        let idx = index(vec![review, sub, lead]);

        assert_eq!(run(&idx, "role:review"), ["a"]);
        assert_eq!(run(&idx, "role:subagent"), ["b"]);
        assert_eq!(run(&idx, "role:lead"), ["c"]);
        assert_eq!(run(&idx, "role:review,lead"), ["a", "c"]);
        let hit = find(&idx, &query("role:review"), false).unwrap();
        assert_eq!(hit.hits[0].matched[0].field, "task");
        assert_eq!(hit.hits[0].matched[0].value, "review-pr");
        assert_eq!(hit.hits[0].matched[0].how, Grade::Prefix);
    }

    #[test]
    fn mailbox_matches_a_read_mailbox_its_task_inbox_or_its_petname_on_this_box_only() {
        let mut mine = local("a", "brave-otter", "idle");
        mine.reads = Some(vec!["reviewer".into()]);
        mine.task = Some("review-pr".into());
        let mut far = cached("yomi-strix", "b", "brave-otter", "idle");
        far.task = Some("review-pr".into());
        let idx = index(vec![mine, far]);

        assert_eq!(run(&idx, "mailbox:reviewer"), ["a"]);
        assert_eq!(run(&idx, "mailbox:review-pr"), ["a"], "the task inbox");
        assert_eq!(run(&idx, "mailbox:brave"), ["a"], "the petname, here only");
        assert_eq!(run(&idx, "mailbox:review-pr brave"), ["a"]);
    }

    #[test]
    fn cc_searches_the_sender_to_and_subject_and_a_thread_id_by_prefix() {
        let mut row = local("a", "alpha", "idle");
        row.cc = Some(vec![Cc {
            sender: "osaka/builder".into(),
            to: vec!["reviewer".into()],
            subject: "PR 41 review".into(),
            thread_id: "3f2a9c".into(),
        }]);
        let mut far = cached("yomi-strix", "b", "bravo", "idle");
        far.cc = None;
        let idx = index(vec![row, far]);

        assert_eq!(run(&idx, r#"cc:"pr 41""#), ["a"], "the subject, as a prefix");
        assert_eq!(run(&idx, "cc:review"), ["a"], "the subject, as a substring");
        assert_eq!(run(&idx, "cc:builder"), ["a"], "the sender");
        assert_eq!(run(&idx, "cc:reviewer"), ["a"], "a To address");
        assert_eq!(run(&idx, "cc:3f2a"), ["a"], "the thread id, by prefix");
        assert_eq!(run(&idx, "cc:9c"), Vec::<String>::new(), "a thread id is not a substring");
        assert_eq!(run(&idx, "cc:bravo"), Vec::<String>::new(), "a row whose cc is unknown never matches");
    }

    #[test]
    fn parent_resolves_case_sensitively_and_names_a_local_or_remote_parent() {
        let warm = local("warm-id", "warm-birch", "idle");
        let mut child = local("child-id", "child", "working");
        child.parent = Some(("osaka".into(), "warm-id".into()));
        let mut far_child = cached("yomi-strix", "far-id", "far", "working");
        far_child.parent = Some(("yomi-strix".into(), "x".into()));
        let mut far_of_mine = cached("yomi-strix", "far-two", "two", "working");
        far_of_mine.remote_parent = Some(("osaka".into(), "warm-id".into()));
        let idx = index(vec![warm, child, far_child, far_of_mine]);

        assert_eq!(run(&idx, "parent:warm-birch"), ["child-id", "far-two"]);
        assert_eq!(run(&idx, "parent:Warm-Birch"), Vec::<String>::new(), "the reference is case-sensitive");
        assert_eq!(run(&idx, "parent:yomi-strix/x"), ["far-id"], "a node/<rest> reference");
        assert_eq!(run(&idx, "parent:nobody"), Vec::<String>::new());
        let found = find(&idx, &query("parent:warm-birch"), false).unwrap();
        assert_eq!(found.hits[1].matched[0].field, "remote parent");
    }

    #[test]
    fn an_ambiguous_parent_is_a_taught_error_listing_the_candidates() {
        let idx = index(vec![local("id-1", "brave", "idle"), local("id-2", "brave", "idle")]);
        let err = find(&idx, &query("parent:brave"), false).unwrap_err();
        assert!(err.what.contains("brave") && err.what.contains("more than one"), "{err:?}");
        assert_eq!(err.why, "candidates: id-1, id-2");
    }

    #[test]
    fn bare_words_grade_exact_then_prefix_then_substring_on_the_best_field() {
        let mut row = local("a1b2", "brave-otter", "idle");
        row.title = Some("fix the thing".into());
        row.cwd = "/home/k/aoide/wt".into();
        let idx = index(vec![row]);
        let grade_of = |word: &str| {
            let found = find(&idx, &query(word), false).unwrap();
            let m = found.hits[0].matched[0].clone();
            (m.field, m.how)
        };
        assert_eq!(grade_of("brave-otter"), ("petname", Grade::Exact));
        assert_eq!(grade_of("brave"), ("petname", Grade::Prefix));
        assert_eq!(grade_of("otter"), ("petname", Grade::Substring));
        assert_eq!(grade_of("aoide"), ("cwd", Grade::Exact), "a whole cwd component");
        assert_eq!(grade_of("oide"), ("cwd", Grade::Substring), "a fragment of one");
        assert_eq!(grade_of("thing"), ("title", Grade::Substring));
        assert_eq!(grade_of("a1b2"), ("id", Grade::Exact));
    }

    #[test]
    fn a_bare_word_never_matches_across_a_cwd_path_separator() {
        let idx = index(vec![local("a", "alpha", "idle")]);
        assert_eq!(run(&idx, "k/aoi"), Vec::<String>::new());
    }

    #[test]
    fn a_tie_names_the_earlier_field_petname_then_task() {
        let mut row = local("a", "pr-41", "idle");
        row.task = Some("pr-41".into());
        let idx = index(vec![row]);
        let found = find(&idx, &query("pr-41"), false).unwrap();
        assert_eq!(found.hits[0].matched[0].field, "petname");
    }

    #[test]
    fn the_weakest_grade_ranks_first_then_the_sum_of_grades() {
        let rows = vec![
            local("x", "brave-otter", "idle"),
            local("y", "brave", "idle"),
            local("z", "brave-x", "idle"),
        ];
        let mut y = rows[1].clone();
        y.title = Some("otter".into());
        let mut z = rows[2].clone();
        z.title = Some("otter-y".into());
        let idx = index(vec![rows[0].clone(), y, z]);
        assert_eq!(run(&idx, "brave otter"), ["y", "z", "x"], "y: exact, exact; z: prefix, prefix; x: prefix, substring");

        // p: alpha exact, aa prefix (weakest prefix, sum 5); q: alpha prefix,
        // aa prefix (weakest prefix, sum 4). Equal weakest, so the sum decides.
        let mut p = local("p", "alpha", "idle");
        p.title = Some("aardvark".into());
        let mut q = local("q", "alphabet", "idle");
        q.title = Some("aardvark".into());
        let idx = index(vec![q, p]);
        assert_eq!(run(&idx, "alpha aa"), ["p", "q"], "equal weakest grades, so the larger sum first");
    }

    #[test]
    fn a_local_row_comes_before_a_cached_row_on_a_tie() {
        let idx = index(vec![cached("yomi-strix", "b", "brave", "idle"), local("a", "brave", "idle")]);
        assert_eq!(run(&idx, "brave"), ["a", "b"]);
    }

    #[test]
    fn state_urgency_orders_awaiting_working_idle_stopped_then_done() {
        let idx = index(vec![
            local("d", "p-done", "done"),
            local("s", "p-stopped", "stopped"),
            local("i", "p-idle", "idle"),
            local("w", "p-working", "working"),
            local("a", "p-awaiting", "awaiting"),
        ]);
        let all = find(&idx, &query(""), true).unwrap();
        let order: Vec<&str> = all.hits.iter().map(|h| h.row.session_id.as_str()).collect();
        assert_eq!(order, ["a", "w", "i", "s", "d"]);
    }

    #[test]
    fn the_label_then_node_then_session_id_break_what_is_left() {
        let mut unnamed = local("zzz", "unused", "idle");
        unnamed.petname = None;
        let idx = index(vec![unnamed, local("mid", "bravo", "idle"), local("low", "alpha", "idle")]);
        assert_eq!(run(&idx, ""), ["low", "mid", "zzz"], "a row with no petname sorts by its id");

        let idx = index(vec![cached("yomi-strix", "b", "same", "idle"), cached("sakaki", "c", "same", "idle")]);
        assert_eq!(run(&idx, ""), ["c", "b"], "then the node, sakaki before yomi-strix");
    }

    #[test]
    fn done_rows_are_left_out_unless_all() {
        let idx = index(vec![local("a", "alpha", "done"), local("b", "bravo", "idle")]);
        assert_eq!(run(&idx, ""), ["b"]);
        assert_eq!(find(&idx, &query(""), true).unwrap().hits.len(), 2);
        assert_eq!(find(&idx, &query(""), true).unwrap().total, 2);
    }

    #[test]
    fn a_cached_row_matches_its_last_seen_state_and_stays_cached() {
        let idx = index(vec![cached("yomi-strix", "b", "bravo", "idle")]);
        let found = find(&idx, &query("state:idle"), false).unwrap();
        assert_eq!(found.hits.len(), 1);
        assert!(!found.hits[0].row.local, "the renderer shows this as `was`");
    }

    #[test]
    fn terms_filter_and_bare_words_all_must_match() {
        let idx = index(vec![local("a", "brave-otter", "idle")]);
        assert_eq!(run(&idx, "agent:kimi brave"), Vec::<String>::new());
        assert_eq!(run(&idx, "agent:claude brave"), ["a"]);
        assert_eq!(run(&idx, "agent:claude zebra"), Vec::<String>::new());
    }

    #[test]
    fn an_empty_query_lists_every_row_in_the_order_and_matches_nothing_by_name() {
        let idx = index(vec![local("b", "bravo", "idle"), local("a", "alpha", "awaiting")]);
        let found = find(&idx, &query(""), false).unwrap();
        assert_eq!(found.hits.iter().map(|h| h.row.session_id.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        assert!(found.hits.iter().all(|h| h.matched.is_empty()));
    }
}
