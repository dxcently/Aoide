//! The chat line v1 holds to its fixture and to CONTRACTS.md's block.

use aoide_protocol::chat::{Ask, AskKind, ChatLine, Role, State};

const FIXTURE: &str = include_str!("fixtures/chat-v1.jsonl");
const FIXTURE_PATH: &str = "pkgs/aoide/crates/protocol/tests/fixtures/chat-v1.jsonl";

/// One line of each role, plus a thinking line, a tool result, an error, a
/// peer message, a state line, two asks and a permission prompt.
fn lines() -> Vec<ChatLine> {
    let s = "s-1";
    vec![
        ChatLine::new(s, "1", Some(1789603005570), Role::User).text("hello <script>"),
        ChatLine::new(s, "2", Some(1789603009102), Role::Agent).text("Let me read the slot catalog."),
        ChatLine::new(s, "2.0", Some(1789603009100), Role::Agent)
            .thinking()
            .text("I should read the catalog first."),
        ChatLine::new(s, "2.1", Some(1789603009102), Role::Tool)
            .tool("read", Some("{\"path\":\"slots.md\"}"), None, false),
        ChatLine::new(s, "3", Some(1789603009140), Role::Tool)
            .tool("read", None, Some("1 # Per-rice widget slots"), false),
        ChatLine::new(s, "4", Some(1789603009200), Role::Tool)
            .tool("read", None, Some("ENOENT: no such file"), true),
        ChatLine::new(s, "5", Some(1789603010000), Role::System).from("peer-7").text("ping"),
        ChatLine::new(s, "6", None, Role::System).state(State::Waiting),
        ChatLine::new(s, "7", Some(1789626500000), Role::Agent)
            .ask(Ask::new("call_x", AskKind::Approval, "Overwrite?").answers(&["yes", "no"])),
        ChatLine::new(s, "8", Some(1789626510000), Role::Agent).ask(
            Ask::new("call_q", AskKind::Question, "Which rice?")
                .option("sunrise", "warm palette")
                .option("dusk", "cool palette"),
        ),
        ChatLine::new(s, "9", None, Role::System)
            .ask(Ask::new("perm-1", AskKind::Approval, "Bash: rm -rf build").fingerprint("3f9a")),
    ]
}

#[test]
fn each_line_is_the_fixture_byte_for_byte() {
    let got: String = lines().iter().map(|l| format!("{}\n", l.json_line())).collect();
    assert_eq!(got, FIXTURE);
}

/// CONTRACTS.md "The chat line — v1" quotes the fixture. The quote's first
/// line names the fixture, and the rest must equal it. The document sits two
/// levels above the crate in a checkout; a source tree without it (the nix
/// package's own source) has nothing to compare, and says so.
#[test]
fn the_contract_block_is_the_fixture() {
    let doc_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../CONTRACTS.md");
    let doc = std::fs::read_to_string(doc_path)
        .expect("CONTRACTS.md is in this source tree: the block compare has nothing to compare without it");
    let section = doc
        .split("### The chat line — **v1**")
        .nth(1)
        .expect("CONTRACTS.md has the chat line v1 section");
    let open = section.find("```").expect("the section has an example block") + 3;
    let rest = &section[open..];
    let rest = &rest[rest.find('\n').expect("the block opens on its own line") + 1..];
    let block = &rest[..rest.find("\n```").expect("the block closes")];
    let mut block_lines = block.lines();
    assert_eq!(block_lines.next(), Some(FIXTURE_PATH), "the block's first line names the fixture");
    let quoted: Vec<&str> = block_lines.collect();
    let expected: Vec<&str> = FIXTURE.lines().collect();
    assert_eq!(quoted, expected, "CONTRACTS.md's block and the fixture differ");
}
