//! `aoide secrets watch` — a foreground, line-mode terminal surface (design
//! doc: tracker #71 Part 1) that tail-follows the broker-owned events feed
//! (`socket::events_path` — P-G4, task #77, corrected from the mirrored
//! `~/Aoide/log` this module read through P-N3: the deployed broker unit
//! runs with `ProtectHome=true`, so its best-effort mirror into the
//! operator's home silently failed there, and this watcher saw ZERO event
//! lines, falling back to its 30s pending-reconcile tick for every popup —
//! found live on yomi-strix, 2026-08-23), narrates every broker event, and
//! — when stdin is a terminal and `--json` is absent — prompts inline for
//! each parked ask: print the approve command, dismiss it outright,
//! or ignore it (leaving it parked for any other terminal). This is one of
//! the seven I/O-carrying modules in the crate (`AGENTS.md`'s "I/O is
//! confined to seven named modules" invariant) — `broker`/`client`/`store`/
//! `backend`/`enroll` are the other five.
//!
//! **`--popup` (tracker #71 Part 2)** swaps the tty prompt for a zenity
//! `--info` notice on each parked ask. The notice names
//! `aoide secrets approve <id>`; it takes no code. The watch never reads a
//! code on either surface (ruling 98): a code is typed only at
//! `secrets approve`, the conductor's approve child, and the app's entry
//! window (ruling 87). `zenity` remains a runtime shell-out declared BY NAME
//! (zero new Cargo dependencies, the same feature-detection shape
//! `enroll::render_qr` uses for `qrencode`); [`run`]'s startup gate refuses
//! to enter `--popup` only when `zenity` is not available.
//!
//! Notices are UNLOCK-GATED ([`locked_state`]: `loginctl LockedHint` OR'd
//! with a `/proc` scan for a named locker process, default `hyprlock`;
//! `AOIDE_SECRETS_LOCKER` overrides it). The `/proc` half is load-bearing:
//! hyprlock cannot set `LockedHint`, so a shell lock holds notices too. A
//! notice waits while the screen is locked and is PARKED-ONLY:
//! `released`/`completed`/`dismissed`/`expired` never open a notice, only
//! narrate, so a tight automation loop never becomes a toast storm. If an
//! ask resolves elsewhere while its notice is open, [`run_zenity_notice`]
//! kills that EXACT child by the `std::process::Child` handle it holds and
//! narrates. `--popup`+`--json` is a usage error (`commands::
//! handle_secrets_watch`).
//!
//! **A notice failure must NEVER be silent.** A notice that cannot open
//! (zenity missing from the unit's `PATH`, say) must not leave a parked ask
//! invisible until its timeout. [`popup_loop`] narrates the failure on
//! stderr, names the ask id, and backs off ([`SPAWN_BACKOFF_INITIAL`]/
//! [`next_spawn_backoff`]). A failed ask is never added to `ignored`, so the
//! loop keeps re-offering it. **This loop, not the 30-second `reconcile`
//! tick, is what retries a failed notice** (`Queue::reconcile` syncs which
//! asks exist; it does not re-drive a dialog). Do not insert a failed ask
//! into `ignored` to stop the retries: that re-introduces the silent failure.
//!
//! **Why this crate, not a conductor pane.** Reaching the broker from
//! `aoide-conductor` would add a NEW `aoide-conductor` → `aoide-secrets`
//! dependency edge; `client::pending`/`approve`/`dismiss` are already right
//! here, and the code must never cross a process boundary as an argv token
//! (`client`'s own module doc). A conductor pane, or any future graphical
//! popup, stays a reasonable LATER addition as a consumer of this command's
//! `--json` stream — never a reason to duplicate this logic elsewhere.
//!
//! **Shape**: [`parse_notify_line`]/[`Queue`]/[`pick_next`]/
//! [`narrate_event`]/[`event_to_json`] are PURE —
//! clock-as-parameter throughout (this crate's own standing rule for
//! `totp`/`replay`; extended here to the fold, for the same testability
//! reason — `parse_notify_line` takes its own `ts` as a parameter for
//! exactly this reason, P-G4). [`Follower`] is the one piece of real file
//! I/O (tail-follow `AGENTS.md`'s events-feed discipline: open once, seek
//! to EOF, delta reads only, reopen-at-0 on a shrink — the events feed is
//! capped at 1 MiB and truncated back to empty in place rather than
//! rotated, `broker::append_events_feed`'s own doc, so this same
//! reopen-on-shrink branch is what makes that truncation transparent to a
//! live watcher). [`run`] wires both together behind two threads sharing
//! one `Queue` and one output lock, and is the only piece that touches a
//! socket, a terminal, or a signal.
//!
//! **The tail is a TRIGGER; `client::pending` is the AUTHORITY** (this
//! crate's own `AGENTS.md`, "P-N2's park lifecycle" precedent, extended
//! here): [`Queue::reconcile`] runs once at startup and again on every
//! event plus a 30s safety tick, so a watcher that started late, missed a
//! line, or is racing a broker restart still converges on the truth. An ask
//! `reconcile` discovers with no matching `parked` line has no
//! `timeoutSecs` to go on (the wire's own `pending` reply never carries
//! one) — it is given [`crate::park::park_timeout`] as an ESTIMATE
//! (`Ask::estimated`), rendered with a `~` prefix so the operator knows it
//! is a guess, not a fact from the broker.
//!
//! **Never displayed, ever: a secret value.** This module holds `id`,
//! secret *name*, consumer, timing — never a value. No code reaches
//! this module: a TOTP code is typed only at `secrets approve`, the
//! conductor's approve child, and the app's entry window (ruling 87).

use crate::client::{self, PendingAsk};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

// P-P5, F5: the dialog substrate (screen-lock probes, the spawn-retry
// backoff, the generic dialog-child run loop, and its outcome type) moved
// to `aoide_protocol::dialog` — a SECOND consumer (`aoide-client`'s own
// P-P5 popup arm) is what forces the extraction (that module's own doc).
// Every external spelling at THIS path stays byte-identical: `pub use` for
// what was already `pub` here (`ZenityResult`/`locked_state`), a bare
// `use` for what was crate-private (preserving the exact same restricted
// visibility, never widening it just because the shim needs an import).
pub use aoide_protocol::dialog::DialogResult as ZenityResult;
pub use aoide_protocol::dialog::locked_state;
use aoide_protocol::dialog::{
    is_locked, locker_process_name, next_spawn_backoff, run_entry_dialog, sleep_backoff_interruptible,
    zenity_available, DISMISS_LABEL, SPAWN_BACKOFF_INITIAL, SPAWN_BACKOFF_MAX,
};
use aoide_protocol::untrusted::{clean_line, escape_for_log};

// ── P1: the pure fold ───────────────────────────────────────────────────

/// One broker notification, parsed from an events-feed line — the exact
/// five shapes `broker::emit_notify` writes (crate `README.md`'s "Broker
/// notifications"). `ts` is the instant this line was READ (P-G4: the
/// events feed carries no per-line timestamp of its own — see
/// `parse_notify_line`'s own doc), reused as `requestedAt` for a
/// freshly-seen `Parked` ask — the notify fires right at park time and is
/// seen within one poll tick, so the two remain the same instant in
/// practice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Released { secret: String, consumer: String, ts: u64 },
    /// A step's wrong-code budget is spent (ruling 44). It names no ask, so it
    /// carries no secret or consumer: it is about the step, not one request.
    BudgetSpent { ts: u64 },
    /// `reason`/`origin` are additive (P3): the wire's optional,
    /// self-asserted/best-effort context block a popup/prompt surface shows
    /// alongside the ask (`park::ParkedAsk::reason`/`park::AskOrigin`'s own
    /// docs) — `None`/default when the broker sent none.
    Parked {
        id: String,
        secret: String,
        consumer: String,
        timeout_secs: u64,
        ts: u64,
        reason: Option<String>,
        origin: client::PendingOrigin,
    },
    Completed { id: String, secret: String, consumer: String, ts: u64 },
    Dismissed { id: String, secret: String, consumer: String, ts: u64 },
    Expired { id: String, secret: String, consumer: String, ts: u64 },
}

impl Event {
    /// The parked-ask id this event names, if any (`Released` has none —
    /// it never carries an id, `broker.rs`'s own notify shape table).
    pub fn ask_id(&self) -> Option<&str> {
        match self {
            Event::Released { .. } | Event::BudgetSpent { .. } => None,
            Event::Parked { id, .. }
            | Event::Completed { id, .. }
            | Event::Dismissed { id, .. }
            | Event::Expired { id, .. } => Some(id),
        }
    }
}

/// Parse ONE events-feed line into an [`Event`] — pure, total, and never
/// panics. As of P-G4 (task #77) this reads the RAW payload
/// `broker::emit_notify` already writes verbatim to its own `audit.log`
/// (`append_own_log`) and now, identically, to the broker-owned events
/// feed (`append_events_feed`) — a bare `{"event": "<kind>", ...}` object,
/// no `AuditRecord` wrapper and no `message`-as-JSON-string indirection
/// (that wrapper shape only ever existed on the mirrored `~/Aoide/log`
/// side, which this module no longer reads — module doc). `ts` arrives as
/// a PARAMETER (this module's own clock-as-parameter discipline) rather
/// than being read from the line itself: none of `emit_notify`'s five
/// payload shapes carry a timestamp field (they never have, even before
/// this phase — this parser previously borrowed the OUTER `AuditRecord`'s
/// own `ts` for that purpose, which no longer exists on this path), so the
/// caller (the tail loop, `unix_now()` at the moment the line was read)
/// supplies it instead. A malformed line — bad JSON, an unrecognized
/// `event` kind, a missing required field — returns `None` rather than
/// erroring; the caller skips it and moves on, same posture as before.
pub fn parse_notify_line(line: &str, ts: u64) -> Option<Event> {
    let payload: Value = serde_json::from_str(line).ok()?;
    let kind = payload.get("event").and_then(Value::as_str)?;
    if kind == "totp-budget-spent" {
        return Some(Event::BudgetSpent { ts });
    }
    let secret = payload.get("secret").and_then(Value::as_str)?.to_string();
    let consumer = payload.get("consumer").and_then(Value::as_str)?.to_string();

    match kind {
        "released" => Some(Event::Released { secret, consumer, ts }),
        "parked" => {
            let id = payload.get("id").and_then(Value::as_str)?.to_string();
            let timeout_secs = payload.get("timeoutSecs").and_then(Value::as_u64)?;
            let reason = payload.get("reason").and_then(Value::as_str).map(str::to_string);
            let origin = parse_origin(payload.get("origin"));
            Some(Event::Parked { id, secret, consumer, timeout_secs, ts, reason, origin })
        }
        "completed" => {
            let id = payload.get("id").and_then(Value::as_str)?.to_string();
            Some(Event::Completed { id, secret, consumer, ts })
        }
        "dismissed" => {
            let id = payload.get("id").and_then(Value::as_str)?.to_string();
            Some(Event::Dismissed { id, secret, consumer, ts })
        }
        "expired" => {
            let id = payload.get("id").and_then(Value::as_str)?.to_string();
            Some(Event::Expired { id, secret, consumer, ts })
        }
        _ => None,
    }
}

/// Parse a `parked` event's/`pending` reply entry's `"origin"` object
/// (`broker::origin_to_json`'s exact shape) into a [`client::PendingOrigin`]
/// — `None`/missing at any level (an absent `origin` key, an old broker, a
/// field individually `null`) reads as that field's own default, never a
/// parse error (this module's own "malformed line -> skip, never error"
/// posture, extended to a field rather than the whole line).
fn parse_origin(origin: Option<&Value>) -> client::PendingOrigin {
    let Some(o) = origin else { return client::PendingOrigin::default() };
    client::PendingOrigin {
        username: o.get("username").and_then(Value::as_str).map(str::to_string),
        pid: o.get("pid").and_then(Value::as_i64),
        comm: o.get("comm").and_then(Value::as_str).map(str::to_string),
        hostname: o.get("hostname").and_then(Value::as_str).map(str::to_string),
    }
}

/// The `--json` mirror of `broker::origin_to_json` — same shape, this
/// module's own copy since it renders `client::PendingOrigin`, not
/// `park::AskOrigin` (two different crate-internal types over the identical
/// wire shape, `client::PendingOrigin`'s own doc on why).
fn origin_to_json(origin: &client::PendingOrigin) -> Value {
    json!({
        "username": origin.username,
        "pid": origin.pid,
        "comm": origin.comm,
        "hostname": origin.hostname,
    })
}

/// One parked ask as this surface tracks it — id/secret/consumer/timing
/// ONLY, never a value (same "never store a value" rule `park::ParkedAsk`
/// itself holds). `estimated` is true only when [`Queue::reconcile`]
/// invented `timeout_secs` from [`crate::park::park_timeout`] because no
/// `parked` line was ever seen for this ask (this module's own doc).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    pub id: String,
    pub secret: String,
    pub consumer: String,
    pub requested_at: u64,
    pub timeout_secs: u64,
    pub estimated: bool,
    /// Additive (P3) — see [`Event::Parked`]'s own doc.
    pub reason: Option<String>,
    pub origin: client::PendingOrigin,
}

impl Ask {
    pub fn expires_at(&self) -> u64 {
        self.requested_at.saturating_add(self.timeout_secs)
    }

    /// Seconds left until expiry, at `now` — negative once past due.
    pub fn remaining(&self, now: u64) -> i64 {
        self.expires_at() as i64 - now as i64
    }
}

/// The in-memory set of currently-parked asks this watcher knows about.
/// [`Queue::apply`] folds one [`Event`] in (a trigger); [`Queue::reconcile`]
/// replaces it with the truth from `client::pending` (the authority) — see
/// this module's own doc for why both exist.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Queue {
    asks: Vec<Ask>,
}

impl Queue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.asks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.asks.is_empty()
    }

    pub fn get(&self, id: &str) -> Option<&Ask> {
        self.asks.iter().find(|a| a.id == id)
    }

    pub fn remove(&mut self, id: &str) {
        self.asks.retain(|a| a.id != id);
    }

    pub fn snapshot(&self) -> &[Ask] {
        &self.asks
    }

    /// Fold one event into the queue. `Parked` inserts (a `parked` line
    /// seen twice for the same id, e.g. a replayed log tail, is a no-op —
    /// `apply` never duplicates an id); `Completed`/`Dismissed`/`Expired`
    /// remove by id (an id this queue never had — e.g. an `expired` for an
    /// ask this watcher started too late to see `parked` for and hasn't
    /// reconciled yet — is also a no-op, never a panic); `Released` never
    /// touches the queue at all (it carries no id — `Event::ask_id`).
    pub fn apply(&mut self, event: &Event) {
        match event {
            Event::Parked { id, secret, consumer, timeout_secs, ts, reason, origin } => {
                if !self.asks.iter().any(|a| &a.id == id) {
                    self.asks.push(Ask {
                        id: id.clone(),
                        secret: secret.clone(),
                        consumer: consumer.clone(),
                        requested_at: *ts,
                        timeout_secs: *timeout_secs,
                        estimated: false,
                        reason: reason.clone(),
                        origin: origin.clone(),
                    });
                }
            }
            Event::Completed { id, .. } | Event::Dismissed { id, .. } | Event::Expired { id, .. } => {
                self.remove(id);
            }
            Event::Released { .. } | Event::BudgetSpent { .. } => {}
        }
    }

    /// Reconcile against `client::pending`'s own list — THE authority
    /// (module doc). Drops any tracked ask no longer in `pending` (approved/
    /// dismissed/expired via a path this watcher's tail missed — e.g.
    /// completed from another terminal); adds any `pending` ask this queue
    /// doesn't know yet, with `default_timeout_secs` as an ESTIMATE
    /// (`Ask::estimated = true`) since the wire's `pending` reply carries
    /// no `timeoutSecs` field at all. An ask already tracked (a `parked`
    /// line was seen for it) is left exactly as it was — reconcile never
    /// downgrades a KNOWN timeout back into an estimate.
    ///
    /// The `pending` reply is raw, and the events feed is escaped
    /// ([`escape_for_log`]), so each text field is escaped here once, as the
    /// feed escapes it. An ask then reads the same whichever of the two
    /// reaches the queue first. The id stays as it is: the broker mints it, and
    /// every later event matches on it.
    pub fn reconcile(&mut self, pending: &[PendingAsk], default_timeout_secs: u64) {
        self.asks.retain(|a| pending.iter().any(|p| p.id == a.id));
        for p in pending {
            if !self.asks.iter().any(|a| a.id == p.id) {
                self.asks.push(Ask {
                    id: p.id.clone(),
                    secret: escape_for_log(&p.secret),
                    consumer: escape_for_log(&p.consumer),
                    requested_at: p.requested_at,
                    timeout_secs: default_timeout_secs,
                    estimated: true,
                    reason: p.reason.as_deref().map(escape_for_log),
                    origin: client::PendingOrigin {
                        username: p.origin.username.as_deref().map(escape_for_log),
                        pid: p.origin.pid,
                        comm: p.origin.comm.as_deref().map(escape_for_log),
                        hostname: p.origin.hostname.as_deref().map(escape_for_log),
                    },
                });
            }
        }
    }
}

/// Queue ordering — FIFO by `requested_at` (the ask closest to expiry
/// prompts first, design doc's "1.5 Prompt flow"), excluding any id in
/// `ignored` (the `[i]` gesture, this session only — the caller filters,
/// this queue never forgets the ask). Pure and testable with a plain
/// `Vec<Ask>`, no queue/session state needed.
pub fn pick_next<'a>(asks: &'a [Ask], ignored: &HashSet<String>) -> Option<&'a Ask> {
    asks.iter().filter(|a| !ignored.contains(&a.id)).min_by_key(|a| a.requested_at)
}

/// Which loop `run` drives, decided once from the three inputs that can
/// ever disagree — pure and total, so every combination (including the
/// `json_mode && popup_mode` one `commands::handle_secrets_watch` already
/// refuses as a usage error before `run` is ever called) has a defined
/// answer. `Json` wins over everything (the seam every future consumer
/// subscribes to, README's "Watching events"); `Popup` wins over tty
/// detection when `--json` is absent (design doc: "`--popup` with non-tty
/// stdin is fine — dialogs replace prompts", so a popup session never falls
/// back to narration-only just because stdin isn't a terminal).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Json,
    Popup,
    InteractiveTty,
    NarrationOnly,
}

pub fn select_mode(json_mode: bool, popup_mode: bool, stdin_tty: bool) -> Mode {
    if json_mode {
        Mode::Json
    } else if popup_mode {
        Mode::Popup
    } else if stdin_tty {
        Mode::InteractiveTty
    } else {
        Mode::NarrationOnly
    }
}

// ── narration + `--json` rendering (pure) ───────────────────────────────

fn hms(ts: u64) -> String {
    let s = ts % 86_400;
    format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}

fn mmss(remaining_secs: i64) -> String {
    let r = remaining_secs.max(0);
    format!("{}m{:02}s", r / 60, r % 60)
}

/// The "for: ..." context line (P3) — `None` when the ask carries no
/// reason, so a caller with nothing to show adds nothing (every surface
/// renders EXACTLY as before this phase when `reason` is absent, module
/// doc). The wrapping quotes/label live here, the ONE place, so the tty
/// prompt and zenity's `--text` show byte-identical wording.
fn format_reason_line(reason: &Option<String>) -> Option<String> {
    reason.as_deref().map(|r| format!("for: \"{r}\""))
}

/// The "from: ..." context line (P3) — degrades gracefully, field by field:
/// an entirely unidentified origin (every field `None`, `park::AskOrigin`'s
/// own doc on when that happens) renders NOTHING at all, never a bare
/// "from:" with nothing after it. `username` falls back to the literal
/// `unidentified` when a `comm`/`pid`/`hostname` piece IS known but the peer
/// itself wasn't (should not happen in practice — `peercred::peer_cred`
/// either resolves the whole `PeerCred` or none of it — kept anyway so this
/// function never assumes that invariant from outside). This is the ONE
/// place this wording is built — the provider's `secrets ask` receives the finished
/// string rather than re-deriving it, so every surface (the watch's tty
/// prompt, the popup notice, `approve`'s prompt) shows byte-identical text.
pub(crate) fn format_origin_line(origin: &client::PendingOrigin) -> Option<String> {
    if origin.username.is_none() && origin.pid.is_none() && origin.comm.is_none() && origin.hostname.is_none() {
        return None;
    }
    // Each field is cleaned as it is shown: the broker validates none of them
    // (comm is the process's own text, and the host and user are the machine's),
    // so an escape or a return in one would otherwise rewrite the line.
    let mut who = origin.username.as_deref().map_or_else(|| "unidentified".to_string(), clean_line);
    if let Some(comm) = &origin.comm {
        who.push_str(&format!(" \u{b7} {}", clean_line(comm)));
    }
    let mut line = format!("from: {who}");
    if let Some(pid) = origin.pid {
        line.push_str(&format!(" (pid {pid})"));
    }
    if let Some(host) = &origin.hostname {
        line.push_str(&format!(" @ {}", clean_line(host)));
    }
    Some(line)
}

/// Render one [`Event`] as a single narration line (interactive/piped
/// text mode) — never a value, ever (module doc).
pub fn narrate_event(event: &Event) -> String {
    match event {
        Event::Released { secret, consumer, ts } => {
            format!("  {}  released    {secret} \u{2192} {consumer}   (no code required)", hms(*ts))
        }
        Event::Parked { id, secret, consumer, timeout_secs, ts, .. } => {
            // `reason`/`origin` are shown on the PROMPT block, not this
            // terse one-line narration (`format_prompt_header`'s own doc) —
            // keeping the scrolling narration line unchanged in shape.
            format!(
                "  {}  parked      {secret} \u{2192} {consumer}   ask {id}   times out in {}   approve: aoide secrets approve {id}",
                hms(*ts),
                mmss(*timeout_secs as i64)
            )
        }
        Event::Completed { id, secret, consumer, ts } => {
            format!("  {}  completed   {secret} \u{2192} {consumer}   ask {id}", hms(*ts))
        }
        Event::Dismissed { id, secret, consumer, ts } => {
            format!("  {}  dismissed   {secret} \u{2192} {consumer}   ask {id}", hms(*ts))
        }
        Event::Expired { id, secret, consumer, ts } => {
            format!("  {}  expired     {secret} \u{2192} {consumer}   ask {id}   (park timed out)", hms(*ts))
        }
        Event::BudgetSpent { ts } => {
            format!("  {}  budget      a step's wrong-code budget is spent: checks are refused until the next {} s step", hms(*ts), crate::totp::STEP_SECONDS)
        }
    }
}

/// Render one [`Event`] as the `--json` line shape (crate `README.md`'s
/// "Watching events" section documents this as a contract surface — the
/// seam a future popup helper subscribes to instead of re-tailing the log
/// itself). One object per line, flushed per line by the caller.
pub fn event_to_json(event: &Event) -> Value {
    match event {
        Event::Released { secret, consumer, ts } => {
            json!({ "event": "released", "secret": secret, "consumer": consumer, "ts": ts })
        }
        Event::Parked { id, secret, consumer, timeout_secs, ts, reason, origin } => json!({
            "event": "parked",
            "id": id,
            "secret": secret,
            "consumer": consumer,
            "timeoutSecs": timeout_secs,
            "requestedAt": ts,
            "expiresAt": ts + timeout_secs,
            "ts": ts,
            "reason": reason,
            "origin": origin_to_json(origin),
        }),
        Event::Completed { id, secret, consumer, ts } => {
            json!({ "event": "completed", "id": id, "secret": secret, "consumer": consumer, "ts": ts })
        }
        Event::Dismissed { id, secret, consumer, ts } => {
            json!({ "event": "dismissed", "id": id, "secret": secret, "consumer": consumer, "ts": ts })
        }
        Event::Expired { id, secret, consumer, ts } => {
            json!({ "event": "expired", "id": id, "secret": secret, "consumer": consumer, "ts": ts })
        }
        Event::BudgetSpent { ts } => json!({ "event": "totp-budget-spent", "ts": ts }),
    }
}

// ── P2: the follower ─────────────────────────────────────────────────────

/// Tail-follows one file from EOF, delta-reads only — NEVER re-reads from
/// the start. **Moved to `aoide_protocol::feed::Follower` at P-D1**
/// (`docs/architecture/AOIDED.md`'s "L1 — the event bus" section): the
/// mechanics — open-at-end, `(dev, ino)` reopen detection across a broker
/// restart, `len() < pos` reopen on the events feed's own 1 MiB
/// truncate-in-place cap (`broker::append_events_feed`'s own doc), and
/// holding a partial trailing line across polls — are documented on
/// [`aoide_protocol::feed::Follower`] itself now; this re-export is the
/// shim discipline (`pkgs/aoide/crates/AGENTS.md`'s "no cross-crate
/// copying") that keeps every existing `watch::Follower` call site
/// (this module's own `tail_loop`/`wait_for_follower`, `tests/e2e.rs`)
/// unchanged.
pub use aoide_protocol::feed::Follower;

// ── `--popup`: lock probes + the zenity dialog ──────────────────────────
//
// The lock probes (`locker_process_name`/`probe_loginctl_locked`/
// `probe_locker_running`/`is_locked`/`locked_state`) live in
// `aoide_protocol::dialog` now (P-P5, F5) — shimmed back at these names via
// the `use`/`pub use` block near the top of this module.

/// The default `zenity` binary name `run` spawns in production — tests pass
/// a fake shim's own full path instead (this module's own test section),
/// never mutate `PATH` (unlike `enroll::render_qr`'s test, which predates
/// this pattern and still uses a `PATH` shim under `env_lock` — this
/// function exists so `--popup`'s own tests need neither).
const ZENITY_CMD: &str = "zenity";

// `DISMISS_LABEL`/`SPAWN_BACKOFF_INITIAL`/`SPAWN_BACKOFF_MAX`/
// `next_spawn_backoff`/`DIALOG_INFRA_FAILURE_EXIT`/`ZenityResult` (now
// `dialog::DialogResult`, shimmed back under the old name) all live in
// `aoide_protocol::dialog` now (P-P5, F5) — see this module's own shim
// import block near the top.

/// `--no-markup`: `--text` is built by
/// interpolating `secret`/`consumer`/`reason`/the origin line — all
/// UNTRUSTED display text (`ask.reason`/`ask.origin.comm` in particular,
/// this crate's own honesty note: `comm` is PROCESS-CONTROLLED, any process
/// can `prctl(PR_SET_NAME, ...)` itself to anything) — and zenity renders
/// `--text` as Pango markup BY DEFAULT, on `--entry` too (verified live on
/// this host, zenity 4.2.2: `--no-markup` is undocumented under `--help-
/// entry`'s own "Text entry options" section but IS accepted there and DOES
/// suppress markup — confirmed by screenshot, `<b>`/`&`/`<i>` rendered as
/// literal text rather than bold/ampersand-entity/italic once passed). A
/// `reason`/`comm` containing real Pango markup would otherwise render
/// as formatting, or worse: `&` alone is an entity-reference PREFIX, so an
/// unescaped `&` in an origin line can corrupt the rendered text or throw a
/// GMarkup parse warning. Don't drop this flag "since the text is just a
/// secret/consumer name" — `reason`/`origin` (P3) made this sink reachable
/// with genuinely free-text, caller-influenced content for the first time.
fn spawn_zenity_notice(zenity_cmd: &str, title: &str, text: &str) -> std::io::Result<Child> {
    Command::new(zenity_cmd)
        .args(["--info", "--no-markup", "--title", title, "--text", text])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
}

// `run_entry_dialog` lives in `aoide_protocol::dialog` (P-P5, F5). `--info`
// has no extra button, so the dismiss label can never match its empty stdout;
// it is passed only because the shared reader takes one.
fn run_zenity_notice(zenity_cmd: &str, title: &str, text: &str, should_cancel: impl FnMut() -> bool) -> ZenityResult {
    run_entry_dialog(|| spawn_zenity_notice(zenity_cmd, title, text), DISMISS_LABEL, should_cancel)
}

/// The popup's text for one parked ask: the line a person reads, then the
/// same context lines the tty prompt shows (reason, origin), each present only
/// when the ask carries it.
fn popup_text(ask: &Ask, seconds: u64) -> String {
    let mut text = format!("ask {} for `{}` \u{2190} {} \u{00b7} {}s left\nrun: aoide secrets approve {}", ask.id, ask.secret, ask.consumer, seconds, ask.id);
    for line in [format_reason_line(&ask.reason), format_origin_line(&ask.origin)].into_iter().flatten() {
        text.push('\n');
        text.push_str(&line);
    }
    text
}

/// The `--popup` loop — main thread only, mirrors [`prompt_loop`]'s own
/// shape (pick the next un-ignored ask, act, repeat) but drives a dialog
/// instead of reading `[a]`/`[d]`/`[i]` from stdin: a zenity notice, see
/// [`run_zenity_notice`].
/// `ignored` is the SAME "Cancel/close stops re-prompting for THIS ask, this
/// session only" semantics `[i]` holds in [`prompt_loop`] (design doc:
/// "Cancel/close = IGNORE") — without it, a cancelled dialog would reopen
/// every ~200ms forever.
#[allow(clippy::too_many_arguments)]
fn popup_loop(queue: &Arc<Mutex<Queue>>, out_lock: &Arc<Mutex<()>>, zenity_cmd: &str, locker_process: &str) {
    let mut ignored: HashSet<String> = HashSet::new();
    let mut spawn_backoff = SPAWN_BACKOFF_INITIAL;
    let mut spawn_failing = false;
    loop {
        if INTERRUPTED.load(Ordering::SeqCst) {
            return;
        }
        let ask = {
            let q = queue.lock().unwrap_or_else(|e| e.into_inner());
            ignored.retain(|id| q.get(id).is_some());
            pick_next(q.snapshot(), &ignored).cloned()
        };
        let Some(ask) = ask else {
            thread::sleep(Duration::from_millis(200));
            continue;
        };
        if is_locked(locker_process) {
            thread::sleep(Duration::from_secs(1));
            continue;
        }

        let seconds = ask.remaining(unix_now()).max(0) as u64;
        let title = format!("aoide \u{b7} {}", ask.secret);
        let text = popup_text(&ask, seconds);
        let cancel_queue = Arc::clone(queue);
        let cancel_id = ask.id.clone();
        let result = run_zenity_notice(zenity_cmd, &title, &text, || {
            cancel_queue.lock().unwrap_or_else(|e| e.into_inner()).get(&cancel_id).is_none()
        });

        if !matches!(result, ZenityResult::SpawnError(_) | ZenityResult::DialogFailure(_)) && spawn_failing {
            spawn_failing = false;
            spawn_backoff = SPAWN_BACKOFF_INITIAL;
            let _g = out_lock.lock().unwrap_or_else(|e| e.into_inner());
            println!("  aoide secrets watch --popup: the notice is opening again \u{2014} backoff cleared");
        }

        match result {
            // OK and a bare close both mean "seen". The ask stays parked, and
            // `secrets approve` is the only way to complete it.
            ZenityResult::Approved(_) | ZenityResult::Dismissed | ZenityResult::Cancelled => {
                ignored.insert(ask.id.clone());
            }
            ZenityResult::CancelledExternally => {
                let _g = out_lock.lock().unwrap_or_else(|e| e.into_inner());
                println!("  ask {} resolved elsewhere while its notice was open \u{2014} closing the notice", ask.id);
            }
            ZenityResult::SpawnError(e) | ZenityResult::DialogFailure(e) => {
                if !spawn_failing {
                    spawn_failing = true;
                    let _g = out_lock.lock().unwrap_or_else(|e2| e2.into_inner());
                    eprintln!(
                        "  aoide secrets watch --popup: ask {} has no working notice right now ({e}) \u{2014} backing off, retrying up to every {}s",
                        ask.id,
                        SPAWN_BACKOFF_MAX.as_secs()
                    );
                }
                // #108: interruptible, so Ctrl-C does not wait out the backoff.
                sleep_backoff_interruptible(spawn_backoff, &INTERRUPTED);
                spawn_backoff = next_spawn_backoff(spawn_backoff);
            }
        }
    }
}

// ── P3/P4: the loop ──────────────────────────────────────────────────────

static INTERRUPTED: AtomicBool = AtomicBool::new(false);

extern "C" fn on_sigint(_signum: libc::c_int) {
    INTERRUPTED.store(true, Ordering::SeqCst);
}

fn install_sigint_handler() {
    unsafe {
        libc::signal(libc::SIGINT, on_sigint as *const () as libc::sighandler_t);
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Reconcile once against `client::pending` — a socket error is the taught
/// connect message (`client::describe_connect_error`, baked into
/// `pending`'s own `Err`), printed and swallowed: the broker may simply not
/// be running yet or may be restarting, and this watcher keeps tailing the
/// log regardless (module doc). No throttle on a repeated failure —
/// deliberate, the same "wait for the field to complain" posture this
/// crate's other notify/audit paths hold (`AGENTS.md`'s no-dedup note).
fn reconcile_once(socket_path: &Path, queue: &Mutex<Queue>) {
    match client::pending(socket_path) {
        Ok(asks) => {
            let mut q = queue.lock().unwrap_or_else(|e| e.into_inner());
            q.reconcile(&asks, crate::park::park_timeout().as_secs());
        }
        Err(e) => eprintln!("aoide secrets watch: {e}"),
    }
}

/// Print one event (narration or `--json`), under the output lock, and — if
/// a prompt is currently open — reprint the FULL prompt frame (header,
/// options line, and the `└ > ` entry marker — never just the header)
/// underneath, so an async narration line can never leave the operator's
/// own open prompt looking like just a bare header with no way to tell what
/// `[a]`/`[d]`/`[i]` even do (review rider: "reprint the full prompt frame,
/// not just the header line" — design doc's "1.3 Signal flow" ASCII already
/// shows the full block reprinted this way).
fn emit_event(event: &Event, json_mode: bool, out_lock: &Mutex<()>, queue: &Mutex<Queue>, prompt_frame: &Mutex<Option<String>>) {
    let _g = out_lock.lock().unwrap_or_else(|e| e.into_inner());
    if json_mode {
        println!("{}", event_to_json(event));
        let _ = std::io::stdout().flush();
        return;
    }
    println!("{}", narrate_event(event));
    if matches!(event, Event::Parked { .. }) {
        let n = queue.lock().unwrap_or_else(|e| e.into_inner()).len();
        if n > 1 {
            println!("            (queued \u{2014} {} ask(s) waiting behind this one)", n - 1);
        }
    }
    if let Some(frame) = prompt_frame.lock().unwrap_or_else(|e| e.into_inner()).clone() {
        println!();
        print!("{frame}");
    }
    let _ = std::io::stdout().flush();
}

fn print_farewell(queue: &Mutex<Queue>, out_lock: &Mutex<()>) {
    let _g = out_lock.lock().unwrap_or_else(|e| e.into_inner());
    let n = queue.lock().unwrap_or_else(|e| e.into_inner()).len();
    println!();
    println!("left the watcher \u{2014} {n} ask(s) still parked; complete with aoide secrets approve <id>");
    let _ = std::io::stdout().flush();
}

fn tail_loop(mut follower: Follower, socket_path: PathBuf, queue: Arc<Mutex<Queue>>, out_lock: Arc<Mutex<()>>, prompt_frame: Arc<Mutex<Option<String>>>, json_mode: bool) -> ! {
    let mut ticks_since_reconcile: u32 = 0;
    loop {
        if INTERRUPTED.load(Ordering::SeqCst) {
            print_farewell(&queue, &out_lock);
            std::process::exit(0);
        }
        match follower.poll() {
            Ok(lines) => {
                for line in lines {
                    let Some(event) = parse_notify_line(&line, unix_now()) else { continue };
                    {
                        let mut q = queue.lock().unwrap_or_else(|e| e.into_inner());
                        q.apply(&event);
                    }
                    emit_event(&event, json_mode, &out_lock, &queue, &prompt_frame);
                    reconcile_once(&socket_path, &queue);
                }
            }
            Err(e) => eprintln!("aoide secrets watch: reading the log: {e}"),
        }
        ticks_since_reconcile += 1;
        if ticks_since_reconcile >= 30 {
            ticks_since_reconcile = 0;
            reconcile_once(&socket_path, &queue);
        }
        thread::sleep(Duration::from_secs(1));
    }
}

fn format_prompt_header(ask: &Ask, remaining: i64, queued: usize) -> String {
    let asked = hms(ask.requested_at);
    let est = if ask.estimated { "~" } else { "" };
    let left = format!("{est}{} left", mmss(remaining));
    let tail = if queued > 0 { format!(" \u{2014} {queued} queued \u{2014}") } else { String::new() };
    let mut header =
        format!("\u{250c} ask {} \u{2500} {} \u{2190} {} \u{2500} asked {asked} \u{2500} {left}{tail}", ask.id, ask.secret, ask.consumer);
    // P3: the context block — reason then origin, each its own line, both
    // absent when the ask carries neither (byte-identical to before this
    // phase in that case).
    for line in [format_reason_line(&ask.reason), format_origin_line(&ask.origin)].into_iter().flatten() {
        header.push('\n');
        header.push_str(&format!("\u{2502} {line}"));
    }
    header
}

/// The FULL prompt block `emit_event` reprints verbatim after an async
/// narration line (this module's own "reprint the full frame" rider,
/// `emit_event`'s doc) — `header`, the options line, and the
/// `└ > ` entry marker, ending WITHOUT a trailing newline so the cursor
/// sits ready for input exactly where it would after the original print.
fn format_prompt_frame(header: &str) -> String {
    let options = "\u{2502} [a] show the approve command   [d] dismiss the ask   [i] ignore (stays parked)";
    format!("{header}\n{options}\n\u{2514} > ")
}

fn clear_prompt_frame(prompt_frame: &Mutex<Option<String>>) {
    *prompt_frame.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// The interactive prompt loop — main thread only, only when stdin is a
/// terminal and `--json` is absent (module doc). Picks the next un-ignored
/// ask ([`pick_next`], oldest `requested_at` first), opens its prompt
/// block, and reads ONE line for `[a]`/`[d]`/`[i]` — never raw single-key
/// (design doc's "1.5 Prompt flow": works over ssh, no terminal-state
/// restoration risk).
fn prompt_loop(
    socket_path: &Path,
    queue: &Arc<Mutex<Queue>>,
    out_lock: &Arc<Mutex<()>>,
    prompt_frame: &Arc<Mutex<Option<String>>>,
    input: &mut impl BufRead,
) {
    let mut ignored: HashSet<String> = HashSet::new();
    loop {
        if INTERRUPTED.load(Ordering::SeqCst) {
            return;
        }
        let (ask, queued) = {
            let q = queue.lock().unwrap_or_else(|e| e.into_inner());
            ignored.retain(|id| q.get(id).is_some());
            let picked = pick_next(q.snapshot(), &ignored).cloned();
            let queued = q.len().saturating_sub(1);
            (picked, queued)
        };
        let Some(ask) = ask else {
            thread::sleep(Duration::from_millis(200));
            continue;
        };

        let now = unix_now();
        let remaining = ask.remaining(now);
        let header = format_prompt_header(&ask, remaining, queued);
        let frame = format_prompt_frame(&header);
        {
            let _g = out_lock.lock().unwrap_or_else(|e| e.into_inner());
            *prompt_frame.lock().unwrap_or_else(|e| e.into_inner()) = Some(frame.clone());
            println!();
            print!("{frame}");
            let _ = std::io::stdout().flush();
        }

        let mut line = String::new();
        let read = input.read_line(&mut line);
        match read {
            Ok(0) | Err(_) => {
                clear_prompt_frame(prompt_frame);
                return; // EOF or a read error: leave the watcher cleanly.
            }
            Ok(_) => {}
        }
        let choice = line.trim().to_ascii_lowercase();

        match choice.as_str() {
            "a" => {
                let _g = out_lock.lock().unwrap_or_else(|e| e.into_inner());
                println!("  approve it with: aoide secrets approve {}", ask.id);
            }
            "d" => {
                let result = client::dismiss(socket_path, &ask.id);
                queue.lock().unwrap_or_else(|e| e.into_inner()).remove(&ask.id);
                let _g = out_lock.lock().unwrap_or_else(|e| e.into_inner());
                match result {
                    Ok(()) => println!("  dismissed ask {} \u{2014} the waiting caller gets a clean refusal", ask.id),
                    Err(e) => println!("  could not dismiss ask {}: {e}", ask.id),
                }
            }
            "i" => {
                ignored.insert(ask.id.clone());
            }
            _ => {
                let _g = out_lock.lock().unwrap_or_else(|e| e.into_inner());
                println!("  unrecognized \u{2014} [a] approve command  [d] dismiss  [i] ignore");
            }
        }
        clear_prompt_frame(prompt_frame);
    }
}

/// Wait for `events_path` to exist, narrating the wait exactly once, then
/// open it at EOF — review rider: on a brand-new host the events feed may
/// not exist yet at `secrets watch` startup (the broker hasn't emitted
/// anything since boot), and exiting 1 immediately (the pre-rider
/// behavior) is needlessly hostile when the fix is just "the broker hasn't
/// written its first line yet, wait a moment." Only
/// [`std::io::ErrorKind::NotFound`] waits — a PERMISSION error or anything
/// else still fails immediately (`Err(1)`), same as before this rider: a
/// wait would only mislead when the file exists but can't be read. `Err(0)`
/// means Ctrl-C landed while waiting — a clean exit, not a failure.
/// `poll_interval` is a parameter (never a bare `Duration::from_secs(1)`
/// inline) so the tempfile test below doesn't have to spend real seconds
/// waiting on it. This NotFound-poll semantics is unchanged by P-G4 (task
/// #77) moving the tail source from the mirrored `~/Aoide/log` to the
/// broker-owned events feed — a fresh `/run/aoide-secrets/` with no event
/// emitted yet is exactly the same "wait, don't exit 1" shape a brand-new
/// `~/Aoide/log` used to be.
fn wait_for_follower(events_path: &Path, poll_interval: Duration) -> Result<Follower, i32> {
    let mut narrated = false;
    loop {
        match Follower::open_at_end(events_path) {
            Ok(f) => return Ok(f),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if !narrated {
                    eprintln!("aoide secrets watch: waiting for the events feed to appear at {}", events_path.display());
                    narrated = true;
                }
            }
            Err(e) => {
                eprintln!("aoide secrets watch: opening {}: {e}", events_path.display());
                return Err(1);
            }
        }
        if INTERRUPTED.load(Ordering::SeqCst) {
            return Err(0);
        }
        thread::sleep(poll_interval);
        if INTERRUPTED.load(Ordering::SeqCst) {
            return Err(0);
        }
    }
}

/// The full `aoide secrets watch` command — foreground, blocks until Ctrl-C or
/// (in the interactive/`--popup` loops) stdin EOF. `events_path`/
/// `socket_path` are resolved ONCE by the caller and passed in (this
/// crate's own `home`/`socket` resolution discipline, `AGENTS.md`) — this
/// function never re-derives either; `events_path` is `socket::events_path`
/// applied to the SAME resolved `socket_path` (P-G4, task #77 — replacing
/// the mirrored `~/Aoide/log` path this parameter carried through P-N3,
/// see module doc for why). `json_mode` forces narration-only regardless of
/// tty (module doc); `popup_mode` (`--popup`, mutually exclusive with
/// `json_mode` — `commands::handle_secrets_watch` refuses the combination
/// before this function is ever called) swaps the tty prompt for a zenity
/// notice — and runs regardless of whether
/// stdin is a terminal. See [`select_mode`] for the exact precedence between
/// the three.
pub fn run(socket_path: &Path, events_path: &Path, json_mode: bool, popup_mode: bool) -> i32 {
    if popup_mode && !zenity_available(ZENITY_CMD) {
        eprintln!(
            "aoide secrets watch --popup: `zenity` was not found \u{2014} install it, or run `aoide secrets watch` (without --popup) instead"
        );
        return 1;
    }

    install_sigint_handler();

    let follower = match wait_for_follower(events_path, Duration::from_secs(1)) {
        Ok(f) => f,
        Err(code) => return code,
    };

    let queue = Arc::new(Mutex::new(Queue::new()));
    let out_lock = Arc::new(Mutex::new(()));
    let prompt_frame: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let mode = select_mode(json_mode, popup_mode, client::stdin_is_tty());

    if !json_mode {
        println!("watching secret events \u{2014} ^C to leave (parked asks stay parked)");
        let _ = std::io::stdout().flush();
    }

    // Catch up on any ask already parked before this watcher started
    // (module doc: the tail alone would never see it).
    reconcile_once(socket_path, &queue);

    let tail_handle = {
        let socket_path = socket_path.to_path_buf();
        let queue = Arc::clone(&queue);
        let out_lock = Arc::clone(&out_lock);
        let prompt_frame = Arc::clone(&prompt_frame);
        thread::spawn(move || tail_loop(follower, socket_path, queue, out_lock, prompt_frame, json_mode))
    };

    match mode {
        Mode::Popup => {
            popup_loop(&queue, &out_lock, ZENITY_CMD, &locker_process_name());
            0
        }
        Mode::InteractiveTty => {
            prompt_loop(socket_path, &queue, &out_lock, &prompt_frame, &mut std::io::stdin().lock());
            0
        }
        Mode::Json | Mode::NarrationOnly => {
            let _ = tail_handle.join();
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── the POSIX-shell fixture class (gated, with the reason) ───────────
    //
    // Every test below carrying `#[cfg(unix)]` drives the backend TEMPLATE
    // mechanism with a POSIX fixture: an `sh -c` command line, or a
    // `#!/bin/sh` shim script on `PATH`. The mechanism itself is portable and
    // HAS a native arm — `sh -c` on Unix, `cmd /C` on native Windows
    // (`backend::run_backend_command`'s own doc) — so these gates name the
    // FIXTURE, never the code under test. The Windows arm of the same path is
    // exercised natively by `backend::tests::a_native_windows_template_*`.

    /// The events-feed line shape (P-G4, task #77): the bare
    /// `emit_notify` payload, verbatim — no `AuditRecord` wrapper. `ts` is
    /// no longer part of the line at all (`parse_notify_line`'s own doc);
    /// it is supplied by the CALLER of `parse_notify_line`, not embedded
    /// here.
    fn notify_line(payload: &Value) -> String {
        payload.to_string()
    }

    // ── parse_notify_line ────────────────────────────────────────────

    #[test]
    fn parses_every_one_of_the_five_event_kinds() {
        let released = json!({ "event": "released", "secret": "aws-ci", "consumer": "melete" });
        assert_eq!(
            parse_notify_line(&notify_line(&released), 1),
            Some(Event::Released { secret: "aws-ci".into(), consumer: "melete".into(), ts: 1 })
        );

        let parked = json!({ "event": "parked", "id": "ab12-1", "secret": "db-prod", "consumer": "claude", "timeoutSecs": 300 });
        assert_eq!(
            parse_notify_line(&notify_line(&parked), 2),
            Some(Event::Parked { id: "ab12-1".into(), secret: "db-prod".into(), consumer: "claude".into(), timeout_secs: 300, ts: 2, reason: None, origin: Default::default() })
        );

        let completed = json!({ "event": "completed", "id": "ab12-1", "secret": "db-prod", "consumer": "claude" });
        assert_eq!(
            parse_notify_line(&notify_line(&completed), 3),
            Some(Event::Completed { id: "ab12-1".into(), secret: "db-prod".into(), consumer: "claude".into(), ts: 3 })
        );

        let dismissed = json!({ "event": "dismissed", "id": "ab12-1", "secret": "db-prod", "consumer": "claude" });
        assert_eq!(
            parse_notify_line(&notify_line(&dismissed), 4),
            Some(Event::Dismissed { id: "ab12-1".into(), secret: "db-prod".into(), consumer: "claude".into(), ts: 4 })
        );

        let expired = json!({ "event": "expired", "id": "ab12-1", "secret": "db-prod", "consumer": "claude" });
        assert_eq!(
            parse_notify_line(&notify_line(&expired), 5),
            Some(Event::Expired { id: "ab12-1".into(), secret: "db-prod".into(), consumer: "claude".into(), ts: 5 })
        );
    }

    /// P3: a `parked` line carrying `reason`/`origin` parses both into the
    /// `Event`; a line WITHOUT either (the pre-P3 shape, an older broker)
    /// still parses cleanly with both defaulted — `parses_every_one_of_the_
    /// five_event_kinds`'s own `parked` case above already covers that half.
    #[test]
    fn parked_carries_reason_and_origin_when_the_broker_sent_them() {
        let parked = json!({
            "event": "parked",
            "id": "ab12-1",
            "secret": "db-prod",
            "consumer": "claude",
            "timeoutSecs": 300,
            "reason": "sudo nixos-rebuild switch",
            "origin": { "username": "khoa", "pid": 4242, "comm": "bash", "hostname": "yomi-strix" },
        });
        let event = parse_notify_line(&notify_line(&parked), 2).unwrap();
        assert_eq!(
            event,
            Event::Parked {
                id: "ab12-1".into(),
                secret: "db-prod".into(),
                consumer: "claude".into(),
                timeout_secs: 300,
                ts: 2,
                reason: Some("sudo nixos-rebuild switch".into()),
                origin: client::PendingOrigin {
                    username: Some("khoa".into()),
                    pid: Some(4242),
                    comm: Some("bash".into()),
                    hostname: Some("yomi-strix".into()),
                },
            }
        );
    }

    // ── format_reason_line / format_origin_line (P3 context lines) ──────

    #[test]
    fn format_reason_line_is_none_when_absent() {
        assert_eq!(format_reason_line(&None), None);
    }

    #[test]
    fn format_reason_line_quotes_and_labels_the_text() {
        assert_eq!(format_reason_line(&Some("sudo nixos-rebuild switch".into())), Some("for: \"sudo nixos-rebuild switch\"".into()));
    }

    #[test]
    fn format_origin_line_is_none_when_every_field_is_unknown() {
        assert_eq!(format_origin_line(&client::PendingOrigin::default()), None);
    }

    #[test]
    fn format_origin_line_renders_every_known_field() {
        let origin = client::PendingOrigin {
            username: Some("khoa".into()),
            pid: Some(4242),
            comm: Some("bash".into()),
            hostname: Some("yomi-strix".into()),
        };
        assert_eq!(format_origin_line(&origin), Some("from: khoa \u{b7} bash (pid 4242) @ yomi-strix".into()));
    }

    #[test]
    fn format_origin_line_degrades_to_unidentified_when_only_the_hostname_is_known() {
        // A totally unidentified peer (`peercred::peer_cred` itself failed)
        // still yields a hostname (`capture_origin`'s own doc, broker.rs) —
        // this proves the degraded rendering never produces a bare "from:"
        // with nothing after the label.
        let origin = client::PendingOrigin { hostname: Some("yomi-strix".into()), ..Default::default() };
        assert_eq!(format_origin_line(&origin), Some("from: unidentified @ yomi-strix".into()));
    }

    #[test]
    fn format_origin_line_strips_control_bytes_from_every_field_it_shows() {
        // None of these is the broker's own word: comm is set by the process and
        // the username and hostname come from the machine. Each is cleaned as it
        // is shown, so the escape and the return are gone and the line stays one
        // line.
        let origin = client::PendingOrigin {
            username: Some("khoa\u{1b}[2J".into()),
            pid: Some(4242),
            comm: Some("bash\u{1b}[31m".into()),
            hostname: Some("yomi\rstrix".into()),
        };
        assert_eq!(
            format_origin_line(&origin),
            Some("from: khoa[2J \u{b7} bash[31m (pid 4242) @ yomistrix".into())
        );
    }

    /// `age-identity-minted` (P-G1) is the one `emit_notify` kind this
    /// parser has never recognized — it carries no `secret`/`consumer`
    /// field at all, so it falls out on the `?` right after `kind` is
    /// read, same as before this phase.
    #[test]
    fn an_age_identity_minted_line_is_skipped() {
        let line = json!({ "event": "age-identity-minted" }).to_string();
        assert_eq!(parse_notify_line(&line, 1), None);
    }

    #[test]
    fn a_line_missing_the_event_field_is_skipped() {
        let line = json!({ "secret": "t", "consumer": "m" }).to_string();
        assert_eq!(parse_notify_line(&line, 1), None);
    }

    #[test]
    fn an_unrecognized_event_kind_is_skipped() {
        let payload = json!({ "event": "something-new", "secret": "t", "consumer": "m" });
        assert_eq!(parse_notify_line(&notify_line(&payload), 1), None);
    }

    /// `broker::handle_resolve` writes this exact line when a step's
    /// wrong-code budget is spent. It names no secret or consumer, so the
    /// parser must not require either, or the watch never shows the notice.
    #[test]
    fn the_budget_notice_parses_though_it_names_no_secret_or_consumer() {
        assert!(parse_notify_line(r#"{"event":"totp-budget-spent"}"#, 1).is_some(), "the budget notice is parsed");
    }

    #[test]
    fn the_budget_notice_is_narrated_as_a_line_for_the_person() {
        let event = parse_notify_line(r#"{"event":"totp-budget-spent"}"#, 1).expect("the budget notice parses");
        let line = narrate_event(&event);
        assert!(line.contains("wrong"), "the watch says what was refused: {line}");
    }

    #[test]
    fn not_even_valid_json_is_skipped() {
        assert_eq!(parse_notify_line("{{{not json", 1), None);
    }

    // ── Queue::apply / reconcile ─────────────────────────────────────

    #[test]
    fn apply_parked_inserts_and_a_repeat_parked_line_never_duplicates() {
        let mut q = Queue::new();
        let event = Event::Parked { id: "1".into(), secret: "t".into(), consumer: "m".into(), timeout_secs: 300, ts: 100, reason: None, origin: Default::default() };
        q.apply(&event);
        q.apply(&event);
        assert_eq!(q.len(), 1);
        assert_eq!(q.get("1").unwrap().requested_at, 100);
        assert!(!q.get("1").unwrap().estimated);
    }

    #[test]
    fn apply_completed_dismissed_expired_all_remove_by_id() {
        for make in [
            |id: &str| Event::Completed { id: id.into(), secret: "t".into(), consumer: "m".into(), ts: 1 },
            |id: &str| Event::Dismissed { id: id.into(), secret: "t".into(), consumer: "m".into(), ts: 1 },
            |id: &str| Event::Expired { id: id.into(), secret: "t".into(), consumer: "m".into(), ts: 1 },
        ] {
            let mut q = Queue::new();
            q.apply(&Event::Parked { id: "1".into(), secret: "t".into(), consumer: "m".into(), timeout_secs: 300, ts: 0, reason: None, origin: Default::default() });
            assert_eq!(q.len(), 1);
            q.apply(&make("1"));
            assert!(q.is_empty());
        }
    }

    #[test]
    fn an_expired_for_an_unknown_id_is_a_harmless_no_op() {
        let mut q = Queue::new();
        q.apply(&Event::Expired { id: "never-seen".into(), secret: "t".into(), consumer: "m".into(), ts: 1 });
        assert!(q.is_empty());
    }

    #[test]
    fn released_never_touches_the_queue() {
        let mut q = Queue::new();
        q.apply(&Event::Parked { id: "1".into(), secret: "t".into(), consumer: "m".into(), timeout_secs: 300, ts: 0, reason: None, origin: Default::default() });
        q.apply(&Event::Released { secret: "other".into(), consumer: "m".into(), ts: 5 });
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn reconcile_adds_an_ask_the_tail_never_saw_as_an_estimate() {
        let mut q = Queue::new();
        let pending = vec![PendingAsk {
            id: "unseen".into(),
            secret: "t".into(),
            consumer: "m".into(),
            requested_at: 42,
            peer_uid: None,
                peer_sid: None,
            reason: None,
            argv0: None,
            origin: client::PendingOrigin::default(),
        }];
        q.reconcile(&pending, 300);
        let ask = q.get("unseen").unwrap();
        assert_eq!(ask.requested_at, 42);
        assert_eq!(ask.timeout_secs, 300);
        assert!(ask.estimated);
    }

    #[test]
    fn reconcile_drops_an_ask_completed_elsewhere() {
        let mut q = Queue::new();
        q.apply(&Event::Parked { id: "1".into(), secret: "t".into(), consumer: "m".into(), timeout_secs: 300, ts: 0, reason: None, origin: Default::default() });
        assert_eq!(q.len(), 1);
        q.reconcile(&[], 300); // broker no longer lists it as pending
        assert!(q.is_empty());
    }

    #[test]
    fn reconcile_never_downgrades_a_known_timeout_into_an_estimate() {
        let mut q = Queue::new();
        q.apply(&Event::Parked { id: "1".into(), secret: "t".into(), consumer: "m".into(), timeout_secs: 60, ts: 0, reason: None, origin: Default::default() });
        let pending =
            vec![PendingAsk {
                id: "1".into(),
                secret: "t".into(),
                consumer: "m".into(),
                requested_at: 0,
                peer_uid: None,
                peer_sid: None,
                reason: None,
                argv0: None,
                origin: client::PendingOrigin::default(),
            }];
        q.reconcile(&pending, 300);
        let ask = q.get("1").unwrap();
        assert_eq!(ask.timeout_secs, 60);
        assert!(!ask.estimated);
    }

    /// One ask with a bidi character in its comm and a typed escape in its
    /// reason reads the same through the feed (`apply`, the broker's escape
    /// applied to the line) and through the pending reply (`reconcile`, raw).
    #[test]
    fn an_ask_reads_the_same_through_the_feed_and_through_reconcile() {
        let raw_comm = "evil\u{202e}name";
        let raw_reason = r"typed \u{202e} here";
        let feed = aoide_protocol::untrusted::escape_for_log_value(&json!({
            "event": "parked",
            "id": "ab12-7",
            "secret": "t",
            "consumer": "m",
            "timeoutSecs": 300,
            "reason": raw_reason,
            "origin": {"username": "khoa", "pid": 42, "comm": raw_comm, "hostname": "osaka"},
        }));
        let event = parse_notify_line(&feed.to_string(), 1_700_000_000).expect("the feed line parses");
        let mut from_feed = Queue::new();
        from_feed.apply(&event);

        let pending = vec![PendingAsk {
            id: "ab12-7".into(),
            secret: "t".into(),
            consumer: "m".into(),
            requested_at: 1_700_000_000,
            peer_uid: Some(1000),
            peer_sid: None,
            reason: Some(raw_reason.into()),
            argv0: None,
            origin: client::PendingOrigin {
                username: Some("khoa".into()),
                pid: Some(42),
                comm: Some(raw_comm.into()),
                hostname: Some("osaka".into()),
            },
        }];
        let mut from_pending = Queue::new();
        from_pending.reconcile(&pending, 300);

        let fed = from_feed.get("ab12-7").expect("the feed's ask");
        let reconciled = from_pending.get("ab12-7").expect("the reconciled ask");
        assert!(popup_text(fed, 30).contains(r"\u{202e}name"), "{}", popup_text(fed, 30));
        assert_eq!(popup_text(fed, 30), popup_text(reconciled, 30));
        let mut aligned = fed.clone();
        aligned.estimated = reconciled.estimated;
        assert_eq!(format_prompt_header(&aligned, 30, 0), format_prompt_header(reconciled, 30, 0));
    }

    // ── pick_next (queue ordering) ───────────────────────────────────

    #[test]
    fn pick_next_returns_the_oldest_requested_at_first() {
        let asks = vec![
            Ask { id: "b".into(), secret: "t".into(), consumer: "m".into(), requested_at: 200, timeout_secs: 300, estimated: false, reason: None, origin: Default::default() },
            Ask { id: "a".into(), secret: "t".into(), consumer: "m".into(), requested_at: 100, timeout_secs: 300, estimated: false, reason: None, origin: Default::default() },
        ];
        let picked = pick_next(&asks, &HashSet::new()).unwrap();
        assert_eq!(picked.id, "a");
    }

    #[test]
    fn pick_next_skips_ignored_ids() {
        let asks = vec![
            Ask { id: "a".into(), secret: "t".into(), consumer: "m".into(), requested_at: 100, timeout_secs: 300, estimated: false, reason: None, origin: Default::default() },
            Ask { id: "b".into(), secret: "t".into(), consumer: "m".into(), requested_at: 200, timeout_secs: 300, estimated: false, reason: None, origin: Default::default() },
        ];
        let mut ignored = HashSet::new();
        ignored.insert("a".to_string());
        let picked = pick_next(&asks, &ignored).unwrap();
        assert_eq!(picked.id, "b");
    }

    #[test]
    fn pick_next_on_an_empty_queue_is_none() {
        assert_eq!(pick_next(&[], &HashSet::new()), None);
    }

    // ── narration / --json rendering ─────────────────────────────────

    #[test]
    fn narrate_released_names_secret_and_consumer_never_a_value() {
        let line = narrate_event(&Event::Released { secret: "aws-ci".into(), consumer: "melete".into(), ts: 3600 + 61 });
        assert!(line.contains("released"));
        assert!(line.contains("aws-ci"));
        assert!(line.contains("melete"));
        assert!(line.contains("01:01:01"));
    }

    #[test]
    fn narrate_parked_names_the_id_and_the_countdown() {
        let line = narrate_event(&Event::Parked {
            id: "ab12-1".into(),
            secret: "db-prod".into(),
            consumer: "claude".into(),
            timeout_secs: 300,
            ts: 0,
            reason: None,
            origin: Default::default(),
        });
        assert!(line.contains("ab12-1"));
        assert!(line.contains("5m00s"));
    }

    /// The terminal prompt reads no code and sends no approve op. Given `a`,
    /// then
    /// a code line, it names the approve command and never asks for the code.
    /// The fake socket records every request it gets and refuses each one, so a
    /// request that reached it is on record.
    #[test]
    fn the_terminal_prompt_reads_no_code_and_sends_no_approve_op() {
        let _env = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let socket = tmp_path("prompt-no-approve");
        let _ = std::fs::remove_file(&socket);
        let listener = std::os::unix::net::UnixListener::bind(&socket).expect("bind the fake socket");
        listener.set_nonblocking(true).expect("nonblocking accept");
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let recorded: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let server = {
            let stop = Arc::clone(&stop);
            let recorded = Arc::clone(&recorded);
            thread::spawn(move || {
                while !stop.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            let mut line = String::new();
                            let mut reader = std::io::BufReader::new(stream.try_clone().expect("clone the stream"));
                            if reader.read_line(&mut line).is_ok() {
                                recorded.lock().unwrap_or_else(|e| e.into_inner()).push(line);
                            }
                            let _ = stream.write_all(b"{\"ok\":false,\"error\":\"fake\"}\n");
                        }
                        Err(_) => thread::sleep(Duration::from_millis(10)),
                    }
                }
            })
        };

        let queue = Arc::new(Mutex::new(Queue::new()));
        queue.lock().unwrap_or_else(|e| e.into_inner()).apply(&Event::Parked {
            id: "7".into(),
            secret: "db-prod".into(),
            consumer: "claude".into(),
            timeout_secs: 300,
            ts: unix_now(),
            reason: None,
            origin: Default::default(),
        });
        let out_lock = Arc::new(Mutex::new(()));
        let prompt_frame: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let mut input = std::io::Cursor::new(b"a\n654321\n".to_vec());
        prompt_loop(&socket, &queue, &out_lock, &prompt_frame, &mut input);

        stop.store(true, Ordering::SeqCst);
        server.join().expect("the fake socket thread");
        let _ = std::fs::remove_file(&socket);
        let requests = recorded.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let approves = requests.iter().filter(|line| serde_json::from_str::<Value>(line.trim()).is_ok_and(|v| v["op"] == "approve")).count();
        assert_eq!(approves, 0, "the terminal prompt sent an approve op: {requests:?}");
    }

    /// The parked narration names the command that completes the ask.
    #[test]
    fn a_parked_notice_names_the_approve_command() {
        let line = narrate_event(&Event::Parked {
            id: "ab12-1".into(),
            secret: "db-prod".into(),
            consumer: "claude".into(),
            timeout_secs: 300,
            ts: 0,
            reason: None,
            origin: Default::default(),
        });
        assert!(line.contains("aoide secrets approve ab12-1"), "the parked notice names the command: {line}");
    }

    /// The popup notice names the command that completes the ask.
    #[test]
    fn the_popup_notice_names_the_approve_command() {
        let ask = Ask {
            id: "ab12-1".into(),
            secret: "db-prod".into(),
            consumer: "claude".into(),
            requested_at: 0,
            timeout_secs: 300,
            estimated: false,
            reason: None,
            origin: Default::default(),
        };
        let text = popup_text(&ask, 300);
        assert!(text.contains("aoide secrets approve ab12-1"), "the popup notice names the command: {text}");
    }

    #[test]
    fn event_to_json_parked_carries_requested_and_expires_at() {
        let v = event_to_json(&Event::Parked {
            id: "ab12-1".into(),
            secret: "db-prod".into(),
            consumer: "claude".into(),
            timeout_secs: 300,
            ts: 1_000,
            reason: None,
            origin: Default::default(),
        });
        assert_eq!(v["event"], "parked");
        assert_eq!(v["id"], "ab12-1");
        assert_eq!(v["timeoutSecs"], 300);
        assert_eq!(v["requestedAt"], 1_000);
        assert_eq!(v["expiresAt"], 1_300);
        assert_eq!(v["ts"], 1_000);
    }

    #[test]
    fn event_to_json_released_carries_no_id_at_all() {
        let v = event_to_json(&Event::Released { secret: "aws-ci".into(), consumer: "melete".into(), ts: 1 });
        assert_eq!(v["event"], "released");
        assert!(v.get("id").is_none());
    }

    #[test]
    fn event_to_json_never_carries_a_value_field_on_any_shape() {
        for event in [
            Event::Released { secret: "t".into(), consumer: "m".into(), ts: 1 },
            Event::Parked { id: "1".into(), secret: "t".into(), consumer: "m".into(), timeout_secs: 1, ts: 1, reason: None, origin: Default::default() },
            Event::Completed { id: "1".into(), secret: "t".into(), consumer: "m".into(), ts: 1 },
            Event::Dismissed { id: "1".into(), secret: "t".into(), consumer: "m".into(), ts: 1 },
            Event::Expired { id: "1".into(), secret: "t".into(), consumer: "m".into(), ts: 1 },
        ] {
            assert!(event_to_json(&event).get("value").is_none());
        }
    }

    // ── Follower (P2, a real growing tempfile) ───────────────────────

    fn tmp_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "aoide-secrets-watch-follower-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ))
    }

    #[test]
    fn follower_growth_reads_only_the_delta() {
        let path = tmp_path("growth");
        std::fs::write(&path, b"before this point\n").unwrap();
        let mut f = Follower::open_at_end(&path).unwrap();
        assert_eq!(f.poll().unwrap(), Vec::<String>::new());

        let mut file = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"line one\nline two\n").unwrap();
        assert_eq!(f.poll().unwrap(), vec!["line one".to_string(), "line two".to_string()]);

        file.write_all(b"line three\n").unwrap();
        assert_eq!(f.poll().unwrap(), vec!["line three".to_string()]);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn follower_no_growth_reads_nothing() {
        let path = tmp_path("no-growth");
        std::fs::write(&path, b"x\n").unwrap();
        let mut f = Follower::open_at_end(&path).unwrap();
        assert_eq!(f.poll().unwrap(), Vec::<String>::new());
        assert_eq!(f.poll().unwrap(), Vec::<String>::new());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn follower_truncation_reopens_at_zero() {
        let path = tmp_path("truncate");
        std::fs::write(&path, b"aaaaaaaaaaaaaaaaaaaa\n").unwrap();
        let mut f = Follower::open_at_end(&path).unwrap();
        assert_eq!(f.poll().unwrap(), Vec::<String>::new());

        // Replace with a SHORTER file (simulates truncation/rotation).
        std::fs::write(&path, b"fresh\n").unwrap();
        assert_eq!(f.poll().unwrap(), vec!["fresh".to_string()]);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn follower_survives_delete_and_recreate() {
        // The judge's repro shape: systemd wipes RuntimeDirectory on a
        // broker restart, unlinking the file our open fd still refers to.
        // A brand-new inode is created at the SAME path afterward — the old
        // fd's own `metadata().len()` freezes at whatever it was at
        // deletion and never reflects the new file's growth, so `poll` must
        // notice the path now names a DIFFERENT inode, not just compare
        // lengths.
        let path = tmp_path("delete-recreate");
        std::fs::write(&path, b"before restart\n").unwrap();
        let mut f = Follower::open_at_end(&path).unwrap();
        assert_eq!(f.poll().unwrap(), Vec::<String>::new());

        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, b"after restart\n").unwrap();
        assert_eq!(
            f.poll().unwrap(),
            vec!["after restart".to_string()],
            "a delete-and-recreate at the same path must not leave the follower deaf"
        );

        // Keep proving it's actually live, not a one-shot recovery.
        let mut file = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"one more\n").unwrap();
        assert_eq!(f.poll().unwrap(), vec!["one more".to_string()]);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn follower_survives_rename_away_and_recreate() {
        let path = tmp_path("rename-away");
        let moved = tmp_path("rename-away-moved");
        std::fs::write(&path, b"before rename\n").unwrap();
        let mut f = Follower::open_at_end(&path).unwrap();
        assert_eq!(f.poll().unwrap(), Vec::<String>::new());

        std::fs::rename(&path, &moved).unwrap();
        std::fs::write(&path, b"after rename\n").unwrap();
        assert_eq!(
            f.poll().unwrap(),
            vec!["after rename".to_string()],
            "a rename-away-and-recreate at the same path must not leave the follower deaf"
        );

        std::fs::remove_file(&path).ok();
        std::fs::remove_file(&moved).ok();
    }

    #[test]
    fn follower_holds_a_partial_trailing_line_until_the_newline_arrives() {
        let path = tmp_path("partial");
        std::fs::write(&path, b"").unwrap();
        let mut f = Follower::open_at_end(&path).unwrap();

        let mut file = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"no newline yet").unwrap();
        assert_eq!(f.poll().unwrap(), Vec::<String>::new(), "a partial line must not be returned early");

        file.write_all(b" - now complete\n").unwrap();
        assert_eq!(f.poll().unwrap(), vec!["no newline yet - now complete".to_string()]);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn follower_open_at_end_skips_pre_existing_content() {
        let path = tmp_path("skip-history");
        std::fs::write(&path, b"history line 1\nhistory line 2\n").unwrap();
        let mut f = Follower::open_at_end(&path).unwrap();
        assert_eq!(f.poll().unwrap(), Vec::<String>::new(), "must never re-read from the start");
        std::fs::remove_file(&path).ok();
    }

    // ── wait_for_follower (review rider: wait, don't exit 1) ─────────

    #[test]
    fn wait_for_follower_blocks_until_the_log_appears_then_opens_it() {
        let path = tmp_path("wait-for-log");
        assert!(!path.exists(), "precondition: the log must not exist yet");
        let path2 = path.clone();
        let writer = thread::spawn(move || {
            thread::sleep(Duration::from_millis(60));
            std::fs::write(&path2, b"hello\n").unwrap();
        });
        let result = wait_for_follower(&path, Duration::from_millis(10));
        writer.join().unwrap();
        assert!(result.is_ok(), "expected wait_for_follower to open the log once it appeared");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn wait_for_follower_returns_ok_immediately_when_the_log_already_exists() {
        let path = tmp_path("wait-for-log-present");
        std::fs::write(&path, b"already here\n").unwrap();
        let result = wait_for_follower(&path, Duration::from_secs(30));
        assert!(result.is_ok());
        std::fs::remove_file(&path).ok();
    }

    // ── select_mode (the popup/tty mode selection, pure) ─────────────

    #[test]
    fn select_mode_json_wins_over_popup_and_tty() {
        assert_eq!(select_mode(true, true, true), Mode::Json);
        assert_eq!(select_mode(true, false, false), Mode::Json);
    }

    #[test]
    fn select_mode_popup_wins_over_tty_detection_when_json_is_absent() {
        assert_eq!(select_mode(false, true, true), Mode::Popup);
        assert_eq!(select_mode(false, true, false), Mode::Popup, "--popup on a non-tty stdin still drives the popup loop");
    }

    #[test]
    fn select_mode_interactive_tty_when_neither_json_nor_popup() {
        assert_eq!(select_mode(false, false, true), Mode::InteractiveTty);
    }

    #[test]
    fn select_mode_narration_only_on_a_pipe_with_no_popup() {
        assert_eq!(select_mode(false, false, false), Mode::NarrationOnly);
    }

    // `locked_state`'s own tests moved to `aoide_protocol::dialog`'s test
    // module (P-P5, F5) with the function itself.

    // ── run_zenity_notice (fake-zenity shims) ──
    //
    // Every shim here is a full path handed directly to `run_zenity_notice`/
    // `zenity_available` as `zenity_cmd` — never a `PATH` mutation (module
    // doc: this is exactly why `run`'s `zenity_cmd` parameter exists,
    // distinct from `enroll::render_qr`'s older `PATH`-shim precedent,
    // which needs `env_lock` because `PATH` is process-global and cargo
    // test runs in parallel threads). No `env_lock` needed here for that
    // reason.

    #[cfg(unix)]
    fn write_shim(tag: &str, script: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "aoide-secrets-watch-zenity-shim-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let shim = dir.join("zenity-shim");
        std::fs::write(&shim, script).unwrap();
        crate::home::set_mode(&shim, 0o755).unwrap();
        thread::sleep(Duration::from_millis(5));
        shim
    }

    #[cfg(unix)]
    fn remove_shim(shim: &Path) {
        if let Some(dir) = shim.parent() {
            std::fs::remove_dir_all(dir).ok();
        }
    }

    /// Serializes every test below that WRITES a shim script and then
    /// immediately EXECS it, against every OTHER such test — diagnosed root
    /// cause of the "Text file busy" flake this lock exists to close (this
    /// commit, reproduced live under `--test-threads` > 1 and confirmed by
    /// instrumenting `write_shim`/`run_zenity_notice` with a shared debug
    /// log): it is NOT a path collision — every shim already gets its own
    /// unique tempdir (`write_shim`'s own doc, `{tag}-{pid}-{nanos}`), and
    /// the instrumented trace caught the failure on a test's OWN
    /// just-written, just-closed, just-chmod'd shim, same thread, same
    /// never-reused path, no second writer anywhere. It is a genuine Linux
    /// `execve()`/`close()` TOCTOU that only manifests under heavy parallel
    /// CPU/scheduler contention (it reproduced readily at
    /// `--test-threads=32`, even restricted to ONLY this module's five
    /// shim tests — i.e. contention among a handful of write-then-exec
    /// pairs is already enough, no unrelated test needed). Two textbook
    /// non-retry fixes were tried and BOTH still reproduced it: giving each
    /// shim a unique name changes nothing (already true here) and
    /// write-close-then-rename doesn't touch the mechanism either — a
    /// rename repoints a directory entry, it does not change the
    /// underlying inode's write-access state, which is what `execve()`
    /// actually checks. What removes the failure, reliably, across dozens
    /// of `--test-threads=32` reruns, is simply not contending: this lock
    /// serializes this module's own write+exec pairs against EACH OTHER,
    /// which is exactly the contention the race needs — the rest of the
    /// suite (everything outside this section) still runs at full
    /// parallelism, and this is a real fix for a real kernel-timing race,
    /// never a retry loop hiding it.
    #[cfg(unix)]
    fn shim_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    // `zenity_available`'s own tests moved to `aoide_protocol::dialog`'s
    // test module (P-P5, F5) with the function itself.

    // cfg(unix): the fixture is a POSIX shell template or a `#!/bin/sh` shim
    // (the module note above names the class; the code under test is portable).
    #[cfg(unix)]
    #[test]
    fn run_zenity_notice_treats_a_bare_close_as_cancelled() {
        let _env = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _guard = shim_lock();
        let shim = write_shim("bare-close", "#!/bin/sh\nexit 1\n");
        let result = run_zenity_notice(shim.to_str().unwrap(), "t", "x", || false);
        assert!(matches!(result, ZenityResult::Cancelled), "expected Cancelled, got {result:?}");
        remove_shim(&shim);
    }

    #[test]
    fn run_zenity_notice_reports_a_spawn_error_for_a_nonexistent_shim() {
        let result = run_zenity_notice("/no/such/aoide-secrets-watch-zenity-shim", "t", "x", || false);
        assert!(matches!(result, ZenityResult::SpawnError(_)), "expected SpawnError, got {result:?}");
    }

    /// The notice is an info box with no entry field: `--info` and
    /// `--no-markup` ride every call, `--entry` never does. `--no-markup`
    /// matters because `--text` carries untrusted `reason`/`origin` text
    /// (`spawn_zenity_notice`'s own doc).
    // cfg(unix): the fixture is a POSIX shell template or a `#!/bin/sh` shim
    // (the module note above names the class; the code under test is portable).
    #[cfg(unix)]
    #[test]
    fn spawn_zenity_notice_argv_is_an_info_box_with_no_markup_and_no_entry() {
        let _env = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _guard = shim_lock();
        let shim = write_shim("argv-notice", "#!/bin/sh\necho \"$@\" > \"$(dirname \"$0\")/argv.log\"\nexit 0\n");
        let _ = run_zenity_notice(shim.to_str().unwrap(), "t", "x", || false);
        let argv = std::fs::read_to_string(shim.parent().unwrap().join("argv.log")).unwrap();
        assert!(argv.contains("--info") && argv.contains("--no-markup"), "argv must be a no-markup info box, got: {argv}");
        assert!(!argv.contains("--entry"), "a notice takes no code, so no entry field, got: {argv}");
        remove_shim(&shim);
    }

    /// Proves the "kill by its EXACT pid" mechanism (module doc,
    /// [`ZenityResult::CancelledExternally`]): a shim that sleeps far longer
    /// than this test's own timeout must still be killed and this call must
    /// still return promptly, once `should_cancel` starts returning `true`.
    // cfg(unix): the fixture is a POSIX shell template or a `#!/bin/sh` shim
    // (the module note above names the class; the code under test is portable).
    #[cfg(unix)]
    #[test]
    fn run_zenity_notice_kills_the_exact_child_when_the_ask_resolves_elsewhere() {
        let _env = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _guard = shim_lock();
        let shim = write_shim("longsleep", "#!/bin/sh\nsleep 30\necho should-not-appear\nexit 0\n");
        let mut polls = 0u32;
        let start = std::time::Instant::now();
        let result = run_zenity_notice(shim.to_str().unwrap(), "t", "x", || {
            polls += 1;
            polls >= 2 // cancel on the second poll tick, well before the 30s sleep would finish
        });
        let elapsed = start.elapsed();
        assert!(matches!(result, ZenityResult::CancelledExternally), "expected CancelledExternally, got {result:?}");
        assert!(elapsed < Duration::from_secs(10), "should_cancel should have killed the sleeping shim promptly, took {elapsed:?}");
        remove_shim(&shim);
    }

    /// An ask that vanishes from `pending` with NO feed event at all (a
    /// broker restart wiped the park registry, or an event line was lost)
    /// must not leave a notice up forever. `Queue::reconcile` drops such an
    /// ask (`reconcile_drops_an_ask_completed_elsewhere`, above); this pins
    /// that the notice path acts on that removal.
    // cfg(unix): the fixture is a POSIX shell template or a `#!/bin/sh` shim
    // (the module note above names the class; the code under test is portable).
    #[cfg(unix)]
    #[test]
    fn popup_kills_the_notice_when_the_ask_vanishes_via_reconcile_not_an_event() {
        let _env = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _guard = shim_lock();
        let shim = write_shim("phantom", "#!/bin/sh\nsleep 30\necho should-not-appear\nexit 0\n");

        let queue: Arc<Mutex<Queue>> = Arc::new(Mutex::new(Queue::new()));
        queue.lock().unwrap().apply(&Event::Parked {
            id: "1".into(),
            secret: "t".into(),
            consumer: "m".into(),
            timeout_secs: 300,
            ts: unix_now(),
            reason: None,
            origin: Default::default(),
        });
        assert_eq!(queue.lock().unwrap().len(), 1);

        let reconciler_queue = Arc::clone(&queue);
        let reconciler = thread::spawn(move || {
            thread::sleep(Duration::from_millis(150));
            // `pending` no longer lists the ask and NO `Event` fires for it:
            // `reconcile` is the only thing that ever learns about this.
            reconciler_queue.lock().unwrap_or_else(|e| e.into_inner()).reconcile(&[], 300);
        });

        let cancel_queue = Arc::clone(&queue);
        let start = std::time::Instant::now();
        let result = run_zenity_notice(shim.to_str().unwrap(), "t", "x", || {
            cancel_queue.lock().unwrap_or_else(|e| e.into_inner()).get("1").is_none()
        });
        let elapsed = start.elapsed();
        reconciler.join().unwrap();

        assert!(matches!(result, ZenityResult::CancelledExternally), "expected CancelledExternally, got {result:?}");
        assert!(elapsed < Duration::from_secs(10), "a phantom-ask reconcile should kill the notice promptly, took {elapsed:?}");
        assert!(queue.lock().unwrap().get("1").is_none(), "the phantom ask must stay gone from the queue");

        remove_shim(&shim);
    }

    // ── wait_for_follower: permission error vs NotFound ────────────────

    /// Task #76 item 4: a NotFound open waits and narrates
    /// (`wait_for_follower_blocks_until_the_log_appears_then_opens_it`,
    /// above) — a PERMISSION error must instead fail IMMEDIATELY
    /// (`Err(1)`), never wait, since waiting would only mislead when the
    /// file exists but can't be read (`wait_for_follower`'s own doc). Root
    /// ignores file permissions, so this skips under a root test runner —
    /// same precedent `an_unreadable_policy_json_teaches_the_chown_
    /// reference_fix_on_both_gates` (`broker.rs`) sets.
    // cfg(unix): the fixture is a MODE, not a shim — `set_mode(path, 0o000)`
    // makes the file unreadable to its own owner on Unix, and that is the
    // permission error `wait_for_follower` must fail fast on. The native
    // Windows arm of `set_mode` is a documented no-op for a LOOSENING mode
    // (there is no world-writable policy to attach and no "unreadable by its
    // owner" state to arrange), so this host has no such fixture to build.
    #[cfg(unix)]
    #[test]
    fn wait_for_follower_fails_immediately_on_a_permission_error_never_waiting() {
        if crate::home::running_as_root() {
            return;
        }
        let path = tmp_path("wait-for-log-perm-denied");
        std::fs::write(&path, b"x\n").unwrap();
        crate::home::set_mode(&path, 0o000).unwrap();

        let start = std::time::Instant::now();
        // A deliberately LONG poll interval: if this incorrectly treated
        // PermissionDenied as NotFound-and-wait, the test would hang for
        // up to this long instead of returning immediately.
        let result = wait_for_follower(&path, Duration::from_secs(30));
        let elapsed = start.elapsed();

        // Restore before cleanup can remove the tempfile.
        let _ = crate::home::set_mode(&&path, 0o644);

        assert_eq!(result.err(), Some(1), "a permission error must fail immediately (Err(1)), never wait");
        assert!(elapsed < Duration::from_secs(5), "must not have waited at all, took {elapsed:?}");

        std::fs::remove_file(&path).ok();
    }
}
