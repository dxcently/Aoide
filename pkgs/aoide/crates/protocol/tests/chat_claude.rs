//! Claude Code's transcript projected into chat lines. The fixture is written
//! from Claude Code's public transcript docs (`docs-claude-transcript.jsonl`);
//! real-session fixtures replace it when Rook's probe (ruling 198) runs.

use aoide_protocol::agents::agent_profile;
use aoide_protocol::chat::{ChatLine, ChatSupport};
use serde_json::{json, Value};

// `docs-*` is written from Claude Code's public transcript docs; `claude-real-*`
// is a real session (a throwaway project, one prompt, one Read), sanitised.
const FIXTURE: &str = include_str!("fixtures/docs-claude-transcript.jsonl");
const REAL: &str = include_str!("fixtures/claude-real-read-tool.jsonl");

fn project(line: &str) -> Vec<Value> {
    let profile = agent_profile("claude").expect("claude has a profile");
    let ChatSupport::Adapter(adapter) = &profile.chat else {
        panic!("claude has a chat adapter");
    };
    (adapter.project)("s-1", line).iter().map(ChatLine::json).collect()
}

fn real(n: usize) -> &'static str {
    REAL.lines().nth(n).expect("real fixture line")
}

fn line(n: usize) -> &'static str {
    FIXTURE.lines().nth(n).expect("fixture line")
}

#[test]
fn a_user_string_is_one_user_line() {
    let got = project(line(0));
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0]["role"], json!("user"));
    assert_eq!(got[0]["text"], json!("fix the bug"));
    assert_eq!(got[0]["id"], json!("u1"));
}

#[test]
fn an_assistant_turn_splits_into_thinking_text_and_tool_lines() {
    let got = project(line(1));
    let ids: Vec<&Value> = got.iter().map(|v| &v["id"]).collect();
    assert_eq!(ids, [&json!("a1.0"), &json!("a1.1"), &json!("a1.2")], "{got:?}");
    assert_eq!(got[0]["role"], json!("agent"));
    assert_eq!(got[0]["thinking"], json!(true));
    assert_eq!(got[0]["text"], json!("read the file first"));
    assert_eq!(got[1]["text"], json!("Reading it now."));
    assert!(got[1].get("thinking").is_none());
    assert_eq!(got[2]["role"], json!("tool"));
    assert_eq!(got[2]["tool"]["name"], json!("Read"));
    assert!(got[2]["tool"]["input"].as_str().unwrap().contains("/x.rs"));
}

#[test]
fn a_tool_result_names_its_call_and_takes_string_or_array_content() {
    let ok = project(line(2));
    assert_eq!(ok.len(), 1, "{ok:?}");
    assert_eq!(ok[0]["role"], json!("tool"));
    assert_eq!(ok[0]["tool"]["name"], json!("toolu_1"));
    assert_eq!(ok[0]["tool"]["result"], json!("fn main() {}"));
    assert!(ok[0]["tool"].get("error").is_none());

    let err = project(line(3));
    assert_eq!(err[0]["tool"]["name"], json!("toolu_2"));
    assert_eq!(err[0]["tool"]["result"], json!("no such file"));
    assert_eq!(err[0]["tool"]["error"], json!(true));
}

#[test]
fn summaries_sidechains_and_meta_records_project_to_nothing() {
    for n in [4, 5, 6] {
        assert!(project(line(n)).is_empty(), "fixture line {n} must project to nothing");
    }
}

#[test]
fn a_string_assistant_reply_is_one_agent_line() {
    let got = project(line(7));
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0]["role"], json!("agent"));
    assert_eq!(got[0]["text"], json!("Fixed."));
}

#[test]
fn a_profile_names_claude_as_an_adapter_reading_the_transcript() {
    let profile = agent_profile("claude").expect("claude has a profile");
    assert!(matches!(profile.chat, ChatSupport::Adapter(_)));
}

#[test]
fn a_turn_spans_records_that_share_a_request_and_each_projects_its_own_lines() {
    // Records 3 and 4 are one assistant turn (one requestId), two records with
    // two uuids: each projects on its own, under its own id.
    let thinking: Value = serde_json::from_str(real(3)).unwrap();
    let text: Value = serde_json::from_str(real(4)).unwrap();
    assert_eq!(thinking["requestId"], text["requestId"]);
    assert_ne!(thinking["uuid"], text["uuid"]);
    let got = project(real(4));
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0]["role"], json!("agent"));
    assert_eq!(got[0]["id"], text["uuid"]);
    assert!(!got[0]["text"].as_str().unwrap_or("").is_empty());
}

#[test]
fn the_real_prompt_is_a_string_and_the_tool_result_is_an_array() {
    let prompt = project(real(0));
    assert_eq!(prompt.len(), 1, "{prompt:?}");
    assert_eq!(prompt[0]["role"], json!("user"));
    assert!(!prompt[0]["text"].as_str().unwrap_or("").is_empty());

    let result = project(real(2));
    assert_eq!(result.len(), 1, "{result:?}");
    assert_eq!(result[0]["role"], json!("tool"));
    assert!(result[0]["tool"]["result"].is_string(), "{result:?}");
}

#[test]
fn an_empty_thinking_block_emits_no_line() {
    // Print mode writes the thinking text empty, with only a signature. The
    // record is a thinking block and nothing else: no line at all.
    let thinking: Value = serde_json::from_str(real(3)).unwrap();
    assert_eq!(thinking["message"]["content"][0]["thinking"], json!(""));
    assert!(project(real(3)).is_empty(), "an empty thinking block is not a message");
}

#[test]
fn extra_keys_on_real_records_are_ignored_and_no_record_fails() {
    for n in 0..5 {
        let _ = project(real(n));
    }
    let lines: usize = (0..5).map(|n| project(real(n)).len()).sum();
    assert_eq!(lines, 4, "five records, one of them an empty thinking block");
}
