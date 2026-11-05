//! The span view of a session's trace: `session trace --json`'s `data.turns`
//! and `data.spans` (`docs/architecture/EIDOLON-TRACE.md`, "Consumer changes").
//!
//! A pure projection from `&[TraceRecord]` to turns and a span tree. It takes
//! records, never lines and never a harness name: the agent and the
//! conversation id arrive as data from the caller, and every record is read by
//! its kind and its fields. A second producer whose records are translated
//! into the same kinds gets the same spans.
//!
//! **Turns.** Closers are `TurnSettled` and `Cancelled`. A turn is the run of
//! records after the previous closer (or the window start), up to and
//! including the next closer. Openers are `UserMessage` and `ExternalMessage`,
//! and a turn starts at its first opener. A run whose first `AssistantMessage`
//! comes before any opener is `partial`: the window cut its opener off, so it
//! starts at its first record. A run with neither an `AssistantMessage` nor an
//! opener is not a turn. The last run with no closer is the open turn.
//!
//! **The tree of one turn** (OTel GenAI names, epoch-millisecond times): the
//! root `invoke_agent <agent>` holds one `step <n>` per model call, and a step
//! holds its `chat <model>` and one `execute_tool <name>` per `tool_use` block.
//! A model call is the run of `AssistantMessage` records that share a
//! `message_id` (one response, written block by block); a record with no
//! `message_id` is a call of its own. A tool ends at the `ToolResult` carrying
//! its id; a call that never got one ends at the `Cancelled` that closed its
//! turn, or is `running` in the open turn.
//!
//! **Limits, stated.** Records follow line order, so a fork's abandoned branch
//! is not separated. A model call still in flight has no record yet, so it has
//! no chat span. Turn ids are record ids: a window whose records carry
//! unreadable ids may repeat one, and `--turn` picks the first.
//!
//! **No content.** A span carries names, ids, times, status and counts. It
//! never carries message text, thinking, tool arguments or results, an external
//! message's text or a question's prompt.

use super::trace::one_line_clip;
use aoide_protocol::agents::TraceRecord;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

/// Turns `data.turns` lists: the newest, with the rest counted in `turnsOmitted`.
pub(in crate::graph) const MAX_TURNS: usize = 100;
/// Spans one turn's tree may carry, its root included. Whole steps are kept,
/// newest first, and `spansOmitted` counts the spans of the steps left out.
pub(in crate::graph) const MAX_SPANS: usize = 1000;
/// Turn ids a `no-turn` refusal lists.
pub(in crate::graph) const NEWEST_IDS: usize = 20;
/// Characters a producer's string may carry into a span, after the strip.
const MAX_TEXT: usize = 200;

/// How a turn ends: a `TurnSettled`, a `Cancelled`, or the window's end.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Close {
    Settled,
    Cancelled,
    Open,
}

/// One turn, as the records it spans: `first..=last`, from its opener (or the
/// first record of a partial run) through its closer, or to the window's end
/// while it is open.
pub(in crate::graph) struct Turn {
    id: String,
    first: usize,
    last: usize,
    close: Close,
    partial: bool,
    steps: usize,
    tools: usize,
    failed: usize,
}

impl Turn {
    fn status(&self) -> &'static str {
        match self.close {
            Close::Settled => "ok",
            Close::Cancelled => "cancelled",
            Close::Open => "running",
        }
    }

    fn end(&self, records: &[TraceRecord]) -> Option<i64> {
        match self.close {
            Close::Open => None,
            Close::Settled | Close::Cancelled => records[self.last].ts_ms,
        }
    }

    /// The `data.turns` entry: the turn's own counts, never its content.
    fn json(&self, records: &[TraceRecord]) -> Value {
        json!({
            "id": self.id,
            "start": records[self.first].ts_ms,
            "end": self.end(records),
            "status": self.status(),
            "partial": self.partial,
            "steps": self.steps,
            "tools": self.tools,
            "failed": self.failed,
        })
    }
}

/// The window's turns, the model in force at each record, and the spans of any
/// one turn. Every method reads records by kind and their fields; no harness is
/// named here.
pub(in crate::graph) struct Turns<'a> {
    records: &'a [TraceRecord],
    in_force: Vec<Option<&'a str>>,
    turns: Vec<Turn>,
}

impl<'a> Turns<'a> {
    pub(in crate::graph) fn new(records: &'a [TraceRecord]) -> Self {
        Self {
            records,
            in_force: models_in_force(records),
            turns: split_turns(records),
        }
    }

    pub(in crate::graph) fn count(&self) -> usize {
        self.turns.len()
    }

    /// The newest [`MAX_TURNS`] turns, oldest first, and how many were left out.
    pub(in crate::graph) fn listing(&self) -> (Vec<Value>, usize) {
        let omitted = self.count().saturating_sub(MAX_TURNS);
        let listed: Vec<Value> =
            self.turns[omitted..].iter().map(|t| t.json(self.records)).collect();
        (listed, omitted)
    }

    /// The index of the newest turn, or `None` when the window holds none.
    pub(in crate::graph) fn newest(&self) -> Option<usize> {
        self.turns.len().checked_sub(1)
    }

    /// The index of the first turn with this id.
    pub(in crate::graph) fn position(&self, id: &str) -> Option<usize> {
        self.turns.iter().position(|t| t.id == id)
    }

    /// The ids of the newest [`NEWEST_IDS`] turns, oldest first.
    pub(in crate::graph) fn newest_ids(&self) -> Vec<String> {
        self.turns[self.turns.len().saturating_sub(NEWEST_IDS)..]
            .iter()
            .map(|t| t.id.clone())
            .collect()
    }

    /// Turn `index`'s span tree and how many of its spans the bound left out.
    /// `agent` and `conversation` are the session's, as the caller resolved them.
    pub(in crate::graph) fn tree(
        &self,
        index: usize,
        agent: &str,
        conversation: &str,
    ) -> (Value, usize) {
        let Some(turn) = self.turns.get(index) else {
            return (Value::Null, 0);
        };
        let (root, omitted) = self.root(turn, &clip(agent), &clip(conversation));
        (root.json(), omitted)
    }

    /// The root `invoke_agent` span over the turn's steps. The newest whole steps
    /// are kept within [`MAX_SPANS`]; the second value counts the spans left out.
    fn root(&self, turn: &Turn, agent: &str, conversation: &str) -> (Span, usize) {
        let records = self.records;
        let calls = model_calls(records, turn.first, turn.last);
        let steps: Vec<Span> = calls
            .iter()
            .enumerate()
            .map(|(n, call)| self.step(turn, call, n + 1, agent, conversation))
            .collect();

        let total = 1 + steps.iter().map(Span::count).sum::<usize>();
        let mut kept = 0;
        let mut first_kept = steps.len();
        for (i, step) in steps.iter().enumerate().rev() {
            let size = step.count();
            if kept + size > MAX_SPANS - 1 {
                break;
            }
            kept += size;
            first_kept = i;
        }
        let shown = 1 + kept;
        let children: Vec<Span> = steps.into_iter().skip(first_kept).collect();

        let model_at = calls.first().map_or(turn.last, |call| call[0]);
        let (provider, model) = model_parts(self.in_force[model_at]);
        let mut attributes = Map::new();
        put(&mut attributes, "gen_ai.operation.name", json!("invoke_agent"));
        put(&mut attributes, "gen_ai.agent.name", json!(agent));
        put(&mut attributes, "gen_ai.conversation.id", json!(conversation));
        if let Some(provider) = provider {
            put(&mut attributes, "gen_ai.provider.name", json!(provider));
        }
        if let Some(model) = model {
            put(&mut attributes, "gen_ai.request.model", json!(model));
        }
        if turn.close == Close::Settled {
            usage_into(&mut attributes, &records[turn.last]);
        }
        put(&mut attributes, "aoide.turn", json!(turn.id));

        let root = Span {
            id: format!("turn-{}", turn.id),
            name: format!("invoke_agent {agent}"),
            kind: "internal",
            start: records[turn.first].ts_ms,
            end: turn.end(records),
            status: turn.status(),
            attributes: Value::Object(attributes),
            children,
        };
        (root, total - shown)
    }

    /// One model call: its `chat`, then one `execute_tool` per `tool_use` block of
    /// any of its records. `call` lists the records of one response, in order.
    fn step(&self, turn: &Turn, call: &[usize], n: usize, agent: &str, conversation: &str) -> Span {
        let records = self.records;
        let (first, last) = (call[0], call[call.len() - 1]);
        let chat_start = match self.input_before(turn, first) {
            Some(j) => records[j].ts_ms,
            None => records[first].ts_ms,
        };
        let (provider, model) = model_parts(self.in_force[first]);
        let mut attributes = Map::new();
        put(&mut attributes, "gen_ai.operation.name", json!("chat"));
        if let Some(provider) = &provider {
            put(&mut attributes, "gen_ai.provider.name", json!(provider));
        }
        if let Some(model) = &model {
            put(&mut attributes, "gen_ai.request.model", json!(model));
        }
        put(&mut attributes, "gen_ai.conversation.id", json!(conversation));
        let blocks = block_types(call.iter().map(|&r| &records[r])).join(",");
        put(&mut attributes, "aoide.blocks", json!(blocks));
        let chat = Span {
            id: format!("chat-{}", record_id(&records[first])),
            name: match &model {
                Some(model) => format!("chat {model}"),
                None => "chat".to_string(),
            },
            kind: "client",
            start: chat_start,
            end: records[last].ts_ms,
            status: "ok",
            attributes: Value::Object(attributes),
            children: Vec::new(),
        };

        let mut children = vec![chat];
        for &r in call {
            for (index, block) in content(&records[r]).iter().enumerate() {
                if block_type(block) == "tool_use" {
                    children.push(self.tool(turn, r, index, block, agent, conversation));
                }
            }
        }
        let status = if children.iter().any(|c| c.status == "running") {
            "running"
        } else if children.iter().any(|c| c.status == "cancelled") {
            "cancelled"
        } else {
            "ok"
        };
        let end = match status {
            "running" => None,
            _ => children.iter().filter_map(|c| c.end).max(),
        };
        Span {
            id: format!("step-{}", record_id(&records[first])),
            name: format!("step {n}"),
            kind: "internal",
            start: chat_start,
            end,
            status,
            attributes: json!({ "aoide.step": n }),
            children,
        }
    }

    /// One `tool_use` block of record `r`, paired with the `ToolResult` that carries
    /// its call id after that record. A block with no call id gets a positional id
    /// and no pair.
    fn tool(
        &self,
        turn: &Turn,
        r: usize,
        index: usize,
        block: &Value,
        agent: &str,
        conversation: &str,
    ) -> Span {
        let records = self.records;
        let call = block.get("id").and_then(Value::as_str).filter(|s| !s.is_empty());
        let name = block.get("name").and_then(Value::as_str).map(clip).filter(|s| !s.is_empty());
        let result = call.and_then(|call| {
            (r + 1..=turn.last).find(|&j| {
                records[j].kind == "ToolResult"
                    && payload_str(&records[j], "tool_use_id") == Some(call)
            })
        });
        let (end, status, error) = match (result, turn.close) {
            (Some(j), _) if is_error(&records[j]) => {
                (records[j].ts_ms, "error", Some("tool_error"))
            }
            (Some(j), _) => (records[j].ts_ms, "ok", None),
            (None, Close::Cancelled) => (records[turn.last].ts_ms, "cancelled", Some("cancelled")),
            (None, Close::Settled) => (records[turn.last].ts_ms, "error", Some("no_result")),
            (None, Close::Open) => (None, "running", None),
        };

        let mut attributes = Map::new();
        put(&mut attributes, "gen_ai.operation.name", json!("execute_tool"));
        if let Some(name) = &name {
            put(&mut attributes, "gen_ai.tool.name", json!(name));
        }
        if let Some(call) = call {
            put(&mut attributes, "gen_ai.tool.call.id", json!(clip(call)));
        }
        put(&mut attributes, "gen_ai.tool.type", json!("function"));
        put(&mut attributes, "gen_ai.agent.name", json!(agent));
        put(&mut attributes, "gen_ai.conversation.id", json!(conversation));
        if let Some(error) = error {
            put(&mut attributes, "error.type", json!(error));
        }
        Span {
            id: match call {
                Some(call) => clip(call),
                None => format!("tool-{}-{index}", record_id(&records[r])),
            },
            name: match &name {
                Some(name) => format!("execute_tool {name}"),
                None => "execute_tool".to_string(),
            },
            kind: "internal",
            start: records[r].ts_ms,
            end,
            status,
            attributes: Value::Object(attributes),
            children: Vec::new(),
        }
    }

    /// The last input record before `a` in the turn: the record whose time starts
    /// the chat at `a`.
    fn input_before(&self, turn: &Turn, a: usize) -> Option<usize> {
        (turn.first..a).rev().find(|&j| is_input(&self.records[j]))
    }
}

/// The model in force at each record: the latest `SessionStart` or `ModelChanged`
/// model at or before it, `None` where the window has set none.
fn models_in_force(records: &[TraceRecord]) -> Vec<Option<&str>> {
    let mut current = None;
    records
        .iter()
        .map(|record| {
            if matches!(record.kind.as_str(), "SessionStart" | "ModelChanged") {
                current = payload_str(record, "model").filter(|model| !model.is_empty());
            }
            current
        })
        .collect()
}

/// Splits the window into turns. A run of records ends at each closer, and the
/// run after the last closer is the open turn when it holds one.
fn split_turns(records: &[TraceRecord]) -> Vec<Turn> {
    let mut turns = Vec::new();
    let mut from = 0;
    for (i, record) in records.iter().enumerate() {
        let close = match record.kind.as_str() {
            "TurnSettled" => Close::Settled,
            "Cancelled" => Close::Cancelled,
            _ => continue,
        };
        turns.extend(close_run(records, from, i, close));
        from = i + 1;
    }
    if from < records.len() {
        turns.extend(close_run(records, from, records.len() - 1, Close::Open));
    }
    turns
}

/// A run as a turn: opened by its first opener, or partial (starting at its first
/// record) when an assistant message comes before any opener. A run with neither
/// is no turn.
fn close_run(records: &[TraceRecord], from: usize, last: usize, close: Close) -> Option<Turn> {
    let run = &records[from..=last];
    let opener = run.iter().position(is_opener).map(|p| from + p);
    let assistant = run.iter().position(|r| r.kind == "AssistantMessage").map(|p| from + p);
    let (first, partial) = match (opener, assistant) {
        (None, None) => return None,
        (Some(o), None) => (o, false),
        (None, Some(_)) => (from, true),
        (Some(o), Some(a)) if o < a => (o, false),
        (Some(_), Some(_)) => (from, true),
    };
    let range = &records[first..=last];
    Some(Turn {
        id: record_id(&records[first]),
        first,
        last,
        close,
        partial,
        steps: model_calls(records, first, last).len(),
        tools: range
            .iter()
            .filter(|r| r.kind == "AssistantMessage")
            .map(tool_calls)
            .sum::<usize>(),
        failed: range.iter().filter(|r| r.kind == "ToolResult" && is_error(r)).count(),
    })
}

/// The model calls of records `first..=last`, each as the record indexes it holds in
/// order. Records that share a `message_id` are one call, listed at its first
/// record; a record with no `message_id` is a call of its own.
fn model_calls(records: &[TraceRecord], first: usize, last: usize) -> Vec<Vec<usize>> {
    let mut calls: Vec<Vec<usize>> = Vec::new();
    let mut by_id: HashMap<&str, usize> = HashMap::new();
    for (i, record) in records.iter().enumerate().take(last + 1).skip(first) {
        if record.kind != "AssistantMessage" {
            continue;
        }
        match payload_str(record, "message_id").filter(|id| !id.is_empty()) {
            Some(id) => match by_id.get(id) {
                Some(&k) => calls[k].push(i),
                None => {
                    by_id.insert(id, calls.len());
                    calls.push(vec![i]);
                }
            },
            None => calls.push(vec![i]),
        }
    }
    calls
}

fn is_opener(record: &TraceRecord) -> bool {
    matches!(record.kind.as_str(), "UserMessage" | "ExternalMessage")
}

/// A record that feeds the next model call: a message in, a tool's result, or an
/// answered question.
fn is_input(record: &TraceRecord) -> bool {
    match record.kind.as_str() {
        "UserMessage" | "ExternalMessage" | "ToolResult" => true,
        "AskUser" => payload(record, "answer").is_some_and(|answer| !answer.is_null()),
        _ => false,
    }
}

fn is_error(record: &TraceRecord) -> bool {
    payload(record, "is_error").and_then(Value::as_bool) == Some(true)
}

fn tool_calls(message: &TraceRecord) -> usize {
    content(message).iter().filter(|block| block_type(block) == "tool_use").count()
}

/// A message's content blocks, in the order it emitted them.
fn content(record: &TraceRecord) -> &[Value] {
    match payload(record, "content").and_then(Value::as_array) {
        Some(blocks) => blocks.as_slice(),
        None => &[],
    }
}

fn block_type(block: &Value) -> &str {
    block.get("type").and_then(Value::as_str).unwrap_or("")
}

/// The distinct block types of the given messages, in the order they were emitted.
fn block_types<'a>(messages: impl Iterator<Item = &'a TraceRecord>) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for block in messages.flat_map(content) {
        let kind = clip(block_type(block));
        if !kind.is_empty() && !seen.contains(&kind) {
            seen.push(kind);
        }
    }
    seen
}

fn payload<'r>(record: &'r TraceRecord, key: &str) -> Option<&'r Value> {
    record.payload.as_ref()?.get(key)
}

fn payload_str<'r>(record: &'r TraceRecord, key: &str) -> Option<&'r str> {
    payload(record, key)?.as_str()
}

/// The settled turn's usage, summed as Anthropic reports it: cached tokens are
/// input, and a cache count is named only when it is non-zero.
fn usage_into(attributes: &mut Map<String, Value>, settled: &TraceRecord) {
    let Some(usage) = payload(settled, "usage").filter(|usage| usage.is_object()) else {
        return;
    };
    let count = |key: &str| usage.get(key).and_then(Value::as_u64).unwrap_or(0);
    let read = count("cache_read_input_tokens");
    let write = count("cache_creation_input_tokens");
    let input = count("input_tokens").saturating_add(read).saturating_add(write);
    put(attributes, "gen_ai.usage.input_tokens", json!(input));
    put(attributes, "gen_ai.usage.output_tokens", json!(count("output_tokens")));
    if read > 0 {
        put(attributes, "gen_ai.usage.cache_read.input_tokens", json!(read));
    }
    if write > 0 {
        put(attributes, "gen_ai.usage.cache_write.input_tokens", json!(write));
    }
}

/// `provider:model` as its two attributes, split at the first colon. An empty
/// side is absent.
fn model_parts(raw: Option<&str>) -> (Option<String>, Option<String>) {
    let Some(raw) = raw else {
        return (None, None);
    };
    match raw.split_once(':') {
        Some((provider, model)) => (clipped_present(provider), clipped_present(model)),
        None => (None, clipped_present(raw)),
    }
}

/// A producer's text as a span carries it: stripped and cut, or absent when empty.
fn clipped_present(text: &str) -> Option<String> {
    Some(clip(text)).filter(|text| !text.is_empty())
}

/// A producer's text as a span carries it: stripped, flattened and cut to
/// [`MAX_TEXT`] characters.
fn clip(text: &str) -> String {
    one_line_clip(text, MAX_TEXT)
}

/// A record's id as a span names it.
fn record_id(record: &TraceRecord) -> String {
    clip(&record.id)
}

fn put(attributes: &mut Map<String, Value>, key: &str, value: Value) {
    attributes.insert(key.to_string(), value);
}

/// One span of a turn's tree, built before it is rendered so the bound can keep
/// whole steps.
struct Span {
    id: String,
    name: String,
    kind: &'static str,
    start: Option<i64>,
    end: Option<i64>,
    status: &'static str,
    attributes: Value,
    children: Vec<Span>,
}

impl Span {
    fn count(&self) -> usize {
        1 + self.children.iter().map(Span::count).sum::<usize>()
    }

    fn json(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "kind": self.kind,
            "start": self.start,
            "end": self.end,
            "status": self.status,
            "attributes": self.attributes,
            "children": self.children.iter().map(Span::json).collect::<Vec<Value>>(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_protocol::agents::eidolon_trace_record;

    const SESSION_START: &str = r#"{"id":0,"parent":null,"ts_ms":1000,"kind":{"SessionStart":{"model":"ollama:deepseek-v4.1-flash","cwd":"/w","system":null}}}"#;
    const U1: &str = r#"{"id":1,"parent":0,"ts_ms":1010,"kind":{"UserMessage":{"role":"user","content":[{"type":"text","text":"first"}]}}}"#;
    const A2: &str = r#"{"id":2,"parent":1,"ts_ms":1100,"kind":{"AssistantMessage":{"role":"assistant","content":[{"type":"thinking","thinking":"t1","signature":"s"},{"type":"text","text":"say1"},{"type":"tool_use","id":"call_a","name":"read","input":"{\"path\":\"p\"}"}]}}}"#;
    const R3: &str = r#"{"id":3,"parent":2,"ts_ms":1150,"kind":{"ToolResult":{"tool_use_id":"call_a","content":"file body","is_error":false}}}"#;
    const A4: &str = r#"{"id":4,"parent":3,"ts_ms":1200,"kind":{"AssistantMessage":{"role":"assistant","content":[{"type":"text","text":"say2"}]}}}"#;
    const T5: &str = r#"{"id":5,"parent":4,"ts_ms":1210,"kind":{"TurnSettled":{"stop_reason":"end_turn","usage":{"input_tokens":100,"output_tokens":20,"cache_creation_input_tokens":0,"cache_read_input_tokens":50}}}}"#;
    const U6: &str = r#"{"id":6,"parent":5,"ts_ms":2000,"kind":{"UserMessage":{"role":"user","content":[{"type":"text","text":"second"}]}}}"#;
    const A7: &str = r#"{"id":7,"parent":6,"ts_ms":2100,"kind":{"AssistantMessage":{"role":"assistant","content":[{"type":"tool_use","id":"call_b","name":"bash","input":"{\"command\":\"ls\"}"}]}}}"#;
    const R8: &str = r#"{"id":8,"parent":7,"ts_ms":2150,"kind":{"ToolResult":{"tool_use_id":"call_b","content":"ok","is_error":false}}}"#;
    const A9: &str = r#"{"id":9,"parent":8,"ts_ms":2200,"kind":{"AssistantMessage":{"role":"assistant","content":[{"type":"text","text":"done"}]}}}"#;
    const T10: &str = r#"{"id":10,"parent":9,"ts_ms":2210,"kind":{"TurnSettled":{"stop_reason":"end_turn","usage":{"input_tokens":5,"output_tokens":6}}}}"#;

    const P0: &str = r#"{"id":0,"parent":null,"ts_ms":100,"kind":{"UserMessage":{"role":"user","content":[{"type":"text","text":"go"}]}}}"#;
    const P1: &str = r#"{"id":1,"parent":0,"ts_ms":200,"kind":{"AssistantMessage":{"role":"assistant","content":[{"type":"tool_use","id":"c1","name":"read","input":"{}"},{"type":"tool_use","id":"c2","name":"grep","input":"{}"}]}}}"#;
    const P2: &str = r#"{"id":2,"parent":1,"ts_ms":250,"kind":{"ToolResult":{"tool_use_id":"c2","content":"two","is_error":false}}}"#;
    const P3: &str = r#"{"id":3,"parent":2,"ts_ms":260,"kind":{"ToolResult":{"tool_use_id":"c1","content":"one","is_error":false}}}"#;
    const P4: &str = r#"{"id":4,"parent":3,"ts_ms":300,"kind":{"AssistantMessage":{"role":"assistant","content":[{"type":"text","text":"x"}]}}}"#;
    const P5: &str = r#"{"id":5,"parent":4,"ts_ms":310,"kind":{"TurnSettled":{"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}}}}"#;

    const F0: &str = r#"{"id":0,"parent":null,"ts_ms":100,"kind":{"UserMessage":{"role":"user","content":[{"type":"text","text":"try"}]}}}"#;
    const F1: &str = r#"{"id":1,"parent":0,"ts_ms":200,"kind":{"AssistantMessage":{"role":"assistant","content":[{"type":"tool_use","id":"c9","name":"bash","input":"{}"}]}}}"#;
    const F2: &str = r#"{"id":2,"parent":1,"ts_ms":230,"kind":{"ToolResult":{"tool_use_id":"c9","content":"boom","is_error":true}}}"#;
    const F3: &str = r#"{"id":3,"parent":2,"ts_ms":240,"kind":{"AssistantMessage":{"role":"assistant","content":[{"type":"text","text":"retry"}]}}}"#;
    const F4: &str = r#"{"id":4,"parent":3,"ts_ms":250,"kind":{"TurnSettled":{"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}}}}"#;

    const D0: &str = r#"{"id":0,"parent":null,"ts_ms":10,"kind":{"SessionStart":{"model":"anthropic:claude-x","cwd":"/w","system":null}}}"#;
    const D1: &str = r#"{"id":1,"parent":0,"ts_ms":100,"kind":{"UserMessage":{"role":"user","content":[{"type":"text","text":"run"}]}}}"#;
    const D2: &str = r#"{"id":2,"parent":1,"ts_ms":200,"kind":{"AssistantMessage":{"role":"assistant","content":[{"type":"tool_use","id":"cR","name":"bash","input":"{}"}]}}}"#;

    const E0: &str = r#"{"id":0,"parent":null,"ts_ms":100,"kind":{"UserMessage":{"role":"user","content":[{"type":"text","text":"go"}]}}}"#;
    const E1: &str = r#"{"id":1,"parent":0,"ts_ms":200,"kind":{"AssistantMessage":{"role":"assistant","content":[{"type":"tool_use","id":"cX","name":"bash","input":"{}"}]}}}"#;
    const E2: &str = r#"{"id":2,"parent":1,"ts_ms":300,"kind":"Cancelled"}"#;

    const G0: &str = r#"{"id":0,"parent":null,"ts_ms":50,"kind":{"AssistantMessage":{"role":"assistant","content":[{"type":"text","text":"mid"}]}}}"#;
    const G1: &str = r#"{"id":1,"parent":0,"ts_ms":60,"kind":{"TurnSettled":{"stop_reason":"end_turn","usage":{"input_tokens":3,"output_tokens":4}}}}"#;
    const G2: &str = r#"{"id":2,"parent":1,"ts_ms":100,"kind":{"UserMessage":{"role":"user","content":[{"type":"text","text":"hi"}]}}}"#;
    const G3: &str = r#"{"id":3,"parent":2,"ts_ms":150,"kind":{"AssistantMessage":{"role":"assistant","content":[{"type":"text","text":"hello"}]}}}"#;
    const G4: &str = r#"{"id":4,"parent":3,"ts_ms":160,"kind":{"TurnSettled":{"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}}}}"#;

    const M0: &str = r#"{"id":0,"parent":null,"ts_ms":10,"kind":{"SessionStart":{"model":"ollama:m-one","cwd":"/w","system":null}}}"#;
    const M1: &str = r#"{"id":1,"parent":0,"ts_ms":100,"kind":{"UserMessage":{"role":"user","content":[{"type":"text","text":"a"}]}}}"#;
    const M2: &str = r#"{"id":2,"parent":1,"ts_ms":110,"kind":{"AssistantMessage":{"role":"assistant","content":[{"type":"text","text":"a"}]}}}"#;
    const M3: &str = r#"{"id":3,"parent":2,"ts_ms":120,"kind":{"TurnSettled":{"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}}}}"#;
    const M4: &str = r#"{"id":4,"parent":3,"ts_ms":200,"kind":{"ModelChanged":{"model":"bare-model"}}}"#;
    const M5: &str = r#"{"id":5,"parent":4,"ts_ms":210,"kind":{"UserMessage":{"role":"user","content":[{"type":"text","text":"b"}]}}}"#;
    const M6: &str = r#"{"id":6,"parent":5,"ts_ms":220,"kind":{"AssistantMessage":{"role":"assistant","content":[{"type":"text","text":"b"}]}}}"#;
    const M7: &str = r#"{"id":7,"parent":6,"ts_ms":230,"kind":{"TurnSettled":{"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}}}}"#;
    const M8: &str = r#"{"id":8,"parent":7,"ts_ms":300,"kind":{"ModelChanged":{"model":null}}}"#;
    const M9: &str = r#"{"id":9,"parent":8,"ts_ms":310,"kind":{"UserMessage":{"role":"user","content":[{"type":"text","text":"c"}]}}}"#;
    const M10: &str = r#"{"id":10,"parent":9,"ts_ms":320,"kind":{"AssistantMessage":{"role":"assistant","content":[{"type":"text","text":"c"}]}}}"#;
    const M11: &str = r#"{"id":11,"parent":10,"ts_ms":330,"kind":{"TurnSettled":{"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}}}}"#;

    const H0: &str = r#"{"id":0,"parent":null,"ts_ms":100,"kind":{"UserMessage":{"role":"user","content":[{"type":"text","text":"u"}]}}}"#;
    const H1: &str = r#"{"id":1,"parent":0,"ts_ms":110,"kind":{"AssistantMessage":{"role":"assistant","content":[{"type":"text","text":"a"}]}}}"#;
    const H2: &str = r#"{"id":2,"parent":1,"ts_ms":120,"kind":{"TurnSettled":{"stop_reason":"end_turn","usage":{"input_tokens":10,"output_tokens":4,"cache_creation_input_tokens":0,"cache_read_input_tokens":30}}}}"#;
    const H3: &str = r#"{"id":3,"parent":2,"ts_ms":200,"kind":{"UserMessage":{"role":"user","content":[{"type":"text","text":"u"}]}}}"#;
    const H4: &str = r#"{"id":4,"parent":3,"ts_ms":210,"kind":{"AssistantMessage":{"role":"assistant","content":[{"type":"text","text":"a"}]}}}"#;
    const H5: &str = r#"{"id":5,"parent":4,"ts_ms":220,"kind":{"TurnSettled":{"stop_reason":"end_turn","usage":{"input_tokens":7,"output_tokens":1,"cache_creation_input_tokens":9,"cache_read_input_tokens":0}}}}"#;

    const C0: &str = r#"{"id":0,"parent":null,"ts_ms":1,"kind":{"SessionStart":{"model":"ollama:m","cwd":"/w","system":null}}}"#;
    const C1: &str = r#"{"id":1,"parent":0,"ts_ms":10,"kind":{"UserMessage":{"role":"user","content":[{"type":"text","text":"MARK-USER-TEXT"}]}}}"#;
    const C2: &str = r#"{"id":2,"parent":1,"ts_ms":20,"kind":{"AssistantMessage":{"role":"assistant","content":[{"type":"thinking","thinking":"MARK-THINK","signature":"s"},{"type":"text","text":"MARK-TEXT"},{"type":"tool_use","id":"cz","name":"read","input":"{\"path\":\"MARK-INPUT\"}"}]}}}"#;
    const C3: &str = r#"{"id":3,"parent":2,"ts_ms":30,"kind":{"ToolResult":{"tool_use_id":"cz","content":"MARK-RESULT","is_error":false}}}"#;
    const C4: &str = r#"{"id":4,"parent":3,"ts_ms":40,"kind":{"ExternalMessage":{"from":"MARK-FROM","channel":null,"text":"MARK-EXT"}}}"#;
    const C5: &str = r#"{"id":5,"parent":4,"ts_ms":50,"kind":{"AssistantMessage":{"role":"assistant","content":[{"type":"text","text":"after"}]}}}"#;
    const C6: &str = r#"{"id":6,"parent":5,"ts_ms":60,"kind":{"AskUser":{"call_id":"q","prompt":"MARK-ASK","answer":"MARK-ANSWER"}}}"#;
    const C7: &str = r#"{"id":7,"parent":6,"ts_ms":70,"kind":{"AssistantMessage":{"role":"assistant","content":[{"type":"text","text":"end"}]}}}"#;
    const C8: &str = r#"{"id":8,"parent":7,"ts_ms":80,"kind":{"TurnSettled":{"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}}}}"#;

    const X0: &str = r#"{"id":0,"parent":null,"ts_ms":5,"kind":{"ExternalMessage":{"from":"orchestrator","channel":null,"text":"go"}}}"#;
    const X1: &str = r#"{"id":1,"parent":0,"ts_ms":10,"kind":{"AssistantMessage":{"role":"assistant","content":[{"type":"text","text":"on it"}]}}}"#;
    const X2: &str = r#"{"id":2,"parent":1,"ts_ms":20,"kind":{"TurnSettled":{"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}}}}"#;

    const N0: &str = r#"{"id":0,"parent":null,"kind":{"UserMessage":{"role":"user","content":[{"type":"text","text":"x"}]}}}"#;
    const N1: &str = r#"{"id":1,"parent":0,"ts_ms":20,"kind":{"AssistantMessage":{"role":"assistant","content":[{"type":"text","text":"y"}]}}}"#;
    const N2: &str = r#"{"id":2,"parent":1,"ts_ms":30,"kind":{"TurnSettled":{"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}}}}"#;

    const Q0: &str = r#"{"id":0,"parent":null,"ts_ms":100,"kind":{"UserMessage":{"role":"user","content":[{"type":"text","text":"ask"}]}}}"#;
    const Q1: &str = r#"{"id":1,"parent":0,"ts_ms":200,"kind":{"AssistantMessage":{"role":"assistant","content":[{"type":"tool_use","id":"cq","name":"bash","input":"{}"}]}}}"#;
    const Q2: &str = r#"{"id":2,"parent":1,"ts_ms":300,"kind":{"TurnSettled":{"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}}}}"#;

    const I0: &str = r#"{"id":0,"parent":null,"ts_ms":100,"kind":{"UserMessage":{"role":"user","content":[{"type":"text","text":"id-less"}]}}}"#;
    const I1: &str = r#"{"id":1,"parent":0,"ts_ms":200,"kind":{"AssistantMessage":{"role":"assistant","content":[{"type":"tool_use","name":"bash","input":"{}"}]}}}"#;

    const CONTEXT: &str = r#"{"id":1,"parent":0,"ts_ms":20,"kind":{"ContextSize":{"tokens":5}}}"#;

    fn window(lines: &[&str]) -> Vec<TraceRecord> {
        lines.iter().map(|l| eidolon_trace_record(l).expect("a trace record")).collect()
    }

    fn kids(span: &Value) -> usize {
        span["children"].as_array().map_or(0, Vec::len)
    }

    fn spans_in(span: &Value) -> usize {
        1 + span["children"].as_array().map_or(0, |c| c.iter().map(spans_in).sum::<usize>())
    }

    #[test]
    fn two_settled_turns_are_indexed_oldest_first_and_the_newest_is_the_default() {
        let w = window(&[SESSION_START, U1, A2, R3, A4, T5, U6, A7, R8, A9, T10]);
        let turns = Turns::new(&w);
        assert_eq!(turns.count(), 2);
        let (listed, omitted) = turns.listing();
        assert_eq!(omitted, 0);
        assert_eq!(listed.len(), 2, "{listed:?}");
        assert_eq!(
            listed[0],
            json!({"id":"1","start":1010,"end":1210,"status":"ok","partial":false,"steps":2,"tools":1,"failed":0})
        );
        assert_eq!(
            listed[1],
            json!({"id":"6","start":2000,"end":2210,"status":"ok","partial":false,"steps":2,"tools":1,"failed":0})
        );
        assert_eq!(turns.newest(), Some(1));
        assert_eq!(turns.position("1"), Some(0));
        assert_eq!(turns.position("6"), Some(1));
        assert_eq!(turns.position("7"), None);
        assert_eq!(turns.newest_ids(), vec!["1".to_string(), "6".to_string()]);
    }

    #[test]
    fn the_root_span_names_the_agent_its_conversation_and_its_usage() {
        let w = window(&[SESSION_START, U1, A2, R3, A4, T5, U6, A7, R8, A9, T10]);
        let turns = Turns::new(&w);
        let (root, omitted) = turns.tree(1, "eidolon", "Aoide-x");
        assert_eq!(omitted, 0);
        assert_eq!(root["id"], "turn-6");
        assert_eq!(root["name"], "invoke_agent eidolon");
        assert_eq!(root["kind"], "internal");
        assert_eq!(root["status"], "ok");
        assert_eq!(root["start"], 2000);
        assert_eq!(root["end"], 2210);
        assert_eq!(
            root["attributes"],
            json!({
                "gen_ai.operation.name": "invoke_agent",
                "gen_ai.agent.name": "eidolon",
                "gen_ai.conversation.id": "Aoide-x",
                "gen_ai.provider.name": "ollama",
                "gen_ai.request.model": "deepseek-v4.1-flash",
                "gen_ai.usage.input_tokens": 5,
                "gen_ai.usage.output_tokens": 6,
                "aoide.turn": "6"
            })
        );
        assert_eq!(kids(&root), 2, "{root}");
        assert_eq!(root["children"][0]["name"], "step 1");
        assert_eq!(root["children"][0]["id"], "step-7");
        assert_eq!(root["children"][0]["start"], 2000, "a step starts at its chat's start");
        assert_eq!(root["children"][0]["end"], 2150, "a step ends at its latest child");
        assert_eq!(root["children"][0]["attributes"], json!({"aoide.step": 1}));
        assert_eq!(root["children"][1]["name"], "step 2");
        assert_eq!(root["children"][1]["id"], "step-9");
    }

    #[test]
    fn a_chat_starts_at_the_previous_input_and_a_tool_pairs_with_its_result_by_id() {
        let w = window(&[SESSION_START, U1, A2, R3, A4, T5, U6, A7, R8, A9, T10]);
        let turns = Turns::new(&w);
        let (root, _) = turns.tree(1, "eidolon", "Aoide-x");
        let chat = &root["children"][0]["children"][0];
        assert_eq!(chat["id"], "chat-7");
        assert_eq!(chat["name"], "chat deepseek-v4.1-flash");
        assert_eq!(chat["kind"], "client");
        assert_eq!(chat["start"], 2000, "the UserMessage before it");
        assert_eq!(chat["end"], 2100, "the AssistantMessage's own time");
        assert_eq!(
            chat["attributes"],
            json!({
                "gen_ai.operation.name": "chat",
                "gen_ai.provider.name": "ollama",
                "gen_ai.request.model": "deepseek-v4.1-flash",
                "gen_ai.conversation.id": "Aoide-x",
                "aoide.blocks": "tool_use"
            })
        );
        let tool = &root["children"][0]["children"][1];
        assert_eq!(tool["id"], "call_b");
        assert_eq!(tool["name"], "execute_tool bash");
        assert_eq!(tool["kind"], "internal");
        assert_eq!(tool["start"], 2100);
        assert_eq!(tool["end"], 2150, "ends at the ToolResult carrying its id");
        assert_eq!(tool["status"], "ok");
        assert_eq!(
            tool["attributes"],
            json!({
                "gen_ai.operation.name": "execute_tool",
                "gen_ai.tool.name": "bash",
                "gen_ai.tool.call.id": "call_b",
                "gen_ai.tool.type": "function",
                "gen_ai.agent.name": "eidolon",
                "gen_ai.conversation.id": "Aoide-x"
            })
        );
        let second_chat = &root["children"][1]["children"][0];
        assert_eq!(second_chat["start"], 2150, "a chat starts at the previous ToolResult");
        assert_eq!(second_chat["attributes"]["aoide.blocks"], "text");
    }

    #[test]
    fn the_first_turn_carries_its_thinking_text_and_tool_blocks_in_order() {
        let w = window(&[SESSION_START, U1, A2, R3, A4, T5, U6, A7, R8, A9, T10]);
        let turns = Turns::new(&w);
        let (root, _) = turns.tree(0, "eidolon", "Aoide-x");
        assert_eq!(root["id"], "turn-1");
        assert_eq!(root["attributes"]["gen_ai.provider.name"], "ollama");
        assert_eq!(root["attributes"]["gen_ai.usage.input_tokens"], 150, "100 + 50 cache read");
        assert_eq!(root["attributes"]["gen_ai.usage.output_tokens"], 20);
        assert_eq!(root["attributes"]["gen_ai.usage.cache_read.input_tokens"], 50);
        assert!(root["attributes"].get("gen_ai.usage.cache_write.input_tokens").is_none());
        let chat = &root["children"][0]["children"][0];
        assert_eq!(chat["attributes"]["aoide.blocks"], "thinking,text,tool_use");
        assert_eq!(chat["start"], 1010);
        let tool = &root["children"][0]["children"][1];
        assert_eq!(tool["name"], "execute_tool read");
        assert_eq!(tool["end"], 1150);
        assert_eq!(root["children"][0]["end"], 1150);
        assert_eq!(root["children"][1]["children"][0]["start"], 1150);
    }

    #[test]
    fn parallel_calls_in_one_message_each_end_at_their_own_result() {
        let w = window(&[P0, P1, P2, P3, P4, P5]);
        let turns = Turns::new(&w);
        assert_eq!(turns.count(), 1);
        let (root, _) = turns.tree(0, "eidolon", "Aoide-x");
        let step = &root["children"][0];
        assert_eq!(kids(step), 3, "one chat and one span per tool_use: {step}");
        assert_eq!(step["children"][1]["id"], "c1");
        assert_eq!(step["children"][1]["end"], 260, "c1 pairs with its own result, not the first one");
        assert_eq!(step["children"][2]["id"], "c2");
        assert_eq!(step["children"][2]["end"], 250);
        assert_eq!(step["end"], 260, "the step ends at its latest child");
        assert_eq!(root["children"][1]["children"][0]["start"], 260, "the last ToolResult before the chat");
    }

    #[test]
    fn a_failed_result_marks_its_tool_as_an_error_and_the_turn_counts_it() {
        let w = window(&[F0, F1, F2, F3, F4]);
        let turns = Turns::new(&w);
        let (listed, _) = turns.listing();
        assert_eq!(listed.len(), 1, "{listed:?}");
        assert_eq!(listed[0]["failed"], 1);
        assert_eq!(listed[0]["tools"], 1);
        let (root, _) = turns.tree(0, "eidolon", "Aoide-x");
        assert_eq!(root["status"], "ok", "a failed tool does not fail the turn");
        let tool = &root["children"][0]["children"][1];
        assert_eq!(tool["status"], "error");
        assert_eq!(tool["end"], 230);
        assert_eq!(tool["attributes"]["error.type"], "tool_error");
        assert_eq!(root["children"][0]["status"], "ok");
    }

    #[test]
    fn an_open_turn_is_running_and_its_unanswered_tool_is_running_too() {
        let w = window(&[D0, D1, D2]);
        let turns = Turns::new(&w);
        let (listed, _) = turns.listing();
        assert_eq!(listed.len(), 1, "{listed:?}");
        assert_eq!(listed[0]["status"], "running");
        assert!(listed[0]["end"].is_null());
        let (root, _) = turns.tree(0, "eidolon", "Aoide-x");
        assert_eq!(root["status"], "running");
        assert!(root["end"].is_null());
        assert_eq!(root["attributes"]["gen_ai.provider.name"], "anthropic");
        assert_eq!(root["attributes"]["gen_ai.request.model"], "claude-x");
        assert!(root["attributes"].get("gen_ai.usage.input_tokens").is_none(), "no settle, no usage");
        let step = &root["children"][0];
        assert_eq!(step["status"], "running");
        assert!(step["end"].is_null());
        let tool = &step["children"][1];
        assert_eq!(tool["status"], "running");
        assert!(tool["end"].is_null());
        assert!(tool["attributes"].get("error.type").is_none());
    }

    #[test]
    fn a_cancel_mid_tool_ends_the_tool_and_its_step_and_the_turn_at_the_cancel() {
        let w = window(&[E0, E1, E2]);
        let turns = Turns::new(&w);
        let (listed, _) = turns.listing();
        assert_eq!(listed.len(), 1, "{listed:?}");
        assert_eq!(listed[0]["status"], "cancelled");
        assert_eq!(listed[0]["end"], 300);
        let (root, _) = turns.tree(0, "eidolon", "Aoide-x");
        assert_eq!(root["status"], "cancelled");
        assert_eq!(root["end"], 300);
        let step = &root["children"][0];
        assert_eq!(step["status"], "cancelled");
        assert_eq!(step["end"], 300);
        let tool = &step["children"][1];
        assert_eq!(tool["status"], "cancelled");
        assert_eq!(tool["end"], 300);
        assert_eq!(tool["attributes"]["error.type"], "cancelled");
    }

    #[test]
    fn a_window_cut_mid_turn_starts_that_turn_at_its_first_record() {
        let w = window(&[G0, G1, G2, G3, G4]);
        let turns = Turns::new(&w);
        let (listed, _) = turns.listing();
        assert_eq!(listed.len(), 2, "{listed:?}");
        assert_eq!(listed[0]["id"], "0");
        assert_eq!(listed[0]["partial"], true);
        assert_eq!(listed[0]["start"], 50);
        assert_eq!(listed[0]["end"], 60);
        assert_eq!(listed[1]["id"], "2");
        assert_eq!(listed[1]["partial"], false);
        assert_eq!(listed[1]["start"], 100);
        let (root, _) = turns.tree(0, "eidolon", "Aoide-x");
        assert_eq!(root["id"], "turn-0");
        assert_eq!(root["start"], 50);
        assert_eq!(root["children"][0]["children"][0]["start"], 50, "no input before it: its own time");
    }

    #[test]
    fn the_model_in_force_splits_at_the_first_colon_and_is_left_out_when_absent() {
        let w = window(&[M0, M1, M2, M3, M4, M5, M6, M7, M8, M9, M10, M11]);
        let turns = Turns::new(&w);
        assert_eq!(turns.count(), 3);
        let (first, _) = turns.tree(0, "eidolon", "Aoide-x");
        assert_eq!(first["attributes"]["gen_ai.provider.name"], "ollama");
        assert_eq!(first["attributes"]["gen_ai.request.model"], "m-one");
        assert_eq!(first["children"][0]["children"][0]["name"], "chat m-one");
        let (second, _) = turns.tree(1, "eidolon", "Aoide-x");
        assert!(second["attributes"].get("gen_ai.provider.name").is_none(), "no colon, no provider");
        assert_eq!(second["attributes"]["gen_ai.request.model"], "bare-model");
        assert_eq!(second["children"][0]["children"][0]["name"], "chat bare-model");
        let (third, _) = turns.tree(2, "eidolon", "Aoide-x");
        assert!(third["attributes"].get("gen_ai.provider.name").is_none());
        assert!(third["attributes"].get("gen_ai.request.model").is_none(), "a null model is none in force");
        assert_eq!(third["children"][0]["children"][0]["name"], "chat");
        assert!(third["children"][0]["children"][0]["attributes"].get("gen_ai.request.model").is_none());
    }

    #[test]
    fn a_window_with_no_model_at_all_leaves_both_model_keys_out() {
        let w = window(&[H0, H1, H2]);
        let turns = Turns::new(&w);
        let (root, _) = turns.tree(0, "eidolon", "Aoide-x");
        assert!(root["attributes"].get("gen_ai.provider.name").is_none());
        assert!(root["attributes"].get("gen_ai.request.model").is_none());
        assert_eq!(root["children"][0]["children"][0]["name"], "chat");
    }

    #[test]
    fn usage_maps_to_anthropic_totals_and_leaves_zero_cache_out() {
        let w = window(&[H0, H1, H2, H3, H4, H5]);
        let turns = Turns::new(&w);
        assert_eq!(turns.count(), 2);
        let (first, _) = turns.tree(0, "eidolon", "Aoide-x");
        assert_eq!(first["attributes"]["gen_ai.usage.input_tokens"], 40, "10 + 30 cache read + 0 cache write");
        assert_eq!(first["attributes"]["gen_ai.usage.output_tokens"], 4);
        assert_eq!(first["attributes"]["gen_ai.usage.cache_read.input_tokens"], 30);
        assert!(first["attributes"].get("gen_ai.usage.cache_write.input_tokens").is_none(), "zero is left out");
        let (second, _) = turns.tree(1, "eidolon", "Aoide-x");
        assert_eq!(second["attributes"]["gen_ai.usage.input_tokens"], 16, "7 + 0 + 9");
        assert_eq!(second["attributes"]["gen_ai.usage.output_tokens"], 1);
        assert_eq!(second["attributes"]["gen_ai.usage.cache_write.input_tokens"], 9);
        assert!(second["attributes"].get("gen_ai.usage.cache_read.input_tokens").is_none());
    }

    #[test]
    fn an_external_message_opens_a_turn_and_an_answered_question_is_an_input() {
        let w = window(&[X0, X1, X2]);
        let turns = Turns::new(&w);
        let (listed, _) = turns.listing();
        assert_eq!(listed[0]["id"], "0");
        assert_eq!(listed[0]["start"], 5);
        let c = window(&[C0, C1, C2, C3, C4, C5, C6, C7, C8]);
        let turns = Turns::new(&c);
        let (root, _) = turns.tree(0, "eidolon", "Aoide-x");
        assert_eq!(root["children"][1]["children"][0]["start"], 40, "after the external message");
        assert_eq!(root["children"][2]["children"][0]["start"], 60, "after the answered question");
    }

    #[test]
    fn spans_and_turns_carry_no_content() {
        let c = window(&[C0, C1, C2, C3, C4, C5, C6, C7, C8]);
        let turns = Turns::new(&c);
        let (listed, _) = turns.listing();
        let (root, _) = turns.tree(0, "eidolon", "Aoide-x");
        let text = format!("{listed:?} {root}");
        for marker in [
            "MARK-USER-TEXT",
            "MARK-THINK",
            "MARK-TEXT",
            "MARK-INPUT",
            "MARK-RESULT",
            "MARK-FROM",
            "MARK-EXT",
            "MARK-ASK",
            "MARK-ANSWER",
        ] {
            assert!(!text.contains(marker), "{marker} leaked into a span: {text}");
        }
        assert!(text.contains("execute_tool read"), "the tool name is a name, and it is kept: {text}");
    }

    #[test]
    fn a_window_with_no_assistant_message_and_no_opener_holds_no_turn() {
        let w = window(&[SESSION_START, CONTEXT]);
        let turns = Turns::new(&w);
        assert_eq!(turns.count(), 0);
        assert_eq!(turns.newest(), None);
        assert_eq!(turns.listing(), (Vec::new(), 0));
        assert!(turns.newest_ids().is_empty());
    }

    #[test]
    fn a_settled_turn_whose_call_never_got_a_result_ends_that_call_as_no_result() {
        let w = window(&[Q0, Q1, Q2]);
        let turns = Turns::new(&w);
        let (root, _) = turns.tree(0, "eidolon", "Aoide-x");
        let tool = &root["children"][0]["children"][1];
        assert_eq!(tool["status"], "error");
        assert_eq!(tool["end"], 300, "the settle is the last thing the turn knows");
        assert_eq!(tool["attributes"]["error.type"], "no_result");
        assert_eq!(root["status"], "ok");
    }

    #[test]
    fn a_call_without_an_id_gets_a_positional_id_and_no_result() {
        let w = window(&[I0, I1]);
        let turns = Turns::new(&w);
        let (root, _) = turns.tree(0, "eidolon", "Aoide-x");
        let tool = &root["children"][0]["children"][1];
        assert_eq!(tool["id"], "tool-1-0");
        assert_eq!(tool["status"], "running");
        assert!(tool["attributes"].get("gen_ai.tool.call.id").is_none());
    }

    #[test]
    fn a_record_without_a_timestamp_gives_its_span_a_null_time() {
        let w = window(&[N0, N1, N2]);
        let turns = Turns::new(&w);
        let (root, _) = turns.tree(0, "eidolon", "Aoide-x");
        assert!(root["start"].is_null(), "no read time is ever substituted");
        assert!(root["children"][0]["children"][0]["start"].is_null());
        assert_eq!(root["end"], 30);
        assert_eq!(root["status"], "ok");
    }

    #[test]
    fn the_listing_keeps_the_newest_hundred_turns() {
        let mut lines: Vec<String> = Vec::new();
        for k in 0..150u64 {
            lines.push(json!({"id": 3 * k, "ts_ms": 1000 + 10 * k, "kind": {"UserMessage": {"role": "user", "content": []}}}).to_string());
            lines.push(json!({"id": 3 * k + 1, "ts_ms": 1001 + 10 * k, "kind": {"AssistantMessage": {"role": "assistant", "content": []}}}).to_string());
            lines.push(json!({"id": 3 * k + 2, "ts_ms": 1002 + 10 * k, "kind": {"TurnSettled": {"stop_reason": "end_turn"}}}).to_string());
        }
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let w = window(&refs);
        let turns = Turns::new(&w);
        assert_eq!(turns.count(), 150);
        let (listed, omitted) = turns.listing();
        assert_eq!(omitted, 50);
        assert_eq!(listed.len(), 100);
        assert_eq!(listed[0]["id"], "150");
        assert_eq!(listed[99]["id"], "447");
        assert_eq!(turns.newest_ids().len(), NEWEST_IDS);
    }

    #[test]
    fn the_tree_keeps_whole_newest_steps_within_the_span_bound() {
        let mut lines: Vec<String> = vec![json!({"id": 0, "ts_ms": 1, "kind": {"UserMessage": {"role": "user", "content": []}}}).to_string()];
        for k in 0..400u64 {
            lines.push(json!({"id": 1 + 2 * k, "ts_ms": 10 + 2 * k, "kind": {"AssistantMessage": {"role": "assistant", "content": [{"type": "tool_use", "id": format!("c{k}"), "name": "read", "input": "{}"}]}}}).to_string());
            lines.push(json!({"id": 2 + 2 * k, "ts_ms": 11 + 2 * k, "kind": {"ToolResult": {"tool_use_id": format!("c{k}"), "content": "", "is_error": false}}}).to_string());
        }
        lines.push(json!({"id": 801, "ts_ms": 2000, "kind": {"TurnSettled": {"stop_reason": "end_turn"}}}).to_string());
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let w = window(&refs);
        let turns = Turns::new(&w);
        let (root, omitted) = turns.tree(0, "eidolon", "Aoide-x");
        assert_eq!(spans_in(&root), MAX_SPANS, "the kept tree is exactly the bound");
        assert_eq!(omitted, 201, "1 root + 400 steps of 3 spans, less the 1000 kept");
        assert_eq!(kids(&root), 333, "whole steps only: 333 steps of 3 spans beside the root");
        assert_eq!(root["children"][0]["name"], "step 68", "the newest are kept");
        assert_eq!(root["children"][332]["name"], "step 400");
    }

    #[test]
    fn a_tree_inside_the_bounds_omits_nothing() {
        let w = window(&[SESSION_START, U1, A2, R3, A4, T5]);
        let turns = Turns::new(&w);
        let (_, omitted) = turns.tree(0, "eidolon", "Aoide-x");
        assert_eq!(omitted, 0);
    }

    // One model response, three AssistantMessage records sharing message_id
    // "m1". The first tool's result lands between the first and second record,
    // as a Claude transcript writes it.
    const GA0: &str = r#"{"id":0,"parent":null,"ts_ms":100,"kind":{"UserMessage":{"role":"user","content":[{"type":"text","text":"go"}]}}}"#;
    const GA1: &str = r#"{"id":1,"parent":0,"ts_ms":200,"kind":{"AssistantMessage":{"role":"assistant","message_id":"m1","content":[{"type":"tool_use","id":"g1","name":"read","input":"{}"}]}}}"#;
    const GA2: &str = r#"{"id":2,"parent":1,"ts_ms":230,"kind":{"ToolResult":{"tool_use_id":"g1","content":"one","is_error":false}}}"#;
    const GA3: &str = r#"{"id":3,"parent":2,"ts_ms":240,"kind":{"AssistantMessage":{"role":"assistant","message_id":"m1","content":[{"type":"tool_use","id":"g2","name":"grep","input":"{}"}]}}}"#;
    const GA4: &str = r#"{"id":4,"parent":3,"ts_ms":260,"kind":{"ToolResult":{"tool_use_id":"g2","content":"two","is_error":false}}}"#;
    const GA5: &str = r#"{"id":5,"parent":4,"ts_ms":270,"kind":{"AssistantMessage":{"role":"assistant","message_id":"m1","content":[{"type":"text","text":"done"}]}}}"#;
    const GA6: &str = r#"{"id":6,"parent":5,"ts_ms":280,"kind":{"TurnSettled":{"stop_reason":"end_turn","usage":{"input_tokens":3,"output_tokens":4}}}}"#;

    #[test]
    fn records_sharing_a_message_id_are_one_model_call_and_each_tool_starts_at_its_own_record() {
        let w = window(&[GA0, GA1, GA2, GA3, GA4, GA5, GA6]);
        let turns = Turns::new(&w);
        let (listed, _) = turns.listing();
        assert_eq!(listed[0]["steps"], 1, "three records, one model call: {listed:?}");
        assert_eq!(listed[0]["tools"], 2, "{listed:?}");
        let (root, _) = turns.tree(0, "eidolon", "Aoide-x");
        assert_eq!(kids(&root), 1, "one step for the call: {root}");
        let step = &root["children"][0];
        assert_eq!(step["id"], "step-1", "the step is named by the call's first record");
        assert_eq!(step["start"], 100, "the chat starts at the input before the call's first record");
        assert_eq!(step["end"], 270, "the step ends at the latest of its children");
        assert_eq!(kids(step), 3, "one chat and one span per tool: {step}");
        let chat = &step["children"][0];
        assert_eq!(chat["id"], "chat-1");
        assert_eq!(chat["end"], 270, "the chat ends at the call's LAST record");
        assert_eq!(
            chat["attributes"]["aoide.blocks"], "tool_use,text",
            "the call's block types in order, without repeats"
        );
        let first = &step["children"][1];
        assert_eq!(first["id"], "g1");
        assert_eq!(first["start"], 200, "the first tool starts at its record's time");
        assert_eq!(first["end"], 230, "and ends at its own result");
        let second = &step["children"][2];
        assert_eq!(second["id"], "g2");
        assert_eq!(second["start"], 240, "the second tool starts at its own record, after the first result");
        assert_eq!(second["end"], 260, "and ends at its own result");
    }
}
