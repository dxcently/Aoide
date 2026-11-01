//! The asks a door's stream carries, and the answer the app sends for one (plan
//! 3.6; A1 and A2). An ask is answered only at the door whose stream carried it.
//! A yes on an approval is confirmed on a dialog that shows the whole call, so
//! a yes is never given on a view that is cut short. Above the cap the native
//! confirm offers no yes; the app's own pane shows the whole input and takes the
//! yes there, under the digest of exactly what it showed.
//!
//! Core reports the asks too: `aoide ask watch --json` prints a row for each
//! open ask of an Eidolon door, with the ask in its visible form and the
//! fingerprint core derived from it. `Watch` follows that stream and `change`
//! reads one row into a `Held` (the window's record of an ask) or a removal.
//!
//! The rules are Eidolon's, from `crates/web/src/user.rs`. An approval offers
//! `yes` and `no`, and a question offers its option labels. Only an approval's
//! `yes` approves: `eidolon_core::user::Consent` is given when the label is
//! `yes`. A question answer, and any `no`, skips the confirm.

use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use aoide_door::Ask;
use crate::exec::untrusted::{is_unsafe, visible, visible_json, visible_json_line};
use crate::exec::{Event, Exec, Follow};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

/// The dialog's cap in bytes. Above it, or above [`CAP_LINES`], the native confirm offers no yes.
pub const CAP_BYTES: usize = 4 * 1024;
/// The dialog's cap in lines.
pub const CAP_LINES: usize = 60;

const YES: &str = "yes";

/// The id an ask is keyed by within its door.
pub fn ask_id(ask: &Ask) -> u64 {
    match ask {
        Ask::Approval { ask_id, .. } | Ask::Question { ask_id, .. } => *ask_id,
    }
}

/// Whether the ask offers `label`: an approval its two answers, a question its option labels.
pub fn offers(ask: &Ask, label: &str) -> bool {
    match ask {
        Ask::Approval { answers, .. } => answers.iter().any(|answer| answer.as_str() == label),
        Ask::Question { options, .. } => options.iter().any(|option| option.label == label),
    }
}

/// The dialog a yes on this ask shows, or None when the answer skips the confirm.
/// The dialog is the call the approval asks about: its id, tool, prompt, reason
/// when one is set, and input.
pub(crate) fn dialog(ask: &Ask, label: &str) -> Option<Dialog> {
    let Ask::Approval { call_id, tool, prompt, reason, input, .. } = ask else {
        return None;
    };
    if label != YES {
        return None;
    }
    let mut call = json!({"callId": call_id, "tool": tool, "prompt": prompt, "input": input});
    if let Some(reason) = reason {
        call["reason"] = json!(reason);
    }
    let text = visible_json(&call);
    let capped = text.len() > CAP_BYTES || text.lines().count() > CAP_LINES;
    let digest: [u8; 32] = Sha256::digest(text.as_bytes()).into();
    Some(Dialog { text, capped, digest })
}

/// The dialog a yes shows: the approval's call as `visible_json` text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dialog {
    text: String,
    capped: bool,
    digest: [u8; 32],
}

impl Dialog {
    /// The text on screen: the call as `visible_json`, so every character
    /// `visible` would spell is written as an escape.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Whether the text is over the cap, so the native confirm offers no yes on it.
    pub fn capped(&self) -> bool {
        self.capped
    }

    /// The SHA-256 of the text's bytes.
    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }
}

/// An answer prepared from a stored ask: ids and a label, and the dialog a yes
/// shows. It is built from the stored ask only, never from a webview's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepared {
    pub(crate) launch_id: String,
    pub(crate) ask_id: u64,
    pub(crate) label: String,
    pub(crate) dialog: Option<Dialog>,
}

impl Prepared {
    pub fn launch_id(&self) -> &str {
        &self.launch_id
    }

    pub fn ask_id(&self) -> u64 {
        self.ask_id
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    /// The dialog to show before the answer is sent: Some only for a yes on an approval.
    pub fn dialog(&self) -> Option<&Dialog> {
        self.dialog.as_ref()
    }
}

/// What the door answered to a sent answer. A refusal the app makes before any
/// request is a `DoorsError::Ask`, and a 401 or any other error is a
/// `DoorsError::Door`. Neither removes the card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sent {
    /// 204. The card stays until the door's `ask-settled` arrives.
    Answered,
    /// 409. Nothing is pending under that id, so it was answered elsewhere.
    NotPending,
    /// 422. The door did not take the answer, and the card stays.
    Rejected { reason: String },
}

/// What the app showed before its answer: nothing (a no, or a question's
/// answer), the native confirm, or the app's own pane. Each shown dialog
/// carries the digest of the text it showed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shown {
    None,
    Confirm([u8; 32]),
    Pane([u8; 32]),
}

/// Whether what was shown lets this answer go. An answer with no dialog needs
/// none shown. A yes needs the digest of the dialog built now, and a capped yes
/// needs the app's own pane, since the native confirm offers no yes above the cap.
pub fn permits(dialog: Option<&Dialog>, shown: Shown) -> Result<(), String> {
    match (dialog, shown) {
        (None, Shown::None) => Ok(()),
        (None, _) => Err("this answer shows no dialog, so none may be shown for it; nothing was sent".into()),
        (Some(_), Shown::None) => Err("a yes needs its dialog on screen, and none was shown; nothing was sent".into()),
        (Some(d), Shown::Confirm(_)) if d.capped => {
            Err("no yes is offered above the native confirm's cap; nothing was sent, the app's pane takes it".into())
        }
        (Some(d), Shown::Confirm(digest) | Shown::Pane(digest)) if digest == d.digest => Ok(()),
        (Some(_), _) => Err("the ask on screen is not the one stored; nothing was sent".into()),
    }
}

/// The curl line that sends `label` for this ask to this door, for a no or a
/// question's answer typed in a terminal. None for an approval's yes: that
/// answer is taken in the app's pane and never on a line. The line reads the
/// token from its file, so it names the file and never the token. Every value
/// is one POSIX shell word, with `'` written `'\''`. The body is the
/// `visible_json_line` of the answer, so it holds no raw character `visible`
/// would spell.
pub fn curl_line(addr: SocketAddr, token_file: &Path, ask: &Ask, label: &str) -> Option<String> {
    if matches!(ask, Ask::Approval { .. }) && label == YES {
        return None;
    }
    let body = visible_json_line(&json!({"ask_id": ask_id(ask), "answer": label}));
    Some(format!(
        "curl -sS -X POST -H {host} -H \"Authorization: Bearer $(cat {file})\" -H {kind} --data-binary {body} {url}",
        host = sq(&format!("Host: {addr}")),
        file = sq(&token_file.to_string_lossy()),
        kind = sq("Content-Type: application/json"),
        body = sq(&body),
        url = sq(&format!("http://{addr}/api/answer")),
    ))
}

/// One POSIX shell word: single quotes, each `'` written `'\''`.
fn sq(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// One open ask as core's `ask watch` reported it: the window's record of what
/// the page was shown. `ask` is core's visible form of the door's ask, and
/// `fingerprint` is the one core derived from the session, the port, the ask id
/// and that form; an answer names it with `--expect`.
#[derive(Debug, Clone, PartialEq)]
pub struct Held {
    pub session: String,
    pub port: u16,
    pub ask_id: u64,
    pub fingerprint: String,
    pub ask: Ask,
}

/// One option of a question, as the page draws it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OptionView {
    pub label: String,
    pub description: String,
}

/// An ask as the page draws it. `call` is an approval's whole dialog text, and
/// `digest` its SHA-256 in hex; `capped` says the native confirm offers no yes
/// for it. The page is shown the fingerprint and never sends it back.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct View {
    pub session: String,
    pub ask_id: u64,
    pub kind: &'static str,
    pub prompt: String,
    pub answers: Vec<String>,
    pub options: Vec<OptionView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    pub capped: bool,
    pub fingerprint: String,
}

impl Held {
    /// The ask as the page draws it.
    pub fn view(&self) -> View {
        let shown = dialog(&self.ask, YES);
        let (kind, prompt, answers, options) = match &self.ask {
            Ask::Approval { prompt, answers, .. } => ("approval", prompt.clone(), answers.to_vec(), Vec::new()),
            Ask::Question { prompt, options, .. } => {
                let options = options
                    .iter()
                    .map(|o| OptionView { label: o.label.clone(), description: o.description.clone().unwrap_or_default() })
                    .collect();
                ("question", prompt.clone(), Vec::new(), options)
            }
        };
        View {
            session: self.session.clone(),
            ask_id: self.ask_id,
            kind,
            prompt,
            answers,
            options,
            call: shown.as_ref().map(|d| d.text().to_owned()),
            digest: shown.as_ref().map(|d| hex(&d.digest())),
            capped: shown.as_ref().is_some_and(Dialog::capped),
            fingerprint: self.fingerprint.clone(),
        }
    }
}

/// A digest as lowercase hex.
pub fn hex(digest: &[u8; 32]) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// What one line of `ask watch --json` says.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// An ask is open, or open with other content than before.
    Added(Box<Held>),
    /// The ask left the store: answered, settled, or its door went.
    Removed { session: String, port: u16, ask_id: u64 },
    /// The stream closed: core's `end` row, its closing envelope, or the child's end.
    End,
    /// A line that is not a row this app reads, spelled.
    Other(String),
    /// A line past the exec crate's cap, dropped; its length in bytes.
    Dropped(usize),
}

/// Reads one line of `ask watch --json`. A row is an ask only when core's view
/// of it holds no character a terminal must not be handed (core spells them), its
/// ask is one this app reads, and the ask id inside it is the row's. Anything
/// else is `Other`, never a card.
pub fn change(text: &str) -> Change {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return Change::Other(visible(text));
    };
    let closing = value["type"] == "end" || (value["command"].is_string() && value["status"].is_string());
    if closing {
        return Change::End;
    }
    row(&value).unwrap_or_else(|| Change::Other(visible(text)))
}

fn row(value: &Value) -> Option<Change> {
    let session = value["sessionId"].as_str()?.to_owned();
    let port = u16::try_from(value["port"].as_u64()?).ok()?;
    let ask_id = value["askId"].as_u64()?;
    if session.chars().any(is_unsafe) {
        return None;
    }
    match value["type"].as_str()? {
        "removed" => Some(Change::Removed { session, port, ask_id }),
        "added" => {
            let fingerprint = value["fingerprint"].as_str().filter(|f| !f.is_empty() && f.chars().all(|c| c.is_ascii_alphanumeric()))?;
            let ask: Ask = serde_json::from_value(value["ask"].clone()).ok()?;
            if !is_shown(&value["ask"]) || self::ask_id(&ask) != ask_id {
                return None;
            }
            Some(Change::Added(Box::new(Held { session, port, ask_id, fingerprint: fingerprint.to_owned(), ask })))
        }
        _ => None,
    }
}

/// Whether no string or key in `value` holds a character a terminal must not be
/// handed: core's view spells each one, so a raw one is not core's view.
fn is_shown(value: &Value) -> bool {
    match value {
        Value::String(text) => !text.chars().any(is_unsafe),
        Value::Array(items) => items.iter().all(is_shown),
        Value::Object(map) => map.iter().all(|(key, v)| !key.chars().any(is_unsafe) && is_shown(v)),
        _ => true,
    }
}

/// How long `ask watch` may stay silent before it is taken as running. One that
/// refuses exits at once.
const REFUSAL_WAIT: Duration = Duration::from_millis(500);

/// What one event of the stream says; a blank line says nothing.
fn read(event: Event) -> Option<Change> {
    match event {
        Event::Line(bytes) if bytes.iter().all(u8::is_ascii_whitespace) => None,
        Event::Line(bytes) => Some(change(&String::from_utf8_lossy(&bytes))),
        Event::Dropped(bytes) => Some(Change::Dropped(bytes)),
        Event::Eof => Some(Change::End),
    }
}

/// The held `aoide ask watch`: one per window. It prints nothing while no ask is
/// open, so a child that stays up for `REFUSAL_WAIT` is the watch.
pub struct Watch {
    follow: Follow,
    pending: Option<Event>,
    ended: bool,
}

/// Starts `ask watch`. A child that ends at once has refused, and its stderr is
/// the reason, spelled.
pub fn watch(exec: &Exec) -> Result<Watch, String> {
    let follow = exec.follow(&["ask", "watch"], &[], &[]).map_err(|e| visible(&e.to_string()))?;
    match follow.next(REFUSAL_WAIT) {
        Some(Event::Eof) => {
            let ended = follow.end().map_err(|e| visible(&e.to_string()))?;
            let text = String::from_utf8_lossy(&ended.stderr);
            Err(match text.trim() {
                "" => visible(&format!("ask watch exited with {:?}", ended.status.and_then(|s| s.code()))),
                text => visible(text),
            })
        }
        pending => Ok(Watch { follow, pending, ended: false }),
    }
}

impl Watch {
    /// The next change, or `None` when none comes within `wait` or the stream
    /// has ended.
    pub fn next(&mut self, wait: Duration) -> Option<Change> {
        loop {
            if self.ended {
                return None;
            }
            let event = match self.pending.take() {
                Some(event) => event,
                None => self.follow.next(wait)?,
            };
            let Some(change) = read(event) else { continue };
            self.ended = matches!(change, Change::End);
            return Some(change);
        }
    }

    /// Interrupts the child, reaps it, and returns what it printed from the
    /// interrupt on, through its closing line, read as `next` reads. Nothing
    /// follows the end.
    pub fn close(self) -> Result<Vec<Change>, String> {
        let Watch { follow, mut ended, .. } = self;
        let rest = follow.end().map_err(|e| visible(&e.to_string()))?.rest;
        let mut out = Vec::new();
        for change in rest.into_iter().filter_map(read) {
            if ended {
                break;
            }
            ended = matches!(change, Change::End);
            out.push(change);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::untrusted::is_unsafe;
    use serde_json::Value;
    use std::net::Ipv4Addr;

    fn addr() -> SocketAddr {
        SocketAddr::from((Ipv4Addr::LOCALHOST, 4477))
    }

    fn token() -> &'static Path {
        Path::new("/run/user/1000/eidolon/web-4477.token")
    }

    fn approval(input: Value) -> Ask {
        Ask::Approval {
            ask_id: 1,
            call_id: "tu1".into(),
            tool: "write".into(),
            input,
            prompt: "write?".into(),
            reason: None,
            structural: false,
            judged: None,
            yolo: false,
            answers: ["yes".into(), "no".into()],
        }
    }

    fn dialog_of(input: Value) -> Dialog {
        dialog(&approval(input), "yes").expect("a yes on an approval has a dialog")
    }

    #[test]
    fn a_five_kib_input_is_capped_and_its_dialog_holds_the_whole_input() {
        let d = dialog_of(json!({"blob": "x".repeat(5 * 1024)}));
        assert!(d.capped());
        assert!(d.text().starts_with("{\n"), "{}", d.text());
    }

    #[test]
    fn a_61_line_dialog_is_capped_and_a_60_line_one_is_whole() {
        let over = dialog_of(json!((0..54).collect::<Vec<u32>>()));
        assert!(over.capped());
        assert_eq!(over.text().lines().count(), 61);
        let whole = dialog_of(json!((0..53).collect::<Vec<u32>>()));
        assert!(!whole.capped());
        assert_eq!(whole.text().lines().count(), 60);
    }

    #[test]
    fn a_hidden_character_comes_out_spelled_in_the_dialog() {
        let d = dialog_of(json!({"path": "a\u{202e}b\u{9b}c"}));
        assert!(!d.text().chars().filter(|c| *c != '\n').any(is_unsafe), "{}", d.text());
        assert!(d.text().contains(r"\u202e") && d.text().contains(r"\u009b"), "{}", d.text());
    }

    #[test]
    fn the_digest_is_the_sha256_of_the_dialog_bytes() {
        let d = dialog_of(json!({"path": "x"}));
        let expect: [u8; 32] = Sha256::digest(d.text().as_bytes()).into();
        assert_eq!(d.digest(), expect);
    }

    #[test]
    fn the_dialog_is_the_call_with_its_reason_only_when_set() {
        let write = dialog_of(json!({"path": "x"}));
        assert!(write.text().contains("\"tool\": \"write\""), "{}", write.text());
        assert!(!write.text().contains("reason"), "{}", write.text());
        let mut read = approval(json!({"path": "x"}));
        if let Ask::Approval { tool, .. } = &mut read {
            *tool = "read".into();
        }
        let other = dialog(&read, "yes").expect("a yes has a dialog");
        assert_ne!(other.digest(), write.digest(), "a changed tool is a changed dialog");
        let mut reasoned = approval(json!({"path": "x"}));
        if let Ask::Approval { reason, .. } = &mut reasoned {
            *reason = Some("why".into());
        }
        let shown = dialog(&reasoned, "yes").expect("a yes has a dialog");
        assert!(shown.text().contains("\"reason\": \"why\""), "{}", shown.text());
    }

    #[test]
    fn two_asks_that_differ_only_in_key_order_have_one_dialog_and_one_digest() {
        let ordered = dialog_of(json!({"a": 1, "b": 2}));
        let reversed = dialog_of(json!({"b": 2, "a": 1}));
        assert_eq!(ordered.text(), reversed.text(), "the keys are sorted: {}", ordered.text());
        assert_eq!(ordered.digest(), reversed.digest());
    }

    #[test]
    fn an_absent_reason_is_left_out_and_an_empty_one_is_shown_empty() {
        let absent = dialog_of(json!({"path": "x"}));
        assert!(!absent.text().contains("reason"), "{}", absent.text());
        let mut empty = approval(json!({"path": "x"}));
        if let Ask::Approval { reason, .. } = &mut empty {
            *reason = Some(String::new());
        }
        let empty = dialog(&empty, "yes").expect("a yes has a dialog");
        assert!(empty.text().contains("\"reason\": \"\""), "{}", empty.text());
        assert_ne!(absent.digest(), empty.digest(), "a reason that is set is part of the dialog");
    }

    #[test]
    fn a_no_and_a_question_answer_have_no_dialog() {
        assert!(dialog(&approval(json!({})), "no").is_none());
        let question = Ask::Question {
            ask_id: 2,
            prompt: "pick".into(),
            options: vec![aoide_door::AskOption { label: "yes".into(), description: None }],
        };
        assert!(dialog(&question, "yes").is_none(), "a question answer skips the confirm");
    }

    #[test]
    fn what_is_shown_must_match_the_dialog_and_a_capped_yes_needs_the_pane() {
        let small = dialog_of(json!({"path": "x"}));
        let big = dialog_of(json!({"blob": "x".repeat(5 * 1024)}));
        let other = [0u8; 32];
        assert!(permits(None, Shown::None).is_ok());
        assert!(permits(None, Shown::Confirm(small.digest())).is_err());
        assert!(permits(Some(&small), Shown::None).is_err());
        assert!(permits(Some(&small), Shown::Confirm(small.digest())).is_ok());
        assert!(permits(Some(&small), Shown::Pane(small.digest())).is_ok());
        assert!(permits(Some(&small), Shown::Confirm(other)).is_err(), "a digest of other text is refused");
        assert!(permits(Some(&small), Shown::Pane(other)).is_err(), "a digest of other text is refused");
        assert!(permits(Some(&big), Shown::Confirm(big.digest())).is_err(), "a capped yes is not taken on the native confirm");
        assert!(permits(Some(&big), Shown::Pane(big.digest())).is_ok(), "a capped yes is taken in the app's pane");
        assert!(permits(Some(&big), Shown::Pane(other)).is_err(), "a digest of other text is refused in the pane");
    }

    #[test]
    fn an_approval_yes_has_no_curl_line_and_a_no_has_one() {
        let ask = approval(json!({}));
        assert!(curl_line(addr(), token(), &ask, "yes").is_none());
        assert!(curl_line(addr(), token(), &ask, "no").is_some());
    }

    #[test]
    fn a_curl_line_body_parses_back_and_holds_no_hidden_character() {
        let label = "it's \u{202e}\u{9b}\n$(id)";
        let line = curl_line(addr(), token(), &approval(json!({})), label).expect("a line for this answer");
        let quoted = line
            .split(" --data-binary ")
            .nth(1)
            .and_then(|rest| rest.split(" 'http").next())
            .expect("the body is quoted");
        let body = quoted
            .strip_prefix('\'')
            .and_then(|b| b.strip_suffix('\''))
            .expect("the body is one shell word")
            .replace(r"'\''", "'");
        let parsed: Value = serde_json::from_str(&body).expect("the body is JSON");
        assert_eq!(parsed, json!({"ask_id": 1, "answer": label}));
        assert!(!body.chars().any(is_unsafe), "{body}");
    }

    #[test]
    fn the_curl_line_names_the_token_file_and_never_the_token() {
        let line = curl_line(addr(), Path::new("/run/t/web-4477.token"), &approval(json!({})), "no")
            .expect("a line for this answer");
        assert!(line.contains("$(cat '/run/t/web-4477.token')"), "{line}");
        assert!(line.contains("'http://127.0.0.1:4477/api/answer'"), "{line}");
        assert!(line.contains(r#"--data-binary '{"answer":"no","ask_id":1}'"#), "{line}");
    }
}
