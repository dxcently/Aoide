//! A2A (Agent2Agent) wire shapes (CONTRACTS.md §6) — the AgentCard, the Task
//! envelope, the `message/send` request body, and the SSE
//! `TaskStatusUpdateEvent` the streaming methods emit as their final event.
//!
//! These mirror the A2A v0.3.x JSON-RPC binding aoide's own server speaks
//! (root `src/a2a.rs`): a flat `url` on the card, lowercase-kebab
//! `TaskState`s, `message/send`/`tasks/get`/`message/stream`/
//! `tasks/resubscribe` methods. [`AgentCard`] is also loose enough to
//! deserialize a REMOTE node's card in the v1.0 `interfaces[]` form
//! (`interfaces` is additive/optional here for that reason), though today
//! only the OUTBOUND build side (`a2a.rs::agent_card_from_commands`) uses
//! this type directly — a fetched remote card (`node add`'s verification
//! fetch) is only ever checked for well-formed JSON, never deserialized
//! field-by-field through this struct: a remote AgentCard is
//! attacker-influenced input (any node an operator registers), and a
//! single mistyped/extra field on it should never fail an all-or-nothing
//! `Deserialize` for the whole card.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The A2A AgentCard (CONTRACTS.md §6) — aoide's own, as served from
/// `/.well-known/agent-card.json` (`a2a.rs::agent_card_from_commands`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AgentCard {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(rename = "protocolVersion", default, skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<String>,
    /// The A2A v0.3.x JSON-RPC binding's flat endpoint. A v1.0 card instead
    /// carries `interfaces[]` (below) — aoide's own card only ever emits
    /// `url`, but a REMOTE node's card may use either form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<AgentCapabilities>,
    #[serde(rename = "defaultInputModes", default, skip_serializing_if = "Option::is_none")]
    pub default_input_modes: Option<Vec<String>>,
    #[serde(rename = "defaultOutputModes", default, skip_serializing_if = "Option::is_none")]
    pub default_output_modes: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<AgentSkill>>,
    /// The A2A v1.0 card form's endpoint list — absent on aoide's own card
    /// (v0.3.x, flat `url` only); present on some remote nodes' cards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interfaces: Option<Vec<AgentInterface>>,
}

/// `AgentCard.capabilities` — today just the one streaming flag (CONTRACTS.md
/// §6 Phase C: `message/stream` + `tasks/resubscribe`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct AgentCapabilities {
    #[serde(default)]
    pub streaming: bool,
}

/// One entry in `AgentCard.skills` — one per `implemented: true` registry
/// command, derived from `schema --json` (`a2a.rs::agent_card_from_commands`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AgentSkill {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// One entry in the A2A v1.0 card form's `interfaces[]` — the transport +
/// endpoint a remote node's card may advertise instead of a flat `url`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AgentInterface {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// An A2A Task envelope (`tasks/get`'s result, `message/send`'s result, and
/// each non-final `message/stream`/`tasks/resubscribe` SSE event's result).
/// CONTRACTS.md §6 MVP simplification: `id` and `context_id` are BOTH the
/// aoide sessionId (see `a2a.rs::task_from_sessions`'s doc comment).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    #[serde(rename = "contextId")]
    pub context_id: String,
    pub status: TaskStatus,
    /// Always `"task"` — the A2A discriminant for this result shape.
    pub kind: String,
    /// A2A's `Task.artifacts` — absent on a plain status read, and BOTH
    /// optional fields are skipped when `None` so every response that carries
    /// no output stays byte-identical to the pre-P-RSA shape. The aoide
    /// extension rides here: `tasks/get` with `metadata["aoide/frame"]` asks
    /// for the session's watch frame as ONE `data` artifact (CONTRACTS.md §6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifacts: Option<Vec<Artifact>>,
    /// A2A's `Task.history` — the same optional discipline as `artifacts`.
    /// aoide's use is the ping-back ring a remote parent pulls (CONTRACTS.md
    /// §6, `aoide/linesAfter`); a status-only read carries neither field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history: Option<Vec<Message>>,
}

/// One A2A `Artifact` — a named bag of [`Part`]s attached to a [`Task`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Artifact {
    #[serde(rename = "artifactId")]
    pub artifact_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub parts: Vec<Part>,
}

/// `Task.status` — the mapped `TaskState` (see `a2a.rs::a2a_task_state`) plus
/// the timestamp it was observed at.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskStatus {
    pub state: String,
    pub timestamp: String,
}

/// The FINAL event a `message/stream`/`tasks/resubscribe` SSE loop emits
/// (`a2a.rs::build_stream_event`, Phase C) — an A2A `TaskStatusUpdateEvent`,
/// distinct from the plain [`Task`] every non-final event carries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskStatusUpdateEvent {
    #[serde(rename = "taskId")]
    pub task_id: Value,
    #[serde(rename = "contextId")]
    pub context_id: Value,
    pub status: Value,
    #[serde(rename = "final")]
    pub is_final: bool,
    /// Always `"status-update"`.
    pub kind: String,
}

/// `message/send`'s `params` — one [`Message`] (CONTRACTS.md §6). Built by
/// aoide as the OUTBOUND request body (`a2a.rs::build_message_send_body`,
/// driving a registered external agent); the INBOUND server-side reader
/// (`a2a.rs::parse_message_send_params`) stays `Value`-based, since it tolerates
/// `contextId`/`metadata` living at either `message.*` or the top-level
/// `params.*` (a fallback shape [`MessageSendParams`] does not need to model
/// for the outbound builder's purposes).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageSendParams {
    pub message: Message,
}

/// The `message.metadata` key carrying the caller's OWN session id on the
/// node that signs the request (CONTRACTS.md §6; P-RSA). One const, read by
/// the door that honours the claim and written by the client that makes it —
/// the receiver qualifies the value with the KEY that verified the signature,
/// never with a name taken from the body.
pub const FROM_SESSION_KEY: &str = "aoide/from";

/// The `tasks/get` `params.metadata` key that asks for a session's watch
/// frame (CONTRACTS.md §6; P-RSA S6). Its value is an object whose optional
/// `tail` is the output-line window; the whole key being present is what
/// makes the response carry [`Artifact`]s at all. Read by the door that
/// serves the frame and written by the client that asks for it — one const,
/// both sides, the same discipline [`FROM_SESSION_KEY`] already sets.
pub const FRAME_KEY: &str = "aoide/frame";

/// The `artifactId` the watch frame rides under. Named once so the reader
/// finds the frame by identity rather than by position.
pub const FRAME_ARTIFACT_ID: &str = "frame";

/// One A2A `Message` (`message/send`'s `params.message`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub parts: Vec<Part>,
    #[serde(rename = "messageId", default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(rename = "contextId", default, skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

/// One A2A message `Part` — the `text`/`file`/`data` union. aoide's own
/// outbound builder only ever emits `kind: "text"` with `text` populated;
/// the door's own watch-frame artifact is the other shape in use
/// (`kind: "data"`, the frame under `data`), and `extra` round-trips
/// whatever either carries so this one type stays usable for both without
/// losing fields.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Part {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn agent_card_serializes_every_field_and_round_trips() {
        let card = AgentCard {
            name: Some("aoide".to_string()),
            description: "a headless conductor".to_string(),
            version: Some("0.0.0".to_string()),
            protocol_version: Some("0.3.0".to_string()),
            url: Some("http://127.0.0.1:8710/".to_string()),
            capabilities: Some(AgentCapabilities { streaming: true }),
            default_input_modes: Some(vec!["text/plain".to_string()]),
            default_output_modes: Some(vec!["text/plain".to_string()]),
            skills: Some(vec![AgentSkill {
                id: "foo.bar".to_string(),
                name: "foo.bar".to_string(),
                description: "does a thing".to_string(),
                tags: vec!["foo".to_string()],
            }]),
            interfaces: None,
        };
        let v = serde_json::to_value(&card).unwrap();
        assert_eq!(v["name"], "aoide");
        assert_eq!(v["protocolVersion"], "0.3.0");
        assert_eq!(v["capabilities"]["streaming"], true);
        assert_eq!(v["defaultInputModes"][0], "text/plain");
        assert_eq!(v["skills"][0]["id"], "foo.bar");
        // interfaces is None -> omitted entirely, not null.
        assert!(v.get("interfaces").is_none(), "serialized: {v}");

        let back: AgentCard = serde_json::from_value(v).unwrap();
        assert_eq!(back, card);
    }

    #[test]
    fn agent_card_v1_interfaces_form_deserializes() {
        let raw = json!({
            "name": "v1",
            "interfaces": [{ "transport": "JSONRPC", "url": "http://host:9000/rpc" }],
        });
        let card: AgentCard = serde_json::from_value(raw).unwrap();
        assert_eq!(card.name.as_deref(), Some("v1"));
        assert_eq!(card.interfaces.unwrap()[0].url.as_deref(), Some("http://host:9000/rpc"));
    }

    #[test]
    fn task_round_trips_the_captured_wire_shape() {
        let raw = json!({
            "id": "sess-1",
            "contextId": "sess-1",
            "status": { "state": "working", "timestamp": "2026-01-01T00:00:00Z" },
            "kind": "task",
        });
        let task: Task = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(task.id, "sess-1");
        assert_eq!(task.context_id, "sess-1");
        assert_eq!(task.status.state, "working");
        assert_eq!(task.kind, "task");
        assert_eq!(serde_json::to_value(&task).unwrap(), raw);
    }

    /// The status-only response is the shape every pre-P-RSA caller reads:
    /// `artifacts`/`history` are absent, never `null`.
    #[test]
    fn task_status_only_carries_no_artifacts_or_history() {
        let task = Task {
            id: "sess-1".to_string(),
            context_id: "sess-1".to_string(),
            status: TaskStatus { state: "working".to_string(), timestamp: "2026-01-01T00:00:00Z".to_string() },
            kind: "task".to_string(),
            artifacts: None,
            history: None,
        };
        let v = serde_json::to_value(&task).unwrap();
        assert_eq!(
            v,
            json!({
                "id": "sess-1",
                "contextId": "sess-1",
                "status": { "state": "working", "timestamp": "2026-01-01T00:00:00Z" },
                "kind": "task",
            }),
            "a status read must not grow a field"
        );
    }

    #[test]
    fn task_with_a_frame_artifact_round_trips() {
        let mut data = Part { kind: "data".to_string(), text: None, extra: Default::default() };
        data.extra.insert("data".to_string(), json!({ "sessionId": "sess-1", "output": ["one line"] }));
        let task = Task {
            id: "sess-1".to_string(),
            context_id: "sess-1".to_string(),
            status: TaskStatus { state: "working".to_string(), timestamp: "2026-01-01T00:00:00Z".to_string() },
            kind: "task".to_string(),
            artifacts: Some(vec![Artifact {
                artifact_id: FRAME_ARTIFACT_ID.to_string(),
                name: Some("session watch frame".to_string()),
                parts: vec![data],
            }]),
            history: None,
        };
        let v = serde_json::to_value(&task).unwrap();
        assert_eq!(v["artifacts"][0]["artifactId"], "frame");
        assert_eq!(v["artifacts"][0]["name"], "session watch frame");
        assert_eq!(v["artifacts"][0]["parts"][0]["kind"], "data");
        assert_eq!(v["artifacts"][0]["parts"][0]["data"]["output"][0], "one line");
        assert!(v.get("history").is_none(), "serialized: {v}");

        let back: Task = serde_json::from_value(v).unwrap();
        assert_eq!(back, task);
    }

    #[test]
    fn task_status_update_event_matches_the_captured_final_event_shape() {
        let ev = TaskStatusUpdateEvent {
            task_id: json!("sess-1"),
            context_id: json!("sess-1"),
            status: json!({ "state": "completed", "timestamp": "2026-01-01T00:00:01Z" }),
            is_final: true,
            kind: "status-update".to_string(),
        };
        let v = serde_json::to_value(&ev).unwrap();
        assert_eq!(v["taskId"], "sess-1");
        assert_eq!(v["contextId"], "sess-1");
        assert_eq!(v["final"], true);
        assert_eq!(v["kind"], "status-update");
        assert_eq!(v["status"]["state"], "completed");
    }

    #[test]
    fn message_send_params_matches_the_build_message_send_body_shape() {
        let params = MessageSendParams {
            message: Message {
                role: "user".to_string(),
                parts: vec![Part { kind: "text".to_string(), text: Some("hello there".to_string()), extra: Default::default() }],
                message_id: Some("mid-123".to_string()),
                context_id: None,
                metadata: None,
            },
        };
        let v = serde_json::to_value(&params).unwrap();
        assert_eq!(v["message"]["role"], "user");
        assert_eq!(v["message"]["messageId"], "mid-123");
        assert_eq!(v["message"]["parts"][0]["kind"], "text");
        assert_eq!(v["message"]["parts"][0]["text"], "hello there");
        // contextId/metadata are None -> omitted, not null.
        assert!(v["message"].get("contextId").is_none(), "serialized: {v}");
        assert!(v["message"].get("metadata").is_none(), "serialized: {v}");
    }

    #[test]
    fn part_extra_fields_round_trip_for_non_text_kinds() {
        let raw = json!({ "kind": "file", "uri": "ignored://non-text-part" });
        let part: Part = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(part.kind, "file");
        assert_eq!(part.text, None);
        assert_eq!(part.extra.get("uri").unwrap(), "ignored://non-text-part");
        assert_eq!(serde_json::to_value(&part).unwrap(), raw);
    }
}
