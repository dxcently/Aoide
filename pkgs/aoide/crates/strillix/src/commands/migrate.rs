//! `strx migrate` — the rice state's move, run by hand. The strillix binary
//! moves it on its own only under `STRILLIX_MIGRATE=1` (the home activation
//! sets it); this verb is the same move for an operator, and `--check` is the
//! read-only report of what a run would do. Storage's `rice_status` and
//! `migrate_rice_root` own the logic; this file only narrates it.

use crate::dispatch::Invocation;
use crate::output::Outcome;
use crate::registry::{cmd, flag, Registry};
use aoide_protocol::Door;
use aoide_storage::fs::{migrate_rice_root, rice_status, RiceStatus};
use serde_json::{json, Value};

pub fn register(r: &mut Registry) {
    r.insert(cmd!(
        path: ["migrate"],
        summary: "Move the old rice state into the strillix root: the rice tree that sat under $AOIDE_ROOT/song, the old lyra state, and the file renames inside it. A rice state held by both roots is refused and nothing moves. --check reports the state and moves nothing.",
        args: [],
        flags: [flag!("check", "bool", "Report the rice state and what a run would move; move nothing.")],
        gated: false,
        implemented: true,
        handler: handle_migrate,
        examples: ["migrate --check", "migrate --json"],
        brief: "Move the old rice state into the strillix root.",
        doors: &[Door::Cli],
    ));
}

fn handle_migrate(inv: &Invocation) -> Outcome {
    let check = inv.flag_present("check");
    let before = rice_status();
    if !check && before != RiceStatus::Settled {
        if let Err(e) = migrate_rice_root() {
            return Outcome::error("migrate", format!("the rice state did not move: {e}"));
        }
    }
    let after = rice_status();
    let message = if check {
        format!("rice state is {} (--check: nothing moved)", label(&before))
    } else {
        format!("rice state was {}, is now {}", label(&before), label(&after))
    };
    Outcome::ok("migrate", message).with_data(json!({
        "check": check,
        "before": status(&before),
        "after": status(&after),
    }))
}

fn label(s: &RiceStatus) -> &'static str {
    match s {
        RiceStatus::Settled => "settled",
        RiceStatus::Due(_) => "due",
        RiceStatus::Both(_) => "held by both roots",
    }
}

fn status(s: &RiceStatus) -> Value {
    let paths = |p: &[std::path::PathBuf]| p.iter().map(|x| x.display().to_string()).collect::<Vec<_>>();
    match s {
        RiceStatus::Settled => json!({ "state": "settled", "paths": [] }),
        RiceStatus::Due(p) => json!({ "state": "due", "paths": paths(p) }),
        RiceStatus::Both(p) => json!({ "state": "both", "paths": paths(p) }),
    }
}
