//! The one place a line of a chat stream is parsed. A JSON object that is a v1
//! chat line (CONTRACTS.md, "The chat line") becomes a `Line` holding only the
//! fields the page renders. An object that is core's closing envelope ends the
//! stream. Any other object is a record, passed through with every string
//! spelled, and anything else is text, spelled as one line. Every string is
//! hostile data: core strips and clips it, and it is spelled again here.

use serde::Serialize;
use serde_json::Value;

use crate::exec::untrusted::visible;

/// One message from a chat stream, ready for the page.
#[derive(Debug, Clone, PartialEq)]
pub enum Push {
    /// A v1 chat line.
    Line(Box<Line>),
    /// A JSON object the stream printed that is neither a chat line nor the
    /// closing envelope, with every string spelled.
    Record(Value),
    /// A line that is not a JSON object, spelled.
    Other(String),
    /// A line past the exec crate's cap, dropped; its length in bytes.
    Dropped(usize),
    /// The stream closed: core's closing envelope arrived.
    End,
}

/// Who a line is from. Any name outside the closed set is `System`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Agent,
    Tool,
    System,
}

/// Where a chat is in its turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Waiting,
    Settled,
}

/// A tool call (`input`) or its result (`result`, `error`) under one `name`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Tool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    pub error: bool,
}

/// One option of a question ask.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AskOption {
    pub label: String,
    pub description: String,
}

/// A question the session put to the person. `kind` stays the text core wrote.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Ask {
    pub id: String,
    pub kind: String,
    pub prompt: String,
    pub answers: Vec<String>,
    pub options: Vec<AskOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
}

/// One chat line, v1: the fields the page renders. A field the contract adds
/// later is not read, and a field of the wrong type is as if absent.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Line {
    pub id: String,
    pub role: Role,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    pub thinking: bool,
    pub clipped: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<Tool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ask: Option<Ask>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<State>,
}

/// Parses one line the stream printed.
pub fn line(text: &str) -> Push {
    match serde_json::from_str::<Value>(text) {
        Ok(value @ Value::Object(_)) if is_envelope(&value) => Push::End,
        Ok(value @ Value::Object(_)) => match chat_line(&value) {
            Some(line) => Push::Line(Box::new(line)),
            None => Push::Record(spell(value)),
        },
        _ => Push::Other(visible(text)),
    }
}

/// Core's closing envelope: the outcome a command prints last under `--json`,
/// which names its `command` and its `status`. A chat line has neither.
fn is_envelope(value: &Value) -> bool {
    value["command"].is_string() && value["status"].is_string()
}

/// `value` as a v1 chat line, or `None` when it is not one: its `v` is not 1,
/// or it names no `id` or no `role`.
fn chat_line(value: &Value) -> Option<Line> {
    if value["v"] != 1 {
        return None;
    }
    let id = text_of(&value["id"])?;
    let role = match value["role"].as_str()? {
        "user" => Role::User,
        "agent" => Role::Agent,
        "tool" => Role::Tool,
        _ => Role::System,
    };
    let state = match value["state"].as_str() {
        Some("waiting") => Some(State::Waiting),
        Some("settled") => Some(State::Settled),
        _ => None,
    };
    Some(Line {
        id,
        role,
        from: text_of(&value["from"]),
        text: text_of(&value["text"]),
        thinking: value["thinking"] == true,
        clipped: value["clipped"] == true,
        tool: tool_of(&value["tool"]),
        ask: ask_of(&value["ask"]),
        state,
    })
}

fn text_of(value: &Value) -> Option<String> {
    value.as_str().map(visible)
}

fn tool_of(value: &Value) -> Option<Tool> {
    Some(Tool {
        name: text_of(&value["name"])?,
        input: text_of(&value["input"]),
        result: text_of(&value["result"]),
        error: value["error"] == true,
    })
}

fn ask_of(value: &Value) -> Option<Ask> {
    let strings = |key: &str| -> Vec<String> {
        value[key].as_array().into_iter().flatten().filter_map(text_of).collect()
    };
    let options = value["options"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|o| Some(AskOption { label: text_of(&o["label"])?, description: text_of(&o["description"]).unwrap_or_default() }))
        .collect();
    Some(Ask {
        id: text_of(&value["id"])?,
        kind: text_of(&value["kind"])?,
        prompt: text_of(&value["prompt"])?,
        answers: strings("answers"),
        options,
        fingerprint: text_of(&value["fingerprint"]),
    })
}

/// `value` with every string in it, keys included, spelled by `visible`.
pub fn spell(value: Value) -> Value {
    match value {
        Value::String(s) => Value::String(visible(&s)),
        Value::Array(items) => Value::Array(items.into_iter().map(spell).collect()),
        Value::Object(map) => Value::Object(map.into_iter().map(|(k, v)| (visible(&k), spell(v))).collect()),
        other => other,
    }
}
