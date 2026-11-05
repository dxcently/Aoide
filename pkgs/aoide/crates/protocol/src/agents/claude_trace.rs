//! Claude's transcript as trace lines: the `records` reader of the Claude
//! profile, and the `subagent_records` reader of one sub-agent's own file, both
//! read by `session trace` only. The transcript is Claude Code's own
//! log, one record per content block. This turns it into the record kinds
//! `eidolon_trace_record` reads (docs/architecture/EIDOLON-TRACE.md), so the
//! span view reads one shape. Claude keeps `trace: None`: these lines are never
//! a ping-back source.

use super::{transcript_tail_bounded, CHAT_FIRST_BYTES};
use crate::chat::claude::result_text;
use ground::time::parse_iso_utc_ms;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;

/// The window's trace lines for `session trace`, or `None` when `path` is not a
/// file. The window is the transcript's last [`CHAT_FIRST_BYTES`]. A session's
/// own transcript: its sidechain records are skipped.
pub(crate) fn claude_trace_tail(path: &Path) -> Option<Vec<String>> {
    path.is_file()
        .then(|| translate(&transcript_tail_bounded(path, CHAT_FIRST_BYTES), true))
}

/// One sub-agent's own file (`agent-<id>.jsonl`) as trace lines, over the same
/// window. Every record of such a file is a sidechain, so none is skipped.
pub(crate) fn claude_subagent_trace_tail(path: &Path) -> Option<Vec<String>> {
    path.is_file()
        .then(|| translate(&transcript_tail_bounded(path, CHAT_FIRST_BYTES), false))
}

/// The tool a sub-agent calls to hand its report back, which ends its turn.
const HAND_BACK_TOOL: &str = "SubagentHandback";

/// The window's transcript lines as trace lines, in file order. A line that does
/// not parse is no line. `skip_sidechain` is true for a session's own transcript
/// and false for a sub-agent's own file, as `TranscriptSpec::say` takes it.
fn translate(lines: &[String], skip_sidechain: bool) -> Vec<String> {
    let records: Vec<Value> = lines
        .iter()
        .filter_map(|line| serde_json::from_str(line.trim()).ok())
        .collect();
    // Each message's last record: the one its settle is emitted after.
    let last_of: HashMap<&str, usize> = records
        .iter()
        .enumerate()
        .filter(|(_, record)| record.get("type").and_then(Value::as_str) == Some("assistant"))
        .filter_map(|(i, record)| Some((text_at(record, "/message/id")?, i)))
        .collect();
    // Each message's stop reason as its last record writes it: a hand-back carries it.
    let stops: HashMap<&str, Value> = last_of
        .iter()
        .map(|(&id, &i)| (id, json!(text_at(&records[i], "/message/stop_reason"))))
        .collect();
    let mut st = State { skip_sidechain, ..State::default() };
    for (i, record) in records.iter().enumerate() {
        match record.get("type").and_then(Value::as_str) {
            Some("user") => user(record, &mut st),
            Some("assistant") => assistant(record, i, &last_of, &stops, &mut st),
            _ => {}
        }
    }
    st.out
}

/// The window's running state: whether a turn is open, the model last emitted,
/// the open turn's usage sums, and the hand-back calls read so far.
#[derive(Default)]
struct State {
    skip_sidechain: bool,
    open: bool,
    model: Option<String>,
    last_ts: Option<i64>,
    sums: Sums,
    /// Call id to the stop reason of the call's message, for each `SubagentHandback`
    /// call read so far. A result settles only through an entry here, so its call
    /// comes earlier in the window.
    handbacks: HashMap<String, Value>,
    out: Vec<String>,
}

impl State {
    fn emit(&mut self, id: String, ts: Option<i64>, kind: Value) {
        let mut line = json!({ "id": id, "kind": kind });
        if let Some(ts) = ts {
            line["ts_ms"] = json!(ts);
            self.last_ts = Some(ts);
        }
        self.out.push(line.to_string());
    }

    /// A settle or a cancel has been emitted: the turn is over and its sums are
    /// spent.
    fn close(&mut self) {
        self.open = false;
        self.sums = Sums::default();
    }
}

#[derive(Default)]
struct Sums {
    input: u64,
    output: u64,
    cache_write: u64,
    cache_read: u64,
}

impl Sums {
    fn add(&mut self, usage: Option<&Value>) {
        let n = |key: &str| usage.and_then(|u| u.get(key)).and_then(Value::as_u64).unwrap_or(0);
        self.input += n("input_tokens");
        self.output += n("output_tokens");
        self.cache_write += n("cache_creation_input_tokens");
        self.cache_read += n("cache_read_input_tokens");
    }

    fn json(&self) -> Value {
        json!({
            "input_tokens": self.input,
            "output_tokens": self.output,
            "cache_creation_input_tokens": self.cache_write,
            "cache_read_input_tokens": self.cache_read,
        })
    }
}

fn flag(record: &Value, key: &str) -> bool {
    record.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn text_at<'a>(record: &'a Value, path: &str) -> Option<&'a str> {
    record.pointer(path)?.as_str()
}

fn text_field<'a>(block: &'a Value, key: &str) -> &'a str {
    block.get(key).and_then(Value::as_str).unwrap_or("")
}

fn ts_of(record: &Value) -> Option<i64> {
    text_at(record, "/timestamp").and_then(parse_iso_utc_ms)
}

/// A user record: an opener (`turnPosition`), a tool result, or other user text.
/// Its content is a string or a list of blocks; any other shape is no line.
fn user(record: &Value, st: &mut State) {
    if (st.skip_sidechain && flag(record, "isSidechain"))
        || flag(record, "isCompactSummary")
        || flag(record, "isVisibleInTranscriptOnly")
    {
        return;
    }
    let Some(uuid) = text_at(record, "/uuid") else {
        return;
    };
    let ts = ts_of(record);
    let content = record.pointer("/message/content");
    if record.get("turnPosition").is_some() {
        let Some(text) = plain_text(content) else {
            return;
        };
        if st.open {
            st.emit(format!("cancelled-{uuid}"), st.last_ts.or(ts), json!("Cancelled"));
            st.close();
        }
        st.emit(uuid.to_string(), ts, message_kind(record, &text));
        st.open = true;
        return;
    }
    let blocks = content.and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
    let results: Vec<(usize, &Value)> = blocks
        .iter()
        .enumerate()
        .filter(|(_, block)| block.get("type").and_then(Value::as_str) == Some("tool_result"))
        .collect();
    if !results.is_empty() {
        let one = results.len() == 1;
        for (index, block) in results {
            let id = if one { uuid.to_string() } else { format!("{uuid}.{index}") };
            let call = text_field(block, "tool_use_id");
            let kind = json!({"ToolResult": {
                "tool_use_id": call,
                "content": result_text(block.get("content")),
                "is_error": flag(block, "is_error"),
            }});
            st.emit(id.clone(), ts, kind);
            // A hand-back's result closes the open turn, right after its own line.
            if let Some(stop) = st.handbacks.get(call).cloned().filter(|_| st.open) {
                let usage = st.sums.json();
                st.emit(format!("settled-{id}"), ts, json!({"TurnSettled": {"stop_reason": stop, "usage": usage}}));
                st.close();
            }
        }
        return;
    }
    let text = plain_text(content).unwrap_or_default();
    if !flag(record, "isMeta") && !text.is_empty() {
        st.emit(uuid.to_string(), ts, message_kind(record, &text));
    }
}

/// The kind of a user message: a human's is a `UserMessage`, anything else an
/// `ExternalMessage` named by who sent it.
fn message_kind(record: &Value, text: &str) -> Value {
    if text_at(record, "/turnOrigin") == Some("human") {
        return json!({"UserMessage": {"role": "user", "content": [{"type": "text", "text": text}]}});
    }
    let from = ["/origin/from", "/origin/kind", "/turnOrigin"]
        .iter()
        .find_map(|path| text_at(record, path))
        .unwrap_or("unknown");
    json!({"ExternalMessage": {"from": from, "channel": null, "text": text}})
}

/// A message's text: a string as it is, or the `text` blocks of a list joined by
/// newlines. Any other shape is `None`.
fn plain_text(content: Option<&Value>) -> Option<String> {
    match content? {
        Value::String(s) => Some(s.clone()),
        Value::Array(blocks) => Some(
            blocks
                .iter()
                .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|block| block.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        _ => None,
    }
}

/// An assistant record: an `AssistantMessage`, after a `ModelChanged` when the
/// model moved. A `<synthetic>` reply emits no line and adds no usage, but its
/// stop reason can still settle an open turn. A record whose content is not a
/// list is no line.
fn assistant(record: &Value, index: usize, last_of: &HashMap<&str, usize>, stops: &HashMap<&str, Value>, st: &mut State) {
    if st.skip_sidechain && flag(record, "isSidechain") {
        return;
    }
    let (Some(uuid), Some(mid), Some(content)) = (
        text_at(record, "/uuid"),
        text_at(record, "/message/id"),
        record.pointer("/message/content").and_then(Value::as_array),
    ) else {
        return;
    };
    let ts = ts_of(record);
    let model = text_at(record, "/message/model");
    let real = model != Some("<synthetic>");
    if let Some(model) = model.filter(|m| real && st.model.as_deref() != Some(*m)) {
        st.emit(format!("model-{uuid}"), ts, json!({"ModelChanged": {"model": format!(":{model}")}}));
        st.model = Some(model.to_string());
    }
    if real {
        let kind = json!({"AssistantMessage": {"role": "assistant", "message_id": mid, "content": blocks(content)}});
        st.emit(uuid.to_string(), ts, kind);
        st.open = true;
    }
    // A message's earlier records can carry partial usage: its last record holds the total.
    if real && last_of.get(mid) == Some(&index) {
        st.sums.add(record.pointer("/message/usage"));
    }
    let stop = text_at(record, "/message/stop_reason");
    for block in content
        .iter()
        .filter(|block| text_field(block, "type") == "tool_use" && text_field(block, "name") == HAND_BACK_TOOL)
    {
        let reason = stops.get(mid).cloned().unwrap_or(Value::Null);
        st.handbacks.insert(text_field(block, "id").to_string(), reason);
    }
    let settles = matches!(stop, Some(s) if s != "tool_use" && s != "pause_turn");
    if st.open && settles && last_of.get(mid) == Some(&index) {
        let kind = json!({"TurnSettled": {"stop_reason": stop, "usage": st.sums.json()}});
        st.emit(format!("settled-{mid}"), ts, kind);
        st.close();
    }
}

/// A message's content blocks as trace blocks: text, thinking (signature
/// dropped), tool_use (input as a JSON string), and nothing else.
fn blocks(content: &[Value]) -> Vec<Value> {
    content
        .iter()
        .filter_map(|block| match block.get("type")?.as_str()? {
            "text" => Some(json!({"type": "text", "text": text_field(block, "text")})),
            "thinking" => Some(json!({"type": "thinking", "thinking": text_field(block, "thinking")})),
            "redacted_thinking" => Some(json!({"type": "thinking", "thinking": ""})),
            "tool_use" => Some(json!({
                "type": "tool_use",
                "id": text_field(block, "id"),
                "name": text_field(block, "name"),
                "input": block.get("input").map_or_else(String::new, Value::to_string),
            })),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{claude_subagent_trace_tail, claude_trace_tail, translate};
    use crate::agents::eidolon_trace_record;
    use serde_json::{json, Value};

    const TURNS: &str = include_str!("../../tests/fixtures/claude-turns.jsonl");
    const COMPACT: &str = include_str!("../../tests/fixtures/claude-compact-window.jsonl");
    const READ_TOOL: &str = include_str!("../../tests/fixtures/claude-real-read-tool.jsonl");

    fn window(text: &str) -> Vec<String> {
        text.lines().map(str::to_string).collect()
    }

    /// The translated window, one JSON value per line, in order.
    fn out(lines: &[String]) -> Vec<Value> {
        translate(lines, true)
            .iter()
            .map(|line| serde_json::from_str(line).expect("a translated line is JSON"))
            .collect()
    }

    /// A line's kind: `Cancelled` for the unit kind, else the one key a kind names.
    fn kind_of(line: &Value) -> &str {
        match &line["kind"] {
            Value::String(name) => name.as_str(),
            kind => kind.as_object().and_then(|map| map.keys().next()).map_or("", String::as_str),
        }
    }

    fn by_id<'a>(lines: &'a [Value], id: &str) -> &'a Value {
        lines.iter().find(|line| line["id"] == id).expect("a line with that id")
    }

    /// The `tool_use` block of an `AssistantMessage` payload.
    fn tool_block(message: &Value) -> Value {
        message["content"]
            .as_array()
            .expect("content blocks")
            .iter()
            .find(|block| block["type"] == "tool_use")
            .cloned()
            .expect("a tool call")
    }

    #[test]
    fn every_line_is_a_trace_record_eidolon_reads_with_a_real_id() {
        for text in [TURNS, COMPACT, READ_TOOL] {
            let lines = translate(&window(text), true);
            assert!(!lines.is_empty(), "a fixture translates to lines");
            for line in lines {
                let record = eidolon_trace_record(&line).expect("eidolon reads the line");
                assert_ne!(record.id, "?", "{line}");
            }
        }
    }

    #[test]
    fn the_turn_fixture_reads_as_four_turns_each_settled_with_its_usage() {
        let lines = out(&window(TURNS));
        let openers: Vec<&str> = lines
            .iter()
            .filter(|line| matches!(kind_of(line), "UserMessage" | "ExternalMessage"))
            .filter_map(|line| line["id"].as_str())
            .collect();
        assert_eq!(openers, ["u-0001", "u-0012", "u-0019", "u-0037"]);
        let settles: Vec<(&str, [u64; 4])> = lines
            .iter()
            .filter(|line| kind_of(line) == "TurnSettled")
            .map(|line| {
                let settle = &line["kind"]["TurnSettled"];
                let usage = |key: &str| settle["usage"][key].as_u64().expect("a usage count");
                (
                    settle["stop_reason"].as_str().expect("a stop reason"),
                    [
                        usage("input_tokens"),
                        usage("cache_creation_input_tokens"),
                        usage("cache_read_input_tokens"),
                        usage("output_tokens"),
                    ],
                )
            })
            .collect();
        assert_eq!(
            settles,
            [
                ("end_turn", [4, 2078, 554493, 2487]),
                ("refusal", [2, 3298, 159070, 748]),
                ("end_turn", [10, 3261, 1619471, 1424]),
                ("end_turn", [10, 151421, 795629, 2056]),
            ]
        );
    }

    #[test]
    fn the_model_is_named_once_before_the_first_message() {
        let lines = out(&window(TURNS));
        let changes: Vec<&Value> = lines.iter().filter(|line| kind_of(line) == "ModelChanged").collect();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0]["kind"]["ModelChanged"]["model"].as_str(), Some(":claude-opus-5-5"));
        let changed = lines.iter().position(|line| kind_of(line) == "ModelChanged");
        let first_message = lines.iter().position(|line| kind_of(line) == "AssistantMessage");
        assert!(changed < first_message);
    }

    #[test]
    fn a_result_landing_between_two_blocks_of_one_message_stays_between_them() {
        let lines = out(&window(TURNS));
        let blocks: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line["kind"]["AssistantMessage"]["message_id"] == "msg_0011")
            .map(|(i, _)| i)
            .collect();
        assert_eq!(blocks.len(), 2, "msg_0011 is two Read calls");
        assert!(lines[blocks[0]..blocks[1]].iter().any(|line| kind_of(line) == "ToolResult"));
        let first = &lines[blocks[0]]["kind"]["AssistantMessage"];
        assert_eq!(first["role"], "assistant");
        assert_eq!(first["message_id"], "msg_0011");
        let call = tool_block(first);
        assert_eq!(call["id"], "toolu_0007");
        assert_eq!(call["name"], "Read");
        let input = call["input"].as_str().expect("input is a JSON string");
        assert!(serde_json::from_str::<Value>(input).expect("it parses").is_object());
        let result = &by_id(&lines, "u-0045")["kind"]["ToolResult"];
        assert_eq!(result["tool_use_id"], "toolu_0007");
        assert_eq!(result["is_error"], false, "an absent is_error reads false");
    }

    #[test]
    fn a_window_that_crosses_a_compaction_is_one_open_turn_with_no_summary_line() {
        let lines = out(&window(COMPACT));
        let kinds: Vec<&str> = lines.iter().map(kind_of).collect();
        assert!(!kinds.is_empty());
        assert!(kinds.iter().all(|k| !matches!(*k, "Cancelled" | "TurnSettled" | "UserMessage" | "ExternalMessage")));
    }

    #[test]
    fn the_real_read_tool_session_settles_once_with_its_usage() {
        let lines = out(&window(READ_TOOL));
        let settles: Vec<&Value> = lines.iter().filter(|line| kind_of(line) == "TurnSettled").collect();
        assert_eq!(settles.len(), 1);
        let usage = &settles[0]["kind"]["TurnSettled"]["usage"];
        assert_eq!(
            [
                usage["input_tokens"].as_u64(),
                usage["cache_creation_input_tokens"].as_u64(),
                usage["cache_read_input_tokens"].as_u64(),
                usage["output_tokens"].as_u64(),
            ],
            [Some(4), Some(13219), Some(60906), Some(138)],
        );
    }

    #[test]
    fn an_opener_after_an_open_turn_cancels_it_at_the_last_timed_line() {
        // u-0001's turn stops at its first tool call, which has no result, and
        // u-0012's opener follows it.
        let source = window(TURNS);
        let call = source.iter().position(|line| line.contains("\"type\":\"tool_use\"")).expect("a tool call");
        let opener = source.iter().position(|line| line.contains("\"uuid\":\"u-0012\"")).expect("the opener");
        let mut cut = source[..=call].to_vec();
        cut.push(source[opener].clone());
        let lines = out(&cut);
        let call_id = serde_json::from_str::<Value>(&source[call]).expect("a record")["uuid"]
            .as_str()
            .expect("a uuid")
            .to_string();
        let cancelled = by_id(&lines, "cancelled-u-0012");
        assert_eq!(kind_of(cancelled), "Cancelled");
        assert_eq!(cancelled["ts_ms"], by_id(&lines, &call_id)["ts_ms"]);
        let at = |id: &str| lines.iter().position(|line| line["id"] == id).expect("a line");
        assert!(at("cancelled-u-0012") < at("u-0012"));
    }

    #[test]
    fn a_plain_user_message_is_an_input_and_leaves_its_turn_open() {
        let lines = window(
            &[
                r#"{"type":"user","uuid":"o-1","timestamp":"2026-03-01T12:34:56Z","turnPosition":{"promptIndex":1,"turnIndex":1},"turnOrigin":"human","message":{"role":"user","content":"hi"}}"#,
                r#"{"type":"user","uuid":"i-1","timestamp":"2026-03-01T12:34:57Z","message":{"role":"user","content":"one more thing"}}"#,
                r#"{"type":"assistant","uuid":"a-1","timestamp":"2026-03-01T12:34:58Z","message":{"id":"m-1","model":"claude-opus-5-5","role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"ok"}],"usage":{"input_tokens":1,"output_tokens":2,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#,
            ]
            .join("\n"),
        );
        let lines = out(&lines);
        let kinds: Vec<&str> = lines.iter().map(kind_of).collect();
        assert_eq!(kinds, ["UserMessage", "ExternalMessage", "ModelChanged", "AssistantMessage", "TurnSettled"]);
    }

    #[test]
    fn synthetic_sidechain_meta_and_unnamed_shapes_yield_no_line() {
        // A real reply is the positive control: it must read, so an empty
        // translator cannot pass the negative cases below.
        let lines = window(
            &[
                r#"{"type":"assistant","uuid":"ok-1","message":{"id":"m-ok","model":"claude-opus-5-5","content":[{"type":"text","text":"hi"}]}}"#,
                r#"{"type":"assistant","uuid":"s-1","message":{"id":"m-s","model":"<synthetic>","stop_reason":null,"content":[{"type":"text","text":"x"}]}}"#,
                r#"{"type":"assistant","uuid":"c-1","isSidechain":true,"message":{"id":"m-c","model":"claude-opus-5-5","content":[]}}"#,
                r#"{"type":"user","uuid":"m-1","isMeta":true,"message":{"role":"user","content":"meta"}}"#,
                r#"{"type":"system","subtype":"compact_boundary","uuid":"y-1"}"#,
                r#"{"type":"assistant","uuid":"x-1","message":{"id":"m-x","model":"claude-opus-5-5","content":"not blocks"}}"#,
                r#"{"type":"assistant","message":{"id":"m-nu","model":"claude-opus-5-5","content":[{"type":"text","text":"x"}]}}"#,
                r#"{"type":"assistant","uuid":"nid-1","message":{"model":"claude-opus-5-5","content":[{"type":"text","text":"x"}]}}"#,
                "not json at all",
            ]
            .join("\n"),
        );
        let ids: Vec<String> = out(&lines).iter().map(|line| line["id"].as_str().unwrap().to_string()).collect();
        assert_eq!(ids, ["model-ok-1", "ok-1"]);
    }

    #[test]
    fn a_synthetic_stop_settles_the_open_turn_it_closes() {
        let lines = out(&window(
            &[
                r#"{"type":"user","uuid":"o-1","timestamp":"2026-03-01T12:34:56Z","turnPosition":{"promptIndex":1,"turnIndex":1},"turnOrigin":"human","message":{"role":"user","content":"hi"}}"#,
                r#"{"type":"assistant","uuid":"a-1","timestamp":"2026-03-01T12:34:57Z","message":{"id":"m-1","model":"claude-opus-5-5","role":"assistant","stop_reason":null,"content":[{"type":"text","text":"working"}]}}"#,
                r#"{"type":"assistant","uuid":"s-1","timestamp":"2026-03-01T12:34:58Z","message":{"id":"m-s","model":"<synthetic>","role":"assistant","stop_reason":"refusal","content":[{"type":"text","text":"refused"}]}}"#,
            ]
            .join("\n"),
        ));
        let settled = by_id(&lines, "settled-m-s");
        assert_eq!(kind_of(settled), "TurnSettled");
        assert_eq!(settled["kind"]["TurnSettled"]["stop_reason"], "refusal");
        assert!(lines.iter().all(|line| line["id"] != "s-1"));
    }

    #[test]
    fn a_stamp_that_does_not_parse_leaves_ts_ms_out_and_a_fraction_keeps_its_millis() {
        let lines = window(
            &[
                r#"{"type":"user","uuid":"o-1","timestamp":"not a time","turnPosition":{"promptIndex":1,"turnIndex":1},"turnOrigin":"human","message":{"role":"user","content":"hi"}}"#,
                r#"{"type":"user","uuid":"o-2","timestamp":"2026-03-01T12:34:56.5Z","turnPosition":{"promptIndex":2,"turnIndex":2},"turnOrigin":"human","message":{"role":"user","content":"hi"}}"#,
            ]
            .join("\n"),
        );
        let lines = out(&lines);
        assert!(by_id(&lines, "o-1").get("ts_ms").is_none());
        assert_eq!(by_id(&lines, "o-2")["ts_ms"], 1772368496500_i64);
    }

    #[test]
    fn a_message_takes_its_usage_from_its_last_record() {
        // A message's earlier records can carry partial usage; its last record
        // holds the total.
        let lines = out(&window(
            &[
                r#"{"type":"user","uuid":"o-1","timestamp":"2026-03-01T12:34:56Z","turnPosition":{"promptIndex":1,"turnIndex":1},"turnOrigin":"human","message":{"role":"user","content":"hi"}}"#,
                r#"{"type":"assistant","uuid":"a-1","timestamp":"2026-03-01T12:34:57Z","message":{"id":"m-1","model":"claude-opus-5-5","role":"assistant","stop_reason":null,"content":[{"type":"text","text":"one"}],"usage":{"input_tokens":3,"output_tokens":5,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#,
                r#"{"type":"assistant","uuid":"a-2","timestamp":"2026-03-01T12:34:58Z","message":{"id":"m-1","model":"claude-opus-5-5","role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"two"}],"usage":{"input_tokens":3,"output_tokens":6058,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#,
            ]
            .join("\n"),
        ));
        let settles: Vec<&Value> = lines.iter().filter(|line| kind_of(line) == "TurnSettled").collect();
        assert_eq!(settles.len(), 1);
        assert_eq!(settles[0]["kind"]["TurnSettled"]["usage"]["output_tokens"], 6058);
    }

    #[test]
    fn a_pause_turn_leaves_its_turn_open() {
        let lines = out(&window(
            &[
                r#"{"type":"user","uuid":"o-1","timestamp":"2026-03-01T12:34:56Z","turnPosition":{"promptIndex":1,"turnIndex":1},"turnOrigin":"human","message":{"role":"user","content":"hi"}}"#,
                r#"{"type":"assistant","uuid":"a-1","timestamp":"2026-03-01T12:34:57Z","message":{"id":"m-1","model":"claude-opus-5-5","role":"assistant","stop_reason":"pause_turn","content":[{"type":"text","text":"paused"}],"usage":{"input_tokens":1,"output_tokens":1,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#,
            ]
            .join("\n"),
        ));
        assert!(lines.iter().any(|line| kind_of(line) == "AssistantMessage"));
        assert!(lines.iter().all(|line| kind_of(line) != "TurnSettled"));
    }

    #[test]
    fn the_window_is_the_transcripts_last_eight_mebibytes() {
        const MIB: usize = 1 << 20;
        let opener = |uuid: &str| {
            json!({
                "type": "user",
                "uuid": uuid,
                "timestamp": "2026-03-01T12:34:56Z",
                "turnPosition": {"promptIndex": 1, "turnIndex": 1},
                "turnOrigin": "human",
                "message": {"role": "user", "content": "hi"}
            })
            .to_string()
        };
        let filler = json!({"type": "attachment", "pad": "x".repeat(1000)}).to_string();
        let reply = json!({
            "type": "assistant",
            "uuid": "a-end",
            "timestamp": "2026-03-01T12:40:00Z",
            "message": {
                "id": "m-end",
                "model": "claude-opus-5-5",
                "role": "assistant",
                "stop_reason": "end_turn",
                "content": [{"type": "text", "text": "done"}],
                "usage": {"input_tokens": 1, "output_tokens": 1, "cache_creation_input_tokens": 0, "cache_read_input_tokens": 0}
            }
        })
        .to_string();
        let mut text = format!("{}\n", opener("u-early"));
        while text.len() < 4 * MIB {
            text.push_str(&filler);
            text.push('\n');
        }
        text.push_str(&opener("u-late"));
        text.push('\n');
        while text.len() < 10 * MIB {
            text.push_str(&filler);
            text.push('\n');
        }
        text.push_str(&reply);
        text.push('\n');

        let dir = std::env::temp_dir().join(format!("claude-trace-window-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a temp dir");
        let path = dir.join("transcript.jsonl");
        std::fs::write(&path, &text).expect("the transcript");
        let read = claude_trace_tail(&path);
        let _ = std::fs::remove_dir_all(&dir);
        let ids: Vec<String> = read
            .expect("a file reads")
            .iter()
            .map(|line| {
                let record: Value = serde_json::from_str(line).expect("a line");
                record["id"].as_str().expect("an id").to_string()
            })
            .collect();
        assert!(ids.iter().any(|id| id == "u-late"), "an opener 6 MiB back is in the window");
        assert!(ids.iter().all(|id| id != "u-early"), "an opener 10 MiB back is not");
        assert!(ids.iter().any(|id| id == "a-end"), "the last reply is in the window");
    }

    #[test]
    fn a_tool_result_names_its_call_and_carries_its_content_and_error() {
        let lines = out(&window(
            &[
                r#"{"type":"user","uuid":"r-1","timestamp":"2026-03-01T12:34:57Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"call-1","content":"file body","is_error":true}]}}"#,
                r#"{"type":"user","uuid":"r-2","timestamp":"2026-03-01T12:34:58Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"call-2","content":[{"type":"text","text":"a"},{"type":"text","text":"b"}]}]}}"#,
            ]
            .join("\n"),
        ));
        let first = &by_id(&lines, "r-1")["kind"]["ToolResult"];
        assert_eq!(first["tool_use_id"], "call-1");
        assert_eq!(first["content"], "file body");
        assert_eq!(first["is_error"], true);
        let second = &by_id(&lines, "r-2")["kind"]["ToolResult"];
        assert_eq!(second["tool_use_id"], "call-2");
        assert_eq!(second["content"], "a\nb");
        assert_eq!(second["is_error"], false);
    }

    #[test]
    fn results_in_one_record_take_their_block_index_and_one_takes_the_bare_id() {
        let lines = out(&window(
            &[
                r#"{"type":"user","uuid":"m-1","timestamp":"2026-03-01T12:34:57Z","message":{"role":"user","content":[{"type":"text","text":"note"},{"type":"tool_result","tool_use_id":"call-a","content":"a"},{"type":"tool_result","tool_use_id":"call-b","content":"b"}]}}"#,
            ]
            .join("\n"),
        ));
        let results: Vec<(&str, &str)> = lines
            .iter()
            .filter(|line| kind_of(line) == "ToolResult")
            .map(|line| {
                (
                    line["id"].as_str().expect("an id"),
                    line["kind"]["ToolResult"]["tool_use_id"].as_str().expect("a call id"),
                )
            })
            .collect();
        assert_eq!(results, [("m-1.1", "call-a"), ("m-1.2", "call-b")]);
    }

    #[test]
    fn a_message_keeps_its_text_and_thinking_and_drops_signatures_and_unknown_blocks() {
        let lines = out(&window(
            &[
                r#"{"type":"assistant","uuid":"b-1","timestamp":"2026-03-01T12:34:57Z","message":{"id":"m-3","model":"claude-opus-5-5","role":"assistant","stop_reason":"tool_use","content":[{"type":"thinking","thinking":"hmm","signature":"sig"},{"type":"thinking","thinking":""},{"type":"redacted_thinking","data":"opaque"},{"type":"server_tool_use","id":"s-1","name":"web","input":{}},{"type":"tool_use","id":"call-1","name":"Read","input":{"file_path":"/x"}},{"type":"text","text":"done"}]}}"#,
            ]
            .join("\n"),
        ));
        let message = &by_id(&lines, "b-1")["kind"]["AssistantMessage"];
        assert_eq!(message["role"], "assistant");
        assert_eq!(message["message_id"], "m-3");
        let content = message["content"].as_array().expect("content blocks");
        assert_eq!(content.len(), 5, "server_tool_use is dropped: {content:?}");
        assert_eq!(content[0], json!({"type": "thinking", "thinking": "hmm"}));
        assert_eq!(content[1], json!({"type": "thinking", "thinking": ""}));
        assert_eq!(content[2], json!({"type": "thinking", "thinking": ""}));
        assert_eq!(content[3]["id"], "call-1");
        assert_eq!(content[3]["name"], "Read");
        let input = content[3]["input"].as_str().expect("input is a JSON string");
        assert_eq!(serde_json::from_str::<Value>(input).expect("it parses"), json!({"file_path": "/x"}));
        assert_eq!(content[4], json!({"type": "text", "text": "done"}));
    }

    #[test]
    fn an_external_opener_is_named_by_its_origin_then_its_turn_origin_then_unknown() {
        let lines = out(&window(
            &[
                r#"{"type":"user","uuid":"p-1","timestamp":"2026-03-01T12:34:56Z","turnPosition":{"promptIndex":1,"turnIndex":1},"turnOrigin":"peer","origin":{"kind":"peer","from":"agent-b"},"message":{"role":"user","content":"hi"}}"#,
                r#"{"type":"user","uuid":"p-2","timestamp":"2026-03-01T12:34:57Z","turnPosition":{"promptIndex":2,"turnIndex":2},"turnOrigin":"peer","origin":{"kind":"task_notification"},"message":{"role":"user","content":"done"}}"#,
                r#"{"type":"user","uuid":"p-3","timestamp":"2026-03-01T12:34:58Z","turnPosition":{"promptIndex":3,"turnIndex":3},"turnOrigin":"peer","message":{"role":"user","content":"again"}}"#,
                r#"{"type":"user","uuid":"p-4","timestamp":"2026-03-01T12:34:59Z","turnPosition":{"promptIndex":4,"turnIndex":4},"message":{"role":"user","content":"who"}}"#,
            ]
            .join("\n"),
        ));
        let from = |id: &str| by_id(&lines, id)["kind"]["ExternalMessage"]["from"].clone();
        assert_eq!(from("p-1"), "agent-b");
        assert_eq!(from("p-2"), "task_notification");
        assert_eq!(from("p-3"), "peer");
        assert_eq!(from("p-4"), "unknown");
        assert!(by_id(&lines, "p-1")["kind"]["ExternalMessage"]["channel"].is_null());
    }

    #[test]
    fn a_failed_result_is_carried_with_its_call_id_and_its_error() {
        let lines = out(&window(TURNS));
        let failed = &by_id(&lines, "u-0029")["kind"]["ToolResult"];
        assert_eq!(failed["tool_use_id"], "toolu_0003");
        assert_eq!(failed["is_error"], true);
    }

    const SUBAGENT: &str = include_str!("../../tests/fixtures/claude-subagent.jsonl");

    /// A temp dir holding `files`, named `tag`: a sub-agent's `subagents/` dir in
    /// miniature. The caller removes it.
    fn staged(tag: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("claude-subagent-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a temp dir");
        for (name, text) in files {
            std::fs::write(dir.join(name), text).expect("a staged file");
        }
        dir
    }

    /// Read `lines` as one sub-agent file through its own reader, parsed.
    fn sub_out(tag: &str, lines: &[&str]) -> Vec<Value> {
        let body = lines.join("\n") + "\n";
        let dir = staged(tag, &[("agent-s.jsonl", body.as_str())]);
        let read = claude_subagent_trace_tail(&dir.join("agent-s.jsonl")).expect("a file reads");
        let _ = std::fs::remove_dir_all(&dir);
        read.iter().map(|line| serde_json::from_str(line).expect("a line is JSON")).collect()
    }

    #[test]
    fn a_subagent_file_reads_whole_through_its_own_reader_and_not_through_the_main_one() {
        let dir = staged("fixture", &[("agent-a1.jsonl", SUBAGENT)]);
        let file = dir.join("agent-a1.jsonl");
        // Every record of a sub-agent file is a sidechain: the main reader drops them all.
        assert_eq!(claude_trace_tail(&file), Some(Vec::new()));
        let lines = claude_subagent_trace_tail(&file).expect("a file reads");
        let _ = std::fs::remove_dir_all(&dir);
        let out: Vec<Value> = lines.iter().map(|l| serde_json::from_str(l).expect("a line is JSON")).collect();
        assert_eq!(out.len(), 44, "{out:?}");
        // The first record is the prompt, from no named sender; the meta record yields no line.
        assert_eq!(out[0]["id"], "u-0001");
        assert_eq!(out[0]["kind"]["ExternalMessage"]["from"], "unknown");
        assert!(out.iter().all(|line| line["id"] != "u-0002"));
        // One model named once, before the first of 26 messages.
        let model_at = out.iter().position(|line| kind_of(line) == "ModelChanged").expect("a model line");
        assert_eq!(out[model_at]["id"], "model-u-0011");
        assert_eq!(out[model_at]["kind"]["ModelChanged"]["model"], ":claude-haiku-5-5");
        let first_message = out.iter().position(|line| kind_of(line) == "AssistantMessage").expect("a message");
        assert!(model_at < first_message);
        assert_eq!(out.iter().filter(|line| kind_of(line) == "AssistantMessage").count(), 26);
        assert_eq!(out.iter().filter(|line| kind_of(line) == "ToolResult").count(), 15);
        // The hand-back result closes the turn: its settle comes last, with the four sums.
        let settle = out.last().expect("a last line");
        assert_eq!(kind_of(settle), "TurnSettled");
        assert_eq!(settle["id"], "settled-u-0063");
        assert_eq!(settle["ts_ms"], 1_767_225_659_002_i64);
        assert!(settle["kind"]["TurnSettled"]["stop_reason"].is_null());
        assert_eq!(
            settle["kind"]["TurnSettled"]["usage"],
            json!({
                "input_tokens": 22,
                "output_tokens": 52,
                "cache_creation_input_tokens": 65_202,
                "cache_read_input_tokens": 431_249,
            })
        );
    }

    #[test]
    fn a_hand_back_result_settles_its_open_turn_right_after_its_line() {
        let out = sub_out(
            "hand-back",
            &[
                r#"{"type":"user","uuid":"s-1","timestamp":"2026-03-01T12:34:56Z","isSidechain":true,"message":{"role":"user","content":"go"}}"#,
                r#"{"type":"assistant","uuid":"s-2","timestamp":"2026-03-01T12:34:57Z","isSidechain":true,"message":{"id":"m-h","model":"claude-haiku-5-5","role":"assistant","stop_reason":null,"content":[{"type":"tool_use","id":"toolu-h","name":"SubagentHandback","input":{}}],"usage":{"input_tokens":3,"output_tokens":4,"cache_creation_input_tokens":5,"cache_read_input_tokens":6}}}"#,
                r#"{"type":"user","uuid":"s-3","timestamp":"2026-03-01T12:34:58Z","isSidechain":true,"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu-h","content":"report"}]}}"#,
            ],
        );
        let kinds: Vec<&str> = out.iter().map(kind_of).collect();
        assert_eq!(kinds, ["ExternalMessage", "ModelChanged", "AssistantMessage", "ToolResult", "TurnSettled"]);
        let result = &out[3];
        let settle = &out[4];
        assert_eq!(settle["id"], "settled-s-3");
        assert_eq!(settle["ts_ms"], 1_772_368_498_000_i64, "the result line's time");
        assert!(settle["kind"]["TurnSettled"]["stop_reason"].is_null(), "the call's stop reason, null as written");
        assert_eq!(
            settle["kind"]["TurnSettled"]["usage"],
            json!({"input_tokens": 3, "output_tokens": 4, "cache_creation_input_tokens": 5, "cache_read_input_tokens": 6}),
        );
        assert_eq!(result["kind"]["ToolResult"]["tool_use_id"], "toolu-h");
    }

    #[test]
    fn another_tools_result_does_not_settle_and_a_hand_back_with_no_open_turn_emits_no_settle() {
        let other = sub_out(
            "other-tool",
            &[
                r#"{"type":"assistant","uuid":"s-2","timestamp":"2026-03-01T12:34:57Z","isSidechain":true,"message":{"id":"m-b","model":"claude-haiku-5-5","role":"assistant","stop_reason":null,"content":[{"type":"tool_use","id":"toolu-b","name":"Bash","input":{}}]}}"#,
                r#"{"type":"user","uuid":"s-3","timestamp":"2026-03-01T12:34:58Z","isSidechain":true,"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu-b","content":"ok"}]}}"#,
            ],
        );
        // The positive control: the file reads, so the negative below means something.
        let other_kinds: Vec<&str> = other.iter().map(kind_of).collect();
        assert_eq!(other_kinds, ["ModelChanged", "AssistantMessage", "ToolResult"], "{other:?}");
        assert!(other.iter().all(|line| kind_of(line) != "TurnSettled"), "{other:?}");

        // The hand-back's message is open; a plain end-of-turn message settles that turn first, so the
        // result lands with no open turn and settles nothing.
        let closed = sub_out(
            "no-open-turn",
            &[
                r#"{"type":"assistant","uuid":"s-2","timestamp":"2026-03-01T12:34:57Z","isSidechain":true,"message":{"id":"m-h","model":"claude-haiku-5-5","role":"assistant","stop_reason":null,"content":[{"type":"tool_use","id":"toolu-h","name":"SubagentHandback","input":{}}]}}"#,
                r#"{"type":"assistant","uuid":"s-4","timestamp":"2026-03-01T12:34:57.500Z","isSidechain":true,"message":{"id":"m-e","model":"claude-haiku-5-5","role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"done"}]}}"#,
                r#"{"type":"user","uuid":"s-5","timestamp":"2026-03-01T12:34:58Z","isSidechain":true,"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu-h","content":"late"}]}}"#,
            ],
        );
        let settles: Vec<&str> = closed.iter().filter(|line| kind_of(line) == "TurnSettled").filter_map(|line| line["id"].as_str()).collect();
        assert_eq!(settles, ["settled-m-e"], "{closed:?}");
    }

    #[test]
    fn a_hand_back_whose_call_comes_after_its_result_settles_nothing() {
        let out = sub_out(
            "result-first",
            &[
                r#"{"type":"assistant","uuid":"s-1","timestamp":"2026-03-01T12:34:57Z","isSidechain":true,"message":{"id":"m-t","model":"claude-haiku-5-5","role":"assistant","stop_reason":null,"content":[{"type":"text","text":"working"}]}}"#,
                r#"{"type":"user","uuid":"s-2","timestamp":"2026-03-01T12:34:58Z","isSidechain":true,"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu-h","content":"early"}]}}"#,
                r#"{"type":"assistant","uuid":"s-3","timestamp":"2026-03-01T12:34:59Z","isSidechain":true,"message":{"id":"m-h","model":"claude-haiku-5-5","role":"assistant","stop_reason":null,"content":[{"type":"tool_use","id":"toolu-h","name":"SubagentHandback","input":{}}]}}"#,
            ],
        );
        // The positive control: the file reads, so the negative below means something.
        let kinds: Vec<&str> = out.iter().map(kind_of).collect();
        assert_eq!(kinds, ["ModelChanged", "AssistantMessage", "ToolResult", "AssistantMessage"], "{out:?}");
        assert!(out.iter().all(|line| kind_of(line) != "TurnSettled"), "{out:?}");
    }
}
