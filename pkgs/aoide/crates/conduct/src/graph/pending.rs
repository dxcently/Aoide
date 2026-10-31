//! `session pending list|approve|deny` — the pending queue's OTHER half.
//!
//! `state/stage/pending.json` is where [`super::send::session_send`] parks a
//! held injection when none of the gate's delivering rules clears it. A door
//! that cannot approve (`Door::can_approve`) is cleared only by the attested
//! parent and sibling rules, and the A2A door has no attested sender at all, so
//! it parks every inject it admits and its spawn's opening turn
//! (`crates/server/src/a2a.rs`). An MCP send parks unless its server runs
//! under a sealed session that has a rule for the target, and a whole MCP spawn
//! parks when the server has no sealed session, and so does a whole MCP
//! `resurrect` (`PendingSend::spawn`: approving one launches it, as the
//! approver's door, with the command, argv and flags it was held with). Until
//! this module,
//! nothing in the tree ever read that file back: it was a write-only dead
//! drop, holding entries forever, invisibly. This is the read/approve/deny
//! surface every other surface (TUI, QML, web) can render later — CLI-only,
//! no TUI/QML/auto-approval/allowlist here (YAGNI; those are later phases).
//!
//! `approve` does NOT open a second injection path. It re-synthesizes the
//! exact `aoide send --id <id> --yes -- <text>` invocation the held
//! entry represents and calls [`super::send::session_send`] directly,
//! in-process — the SAME one gated injection door `session permit`'s
//! `type_verdict` already goes through (`permit.rs`). `deny` never calls it
//! at all.
//!
//! Resolving either way REMOVES the entry from `pending.json` (under the
//! stage lock) rather than flipping a persisted "resolved" flag — the
//! resolution record is the audit line (`session.pending.approve` /
//! `session.pending.deny`), exactly where every other outcome in this door
//! already lives. Nothing here keeps a resolved-entry archive nobody asked
//! for.
//!
//! Addressing: `pending.json`'s `PendingSend` carries no id of its own (only
//! `sessionId`/`text`/`submit`/`queuedAt`/`from`/`claimed`/`spawn` — see
//! [`super::send::PendingSend`]), so `list`'s `id` is the entry's POSITION in
//! the array. Positions shift the
//! moment any entry resolves, so `approve`/`deny` always re-read the file
//! fresh under the lock rather than trusting a stale list; a caller scripting
//! multiple resolutions in one breath should re-list between them.
//!
//! Malformed entries (the schema's real-world failure mode: a stale
//! hand-edited or half-written array element — a bare string instead of an
//! object, or an object with no `sessionId`) are listed best-effort rather
//! than losing the whole queue to one bad line, and `approve`/`deny` on one
//! fail cleanly (a structured error, never a panic) without touching it —
//! the entry stays exactly where it was for a human to inspect by hand.

use super::common::{clean_line, refuse_unapproving_door, require_args, stage_error, visible, visible_held};
use super::doc::is_conductable_now;
use super::model::{
    canonical_state, load_stage, resolved_parent, sessions_path, write_stage, SessionRecord,
    SessionsFile,
};
use super::send::{expires_stamp, is_expired, pending_path, PendingSpawn, DOOR_CANNOT_APPROVE};
use aoide_protocol::output::{Fix, Kind, Outcome, Status};
use aoide_protocol::Invocation;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};

/// Is this raw `pending` array element too broken to resolve? The one field
/// every entry [`super::send::record_pending`] ever wrote is a non-empty
/// `sessionId` — anything without one (not an object at all, or an object
/// missing it) cannot be addressed to a session, approved, or denied.
fn is_malformed(v: &Value) -> bool {
    let named = v
        .as_object()
        .and_then(|o| o.get("sessionId"))
        .and_then(Value::as_str)
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    // A `spawn` that does not parse, or names a command no approval launches,
    // must not fall back to being read as a send: approving it would type its
    // command line into a session.
    let spawn_unreadable = v
        .get("spawn")
        .is_some_and(|s| serde_json::from_value::<PendingSpawn>(s.clone()).map_or(true, |p| !p.is_known()));
    !named || spawn_unreadable
}

/// The spawn a held entry is, or `None` for the ordinary held send.
fn spawn_of(v: &Value) -> Option<PendingSpawn> {
    serde_json::from_value(v.get("spawn")?.clone()).ok()
}

/// Read `pending.json`'s `pending` array as raw JSON — not the strict
/// [`super::send::PendingFile`] — so ONE malformed element can't fail
/// deserializing the whole `Vec<PendingSend>` and blind `list` to every
/// entry after it. A missing file is an empty queue (same tolerance as every
/// other stage file); a corrupt (unparseable) `pending.json` itself is still
/// a clean error — that is a broken FILE, not a broken entry.
fn load_pending_array() -> Result<Vec<Value>, String> {
    let raw: Value = load_stage(&pending_path())?;
    Ok(raw
        .get("pending")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

/// Render a session id for the human line via the canonical display grammar
/// (petnames plan P3): `<host>/<role>/<petname> (…<tail4>)`, degrading to
/// `<host>/<role>/<sessionId>` for a legacy/petname-less record. When the id
/// no longer resolves to any record in the current roster (the target was
/// pruned/reaped since the entry was queued), falls back to the bare raw id
/// — there is no record left to derive a role from, and a line inventing one
/// would be a lie.
fn grammar_label(
    id: &str,
    by_id: &BTreeMap<&str, &SessionRecord>,
    ids: &HashSet<&str>,
    host: &str,
) -> String {
    match by_id.get(id) {
        Some(rec) => {
            let role = if resolved_parent(rec, ids).is_some() { "child" } else { "root" };
            aoide_storage::display::session_label(rec, host, role)
        }
        None => id.to_string(),
    }
}

/// Render a sender id for the human line's TERSE `(from …)` tag — petname+
/// tail only (never host/role — matches `send.rs`'s composer-prefix grammar,
/// not the fuller tree-line one `grammar_label` above renders), falling back
/// to the raw sender id when it does not resolve to a live petnamed record.
/// An MCP source (`mcp:<session>`) keeps its door in front of the session's
/// name.
fn from_label(id: &str, by_id: &BTreeMap<&str, &SessionRecord>) -> String {
    if let Some(session) = id.strip_prefix("mcp:") {
        return format!("mcp:{}", from_label(session, by_id));
    }
    match by_id.get(id).and_then(|r| r.petname.as_deref()) {
        Some(petname) => format!("{petname} (…{})", aoide_storage::display::short_tail(id)),
        None => id.to_string(),
    }
}

/// A hold whose target has no record this long after it was queued is gone.
const GONE_ABSENT_SECS: i64 = 4 * 3_600;

/// Why a hold's target can never take it, or `None` while it still might. A
/// `done` record is a finished session. No record at all, once `GONE_ABSENT_SECS`
/// have passed since the hold was queued, is treated as gone: the hold is refused,
/// not held forever.
fn gone_reason(record: Option<&SessionRecord>, queued_at: &str, now: i64) -> Option<&'static str> {
    match record {
        Some(r) if canonical_state(&r.state) == "done" => Some("its session is done"),
        Some(_) => None,
        None => {
            let queued = aoide_storage::time::parse_iso_utc(queued_at)?;
            (now >= queued + GONE_ABSENT_SECS).then_some("its session has no record")
        }
    }
}

/// How long ago a hold was queued, in the largest whole unit it fills. `None`
/// when `queuedAt` does not parse: there is no age to show.
fn queued_ago(queued_at: &str, now: i64) -> Option<String> {
    let secs = (now - aoide_storage::time::parse_iso_utc(queued_at)?).max(0);
    Some(match secs {
        s if s >= 86_400 => format!("{} d", s / 86_400),
        s if s >= 3_600 => format!("{} h", s / 3_600),
        s if s >= 60 => format!("{} min", s / 60),
        s => format!("{s} s"),
    })
}

/// A hold's state, as `list` and every resolution refusal read it: `expired`
/// past seven days, else `gone` with its reason when its target can no longer
/// take it, else `pending`. A held spawn has no target until it is approved, so
/// it is never `gone`.
fn hold_state(v: &Value, record: Option<&SessionRecord>, now: i64) -> (&'static str, Option<&'static str>) {
    let queued = v.get("queuedAt").and_then(Value::as_str).unwrap_or("");
    let gone = if spawn_of(v).is_none() { gone_reason(record, queued, now) } else { None };
    if is_expired(queued, now) {
        ("expired", None)
    } else if gone.is_some() {
        ("gone", gone)
    } else {
        ("pending", None)
    }
}

/// One JSON view of a pending entry for `list`'s `data.pending`, and the text
/// line for its human render. `from` is the source the door derived at queue
/// time (see [`super::send::PendingSend`]) — `None`/absent for an unattributed
/// send AND for a LEGACY entry written before this field existed (serde
/// default on read). `claimed` is the label an MCP caller typed, cleaned
/// again here and rendered under its own name, never as the source.
///
/// `by_id`/`ids`/`host` are the CURRENT session roster, loaded once by the
/// caller ([`pending_list`]) and threaded through every entry — the JSON
/// `sessionId`/`from` stay the entry's own canonical raw ids verbatim
/// (machine contract unchanged); only the human LINE renders through the
/// display grammar.
fn entry_view(
    index: usize,
    v: &Value,
    by_id: &BTreeMap<&str, &SessionRecord>,
    ids: &HashSet<&str>,
    host: &str,
    now: i64,
) -> (Value, String) {
    let malformed = is_malformed(v);
    if malformed {
        let json = json!({
            "id": index.to_string(),
            "sessionId": "",
            "text": v.to_string(),
            "submit": false,
            "queuedAt": "",
            "from": Value::Null,
            "claimed": Value::Null,
            "state": "malformed",
        });
        let line = format!("[{index}] <malformed entry — cannot resolve; edit state/stage/pending.json by hand>");
        return (json, line);
    }
    let o = v.as_object().expect("checked non-malformed above");
    let session_id = o.get("sessionId").and_then(Value::as_str).unwrap_or("").to_string();
    let text = o.get("text").and_then(Value::as_str).unwrap_or("").to_string();
    let submit = o.get("submit").and_then(Value::as_bool).unwrap_or(false);
    let queued_at = o.get("queuedAt").and_then(Value::as_str).unwrap_or("").to_string();
    let from = o
        .get("from")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let claimed = o
        .get("claimed")
        .and_then(Value::as_str)
        .map(clean_line)
        .filter(|s| !s.is_empty());
    let spawn = spawn_of(v);
    // A held spawn is shown from what approving it replays, whole: an approver
    // who saw a clipped command line would be approving the rest blind. The
    // data carries it as it replays; `shown` is what a human reads, spelled
    // once here so every surface reads the same escapes.
    let text = spawn.as_ref().map(PendingSpawn::command_line).unwrap_or(text);
    let shown = if spawn.is_some() { visible_held(&text) } else { visible(&text) };
    let plan: Vec<String> = spawn.as_ref().map(|s| s.shown.iter().map(|l| visible_held(l)).collect()).unwrap_or_default();
    let (state, gone) = hold_state(v, by_id.get(session_id.as_str()).copied(), now);
    let expired = state == "expired";
    let mut json = json!({
        "id": index.to_string(),
        "fingerprint": fingerprint(v),
        "sessionId": session_id,
        "text": text,
        "shown": shown,
        "submit": submit,
        "queuedAt": queued_at,
        "expires": expires_stamp(&queued_at),
        "from": from,
        "claimed": claimed,
        "state": state,
    });
    if let Some(spawn) = &spawn {
        json["spawn"] = json!(spawn);
        json["plan"] = json!(plan);
    }
    let what = match spawn {
        Some(_) => format!("spawn {} ← {shown}", visible(&session_id)),
        None => format!("{} ← {shown}", visible(&grammar_label(&session_id, by_id, ids, host))),
    };
    let words = match gone {
        Some(why) => format!("gone ({why})"),
        None => state.to_string(),
    };
    let since = match queued_ago(&queued_at, now) {
        Some(ago) => format!("{words}, queued {ago} ago"),
        None => format!("{words}, queued at no readable time"),
    };
    // A lapsed or gone hold offers one resolution, deny: approving it is refused.
    let hint = if expired || gone.is_some() {
        format!("; deny it: aoide session pending deny {index} --expect {}", fingerprint(v))
    } else {
        String::new()
    };
    let line = format!(
        "[{index}] {since}: {what}{}{}{} ({}){hint}",
        if submit { " [submit]" } else { "" },
        from.as_deref()
            .map(|f| format!(" (from {})", visible(&from_label(f, by_id))))
            .unwrap_or_default(),
        claimed.as_deref().map(|c| format!(" (claims {c:?})")).unwrap_or_default(),
        visible(&queued_at),
    );
    (json, line)
}

/// `session pending list [--json]` — enumerate every held entry in
/// `pending.json`. Never errors on an empty or missing queue.
pub fn pending_list(_inv: &Invocation) -> Outcome {
    let cmd = "session.pending.list";
    let arr = match load_pending_array() {
        Ok(a) => a,
        Err(e) => return stage_error(cmd, e),
    };
    // The current session roster, loaded ONCE for this whole list — every
    // entry's human line renders its target/sender through the SAME
    // snapshot (and the SAME once-resolved host), matching the render
    // pass's "host resolved once per render" rule in `doc.rs`.
    let sessions: Vec<SessionRecord> = load_stage::<SessionsFile>(&sessions_path())
        .map(|f| f.sessions)
        .unwrap_or_default();
    let by_id: BTreeMap<&str, &SessionRecord> =
        sessions.iter().map(|s| (s.session_id.as_str(), s)).collect();
    let ids: HashSet<&str> = sessions.iter().map(|s| s.session_id.as_str()).collect();
    let host = aoide_storage::display::local_host_name();
    let now = super::conduct::unix_ts() as i64;

    let mut data = Vec::with_capacity(arr.len());
    let mut lines = Vec::with_capacity(arr.len());
    for (i, v) in arr.iter().enumerate() {
        let (j, l) = entry_view(i, v, &by_id, &ids, &host, now);
        data.push(j);
        lines.push(l);
    }
    let n = arr.len();
    let body = if n == 0 {
        "state/stage/pending.json is empty".to_string()
    } else {
        lines.join("\n")
    };
    Outcome::ok(cmd, format!("{n} pending\n{body}")).with_data(json!({ "pending": data }))
}

/// Is `id` a session that `session_send` can type into right now? The same
/// test its own not-conductable refusal makes (`is_conductable_now`), read off
/// the roster as it stands. A session that has not registered is absent.
fn target_is_live(id: &str) -> bool {
    load_stage::<SessionsFile>(&sessions_path())
        .map(|f| f.sessions.iter().any(|s| s.session_id == id && is_conductable_now(s)))
        .unwrap_or(false)
}

/// A short fingerprint of one raw entry: the content an approver read, named by
/// `list --json` and matched by `--expect`. It binds a choice to the entry, not
/// to the position the entry held when it was listed.
fn fingerprint(v: &Value) -> String {
    let body = serde_json::to_string(v).unwrap_or_default();
    aoide_storage::seal::hex_encode(&aoide_storage::seal::sha256(body.as_bytes()))[..16].to_string()
}

/// Remove and return the raw entry at `index`, under the stage lock — but
/// ONLY when it is the entry `observed` was read from, and resolves to a real
/// session (see [`is_malformed`]). The checks a resolution ran on `observed`
/// therefore cover the entry it removes, and a caller that named one with
/// `expect` (its fingerprint) takes nothing else. A broken entry is left exactly
/// where it was rather than destroyed by a failed resolve attempt. A missing
/// file or an out-of-range index is the same clean "not found" error.
fn take_pending_entry(index: usize, observed: &Value, expect: Option<&str>) -> Result<Value, String> {
    aoide_storage::fs::with_stage_lock(move || {
        let mut raw: Value = load_stage(&pending_path())?;
        let arr = raw
            .get_mut("pending")
            .and_then(Value::as_array_mut)
            .filter(|a| index < a.len())
            .ok_or_else(|| format!("no pending entry at index {index}"))?;
        if is_malformed(&arr[index]) {
            return Err(format!(
                "pending entry {index} is malformed — cannot resolve automatically; edit state/stage/pending.json by hand"
            ));
        }
        let fp = fingerprint(&arr[index]);
        if fp != fingerprint(observed) {
            return Err(format!(
                "pending entry {index} changed since it was read — nothing was resolved; list the queue again"
            ));
        }
        if expect.is_some_and(|e| e != fp) {
            return Err(format!(
                "pending entry {index} is not the one named by --expect {} — nothing was resolved; list the queue again",
                expect.unwrap_or_default()
            ));
        }
        let removed = arr.remove(index);
        write_stage(&pending_path(), &raw)?;
        Ok(removed)
    })
}

/// The `--expect <fingerprint>` a caller names to bind a resolution to the entry
/// it read, if it named one.
fn expect_flag(inv: &Invocation) -> Option<String> {
    inv.flags.get("expect").map(|e| e.trim().to_string()).filter(|e| !e.is_empty())
}

/// A blank `--expect` names no entry, and reading it as no bind would let the
/// take resolve by position alone. Refused before the take, so nothing moves.
fn refuse_empty_expect(inv: &Invocation, cmd: &str, index: usize) -> Option<Outcome> {
    if !inv.flags.get("expect").is_some_and(|e| e.trim().is_empty()) {
        return None;
    }
    audit_pending(inv, cmd, "refused", &format!("{cmd} [{index}]: --expect is empty"), "");
    Some(
        Outcome::refuse(
            cmd,
            Kind::Refused,
            format!("pending entry {index} not resolved: --expect is empty"),
            "the fingerprint is empty; list the queue again and copy it",
            Fix::Run("aoide session pending list".to_string()),
        )
        .with_fields(json!({ "reason": "expect-empty", "id": index.to_string() })),
    )
}

/// How much of an entry's text a refusal shows, in characters.
const SHOWN_HEAD_CHARS: usize = 60;

/// The command that resolves the entry at `index` by its fingerprint: what a
/// refusal tells the person to run. With no readable entry there is nothing to
/// name, so it points at the list.
fn resolve_command(verb: &str, index: usize, entry: Option<&Value>) -> String {
    match entry.filter(|v| !is_malformed(v)) {
        Some(v) => format!("aoide session pending {verb} {index} --expect {}", fingerprint(v)),
        None => "aoide session pending list".to_string(),
    }
}

/// The raw entry at `index` as the queue holds it now.
fn entry_at(index: usize) -> Option<Value> {
    load_pending_array().ok().and_then(|a| a.get(index).cloned())
}

/// `approve` and `deny` name an entry by position, and a position moves: an
/// eviction, or a resolution ahead of the entry, shifts what an index names. So
/// a bare index resolves nothing. The refusal prints the entry the index names
/// and the command that names it by fingerprint; nothing is taken or launched.
fn refuse_bare_index(inv: &Invocation, cmd: &str, verb: &str, index: usize, observed: &Value) -> Option<Outcome> {
    if inv.flags.contains_key("expect") || is_malformed(observed) {
        return None;
    }
    let now = super::conduct::unix_ts() as i64;
    let sid = observed.get("sessionId").and_then(Value::as_str).unwrap_or("");
    let queued = observed.get("queuedAt").and_then(Value::as_str).unwrap_or("");
    let roster: Vec<SessionRecord> = load_stage::<SessionsFile>(&sessions_path())
        .map(|f| f.sessions)
        .unwrap_or_default();
    let (hold, gone) = hold_state(observed, roster.iter().find(|r| r.session_id == sid), now);
    // Approve refuses a lapsed hold before any take, so the command it names is deny.
    let fix_verb = if verb == "approve" && hold != "pending" { "deny" } else { verb };
    let state = match gone {
        Some(why) => format!("{hold} ({why})"),
        None => hold.to_string(),
    };
    let spawn = spawn_of(observed);
    let target = match spawn {
        Some(_) => format!("spawn {}", visible(sid)),
        None => visible(sid),
    };
    let sender = observed
        .get("from")
        .and_then(Value::as_str)
        .filter(|f| !f.is_empty())
        .map_or_else(|| "unattributed".to_string(), visible);
    let age = queued_ago(queued, now).unwrap_or_else(|| "no readable queue time".to_string());
    let text = match &spawn {
        Some(spawn) => spawn.command_line(),
        None => observed.get("text").and_then(Value::as_str).unwrap_or("").to_string(),
    };
    // A line break is a space to the reader; `clean_line` would delete it and join the words.
    let flat: String = text.chars().map(|c| if c.is_whitespace() { ' ' } else { c }).collect();
    let clean = clean_line(&flat);
    let head = if clean.chars().count() > SHOWN_HEAD_CHARS {
        format!("{}...", clean.chars().take(SHOWN_HEAD_CHARS).collect::<String>())
    } else {
        clean
    };
    audit_pending(inv, cmd, "refused", &format!("{verb} [{index}]: no --expect"), "");
    Some(
        Outcome::refuse(
            cmd,
            Kind::Refused,
            format!(
                "pending entry {index} not resolved: {verb} needs --expect\n  state:  {state}\n  target: {target}\n  sender: {sender}\n  age:    {age}\n  text:   {head}"
            ),
            "a position moves when an entry ahead of it resolves or expires, so --expect names the entry you read",
            Fix::Run(resolve_command(fix_verb, index, Some(observed))),
        )
        .with_fields(json!({
            "reason": "expect-required",
            "id": index.to_string(),
            "fingerprint": fingerprint(observed),
        })),
    )
}

/// One audit line per pending-resolution outcome, same shape as
/// [`super::send::session_send`]'s own `audit_send` — the injected/dropped
/// text rides as `untrusted_data`, never re-interpreted as a command.
pub(super) fn audit_pending(inv: &Invocation, command: &str, status: &str, message: &str, text: &str) {
    let log = inv
        .flags
        .get("audit-log")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(aoide_protocol::default_audit_log);
    let _ = aoide_protocol::append_audit(
        &log,
        &aoide_protocol::AuditRecord {
            ts: super::conduct::unix_ts(),
            door: inv.door,
            class: aoide_protocol::EventClass::Audit,
            command: command.to_string(),
            status: status.to_string(),
            message: message.to_string(),
            untrusted_data: Some(super::send::audited_text(status, text)),
        },
    );
}

/// Parse the `<id>` positional as a `pending list` index; a clean usage error
/// (never a panic) for anything else.
fn parse_id(cmd: &str, raw: &str) -> Result<usize, Outcome> {
    raw.trim().parse::<usize>().map_err(|_| {
        Outcome::usage(
            cmd,
            format!(
                "`{raw}` is not a pending id — ids are the position shown by `session pending list` (e.g. 0)"
            ),
        )
        .with_data(json!({ "reason": "bad-id", "id": raw }))
    })
}

/// `session pending approve <id> --expect <fingerprint> [--json]` — approve one
/// held entry: re-drive it through [`super::send::session_send`] with `--yes`
/// (the one injection door, in-process, no shell-out — see the module doc),
/// then remove it from the queue. The entry is taken OFF the queue before
/// injection is attempted (matching `send`'s own "every outcome is audited,
/// nothing pending forever" posture) — if delivery then fails (the target
/// session died while queued, say), that failure is reported plainly rather
/// than silently re-queued; the human re-sends by hand if the target is still
/// reachable. A target that is not live yet is refused before the take, so an
/// entry held for a session still registering is not lost to the failed
/// delivery.
pub fn pending_approve(inv: &Invocation) -> Outcome {
    let cmd = "session.pending.approve";
    let args = match require_args(inv, &["id"]) {
        Ok(a) => a,
        Err(o) => return o,
    };
    let index = match parse_id(cmd, &args[0]) {
        Ok(i) => i,
        Err(o) => return o,
    };
    // Refused before the entry leaves the queue: a caller on a door that
    // cannot approve gets a taught refusal and the held send stays held.
    if let Some(refusal) = refuse_unapproving_door(
        inv,
        cmd,
        "this door cannot approve a gated send".to_string(),
        DOOR_CANNOT_APPROVE,
        Fix::Run(resolve_command("approve", index, entry_at(index).as_ref())),
    ) {
        return refusal.with_fields(json!({ "id": index.to_string() }));
    }
    if let Some(refusal) = refuse_empty_expect(inv, cmd, index) {
        return refusal;
    }
    // Refused before the entry leaves the queue while its target is not live.
    // An A2A spawn's opening turn can be held before its child has registered,
    // and `session_send` refuses a target with no socket: taking the entry first
    // would lose it. The entry stays put for a later approve, or a deny. A held
    // spawn has no target yet: approving it is what creates one.
    let Some(observed) = load_pending_array().ok().and_then(|a| a.get(index).cloned()).filter(|v| !is_malformed(v)) else {
        // Missing or malformed: the take names which, and removes nothing.
        let e = take_pending_entry(index, &Value::Null, None).err().unwrap_or_default();
        audit_pending(inv, cmd, "error", &e, "");
        return Outcome::error(cmd, e)
            .with_data(json!({ "reason": "pending-entry-unresolvable", "id": index.to_string() }));
    };
    if let Some(refusal) = refuse_bare_index(inv, cmd, "approve", index, &observed) {
        return refusal;
    }
    // A lapsed or gone hold is refused before the entry leaves the queue. The list
    // offers only deny for either, and a deny removes it.
    let now = super::conduct::unix_ts() as i64;
    let queued = observed.get("queuedAt").and_then(Value::as_str).unwrap_or("");
    if is_expired(queued, now) {
        audit_pending(inv, cmd, "refused", &format!("approve [{index}]: expired (queued {queued})"), "");
        return Outcome::refuse(
            cmd,
            Kind::Refused,
            format!("pending entry {index} expired (queued {})", visible(queued)),
            "deny it, or ask again",
            Fix::Run(resolve_command("deny", index, Some(&observed))),
        )
        .with_fields(json!({ "reason": "expired", "id": index.to_string() }));
    }
    // The roster is read once for the gone check here and the registered wording
    // at the target check below.
    let roster: Vec<SessionRecord> = load_stage::<SessionsFile>(&sessions_path())
        .map(|f| f.sessions)
        .unwrap_or_default();
    let target_sid = observed.get("sessionId").and_then(Value::as_str).unwrap_or("");
    let gone = if spawn_of(&observed).is_none() {
        gone_reason(roster.iter().find(|s| s.session_id == target_sid), queued, now)
    } else {
        None
    };
    if let Some(why) = gone {
        audit_pending(inv, cmd, "refused", &format!("approve [{index}] {target_sid}: gone ({why})"), "");
        return Outcome::refuse(
            cmd,
            Kind::Refused,
            format!("pending entry {index} is gone: {why}"),
            "deny it: its target can no longer take it",
            Fix::Run(resolve_command("deny", index, Some(&observed))),
        )
        .with_fields(json!({ "reason": "gone", "id": index.to_string(), "sessionId": target_sid }));
    }
    let spawn_id = spawn_of(&observed).filter(|s| !s.is_resurrect()).and_then(|s| s.flags.get("id").cloned());
    if let Some(id) = spawn_id {
        // The id was free when the spawn was held. Launching binds its socket,
        // which unlinks what is there, so a session that has taken the id since
        // keeps it and the entry stays queued for the approver to deny.
        if super::spawn::spawn_id_in_use(&id) {
            audit_pending(inv, cmd, "refused", &format!("approve [{index}] spawn {id}: id in use"), "");
            return Outcome::refuse(
                cmd,
                Kind::Refused,
                format!("`{}` names a session that is live", visible(&id)),
                "launching would take over that session's record and control socket",
                Fix::Run(resolve_command("deny", index, Some(&observed))),
            )
            .with_fields(json!({ "reason": "id-in-use", "id": index.to_string(), "sessionId": id }));
        }
    }
    // A held resurrect launches only the selection its approver was shown. A
    // plan that moved is refused here, before the entry leaves the queue.
    let resurrect_selection = match spawn_of(&observed).filter(PendingSpawn::is_resurrect) {
        Some(spawn) => {
            let launch = Invocation { path: spawn.path, args: spawn.args, flags: spawn.flags, door: inv.door };
            match super::resurrect::approved_selection(&launch, &spawn.shown) {
                Ok(selection) => Some(selection),
                Err(refusal) => {
                    audit_pending(inv, cmd, "refused", &refusal.message, "");
                    return refusal.with_fields(json!({ "id": index.to_string() }));
                }
            }
        }
        None => None,
    };
    let target = Some(&observed)
        .filter(|v| spawn_of(v).is_none())
        .and_then(|v| v.get("sessionId").and_then(Value::as_str).map(str::to_string));
    if let Some(sid) = target.filter(|sid| !target_is_live(sid)) {
        // A session with a record has registered: it is told when it can be typed
        // into, never to register again.
        let (what, why) = if roster.iter().any(|s| s.session_id == sid) {
            (
                format!("`{}` is registered but cannot be typed into now", visible(&sid)),
                "approve again once it can be typed into, or deny it",
            )
        } else {
            (
                format!("`{}` is not live (not registered yet, or ended)", visible(&sid)),
                "approve again once it is live, or deny",
            )
        };
        audit_pending(inv, cmd, "refused", &format!("approve [{index}] {sid}: target not live"), "");
        return Outcome::refuse(
            cmd,
            Kind::Refused,
            what,
            why,
            Fix::Run(resolve_command("approve", index, Some(&observed))),
        )
        .with_fields(json!({ "reason": "target-not-live", "id": index.to_string(), "sessionId": sid }));
    }
    let removed = match take_pending_entry(index, &observed, expect_flag(inv).as_deref()) {
        Ok(v) => v,
        Err(e) => {
            audit_pending(inv, cmd, "error", &e, "");
            return Outcome::error(cmd, e)
                .with_data(json!({ "reason": "pending-entry-unresolvable", "id": index.to_string() }));
        }
    };
    let o = removed.as_object().expect("take_pending_entry only returns well-formed entries");
    let session_id = o.get("sessionId").and_then(Value::as_str).unwrap_or("").to_string();
    if let Some(spawn) = spawn_of(&removed) {
        return approve_spawn(inv, index, &session_id, spawn, resurrect_selection);
    }
    let text = o.get("text").and_then(Value::as_str).unwrap_or("").to_string();
    let submit = o.get("submit").and_then(Value::as_bool).unwrap_or(false);
    let from = o
        .get("from")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let mut flags = BTreeMap::new();
    flags.insert("id".to_string(), session_id.clone());
    flags.insert("yes".to_string(), "true".to_string());
    if submit {
        flags.insert("submit".to_string(), "true".to_string());
    }
    // Carry the ORIGINAL entry's attribution through as `--from` — ALWAYS,
    // even when the entry had none. `resolve_sender`'s tri-state rule makes
    // this the only correct move: a PRESENT `--from` (the entry's sender, or
    // `""` when it had none) is used as-is and the env is never consulted;
    // an ABSENT `--from` would instead fall through to the approver's own
    // live `AOIDE_SESSION_ID` (an orchestrating conducted session always has
    // one) and misattribute an anonymously-queued send to whoever happened
    // to approve it. `--from ""` keeps that entry anonymous through
    // delivery, exactly as it was queued.
    flags.insert("from".to_string(), from.clone().unwrap_or_default());
    if let Some(log) = inv.flags.get("audit-log") {
        flags.insert("audit-log".to_string(), log.clone());
    }
    // The one injection door, in-process — same pattern as
    // `permit.rs::type_verdict`. `args: vec![text]` (a single element) round
    // -trips the exact original text through `session_send`'s own
    // `inv.args.join(" ")`, whitespace and all.
    let operator = o.get("operator").and_then(Value::as_bool).unwrap_or(false);
    let inner = super::send::session_send_held(
        &Invocation {
            path: vec!["send".to_string()],
            args: vec![text.clone()],
            flags,
            door: inv.door,
        },
        &session_id,
        operator,
    );

    let audit_status = if inner.status == Status::Ok { "delivered" } else { "error" };
    audit_pending(
        inv,
        cmd,
        audit_status,
        &format!("approve [{index}] {session_id}: {}", inner.message),
        &text,
    );

    if inner.status != Status::Ok {
        return Outcome::error(
            cmd,
            format!(
                "pending entry {index} left the queue but delivery to `{}` failed: {}",
                visible(&session_id),
                inner.message
            ),
        )
        .with_data(json!({
            "id": index.to_string(),
            "sessionId": session_id,
            "injected": false,
            "reason": "delivery-failed",
            "sendOutcome": inner.data,
        }));
    }
    Outcome::ok(cmd, format!("pending entry {index} approved → delivered to `{}`", visible(&session_id)))
        .changed(vec![format!("pending[{index}] for {}: approved and injected", visible(&session_id))])
        .with_data(json!({
            "id": index.to_string(),
            "sessionId": session_id,
            "injected": true,
            "sendOutcome": inner.data,
        }))
}

/// Approve a held spawn: launch it, as the approver's door, with the argv and
/// flags it was held with. Like a held send it has left the queue before the
/// launch is attempted, and a launch that fails is reported plainly.
fn approve_spawn(
    inv: &Invocation,
    index: usize,
    session_id: &str,
    spawn: PendingSpawn,
    selection: Option<super::resurrect::Selection>,
) -> Outcome {
    let cmd = "session.pending.approve";
    let line = spawn.command_line();
    let resurrect = spawn.is_resurrect();
    let launch = Invocation { path: spawn.path, args: spawn.args, flags: spawn.flags, door: inv.door };
    let inner = match (resurrect, selection) {
        (true, Some(selection)) => super::resurrect::launch_selection(&launch, &selection),
        (true, None) => Outcome::error("resurrect", "a held resurrect was approved without the selection it was shown"),
        (false, _) => super::spawn::session_spawn(&launch),
    };
    let verb = if resurrect { "resurrect" } else { "spawn" };
    // A resurrect answers ok when it revived nothing (its failures sit in
    // `failed`), so "launched" for one means a session came back.
    let revived = !resurrect
        || inner
            .data
            .as_ref()
            .and_then(|d| d.get("resurrected"))
            .and_then(Value::as_array)
            .is_some_and(|rows| !rows.is_empty());
    let launched = inner.status == Status::Ok && revived;
    let status = if launched { "launched" } else { "error" };
    audit_pending(inv, cmd, status, &format!("approve [{index}] {session_id}: {}", inner.message), &line);
    if !launched {
        let why = if inner.status == Status::Ok && !revived {
            let failed: Vec<String> = inner
                .data
                .as_ref()
                .and_then(|d| d.get("failed"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|f| f.get("reason").and_then(Value::as_str).map(clean_line))
                .collect();
            if failed.is_empty() { inner.message.clone() } else { failed.join("; ") }
        } else {
            inner.message.clone()
        };
        return Outcome::error(
            cmd,
            format!(
                "pending {verb} {index} left the queue but the launch of `{}` failed: {why}",
                visible(session_id),
            ),
        )
        .with_data(json!({
            "id": index.to_string(),
            "sessionId": session_id,
            "launched": false,
            "reason": if inner.status == Status::Ok && !revived { "nothing-revived" } else { "launch-failed" },
            "spawnOutcome": inner.data,
        }));
    }
    let done = if resurrect { "resurrected" } else { "spawned" };
    Outcome::ok(cmd, format!("pending entry {index} approved → {done} `{}`", visible(session_id)))
        .changed(vec![format!("pending[{index}] for {}: approved and launched", visible(&session_id))])
        .with_data(json!({
            "id": index.to_string(),
            "sessionId": session_id,
            "launched": true,
            "spawnOutcome": inner.data,
        }))
}

/// `session pending deny <id> --expect <fingerprint> [--json]` — reject one
/// held entry: remove it from the queue, inject nothing, one audit line.
pub fn pending_deny(inv: &Invocation) -> Outcome {
    let cmd = "session.pending.deny";
    let args = match require_args(inv, &["id"]) {
        Ok(a) => a,
        Err(o) => return o,
    };
    let index = match parse_id(cmd, &args[0]) {
        Ok(i) => i,
        Err(o) => return o,
    };
    // Denying is the other half of resolving the queue, and a position is only
    // good until something resolves: a caller that could deny earlier entries
    // would shift a held spawn of its own onto the index a human just read.
    if let Some(refusal) = refuse_unapproving_door(
        inv,
        cmd,
        "this door cannot deny a held entry".to_string(),
        "entries are addressed by position and positions shift as entries resolve, so only a door that \
         can approve resolves them; approval comes from a terminal or the conductor",
        Fix::Run(resolve_command("deny", index, entry_at(index).as_ref())),
    ) {
        return refusal.with_fields(json!({ "id": index.to_string() }));
    }
    if let Some(refusal) = refuse_empty_expect(inv, cmd, index) {
        return refusal;
    }
    let observed = load_pending_array().ok().and_then(|a| a.get(index).cloned()).unwrap_or(Value::Null);
    if let Some(refusal) = refuse_bare_index(inv, cmd, "deny", index, &observed) {
        return refusal;
    }
    let removed = match take_pending_entry(index, &observed, expect_flag(inv).as_deref()) {
        Ok(v) => v,
        Err(e) => {
            audit_pending(inv, cmd, "error", &e, "");
            return Outcome::error(cmd, e)
                .with_data(json!({ "reason": "pending-entry-unresolvable", "id": index.to_string() }));
        }
    };
    let o = removed.as_object().expect("take_pending_entry only returns well-formed entries");
    let session_id = o.get("sessionId").and_then(Value::as_str).unwrap_or("").to_string();
    let text = o.get("text").and_then(Value::as_str).unwrap_or("").to_string();

    audit_pending(inv, cmd, "denied", &format!("deny [{index}] {session_id}"), &text);
    Outcome::ok(cmd, format!("pending entry {index} for `{}` denied — nothing injected", visible(&session_id)))
        .changed(vec![format!("pending[{index}] for {}: denied", visible(&session_id))])
        .with_data(json!({ "id": index.to_string(), "sessionId": session_id, "injected": false }))
}

/// The `--expect` an approver names for the entry at `index` as the queue
/// holds it now, for tests in other modules that approve what they queued.
#[cfg(test)]
pub(crate) fn expect_at(index: usize) -> BTreeMap<String, String> {
    let fp = load_pending_array().ok().and_then(|a| a.get(index).map(fingerprint)).unwrap_or_default();
    BTreeMap::from([("expect".to_string(), fp)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::conduct::conduct_socket_path;
    use crate::graph::model::{sessions_path, SessionsFile};
    use crate::graph::send::session_send;
    use crate::graph::session_store::do_session_start;
    use crate::graph::testutil::*;
    use aoide_test_support::expect_delivery;
    #[cfg(unix)]
    use std::os::unix::net::UnixListener;
    #[cfg(windows)]
    use aoide_protocol::win_unix::UnixListener;

    fn pending_invocation(path: &[&str], args: &[&str]) -> Invocation {
        Invocation {
            path: path.iter().map(|s| s.to_string()).collect(),
            args: args.iter().map(|s| s.to_string()).collect(),
            flags: BTreeMap::new(),
            door: aoide_protocol::Door::Cli,
        }
    }

    /// The invocation an approver types after reading the list: `verb` at
    /// `index`, with the fingerprint the queue holds there.
    fn named_invocation(verb: &str, index: &str) -> Invocation {
        let mut inv = pending_invocation(&["session", "pending", verb], &[index]);
        let entry = index.parse::<usize>().ok().and_then(|i| load_pending_array().ok()?.get(i).cloned());
        if let Some(entry) = entry {
            inv.flags.insert("expect".to_string(), fingerprint(&entry));
        }
        inv
    }

    fn setup(tag: &str) -> std::path::PathBuf {
        let root = unique_stage(tag);
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        std::env::set_var("AOIDE_AUDIT_LOG", root.join("log"));
        std::env::remove_var("AOIDE_CONDUCT_AUTOGATE");
        // Tests running under a REAL conducted session (this one included)
        // inherit a real AOIDE_SESSION_ID from the ambient environment; now
        // that `pending_approve`'s re-drive resolves sender attribution, an
        // unguarded env would leak that ambient id into `list`/`approve`
        // assertions. Callers that need a specific sender set it themselves
        // AFTER `setup` and must include "AOIDE_SESSION_ID" in their own
        // `EnvVars::save` so it is restored.
        std::env::remove_var("AOIDE_SESSION_ID");
        root
    }

    #[test]
    fn list_reads_real_and_malformed_entries_without_dying() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-ls");

        std::fs::write(
            pending_path(),
            r#"{"schemaVersion":"0","pending":[
                {"sessionId":"s1","text":"do the thing","submit":true,"queuedAt":"QUEUED_AT"},
                "SHOULD_NOT_RUN_UNAUTH",
                {"text":"no session id at all"}
            ]}"#
            .replace("QUEUED_AT", &stamp_ago(3_600)),
        )
        .unwrap();

        let out = pending_list(&pending_invocation(&["session", "pending", "list"], &[]));
        assert_eq!(out.status, Status::Ok, "a malformed entry must not fail the whole list");
        let data = out.data.unwrap();
        let arr = data["pending"].as_array().unwrap();
        assert_eq!(arr.len(), 3, "every entry is listed, real or not");
        assert_eq!(arr[0]["sessionId"], "s1");
        assert_eq!(arr[0]["state"], "pending");
        assert_eq!(arr[1]["state"], "malformed", "a bare string entry is flagged, not fatal");
        assert_eq!(arr[2]["state"], "malformed", "an object with no sessionId is unresolvable too");
        assert!(out.message.contains("3 pending"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn list_human_line_renders_the_display_grammar_while_the_json_stays_raw() {
        // Petnames plan P3: `list`'s human line renders the target through
        // the canonical grammar (host/role/petname/tail) and the sender
        // through the terse petname+tail tag — but `data.pending[]`'s
        // `sessionId`/`from` are the machine contract and must stay the
        // entry's raw canonical ids, untouched.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_CONDUCT_AUTOGATE",
            "AOIDE_SESSION_ID",
        ]);
        let root = setup("pnd-grammar");

        let sender_id = "grammar-sender";
        do_session_start(sender_id, Some("claude"), Some("/w"), None, None, None, None, None, None);

        let target = "grammar-target";
        let socket = conduct_socket_path(target);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let _listener = UnixListener::bind(&socket).unwrap();
        do_session_start(
            target,
            Some("claude"),
            Some("/w"),
            None,
            None,
            Some(true),
            Some(socket.to_str().unwrap()),
            None,
            None,
        );

        // Held pending: no --yes, no autogate.
        std::env::set_var("AOIDE_SESSION_ID", sender_id);
        let queued = session_send(&send_invocation(&["do", "the", "thing"], &[("id", target), ("submit", "true")]));
        assert_eq!(queued.data.as_ref().unwrap()["state"], "pending");

        // Pull the SAME petnames the mint actually chose (non-deterministic
        // roll) rather than hand-guessing a literal.
        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        let target_petname = s
            .sessions
            .iter()
            .find(|r| r.session_id == target)
            .and_then(|r| r.petname.clone())
            .expect("every minted session carries a petname (P2)");
        let sender_petname = s
            .sessions
            .iter()
            .find(|r| r.session_id == sender_id)
            .and_then(|r| r.petname.clone())
            .expect("every minted session carries a petname (P2)");
        let host = aoide_storage::display::local_host_name();

        let out = pending_list(&pending_invocation(&["session", "pending", "list"], &[]));
        assert_eq!(out.status, Status::Ok);
        let data = out.data.unwrap();
        let arr = data["pending"].as_array().unwrap();
        // The JSON contract: raw canonical ids, verbatim.
        assert_eq!(arr[0]["sessionId"], target, "JSON sessionId stays the raw canonical id");
        assert_eq!(arr[0]["from"], sender_id, "JSON from stays the raw canonical id");
        // The human line: full grammar for the target (a root — no parent),
        // terse petname+tail for the sender.
        let expected_target = format!(
            "{host}/root/{target_petname} (…{})",
            aoide_storage::display::short_tail(target)
        );
        let expected_from = format!(
            "(from {sender_petname} (…{}))",
            aoide_storage::display::short_tail(sender_id)
        );
        assert!(
            out.message.contains(&expected_target),
            "target renders via the display grammar: {}",
            out.message
        );
        assert!(
            out.message.contains(&expected_from),
            "sender renders via the terse petname+tail tag: {}",
            out.message
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A held eidolon send, approved by the person, goes as the sender it was
    /// held as: the recorded operator fact, never the approver's own process.
    /// Red on the base: approve re-drives through `session_send`, which read
    /// `AOIDE_SESSION_ID` from this process and so marked every approve operator.
    #[test]
    fn an_agent_send_held_then_approved_by_the_person_goes_as_a_peer() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_CONDUCT_AUTOGATE",
            "AOIDE_SESSION_ID",
            "PATH",
        ]);
        let root = setup("pnd-native-peer");
        let id = "apn1";
        let socket = conduct_socket_path(id);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let _listener = UnixListener::bind(&socket).unwrap();
        do_session_start(
            id,
            Some("eidolon"),
            Some("/w"),
            None,
            None,
            Some(true),
            Some(socket.to_str().unwrap()),
            None,
            None,
        );
        let bin = root.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let script = bin.join("eidolon");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{0}/record.argv'\ncat > '{0}/record.stdin'\necho 'delivered to x'\n",
                root.display()
            ),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()));

        let queued = stamp_ago(3_600);
        std::fs::write(
            pending_path(),
            format!(
                r#"{{"schemaVersion":"0","pending":[{{"sessionId":"{id}","text":"from an agent","submit":false,"queuedAt":"{queued}","from":"agent-7","operator":false}}]}}"#
            ),
        )
        .unwrap();

        let out = pending_approve(&named_invocation("approve", "0"));
        assert_eq!(out.status, Status::Ok, "msg: {}", out.message);
        let argv = std::fs::read_to_string(root.join("record.argv")).unwrap();
        assert!(!argv.contains("--operator"), "a held agent send goes as a peer when approved: {argv}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn approve_routes_through_session_send_and_removes_the_entry() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_CONDUCT_AUTOGATE",
            "AOIDE_SESSION_ID",
        ]);
        let root = setup("pnd-ap");

        let id = "apt1";
        let socket = conduct_socket_path(id);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let listener = UnixListener::bind(&socket).unwrap();
        do_session_start(
            id,
            Some("claude"),
            Some("/w"),
            None,
            None,
            Some(true),
            Some(socket.to_str().unwrap()),
            None,
            None,
        );

        let queued = stamp_ago(3_600);
        std::fs::write(
            pending_path(),
            format!(
                r#"{{"schemaVersion":"0","pending":[{{"sessionId":"{id}","text":"hello world","submit":true,"queuedAt":"{queued}"}}]}}"#
            ),
        )
        .unwrap();

        let acc = expect_delivery(listener, "an approved entry re-drives the injection door");

        let out = pending_approve(&named_invocation("approve", "0"));
        let got = acc.join().unwrap();

        assert_eq!(out.status, Status::Ok, "msg: {}", out.message);
        assert_eq!(out.data.as_ref().unwrap()["injected"], true);
        // The exact text (with its embedded space) rode through unmangled, +
        // the CR `submit` demands — proves the SAME injection door ran.
        assert_eq!(String::from_utf8(got).unwrap(), "hello world\r");

        // The entry is gone from the queue.
        let arr = load_pending_array().unwrap();
        assert!(arr.is_empty(), "a resolved entry leaves the queue");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn approve_delivers_an_entry_a_door_derived_source_queued_and_names_it() {
        // The A2A door queues under `node:<name>`; the CLI door refuses that
        // spelling as a `--from`, but approval re-drives the entry with its own
        // recorded source and must still deliver it, naming whoever queued it.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_CONDUCT_AUTOGATE",
            "AOIDE_SESSION_ID",
        ]);
        let root = setup("pnd-src");

        let id = "apt-src";
        let socket = conduct_socket_path(id);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let listener = UnixListener::bind(&socket).unwrap();
        do_session_start(
            id,
            Some("claude"),
            Some("/w"),
            None,
            None,
            Some(true),
            Some(socket.to_str().unwrap()),
            None,
            None,
        );
        crate::graph::hold_pending(id, "hello from afar", true, Some("node:peer"), &aoide_protocol::default_audit_log()).unwrap();

        let acc = expect_delivery(listener, "an approved entry delivers under the source it was queued with");
        let out = pending_approve(&named_invocation("approve", "0"));
        let got = acc.join().unwrap();

        assert_eq!(out.status, Status::Ok, "msg: {}", out.message);
        assert_eq!(String::from_utf8(got).unwrap(), "from node:peer: hello from afar\r");
        assert!(load_pending_array().unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn list_shows_an_mcp_claim_under_its_own_label_and_never_as_the_source() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-claim");

        let sender_id = "claim-sender";
        do_session_start(sender_id, Some("claude"), Some("/w"), None, None, None, None, None, None);
        let petname = load_stage::<SessionsFile>(&sessions_path())
            .unwrap()
            .sessions
            .into_iter()
            .find(|r| r.session_id == sender_id)
            .and_then(|r| r.petname)
            .expect("every minted session carries a petname (P2)");

        // `claimed` is hand-written here with a control sequence in it: the
        // queue is a file, and the screen cleans what it shows whatever wrote it.
        std::fs::write(
            pending_path(),
            format!(
                r#"{{"schemaVersion":"0","pending":[
                    {{"sessionId":"s1","text":"x","submit":false,"queuedAt":"t","from":"mcp:{sender_id}","claimed":"node:victim\u001b[2J"}},
                    {{"sessionId":"s2","text":"y","submit":false,"queuedAt":"t","from":"mcp"}}
                ]}}"#
            ),
        )
        .unwrap();

        let out = pending_list(&pending_invocation(&["session", "pending", "list"], &[]));
        let data = out.data.as_ref().unwrap();
        let arr = data["pending"].as_array().unwrap();
        assert_eq!(arr[0]["from"], format!("mcp:{sender_id}"), "JSON from stays the raw source");
        assert_eq!(arr[0]["claimed"], "node:victim[2J", "the claim is cleaned again on the way out");
        assert!(arr[1]["claimed"].is_null());
        let lines: Vec<&str> = out.message.lines().collect();
        assert!(
            lines[1].contains(&format!("(from mcp:{petname} (…")) && lines[1].contains("(claims \"node:victim[2J\")"),
            "the source first, the claim under its own name: {}",
            lines[1]
        );
        assert!(!lines[1].contains("(from node:victim"), "{}", lines[1]);
        assert!(lines[2].contains("(from mcp)") && !lines[2].contains("claims"), "{}", lines[2]);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn approving_a_pending_entry_files_it_into_the_mailbase_via_the_shared_delivery_seam() {
        // Messaging plan P-M1: the mailbase append lives ONLY in
        // `deliver_local` (send.rs) — `pending_approve`'s re-drive goes
        // through the SAME `session_send` door, so a previously-held entry
        // lands in the mailbase naturally the moment it is actually
        // delivered, never at queue time.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_CONDUCT_AUTOGATE",
            "AOIDE_SESSION_ID",
        ]);
        let root = setup("pnd-mail");
        let state = root.join("state");
        std::env::set_var("AOIDE_STATE_DIR", &state);

        let id = "mail-approve-target";
        let socket = conduct_socket_path(id);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let listener = UnixListener::bind(&socket).unwrap();
        do_session_start(
            id,
            Some("claude"),
            Some("/w"),
            None,
            None,
            Some(true),
            Some(socket.to_str().unwrap()),
            None,
            None,
        );

        let queued = stamp_ago(3_600);
        std::fs::write(
            pending_path(),
            format!(
                r#"{{"schemaVersion":"0","pending":[{{"sessionId":"{id}","text":"hello mail","submit":true,"queuedAt":"{queued}","from":"queuer-a"}}]}}"#
            ),
        )
        .unwrap();

        // Nothing in the mailbase yet — the entry is only PENDING.
        assert!(aoide_storage::mail::read_base().unwrap().is_empty());

        let acc = expect_delivery(listener, "an approved entry's delivery is filed into the mailbase");
        let out = pending_approve(&named_invocation("approve", "0"));
        let _ = acc.join().unwrap();
        assert_eq!(out.status, Status::Ok, "msg: {}", out.message);

        let entries = aoide_storage::mail::read_base().unwrap();
        assert_eq!(entries.len(), 1, "approval's re-drive filed the message");
        assert_eq!(entries[0].envelope.header.from.name, "queuer-a", "the ORIGINAL queuer, not the approver");
        assert_eq!(entries[0].envelope.header.to.name, id);
        let digest = entries[0].envelope.text.strip_prefix("sha256:").and_then(|r| r.strip_suffix(" 10B"));
        assert!(
            digest.is_some_and(|d| d.len() == 64 && d.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))),
            "a local receipt keeps the sha256 and length of the RAW text, got {:?}",
            entries[0].envelope.text
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pending_round_trip_keeps_the_original_queuer_as_sender_through_approve() {
        // Queue under sender A, approve under sender B → the delivered bytes
        // must name A (the original queuer), never B (the approver) — the
        // whole point of carrying `from` through the queue.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_CONDUCT_AUTOGATE",
            "AOIDE_SESSION_ID",
        ]);
        let root = setup("pnd-rt");

        let id = "rt-target";
        let socket = conduct_socket_path(id);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        do_session_start(
            id,
            Some("claude"),
            Some("/w"),
            None,
            None,
            Some(true),
            Some(socket.to_str().unwrap()),
            None,
            None,
        );

        // Queue under sender A (no --yes, no autogate → held pending).
        std::env::set_var("AOIDE_SESSION_ID", "sender-a");
        let queued = session_send(&send_invocation(&["do", "the", "thing"], &[("id", id), ("submit", "true")]));
        assert_eq!(queued.data.as_ref().unwrap()["state"], "pending");

        // `from` is visible in `list` BEFORE approve.
        let listed = pending_list(&pending_invocation(&["session", "pending", "list"], &[]));
        let arr = listed.data.unwrap()["pending"].as_array().unwrap().clone();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["from"], "sender-a", "the queuer is visible before approval");

        // Approve under a DIFFERENT sender B.
        std::env::set_var("AOIDE_SESSION_ID", "sender-b");
        listener.set_nonblocking(false).unwrap();
        let acc = expect_delivery(listener, "the original queuer's attribution reaches the socket");
        let approved = pending_approve(&named_invocation("approve", "0"));
        let got = acc.join().unwrap();
        assert_eq!(approved.status, Status::Ok, "msg: {}", approved.message);
        assert_eq!(
            String::from_utf8(got).unwrap(),
            "from sender-a: do the thing\r",
            "the delivered bytes name the ORIGINAL queuer, not the approver"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn approving_an_anonymously_queued_entry_never_wears_the_approvers_name() {
        // Regression: an entry queued with NO sender (env cleared at queue
        // time) must NOT inherit the approver's own live AOIDE_SESSION_ID —
        // an orchestrating conducted session always has one, so an unguarded
        // re-drive would silently misattribute an anonymous send to whoever
        // happened to approve it.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_CONDUCT_AUTOGATE",
            "AOIDE_SESSION_ID",
        ]);
        let root = setup("pnd-anon");

        let id = "anon-target";
        let socket = conduct_socket_path(id);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        do_session_start(
            id,
            Some("claude"),
            Some("/w"),
            None,
            None,
            Some(true),
            Some(socket.to_str().unwrap()),
            None,
            None,
        );

        // Queue with NO sender at all (env cleared — `setup` already does
        // this, reasserted here for clarity).
        std::env::remove_var("AOIDE_SESSION_ID");
        let queued = session_send(&send_invocation(&["do", "the", "thing"], &[("id", id), ("submit", "true")]));
        assert_eq!(queued.data.as_ref().unwrap()["state"], "pending");

        let listed = pending_list(&pending_invocation(&["session", "pending", "list"], &[]));
        let arr = listed.data.unwrap()["pending"].as_array().unwrap().clone();
        assert_eq!(arr[0]["from"], Value::Null, "queued with no attribution");

        // Approve under a REAL, non-empty sender.
        std::env::set_var("AOIDE_SESSION_ID", "approver-x");
        listener.set_nonblocking(false).unwrap();
        let acc = expect_delivery(listener, "an anonymous entry's delivery wears no approver name");
        let approved = pending_approve(&named_invocation("approve", "0"));
        let got = acc.join().unwrap();
        assert_eq!(approved.status, Status::Ok, "msg: {}", approved.message);
        assert_eq!(
            String::from_utf8(got).unwrap(),
            "do the thing\r",
            "no prefix at all — the approver's own session id must never leak in"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Path 4 of the audit: `session pending approve` on a remote door. The
    /// refusal comes before the entry leaves the queue, so the held send is
    /// still there afterwards and reaches nothing; the same entry approves from
    /// the local CLI, which proves the entry itself was sound.
    #[test]
    fn pending_approve_from_a_remote_door_is_refused_and_the_entry_stays_held() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_CONDUCT_AUTOGATE",
            "AOIDE_SESSION_ID",
        ]);
        let root = setup("pnd-remote");

        let id = "apr1";
        let socket = conduct_socket_path(id);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        do_session_start(
            id,
            Some("claude"),
            Some("/w"),
            None,
            None,
            Some(true),
            Some(socket.to_str().unwrap()),
            None,
            None,
        );
        let queued = stamp_ago(3_600);
        std::fs::write(
            pending_path(),
            format!(
                r#"{{"schemaVersion":"0","pending":[{{"sessionId":"{id}","text":"hello world","submit":true,"queuedAt":"{queued}"}}]}}"#
            ),
        )
        .unwrap();

        for door in [aoide_protocol::Door::Mcp, aoide_protocol::Door::A2a] {
            let mut inv = pending_invocation(&["session", "pending", "approve"], &["0"]);
            inv.door = door;
            let out = pending_approve(&inv);
            assert_eq!(out.status, Status::Error, "{door:?}: {}", out.message);
            assert_eq!(out.message, "this door cannot approve a gated send", "{door:?}");
            assert_eq!(out.data.as_ref().unwrap()["reason"], "door-cannot-approve", "{door:?}");
            assert_eq!(load_pending_array().unwrap().len(), 1, "{door:?}: the entry stays held");
            assert!(
                matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock),
                "{door:?}: a refused approval delivers nothing"
            );
        }

        listener.set_nonblocking(false).unwrap();
        let acc = expect_delivery(listener, "the local CLI still approves the same entry");
        let out = pending_approve(&named_invocation("approve", "0"));
        let _ = acc.join().unwrap();
        assert_eq!(out.status, Status::Ok, "msg: {}", out.message);
        assert_eq!(out.data.as_ref().unwrap()["injected"], true);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Finding 11: an entry held for a session that has not registered yet (an
    /// A2A spawn's opening turn is queued before its child exists) survives an
    /// early approve. The refusal says why, the entry stays, and the same
    /// approve delivers it once the session registers.
    #[test]
    fn an_approve_before_the_target_registers_is_refused_and_keeps_the_entry() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_CONDUCT_AUTOGATE",
            "AOIDE_SESSION_ID",
        ]);
        let root = setup("pnd-early");

        let id = "apr-early";
        let queued = stamp_ago(3_600);
        std::fs::write(
            pending_path(),
            format!(
                r#"{{"schemaVersion":"0","pending":[{{"sessionId":"{id}","text":"opening turn","submit":true,"queuedAt":"{queued}"}}]}}"#
            ),
        )
        .unwrap();

        let out = pending_approve(&named_invocation("approve", "0"));
        assert_eq!(out.status, Status::Error, "msg: {}", out.message);
        let data = out.data.as_ref().unwrap();
        assert_eq!(data["reason"], "target-not-live", "data: {data}");
        assert_eq!(
            data["refusal"]["why"], "approve again once it is live, or deny",
            "data: {data}"
        );
        assert!(
            data["refusal"]["what"].as_str().unwrap_or_default().contains("is not live (not registered yet, or ended)"),
            "data: {data}"
        );
        assert_eq!(load_pending_array().unwrap().len(), 1, "the early approve took nothing");

        let socket = conduct_socket_path(id);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let listener = UnixListener::bind(&socket).unwrap();
        do_session_start(
            id,
            Some("claude"),
            Some("/w"),
            None,
            None,
            Some(true),
            Some(socket.to_str().unwrap()),
            None,
            None,
        );
        let acc = expect_delivery(listener, "the registered session receives the approved opening turn");
        let out = pending_approve(&named_invocation("approve", "0"));
        let _ = acc.join().unwrap();
        assert_eq!(out.status, Status::Ok, "msg: {}", out.message);
        assert!(load_pending_array().unwrap().is_empty(), "approved, so it left the queue");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_legacy_pending_entry_with_no_from_field_still_lists_and_approves_cleanly() {
        // An entry written before this field existed (serde default on read)
        // must not fail to list or approve.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_CONDUCT_AUTOGATE",
            "AOIDE_SESSION_ID",
        ]);
        std::env::remove_var("AOIDE_SESSION_ID"); // no ambient attribution — legacy path only.
        let root = setup("pnd-legacy");

        let id = "legacy-target";
        let socket = conduct_socket_path(id);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let listener = UnixListener::bind(&socket).unwrap();
        do_session_start(
            id,
            Some("claude"),
            Some("/w"),
            None,
            None,
            Some(true),
            Some(socket.to_str().unwrap()),
            None,
            None,
        );

        // Hand-written, NO `from` key at all — exactly the pre-P6 shape.
        let queued = stamp_ago(3_600);
        std::fs::write(
            pending_path(),
            format!(
                r#"{{"schemaVersion":"0","pending":[{{"sessionId":"{id}","text":"legacy send","submit":false,"queuedAt":"{queued}"}}]}}"#
            ),
        )
        .unwrap();

        let listed = pending_list(&pending_invocation(&["session", "pending", "list"], &[]));
        assert_eq!(listed.status, Status::Ok);
        let arr = listed.data.unwrap()["pending"].as_array().unwrap().clone();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["from"], Value::Null, "no attribution on a legacy entry");

        let acc = expect_delivery(listener, "a legacy entry's delivery carries no prefix");
        let approved = pending_approve(&named_invocation("approve", "0"));
        let got = acc.join().unwrap();
        assert_eq!(approved.status, Status::Ok, "msg: {}", approved.message);
        assert_eq!(
            String::from_utf8(got).unwrap(),
            "legacy send",
            "no provenance prefix for a legacy entry with no sender to attribute"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// What an approver reads is what replays. Every field of an entry a human
    /// sees (target, text, sender, time, a held spawn's id and command) shows an
    /// unsafe character as a visible escape: a newline stays a `\n` and the
    /// line after it is not hidden, a bidi override is `\u{202e}`, a literal
    /// backslash is doubled so it reads differently from an ESC, and the data
    /// keeps the bytes. The approve and deny answers spell the id the same way.
    #[test]
    fn list_spells_unsafe_characters_in_every_field_a_human_reads() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-visible");

        let send_text = "ok\nrm -rf ~\u{202e}txt\r\u{1b}[2J printf '\\x1b'";
        write_stage(
            &pending_path(),
            &json!({
                "schemaVersion": "0",
                "pending": [
                    {
                        "sessionId": "tgt\u{1b}[2J",
                        "text": send_text,
                        "submit": true,
                        "queuedAt": "2026\u{7}",
                        "from": "mcp:evil\u{202e}",
                    },
                    {
                        "sessionId": "held\u{1b}[8m",
                        "text": "ignored for a spawn",
                        "queuedAt": stamp_ago(3_600),
                        "from": "mcp",
                        "spawn": { "args": ["sh", "-c", "echo hi\nreboot\u{202e}"], "flags": {} },
                    },
                ],
            }),
        )
        .unwrap();

        let out = pending_list(&pending_invocation(&["session", "pending", "list"], &[]));
        assert_eq!(out.status, Status::Ok, "{}", out.message);
        let unsafe_in = |text: &str| text.lines().any(|l| l.chars().any(crate::graph::common::is_unsafe));
        assert!(!unsafe_in(&out.message), "nothing a terminal would act on reaches the line: {:?}", out.message);

        let lines: Vec<&str> = out.message.lines().skip(1).collect();
        assert_eq!(lines.len(), 2, "one line per entry, none split by a newline in the text: {lines:?}");
        assert!(
            lines[0].contains("tgt\\x1b[2J"),
            "the target's id is spelled: {}",
            lines[0]
        );
        assert!(
            lines[0].contains("ok\\nrm -rf ~\\u{202e}txt\\r\\x1b[2J printf '\\\\x1b'"),
            "the text after its first newline is shown, escapes spelled, a backslash doubled: {}",
            lines[0]
        );
        assert!(lines[0].contains("(from mcp:evil\\u{202e})"), "{}", lines[0]);
        assert!(lines[0].contains("(2026\\x07)"), "{}", lines[0]);
        assert!(lines[1].starts_with("[1] pending, queued "), "{}", lines[1]);
        assert!(lines[1].contains("spawn held\\x1b[8m"), "{}", lines[1]);
        assert!(lines[1].contains("echo hi\\nreboot\\u{202e}"), "{}", lines[1]);

        let data = out.data.as_ref().unwrap();
        assert_eq!(data["pending"][0]["text"], send_text, "the data keeps what replays");
        assert_eq!(data["pending"][0]["sessionId"], "tgt\u{1b}[2J");
        assert!(data["pending"][1]["text"].as_str().unwrap().contains("hi\nreboot\u{202e}"));

        let denied = pending_deny(&named_invocation("deny", "0"));
        assert_eq!(denied.status, Status::Ok, "{}", denied.message);
        assert!(denied.message.contains("tgt\\x1b[2J"), "{}", denied.message);
        assert!(!unsafe_in(&denied.message), "{:?}", denied.message);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A held spawn is listed as the command that approving it would run, whole
    /// and on one line, and its JSON carries the argv and flags to replay. A
    /// terminal sequence in an argument is spelled out in the line a human
    /// reads (never stripped, never raw) and left as it replays in the data.
    /// Denying it launches nothing.
    #[test]
    fn a_held_spawn_lists_as_its_whole_command_and_deny_launches_nothing() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-spawn-list");

        let tail = "x".repeat(300);
        let marker = root.join("ran");
        let spawn = PendingSpawn {
            args: vec!["sh".into(), "-c".into(), format!("touch {}; echo \u{1b}[2J{tail}", marker.display())],
            flags: BTreeMap::from([("agent".into(), "claude".into()), ("windowed".into(), "true".into())]),
            ..PendingSpawn::default()
        };
        crate::graph::send::hold_spawn("held-1", &spawn.command_line(), spawn.clone(), &aoide_protocol::default_audit_log()).unwrap();

        let out = pending_list(&pending_invocation(&["session", "pending", "list"], &[]));
        assert_eq!(out.status, Status::Ok, "{}", out.message);
        let entry = &out.data.as_ref().unwrap()["pending"][0];
        assert_eq!(entry["state"], "pending");
        assert_eq!(entry["sessionId"], "held-1");
        assert_eq!(entry["from"], "mcp");
        assert_eq!(entry["spawn"]["args"][0], "sh");
        assert_eq!(entry["spawn"]["flags"]["agent"], "claude");
        let replayed = entry["text"].as_str().unwrap();
        assert_eq!(replayed, spawn.command_line(), "the data is the command as it replays");
        assert!(replayed.contains('\u{1b}'));
        let shown = visible(replayed);
        assert!(shown.starts_with("aoide spawn --agent=claude --windowed -- sh -c "), "{shown}");
        assert!(shown.ends_with(&format!("{tail}'")), "the command is shown to its last character: {shown}");
        assert!(shown.contains("\\x1b[2J"), "the escape is spelled out, not dropped: {shown}");
        assert!(out.message.contains(&shown), "the human line carries the same whole command: {}", out.message);
        assert!(
            out.message.lines().all(|l| l.chars().all(|c| !crate::graph::common::is_unsafe(c))),
            "no terminal sequence reaches the approver: {}",
            out.message
        );
        assert!(out.message.contains("(from mcp)"), "{}", out.message);

        let denied = pending_deny(&named_invocation("deny", "0"));
        assert_eq!(denied.status, Status::Ok, "{}", denied.message);
        assert!(load_pending_array().unwrap().is_empty());
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(!marker.exists(), "a denied spawn never ran");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// An entry whose `spawn` field does not parse is not a send: reading it as
    /// one would approve by typing its command line into the session it names.
    /// It is malformed, approve refuses it, and the target hears nothing.
    #[test]
    fn a_spawn_entry_that_does_not_parse_is_malformed_and_never_typed_as_a_send() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-spawn-bad");

        let id = "live-target";
        let socket = conduct_socket_path(id);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        do_session_start(id, Some("claude"), Some("/w"), None, None, Some(true), Some(socket.to_str().unwrap()), None, None);
        let queued = stamp_ago(3_600);
        std::fs::write(
            pending_path(),
            format!(
                r#"{{"schemaVersion":"0","pending":[{{"sessionId":"{id}","text":"aoide spawn -- rm","spawn":"not an object","queuedAt":"{queued}"}}]}}"#
            ),
        )
        .unwrap();

        let out = pending_list(&pending_invocation(&["session", "pending", "list"], &[]));
        assert_eq!(out.data.as_ref().unwrap()["pending"][0]["state"], "malformed");
        let out = pending_approve(&named_invocation("approve", "0"));
        assert_eq!(out.status, Status::Error, "{}", out.message);
        assert_eq!(load_pending_array().unwrap().len(), 1, "the entry stays for a human to inspect");
        assert!(
            matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock),
            "nothing was typed into the session"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Attestation reaches the sending gate and nothing that approves. With a
    /// fake daemon and this process sealed as session `orch`, an MCP caller is
    /// attested; its `pending approve`, `session permit` and `mail ring` are
    /// refused exactly as an unattested one's are, the held send stays held and
    /// nothing is typed.
    #[cfg(unix)]
    #[test]
    fn an_attested_mcp_caller_still_cannot_approve_permit_or_ring() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_SESSION_ID",
            "AOIDE_DAEMON_SOCKET",
        ]);
        let root = setup("pnd-attested-mcp");
        let kp = aoide_storage::identity::mint_ephemeral().unwrap();
        std::env::set_var("AOIDE_DAEMON_SOCKET", fake_seal_daemon(&kp.info().pubkey_hex));
        do_session_start("orch", Some("claude"), Some("/w"), None, None, None, None, None, Some(std::process::id()));
        seal_record("orch", &kp);

        let id = "child-of-orch";
        let socket = conduct_socket_path(id);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        do_session_start(id, Some("claude"), Some("/w"), None, Some("orch"), Some(true), Some(socket.to_str().unwrap()), None, None);
        crate::graph::send::hold_pending(id, "held", true, Some("someone"), &aoide_protocol::default_audit_log()).unwrap();

        let on_mcp = |path: &[&str], args: &[&str], flags: &[(&str, &str)]| {
            let mut inv = pending_invocation(path, args);
            inv.flags = flags.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
            inv.door = aoide_protocol::Door::Mcp;
            inv
        };
        let out = pending_approve(&on_mcp(&["session", "pending", "approve"], &["0"], &[]));
        assert_eq!(out.data.as_ref().unwrap()["reason"], "door-cannot-approve", "{}", out.message);
        let out = crate::graph::session_permit(&on_mcp(&["session", "permit"], &[], &[("id", id)]));
        assert_eq!(out.data.as_ref().unwrap()["reason"], "door-cannot-approve", "{}", out.message);
        let out = crate::graph::mail_ring(&on_mcp(&["mail", "ring"], &[], &[("for", "any-mailbox")]));
        assert_eq!(out.data.as_ref().unwrap()["reason"], "door-cannot-approve", "{}", out.message);

        assert_eq!(load_pending_array().unwrap().len(), 1, "the held send stays held");
        assert!(
            matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock),
            "nothing was typed into the session"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// `pending deny` is the other half of resolving the queue, and entries are
    /// addressed by position: a caller that could deny the entries ahead of its
    /// own would slide it onto the index a human just read. Over a door that
    /// cannot approve, attested or not, it is refused and audited, and every
    /// entry stays at its index. The CLI is the control.
    #[cfg(unix)]
    #[test]
    fn pending_deny_from_a_door_that_cannot_approve_is_refused_and_every_entry_stays() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_SESSION_ID",
            "AOIDE_DAEMON_SOCKET",
        ]);
        let root = setup("pnd-deny-door");
        let kp = aoide_storage::identity::mint_ephemeral().unwrap();
        std::env::set_var("AOIDE_DAEMON_SOCKET", fake_seal_daemon(&kp.info().pubkey_hex));
        do_session_start("orch", Some("claude"), Some("/w"), None, None, None, None, None, Some(std::process::id()));
        seal_record("orch", &kp);
        for n in 0..3 {
            crate::graph::send::hold_pending("target", &format!("entry {n}"), false, Some("someone"), &aoide_protocol::default_audit_log()).unwrap();
        }
        let texts = || -> Vec<String> {
            load_pending_array().unwrap().iter().map(|e| e["text"].as_str().unwrap().to_string()).collect()
        };

        // This process is the sealed `orch`, so an MCP call from it is attested;
        // with `AOIDE_DAEMON_SOCKET` pointing nowhere the same call is not.
        let attested_socket = std::env::var("AOIDE_DAEMON_SOCKET").unwrap();
        for (door, attested) in [
            (aoide_protocol::Door::Mcp, true),
            (aoide_protocol::Door::Mcp, false),
            (aoide_protocol::Door::A2a, false),
        ] {
            std::env::set_var(
                "AOIDE_DAEMON_SOCKET",
                if attested { attested_socket.clone() } else { root.join("dead.sock").to_string_lossy().into_owned() },
            );
            let mut inv = pending_invocation(&["session", "pending", "deny"], &["0"]);
            inv.door = door;
            let out = pending_deny(&inv);
            assert_eq!(out.status, Status::Error, "{door:?}/{attested}: {}", out.message);
            assert_eq!(out.data.as_ref().unwrap()["reason"], "door-cannot-approve", "{door:?}/{attested}");
            assert_eq!(texts(), ["entry 0", "entry 1", "entry 2"], "{door:?}/{attested}: nothing moved");
        }
        let refused = std::fs::read_to_string(root.join("log"))
            .unwrap_or_default()
            .lines()
            .filter(|l| l.contains("session.pending.deny") && l.contains("\"refused\""))
            .count();
        assert_eq!(refused, 3, "each refusal is audited");

        let out = pending_deny(&named_invocation("deny", "0"));
        assert_eq!(out.status, Status::Ok, "the CLI still denies: {}", out.message);
        assert_eq!(texts(), ["entry 1", "entry 2"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn deny_removes_without_touching_the_socket() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_CONDUCT_AUTOGATE",
            "AOIDE_SESSION_ID",
        ]);
        let root = setup("pnd-dn");

        let id = "dnt1";
        let socket = conduct_socket_path(id);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        do_session_start(
            id,
            Some("claude"),
            Some("/w"),
            None,
            None,
            Some(true),
            Some(socket.to_str().unwrap()),
            None,
            None,
        );

        let queued = stamp_ago(3_600);
        std::fs::write(
            pending_path(),
            format!(
                r#"{{"schemaVersion":"0","pending":[{{"sessionId":"{id}","text":"SHOULD_NOT_RUN","submit":false,"queuedAt":"{queued}"}}]}}"#
            ),
        )
        .unwrap();

        let out = pending_deny(&named_invocation("deny", "0"));
        assert_eq!(out.status, Status::Ok, "msg: {}", out.message);
        assert_eq!(out.data.as_ref().unwrap()["injected"], false);

        assert!(
            matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock),
            "deny must never write to the socket"
        );
        let arr = load_pending_array().unwrap();
        assert!(arr.is_empty(), "a denied entry leaves the queue too");

        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert!(
            s.sessions.iter().find(|r| r.session_id == id).unwrap().title.is_none(),
            "deny never auto-renames — nothing was ever delivered"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn approve_and_deny_on_a_malformed_or_missing_id_fail_cleanly() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-mf");

        std::fs::write(
            pending_path(),
            r#"{"schemaVersion":"0","pending":["SHOULD_NOT_RUN_UNAUTH"]}"#,
        )
        .unwrap();

        let out = pending_approve(&named_invocation("approve", "0"));
        assert_eq!(out.status, Status::Error, "a malformed entry cannot be approved");
        assert_eq!(out.data.unwrap()["reason"], "pending-entry-unresolvable");
        // Left exactly where it was — a failed resolve must not destroy it.
        assert_eq!(load_pending_array().unwrap().len(), 1);

        let out = pending_deny(&named_invocation("deny", "0"));
        assert_eq!(out.status, Status::Error, "a malformed entry cannot be denied either");
        assert_eq!(load_pending_array().unwrap().len(), 1, "still there");

        // An out-of-range index is the same clean error, not a panic.
        let out = pending_approve(&named_invocation("approve", "9"));
        assert_eq!(out.status, Status::Error);

        // A non-numeric id is a usage error.
        let out = pending_deny(&named_invocation("deny", "not-a-number"));
        assert_eq!(out.status, Status::Usage);

        let _ = std::fs::remove_dir_all(&root);
    }
    /// An entry whose spawn names a reserved `sub:` id, or carries a `--parent`,
    /// is not one an approval may launch, however it got into the queue: approve
    /// refuses it at take time and the entry stays where it is. Before the
    /// check moved into `is_known`, approve trusted the hold-time checks alone.
    #[test]
    fn approve_refuses_a_held_spawn_that_no_hold_would_queue_and_keeps_it() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-reserved-id");
        let reserved = PendingSpawn {
            flags: BTreeMap::from([("id".into(), "sub:escape".into())]),
            ..PendingSpawn::default()
        };
        crate::graph::send::hold_spawn("sub:escape", &reserved.command_line(), reserved, &aoide_protocol::default_audit_log()).unwrap();

        let out = pending_approve(&named_invocation("approve", "0"));
        assert_eq!(out.status, Status::Error, "{}", out.message);
        assert_eq!(load_pending_array().unwrap().len(), 1, "the entry is kept, not consumed");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// `--expect <fingerprint>` binds a resolution to the entry the caller read:
    /// a wrong fingerprint takes nothing, and the fingerprint `list` prints takes
    /// exactly that entry. Before the bind, the position alone decided what went.
    #[test]
    fn deny_with_a_stale_fingerprint_takes_nothing_and_the_listed_one_takes_its_entry() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-expect");
        for id in ["held-a", "held-b"] {
            let spawn = PendingSpawn::default();
            crate::graph::send::hold_spawn(id, &spawn.command_line(), spawn, &aoide_protocol::default_audit_log()).unwrap();
        }
        let listed = pending_list(&pending_invocation(&["session", "pending", "list"], &[]));
        let fp = listed.data.as_ref().unwrap()["pending"][0]["fingerprint"].as_str().unwrap().to_string();

        let deny = |expect: &str| {
            let inv = Invocation {
                path: vec!["session".into(), "pending".into(), "deny".into()],
                args: vec!["0".into()],
                flags: BTreeMap::from([("expect".to_string(), expect.to_string())]),
                door: aoide_protocol::Door::Cli,
            };
            pending_deny(&inv)
        };
        let stale = deny("0000000000000000");
        assert_eq!(stale.status, Status::Error, "{}", stale.message);
        assert_eq!(load_pending_array().unwrap().len(), 2, "a stale bind takes nothing");

        let bound = deny(&fp);
        assert_eq!(bound.status, Status::Ok, "{}", bound.message);
        let left = load_pending_array().unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0]["sessionId"], "held-b", "the listed entry, and only it, was denied");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// An empty `--expect` names no entry, so approve refuses it before the take.
    /// Read as no bind at all, a script's unset `$fp` would approve by position.
    #[test]
    fn approve_refuses_an_empty_expect_before_the_take() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-empty-expect-approve");
        let id = "empty-expect-live";
        let listener = register_live(id);
        listener.set_nonblocking(true).unwrap();
        write_queue(&[queued_entry(id, &stamp_ago(3_600), "someone", 5)]);

        let inv = Invocation {
            path: vec!["session".into(), "pending".into(), "approve".into()],
            args: vec!["0".into()],
            flags: BTreeMap::from([("expect".to_string(), String::new())]),
            door: aoide_protocol::Door::Cli,
        };
        let out = pending_approve(&inv);
        assert_eq!(out.status, Status::Error, "{}", out.message);
        let data = out.data.as_ref().unwrap();
        assert_eq!(data["reason"], "expect-empty", "{data}");
        assert_eq!(data["refusal"]["why"], "the fingerprint is empty; list the queue again and copy it", "{data}");
        assert_eq!(load_pending_array().unwrap().len(), 1, "a refused approval leaves the entry queued");
        assert!(
            matches!(listener.accept(), Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock),
            "an empty bind delivers nothing"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The deny side of the empty `--expect` refusal: a blank value removes nothing.
    #[test]
    fn deny_refuses_an_empty_expect_before_the_take() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-empty-expect-deny");
        write_queue(&[queued_entry("empty-expect-held", &stamp_ago(3_600), "someone", 5)]);

        let inv = Invocation {
            path: vec!["session".into(), "pending".into(), "deny".into()],
            args: vec!["0".into()],
            flags: BTreeMap::from([("expect".to_string(), " ".to_string())]),
            door: aoide_protocol::Door::Cli,
        };
        let out = pending_deny(&inv);
        assert_eq!(out.status, Status::Error, "{}", out.message);
        assert_eq!(out.data.as_ref().unwrap()["reason"], "expect-empty", "{}", out.message);
        assert_eq!(load_pending_array().unwrap().len(), 1, "a refused deny removes nothing");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_bare_approve_is_refused_and_prints_its_fingerprinted_command() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-bare-approve");
        let id = "bare-approve-live";
        let listener = register_live(id);
        listener.set_nonblocking(true).unwrap();
        write_queue(&[queued_entry(id, &stamp_ago(3_600), "someone", 5)]);
        let fp = listed_row(0)["fingerprint"].as_str().unwrap().to_string();

        let out = pending_approve(&pending_invocation(&["session", "pending", "approve"], &["0"]));
        assert_eq!(out.status, Status::Error, "{}", out.message);
        let data = out.data.as_ref().unwrap();
        assert_eq!(data["reason"], "expect-required", "{data}");
        assert!(out.message.contains("state:  pending"), "{}", out.message);
        assert!(out.message.contains("sender: someone"), "{}", out.message);
        assert_eq!(data["refusal"]["fix"]["run"], format!("aoide session pending approve 0 --expect {fp}"), "{data}");
        assert_eq!(load_pending_array().unwrap().len(), 1, "a refused approve takes nothing");
        assert!(
            matches!(listener.accept(), Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock),
            "a refused approve delivers nothing"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_bare_approve_of_an_expired_hold_prints_the_deny_command() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-bare-expired-fix");
        write_queue(&[queued_entry("bare-expired-held", &stamp_ago(8 * 86_400), "someone", 5)]);
        let fp = listed_row(0)["fingerprint"].as_str().unwrap().to_string();

        let out = pending_approve(&pending_invocation(&["session", "pending", "approve"], &["0"]));
        assert_eq!(out.status, Status::Error, "{}", out.message);
        let data = out.data.as_ref().unwrap();
        assert_eq!(data["reason"], "expect-required", "{data}");
        assert!(out.message.contains("state:  expired"), "{}", out.message);
        assert_eq!(data["refusal"]["fix"]["run"], format!("aoide session pending deny 0 --expect {fp}"), "{data}");
        assert_eq!(load_pending_array().unwrap().len(), 1, "a refused approve takes nothing");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_bare_approve_of_a_gone_hold_prints_the_deny_command() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-bare-gone-fix");
        let id = "bare-gone-target";
        do_session_start(id, Some("claude"), Some("/w"), None, None, None, None, None, None);
        write_queue(&[queued_entry(id, &stamp_ago(3_600), "someone", 5)]);
        set_session_state(id, "done");
        let fp = listed_row(0)["fingerprint"].as_str().unwrap().to_string();

        let out = pending_approve(&pending_invocation(&["session", "pending", "approve"], &["0"]));
        assert_eq!(out.status, Status::Error, "{}", out.message);
        let data = out.data.as_ref().unwrap();
        assert_eq!(data["reason"], "expect-required", "{data}");
        assert!(out.message.contains("state:  gone"), "{}", out.message);
        assert_eq!(data["refusal"]["fix"]["run"], format!("aoide session pending deny 0 --expect {fp}"), "{data}");
        assert_eq!(load_pending_array().unwrap().len(), 1, "a refused approve takes nothing");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_printed_approve_command_approves_that_entry() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-printed-approve");
        let (first, second) = ("printed-approve-first", "printed-approve-second");
        let _first_listener = register_live(first);
        let _second_listener = register_live(second);
        write_queue(&[
            queued_entry(first, &stamp_ago(3_600), "someone", 5),
            queued_entry(second, &stamp_ago(3_600), "someone", 6),
        ]);

        let refused = pending_approve(&pending_invocation(&["session", "pending", "approve"], &["0"]));
        let run = refused.data.as_ref().unwrap()["refusal"]["fix"]["run"].as_str().unwrap_or_default().to_string();
        let fp = run
            .strip_prefix("aoide session pending approve 0 --expect ")
            .unwrap_or_else(|| panic!("the refusal prints no approve command: {run:?}"))
            .to_string();

        let mut printed = pending_invocation(&["session", "pending", "approve"], &["0"]);
        printed.flags.insert("expect".to_string(), fp);
        let out = pending_approve(&printed);
        assert_eq!(out.status, Status::Ok, "{}", out.message);
        assert_eq!(out.data.as_ref().unwrap()["sessionId"], first, "{:?}", out.data);
        let left = load_pending_array().unwrap();
        assert_eq!(left.len(), 1, "only the named entry left the queue");
        assert_eq!(left[0]["sessionId"], second);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_old_approve_fingerprint_after_an_eviction_resolves_nothing() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-evict-approve");
        let (first, second) = ("evict-approve-first", "evict-approve-second");
        let _first_listener = register_live(first);
        let _second_listener = register_live(second);
        write_queue(&[
            queued_entry("evict-approve-old", &stamp_ago(8 * 86_400), "someone", 3),
            queued_entry(first, &stamp_ago(3_600), "someone", 4),
            queued_entry(second, &stamp_ago(3_600), "someone", 5),
        ]);
        let read_fp = listed_row(1)["fingerprint"].as_str().unwrap().to_string();

        crate::graph::send::hold_pending(
            "evict-approve-new",
            "new text",
            false,
            Some("node:fresh"),
            &aoide_protocol::default_audit_log(),
        )
        .unwrap();
        assert_eq!(load_pending_array().unwrap()[0]["sessionId"], first, "the eviction moved the first entry up");

        let mut stale = pending_invocation(&["session", "pending", "approve"], &["1"]);
        stale.flags.insert("expect".to_string(), read_fp);
        let out = pending_approve(&stale);
        assert_eq!(out.status, Status::Error, "{}", out.message);
        assert_eq!(load_pending_array().unwrap().len(), 3, "the old fingerprint resolved nothing");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_bare_deny_is_refused_and_prints_its_fingerprinted_command() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-bare-deny");
        write_queue(&[queued_entry("bare-deny-held", &stamp_ago(3_600), "someone", 5)]);
        let fp = listed_row(0)["fingerprint"].as_str().unwrap().to_string();

        let out = pending_deny(&pending_invocation(&["session", "pending", "deny"], &["0"]));
        assert_eq!(out.status, Status::Error, "{}", out.message);
        let data = out.data.as_ref().unwrap();
        assert_eq!(data["reason"], "expect-required", "{data}");
        assert!(out.message.contains("state:  pending"), "{}", out.message);
        assert_eq!(data["refusal"]["fix"]["run"], format!("aoide session pending deny 0 --expect {fp}"), "{data}");
        assert_eq!(load_pending_array().unwrap().len(), 1, "a refused deny removes nothing");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_printed_deny_command_removes_that_entry() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-printed-deny");
        write_queue(&[
            queued_entry("printed-deny-first", &stamp_ago(3_600), "someone", 5),
            queued_entry("printed-deny-second", &stamp_ago(3_600), "someone", 6),
        ]);

        let refused = pending_deny(&pending_invocation(&["session", "pending", "deny"], &["0"]));
        let run = refused.data.as_ref().unwrap()["refusal"]["fix"]["run"].as_str().unwrap_or_default().to_string();
        let fp = run
            .strip_prefix("aoide session pending deny 0 --expect ")
            .unwrap_or_else(|| panic!("the refusal prints no deny command: {run:?}"))
            .to_string();

        let mut printed = pending_invocation(&["session", "pending", "deny"], &["0"]);
        printed.flags.insert("expect".to_string(), fp);
        let out = pending_deny(&printed);
        assert_eq!(out.status, Status::Ok, "{}", out.message);
        let left = load_pending_array().unwrap();
        assert_eq!(left.len(), 1, "only the named entry left the queue");
        assert_eq!(left[0]["sessionId"], "printed-deny-second");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_old_deny_fingerprint_after_an_eviction_removes_nothing() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-evict-deny");
        let (first, second) = ("evict-deny-first", "evict-deny-second");
        write_queue(&[
            queued_entry("evict-deny-old", &stamp_ago(8 * 86_400), "someone", 3),
            queued_entry(first, &stamp_ago(3_600), "someone", 4),
            queued_entry(second, &stamp_ago(3_600), "someone", 5),
        ]);
        let read_fp = listed_row(1)["fingerprint"].as_str().unwrap().to_string();

        crate::graph::send::hold_pending(
            "evict-deny-new",
            "new text",
            false,
            Some("node:fresh"),
            &aoide_protocol::default_audit_log(),
        )
        .unwrap();
        assert_eq!(load_pending_array().unwrap()[0]["sessionId"], first, "the eviction moved the first entry up");

        let mut stale = pending_invocation(&["session", "pending", "deny"], &["1"]);
        stale.flags.insert("expect".to_string(), read_fp);
        let out = pending_deny(&stale);
        assert_eq!(out.status, Status::Error, "{}", out.message);
        assert_eq!(load_pending_array().unwrap().len(), 3, "the old fingerprint removed nothing");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_bare_refusal_prints_a_multi_line_text_as_one_line() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-bare-head");
        write_queue(&[json!({
            "sessionId": "bare-head-held",
            "text": "hello from afar\nsecond line",
            "submit": false,
            "queuedAt": stamp_ago(3_600),
            "from": "someone",
        })]);

        let out = pending_deny(&pending_invocation(&["session", "pending", "deny"], &["0"]));
        assert_eq!(out.status, Status::Error, "{}", out.message);
        assert!(out.message.contains("text:   hello from afar second line"), "{}", out.message);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A hold that is both expired and gone lists and refuses as expired: the
    /// expiry check runs before the gone check.
    #[test]
    fn a_hold_both_expired_and_gone_lists_and_refuses_as_expired() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-expired-and-gone");
        // An absent target is gone four hours in; eight days in, the hold is also expired.
        write_queue(&[queued_entry("absent-and-lapsed", &stamp_ago(8 * 86_400), "someone", 5)]);

        assert_eq!(listed_row(0)["state"], "expired", "expiry is checked before gone");
        let out = pending_approve(&named_invocation("approve", "0"));
        assert_eq!(out.status, Status::Error, "{}", out.message);
        assert_eq!(out.data.as_ref().unwrap()["reason"], "expired", "{}", out.message);
        assert_eq!(load_pending_array().unwrap().len(), 1, "a refused approval leaves the entry queued");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Each eviction line carries the door of the call that triggered it: a send
    /// on the CLI door and a spawn held over MCP each write their own door.
    #[test]
    fn each_eviction_line_carries_the_door_of_the_call_that_triggered_it() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-evict-doors");
        let target = "evict-doors-target";
        let _listener = register_live(target);
        let sender = "evict-doors-sender";
        do_session_start(sender, Some("claude"), Some("/w"), None, None, None, None, None, None);

        write_queue(&[queued_entry("cli-lapsed", &stamp_ago(8 * 86_400), "node:x", 5)]);
        std::env::set_var("AOIDE_SESSION_ID", sender);
        let sent = session_send(&send_invocation(&["do", "the", "thing"], &[("id", target), ("submit", "true")]));
        assert_eq!(sent.data.as_ref().unwrap()["state"], "pending", "{}", sent.message);

        write_queue(&[queued_entry("mcp-lapsed", &stamp_ago(8 * 86_400), "node:x", 5)]);
        let mcp = Invocation {
            path: vec!["spawn".into()],
            args: vec!["true".into()],
            flags: BTreeMap::new(),
            door: aoide_protocol::Door::Mcp,
        };
        let held = PendingSpawn {
            path: vec!["spawn".into()],
            args: vec!["true".into()],
            flags: BTreeMap::from([("id".to_string(), "mcp-held".to_string())]),
            ..PendingSpawn::default()
        };
        let spawned = crate::graph::spawn::hold_whole(&mcp, "mcp-held", held, "");
        assert_eq!(spawned.status, Status::Ok, "{}", spawned.message);

        let doors: Vec<String> = std::fs::read_to_string(root.join("log"))
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .filter(|v| v["command"] == "session.pending.evict")
            .map(|v| v["door"].as_str().unwrap_or_default().to_string())
            .collect();
        assert_eq!(doors, ["cli", "mcp"], "each eviction line names the door that queued the hold");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A gone hold that is not expired survives an eviction pass: eviction drops
    /// only the lapsed, and a gone hold waits for its deny or its seven days.
    #[test]
    fn a_gone_hold_that_is_not_expired_survives_an_eviction_pass() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-gone-survives");
        let done = "gone-survives-done";
        let _listener = register_live(done);
        set_session_state(done, "done");
        write_queue(&[
            queued_entry(done, &stamp_ago(3_600), "someone", 5),
            queued_entry("gone-survives-lapsed", &stamp_ago(8 * 86_400), "someone", 5),
        ]);
        assert_eq!(listed_row(0)["state"], "gone", "a done target is gone however young the hold");

        let held = crate::graph::send::hold_pending("gone-survives-fresh", "new text", false, Some("node:fresh"), &aoide_protocol::default_audit_log());
        assert!(held.is_ok(), "{held:?}");
        let left: Vec<String> = load_pending_array()
            .unwrap()
            .iter()
            .map(|v| v["sessionId"].as_str().unwrap_or_default().to_string())
            .collect();
        assert_eq!(left, [done, "gone-survives-fresh"], "the lapsed hold goes and the gone one stays");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A failed file write audits nothing: an eviction is recorded only once the
    /// file that dropped the entry is on disk.
    #[cfg(unix)]
    #[test]
    fn a_failed_file_write_audits_nothing() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-failed-write");
        write_queue(&[queued_entry("failed-write-lapsed", &stamp_ago(8 * 86_400), "someone", 5)]);
        // A stage directory that cannot take the temp file fails the rewrite. The
        // lock falls back to running unlocked, so only the write fails.
        let stage = root.join("stage");
        std::fs::set_permissions(&stage, std::fs::Permissions::from_mode(0o555)).unwrap();
        let held = crate::graph::send::hold_pending("failed-write-fresh", "new text", false, Some("node:fresh"), &aoide_protocol::default_audit_log());
        std::fs::set_permissions(&stage, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert!(matches!(held, Err(crate::graph::send::HoldRefused::Stage(_))), "{held:?}");
        assert_eq!(eviction_lines(&root), 0, "a write that did not land audits no eviction");
        assert_eq!(load_pending_array().unwrap().len(), 1, "the lapsed entry is still on disk");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// An eviction a held spawn causes goes to the log its caller writes: a spawn
    /// held over MCP with `audit-log` set audits the eviction there, and the
    /// default log stays untouched.
    #[test]
    fn an_eviction_from_a_spawn_held_over_mcp_goes_to_the_callers_log() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-evict-caller-log");
        let caller = root.join("caller");
        std::fs::create_dir_all(&caller).unwrap();
        write_queue(&[queued_entry("caller-lapsed", &stamp_ago(8 * 86_400), "node:x", 5)]);

        let mcp = Invocation {
            path: vec!["spawn".into()],
            args: vec!["true".into()],
            flags: BTreeMap::from([("audit-log".to_string(), caller.join("log").to_string_lossy().into_owned())]),
            door: aoide_protocol::Door::Mcp,
        };
        let held = PendingSpawn {
            path: vec!["spawn".into()],
            args: vec!["true".into()],
            flags: BTreeMap::from([("id".to_string(), "caller-held".to_string())]),
            ..PendingSpawn::default()
        };
        let spawned = crate::graph::spawn::hold_whole(&mcp, "caller-held", held, "");
        assert_eq!(spawned.status, Status::Ok, "{}", spawned.message);
        assert_eq!(eviction_lines(&caller), 1, "the eviction is audited in the log the caller named");
        assert_eq!(eviction_lines(&root), 0, "and not in the default log");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The same for a send held over A2A: its eviction goes to the log the A2A
    /// worker writes, and the default log stays untouched.
    #[test]
    fn an_eviction_from_a_send_held_over_a2a_goes_to_the_callers_log() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-evict-a2a-log");
        let caller = root.join("caller");
        std::fs::create_dir_all(&caller).unwrap();
        write_queue(&[queued_entry("a2a-lapsed", &stamp_ago(8 * 86_400), "node:x", 5)]);

        let held = crate::graph::send::hold_pending("a2a-fresh", "new text", true, Some("node:fresh"), &caller.join("log"));
        assert!(held.is_ok(), "{held:?}");
        assert_eq!(eviction_lines(&caller), 1, "the eviction is audited in the log the A2A worker writes");
        assert_eq!(eviction_lines(&root), 0, "and not in the default log");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Approve binds to the fingerprint the way deny does: a stale one takes
    /// nothing and says to list the queue again, and the listed one delivers.
    #[test]
    fn approve_with_a_stale_fingerprint_takes_nothing_and_the_listed_one_delivers() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
            "AOIDE_CONDUCT_AUTOGATE",
            "AOIDE_SESSION_ID",
        ]);
        let root = setup("pnd-approve-expect");
        let id = "bound-live";
        let _listener = register_live(id);
        write_queue(&[queued_entry(id, &stamp_ago(3_600), "someone", 5)]);
        let fp = listed_row(0)["fingerprint"].as_str().unwrap().to_string();

        let approve = |expect: &str| {
            let inv = Invocation {
                path: vec!["session".into(), "pending".into(), "approve".into()],
                args: vec!["0".into()],
                flags: BTreeMap::from([("expect".to_string(), expect.to_string())]),
                door: aoide_protocol::Door::Cli,
            };
            pending_approve(&inv)
        };
        let stale = approve("0000000000000000");
        assert_eq!(stale.status, Status::Error, "{}", stale.message);
        assert!(stale.message.contains("list the queue again"), "{}", stale.message);
        assert_eq!(load_pending_array().unwrap().len(), 1, "a stale bind takes nothing");

        let bound = approve(&fp);
        assert_eq!(bound.status, Status::Ok, "{}", bound.message);
        assert!(load_pending_array().unwrap().is_empty(), "the listed entry, and only it, was approved");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A held send's human line is its whole escaped text, not a clip of its
    /// first sixty characters: the tail that matters is shown, and the JSON
    /// carries the same spelling as `shown`. Before the change the line stopped
    /// at sixty characters with an ellipsis and the JSON had no `shown`.
    #[test]
    fn a_send_preview_is_the_whole_escaped_text_and_json_carries_shown() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-whole-preview");
        let text = format!("{} \n rm -rf ~", "x".repeat(300));
        crate::graph::send::hold_pending("held-long", &text, false, Some("node:victim"), &aoide_protocol::default_audit_log()).unwrap();

        let out = pending_list(&pending_invocation(&["session", "pending", "list"], &[]));
        assert!(out.message.contains("rm -rf ~"), "the tail is on the line: {}", out.message);
        assert!(out.message.contains("\\n rm"), "the newline is spelled: {}", out.message);
        assert!(!out.message.contains('…'), "nothing is clipped: {}", out.message);
        let shown = out.data.as_ref().unwrap()["pending"][0]["shown"].as_str().unwrap().to_string();
        assert!(shown.ends_with("\\n rm -rf ~"), "{shown}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The queue file is bounded in its stored form: the pretty-printed JSON
    /// that `write_stage` writes, not a compact count. Holds of a large entry
    /// fill it until the file bound refuses one, and the file on disk never
    /// passes the bound.
    #[test]
    fn the_file_bound_refuses_a_hold_and_the_stored_file_stays_under_it() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-file-bound");
        let big = "x".repeat(60_000);
        let mut refused = None;
        for i in 0..400 {
            let from = format!("node:{i}");
            if let Err(e) = crate::graph::send::hold_pending(&format!("id{i}"), &big, true, Some(&from), &aoide_protocol::default_audit_log()) {
                refused = Some(e);
                break;
            }
        }
        assert!(matches!(refused, Some(crate::graph::send::HoldRefused::FileFull)), "the bound must bite: {refused:?}");
        let stored = std::fs::metadata(crate::graph::send::pending_path()).unwrap().len() as usize;
        assert!(stored <= crate::graph::send::PENDING_FILE_MAX_BYTES, "{stored} bytes on disk");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn deny_spells_the_session_id_it_names_in_its_changed_line() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-changed-id");
        let spawn = PendingSpawn { args: vec!["true".into()], ..PendingSpawn::default() };
        crate::graph::send::hold_spawn("bad\n  changed: ok", &spawn.command_line(), spawn, &aoide_protocol::default_audit_log()).unwrap();
        let out = pending_deny(&named_invocation("deny", "0"));
        assert_eq!(out.status, Status::Ok, "{}", out.message);
        assert!(out.changed.iter().all(|c| !c.contains('\n')), "{:?}", out.changed);
        assert!(out.changed.iter().any(|c| c.contains("bad\\n  changed: ok")), "{:?}", out.changed);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn list_on_a_missing_queue_is_an_empty_ok_no_op() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG"]);
        let root = setup("pnd-ok");
        let out = pending_list(&pending_invocation(&["session", "pending", "list"], &[]));
        assert_eq!(out.status, Status::Ok);
        assert_eq!(out.data.unwrap()["pending"].as_array().unwrap().len(), 0);
        assert!(out.message.contains("0 pending"));
        let _ = std::fs::remove_dir_all(&root);
    }

    // ── H1 slice 1: derived expiry and the eviction ───────────────────────
    //
    // Every `queuedAt` below is read off the clock (`stamp_ago`), so the 7-day
    // rule never ages a fixture out, and no test sleeps.

    /// The stamp `secs` seconds before now, in the shape the queue writes.
    fn stamp_ago(secs: i64) -> String {
        aoide_storage::time::shift_iso_utc(&aoide_storage::time::now_iso_utc(), -secs)
    }

    /// A raw `pending.json` entry sent by `from`, queued at `queued_at`, whose
    /// text is `text_len` bytes.
    fn queued_entry(id: &str, queued_at: &str, from: &str, text_len: usize) -> Value {
        json!({
            "sessionId": id,
            "text": "x".repeat(text_len),
            "submit": false,
            "queuedAt": queued_at,
            "from": from,
        })
    }

    /// Replace `pending.json` with `entries`, written the way the queue writes it.
    fn write_queue(entries: &[Value]) {
        write_stage(&pending_path(), &json!({ "schemaVersion": "0", "pending": entries })).unwrap();
    }

    /// The row `session pending list --json` reports at `index`.
    fn listed_row(index: usize) -> Value {
        let out = pending_list(&pending_invocation(&["session", "pending", "list"], &[]));
        out.data.unwrap()["pending"][index].clone()
    }

    /// How many lines in the audit log `setup` points at record an eviction.
    fn eviction_lines(root: &std::path::Path) -> usize {
        std::fs::read_to_string(root.join("log"))
            .unwrap_or_default()
            .lines()
            .filter(|l| l.contains("session.pending.evict"))
            .count()
    }

    /// Register `id` as a live conducted session with a control socket, and
    /// return the listener its deliveries would reach.
    fn register_live(id: &str) -> UnixListener {
        let socket = conduct_socket_path(id);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let listener = UnixListener::bind(&socket).unwrap();
        do_session_start(
            id,
            Some("claude"),
            Some("/w"),
            None,
            None,
            Some(true),
            Some(socket.to_str().unwrap()),
            None,
            None,
        );
        listener
    }

    /// Fill `pending.json` to exactly 1 200 bytes under `PENDING_FILE_MAX_BYTES`:
    /// 138 live holds of 60 000 bytes, a 60 000-byte hold that is expired when
    /// `last_expired` and live otherwise, and a tuning entry that closes the gap.
    fn queue_near_the_file_cap(last_expired: bool) {
        let max = crate::graph::send::PENDING_FILE_MAX_BYTES;
        let live = stamp_ago(3_600);
        let last_at = if last_expired { stamp_ago(8 * 86_400) } else { live.clone() };
        let mut entries: Vec<Value> = (0..138)
            .map(|n| queued_entry(&format!("live-{n}"), &live, &format!("node:live-{n}"), 60_000))
            .collect();
        entries.push(queued_entry("last", &last_at, "node:last", 60_000));
        entries.push(queued_entry("tuned", &live, "node:tuned", 0));
        write_queue(&entries);
        let base = std::fs::metadata(pending_path()).unwrap().len() as usize;
        entries.last_mut().unwrap()["text"] = json!("x".repeat(max - 1_200 - base));
        write_queue(&entries);
        let on_disk = std::fs::metadata(pending_path()).unwrap().len() as usize;
        assert_eq!(on_disk, max - 1_200, "the fixture sits 1 200 bytes under the file cap");
    }

    #[test]
    fn a_hold_queued_over_seven_days_ago_lists_as_expired_and_offers_only_deny() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-expired-list");
        let queued = stamp_ago(8 * 86_400);
        write_queue(&[queued_entry("old-target", &queued, "someone", 5)]);

        let row = listed_row(0);
        assert_eq!(row["state"], "expired", "{row}");
        assert_eq!(row["expires"], aoide_storage::time::shift_iso_utc(&queued, 7 * 86_400).as_str(), "{row}");

        let out = pending_list(&pending_invocation(&["session", "pending", "list"], &[]));
        let line = out.message.lines().nth(1).unwrap();
        assert!(line.contains("expired") && line.contains("deny it"), "{line}");
        assert!(!line.contains("approve"), "an expired hold offers only deny: {line}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_hold_is_held_for_seven_days_from_its_queued_stamp_and_no_longer() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-seven-days");
        // Both targets are registered: an absent one is gone four hours in, and
        // this test is about the seven-day line alone.
        for id in ["young", "old"] {
            do_session_start(id, Some("claude"), Some("/w"), None, None, None, None, None, None);
        }
        write_queue(&[
            queued_entry("young", &stamp_ago(7 * 86_400 - 3_600), "a", 5),
            queued_entry("old", &stamp_ago(7 * 86_400 + 3_600), "b", 5),
        ]);
        assert_eq!(listed_row(0)["state"], "pending", "six days and 23 hours old is still held");
        assert_eq!(listed_row(1)["state"], "expired", "seven days and an hour old is expired");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn approve_refuses_an_expired_hold_before_the_take_and_delivers_nothing() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-expired-approve");
        let id = "expired-live";
        let listener = register_live(id);
        listener.set_nonblocking(true).unwrap();
        let queued = stamp_ago(8 * 86_400);
        write_queue(&[queued_entry(id, &queued, "someone", 5)]);

        let out = pending_approve(&named_invocation("approve", "0"));
        assert_eq!(out.status, Status::Error, "{}", out.message);
        let data = out.data.as_ref().unwrap();
        assert_eq!(data["reason"], "expired", "{data}");
        assert!(out.message.contains(&format!("expired (queued {queued})")), "{}", out.message);
        assert_eq!(data["refusal"]["why"], "deny it, or ask again", "{data}");
        assert_eq!(load_pending_array().unwrap().len(), 1, "a refused approval leaves the entry queued");
        assert!(
            matches!(listener.accept(), Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock),
            "an expired hold delivers nothing"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn deny_removes_an_expired_hold_and_injects_nothing() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-expired-deny");
        write_queue(&[queued_entry("old-target", &stamp_ago(8 * 86_400), "someone", 5)]);

        let out = pending_deny(&named_invocation("deny", "0"));
        assert_eq!(out.status, Status::Ok, "{}", out.message);
        assert_eq!(out.data.as_ref().unwrap()["injected"], false);
        assert!(load_pending_array().unwrap().is_empty(), "deny removes an expired hold");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_empty_or_unreadable_queued_at_counts_as_expired_with_no_expiry() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-empty-stamp");
        write_queue(&[queued_entry("blank", "", "a", 5), queued_entry("unreadable", "2026-10-12T09:14Z", "b", 5)]);
        for index in 0..2 {
            let row = listed_row(index);
            assert_eq!(row["state"], "expired", "{row}");
            assert!(row["expires"].is_null(), "no readable stamp, no expiry: {row}");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_hold_evicts_expired_entries_before_the_source_cap_and_audits_each_one() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-evict-hold");
        let full: Vec<Value> = (0..crate::graph::send::PENDING_CAP_PER_SOURCE)
            .map(|n| queued_entry(&format!("old{n}"), &stamp_ago(8 * 86_400), "node:evict", 5))
            .collect();
        write_queue(&full);

        let held = crate::graph::send::hold_pending("fresh", "new text", false, Some("node:evict"), &aoide_protocol::default_audit_log());
        assert!(held.is_ok(), "expired entries do not count against the source cap: {held:?}");
        assert_eq!(load_pending_array().unwrap().len(), 1, "only the new hold is left");
        assert_eq!(
            eviction_lines(&root),
            crate::graph::send::PENDING_CAP_PER_SOURCE,
            "one audit line per evicted entry"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_held_spawn_evicts_expired_entries_and_audits_each_one() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-evict-spawn");
        write_queue(&[
            queued_entry("old-a", &stamp_ago(8 * 86_400), "mcp", 5),
            queued_entry("old-b", &stamp_ago(9 * 86_400), "mcp", 5),
        ]);

        let spawn = PendingSpawn { args: vec!["true".into()], ..PendingSpawn::default() };
        let held = crate::graph::send::hold_spawn("held-new", &spawn.command_line(), spawn, &aoide_protocol::default_audit_log());
        assert!(held.is_ok(), "{held:?}");
        assert_eq!(load_pending_array().unwrap().len(), 1, "only the new spawn is left");
        assert_eq!(eviction_lines(&root), 2, "one audit line per evicted entry, from the spawn's entry point");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_held_send_evicts_expired_entries_and_audits_each_one() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-evict-send");
        let target = "evict-send-target";
        let _listener = register_live(target);
        let sender = "evict-send-sender";
        do_session_start(sender, Some("claude"), Some("/w"), None, None, None, None, None, None);
        write_queue(&[
            queued_entry("old-1", &stamp_ago(8 * 86_400), "node:x", 5),
            queued_entry("old-2", &stamp_ago(8 * 86_400), "node:x", 5),
        ]);

        std::env::set_var("AOIDE_SESSION_ID", sender);
        let queued = session_send(&send_invocation(&["do", "the", "thing"], &[("id", target), ("submit", "true")]));
        assert_eq!(queued.data.as_ref().unwrap()["state"], "pending", "{}", queued.message);
        assert_eq!(load_pending_array().unwrap().len(), 1, "only the new send is left");
        assert_eq!(eviction_lines(&root), 2, "one audit line per evicted entry, from the send's entry point");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_queue_at_its_file_cap_refuses_a_hold_taught_while_nothing_in_it_is_expired() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-cap-refused");
        let target = "cap-target";
        let _listener = register_live(target);
        let sender = "cap-sender";
        do_session_start(sender, Some("claude"), Some("/w"), None, None, None, None, None, None);
        std::env::set_var("AOIDE_SESSION_ID", sender);
        queue_near_the_file_cap(false);

        let big = "y".repeat(3_000);
        let out = session_send(&send_invocation(&[big.as_str()], &[("id", target)]));
        let data = out.data.as_ref().unwrap();
        assert_eq!(out.status, Status::Error, "{}", out.message);
        assert_eq!(data["reason"], crate::graph::send::PENDING_FILE_FULL, "{data}");
        assert!(!data["refusal"]["fix"].is_null(), "the refusal teaches its next step: {data}");
        assert_eq!(load_pending_array().unwrap().len(), 140, "nothing is dropped while nothing has expired");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_queue_at_its_file_cap_takes_a_hold_again_once_an_expired_entry_is_evicted() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-cap-evict");
        queue_near_the_file_cap(true);

        let held = crate::graph::send::hold_pending("fresh", &"y".repeat(3_000), false, Some("node:fresh"), &aoide_protocol::default_audit_log());
        assert!(held.is_ok(), "the expired entry is evicted before the file bound is counted: {held:?}");
        let left = load_pending_array().unwrap();
        assert!(left.iter().all(|e| e["sessionId"] != "last"), "the expired entry is gone");
        assert_eq!(eviction_lines(&root), 1, "one audit line for the one evicted entry");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Set the stored `state` of every session record named `id`.
    fn set_session_state(id: &str, state: &str) {
        let mut file: SessionsFile = load_stage(&sessions_path()).unwrap();
        for s in file.sessions.iter_mut().filter(|s| s.session_id == id) {
            s.state = state.to_string();
        }
        write_stage(&sessions_path(), &file).unwrap();
    }

    #[test]
    fn an_absent_target_stays_pending_until_four_hours_then_lists_as_gone() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-absent-gone");
        write_queue(&[
            queued_entry("absent-young", &stamp_ago(3_600), "someone", 5),
            queued_entry("absent-old", &stamp_ago(4 * 3_600 + 60), "someone", 5),
        ]);

        assert_eq!(listed_row(0)["state"], "pending", "an hour old, still registering");
        assert_eq!(listed_row(1)["state"], "gone", "absent four hours after it was queued");

        let young = pending_approve(&named_invocation("approve", "0"));
        assert_eq!(young.data.as_ref().unwrap()["reason"], "target-not-live", "{}", young.message);
        assert_eq!(
            young.data.as_ref().unwrap()["refusal"]["why"],
            "approve again once it is live, or deny"
        );
        let old = pending_approve(&named_invocation("approve", "1"));
        assert_eq!(old.data.as_ref().unwrap()["reason"], "gone", "{}", old.message);
        assert_eq!(load_pending_array().unwrap().len(), 2, "both refusals leave the queue alone");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_target_whose_record_is_done_lists_as_gone_and_approve_refuses_it_while_deny_removes_it() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-done-gone");
        let id = "done-target";
        let listener = register_live(id);
        listener.set_nonblocking(true).unwrap();
        set_session_state(id, "done");
        write_queue(&[queued_entry(id, &stamp_ago(3_600), "someone", 5)]);

        assert_eq!(listed_row(0)["state"], "gone", "a done target is gone however young the hold");
        let out = pending_approve(&named_invocation("approve", "0"));
        assert_eq!(out.status, Status::Error, "{}", out.message);
        assert_eq!(out.data.as_ref().unwrap()["reason"], "gone", "{}", out.message);
        assert!(!out.message.contains("not live yet"), "a gone hold is not 'not live yet': {}", out.message);
        assert!(
            matches!(listener.accept(), Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock),
            "a gone hold delivers nothing"
        );
        assert_eq!(load_pending_array().unwrap().len(), 1, "the refused approval leaves it queued");

        let denied = pending_deny(&named_invocation("deny", "0"));
        assert_eq!(denied.status, Status::Ok, "{}", denied.message);
        assert!(load_pending_array().unwrap().is_empty(), "deny removes a gone hold");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_registered_target_that_cannot_be_typed_into_lists_as_pending_and_never_says_once_it_registers() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-registered");
        let id = "registered-plain";
        do_session_start(id, Some("claude"), Some("/w"), None, None, None, None, None, None);
        write_queue(&[queued_entry(id, &stamp_ago(3_600), "someone", 5)]);

        assert_eq!(listed_row(0)["state"], "pending", "registered and not done is pending");
        let out = pending_approve(&named_invocation("approve", "0"));
        assert_eq!(out.status, Status::Error, "{}", out.message);
        assert!(out.message.contains("registered but cannot be typed into now"), "{}", out.message);
        let why = out.data.as_ref().unwrap()["refusal"]["why"].as_str().unwrap().to_string();
        assert!(!why.contains("once it registers"), "a registered session has registered: {why}");
        assert_eq!(why, "approve again once it can be typed into, or deny it", "{why}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_held_spawn_is_never_gone_even_when_its_id_names_a_done_record() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR", "AOIDE_AUDIT_LOG", "AOIDE_CONDUCT_AUTOGATE", "AOIDE_SESSION_ID"]);
        let root = setup("pnd-spawn-never-gone");
        let id = "spawn-done-id";
        do_session_start(id, Some("claude"), Some("/w"), None, None, None, None, None, None);
        set_session_state(id, "done");
        write_queue(&[json!({
            "sessionId": id,
            "text": "aoide spawn --id=spawn-done-id -- true",
            "submit": false,
            "queuedAt": stamp_ago(5 * 3_600),
            "from": "mcp",
            "spawn": { "path": ["spawn"], "args": ["true"], "flags": { "id": id } },
        })]);

        assert_eq!(listed_row(0)["state"], "pending", "a held spawn has no target until it is approved");
        let _ = std::fs::remove_dir_all(&root);
    }
}
