//! Session pid and window records, core side: the harness walk over a hook's pid ancestry, the
//! chain a hook door stores, registration parentage, the untracked-terminal (`win:*`) reconciler,
//! and the three window verbs the desktop calls through the door: `session window attach` (a
//! session's window and workspace), `session window closed` (the sessions that ran in a closed
//! window end, via the reaper) and `session window roster` (the terminal windows the desktop
//! reads, published as `win:*` records). Nothing here reads a compositor: the desktop hands core
//! what it saw. A session's liveness is its pid alone, except that a window reported closed is
//! evidence the reaper acts on at once.

use super::conduct::{is_invocation_plumbing, proc_argv};
use super::doc::restage_graph;
use super::model::{
    canonical_state, load_stage, observe_workspace, projects_path, sessions_path, write_stage, Project,
    ProjectsFile, SessionRecord, SessionsFile, STAGE_GRAPH_VERSION,
};
use super::send::STDIN_PAYLOAD_FLAG;
use super::session_store::{ancestry_may_move, chain_is_takeable};
use aoide_protocol::output::Outcome;
use aoide_protocol::{Door, Invocation};
use aoide_storage::fs::{pid_is_alive, with_stage_lock};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::io::{IsTerminal, Read};

/// Normalise a Hyprland window address for comparison: lowercased, with any
/// leading `0x` stripped. The stored `windowAddress` and hyprctl's reported
/// addresses can disagree on case and on a present/absent `0x` prefix
/// (hyprctl reports e.g. `0x55…`); this makes the match tolerant of both.
///
/// Widened from `pub(crate)` to `pub` at P-A1 of the binary-split
/// workstream: `aoide-screen` (moved out of this crate) needs the same
/// address comparison for its session-targeted commands (`screen shot
/// --session`, `screen point --from-shot`, …).
pub fn normalize_addr(addr: &str) -> String {
    let a = addr.trim();
    let a = a
        .strip_prefix("0x")
        .or_else(|| a.strip_prefix("0X"))
        .unwrap_or(a);
    a.to_ascii_lowercase()
}

/// The registered projects, for [`observe_workspace`] — read INSIDE the same
/// stage-lock hold the observation is written under, so a binding can never be
/// read half-written, and an unreadable/absent `projects.json` degrades to "no
/// bindings" (no stamp) rather than failing the window write.
fn projects_now() -> Vec<Project> {
    load_stage::<ProjectsFile>(&projects_path())
        .map(|f| f.projects)
        .unwrap_or_default()
}

/// The pid-ancestry chain of `pid`, self first, walking up the ppid chain via
/// `/proc`. Bounded (a bad `/proc` or a self-parenting loop can never spin) and
/// stops at init (ppid ≤ 1) — the terminal is always a mid-chain ancestor.
///
/// Widened from module-private to `pub(in crate::graph)` (task #89): the
/// automatic-parenting seam (`ancestry_parent`, below) and the hook door
/// (`send.rs`, stamping a fresh session's own `hookAncestry`) both need this
/// SAME real-`/proc` walk — never a second implementation of it. The BODY
/// (and its `parent_pid` stat-line parse) lives in
/// `aoide_storage::attest::pid_ancestry` as of LANE IDENTITY P-ID4 — the
/// secrets broker's origin gate needs the identical walk and `aoide-secrets`
/// cannot depend on this crate (that module's own doc has the full DAG
/// argument); this delegate keeps every `crate::graph` call site unchanged.
pub fn pid_ancestry(pid: i32) -> Vec<i32> {
    aoide_storage::attest::pid_ancestry(pid)
}

/// The index in `ancestry` (self-first, the hook process at 0) of the hook's harness: the first
/// ancestor past the hook that is not invocation plumbing ([`is_invocation_plumbing`]). `None`
/// when the chain ends first.
fn harness_index(ancestry: &[i32]) -> Option<usize> {
    ancestry
        .iter()
        .enumerate()
        .skip(1)
        .find(|&(_, &pid)| !is_invocation_plumbing(proc_argv(pid).as_deref()))
        .map(|(i, _)| i)
}

/// The chain a hook door lands on its record: the hook's ancestry from its harness up, self-first,
/// at most 8 pids. The hook process and every shell or launcher beneath the harness are never
/// stored, whatever wrap ran the hook. `None` when no harness is found, and the door then moves no
/// chain. It lands at registration and when a per-turn hook finds the stored chain stale or empty
/// ([`ensure_session_chain`]). Each move is gated by `ancestry_may_move`, and a per-turn move also
/// by `chain_is_takeable`. It walks `hook_pid`, never `std::process::id()`: on the daemon-routed
/// path those are two different processes, and `aoided`'s own ancestry describes systemd's tree,
/// not the agent's.
pub(in crate::graph) fn hook_chain(hook_pid: i32) -> Option<Vec<i32>> {
    chain_of(&pid_ancestry(hook_pid))
}

/// [`hook_chain`]'s walk over an ancestry already read.
fn chain_of(ancestry: &[i32]) -> Option<Vec<i32>> {
    let at = harness_index(ancestry)?;
    Some(ancestry[at..].iter().take(8).copied().collect())
}


/// Read `pid`'s start time (`/proc/<pid>/stat` field 22, 1-indexed —
/// `man proc(5)`) — LANE IDENTITY P-ID1's other half of the (pid,
/// starttime) reuse-proof identity a sealed credential is minted over (this
/// module's own doc header: "a pid alone is reusable, (pid, starttime) is
/// the reuse-proof identity"). The BODY (the "split after the FINAL `)`"
/// stat-line parse) lives in `aoide_storage::attest::pid_starttime` as of
/// LANE IDENTITY P-ID4, beside the walk and the seal-verify it feeds — this
/// delegate keeps the `crate::graph::pid_starttime` re-export and its
/// callers unchanged.
pub fn pid_starttime(pid: i32) -> Option<u64> {
    aoide_storage::attest::pid_starttime(pid)
}

/// Automatic-parenting seam (task #89), tier 2: find the live AGENT-kind
/// session whose own `hookAncestry` (stamped at ITS latest SessionStart, see
/// `session_store::do_hook_session_start`) intersects THIS process's own
/// `/proc` ancestry — i.e. the agent this `wrap`/`conduct`/`spawn`
/// registration is genuinely running underneath, however many shells or
/// nested `conduct`s separate them. Walks the ancestry SELF-OUTWARD (nearest
/// ancestor first) so the first match is also the DEEPEST/closest one —
/// exactly the tie-break the design calls for when more than one live
/// agent's ancestry could theoretically match. `None` when nothing
/// intersects (a bare-terminal `aoide conduct` with no agent above it).
pub(in crate::graph) fn ancestry_parent(sessions: &[SessionRecord]) -> Option<String> {
    let mine = pid_ancestry(std::process::id() as i32);
    for pid in &mine {
        if let Some(s) = sessions
            .iter()
            .find(|s| s.state != "done" && crate::reap::is_agent_kind(s) && s.hook_ancestry.contains(pid))
        {
            return Some(s.session_id.clone());
        }
    }
    None
}

/// Does the KERNEL back `parent` as a session this process runs beneath?
/// True when the parent record (live, not `id` itself) carries a pid that is
/// really in this process's own `/proc` ancestry — a conducted wrap running
/// above this registration — or is the agent [`ancestry_parent`] finds by its
/// `hookAncestry`. An explicit `--parent` or the ambient `AOIDE_SESSION_ID`
/// naming anything else is only a caller's word, however honest. Evaluated
/// ONCE, by the registering process itself, because a detached spawn leaves
/// its child reparented to init and the evidence is gone from `/proc` the
/// moment the spawner returns — a registration that only happens after that
/// attests nothing, and the child stays silent: the answer is stamped as
/// `attestedSpawner`.
pub(in crate::graph) fn spawner_is_attested(
    parent: &str,
    id: &str,
    sessions: &[SessionRecord],
) -> bool {
    if parent == id {
        return false;
    }
    let mine = pid_ancestry(std::process::id() as i32);
    let by_pid = sessions.iter().any(|s| {
        s.session_id == parent
            && s.state != "done"
            && s.pid.is_some_and(|p| mine.contains(&(p as i32)))
    });
    by_pid || ancestry_parent(sessions).as_deref() == Some(parent)
}

/// The full 3-tier parent-resolution precedence for a `wrap`/`conduct`/
/// `spawn` registration (task #89) — `graph spawn` re-execs `conduct
/// --headless`, so this single function backs all three commands via
/// `conduct`'s own call and `wrap`'s parallel one:
///
///   1. an explicit `--parent` flag wins outright (unchanged, pre-existing);
///   2. failing that, [`ancestry_parent`] — the registering process's own
///      `/proc` ancestry intersected against every live agent's
///      `hookAncestry`;
///   3. failing THAT, the ambient `AOIDE_SESSION_ID` env, guarded against
///      naming the record's OWN fresh `id` (a self-parent) — the same
///      env-parent pattern the hook door (`send.rs`) already uses, kept
///      here only as a last resort now that the ancestry walk is primary:
///      unlike a hook session (whose env is set explicitly by ITS OWN
///      launching wrap, and so is reliable), a `wrap`/`conduct`/`spawn`
///      invoked from deep inside an agent's shell tool inherits whatever
///      ambient id an OUTER terminal wrap set, which can be a stale
///      ancestor rather than the true launching agent — exactly the
///      "spawned wraps parent under the terminal as siblings" bug this
///      whole precedence order exists to fix. S-B (the Osaka wrong-ancestry
///      fix's belt-and-braces half): the ambient id must ALSO name a record
///      in `sessions` that is still live (`canonical_state(&s.state) !=
///      "done"`) — a value naming no record at all, or a record that has
///      already ended, is refused outright rather than adopted as a parent.
///      This alone would not have caught the Osaka case (the leaked id
///      named the User's own LIVE session), so it is depth, not the fix —
///      see `aoide-server`'s `a2a.rs::spawn_child_command` doc comment for
///      the actual fix, the daemon clearing its own `AOIDE_SESSION_ID`
///      before it ever reaches a spawned child's env.
pub(in crate::graph) fn resolve_registration_parent(
    explicit: Option<&str>,
    id: &str,
    sessions: &[SessionRecord],
) -> Option<String> {
    if let Some(p) = explicit {
        return Some(p.to_string());
    }
    if let Some(p) = ancestry_parent(sessions) {
        return Some(p);
    }
    std::env::var("AOIDE_SESSION_ID")
        .ok()
        .filter(|p| !p.is_empty() && p != id)
        .filter(|p| {
            sessions
                .iter()
                .find(|s| &s.session_id == p)
                .is_some_and(|s| canonical_state(&s.state) != "done")
        })
}

/// Is `rec` itself a headless conducted wrap — the PERMANENT windowless
/// anchor a lineage walk stops at? Checks the `headless` flag FIRST (task
/// #89: a registration-fact, stamped once, that survives a
/// corrupted `windowAddress`) and falls back to `windowAddress` emptiness
/// only for a record from before that field existed (or a conducted wrap
/// that legitimately hasn't resolved its window yet — an INTERACTIVE
/// `conduct`/`wrap` still degrades to the old empty-address signal).
fn is_windowless_wrap(rec: &SessionRecord) -> bool {
    rec.conductable == Some(true) && (rec.headless || rec.window_address.is_empty())
}

/// True when `parent`'s lineage runs through a conducted wrap (`conductable`)
/// that is windowless — either `headless` (the permanent registration fact)
/// or, for a legacy record, an EMPTY `windowAddress` — before reaching any
/// windowed anchor. The session is windowless BY CONSTRUCTION (a nested
/// headless `conduct`/`spawn`, task #89), so the caller must skip the window
/// backfill outright rather than pid-ancestry-walking to the wrong window
/// (the ENCLOSING terminal's — the exact bug that made a headless nested
/// session collide, same-window, with the very agent it runs beneath). Walks
/// `parentSessionId` from `parent` itself, bounded against a cycle; stops at
/// the FIRST conducted-wrap ancestor found — [`is_windowless_wrap`] decides
/// outright, whichever way. No conducted-wrap ancestor at all (no parent, a
/// dangling link, or a chain that never crosses one) keeps today's
/// backfill — there is no evidence of windowlessness to act on.
pub(in crate::graph) fn windowless_by_lineage_from_parent(
    parent: Option<&str>,
    sessions: &[SessionRecord],
) -> bool {
    let Some(mut current) = parent.map(str::to_string) else {
        return false;
    };
    let mut seen = HashSet::new();
    loop {
        if !seen.insert(current.clone()) {
            return false; // cycle guard
        }
        let Some(rec) = sessions.iter().find(|s| s.session_id == current) else {
            return false; // dangling parent link
        };
        if rec.conductable == Some(true) {
            return is_windowless_wrap(rec);
        }
        match &rec.parent_session_id {
            Some(p) => current = p.clone(),
            None => return false,
        }
    }
}

/// Same check, starting from an ALREADY-REGISTERED session's own record
/// rather than a raw parent id — the shape the desktop's sweep and the
/// chain fill need. `false` (keep today's backfill)
/// for an unknown `id`. Answers `true` in TWO cases: `id` is ITSELF a
/// windowless conducted wrap (task #89 — the desktop's
/// sweep iterates every record with an empty
/// address, the wrap's own included, so its self-windowlessness must be
/// checked directly, not only inferred from a child's parent chain), or its
/// `parentSessionId` chain is (delegates to
/// [`windowless_by_lineage_from_parent`]).
pub fn windowless_by_lineage(id: &str, sessions: &[SessionRecord]) -> bool {
    let Some(rec) = sessions.iter().find(|s| s.session_id == id) else {
        return false;
    };
    if is_windowless_wrap(rec) {
        return true;
    }
    windowless_by_lineage_from_parent(rec.parent_session_id.as_deref(), sessions)
}

/// Per-turn fill and heal of a hook-registered session's stored `hookAncestry`: a chainless record
/// live transition left it behind) is replaced by the live harness's chain, in one locked write. A
/// takes the live harness's chain, and a stale chain (a dead to live transition left it behind) is
/// replaced, in one locked write. A chain a live process of the record's own holds is never
/// written here, and a windowless-by-lineage record is never touched.
/// `hook_pid` is the hook process, and `None` walks nothing. It never writes `pid`,
/// `windowAddress` or the session `state`: binding a window is the desktop's, through
/// [`attach_window`].
pub(in crate::graph) fn ensure_session_chain(id: &str, hook_pid: Option<i32>) {
    let Some(hook_pid) = hook_pid else {
        return;
    };
    // Cheap, unlocked pre-check. `load_stage`/`write_stage` are atomic-rename based, so an unlocked
    // read never tears, only occasionally stales; the locked write below re-checks.
    let Ok(file) = load_stage::<SessionsFile>(&sessions_path()) else {
        return;
    };
    let Some(rec) = file.sessions.iter().find(|s| s.session_id == id) else {
        return;
    };
    if windowless_by_lineage(id, &file.sessions) {
        return;
    }
    let Some(chain) = chain_of(&pid_ancestry(hook_pid)) else {
        return;
    };
    if !chain_is_takeable(rec, &chain, pid_is_alive) {
        return;
    }
    with_stage_lock(|| {
        let mut file: SessionsFile = match load_stage(&sessions_path()) {
            Ok(f) => f,
            Err(_) => return,
        };
        let Some(s) = file.sessions.iter_mut().find(|s| s.session_id == id) else {
            return;
        };
        if !chain_is_takeable(s, &chain, pid_is_alive) || !ancestry_may_move(s, &chain, pid_is_alive) {
            return;
        }
        s.hook_ancestry = chain.clone();
        if file.schema_version.is_empty() {
            file.schema_version = STAGE_GRAPH_VERSION.to_string();
        }
        if write_stage(&sessions_path(), &file).is_ok() {
            let _ = restage_graph();
        }
    });
}

/// What a window attach did: the stage it left, or the refusal it hit.
pub enum Attach {
    /// The session holds the window now. `changed` says the stage moved, and `workspace` is the id
    /// it holds.
    Bound { changed: bool, workspace: Option<i64> },
    /// No session has that id.
    NoSession,
    /// The session already holds another window: one window per session until that one closes.
    Held { window: String },
    /// `sessions.json` could not be read or written.
    Unwritten,
}

/// Record the window a session runs in. Filling an empty window is the bind. The same window
/// again only moves the workspace, through the one `observe_workspace` seam, so the birth default
/// project is never re-stamped. A different window is refused.
pub fn attach_window(id: &str, window: &str, workspace: Option<i64>) -> Attach {
    with_stage_lock(|| {
        let mut file: SessionsFile = match load_stage(&sessions_path()) {
            Ok(f) => f,
            Err(_) => return Attach::Unwritten,
        };
        let Some(s) = file.sessions.iter_mut().find(|s| s.session_id == id) else {
            return Attach::NoSession;
        };
        if !s.window_address.is_empty() && normalize_addr(&s.window_address) != normalize_addr(window) {
            return Attach::Held {
                window: s.window_address.clone(),
            };
        }
        let mut changed = false;
        if s.window_address.is_empty() {
            s.window_address = window.to_string();
            changed = true;
        }
        if let Some(ws) = workspace {
            changed |= observe_workspace(s, Some(ws), &projects_now());
        }
        let bound = Attach::Bound {
            changed,
            workspace: s.workspace,
        };
        if !changed {
            return bound;
        }
        if file.schema_version.is_empty() {
            file.schema_version = STAGE_GRAPH_VERSION.to_string();
        }
        match write_stage(&sessions_path(), &file) {
            Ok(_) => {
                let _ = restage_graph();
                bound
            }
            Err(_) => Attach::Unwritten,
        }
    })
}

/// `aoide session window attach <id> <address> [--workspace <n>]`, the one write a desktop window
/// binding makes. It routes through `aoided` as `session bind` does. The gate refuses a missing
/// or blank id or address; the handler refuses an unknown session and a session that already holds
/// a different window.
pub fn session_window_attach(inv: &Invocation) -> Outcome {
    let cmd = "session.window.attach";
    if inv.door != Door::Daemon {
        return aoide_client::daemon::daemon_dispatch(inv).unwrap_or_else(|| {
            Outcome::error(cmd, "aoided must be running to attach a session window")
        });
    }
    let id = inv.args.first().map_or("", String::as_str);
    let window = inv.args.get(1).map_or("", |w| w.trim());
    let workspace = match inv.flags.get("workspace") {
        None => None,
        Some(raw) => match raw.parse::<i64>() {
            Ok(ws) => Some(ws),
            Err(_) => {
                return Outcome::usage(cmd, format!("--workspace takes an integer id, not `{raw}`"));
            }
        },
    };
    match attach_window(id, window, workspace) {
        Attach::Bound { changed, workspace } => {
            let out = Outcome::ok(cmd, format!("session `{id}` runs in window {window}"))
                .with_data(json!({
                    "session": id,
                    "window": window,
                    "workspace": workspace,
                    "changed": changed,
                }));
            if changed {
                out.changed([sessions_path().to_string_lossy().into_owned()])
            } else {
                out
            }
        }
        Attach::NoSession => Outcome::error(cmd, format!("no session `{id}`"))
            .with_data(json!({ "reason": "session-not-found", "session": id })),
        Attach::Held { window: held } => Outcome::error(
            cmd,
            format!("session `{id}` already runs in window {held}; a session has one window until it closes"),
        )
        .with_data(json!({ "reason": "window-held", "session": id, "window": held })),
        Attach::Unwritten => Outcome::error(cmd, "could not write sessions.json"),
    }
}



/// Publish the untracked-terminal roster for these terminal windows: a `shell` record for every
/// window no tracked session claims, and removal of any `win:*` record whose window has closed.
/// Returns true iff `sessions.json` changed. Called by [`session_window_roster`].
fn sync_untracked_terminal_rows(windows: &[TermWindow]) -> bool {
    with_stage_lock(|| {
        let mut file: SessionsFile = match load_stage(&sessions_path()) {
            Ok(f) => f,
            Err(_) => return false,
        };
        let (sessions, changed) = reconcile_untracked_terminals(
            std::mem::take(&mut file.sessions),
            windows,
            &projects_now(),
        );
        file.sessions = sessions;
        if !changed {
            return false;
        }
        if file.schema_version.is_empty() {
            file.schema_version = STAGE_GRAPH_VERSION.to_string();
        }
        if write_stage(&sessions_path(), &file).is_ok() {
            let _ = restage_graph();
            return true;
        }
        false
    })
}



/// `aoide session window closed <address>`: the desktop saw this window close. The reaper judges the
/// sessions that ran in it on that evidence alone (`reap::reap_closed_window`): each ends unless a
/// live harness still anchors it, and every claim on the window is released. Requires aoided.
pub fn session_window_closed(inv: &Invocation) -> Outcome {
    let cmd = "session.window.closed";
    let window = inv.args.first().map_or("", |w| w.trim());
    if window.is_empty() {
        return Outcome::usage(cmd, "session window closed takes the address of the window that closed");
    }
    if inv.door != Door::Daemon {
        return aoide_client::daemon::daemon_dispatch(inv)
            .unwrap_or_else(|| Outcome::error(cmd, "aoided must be running to record a closed window"));
    }
    let mut outcome = crate::reap::reap_closed_window(inv, window);
    crate::reap::announce_if_changed(inv, &mut outcome);
    outcome
}

/// `aoide session show <id>`: one local session's window and workspace, read-only. The desktop's
/// focus verb reads them through the door, not from `sessions.json` itself. `<id>` is an
/// exact local session id; an id no record carries is refused as `session-not-found`.
pub fn session_show(inv: &Invocation) -> Outcome {
    let cmd = "session.show";
    let id = inv.args.first().map_or("", |s| s.trim());
    if id.is_empty() {
        return Outcome::usage(cmd, "session show takes the id of a local session");
    }
    let file: SessionsFile = match load_stage(&sessions_path()) {
        Ok(f) => f,
        Err(e) => return super::common::stage_error(cmd, e),
    };
    let Some(rec) = file.sessions.iter().find(|s| s.session_id == id) else {
        return Outcome::error(cmd, format!("no local session with id `{id}`"))
            .with_data(json!({ "reason": "session-not-found", "sessionId": id }));
    };
    Outcome::ok(cmd, format!("session {id}")).with_data(json!({
        "sessionId": rec.session_id,
        "windowAddress": rec.window_address,
        "workspace": rec.workspace,
    }))
}

/// `aoide session window roster`, the terminal windows on stdin as a JSON array of `TermWindow`s:
/// the one reconcile the desktop makes, a `win:*` record for each terminal window no tracked session
/// claims and the removal of any whose window is gone. Requires aoided. The CLI leg reads stdin once
/// and forwards it on the payload flag, as `session hook` does.
pub fn session_window_roster(inv: &Invocation) -> Outcome {
    let cmd = "session.window.roster";
    if inv.door != Door::Daemon {
        if std::io::stdin().is_terminal() {
            return Outcome::usage(cmd, "pipe the window list in on stdin: a JSON array of terminal windows");
        }
        let mut payload = String::new();
        if let Err(e) = std::io::stdin().read_to_string(&mut payload) {
            return Outcome::error(cmd, format!("reading the window list from stdin: {e}"));
        }
        let mut routed = inv.clone();
        routed.flags.insert(STDIN_PAYLOAD_FLAG.to_string(), payload);
        return aoide_client::daemon::daemon_dispatch(&routed)
            .unwrap_or_else(|| Outcome::error(cmd, "aoided must be running to publish the window roster"));
    }
    let Some(payload) = inv.flags.get(STDIN_PAYLOAD_FLAG) else {
        return Outcome::usage(cmd, "session window roster reads the window list from stdin");
    };
    let windows: Vec<TermWindow> = match serde_json::from_str(payload) {
        Ok(w) => w,
        Err(e) => return Outcome::usage(cmd, format!("stdin is not a JSON array of terminal windows: {e}")),
    };
    let changed = sync_untracked_terminal_rows(&windows);
    let out = Outcome::ok(cmd, format!("published {} terminal window(s)", windows.len()))
        .with_data(json!({ "windows": windows.len(), "changed": changed }));
    if changed {
        out.changed([sessions_path().to_string_lossy().into_owned()])
    } else {
        out
    }
}

/// Terminal window CLASSES we recognize as a tty (mirrors the QML `termClassRe`
/// this supersedes) — ascii-lowercased compare.
const TERMINAL_CLASSES: &[&str] = &[
    "kitty",
    "foot",
    "footclient",
    "alacritty",
    "wezterm",
    "org.wezfurlong.wezterm",
    "ghostty",
    "com.mitchellh.ghostty",
    "xterm",
    "uxterm",
    "konsole",
    "urxvt",
    "rxvt",
    "termite",
    "tilix",
    "contour",
    "rio",
    "st",
    "kgx",
    "org.gnome.console",
    "blackbox",
    "terminator",
    "sakura",
    "wave",
    "xfce4-terminal",
    "gnome-terminal",
    "qterminal",
    "lxterminal",
    "deepin-terminal",
];

/// Is a window class one we treat as a terminal (ascii-lowercased exact match
/// against [`TERMINAL_CLASSES`])? Pure — the classification seam under test.
pub(crate) fn is_terminal_class(class: &str) -> bool {
    let c = class.trim().to_ascii_lowercase();
    TERMINAL_CLASSES.contains(&c.as_str())
}

/// One live terminal window distilled from a client the desktop reads off the compositor — the
/// pure-core input for [`reconcile_untracked_terminals`], so the reconciliation
/// is testable without a compositor (`cwd` is pre-read from `/proc/<pid>/cwd` by
/// the I/O wrapper). The wire shape of one entry in the roster verb's stdin array.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TermWindow {
    pub address: String,
    pub class: String,
    pub title: String,
    pub workspace: Option<i64>,
    pub pid: Option<i32>,
    pub mapped: bool,
    pub cwd: String,
}

/// Reconcile the synthetic `win:*` terminal records against the live terminal
/// windows — the PURE CORE (fed fake `TermWindow`s in tests). Given the current
/// sessions and the live windows, returns the updated sessions plus whether it
/// changed anything.
///
/// Rules:
///   * A LIVE, mapped, terminal-class window whose (normalized) address is NOT
///     claimed by any TRACKED (non-`win:`) session gets a synthetic record keyed
///     `win:<normalized-address>` (`agent`/`kind` = "shell", `state` = "idle").
///     Upserted in place, so a re-scan never duplicates and only mutates the
///     record's live fields (`cwd`/`title`/`workspace`/`pid`/`windowAddress`).
///   * A window already claimed by a tracked session is left to that session —
///     never double-published (the tracked record carries the rich state).
///   * A synthetic `win:*` record whose window no longer appears live (closed, or
///     newly claimed by a tracked session) is REMOVED.
pub fn reconcile_untracked_terminals(
    mut sessions: Vec<SessionRecord>,
    windows: &[TermWindow],
    projects: &[Project],
) -> (Vec<SessionRecord>, bool) {
    // Addresses owned by a TRACKED (non-synthetic) session — a real agent/shell
    // record already represents that window, so it is never re-published.
    let tracked_addrs: HashSet<String> = sessions
        .iter()
        .filter(|s| !s.session_id.starts_with("win:"))
        .filter(|s| !s.window_address.is_empty())
        .map(|s| normalize_addr(&s.window_address))
        .collect();

    // The synthetic roster we WANT: one entry per live, mapped, terminal-class,
    // unclaimed window, keyed by normalized address.
    let mut desired: HashMap<String, &TermWindow> = HashMap::new();
    for w in windows {
        if !w.mapped || !is_terminal_class(&w.class) {
            continue;
        }
        let na = normalize_addr(&w.address);
        if na.is_empty() || tracked_addrs.contains(&na) {
            continue;
        }
        desired.insert(na, w);
    }

    let mut changed = false;

    // Drop synthetic records whose window is no longer desired (closed, or a
    // tracked session now owns the address).
    let before = sessions.len();
    sessions.retain(|s| match s.session_id.strip_prefix("win:") {
        Some(addr) => desired.contains_key(addr),
        None => true,
    });
    if sessions.len() != before {
        changed = true;
    }

    // Upsert a synthetic record per desired window.
    for (na, w) in &desired {
        let sid = format!("win:{na}");
        let want_title = if w.title.is_empty() {
            None
        } else {
            Some(w.title.clone())
        };
        let want_pid = w.pid.filter(|p| *p > 0).map(|p| p as u32);
        if let Some(rec) = sessions.iter_mut().find(|s| s.session_id == sid) {
            if rec.window_address != w.address {
                rec.window_address = w.address.clone();
                changed = true;
            }
            if rec.cwd != w.cwd {
                rec.cwd = w.cwd.clone();
                changed = true;
            }
            if rec.title != want_title {
                rec.title = want_title;
                changed = true;
            }
            // A bare terminal BORN on a bound workspace joins its project,
            // exactly as an agent's session does — the observation seam is the
            // same one, so the win: record's birth default and the id it keeps
            // across a drag cannot drift from a tracked record's.
            changed |= observe_workspace(rec, w.workspace, projects);
            if rec.pid != want_pid {
                rec.pid = want_pid;
                changed = true;
            }
            // Keep the identity invariants a synthetic record always carries.
            if rec.agent != "shell" {
                rec.agent = "shell".to_string();
                changed = true;
            }
            if rec.state != "idle" {
                rec.state = "idle".to_string();
                changed = true;
            }
            if rec.kind.as_deref() != Some("shell") {
                rec.kind = Some("shell".to_string());
                changed = true;
            }
        } else {
            let petname = aoide_storage::petname::mint_for(&sessions);
            let mut rec = SessionRecord {
                session_id: sid,
                agent: "shell".to_string(),
                window_address: w.address.clone(),
                cwd: w.cwd.clone(),
                state: "idle".to_string(),
                kind: Some("shell".to_string()),
                title: want_title,
                pid: want_pid,
                // Born with no workspace, then observed through the ONE seam
                // below: constructing it WITH the id would make the seam read
                // this as a move and skip the birth default entirely.
                workspace: None,
                petname: Some(petname),
                ..Default::default()
            };
            observe_workspace(&mut rec, w.workspace, projects);
            sessions.push(rec);
            changed = true;
        }
    }

    (sessions, changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::session_store::{do_hook_session_start, superseded_by};
    use crate::graph::testutil::*;

    #[test]
    fn terminal_class_is_case_insensitive_and_exact() {
        assert!(is_terminal_class("kitty"));
        assert!(is_terminal_class("Kitty"));
        assert!(is_terminal_class("org.wezfurlong.wezterm"));
        assert!(!is_terminal_class("firefox"));
        assert!(!is_terminal_class("kittyfoo"));
        assert!(!is_terminal_class(""));
    }

    #[test]
    fn untracked_terminal_synthesizes_a_win_record() {
        let (out, changed) =
            reconcile_untracked_terminals(vec![], &[term_win("0xAABB", "kitty", "/home/khoa")], &[]);
        assert!(changed);
        assert_eq!(out.len(), 1);
        let r = &out[0];
        assert_eq!(r.session_id, "win:aabb");
        assert_eq!(r.agent, "shell");
        assert_eq!(r.kind.as_deref(), Some("shell"));
        assert_eq!(r.state, "idle");
        assert_eq!(r.window_address, "0xAABB");
        assert_eq!(r.cwd, "/home/khoa");
        assert_eq!(r.workspace, Some(1));
        assert_eq!(r.pid, Some(4321));
    }

    #[test]
    fn window_claimed_by_tracked_session_gets_no_synthetic_duplicate() {
        // A tracked agent already owns 0xAABB (address stored 0x-prefixed here,
        // the live client reports the same) — no `win:` record is synthesized.
        let mut tracked = session("efdc", "/w", "working", "t", None);
        tracked.window_address = "0xAABB".into();
        let (out, changed) = reconcile_untracked_terminals(
            vec![tracked],
            &[term_win("0xAABB", "kitty", "/home/khoa")],
            &[],
        );
        assert!(!changed);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].session_id, "efdc");
        assert!(!out.iter().any(|s| s.session_id.starts_with("win:")));
    }

    #[test]
    fn closed_windows_synthetic_record_is_removed() {
        // A pre-existing synthetic record whose window no longer appears live is
        // dropped; the surviving window keeps its record.
        let stale = SessionRecord {
            session_id: "win:dead".into(),
            agent: "shell".into(),
            window_address: "0xDEAD".into(),
            state: "idle".into(),
            kind: Some("shell".into()),
            ..Default::default()
        };
        let (out, changed) =
            reconcile_untracked_terminals(vec![stale], &[term_win("0xLIVE", "foot", "/tmp")], &[]);
        assert!(changed);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].session_id, "win:live");
        assert!(!out.iter().any(|s| s.session_id == "win:dead"));
    }

    #[test]
    fn non_terminal_class_window_is_ignored() {
        let (out, changed) =
            reconcile_untracked_terminals(vec![], &[term_win("0xAABB", "firefox", "/home/khoa")], &[]);
        assert!(!changed);
        assert!(out.is_empty());
    }

    #[test]
    fn unmapped_terminal_window_is_ignored() {
        let mut w = term_win("0xAABB", "kitty", "/home/khoa");
        w.mapped = false;
        let (out, changed) = reconcile_untracked_terminals(vec![], &[w], &[]);
        assert!(!changed);
        assert!(out.is_empty());
    }

    #[test]
    fn rescan_is_idempotent_and_upserts_in_place() {
        let (first, _) =
            reconcile_untracked_terminals(vec![], &[term_win("0xAABB", "kitty", "/home/khoa")], &[]);
        // A second reconcile with the same window makes no change and no duplicate.
        let (second, changed) =
            reconcile_untracked_terminals(first, &[term_win("0xAABB", "kitty", "/home/khoa")], &[]);
        assert!(!changed);
        assert_eq!(second.len(), 1);
        // A cwd change upserts the existing record in place (still one record).
        let (third, changed3) =
            reconcile_untracked_terminals(second, &[term_win("0xAABB", "kitty", "/other")], &[]);
        assert!(changed3);
        assert_eq!(third.len(), 1);
        assert_eq!(third[0].cwd, "/other");
    }

    #[test]
    fn reconcile_untracked_terminals_mints_petname_and_never_rewrites_a_named_record() {
        // A NEW synthetic terminal gets a minted petname on creation…
        let (first, changed1) =
            reconcile_untracked_terminals(vec![], &[term_win("0xAABB", "kitty", "/home/khoa")], &[]);
        assert!(changed1);
        assert!(first[0].petname.is_some(), "a freshly synthesized win: record must mint a petname");

        // …and a re-scan of the SAME window (already named) must not touch it:
        // no re-mint, and the changed flag stays false (no rewrite churn).
        let (second, changed2) =
            reconcile_untracked_terminals(first.clone(), &[term_win("0xAABB", "kitty", "/home/khoa")], &[]);
        assert!(!changed2, "re-scanning an already-named record must not flip changed");
        assert_eq!(second[0].petname, first[0].petname, "an existing petname must never be re-minted");
    }

    #[test]
    fn pid_ancestry_starts_at_self_and_is_bounded() {
        // Real /proc: our own ancestry begins with our pid and includes a parent.
        let me = std::process::id() as i32;
        let chain = pid_ancestry(me);
        assert_eq!(chain.first(), Some(&me), "self is first in the chain");
        assert!(chain.len() >= 2, "we always have at least one ancestor");
        assert!(chain.len() <= 64, "the walk is bounded");
        // A nonexistent pid yields just the seed (no /proc entry to walk up).
        assert_eq!(pid_ancestry(2_000_000_000), vec![2_000_000_000]);
    }

    #[test]
    fn pid_starttime_reads_a_nonzero_value_for_our_own_real_pid() {
        // LANE IDENTITY P-ID1: this is the exact call the daemon's `mint_seal`
        // makes over a live pid — proving it against a real, running process
        // (this test's own pid) is the ground-truth case, not a fixture.
        let me = std::process::id() as i32;
        let starttime = pid_starttime(me).expect("our own /proc/<pid>/stat must parse");
        assert!(starttime > 0, "a real process's starttime is never zero");

        // Reading it twice for the same live pid must be stable (it never
        // changes for the life of the process).
        assert_eq!(pid_starttime(me), Some(starttime));

        // A nonexistent pid yields None (no /proc entry to read) — never a
        // panic, never a fabricated 0.
        assert_eq!(pid_starttime(2_000_000_000), None);
    }

    #[test]
    fn windowless_by_lineage_stops_at_the_first_conducted_wrap_ancestor() {
        // No parent at all: no evidence of windowlessness — keep today's
        // backfill.
        assert!(!windowless_by_lineage_from_parent(None, &[]));

        // A dangling parent link (the record doesn't exist): same — no
        // evidence, keep today's backfill.
        assert!(!windowless_by_lineage_from_parent(Some("ghost"), &[]));

        // The immediate parent IS a conducted wrap with an EMPTY window —
        // windowless by construction.
        let mut wrap = session("wrap", "/w", "working", "1", None);
        wrap.conductable = Some(true);
        wrap.window_address = String::new();
        assert!(windowless_by_lineage_from_parent(Some("wrap"), &[wrap.clone()]));

        // The immediate parent is a conducted wrap WITH a window — anchored
        // normally, backfill stays live.
        let mut windowed_wrap = wrap.clone();
        windowed_wrap.window_address = "0xWIN".into();
        assert!(!windowless_by_lineage_from_parent(
            Some("wrap"),
            &[windowed_wrap]
        ));

        // A non-wrap ancestor in between (e.g. an agent parenting another
        // agent directly) is skipped — the walk continues past it to find
        // the conducted wrap further up.
        let mut mid_agent = session("mid", "/w", "working", "2", Some("wrap"));
        mid_agent.kind = Some("agent".into());
        let sessions = vec![wrap.clone(), mid_agent];
        assert!(windowless_by_lineage_from_parent(Some("mid"), &sessions));

        // A cycle in the parent chain must never loop forever — it degrades
        // to "no evidence, keep today's backfill" rather than hanging.
        let mut a = session("a", "/w", "working", "1", Some("b"));
        let mut b = session("b", "/w", "working", "2", Some("a"));
        a.kind = Some("agent".into());
        b.kind = Some("agent".into());
        assert!(!windowless_by_lineage_from_parent(Some("a"), &[a, b]));
    }

    #[test]
    fn windowless_by_lineage_looks_up_its_own_record_first() {
        // `windowless_by_lineage` (unlike the `_from_parent` core) starts
        // from an EXISTING session's own record — an unknown id has no
        // evidence either way.
        assert!(!windowless_by_lineage("unknown", &[]));

        let mut wrap = session("wrap", "/w", "working", "1", None);
        wrap.conductable = Some(true);
        wrap.window_address = String::new();
        let child = session("child", "/w", "working", "2", Some("wrap"));
        assert!(windowless_by_lineage("child", &[wrap, child]));
    }

    /// Task #89: `headless` is the PERMANENT registration
    /// fact — it must decide windowlessness even when `windowAddress` has
    /// (wrongly) been stamped with something, the exact "re-poison" shape
    /// the review flagged (an unconditional discovery call, or the listener
    /// sweep, backfilling the ENCLOSING terminal's window onto a headless
    /// wrap's own record before this fix).
    #[test]
    fn headless_flag_overrides_a_stray_nonempty_window_address() {
        let mut wrap = session("wrap", "/w", "working", "1", None);
        wrap.conductable = Some(true);
        wrap.headless = true;
        wrap.window_address = "0xSTALE".to_string(); // corrupted/stray — must not matter
        assert!(
            is_windowless_wrap(&wrap),
            "a headless wrap is windowless regardless of a stray windowAddress"
        );

        let child = session("child", "/w", "working", "2", Some("wrap"));
        assert!(
            windowless_by_lineage("child", &[wrap.clone(), child]),
            "the child's own backfill check must see through the parent's stray address"
        );

        // The self-check on `windowless_by_lineage` (not just `_from_parent`)
        // also answers true for the WRAP's own record — the shape
        // the desktop's sweep needs, since it iterates
        // every record with an empty address (or, here, a stray non-empty
        // one it might otherwise "confirm") including the wrap itself.
        assert!(is_windowless_wrap(&wrap));

        // An INTERACTIVE conducted wrap (no `headless` flag) still falls
        // back to plain `windowAddress` emptiness — unchanged, pre-existing
        // behavior for a legacy record or a real interactive `conduct`.
        let mut interactive = session("term", "/w", "working", "1", None);
        interactive.conductable = Some(true);
        interactive.window_address = String::new();
        assert!(is_windowless_wrap(&interactive));
        interactive.window_address = "0xREAL".to_string();
        assert!(!is_windowless_wrap(&interactive));
    }

    /// The walk's pure core over synthetic ancestry: the hook's own pid is never a candidate, a
    /// dead pid is plumbing (its argv cannot be read), and the harness is the first live ancestor
    /// past the hook that is not plumbing.
    #[test]
    fn harness_index_skips_plumbing() {
        const DEAD: i32 = 2_000_000_000;
        let me = std::process::id() as i32;
        let parent = pid_ancestry(me)[1];
        let ancestry = [DEAD, DEAD + 1, me, parent];
        assert_eq!(harness_index(&ancestry), Some(2), "the dead hook and sh are plumbing");
        assert_eq!(harness_index(&[DEAD]), None, "a hook with no parent has no harness");
    }

    /// Spawns one stand-in hook through `spawn`, which gets the path of `standin.sh` (it
    /// writes its own pid to the path it is given, then execs `sleep 30`, so the hook is
    /// a live process of its own) and the output path. Returns the spawned child and the
    /// hook's pid, read back from the output file.
    #[cfg(unix)]
    fn stand_in_hook(
        dir: &std::path::Path,
        name: &str,
        spawn: impl FnOnce(&std::path::Path, &std::path::Path) -> std::process::Command,
    ) -> (std::process::Child, i32) {
        let script = dir.join("standin.sh");
        std::fs::write(&script, "echo $$ > \"$1\"\nexec sleep 30\n").unwrap();
        let out = dir.join(name);
        let child = spawn(&script, &out)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let pid = (0..500)
            .find_map(|_| {
                std::thread::sleep(std::time::Duration::from_millis(10));
                std::fs::read_to_string(&out).ok()?.trim().parse::<i32>().ok()
            })
            .expect("the stand-in hook names its pid");
        (child, pid)
    }

    /// `sh -c <script>`, the shape every harness's hook command runs under.
    #[cfg(unix)]
    fn sh_c(script: &str) -> std::process::Command {
        let mut cmd = std::process::Command::new("sh");
        cmd.arg("-c").arg(script);
        cmd
    }

    /// The binary behind the `sh` that [`sh_c`] runs, found on `PATH`. A link to it under
    /// a launcher's name must still run a shell, which rules out a multi-call binary that
    /// picks its applet from argv0 (the nix build sandbox's `/bin/sh` is busybox).
    #[cfg(unix)]
    fn path_sh() -> std::path::PathBuf {
        let path = std::env::var_os("PATH").expect("PATH is set");
        let sh = std::env::split_paths(&path)
            .map(|dir| dir.join("sh"))
            .find(|sh| sh.is_file())
            .expect("an sh on PATH");
        let sh = std::fs::canonicalize(sh).unwrap();
        assert_ne!(sh.file_name().and_then(|n| n.to_str()), Some("busybox"), "a link to busybox runs its applet, not a shell");
        sh
    }

    /// Stops a stand-in hook this test started, and reaps its shell.
    #[cfg(unix)]
    fn release_stand_in(mut child: std::process::Child, pid: i32) {
        // SAFETY: `pid` is a `sleep` this test started through its own shell; nothing else holds it.
        unsafe { libc::kill(pid, libc::SIGKILL) };
        let _ = child.wait();
    }

    /// The walk's answer for a stand-in hook shaped `depth` pids above this test
    /// process: this process is the harness, the chain starts at it, and it is the
    /// ancestry from there up.
    #[cfg(unix)]
    fn assert_walk_finds_this_process_at_depth(hook: i32, depth: usize) {
        let me = std::process::id() as i32;
        let ancestry = pid_ancestry(hook);
        assert_eq!(ancestry.get(depth), Some(&me), "the shape puts this process at depth {depth}: {ancestry:?}");
        let chain = hook_chain(hook).expect("the hook finds its harness");
        assert_eq!(chain.first(), Some(&me), "the chain starts at the harness: {chain:?}");
        assert_eq!(chain, ancestry[depth..].iter().copied().take(8).collect::<Vec<i32>>());
    }

    /// Runs one stand-in shape against this test process as the harness.
    #[cfg(unix)]
    fn with_a_harness_above(name: &str, body: impl FnOnce(&std::path::Path)) {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let dir = unique_stage(name);
        std::fs::create_dir_all(&dir).unwrap();
        body(&dir);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Non-exec compound: `sh -c 'HOOK; true'` keeps the shell as the hook's parent.
    #[cfg(unix)]
    #[test]
    fn a_hook_under_a_compound_sh_c_finds_its_harness_past_the_shell() {
        with_a_harness_above("hook-walk-compound", |dir| {
            let (child, hook) = stand_in_hook(dir, "pid", |s, o| {
                sh_c(&format!("sh {} {}; true", s.display(), o.display()))
            });
            assert_walk_finds_this_process_at_depth(hook, 2);
            release_stand_in(child, hook);
        });
    }

    /// Exec in place: `sh -c 'exec HOOK'` replaces the shell, so the hook is the
    /// harness's direct child.
    #[cfg(unix)]
    #[test]
    fn a_hook_under_an_exec_in_place_sh_c_finds_its_harness() {
        with_a_harness_above("hook-walk-exec", |dir| {
            let (child, hook) = stand_in_hook(dir, "pid", |s, o| {
                sh_c(&format!("exec sh {} {}", s.display(), o.display()))
            });
            assert_walk_finds_this_process_at_depth(hook, 1);
            release_stand_in(child, hook);
        });
    }

    /// The capture wrap: `sh -c 'tee FILE | HOOK'`, the pipeline's shell above the hook.
    #[cfg(unix)]
    #[test]
    fn a_capture_pipeline_hook_finds_its_harness_past_the_pipeline_shell() {
        with_a_harness_above("hook-walk-capture", |dir| {
            let tee = dir.join("hooks.jsonl");
            let (child, hook) = stand_in_hook(dir, "pid", |s, o| {
                sh_c(&format!("tee -a {} | sh {} {}", tee.display(), s.display(), o.display()))
            });
            assert_walk_finds_this_process_at_depth(hook, 2);
            release_stand_in(child, hook);
        });
    }

    /// A direct child of the harness: the hook is spawned by this process with no shell.
    #[cfg(unix)]
    #[test]
    fn a_direct_child_hook_finds_its_harness_at_once() {
        with_a_harness_above("hook-walk-direct", |dir| {
            let (child, hook) = stand_in_hook(dir, "pid", |s, o| {
                let mut cmd = std::process::Command::new("sh");
                cmd.arg(s).arg(o);
                cmd
            });
            assert_walk_finds_this_process_at_depth(hook, 1);
            release_stand_in(child, hook);
        });
    }

    /// Launchers: `env` and `nice` exec in place inside a shell (the shell stays the
    /// parent), and launchers that persist as processes are skipped by name. The named
    /// links are `sh` under the launcher's name, so they stay in the chain.
    #[cfg(unix)]
    #[test]
    fn a_hook_behind_env_and_nice_launchers_finds_its_harness() {
        with_a_harness_above("hook-walk-launchers", |dir| {
            let (child, hook) = stand_in_hook(dir, "real", |s, o| {
                sh_c(&format!("env A=1 nice -n 5 sh {} {}; true", s.display(), o.display()))
            });
            assert_walk_finds_this_process_at_depth(hook, 2);
            release_stand_in(child, hook);

            let sh = path_sh();
            let env = dir.join("env");
            let nice = dir.join("nice");
            std::os::unix::fs::symlink(&sh, &env).unwrap();
            std::os::unix::fs::symlink(&sh, &nice).unwrap();
            let (child, hook) = stand_in_hook(dir, "named", |s, o| {
                sh_c(&format!(
                    "'{}' -c \"'{}' -c 'sh {} {}; true'; true\"",
                    env.display(),
                    nice.display(),
                    s.display(),
                    o.display()
                ))
            });
            assert_walk_finds_this_process_at_depth(hook, 3);
            release_stand_in(child, hook);
        });
    }

    /// A bound turn whose stored chain is live asks the compositor for nothing. The read is
    #[cfg(unix)]
    #[test]
    fn a_bound_turn_with_a_live_stored_chain_is_left_alone() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR"]);
        let stage = unique_stage("hook-walk-quiet-turn");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        let dir = unique_stage("hook-walk-quiet-turn-shell");
        std::fs::create_dir_all(&dir).unwrap();

        let (child, hook) = stand_in_hook(&dir, "pid", |s, o| {
            sh_c(&format!("sh {} {}; true", s.display(), o.display()))
        });
        let stored: Vec<i32> = pid_ancestry(hook).into_iter().skip(2).take(8).collect();
        assert_eq!(stored.first(), Some(&(std::process::id() as i32)), "the stored chain starts at the harness");
        do_hook_session_start(aoide_protocol::Door::Cli, "bound", Some("claude"), Some("/w"), Some("0xW"), None, None, &stored);

        ensure_session_chain("bound", Some(hook));

        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec = s.sessions.iter().find(|r| r.session_id == "bound").unwrap();
        assert_eq!(rec.hook_ancestry, stored, "the live stored chain is not rewritten");
        assert_eq!(rec.window_address, "0xW", "the bound window is untouched");
        release_stand_in(child, hook);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// A stored chain that went stale (its harness exited; a new harness runs under
    /// the same window owner) is healed by the next per-turn hook, which records the
    /// live harness. Until then a same-depth sibling registration retires the record;
    /// after it, the record survives. The hook and shell pids are children of this
    /// test, so they die before the checks that depend on them.
    #[cfg(unix)]
    #[test]
    fn a_per_turn_hook_heals_a_stale_chain_and_the_record_outlives_a_sibling() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("per-turn-heal");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        const DEAD: i32 = 2_000_000_000;
        let me = std::process::id() as i32;
        let a = pid_ancestry(me);
        assert!(a.len() >= 3, "the test process has a harness and an owner above it");
        let spawn = || std::process::Command::new("sleep").arg("30").spawn().unwrap();
        let mut hook = spawn();
        let mut sibling = spawn();
        let load = |id: &str| {
            load_stage::<SessionsFile>(&sessions_path())
                .unwrap()
                .sessions
                .into_iter()
                .find(|r| r.session_id == id)
                .unwrap()
        };
        let bytes = || std::fs::read(sessions_path()).unwrap();

        // The old run's chain, from its harness: the harness (DEAD+2) is dead, the owner
        // and above are live.
        let stored: Vec<i32> = [DEAD + 2]
            .into_iter()
            .chain(a[2..].iter().copied())
            .take(8)
            .collect();
        do_hook_session_start(aoide_protocol::Door::Cli, "h", Some("claude"), Some("/w"), Some("0xW"), None, None, &stored);
        let sibling_chain: Vec<i32> = [sibling.id() as i32]
            .into_iter()
            .chain(a[2..].iter().copied())
            .take(8)
            .collect();
        let stale = load("h");
        assert!(
            superseded_by(&stale, &sibling_chain, None, pid_is_alive),
            "unhealed, a same-depth sibling retires the record"
        );

        // The first per-turn hook after the stale chain: its ancestry is [hook, this
        // process, the owner, …], and this process is its harness, so its chain is the
        // whole of `a`.
        let hook_pid = hook.id() as i32;
        let live_chain: Vec<i32> = pid_ancestry(hook_pid).into_iter().skip(1).take(8).collect();
        assert_eq!(live_chain, a.iter().copied().take(8).collect::<Vec<i32>>());
        ensure_session_chain("h", Some(hook_pid));
        assert_eq!(load("h").hook_ancestry, live_chain, "the live chain is recorded");
        assert_eq!(load("h").window_address, "0xW", "the window is untouched");

        // The hook exits. The live harness now stands in the record's own chain.
        let _ = hook.kill();
        let _ = hook.wait();
        do_hook_session_start(aoide_protocol::Door::Cli,
            "sib", Some("claude"), Some("/w"), Some("0xW"), None, None, &sibling_chain,
        );
        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert!(
            s.sessions.iter().any(|r| r.session_id == "h" && r.state != "done"),
            "the healed record survives the same-depth sibling"
        );

        // A later turn from a new hook process, same harness: the live chain is not rewritten.
        let mut next_hook = spawn();
        let before = bytes();
        ensure_session_chain("h", Some(next_hook.id() as i32));
        assert!(bytes() == before, "a live stored chain is never rewritten per turn");

        for c in [&mut sibling, &mut next_hook] {
            let _ = c.kill();
            let _ = c.wait();
        }
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// A chain with only dead pids (the records a harness left behind on an earlier
    /// boot) heals on the next per-turn hook, from the live harness that fired it.
    #[cfg(unix)]
    #[test]
    fn an_all_dead_stored_chain_heals_on_the_next_per_turn_hook() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("all-dead-heal");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        const DEAD: i32 = 2_000_000_000;
        let me = std::process::id() as i32;
        let a = pid_ancestry(me);
        assert!(a.len() >= 3, "the test process has a harness and an owner above it");
        do_hook_session_start(aoide_protocol::Door::Cli,
            "old", Some("claude"), Some("/w"), Some("0xW"), None, None,
            &[DEAD, DEAD + 1, DEAD + 2],
        );
        let mut hook = std::process::Command::new("sleep").arg("30").spawn().unwrap();
        let hook_pid = hook.id() as i32;
        ensure_session_chain("old", Some(hook_pid));
        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        let rec = s.sessions.iter().find(|r| r.session_id == "old").unwrap();
        assert_eq!(
            rec.hook_ancestry,
            pid_ancestry(hook_pid).into_iter().skip(1).take(8).collect::<Vec<i32>>(),
            "the dead chain is replaced by the live hook's harness chain"
        );

        let _ = hook.kill();
        let _ = hook.wait();
        let mut sibling = std::process::Command::new("sleep").arg("30").spawn().unwrap();
        let sibling_chain: Vec<i32> = [sibling.id() as i32]
            .into_iter()
            .chain(a[2..].iter().copied())
            .take(8)
            .collect();
        do_hook_session_start(aoide_protocol::Door::Cli,
            "sib", Some("claude"), Some("/w"), Some("0xW"), None, None, &sibling_chain,
        );
        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert!(
            s.sessions.iter().any(|r| r.session_id == "old" && r.state != "done"),
            "the healed record survives the same-depth sibling"
        );
        let _ = sibling.kill();
        let _ = sibling.wait();
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// The heal needs no compositor. With `HYPRLAND_INSTANCE_SIGNATURE` unset (a daemon
    /// on a host without Hyprland), a stale stored chain still takes the live harness's
    /// chain on the next per-turn hook, so a same-depth sibling no longer retires the
    /// record, and a later turn leaves the healed chain alone.
    #[cfg(unix)]
    #[test]
    fn a_stale_chain_heals_without_a_compositor() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("heal-without-compositor");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        const DEAD: i32 = 2_000_000_000;
        let me = std::process::id() as i32;
        let a = pid_ancestry(me);
        assert!(a.len() >= 3, "the test process has a harness and an owner above it");
        let spawn = || std::process::Command::new("sleep").arg("30").spawn().unwrap();
        let mut hook = spawn();
        let mut sibling = spawn();
        let mut next_hook = spawn();
        let stored: Vec<i32> = [DEAD + 2]
            .into_iter()
            .chain(a[2..].iter().copied())
            .take(8)
            .collect();
        do_hook_session_start(aoide_protocol::Door::Cli, "h", Some("claude"), Some("/w"), Some("0xW"), None, None, &stored);
        let sibling_chain: Vec<i32> = [sibling.id() as i32]
            .into_iter()
            .chain(a[2..].iter().copied())
            .take(8)
            .collect();
        let record = || {
            load_stage::<SessionsFile>(&sessions_path())
                .unwrap()
                .sessions
                .into_iter()
                .find(|r| r.session_id == "h")
                .unwrap()
        };
        assert!(
            superseded_by(&record(), &sibling_chain, None, pid_is_alive),
            "unhealed, a same-depth sibling retires the record"
        );

        let hook_pid = hook.id() as i32;
        ensure_session_chain("h", Some(hook_pid));
        let live_chain: Vec<i32> = pid_ancestry(hook_pid).into_iter().skip(1).take(8).collect();
        assert_eq!(record().hook_ancestry, live_chain, "the live chain is recorded");
        assert_eq!(
            record().window_address,
            "0xW",
            "no compositor, so no discovery: the window is untouched"
        );

        let _ = hook.kill();
        let _ = hook.wait();
        do_hook_session_start(aoide_protocol::Door::Cli,
            "sib", Some("claude"), Some("/w"), Some("0xW"), None, None, &sibling_chain,
        );
        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert!(
            s.sessions.iter().any(|r| r.session_id == "h" && r.state != "done"),
            "the healed record survives the same-depth sibling"
        );

        let before = std::fs::read(sessions_path()).unwrap();
        ensure_session_chain("h", Some(next_hook.id() as i32));
        assert!(
            std::fs::read(sessions_path()).unwrap() == before,
            "a live stored chain is never rewritten per turn"
        );

        for c in [&mut sibling, &mut next_hook] {
            let _ = c.kill();
            let _ = c.wait();
        }
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// A record anchored to a live pid of its own, one the new chain does not hold,
    /// keeps its stored chain across a per-turn hook: the chain move is refused.
    #[cfg(unix)]
    #[test]
    fn a_per_turn_hook_never_heals_over_a_live_pid_of_the_records_own() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("anchored-no-heal");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        const DEAD: i32 = 2_000_000_000;
        let me = std::process::id() as i32;
        let a = pid_ancestry(me);
        assert!(a.len() >= 3, "the test process has a harness and an owner above it");
        let mut anchor = std::process::Command::new("sleep").arg("30").spawn().unwrap();
        let mut hook = std::process::Command::new("sleep").arg("30").spawn().unwrap();
        let stored: Vec<i32> = [DEAD, DEAD + 1, DEAD + 2]
            .into_iter()
            .chain(a[2..].iter().copied())
            .take(8)
            .collect();
        do_hook_session_start(aoide_protocol::Door::Cli,
            "anchored", Some("claude"), Some("/w"), Some("0xW"), None, Some(anchor.id()),
            &stored,
        );
        let before = std::fs::read(sessions_path()).unwrap();
        ensure_session_chain("anchored", Some(hook.id() as i32));
        assert!(
            std::fs::read(sessions_path()).unwrap() == before,
            "a live pid of the record's own holds its chain against a per-turn heal"
        );

        for c in [&mut anchor, &mut hook] {
            let _ = c.kill();
            let _ = c.wait();
        }
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn ancestry_parent_finds_the_deepest_matching_live_agent() {
        // No live agent's `hookAncestry` intersects THIS process's own real
        // ancestry — no match.
        assert_eq!(ancestry_parent(&[]), None);

        let mine = pid_ancestry(std::process::id() as i32);
        assert!(mine.len() >= 3, "need at least self+parent+grandparent to test depth ordering");

        // A distant ancestor (grandparent) matches — found.
        let mut distant = session("distant", "/w", "working", "1", None);
        distant.kind = Some("agent".into());
        distant.hook_ancestry = vec![mine[2]];
        assert_eq!(ancestry_parent(&[distant.clone()]), Some("distant".to_string()));

        // A CLOSER ancestor (immediate parent) also matches, on a DIFFERENT
        // live agent — the closer one wins (deepest/closest match), whatever
        // order the sessions list carries them in.
        let mut close = session("close", "/w", "working", "2", None);
        close.kind = Some("agent".into());
        close.hook_ancestry = vec![mine[1]];
        assert_eq!(
            ancestry_parent(&[distant.clone(), close.clone()]),
            Some("close".to_string()),
            "the nearer ancestor's own live agent wins over a more distant match"
        );

        // A `done` agent is never a candidate, even with a matching ancestry.
        let mut dead = close.clone();
        dead.session_id = "dead".into();
        dead.state = "done".into();
        assert_eq!(ancestry_parent(&[dead]), None);

        // A conducted wrap (`is_agent_kind` refuses it) is never a candidate
        // either, even with a matching ancestry and a published "agent" kind.
        let mut wrap = close.clone();
        wrap.session_id = "wrap".into();
        wrap.conductable = Some(true);
        assert_eq!(ancestry_parent(&[wrap]), None);
    }

    #[test]
    fn a_spawner_is_attested_only_when_the_kernel_backs_it() {
        let mine = pid_ancestry(std::process::id() as i32);
        assert!(mine.len() >= 3);

        // A conducted wrap whose pid really is above this process.
        let mut wrap = session("wrap", "/w", "working", "1", None);
        wrap.conductable = Some(true);
        wrap.pid = Some(mine[1] as u32);
        assert!(spawner_is_attested("wrap", "me", &[wrap.clone()]));

        // The same claim naming a pid that is not above this process, however
        // honest the caller sounds. A registrant reparented to init before it
        // registered has no spawner above it at all: this same shape, silent.
        let mut elsewhere = wrap.clone();
        elsewhere.session_id = "elsewhere".into();
        elsewhere.pid = Some(i32::MAX as u32);
        assert!(!spawner_is_attested("elsewhere", "me", &[elsewhere]));

        // A hook-registered agent confirmed by its `hookAncestry`.
        let mut agent = session("agent", "/w", "working", "2", None);
        agent.kind = Some("agent".into());
        agent.hook_ancestry = vec![mine[2]];
        assert!(spawner_is_attested("agent", "me", &[agent.clone()]));
        agent.hook_ancestry = vec![i32::MAX];
        assert!(!spawner_is_attested("agent", "me", &[agent]));

        // Never itself, never a record that has ended, never one that is absent.
        assert!(!spawner_is_attested("wrap", "wrap", &[wrap.clone()]));
        let mut done = wrap.clone();
        done.state = "done".into();
        assert!(!spawner_is_attested("wrap", "me", &[done]));
        assert!(!spawner_is_attested("ghost", "me", &[wrap]));
    }

    #[test]
    fn resolve_registration_parent_precedence_explicit_then_ancestry_then_env() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_SESSION_ID"]);

        // Tier 1: an explicit `--parent` wins outright, even with a matching
        // ancestry candidate ALSO present.
        let mine = pid_ancestry(std::process::id() as i32);
        let mut ancestry_match = session("ancestry-match", "/w", "working", "1", None);
        ancestry_match.kind = Some("agent".into());
        ancestry_match.hook_ancestry = vec![mine[1]];
        std::env::set_var("AOIDE_SESSION_ID", "env-sid");
        assert_eq!(
            resolve_registration_parent(Some("explicit"), "new-id", &[ancestry_match.clone()]),
            Some("explicit".to_string())
        );

        // Tier 2: no explicit flag, but the ancestry walk matches — wins over
        // the env fallback even though both are present.
        assert_eq!(
            resolve_registration_parent(None, "new-id", &[ancestry_match]),
            Some("ancestry-match".to_string())
        );

        // Tier 3: no explicit flag, no ancestry match — the ambient env,
        // guarded against naming the record's OWN fresh id AND (S-B) required
        // to name a still-live record; the fixture supplies that live record
        // so this precedence test keeps proving tier 3 fires at all — the
        // refusal shapes get their own tests below.
        let env_sid_live = session("env-sid", "/w", "working", "3", None);
        assert_eq!(
            resolve_registration_parent(None, "new-id", &[env_sid_live]),
            Some("env-sid".to_string())
        );
        std::env::set_var("AOIDE_SESSION_ID", "new-id");
        assert_eq!(
            resolve_registration_parent(None, "new-id", &[]),
            None,
            "the env value must never self-parent the fresh id"
        );
        std::env::remove_var("AOIDE_SESSION_ID");
        assert_eq!(resolve_registration_parent(None, "new-id", &[]), None);
    }

    #[test]
    fn the_ambient_parent_fallback_refuses_a_session_id_with_no_live_record() {
        // S-B: an `AOIDE_SESSION_ID` naming NO record at all (a leaked or
        // stale id from some other door/host) must never become a parent —
        // there is nothing to check liveness against, so it is refused, not
        // trusted on the strength of the env var alone.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_SESSION_ID"]);
        std::env::set_var("AOIDE_SESSION_ID", "ghost-sid");
        assert_eq!(resolve_registration_parent(None, "new-id", &[]), None);
    }

    #[test]
    fn the_ambient_parent_fallback_refuses_an_ended_session_id() {
        // S-B: the ambient id resolves to a REAL record, but that record has
        // already ended (`state: "done"`) — still refused. This is the
        // belt-and-braces half of the Osaka fix: a stale id from ANY door
        // must not resurrect a dead session as a live parent.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_SESSION_ID"]);
        std::env::set_var("AOIDE_SESSION_ID", "ended-sid");
        let mut ended = session("ended-sid", "/w", "working", "9", None);
        ended.state = "done".into();
        assert_eq!(resolve_registration_parent(None, "new-id", &[ended]), None);
    }

    #[test]
    fn the_ambient_parent_fallback_still_accepts_a_live_local_session() {
        // S-B's positive case: an ambient id naming a genuinely LIVE local
        // record still resolves as a tier-3 parent — the fix narrows tier 3,
        // it doesn't disable it.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_SESSION_ID"]);
        std::env::set_var("AOIDE_SESSION_ID", "live-sid");
        let live = session("live-sid", "/w", "working", "9", None);
        assert_eq!(
            resolve_registration_parent(None, "new-id", &[live]),
            Some("live-sid".to_string())
        );
    }

    /// One window per session: the first attach fills the empty window, the same window again
    /// writes nothing, another window is refused, and an unknown session is refused.
    #[test]
    fn attach_fills_an_empty_window_and_refuses_a_second_one() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR"]);
        let stage = unique_stage("attach-fills");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        let file = SessionsFile {
            sessions: vec![SessionRecord { window_address: String::new(), ..session("term", "/w", "working", "1", None) }],
            ..Default::default()
        };
        write_stage(&sessions_path(), &file).unwrap();

        assert!(matches!(
            attach_window("term", "0xA", None),
            Attach::Bound { changed: true, workspace: None }
        ));
        assert!(matches!(
            attach_window("term", "0xA", None),
            Attach::Bound { changed: false, .. }
        ));
        assert!(matches!(
            attach_window("term", "0xB", None),
            Attach::Held { ref window } if window == "0xA"
        ));
        assert!(matches!(attach_window("ghost", "0xA", None), Attach::NoSession));

        let s: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert_eq!(s.sessions[0].window_address, "0xA");
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// The workspace follows the window's report: a move updates the id, and the same id again
    /// changes nothing.
    #[test]
    fn attach_moves_the_workspace_and_only_a_real_move_writes() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STAGE_DIR"]);
        let stage = unique_stage("attach-moves");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        let file = SessionsFile {
            sessions: vec![SessionRecord { window_address: String::new(), ..session("term", "/w", "working", "1", None) }],
            ..Default::default()
        };
        write_stage(&sessions_path(), &file).unwrap();

        assert!(matches!(
            attach_window("term", "0xA", Some(3)),
            Attach::Bound { changed: true, workspace: Some(3) }
        ));
        assert!(matches!(
            attach_window("term", "0xA", Some(3)),
            Attach::Bound { changed: false, workspace: Some(3) }
        ));
        assert!(matches!(
            attach_window("term", "0xA", Some(5)),
            Attach::Bound { changed: true, workspace: Some(5) }
        ));
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// The session hook, the reaper and the window attach run with no Hyprland environment at
    /// all: no signature, no request socket under the runtime dir, and a `hyprctl` first on the
    /// search path that leaves a marker when run. The marker must never appear.
    #[cfg(unix)]
    #[test]
    fn hook_reap_and_attach_run_with_no_hyprland_environment() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&[
            "AOIDE_STAGE_DIR",
            "HYPRLAND_INSTANCE_SIGNATURE",
            "XDG_RUNTIME_DIR",
            "PATH",
        ]);
        let stage = unique_stage("no-hyprland-env");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("XDG_RUNTIME_DIR", &stage);
        std::env::remove_var("HYPRLAND_INSTANCE_SIGNATURE");
        let shim_dir = stage.join("shim");
        std::fs::create_dir_all(&shim_dir).unwrap();
        let marker = stage.join("hyprctl-ran");
        let shim = shim_dir.join("hyprctl");
        std::fs::write(&shim, format!("#!/bin/sh\n: > '{}'\necho '[]'\n", marker.display())).unwrap();
        std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
        let path = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", format!("{}:{path}", shim_dir.display()));

        let me = std::process::id() as i32;
        assert!(hook_chain(me).is_some(), "the walk finds a harness with no compositor");

        const DEAD: i32 = 2_000_000_000;
        let mut rec = session("hooked", "/w", "working", "1", None);
        rec.window_address.clear();
        rec.hook_ancestry = vec![DEAD, DEAD + 1];
        let file = SessionsFile {
            sessions: vec![rec],
            ..Default::default()
        };
        write_stage(&sessions_path(), &file).unwrap();
        ensure_session_chain("hooked", Some(me));
        let healed: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert_eq!(healed.sessions[0].hook_ancestry, hook_chain(me).unwrap());

        crate::reap::reap(&invocation(&["session", "reap"], &[]));
        let kept: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert!(
            kept.sessions.iter().any(|s| s.session_id == "hooked"),
            "a live harness keeps its record through the reap"
        );

        assert!(matches!(
            attach_window("hooked", "0xW", Some(3)),
            Attach::Bound { changed: true, workspace: Some(3) }
        ));
        let bound: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert_eq!(bound.sessions[0].window_address, "0xW");
        assert!(!marker.exists(), "the core ran hyprctl: it must read no compositor");
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn a_bare_terminal_born_on_a_bound_workspace_joins_its_project() {
        // The THIRD stamp site: the synthetic `win:` record for a bare tty has
        // no agent, no project and no birth of its own — but it is a session on
        // the workspace, so a bound workspace carries it exactly as it carries
        // an agent's.
        let projects = vec![
            Project { name: "aoide".into(), workspaces: vec![3], ..Default::default() },
            Project { name: "cadenza".into(), workspaces: vec![7], ..Default::default() },
        ];
        let mut w = term_win("0xAABB", "kitty", "/home/khoa");
        w.workspace = Some(3);
        let (out, changed) = reconcile_untracked_terminals(vec![], &[w.clone()], &projects);
        assert!(changed);
        assert_eq!(out[0].session_id, "win:aabb");
        assert_eq!(out[0].workspace, Some(3));
        assert_eq!(
            out[0].workspace_project.as_deref(),
            Some("aoide"),
            "a bare terminal on a bound workspace joins its project"
        );

        // A re-scan after the window is DRAGGED: the id follows, the birth
        // default does not.
        let mut moved = w.clone();
        moved.workspace = Some(7);
        let (out, changed) = reconcile_untracked_terminals(out, &[moved], &projects);
        assert!(changed);
        assert_eq!(out[0].workspace, Some(7));
        assert_eq!(out[0].workspace_project.as_deref(), Some("aoide"));

        // Unbound, and with no projects at all: observed and unstamped.
        let mut unbound = w.clone();
        unbound.workspace = Some(99);
        let (out, _) = reconcile_untracked_terminals(
            vec![SessionRecord {
                session_id: "win:aabb".into(),
                workspace: None,
                ..Default::default()
            }],
            &[unbound],
            &[],
        );
        assert_eq!(out[0].workspace, Some(99));
        assert_eq!(out[0].workspace_project, None);
    }

    use aoide_protocol::output::Status;

    /// One window-verb test's world: a private stage, codex home and runtime dir, the env pointing at
    /// them, and the env lock held. Fields drop in order, so the env is restored before the lock goes.
    struct WindowWorld {
        _env: EnvVars,
        _xdg: EnvVars,
        stage: std::path::PathBuf,
        _guard: std::sync::MutexGuard<'static, ()>,
    }

    impl Drop for WindowWorld {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.stage);
        }
    }

    fn window_world(tag: &str) -> WindowWorld {
        let guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let env = EnvVars::save(&["AOIDE_STAGE_DIR", "CODEX_HOME", "AOIDE_DAEMON_SOCKET"]);
        let stage = unique_stage(tag);
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("CODEX_HOME", unique_stage(&format!("{tag}-codex")));
        std::env::set_var("AOIDE_DAEMON_SOCKET", stage.join("no-aoided.sock"));
        let xdg = isolated_xdg_runtime(&format!("{tag}-xdg"));
        WindowWorld { _env: env, _xdg: xdg, stage, _guard: guard }
    }

    /// A session that ran in `window` with `pid`, started now: the fixture the window verbs judge.
    fn ran_in(id: &str, window: &str, pid: Option<u32>) -> SessionRecord {
        SessionRecord {
            window_address: window.into(),
            pid,
            ..session(id, "/w", "working", &crate::graph::now_iso_utc(), None)
        }
    }

    /// `session show` answers one local session's window and workspace through the door, and
    /// refuses an id no record carries as `session-not-found`: the two facts the desktop's focus verb
    /// reads, and nothing else of the roster.
    #[test]
    fn session_show_answers_a_local_session_and_refuses_an_unknown_one() {
        let _world = window_world("session-show");
        let mut rec = ran_in("show-me", "0x55d0c2a1", None);
        rec.workspace = Some(3);
        write_stage(
            &sessions_path(),
            &SessionsFile { schema_version: "0".into(), sessions: vec![rec] },
        )
        .unwrap();

        let found = session_show(&invocation(&["session", "show"], &["show-me"]));
        assert_eq!(found.status, aoide_protocol::output::Status::Ok, "{}", found.message);
        let data = found.data.expect("the answer carries its data");
        assert_eq!(data["sessionId"], "show-me");
        assert_eq!(data["windowAddress"], "0x55d0c2a1");
        assert_eq!(data["workspace"], 3);

        let missing = session_show(&invocation(&["session", "show"], &["nobody"]));
        assert_eq!(missing.status, aoide_protocol::output::Status::Error);
        assert_eq!(missing.data.expect("a refusal names its reason")["reason"], "session-not-found");
    }

    /// The roster verb through the door, its stdin payload on the flag the CLI leg forwards.
    fn roster_door(payload: Option<&str>) -> Invocation {
        let mut inv = invocation(&["session", "window", "roster"], &[]);
        inv.door = Door::Daemon;
        if let Some(text) = payload {
            inv.flags.insert(STDIN_PAYLOAD_FLAG.to_string(), text.to_string());
        }
        inv
    }

    /// The closed window ends what ran in it and nothing in another window: the pid of the first is
    /// dead, the second's pid is alive yet it still ends, because a terminal outlives its agent.
    #[test]
    fn a_closed_window_ends_the_sessions_that_ran_in_it_and_spares_another_window() {
        let world = window_world("window-closed-ends");
        let me = std::process::id();
        write_stage(
            &sessions_path(),
            &SessionsFile {
                schema_version: "0".into(),
                sessions: vec![
                    ran_in("gone", "0xW", Some(reaped_pid())),
                    ran_in("pid-alive", "0xW", Some(me)),
                    ran_in("elsewhere", "0xV", Some(me)),
                ],
            },
        )
        .unwrap();

        let inv = invocation(&["session", "window", "closed"], &["0xW"]);
        let out = crate::reap::reap_closed_window(&inv, "0xW");
        assert_eq!(out.status, Status::Ok);
        assert_eq!(out.data.as_ref().unwrap()["windowEnded"], json!(["gone", "pid-alive"]));
        let after: SessionsFile = load_stage(&sessions_path()).unwrap();
        let ids: Vec<&str> = after.sessions.iter().map(|s| s.session_id.as_str()).collect();
        assert_eq!(ids, ["elsewhere"], "the other window's session is untouched");
        assert_eq!(after.sessions[0].window_address, "0xV");
        drop(world);
    }

    /// A live harness anchors its session through the closed window: the record survives, and its
    /// claim on the window is released so no record points at a window that is gone.
    #[test]
    fn a_closed_window_spares_a_harness_anchored_session_and_releases_its_claim() {
        let world = window_world("window-closed-anchored");
        let mut anchored = ran_in("anchored", "0xW", None);
        anchored.hook_ancestry = vec![std::process::id() as i32];
        write_stage(&sessions_path(), &SessionsFile { schema_version: "0".into(), sessions: vec![anchored] }).unwrap();

        let inv = invocation(&["session", "window", "closed"], &["0xW"]);
        let out = crate::reap::reap_closed_window(&inv, "0xW");
        let data = out.data.as_ref().unwrap();
        assert_eq!(data["windowEnded"], json!([]));
        assert_eq!(data["windowReleased"], json!(["anchored"]));
        let after: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert_eq!(after.sessions.len(), 1, "a live harness keeps its record");
        assert!(after.sessions[0].window_address.is_empty(), "and lets go of the closed window");
        drop(world);
    }

    /// The verb refuses a missing or blank address before anything is reaped, and without aoided it
    /// says so rather than reaping in-process.
    #[test]
    fn the_closed_window_verb_refuses_a_missing_address_and_needs_aoided() {
        let world = window_world("window-closed-verb");
        let bare = session_window_closed(&invocation(&["session", "window", "closed"], &[]));
        assert_eq!(bare.status, Status::Usage);
        let blank = session_window_closed(&invocation(&["session", "window", "closed"], &["  "]));
        assert_eq!(blank.status, Status::Usage);
        let no_daemon = session_window_closed(&invocation(&["session", "window", "closed"], &["0xW"]));
        assert_eq!(no_daemon.status, Status::Error);
        assert!(no_daemon.message.contains("aoided must be running"), "{}", no_daemon.message);
        drop(world);
    }

    /// The roster publishes a terminal's `win:*` record once, is idempotent, and keeps that record
    /// through a reap while its window is open (its live pid is the proof). A payload that is not a
    /// roster is refused.
    #[test]
    fn the_roster_verb_publishes_a_terminal_once_and_refuses_what_is_not_a_roster() {
        let world = window_world("window-roster");
        write_stage(&sessions_path(), &SessionsFile { schema_version: "0".into(), sessions: vec![] }).unwrap();
        let window = TermWindow {
            address: "0xROSTER".into(),
            class: "kitty".into(),
            title: String::new(),
            workspace: Some(1),
            pid: Some(std::process::id() as i32),
            mapped: true,
            cwd: "/w".into(),
        };
        let payload = serde_json::to_string(&vec![window]).unwrap();

        let first = session_window_roster(&roster_door(Some(&payload)));
        assert_eq!(first.status, Status::Ok);
        assert_eq!(first.data.as_ref().unwrap()["changed"], json!(true));
        let after: SessionsFile = load_stage(&sessions_path()).unwrap();
        let term = after
            .sessions
            .iter()
            .find(|s| s.session_id == "win:roster")
            .expect("the terminal gets a win record");
        assert_eq!(term.window_address, "0xROSTER");

        let again = session_window_roster(&roster_door(Some(&payload)));
        assert_eq!(again.data.as_ref().unwrap()["changed"], json!(false), "the same roster changes nothing");
        assert!(again.changed.is_empty());

        crate::reap::reap(&invocation(&["session", "reap"], &[]));
        let kept: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert!(kept.sessions.iter().any(|s| s.session_id == "win:roster"), "its window is open: the reap keeps it");

        for bad in [None, Some("not json"), Some("{\"not\": \"an array\"}")] {
            assert_eq!(session_window_roster(&roster_door(bad)).status, Status::Usage);
        }
        drop(world);
    }

    /// The attach verb through the door binds an empty window once, writes nothing for the same
    /// window and workspace again, writes a real workspace move, and refuses a second window.
    #[test]
    fn the_attach_verb_binds_a_window_once_and_writes_only_a_real_workspace_move() {
        let world = window_world("window-attach-verb");
        let term = SessionRecord { window_address: String::new(), ..session("term", "/w", "working", "1", None) };
        write_stage(&sessions_path(), &SessionsFile { schema_version: "0".into(), sessions: vec![term] }).unwrap();
        let attach = |window: &str, workspace: Option<&str>| {
            let mut inv = invocation(&["session", "window", "attach"], &["term", window]);
            inv.door = Door::Daemon;
            if let Some(ws) = workspace {
                inv.flags.insert("workspace".into(), ws.into());
            }
            session_window_attach(&inv)
        };

        let bound = attach("0xA", Some("3"));
        assert_eq!(bound.status, Status::Ok);
        assert_eq!(bound.data.as_ref().unwrap()["changed"], json!(true));
        assert_eq!(bound.data.as_ref().unwrap()["workspace"], json!(3));
        let same = attach("0xA", Some("3"));
        assert_eq!(same.data.as_ref().unwrap()["changed"], json!(false));
        let moved = attach("0xA", Some("5"));
        assert_eq!(moved.data.as_ref().unwrap()["changed"], json!(true));
        assert_eq!(moved.data.as_ref().unwrap()["workspace"], json!(5));
        let held = attach("0xB", None);
        assert_eq!(held.status, Status::Error);
        assert_eq!(held.data.as_ref().unwrap()["reason"], json!("window-held"));
        assert_eq!(attach("0xA", Some("x")).status, Status::Usage);
        drop(world);
    }

    /// The codex verb with no desktop threads on disk changes nothing and enrolls no thread; without
    /// aoided it says so.
    #[test]
    fn the_codex_sync_verb_is_quiet_when_the_desktop_has_no_threads() {
        let world = window_world("window-codex-quiet");
        write_stage(&sessions_path(), &SessionsFile { schema_version: "0".into(), sessions: vec![] }).unwrap();
        let mut inv = invocation(&["session", "codex", "sync"], &[]);
        inv.door = Door::Daemon;
        let out = crate::graph::session_codex_sync(&inv);
        assert_eq!(out.status, Status::Ok);
        assert_eq!(out.data.as_ref().unwrap()["changed"], json!(false));
        assert!(out.changed.is_empty());
        let after: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert!(after.sessions.is_empty(), "no thread is enrolled");
        let cli = crate::graph::session_codex_sync(&invocation(&["session", "codex", "sync"], &[]));
        assert_eq!(cli.status, Status::Error);
        assert!(cli.message.contains("aoided must be running"), "{}", cli.message);
        drop(world);
    }
}
