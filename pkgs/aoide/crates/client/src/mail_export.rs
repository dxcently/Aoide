//! `aoide mail export` — one Markdown note per mail THREAD, one block per
//! SEND (`docs/architecture/MAIL.md` "Export"). A read-only projection of the
//! mailbase ([`aoide_storage::mail::read_base`]): it reads, groups, renders,
//! and writes notes under its OWN directory, and advances no cursor, marks
//! nothing, removes nothing, rings nothing.
//!
//! Letter text is untrusted data (root `AGENTS.md` house rule 4). Every byte
//! of a letter reaches a note through exactly one of two doors: inside a
//! fence one backtick longer than any backtick run in the text ([`fence`]),
//! or through the header-line clamps ([`single_line`], [`yaml_quote`]) that
//! every free-form field interpolated OUTSIDE a fence passes through —
//! `header.from.name` is attribution text a peer supplies, and can carry CR,
//! LF and ESC.

use aoide_storage::letter;
use aoide_storage::mail::{Address, Entry, ENTRY_TYPE_LETTER};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// What one export did — the handler's summary line, and its `--json` form.
pub(crate) struct Report {
    pub threads: usize,
    pub written: usize,
    pub unchanged: usize,
    pub dir: PathBuf,
}

/// One thread's letters, in `seq` order, under the key they group by.
struct Thread {
    key: String,
    letters: Vec<Entry>,
}

/// `$AOIDE_STATE_DIR/mail-export/` — the ordinary `state_dir()` resolution
/// (`AOIDE_STATE_DIR` when absolute, else `AOIDE_ROOT/state`), like every
/// other state path. Register §30 gives the export a directory under `state/`
/// until the User's Mneme vault is reachable from this box.
fn default_dir() -> PathBuf {
    aoide_storage::fs::state_dir().join("mail-export")
}

/// A structured letter (`letter::decode` succeeds) with a `threadId` groups by
/// that id; a legacy or unstructured letter — or a structured one with no
/// `threadId` — is its own thread, keyed by its msgid. Both keys are hex by
/// construction (`mail::seal`'s msgid, `LetterContent::validate`'s threadId).
fn thread_key(entry: &Entry) -> String {
    match letter::decode(&entry.envelope.text).and_then(|c| c.thread_id) {
        Some(id) => id,
        None => entry.envelope.msgid.clone(),
    }
}

/// The note's file stem, 17 characters either way and always `[0-9a-z]`. A key
/// that is exactly 64 lowercase hex — what both of [`thread_key`]'s paths mint
/// (`mail::seal`'s msgid, `LetterContent::validate`'s threadId) — keeps its
/// first 16 characters. Anything else gets `x` plus the first 16 hex of its
/// sha256, so the stem can never shorten toward the empty one that would name
/// the hidden file `.md`, and no key, hex or hand-corrupted, can name a path
/// outside the export directory. Stable, so a re-run overwrites the same file.
///
/// Distinct keys can still land on one stem; [`stems`] refuses that run.
fn note_stem(key: &str) -> String {
    let hex = |b: u8| b.is_ascii_digit() || (b'a'..=b'f').contains(&b);
    if key.len() == 64 && key.bytes().all(hex) {
        return key[..16].to_string();
    }
    let digest = Sha256::digest(key.as_bytes());
    let mut stem = String::with_capacity(17);
    stem.push('x');
    for byte in &digest[..8] {
        stem.push_str(&format!("{byte:02x}"));
    }
    stem
}

/// Every `letter` entry, grouped. Receipts and any other kind are skipped: they
/// are delivery bookkeeping, not correspondence. A `BTreeMap`, so notes are
/// written in key order rather than base order.
fn threads(entries: Vec<Entry>) -> Vec<Thread> {
    let mut grouped: BTreeMap<String, Vec<Entry>> = BTreeMap::new();
    for entry in entries {
        if entry.kind == ENTRY_TYPE_LETTER {
            grouped.entry(thread_key(&entry)).or_default().push(entry);
        }
    }
    grouped
        .into_iter()
        .map(|(key, mut letters)| {
            letters.sort_by_key(|e| e.seq);
            Thread { key, letters }
        })
        .collect()
}

/// MAIL.md "Reading"'s free-form clamp — CR, LF and ESC out — for anything
/// rendered outside a fence.
fn single_line(s: &str) -> String {
    s.chars().filter(|c| !matches!(c, '\r' | '\n' | '\x1b')).collect()
}

/// A double-quoted YAML scalar: a frontmatter value can be any byte a peer
/// sent, and an unquoted one carrying a newline or `---` would end the block.
fn yaml_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `node/name`, the box's own address grammar.
fn address(a: &Address) -> String {
    format!("{}/{}", single_line(&a.node), single_line(&a.name))
}

/// The addresses a letter names as its recipients, for both the per-letter `→`
/// list and the thread's `participants`: a structured letter's own To and Cc as
/// the sender declared them (its envelope header names only the ONE copy that
/// reached this box), else the envelope's own `to`.
fn recipients(entry: &Entry) -> (Vec<String>, Vec<String>) {
    match letter::decode(&entry.envelope.text) {
        Some(content) => (content.to.iter().map(address).collect(), content.cc.iter().map(address).collect()),
        None => (vec![address(&entry.envelope.header.to)], Vec::new()),
    }
}

/// A fence one backtick longer than the longest backtick run in `body` (three
/// at minimum): no letter can close its own fence and write markdown outside it
/// — MAIL.md "Reading"'s adaptive fence, applied to the note.
fn fence(body: &str) -> String {
    let mut longest = 0;
    let mut run = 0;
    for c in body.chars() {
        run = if c == '`' { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    "`".repeat(longest.max(2) + 1)
}

/// One thread's entries as the blocks the note renders, in first-appearance
/// order: the fan-out copies of ONE send collapse into a single block, every
/// other entry is a block of its own.
///
/// A `--to`/`--cc` send files one copy per mailbox (`letter_send::send` loops
/// the scalar handler once per recipient), and each copy is sealed on its
/// own: its own `header.to`, its own `sig` and `msgid`, its own `minted_at`
/// and `received_at`. No timestamp and no id is shared across the copies —
/// two copies of one send can even straddle a second boundary — so the key is
/// the signed content they DO share: `(envelope.text, header.from)`. The
/// copy's own mailbox is the tie-break that keeps two separate sends of the
/// same words apart, since a send addresses each mailbox at most once
/// ([`aoide_storage::letter::deduplicate_recipients`]): a copy joins the
/// newest block of its key unless that block has already taken this mailbox.
/// One To+Cc fan-out is one block; the same words sent twice to the same
/// mailbox are two.
fn blocks(letters: &[Entry]) -> Vec<Vec<&Entry>> {
    let mut blocks: Vec<Vec<&Entry>> = Vec::new();
    for entry in letters {
        let mut joined = false;
        if let Some(block) = blocks.iter_mut().rev().find(|b| {
            b[0].envelope.text == entry.envelope.text
                && b[0].envelope.header.from == entry.envelope.header.from
        }) {
            if !block.iter().any(|e| e.envelope.header.to == entry.envelope.header.to) {
                block.push(entry);
                joined = true;
            }
        }
        if !joined {
            blocks.push(vec![entry]);
        }
    }
    blocks
}

/// One thread as one note: frontmatter, heading, then a block per send in
/// `seq` order.
fn note(thread: &Thread, node: &str) -> String {
    let subject = thread
        .letters
        .first()
        .and_then(|e| letter::decode(&e.envelope.text))
        .map(|c| c.subject)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "(no subject)".to_string());
    let mut participants = BTreeSet::new();
    for entry in &thread.letters {
        participants.insert(address(&entry.envelope.header.from));
        let (to, cc) = recipients(entry);
        participants.extend(to);
        participants.extend(cc);
    }
    let first = thread.letters.first().map(|e| e.received_at.as_str()).unwrap_or_default();
    let last = thread.letters.last().map(|e| e.received_at.as_str()).unwrap_or_default();
    let quoted: Vec<String> = participants.iter().map(|p| yaml_quote(p)).collect();

    let mut out = String::new();
    let blocks = blocks(&thread.letters);
    out.push_str("---\n");
    out.push_str("type: mail-thread\n");
    out.push_str(&format!("thread: {}\n", yaml_quote(&thread.key)));
    out.push_str(&format!("subject: {}\n", yaml_quote(&subject)));
    out.push_str(&format!("node: {}\n", yaml_quote(node)));
    out.push_str(&format!("participants: [{}]\n", quoted.join(", ")));
    out.push_str(&format!("first: {}\n", yaml_quote(first)));
    out.push_str(&format!("last: {}\n", yaml_quote(last)));
    out.push_str(&format!("letters: {}\n", blocks.len()));
    out.push_str("---\n");
    out.push_str(&format!("\n# {}\n", single_line(&subject)));

    for block in &blocks {
        let head = block[0];
        let (to, cc) = recipients(head);
        let from = address(&head.envelope.header.from);
        let at = single_line(block.iter().map(|e| e.received_at.as_str()).min().unwrap_or_default());
        out.push_str(&format!("\n## {at} · {from} → {}\n", to.join(", ")));
        if !cc.is_empty() {
            out.push_str(&format!("cc: {}\n", cc.join(", ")));
        }
        // A structured letter's decoded body: its subject, To and Cc are
        // already lifted into the frontmatter and this heading, and the
        // container's JSON escaping would hide the words from a search index.
        // A legacy or invalid body is the envelope's text verbatim — MAIL.md's
        // "invalid structured content displays verbatim".
        let decoded = letter::decode(&head.envelope.text);
        let body = decoded.as_ref().map(|c| c.body.as_str()).unwrap_or(&head.envelope.text);
        let fence = fence(body);
        out.push_str(&format!("\n{fence}\n{body}\n{fence}\n"));
    }
    out
}

/// One file stem per thread, in the same order — or the whole run refused.
/// Two distinct keys can collide (two 64-hex keys sharing their first 16
/// characters, or two sha256 prefixes agreeing), and the alternative to
/// refusing is one thread's note silently overwriting another's, which no
/// re-run ever recovers.
fn stems(threads: &[Thread]) -> Result<Vec<String>, String> {
    let mut taken: BTreeMap<String, &str> = BTreeMap::new();
    let mut stems = Vec::with_capacity(threads.len());
    for thread in threads {
        let stem = note_stem(&thread.key);
        if let Some(other) = taken.get(&stem) {
            return Err(format!(
                "state/mail-export: thread keys {other} and {} both name {stem}.md; nothing written",
                thread.key
            ));
        }
        taken.insert(stem.clone(), &thread.key);
        stems.push(stem);
    }
    Ok(stems)
}

/// Write one note per thread into `dir` (default [`default_dir`]).
///
/// Each note lands atomically ([`aoide_storage::fs::atomic_write`]: a temp file
/// in the same directory, then a rename), and a note whose bytes are already
/// what this run would write is not written at all — a second run leaves every
/// file's inode and mtime alone, which is what makes this safe to run on a
/// timer. Two threads naming one file refuse the run ([`stems`]) before the
/// first byte is written.
pub(crate) fn export(dir: Option<&Path>) -> Result<Report, String> {
    let dir = dir.map(Path::to_path_buf).unwrap_or_else(default_dir);
    let entries = aoide_storage::mail::read_base().map_err(|e| format!("state/mail: {e}"))?;
    let node = aoide_storage::display::local_host_name();
    let threads = threads(entries);
    let names = stems(&threads)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("state/mail-export: {}: {e}", dir.display()))?;
    let mut report = Report { threads: 0, written: 0, unchanged: 0, dir };
    for (thread, stem) in threads.iter().zip(&names) {
        let note = note(thread, &node);
        let path = report.dir.join(format!("{stem}.md"));
        report.threads += 1;
        match std::fs::read(&path) {
            Ok(old) if old == note.as_bytes() => report.unchanged += 1,
            _ => {
                aoide_storage::fs::atomic_write(&path, &note)
                    .map_err(|e| format!("state/mail-export: {}: {e}", path.display()))?;
                report.written += 1;
            }
        }
    }
    Ok(report)
}
