//! The asks a door's stream carries, and the answer the app sends for one (plan
//! 3.6; A1 and A2). An ask is answered only at the door whose stream carried it.
//! A yes on an approval is confirmed on a dialog that shows the whole input, so
//! a yes is never given on a view that is cut short.
//!
//! The rules are Eidolon's, from `crates/web/src/user.rs`. An approval offers
//! `yes` and `no`, and a question offers its option labels. Only an approval's
//! `yes` approves: `eidolon_core::user::Consent` is given when the label is
//! `yes`. A question answer, and any `no`, skips the confirm.

use std::net::SocketAddr;
use std::path::Path;

use aoide_desktop_door::Ask;
use aoide_protocol::untrusted::{is_unsafe, visible};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

/// The dialog's cap in bytes. Above it, and above [`CAP_LINES`], no yes is offered.
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
pub(crate) fn dialog(ask: &Ask, label: &str, addr: SocketAddr, token_file: &Path) -> Option<Dialog> {
    let Ask::Approval { input, .. } = ask else {
        return None;
    };
    if label != YES {
        return None;
    }
    let json = canonical(input);
    let capped = json.len() > CAP_BYTES || json.lines().count() > CAP_LINES;
    let text = if capped { curl_line(addr, token_file, ask_id(ask), label) } else { json };
    let digest: [u8; 32] = Sha256::digest(text.as_bytes()).into();
    Some(Dialog { text, capped, digest })
}

/// The dialog a yes shows: the approval's input as canonical JSON, or above the
/// cap the curl line that answers this ask at this door. Above the cap the
/// native dialog offers no yes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dialog {
    text: String,
    capped: bool,
    digest: [u8; 32],
}

impl Dialog {
    /// The text on screen, with every hidden character spelled as an escape.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Whether the dialog is over the cap, so no yes is offered on it.
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

/// The curl line that sends `label` for this ask to this door. It reads the
/// token from the token file itself, so the line names the file and never the
/// token. Every value is one POSIX shell word, with `'` written `'\''`. The
/// body is the JSON from serde_json, with each hidden character spelled as a
/// JSON `\u` escape, so the JSON value is unchanged. The line is not passed
/// through `visible` as a whole: that would double each backslash, and the
/// shell escape and the JSON escapes would read as text no shell parses.
pub fn curl_line(addr: SocketAddr, token_file: &Path, ask_id: u64, label: &str) -> String {
    let body = json!({"ask_id": ask_id, "answer": label}).to_string();
    let body = spell_json(&body);
    let token_file = spelled(&token_file.to_string_lossy());
    format!(
        "curl -sS -X POST -H {host} -H \"Authorization: Bearer $(cat {file})\" -H {kind} --data-binary {body} {url}",
        host = sq(&format!("Host: {addr}")),
        file = sq(&token_file),
        kind = sq("Content-Type: application/json"),
        body = sq(&body),
        url = sq(&format!("http://{addr}/api/answer")),
    )
}

/// The input as canonical JSON, each line through `visible`.
fn canonical(input: &Value) -> String {
    let pretty = serde_json::to_string_pretty(&sorted(input)).expect("a json value serializes");
    pretty.lines().map(visible).collect::<Vec<_>>().join("\n")
}

/// The value with every object's keys in sorted order, at every depth. The
/// order is set here, not left to serde_json's map, which a `preserve_order`
/// feature anywhere in the build would change.
fn sorted(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            Value::Object(keys.into_iter().map(|k| (k.clone(), sorted(&map[k]))).collect::<Map<String, Value>>())
        }
        Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
        other => other.clone(),
    }
}

/// One POSIX shell word: single quotes, each `'` written `'\''`.
fn sq(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// Text with each hidden character spelled as `\u{hex}`. Nothing else changes.
fn spelled(text: &str) -> String {
    text.chars()
        .map(|c| if is_unsafe(c) { format!("\\u{{{:x}}}", c as u32) } else { c.to_string() })
        .collect()
}

/// JSON text with each hidden character spelled as a JSON `\uXXXX` escape. A
/// character past the BMP is written as its UTF-16 surrogate pair, as JSON needs.
fn spell_json(json: &str) -> String {
    let mut out = String::with_capacity(json.len());
    for c in json.chars() {
        if is_unsafe(c) {
            let mut units = [0u16; 2];
            for unit in c.encode_utf16(&mut units) {
                out.push_str(&format!("\\u{unit:04x}"));
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
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
        dialog(&approval(input), "yes", addr(), token()).expect("a yes on an approval has a dialog")
    }

    #[test]
    fn a_five_kib_input_offers_no_yes() {
        let d = dialog_of(json!({"blob": "x".repeat(5 * 1024)}));
        assert!(d.capped());
        assert!(d.text().starts_with("curl "), "{}", d.text());
    }

    #[test]
    fn a_61_line_input_offers_no_yes_and_a_60_line_one_is_shown_whole() {
        let over = dialog_of(json!((0..59).collect::<Vec<u32>>()));
        assert_eq!(over.text().lines().count(), 1, "the capped dialog is the curl line");
        assert!(over.capped());
        let whole = dialog_of(json!((0..58).collect::<Vec<u32>>()));
        assert_eq!(whole.text().lines().count(), 60);
        assert!(!whole.capped());
    }

    #[test]
    fn a_right_to_left_override_and_an_escape_come_out_escaped() {
        let d = dialog_of(json!({"path": "a\u{202e}b\u{1b}c"}));
        assert!(!d.text().contains('\u{202e}'), "{}", d.text());
        assert!(!d.text().contains('\u{1b}'), "{}", d.text());
    }

    #[test]
    fn the_digest_is_the_sha256_of_the_dialog_bytes() {
        let d = dialog_of(json!({"path": "x"}));
        let expect: [u8; 32] = Sha256::digest(d.text().as_bytes()).into();
        assert_eq!(d.digest(), expect);
    }

    #[test]
    fn keys_are_sorted_at_every_depth() {
        let d = dialog_of(json!({"b": 1, "a": {"d": 2, "c": 3}}));
        let text = d.text();
        assert!(text.find("\"a\"") < text.find("\"b\""), "{text}");
        assert!(text.find("\"c\"") < text.find("\"d\""), "{text}");
    }

    #[test]
    fn a_no_and_a_question_answer_have_no_dialog() {
        assert!(dialog(&approval(json!({})), "no", addr(), token()).is_none());
        let question = Ask::Question {
            ask_id: 2,
            prompt: "pick".into(),
            options: vec![aoide_desktop_door::AskOption { label: "yes".into(), description: None }],
        };
        assert!(dialog(&question, "yes", addr(), token()).is_none(), "a question answer skips the confirm");
    }

    #[test]
    fn a_label_is_one_shell_word_with_its_escapes_spelled() {
        let line = curl_line(addr(), Path::new("/run/t/web-4477.token"), 7, "it's $(id)\nx\u{202e}");
        assert!(line.contains(r#"--data-binary '{"answer":"it'\''s $(id)\nx\u202e","ask_id":7}'"#), "{line}");
        assert!(!line.contains('\u{202e}') && !line.contains('\n'), "{line}");
    }

    #[test]
    fn the_curl_line_names_the_token_file_and_never_the_token() {
        let line = curl_line(addr(), Path::new("/run/t/web-4477.token"), 1, "yes");
        assert!(line.contains("$(cat '/run/t/web-4477.token')"), "{line}");
        assert!(line.contains("'http://127.0.0.1:4477/api/answer'"), "{line}");
        let body = json!({"ask_id": 1, "answer": "yes"}).to_string();
        assert!(line.contains(&format!("--data-binary '{body}'")), "{line}");
    }
}
