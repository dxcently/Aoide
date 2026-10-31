//! The frames `GET /api/events` carries: the door's own (`hello`, `caught-up`,
//! `turn-state`, `ask`, `ask-settled`, `lagged`, `goodbye`) and the projections
//! a transcript draws. The fields are the ones `eidolon/crates/web`'s
//! `stream.rs`, `wire.rs` and `user.rs` write. A frame type this client does not
//! know is [`Frame::Other`], kept as data.

use serde::Deserialize;
use serde_json::Value;

use crate::error::DoorError;
use crate::PROTOCOL;

/// One decoded frame. Categorical fields (`origin`, `outcome`, `how`,
/// `stop_reason`) stay strings, so a value this client does not know is data.
#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    Hello { session: String, cwd: String, model: String, yolo: bool, pending: u64, protocol: u32 },
    CaughtUp,
    TurnState { running: bool },
    Ask(Ask),
    AskSettled { ask_id: u64, how: String, answer: Option<String>, note: Option<String> },
    Lagged { dropped: u64 },
    Goodbye { text: String },
    TextDelta { text: String },
    AssistantMessage { record: u64, text: String, thinking: String, redacted: u64, tool_uses: Vec<ToolUse> },
    UserMessage { record: u64, text: String, images: u64 },
    ToolCallStarted { id: String, name: String, input: Value, origin: String },
    ToolCallFinished { record: Option<u64>, id: String, name: String, output: String, is_error: bool, images: u64 },
    PolicyVerdict { tool: String, reason: String, outcome: String, note: Option<String> },
    TurnSettled { stop_reason: String, usage: Usage, timing: Option<Timing> },
    Cancelled { usage: Usage, calls: u64 },
    Queued { waiting: u64 },
    Error { text: String },
    /// A frame type this client does not know, with its whole JSON.
    Other { kind: String, json: Value },
}

/// An ask the door raised: an approval of a tool call, or a question.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Ask {
    Approval {
        ask_id: u64,
        call_id: String,
        tool: String,
        input: Value,
        prompt: String,
        reason: Option<String>,
        structural: bool,
        judged: Option<String>,
        yolo: bool,
        answers: [String; 2],
    },
    Question {
        ask_id: u64,
        prompt: String,
        options: Vec<AskOption>,
    },
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AskOption {
    pub label: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ToolUse {
    pub id: String,
    pub name: String,
    pub input: Value,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Timing {
    pub calls: u32,
    pub first_token_ms: Option<u64>,
    pub decode_ms: u64,
    pub total_ms: u64,
}

/// Every `type` this client decodes. Anything else is [`Frame::Other`].
const KNOWN: &[&str] = &[
    "hello",
    "caught-up",
    "turn-state",
    "ask",
    "ask-settled",
    "lagged",
    "goodbye",
    "text-delta",
    "assistant-message",
    "user-message",
    "tool-call-started",
    "tool-call-finished",
    "policy-verdict",
    "turn-settled",
    "cancelled",
    "queued",
    "error",
];

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum Known {
    Hello { session: String, cwd: String, model: String, yolo: bool, pending: u64, protocol: Value },
    CaughtUp {},
    TurnState { running: bool },
    Ask(Ask),
    AskSettled { ask_id: u64, how: String, answer: Option<String>, note: Option<String> },
    Lagged { dropped: u64 },
    Goodbye { text: String },
    TextDelta { text: String },
    AssistantMessage { record: u64, text: String, thinking: String, redacted: u64, tool_uses: Vec<ToolUse> },
    UserMessage { record: u64, text: String, images: u64 },
    ToolCallStarted { id: String, name: String, input: Value, origin: String },
    ToolCallFinished { record: Option<u64>, id: String, name: String, output: String, is_error: bool, images: u64 },
    PolicyVerdict { tool: String, reason: String, outcome: String, note: Option<String> },
    TurnSettled { stop_reason: String, usage: Usage, timing: Option<Timing> },
    Cancelled { usage: Usage, calls: u64 },
    Queued { waiting: u64 },
    Error { text: String },
}

/// Decodes one frame's JSON. A `hello` whose protocol is not the number
/// [`PROTOCOL`] is [`DoorError::Protocol`], holding the value as sent (read as
/// JSON, so a non-number is one too). Unknown fields on a known frame are ignored.
pub(crate) fn decode(bytes: &[u8]) -> Result<Frame, DoorError> {
    let json: Value = serde_json::from_slice(bytes).map_err(|e| DoorError::Malformed(e.to_string()))?;
    let kind = json.get("type").and_then(Value::as_str).unwrap_or_default().to_string();
    if !KNOWN.contains(&kind.as_str()) {
        return Ok(Frame::Other { kind, json });
    }
    let known: Known = serde_json::from_value(json).map_err(|e| DoorError::Malformed(format!("{kind}: {e}")))?;
    Ok(match known {
        Known::Hello { session, cwd, model, yolo, pending, protocol } => {
            if protocol.as_u64() != Some(u64::from(PROTOCOL)) {
                return Err(DoorError::Protocol { got: protocol });
            }
            Frame::Hello { session, cwd, model, yolo, pending, protocol: PROTOCOL }
        }
        Known::CaughtUp {} => Frame::CaughtUp,
        Known::TurnState { running } => Frame::TurnState { running },
        Known::Ask(ask) => Frame::Ask(ask),
        Known::AskSettled { ask_id, how, answer, note } => Frame::AskSettled { ask_id, how, answer, note },
        Known::Lagged { dropped } => Frame::Lagged { dropped },
        Known::Goodbye { text } => Frame::Goodbye { text },
        Known::TextDelta { text } => Frame::TextDelta { text },
        Known::AssistantMessage { record, text, thinking, redacted, tool_uses } => {
            Frame::AssistantMessage { record, text, thinking, redacted, tool_uses }
        }
        Known::UserMessage { record, text, images } => Frame::UserMessage { record, text, images },
        Known::ToolCallStarted { id, name, input, origin } => Frame::ToolCallStarted { id, name, input, origin },
        Known::ToolCallFinished { record, id, name, output, is_error, images } => {
            Frame::ToolCallFinished { record, id, name, output, is_error, images }
        }
        Known::PolicyVerdict { tool, reason, outcome, note } => Frame::PolicyVerdict { tool, reason, outcome, note },
        Known::TurnSettled { stop_reason, usage, timing } => Frame::TurnSettled { stop_reason, usage, timing },
        Known::Cancelled { usage, calls } => Frame::Cancelled { usage, calls },
        Known::Queued { waiting } => Frame::Queued { waiting },
        Known::Error { text } => Frame::Error { text },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn decoded(json: &str) -> Frame {
        decode(json.as_bytes()).unwrap_or_else(|e| panic!("{json}: {e}"))
    }

    #[test]
    fn hello_carries_its_fields() {
        let frame = decoded(
            r#"{"type":"hello","session":"s1","cwd":"/w","model":"m","yolo":true,"pending":2,"protocol":1}"#,
        );
        assert_eq!(
            frame,
            Frame::Hello {
                session: "s1".into(),
                cwd: "/w".into(),
                model: "m".into(),
                yolo: true,
                pending: 2,
                protocol: 1,
            }
        );
    }

    #[test]
    fn caught_up_and_turn_state_decode() {
        assert_eq!(decoded(r#"{"type":"caught-up"}"#), Frame::CaughtUp);
        assert_eq!(decoded(r#"{"type":"turn-state","running":true}"#), Frame::TurnState { running: true });
    }

    #[test]
    fn an_approval_ask_carries_every_field_user_rs_gives_it() {
        let frame = decoded(
            r#"{"type":"ask","kind":"approval","ask_id":3,"call_id":"tu1","tool":"write","input":{"path":"/tmp/x"},
                "prompt":"write? Run it?","reason":"outside-cwd","structural":true,"judged":null,"yolo":false,
                "answers":["yes","no"]}"#,
        );
        assert_eq!(
            frame,
            Frame::Ask(Ask::Approval {
                ask_id: 3,
                call_id: "tu1".into(),
                tool: "write".into(),
                input: json!({"path": "/tmp/x"}),
                prompt: "write? Run it?".into(),
                reason: Some("outside-cwd".into()),
                structural: true,
                judged: None,
                yolo: false,
                answers: ["yes".into(), "no".into()],
            })
        );
    }

    #[test]
    fn a_question_ask_carries_its_options_with_or_without_a_description() {
        let frame = decoded(
            r#"{"type":"ask","kind":"question","ask_id":4,"prompt":"pick one",
                "options":[{"label":"a"},{"label":"b","description":"the second"}]}"#,
        );
        assert_eq!(
            frame,
            Frame::Ask(Ask::Question {
                ask_id: 4,
                prompt: "pick one".into(),
                options: vec![
                    AskOption { label: "a".into(), description: None },
                    AskOption { label: "b".into(), description: Some("the second".into()) },
                ],
            })
        );
    }

    #[test]
    fn ask_settled_with_and_without_a_note() {
        assert_eq!(
            decoded(r#"{"type":"ask-settled","ask_id":3,"how":"answered","answer":"no","note":"not on main"}"#),
            Frame::AskSettled {
                ask_id: 3,
                how: "answered".into(),
                answer: Some("no".into()),
                note: Some("not on main".into()),
            }
        );
        assert_eq!(
            decoded(r#"{"type":"ask-settled","ask_id":3,"how":"cancelled"}"#),
            Frame::AskSettled { ask_id: 3, how: "cancelled".into(), answer: None, note: None }
        );
    }

    #[test]
    fn lagged_and_goodbye_decode() {
        assert_eq!(decoded(r#"{"type":"lagged","dropped":5}"#), Frame::Lagged { dropped: 5 });
        assert_eq!(decoded(r#"{"type":"goodbye","text":"closing"}"#), Frame::Goodbye { text: "closing".into() });
    }

    #[test]
    fn the_transcript_projections_decode() {
        assert_eq!(decoded(r#"{"type":"text-delta","text":"hi"}"#), Frame::TextDelta { text: "hi".into() });
        assert_eq!(
            decoded(r#"{"type":"assistant-message","record":5,"text":"hello","thinking":"","redacted":0,
                "tool_uses":[{"id":"t1","name":"read","input":{"path":"x"}}]}"#),
            Frame::AssistantMessage {
                record: 5,
                text: "hello".into(),
                thinking: String::new(),
                redacted: 0,
                tool_uses: vec![ToolUse { id: "t1".into(), name: "read".into(), input: json!({"path": "x"}) }],
            }
        );
        assert_eq!(
            decoded(r#"{"type":"user-message","record":2,"text":"look","images":1}"#),
            Frame::UserMessage { record: 2, text: "look".into(), images: 1 }
        );
        assert_eq!(
            decoded(r#"{"type":"tool-call-started","id":"t1","name":"bash","input":{"cmd":"ls"},"origin":"script"}"#),
            Frame::ToolCallStarted {
                id: "t1".into(),
                name: "bash".into(),
                input: json!({"cmd": "ls"}),
                origin: "script".into(),
            }
        );
        assert_eq!(
            decoded(r#"{"type":"tool-call-finished","record":null,"id":"t1","name":"bash","output":"nope","is_error":true,"images":0}"#),
            Frame::ToolCallFinished {
                record: None,
                id: "t1".into(),
                name: "bash".into(),
                output: "nope".into(),
                is_error: true,
                images: 0,
            }
        );
        assert_eq!(
            decoded(r#"{"type":"policy-verdict","tool":"bash","reason":"subshell","outcome":"judged","note":"safe"}"#),
            Frame::PolicyVerdict {
                tool: "bash".into(),
                reason: "subshell".into(),
                outcome: "judged".into(),
                note: Some("safe".into()),
            }
        );
        assert_eq!(
            decoded(r#"{"type":"policy-verdict","tool":"bash","reason":"outside-cwd","outcome":"yolo"}"#),
            Frame::PolicyVerdict {
                tool: "bash".into(),
                reason: "outside-cwd".into(),
                outcome: "yolo".into(),
                note: None,
            }
        );
    }

    #[test]
    fn turn_settled_and_cancelled_carry_their_usage_and_timing() {
        let usage = Usage { input_tokens: 10, output_tokens: 20, cache_creation_input_tokens: 1, cache_read_input_tokens: 2 };
        assert_eq!(
            decoded(
                r#"{"type":"turn-settled","stop_reason":"end-turn",
                    "usage":{"input_tokens":10,"output_tokens":20,"cache_creation_input_tokens":1,"cache_read_input_tokens":2},
                    "timing":{"calls":1,"first_token_ms":120,"decode_ms":300,"total_ms":420}}"#
            ),
            Frame::TurnSettled {
                stop_reason: "end-turn".into(),
                usage,
                timing: Some(Timing { calls: 1, first_token_ms: Some(120), decode_ms: 300, total_ms: 420 }),
            }
        );
        assert_eq!(
            decoded(
                r#"{"type":"turn-settled","stop_reason":"cancelled",
                    "usage":{"input_tokens":0,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}"#
            ),
            Frame::TurnSettled { stop_reason: "cancelled".into(), usage: Usage::default(), timing: None }
        );
        assert_eq!(
            decoded(r#"{"type":"cancelled","calls":3,"usage":{"input_tokens":5,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}"#),
            Frame::Cancelled { usage: Usage { input_tokens: 5, ..Usage::default() }, calls: 3 }
        );
    }

    #[test]
    fn queued_and_error_decode() {
        assert_eq!(decoded(r#"{"type":"queued","waiting":2}"#), Frame::Queued { waiting: 2 });
        assert_eq!(decoded(r#"{"type":"error","text":"append failed"}"#), Frame::Error { text: "append failed".into() });
    }

    #[test]
    fn an_unknown_frame_type_is_kept_as_data() {
        let frame = decoded(r#"{"type":"future-frame","n":1}"#);
        assert_eq!(
            frame,
            Frame::Other { kind: "future-frame".into(), json: json!({"type": "future-frame", "n": 1}) }
        );
    }

    #[test]
    fn an_unknown_field_on_a_known_frame_is_not_an_error() {
        assert_eq!(decoded(r#"{"type":"caught-up","later":true}"#), Frame::CaughtUp);
        assert_eq!(
            decoded(r#"{"type":"turn-state","running":false,"later":"x"}"#),
            Frame::TurnState { running: false }
        );
    }

    #[test]
    fn a_protocol_2_hello_is_refused() {
        let err = decode(br#"{"type":"hello","session":"s","cwd":"/","model":"m","yolo":false,"pending":0,"protocol":2}"#)
            .unwrap_err();
        assert!(matches!(&err, DoorError::Protocol { got } if *got == json!(2)), "{err}");
    }

    #[test]
    fn a_protocol_that_is_not_the_number_1_is_refused_as_sent() {
        for sent in [r#""1""#, "4294967297", "1.0"] {
            let text = format!(
                r#"{{"type":"hello","session":"s","cwd":"/","model":"m","yolo":false,"pending":0,"protocol":{sent}}}"#
            );
            let err = decode(text.as_bytes()).unwrap_err();
            let want: serde_json::Value = serde_json::from_str(sent).unwrap();
            assert!(matches!(&err, DoorError::Protocol { got } if *got == want), "{sent}: {err}");
        }
    }

    #[test]
    fn malformed_json_and_a_known_frame_missing_a_field_are_malformed() {
        assert!(matches!(decode(br#"{"type": "#), Err(DoorError::Malformed(_))));
        assert!(matches!(decode(br#"{"type":"lagged"}"#), Err(DoorError::Malformed(_))));
    }
}
