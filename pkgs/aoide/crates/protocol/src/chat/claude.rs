//! Claude Code's chat adapter: its transcript records as chat lines. A record's
//! `uuid` is the line's `id`; a record with several content blocks gives
//! `<uuid>.<n>` per block, in order.
//!
//! A tool result names its call by `tool_use_id` in `tool.name`, as Eidolon's
//! does: the transcript's result block does not repeat the call's name.
//! Summaries, side-chain (`isSidechain`) and meta (`isMeta`) records project to
//! no line. The record's `timestamp` is not read: the protocol has no timestamp
//! parser yet, so these lines carry `ts: None`.
//!
//! A turn is several records: each assistant content block arrives as its own
//! record, sharing a `requestId`, and each projects under its own `uuid`. A
//! thinking block is empty in print mode (text blank, signature only), so an
//! empty one emits no line.
//!
//! Fixtures: `tests/fixtures/docs-claude-transcript.jsonl` is written from
//! Claude Code's public transcript docs; `claude-real-read-tool.jsonl` is a real
//! session (a throwaway project, one prompt, one Read), sanitised from that
//! run. Unknown keys on any record are ignored.

use super::{ChatLine, Role};
use serde_json::Value;

pub fn project(session: &str, line: &str) -> Vec<ChatLine> {
    let Ok(v) = serde_json::from_str::<Value>(line.trim()) else {
        return Vec::new();
    };
    let flag = |key| v.get(key).and_then(Value::as_bool).unwrap_or(false);
    if flag("isSidechain") || flag("isMeta") {
        return Vec::new();
    }
    let id = field(&v, "uuid");
    let content = v.pointer("/message/content");
    match v.get("type").and_then(Value::as_str) {
        Some("user") => content_lines(session, id, content, Role::User),
        Some("assistant") => content_lines(session, id, content, Role::Agent),
        _ => Vec::new(),
    }
}

/// A message's content: a plain string, or a list of blocks each projected on
/// its own line. `speaker` is who wrote the message.
fn content_lines(session: &str, id: &str, content: Option<&Value>, speaker: Role) -> Vec<ChatLine> {
    match content {
        Some(Value::String(s)) => vec![ChatLine::new(session, id, None, speaker).text(s)],
        Some(Value::Array(blocks)) => {
            let n = blocks.len();
            blocks
                .iter()
                .enumerate()
                .filter_map(|(i, b)| {
                    let line = |role| ChatLine::new(session, &id_at(id, i, n), None, role);
                    match b.get("type").and_then(Value::as_str)? {
                        "text" => Some(line(speaker).text(field(b, "text"))),
                        // Print mode writes thinking empty, with only a signature: an
                        // empty block is no message, so it emits no line.
                        "thinking" if matches!(speaker, Role::Agent) => {
                            let thought = field(b, "thinking");
                            (!thought.is_empty()).then(|| line(Role::Agent).thinking().text(thought))
                        }
                        "tool_use" => {
                            let input = b.get("input").map(Value::to_string);
                            Some(line(Role::Tool).tool(field(b, "name"), input.as_deref(), None, false))
                        }
                        "tool_result" => {
                            let error = b.get("is_error").and_then(Value::as_bool).unwrap_or(false);
                            let result = result_text(b.get("content"));
                            Some(line(Role::Tool).tool(field(b, "tool_use_id"), None, Some(&result), error))
                        }
                        _ => None,
                    }
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

/// A tool result's text: a plain string, or the `text` blocks of a list joined
/// by newlines.
fn result_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

fn id_at(id: &str, i: usize, n: usize) -> String {
    if n == 1 {
        id.to_string()
    } else {
        format!("{id}.{i}")
    }
}

fn field<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}
