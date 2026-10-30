//! `job`: local jobs on systemd user timers (app-plan-v21 §7.2). Thin
//! registrations over `crate::graph::job_*`; the handlers, the unit texts and
//! the record live in `graph/job.rs`.

use crate::graph::{job_add, job_list, job_remove, job_runs};
use aoide_protocol::registry::{arg, cmd, flag, Registry};

pub fn register(r: &mut Registry) {
    r.insert(cmd!(
        path: ["job", "add"],
        summary: "Add a local job: a program the systemd user manager runs on a calendar. Its argv is recorded; its two unit files are written under ~/.config/systemd/user. Runs only while the user manager runs.",
        args: [
            arg!("argv0", "string", true, "Absolute path of the program. Not looked up on PATH; it must exist and be executable now."),
            arg!("arg", "string", false, "Each further argument, passed to the program verbatim."),
        ],
        flags: [
            flag!("name", "string", "Job name: lowercase letters, digits and dashes, up to 48. Unique.", required: true),
            flag!("on", "string", "systemd calendar expression, as `systemd-analyze calendar` reads it, e.g. daily.", required: true),
            flag!("cwd", "string", "Absolute working directory for each run. Omitted: where systemd starts a user service."),
        ],
        gated: false,
        implemented: true,
        handler: job_add,
        examples: [
            "job add --name=nightly --on=daily -- /usr/bin/true",
        ],
        brief: "Add a local job on a systemd user timer.",
        approves: true, launches: true,
        kind: "job", verb: "act",
    ));
    r.insert(cmd!(
        path: ["job", "remove"],
        summary: "Remove a local job: disable its timer, delete the unit files aoide wrote, drop its record. A unit already gone is skipped. Refuses when a unit was edited, and deletes nothing then.",
        args: [],
        flags: [
            flag!("name", "string", "Name of the job to remove.", required: true),
        ],
        gated: false,
        implemented: true,
        handler: job_remove,
        examples: [
            "job remove --name=nightly",
        ],
        brief: "Remove a local job and its units.",
        approves: true,
        kind: "job", verb: "act",
    ));
    r.insert(cmd!(
        path: ["job", "list"],
        summary: "List local jobs with a status each: ok, inactive (timer not active), changed (a unit was edited), missing (a unit is gone), stale (argv0 is gone or not executable), or foreign (an aoide-job unit aoide did not write).",
        args: [],
        flags: [],
        gated: false,
        implemented: true,
        handler: job_list,
        examples: [
            "job list",
            "job list --json",
        ],
        brief: "List local jobs and their status.",
        kind: "job", verb: "list",
    ));
    r.insert(cmd!(
        path: ["job", "runs"],
        summary: "Show the runs of local jobs, read from the user journal: one row per run that ended completed, parked, failed or refused.",
        args: [],
        flags: [
            flag!("name", "string", "Only the runs of this job. Omitted: every local job's runs."),
        ],
        gated: false,
        implemented: true,
        handler: job_runs,
        examples: [
            "job runs",
            "job runs --name=nightly --json",
        ],
        brief: "Show the runs of local jobs from the journal.",
        kind: "job", verb: "list",
    ));
}
