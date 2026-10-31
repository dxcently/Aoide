//! Eidolon's chat adapter: its journal records as chat lines. The record's own
//! `id` is the line's `id`; a record with several blocks gives `<id>.<n>` per
//! block, in order.
//!
//! A `ToolResult` line names its call by `tool_use_id` in `tool.name`: the
//! journal's result record does not repeat the tool's name. `PeerMessage` reads
//! the same `from`/`text` fields as `ExternalMessage`, which the journal's own
//! export has not been checked against. Kinds the chat does not show project to
//! no line (`session trace` keeps them all).

use super::{Ask, AskKind, ChatLine, Role, State};
use crate::agents::eidolon_trace_record;
use serde_json::Value;

pub fn project(session: &str, line: &str) -> Vec<ChatLine> {
    let Some(rec) = eidolon_trace_record(line) else {
        return Vec::new();
    };
    let p = rec.payload.as_ref();
    let ts = rec.ts_ms;
    let one = |role: Role| ChatLine::new(session, &rec.id, ts, role);
    match rec.kind.as_str() {
        "UserMessage" => blocks(p, |i, n, b| {
            let line = ChatLine::new(session, &id_at(&rec.id, i, n), ts, Role::User);
            text_block(b, "text").map(|t| line.text(t))
        }),
        "AssistantMessage" => blocks(p, |i, n, b| {
            let line = |role| ChatLine::new(session, &id_at(&rec.id, i, n), ts, role);
            match b.get("type").and_then(Value::as_str)? {
                "thinking" => Some(line(Role::Agent).thinking().text(field(Some(b), "thinking"))),
                "text" => Some(line(Role::Agent).text(field(Some(b), "text"))),
                "tool_use" => {
                    let input = b.get("input").map(Value::to_string);
                    Some(line(Role::Tool).tool(field(Some(b), "name"), input.as_deref(), None, false))
                }
                _ => None,
            }
        }),
        "ToolResult" => {
            let content = match p.and_then(|v| v.get("content")) {
                Some(Value::String(s)) => s.clone(),
                Some(other) => other.to_string(),
                None => String::new(),
            };
            let error = p.and_then(|v| v.get("is_error")).and_then(Value::as_bool).unwrap_or(false);
            vec![one(Role::Tool).tool(field(p, "tool_use_id"), None, Some(&content), error)]
        }
        "OperatorMessage" => vec![one(Role::User).from(field(p, "from")).text(field(p, "text"))],
        "ExternalMessage" | "PeerMessage" => {
            vec![one(Role::System).from(field(p, "from")).text(field(p, "text"))]
        }
        "AskUser" => vec![one(Role::Agent).ask(Ask::new(
            field(p, "call_id"),
            AskKind::Question,
            field(p, "prompt"),
        ))],
        "TurnSettled" => vec![one(Role::System).text(field(p, "stop_reason")).state(State::Settled)],
        "Cancelled" => vec![one(Role::System).text("cancelled").state(State::Settled)],
        _ => Vec::new(),
    }
}

/// The record's `content` blocks, each projected by `each` with its index and
/// the block count, so the id can say which block a line came from.
fn blocks(
    payload: Option<&Value>,
    each: impl Fn(usize, usize, &Value) -> Option<ChatLine>,
) -> Vec<ChatLine> {
    let Some(list) = payload.and_then(|v| v.get("content")).and_then(Value::as_array) else {
        return Vec::new();
    };
    let n = list.len();
    list.iter().enumerate().filter_map(|(i, b)| each(i, n, b)).collect()
}

fn id_at(id: &str, i: usize, n: usize) -> String {
    if n == 1 {
        id.to_string()
    } else {
        format!("{id}.{i}")
    }
}

fn field<'a>(v: Option<&'a Value>, key: &str) -> &'a str {
    v.and_then(|v| v.get(key)).and_then(Value::as_str).unwrap_or("")
}

/// A `text` block's string, or none for a block whose text is absent or empty.
fn text_block<'a>(block: &'a Value, key: &str) -> Option<&'a str> {
    let t = field(Some(block), key);
    (!t.is_empty()).then_some(t)
}
