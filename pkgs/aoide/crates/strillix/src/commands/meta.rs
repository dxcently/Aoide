//! `guide` / `schema` — strillix's own onboarding text and machine-readable
//! command schema. Mirrors `aoide-cli`'s `commands/meta.rs`, reading strillix's
//! own assembled registry rather than core's.

use crate::dispatch::Invocation;
use crate::output::Outcome;
use crate::registry::{cmd, Registry};
use serde_json::{json, Value};
use aoide_protocol::Door;

pub fn register(r: &mut Registry) {
    r.insert(cmd!(
        path: ["guide"],
        summary: "Print strillix's tier-0 onboarding (the rice/screen/notifications bundle map).",
        args: [],
        flags: [],
        gated: false,
        implemented: true,
        handler: handle_guide,
        doors: &[Door::Cli, Door::Daemon, Door::Mcp],
    ));
    r.insert(cmd!(
        path: ["schema"],
        summary: "Emit the versioned machine-readable schema of every strillix command.",
        args: [],
        flags: [],
        gated: false,
        implemented: true,
        handler: handle_schema,
        brief: "Print every strillix command and state shape as JSON, for machines.",
        doors: &[Door::Cli, Door::Daemon, Door::Mcp],
    ));
}

fn handle_guide(_inv: &Invocation) -> Outcome {
    let text = crate::guide::render(crate::dispatch::registry());
    Outcome::ok("guide", "printed strillix's tier-0 onboarding").with_data(json!({ "text": text }))
}

fn handle_schema(_inv: &Invocation) -> Outcome {
    let doc = crate::dispatch::registry().schema("strillix");
    let val = serde_json::to_value(&doc).unwrap_or(Value::Null);
    Outcome::ok("schema", "emitted strillix's v0 command schema").with_data(val)
}
