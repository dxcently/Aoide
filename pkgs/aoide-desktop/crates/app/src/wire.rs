//! The push wire: what the window receives on a chat's channel, and on the asks
//! channel. Each string is spelled before it leaves Rust.

use std::sync::Arc;

use aoide_desktop_shell::asks::{Change, View};
use aoide_desktop_shell::chat::record::{Line, Push as ChatPush};
use serde::Serialize;
use serde_json::Value;

/// One message of a chat, tagged by `type`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Push {
    Line { line: Box<Line> },
    Record { value: Value },
    Other { text: String },
    Dropped { bytes: usize },
    End,
}

/// Where a chat's messages go: the window's channel, or a test's collector.
pub type Sink = Arc<dyn Fn(Push) + Send + Sync>;

pub fn wire(push: ChatPush) -> Push {
    match push {
        ChatPush::Line(line) => Push::Line { line },
        ChatPush::Record(value) => Push::Record { value },
        ChatPush::Other(text) => Push::Other { text },
        ChatPush::Dropped(bytes) => Push::Dropped { bytes },
        ChatPush::End => Push::End,
    }
}

/// One message of the asks channel, tagged by `type`: an ask open (or open with
/// other content), an ask gone, a line the watch printed that is no ask, a line
/// dropped past the cap, or the end of the watch.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum AskPush {
    Open { ask: Box<View> },
    Gone { session: String, ask_id: u64 },
    Other { text: String },
    Dropped { bytes: usize },
    End,
}

/// Where the asks go: the window's channel, or a test's collector.
pub type AskSink = Arc<dyn Fn(AskPush) + Send + Sync>;

pub fn ask_wire(change: Change) -> AskPush {
    match change {
        Change::Added(held) => AskPush::Open { ask: Box::new(held.view()) },
        Change::Removed { session, ask_id, .. } => AskPush::Gone { session, ask_id },
        Change::Other(text) => AskPush::Other { text },
        Change::Dropped(bytes) => AskPush::Dropped { bytes },
        Change::End => AskPush::End,
    }
}
