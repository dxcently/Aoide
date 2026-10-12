//! The session write door: session-registration commands (`session start/phase/
//! end/wrap`), transcript "say"/title/model extraction, and the sub-agent
//! node lifecycle (`do_subagent_*`). shellbridge only ever SEEDS empty
//! sessions.json/hooks.json (its socket accept loop is future work), so
//! nothing else registers a live session — a session harness (or a Claude
//! Code hook) upserts its own record here, and every mutation re-stages
//! graph.json so the read path lights up immediately.
//!
//! **Both hosts reach these now.** The `stamp_*` helpers below
//! (shell/headless/spawned/task/exit/log-path) are written by the conducted
//! session's own registration and exit path; that capability is `graph/pty`'s
//! seam on both hosts, so the `allow(dead_code)` this module carried while the
//! PTY was refused on native Windows is gone with the refusal.

use super::common::{require_flag, stage_error};
use super::doc::{
    doomed_subagent_descendants, ledger_session_exit, prune_done, restage_graph, would_cycle,
};
use super::model::{
    canonical_state, clear_screen_ask, hooks_path, load_stage, sessions_path, write_stage,
    HooksFile, SessionRecord, SessionsFile, STAGE_GRAPH_VERSION,
};
#[cfg(test)]
use super::model::HookRecord;
use aoide_protocol::agents::AgentProfile;
use aoide_protocol::Invocation;
use aoide_protocol::output::Outcome;
use aoide_storage::fs::{pid_is_alive, with_stage_lock};
use serde_json::json;
#[cfg(test)]
use serde_json::Value;
use std::collections::HashSet;

/// `now_iso_utc`/`iso_utc_from_epoch` (time) and `upsert_session`/
/// `upsert_hook` (pure Vec<Record> mutators — verified DAG-free: neither
/// touches `restage_graph`/`would_cycle`/`require_flag`/`stage_error`) moved
/// to `aoide-storage` (Phase 3a restructure,
/// docs/architecture/PACKAGE-LAYOUT.md); re-exported here so every existing
/// `crate::graph::session_store::{now_iso_utc, upsert_session, …}` caller
/// (and this file's own `do_session_*`/`do_subagent_*` handlers below, which
/// stay in root through Phase 3b) is untouched.
pub use aoide_storage::session::{upsert_hook, upsert_session};
pub use aoide_storage::time::now_iso_utc;
#[cfg(test)]
use aoide_storage::time::iso_utc_from_epoch;

/// Every ancestor (walking `parentSessionId` to the root) AND every
/// descendant (transitive children) of `id` — the same-window eviction's
/// carve-out (task #89): a member of `id`'s own lineage is never a stale
/// twin, however many hops of `conduct`/`spawn` nesting separate them.
/// Bounded against a cycle on the ancestor walk (a malformed
/// `parentSessionId` chain must never loop forever); the descendant walk is
/// naturally bounded by the finite session list. `id` itself is never
/// inserted (the caller already excludes `s.session_id != id` separately).
///
/// `pub(crate)`, not private or `pub(in crate::graph)`: `crate::reap`'s own
/// `superseded_agent_duplicates` (task #89) — a SIBLING
/// module, not a descendant of `graph` — reuses this SAME lineage set as its
/// own defense-in-depth carve-out — never a second, parallel lineage
/// computation.
pub(crate) fn lineage_of(id: &str, sessions: &[SessionRecord]) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut current = id.to_string();
    let mut seen_ancestors = HashSet::new();
    while let Some(rec) = sessions.iter().find(|s| s.session_id == current) {
        let Some(p) = rec.parent_session_id.clone() else {
            break;
        };
        if !seen_ancestors.insert(p.clone()) {
            break; // cycle guard
        }
        out.insert(p.clone());
        current = p;
    }
    let mut stack: Vec<String> = vec![id.to_string()];
    let mut seen_descendants: HashSet<String> = HashSet::new();
    while let Some(cur) = stack.pop() {
        for s in sessions {
            if s.parent_session_id.as_deref() == Some(cur.as_str())
                && seen_descendants.insert(s.session_id.clone())
            {
                out.insert(s.session_id.clone());
                stack.push(s.session_id.clone());
            }
        }
    }
    out
}

/// Some process of `rec`'s own, its recorded pid or a pid of its stored
/// `hookAncestry`, is alive and is not one of the new record's: not in `chain`
/// and not `pid`. Pure over an injected `alive`, so the rule is testable with
/// fake liveness; production passes `pid_is_alive`.
fn lives_apart(
    rec: &SessionRecord,
    chain: &[i32],
    pid: Option<u32>,
    alive: &impl Fn(u32) -> bool,
) -> bool {
    let new_pids: HashSet<u32> = chain
        .iter()
        .filter_map(|&p| u32::try_from(p).ok())
        .chain(pid)
        .collect();
    rec.hook_ancestry
        .iter()
        .filter_map(|&p| u32::try_from(p).ok())
        .chain(rec.pid)
        .any(|p| !new_pids.contains(&p) && alive(p))
}

/// May `old`, a same-window agent not in the new record's lineage, be retired in
/// favour of a record with `chain` and `pid`? The window is shared by design (one
/// app window hosts many agents), so the window itself never decides. Only a
/// twin does: the same harness under a new session id (a `/clear`, a compact, a
/// same-tab successor). Rules, in order:
///   - a process of `old`'s own still alive outside the new chain: never (it is
///     a live sibling, not a twin);
///   - `old` has no `hookAncestry` and holds a pid: yes (the legacy owner-pid
///     record, unchanged); no `hookAncestry` and no pid: no (nothing names a
///     process position, so nothing marks it a twin);
///   - the first pid of `old`'s chain that the new chain also holds sits at the
///     same depth in both chains: yes (the same process position);
///   - otherwise, including no shared pid at all: no (the new run is nested
///     beneath `old`, or another shape).
/// Both eviction passes call this, so registration and the reaper agree.
pub(crate) fn superseded_by(
    old: &SessionRecord,
    chain: &[i32],
    pid: Option<u32>,
    alive: impl Fn(u32) -> bool,
) -> bool {
    if lives_apart(old, chain, pid, &alive) {
        return false;
    }
    if old.hook_ancestry.is_empty() {
        return old.pid.is_some();
    }
    match old.hook_ancestry.iter().find(|p| chain.contains(*p)) {
        None => false,
        Some(shared) => {
            old.hook_ancestry.iter().position(|p| p == shared)
                == chain.iter().position(|p| p == shared)
        }
    }
}

/// May `rec`'s stored `hookAncestry` be replaced by `chain`, the chain of a fresh
/// SessionStart? Yes when nothing is stored, when the record has no pid, or when
/// its pid is dead or still in the new chain. Refused only while the record's own
/// pid is alive outside the new chain: that pid is the record's anchor. A pid-less
/// record follows its latest SessionStart, so two live processes under one session
/// id resolve to the latest one. Sits beside pid trust, not inside it: the chain is
/// kernel truth and never a payload claim, and a sealed record's chain may still
/// refresh (the seal binds `(pid, starttime)`, not the chain).
pub(in crate::graph) fn ancestry_may_move(
    rec: &SessionRecord,
    chain: &[i32],
    alive: impl Fn(u32) -> bool,
) -> bool {
    rec.hook_ancestry.is_empty()
        || match rec.pid {
            None => true,
            Some(p) => !alive(p) || chain.iter().any(|&c| u32::try_from(c).ok() == Some(p)),
        }
}

/// Is `rec`'s stored `hookAncestry` stale against `chain`, the harness chain of a live
/// per-turn hook ([`super::window::hook_chain`])? Stale means the record holds no live
/// process of its own outside `chain` ([`lives_apart`] is false), and `chain` carries a
/// live harness the stored chain does not hold. A chain starts at its harness, so the
/// hook process and its shell layers are never in it. The shared far ancestors (the
/// window owner, systemd) are alive while any sibling runs, so their liveness is not
/// evidence either: only a process of the record's own, outside the new chain, holds it live.
pub(in crate::graph) fn chain_is_stale(
    rec: &SessionRecord,
    chain: &[i32],
    alive: impl Fn(u32) -> bool,
) -> bool {
    !rec.hook_ancestry.is_empty()
        && !lives_apart(rec, chain, None, &alive)
        && chain
            .iter()
            .any(|&p| !rec.hook_ancestry.contains(&p) && u32::try_from(p).is_ok_and(&alive))
}

/// Core of `session start`: cycle-check a parent, UPSERT, re-stage.
#[allow(clippy::too_many_arguments)]
pub(in crate::graph) fn do_session_start(
    id: &str,
    agent: Option<&str>,
    cwd: Option<&str>,
    window: Option<&str>,
    parent: Option<&str>,
    conductable: Option<bool>,
    socket: Option<&str>,
    title: Option<&str>,
    pid: Option<u32>,
) -> Outcome {
    with_stage_lock(|| {
        do_session_start_inner(
            id, agent, cwd, window, parent, conductable, socket, title, pid, &[],
        )
    })
}

/// Core of a hook door's SessionStart registration (`send.rs`): [`do_session_start`]
/// with the hook process's own chain (`ancestry`, self-first) landed in the same
/// locked write. Registration is a SessionStart or the first hook event of an
/// unknown id, so a resumed harness that got a new process gets a current chain;
/// the move is gated by [`ancestry_may_move`]. A protected live wrap is decided
/// by [`live_wrap_hook_start`] first.
pub(in crate::graph) fn do_hook_session_start(
    door: aoide_protocol::Door,
    id: &str,
    agent: Option<&str>,
    cwd: Option<&str>,
    window: Option<&str>,
    parent: Option<&str>,
    pid: Option<u32>,
    ancestry: &[i32],
) -> Outcome {
    with_stage_lock(|| {
        // The hook door never moves a conducted wrap's edge (`conducted_wrap`).
        let conducted = load_stage::<SessionsFile>(&sessions_path())
            .is_ok_and(|f| f.sessions.iter().any(|s| s.session_id == id && conducted_wrap(s)));
        let parent = parent.filter(|_| !conducted);
        if let Some(outcome) =
            live_wrap_hook_start(door, id, agent, cwd, window, parent, ancestry)
        {
            return outcome;
        }
        do_session_start_inner(
            id, agent, cwd, window, parent, None, None, None, pid, ancestry,
        )
    })
}

/// The hook door's SessionStart on a protected live wrap (`identity::
/// is_protected_live_wrap`), decided before any write. A start that would change
/// agent, cwd, window or parent is refused, writes nothing, and appends an audit
/// line on `door`: the hook discards its outcome at one call site, so the line is
/// the only trace there. A start that changes none of them answers ok and writes
/// no identity field. `None` for every other record, and for a protected wrap
/// whose hook chain may move: that move goes through the full path below.
fn live_wrap_hook_start(
    door: aoide_protocol::Door,
    id: &str,
    agent: Option<&str>,
    cwd: Option<&str>,
    window: Option<&str>,
    parent: Option<&str>,
    ancestry: &[i32],
) -> Option<Outcome> {
    let file: SessionsFile = load_stage(&sessions_path()).ok()?;
    let rec = file.sessions.iter().find(|s| s.session_id == id)?;
    if !super::identity::is_protected_live_wrap(rec) {
        return None;
    }
    let changed = start_changes(rec, agent, cwd, window, parent);
    if !changed.is_empty() {
        audit_refused_wrap_hook_start(door, id, &changed.join(", "));
        return Some(live_wrap_refused(id, &changed));
    }
    if !ancestry.is_empty()
        && rec.hook_ancestry != ancestry
        && ancestry_may_move(rec, ancestry, pid_is_alive)
    {
        return None;
    }
    Some(live_wrap_unchanged(id, rec))
}

/// The identity fields a start would change on a protected wrap, by name: each
/// one the start supplies and the record holds differently. A field the start
/// does not carry changes nothing, since `upsert_session` keeps the stored value.
/// Only these four are compared, so a start never refuses over `kind` or any
/// other stored field.
fn start_changes(
    rec: &SessionRecord,
    agent: Option<&str>,
    cwd: Option<&str>,
    window: Option<&str>,
    parent: Option<&str>,
) -> Vec<&'static str> {
    let mut changed = Vec::new();
    if agent.is_some_and(|a| a != rec.agent) {
        changed.push("agent");
    }
    if cwd.is_some_and(|c| c != rec.cwd) {
        changed.push("cwd");
    }
    if window.is_some_and(|w| w != rec.window_address) {
        changed.push("window");
    }
    if parent.is_some_and(|p| rec.parent_session_id.as_deref() != Some(p)) {
        changed.push("parent");
    }
    changed
}

/// The ok answer to a start that changes no identity field of a protected wrap.
/// Nothing is written.
fn live_wrap_unchanged(id: &str, rec: &SessionRecord) -> Outcome {
    Outcome::ok(
        "session.start",
        format!("re-started session `{id}` (no field changed; startedAt preserved)"),
    )
    .with_data(json!({
        "sessionId": id,
        "agent": rec.agent,
        "startedAt": rec.started_at,
        "inserted": false,
        "file": sessions_path().to_string_lossy(),
    }))
}

/// The refusal of a start that would change an identity field of a protected
/// wrap. `fields` names them.
fn live_wrap_refused(id: &str, fields: &[&str]) -> Outcome {
    Outcome::error("session.start", super::identity::live_wrap_refusal(id))
        .with_data(json!({ "reason": "live-sealed-wrap", "id": id, "fields": fields }))
}

/// Append the audit line for a refused hook start on a protected wrap, on the
/// door the hook arrived on.
fn audit_refused_wrap_hook_start(door: aoide_protocol::Door, id: &str, fields: &str) {
    let _ = aoide_protocol::audit::audit(
        &aoide_protocol::audit::default_audit_log(),
        door,
        aoide_protocol::audit::EventClass::Audit,
        "session.start",
        "error",
        &format!("live-sealed-wrap: a hook start on `{id}` would change {fields}; nothing was written"),
    );
}

#[allow(clippy::too_many_arguments)]
fn do_session_start_inner(
    id: &str,
    agent: Option<&str>,
    cwd: Option<&str>,
    window: Option<&str>,
    parent: Option<&str>,
    conductable: Option<bool>,
    socket: Option<&str>,
    title: Option<&str>,
    pid: Option<u32>,
    ancestry: &[i32],
) -> Outcome {
    let cmd = "session.start";
    let mut file: SessionsFile = match load_stage(&sessions_path()) {
        Ok(f) => f,
        Err(e) => return stage_error(cmd, e),
    };
    // Reuse `link`'s cycle guard: an id parented under its own descendant (or
    // itself) is refused before we mutate anything.
    if let Some(p) = parent {
        if would_cycle(&file.sessions, id, p) {
            return Outcome::error(
                cmd,
                format!("parenting `{id}` under `{p}` would create a cycle"),
            )
            .with_data(json!({ "reason": "cycle", "sessionId": id, "parent": p }));
        }
    }

    let now = now_iso_utc();
    let inserted = upsert_session(
        &mut file.sessions,
        id,
        agent,
        cwd,
        window,
        parent,
        conductable,
        socket,
        title,
        pid,
        &now,
    );
    // The hook chain lands in this same locked write, and only where
    // `ancestry_may_move` allows (see that fn): a record anchored to a live pid of
    // its own keeps its chain.
    if let Some(rec) = file.sessions.iter_mut().find(|s| s.session_id == id) {
        if !ancestry.is_empty()
            && rec.hook_ancestry != ancestry
            && ancestry_may_move(rec, ancestry, pid_is_alive)
        {
            rec.hook_ancestry = ancestry.to_vec();
        }
    }
    let (r_agent, r_started) = file
        .sessions
        .iter()
        .find(|s| s.session_id == id)
        .map(|s| (s.agent.clone(), s.started_at.clone()))
        .unwrap_or_default();

    // Registration-time same-window agent eviction. A window hosts any number of
    // live agents (one Claude desktop app runs six headless sessions in one
    // window), so the shared window alone never retires anyone. A same-window
    // record is retired only when it is this record's superseded twin
    // ([`superseded_by`]): its own processes are gone, and the new record is the
    // same harness position under a new session id. The new registration itself
    // is the deciding signal, so no grace is needed here (unlike the reaper's
    // dedup pass, which resolves a pair it merely OBSERVES together and so must
    // wait to tell which twin is real). The ~12s reaper (`crate::reap`) is the
    // safety net for the slower path. Carve-outs: a conducted PTY host
    // (`conductable` — e.g. `conduct -- kimi`'s wrapper, which `is_agent_kind`
    // refuses despite its published "agent" kind) is the control-socket owner,
    // not a second foreground agent; and the new record's WHOLE LINEAGE (every
    // ancestor AND descendant) is excluded outright, so a nested headless
    // `conduct`/`spawn` chain is never evicted as a twin of its own launcher.
    let new_rec = file.sessions.iter().find(|s| s.session_id == id).cloned();
    let evicted: Vec<String> = match &new_rec {
        Some(rec) if crate::reap::is_agent_kind(rec) && !rec.window_address.is_empty() => {
            let lineage = lineage_of(id, &file.sessions);
            file.sessions
                .iter()
                .filter(|s| {
                    s.session_id != id
                        && !lineage.contains(&s.session_id)
                        && s.state != "done"
                        && s.window_address == rec.window_address
                        && crate::reap::is_agent_kind(s)
                        && superseded_by(s, &rec.hook_ancestry, rec.pid, pid_is_alive)
                })
                .map(|s| s.session_id.clone())
                .collect()
        }
        _ => Vec::new(),
    };
    if !evicted.is_empty() {
        let mut h_file: HooksFile = load_stage(&hooks_path()).unwrap_or_default();
        let done_at = now_iso_utc();
        for eid in &evicted {
            upsert_hook(&mut h_file.hooks, eid, "done", &done_at);
        }
        for s in file.sessions.iter_mut() {
            if evicted.contains(&s.session_id) {
                s.state = "done".to_string();
            }
        }
        let (kept_s, kept_h, _removed, _cleared) =
            prune_done(std::mem::take(&mut file.sessions), std::mem::take(&mut h_file.hooks));
        file.sessions = kept_s;
        h_file.hooks = kept_h;
        if h_file.schema_version.is_empty() {
            h_file.schema_version = STAGE_GRAPH_VERSION.to_string();
        }
        let _ = write_stage(&hooks_path(), &h_file);
    }

    if file.schema_version.is_empty() {
        file.schema_version = STAGE_GRAPH_VERSION.to_string();
    }
    if let Err(e) = write_stage(&sessions_path(), &file) {
        return stage_error(cmd, e);
    }

    let mut changed = vec![if inserted {
        format!("registered session {id} (idle)")
    } else {
        format!("updated session {id}")
    }];
    match restage_graph() {
        Ok(g) => changed.push(g.to_string_lossy().into_owned()),
        Err(e) => return stage_error(cmd, e),
    }
    let message = if inserted {
        format!("started session `{id}` (agent {r_agent}, idle)")
    } else {
        format!("re-started session `{id}` (fields updated; startedAt preserved)")
    };
    Outcome::ok(cmd, message).changed(changed).with_data(json!({
        "sessionId": id,
        "agent": r_agent,
        "startedAt": r_started,
        "inserted": inserted,
        "file": sessions_path().to_string_lossy(),
    }))
}

/// Land the canonical live `state` onto a session record in `sessions.json` —
/// the widget-facing file. This is the fix for hook states never reaching the
/// widgets: they watch `sessions.json`, whose `state` used to be only
/// `running`/`done` (hook phases lived only in `hooks.json`), so `awaiting`
/// and the dock peek could never fire. A no-op for an unregistered id.
/// Change-only, and it migrates any other legacy state it passes, so repeated
/// same-state phases and old vocab don't churn the file. `Ok(true)` when it wrote.
fn set_session_state(id: &str, state: &str) -> Result<bool, String> {
    let mut file: SessionsFile = load_stage(&sessions_path())?;
    let canon = canonical_state(state);
    let mut changed = false;
    for s in file.sessions.iter_mut() {
        let target = if s.session_id == id {
            canon
        } else {
            canonical_state(&s.state) // migrate legacy vocab in passing
        };
        if s.state != target {
            s.state = target.to_string();
            changed = true;
        }
        // A non-working session isn't running anything → drop its stale activity
        // (the last tool/command). A working session keeps it (a tool hook owns it).
        if s.session_id == id && target != "working" && s.activity.is_some() {
            s.activity = None;
            changed = true;
        }
    }
    if changed {
        if file.schema_version.is_empty() {
            file.schema_version = STAGE_GRAPH_VERSION.to_string();
        }
        write_stage(&sessions_path(), &file)?;
    }
    Ok(changed)
}

/// Set the live state + `activity` (the current tool) on a session OR a sub-node
/// — the tool hooks' writer. The "owner" is the session, or a sub-node
/// `sub:<id>` when the tool ran inside a Task sub-agent (Fable's activity
/// routing, so a sub-agent's churn never clobbers its parent's display). Writes
/// the canonical phase to hooks.json (audit + graph merge) and the
/// state+activity to sessions.json (the widgets), under one stage lock.
///
/// hooks.json is written unconditionally — [`upsert_hook`] always stamps a
/// fresh `updatedAt`, so its content is genuinely new on every call (it's the
/// per-hook heartbeat, not a mirror of session state). sessions.json (and the
/// `graph.json` restage it triggers) is CHANGE-ONLY, same discipline as
/// `conduct.rs`'s `do_session_refresh`: a tool hook fires far more often than
/// `state`/`activity` actually change, and this file is what the widgets'
/// FileView watches — an unconditional rewrite here churns the UI on every
/// single hook, not just the ones that moved anything.
pub(in crate::graph) fn set_owner_activity(owner: &str, state: &str, activity: Option<&str>) {
    with_stage_lock(|| {
        let canon = canonical_state(state);
        let mut h: HooksFile = load_stage(&hooks_path()).unwrap_or_default();
        upsert_hook(&mut h.hooks, owner, canon, &now_iso_utc());
        if h.schema_version.is_empty() {
            h.schema_version = STAGE_GRAPH_VERSION.to_string();
        }
        let _ = write_stage(&hooks_path(), &h);

        let mut file: SessionsFile = match load_stage(&sessions_path()) {
            Ok(f) => f,
            Err(_) => return,
        };
        let want_activity = activity.filter(|a| !a.is_empty()).map(str::to_string);
        let mut changed = false;
        for s in file.sessions.iter_mut() {
            if s.session_id != owner {
                continue;
            }
            if s.state != canon {
                s.state = canon.to_string();
                changed = true;
            }
            if s.activity != want_activity {
                s.activity = want_activity.clone();
                changed = true;
            }
        }
        if changed {
            if file.schema_version.is_empty() {
                file.schema_version = STAGE_GRAPH_VERSION.to_string();
            }
            if write_stage(&sessions_path(), &file).is_ok() {
                let _ = restage_graph();
            }
        }
    });
}

// ── Transcript "say" — the agent's latest words, straight off its JSONL ──────
//
// The transcript layout, locator, and JSONL extractors are agent-harness
// knowledge: they live in the profile (`aoide_protocol::agents::TranscriptSpec`,
// claude: `~/.claude/projects/<munge(cwd)>/<session_id>.jsonl`). The refresh
// commands below dispatch through the profile the hook door hands down.

/// Publish a harness-reported context-window ceiling onto the session record
/// (pi's extension reports its active model's `contextWindow` on every hook
/// payload). A locked read-modify-write like every other stage writer; only
/// touches the stage when the value actually changed, and an absent report
/// never clears a stored ceiling.
pub(in crate::graph) fn ensure_session_ceiling(id: &str, ceiling: u64) {
    with_stage_lock(|| {
        let mut file: SessionsFile = match load_stage(&sessions_path()) {
            Ok(f) => f,
            Err(_) => return,
        };
        if let Some(s) = file
            .sessions
            .iter_mut()
            .find(|s| s.session_id == id && s.context_ceiling != Some(ceiling))
        {
            s.context_ceiling = Some(ceiling);
            if file.schema_version.is_empty() {
                file.schema_version = STAGE_GRAPH_VERSION.to_string();
            }
            let _ = write_stage(&sessions_path(), &file);
            let _ = restage_graph();
        }
    });
}

/// Stamp a conducted session's `logPath` — the absolute path of its
/// pty-master transcript (`state/sessions/<id>.log`), opened by every
/// conduct-owned pty (task #15: interactive and headless alike, "everything
/// tees"). A locked
/// read-modify-write like [`ensure_session_ceiling`]: change-only (a second
/// identical call is a no-op that never re-stages), and a silent no-op for an
/// unknown id — conduct calls this right after `do_session_start`, so the id
/// is always fresh, but the discipline matches every other stage writer here.
pub(in crate::graph) fn set_session_log_path(id: &str, path: &str) {
    with_stage_lock(|| {
        let mut file: SessionsFile = match load_stage(&sessions_path()) {
            Ok(f) => f,
            Err(_) => return,
        };
        if let Some(s) = file
            .sessions
            .iter_mut()
            .find(|s| s.session_id == id && s.log_path.as_deref() != Some(path))
        {
            s.log_path = Some(path.to_string());
            if file.schema_version.is_empty() {
                file.schema_version = STAGE_GRAPH_VERSION.to_string();
            }
            let _ = write_stage(&sessions_path(), &file);
            let _ = restage_graph();
        }
    });
}

/// Stamp a managed task wrapper run's two own task keys at registration
/// (`task`, `instructionsPath`; `docs/Aoide-Wiki/concepts/orchestration/
/// Managed-Task-Wrapper.md`, `CONTRACTS.md` §4). Called by the CHILD's own
/// `conduct`, right after `do_session_start`, from the `--task` /
/// `--instructions-path` flags `spawn` threaded through — one writer for
/// these facts, so there is no parent/child race on the record, the same
/// posture `logPath` has. Change-only and a silent no-op for an unknown id,
/// exactly like [`set_session_log_path`]. No `restage_graph()`: neither key
/// is rendered into the widget-facing `graph.json`, so stamping them must
/// not churn it.
pub(in crate::graph) fn stamp_task(
    id: &str,
    task: &str,
    instructions_path: Option<&str>,
    report_to: Option<&str>,
) {
    with_stage_lock(|| {
        let mut file: SessionsFile = match load_stage(&sessions_path()) {
            Ok(f) => f,
            Err(_) => return,
        };
        let mut changed = false;
        for s in file.sessions.iter_mut().filter(|s| s.session_id == id) {
            if s.task.as_deref() != Some(task) {
                s.task = Some(task.to_string());
                changed = true;
            }
            if let Some(path) = instructions_path {
                if s.instructions_path.as_deref() != Some(path) {
                    s.instructions_path = Some(path.to_string());
                    changed = true;
                }
            }
            if let Some(name) = report_to {
                if s.report_to.as_deref() != Some(name) {
                    s.report_to = Some(name.to_string());
                    changed = true;
                }
            }
        }
        if changed {
            if file.schema_version.is_empty() {
                file.schema_version = STAGE_GRAPH_VERSION.to_string();
            }
            let _ = write_stage(&sessions_path(), &file);
        }
    });
}

/// Stamp a managed task run's END facts: the REAL `exitCode` when the
/// caller's own exit path knows it, and always `endedAt`. `exit_code: None`
/// means "no code to report" and writes nothing there — an absent
/// `exitCode` is the honest answer for a killed/reaped run, and must never
/// be flattened to `0`. Stamped by `conduct`'s own exit path (the code
/// `conduct_multiplex` returned) and by `do_session_end`/the reaper (the
/// `None` arm), change-only, no-op on an unknown id, no `restage_graph()`
/// (same reasoning as [`stamp_task`]).
pub(in crate::graph) fn stamp_session_exit(
    id: &str,
    exit_code: Option<i32>,
    outcome: Option<&str>,
    ended_at: &str,
) {
    with_stage_lock(|| {
        let mut file: SessionsFile = match load_stage(&sessions_path()) {
            Ok(f) => f,
            Err(_) => return,
        };
        let mut changed = false;
        for s in file.sessions.iter_mut().filter(|s| s.session_id == id) {
            if let Some(code) = exit_code {
                if s.exit_code != Some(code) {
                    s.exit_code = Some(code);
                    changed = true;
                }
            }
            if let Some(name) = outcome {
                if s.outcome.as_deref() != Some(name) {
                    s.outcome = Some(name.to_string());
                    changed = true;
                }
            }
            if s.ended_at.as_deref() != Some(ended_at) {
                s.ended_at = Some(ended_at.to_string());
                changed = true;
            }
        }
        if changed {
            if file.schema_version.is_empty() {
                file.schema_version = STAGE_GRAPH_VERSION.to_string();
            }
            let _ = write_stage(&sessions_path(), &file);
        }
    });
}

/// A conducted wrap (`conduct`'s own registration, PTY or headless). Its
/// parent edge is set there, where the spawner was checked against the
/// registering process's own kernel ancestry, so no hook door may move it.
fn conducted_wrap(s: &SessionRecord) -> bool {
    s.conductable == Some(true)
}

/// Move the parent edge of a hook-registered record to the attested wrap that
/// hosts it NOW (P-QOL-C §1, `send.rs`'s `hook_ensure_session`). That is the
/// fix for a record born under an EARLIER wrap (a harness id survives
/// `--resume`) never re-parenting onto the wrap that hosts it now. Three
/// differences from the hook chain ([`do_hook_session_start`]), despite the
/// identical silent-no-op-on-error/unknown-id posture: (a) CHANGE-ONLY on
/// difference, not write-once — a resumed record must keep following its
/// current wrap, the whole transfer-window case this exists for; (b) refuses a
/// self-parent or any parent that would close a cycle ([`would_cycle`], the
/// same guard `do_session_start` applies to an explicit `--parent`) — attested
/// evidence is still just an ordinary edge once it reaches the DAG, never
/// license to corrupt it; (c) calls [`restage_graph`] — unlike `hookAncestry`
/// (consumed internally only), `parentSessionId` IS a rendered edge, so a
/// change here must reach `graph.json` immediately, not wait for some other
/// writer to re-stage it.
///
/// It never attests the edge it moves: `attestedSpawner` is written only by
/// registration (`stamp_attested_spawner`), and a conducted wrap is never
/// touched here at all.
pub(in crate::graph) fn stamp_attested_parent(id: &str, parent: &str) {
    if parent.is_empty() || parent == id {
        return;
    }
    with_stage_lock(|| {
        let mut file: SessionsFile = match load_stage(&sessions_path()) {
            Ok(f) => f,
            Err(_) => return,
        };
        if would_cycle(&file.sessions, id, parent) {
            return;
        }
        let Some(s) = file.sessions.iter_mut().find(|s| {
            s.session_id == id && !conducted_wrap(s) && s.parent_session_id.as_deref() != Some(parent)
        }) else {
            return;
        };
        s.set_parent_session(Some(parent.to_string()));
        if file.schema_version.is_empty() {
            file.schema_version = STAGE_GRAPH_VERSION.to_string();
        }
        if write_stage(&sessions_path(), &file).is_ok() {
            let _ = restage_graph();
        }
    });
}

/// Clear a stale `parentSessionId` on `id` (P-QOL-C4 §2): outside any wrap —
/// no attested wrap, no `AOIDE_SESSION_ID` — there is no host, so a resumed
/// record must not keep following the terminal that hosted its PREVIOUS run.
/// The `HookAction::Start` arm of `hook_for_profile_gated` (send.rs) calls
/// this the moment both parent sources resolve to `None`, before its own
/// `do_session_start` upsert, so a later `session kill` refuses
/// (`kill_target`'s `NO_DEDICATED_PROCESS`) instead of resolving through
/// that stale terminal. Change-only, same posture as
/// [`stamp_attested_parent`] just above: no record, or one already
/// parentless, or a conducted wrap, is a silent no-op.
pub(in crate::graph) fn clear_stale_parent(id: &str) {
    with_stage_lock(|| {
        let mut file: SessionsFile = match load_stage(&sessions_path()) {
            Ok(f) => f,
            Err(_) => return,
        };
        let Some(s) = file
            .sessions
            .iter_mut()
            .find(|s| s.session_id == id && s.parent_session_id.is_some() && !conducted_wrap(s))
        else {
            return;
        };
        s.set_parent_session(None);
        if file.schema_version.is_empty() {
            file.schema_version = STAGE_GRAPH_VERSION.to_string();
        }
        if write_stage(&sessions_path(), &file).is_ok() {
            let _ = restage_graph();
        }
    });
}

/// Stamp `headless = true` on a headless `aoide conduct` wrap's OWN record —
/// a PERMANENT registration fact (task #89), called
/// unconditionally right after `do_session_start` whenever `--headless` was
/// given, regardless of whether window discovery or the log-file open below
/// succeed. Distinct from "`windowAddress` happens to be empty right now":
/// `window::windowless_by_lineage`/`resolve_pending_session_windows` key off
/// THIS field first, so a headless wrap's windowlessness (and its whole
/// hook-child subtree's) survives even if something upstream still manages
/// to stamp a stray window onto the record. Change-only (a no-op once
/// already `true`) and a silent no-op for an unknown id, same posture as
/// [`set_session_log_path`]. No `restage_graph()` — like `hookAncestry`,
/// `headless` is consumed internally, never rendered.
pub(in crate::graph) fn stamp_headless(id: &str) {
    with_stage_lock(|| {
        let mut file: SessionsFile = match load_stage(&sessions_path()) {
            Ok(f) => f,
            Err(_) => return,
        };
        if let Some(s) = file
            .sessions
            .iter_mut()
            .find(|s| s.session_id == id && !s.headless)
        {
            s.headless = true;
            if file.schema_version.is_empty() {
                file.schema_version = STAGE_GRAPH_VERSION.to_string();
            }
            let _ = write_stage(&sessions_path(), &file);
        }
    });
}

/// Mark a record as created by `aoide spawn`, permanently.
///
/// The `stamp_headless` sibling above, for the same reason and with the same
/// once-only guard: the find already excludes an id whose flag is set, so a
/// re-registration can never rewrite it and no caller has to check first.
pub(in crate::graph) fn stamp_spawned(id: &str) {
    with_stage_lock(|| {
        let mut file: SessionsFile = match load_stage(&sessions_path()) {
            Ok(f) => f,
            Err(_) => return,
        };
        if let Some(s) = file
            .sessions
            .iter_mut()
            .find(|s| s.session_id == id && !s.spawned)
        {
            s.spawned = true;
            if file.schema_version.is_empty() {
                file.schema_version = STAGE_GRAPH_VERSION.to_string();
            }
            let _ = write_stage(&sessions_path(), &file);
        }
    });
}

/// Stamp `attestedSpawner`: the parent id the registering process's own kernel
/// ancestry confirmed (`window::spawner_is_attested`), or clear it.
///
/// Set-or-clear on every registration — never once-only — because the value
/// names the parent it vouches for: re-registering an id under a parent the
/// kernel does NOT confirm must drop the stamp, and a later `session start
/// --parent <other>` that rewrites `parentSessionId` drops the stamp with it,
/// since every parent write goes through `SessionRecord::set_parent_session`.
pub(in crate::graph) fn stamp_attested_spawner(id: &str, attested: Option<&str>) {
    with_stage_lock(|| {
        let mut file: SessionsFile = match load_stage(&sessions_path()) {
            Ok(f) => f,
            Err(_) => return,
        };
        if let Some(s) = file
            .sessions
            .iter_mut()
            .find(|s| s.session_id == id && s.attested_spawner.as_deref() != attested)
        {
            s.attested_spawner = attested.map(str::to_string);
            if file.schema_version.is_empty() {
                file.schema_version = STAGE_GRAPH_VERSION.to_string();
            }
            let _ = write_stage(&sessions_path(), &file);
        }
    });
}

/// Stamp whether this session's wrapped command IS a shell — the durable
/// half of `conduct::program_is_a_shell`'s verdict, and the ONE registration
/// fact the auto-typing refusals read off a record.
///
/// Unlike `stamp_headless`/`stamp_spawned` above this is not once-only, and
/// the difference is the point: re-registering an id is a real shape (`spawn
/// --id X -- bash` onto a live id), and the answer must follow the CURRENT
/// argv — setting it for `-- bash` and CLEARING it for a harness on the same
/// id. Change-only, so a no-op write when the value already matches, and both
/// arms of `upsert_session` are covered because this reads and writes the
/// record that exists, whichever arm produced or refreshed it.
pub(in crate::graph) fn stamp_wrap(id: &str, is_shell: bool, program: Option<String>) {
    with_stage_lock(|| {
        let mut file: SessionsFile = match load_stage(&sessions_path()) {
            Ok(f) => f,
            Err(_) => return,
        };
        if let Some(s) = file.sessions.iter_mut().find(|s| {
            s.session_id == id && (s.shell != is_shell || s.wrapped_program != program)
        }) {
            s.shell = is_shell;
            s.wrapped_program = program.clone();
            if file.schema_version.is_empty() {
                file.schema_version = STAGE_GRAPH_VERSION.to_string();
            }
            let _ = write_stage(&sessions_path(), &file);
        }
    });
}

/// Stamp `origin` on a just-registered session record (P-P3,
/// `docs/architecture/PAIRING.md` decision 7). A PERMANENT birth fact, like
/// `headless`: stamped once, change-only (a no-op once
/// already set to this exact value), never re-derived later. No
/// `restage_graph()` — like `hookAncestry`/`headless`, consumed internally
/// (the durable session ledger, via `doc::ledger_session_exit`) rather than
/// rendered into `graph.json`, so stamping it must not churn the
/// widget-facing document. A silent no-op for an unknown id or an empty
/// `origin`.
///
/// `pub` (crosses the crate boundary) and has exactly TWO legitimate
/// callers, each the record-layer authority for one origin shape (LANE
/// IDENTITY P-ID0, G16/G5 — the write-once-BY-AUTHORITY tightening at the
/// record layer, distinct from the sealed credential below):
///   - `graph/conduct.rs::session_conduct`, for a LOCAL-CLASS origin off its
///     own inherited `AOIDE_SESSION_ORIGIN` env — and that call site now
///     REFUSES a `node:*` shape from that env read, because inherited env
///     is exactly what a same-uid process can set on itself before invoking
///     `aoide conduct` directly.
///   - `aoide-server`'s `a2a::do_spawn` (`stamp_spawn_provenance`), for a
///     `node:<name>` origin — called directly on the just-spawned session's
///     record from the DOOR that authenticated the node name, never
///     threaded through the child's env at all. This is the only place a
///     `node:*` value may originate.
/// The raw field is attribution, not authentication: a same-uid process
/// can still forge a LOCAL-class origin. The authenticated form is the
/// sealed credential (`stamp_seal` below, P-ID1/P-ID2) — a consumer gates
/// only on an `originClass` read off a VERIFIED seal, never this raw
/// string; consumer-NAME authentication remains a separate, unbuilt axis.
pub fn stamp_origin(id: &str, origin: &str) {
    if origin.is_empty() {
        return;
    }
    with_stage_lock(|| {
        let mut file: SessionsFile = match load_stage(&sessions_path()) {
            Ok(f) => f,
            Err(_) => return,
        };
        if let Some(s) = file
            .sessions
            .iter_mut()
            .find(|s| s.session_id == id && s.origin.as_deref() != Some(origin))
        {
            s.origin = Some(origin.to_string());
            if file.schema_version.is_empty() {
                file.schema_version = STAGE_GRAPH_VERSION.to_string();
            }
            let _ = write_stage(&sessions_path(), &file);
        }
    });
}

/// Stamp `remoteParent` on a just-registered session record — the CHILD's own
/// node recording which session on ANOTHER node asked for it
/// (`docs/architecture/CONTRACTS.md` §4's `remoteParent`, the a2a door's
/// counterpart to the caller-side `remote-children.json` ledger).
///
/// `pub` (crosses the crate boundary) with ONE caller by construction, the same
/// shape [`stamp_origin`] just above holds: `aoide-server`'s `a2a::do_spawn`
/// (`stamp_spawn_provenance`), on the record it just spawned, from the door
/// where the caller's signature was verified and the node's KEY is
/// authenticated. There is deliberately no env var and no `conduct`/`spawn`
/// flag for it — `session_conduct` reads none, and `resurrect` carries none
/// forward — because an ambient value is exactly the unauthenticated shape
/// `origin`'s own refusal above closes (`AOIDE_SESSION_ORIGIN`); a name or id
/// taken from a request's headers or body would be the same mistake one layer
/// out, so the door builds this value from the RESOLVED node record, never
/// from wire bytes ([`stamp_origin`]'s "stamp from the authority that just
/// authenticated the fact").
///
/// Change-only and a silent no-op for an unknown id or an empty `sessionId` —
/// inherited from [`stamp_origin`], not re-derived. Like [`stamp_seal`], this
/// is NOT an immutability guard: a later call with a genuinely DIFFERENT parent
/// still overwrites, since the guard compares against the value already on the
/// record, not against "has this ever been set". In practice the id is minted
/// once per spawn and this door is the only writer, so it behaves as a birth
/// fact — a property of the call site, not an invariant this function
/// enforces. No `restage_graph()`: this stamps the RECORD fact, and a reader
/// that wants it rendered restages on its own pass rather than paying a
/// widget-document churn on every spawn (the cost `origin`/`hookAncestry`
/// avoid the same way).
///
/// **The field is attribution, never a gate** (`origin`'s own posture):
/// `sessions.json` stays a plain, same-uid-writable file, so nothing may key a
/// security decision on `remoteParent` as read off disk. The gate is the key
/// comparison the DOOR makes against the verifying node's pubkey.
pub fn stamp_remote_parent(id: &str, parent: &aoide_storage::records::RemoteParent) {
    if parent.session_id.is_empty() {
        return;
    }
    with_stage_lock(|| {
        let mut file: SessionsFile = match load_stage(&sessions_path()) {
            Ok(f) => f,
            Err(_) => return,
        };
        if let Some(s) = file
            .sessions
            .iter_mut()
            .find(|s| s.session_id == id && s.remote_parent.as_ref() != Some(parent))
        {
            s.remote_parent = Some(parent.clone());
            if file.schema_version.is_empty() {
                file.schema_version = STAGE_GRAPH_VERSION.to_string();
            }
            let _ = write_stage(&sessions_path(), &file);
        }
    });
}

/// Stamp `seal` on a just-registered session record (LANE IDENTITY P-ID1,
/// `docs/architecture/CONTRACTS.md`'s identity section). Change-only (a
/// no-op once already set to this exact value) and a silent no-op for an
/// unknown id or an empty `seal` — the same shape [`stamp_origin`] just
/// above holds, inherited rather than re-derived. Like `stamp_origin`,
/// this is NOT an immutability guard: a later call with a genuinely
/// DIFFERENT `seal` string for the same id still overwrites it, since the
/// guard only compares against the value already on the record, not
/// against "has this ever been set before." In practice its one caller
/// (below) only ever calls this once per successful registration, so it
/// behaves as a birth fact — but that is a property of the call site, not
/// an invariant this function enforces. No
/// `restage_graph()`: like `origin`/`hookAncestry`, this field is consumed
/// internally (P-ID2's verify-on-accept, not built yet) rather than
/// rendered into `graph.json`, so stamping it must not churn the
/// widget-facing document.
///
/// `pub` (crosses the crate boundary), called from `aoide-server`'s daemon
/// (LANE IDENTITY P-ID2, `CONTRACTS.md`'s identity section): once from the
/// `dispatch` handler's `session start` path (P-ID1's original caller,
/// pid already on the record), and once from the daemon tick's own
/// sealing sweep — the fix for the gap P-ID1 shipped as scaffolding
/// (module doc there): `session_conduct`'s own DIRECT, non-dispatched
/// registration — the common real-world path, `aoide conduct -- <agent>`
/// — never touches the daemon's `dispatch` handler at all, so it was
/// NEVER sealed under P-ID1. The tick sweep (`server::daemon::
/// seal_unsealed_live_sessions`) closes that: every live, pid-carrying,
/// unsealed session gets a seal within one tick (~1s) of registering,
/// regardless of which path registered it.
///
/// `issued_at` rides alongside `seal` (both stamped together, both
/// change-only against the value already on the record) — `verify_seal`
/// needs the EXACT signed `SealedIdentity` to check a signature against,
/// and `issued_at` has no live fact a verifier can re-derive it from
/// (`SessionRecord::sealed_issued_at`'s own doc) — this is the same
/// "stamp from the authority that just authenticated the fact" shape
/// [`stamp_origin`]'s own doc names.
///
/// `pid_starttime` is the value the seal was minted over, written in plain text
/// as `sealedPidStarttime`, and the boot id is written as `sealedBootId`, both in
/// the same change. The refusal predicate reads those, not the signature, so
/// they must land with the seal. A record that carries a seal, or ever did, is
/// never stamped again.
pub fn stamp_seal(id: &str, seal: &str, issued_at: i64, pid_starttime: u64) {
    if seal.is_empty() {
        return;
    }
    with_stage_lock(|| {
        let mut file: SessionsFile = match load_stage(&sessions_path()) {
            Ok(f) => f,
            Err(_) => return,
        };
        if let Some(s) = file
            .sessions
            .iter_mut()
            .find(|s| s.session_id == id && s.seal.is_none() && s.ever_sealed != Some(true))
        {
            s.seal = Some(seal.to_string());
            s.sealed_issued_at = Some(issued_at);
            s.sealed_pid_starttime = Some(pid_starttime);
            s.sealed_boot_id = aoide_storage::attest::boot_id();
            s.ever_sealed = Some(true);
            if file.schema_version.is_empty() {
                file.schema_version = STAGE_GRAPH_VERSION.to_string();
            }
            let _ = write_stage(&sessions_path(), &file);
        }
    });
}

/// Stamp `harnessSessionId` — the raw hook payload's OWN `session_id` field
/// (P-D7) — on a session record, on EVERY hook event that carries one, not
/// just at registration: unlike `headless` (a birth fact,
/// stamped once), this can legitimately be re-asserted on every event
/// without harm (a resumed/compacted session refires SessionStart with the
/// same id), so the guard is "would this write actually change anything",
/// not "is this the first time". A locked read-modify-write like
/// [`ensure_session_ceiling`]/[`set_session_log_path`]: change-only, and a
/// silent no-op for an unknown id. No `restage_graph()` — like
/// `hookAncestry`/`headless`, this field is consumed internally (a future
/// `resurrect` reader, P-D8) rather than rendered, so stamping it must
/// not churn the widget-facing `graph.json`.
pub(in crate::graph) fn stamp_harness_session_id(id: &str, harness_session_id: &str) {
    if harness_session_id.is_empty() {
        return;
    }
    with_stage_lock(|| {
        let mut file: SessionsFile = match load_stage(&sessions_path()) {
            Ok(f) => f,
            Err(_) => return,
        };
        if let Some(s) = file.sessions.iter_mut().find(|s| {
            s.session_id == id && s.harness_session_id.as_deref() != Some(harness_session_id)
        }) {
            s.harness_session_id = Some(harness_session_id.to_string());
            if file.schema_version.is_empty() {
                file.schema_version = STAGE_GRAPH_VERSION.to_string();
            }
            let _ = write_stage(&sessions_path(), &file);
        }
    });
}

/// Stamp `sessionStartAt` — WHEN this harness's own `SessionStart` was
/// recorded for this session. One writer (the hook door's `SessionStart`
/// arm, `send.rs`), one consumer: `aoide_conduct::graph::wait_ready`, which
/// will not type a first turn into a target whose harness has not said hello
/// since that launch began. A record that merely EXISTS is not the fact —
/// an earlier run's child under a reused `--id` lingers for days (a
/// hook-registered child carries no pid, so the reaper's `/proc` signal
/// never fires for it), and reading it as readiness is the original
/// early-inject defect with a `delivered` message over it.
///
/// Same locked, change-only read-modify-write as
/// [`stamp_harness_session_id`] above, and the same silent no-op for an
/// unknown id: a later `SessionStart` (a resume, a compact) re-stamps it, so
/// the guard is "would this write change anything", never "is this the
/// first". No `restage_graph()` — the field is read by launch paths, not
/// rendered by the widgets.
pub(in crate::graph) fn stamp_session_start_at(id: &str, at: &str) {
    if at.is_empty() {
        return;
    }
    with_stage_lock(|| {
        let mut file: SessionsFile = match load_stage(&sessions_path()) {
            Ok(f) => f,
            Err(_) => return,
        };
        if let Some(s) = file.sessions.iter_mut().find(|s| {
            s.session_id == id && s.session_start_at.as_deref() != Some(at)
        }) {
            s.session_start_at = Some(at.to_string());
            if file.schema_version.is_empty() {
                file.schema_version = STAGE_GRAPH_VERSION.to_string();
            }
            let _ = write_stage(&sessions_path(), &file);
        }
    });
}

/// Stamp `openingTurn` — what became of the first turn a REMOTE peer asked
/// for when the A2A door spawned this session. One writer, from another
/// crate: `aoide-server`'s `a2a::do_spawn` stamps `pending` as it accepts the
/// spawn (the reply it hands back cannot yet know) and its worker stamps the
/// conclusion once the wait-and-type is over; `tasks/get` reads it so a peer
/// whose opening turn never ran is told that, rather than reading a bare
/// `submitted` over a session no turn ever reached. Change-only and a silent
/// no-op for an unknown id, like every other stamp here.
pub fn stamp_opening_turn(id: &str, word: &str) {
    if word.is_empty() {
        return;
    }
    with_stage_lock(|| {
        let mut file: SessionsFile = match load_stage(&sessions_path()) {
            Ok(f) => f,
            Err(_) => return,
        };
        if let Some(s) = file
            .sessions
            .iter_mut()
            .find(|s| s.session_id == id && s.opening_turn.as_deref() != Some(word))
        {
            s.opening_turn = Some(word.to_string());
            if file.schema_version.is_empty() {
                file.schema_version = STAGE_GRAPH_VERSION.to_string();
            }
            let _ = write_stage(&sessions_path(), &file);
        }
    });
}

/// Settle the opening turns whose verdict was LOST — every record still
/// carrying `pending`, flipped to `unknown`. One caller: the daemon's boot
/// pass (off-thread, `server/src/daemon.rs`). The verdict is stamped by the
/// A2A door's worker, and a process that dies with that worker still in flight
/// leaves `pending` on the record with nobody left to decide it; a fresh boot
/// is exactly the moment that is knowable, and `unknown` says what happened
/// ("the process that owed this verdict is gone") rather than leaving a word
/// that reads as "still waiting". Change-only, locked like every other stamp
/// here, and a no-op when there is nothing stranded.
pub fn settle_lost_opening_turns() -> usize {
    with_stage_lock(|| {
        let mut file: SessionsFile = match load_stage(&sessions_path()) {
            Ok(f) => f,
            Err(_) => return 0,
        };
        let mut settled = 0;
        for s in file.sessions.iter_mut() {
            if s.opening_turn.as_deref() == Some("pending") {
                s.opening_turn = Some("unknown".to_string());
                settled += 1;
            }
        }
        if settled > 0 {
            if file.schema_version.is_empty() {
                file.schema_version = STAGE_GRAPH_VERSION.to_string();
            }
            let _ = write_stage(&sessions_path(), &file);
        }
        settled
    })
}

/// Stamp `resumedFrom` on a just-registered session record — the ledger
/// entry's own `sessionId` its resume argv was built from (P-D8,
/// `docs/architecture/AOIDED.md`'s "L5"). Unlike [`stamp_harness_session_id`]
/// this DOES call [`restage_graph`]: `build_graph` projects the field as a
/// `resumed` edge beside `spawned`/`anchors` (CONTRACTS.md §4), so it must be
/// RENDERED, not only held for internal use — the conductor needs to see the
/// edge without a second manual resync. Change-only; a silent no-op for an
/// unknown id (the windowed spawn's own terminal may not have registered by
/// the time `resurrect` gets here — an honest no-op, never a crash).
pub(in crate::graph) fn stamp_resumed_from(id: &str, resumed_from: &str) {
    if resumed_from.is_empty() {
        return;
    }
    with_stage_lock(|| {
        let mut file: SessionsFile = match load_stage(&sessions_path()) {
            Ok(f) => f,
            Err(_) => return,
        };
        let Some(s) = file
            .sessions
            .iter_mut()
            .find(|s| s.session_id == id && s.resumed_from.as_deref() != Some(resumed_from))
        else {
            return;
        };
        s.resumed_from = Some(resumed_from.to_string());
        if file.schema_version.is_empty() {
            file.schema_version = STAGE_GRAPH_VERSION.to_string();
        }
        if write_stage(&sessions_path(), &file).is_ok() {
            let _ = restage_graph();
        }
    });
}

/// Best-effort: refresh a session's transcript-derived fields at a hook boundary —
/// its `say` (the agent's latest words) and, set-once, its `title` (the session
/// NAME, from `custom-title`). Change-only; never touches state/activity/pid;
/// re-stages only when something moved. Silent no-op when the transcript can't be
/// located or read. One tail read serves both fields. All harness knowledge is
/// dispatched through the session agent's `profile`.
///
/// Returns whether it actually WROTE — the hook callers ignore it; the reaper's
/// sweep counts it, so its toast can say how many agents it brought current
/// (see `reap::refresh_live_agents`).
pub(crate) fn refresh_transcript_fields(
    profile: &'static AgentProfile,
    session_id: &str,
    cwd: Option<&str>,
    transcript_hint: Option<&str>,
    preferred_ceiling: Option<u64>,
) -> bool {
    let spec = &profile.transcript;
    let Some(path) = (spec.locate)(session_id, cwd, transcript_hint) else {
        return false;
    };
    let lines = (spec.tail)(&path);
    if lines.is_empty() {
        return false;
    }
    let say = (spec.say)(&lines, true);
    let name = (spec.title)(&lines);
    let model = (spec.model)(&lines, true);
    let context_tokens = (spec.context_tokens)(&lines);
    let tool = (spec.tool)(&lines, true);
    if say.is_none()
        && name.is_none()
        && model.is_none()
        && context_tokens.is_none()
        && tool.is_none()
    {
        return false;
    }
    with_stage_lock(|| {
        let mut file: SessionsFile = match load_stage(&sessions_path()) {
            Ok(f) => f,
            Err(_) => return false,
        };
        let mut changed = false;
        for s in file.sessions.iter_mut() {
            if s.session_id != session_id {
                continue;
            }
            if let Some(say) = &say {
                if s.say.as_deref() != Some(say.as_str()) {
                    s.say = Some(say.clone());
                    changed = true;
                }
            }
            if let Some(tool) = &tool {
                if s.tool.as_deref() != Some(tool.as_str()) {
                    s.tool = Some(tool.clone());
                    changed = true;
                }
            }
            // The session NAME is set ONCE — a hook-set or graph-send title wins,
            // so a renamed conductor task is never clobbered by the tab title.
            if let Some(name) = &name {
                if s.title.as_deref().unwrap_or("").is_empty() {
                    s.title = Some(name.clone());
                    changed = true;
                }
            }
            if let Some(model) = &model {
                if s.model.as_deref() != Some(model.as_str()) {
                    s.model = Some(model.clone());
                    changed = true;
                }
                // Publish the model's context ceiling (CONTRACTS.md §4). A
                // harness-reported ceiling (pi's payload `context_ceiling`)
                // WINS over the aoide catalog — a custom/provider model the
                // catalog has never heard of gets the right meter; the catalog
                // stays the fallback for harnesses that don't self-report.
                let ceiling = Some(preferred_ceiling.unwrap_or_else(|| {
                    (profile.model_ceiling)(Some(model.as_str()))
                }));
                if s.context_ceiling != ceiling {
                    s.context_ceiling = ceiling;
                    changed = true;
                }
            }
            if let Some(ctx) = context_tokens {
                if s.context_tokens != Some(ctx) {
                    s.context_tokens = Some(ctx);
                    changed = true;
                }
            }
        }
        if changed {
            if file.schema_version.is_empty() {
                file.schema_version = STAGE_GRAPH_VERSION.to_string();
            }
            if write_stage(&sessions_path(), &file).is_ok() {
                let _ = restage_graph();
            }
        }
        changed
    })
}

/// Best-effort: refresh the `say` of a session's ACTIVE sub-agent nodes from their
/// own transcript files. Runs on each of the PARENT session's hooks, so a
/// background Task (which outlives the turn) shows its latest words on its beamed
/// child row; a synchronous Task blocks the parent and is too transient to catch.
/// Change-only and bounded to the currently-live sub-nodes (depth-1). The
/// sub-agent transcript layout is the profile's (`spec.subagents_dir` /
/// `spec.find_subagent`).
///
/// Returns whether it WROTE, same as [`refresh_transcript_fields`] and for the
/// same one caller — the reaper counts what it brought current.
pub(crate) fn refresh_subagent_says(
    profile: &'static AgentProfile,
    session_id: &str,
    cwd: Option<&str>,
) -> bool {
    let subs: Vec<String> = match load_stage::<SessionsFile>(&sessions_path()) {
        Ok(file) => file
            .sessions
            .iter()
            .filter(|s| s.kind.as_deref() == Some("subagent"))
            .filter(|s| s.parent_session_id.as_deref() == Some(session_id))
            .map(|s| s.session_id.clone())
            .collect(),
        Err(_) => return false,
    };
    if subs.is_empty() {
        return false;
    }
    let spec = &profile.transcript;
    let Some(dir) = (spec.subagents_dir)(session_id, cwd) else {
        return false;
    };
    // (sub_id, say, model, tool) — any of the three may be None for a given sub.
    let mut updates: Vec<(String, Option<String>, Option<String>, Option<String>)> = Vec::new();
    for sub_id in &subs {
        let Some(tuid) = sub_id.strip_prefix("sub:") else {
            continue;
        };
        let Some(file) = (spec.find_subagent)(&dir, tuid) else {
            continue;
        };
        // A sub-agent's OWN dedicated transcript marks every line isSidechain —
        // don't skip them here (see `extract_say`'s doc). Its model is its OWN
        // (subagents can run a different model than their parent).
        let lines = (spec.tail)(&file);
        let say = (spec.say)(&lines, false);
        let model = (spec.model)(&lines, false);
        let tool = (spec.tool)(&lines, false);
        if say.is_some() || model.is_some() || tool.is_some() {
            updates.push((sub_id.clone(), say, model, tool));
        }
    }
    if updates.is_empty() {
        return false;
    }
    with_stage_lock(|| {
        let mut file: SessionsFile = match load_stage(&sessions_path()) {
            Ok(f) => f,
            Err(_) => return false,
        };
        let mut changed = false;
        for (sub_id, say, model, tool) in &updates {
            for s in file.sessions.iter_mut() {
                if &s.session_id != sub_id {
                    continue;
                }
                if let Some(say) = say {
                    if s.say.as_deref() != Some(say.as_str()) {
                        s.say = Some(say.clone());
                        changed = true;
                    }
                }
                if let Some(tool) = tool {
                    if s.tool.as_deref() != Some(tool.as_str()) {
                        s.tool = Some(tool.clone());
                        changed = true;
                    }
                }
                if let Some(model) = model {
                    if s.model.as_deref() != Some(model.as_str()) {
                        s.model = Some(model.clone());
                        changed = true;
                    }
                    // Publish the subagent's own context ceiling — parity with the
                    // parent-session derivation in `refresh_transcript_fields`, so a
                    // subagent card gets a correct meter too (it runs its own model).
                    let ceiling = Some((profile.model_ceiling)(Some(model.as_str())));
                    if s.context_ceiling != ceiling {
                        s.context_ceiling = ceiling;
                        changed = true;
                    }
                }
            }
        }
        if changed {
            if file.schema_version.is_empty() {
                file.schema_version = STAGE_GRAPH_VERSION.to_string();
            }
            if write_stage(&sessions_path(), &file).is_ok() {
                let _ = restage_graph();
            }
        }
        changed
    })
}

/// Create (or refresh) a sub-agent node — a Task the agent spawned, a leaf of
/// the conductor tree. Keyed by `sub:<tool_use_id>`; parented to `owner` (a
/// parent sub-node when nested, else the session). An existing node is enriched
/// (keeps its parent + first name), so the PreToolUse(Task) and SubagentStart
/// paths converge on one record however they race.
///
/// `allow_create` guards the create branch: the PreToolUse spawn and the classic
/// `parent_tool_use_id` SubagentStart backstop pass `true` (they are the record's
/// origin). The async `Agent` path's SubagentStart carries ONLY an `agent_id`
/// and reaches here keyed `sub:<agent_id>` — but the authoritative node is the
/// one PreToolUse created under `sub:<tool_use_id>` and PostToolUse re-keys to
/// `sub:<agent_id>`; so that fallback passes `false` (enrich-only): it confirms
/// the node once the re-key has landed and is a harmless no-op before then,
/// never a duplicate.
pub(in crate::graph) fn do_subagent_spawn(
    sub_id: &str,
    owner: &str,
    name: &str,
    agent_type: &str,
    allow_create: bool,
) {
    with_stage_lock(|| {
        let mut file: SessionsFile = match load_stage(&sessions_path()) {
            Ok(f) => f,
            Err(_) => return,
        };
        if let Some(s) = file.sessions.iter_mut().find(|s| s.session_id == sub_id) {
            if s.state != "done" && s.state != "working" {
                s.state = "working".to_string();
            }
            if s.title.as_deref().unwrap_or("").is_empty() && !name.is_empty() {
                s.title = Some(name.to_string());
            }
            if (s.agent.is_empty() || s.agent == "subagent") && !agent_type.is_empty() {
                s.agent = agent_type.to_string();
            }
            s.kind = Some("subagent".to_string());
        } else if !allow_create {
            // Enrich-only: the node does not exist yet (the async re-key has not
            // landed). Do nothing rather than create a duplicate/misparented one.
            return;
        } else {
            let petname = aoide_storage::petname::mint_for(&file.sessions);
            file.sessions.push(SessionRecord {
                session_id: sub_id.to_string(),
                agent: if agent_type.is_empty() {
                    "subagent".to_string()
                } else {
                    agent_type.to_string()
                },
                state: "working".to_string(),
                started_at: now_iso_utc(),
                parent_session_id: Some(owner.to_string()),
                title: if name.is_empty() {
                    None
                } else {
                    Some(name.to_string())
                },
                kind: Some("subagent".to_string()),
                petname: Some(petname),
                ..Default::default()
            });
        }
        if file.schema_version.is_empty() {
            file.schema_version = STAGE_GRAPH_VERSION.to_string();
        }
        if write_stage(&sessions_path(), &file).is_ok() {
            let _ = restage_graph();
        }
    });
}

/// Close a sub-agent node — REMOVE it (a Task returned / a subagent stopped), so
/// the tree stays clean. Idempotent; re-stages only when it removed one.
pub(in crate::graph) fn do_subagent_end(sub_id: &str) {
    with_stage_lock(|| {
        let mut file: SessionsFile = match load_stage(&sessions_path()) {
            Ok(f) => f,
            Err(_) => return,
        };
        let before = file.sessions.len();
        file.sessions.retain(|s| s.session_id != sub_id);
        if file.sessions.len() != before {
            let _ = write_stage(&sessions_path(), &file);
            let _ = restage_graph();
        }
    });
}

/// Re-key a sub-agent node's `sessionId` in place. An async `Agent` dispatch is
/// created under `sub:<tool_use_id>` (all PreToolUse carries), but the only id
/// its later SubagentStart/SubagentStop carry is the `agent_id` — a different
/// string with no derivable link to the tool_use_id. The dispatch's OWN
/// PostToolUse is the single payload where both co-occur (`tool_use_id` +
/// `tool_response.agentId`), so it renames the record there from
/// `sub:<tool_use_id>` to `sub:<agent_id>` — keeping every other field (parent,
/// kind, state, title, startedAt) untouched — and reparents any child that
/// pointed at the old id. Because each dispatch re-keys using the id pair from
/// its OWN PostToolUse, two Agent calls in one turn never cross-attribute.
///
/// Idempotent: a no-op when the source is gone or the ids already match; if the
/// target id somehow already exists, the stale source is dropped rather than
/// duplicated.
pub(in crate::graph) fn do_subagent_rekey(from_sub_id: &str, to_sub_id: &str) {
    if from_sub_id == to_sub_id || from_sub_id.is_empty() || to_sub_id.is_empty() {
        return;
    }
    with_stage_lock(|| {
        let mut file: SessionsFile = match load_stage(&sessions_path()) {
            Ok(f) => f,
            Err(_) => return,
        };
        if !file.sessions.iter().any(|s| s.session_id == from_sub_id) {
            return; // source gone (PreToolUse missed, or already re-keyed) — no-op
        }
        let target_exists = file.sessions.iter().any(|s| s.session_id == to_sub_id);
        let mut changed = false;
        if target_exists {
            // A node already lives under the new id — never duplicate; drop the
            // stale source and let the existing target stand.
            let before = file.sessions.len();
            file.sessions.retain(|s| s.session_id != from_sub_id);
            changed = file.sessions.len() != before;
        } else {
            for s in file.sessions.iter_mut() {
                if s.session_id == from_sub_id {
                    s.session_id = to_sub_id.to_string();
                    changed = true;
                }
            }
        }
        // Reparent any child that pointed at the old id (none at launch time, but
        // keeps the tree consistent if a nested node ever raced in first).
        for s in file.sessions.iter_mut() {
            if s.parent_session_id.as_deref() == Some(from_sub_id) {
                s.set_parent_session(Some(to_sub_id.to_string()));
                changed = true;
            }
        }
        if changed {
            if file.schema_version.is_empty() {
                file.schema_version = STAGE_GRAPH_VERSION.to_string();
            }
            if write_stage(&sessions_path(), &file).is_ok() {
                let _ = restage_graph();
            }
        }
    });
}

/// A `kind:"app"` record's `state` comes from its rollout's turn bracket
/// (`apply_codex_capture`, `codex_app.rs`, P-CX-5 S3) — the one writer for
/// it from here on. `session phase`/`session end` stamping an arbitrary
/// state onto one would stick until a later capture happened to disagree
/// (no per-tick reset survives S3 to self-heal a stray write), so both
/// refuse it outright — the same class of refusal `send`
/// (`codex-app-unsupported`, `send.rs`) and `kill` (`APP_OWNS_PROCESS`,
/// `actions.rs`) already hold for their own writes onto an app record. An
/// id with no record yet (or any other kind) is untouched — `None`.
fn refuse_app_record(cmd: &'static str, id: &str, sessions: &[SessionRecord]) -> Option<Outcome> {
    let rec = sessions.iter().find(|s| s.session_id == id)?;
    if rec.kind.as_deref() != Some("app") {
        return None;
    }
    Some(
        Outcome::error(
            cmd,
            format!(
                "session `{id}` lives inside the desktop app that owns its server; aoide has no channel to it"
            ),
        )
        .with_data(json!({ "reason": "codex-app-unsupported", "id": id })),
    )
}

/// Core of `session phase`: UPSERT the hook record (audit), land the
/// canonical live state on sessions.json (the widget file), re-stage. The whole
/// load-modify-write is serialised against every other stage writer by the
/// stage lock, held ONCE around the whole body — its inner helpers stay
/// lock-free, and the lock nests only for the thread already holding it.
pub(in crate::graph) fn do_session_phase(id: &str, phase: &str) -> Outcome {
    with_stage_lock(|| do_session_phase_inner(id, phase))
}
fn do_session_phase_inner(id: &str, phase: &str) -> Outcome {
    let cmd = "session.phase";
    let sessions: SessionsFile = match load_stage(&sessions_path()) {
        Ok(f) => f,
        Err(e) => return stage_error(cmd, e),
    };
    if let Some(out) = refuse_app_record(cmd, id, &sessions.sessions) {
        return out;
    }
    let mut file: HooksFile = match load_stage(&hooks_path()) {
        Ok(f) => f,
        Err(e) => return stage_error(cmd, e),
    };
    let now = now_iso_utc();
    upsert_hook(&mut file.hooks, id, phase, &now);
    if file.schema_version.is_empty() {
        file.schema_version = STAGE_GRAPH_VERSION.to_string();
    }
    if let Err(e) = write_stage(&hooks_path(), &file) {
        return stage_error(cmd, e);
    }
    // Land the canonical live state on sessions.json (the widget file), so the
    // roster/dock render working/awaiting/idle — not just running/done.
    if let Err(e) = set_session_state(id, phase) {
        return stage_error(cmd, e);
    }
    let mut changed = vec![format!("session {id}: phase → {phase}")];
    match restage_graph() {
        Ok(g) => changed.push(g.to_string_lossy().into_owned()),
        Err(e) => return stage_error(cmd, e),
    }
    Outcome::ok(cmd, format!("session `{id}` phase = {phase}"))
        .changed(changed)
        .with_data(json!({
            "sessionId": id,
            "phase": phase,
            "updatedAt": now,
            "file": hooks_path().to_string_lossy(),
        }))
}

/// The session's stored phase, canonicalized — the same authority
/// [`do_session_phase_if`] compares `expected` against, exposed standalone so
/// a caller can inspect it BEFORE a mutating call changes it. Task #139
/// phase 2's Start arm reads this ahead of `do_session_start`/
/// `do_session_phase_if` to tell a fresh launch from a mid-turn resume
/// (`working` ⇔ a turn is in flight) — the one authority for that fact, never
/// a second turn-tracking flag and never the hook payload's own `source`
/// string (a manual between-turns `/compact` says the same word as an
/// auto-compact but is a settled boundary; the stored phase tells them
/// apart). An id with no hook record yet reads as `idle`
/// (`canonical_state`'s own default for an empty string), matching every
/// other reader of this file.
pub(in crate::graph) fn stored_phase(id: &str) -> String {
    let file: HooksFile = load_stage(&hooks_path()).unwrap_or_default();
    let raw = file
        .hooks
        .iter()
        .find(|h| h.session_id == id)
        .map(|h| h.phase.clone())
        .unwrap_or_default();
    canonical_state(&raw).to_string()
}

/// Conditional sibling of [`do_session_phase`]: UPSERT `phase` for `id` ONLY when
/// its CURRENT hook phase equals `expected`, else an ok no-op that writes nothing.
/// hooks.json is loaded ONCE — the guard read and the write share the same load,
/// so the current phase is never read twice. This is the door the ambiguous idle
/// Notification walks: only a still-`working` turn becomes `awaiting`.
pub(in crate::graph) fn do_session_phase_if(id: &str, phase: &str, expected: &str) -> Outcome {
    with_stage_lock(|| do_session_phase_if_inner(id, phase, expected))
}
fn do_session_phase_if_inner(id: &str, phase: &str, expected: &str) -> Outcome {
    let cmd = "session.phase";
    let mut file: HooksFile = match load_stage(&hooks_path()) {
        Ok(f) => f,
        Err(e) => return stage_error(cmd, e),
    };
    let current = file
        .hooks
        .iter()
        .find(|h| h.session_id == id)
        .map(|h| h.phase.clone())
        .unwrap_or_default();
    // Compare canonically so a mid-migration legacy phase (`running`) still
    // counts as `working` — the guard is about the effective state, not the
    // exact stored token.
    if canonical_state(&current) != canonical_state(expected) {
        return Outcome::ok(
            cmd,
            format!("session `{id}` phase unchanged (current `{current}` ≠ `{expected}`)"),
        )
        .with_data(json!({
            "sessionId": id,
            "phase": current,
            "skipped": true,
            "expected": expected,
        }));
    }
    let now = now_iso_utc();
    upsert_hook(&mut file.hooks, id, phase, &now);
    if file.schema_version.is_empty() {
        file.schema_version = STAGE_GRAPH_VERSION.to_string();
    }
    if let Err(e) = write_stage(&hooks_path(), &file) {
        return stage_error(cmd, e);
    }
    // Land the canonical live state on sessions.json (the widget file), so the
    // roster/dock render working/awaiting/idle — not just running/done.
    if let Err(e) = set_session_state(id, phase) {
        return stage_error(cmd, e);
    }
    let mut changed = vec![format!("session {id}: phase → {phase}")];
    match restage_graph() {
        Ok(g) => changed.push(g.to_string_lossy().into_owned()),
        Err(e) => return stage_error(cmd, e),
    }
    Outcome::ok(cmd, format!("session `{id}` phase = {phase}"))
        .changed(changed)
        .with_data(json!({
            "sessionId": id,
            "phase": phase,
            "updatedAt": now,
            "file": hooks_path().to_string_lossy(),
        }))
}

/// Core of `session end`: mark the session `done` (and its hook phase
/// `done`), re-stage. An unknown id is an ok no-op (matching `project remove`).
///
/// Also closes every ssh tunnel this session opened
/// (`aoide_client::tunnel::close_all_for_session`, ssh-transport lane P-S5) —
/// the FAST path for the lifecycle rule "persistent within a session,
/// ephemeral across sessions": a clean `session end` tears its own tunnels
/// down immediately rather than waiting on the reaper's
/// `sweep_orphan_tunnels` backstop, which exists precisely for the session
/// that never gets to run this exit path (SUPER+Q, SIGKILL). Runs AFTER the
/// stage lock is released, deliberately: closing a tunnel means signaling
/// and `waitpid`-reaping a real `ssh` child, which can take up to ~1s
/// (`terminate_pid`'s bounded waits) and has nothing to do with the
/// sessions/hooks stage files the lock actually guards. Best-effort — a
/// tunnel that fails to close here is still caught by the reaper, so its
/// error is never allowed to turn a successful `session end` into a
/// reported failure.
pub(in crate::graph) fn do_session_end(id: &str) -> Outcome {
    let outcome = with_stage_lock(|| do_session_end_inner(id));
    let _ = aoide_client::tunnel::close_all_for_session(id);
    // The check lane's own baseline for this session (task #139 review
    // finding: nothing ever deleted `state/checklane/<id>.json` — an effect
    // with no inverse). Same best-effort posture as the tunnel close above;
    // a session with no baseline (the lane was never configured) is a no-op.
    aoide_upkeep::checklane::forget_baseline(id);
    outcome
}
fn do_session_end_inner(id: &str) -> Outcome {
    let cmd = "session.end";
    let mut s_file: SessionsFile = match load_stage(&sessions_path()) {
        Ok(f) => f,
        Err(e) => return stage_error(cmd, e),
    };
    if !s_file.sessions.iter().any(|s| s.session_id == id) {
        return Outcome::ok(
            cmd,
            format!("session `{id}` was not registered (no change)"),
        )
        .with_data(json!({ "sessionId": id }));
    }
    if let Some(out) = refuse_app_record(cmd, id, &s_file.sessions) {
        return out;
    }
    let now = now_iso_utc();
    for s in s_file.sessions.iter_mut() {
        if s.session_id == id {
            // The ledger write (P-D8), gated on an ACTUAL transition: unlike
            // `reap_inner` (whose `reaped` set is only ever built from
            // not-`done` records, so it can never re-process an id), this
            // handler leaves the record present-and-`done` rather than
            // pruning it — so a REPEAT `session end` on the same id must
            // find it already `done` and skip the write, or every repeat
            // call would append a second line for one exit. Only the first
            // call (a genuine not-done → done transition) reaches
            // `ledger_session_exit` (`doc.rs`) — the same one shared call
            // `reap_inner`'s own exit routes through too.
            if s.state != "done" {
                ledger_session_exit(s, &now);
            }
            s.state = "done".to_string();
            // The pty is gone and so is whatever it was asking.
            clear_screen_ask(s);
            // A managed task run's end instant and its discriminated end are
            // facts the `session watch` footer and the exit-report lane both
            // read. Stamped only for a task record, and only where the child's
            // own exit path (which knows the real outcome) has not already
            // stamped them — an ordinary session's record is left exactly as it
            // was. `stopped` is the honest arm here: this path saw no status.
            if s.task.is_some() {
                if s.ended_at.is_none() {
                    s.ended_at = Some(now.clone());
                }
                if s.outcome.is_none() {
                    s.outcome = Some("stopped".to_string());
                }
            }
        }
    }
    // Cascade: remove the session's sub-agent subtree. Task nodes carry no pid or
    // window, so the liveness reaper can never clean them — the owning session's
    // end IS their lifecycle end. Collect the transitive `subagent` descendants
    // (shared with `prune_done`'s own cascade, `doomed_subagent_descendants`) and
    // drop them (the session itself stays, marked done).
    let roots: HashSet<&str> = std::iter::once(id).collect();
    let doomed = doomed_subagent_descendants(&s_file.sessions, &roots);
    s_file.sessions.retain(|s| !doomed.contains(&s.session_id));
    if s_file.schema_version.is_empty() {
        s_file.schema_version = STAGE_GRAPH_VERSION.to_string();
    }
    if let Err(e) = write_stage(&sessions_path(), &s_file) {
        return stage_error(cmd, e);
    }

    // Mirror the terminal state into hooks.json so the merged live phase agrees.
    // Reuses the SAME `now` the ledger write above stamped as this session's
    // `endedAt` — one instant for the whole roster-exit, not two clock reads.
    let mut h_file: HooksFile = match load_stage(&hooks_path()) {
        Ok(f) => f,
        Err(e) => return stage_error(cmd, e),
    };
    upsert_hook(&mut h_file.hooks, id, "done", &now);
    if h_file.schema_version.is_empty() {
        h_file.schema_version = STAGE_GRAPH_VERSION.to_string();
    }
    if let Err(e) = write_stage(&hooks_path(), &h_file) {
        return stage_error(cmd, e);
    }

    let mut changed = vec![
        format!("session {id}: state → done"),
        format!("session {id}: phase → done"),
    ];
    match restage_graph() {
        Ok(g) => changed.push(g.to_string_lossy().into_owned()),
        Err(e) => return stage_error(cmd, e),
    }
    Outcome::ok(cmd, format!("ended session `{id}`"))
        .changed(changed)
        .with_data(json!({ "sessionId": id, "file": sessions_path().to_string_lossy() }))
}

/// `session start --id <id> [--agent --cwd --window --parent]` — UPSERT a
/// running session record (idempotent; startedAt preserved on re-start).
///
/// P-D6 graph residency (`docs/architecture/AOIDED.md`'s "L4"): tries the
/// resident daemon's `dispatch` op FIRST — same registered handler, same
/// stage-write bytes, just run in the daemon's process — and falls back to
/// the direct write below byte-identically when nothing answers
/// (`aoide_client::daemon::daemon_dispatch`'s own contract).
///
/// A start on a protected live wrap is decided here, after that hop, with no
/// daemon, daemon key or `AOIDE_DAEMON_SOCKET` involved: a start that changes no
/// identity field (agent, cwd, window, parent) is an ok no-op that writes
/// nothing, and one that would change one is refused (see [`live_wrap_start`]).
pub fn session_start(inv: &Invocation) -> Outcome {
    if let Some(outcome) = aoide_client::daemon::daemon_dispatch(inv) {
        return outcome;
    }
    let id = match require_flag(inv, "id") {
        Ok(v) => v,
        Err(o) => return o,
    };
    let agent = inv.flags.get("agent").map(String::as_str);
    let cwd = inv.flags.get("cwd").map(String::as_str);
    let window = inv.flags.get("window").map(String::as_str);
    let parent = inv.flags.get("parent").map(String::as_str);
    with_stage_lock(|| {
        if let Some(outcome) = live_wrap_start(&id, agent, cwd, window, parent) {
            return outcome;
        }
        do_session_start_inner(&id, agent, cwd, window, parent, None, None, None, None, &[])
    })
}

/// The start door's verdict on a protected live wrap (`identity::
/// is_protected_live_wrap`). A start that changes none of agent, cwd, window or
/// parent answers ok and writes nothing, so the seal and the plaintext starttime
/// are kept. One that would change any of them is refused. `None` for every
/// other record. The caller holds the stage lock, so the verdict and the write
/// see the same roster.
fn live_wrap_start(
    id: &str,
    agent: Option<&str>,
    cwd: Option<&str>,
    window: Option<&str>,
    parent: Option<&str>,
) -> Option<Outcome> {
    let file: SessionsFile = load_stage(&sessions_path()).ok()?;
    let rec = file.sessions.iter().find(|s| s.session_id == id)?;
    if !super::identity::is_protected_live_wrap(rec) {
        return None;
    }
    let changed = start_changes(rec, agent, cwd, window, parent);
    if !changed.is_empty() {
        return Some(live_wrap_refused(id, &changed));
    }
    Some(live_wrap_unchanged(id, rec))
}

/// `session phase --id <id> --phase <phase>` — UPSERT the live hook phase.
///
/// P-D6 routing (see [`session_start`]'s own doc for the full contract).
pub fn session_phase(inv: &Invocation) -> Outcome {
    if let Some(outcome) = aoide_client::daemon::daemon_dispatch(inv) {
        return outcome;
    }
    let id = match require_flag(inv, "id") {
        Ok(v) => v,
        Err(o) => return o,
    };
    let phase = match require_flag(inv, "phase") {
        Ok(v) => v,
        Err(o) => return o,
    };
    if canonical_state(&phase) == "done" {
        if let Some(message) = live_wrap_refusal_for(&id) {
            return Outcome::error("session.phase", message)
                .with_data(json!({ "reason": "live-sealed-wrap", "id": id }));
        }
    }
    do_session_phase(&id, &phase)
}

/// `session end --id <id>` — mark the session done (ok no-op if unknown).
///
/// P-D6 routing (see [`session_start`]'s own doc for the full contract).
pub fn session_end(inv: &Invocation) -> Outcome {
    if let Some(outcome) = aoide_client::daemon::daemon_dispatch(inv) {
        return outcome;
    }
    let id = match require_flag(inv, "id") {
        Ok(v) => v,
        Err(o) => return o,
    };
    if let Some(message) = live_wrap_refusal_for(&id) {
        return Outcome::error("session.end", message)
            .with_data(json!({ "reason": "live-sealed-wrap", "id": id }));
    }
    do_session_end(&id)
}

/// The refusal a door returns when `id` names a protected live wrap
/// (`identity::is_protected_live_wrap`). `None` for every other record. It reads
/// no daemon key, so it answers the same with a daemon or without one.
pub(in crate::graph) fn live_wrap_refusal_for(id: &str) -> Option<String> {
    let file: SessionsFile = load_stage(&sessions_path()).ok()?;
    super::identity::live_sealed_wrap_ids(&file.sessions)
        .contains(id)
        .then(|| super::identity::live_wrap_refusal(id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::model::merged_sessions;
    use crate::graph::testutil::*;
    use crate::graph::manage::{project_add, view};
    // The reaper moved to `crate::reap`; its tests still live here (they
    // lean on session-lifecycle fixtures/helpers this module owns).
    use crate::reap::{effective_live_addresses, is_session_dead, reap};

    #[test]
    fn do_subagent_spawn_mints_a_petname_on_create() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("subagent-spawn-petname");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        do_subagent_spawn("sub:t1", "owner", "Task name", "explore", true);

        let s: SessionsFile =
            serde_json::from_str(&std::fs::read_to_string(stage.join("sessions.json")).unwrap())
                .unwrap();
        let rec = s.sessions.iter().find(|r| r.session_id == "sub:t1").unwrap();
        assert!(rec.petname.is_some(), "a freshly created subagent node must mint a petname");

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }
    #[test]
    fn iso_utc_formats_and_round_trips_through_the_reader() {
        // The Unix epoch and a known instant, formatted exactly.
        assert_eq!(iso_utc_from_epoch(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_utc_from_epoch(1_700_000_000), "2023-11-14T22:13:20Z");
        // Whatever we stamp must parse back through conductor's reader (the inverse).
        let stamp = now_iso_utc();
        let epoch = aoide_storage::time::parse_iso_utc(&stamp)
            .expect("a stamp we write is readable by the reader that consumes it");
        // And that epoch re-formats to the very same string (round-trip closed).
        assert_eq!(iso_utc_from_epoch(epoch), stamp);
    }
    #[test]
    fn upsert_session_is_idempotent_and_preserves_started_at() {
        let mut sessions: Vec<SessionRecord> = Vec::new();
        // First start: inserted, idle, agent defaulted, startedAt stamped.
        assert!(upsert_session(
            &mut sessions,
            "s1",
            None,
            Some("/w"),
            None,
            None,
            None,
            None,
            None,
            None,
            "2026-01-01T00:00:00Z"
        ));
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].agent, "claude");
        assert_eq!(sessions[0].state, "idle");
        assert_eq!(sessions[0].started_at, "2026-01-01T00:00:00Z");

        // Re-start with a NEW now + agent + conductor fields: no duplicate,
        // fields updated, but startedAt is NEVER clobbered.
        assert!(!upsert_session(
            &mut sessions,
            "s1",
            Some("melete"),
            None,
            Some("0xabc"),
            Some("parent"),
            Some(true),
            Some("/run/user/1000/aoide/session-s1.sock"),
            Some("do the thing"),
            None,
            "2026-02-02T00:00:00Z"
        ));
        assert_eq!(sessions.len(), 1, "re-start never duplicates");
        assert_eq!(sessions[0].agent, "melete");
        assert_eq!(sessions[0].window_address, "0xabc");
        assert_eq!(sessions[0].parent_session_id.as_deref(), Some("parent"));
        assert_eq!(sessions[0].conductable, Some(true));
        assert_eq!(
            sessions[0].socket.as_deref(),
            Some("/run/user/1000/aoide/session-s1.sock")
        );
        assert_eq!(sessions[0].title.as_deref(), Some("do the thing"));
        assert_eq!(
            sessions[0].started_at, "2026-01-01T00:00:00Z",
            "startedAt preserved across re-start"
        );
    }
    #[test]
    fn upsert_hook_keeps_one_bounded_record_per_session() {
        let mut hooks: Vec<HookRecord> = Vec::new();
        upsert_hook(&mut hooks, "s1", "running", "2026-01-01T00:00:01Z");
        upsert_hook(&mut hooks, "s1", "waiting", "2026-01-01T00:00:02Z");
        upsert_hook(&mut hooks, "s2", "running", "2026-01-01T00:00:03Z");
        assert_eq!(hooks.len(), 2, "one record per session id, never appended");
        let s1 = hooks.iter().find(|h| h.session_id == "s1").unwrap();
        assert_eq!(s1.phase, "waiting");
        assert_eq!(s1.updated_at, "2026-01-01T00:00:02Z");
        // merged_sessions tolerates the one-per-session shape: latest phase wins,
        // folded to the canonical vocab (`waiting` → `idle`).
        let sessions = vec![session("s1", "/w", "running", "t", None)];
        let merged = merged_sessions(&sessions, &hooks);
        assert_eq!(merged[0].state, "idle");
    }
    #[test]
    fn registration_evicts_a_same_window_agent_sibling_immediately() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("regi-evict");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

                // A phantom claude registers on a window first (the old, un-ended id). It holds a
        // dead legacy pid: a bare record with neither pid nor chain is never a twin.
        do_session_start(
            "old",
            Some("claude"),
            Some("/p"),
            Some("0xWIN"),
            None,
            None,
            None,
            None,
            Some(2_000_000_000),
        );
        // A NEW claude registers on the SAME window (the real re-id after a
        // compact/resume) — this must retire "old" immediately, no grace, no
        // waiting on the reaper.
        session_start(&flag_invocation(
            &["session", "start"],
            &[("id", "new"), ("agent", "claude"), ("cwd", "/p"), ("window", "0xWIN")],
        ));

        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        let ids: Vec<&str> = s.sessions.iter().map(|r| r.session_id.as_str()).collect();
        assert!(ids.contains(&"new"), "the newly-registered session survives");
        assert!(!ids.contains(&"old"), "the same-window sibling was evicted at registration");

        // A conducted SHELL registering onto the SAME window as an agent must
        // NEVER evict it — that is the normal "shell hosts claude" pairing, not a
        // duplicate. Re-seed "old" as a fresh agent, then register a shell.
        session_start(&flag_invocation(
            &["session", "start"],
            &[("id", "again", ), ("agent", "claude"), ("cwd", "/p"), ("window", "0xWIN2")],
        ));
        do_session_start(
            "shellhost",
            Some("shell"),
            Some("/p"),
            Some("0xWIN2"),
            None,
            Some(true), // conductable → is_agent_kind() is false for this record
            None,
            None,
            None,
        );
        let s2: SessionsFile = load_stage(&sessions_path()).unwrap();
        let ids2: Vec<&str> = s2.sessions.iter().map(|r| r.session_id.as_str()).collect();
        assert!(ids2.contains(&"again"), "a conducted shell never evicts its hosted agent");
        assert!(ids2.contains(&"shellhost"));

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }
    #[test]
    fn registration_never_evicts_a_conducted_pty_host() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("regi-host");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        // The wrapper: `aoide conduct -- kimi` registers its own record —
        // agent name = the child's basename, conductable + socket (the
        // control-socket owner). upsert_session classifies it kind="agent"
        // from that basename; it is the HOST, not a foreground agent.
        do_session_start(
            "conduct-1",
            Some("kimi"),
            Some("/p"),
            Some("0xWIN3"),
            None,
            Some(true),
            Some("/run/aoide/conduct-1.sock"),
            None,
            None,
        );
        // kimi's SessionStart hook fires inside the conducted child:
        // agent-kind, SAME window, parent = the wrapper id (threaded via
        // AOIDE_SESSION_ID). Before the host carve-outs this registration
        // evicted the wrapper — `send --id conduct-1` then failed
        // "unknown session" and the session could never be commanded.
        do_session_start(
            "kimi-hook-1",
            Some("kimi"),
            Some("/p"),
            Some("0xWIN3"),
            Some("conduct-1"),
            None,
            None,
            None,
            None,
        );

        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        let ids: Vec<&str> = s.sessions.iter().map(|r| r.session_id.as_str()).collect();
        assert!(
            ids.contains(&"conduct-1"),
            "the conducted PTY host survives its child's SessionStart"
        );
        assert!(
            ids.contains(&"kimi-hook-1"),
            "the hook session registers alongside its host"
        );
        // The record really is the shape that fooled the old
        // published-kind-wins rule: kind="agent" AND conductable.
        let host = s.sessions.iter().find(|r| r.session_id == "conduct-1").unwrap();
        assert_eq!(host.kind.as_deref(), Some("agent"));
        assert_eq!(host.conductable, Some(true));
        assert_eq!(host.state, "idle", "still live, not evicted-done");

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }
    /// The exact live topology diagnosed for task #89: a windowed terminal
    /// wrap hosts claude (same window); claude's own shell tool runs `graph
    /// spawn` for kimi with NO `--parent`, launching a nested HEADLESS wrap
    /// whose own hook session must land as claude's CHILD — not, via a stale
    /// `AOIDE_SESSION_ID` env fallback, as the TERMINAL's sibling. All three
    /// User-decided fixes this regression pins at once: (1) kimi is
    /// windowless by construction (never backfilled to the terminal's
    /// window); (3) the nested wrap parents under claude via the `/proc`
    /// ancestry ↔ `hookAncestry` walk, not the terminal; (2) even forced onto
    /// the SAME window as claude (the pre-fix degraded symptom), the widened
    /// lineage carve-out never evicts either side, because they are
    /// grandparent/grandchild through the nested wrap — never unrelated
    /// same-window twins.
    #[test]
    fn nested_headless_lineage_is_windowless_ancestry_parented_and_never_evicted() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("lineage-nested");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        // Terminal wrap: windowed, conducted (`aoide conduct` on the shell).
        do_session_start(
            "term-wrap", Some("shell"), Some("/w"), Some("0xWIN"), None, Some(true), None, None,
            None,
        );
        // claude's own hook SessionStart: same window (it runs IN that
        // terminal), parented under the wrap.
        do_session_start(
            "claude", Some("claude"), Some("/w"), Some("0xWIN"), Some("term-wrap"), None, None,
            None, None,
        );
        // claude's own `hookAncestry`, landed as its real SessionStart lands
        // it: the hook's dead pids, then the harness and the live tail of THIS
        // test process's own ancestry — the ancestry walk below has no fake
        // `/proc` to read from, so the mechanism is exercised with genuine pids.
        let my_ancestry = crate::graph::window::pid_ancestry(std::process::id() as i32);
        do_hook_session_start(aoide_protocol::Door::Cli,
            "claude", Some("claude"), Some("/w"), Some("0xWIN"), Some("term-wrap"), None,
            &[DEAD, DEAD + 1, my_ancestry[1], my_ancestry[2]],
        );

        // Part 3 — automatic parenting: `spawn`'s own re-exec'd
        // `conduct --headless` for kimi carries no explicit `--parent`. The
        // ancestry walk must resolve to claude, not fall through to an env
        // fallback that would carry the TERMINAL's id (the "spawned wraps
        // parent under the terminal as siblings" bug).
        let snapshot = load_stage::<SessionsFile>(&sessions_path()).unwrap().sessions;
        let resolved = crate::graph::window::resolve_registration_parent(None, "kimi-wrap", &snapshot);
        assert_eq!(
            resolved.as_deref(),
            Some("claude"),
            "kimi's nested wrap must parent under claude via ancestry, never the terminal"
        );
        // The nested wrap itself: headless (no window), conducted. Registered
        // the same two-step way the real `session_conduct` command does it —
        // `do_session_start` first, then `stamp_headless` right after (see
        // `conduct.rs`'s own `headless_conduct_registration_stamps_the_
        // permanent_headless_marker`, which exercises the REAL command end to
        // end, discovery gate included; this test reuses the same
        // stage-writer `stamp_headless` calls, not a raw field literal, so
        // it stays honest about what actually gets written).
        do_session_start(
            "kimi-wrap", Some("kimi"), Some("/w"), None, resolved.as_deref(), Some(true), None,
            None, None,
        );
        stamp_headless("kimi-wrap");

        // Part 1 — windowless lineage: kimi-wrap is a conducted wrap with an
        // EMPTY window, so kimi's own hook session (its child) is windowless
        // BY CONSTRUCTION and must never backfill to the enclosing
        // terminal's window.
        let snapshot = load_stage::<SessionsFile>(&sessions_path()).unwrap().sessions;
        assert!(
            crate::graph::window::windowless_by_lineage_from_parent(Some("kimi-wrap"), &snapshot),
            "kimi must be windowless: its own wrap is headless"
        );
        // claude, by contrast, anchors in a WINDOWED wrap directly — its own
        // backfill stays live (today's behavior, unchanged).
        assert!(!crate::graph::window::windowless_by_lineage_from_parent(Some("term-wrap"), &snapshot));

        // The re-poison scenario (task #89): even if window discovery
        // (or the shellbridge listener) somehow still stamped a STRAY window
        // onto kimi-wrap's own record — the exact bug the gate at
        // `conduct.rs:825` and the listener skip now prevent — the
        // PERMANENT `headless` marker keeps kimi's own backfill check
        // windowless regardless. Force the stray value directly on the
        // stage file (bypassing every write path) to prove the marker alone
        // decides, not `windowAddress` emptiness.
        with_stage_lock(|| {
            let mut f: SessionsFile = load_stage(&sessions_path()).unwrap();
            for s in f.sessions.iter_mut() {
                if s.session_id == "kimi-wrap" {
                    s.window_address = "0xWIN".to_string(); // stray/corrupted
                }
            }
            write_stage(&sessions_path(), &f).unwrap();
        });
        let poisoned = load_stage::<SessionsFile>(&sessions_path()).unwrap().sessions;
        assert!(
            crate::graph::window::windowless_by_lineage_from_parent(Some("kimi-wrap"), &poisoned),
            "the headless marker must override even a corrupted, non-empty windowAddress"
        );
        // Restore for the rest of the test — the corruption above was a
        // deliberate, isolated probe, not this test's real topology.
        with_stage_lock(|| {
            let mut f: SessionsFile = load_stage(&sessions_path()).unwrap();
            for s in f.sessions.iter_mut() {
                if s.session_id == "kimi-wrap" {
                    s.window_address = String::new();
                }
            }
            write_stage(&sessions_path(), &f).unwrap();
        });

        // Part 2 — lineage-safe eviction: register kimi's own hook session,
        // deliberately forced (the pre-fix DEGRADED symptom — what actually
        // happened live before part 1 existed) onto the SAME window as
        // claude. The widened carve-out must protect BOTH sides: kimi and
        // claude are grandchild/grandparent through kimi-wrap, never
        // unrelated same-window twins.
        let out = do_session_start(
            "kimi", Some("kimi"), Some("/w"), Some("0xWIN"), Some("kimi-wrap"), None, None, None,
            None,
        );
        assert_eq!(out.status, aoide_protocol::output::Status::Ok);
        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        for id in ["term-wrap", "claude", "kimi-wrap", "kimi"] {
            let rec = s
                .sessions
                .iter()
                .find(|r| r.session_id == id)
                .unwrap_or_else(|| panic!("{id} missing from sessions.json"));
            assert_ne!(rec.state, "done", "{id} must survive — lineage-safe eviction");
        }

        // The legitimate case, regression-pinned alongside the fix: a
        // genuine compact/resume twin holding the SAME window with NO
        // lineage relation to anything already there must still collapse
        // instantly — the widened carve-out protects only an ACTUAL lineage
        // member, never every same-window occupant.
        // The resumed twin is the same harness position under a new id: its
        // chain shares claude's depth (the desktop-sibling dead-process case).
        let out2 = do_hook_session_start(aoide_protocol::Door::Cli,
            "claude-resumed", Some("claude"), Some("/w"), Some("0xWIN"), None, None,
            &[DEAD + 2, DEAD + 3, my_ancestry[1], my_ancestry[2]],
        );
        assert_eq!(out2.status, aoide_protocol::output::Status::Ok);
        let s2: SessionsFile = load_stage(&sessions_path()).unwrap();
        let ids2: Vec<&str> = s2.sessions.iter().map(|r| r.session_id.as_str()).collect();
        assert!(ids2.contains(&"claude-resumed"), "the new twin survives");
        assert!(
            !ids2.contains(&"claude"),
            "the old same-window twin with no lineage relation to the new one still collapses (evicted, then pruned)"
        );
        assert!(
            ids2.contains(&"kimi"),
            "kimi survives: it names no process (no chain, no pid), so a same-window sibling does not retire it"
        );

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }
    /// A pid no process holds: `kill` answers ESRCH, so it is the dead half of a chain.
    const DEAD: i32 = 2_000_000_000;

    /// A hosted agent in window `0xW`. Its chain and pid are the only facts the
    /// pure tests below vary.
    fn hosted(id: &str, chain: &[i32], pid: Option<u32>) -> SessionRecord {
        let mut r = session(id, "/w", "working", "2026-10-10T00:00:00Z", None);
        r.window_address = "0xW".into();
        r.hook_ancestry = chain.to_vec();
        r.pid = pid;
        r
    }

    /// Fake liveness: exactly the pids named here answer as alive.
    fn live(pids: &'static [u32]) -> impl Fn(u32) -> bool {
        move |p| pids.contains(&p)
    }

    #[test]
    fn desktop_siblings_live_apart_are_never_superseded() {
        // Two headless claudes under one app (867785, under systemd 1454). The
        // old one's harness (1137986) is still alive, so the two are siblings.
        let old = hosted("sib-a", &[DEAD, DEAD + 1, 1137986, 867785, 1454], None);
        let new = [DEAD + 2, DEAD + 3, 1651171, 867785, 1454];
        assert!(!superseded_by(&old, &new, None, live(&[1137986, 1651171, 867785, 1454])));
    }

    #[test]
    fn a_clear_twin_in_the_same_harness_is_superseded() {
        // `/clear`: the same claude process (500) under a new hook. Shell and
        // kitty are shared, and the old hook is gone.
        let old = hosted("before", &[DEAD, DEAD + 1, 500, 600, 700], None);
        let new = [DEAD + 2, DEAD + 3, 500, 600, 700];
        assert!(superseded_by(&old, &new, None, live(&[500, 600, 700])));
    }

    #[test]
    fn a_same_tab_successor_is_superseded_once_the_old_harness_is_dead() {
        // The old claude (DEAD+4) exited; a new claude (800) runs at the same
        // depth under the same shell (600) and kitty (700).
        let old = hosted("old", &[DEAD, DEAD + 1, DEAD + 4, 600, 700], None);
        let new = [DEAD + 2, DEAD + 3, 800, 600, 700];
        assert!(superseded_by(&old, &new, None, live(&[600, 700, 800])));
    }

    #[test]
    fn a_session_nested_beneath_a_live_one_is_not_a_twin() {
        // The old claude (500) is alive, and the new session runs beneath it:
        // the shared claude sits at depth 2 in the old chain and depth 3 in the new.
        let old = hosted("outer", &[DEAD, DEAD + 1, 500, 600, 700], None);
        let new = [DEAD + 2, DEAD + 3, 900, 500, 600, 700];
        assert!(!superseded_by(&old, &new, None, live(&[500, 600, 700, 900])));
    }

    #[test]
    fn a_legacy_record_holding_the_owner_pid_is_still_superseded() {
        // A pre-chain record whose pid is the kitty window's pid, which the new
        // chain holds too: today's rule, unchanged.
        let old = hosted("legacy", &[], Some(700));
        assert!(superseded_by(&old, &[800, 600, 700], None, live(&[700, 600, 800])));
    }

    #[test]
    fn a_legacy_record_with_a_live_payload_pid_lives_apart() {
        // A harness that reports its own pid (pi) and is still running.
        let old = hosted("pi", &[], Some(4242));
        assert!(!superseded_by(&old, &[800, 600, 700], None, live(&[4242, 600, 700, 800])));
    }

    #[test]
    fn a_reused_pid_keeps_the_record() {
        // The chain's middle pid (555) was reused by an unrelated live process.
        // "Alive" keeps the record: the fail-safe direction.
        let old = hosted("reused", &[DEAD, 555, 700], None);
        assert!(!superseded_by(&old, &[800, 700], None, live(&[555, 700, 800])));
    }

    #[test]
    fn ancestry_moves_off_a_dead_chain_whatever_the_seal() {
        let plain = hosted("plain", &[DEAD, DEAD + 1, DEAD + 2], None);
        assert!(ancestry_may_move(&plain, &[800, 700], live(&[700, 800])));
        let mut sealed = hosted("sealed", &[DEAD, DEAD + 1, DEAD + 2], Some(DEAD as u32));
        sealed.seal = Some("seal".into());
        sealed.ever_sealed = Some(true);
        sealed.conductable = Some(true);
        assert!(ancestry_may_move(&sealed, &[800, 700], live(&[700, 800])));
    }

    #[test]
    fn ancestry_refuses_only_while_its_own_pid_lives_outside_the_new_chain() {
        // Pid-less: follows its latest SessionStart, even over a live stored process.
        let pidless = hosted("pidless", &[DEAD, 555, 700], None);
        assert!(ancestry_may_move(&pidless, &[800, 700], live(&[555, 700, 800])));
        // Anchored to a live pid of its own outside the new chain: held.
        let anchored = hosted("anchored", &[DEAD, 555, 700], Some(555));
        assert!(!ancestry_may_move(&anchored, &[800, 700], live(&[555, 700, 800])));
        // The anchor is dead, or sits in the new chain: it moves.
        assert!(ancestry_may_move(&anchored, &[800, 700], live(&[700, 800])));
        assert!(ancestry_may_move(&anchored, &[800, 555], live(&[555, 700, 800])));
        let fresh = hosted("fresh", &[], None);
        assert!(ancestry_may_move(&fresh, &[800, 700], live(&[700, 800])));
    }

    #[test]
    fn a_stale_chain_with_only_dead_pids_heals_and_a_live_chain_is_never_rewritten() {
        // The record's harness exited: every stored pid is dead.
        let stale = hosted("all-dead", &[DEAD, DEAD + 1, DEAD + 2], None);
        let first = [500, 600, 700];
        assert!(chain_is_stale(&stale, &first, live(&[500, 600, 700])));
        // Healed: the next turn from the same harness finds the stored chain live and
        // leaves it alone.
        let mut healed = stale.clone();
        healed.hook_ancestry = first.to_vec();
        assert!(!chain_is_stale(&healed, &first, live(&[500, 600, 700])));
    }

    #[test]
    fn a_stale_desktop_chain_heals_onto_its_live_harness_and_survives_a_sibling() {
        // The desktop shape: the record's harness (DEAD+2) is dead, the window owner
        // (867785) and systemd (1454) are alive, and the live harness 1137986 runs
        // under the owner but was never recorded.
        let stale = hosted("stale", &[DEAD + 2, 867785, 1454], None);
        let harness_chain = [1137986, 867785, 1454];
        let alive = live(&[1137986, 867785, 1454]);
        let sibling = [1651171, 867785, 1454];
        let sibling_alive = live(&[1137986, 1651171, 867785, 1454]);
        assert!(superseded_by(&stale, &sibling, None, &sibling_alive), "unhealed, the sibling retires it");
        assert!(chain_is_stale(&stale, &harness_chain, &alive));
        let mut healed = stale.clone();
        healed.hook_ancestry = harness_chain.to_vec();
        assert!(!superseded_by(&healed, &sibling, None, &sibling_alive));
    }

    #[test]
    fn a_live_pid_of_the_records_own_outside_the_new_chain_blocks_the_heal() {
        // Anchored to a live pid of its own (555), which the new chain does not hold.
        let anchored = hosted("anchored", &[DEAD, DEAD + 1, DEAD + 2, 700], Some(555));
        assert!(!chain_is_stale(&anchored, &[900, 700], live(&[555, 700, 900])));
        // A live process of the stored chain outside the new chain blocks it too.
        let held = hosted("held", &[DEAD, 555, 700], None);
        assert!(!chain_is_stale(&held, &[900, 700], live(&[555, 700, 900])));
    }

    #[test]
    fn a_chain_brings_no_heal_without_a_live_harness_of_its_own() {
        // Only the shared owner is alive and recorded: nothing new to record.
        let stale = hosted("stale", &[DEAD, DEAD + 1, DEAD + 2, 700], None);
        assert!(!chain_is_stale(&stale, &[700], live(&[700])));
        // A record with no stored chain follows registration, never this rule.
        let fresh = hosted("fresh", &[], None);
        assert!(!chain_is_stale(&fresh, &[900], live(&[900])));
    }

    /// A chainless, pid-less record names no process, so a same-window sibling does
    /// not retire it (the `aoide session start --id X --window W` shape).
    #[test]
    fn a_bare_record_is_never_retired_by_a_same_window_sibling() {
        let bare = hosted("bare", &[], None);
        let sibling = [DEAD + 2, DEAD + 3, 800, 700];
        assert!(!superseded_by(&bare, &sibling, None, live(&[800, 700])));
    }

    /// A stored chain whose pids are all dead and share none with the new chain is
    /// not a twin: no process position is shared, so nothing says they are one harness.
    #[test]
    fn a_chain_sharing_no_pid_with_the_new_one_is_not_a_twin() {
        let old = hosted("disjoint", &[DEAD, DEAD + 1, DEAD + 2], None);
        let new = [DEAD + 5, DEAD + 6, 800, 700];
        assert!(!superseded_by(&old, &new, None, live(&[800, 700])));
    }

    /// `aoide session start --id X --window W`: no pid, no chain. A same-window
    /// sibling registration leaves it standing, since nothing names its process.
    #[test]
    fn a_bare_session_start_survives_a_same_window_sibling() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("bare-start");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        do_session_start("x", Some("claude"), Some("/w"), Some("0xW"), None, None, None, None, None);
        do_hook_session_start(aoide_protocol::Door::Cli, "y", Some("claude"), Some("/w"), Some("0xW"), None, None, &[DEAD, DEAD + 1]);
        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert!(
            s.sessions.iter().any(|r| r.session_id == "x" && r.state != "done"),
            "a bare record names no process, so the sibling does not retire it"
        );

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// Seven harnesses in one app window, each with its own live process beneath
    /// the shared app pid (the live desktop shape). None is a twin of another.
    #[test]
    fn seven_hosted_sessions_in_one_window_register_without_evicting_each_other() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("seven-hosted");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        let me = std::process::id() as i32;
        let mut children: Vec<std::process::Child> = (0..7)
            .map(|_| std::process::Command::new("sleep").arg("30").spawn().unwrap())
            .collect();
        for (i, child) in children.iter().enumerate() {
            let i = i as i32;
            let chain = [DEAD + i, DEAD + 100 + i, child.id() as i32, me];
            do_hook_session_start(aoide_protocol::Door::Cli,
                &format!("h{i}"), Some("claude"), Some("/w"), Some("0xW"), None, None, &chain,
            );
        }
        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        let live_in_window = s
            .sessions
            .iter()
            .filter(|r| r.window_address == "0xW" && r.state != "done")
            .count();
        for c in children.iter_mut() {
            let _ = c.kill();
            let _ = c.wait();
        }
        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
        assert_eq!(live_in_window, 7, "all seven hosted sessions survive their siblings' registrations");
    }

    #[test]
    fn a_clear_twin_still_evicts_at_registration() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("clear-twin");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        let a = crate::graph::window::pid_ancestry(std::process::id() as i32);
        do_hook_session_start(aoide_protocol::Door::Cli,
            "before", Some("claude"), Some("/w"), Some("0xW"), None, None,
            &[DEAD, DEAD + 1, a[1], a[2]],
        );
        do_hook_session_start(aoide_protocol::Door::Cli,
            "after", Some("claude"), Some("/w"), Some("0xW"), None, None,
            &[DEAD + 2, DEAD + 3, a[1], a[2]],
        );
        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        let ids: Vec<&str> = s.sessions.iter().map(|r| r.session_id.as_str()).collect();
        assert!(ids.contains(&"after"));
        assert!(!ids.contains(&"before"), "the same-harness twin is superseded at registration");

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn a_same_tab_successor_still_evicts_at_registration() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("same-tab");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        // The old claude's process (DEAD+4) is gone; the shell (a[1]) and the
        // window's own process (a[2]) are the same, at the same depth.
        let a = crate::graph::window::pid_ancestry(std::process::id() as i32);
        do_hook_session_start(aoide_protocol::Door::Cli,
            "old-tab", Some("claude"), Some("/w"), Some("0xW"), None, None,
            &[DEAD, DEAD + 1, DEAD + 4, a[1], a[2]],
        );
        do_hook_session_start(aoide_protocol::Door::Cli,
            "new-tab", Some("claude"), Some("/w"), Some("0xW"), None, None,
            &[DEAD + 2, DEAD + 3, DEAD + 5, a[1], a[2]],
        );
        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        let ids: Vec<&str> = s.sessions.iter().map(|r| r.session_id.as_str()).collect();
        assert!(ids.contains(&"new-tab"));
        assert!(!ids.contains(&"old-tab"), "the dead predecessor in the same tab is superseded");

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn a_nested_unlinked_parent_survives_registration() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("nested-unlinked");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        // The outer claude's harness is a[1] (alive, at depth 2). The new session
        // runs beneath it: the same harness sits at depth 4 in the new chain.
        let a = crate::graph::window::pid_ancestry(std::process::id() as i32);
        do_hook_session_start(aoide_protocol::Door::Cli,
            "outer", Some("claude"), Some("/w"), Some("0xW"), None, None,
            &[DEAD, DEAD + 1, a[1], a[2]],
        );
        do_hook_session_start(aoide_protocol::Door::Cli,
            "inner", Some("claude"), Some("/w"), Some("0xW"), None, None,
            &[DEAD + 2, DEAD + 3, DEAD + 5, DEAD + 6, a[1], a[2]],
        );
        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        let outer = s.sessions.iter().find(|r| r.session_id == "outer");
        assert!(outer.is_some_and(|r| r.state != "done"), "the live outer session survives");

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// The chain lands on registration. A pid-less record follows its latest
    /// SessionStart; a record anchored to a live pid of its own keeps its chain
    /// until that pid leaves the new chain.
    #[test]
    fn a_hook_chain_lands_on_registration_and_only_a_live_anchor_holds_it() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("chain-lands");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        let a = crate::graph::window::pid_ancestry(std::process::id() as i32);
        do_hook_session_start(aoide_protocol::Door::Cli,
            "h", Some("claude"), Some("/w"), Some("0xW"), None, None,
            &[DEAD, DEAD + 1, a[1], a[2]],
        );
        let chain_of = || {
            load_stage::<SessionsFile>(&sessions_path())
                .unwrap()
                .sessions
                .iter()
                .find(|r| r.session_id == "h")
                .unwrap()
                .hook_ancestry
                .clone()
        };
        assert_eq!(chain_of(), vec![DEAD, DEAD + 1, a[1], a[2]]);

        // A resumed run that shares the harness moves the chain.
        do_hook_session_start(aoide_protocol::Door::Cli,
            "h", Some("claude"), Some("/w"), Some("0xW"), None, None,
            &[DEAD + 2, DEAD + 3, a[1], a[2]],
        );
        assert_eq!(chain_of(), vec![DEAD + 2, DEAD + 3, a[1], a[2]]);

        // Pid-less: the latest SessionStart's chain wins, even though a[1] is alive.
        do_hook_session_start(aoide_protocol::Door::Cli,
            "h", Some("claude"), Some("/w"), Some("0xW"), None, None,
            &[DEAD + 4, DEAD + 5],
        );
        assert_eq!(chain_of(), vec![DEAD + 4, DEAD + 5], "a pid-less record follows its latest SessionStart");

        // Anchored to a[1], alive: a registration whose chain drops a[1] is refused.
        do_hook_session_start(aoide_protocol::Door::Cli,
            "anchored", Some("claude"), Some("/w"), Some("0xA"), None, Some(a[1] as u32),
            &[DEAD, a[1], a[2]],
        );
        do_hook_session_start(aoide_protocol::Door::Cli,
            "anchored", Some("claude"), Some("/w"), Some("0xA"), None, None,
            &[DEAD + 6, DEAD + 7],
        );
        let anchored_chain = load_stage::<SessionsFile>(&sessions_path())
            .unwrap()
            .sessions
            .iter()
            .find(|r| r.session_id == "anchored")
            .unwrap()
            .hook_ancestry
            .clone();
        assert_eq!(anchored_chain, vec![DEAD, a[1], a[2]], "a live pid of its own holds the chain");

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn stamp_attested_parent_rewrites_a_changed_parent_and_refuses_a_cycle() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("stamp-attested-parent");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        write_stage(
            &sessions_path(),
            &SessionsFile {
                sessions: vec![
                    session("child", "/w", "idle", "t", Some("old")),
                    session("old", "/w", "idle", "t", None),
                    session("new", "/w", "idle", "t", None),
                ],
                ..Default::default()
            },
        )
        .unwrap();

        stamp_attested_parent("child", "new");
        let file: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert_eq!(
            file.sessions
                .iter()
                .find(|s| s.session_id == "child")
                .unwrap()
                .parent_session_id
                .as_deref(),
            Some("new"),
            "a changed attested parent rewrites the record"
        );

        let before = std::fs::read(sessions_path()).unwrap();
        stamp_attested_parent("child", "new");
        assert_eq!(
            std::fs::read(sessions_path()).unwrap(),
            before,
            "an identical re-stamp writes nothing — the file stays byte-identical"
        );

        // "new" is now downstream of "child" (child → new); parenting "new"
        // under "child" would close the cycle — refused, "new" keeps its
        // own (absent) parent untouched.
        stamp_attested_parent("new", "child");
        let file: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert_eq!(
            file.sessions
                .iter()
                .find(|s| s.session_id == "new")
                .unwrap()
                .parent_session_id,
            None,
            "a cycling parent is refused"
        );

        // An unknown id is a silent no-op — never a panic, never a write.
        let before = std::fs::read(sessions_path()).unwrap();
        stamp_attested_parent("missing", "new");
        assert_eq!(std::fs::read(sessions_path()).unwrap(), before);

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }
    #[test]
    fn stamp_attested_spawner_sets_and_clears_and_a_moved_edge_drops_it() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("stamp-attested-spawner");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        write_stage(
            &sessions_path(),
            &SessionsFile { sessions: vec![session("w", "/w", "idle", "t", Some("p"))], ..Default::default() },
        )
        .unwrap();
        let stamped = || {
            load_stage::<SessionsFile>(&sessions_path()).unwrap().sessions[0].attested_spawner.clone()
        };

        stamp_attested_spawner("w", Some("p"));
        assert_eq!(stamped().as_deref(), Some("p"));
        let before = std::fs::read(sessions_path()).unwrap();
        stamp_attested_spawner("w", Some("p"));
        assert_eq!(std::fs::read(sessions_path()).unwrap(), before, "change-only");

        // `session start --id w --parent victim`: the edge moves and the stamp
        // goes with it, so setting the edge back to "p" revives nothing.
        do_session_start("w", None, None, None, Some("victim"), None, None, None, None);
        let rec = load_stage::<SessionsFile>(&sessions_path()).unwrap().sessions.remove(0);
        assert_eq!(rec.parent_session_id.as_deref(), Some("victim"));
        assert_eq!(rec.attested_spawner, None, "a moved edge drops its stamp");
        do_session_start("w", None, None, None, Some("p"), None, None, None, None);
        assert_eq!(stamped(), None, "the old parent set back is unattested");

        // A registration the kernel did not back clears it.
        stamp_attested_spawner("w", None);
        assert_eq!(stamped(), None);
        stamp_attested_spawner("missing", Some("p"));

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }
    #[test]
    fn session_start_upserts_restages_and_anchors_under_a_project() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("sess-start");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        // The anchor root must now be a REAL absolute dir (project_add
        // rejects anything else), so the project + session cwd live under
        // the per-test stage dir instead of the fictional /home/k/Aoide.
        let proj = stage.join("proj");
        std::fs::create_dir_all(proj.join("sub")).unwrap();
        // `project add` is DAEMON-OWNED now (`manage.rs`'s `local_daemon`):
        // stamp `Door::Daemon` to exercise the local path with no daemon
        // running, same as `manage.rs`'s own handler tests.
        let mut proj_inv = invocation(&["project", "add"], &["aoide", proj.to_str().unwrap()]);
        proj_inv.door = aoide_protocol::Door::Daemon;
        project_add(&proj_inv);
        let out = session_start(&flag_invocation(
            &["session", "start"],
            &[("id", "s1"), ("cwd", proj.join("sub").to_str().unwrap())],
        ));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok);

        let s_file: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert_eq!(s_file.sessions.len(), 1);
        assert_eq!(s_file.sessions[0].state, "idle");
        let started = s_file.sessions[0].started_at.clone();
        assert!(!started.is_empty());

        // Every mutation re-stages: graph.json carries the session node + the
        // project-anchored edge, and equals exactly what `view` computes.
        let staged: Value =
            serde_json::from_str(&std::fs::read_to_string(stage.join("graph.json")).unwrap())
                .unwrap();
        assert!(staged["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["id"] == "session:s1"));
        assert!(staged["edges"].as_array().unwrap().iter().any(|e| {
            e["from"] == "project:aoide" && e["to"] == "session:s1" && e["kind"] == "anchors"
        }));
        let view = view(&invocation(&["graph"], &[]));
        assert_eq!(&staged, view.data.as_ref().unwrap());

        // Re-start updates the agent but preserves startedAt and never dupes.
        session_start(&flag_invocation(
            &["session", "start"],
            &[("id", "s1"), ("agent", "melete")],
        ));
        let s2: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert_eq!(s2.sessions.len(), 1);
        assert_eq!(s2.sessions[0].agent, "melete");
        assert_eq!(s2.sessions[0].started_at, started);

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }
    #[test]
    fn session_start_refuses_a_cyclic_parent() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("sess-cycle");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        session_start(&flag_invocation(&["session", "start"], &[("id", "a")]));
        session_start(&flag_invocation(
            &["session", "start"],
            &[("id", "b"), ("parent", "a")],
        ));
        // a parented under b would close b→a→…: refused (exit 1), no mutation.
        let out = session_start(&flag_invocation(
            &["session", "start"],
            &[("id", "a"), ("parent", "b")],
        ));
        assert_eq!(out.status, aoide_protocol::output::Status::Error);
        assert_eq!(out.data.unwrap()["reason"], "cycle");
        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert!(s.sessions.iter().find(|x| x.session_id == "a").unwrap().parent_session_id.is_none());

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }
    #[test]
    fn session_end_unknown_id_is_ok_noop() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("sess-end-unknown");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        let out = session_end(&flag_invocation(
            &["session", "end"],
            &[("id", "ghost")],
        ));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok);
        assert!(out.changed.is_empty(), "unknown id → no change");
        // No session file was written (nothing to end).
        assert!(!sessions_path().exists());

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }
    #[test]
    fn session_end_appends_exactly_one_ledger_line() {
        // P-D8: the `do_session_end` half of "clean end and reap each
        // produce one ledger line, never two" — `reap`'s own half lives in
        // `reap.rs`'s test suite; both route through the SAME
        // `ledger_session_exit` (`doc.rs`).
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR"]);
        let stage = unique_stage("sess-end-ledger");
        let state = stage.join("state");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("AOIDE_STATE_DIR", &state);

        session_start(&flag_invocation(
            &["session", "start"],
            &[("id", "end-ledger-1"), ("agent", "claude"), ("cwd", "/w")],
        ));
        let out = session_end(&flag_invocation(
            &["session", "end"],
            &[("id", "end-ledger-1")],
        ));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);

        let lines = aoide_storage::ledger::read_ledger().unwrap();
        let mine: Vec<_> = lines.iter().filter(|l| l.session_id == "end-ledger-1").collect();
        assert_eq!(mine.len(), 1, "exactly one ledger line, never two: {lines:?}");
        assert_eq!(mine[0].agent, "claude");
        assert_eq!(mine[0].cwd, "/w");
        assert!(!mine[0].ended_at.is_empty());

        // Ending it again (a repeat/idempotent call) must NOT append a
        // second line — the record is already gone from the LIVE (not-done)
        // set the write loop iterates, so the second call is a genuine no-op
        // for the ledger, same as it already is for sessions.json.
        let _ = session_end(&flag_invocation(
            &["session", "end"],
            &[("id", "end-ledger-1")],
        ));
        let lines2 = aoide_storage::ledger::read_ledger().unwrap();
        let mine2: usize = lines2.iter().filter(|l| l.session_id == "end-ledger-1").count();
        assert_eq!(mine2, 1, "a repeat `session end` must not double-append");

        let _ = std::fs::remove_dir_all(&stage);
    }
    #[test]
    fn a_session_end_clears_the_screen_question_and_the_sudo_badge_together() {
        // M2: a `sudo` wrap whose prompt is still on screen when it is ended
        // must not keep the lock badge after its question is gone. Both go
        // through one helper (`clear_screen_ask`), so neither can outlive the
        // other.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR"]);
        let stage = unique_stage("sess-end-sudo");
        let state = stage.join("state");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("AOIDE_STATE_DIR", &state);

        session_start(&flag_invocation(
            &["session", "start"],
            &[("id", "sudo-end-1"), ("agent", "claude"), ("cwd", "/w")],
        ));
        let mut file: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec = file.sessions.iter_mut().find(|s| s.session_id == "sudo-end-1").unwrap();
        rec.state = "awaiting".to_string();
        rec.question = Some("[sudo] password for khoa:".to_string());
        rec.needs_sudo = Some(true);
        write_stage(&sessions_path(), &file).unwrap();

        let out = session_end(&flag_invocation(&["session", "end"], &[("id", "sudo-end-1")]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);

        let file: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec = file.sessions.iter().find(|s| s.session_id == "sudo-end-1").unwrap();
        assert_eq!(
            (rec.state.as_str(), rec.question.as_deref(), rec.needs_sudo),
            ("done", None, None),
            "the question and its lock badge end together: {rec:?}"
        );

        let _ = std::fs::remove_dir_all(&stage);
    }
    #[test]
    fn session_end_closes_its_own_tunnels_and_spares_another_sessions() {
        // The ssh-transport lane's P-S5 fast path: a clean `session end`
        // closes every tunnel THIS session opened
        // (`aoide_client::tunnel::close_all_for_session`) without touching a
        // different session's — the reaper's `sweep_orphan_tunnels` is only
        // the backstop for the session that never gets to run this path.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR", "XDG_RUNTIME_DIR"]);
        let stage = unique_stage("sess-end-tunnels");
        let state = stage.join("state");
        let runtime = stage.join("runtime");
        std::fs::create_dir_all(runtime.join("aoide")).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("AOIDE_STATE_DIR", &state);
        std::env::set_var("XDG_RUNTIME_DIR", &runtime);

        session_start(&flag_invocation(
            &["session", "start"],
            &[("id", "end-tunnel-1"), ("agent", "claude"), ("cwd", "/w")],
        ));

        let tunnel_record = |session_id: &str, key: &str| aoide_storage::tunnel::TunnelRecord {
            schema_version: aoide_storage::tunnel::TUNNEL_VERSION.to_string(),
            session_id: session_id.to_string(),
            key: key.to_string(),
            ssh_target: "ssh://user@host".to_string(),
            local_port: 40010,
            remote_host: "127.0.0.1".to_string(),
            remote_port: 8710,
            // A pid nothing on the box holds — `close`'s guarded kill is
            // then a fast no-op, exactly like a genuinely dead forward.
            pid: 999_999_999,
            opened_at: aoide_storage::time::now_iso_utc(),
        };
        aoide_storage::tunnel::save(&tunnel_record("end-tunnel-1", "node-a")).unwrap();
        aoide_storage::tunnel::save(&tunnel_record("other-session", "node-b")).unwrap();

        let out = session_end(&flag_invocation(
            &["session", "end"],
            &[("id", "end-tunnel-1")],
        ));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "msg: {}", out.message);

        assert!(
            aoide_storage::tunnel::load("end-tunnel-1", "node-a").is_none(),
            "the ended session's own tunnel record is closed",
        );
        assert!(
            aoide_storage::tunnel::load("other-session", "node-b").is_some(),
            "another session's tunnel record is untouched",
        );

        let _ = std::fs::remove_dir_all(&stage);
    }
    // ── P-CX-5 S3b: `kind:"app"` refuses `session phase`/`session end` ──
    #[test]
    fn a_codex_app_record_refuses_session_phase() {
        // A `kind:"app"` record's `state` comes from its rollout's turn
        // bracket (`apply_codex_capture`, `codex_app.rs`) — the one writer
        // for it from here on. Before this fix a stray `session phase`
        // self-healed within one tick (the per-tick reset re-stamped
        // `idle`); after S3 removed that reset nothing corrects it, so it
        // must refuse outright, same reason vocabulary as `send`'s own
        // `codex-app-unsupported`.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR"]);
        let stage = unique_stage("sess-phase-app-refuse");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        let mut app = session("01a07d89-app", "/home/khoa/Aoide", "idle", "1", None);
        app.agent = "codex".to_string();
        app.kind = Some("app".to_string());
        let sf = SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![app],
        };
        write_stage(&sessions_path(), &sf).unwrap();

        let out = session_phase(&flag_invocation(
            &["session", "phase"],
            &[("id", "01a07d89-app"), ("phase", "awaiting")],
        ));
        assert_eq!(
            out.status,
            aoide_protocol::output::Status::Error,
            "msg: {}",
            out.message
        );
        assert_eq!(
            out.data.as_ref().unwrap()["reason"],
            "codex-app-unsupported"
        );

        let after: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec = after
            .sessions
            .iter()
            .find(|s| s.session_id == "01a07d89-app")
            .unwrap();
        assert_eq!(
            rec.state, "idle",
            "a refused session phase must not touch the record's state"
        );

        let hooks: HooksFile = load_stage(&hooks_path()).unwrap();
        assert!(
            !hooks.hooks.iter().any(|h| h.session_id == "01a07d89-app"),
            "a refused session phase must not write a hook record either"
        );

        let _ = std::fs::remove_dir_all(&stage);
    }
    #[test]
    fn a_codex_app_record_refuses_session_end() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_STATE_DIR"]);
        let stage = unique_stage("sess-end-app-refuse");
        let state = stage.join("state");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("AOIDE_STATE_DIR", &state);

        let mut app = session("01a07d89-app-end", "/home/khoa/Aoide", "working", "1", None);
        app.agent = "codex".to_string();
        app.kind = Some("app".to_string());
        let sf = SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![app],
        };
        write_stage(&sessions_path(), &sf).unwrap();

        let out = session_end(&flag_invocation(
            &["session", "end"],
            &[("id", "01a07d89-app-end")],
        ));
        assert_eq!(
            out.status,
            aoide_protocol::output::Status::Error,
            "msg: {}",
            out.message
        );
        assert_eq!(
            out.data.as_ref().unwrap()["reason"],
            "codex-app-unsupported"
        );

        let after: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec = after
            .sessions
            .iter()
            .find(|s| s.session_id == "01a07d89-app-end")
            .unwrap();
        assert_eq!(
            rec.state, "working",
            "a refused session end must not mark the app record done"
        );

        let _ = std::fs::remove_dir_all(&stage);
    }
    #[test]
    fn a_non_app_record_still_takes_the_phase() {
        // The kind check above must never touch the ordinary path: a
        // shell/agent/subagent/a2a record's `session phase` behaves
        // byte-for-byte as it did before this refusal existed.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR"]);
        let stage = unique_stage("sess-phase-non-app-ok");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        session_start(&flag_invocation(
            &["session", "start"],
            &[("id", "shell-1"), ("agent", "claude"), ("cwd", "/w")],
        ));

        let out = session_phase(&flag_invocation(
            &["session", "phase"],
            &[("id", "shell-1"), ("phase", "awaiting")],
        ));
        assert_eq!(
            out.status,
            aoide_protocol::output::Status::Ok,
            "msg: {}",
            out.message
        );

        let after: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec = after
            .sessions
            .iter()
            .find(|s| s.session_id == "shell-1")
            .unwrap();
        assert_eq!(rec.state, "awaiting");

        let _ = std::fs::remove_dir_all(&stage);
    }
    #[test]
    fn set_session_log_path_is_change_only_and_noops_on_an_unknown_id() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("sess-log-path");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        session_start(&flag_invocation(&["session", "start"], &[("id", "s")]));

        set_session_log_path("s", "/home/khoa/Aoide/state/sessions/s.log");
        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert_eq!(
            s.sessions.iter().find(|x| x.session_id == "s").unwrap().log_path.as_deref(),
            Some("/home/khoa/Aoide/state/sessions/s.log")
        );
        let mtime_after_first = std::fs::metadata(sessions_path()).unwrap().modified().unwrap();

        // A second call with the SAME path is change-only: no rewrite, so the
        // file's mtime does not advance.
        std::thread::sleep(std::time::Duration::from_millis(10));
        set_session_log_path("s", "/home/khoa/Aoide/state/sessions/s.log");
        let mtime_after_second = std::fs::metadata(sessions_path()).unwrap().modified().unwrap();
        assert_eq!(
            mtime_after_first, mtime_after_second,
            "an identical logPath must not rewrite the stage file"
        );

        // An unknown id is a safe no-op (never panics, never inserts a record).
        set_session_log_path("ghost", "/nope.log");
        let s2: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert_eq!(s2.sessions.len(), 1);
        assert!(!s2.sessions.iter().any(|x| x.session_id == "ghost"));

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }
    #[test]
    fn session_start_requires_the_id_flag() {
        // P-D6: `session_start` now tries the daemon FIRST — this test does
        // no stage/env setup of its own, but still needs `env_lock`'s own
        // one-time `AOIDE_DAEMON_SOCKET` isolation stamp (`lib.rs`'s own
        // doc) so a real resident daemon on this box is never reached.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let out = session_start(&flag_invocation(&["session", "start"], &[]));
        assert_eq!(out.status, aoide_protocol::output::Status::Usage);
        assert_eq!(out.render(false).1, aoide_protocol::output::exit::USAGE);
    }
    #[test]
    fn is_session_dead_combines_signals_and_never_false_reaps() {
        let live: HashSet<String> = ["aaa", "bbb"].iter().map(|s| s.to_string()).collect();
        let alive = |_p: u32| true; // /proc/<pid> exists
        let dead_proc = |_p: u32| false; // /proc/<pid> is gone
        let rec = |window: &str, pid: Option<u32>| SessionRecord {
            window_address: window.into(),
            pid,
            ..Default::default()
        };
        // These assertions are about the window/pid signals only; pin the third
        // (stale hook-only) signal off by reporting "just seen right now" for
        // every record, regardless of state.
        let now = 0_i64;
        let fresh = |_: &SessionRecord| Some(now);

        // Window-gone (the SUPER+Q kill): a non-empty window absent from the live
        // set is dead even when the pid is alive. The match is 0x/case-tolerant.
        assert!(is_session_dead(
            &rec("0xCCC", None),
            Some(&live),
            None,
            alive,
            now,
            fresh
        ));
        assert!(is_session_dead(
            &rec("0xCCC", Some(9)),
            Some(&live),
            None,
            alive,
            now,
            fresh
        ));
        // A live window (normalised match) with a live pid → NOT dead.
        assert!(!is_session_dead(
            &rec("0xAAA", Some(9)),
            Some(&live),
            None,
            alive,
            now,
            fresh
        ));

        // Process-gone: a pid whose /proc vanished is dead regardless of window
        // (here the window IS live, so ONLY the pid signal fires).
        assert!(is_session_dead(
            &rec("0xAAA", Some(9)),
            Some(&live),
            None,
            dead_proc,
            now,
            fresh
        ));

        // NEVER-FALSE-REAP #1 — neither signal (no window, no pid), and the third
        // signal reports "just seen": left alone.
        assert!(!is_session_dead(
            &rec("", None),
            Some(&live),
            None,
            dead_proc,
            now,
            fresh
        ));

        // NEVER-FALSE-REAP #2 — compositor NOT queried (None): the window signal
        // is suppressed, so a windowed session we could not SEE is never reaped;
        // only the authoritative pid signal remains.
        assert!(!is_session_dead(
            &rec("0xCCC", None),
            None,
            None,
            alive,
            now,
            fresh
        ));
        assert!(!is_session_dead(
            &rec("0xCCC", Some(9)),
            None,
            None,
            alive,
            now,
            fresh
        )); // pid alive → alive
        assert!(is_session_dead(
            &rec("0xCCC", Some(9)),
            None,
            None,
            dead_proc,
            now,
            fresh
        )); // pid gone → dead
    }
    #[test]
    fn reap_drops_killed_sessions_but_spares_the_living() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "HYPRLAND_INSTANCE_SIGNATURE"]);
        // Force pid-only liveness: with no compositor the window signal is
        // suppressed, so the reap decision rests purely on /proc/<pid> — fully
        // deterministic in a test (no hyprctl, no real windows).
        std::env::remove_var("HYPRLAND_INSTANCE_SIGNATURE");
        let stage = unique_stage("reap");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        let _xdg = isolated_xdg_runtime("reap-killed-vs-living-xdg");

        // A pid that can NEVER exist (above every Linux pid_max) is the killed
        // session; this very process's pid is the living one; and a hook-only
        // session carries NEITHER the window NOR the pid signal and must be
        // spared — AS LONG AS it is fresh. It is stamped with the real "now"
        // (not the fixed 2026-01-01 the other two use) so the reaper's new
        // stale-hook-only-at-rest signal (which reads the real wall clock) does
        // not fire on it: this test is about the window/pid signals, not
        // staleness (that has its own coverage in `reap.rs`'s unit tests).
        let dead_pid = u32::MAX;
        let now = "2026-01-01T00:00:00Z";
        let fresh_now = now_iso_utc();
        let mut sessions = Vec::new();
        // `live` is stamped with the real "now" for the same reason `hookonly`
        // below is: every reaper signal that reads the wall clock must find it
        // current. The pre-boot signal is the second of those — a record whose
        // every timestamp predates the machine's boot is condemned however
        // alive its pid looks, precisely because a pid outliving a reboot is a
        // recycled one. A fixed 2026-01-01 birth date beside a live pid is a
        // state the world cannot produce.
        upsert_session(
            &mut sessions, "live", None, Some("/w"), None, None, None, None, None,
            Some(std::process::id()), &fresh_now,
        );
        upsert_session(
            &mut sessions, "killed", None, Some("/w"), None, None, None, None, None,
            Some(dead_pid), now,
        );
        upsert_session(
            &mut sessions, "hookonly", None, Some("/w"), None, None, None, None, None,
            None, &fresh_now,
        );
        write_stage(
            &sessions_path(),
            &SessionsFile { schema_version: "0".into(), sessions },
        )
        .unwrap();
        let mut hooks = Vec::new();
        upsert_hook(&mut hooks, "killed", "running", now);
        write_stage(
            &hooks_path(),
            &HooksFile { schema_version: "0".into(), hooks },
        )
        .unwrap();

        let out = reap(&invocation(&["session", "reap"], &[]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok);
        let data = out.data.unwrap();
        assert_eq!(data["reaped"], json!(["killed"]));
        assert_eq!(data["hyprctlAvailable"], json!(false)); // pid-only fallback

        // The killed session AND its hook are gone; live + hook-only survive.
        let s2: SessionsFile = load_stage(&sessions_path()).unwrap();
        let ids: Vec<&str> = s2.sessions.iter().map(|s| s.session_id.as_str()).collect();
        assert!(ids.contains(&"live"), "a live session is never reaped");
        assert!(ids.contains(&"hookonly"), "a signal-less session is never reaped");
        assert!(!ids.contains(&"killed"), "the killed session was reaped");
        let h2: HooksFile = load_stage(&hooks_path()).unwrap();
        assert!(h2.hooks.iter().all(|h| h.session_id != "killed"));

        // graph.json was re-staged and no longer carries the reaped node.
        let g: Value =
            serde_json::from_str(&std::fs::read_to_string(stage.join("graph.json")).unwrap())
                .unwrap();
        assert!(g["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|n| n["id"] != "session:killed"));

        // Idempotent + never non-zero: a second sweep finds nothing to reap.
        let again = reap(&invocation(&["session", "reap"], &[]));
        assert_eq!(again.status, aoide_protocol::output::Status::Ok);
        assert_eq!(again.data.unwrap()["reaped"], json!([]));

        let _ = std::fs::remove_dir_all(&stage);
    }
    #[test]
    fn reap_cascades_an_orphaned_subagent_when_its_top_level_parent_is_reaped() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "HYPRLAND_INSTANCE_SIGNATURE"]);
        std::env::remove_var("HYPRLAND_INSTANCE_SIGNATURE"); // pid-only liveness
        let stage = unique_stage("reap-subagent-cascade");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        let _xdg = isolated_xdg_runtime("reap-subagent-cascade-xdg");

        // A pid that cannot possibly be alive → the pid-dead signal fires for `top`.
        let dead_pid = u32::MAX;
        let now = "2026-01-01T00:00:00Z";
        let mut sessions = Vec::new();
        upsert_session(
            &mut sessions, "top", None, Some("/w"), None, None, None, None, None,
            Some(dead_pid), now,
        );
        // The orphaned Task node — no window, no pid, kind:"subagent" — exactly
        // the shape of the two live ghosts this fix targets.
        sessions.push(SessionRecord {
            session_id: "sub:orphan".into(),
            agent: "general-purpose".into(),
            state: "working".into(),
            started_at: now.into(),
            parent_session_id: Some("top".into()),
            kind: Some("subagent".into()),
            ..Default::default()
        });
        write_stage(
            &sessions_path(),
            &SessionsFile { schema_version: "0".into(), sessions },
        )
        .unwrap();
        let mut hooks = Vec::new();
        upsert_hook(&mut hooks, "top", "running", now);
        write_stage(
            &hooks_path(),
            &HooksFile { schema_version: "0".into(), hooks },
        )
        .unwrap();

        let out = reap(&invocation(&["session", "reap"], &[]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok);

        let s2: SessionsFile = load_stage(&sessions_path()).unwrap();
        let ids: Vec<&str> = s2.sessions.iter().map(|s| s.session_id.as_str()).collect();
        assert!(!ids.contains(&"top"), "the pid-dead top-level session was reaped");
        assert!(
            !ids.contains(&"sub:orphan"),
            "its orphaned subagent child cascades away with it — the gap this test guards"
        );

        let _ = std::fs::remove_dir_all(&stage);
    }
    #[test]
    fn reap_spares_a_live_but_hook_silent_session() {
        // The regression this whole fix pins: the reaper reaps by REAL liveness
        // (window / pid), NEVER by hook silence. A session whose pid is alive but
        // that has emitted NO hook — its hook stream went quiet across a
        // reload/restart window — must survive the sweep. (hooks.json is left
        // absent, so the session is maximally hook-silent.)
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "HYPRLAND_INSTANCE_SIGNATURE"]);
        std::env::remove_var("HYPRLAND_INSTANCE_SIGNATURE"); // pid-only liveness
        let stage = unique_stage("reap-hooksilent");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        let _xdg = isolated_xdg_runtime("reap-hooksilent-xdg");

        // The real "now", not a fixed date: this record must survive every
        // signal that reads the wall clock, and the pre-boot one condemns a
        // record whose whole timeline predates the machine's boot — a live pid
        // on a session born before the last reboot is a recycled pid, which is
        // exactly the ghost that signal exists for.
        let now = now_iso_utc();
        let mut sessions = Vec::new();
        upsert_session(
            &mut sessions, "quiet", None, Some("/w"), Some("0xdead"), None, None, None, None,
            Some(std::process::id()), &now,
        );
        write_stage(
            &sessions_path(),
            &SessionsFile { schema_version: "0".into(), sessions },
        )
        .unwrap();

        let out = reap(&invocation(&["session", "reap"], &[]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok);
        assert_eq!(
            out.data.unwrap()["reaped"],
            json!([]),
            "a live-but-hook-silent session is never reaped"
        );
        let s2: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert!(
            s2.sessions.iter().any(|s| s.session_id == "quiet"),
            "the hook-silent session survives the reaper pass"
        );

        let _ = std::fs::remove_dir_all(&stage);
    }
    #[test]
    fn reaper_grace_ignores_a_degenerate_empty_clients_snapshot() {
        // A momentary empty `hyprctl clients` read during a reload window must not
        // mass-reap the windowed roster: the grace downgrades it to pid-only.
        let windowed = vec![SessionRecord {
            window_address: "0xabc".into(),
            state: "running".into(),
            ..Default::default()
        }];
        // Empty gathered set + a windowed live session → degrade to None (pid-only).
        assert!(effective_live_addresses(Some(HashSet::new()), &windowed).is_none());

        // A NON-empty set is trusted as gathered (the normal path).
        let set: HashSet<String> = ["0xabc".to_string()].into_iter().collect();
        assert_eq!(
            effective_live_addresses(Some(set.clone()), &windowed),
            Some(set)
        );

        // Empty set but nothing windowed to protect (a genuinely empty desktop, or
        // an all-`done` roster) → the empty set passes through; pid signal governs.
        let done_only = vec![SessionRecord {
            window_address: "0xabc".into(),
            state: "done".into(),
            ..Default::default()
        }];
        assert_eq!(
            effective_live_addresses(Some(HashSet::new()), &done_only),
            Some(HashSet::new())
        );

        // No compositor at all stays None (unchanged).
        assert!(effective_live_addresses(None, &windowed).is_none());
    }

    #[test]
    fn stamp_resumed_from_lands_the_field_and_renders_a_resumed_edge() {
        // P-D8: the mechanical half of "resurrect mints a new id with
        // resumedFrom set" — no spawn involved, just the stage-write + the
        // graph.json projection it triggers (unlike `stamp_harness_session_id`,
        // this one DOES restage).
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("stamp-resumed-from");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        do_session_start("new-1", Some("claude"), Some("/w"), None, None, None, None, None, None);
        stamp_resumed_from("new-1", "old-1");

        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec = s.sessions.iter().find(|r| r.session_id == "new-1").unwrap();
        assert_eq!(rec.resumed_from.as_deref(), Some("old-1"));

        let g: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(stage.join("graph.json")).unwrap(),
        )
        .unwrap();
        let edges = g["edges"].as_array().unwrap();
        assert!(
            edges.iter().any(|e| e["from"] == "session:old-1"
                && e["to"] == "session:new-1"
                && e["kind"] == "resumed"),
            "graph.json must be re-staged with the resumed edge: {edges:?}"
        );

        // A second stamp with the SAME value is change-only (no-op) —
        // proven indirectly: an unknown id is a silent no-op and never
        // panics or errors.
        stamp_resumed_from("no-such-session", "old-2");

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn stamp_origin_lands_the_field_and_never_restages_graph_json() {
        // P-P3: `stamp_origin` is `graph.json`-invisible, like
        // `hookAncestry`/`headless` — unlike `stamp_resumed_from`, which
        // DOES restage.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("stamp-origin");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        do_session_start("node-spawned-1", Some("claude"), Some("/w"), None, None, None, None, None, None);
        stamp_origin("node-spawned-1", "node:yomi-strix");

        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec = s.sessions.iter().find(|r| r.session_id == "node-spawned-1").unwrap();
        assert_eq!(rec.origin.as_deref(), Some("node:yomi-strix"));

        // A local (non-node) registration never gets an origin at all.
        do_session_start("local-1", Some("claude"), Some("/w"), None, None, None, None, None, None);
        let s2: SessionsFile = load_stage(&sessions_path()).unwrap();
        let local = s2.sessions.iter().find(|r| r.session_id == "local-1").unwrap();
        assert_eq!(local.origin, None);

        // An empty origin string is a no-op, same as an unknown id.
        stamp_origin("local-1", "");
        stamp_origin("no-such-session", "node:ghost");
        let s3: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert_eq!(s3.sessions.iter().find(|r| r.session_id == "local-1").unwrap().origin, None);

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// N4: a `pending` opening turn whose worker died with its process is
    /// reconciled at the next boot — `unknown`, never a word that reads as
    /// "still waiting" — and every other verdict is left exactly as it was.
    #[test]
    fn settle_lost_opening_turns_reconciles_only_a_stranded_pending() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("settle-opening-turns");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        for (id, word) in [
            ("stranded", Some("pending")),
            ("done-already", Some("not-ready")),
            ("local", None),
        ] {
            do_session_start(id, Some("a2a"), Some("/w"), None, None, None, None, None, None);
            if let Some(word) = word {
                stamp_opening_turn(id, word);
            }
        }

        assert_eq!(settle_lost_opening_turns(), 1, "exactly the stranded one");

        let after: SessionsFile = load_stage(&sessions_path()).unwrap();
        let word = |id: &str| {
            after
                .sessions
                .iter()
                .find(|r| r.session_id == id)
                .and_then(|r| r.opening_turn.clone())
        };
        assert_eq!(word("stranded").as_deref(), Some("unknown"));
        assert_eq!(word("done-already").as_deref(), Some("not-ready"));
        assert_eq!(word("local"), None, "a locally-spawned session carries none");

        // Idempotent: nothing stranded the second time, no write.
        assert_eq!(settle_lost_opening_turns(), 0);

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
    }

    #[test]
    fn stamp_remote_parent_is_change_only_and_leaves_parent_session_id_none() {
        // P-RSA S3: the child's own node records who asked for it on ANOTHER
        // node. `parentSessionId` stays local-only (CONTRACTS.md §4) — the
        // door never writes a foreign id there, and neither does this stamp —
        // and a locally-registered record carries no `remoteParent` at all,
        // because nothing in this crate can mint one: no env var, no
        // `conduct`/`spawn` flag, no `resurrect` carry.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("stamp-remote-parent");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        do_session_start("a2a-child-1", Some("a2a"), Some("/w"), None, None, None, None, None, None);
        let born: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec = born.sessions.iter().find(|r| r.session_id == "a2a-child-1").unwrap();
        assert!(rec.remote_parent.is_none(), "registration alone never mints a remoteParent");

        let parent = aoide_storage::records::RemoteParent {
            node: "yomi-strix".to_string(),
            key: "ab".repeat(32),
            session_id: "conduct-17991-1790312541".to_string(),
            extra: Default::default(),
        };
        stamp_remote_parent("a2a-child-1", &parent);

        let after: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec = after.sessions.iter().find(|r| r.session_id == "a2a-child-1").unwrap();
        assert_eq!(rec.remote_parent.as_ref(), Some(&parent));
        assert_eq!(
            rec.parent_session_id, None,
            "a remote parent is NEVER written as a local parentSessionId — every reader of that field treats it as a local id"
        );

        // Change-only: re-stamping the SAME parent writes no bytes at all
        // (the guard is the value comparison `stamp_origin` uses, not a
        // "has this been set" flag).
        let bytes_before = std::fs::read(sessions_path()).unwrap();
        stamp_remote_parent("a2a-child-1", &parent);
        assert_eq!(
            std::fs::read(sessions_path()).unwrap(),
            bytes_before,
            "a same-value re-stamp must not rewrite the stage file"
        );

        // A genuinely DIFFERENT parent still overwrites — this is NOT an
        // immutability guard, the contract `stamp_seal` documents. Same-uid
        // attribution, never a gate: the key comparison the DOOR makes is the
        // gate, not this file.
        let other = aoide_storage::records::RemoteParent {
            node: "sakaki".to_string(),
            key: "cd".repeat(32),
            session_id: "conduct-2".to_string(),
            extra: Default::default(),
        };
        stamp_remote_parent("a2a-child-1", &other);
        let after2: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert_eq!(
            after2.sessions.iter().find(|r| r.session_id == "a2a-child-1").unwrap().remote_parent.as_ref(),
            Some(&other)
        );

        // An unknown id and an empty `sessionId` are silent no-ops, the shape
        // `stamp_origin` inherits from an empty origin.
        stamp_remote_parent("no-such-session", &other);
        stamp_remote_parent(
            "a2a-child-1",
            &aoide_storage::records::RemoteParent { node: "sakaki".to_string(), key: String::new(), session_id: String::new(), extra: Default::default() },
        );
        let after3: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert_eq!(
            after3.sessions.iter().find(|r| r.session_id == "a2a-child-1").unwrap().remote_parent.as_ref(),
            Some(&other),
            "an empty sessionId is not a parent"
        );

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn stamp_seal_lands_the_field_and_never_restages_graph_json() {
        // LANE IDENTITY P-ID1: `stamp_seal` is `graph.json`-invisible, the
        // same shape `stamp_origin`'s own sibling test just above proves.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("stamp-seal");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        do_session_start("sealed-1", Some("claude"), Some("/w"), None, None, None, None, None, None);
        stamp_seal("sealed-1", "deadbeefcafe", 1_700_000_000, 42);

        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec = s.sessions.iter().find(|r| r.session_id == "sealed-1").unwrap();
        assert_eq!(rec.seal.as_deref(), Some("deadbeefcafe"));
        assert_eq!(rec.sealed_issued_at, Some(1_700_000_000));
        assert_eq!(rec.sealed_pid_starttime, Some(42), "the plaintext starttime lands with the seal");

        // A registration nobody sealed never gets a `seal` at all.
        do_session_start("unsealed-1", Some("claude"), Some("/w"), None, None, None, None, None, None);
        let s2: SessionsFile = load_stage(&sessions_path()).unwrap();
        let unsealed = s2.sessions.iter().find(|r| r.session_id == "unsealed-1").unwrap();
        assert_eq!(unsealed.seal, None);
        assert_eq!(unsealed.sealed_issued_at, None);

        // An empty seal is a no-op, same as an unknown id.
        stamp_seal("unsealed-1", "", 1_700_000_000, 42);
        stamp_seal("no-such-session", "deadbeef", 1_700_000_000, 42);
        let s3: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert_eq!(s3.sessions.iter().find(|r| r.session_id == "unsealed-1").unwrap().seal, None);

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }
}

/// Binding is explicit local orchestration state, never a remote access grant.
pub fn session_bind(inv: &Invocation) -> Outcome {
    let cmd = "session.bind";
    match inv.door {
        aoide_protocol::Door::Cli => {
            return aoide_client::daemon::daemon_dispatch(inv).unwrap_or_else(||
                Outcome::error(cmd, "aoided must be running to bind an enduring agent"));
        }
        aoide_protocol::Door::Daemon => {}
        _ => return Outcome::error(cmd, "session bind is local-only; remote doors cannot assign an enduring identity"),
    }
    let id = match require_flag(inv, "id") {
        Ok(id) => id,
        Err(out) => return out,
    };
    let key = match require_flag(inv, "agent-id") {
        Ok(key) => key,
        Err(out) => return out,
    };
    with_stage_lock(|| {
        let mut file: SessionsFile = match load_stage(&sessions_path()) {
            Ok(file) => file,
            Err(e) => return stage_error(cmd, e),
        };
        let changed = match aoide_storage::session::bind_enduring_agent(&mut file.sessions, &id, &key) {
            Ok(changed) => changed,
            Err(reason) => return Outcome::error(cmd, format!("cannot bind session `{id}`: {reason}"))
                .with_data(json!({"reason": reason, "sessionId": id, "enduringAgentId": key})),
        };
        let mut changed_paths = Vec::new();
        if changed {
            if let Err(e) = write_stage(&sessions_path(), &file) {
                return stage_error(cmd, e);
            }
            changed_paths.push(sessions_path().to_string_lossy().into_owned());
            match restage_graph() {
                Ok(path) => changed_paths.push(path.to_string_lossy().into_owned()),
                Err(e) => return stage_error(cmd, e),
            }
        }
        Outcome::ok(cmd, if changed { "enduring agent bound" } else { "enduring agent already bound" })
            .changed(changed_paths)
            .with_data(json!({"sessionId": id, "enduringAgentId": key, "bindingChanged": changed}))
    })
}

#[cfg(test)]
mod enduring_binding_tests {
    use super::*;

    #[test]
    fn daemon_binding_ignores_optional_knowledge_config_and_preserves_refusals_on_disk() {
        let _lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, root) = aoide_test_support::isolated_mail_root("enduring-binding");
        let _config_env = aoide_test_support::EnvSaver::capture(&["AOIDE_CONFIG"]);
        let broken_config = root.join("deliberately-invalid.toml");
        std::fs::write(&broken_config, "not valid toml !").unwrap();
        std::env::set_var("AOIDE_CONFIG", broken_config);
        let file = SessionsFile { sessions:vec![SessionRecord {
            session_id:"executor".into(), agent:"claude".into(), state:"idle".into(),
            ..Default::default()
        }], ..Default::default() };
        write_stage(&sessions_path(), &file).unwrap();
        let invocation = |id: &str, key: &str| Invocation {
            path:vec!["session".into(), "bind".into()], args:vec![],
            flags:[("id".into(),id.into()),("agent-id".into(),key.into())].into_iter().collect(),
            door:aoide_protocol::Door::Daemon,
        };
        let key = "7e3f5976-98b2-44a4-827c-c687a0d9526e";
        let first = session_bind(&invocation("executor", key));
        assert_eq!(first.status, aoide_protocol::output::Status::Ok, "{first:?}");
        let before = std::fs::read(sessions_path()).unwrap();
        assert_eq!(session_bind(&invocation("executor", key)).data.unwrap()["bindingChanged"], false);
        for (id,key) in [("missing",key),("executor","different-key"),("executor","Bad Key")] {
            assert_eq!(session_bind(&invocation(id,key)).status, aoide_protocol::output::Status::Error);
            assert_eq!(std::fs::read(sessions_path()).unwrap(), before);
        }
        let graph: Value = load_stage(&aoide_storage::stage::graph_path()).unwrap();
        assert_eq!(graph["nodes"][0]["enduringAgentId"], key);
        let bound: SessionsFile = load_stage(&sessions_path()).unwrap();
        ledger_session_exit(&bound.sessions[0], "2026-09-09T12:00:00Z");
        assert_eq!(aoide_storage::ledger::read_ledger().unwrap()[0].enduring_agent_id.as_deref(), Some(key));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn remote_binding_is_refused_before_session_lookup() {
        for door in [aoide_protocol::Door::Mcp, aoide_protocol::Door::A2a] {
            let inv = Invocation {path:vec!["session".into(),"bind".into()], args:vec![], flags:Default::default(), door};
            let out = session_bind(&inv);
            assert_eq!(out.status, aoide_protocol::output::Status::Error);
            assert!(out.message.contains("local-only"));
        }
    }
}

#[cfg(test)]
mod live_sealed_wrap_guards {
    use super::*;
    use std::path::PathBuf;
    use crate::graph::manage::{link, prune};
    use crate::graph::testutil::*;
    use aoide_protocol::output::Status;

    /// The world a sealed wrap lives in: a private stage and runtime dir, and
    /// a fake daemon whose seal pubkey is `kp`'s. The caller holds `env_lock`.
    fn seal_world(tag: &str) -> (EnvVars, aoide_storage::identity::Keypair, PathBuf) {
        let env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "AOIDE_DAEMON_SOCKET",
            "XDG_RUNTIME_DIR",
            "AOIDE_AUDIT_LOG",
        ]);
        let stage = unique_stage(tag);
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("XDG_RUNTIME_DIR", &stage);
        std::env::set_var("AOIDE_AUDIT_LOG", stage.join("audit.log"));
        let kp = aoide_storage::identity::mint_ephemeral().unwrap();
        std::env::set_var("AOIDE_DAEMON_SOCKET", fake_seal_daemon(&kp.info().pubkey_hex));
        (env, kp, stage)
    }

    /// A conducted wrap whose pid is this test process, sealed by the daemon.
    fn sealed_live_wrap(id: &str, kp: &aoide_storage::identity::Keypair) {
        do_session_start(
            id,
            Some("claude"),
            Some("/w"),
            None,
            None,
            Some(true),
            Some("/nonexistent/wrap.sock"),
            None,
            Some(std::process::id()),
        );
        seal_record(id, kp);
    }

    /// A door call as the daemon runs it (`Door::Daemon` skips the dispatch
    /// hop, so the handler runs in-process, as it does inside `aoided`).
    fn door(path: &[&str], id: &str) -> aoide_protocol::Invocation {
        let mut inv = flag_invocation(path, &[("id", id)]);
        inv.door = aoide_protocol::Door::Daemon;
        inv
    }

    fn record(id: &str) -> Option<SessionRecord> {
        load_stage::<SessionsFile>(&sessions_path())
            .ok()?
            .sessions
            .into_iter()
            .find(|s| s.session_id == id)
    }

    fn set_state(id: &str, state: &str) {
        let mut file: SessionsFile = load_stage(&sessions_path()).unwrap();
        file.sessions.iter_mut().find(|s| s.session_id == id).unwrap().state = state.to_string();
        write_stage(&sessions_path(), &file).unwrap();
    }

    /// Clears a record's `kind`, as a legacy record has none (`upsert_session`
    /// backfills it on the next touch).
    fn clear_kind(id: &str) {
        let mut file: SessionsFile = load_stage(&sessions_path()).unwrap();
        file.sessions.iter_mut().find(|s| s.session_id == id).unwrap().kind = None;
        write_stage(&sessions_path(), &file).unwrap();
    }

    /// Sets `id`'s spawn edge the way a spawn does: the parent and its stamp.
    fn set_spawn_edge(id: &str, parent: &str) {
        let mut file: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec = file.sessions.iter_mut().find(|s| s.session_id == id).unwrap();
        rec.set_parent_session(Some(parent.to_string()));
        rec.attested_spawner = Some(parent.to_string());
        write_stage(&sessions_path(), &file).unwrap();
    }

    #[test]
    fn session_end_refuses_a_live_sealed_wrap_and_keeps_its_seal() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, kp, stage) = seal_world("live-wrap-end");
        sealed_live_wrap("wrap", &kp);

        let out = session_end(&door(&["session", "end"], "wrap"));

        assert_eq!(out.status, Status::Error, "a live sealed wrap is refused: {}", out.message);
        let rec = record("wrap").expect("the record survives the refused end");
        assert_ne!(rec.state, "done");
        assert!(rec.seal.is_some(), "the seal survives");
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn end_then_prune_leaves_a_live_sealed_wrap_in_place() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, kp, stage) = seal_world("live-wrap-chain");
        sealed_live_wrap("wrap", &kp);

        let _ = session_end(&door(&["session", "end"], "wrap"));
        let _ = prune(&flag_invocation(&["session", "prune"], &[]));

        let rec = record("wrap").expect("prune does not drop a live sealed wrap");
        assert_ne!(rec.state, "done");
        assert!(rec.seal.is_some(), "the seal survives end and prune");
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn prune_keeps_a_done_record_whose_sealed_wrap_is_running() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, kp, stage) = seal_world("live-wrap-prune");
        sealed_live_wrap("wrap", &kp);
        set_state("wrap", "done");

        let _ = prune(&flag_invocation(&["session", "prune"], &[]));

        assert!(record("wrap").is_some(), "prune keeps a live sealed wrap even when its state reads done");
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn terminal_phase_does_not_finish_a_live_sealed_wrap() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, kp, stage) = seal_world("live-wrap-phase");
        sealed_live_wrap("wrap", &kp);
        let mut inv = door(&["session", "phase"], "wrap");
        inv.flags.insert("phase".to_string(), "exit".to_string());

        let _ = session_phase(&inv);

        assert_ne!(record("wrap").unwrap().state, "done", "`exit` is the terminal vocabulary, so it must not finish a live sealed wrap");
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn the_wraps_own_exit_path_still_ends_its_sealed_record() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, kp, stage) = seal_world("live-wrap-own-exit");
        sealed_live_wrap("wrap", &kp);

        let _ = do_session_end("wrap");

        assert_eq!(record("wrap").unwrap().state, "done", "the wrap's own exit path is not refused");
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn an_unsealed_conducted_record_still_ends_as_today() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, _kp, stage) = seal_world("live-wrap-unsealed");
        do_session_start(
            "plain",
            Some("claude"),
            Some("/w"),
            None,
            None,
            Some(true),
            Some("/nonexistent/plain.sock"),
            None,
            Some(std::process::id()),
        );

        let out = session_end(&door(&["session", "end"], "plain"));

        assert_eq!(out.status, Status::Ok);
        assert_eq!(record("plain").unwrap().state, "done");
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn a_record_repointed_to_a_dead_pid_ends_as_today() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, kp, stage) = seal_world("live-wrap-dead-pid");
        sealed_live_wrap("wrap", &kp);
        let mut file: SessionsFile = load_stage(&sessions_path()).unwrap();
        file.sessions.iter_mut().find(|s| s.session_id == "wrap").unwrap().pid = Some(reaped_pid());
        write_stage(&sessions_path(), &file).unwrap();

        let _ = session_end(&door(&["session", "end"], "wrap"));

        assert_eq!(record("wrap").unwrap().state, "done", "a dead pid is never protected");
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// The start door refuses a protected wrap with no daemon to ask, so a dead
    /// `AOIDE_DAEMON_SOCKET` leaves the record exactly as it was.
    #[test]
    fn a_live_sealed_wrap_is_refused_a_start_with_a_dead_daemon_socket() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, kp, stage) = seal_world("live-wrap-start-dead-socket");
        sealed_live_wrap("wrap", &kp);
        std::env::set_var("AOIDE_DAEMON_SOCKET", stage.join("no-such-aoided.sock"));
        let before = serde_json::to_value(record("wrap").unwrap()).unwrap();

        let inv = flag_invocation(
            &["session", "start"],
            &[("id", "wrap"), ("agent", "codex"), ("cwd", "/elsewhere")],
        );
        let out = session_start(&inv);

        assert_eq!(out.status, Status::Error, "{}", out.message);
        assert_eq!(
            serde_json::to_value(record("wrap").unwrap()).unwrap(),
            before,
            "the record is unchanged"
        );
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// The harness hook's own `session start` (agent and cwd, as the hook doc
    /// sends them) on a protected wrap changes nothing: it answers ok, writes no
    /// byte, and keeps the seal and the plaintext starttime.
    #[test]
    fn a_same_fields_start_on_a_protected_wrap_is_a_no_op() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, kp, stage) = seal_world("live-wrap-start-no-op");
        sealed_live_wrap("wrap", &kp);
        std::env::set_var("AOIDE_DAEMON_SOCKET", stage.join("no-such-aoided.sock"));
        let bytes = std::fs::read(sessions_path()).unwrap();
        let sealed = record("wrap").unwrap();

        let inv = flag_invocation(
            &["session", "start"],
            &[("id", "wrap"), ("agent", "claude"), ("cwd", "/w")],
        );
        let out = session_start(&inv);

        assert_eq!(out.status, Status::Ok, "{}", out.message);
        assert!(out.message.contains("no field changed"), "the no-op answer: {}", out.message);
        assert_eq!(std::fs::read(sessions_path()).unwrap(), bytes, "the roster is byte-identical");
        assert_eq!(record("wrap").unwrap().seal, sealed.seal);
        assert_eq!(record("wrap").unwrap().sealed_pid_starttime, sealed.sealed_pid_starttime);
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// A start that would change any stored field of a protected wrap is
    /// refused, one field at a time, and the record is kept as it was.
    #[test]
    fn a_start_that_would_change_a_field_of_a_protected_wrap_is_refused() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, kp, stage) = seal_world("live-wrap-start-changed");
        sealed_live_wrap("wrap", &kp);
        std::env::set_var("AOIDE_DAEMON_SOCKET", stage.join("no-such-aoided.sock"));
        let before = serde_json::to_value(record("wrap").unwrap()).unwrap();

        for (flag, value) in [
            ("agent", "codex"),
            ("cwd", "/elsewhere"),
            ("window", "0xOTHER"),
            ("parent", "some-parent"),
        ] {
            let inv = flag_invocation(&["session", "start"], &[("id", "wrap"), (flag, value)]);
            let out = session_start(&inv);
            assert_eq!(out.status, Status::Error, "{flag}: {}", out.message);
            assert_eq!(
                serde_json::to_value(record("wrap").unwrap()).unwrap(),
                before,
                "a refused {flag} change leaves the record unchanged"
            );
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// The hook's own SessionStart with a changed window is refused on a
    /// protected wrap: the roster stays byte-identical, and the refusal is
    /// audited, since the hook discards its outcome at one call site.
    #[test]
    fn a_hook_start_that_would_change_a_protected_wraps_window_is_refused_and_audited() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, kp, stage) = seal_world("hook-start-refused");
        sealed_live_wrap("wrap", &kp);
        let bytes = std::fs::read(sessions_path()).unwrap();

        let out = do_hook_session_start(aoide_protocol::Door::Daemon, "wrap", Some("claude"), Some("/w"), Some("0xOTHER"), None, None, &[]);

        assert_eq!(out.status, Status::Error, "{}", out.message);
        assert_eq!(std::fs::read(sessions_path()).unwrap(), bytes, "the roster is byte-identical");
        let audit = std::fs::read_to_string(stage.join("audit.log")).expect("the refusal is audited");
        assert!(audit.contains("live-sealed-wrap") && audit.contains("window"), "{audit}");
        assert!(audit.contains("\"door\":\"daemon\""), "the line names the door the hook arrived on: {audit}");
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// The hook's registration with the wrap's stored fields changes nothing:
    /// the no-op answer, byte-identical roster, and no audit line.
    #[test]
    fn a_same_fields_hook_start_on_a_protected_wrap_is_a_no_op() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, kp, stage) = seal_world("hook-start-no-op");
        sealed_live_wrap("wrap", &kp);
        let bytes = std::fs::read(sessions_path()).unwrap();

        let out = do_hook_session_start(aoide_protocol::Door::Cli, "wrap", Some("claude"), Some("/w"), None, None, None, &[]);

        assert_eq!(out.status, Status::Ok, "{}", out.message);
        assert!(out.message.contains("no field changed"), "the no-op answer: {}", out.message);
        assert_eq!(std::fs::read(sessions_path()).unwrap(), bytes, "the roster is byte-identical");
        assert!(!stage.join("audit.log").exists(), "nothing is refused, so nothing is audited");
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// The hook chain is not an identity field: a protected wrap's chain still
    /// moves, and its identity stays as it was.
    #[test]
    fn a_hook_start_still_moves_the_chain_of_a_protected_wrap() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, kp, stage) = seal_world("hook-start-chain");
        sealed_live_wrap("wrap", &kp);
        let me = std::process::id() as i32;

        let out = do_hook_session_start(aoide_protocol::Door::Cli, "wrap", Some("claude"), Some("/w"), None, None, None, &[me]);

        assert_eq!(out.status, Status::Ok, "{}", out.message);
        let rec = record("wrap").unwrap();
        assert_eq!(rec.hook_ancestry, vec![me], "the chain moved");
        assert_eq!(rec.cwd, "/w", "the identity did not");
        assert!(crate::graph::identity::is_protected_live_wrap(&rec), "the wrap is still protected");
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// A hook start on a record that is not a protected wrap takes the hook's
    /// fields, as it always has.
    #[test]
    fn an_unprotected_record_still_takes_the_hook_fields() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, _kp, stage) = seal_world("hook-start-plain");
        do_session_start("plain", Some("claude"), Some("/w"), None, None, None, None, None, None);

        let out = do_hook_session_start(aoide_protocol::Door::Cli, "plain", Some("claude"), Some("/w2"), Some("0xW"), None, None, &[]);

        assert_eq!(out.status, Status::Ok, "{}", out.message);
        let rec = record("plain").unwrap();
        assert_eq!((rec.cwd.as_str(), rec.window_address.as_str()), ("/w2", "0xW"));
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// The daemon runs this same door (`Door::Daemon`), so a restart with a new
    /// key does not lift the refusal of a dispatched start.
    #[test]
    fn a_live_sealed_wrap_is_refused_a_start_after_the_daemon_restarts() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, kp, stage) = seal_world("live-wrap-start-restart");
        sealed_live_wrap("wrap", &kp);
        let restarted = aoide_storage::identity::mint_ephemeral().unwrap();
        std::env::set_var("AOIDE_DAEMON_SOCKET", fake_seal_daemon(&restarted.info().pubkey_hex));
        let before = serde_json::to_value(record("wrap").unwrap()).unwrap();

        let mut inv = flag_invocation(&["session", "start"], &[("id", "wrap"), ("agent", "codex")]);
        inv.door = aoide_protocol::Door::Daemon;
        let out = session_start(&inv);

        assert_eq!(out.status, Status::Error, "{}", out.message);
        assert_eq!(
            serde_json::to_value(record("wrap").unwrap()).unwrap(),
            before,
            "the record is unchanged"
        );
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// A seal is stamped once. A stamp never replaces a seal, and never seals a
    /// record that ever carried one, even after its seal went missing.
    #[test]
    fn stamp_seal_never_replaces_a_seal_or_reseals_an_ever_sealed_record() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, kp, stage) = seal_world("stamp-once");
        sealed_live_wrap("wrap", &kp);
        let first = record("wrap").unwrap().seal.unwrap();

        stamp_seal("wrap", "replacement-seal", 2, 99);
        assert_eq!(record("wrap").unwrap().seal, Some(first), "a seal is never replaced");

        let mut file: SessionsFile = load_stage(&sessions_path()).unwrap();
        file.sessions.iter_mut().find(|s| s.session_id == "wrap").unwrap().seal = None;
        write_stage(&sessions_path(), &file).unwrap();
        stamp_seal("wrap", "fresh-seal", 3, 7);
        assert_eq!(record("wrap").unwrap().seal, None, "an ever-sealed record is never re-sealed");
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// `AOIDE_DAEMON_SOCKET` names a dead path, so no daemon can answer. The
    /// protection reads no key, so every door still refuses.
    #[test]
    fn a_live_sealed_wrap_is_refused_with_a_dead_daemon_socket() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, kp, stage) = seal_world("live-wrap-dead-socket");
        sealed_live_wrap("wrap", &kp);
        std::env::set_var("AOIDE_DAEMON_SOCKET", stage.join("no-such-aoided.sock"));

        let out = session_end(&door(&["session", "end"], "wrap"));
        assert_eq!(out.status, Status::Error, "{}", out.message);
        assert_ne!(record("wrap").unwrap().state, "done");

        set_state("wrap", "done");
        let _ = prune(&flag_invocation(&["session", "prune"], &[]));
        assert!(record("wrap").is_some(), "prune keeps it with no daemon to ask");
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// A daemon restart mints a new key, so the old seal verifies nothing. The
    /// wrap stays protected: the protection reads the plaintext starttime.
    #[test]
    fn a_live_sealed_wrap_is_refused_after_the_daemon_restarts_with_a_new_key() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, kp, stage) = seal_world("live-wrap-restart");
        sealed_live_wrap("wrap", &kp);
        let restarted = aoide_storage::identity::mint_ephemeral().unwrap();
        std::env::set_var("AOIDE_DAEMON_SOCKET", fake_seal_daemon(&restarted.info().pubkey_hex));

        let out = session_end(&door(&["session", "end"], "wrap"));
        assert_eq!(out.status, Status::Error, "{}", out.message);
        assert_ne!(record("wrap").unwrap().state, "done");
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// A reused pid: the record's starttime is not the live process's starttime,
    /// so the record ends as an unprotected one.
    #[test]
    fn a_reused_pid_does_not_protect_a_sealed_wrap() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, kp, stage) = seal_world("live-wrap-reused-pid");
        sealed_live_wrap("wrap", &kp);
        let live = crate::graph::pid_starttime(std::process::id() as i32).unwrap();
        let mut file: SessionsFile = load_stage(&sessions_path()).unwrap();
        file.sessions.iter_mut().find(|s| s.session_id == "wrap").unwrap().sealed_pid_starttime =
            Some(live + 1);
        write_stage(&sessions_path(), &file).unwrap();

        let out = session_end(&door(&["session", "end"], "wrap"));

        assert_eq!(out.status, Status::Ok, "{}", out.message);
        assert_eq!(record("wrap").unwrap().state, "done");
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// A seal minted before `sealedPidStarttime` existed carries no plaintext
    /// starttime. Nothing backfills it, so it is never protected.
    #[test]
    fn a_legacy_seal_without_a_plaintext_starttime_ends_as_today() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, kp, stage) = seal_world("live-wrap-legacy");
        sealed_live_wrap("wrap", &kp);
        let mut file: SessionsFile = load_stage(&sessions_path()).unwrap();
        file.sessions.iter_mut().find(|s| s.session_id == "wrap").unwrap().sealed_pid_starttime = None;
        write_stage(&sessions_path(), &file).unwrap();

        let out = session_end(&door(&["session", "end"], "wrap"));

        assert_eq!(out.status, Status::Ok, "{}", out.message);
        assert_eq!(record("wrap").unwrap().state, "done");
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn a_prune_drops_every_done_record_except_one_named_as_a_live_sealed_wrap() {
        let held = SessionRecord {
            session_id: "held".into(),
            state: "done".into(),
            ..Default::default()
        };
        let gone = SessionRecord {
            session_id: "gone".into(),
            state: "done".into(),
            ..Default::default()
        };
        let live: HashSet<String> = HashSet::from(["held".to_string()]);

        let (kept, _hooks, removed, _cleared) =
            crate::graph::doc::prune_done_keeping(vec![held, gone], vec![], true, &live);

        assert_eq!(removed, vec!["gone".to_string()]);
        let ids: Vec<&str> = kept.iter().map(|s| s.session_id.as_str()).collect();
        assert_eq!(ids, vec!["held"]);
    }

    #[test]
    fn the_refusal_names_the_running_wrap_and_how_it_ends() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, kp, stage) = seal_world("live-wrap-refusal");
        sealed_live_wrap("wrap", &kp);

        let message = live_wrap_refusal_for("wrap").expect("a live sealed wrap is refused");
        assert!(message.contains("`wrap`"), "{message}");
        assert!(message.contains("aoide session kill --id wrap"), "{message}");
        assert!(live_wrap_refusal_for("unknown").is_none());
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn hook_door_moves_a_hook_edge_and_never_attests_it() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, _kp, stage) = seal_world("live-wrap-hook-stamp");
        do_session_start("child", Some("claude"), Some("/w"), None, None, None, None, None, None);

        stamp_attested_parent("child", "wrap");

        let rec = record("child").unwrap();
        assert_eq!(rec.parent_session_id.as_deref(), Some("wrap"));
        assert_eq!(rec.attested_spawner, None, "only registration attests an edge");
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// The hook door's three edge writers all leave a conducted wrap alone,
    /// while a hook-registered record still follows the wrap that hosts it.
    #[test]
    fn hook_door_never_moves_a_conducted_wraps_edge() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, _kp, stage) = seal_world("live-wrap-hook-conducted");
        do_session_start(
            "victim", Some("claude"), Some("/w"), None, Some("honest"), Some(true), None, None, None,
        );

        stamp_attested_parent("victim", "wrap");
        do_hook_session_start(aoide_protocol::Door::Cli, "victim", Some("claude"), Some("/w"), None, Some("wrap"), None, &[]);
        clear_stale_parent("victim");

        let rec = record("victim").unwrap();
        assert_eq!(rec.parent_session_id.as_deref(), Some("honest"), "a hook names no conducted edge");
        assert_eq!(rec.attested_spawner, None);

        do_session_start("hook-child", Some("claude"), Some("/w"), None, None, None, None, None, None);
        stamp_attested_parent("hook-child", "wrap");
        assert_eq!(record("hook-child").unwrap().parent_session_id.as_deref(), Some("wrap"));
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn session_start_parent_is_never_stamped_as_the_spawner() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, _kp, stage) = seal_world("live-wrap-start-parent");

        do_session_start("child", Some("claude"), Some("/w"), None, Some("orch"), None, None, None, None);

        let rec = record("child").unwrap();
        assert_eq!(rec.parent_session_id.as_deref(), Some("orch"));
        assert_eq!(rec.attested_spawner, None, "an explicit --parent is a caller's word, not a kernel fact");
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// `graph link` would re-parent the wrap, and the set drops its spawn stamp.
    /// A protected wrap's edge is never set there; a plain child still links.
    #[test]
    fn graph_link_refuses_to_re_parent_a_protected_wrap() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, kp, stage) = seal_world("live-wrap-link");
        sealed_live_wrap("wrap", &kp);
        do_session_start("plain", Some("claude"), Some("/p"), None, None, None, None, None, None);
        do_session_start("other", Some("claude"), Some("/o"), None, None, None, None, None, None);
        let before = std::fs::read(sessions_path()).unwrap();

        let out = link(&invocation(&["graph", "link"], &["wrap", "other"]));

        assert_eq!(out.status, Status::Error, "{}", out.message);
        assert!(out.message.contains("cannot be"), "the taught refusal: {}", out.message);
        assert_eq!(std::fs::read(sessions_path()).unwrap(), before, "the roster is byte-identical");
        assert_eq!(record("wrap").unwrap().parent_session_id, None);

        let plain = link(&invocation(&["graph", "link"], &["plain", "other"]));
        assert_eq!(plain.status, Status::Ok, "{}", plain.message);
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// A start that changes no identity field is a no-op on a record with no
    /// `kind`, the field `upsert_session` would backfill. Neither door refuses
    /// it and neither writes it.
    #[test]
    fn a_same_fields_start_on_a_kindless_protected_wrap_is_a_no_op() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, kp, stage) = seal_world("live-wrap-kindless");
        sealed_live_wrap("wrap", &kp);
        clear_kind("wrap");
        std::env::set_var("AOIDE_DAEMON_SOCKET", stage.join("no-such-aoided.sock"));
        let bytes = std::fs::read(sessions_path()).unwrap();

        let cli = session_start(&flag_invocation(
            &["session", "start"],
            &[("id", "wrap"), ("agent", "claude"), ("cwd", "/w")],
        ));
        assert_eq!(cli.status, Status::Ok, "{}", cli.message);
        assert!(cli.message.contains("no field changed"), "{}", cli.message);

        let hook = do_hook_session_start(
            aoide_protocol::Door::Cli, "wrap", Some("claude"), Some("/w"), None, None, None, &[],
        );
        assert_eq!(hook.status, Status::Ok, "{}", hook.message);
        assert_eq!(std::fs::read(sessions_path()).unwrap(), bytes, "neither start writes the record");
        assert_eq!(record("wrap").unwrap().kind, None);
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// `session bind` on a protected wrap: the first bind writes the enduring
    /// id, a different one is refused with the roster byte-identical, and the
    /// bound one again is a no-op.
    #[test]
    fn a_protected_wrap_takes_its_first_enduring_binding_and_refuses_another() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, kp, stage) = seal_world("live-wrap-bind");
        sealed_live_wrap("wrap", &kp);
        let bind = |key: &str| {
            let mut inv = flag_invocation(&["session", "bind"], &[("id", "wrap"), ("agent-id", key)]);
            inv.door = aoide_protocol::Door::Daemon;
            session_bind(&inv)
        };

        let first = bind("alpha");
        assert_eq!(first.status, Status::Ok, "{}", first.message);
        assert_eq!(record("wrap").unwrap().enduring_agent_id.as_deref(), Some("alpha"));
        let bytes = std::fs::read(sessions_path()).unwrap();

        let other = bind("beta");
        assert_eq!(other.status, Status::Error, "{}", other.message);
        assert_eq!(std::fs::read(sessions_path()).unwrap(), bytes, "a refused rebind writes nothing");

        let same = bind("alpha");
        assert_eq!(same.status, Status::Ok, "{}", same.message);
        assert_eq!(std::fs::read(sessions_path()).unwrap(), bytes, "the same binding is a no-op");
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// The reaper prunes a finished spawner. A protected survivor keeps its edge
    /// and spawn stamp, since it is alive; a plain child's edge is cleared as
    /// before. The survivor's parent is then a missing record, which the readers
    /// treat as no parent.
    #[test]
    fn a_reaped_spawner_keeps_a_protected_survivors_edge_and_clears_a_plain_childs() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, kp, stage) = seal_world("live-wrap-reap-edge");
        let _hypr = EnvVars::save(&["HYPRLAND_INSTANCE_SIGNATURE"]);
        std::env::remove_var("HYPRLAND_INSTANCE_SIGNATURE");
        std::fs::create_dir_all(stage.join("aoide")).unwrap();
        std::env::set_var("AOIDE_DAEMON_SOCKET", stage.join("no-such-aoided.sock"));

        do_session_start("spawner", Some("shell"), Some("/s"), None, None, None, None, None, None);
        set_state("spawner", "done");
        sealed_live_wrap("wrap", &kp);
        set_spawn_edge("wrap", "spawner");
        do_session_start("plain", Some("claude"), Some("/p"), None, None, None, None, None, None);
        set_spawn_edge("plain", "spawner");
        do_session_start("dead", Some("claude"), Some("/d"), None, None, None, None, None, Some(999_999_999));
        let mut hooks: HooksFile = load_stage(&hooks_path()).unwrap_or_default();
        upsert_hook(&mut hooks.hooks, "plain", "working", &now_iso_utc());
        write_stage(&hooks_path(), &hooks).unwrap();

        let out = crate::reap::reap(&flag_invocation(&["session", "reap"], &[]));

        assert_ne!(out.status, Status::Error, "{}", out.message);
        assert!(record("spawner").is_none(), "the finished spawner is pruned");
        let wrap = record("wrap").expect("the protected survivor is kept");
        assert_eq!(wrap.parent_session_id.as_deref(), Some("spawner"), "its edge is kept");
        assert_eq!(wrap.attested_spawner.as_deref(), Some("spawner"), "its spawn stamp is kept");
        let plain = record("plain").expect("the plain child is kept");
        assert_eq!(plain.parent_session_id, None, "a plain child's edge is cleared");
        assert_eq!(plain.attested_spawner, None);
        let _ = std::fs::remove_dir_all(&stage);
    }
}
