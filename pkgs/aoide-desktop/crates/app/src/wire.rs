//! The push wire: what the window receives on a chat's channel. Each string is
//! spelled before it leaves Rust.

use std::sync::Arc;

use aoide_desktop_shell::chat::record::Push as ChatPush;
use serde::Serialize;
use serde_json::Value;

/// One message of a chat, tagged by `type`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Push {
    Record { value: Value },
    Other { text: String },
    Dropped { bytes: usize },
    End,
}

/// Where a chat's messages go: the window's channel, or a test's collector.
pub type Sink = Arc<dyn Fn(Push) + Send + Sync>;

pub fn wire(push: ChatPush) -> Push {
    match push {
        ChatPush::Record(value) => Push::Record { value },
        ChatPush::Other(text) => Push::Other { text },
        ChatPush::Dropped(bytes) => Push::Dropped { bytes },
        ChatPush::End => Push::End,
    }
}
