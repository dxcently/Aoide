//! `aoide job`: local jobs on systemd user timers (app-plan-v21 §7.2).
//!
//! A job is two unit files under `$XDG_CONFIG_HOME/systemd/user/`, named
//! `aoide-job-<name>.timer` and `aoide-job-<name>.service`, plus one record in
//! `state/stage/jobs.json` holding both files' sha256. A unit whose bytes no
//! longer hash to its record was edited by someone else: `list` says
//! `changed`, and `remove` never deletes it. A unit that is gone is
//! `missing`, and `remove` skips it.

use super::common::refuse_unapproving_door;
use super::spawn::shell_quote;
use super::{load_stage, visible, write_stage};
use aoide_protocol::output::{io_cause, Fix, Kind, Outcome, Status};
use aoide_protocol::untrusted::is_unsafe;
use aoide_protocol::Invocation;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command as Process;

const UNIT_PREFIX: &str = "aoide-job-";
const NAME_MAX: usize = 48;
/// The statuses the workflow door writes, one JSON line per run (exit 0 to 3,
/// `cli/src/workflow.rs`).
const RUN_STATUSES: [&str; 4] = ["completed", "parked", "failed", "refused"];
const ADD_USAGE: &str = "aoide job add --name=<name> --on=<calendar> [--cwd=<dir>] -- <argv0> [<arg>...]";
const NOT_LINUX: &str = "local jobs are systemd user timers, and this build is not running on Linux";
const NO_JOBS_HERE: &str = "aoide keeps no local jobs on this OS";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Host {
    Linux,
    Other,
}

const HOST: Host = if cfg!(target_os = "linux") { Host::Linux } else { Host::Other };

/// One job as `state/stage/jobs.json` holds it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub name: String,
    pub calendar: String,
    pub argv: Vec<String>,
    pub cwd: Option<String>,
    pub timer_sha256: String,
    pub service_sha256: String,
    pub added_at: String,
    pub added_by: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct JobsFile {
    #[serde(default)]
    jobs: Vec<Job>,
}

/// One run row: the job, the entry's time, and the workflow door's own keys.
#[derive(Debug, Clone, Serialize)]
pub struct Run {
    pub job: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub question: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// `state/stage/jobs.json`: the record of every job aoide wrote.
pub fn jobs_path() -> PathBuf {
    aoide_storage::fs::conducting_stage_dir().join("jobs.json")
}

pub fn job_add(inv: &Invocation) -> Outcome {
    add(inv, HOST)
}

pub fn job_remove(inv: &Invocation) -> Outcome {
    remove(inv, HOST)
}

pub fn job_list(inv: &Invocation) -> Outcome {
    list(inv, HOST)
}

pub fn job_runs(inv: &Invocation) -> Outcome {
    runs(inv, HOST)
}

fn add(inv: &Invocation, host: Host) -> Outcome {
    const CMD: &str = "job.add";
    if let Some(out) = refuse_unapproving_door(
        inv,
        CMD,
        "`aoide job add` is not open to this door".into(),
        "adding a local job needs a person's approval, and this door cannot give one",
        Fix::Run("aoide job add --help".into()),
    ) {
        return out;
    }
    audited(inv, CMD, add_checked(inv, host))
}

fn add_checked(inv: &Invocation, host: Host) -> Outcome {
    const CMD: &str = "job.add";
    if host != Host::Linux {
        let fix = if inv.args.is_empty() { ADD_USAGE.to_string() } else { words(&inv.args) };
        return unsupported(CMD, Fix::Run(fix));
    }
    let name = flag(inv, "name");
    let on = flag(inv, "on");
    let argv = inv.args.as_slice();
    let cwd = inv.flags.get("cwd").map(String::as_str);
    if !valid_name(&name) {
        return usage(CMD, format!("`--name={}` is not a job name", visible(&name)), "a job name is 1 to 48 lowercase letters, digits or dashes: it names the unit files", ADD_USAGE);
    }
    if on.is_empty() || has_control(&on) {
        return usage(CMD, "`--on` is missing or holds a control character", "it is one systemd calendar expression, on one line", ADD_USAGE);
    }
    let Some(argv0) = argv.first() else {
        return usage(CMD, "`aoide job add` needs <argv0>", "the program to run follows `--`", ADD_USAGE);
    };
    if argv.iter().any(|arg| has_control(arg)) {
        return usage(CMD, "an argument holds a control character", "a control character would end the unit's ExecStart= line", ADD_USAGE);
    }
    if !Path::new(argv0).is_absolute() {
        return usage(CMD, format!("`{}` is not an absolute path", visible(argv0)), "systemd runs the program by this path: a bare name is never looked up on PATH", ADD_USAGE);
    }
    if !executable(Path::new(argv0)) {
        return Outcome::refuse(CMD, Kind::Refused, format!("`{}` is not an executable file", visible(argv0)), "argv0 must exist and be executable now", Fix::Run(ADD_USAGE.into()));
    }
    if let Some(cwd) = cwd {
        if let Err(out) = check_cwd(CMD, cwd) {
            return out;
        }
    }
    let root = aoide_storage::fs::root();
    if has_control(&root.display().to_string()) {
        return Outcome::refuse(CMD, Kind::Refused, "AOIDE_ROOT holds a control character", "it is written into every unit's Environment= line", Fix::None("set AOIDE_ROOT to a plain absolute path"));
    }
    let req = Request {
        name: &name,
        on: &on,
        argv,
        cwd,
        root: root.as_path(),
        added_by: format!("{:?}", inv.door).to_lowercase(),
    };
    aoide_storage::fs::with_stage_lock(|| add_world(CMD, &req))
}

/// A validated `add`, ready for the world: the probe, the calendar check, the
/// writes. Held under the stage lock.
struct Request<'a> {
    name: &'a str,
    on: &'a str,
    argv: &'a [String],
    cwd: Option<&'a str>,
    root: &'a Path,
    added_by: String,
}

fn add_world(cmd: &str, req: &Request) -> Outcome {
    if let Err(tool) = systemctl(&["show", "--property=Version"]) {
        return no_user_manager(cmd, tool);
    }
    match run_tool("systemd-analyze", &["calendar", req.on]) {
        Ok(_) => {}
        Err(Tool::Missing(detail)) => {
            return Outcome::refuse(cmd, Kind::Refused, "systemd-analyze is not on PATH", "it checks the calendar before a job is written", Fix::None("install systemd's tools, then retry")).with_detail(detail);
        }
        Err(Tool::Failed(text)) => {
            return Outcome::refuse(
                cmd,
                Kind::Usage,
                format!("`--on={}` is not a calendar systemd accepts", visible(req.on)),
                format!("systemd-analyze rejects it: {}", visible(&first_line(&text))),
                Fix::Run(format!("systemd-analyze calendar {}", shell_quote(req.on))),
            )
            .with_detail(text);
        }
    }
    let mut file = match load_jobs(cmd) {
        Ok(file) => file,
        Err(out) => return out,
    };
    if file.jobs.iter().any(|job| job.name == req.name) {
        return Outcome::refuse(cmd, Kind::Refused, format!("a job named `{}` already exists", req.name), "job names are unique: each one names a unit pair", Fix::Run(format!("aoide job remove --name={}", req.name)));
    }
    let dir = unit_dir();
    if let Err(out) = prepare_unit_dir(cmd, &dir) {
        return out;
    }
    let (timer_unit, service_unit) = unit_names(req.name);
    for unit in [&timer_unit, &service_unit] {
        if let Some(out) = slot_taken(cmd, &dir.join(unit)) {
            return out;
        }
    }
    let (timer_text, service_text) = unit_texts(req.name, req.on, req.argv, req.cwd, req.root);
    if let Err(e) = write_units(&dir, req.name, &timer_text, &service_text) {
        return io_failed(cmd, format!("could not write the units in {}", shown(&dir)), "write", &dir, &e);
    }
    let job = Job {
        name: req.name.to_string(),
        calendar: req.on.to_string(),
        argv: req.argv.to_vec(),
        cwd: req.cwd.map(str::to_string),
        timer_sha256: digest_bytes(timer_text.as_bytes()),
        service_sha256: digest_bytes(service_text.as_bytes()),
        added_at: aoide_storage::time::now_iso_utc(),
        added_by: req.added_by.clone(),
    };
    file.jobs.push(job.clone());
    if let Err(why) = write_stage(&jobs_path(), &file) {
        let _ = std::fs::remove_file(dir.join(&timer_unit));
        let _ = std::fs::remove_file(dir.join(&service_unit));
        return Outcome::refuse(cmd, Kind::Failed, "the job record could not be written", visible(&why), Fix::None("the units were removed again; check the stage directory, then retry"));
    }
    if let Err(tool) = systemctl(&["daemon-reload"]) {
        return recorded_but_not_loaded(cmd, req.name, "systemd did not reload its units", &tool);
    }
    if let Err(tool) = systemctl(&["enable", "--now", timer_unit.as_str()]) {
        return recorded_but_not_loaded(cmd, req.name, "its timer did not start", &tool);
    }
    Outcome::ok(
        cmd,
        format!(
            "added job {}: its timer runs {} while the user manager runs; aoide never enables linger",
            req.name,
            visible(req.on)
        ),
    )
    .with_data(json!({ "job": job }))
    .changed([timer_unit, service_unit, "jobs.json".to_string()])
}

fn remove(inv: &Invocation, host: Host) -> Outcome {
    const CMD: &str = "job.remove";
    if let Some(out) = refuse_unapproving_door(
        inv,
        CMD,
        "`aoide job remove` is not open to this door".into(),
        "removing a local job needs a person's approval, and this door cannot give one",
        Fix::Run("aoide job remove --help".into()),
    ) {
        return out;
    }
    audited(inv, CMD, remove_checked(inv, host))
}

fn remove_checked(inv: &Invocation, host: Host) -> Outcome {
    const CMD: &str = "job.remove";
    if host != Host::Linux {
        return unsupported(CMD, Fix::None(NO_JOBS_HERE));
    }
    let name = flag(inv, "name");
    if !valid_name(&name) {
        return usage(CMD, format!("`--name={}` is not a job name", visible(&name)), "a job name is 1 to 48 lowercase letters, digits or dashes", "aoide job remove --name=<name>");
    }
    aoide_storage::fs::with_stage_lock(|| remove_world(CMD, &name))
}

fn remove_world(cmd: &str, name: &str) -> Outcome {
    if let Err(tool) = systemctl(&["show", "--property=Version"]) {
        return no_user_manager(cmd, tool);
    }
    let mut file = match load_jobs(cmd) {
        Ok(file) => file,
        Err(out) => return out,
    };
    let Some(at) = file.jobs.iter().position(|job| job.name == name) else {
        return Outcome::refuse(cmd, Kind::Refused, format!("no job named `{name}`"), "aoide removes only the jobs it recorded", Fix::Run("aoide job list".into()));
    };
    let job = file.jobs[at].clone();
    let dir = unit_dir();
    let (timer_unit, service_unit) = unit_names(name);
    let units = [
        (dir.join(&timer_unit), job.timer_sha256.as_str()),
        (dir.join(&service_unit), job.service_sha256.as_str()),
    ];
    let states: Vec<Unit> = units.iter().map(|(path, sha)| unit_state(path, sha)).collect();
    if states.contains(&Unit::Changed) {
        return Outcome::refuse(
            cmd,
            Kind::Refused,
            format!("job `{name}` no longer matches its record"),
            "a unit was edited, or is a link; aoide deletes only the units it wrote, so the line below removes them by hand",
            Fix::Run(manual_removal(&timer_unit, &units, &states)),
        );
    }
    let (timer_state, service_state) = (states[0], states[1]);
    if timer_state == Unit::Matches {
        if let Err(tool) = systemctl(&["disable", "--now", timer_unit.as_str()]) {
            return Outcome::refuse(
                cmd,
                Kind::Failed,
                format!("job `{name}` kept: its timer did not stop"),
                tool_why(&tool),
                Fix::Run(format!("systemctl --user disable --now {timer_unit}")),
            )
            .with_detail(tool.raw());
        }
    }
    let mut changed = Vec::new();
    let mut gone = Vec::new();
    for ((path, _), (unit, state)) in units.iter().zip([(&timer_unit, timer_state), (&service_unit, service_state)]) {
        if state == Unit::Matches {
            if let Err(e) = std::fs::remove_file(path) {
                return io_failed(cmd, format!("could not delete {}", shown(path)), "delete", path, &e);
            }
            changed.push(unit.clone());
        } else {
            gone.push(unit.clone());
        }
    }
    if let Err(tool) = systemctl(&["daemon-reload"]) {
        return Outcome::refuse(
            cmd,
            Kind::Failed,
            format!("job `{name}` is not removed yet: its units are gone, but systemd did not reload"),
            tool_why(&tool),
            Fix::Run(format!("aoide job remove --name={name}")),
        )
        .with_detail(tool.raw());
    }
    file.jobs.remove(at);
    if let Err(why) = write_stage(&jobs_path(), &file) {
        return Outcome::refuse(
            cmd,
            Kind::Failed,
            "the units are gone, but the job record was not updated",
            visible(&why),
            Fix::Run(format!("aoide job remove --name={name}")),
        );
    }
    let mut message = format!("removed job {name}");
    if !gone.is_empty() {
        message.push_str(&format!("; already gone: {}", gone.join(", ")));
    }
    if timer_state == Unit::Missing {
        message.push_str(&format!(
            "; the user manager may still have the timer loaded, so stop it by hand: systemctl --user stop {timer_unit}"
        ));
    }
    changed.push("jobs.json".to_string());
    Outcome::ok(cmd, message).changed(changed)
}

fn list(_inv: &Invocation, host: Host) -> Outcome {
    const CMD: &str = "job.list";
    if host != Host::Linux {
        return unsupported(CMD, Fix::None(NO_JOBS_HERE));
    }
    if let Err(tool) = systemctl(&["show", "--property=Version"]) {
        return no_user_manager(CMD, tool);
    }
    let file = match load_jobs(CMD) {
        Ok(file) => file,
        Err(out) => return out,
    };
    let dir = unit_dir();
    let recorded: BTreeSet<&str> = file.jobs.iter().map(|job| job.name.as_str()).collect();
    let mut rows: Vec<Value> = file
        .jobs
        .iter()
        .map(|job| {
            json!({
                "name": job.name,
                "status": job_status(&dir, job),
                "calendar": job.calendar,
                "argv": job.argv,
                "cwd": job.cwd,
                "addedAt": job.added_at,
            })
        })
        .collect();
    rows.extend(foreign_names(&dir, &recorded).into_iter().map(|name| json!({ "name": name, "status": "foreign" })));
    let message = if rows.is_empty() {
        "no local jobs".to_string()
    } else {
        rows.iter().map(row_text).collect::<Vec<_>>().join("\n")
    };
    Outcome::ok(CMD, message).with_data(json!({ "jobs": rows }))
}

fn runs(inv: &Invocation, host: Host) -> Outcome {
    const CMD: &str = "job.runs";
    if host != Host::Linux {
        return unsupported(CMD, Fix::None(NO_JOBS_HERE));
    }
    let name = inv.flags.get("name").filter(|n| !n.is_empty());
    if let Some(name) = name {
        if !valid_name(name) {
            return usage(CMD, format!("`--name={}` is not a job name", visible(name)), "a job name is 1 to 48 lowercase letters, digits or dashes", "aoide job runs --name=<name>");
        }
    }
    if let Err(tool) = systemctl(&["show", "--property=Version"]) {
        return no_user_manager(CMD, tool);
    }
    let unit = match name {
        Some(name) => format!("{UNIT_PREFIX}{name}.service"),
        None => format!("{UNIT_PREFIX}*.service"),
    };
    let journal = match run_tool("journalctl", &["--user", "-u", unit.as_str(), "-o", "json", "--no-pager"]) {
        Ok(text) => text,
        Err(Tool::Missing(detail)) => {
            return Outcome::refuse(CMD, Kind::Refused, "journalctl is not on PATH", "the runs are read from the user journal", Fix::None("install systemd's tools, then retry")).with_detail(detail);
        }
        Err(Tool::Failed(text)) => {
            return Outcome::refuse(CMD, Kind::Failed, "the user journal did not answer", visible(&first_line(&text)), Fix::Run(format!("journalctl --user -u {unit} --no-pager -n 5"))).with_detail(text);
        }
    };
    let rows = run_lines(&journal, name.map(String::as_str));
    let message = if rows.is_empty() {
        "no local job runs in the user journal".to_string()
    } else {
        rows.iter().map(run_text).collect::<Vec<_>>().join("\n")
    };
    Outcome::ok(CMD, message).with_data(json!({ "runs": rows }))
}

/// One run per journal entry whose MESSAGE is a JSON object with a known
/// `status`. The job is `name`, or else the entry's own unit.
fn run_lines(text: &str, name: Option<&str>) -> Vec<Run> {
    text.lines()
        .filter_map(|line| {
            let entry: Value = serde_json::from_str(line).ok()?;
            let message: Value = serde_json::from_str(entry.get("MESSAGE")?.as_str()?).ok()?;
            let status = message.get("status")?.as_str()?;
            if !RUN_STATUSES.contains(&status) {
                return None;
            }
            let job = match name {
                Some(job) => job.to_string(),
                None => entry
                    .get("_SYSTEMD_USER_UNIT")?
                    .as_str()?
                    .strip_prefix(UNIT_PREFIX)?
                    .strip_suffix(".service")?
                    .to_string(),
            };
            let text = |key: &str| message.get(key).and_then(Value::as_str).map(str::to_string);
            Some(Run {
                job,
                time: entry
                    .get("__REALTIME_TIMESTAMP")
                    .and_then(Value::as_str)
                    .and_then(|us| us.parse::<i64>().ok())
                    .map(|us| aoide_storage::time::iso_utc_from_epoch(us / 1_000_000)),
                status: status.to_string(),
                session: text("session"),
                run: message.get("run").and_then(Value::as_u64),
                report: message.get("report").cloned(),
                question: text("question"),
                reason: text("reason"),
                error: text("error"),
            })
        })
        .collect()
}

fn run_text(run: &Run) -> String {
    let detail = run
        .question
        .as_deref()
        .or(run.reason.as_deref())
        .or(run.error.as_deref())
        .or(run.session.as_deref())
        .unwrap_or("");
    visible(&format!("{} {} {} {}", run.time.as_deref().unwrap_or("-"), run.job, run.status, detail))
}

/// The unit's own state against its record: `Missing` (no file), `Changed`
/// (not a regular file, or its bytes hash to something else), `Matches`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unit {
    Matches,
    Changed,
    Missing,
}

fn unit_state(path: &Path, recorded: &str) -> Unit {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Unit::Missing,
        Ok(meta) if meta.is_file() => match std::fs::read(path) {
            Ok(bytes) if digest_bytes(&bytes) == recorded => Unit::Matches,
            _ => Unit::Changed,
        },
        _ => Unit::Changed,
    }
}

/// The first of `changed`, `missing`, `stale`, `inactive`, `ok` that holds.
fn job_status(dir: &Path, job: &Job) -> &'static str {
    let (timer_unit, service_unit) = unit_names(&job.name);
    let units = [
        unit_state(&dir.join(&timer_unit), &job.timer_sha256),
        unit_state(&dir.join(&service_unit), &job.service_sha256),
    ];
    if units.contains(&Unit::Missing) {
        "missing"
    } else if units.contains(&Unit::Changed) {
        "changed"
    } else if job.argv.first().is_none_or(|argv0| !executable(Path::new(argv0))) {
        "stale"
    } else if systemctl(&["is-active", timer_unit.as_str()]).is_err() {
        "inactive"
    } else {
        "ok"
    }
}

/// Every `aoide-job-*` unit name in the unit directory that no record names.
fn foreign_names(dir: &Path, recorded: &BTreeSet<&str>) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return names;
    };
    for entry in entries.flatten() {
        let file = entry.file_name().to_string_lossy().into_owned();
        let stem = file
            .strip_prefix(UNIT_PREFIX)
            .and_then(|rest| rest.strip_suffix(".timer").or_else(|| rest.strip_suffix(".service")));
        if let Some(stem) = stem {
            if !recorded.contains(stem) {
                names.insert(stem.to_string());
            }
        }
    }
    names
}

fn row_text(row: &Value) -> String {
    let name = row["name"].as_str().unwrap_or_default();
    let status = row["status"].as_str().unwrap_or_default();
    match (row["calendar"].as_str(), row["argv"][0].as_str()) {
        (Some(calendar), Some(argv0)) => visible(&format!("{name}  {status}  {calendar}  {argv0}")),
        _ => visible(&format!("{name}  {status}")),
    }
}

/// `aoide-job-<name>.timer` and `aoide-job-<name>.service`.
fn unit_names(name: &str) -> (String, String) {
    (format!("{UNIT_PREFIX}{name}.timer"), format!("{UNIT_PREFIX}{name}.service"))
}

/// The two unit files' exact text. `WorkingDirectory=` is unquoted with `%`
/// doubled; every other word is double-quoted (systemd.syntax(7)), and an
/// `ExecStart=` word also doubles `$` (systemd.service(5)).
fn unit_texts(name: &str, calendar: &str, argv: &[String], cwd: Option<&str>, root: &Path) -> (String, String) {
    let (_, service_unit) = unit_names(name);
    let timer = format!(
        "[Timer]\nOnCalendar={calendar}\nUnit={service_unit}\n\n[Install]\nWantedBy=timers.target\n"
    );
    let mut service = String::from("[Service]\nType=oneshot\n");
    if let Some(cwd) = cwd {
        service.push_str(&format!("WorkingDirectory={}\n", cwd.replace('%', "%%")));
    }
    service.push_str(&format!("Environment={}\n", quote_word(&format!("AOIDE_ROOT={}", root.display()))));
    let words: Vec<String> = argv.iter().map(|arg| quote_word(arg).replace('$', "$$")).collect();
    service.push_str(&format!("ExecStart={}\n", words.join(" ")));
    (timer, service)
}

/// One systemd double-quoted word: `\`, `"` and `%` escaped.
fn quote_word(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\"").replace('%', "%%"))
}

/// Writes the timer, then the service; a failed service write takes the timer
/// back out, so a job is never half on disk.
fn write_units(dir: &Path, name: &str, timer: &str, service: &str) -> std::io::Result<()> {
    let (timer_unit, service_unit) = unit_names(name);
    let timer_path = dir.join(&timer_unit);
    aoide_storage::fs::atomic_write(&timer_path, timer)?;
    if let Err(e) = aoide_storage::fs::atomic_write(&dir.join(&service_unit), service) {
        let _ = std::fs::remove_file(&timer_path);
        return Err(e);
    }
    Ok(())
}

fn unit_dir() -> PathBuf {
    let config = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(dir) if Path::new(&dir).is_absolute() => PathBuf::from(dir),
        _ => aoide_protocol::aoide_home().join(".config"),
    };
    config.join("systemd").join("user")
}

/// The unit directory must be a real directory (or absent, and then created).
/// A link or a file in its place is refused, so nothing is written through it.
fn prepare_unit_dir(cmd: &str, dir: &Path) -> Result<(), Outcome> {
    match std::fs::symlink_metadata(dir) {
        Ok(meta) if meta.is_dir() => Ok(()),
        Ok(_) => Err(Outcome::refuse(
            cmd,
            Kind::Refused,
            format!("{} is not a directory", shown(dir)),
            "aoide writes its units into this directory, and refuses a link or a file in its place",
            Fix::None("replace it with a real directory, or remove it so aoide can create one"),
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => std::fs::create_dir_all(dir)
            .map_err(|e| io_failed(cmd, format!("could not create {}", shown(dir)), "create", dir, &e)),
        Err(e) => Err(io_failed(cmd, format!("could not read {}", shown(dir)), "read", dir, &e)),
    }
}

/// `Some` when anything is at `path`, a link included: aoide never writes over
/// a unit it did not write.
fn slot_taken(cmd: &str, path: &Path) -> Option<Outcome> {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        _ => Some(Outcome::refuse(
            cmd,
            Kind::Refused,
            format!("{} is already there", shown(path)),
            "aoide did not write it, or it is a link; aoide never writes over a unit it did not write",
            Fix::None("move it aside by hand, then retry"),
        )),
    }
}

fn load_jobs(cmd: &str) -> Result<JobsFile, Outcome> {
    load_stage::<JobsFile>(&jobs_path()).map_err(|why| {
        Outcome::refuse(
            cmd,
            Kind::Failed,
            "the job record cannot be read",
            visible(&why),
            Fix::None("aoide does not rewrite a record it cannot read: repair or move it by hand"),
        )
    })
}

/// The check on `--cwd`, each refusal taught (rulings 71, 74, 79).
fn check_cwd(cmd: &str, cwd: &str) -> Result<(), Outcome> {
    let refuse = |what: &str, why: &str| {
        Err(Outcome::refuse(cmd, Kind::Usage, what, why, Fix::Set("--cwd=<absolute directory>".into())))
    };
    if !Path::new(cwd).is_absolute() {
        return refuse(
            "`--cwd` is not an absolute path",
            "systemd would start the run in whatever directory it happens to read, not this one",
        );
    }
    if let Some(c) = cwd.chars().find(|c| is_unsafe(*c)) {
        return refuse(
            &format!("`--cwd` holds {}", visible(&c.to_string())),
            "a control, format or invisible character in a path is not shown to a person as it is",
        );
    }
    if cwd.ends_with(char::is_whitespace) {
        return refuse(
            "`--cwd` ends in whitespace",
            "systemd trims trailing whitespace from a unit value, so the run would start somewhere else",
        );
    }
    if cwd.contains('\\') {
        return refuse(
            "`--cwd` contains a backslash",
            "systemd may read a backslash in a unit value as an escape, and aoide does not verify that it does not here",
        );
    }
    Ok(())
}

/// The record is written before systemd is asked, so a timer that does not load
/// leaves a recorded job that `remove` can clear.
fn recorded_but_not_loaded(cmd: &str, name: &str, what: &str, tool: &Tool) -> Outcome {
    Outcome::refuse(
        cmd,
        Kind::Failed,
        format!("job `{name}` is recorded, but {what}"),
        tool_why(tool),
        Fix::Run(format!("aoide job remove --name={name}")),
    )
    .with_detail(tool.raw())
}

fn manual_removal(timer_unit: &str, units: &[(PathBuf, &str)], states: &[Unit]) -> String {
    let mut steps = vec![format!("systemctl --user disable --now {timer_unit}")];
    let present: Vec<String> = units
        .iter()
        .zip(states)
        .filter(|(_, state)| **state != Unit::Missing)
        .map(|((path, _), _)| shell_quote(&path.display().to_string()))
        .collect();
    if !present.is_empty() {
        steps.push(format!("rm -- {}", present.join(" ")));
    }
    steps.push("systemctl --user daemon-reload".to_string());
    steps.join("; ")
}

fn audited(inv: &Invocation, cmd: &str, out: Outcome) -> Outcome {
    if out.status != Status::Usage {
        let status = out
            .data
            .as_ref()
            .and_then(|data| data.pointer("/refusal/kind"))
            .and_then(Value::as_str)
            .unwrap_or("ok")
            .to_string();
        let _ = aoide_protocol::append_audit(
            &aoide_protocol::audit_log_path(inv),
            &aoide_protocol::AuditRecord {
                ts: aoide_protocol::audit::now_secs(),
                door: inv.door,
                class: aoide_protocol::EventClass::Audit,
                command: cmd.to_string(),
                status,
                message: out.message.clone(),
                untrusted_data: None,
            },
        );
    }
    out
}

/// A systemd tool that gave no answer: not on PATH, or it ran and failed.
enum Tool {
    Missing(String),
    Failed(String),
}

impl Tool {
    fn raw(&self) -> String {
        match self {
            Tool::Missing(detail) | Tool::Failed(detail) => detail.clone(),
        }
    }
}

fn run_tool(program: &str, args: &[&str]) -> Result<String, Tool> {
    match Process::new(program).args(args).output() {
        Err(e) => Err(Tool::Missing(format!("{program}: {e}"))),
        Ok(out) if out.status.success() => Ok(String::from_utf8_lossy(&out.stdout).into_owned()),
        Ok(out) => Err(Tool::Failed(String::from_utf8_lossy(&out.stderr).trim().to_string())),
    }
}

fn systemctl(args: &[&str]) -> Result<String, Tool> {
    let mut all = vec!["--user"];
    all.extend_from_slice(args);
    run_tool("systemctl", &all)
}

fn tool_why(tool: &Tool) -> String {
    match tool {
        Tool::Missing(_) => "systemctl is not on PATH".to_string(),
        Tool::Failed(text) => format!("systemctl --user failed: {}", visible(&first_line(text))),
    }
}

fn no_user_manager(cmd: &str, tool: Tool) -> Outcome {
    let why = match &tool {
        Tool::Missing(_) => "systemctl is not on PATH, so no user manager can be reached",
        Tool::Failed(_) => "systemctl --user show failed, so no user manager answers",
    };
    Outcome::refuse(
        cmd,
        Kind::Refused,
        "no systemd user manager answers",
        format!("{why}; jobs fire only while the user manager runs"),
        Fix::Run("systemctl --user status".into()),
    )
    .with_detail(tool.raw())
}

fn unsupported(cmd: &str, fix: Fix) -> Outcome {
    Outcome::refuse(cmd, Kind::Refused, format!("`aoide {}` runs only on Linux", cmd.replace('.', " ")), NOT_LINUX, fix)
}

fn usage(cmd: &str, what: impl Into<String>, why: &str, fix_line: &str) -> Outcome {
    Outcome::refuse(cmd, Kind::Usage, what, why, Fix::Run(fix_line.to_string()))
}

fn io_failed(cmd: &str, what: String, op: &str, path: &Path, e: &std::io::Error) -> Outcome {
    let cause = io_cause(op, path, e);
    Outcome::refuse(
        cmd,
        Kind::Failed,
        what,
        cause.why,
        Fix::Run(format!("ls -ld -- {}", shell_quote(&path.display().to_string()))),
    )
    .with_detail(cause.detail)
}

fn valid_name(name: &str) -> bool {
    (1..=NAME_MAX).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn flag(inv: &Invocation, name: &str) -> String {
    inv.flags.get(name).cloned().unwrap_or_default()
}

fn has_control(text: &str) -> bool {
    text.chars().any(char::is_control)
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or("").trim().to_string()
}

fn shown(path: &Path) -> String {
    visible(&path.display().to_string())
}

fn words(argv: &[String]) -> String {
    argv.iter().map(|arg| shell_quote(arg)).collect::<Vec<_>>().join(" ")
}

fn digest_bytes(bytes: &[u8]) -> String {
    aoide_storage::seal::hex_encode(&aoide_storage::seal::sha256(bytes))
}

#[cfg(unix)]
fn executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn executable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_protocol::registry::Registry;
    use aoide_protocol::Door;

    fn invocation(path: &[&str], args: &[&str], flags: &[(&str, &str)], door: Door) -> Invocation {
        let mut inv = aoide_test_support::inv(path, args);
        inv.door = door;
        inv.flags.extend(flags.iter().map(|(k, v)| (k.to_string(), v.to_string())));
        inv
    }

    /// The unit texts' exact bytes. An argument holding `"`, `\`, `%`, `$`
    /// and a space comes out quoted and escaped; a `%` in the working
    /// directory is doubled and left unquoted.
    #[test]
    fn unit_texts_quote_each_word_the_systemd_way() {
        let argv = ["/bin/t".to_string(), r#"a"b\c%d$e f"#.to_string()];
        let (timer, service) = unit_texts("nightly", "daily", &argv, Some("/w/a%b c"), Path::new("/r"));
        assert_eq!(
            timer,
            "[Timer]\nOnCalendar=daily\nUnit=aoide-job-nightly.service\n\n[Install]\nWantedBy=timers.target\n"
        );
        assert_eq!(
            service,
            "[Service]\nType=oneshot\nWorkingDirectory=/w/a%%b c\nEnvironment=\"AOIDE_ROOT=/r\"\nExecStart=\"/bin/t\" \"a\\\"b\\\\c%%d$$e f\"\n"
        );
    }

    #[test]
    fn a_job_without_a_cwd_has_no_working_directory_line() {
        let argv = ["/bin/t".to_string()];
        let (_, service) = unit_texts("nightly", "daily", &argv, None, Path::new("/r"));
        assert!(!service.contains("WorkingDirectory"), "{service}");
    }

    /// The four verbs open to the terminal and the daemon hop, and to no
    /// other door. Each is checked with a valid invocation, so a refusal can
    /// only be the door's.
    #[test]
    fn the_job_verbs_open_to_the_cli_and_daemon_doors_only() {
        let mut r = Registry::new();
        crate::commands::job::register(&mut r);
        let verbs: [(&[&str], &[&str], &[(&str, &str)]); 4] = [
            (&["job", "add"], &["/bin/true"], &[("name", "nightly"), ("on", "daily")]),
            (&["job", "remove"], &[], &[("name", "nightly")]),
            (&["job", "list"], &[], &[]),
            (&["job", "runs"], &[], &[]),
        ];
        for (path, args, flags) in verbs {
            let owned: Vec<String> = path.iter().map(|s| s.to_string()).collect();
            let cmd = r.get(&owned).unwrap_or_else(|| panic!("{path:?} is not registered"));
            for door in [Door::Cli, Door::Daemon] {
                let mut inv = invocation(path, args, flags, door);
                assert!(cmd.check("aoide", &mut inv).is_ok(), "{} must open to {door:?}", cmd.dotted());
            }
            for door in [Door::Mcp, Door::A2a] {
                let mut inv = invocation(path, args, flags, door);
                assert!(cmd.check("aoide", &mut inv).is_err(), "{} must refuse {door:?}", cmd.dotted());
            }
        }
    }

    /// A journal fixture of the four run statuses, plus the lines that are not
    /// runs: a plain message, an object with no status, an unknown status, and
    /// a binary message. Only the four are rows.
    #[test]
    fn run_lines_turn_a_journal_fixture_into_typed_rows_and_skip_the_rest() {
        let fixture = [
            r#"{"MESSAGE":"{\"status\":\"completed\",\"session\":\"/s/1\",\"run\":1,\"report\":{\"ok\":true}}","__REALTIME_TIMESTAMP":"1700000000000000","_SYSTEMD_USER_UNIT":"aoide-job-nightly.service"}"#,
            r#"{"MESSAGE":"{\"status\":\"parked\",\"session\":\"/s/2\",\"run\":2,\"question\":\"ship it?\"}","__REALTIME_TIMESTAMP":"1700000060000000","_SYSTEMD_USER_UNIT":"aoide-job-nightly.service"}"#,
            r#"{"MESSAGE":"{\"status\":\"failed\",\"session\":\"/s/3\",\"run\":3,\"reason\":\"no budget\"}","__REALTIME_TIMESTAMP":"1700000120000000","_SYSTEMD_USER_UNIT":"aoide-job-nightly.service"}"#,
            r#"{"MESSAGE":"{\"status\":\"refused\",\"error\":\"plugin not vouched\"}","__REALTIME_TIMESTAMP":"1700000180000000","_SYSTEMD_USER_UNIT":"aoide-job-nightly.service"}"#,
            r#"{"MESSAGE":"Started aoide-job-nightly.service.","__REALTIME_TIMESTAMP":"1700000190000000","_SYSTEMD_USER_UNIT":"aoide-job-nightly.service"}"#,
            r#"{"MESSAGE":"{\"session\":\"/s/9\"}","__REALTIME_TIMESTAMP":"1700000200000000","_SYSTEMD_USER_UNIT":"aoide-job-nightly.service"}"#,
            r#"{"MESSAGE":"{\"status\":\"paused\"}","__REALTIME_TIMESTAMP":"1700000210000000","_SYSTEMD_USER_UNIT":"aoide-job-nightly.service"}"#,
            r#"{"MESSAGE":[104,105],"__REALTIME_TIMESTAMP":"1700000220000000","_SYSTEMD_USER_UNIT":"aoide-job-nightly.service"}"#,
        ]
        .join("\n");
        let rows = run_lines(&fixture, None);
        let got: Vec<(&str, &str)> = rows.iter().map(|r| (r.job.as_str(), r.status.as_str())).collect();
        assert_eq!(got, [("nightly", "completed"), ("nightly", "parked"), ("nightly", "failed"), ("nightly", "refused")]);
        let first = serde_json::to_value(&rows[0]).unwrap();
        assert_eq!(first["time"], "2023-11-14T22:13:20Z");
        assert_eq!(first["session"], "/s/1");
        assert_eq!(first["run"], 1);
        assert_eq!(first["report"], serde_json::json!({ "ok": true }));
        assert_eq!(rows[1].question.as_deref(), Some("ship it?"));
        assert_eq!(rows[2].reason.as_deref(), Some("no budget"));
        assert_eq!(rows[3].error.as_deref(), Some("plugin not vouched"));
    }

    #[test]
    fn a_name_is_lowercase_letters_digits_and_dashes_up_to_48() {
        let long = "x".repeat(49);
        assert!(valid_name("nightly-2"));
        assert!(valid_name(&"x".repeat(48)));
        for bad in ["", "Bad_Name", "a.b", "a b", "a/b", long.as_str()] {
            assert!(!valid_name(bad), "{bad:?} must not be a job name");
        }
    }

    #[cfg(unix)]
    mod shimmed {
        use super::*;
        use aoide_test_support::{pin_roots, unique_tmp, EnvSaver};

        /// The three systemd tools, as recording shims. Builtins only, so a
        /// shim never needs another program: PATH is the shim directory and
        /// nothing else, so no real `systemctl` can run from a test.
        const SYSTEMCTL: &str = r#"#!/bin/sh
d=${0%/*}
printf '%s\n' "systemctl $*" >> "$d/calls.log"
verb=$2
[ -e "$d/fail-$verb" ] && { echo "Failed to connect to bus" >&2; exit 1; }
if [ "$verb" = is-active ]; then
  for unit; do :; done
  [ -e "$d/active-$unit" ] && exit 0
  exit 3
fi
exit 0
"#;
        const JOURNALCTL: &str = r#"#!/bin/sh
d=${0%/*}
printf '%s\n' "journalctl $*" >> "$d/calls.log"
while IFS= read -r line; do printf '%s\n' "$line"; done < "$d/journal.jsonl"
"#;
        const ANALYZE: &str = r#"#!/bin/sh
d=${0%/*}
printf '%s\n' "systemd-analyze $*" >> "$d/calls.log"
case "$2" in
  *BAD*) echo "Failed to parse calendar specification" >&2; exit 1 ;;
esac
exit 0
"#;

        /// One test's world: its own root (state, stage, log, config), and the
        /// shims as its whole PATH. The caller holds the env lock.
        struct World {
            root: PathBuf,
            shims: PathBuf,
            _roots: EnvSaver,
            _vars: EnvSaver,
        }

        impl World {
            fn new(tag: &str) -> World {
                let root = unique_tmp(tag);
                let shims = root.join("shims");
                std::fs::create_dir_all(&shims).unwrap();
                for (tool, body) in [("systemctl", SYSTEMCTL), ("journalctl", JOURNALCTL), ("systemd-analyze", ANALYZE)] {
                    let path = shims.join(tool);
                    std::fs::write(&path, body).unwrap();
                    set_executable(&path);
                }
                let roots = pin_roots(&root);
                let vars = EnvSaver::capture(&["PATH", "XDG_CONFIG_HOME"]);
                std::env::set_var("PATH", &shims);
                std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
                World { root, shims, _roots: roots, _vars: vars }
            }

            fn unit_dir(&self) -> PathBuf {
                self.root.join("config/systemd/user")
            }

            /// An executable absolute program under the test root.
            fn tool(&self, name: &str) -> String {
                let path = self.root.join("bin").join(name);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
                set_executable(&path);
                path.display().to_string()
            }

            fn calls(&self) -> Vec<String> {
                std::fs::read_to_string(self.shims.join("calls.log"))
                    .unwrap_or_default()
                    .lines()
                    .map(String::from)
                    .collect()
            }

            fn fail(&self, verb: &str) {
                std::fs::write(self.shims.join(format!("fail-{verb}")), "").unwrap();
            }

            fn active(&self, unit: &str) {
                std::fs::write(self.shims.join(format!("active-{unit}")), "").unwrap();
            }

            fn journal(&self, lines: &str) {
                std::fs::write(self.shims.join("journal.jsonl"), lines).unwrap();
            }

            fn record(&self) -> Value {
                let text = std::fs::read_to_string(self.root.join("state/stage/jobs.json")).unwrap_or_else(|_| "{}".into());
                serde_json::from_str(&text).unwrap()
            }

            fn audit(&self) -> Vec<String> {
                std::fs::read_to_string(self.root.join("log")).unwrap_or_default().lines().map(String::from).collect()
            }
        }

        impl Drop for World {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.root);
            }
        }

        fn set_executable(path: &Path) {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        fn add_inv(name: &str, on: &str, argv: &[&str]) -> Invocation {
            invocation(&["job", "add"], argv, &[("name", name), ("on", on)], Door::Cli)
        }

        fn digest(text: &str) -> String {
            aoide_storage::seal::hex_encode(&aoide_storage::seal::sha256(text.as_bytes()))
        }

        fn lock() -> std::sync::MutexGuard<'static, ()> {
            crate::env_lock().lock().unwrap_or_else(|e| e.into_inner())
        }

        #[test]
        fn a_host_that_is_not_linux_refuses_every_verb_and_writes_nothing() {
            let _guard = lock();
            let w = World::new("job-not-linux");
            let tool = w.tool("job-tool");
            let outs = [
                add(&add_inv("nightly", "daily", &[tool.as_str()]), Host::Other),
                remove(&invocation(&["job", "remove"], &[], &[("name", "nightly")], Door::Cli), Host::Other),
                list(&invocation(&["job", "list"], &[], &[], Door::Cli), Host::Other),
                runs(&invocation(&["job", "runs"], &[], &[], Door::Cli), Host::Other),
            ];
            for out in &outs {
                assert_eq!(out.status, Status::Error, "{}", out.message);
                assert!(out.data.as_ref().is_some_and(|d| d.get("refusal").is_some()), "a refusal is taught: {}", out.message);
            }
            assert!(!w.unit_dir().exists());
            assert!(w.calls().is_empty(), "no systemd tool may run: {:?}", w.calls());
        }

        #[test]
        fn a_failing_user_manager_probe_refuses_add_with_zero_files() {
            let _guard = lock();
            let w = World::new("job-probe");
            let tool = w.tool("job-tool");
            w.fail("show");
            let out = add(&add_inv("nightly", "daily", &[tool.as_str()]), Host::Linux);
            assert_eq!(out.status, Status::Error, "{}", out.message);
            assert!(!w.unit_dir().exists());
            assert!(w.record().get("jobs").is_none());
            assert_eq!(w.calls(), ["systemctl --user show --property=Version"]);
        }

        /// Each bad input is refused with nothing written and no timer enabled.
        #[test]
        fn add_refuses_each_bad_input_with_zero_files() {
            let _guard = lock();
            let w = World::new("job-bad-input");
            let tool = w.tool("job-tool");
            let plain = w.root.join("bin/plain").display().to_string();
            std::fs::write(&plain, "not a program").unwrap();
            let long = "x".repeat(49);
            let cases: Vec<(&str, &str, Vec<&str>, Vec<(&str, &str)>)> = vec![
                ("Bad_Name", "daily", vec![tool.as_str()], vec![]),
                ("a.b", "daily", vec![tool.as_str()], vec![]),
                (long.as_str(), "daily", vec![tool.as_str()], vec![]),
                ("ok", "daily", vec!["job-tool"], vec![]),
                ("ok", "daily", vec!["/nope/none"], vec![]),
                ("ok", "daily", vec![plain.as_str()], vec![]),
                ("ok", "daily", vec![tool.as_str(), "a\nb"], vec![]),
                ("ok", "BAD", vec![tool.as_str()], vec![]),
                ("ok", "daily", vec![tool.as_str()], vec![("cwd", "rel")]),
                ("ok", "daily", vec![tool.as_str()], vec![("cwd", "/a\tb")]),
                ("ok", "daily", vec![tool.as_str()], vec![("cwd", "/a\u{202e}b")]),
                ("ok", "daily", vec![tool.as_str()], vec![("cwd", "/a ")]),
                ("ok", "daily", vec![tool.as_str()], vec![("cwd", "/a\\b")]),
            ];
            for (name, on, argv, extra) in cases {
                let mut flags = vec![("name", name), ("on", on)];
                flags.extend(extra);
                let out = add(&invocation(&["job", "add"], &argv, &flags, Door::Cli), Host::Linux);
                assert_ne!(out.status, Status::Ok, "{name} {on} {argv:?} must be refused: {}", out.message);
            }
            assert!(!w.unit_dir().exists(), "no unit may be written: {:?}", w.calls());
            assert!(w.record().get("jobs").is_none());
            assert!(w.calls().iter().all(|c| !c.contains("enable") && !c.contains("daemon-reload")), "{:?}", w.calls());
        }

        #[test]
        fn add_writes_both_units_records_their_hashes_enables_the_timer_and_audits_once() {
            let _guard = lock();
            let w = World::new("job-add-ok");
            let tool = w.tool("job-tool");
            let out = add(&add_inv("nightly", "daily", &[tool.as_str(), "--quiet"]), Host::Linux);
            assert_eq!(out.status, Status::Ok, "{}", out.message);
            let timer_path = w.unit_dir().join("aoide-job-nightly.timer");
            let service_path = w.unit_dir().join("aoide-job-nightly.service");
            let timer = std::fs::read_to_string(&timer_path).unwrap();
            let service = std::fs::read_to_string(&service_path).unwrap();
            assert_eq!(timer, "[Timer]\nOnCalendar=daily\nUnit=aoide-job-nightly.service\n\n[Install]\nWantedBy=timers.target\n");
            assert!(service.contains(&format!("ExecStart=\"{tool}\" \"--quiet\"\n")), "{service}");
            let job = &w.record()["jobs"][0];
            assert_eq!(job["name"], "nightly");
            assert_eq!(job["timerSha256"], digest(&timer));
            assert_eq!(job["serviceSha256"], digest(&service));
            assert!(job["cwd"].is_null());
            assert_eq!(job["addedBy"], "cli");
            assert_eq!(
                w.calls(),
                [
                    "systemctl --user show --property=Version",
                    "systemd-analyze calendar daily",
                    "systemctl --user daemon-reload",
                    "systemctl --user enable --now aoide-job-nightly.timer",
                ]
            );
            let audit = w.audit();
            assert_eq!(audit.len(), 1, "{audit:?}");
            assert!(audit[0].contains("\"command\":\"job.add\""), "{audit:?}");
        }

        #[test]
        fn a_second_add_of_a_recorded_name_is_refused_and_enables_nothing_new() {
            let _guard = lock();
            let w = World::new("job-dup");
            let tool = w.tool("job-tool");
            assert_eq!(add(&add_inv("nightly", "daily", &[tool.as_str()]), Host::Linux).status, Status::Ok);
            let out = add(&add_inv("nightly", "daily", &[tool.as_str()]), Host::Linux);
            assert_eq!(out.status, Status::Error, "{}", out.message);
            assert_eq!(w.record()["jobs"].as_array().unwrap().len(), 1);
            assert_eq!(w.calls().iter().filter(|c| c.contains("enable --now")).count(), 1, "{:?}", w.calls());
        }

        #[test]
        fn list_reports_all_six_statuses_from_fixtures() {
            let _guard = lock();
            let w = World::new("job-list");
            let tool = w.tool("job-tool");
            let old_tool = w.tool("old-tool");
            for (name, argv0) in [
                ("ok-job", tool.as_str()),
                ("idle-job", tool.as_str()),
                ("edit-job", tool.as_str()),
                ("gone-job", tool.as_str()),
                ("old-job", old_tool.as_str()),
            ] {
                assert_eq!(add(&add_inv(name, "daily", &[argv0]), Host::Linux).status, Status::Ok, "{name}");
            }
            w.active("aoide-job-ok-job.timer");
            let edited = w.unit_dir().join("aoide-job-edit-job.timer");
            std::fs::write(&edited, format!("{}# edited\n", std::fs::read_to_string(&edited).unwrap())).unwrap();
            std::fs::remove_file(w.unit_dir().join("aoide-job-gone-job.service")).unwrap();
            std::fs::remove_file(&old_tool).unwrap();
            std::fs::write(w.unit_dir().join("aoide-job-alien.timer"), "[Timer]\n").unwrap();

            let out = list(&invocation(&["job", "list"], &[], &[], Door::Cli), Host::Linux);
            assert_eq!(out.status, Status::Ok, "{}", out.message);
            let rows = out.data.as_ref().unwrap()["jobs"].as_array().unwrap().clone();
            let got: BTreeMapStr = rows
                .iter()
                .map(|r| (r["name"].as_str().unwrap().to_string(), r["status"].as_str().unwrap().to_string()))
                .collect();
            let want: BTreeMapStr = [
                ("ok-job", "ok"),
                ("idle-job", "inactive"),
                ("edit-job", "changed"),
                ("gone-job", "missing"),
                ("old-job", "stale"),
                ("alien", "foreign"),
            ]
            .into_iter()
            .map(|(n, s)| (n.to_string(), s.to_string()))
            .collect();
            assert_eq!(got, want);
        }

        type BTreeMapStr = std::collections::BTreeMap<String, String>;

        #[test]
        fn remove_with_matching_units_leaves_no_trace_and_a_changed_unit_is_kept_and_a_foreign_unit_never_removed() {
            let _guard = lock();
            let w = World::new("job-remove");
            let tool = w.tool("job-tool");
            assert_eq!(add(&add_inv("rm-job", "daily", &[tool.as_str()]), Host::Linux).status, Status::Ok);
            let out = remove(&invocation(&["job", "remove"], &[], &[("name", "rm-job")], Door::Cli), Host::Linux);
            assert_eq!(out.status, Status::Ok, "{}", out.message);
            assert!(!w.unit_dir().join("aoide-job-rm-job.timer").exists());
            assert!(!w.unit_dir().join("aoide-job-rm-job.service").exists());
            assert!(w.record()["jobs"].as_array().unwrap().is_empty());
            assert!(w.calls().contains(&"systemctl --user disable --now aoide-job-rm-job.timer".to_string()), "{:?}", w.calls());

            assert_eq!(add(&add_inv("keep-job", "daily", &[tool.as_str()]), Host::Linux).status, Status::Ok);
            let timer = w.unit_dir().join("aoide-job-keep-job.timer");
            std::fs::write(&timer, "tampered\n").unwrap();
            let out = remove(&invocation(&["job", "remove"], &[], &[("name", "keep-job")], Door::Cli), Host::Linux);
            assert_eq!(out.status, Status::Error, "{}", out.message);
            assert!(timer.exists() && w.unit_dir().join("aoide-job-keep-job.service").exists());
            assert!(!w.calls().iter().any(|c| c.contains("disable --now aoide-job-keep-job")), "{:?}", w.calls());
            assert!(out.data.as_ref().is_some_and(|d| d.to_string().contains("rm --")), "{:?}", out.data);

            std::fs::write(w.unit_dir().join("aoide-job-alien.timer"), "[Timer]\n").unwrap();
            let out = remove(&invocation(&["job", "remove"], &[], &[("name", "alien")], Door::Cli), Host::Linux);
            assert_eq!(out.status, Status::Error, "{}", out.message);
            assert!(w.unit_dir().join("aoide-job-alien.timer").exists());
        }

        #[test]
        fn a_job_whose_units_are_both_gone_is_removed_and_its_name_can_be_added_again() {
            let _guard = lock();
            let w = World::new("job-both-gone");
            let tool = w.tool("job-tool");
            assert_eq!(add(&add_inv("gone-job", "daily", &[tool.as_str()]), Host::Linux).status, Status::Ok);
            std::fs::remove_file(w.unit_dir().join("aoide-job-gone-job.timer")).unwrap();
            std::fs::remove_file(w.unit_dir().join("aoide-job-gone-job.service")).unwrap();
            let before = w.calls().len();
            let out = remove(&invocation(&["job", "remove"], &[], &[("name", "gone-job")], Door::Cli), Host::Linux);
            assert_eq!(out.status, Status::Ok, "{}", out.message);
            assert!(out.message.contains("aoide-job-gone-job.timer"), "{}", out.message);
            assert!(out.message.contains("aoide-job-gone-job.service"), "{}", out.message);
            assert!(out.message.contains("systemctl --user stop aoide-job-gone-job.timer"), "{}", out.message);
            assert!(w.record()["jobs"].as_array().unwrap().is_empty());
            let after = w.calls()[before..].to_vec();
            assert!(!after.iter().any(|c| c.contains("aoide-job-gone-job")), "no call may name a missing unit: {:?}", after);
            assert_eq!(add(&add_inv("gone-job", "daily", &[tool.as_str()]), Host::Linux).status, Status::Ok);
        }

        #[test]
        fn a_job_whose_timer_is_gone_drops_its_service_and_prints_the_stop_line() {
            let _guard = lock();
            let w = World::new("job-timer-gone");
            let tool = w.tool("job-tool");
            assert_eq!(add(&add_inv("half-job", "daily", &[tool.as_str()]), Host::Linux).status, Status::Ok);
            std::fs::remove_file(w.unit_dir().join("aoide-job-half-job.timer")).unwrap();
            let before = w.calls().len();
            let out = remove(&invocation(&["job", "remove"], &[], &[("name", "half-job")], Door::Cli), Host::Linux);
            assert_eq!(out.status, Status::Ok, "{}", out.message);
            assert!(!w.unit_dir().join("aoide-job-half-job.service").exists());
            assert!(out.message.contains("systemctl --user stop aoide-job-half-job.timer"), "{}", out.message);
            assert!(w.record()["jobs"].as_array().unwrap().is_empty());
            let after = w.calls()[before..].to_vec();
            assert!(!after.iter().any(|c| c.contains("disable") || c.contains("aoide-job-half-job.timer")), "{:?}", after);
        }

        #[test]
        fn a_symlinked_unit_counts_as_changed_and_remove_refuses_it_and_deletes_nothing() {
            let _guard = lock();
            let w = World::new("job-symlink-remove");
            let tool = w.tool("job-tool");
            assert_eq!(add(&add_inv("linked-job", "daily", &[tool.as_str()]), Host::Linux).status, Status::Ok);
            let timer = w.unit_dir().join("aoide-job-linked-job.timer");
            let copy = w.root.join("copied.timer");
            std::fs::copy(&timer, &copy).unwrap();
            std::fs::remove_file(&timer).unwrap();
            std::os::unix::fs::symlink(&copy, &timer).unwrap();
            let out = remove(&invocation(&["job", "remove"], &[], &[("name", "linked-job")], Door::Cli), Host::Linux);
            assert_eq!(out.status, Status::Error, "{}", out.message);
            assert!(std::fs::symlink_metadata(&timer).unwrap().file_type().is_symlink());
            assert!(w.unit_dir().join("aoide-job-linked-job.service").exists());
            assert!(!w.calls().iter().any(|c| c.contains("disable --now aoide-job-linked-job")), "{:?}", w.calls());
            assert_eq!(w.record()["jobs"].as_array().unwrap().len(), 1);
        }

        #[test]
        fn runs_turns_a_journal_fixture_into_rows_and_shows_their_text_visibly() {
            let _guard = lock();
            let w = World::new("job-runs");
            w.journal(concat!(
                r#"{"MESSAGE":"{\"status\":\"parked\",\"session\":\"/s/2\",\"run\":2,\"question\":\"ship\\u001b[2J?\"}","__REALTIME_TIMESTAMP":"1700000060000000","_SYSTEMD_USER_UNIT":"aoide-job-nightly.service"}"#,
                "\n",
                r#"{"MESSAGE":"not a run","__REALTIME_TIMESTAMP":"1700000070000000","_SYSTEMD_USER_UNIT":"aoide-job-nightly.service"}"#,
                "\n",
            ));
            let out = runs(&invocation(&["job", "runs"], &[], &[], Door::Cli), Host::Linux);
            assert_eq!(out.status, Status::Ok, "{}", out.message);
            let rows = out.data.as_ref().unwrap()["runs"].as_array().unwrap().clone();
            assert_eq!(rows.len(), 1, "{rows:?}");
            assert_eq!(rows[0]["status"], "parked");
            assert_eq!(rows[0]["job"], "nightly");
            assert!(!out.message.contains('\u{1b}'), "a run's text is shown visibly, never raw: {:?}", out.message);
            assert_eq!(w.calls(), ["systemctl --user show --property=Version", "journalctl --user -u aoide-job-*.service -o json --no-pager"]);
            let _ = runs(&invocation(&["job", "runs"], &[], &[("name", "nightly")], Door::Cli), Host::Linux);
            assert_eq!(w.calls().last().unwrap(), "journalctl --user -u aoide-job-nightly.service -o json --no-pager");
        }

        #[test]
        fn a_symlinked_or_foreign_unit_in_the_way_is_refused_and_never_written_through() {
            let _guard = lock();
            let w = World::new("job-symlink");
            let tool = w.tool("job-tool");
            let victim = w.root.join("victim.txt");
            std::fs::write(&victim, "keep").unwrap();
            std::fs::create_dir_all(w.unit_dir()).unwrap();
            std::os::unix::fs::symlink(&victim, w.unit_dir().join("aoide-job-link.timer")).unwrap();
            let out = add(&add_inv("link", "daily", &[tool.as_str()]), Host::Linux);
            assert_eq!(out.status, Status::Error, "{}", out.message);
            assert_eq!(std::fs::read_to_string(&victim).unwrap(), "keep");
            let foreign = w.unit_dir().join("aoide-job-plain.service");
            std::fs::write(&foreign, "mine\n").unwrap();
            let out = add(&add_inv("plain", "daily", &[tool.as_str()]), Host::Linux);
            assert_eq!(out.status, Status::Error, "{}", out.message);
            assert_eq!(std::fs::read_to_string(&foreign).unwrap(), "mine\n");
            assert!(w.record().get("jobs").is_none());
        }

        #[test]
        fn a_unit_directory_that_is_a_symlink_is_refused_and_nothing_is_written_through_it() {
            let _guard = lock();
            let w = World::new("job-symlink-dir");
            let tool = w.tool("job-tool");
            let victim = w.root.join("victim-dir");
            std::fs::create_dir_all(&victim).unwrap();
            std::fs::create_dir_all(w.root.join("config/systemd")).unwrap();
            std::os::unix::fs::symlink(&victim, w.unit_dir()).unwrap();
            let out = add(&add_inv("nightly", "daily", &[tool.as_str()]), Host::Linux);
            assert_eq!(out.status, Status::Error, "{}", out.message);
            assert!(std::fs::read_dir(&victim).unwrap().next().is_none(), "nothing written through the link");
        }

        /// The lane check (app-plan-v21 §7.2): writes one generated pair into
        /// `$TMPDIR/job-lane-check/` for `systemd-analyze verify`, which the
        /// lane runs by hand. Ignored, so the suite never runs it.
        #[test]
        #[ignore = "lane check: writes one generated pair for systemd-analyze verify"]
        fn lane_check_pair_is_generated_for_systemd_analyze_verify() {
            let dir = std::env::temp_dir().join("job-lane-check");
            std::fs::create_dir_all(&dir).unwrap();
            let argv = ["/run/current-system/sw/bin/true".to_string(), r#"a"b\c%d$e f"#.to_string()];
            let (timer, service) = unit_texts("lane-check", "daily", &argv, None, Path::new("/home/khoa/.t/c8/root"));
            write_units(&dir, "lane-check", &timer, &service).unwrap();
            println!("{}", dir.display());
        }
    }
}
