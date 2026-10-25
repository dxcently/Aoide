//! The asks a door's stream carries, and the answer the app sends for one (plan
//! 3.6; A1 and A2). An ask is answered only at the door whose stream carried it.
//! A yes on an approval is confirmed on a dialog that shows the whole input, so
//! a yes is never given on a view that is cut short. Above the cap the native
//! confirm offers no yes; the app's own pane shows the whole input and takes the
//! yes there, under the digest of exactly what it showed (ruling 163).
//!
//! The rules are Eidolon's, from `crates/web/src/user.rs`. An approval offers
//! `yes` and `no`, and a question offers its option labels. Only an approval's
//! `yes` approves: `eidolon_core::user::Consent` is given when the label is
//! `yes`. A question answer, and any `no`, skips the confirm.

use std::net::SocketAddr;
use std::path::Path;

use aoide_desktop_door::Ask;
use aoide_protocol::untrusted::{visible_json, visible_json_line};
use serde_json::json;
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
pub(crate) fn dialog(ask: &Ask, label: &str) -> Option<Dialog> {
    let Ask::Approval { input, .. } = ask else {
        return None;
    };
    if label != YES {
        return None;
    }
    let text = visible_json(input);
    let capped = text.len() > CAP_BYTES || text.lines().count() > CAP_LINES;
    let digest: [u8; 32] = Sha256::digest(text.as_bytes()).into();
    Some(Dialog { text, capped, digest })
}

/// The dialog a yes shows: the approval's whole input as `visible_json` text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dialog {
    text: String,
    capped: bool,
    digest: [u8; 32],
}

impl Dialog {
    /// The text on screen: the input as `visible_json`, so every character
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

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_protocol::untrusted::is_unsafe;
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
    fn a_61_line_input_is_capped_and_a_60_line_one_is_whole() {
        let over = dialog_of(json!((0..59).collect::<Vec<u32>>()));
        assert!(over.capped());
        assert_eq!(over.text().lines().count(), 61);
        let whole = dialog_of(json!((0..58).collect::<Vec<u32>>()));
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
    fn a_no_and_a_question_answer_have_no_dialog() {
        assert!(dialog(&approval(json!({})), "no").is_none());
        let question = Ask::Question {
            ask_id: 2,
            prompt: "pick".into(),
            options: vec![aoide_desktop_door::AskOption { label: "yes".into(), description: None }],
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
