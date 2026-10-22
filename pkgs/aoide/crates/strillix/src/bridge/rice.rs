//! The rice verbs: `ricemode`, `ricemenu`, `ricedraft` and `ricedeclare`. Each runs
//! the `strillix rice …` CLI as a bounded child and reads its `--json` envelope.
//! The mode, draft and declare steps are the CLI's; this module only sequences them.

use aoide_conduct::shellbridge::{
    outcome_envelope, outcome_triple, run_bin_bounded, safe_session_id,
};
use aoide_protocol as daemon;
use strillix_rice::mode::{load_mode_marker, ModeMarker, RiceMode};
use serde_json::{json, Value};

/// How long one `ricemenu` answer may take, both children together: under
/// ShellBridge.qml's own five second `replyTimeoutMs`, so the daemon abandons
/// the child and says so before the caller gives up on the daemon.
const RICE_MENU_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(4);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RiceModeAction {
    /// Stage this runtime rice. Whether it exists is the CLI's call.
    Stage { name: String },
    /// Lock back to the declared rice; the CLI resolves which that is.
    Declarative,
}

impl RiceModeAction {
    pub(crate) fn from_wire(v: &Value) -> Option<Self> {
        let name = v.get("name");
        match v.get("action").and_then(Value::as_str)? {
            "stage" => {
                let name = name?.as_str()?;
                safe_session_id(name).then(|| Self::Stage { name: name.to_string() })
            }
            "declarative" => name.is_none().then_some(Self::Declarative),
            _ => None,
        }
    }

    /// The wire name back, for the audit line.
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Stage { .. } => "stage",
            Self::Declarative => "declarative",
        }
    }
}

/// The four things a `ricedraft` line can ask for. A closed set: an unknown
/// or non-string action, an `enter`/`drop` with no usable `name`, or a
/// `new`/`save` carrying one is refused at the wire (`parse_command` → `None`, no reply),
/// never dispatched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RiceDraftAction {
    /// Route the stage into this draft. The name is one argv token and no
    /// more: whether it is a valid draft name is the CLI's call —
    /// `strillix-rice` is strillix-only and conduct never depends on it, so the
    /// regex lives in one place. Whether the draft EXISTS is nobody's check
    /// here: `rice mode draft` forks the current stage into a valid name that
    /// is not there yet, so an enter is enter-or-create.
    Enter { name: String },
    /// Mint the next free draft name from the staged rice, then enter it.
    New,
    /// Snapshot the stage as a draft of the staged rice. Never changes mode,
    /// and is refused while declarative is locked ([`rice_draft_plan`]).
    Save,
    /// Delete this saved draft of the staged rice. The name rules are
    /// `Enter`'s; whether the draft exists, and the refusal of the draft in
    /// force, are the CLI's (`rice draft drop`). Never changes mode, and is
    /// refused while declarative is locked, like `Save`.
    Drop { name: String },
}

impl RiceDraftAction {
    /// Parse one wire line's `action` and its own fields. `new` and `save`
    /// take no `name`: one beside them is a caller that meant `enter`, and
    /// silently ignoring it would let that mistake read as success (the
    /// [`WorkspaceBinding`] `clear` rule).
    pub(crate) fn from_wire(v: &Value) -> Option<Self> {
        let name = v.get("name");
        match v.get("action").and_then(Value::as_str)? {
            "enter" => {
                let name = name?.as_str()?;
                safe_session_id(name).then(|| Self::Enter { name: name.to_string() })
            }
            "new" => name.is_none().then_some(Self::New),
            "save" => name.is_none().then_some(Self::Save),
            "drop" => {
                let name = name?.as_str()?;
                safe_session_id(name).then(|| Self::Drop { name: name.to_string() })
            }
            _ => None,
        }
    }

    /// The wire name back, for the audit line.
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Enter { .. } => "enter",
            Self::New => "new",
            Self::Save => "save",
            Self::Drop { .. } => "drop",
        }
    }
}

/// Dispatch ONE `ricemode` action: `strillix rice mode stage <name>` or a bare
/// `strillix rice mode declarative` (the CLI resolves the declared twin), waited
/// for through [`run_rice_step`]. The callee is this build's own `strillix`
/// ([`strillix_bin`]). NOT inside
/// `with_stage_lock`: holding it across a blocking child wait is a deadlock
/// waiting to happen. UNBOUNDED like every mutation here. `Ok` is the CLI's
/// own `message`; `Err` is its refusal or the spawn error.
pub(crate) fn dispatch_rice_mode(action: &RiceModeAction) -> Result<String, String> {
    let argv: Vec<String> = match action {
        RiceModeAction::Stage { name } => vec!["rice".into(), "mode".into(), "stage".into(), name.clone()],
        RiceModeAction::Declarative => vec!["rice".into(), "mode".into(), "declarative".into()],
    };
    match run_rice_step(&argv) {
        (true, message, _) => Ok(message),
        (false, message, _) => Err(message),
    }
}

/// Fire a detached `notify-send -- "Aoide" <message>` — the same reaper-thread
/// idiom as [`dispatch_power`]'s spawned child, so a slow or hung
/// `notify-send` can never block a connection. The message can echo a rice
/// folder name: `--` keeps a leading `-` from reading as an option and
/// [`aoide_protocol::dialog::escape_markup`] keeps markup inert. A spawn failure is logged and
/// nothing more: the action this toast reports already ran, and a dead or
/// missing `notify-send` must not turn its outcome into a different one.
pub(crate) fn notify(message: &str) {
    match std::process::Command::new("notify-send")
        .arg("--")
        .arg("Aoide")
        .arg(aoide_protocol::dialog::escape_markup(message))
        .spawn()
    {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        Err(e) => eprintln!("[aoide/shellbridge] notify-send failed (the action's own outcome stands): {e}"),
    }
}

// ── the RICE menu (`ricemenu`, `ricedraft`, `ricedeclare`) ─────────────────

/// Pure plan: the `strillix` argv, in order and without `--json`, that one
/// `ricedraft` action takes in this mode, or the taught refusal that ends it.
/// `enter` and `new` route the stage into a draft, which `rice mode draft`
/// refuses while declarative is locked, so from `Declarative` they unlock
/// first — with a BARE `rice mode stage`, which restores the remembered
/// `stagingRice` (house rule 10), never the declared rice. `save` and `drop`
/// never change the mode, and both are refused while declarative is locked:
/// the stage there is the DECLARED rice (the lock re-pins it), `rice draft
/// save`/`drop` act on the rice `stage/theme.json` names, and the listing
/// ([`drafts_rice`]) names `stagingRice` — a save would land under, and a drop
/// would delete from, a rice the picker never shows. Unlocking first to make
/// them land would change the mode as a side effect of a save or a delete.
/// The unlock is one click away, so the refusal costs one click. `new` plans only the save: the draft's name is minted by the CLI and
/// is not known until that step has answered, so [`dispatch_rice_draft`]
/// appends the entering step itself.
pub(crate) fn rice_draft_plan(mode: RiceMode, action: &RiceDraftAction) -> Result<Vec<Vec<String>>, &'static str> {
    let step = |words: &[&str]| words.iter().map(|w| w.to_string()).collect::<Vec<String>>();
    let mut plan = Vec::new();
    if mode == RiceMode::Declarative {
        match action {
            RiceDraftAction::Save => return Err("declarative mode is locked — unlock first, then save the draft"),
            RiceDraftAction::Drop { .. } => {
                return Err("declarative mode is locked — unlock first, then delete the draft")
            }
            RiceDraftAction::Enter { .. } | RiceDraftAction::New => {}
        }
        plan.push(step(&["rice", "mode", "stage"]));
    }
    match action {
        RiceDraftAction::Enter { name } => plan.push(step(&["rice", "mode", "draft", name.as_str()])),
        RiceDraftAction::New | RiceDraftAction::Save => plan.push(step(&["rice", "draft", "save"])),
        RiceDraftAction::Drop { name } => plan.push(step(&["rice", "draft", "drop", name.as_str()])),
    }
    Ok(plan)
}

/// The `strillix` the rice bridge re-execs: the sibling-binary resolver's
/// answer with this crate's own override (`STRILLIX_BIN`, then a sibling of
/// the running executable, then PATH), so a built `strillix` runs the same
/// build of itself.
fn strillix_bin() -> String {
    daemon::bin::resolve_bin("STRILLIX_BIN", "strillix")
}

/// Run ONE planned step: `strillix <argv> --json`, waited for, through
/// [`strillix_bin`] — and read through
/// [`outcome_triple`], the one reader of a `--json` envelope. NOT inside
/// `with_stage_lock` (a blocking child wait under it is a deadlock waiting to
/// happen). A child that
/// could not be started is `ok: false` with the spawn error as the message,
/// naming the first two argv words only — the command path, never a value.
pub(crate) fn run_rice_step(argv: &[String]) -> (bool, String, Option<Value>) {
    match std::process::Command::new(strillix_bin())
        .args(argv)
        .arg("--json")
        .output()
    {
        Ok(out) => outcome_triple(
            "rice",
            argv.get(1).map(String::as_str).unwrap_or(""),
            out.status.success(),
            &String::from_utf8_lossy(&out.stdout),
            &String::from_utf8_lossy(&out.stderr),
        ),
        Err(e) => (
            false,
            format!(
                "spawning `strillix {} {}`: {e}",
                argv.first().map(String::as_str).unwrap_or(""),
                argv.get(1).map(String::as_str).unwrap_or("")
            ),
            None,
        ),
    }
}

/// Run ONE `ricedraft` action: [`rice_draft_plan`] for the current mode, each
/// step through [`run_rice_step`], stopping at the first that fails. `Ok` is
/// the last step's own CLI `message`; `Err((message, partial))` is the failing
/// step's (or the planner's refusal, before any step ran), with `partial` true
/// when an earlier step had already succeeded — the mode may be unlocked, or a
/// draft saved, and nothing here rolls it back: the toast says so, and the
/// click that follows is one gesture away.
///
/// `new` is the one action whose second step depends on the first's answer:
/// the CLI mints the name (`rice draft save` with none), and the entering
/// step takes that step's `data.name`. UNBOUNDED like every mutation here: one
/// per human gesture, and killing a half-applied one to tidy up is worse than
/// waiting.
pub(crate) fn dispatch_rice_draft(action: &RiceDraftAction) -> Result<String, (String, bool)> {
    let mut plan = rice_draft_plan(load_mode_marker().mode, action).map_err(|why| (why.to_string(), false))?;
    let mut said = String::new();
    let mut ran = 0;
    while ran < plan.len() {
        let (ok, message, data) = run_rice_step(&plan[ran]);
        if !ok {
            return Err((message, ran > 0));
        }
        if *action == RiceDraftAction::New && plan[ran] == ["rice", "draft", "save"] {
            let Some(name) = data.as_ref().and_then(|d| d.get("name")).and_then(Value::as_str) else {
                return Err(("`rice draft save` answered with no draft name".to_string(), true));
            };
            plan.push(vec!["rice".to_string(), "mode".to_string(), "draft".to_string(), name.to_string()]);
        }
        said = message;
        ran += 1;
    }
    Ok(said)
}

/// Run ONE `ricedeclare`: `strillix rice declare <name> --host --json`, waited
/// for like every mutation here (one per human gesture), and read into the
/// reply [`rice_declare_reply`] shapes.
pub(crate) fn dispatch_rice_declare(name: &str) -> Value {
    let argv: Vec<String> = ["rice", "declare", name, "--host"].iter().map(|w| w.to_string()).collect();
    let (ok, message, data) = run_rice_step(&argv);
    rice_declare_reply(ok, message, data)
}

/// The rice-name shape (`strillix_rice::compose::valid_rice_name`, restated
/// because conduct does not depend on the rice crate): a lowercase letter or
/// digit, then lowercase letters, digits and hyphens.
pub(crate) fn rice_name(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Pure: the audit event and outcome a `ricedeclare` answer is logged as, in
/// [`audit_bridge_action`]'s `<noun> action <action>: <outcome>` vocabulary
/// with the rice as the action: `ok changed` / `ok unchanged` (the host
/// already declared it) or `failed`.
pub(crate) fn rice_declare_audit(answer: &Value) -> (&'static str, &'static str) {
    if answer.get("ok").and_then(Value::as_bool) != Some(true) {
        return ("ricedeclare-failed", "failed");
    }
    match answer.get("changed").and_then(Value::as_bool) {
        Some(false) => ("ricedeclare", "ok unchanged"),
        _ => ("ricedeclare", "ok changed"),
    }
}

/// Pure: the one line a `ricedeclare` answers with. Success carries the CLI's
/// own message and `name`, `host`, `changed`, `stagedDiffers` from its data; a
/// refusal carries `reason` (the CLI's, else `cli-failed`) and a `message` that
/// is the refusal's what and why, so the balloon can say both.
pub(crate) fn rice_declare_reply(ok: bool, message: String, data: Option<Value>) -> Value {
    let data = data.unwrap_or(Value::Null);
    let field = |key: &str| data.get(key).cloned().unwrap_or(Value::Null);
    if ok {
        return json!({
            "ok": true,
            "message": message,
            "name": field("name"),
            "host": field("host"),
            "changed": field("changed"),
            "stagedDiffers": field("stagedDiffers"),
        });
    }
    let message = match data.pointer("/refusal/why").and_then(Value::as_str) {
        Some(why) if !why.is_empty() => format!("{message}: {why}"),
        _ => message,
    };
    let reason = data.get("reason").and_then(Value::as_str).unwrap_or("cli-failed");
    json!({ "ok": false, "reason": reason, "message": message })
}

/// Pure decision: which rice's drafts the menu lists, from the marker. The
/// menu's `enter` and `new` from `Declarative` unlock through a bare `rice
/// mode stage`, which lands on the remembered `stagingRice` — so that is the
/// rice a declarative listing must name, ahead of `rice` (the declared one the
/// lock overwrote). Its `save` and `drop` are refused there
/// ([`rice_draft_plan`]), so no action ever writes under, or deletes from, a
/// rice this listing does not name. Every other
/// mode is already working on `rice`; `stagingRice` is only the fallback for a
/// marker that lacks it.
pub(crate) fn drafts_rice(m: &ModeMarker) -> Option<String> {
    match m.mode {
        RiceMode::Declarative => m.staging_rice.clone().or_else(|| m.rice.clone()),
        RiceMode::Staging | RiceMode::Draft => m.rice.clone().or_else(|| m.staging_rice.clone()),
    }
}

/// The ONE line a `ricemenu` refusal answers with: the machine `reason`
/// (`cli-failed` — the CLI answered and refused; `no-answer` — nothing
/// readable came back) and a `message` that is the CLI's own where it has one.
pub(crate) fn rice_menu_refusal(reason: &str, message: &str) -> Value {
    json!({ "ok": false, "reason": reason, "message": message })
}

/// Run ONE bounded `strillix <argv> --json` and return its envelope's `data`, or
/// the refusal the menu answers with. `argv` carries no `--json`.
pub(crate) fn run_rice_json(argv: &[&str], timeout: std::time::Duration) -> Result<Value, Value> {
    let mut args: Vec<String> = argv.iter().map(|w| w.to_string()).collect();
    args.push("--json".to_string());
    let label = argv.iter().take(3).copied().collect::<Vec<_>>().join(" ");
    let (stdout, stderr) = match run_bin_bounded(&strillix_bin(), &args, timeout) {
        Ok((_exited_ok, stdout, stderr)) => (stdout, stderr),
        Err(message) => return Err(rice_menu_refusal("no-answer", &message)),
    };
    let Some((ok, message, data)) = outcome_envelope(&stdout).or_else(|| outcome_envelope(&stderr)) else {
        return Err(rice_menu_refusal("no-answer", &format!("`strillix {label}` printed no parseable answer")));
    };
    if !ok {
        return Err(rice_menu_refusal("cli-failed", &message));
    }
    data.ok_or_else(|| rice_menu_refusal("no-answer", &format!("`strillix {label}` answered with no data")))
}

/// Dispatch ONE read-only menu query: `strillix rice list --json` under
/// [`RICE_MENU_TIMEOUT`], its `data` (`mode`, `rice`, `draft`, `stagingRice`,
/// `declared`, `rices`, `nextRebuild`) passed through verbatim with this
/// machine's `host` name beside it (the name the Declare row's alert speaks)
/// and `hostConfig`, the file `strillix rice declare --host` would rewrite
/// ([`strillix_rice::paths::host_config`], null when none resolves), then — when
/// [`drafts_rice`] names a rice for the marker — `strillix rice draft list <rice>
/// --json` with the budget the first child left, projected to `draftsRice`
/// and `drafts: [{name, savedAt, current}]`. No rice means `drafts: []` and
/// no second child; a failed list starts none either. A failed, unreadable
/// or out-of-budget child is a refusal ([`rice_menu_refusal`]), never an empty
/// answer — the menu must tell "nothing there" from "nobody answered".
pub(crate) fn dispatch_rice_menu() -> Value {
    let deadline = std::time::Instant::now() + RICE_MENU_TIMEOUT;
    let mut answer = match run_rice_json(&["rice", "list"], RICE_MENU_TIMEOUT) {
        Ok(Value::Object(map)) => map,
        Ok(_) => return rice_menu_refusal("no-answer", "`strillix rice list` answered with no object"),
        Err(refusal) => return refusal,
    };
    let marker = load_mode_marker();
    let rice = drafts_rice(&marker);
    let mut drafts: Vec<Value> = Vec::new();
    if let Some(rice) = &rice {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() {
            return rice_menu_refusal("no-answer", "the menu's time budget ran out before the drafts were listed");
        }
        let data = match run_rice_json(&["rice", "draft", "list", rice.as_str()], left) {
            Ok(data) => data,
            Err(refusal) => return refusal,
        };
        let Some(listed) = data.get("drafts").and_then(Value::as_array) else {
            return rice_menu_refusal("no-answer", "`strillix rice draft list` answered with no `drafts`");
        };
        drafts = listed
            .iter()
            .map(|d| {
                let field = |key: &str| d.get(key).cloned().unwrap_or(Value::Null);
                json!({ "name": field("name"), "savedAt": field("savedAt"), "current": field("current") })
            })
            .collect();
    }
    answer.insert("ok".to_string(), Value::Bool(true));
    answer.insert("host".to_string(), json!(aoide_storage::display::os_hostname()));
    answer.insert("hostConfig".to_string(), json!(strillix_rice::paths::host_config()));
    answer.insert("draftsRice".to_string(), json!(rice));
    answer.insert("drafts".to_string(), Value::Array(drafts));
    Value::Object(answer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::testutil::env_lock;
    use crate::bridge::testutil::short_sock;
    use aoide_conduct::shellbridge::serve;
    use crate::bridge::SOCKET;
    use crate::bridge::testutil::{stub_argv_log, stub_rice_bin};
    use aoide_test_support::sv;
    use std::io::{BufRead, BufReader};
    use std::os::unix::net::{UnixListener, UnixStream};
    use crate::bridge::{parse_command, BridgeCommand};

    #[test]
    fn parse_command_accepts_ricemenu_and_both_ricemode_shapes() {
        assert_eq!(parse_command(r#"{"cmd":"ricemenu"}"#), Some(BridgeCommand::RiceMenu));
        assert_eq!(parse_command("  {\"cmd\":\"ricemenu\"}\n"), Some(BridgeCommand::RiceMenu));
        assert_eq!(
            parse_command(r#"{"cmd":"ricemode","action":"stage","name":"fugue"}"#),
            Some(BridgeCommand::RiceMode { action: RiceModeAction::Stage { name: "fugue".to_string() } })
        );
        assert_eq!(
            parse_command(r#"{"cmd":"ricemode","action":"declarative"}"#),
            Some(BridgeCommand::RiceMode { action: RiceModeAction::Declarative })
        );
    }

    #[test]
    fn parse_command_refuses_every_malformed_ricemode_and_the_retired_verbs() {
        for line in [
            r#"{"cmd":"ricemode"}"#,
            r#"{"cmd":"ricemode","action":"stage"}"#,
            r#"{"cmd":"ricemode","action":"stage","name":"-x"}"#,
            r#"{"cmd":"ricemode","action":"stage","name":7}"#,
            r#"{"cmd":"ricemode","action":"declarative","name":"fugue"}"#,
            r#"{"cmd":"ricemode","action":"toggle"}"#,
            r#"{"cmd":"ricemode","action":42}"#,
            r#"{"cmd":"ricedrafts"}"#,
        ] {
            assert_eq!(parse_command(line), None, "{line}");
        }
    }

    #[test]
    fn parse_command_accepts_every_ricedraft_shape() {
        assert_eq!(
            parse_command(r#"{"cmd":"ricedraft","action":"enter","name":"draft-2"}"#),
            Some(BridgeCommand::RiceDraft {
                action: RiceDraftAction::Enter { name: "draft-2".to_string() }
            })
        );
        assert_eq!(
            parse_command(r#"{"cmd":"ricedraft","action":"new"}"#),
            Some(BridgeCommand::RiceDraft { action: RiceDraftAction::New })
        );
        assert_eq!(
            parse_command(r#"{"cmd":"ricedraft","action":"save"}"#),
            Some(BridgeCommand::RiceDraft { action: RiceDraftAction::Save })
        );
        assert_eq!(
            parse_command(r#"{"cmd":"ricedraft","action":"drop","name":"draft-2"}"#),
            Some(BridgeCommand::RiceDraft {
                action: RiceDraftAction::Drop { name: "draft-2".to_string() }
            })
        );
    }

    #[test]
    fn parse_command_refuses_a_ricedraft_it_cannot_name() {
        for line in [
            // `enter` needs a name that is one argv token.
            r#"{"cmd":"ricedraft","action":"enter"}"#,
            r#"{"cmd":"ricedraft","action":"enter","name":""}"#,
            r#"{"cmd":"ricedraft","action":"enter","name":"-x"}"#,
            r#"{"cmd":"ricedraft","action":"enter","name":"a b"}"#,
            r#"{"cmd":"ricedraft","action":"enter","name":"a\n"}"#,
            r#"{"cmd":"ricedraft","action":"enter","name":7}"#,
            r#"{"cmd":"ricedraft","action":"enter","name":null}"#,
            // `drop` needs one exactly like `enter`.
            r#"{"cmd":"ricedraft","action":"drop"}"#,
            r#"{"cmd":"ricedraft","action":"drop","name":""}"#,
            r#"{"cmd":"ricedraft","action":"drop","name":"-x"}"#,
            r#"{"cmd":"ricedraft","action":"drop","name":"a b"}"#,
            r#"{"cmd":"ricedraft","action":"drop","name":"a\n"}"#,
            r#"{"cmd":"ricedraft","action":"drop","name":7}"#,
            r#"{"cmd":"ricedraft","action":"drop","name":null}"#,
            // `new` and `save` take none: a name beside them is a caller that
            // meant `enter`, and ignoring it would read as success.
            r#"{"cmd":"ricedraft","action":"new","name":"draft-2"}"#,
            r#"{"cmd":"ricedraft","action":"save","name":"draft-2"}"#,
            r#"{"cmd":"ricedraft","action":"save","name":null}"#,
            // a closed set
            r#"{"cmd":"ricedraft","action":"delete","name":"draft-2"}"#,
            r#"{"cmd":"ricedraft","action":"Drop","name":"draft-2"}"#,
            r#"{"cmd":"ricedraft","action":"Enter","name":"draft-2"}"#,
            r#"{"cmd":"ricedraft","action":7}"#,
            r#"{"cmd":"ricedraft","action":null}"#,
            r#"{"cmd":"ricedraft"}"#,
        ] {
            assert_eq!(parse_command(line), None, "{line}");
        }
    }

    #[test]
    fn rice_draft_plan_unlocks_declarative_for_enter_and_new_and_refuses_its_save_and_drop() {
        let enter = RiceDraftAction::Enter { name: "draft-2".to_string() };
        let drop = RiceDraftAction::Drop { name: "draft-2".to_string() };
        let gone = sv(&["rice", "draft", "drop", "draft-2"]);
        let unlock = sv(&["rice", "mode", "stage"]);
        let save = sv(&["rice", "draft", "save"]);
        let go = sv(&["rice", "mode", "draft", "draft-2"]);

        // Declarative: `enter` and `new` unlock FIRST, bare — never a rice.
        assert_eq!(rice_draft_plan(RiceMode::Declarative, &enter), Ok(vec![unlock.clone(), go.clone()]));
        assert_eq!(rice_draft_plan(RiceMode::Declarative, &RiceDraftAction::New), Ok(vec![unlock, save.clone()]));
        // `save` there would nest under the DECLARED rice, which the listing
        // (the staged one) never shows: a taught refusal, nothing planned.
        assert_eq!(
            rice_draft_plan(RiceMode::Declarative, &RiceDraftAction::Save),
            Err("declarative mode is locked — unlock first, then save the draft")
        );
        // `drop` there would delete from the DECLARED rice's drafts: the same
        // refusal, and no unlock as a side effect of a delete.
        assert_eq!(
            rice_draft_plan(RiceMode::Declarative, &drop),
            Err("declarative mode is locked — unlock first, then delete the draft")
        );
        // Staging and Draft are already unlocked: one step. `new` plans the
        // save alone — the entering step needs the name that step mints.
        for mode in [RiceMode::Staging, RiceMode::Draft] {
            assert_eq!(rice_draft_plan(mode, &enter), Ok(vec![go.clone()]), "{mode:?}");
            assert_eq!(rice_draft_plan(mode, &RiceDraftAction::New), Ok(vec![save.clone()]), "{mode:?}");
            assert_eq!(rice_draft_plan(mode, &RiceDraftAction::Save), Ok(vec![save.clone()]), "{mode:?}");
            assert_eq!(rice_draft_plan(mode, &drop), Ok(vec![gone.clone()]), "{mode:?}");
        }
    }

    #[test]
    fn drafts_rice_names_the_rice_the_mode_will_work_on() {
        let marker = |mode, rice: Option<&str>, staging: Option<&str>| ModeMarker {
            mode,
            rice: rice.map(str::to_string),
            staging_rice: staging.map(str::to_string),
            ..ModeMarker::default()
        };
        // Declarative: the lock overwrote `rice` with the declared one; the
        // unlock lands on `stagingRice`, so that is what the picker lists.
        let declarative = marker(RiceMode::Declarative, Some("sonata"), Some("cadenza"));
        assert_eq!(drafts_rice(&declarative).as_deref(), Some("cadenza"));
        assert_eq!(drafts_rice(&marker(RiceMode::Declarative, Some("sonata"), None)).as_deref(), Some("sonata"));
        // Staging and Draft already work on `rice`.
        for mode in [RiceMode::Staging, RiceMode::Draft] {
            assert_eq!(drafts_rice(&marker(mode, Some("sonata"), Some("cadenza"))).as_deref(), Some("sonata"));
            assert_eq!(drafts_rice(&marker(mode, None, Some("cadenza"))).as_deref(), Some("cadenza"));
            assert_eq!(drafts_rice(&marker(mode, None, None)), None);
        }
        assert_eq!(drafts_rice(&marker(RiceMode::Declarative, None, None)), None);
    }

    /// GATED on Unix with its reason (see [`stub_rice_bin`]).
    #[cfg(unix)]
    #[test]
    fn a_new_draft_from_declarative_unlocks_saves_and_enters_the_name_the_cli_minted() {
        let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, root) = stub_rice_bin(
            "shellbridge-draft-new",
            r#"{"mode":"declarative","rice":"sonata","stagingRice":"cadenza"}"#,
        );
        assert_eq!(dispatch_rice_draft(&RiceDraftAction::New), Ok("routed draft-3".to_string()));
        assert_eq!(
            stub_argv_log(&root),
            ["rice mode stage --json", "rice draft save --json", "rice mode draft draft-3 --json"]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// GATED on Unix with its reason (see [`stub_rice_bin`]).
    #[cfg(unix)]
    #[test]
    fn an_enter_from_draft_is_one_step_and_a_save_from_declarative_is_refused_unrun() {
        let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, root) = stub_rice_bin("shellbridge-draft-one", r#"{"mode":"draft","rice":"sonata","draft":"draft-1"}"#);
        let enter = RiceDraftAction::Enter { name: "draft-2".to_string() };
        assert_eq!(dispatch_rice_draft(&enter), Ok("routed draft-2".to_string()));
        assert_eq!(stub_argv_log(&root), ["rice mode draft draft-2 --json"]);

        std::fs::write(root.join("stage").join("mode.json"), r#"{"mode":"declarative","rice":"sonata"}"#).unwrap();
        std::fs::remove_file(root.join("argv.log")).unwrap();
        // The refusal is the first thing that happens: not partial, no child.
        assert_eq!(
            dispatch_rice_draft(&RiceDraftAction::Save),
            Err(("declarative mode is locked — unlock first, then save the draft".to_string(), false))
        );
        assert!(stub_argv_log(&root).is_empty(), "a refused save starts no child");

        std::fs::write(root.join("stage").join("mode.json"), r#"{"mode":"staging","rice":"sonata"}"#).unwrap();
        assert_eq!(dispatch_rice_draft(&RiceDraftAction::Save), Ok("saved".to_string()));
        assert_eq!(stub_argv_log(&root), ["rice draft save --json"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// GATED on Unix with its reason (see [`stub_rice_bin`]).
    #[cfg(unix)]
    #[test]
    fn a_drop_is_one_step_its_refusal_is_the_clis_and_declarative_refuses_it_unrun() {
        let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, root) = stub_rice_bin("shellbridge-draft-drop", r#"{"mode":"draft","rice":"sonata","draft":"draft-2"}"#);
        let drop1 = RiceDraftAction::Drop { name: "draft-1".to_string() };
        assert_eq!(dispatch_rice_draft(&drop1), Ok("dropped draft-1".to_string()));
        assert_eq!(stub_argv_log(&root), ["rice draft drop draft-1 --json"]);

        // The draft in force: the bridge does not second-guess it, the CLI
        // refuses, and its own message is the answer (first step, not partial).
        std::fs::remove_file(root.join("argv.log")).unwrap();
        std::fs::write(root.join("refuse-drop"), "").unwrap();
        let live = RiceDraftAction::Drop { name: "draft-2".to_string() };
        assert_eq!(
            dispatch_rice_draft(&live),
            Err(("cannot drop `draft-2` — it is the live draft".to_string(), false))
        );
        assert_eq!(stub_argv_log(&root), ["rice draft drop draft-2 --json"]);

        std::fs::write(root.join("stage").join("mode.json"), r#"{"mode":"declarative","rice":"sonata","stagingRice":"cadenza"}"#).unwrap();
        std::fs::remove_file(root.join("argv.log")).unwrap();
        assert_eq!(
            dispatch_rice_draft(&drop1),
            Err(("declarative mode is locked — unlock first, then delete the draft".to_string(), false))
        );
        assert!(stub_argv_log(&root).is_empty(), "a refused drop starts no child");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// GATED on Unix with its reason (see [`stub_rice_bin`]).
    #[cfg(unix)]
    #[test]
    fn a_failing_later_step_is_partial_and_a_failing_first_step_is_not() {
        let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, root) = stub_rice_bin(
            "shellbridge-draft-partial",
            r#"{"mode":"declarative","rice":"sonata"}"#,
        );
        std::fs::write(root.join("refuse-draft"), "").unwrap();
        let enter = RiceDraftAction::Enter { name: "nope".to_string() };
        // The unlock ran and the entering step refused: the mode is now
        // unlocked, and the answer says so rather than reading as a clean no.
        assert_eq!(dispatch_rice_draft(&enter), Err(("no rice is currently staged".to_string(), true)));
        assert_eq!(stub_argv_log(&root), ["rice mode stage --json", "rice mode draft nope --json"]);

        // Already unlocked: the same refusal is the FIRST step, nothing ran.
        std::fs::write(root.join("stage").join("mode.json"), r#"{"mode":"staging","rice":"sonata"}"#).unwrap();
        assert_eq!(dispatch_rice_draft(&enter), Err(("no rice is currently staged".to_string(), false)));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// GATED on Unix with its reason (see [`stub_rice_bin`]).
    #[cfg(unix)]
    #[test]
    fn dispatch_rice_menu_merges_the_rices_with_the_modes_rice_drafts() {
        let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, root) = stub_rice_bin(
            "shellbridge-menu-list",
            r#"{"mode":"declarative","rice":"sonata","stagingRice":"cadenza"}"#,
        );
        std::env::set_var("STRILLIX_HOST_CONFIG", "/home/u/dxflake/hosts/box/default.nix");
        assert_eq!(
            dispatch_rice_menu(),
            json!({
                "ok": true,
                "mode": "declarative",
                "rice": "sonata",
                "draft": null,
                "stagingRice": "cadenza",
                "declared": "sonata",
                "rices": [{ "name": "cadenza", "ok": true }, { "name": "sonata", "ok": true }],
                "host": aoide_storage::display::os_hostname(),
                "hostConfig": "/home/u/dxflake/hosts/box/default.nix",
                "draftsRice": "cadenza",
                "drafts": [
                    { "name": "draft-1", "savedAt": "2026-01-01T00:00:00Z", "current": false },
                    { "name": "draft-2", "savedAt": null, "current": true },
                ],
            })
        );
        assert_eq!(stub_argv_log(&root), ["rice list --json", "rice draft list cadenza --json"]);

        // A CLI that refuses is a refusal carrying its own words — never an
        // empty list the menu would paint as "no drafts yet".
        std::fs::write(root.join("refuse-list"), "").unwrap();
        assert_eq!(
            dispatch_rice_menu(),
            json!({ "ok": false, "reason": "cli-failed", "message": "no rice staged" })
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn parse_command_takes_a_ricedeclare_with_one_usable_name() {
        assert_eq!(
            parse_command(r#"{"cmd":"ricedeclare","name":"barcarolle"}"#),
            Some(BridgeCommand::RiceDeclare { name: "barcarolle".to_string() })
        );
        for line in [
            r#"{"cmd":"ricedeclare"}"#,
            r#"{"cmd":"ricedeclare","name":""}"#,
            r#"{"cmd":"ricedeclare","name":"-x"}"#,
            r#"{"cmd":"ricedeclare","name":"a b"}"#,
            r#"{"cmd":"ricedeclare","name":7}"#,
            r#"{"cmd":"ricedeclare","name":"Barcarolle"}"#,
            r#"{"cmd":"ricedeclare","name":"a_b"}"#,
            r#"{"cmd":"ricedeclare","name":"../x"}"#,
            r#"{"cmd":"ricedeclare","name":"a:b"}"#,
        ] {
            assert_eq!(parse_command(line), None, "{line}");
        }
    }

    #[test]
    fn a_declare_is_audited_by_rice_and_whether_it_changed() {
        assert_eq!(rice_declare_audit(&json!({ "ok": true, "changed": true })), ("ricedeclare", "ok changed"));
        assert_eq!(rice_declare_audit(&json!({ "ok": true, "changed": false })), ("ricedeclare", "ok unchanged"));
        assert_eq!(rice_declare_audit(&json!({ "ok": false, "reason": "not-built-in" })), ("ricedeclare-failed", "failed"));
        assert_eq!(rice_declare_audit(&json!({})), ("ricedeclare-failed", "failed"));
    }

    /// GATED on Unix with its reason (see [`stub_rice_bin`]).
    #[cfg(unix)]
    #[test]
    fn dispatch_rice_declare_runs_the_host_declare_and_answers_both_outcomes() {
        let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, root) = stub_rice_bin("shellbridge-declare", r#"{"mode":"staging","rice":"barcarolle"}"#);
        assert_eq!(
            dispatch_rice_declare("barcarolle"),
            json!({
                "ok": true,
                "message": "declared `barcarolle`; takes effect on the next rebuild.",
                "name": "barcarolle",
                "host": "box",
                "changed": true,
                "stagedDiffers": true,
            })
        );
        assert_eq!(stub_argv_log(&root), ["rice declare barcarolle --host --json"]);

        std::fs::write(root.join("refuse-declare"), "").unwrap();
        assert_eq!(
            dispatch_rice_declare("fugue"),
            json!({
                "ok": false,
                "reason": "not-built-in",
                "message": "`fugue` is not in the rices box builds from: this build carries sonata",
            })
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_declare_with_no_envelope_is_a_cli_failure() {
        assert_eq!(
            rice_declare_reply(false, "spawning `strillix rice declare`: not found".to_string(), None),
            json!({ "ok": false, "reason": "cli-failed", "message": "spawning `strillix rice declare`: not found" })
        );
    }

    /// GATED on Unix with its reason (see [`stub_rice_bin`]).
    #[cfg(unix)]
    #[test]
    fn a_failed_rice_list_is_a_refusal_and_starts_no_second_child() {
        let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, root) = stub_rice_bin("shellbridge-menu-nolist", r#"{"mode":"staging","rice":"sonata"}"#);
        std::fs::write(root.join("refuse-rices"), "").unwrap();
        assert_eq!(
            dispatch_rice_menu(),
            json!({ "ok": false, "reason": "cli-failed", "message": "rices unreadable" })
        );
        assert_eq!(stub_argv_log(&root), ["rice list --json"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// GATED on Unix with its reason (see [`stub_rice_bin`]).
    #[cfg(unix)]
    #[test]
    fn dispatch_rice_menu_with_no_rice_answers_no_drafts_after_one_child() {
        let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, root) = stub_rice_bin("shellbridge-menu-none", r#"{"mode":"staging"}"#);
        let reply = dispatch_rice_menu();
        assert_eq!(reply["ok"], true, "{reply}");
        assert_eq!(reply["draftsRice"], Value::Null);
        assert_eq!(reply["drafts"], json!([]));
        assert_eq!(reply["rices"].as_array().map(Vec::len), Some(2));
        assert_eq!(stub_argv_log(&root), ["rice list --json"], "no rice means no drafts child");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// GATED on Unix with its reason (see [`stub_rice_bin`]).
    #[cfg(unix)]
    #[test]
    fn dispatch_rice_mode_names_the_rice_to_stage_and_none_for_declarative() {
        let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, root) = stub_rice_bin("shellbridge-mode", r#"{"mode":"staging","rice":"sonata"}"#);
        assert_eq!(
            dispatch_rice_mode(&RiceModeAction::Stage { name: "fugue".to_string() }),
            Ok("staged".to_string())
        );
        assert_eq!(dispatch_rice_mode(&RiceModeAction::Declarative), Ok("staged".to_string()));
        assert_eq!(
            stub_argv_log(&root),
            ["rice mode stage fugue --json", "rice mode declarative --json"]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// `notify` hands `notify-send` a `--` before the positionals and a body
    /// with markup escaped, so a hostile folder name is neither an option nor
    /// markup. A stub `notify-send` on PATH logs its argv; the real one is
    /// never reached.
    ///
    /// GATED on Unix with its reason: the stub is a `#!/bin/sh` script.
    #[cfg(unix)]
    #[test]
    fn notify_ends_options_and_escapes_the_body() {
        let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let root = aoide_test_support::unique_tmp("shellbridge-notify");
        std::fs::create_dir_all(&root).unwrap();
        let stub = root.join("notify-send");
        let log = root.join("argv.log");
        std::fs::write(
            &stub,
            format!("#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\"; done > '{}'\n", log.display()),
        )
        .unwrap();
        std::fs::set_permissions(&stub, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let _env = aoide_test_support::EnvSaver::capture(&["PATH"]);
        std::env::set_var("PATH", &root);

        notify("-x <b>a&b</b>");
        for _ in 0..200 {
            if std::fs::read_to_string(&log).map(|t| t.lines().count() >= 3).unwrap_or(false) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        let argv: Vec<String> = std::fs::read_to_string(&log).unwrap().lines().map(str::to_string).collect();
        assert_eq!(argv, ["--", "Aoide", "-x &lt;b&gt;a&amp;b&lt;/b&gt;"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The menu over a REAL socket: `ricemenu` is answered with one line
    /// on its own connection and the audit log never learns a rice or draft
    /// name; a `ricedraft` line the gate refuses changes nothing and gets NO
    /// reply — it is fire-and-forget, so nothing is parked on one — beyond
    /// the `unparseable` byte count. (No valid `ricedraft` is sent: it would
    /// toast through the real `notify-send`.)
    ///
    /// GATED on Unix with its reason (see [`stub_rice_bin`]).
    #[cfg(unix)]
    #[test]
    fn ricemenu_is_answered_over_the_socket_and_a_refused_ricedraft_is_not() {
        let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_stub_env, root) = stub_rice_bin(
            "shellbridge-menu-sock",
            r#"{"mode":"staging","rice":"quietsong"}"#,
        );
        let _env = aoide_test_support::EnvSaver::capture(&["AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_AUDIT_LOG", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR"]);
        std::env::set_var("AOIDE_ROOT", &root);
        std::env::set_var("STRILLIX_ROOT", &root);
        std::env::set_var("AOIDE_AUDIT_LOG", root.join("log"));
        std::env::set_var("AOIDE_STATE_DIR", &root);
        std::env::set_var("XDG_RUNTIME_DIR", &root);

        let sock = short_sock("menu");
        let listener = UnixListener::bind(&sock).unwrap();
        std::thread::spawn(move || serve(&listener, &SOCKET)); // not joined

        use std::io::Write;
        let mut refused = UnixStream::connect(&sock).unwrap();
        refused.write_all(b"{\"cmd\":\"ricedraft\",\"action\":\"enter\"}\n").unwrap();
        refused.flush().unwrap();
        refused.set_read_timeout(Some(std::time::Duration::from_millis(400))).unwrap();
        let mut silence = String::new();
        let heard = BufReader::new(&refused).read_line(&mut silence);
        assert!(
            heard.is_err() || silence.is_empty(),
            "a refused ricedraft is never answered: {silence:?}"
        );

        let mut client = UnixStream::connect(&sock).unwrap();
        client.write_all(b"{\"cmd\":\"ricemenu\"}\n").unwrap();
        client.flush().unwrap();
        client.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
        let mut answer = String::new();
        BufReader::new(client).read_line(&mut answer).unwrap();
        let reply: Value = serde_json::from_str(answer.trim()).expect("one JSON reply line");
        assert_eq!(reply["ok"], true, "{reply}");
        assert_eq!(reply["draftsRice"], "quietsong");
        assert_eq!(reply["rices"].as_array().map(Vec::len), Some(2), "{reply}");
        assert_eq!(reply["drafts"].as_array().map(Vec::len), Some(2), "{reply}");

        let log = root.join("log");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut text = String::new();
        while std::time::Instant::now() < deadline {
            text = std::fs::read_to_string(&log).unwrap_or_default();
            if text.contains("unparseable") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(text.contains("unparseable"), "{text}");
        assert!(!text.contains("quietsong") && !text.contains("draft-"), "no names in the audit: {text}");

        let _ = std::fs::remove_dir_all(&root);
    }
}
