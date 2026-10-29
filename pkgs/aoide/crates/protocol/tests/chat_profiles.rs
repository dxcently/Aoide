//! Every agent profile names its chat, and Eidolon's records project into chat
//! lines through its adapter.

use aoide_protocol::agents::{agent_profile, known_agents, eidolon_trace_record};
use aoide_protocol::chat::{ChatLine, ChatSupport};
use serde_json::{json, Value};

const USER: &str = r##"{"id":1,"parent":0,"ts_ms":1789603005570,"kind":{"UserMessage":{"role":"user","content":[{"type":"text","text":"# Brief A: …"}]}}}"##;
const ASSISTANT: &str = r#"{"id":2,"parent":1,"ts_ms":1789603009102,"kind":{"AssistantMessage":{"role":"assistant","content":[{"type":"thinking","thinking":"I should read the slot catalog first.","signature":"…"},{"type":"text","text":"Let me read the slot catalog."},{"type":"tool_use","id":"call_8vr43zri","name":"read","input":{"path":"slots.md"}}]}}}"#;
const RESULT: &str = r#"{"id":3,"parent":2,"ts_ms":1789603009140,"kind":{"ToolResult":{"tool_use_id":"call_8vr43zri","content":"1 # Per-rice widget slots","is_error":false}}}"#;
const RESULT_ERR: &str = r#"{"id":4,"parent":3,"ts_ms":1789603009200,"kind":{"ToolResult":{"tool_use_id":"call_9","content":"ENOENT: no such file","is_error":true}}}"#;
const SETTLED: &str = r#"{"id":131,"parent":130,"ts_ms":1789606421000,"kind":{"TurnSettled":{"stop_reason":"end_turn","usage":{"input_tokens":9,"output_tokens":7}}}}"#;
const CANCELLED: &str = r#"{"id":77,"parent":76,"ts_ms":1789626990000,"kind":"Cancelled"}"#;
const ASK: &str = r#"{"id":40,"parent":39,"ts_ms":1789626500000,"kind":{"AskUser":{"call_id":"call_x","prompt":"Overwrite?","answer":null}}}"#;
const EXTERNAL: &str = r#"{"id":55,"parent":54,"ts_ms":1789626700000,"kind":{"ExternalMessage":{"from":"orchestrator","channel":null,"text":"STOP: write the report now"}}}"#;
const OPERATOR: &str = r#"{"id":56,"parent":55,"ts_ms":1789626800000,"kind":{"OperatorMessage":{"from":"khoa","text":"status?"}}}"#;
const START: &str = r#"{"id":0,"parent":null,"ts_ms":1789603005561,"kind":{"SessionStart":{"model":"m","cwd":"/x","system":null}}}"#;

fn project(line: &str) -> Vec<Value> {
    let profile = agent_profile("eidolon").expect("eidolon has a profile");
    let ChatSupport::Adapter(adapter) = &profile.chat else {
        panic!("eidolon has a chat adapter");
    };
    (adapter.project)("s-1", line).iter().map(ChatLine::json).collect()
}

#[test]
fn every_profile_says_what_its_chat_is() {
    for name in known_agents() {
        let profile = agent_profile(name).expect("a known agent has a profile");
        if let ChatSupport::Unsupported(reason) = profile.chat {
            assert!(!reason.trim().is_empty(), "{name} names no reason for having no chat");
        }
    }
}

#[test]
fn a_profile_with_a_trace_reader_has_a_chat_adapter() {
    for name in known_agents() {
        let profile = agent_profile(name).expect("a known agent has a profile");
        if profile.transcript.trace.is_some() {
            assert!(
                matches!(profile.chat, ChatSupport::Adapter(_)),
                "{name} keeps a trace and has no chat adapter"
            );
        }
    }
}

#[test]
fn eidolon_user_and_operator_messages_are_user_lines() {
    let user = project(USER);
    assert_eq!(user, vec![json!({"id":"1","role":"user","session":"s-1","text":"# Brief A: …","ts":1789603005570_i64,"v":1})]);
    let op = project(OPERATOR);
    assert_eq!(op[0]["role"], "user");
    assert_eq!(op[0]["from"], "khoa");
    assert_eq!(op[0]["text"], "status?");
}

#[test]
fn eidolon_assistant_blocks_each_project_one_line_in_order() {
    let lines = project(ASSISTANT);
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert_eq!(lines[0]["id"], "2.0");
    assert_eq!(lines[0]["thinking"], true);
    assert_eq!(lines[0]["text"], "I should read the slot catalog first.");
    assert_eq!(lines[1]["role"], "agent");
    assert_eq!(lines[1]["text"], "Let me read the slot catalog.");
    assert_eq!(lines[2]["role"], "tool");
    assert_eq!(lines[2]["tool"]["name"], "read");
    assert!(lines[2]["tool"]["input"].as_str().unwrap().contains("slots.md"));
}

#[test]
fn eidolon_tool_results_name_the_call_and_carry_the_error_flag() {
    let ok = project(RESULT);
    assert_eq!(ok[0]["role"], "tool");
    assert_eq!(ok[0]["tool"]["name"], "call_8vr43zri");
    assert_eq!(ok[0]["tool"]["result"], "1 # Per-rice widget slots");
    assert!(ok[0]["tool"].get("error").is_none());
    let err = project(RESULT_ERR);
    assert_eq!(err[0]["tool"]["error"], true);
    assert_eq!(err[0]["tool"]["result"], "ENOENT: no such file");
}

#[test]
fn eidolon_turn_ends_and_external_messages_are_system_lines() {
    let settled = project(SETTLED);
    assert_eq!(settled[0]["role"], "system");
    assert_eq!(settled[0]["state"], "settled");
    assert_eq!(settled[0]["text"], "end_turn");
    let cancelled = project(CANCELLED);
    assert_eq!(cancelled[0]["state"], "settled");
    assert_eq!(cancelled[0]["text"], "cancelled");
    let ext = project(EXTERNAL);
    assert_eq!(ext[0]["role"], "system");
    assert_eq!(ext[0]["from"], "orchestrator");
    assert_eq!(ext[0]["text"], "STOP: write the report now");
}

#[test]
fn eidolon_an_ask_is_an_agent_line_carrying_the_ask() {
    let lines = project(ASK);
    assert_eq!(lines[0]["role"], "agent");
    assert_eq!(lines[0]["ask"]["id"], "call_x");
    assert_eq!(lines[0]["ask"]["prompt"], "Overwrite?");
}

#[test]
fn eidolon_a_start_record_projects_nothing() {
    assert!(project(START).is_empty());
    assert!(eidolon_trace_record(START).is_some(), "the start line is a record, not a chat line");
}
