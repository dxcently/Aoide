//! `strx migrate` — Strillix's trees out of core's root into the strillix root.
//! The home activation runs it on every switch; `--check` lists what a run
//! would move and moves nothing. `strillix_rice::migrate` owns the move; this
//! file only narrates it.

use crate::dispatch::Invocation;
use crate::output::Outcome;
use crate::registry::{cmd, flag, Registry};
use aoide_protocol::Door;
use serde_json::json;
use strillix_rice::migrate::{migrate, pending};

pub fn register(r: &mut Registry) {
    r.insert(cmd!(
        path: ["migrate"],
        summary: "Move Strillix's trees out of core's root ($AOIDE_ROOT) into the strillix root: run/qml, run/elements, state/skwd-wall, state/captures and state/pointer-pos.json, merged into a tree the strillix root already holds. The old song marker file is deleted. An entry the strillix root holds with other bytes stays where it is, and the verb exits nonzero. --check lists the old paths a run would move and moves nothing.",
        args: [],
        flags: [flag!("check", "bool", "List the old paths a run would move; move nothing.")],
        gated: false,
        implemented: true,
        handler: handle_migrate,
        examples: ["migrate --check", "migrate --json"],
        brief: "Move Strillix's trees out of core's root into the strillix root.",
        doors: &[Door::Cli],
    ));
}

fn handle_migrate(inv: &Invocation) -> Outcome {
    let names = |paths: Vec<std::path::PathBuf>| paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>();
    if inv.flag_present("check") {
        let due = names(pending());
        let message = match due.len() {
            0 => "nothing is left to move (--check: nothing moved)".to_string(),
            n => format!("{n} old path(s) are due (--check: nothing moved)"),
        };
        return Outcome::ok("migrate", message).with_data(json!({ "check": true, "due": due }));
    }
    let stayed = names(migrate());
    let data = json!({ "check": false, "stayed": stayed });
    if stayed.is_empty() {
        return Outcome::ok("migrate", "Strillix's trees are in the strillix root").with_data(data);
    }
    Outcome::error("migrate", format!("{} old path(s) stay where they are", stayed.len())).with_data(data)
}
