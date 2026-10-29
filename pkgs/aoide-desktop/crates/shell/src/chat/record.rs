//! The one place a line of a chat stream is parsed. A JSON object is a record,
//! passed through as it is, with every string spelled. Anything else is text,
//! spelled as one line. Core will later print one normalized line per message;
//! until then the raw records pass through unchanged.

use serde_json::Value;

use crate::exec::untrusted::visible;

/// One message from a chat stream, ready for the page.
#[derive(Debug, Clone, PartialEq)]
pub enum Push {
    /// A JSON object the stream printed, with every string spelled.
    Record(Value),
    /// A line that is not a JSON object, spelled.
    Other(String),
    /// A line past the exec crate's cap, dropped; its length in bytes.
    Dropped(usize),
    /// The stream closed.
    End,
}

/// Parses one line the stream printed.
pub fn line(text: &str) -> Push {
    match serde_json::from_str::<Value>(text) {
        Ok(value @ Value::Object(_)) => Push::Record(spell(value)),
        _ => Push::Other(visible(text)),
    }
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
