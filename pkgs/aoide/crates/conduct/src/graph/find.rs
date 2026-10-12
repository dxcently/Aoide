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
use aoide_storage::letter;
use aoide_storage::mail::{Address, Cursors, Entry};
use std::cmp::Ordering;
use std::path::Path;

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
/// last-seen state. A cached row's `mail` is `None`: the node does not publish
/// its mailboxes, which is not the same as having none.
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
    pub(crate) model: Option<String>,
    pub(crate) kind: Option<String>,
    pub(crate) state: String,
    pub(crate) project: Option<String>,
    pub(crate) native_role: Option<String>,
    /// Projects this session leads.
    pub(crate) leads: Vec<String>,
    /// A task run's report mailbox. Task runs only.
    pub(crate) report_to: Option<String>,
    /// The git common directory of `cwd`. Local rows only.
    pub(crate) repo: Option<String>,
    /// The conducted ancestor this session reads mail through: its own id when
    /// it is a conducted wrap, else the nearest conducted ancestor. Local only.
    pub(crate) wrap: Option<String>,
    /// The parent, as `(node, session id)`. A local parent sits on this row's
    /// own node.
    pub(crate) parent: Option<Ident>,
    pub(crate) remote_parent: Option<Ident>,
    /// The children this session spawned on other nodes.
    pub(crate) remote_children: Vec<Ident>,
    pub(crate) mail: Option<MailFacts>,
}

/// What a local session's mailboxes say. [`mail_facts`] fills it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct MailFacts {
    pub(crate) reads: Vec<ReadMailbox>,
    pub(crate) cc: Vec<CcLetter>,
    pub(crate) threads: Vec<Thread>,
}

/// One mailbox the session reads: its unread count, and the wrap it reads
/// through when that is not the session itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReadMailbox {
    pub(crate) name: String,
    pub(crate) unread: u64,
    pub(crate) via: Option<String>,
}

/// A letter the session was cc'd on, as its mailbox holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CcLetter {
    pub(crate) msgid: String,
    pub(crate) thread_id: Option<String>,
    pub(crate) subject: String,
    /// `node/name` of the sender.
    pub(crate) from: String,
    /// `node/name` of each To address.
    pub(crate) to: Vec<String>,
}

/// A thread a letter in one of the session's mailboxes belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Thread {
    pub(crate) id: String,
    pub(crate) subject: String,
}

/// A session's identity: its node, then its session id.
pub(crate) type Ident = (String, String);

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

/// Each `parent:` value resolved once, aligned with `query.terms` (empty for
/// every other term). An ambiguous reference is refused here, before any row.
fn parent_idents(index: &FindIndex, query: &Query) -> Result<Vec<Vec<Option<Ident>>>, FindError> {
    query
        .terms
        .iter()
        .map(|term| match term.key {
            Key::Parent => term.values.iter().map(|v| resolve_ref(index, v)).collect::<Result<Vec<_>, FindError>>(),
            _ => Ok(Vec::new()),
        })
        .collect()
}

/// The identity a row answers to: its node and its session id.
fn identity(row: &Row) -> Ident {
    (row.node.clone(), row.session_id.clone())
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
            fields.extend(row.mail.iter().flat_map(|m| &m.reads).map(|r| ("reads", r.name.clone())));
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
    let letters = &row.mail.as_ref()?.cc;
    let mut best = None;
    for value in &term.values {
        for letter in letters {
            let word = &term.word;
            consider(&mut best, word, "sender", &letter.from, grade(value, &letter.from));
            for to in &letter.to {
                consider(&mut best, word, "to", to, grade(value, to));
            }
            consider(&mut best, word, "subject", &letter.subject, grade(value, &letter.subject));
            if let Some(thread) = &letter.thread_id {
                let by_prefix = grade(value, thread).filter(|g| *g != Grade::Substring);
                consider(&mut best, word, "thread", thread, by_prefix);
            }
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

/// The keys after the grades: local before cached, then the state's urgency,
/// then the label, node and session id. Find and `--related` share it.
fn tiebreak(a: &Row, b: &Row) -> Ordering {
    b.local
        .cmp(&a.local)
        .then(urgency(&a.state).cmp(&urgency(&b.state)))
        .then_with(|| label(a).cmp(label(b)))
        .then_with(|| a.node.cmp(&b.node))
        .then_with(|| a.session_id.cmp(&b.session_id))
}

struct Ranked<'a> {
    hit: Hit<'a>,
    weakest: Option<Grade>,
    sum: u32,
}

/// The four keys, in order: the weakest bare-word grade, then the sum of the
/// grades (both higher first), then [`tiebreak`]. Terms only filter, so they
/// never rank.
fn order(a: &Ranked<'_>, b: &Ranked<'_>) -> Ordering {
    b.weakest
        .cmp(&a.weakest)
        .then(b.sum.cmp(&a.sum))
        .then_with(|| tiebreak(a.hit.row, b.hit.row))
}

/// Rank the rows that answer `query`. A `done` row is left out unless `all`.
/// An ambiguous `parent:` reference is refused before any row is read.
pub(crate) fn find<'a>(index: &'a FindIndex, query: &Query, all: bool) -> Result<Found<'a>, FindError> {
    let parents = parent_idents(index, query)?;
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

/// `address` as `node/name`, the form find shows every sender and recipient in.
fn address(a: &Address) -> String {
    format!("{}/{}", a.node, a.name)
}

/// Fill each local row's [`MailFacts`] from the mailbase, with no I/O.
/// `cursors` is `peek_cursors()`'s reader maps and `entries` is `peek_base()`'s
/// correspondence entries. A row reads through its wrap, so a hooked child
/// counts its wrap's marks, and an unread count is the rule `unread_for` uses.
/// A cc'd letter is one filed to a mailbox the session reads whose Cc names that
/// mailbox: the copy under the mailbox is the fact, since Cc is context.
/// Remote rows keep `None`.
pub(crate) fn mail_facts(rows: &mut [Row], cursors: &Cursors, entries: &[Entry]) {
    for row in rows.iter_mut().filter(|r| r.local) {
        let reader = row.wrap.clone().unwrap_or_else(|| row.session_id.clone());
        let via = (reader != row.session_id).then(|| reader.clone());
        let reads: Vec<ReadMailbox> = cursors
            .iter()
            .filter_map(|(name, readers)| {
                let mark = readers.get(&reader)?.seq;
                let unread = entries
                    .iter()
                    .filter(|e| e.envelope.header.to.name == *name && e.seq > mark)
                    .count();
                Some(ReadMailbox { name: name.clone(), unread: unread as u64, via: via.clone() })
            })
            .collect();
        let mut facts = MailFacts { reads, ..MailFacts::default() };
        for entry in entries {
            let to = &entry.envelope.header.to;
            if !facts.reads.iter().any(|r| r.name == to.name) {
                continue;
            }
            let Some(content) = letter::decode(&entry.envelope.text) else { continue };
            if let Some(id) = &content.thread_id {
                if !facts.threads.iter().any(|t| &t.id == id) {
                    facts.threads.push(Thread { id: id.clone(), subject: content.subject.clone() });
                }
            }
            if content.cc.iter().any(|a| a.name == to.name && a.node == to.node) {
                facts.cc.push(CcLetter {
                    msgid: entry.envelope.msgid.clone(),
                    thread_id: content.thread_id.clone(),
                    subject: content.subject.clone(),
                    from: address(&entry.envelope.header.from),
                    to: content.to.iter().map(address).collect(),
                });
            }
        }
        row.mail = Some(facts);
    }
}

/// The edges a row carries, read off the index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Edges {
    pub(crate) parent: Option<Ident>,
    pub(crate) children: Vec<Ident>,
    pub(crate) siblings: Vec<Ident>,
    pub(crate) wrap: Option<String>,
    pub(crate) remote_parent: Option<Ident>,
    pub(crate) remote_children: Vec<Ident>,
    pub(crate) repo: Option<String>,
}

/// What a session carries beyond its own row: its mailboxes, the letters it was
/// cc'd on, its threads, its edges. `unknown` names each mail fact the node does
/// not publish (`mailboxes` is `reads`); a name there is never an empty list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Carries {
    pub(crate) reads: Option<Vec<ReadMailbox>>,
    /// The task slug and the petname: the names filed mail can reach this
    /// session under.
    pub(crate) inbox: Vec<String>,
    pub(crate) report_to: Option<String>,
    pub(crate) cc: Option<Vec<CcLetter>>,
    pub(crate) threads: Option<Vec<Thread>>,
    pub(crate) edges: Edges,
    pub(crate) unknown: Vec<&'static str>,
}

/// The index's view of one row: its mail facts, its inbox, and its edges.
pub(crate) fn carries(index: &FindIndex, row: &Row) -> Carries {
    let me = identity(row);
    let children = index
        .rows
        .iter()
        .filter(|r| r.parent.as_ref() == Some(&me))
        .map(identity)
        .collect();
    let siblings = match &row.parent {
        Some(parent) => index
            .rows
            .iter()
            .filter(|r| r.parent.as_ref() == Some(parent) && identity(r) != me)
            .map(identity)
            .collect(),
        None => Vec::new(),
    };
    let unknown = if row.mail.is_none() { vec!["mailboxes", "cc", "threads"] } else { Vec::new() };
    Carries {
        reads: row.mail.as_ref().map(|m| m.reads.clone()),
        inbox: row.task.iter().chain(&row.petname).cloned().collect(),
        report_to: row.report_to.clone(),
        cc: row.mail.as_ref().map(|m| m.cc.clone()),
        threads: row.mail.as_ref().map(|m| m.threads.clone()),
        edges: Edges {
            parent: row.parent.clone(),
            children,
            siblings,
            wrap: row.wrap.clone(),
            remote_parent: row.remote_parent.clone(),
            remote_children: row.remote_children.clone(),
            repo: row.repo.clone(),
        },
        unknown,
    }
}

/// One reason two rows are related: its place in the order (1 strongest), its
/// kind, and the phrase `--related` prints for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Reason {
    pub(crate) order: u8,
    pub(crate) kind: &'static str,
    pub(crate) shown: String,
}

/// A row related to the target, with every reason it has, strongest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Related<'a> {
    pub(crate) row: &'a Row,
    pub(crate) reasons: Vec<Reason>,
}

/// The repo's name: the directory its `.git` common dir sits in.
fn repo_name(common: &str) -> String {
    Path::new(common)
        .parent()
        .and_then(Path::file_name)
        .map_or_else(|| common.to_string(), |n| n.to_string_lossy().into_owned())
}

/// Every reason `r` shares with `t`, one per distinct phrase, in order. Mail
/// reasons hold only between local rows, and so does the repo reason.
fn reasons_between(t: &Row, r: &Row) -> Vec<Reason> {
    let (t_id, r_id) = (identity(t), identity(r));
    let mut out: Vec<Reason> = Vec::new();
    let mut add = |order: u8, kind: &'static str, shown: String| out.push(Reason { order, kind, shown });

    let both_local = t.local && r.local;
    let r_is_t_wrap = both_local && t.wrap.as_deref() == Some(r.session_id.as_str());
    let t_is_r_wrap = both_local && r.wrap.as_deref() == Some(t.session_id.as_str());
    if t.parent.as_ref() == Some(&r_id) {
        add(1, "lineage", if r_is_t_wrap { "parent (its wrap)" } else { "parent" }.into());
    } else if r_is_t_wrap {
        add(1, "lineage", "parent (its wrap)".into());
    }
    if r.parent.as_ref() == Some(&t_id) {
        add(1, "lineage", if t_is_r_wrap { "child (its wrap)" } else { "child" }.into());
    } else if t_is_r_wrap {
        add(1, "lineage", "child (its wrap)".into());
    }

    if t.remote_parent.as_ref() == Some(&r_id) || r.remote_children.contains(&t_id) {
        add(2, "remote lineage", "remote parent".into());
    }
    if r.remote_parent.as_ref() == Some(&t_id) || t.remote_children.contains(&r_id) {
        add(2, "remote lineage", "remote child".into());
    }

    if t.parent.is_some() && t.parent == r.parent {
        add(3, "sibling", "sibling".into());
    }

    if let (Some(a), Some(b)) = (&t.task, &r.task) {
        if a == b {
            add(4, "same role", format!("same role: {a}"));
        }
    }
    if let (Some(a), Some(b)) = (&t.native_role, &r.native_role) {
        if a == b {
            add(4, "same role", format!("same role: {a}"));
        }
    }
    if !t.leads.is_empty() && !r.leads.is_empty() {
        add(4, "same role", "same role: lead".into());
    }

    if let (true, Some(tm), Some(rm)) = (both_local, &t.mail, &r.mail) {
        for m in &tm.reads {
            if rm.reads.iter().any(|x| x.name == m.name) {
                add(5, "mailbox", format!("same mailbox: {}", m.name));
            }
        }
        for (reports, reader) in [(&r.report_to, tm), (&t.report_to, rm)] {
            if let Some(name) = reports {
                if reader.reads.iter().any(|x| &x.name == name) {
                    add(5, "mailbox", format!("reads its reports: {name}"));
                }
            }
        }
        for th in &tm.threads {
            if rm.threads.iter().any(|x| x.id == th.id) {
                let short: String = th.id.chars().take(4).collect();
                add(6, "thread", format!("shared thread: \"{}\" ({short}…)", th.subject));
            }
        }
    }

    if t.project.is_some() && t.project == r.project {
        add(7, "same project", format!("same project: {}", t.project.as_deref().unwrap_or_default()));
    }

    if let (true, Some(a), Some(b)) = (both_local, &t.repo, &r.repo) {
        if a == b {
            add(8, "same repo", format!("same repo: {}", repo_name(a)));
        }
    }

    let mut unique: Vec<Reason> = Vec::new();
    for reason in out {
        if !unique.contains(&reason) {
            unique.push(reason);
        }
    }
    unique.sort_by_key(|r| r.order);
    unique
}

/// The rows related to `target`, each with its reasons. The target never
/// relates to itself. A `done` row is left out unless `all`, and `narrow`
/// (a query beside `--related`) filters the related rows the way it filters
/// find's. Rows sort by their strongest reason, then by how many reasons they
/// have, then by [`tiebreak`].
pub(crate) fn related<'a>(
    index: &'a FindIndex,
    target: &Row,
    narrow: &Query,
    all: bool,
) -> Result<Vec<Related<'a>>, FindError> {
    let parents = parent_idents(index, narrow)?;
    let me = identity(target);
    let mut found: Vec<Related<'a>> = Vec::new();
    for row in &index.rows {
        if identity(row) == me || (!all && canonical_state(&row.state) == "done") {
            continue;
        }
        if answer(row, narrow, &parents).is_none() {
            continue;
        }
        let reasons = reasons_between(target, row);
        if !reasons.is_empty() {
            found.push(Related { row, reasons });
        }
    }
    found.sort_by(|a, b| {
        let strongest = |x: &Related<'_>| x.reasons.iter().map(|r| r.order).min();
        strongest(a)
            .cmp(&strongest(b))
            .then(b.reasons.len().cmp(&a.reasons.len()))
            .then_with(|| tiebreak(a.row, b.row))
    });
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_storage::{display, letter::LetterContent, mail};

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
            model: None,
            report_to: None,
            repo: None,
            wrap: None,
            parent: None,
            remote_parent: None,
            remote_children: Vec::new(),
            mail: Some(MailFacts::default()),
        }
    }

    /// A cached row from `node`: its mailboxes and cc's are not published.
    fn cached(node: &str, id: &str, petname: &str, state: &str) -> Row {
        Row {
            node: node.into(),
            local: false,
            mail: None,
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

    fn read(name: &str) -> ReadMailbox {
        ReadMailbox { name: name.into(), unread: 0, via: None }
    }

    /// `row` with its mailboxes and threads published, and no cc.
    fn with_mail(mut row: Row, reads: &[&str], threads: Vec<Thread>) -> Row {
        row.mail = Some(MailFacts {
            reads: reads.iter().map(|n| read(n)).collect(),
            threads,
            ..MailFacts::default()
        });
        row
    }

    fn id(node: &str, session: &str) -> Ident {
        (node.into(), session.into())
    }

    fn texts(reasons: &[Reason]) -> Vec<&str> {
        reasons.iter().map(|r| r.shown.as_str()).collect()
    }

    fn related_ids(index: &FindIndex, target: &Row, narrow: &str, all: bool) -> Vec<String> {
        related(index, target, &query(narrow), all)
            .expect("the query runs")
            .iter()
            .map(|r| r.row.session_id.clone())
            .collect()
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
        mine.mail = Some(MailFacts { reads: vec![read("reviewer")], ..MailFacts::default() });
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
        row.mail = Some(MailFacts {
            cc: vec![CcLetter {
                msgid: "m1".into(),
                thread_id: Some("3f2a9c".into()),
                subject: "PR 41 review".into(),
                from: "osaka/builder".into(),
                to: vec!["osaka/reviewer".into()],
            }],
            ..MailFacts::default()
        });
        let idx = index(vec![row, cached("yomi-strix", "b", "bravo", "idle")]);

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

    #[test]
    fn reasons_come_strongest_first_and_each_phrase_once() {
        let mut target = local("t", "alpha", "idle");
        target.parent = Some(id("osaka", "p"));
        target.project = Some("aoide".into());
        target.task = Some("review".into());
        target.native_role = Some("review".into());
        let mut other = local("o", "bravo", "idle");
        other.parent = Some(id("osaka", "p"));
        other.project = Some("aoide".into());
        other.task = Some("review".into());
        other.native_role = Some("review".into());

        let reasons = reasons_between(&target, &other);
        assert_eq!(texts(&reasons), ["sibling", "same role: review", "same project: aoide"]);
        assert_eq!(reasons.iter().map(|r| r.order).collect::<Vec<_>>(), [3, 4, 7]);
    }

    #[test]
    fn a_wrap_is_named_its_wrap_in_a_lineage_reason() {
        let wrap = local("wrap-id", "warm-birch", "idle");
        let mut child = local("child-id", "child", "working");
        child.parent = Some(id("osaka", "wrap-id"));
        child.wrap = Some("wrap-id".into());
        assert_eq!(texts(&reasons_between(&child, &wrap)), ["parent (its wrap)"]);
        assert_eq!(texts(&reasons_between(&wrap, &child)), ["child (its wrap)"]);
    }

    #[test]
    fn a_remote_child_or_parent_is_a_remote_lineage_reason() {
        let mut mine = local("mine", "alpha", "idle");
        mine.remote_children = vec![id("yomi-strix", "far")];
        let far = cached("yomi-strix", "far", "far", "working");
        assert_eq!(texts(&reasons_between(&mine, &far)), ["remote child"]);
        assert_eq!(texts(&reasons_between(&far, &mine)), ["remote parent"]);
    }

    #[test]
    fn a_shared_mailbox_report_and_thread_need_two_local_rows() {
        let thread = Thread { id: "ab".repeat(32), subject: "PR 41".into() };
        let mine = with_mail(local("a", "alpha", "idle"), &["reviewer"], vec![thread.clone()]);
        let mut theirs = with_mail(local("b", "bravo", "idle"), &["reviewer"], vec![thread]);
        theirs.report_to = Some("reviewer".into());

        assert_eq!(
            texts(&reasons_between(&mine, &theirs)),
            ["same mailbox: reviewer", "reads its reports: reviewer", "shared thread: \"PR 41\" (abab…)"],
        );
        let far = cached("yomi-strix", "c", "charlie", "idle");
        assert!(reasons_between(&mine, &far).is_empty(), "a far node's mail is not published here");
    }

    #[test]
    fn a_shared_repo_is_named_by_its_directory_and_holds_only_locally() {
        let mut a = local("a", "alpha", "idle");
        a.repo = Some("/home/k/aoide/.git".into());
        let mut b = local("b", "bravo", "idle");
        b.repo = Some("/home/k/aoide/.git".into());
        assert_eq!(texts(&reasons_between(&a, &b)), ["same repo: aoide"]);
        let mut far = cached("yomi-strix", "c", "charlie", "idle");
        far.repo = Some("/home/k/aoide/.git".into());
        assert!(reasons_between(&a, &far).is_empty());
    }

    #[test]
    fn related_sorts_by_the_strongest_reason_then_the_count_then_the_tiebreak() {
        let mut target = local("t", "alpha", "idle");
        target.parent = Some(id("osaka", "p"));
        target.project = Some("aoide".into());
        let mut sibling = local("s", "sibling", "idle");
        sibling.parent = Some(id("osaka", "p"));
        let mut aaron = local("x", "aaron", "idle");
        aaron.parent = Some(id("osaka", "p"));
        let mut both = local("b", "both", "idle");
        both.parent = Some(id("osaka", "p"));
        both.project = Some("aoide".into());
        let mut project_only = local("q", "only", "idle");
        project_only.project = Some("aoide".into());
        let idx = index(vec![project_only, sibling, aaron, both, target.clone()]);

        assert_eq!(related_ids(&idx, &target, "", false), ["b", "x", "s", "q"]);
    }

    #[test]
    fn related_leaves_out_the_target_and_done_rows_unless_all() {
        let mut target = local("t", "alpha", "idle");
        target.project = Some("aoide".into());
        let mut done = local("d", "delta", "done");
        done.project = Some("aoide".into());
        let mut live = local("l", "lima", "idle");
        live.project = Some("aoide".into());
        let idx = index(vec![target.clone(), done, live]);

        assert_eq!(related_ids(&idx, &target, "", false), ["l"]);
        assert_eq!(related_ids(&idx, &target, "", true), ["l", "d"]);
    }

    #[test]
    fn related_narrows_by_a_query_beside_the_flag() {
        let mut target = local("t", "alpha", "idle");
        target.project = Some("aoide".into());
        let mut kimi = local("k", "kilo", "idle");
        kimi.project = Some("aoide".into());
        kimi.agent = "kimi".into();
        let mut claude = local("c", "charlie", "idle");
        claude.project = Some("aoide".into());
        let idx = index(vec![target.clone(), kimi, claude]);

        assert_eq!(related_ids(&idx, &target, "agent:kimi", false), ["k"]);
    }

    #[test]
    fn carries_names_what_a_node_does_not_publish() {
        let mut far = cached("yomi-strix", "b", "bravo", "idle");
        far.task = Some("review-pr".into());
        let idx = index(vec![far.clone()]);

        let c = carries(&idx, &far);
        assert_eq!(c.unknown, ["mailboxes", "cc", "threads"]);
        assert!(c.reads.is_none() && c.cc.is_none() && c.threads.is_none());
        assert_eq!(c.inbox, ["review-pr", "bravo"]);
    }

    #[test]
    fn carries_reads_the_parent_children_and_siblings_off_the_index() {
        let parent = local("p", "parent", "idle");
        let mut first = local("c1", "first", "working");
        first.parent = Some(id("osaka", "p"));
        let mut second = local("c2", "second", "idle");
        second.parent = Some(id("osaka", "p"));
        let idx = index(vec![parent.clone(), first.clone(), second]);

        assert_eq!(carries(&idx, &parent).edges.children, vec![id("osaka", "c1"), id("osaka", "c2")]);
        let kid = carries(&idx, &first).edges;
        assert_eq!(kid.parent, Some(id("osaka", "p")));
        assert_eq!(kid.siblings, vec![id("osaka", "c2")]);
        assert!(kid.children.is_empty());
    }

    #[test]
    fn a_wrap_reads_its_mailboxes_and_unread_is_the_count_unread_for_gives() {
        let _lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, root) = aoide_test_support::isolated_mail_root("find-b4-unread");
        mail::file_letter("builder", "reviewer", "one").unwrap();
        mail::enrol_reader("reviewer", "wrap-id").unwrap();
        mail::mark("reviewer", Some("wrap-id")).unwrap();
        mail::file_letter("builder", "reviewer", "two").unwrap();
        mail::file_letter("builder", "reviewer", "three").unwrap();
        let (mark, unread) = mail::unread_for("reviewer", Some("wrap-id")).unwrap();
        let want = unread.len() as u64;
        assert_eq!((mark, want), (1, 2));

        let mut child = local("child-id", "child", "working");
        child.wrap = Some("wrap-id".into());
        let wrap = local("wrap-id", "warm-birch", "idle");
        let mut rows = vec![child, wrap];
        mail_facts(&mut rows, &mail::peek_cursors().unwrap(), &mail::peek_base().unwrap());

        let through = &rows[0].mail.as_ref().unwrap().reads;
        assert_eq!(*through, vec![ReadMailbox { name: "reviewer".into(), unread: want, via: Some("wrap-id".into()) }]);
        let own = &rows[1].mail.as_ref().unwrap().reads;
        assert_eq!(*own, vec![ReadMailbox { name: "reviewer".into(), unread: want, via: None }]);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn cc_copies_and_threads_come_from_the_letters_a_row_reads() {
        let _lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, root) = aoide_test_support::isolated_mail_root("find-b4-cc");
        let node = display::local_node_name();
        let thread = "cd".repeat(32);
        let addr = |name: &str| Address { node: node.clone(), name: name.into() };
        let text = |cc: &str| {
            LetterContent {
                subject: "PR 41 review".into(),
                to: vec![addr("reviewer")],
                cc: vec![addr(cc)],
                body: "b".into(),
                thread_id: Some(thread.clone()),
                reply_to: None,
            }
            .encode()
            .expect("the letter encodes")
        };
        let to_qa = mail::file_letter("builder", "qa", &text("qa")).unwrap();
        mail::file_letter("builder", "reviewer", &text("qa")).unwrap();
        mail::enrol_reader("qa", "qa-id").unwrap();
        mail::enrol_reader("reviewer", "rev-id").unwrap();

        let mut rows = vec![
            local("qa-id", "qa-agent", "idle"),
            local("rev-id", "rev-agent", "idle"),
            cached("yomi-strix", "far", "far", "idle"),
        ];
        mail_facts(&mut rows, &mail::peek_cursors().unwrap(), &mail::peek_base().unwrap());

        let qa = rows[0].mail.clone().unwrap();
        assert_eq!(qa.reads.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["qa"]);
        assert_eq!(
            qa.cc,
            vec![CcLetter {
                msgid: to_qa.envelope.msgid.clone(),
                thread_id: Some(thread.clone()),
                subject: "PR 41 review".into(),
                from: format!("{node}/builder"),
                to: vec![format!("{node}/reviewer")],
            }],
        );
        assert_eq!(qa.threads, vec![Thread { id: thread.clone(), subject: "PR 41 review".into() }]);

        let rev = rows[1].mail.clone().unwrap();
        assert!(rev.cc.is_empty(), "the copy to reviewer names qa in cc, not reviewer");
        assert_eq!(rev.threads.len(), 1, "a thread is found through any mailbox the row reads");
        assert!(rows[2].mail.is_none(), "a far node's mail is not published here");
        std::fs::remove_dir_all(&root).ok();
    }
}
