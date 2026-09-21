//! `session trace <id> [--tail N] [--follow] [--json] [--clip line|detail]` —
//! the run, step by step, from the harness's JSON records
//! (`docs/architecture/EIDOLON-TRACE.md`: read side Aoide, write side
//! eidolon).
//!
//! Every other harness's on-disk turn log is its TRANSCRIPT, and Aoide reads
//! it to fill `say`/`tool`/`contextTokens` — a handful of fields, never a
//! stream a caller can watch. Eidolon exposes one JSON record per line through
//! its read-only export or a legacy mirror, so the whole
//! run is readable in order: `AssistantMessage` (thinking, text, tool calls),
//! `ToolResult`, `TurnSettled`, `TurnBudget`/`TurnDeadline`, `AskUser`,
//! `ExternalMessage`, `Cancelled`. This module renders that stream and
//! nothing else — the contract (line shape, the state rule) is eidolon's and
//! is stated once in that design doc.
//!
//! **Nothing here is a second parser or a second resolver.**
//! `aoide_protocol::agents::eidolon_trace_record` turns one line into a
//! [`TraceRecord`] (the SAME function `eidolon_state_from_trace` folds
//! state from), the harness CAPABILITY is
//! `TranscriptSpec::trace` (never `if agent == "eidolon"` — see
//! `aoide-protocol`'s `AGENTS.md`), the trace PATH comes from
//! `TranscriptSpec::locate` (the same locator the reaper's transcript
//! refresh calls, so there is one answer to "which file is this session's"),
//! and `<id>` resolves through `aoide_storage::addr::resolve` — the exact
//! resolver `send --to` and bare `session`'s filter use. This module owns
//! only the PROJECTION (`steps_of` / `steps_of_lines` — one step per emitted
//! content block, which the human `render_line` renders from so the terminal
//! line and `data.steps` cannot drift) and the `--follow` loop.
//!
//! **Read-only, no stage write, no daemon.** Nothing here mutates any file:
//! it loads `sessions.json`, derives a name for the session, reads the trace
//! and prints. It therefore takes no stage lock and never routes through
//! `aoide_client::daemon::daemon_dispatch` — unlike every session-WRITE
//! handler in this crate (P-D6's L4 family).
//!
//! **`--follow` blocks until Ctrl-C** and is CLI-only, the same shape
//! `events tail`/`secrets watch` hold: a follow-style command makes no sense
//! over MCP/A2A, where one connection would be parked until a human at the
//! other end gave up. `--json` in follow mode prints each NEW record line
//! verbatim (the line already IS the wire shape) rather than an envelope per
//! tick.
//!
//! **`--json` carries `data.steps`, and `data.lines` stays exactly what it
//! was.** `lines` is the raw records, byte for byte (unchanged, and what a
//! `--follow` tick prints); `steps` is the same window PROJECTED — one object
//! per emitted content block, in the record's own order, `{ id, ts, kind,
//! text, error, clipped }` with `kind` in `thinking · say · tool · result ·
//! settled · user · other`. It is bounded by construction, never by the
//! journal: the `--tail` window, [`MAX_BLOCKS_PER_RECORD`] blocks of any one
//! record, [`MAX_STEPS`] steps of the whole projection (`stepsOmitted` names
//! how many the last bound dropped), and `clip` characters per step.
//! `--clip detail` widens each block's text (the block's own line breaks kept,
//! up to `DETAIL_MAX_LINES`, a few hundred characters) and adds a `tool_use`'s
//! arguments; `line` (the default, and the only spelling the human body and
//! `--follow` use) is the terminal's own one-line clip.
//!
//! **A session with no trace is a TAUGHT error, never an empty listing** —
//! three separate reasons, each named: the harness keeps no trace at all
//! (`TranscriptSpec::trace` is `None`), the presence metadata names none or
//! the file is gone (`locate` fell back to `meta.json`, which the trace
//! reader refuses), or the agent has no registered profile here.

use super::common::{require_args, stage_error};
use super::model::{load_stage, resolved_parent, sessions_path, SessionRecord, SessionsFile};
use aoide_protocol::agents::{agent_profile, eidolon_trace_record, TraceRecord};
use aoide_protocol::output::Outcome;
use aoide_protocol::Door;
use aoide_protocol::Invocation;
use aoide_storage::addr::{self, LocalCandidate, Resolution};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Records shown when `--tail` is absent — enough to see the last few turns
/// without a whole session's history scrolling past.
const DEFAULT_TAIL: usize = 50;
/// How long `--follow` waits between re-reads of the trace.
const FOLLOW_POLL: Duration = Duration::from_millis(500);
/// An `AssistantMessage`'s thinking block, clipped.
const THINKING_MAX: usize = 80;
/// An `AssistantMessage`'s text block, and every other one-line field, clipped.
const TEXT_MAX: usize = 120;
/// How much of a thinking/text block, or of a tool result, [`Clip::Detail`]
/// keeps — a few hundred characters per step, so emitted reasoning is USABLE
/// in a details view instead of cut at [`THINKING_MAX`] everywhere.
const DETAIL_THINKING_MAX: usize = 600;
/// [`Clip::Detail`]'s text-block window (same reasoning as above).
const DETAIL_TEXT_MAX: usize = 600;
/// [`Clip::Detail`]'s tool-result window — the result's own first lines, which
/// is the "output" a reader is looking for.
const DETAIL_RESULT_MAX: usize = 600;
/// [`Clip::Detail`]'s one-line rendering of a `tool_use`'s arguments — the
/// name alone is all [`Clip::Line`] shows.
const DETAIL_ARGS_MAX: usize = 240;
/// Lines of its OWN text a [`Clip::Detail`] step keeps. `Clip::Line` flattens
/// whitespace to one line; Detail keeps the line breaks (a thinking block's
/// own paragraphing is part of what it says) up to this many.
const DETAIL_MAX_LINES: usize = 12;
/// Steps ONE record may project. A record's `content` array is the producer's
/// to size, so a single hostile or pathological record can carry thousands of
/// blocks; the projection is bounded here, and what it left out is NAMED
/// (`clipped: true`) rather than silently dropped.
const MAX_BLOCKS_PER_RECORD: usize = 32;
/// Steps a WHOLE projection may carry, whatever `--tail` and the per-record
/// bound allow (`--tail N` × blocks is still a lot of text). The newest steps
/// are kept — the same end of the window `--tail` shows — and the count left
/// out is reported (`stepsOmitted`), never silently dropped.
const MAX_STEPS: usize = 200;

/// How much of each emitted block a projection keeps.
///
/// Two named clips, deliberately not one truncation everywhere: [`Clip::Line`]
/// is the terminal/roster spelling (a record is one line there), and
/// [`Clip::Detail`] is a details view's — the same steps with each block's text
/// kept whole enough to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::graph) enum Clip {
    /// One flat line per block, at [`THINKING_MAX`]/[`TEXT_MAX`].
    Line,
    /// The block's own line breaks (up to [`DETAIL_MAX_LINES`]) and a few
    /// hundred characters, plus a `tool_use`'s arguments.
    Detail,
}

impl Clip {
    /// The wire/flag word for this clip — the ONE spelling `--clip`, the
    /// shellbridge's `clip` field and the JSON's `clip` key all carry.
    pub(in crate::graph) fn name(self) -> &'static str {
        match self {
            Self::Line => "line",
            Self::Detail => "detail",
        }
    }

    /// The clip a `--clip`/wire word names, or `None` for anything else — an
    /// unknown word is refused by the caller, never silently widened to Detail.
    pub(in crate::graph) fn parse(word: &str) -> Option<Self> {
        match word.trim() {
            "line" => Some(Self::Line),
            "detail" => Some(Self::Detail),
            _ => None,
        }
    }

    fn thinking_max(self) -> usize {
        match self {
            Self::Line => THINKING_MAX,
            Self::Detail => DETAIL_THINKING_MAX,
        }
    }

    fn text_max(self) -> usize {
        match self {
            Self::Line => TEXT_MAX,
            Self::Detail => DETAIL_TEXT_MAX,
        }
    }

    fn result_max(self) -> usize {
        match self {
            Self::Line => TEXT_MAX,
            Self::Detail => DETAIL_RESULT_MAX,
        }
    }
}

/// One emitted block of one record, projected: the step grammar
/// `session trace --json`'s `data.steps` publishes and [`render_line`] renders
/// from, so the machine projection and the terminal line cannot drift.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::graph) struct Step {
    /// The record's own journal id (`#<id>`'s `<id>`), never a new counter.
    pub id: String,
    /// The record's own `ts_ms` (epoch milliseconds), `None` when it carried
    /// no readable one — never a reading time substituted for it.
    pub ts_ms: Option<i64>,
    /// `thinking` · `say` · `tool` · `result` · `settled` · `user` · `other`.
    /// A closed set: an unknown record kind is `other`, never rendered as
    /// nothing.
    pub kind: &'static str,
    /// The block's own text, clipped per the requested [`Clip`] and with no
    /// decoration — the `→ `/`! `/dim spelling is the renderer's and the
    /// widget's, never baked into the data.
    pub text: String,
    /// A `ToolResult` whose `is_error` is true. False for every other kind.
    pub error: bool,
    /// The text is a KEPT PREFIX of what the record carried: the clip cut it,
    /// or the record carried more blocks than [`MAX_BLOCKS_PER_RECORD`] and
    /// this step is the note saying so. The honest truncation indicator — a
    /// reader is never left thinking it saw the whole block.
    pub clipped: bool,
}

impl Step {
    /// This step's own JSON object — `{ id, ts, kind, text, error, clipped }`,
    /// the shape `data.steps` carries.
    fn json(&self) -> Value {
        json!({
            "id": self.id,
            "ts": self.ts_ms,
            "kind": self.kind,
            "text": self.text,
            "error": self.error,
            "clipped": self.clipped,
        })
    }
}

static INTERRUPTED: AtomicBool = AtomicBool::new(false);

extern "C" fn on_sigint(_signum: libc::c_int) {
    INTERRUPTED.store(true, Ordering::SeqCst);
}

/// Clip to `max` chars at a char boundary with a trailing ellipsis, reporting
/// whether anything was cut — the honest indicator [`Step::clipped`] carries.
/// Same arithmetic as [`one_line_clip`], which delegates here.
fn clip_chars(s: &str, max: usize) -> (String, bool) {
    if s.chars().count() <= max {
        (s.to_string(), false)
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        (out, true)
    }
}

/// [`one_line_clip`] with the cut reported.
fn one_line_clipped(s: &str, max: usize) -> (String, bool) {
    let flat = s.split_whitespace().collect::<Vec<_>>().join(" ");
    clip_chars(&flat, max)
}

/// One whitespace-flattened line, clipped at a char boundary with a trailing
/// ellipsis. A local copy of the same three-line helper every renderer in
/// this crate carries (`protocol/src/agents.rs`, `graph/codex_capture.rs`) —
/// the SHARED copy is private to `protocol/agents`, and this is display-only
/// text, not a second reading of any record.
pub(in crate::graph) fn one_line_clip(s: &str, max: usize) -> String {
    one_line_clipped(s, max).0
}

/// [`Clip::Detail`]'s clip: the text's OWN line breaks kept (up to
/// [`DETAIL_MAX_LINES`] — a thinking block's paragraphing is part of what it
/// says), then a character window of `max`, with the cut reported. The line
/// bound comes first so a pathological one-character-per-line block cannot
/// paint a thousand-line step.
fn detail_clipped(s: &str, max: usize) -> (String, bool) {
    let trimmed = s.trim_end();
    let (kept, cut_lines) = if trimmed.lines().count() > DETAIL_MAX_LINES {
        (trimmed.lines().take(DETAIL_MAX_LINES).collect::<Vec<_>>().join("\n"), true)
    } else {
        (trimmed.to_string(), false)
    };
    let (out, cut_chars) = clip_chars(&kept, max);
    (out, cut_lines || cut_chars)
}

/// One block's own text, clipped per `clip`.
fn clip_block(text: &str, clip: Clip, max: usize) -> (String, bool) {
    match clip {
        Clip::Line => one_line_clipped(text, max),
        Clip::Detail => detail_clipped(text, max),
    }
}

/// Clip to `max` chars at a char boundary with a trailing ellipsis, KEEPING
/// the text's own internal whitespace — the tool-result first line, where a
/// tab or a column of spaces is part of what the result printed.
pub(in crate::graph) fn clip_keep_ws(s: &str, max: usize) -> String {
    clip_chars(s, max).0
}

/// One record's own timestamp as `hh:mm:ss` in LOCAL time — libc's
/// `localtime_r` over `ts_ms / 1000` (eidolon's clock at append, epoch
/// milliseconds). A record with no readable `ts_ms`, or a timestamp
/// `localtime_r` refuses, renders `--:--:--` rather than a guess. Local, not
/// UTC: the reader is a human comparing this against their own clock.
fn hh_mm_ss_local(ts_ms: Option<i64>) -> String {
    const UNKNOWN: &str = "--:--:--";
    let Some(ms) = ts_ms else {
        return UNKNOWN.to_string();
    };
    let secs = ms.div_euclid(1000) as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&secs, &mut tm) }.is_null() {
        return UNKNOWN.to_string();
    }
    format!("{:02}:{:02}:{:02}", tm.tm_hour, tm.tm_min, tm.tm_sec)
}

/// The content blocks of an `AssistantMessage`/`UserMessage` payload; an
/// absent or non-array `content` reads as no blocks.
fn content_blocks(payload: Option<&Value>) -> &[Value] {
    const NONE: &[Value] = &[];
    payload
        .and_then(|p| p.get("content"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(NONE)
}

/// A payload string field, one-lined and clipped; empty when absent/blank.
fn field(payload: Option<&Value>, key: &str, max: usize) -> String {
    payload
        .and_then(|p| p.get(key))
        .and_then(Value::as_str)
        .map(|s| one_line_clip(s, max))
        .unwrap_or_default()
}

/// `AssistantMessage`: one step per non-empty content block, in the record's
/// OWN order — thinking, then text, then one per `tool_use` (the live sample
/// holds all three in one record). An empty block contributes nothing (never
/// an empty step). [`Clip::Detail`] additionally carries a `tool_use`'s
/// arguments, one line — that is the "what did it reach for" a details view is
/// looking at; [`Clip::Line`] stays the tool's NAME alone, its existing
/// spelling.
fn assistant_steps(record: &TraceRecord, clip: Clip) -> Vec<Step> {
    let p = record.payload.as_ref();
    let step = |kind: &'static str, text: String, clipped: bool| Step {
        id: record.id.clone(),
        ts_ms: record.ts_ms,
        kind,
        text,
        error: false,
        clipped,
    };
    let blocks = content_blocks(p);
    let mut steps: Vec<Step> = Vec::new();
    for block in blocks.iter().take(MAX_BLOCKS_PER_RECORD) {
        match block.get("type").and_then(Value::as_str) {
            Some("thinking") => {
                let raw = block.get("thinking").and_then(Value::as_str).unwrap_or("");
                let (text, clipped) = clip_block(raw, clip, clip.thinking_max());
                if !text.is_empty() {
                    steps.push(step("thinking", text, clipped));
                }
            }
            Some("text") => {
                let raw = block.get("text").and_then(Value::as_str).unwrap_or("");
                let (text, clipped) = clip_block(raw, clip, clip.text_max());
                if !text.is_empty() {
                    steps.push(step("say", text, clipped));
                }
            }
            Some("tool_use") => {
                let (name, name_clipped) = clip_block(
                    block.get("name").and_then(Value::as_str).unwrap_or(""),
                    Clip::Line,
                    TEXT_MAX,
                );
                if name.is_empty() {
                    continue;
                }
                let (text, clipped) = match clip {
                    Clip::Line => (name, name_clipped),
                    // `input` serialized compact (serde_json emits no
                    // newlines), clipped — a hostile payload cannot make one
                    // step unbounded.
                    Clip::Detail => match block.get("input") {
                        Some(v) if !v.is_null() && v != &json!({}) => {
                            let (args, cut) = clip_chars(&v.to_string(), DETAIL_ARGS_MAX);
                            (format!("{name} {args}"), name_clipped || cut)
                        }
                        _ => (name, name_clipped),
                    },
                };
                steps.push(step("tool", text, clipped));
            }
            _ => {}
        }
    }
    if blocks.len() > MAX_BLOCKS_PER_RECORD {
        steps.push(step(
            "other",
            format!(
                "+{} more block(s) in this record, never projected",
                blocks.len() - MAX_BLOCKS_PER_RECORD
            ),
            true,
        ));
    }
    steps
}

/// `UserMessage`: one step per non-empty `text` block, in order.
fn user_steps(record: &TraceRecord, clip: Clip) -> Vec<Step> {
    let mut steps: Vec<Step> = Vec::new();
    for block in content_blocks(record.payload.as_ref())
        .iter()
        .take(MAX_BLOCKS_PER_RECORD)
    {
        if block.get("type").and_then(Value::as_str) != Some("text") {
            continue;
        }
        let raw = block.get("text").and_then(Value::as_str).unwrap_or("");
        let (text, clipped) = clip_block(raw, clip, clip.text_max());
        if !text.is_empty() {
            steps.push(Step {
                id: record.id.clone(),
                ts_ms: record.ts_ms,
                kind: "user",
                text,
                error: false,
                clipped,
            });
        }
    }
    steps
}

/// A `ToolResult`'s own content: a plain string, or the first `text` of a
/// block array (a shape nothing forbids).
fn tool_result_raw(payload: Option<&Value>) -> String {
    let Some(p) = payload else {
        return String::new();
    };
    match p.get("content") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(_)) => content_blocks(Some(p))
            .iter()
            .find_map(|b| b.get("text").and_then(Value::as_str))
            .unwrap_or("")
            .to_string(),
        _ => String::new(),
    }
}

/// `ToolResult`: the result's own output, clipped per `clip` — [`Clip::Line`]
/// keeps its FIRST line only (the terminal's one-line spelling, unchanged),
/// [`Clip::Detail`] keeps the first [`DETAIL_MAX_LINES`] lines of it.
fn tool_result_text(payload: Option<&Value>, clip: Clip) -> (String, bool) {
    match clip {
        Clip::Detail => detail_clipped(&tool_result_raw(payload), clip.result_max()),
        Clip::Line => {
            let raw = tool_result_raw(payload);
            let first = raw.lines().next().unwrap_or("").trim();
            if first.is_empty() {
                (String::new(), false)
            } else {
                clip_chars(first, clip.result_max())
            }
        }
    }
}

/// `ToolResult`: the result's FIRST line cut to [`TEXT_MAX`], prefixed `!`
/// when `is_error`. `content` is a string in eidolon's own sample shape; an
/// array of blocks (a shape nothing forbids) falls back to its first `text`.
pub(in crate::graph) fn tool_result_summary(payload: Option<&Value>, prefix: &str) -> String {
    let raw = tool_result_raw(payload);
    let first = raw.lines().next().unwrap_or("").trim();
    if first.is_empty() {
        return String::new();
    }
    format!("{prefix}{}", clip_keep_ws(first, TEXT_MAX))
}

/// `TurnSettled`: the stop reason and the turn's input/output tokens, each
/// omitted when the record does not carry it.
fn settled_summary(payload: Option<&Value>) -> String {
    let stop = field(payload, "stop_reason", TEXT_MAX);
    let usage = payload.and_then(|p| p.get("usage"));
    let num = |key: &str| {
        usage
            .and_then(|u| u.get(key))
            .and_then(Value::as_i64)
            .map(|n| n.to_string())
    };
    let mut parts: Vec<String> = Vec::new();
    if !stop.is_empty() {
        parts.push(stop);
    }
    if let (Some(i), Some(o)) = (num("input_tokens"), num("output_tokens")) {
        parts.push(format!("in {i} out {o}"));
    } else if let Some(i) = num("input_tokens") {
        parts.push(format!("in {i}"));
    } else if let Some(o) = num("output_tokens") {
        parts.push(format!("out {o}"));
    }
    parts.join(" · ")
}

/// `AskUser`: the prompt, plus whether it is still open (`answer: null`) or
/// was answered — the one salient field the state rule also reads.
fn ask_summary(payload: Option<&Value>) -> String {
    let prompt = field(payload, "prompt", TEXT_MAX);
    let answered = payload
        .and_then(|p| p.get("answer"))
        .map(|a| !a.is_null())
        .unwrap_or(false);
    let verdict = if answered { "(answered)" } else { "(open)" };
    if prompt.is_empty() {
        verdict.to_string()
    } else {
        format!("{prompt} {verdict}")
    }
}

/// `ExternalMessage`: who steered it and what they said.
fn external_summary(payload: Option<&Value>) -> String {
    let from = field(payload, "from", TEXT_MAX);
    let text = field(payload, "text", TEXT_MAX);
    match (from.is_empty(), text.is_empty()) {
        (false, false) => format!("{from}: {text}"),
        (false, true) => from,
        (true, false) => text,
        (true, true) => String::new(),
    }
}

/// The one-line summary a NON-message record renders after its kind — the
/// [`Clip::Line`] spelling of its one salient field, and whether rendering it
/// had to CUT that field. `AssistantMessage` / `UserMessage` / `ToolResult`
/// never reach here: their text comes from the projector ([`steps_of`]), which
/// is what keeps this line and `data.steps` from drifting. Kinds the design doc
/// names get their own arm; everything else (a `SessionStart`, an `AskUser`, a
/// newer eidolon's variant Aoide has never heard of) degrades to its own salient
/// field where one is known and to the compact payload otherwise — never to
/// nothing, so an unfamiliar record is still visible rather than silently blank.
///
/// The cut flag is decided from the FIELDS each arm reads (not from the
/// rendered text): `AskUser` appends `(open)` after its prompt and
/// `ExternalMessage`/`TurnSettled` compose two fields, so a text that merely
/// "ends in an ellipsis" would miss a cut in the middle — the flag must be
/// exact, because a machine consumer reads it.
fn summary_text(record: &TraceRecord) -> (String, bool) {
    let p = record.payload.as_ref();
    // Would `one_line_clip(field, TEXT_MAX)` cut this payload's own string?
    let cut = |key: &str| -> bool {
        p.and_then(|p| p.get(key))
            .and_then(Value::as_str)
            .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" ").chars().count() > TEXT_MAX)
            .unwrap_or(false)
    };
    match record.kind.as_str() {
        // `Cancelled`/`TurnDeadline` ride the same arm shape as the rest:
        // a unit variant carries no payload, so it renders the kind alone.
        "TurnSettled" => (settled_summary(p), cut("stop_reason")),
        "AskUser" => (ask_summary(p), cut("prompt")),
        "ExternalMessage" => (external_summary(p), cut("from") || cut("text")),
        "TurnBudget" => (numeric_field(p, "calls_left"), cut("calls_left")),
        "TurnDeadline" => (numeric_field(p, "secs_left"), cut("secs_left")),
        "ContextSize" => (numeric_field(p, "tokens"), cut("tokens")),
        "PolicyVerdict" => (field(p, "outcome", TEXT_MAX), cut("outcome")),
        "SessionStart" | "ModelChanged" => {
            (prefixed_field(p, "model", "model=", TEXT_MAX), cut("model"))
        }
        // An unfamiliar kind: the compact payload itself, clipped — and the
        // flag comes from the SAME clip, never an assumption.
        _ => match p {
            Some(p) => one_line_clipped(&p.to_string(), TEXT_MAX),
            None => (String::new(), false),
        },
    }
}

/// The step `kind` a non-message record's single step carries: the kinds the
/// design doc names get their own; everything else is `other`.
fn other_kind(record_kind: &str) -> &'static str {
    match record_kind {
        "TurnSettled" => "settled",
        _ => "other",
    }
}

/// Project ONE record into the [`Step`]s it emits, in the record's own order:
/// one step per non-empty content block of an `AssistantMessage`/`UserMessage`
/// (a record holding thinking + text + tool_use yields three), one step for a
/// `ToolResult`'s own output, one for a settled turn or any other record's own
/// salient text, and NONE for a record that says nothing at all (a bare
/// `Cancelled`). **A `Vec`, never one `Option<Step>`:** a single-step
/// projector would silently drop all but one block of a multi-block message.
///
/// Bounded by construction, whatever the journal holds: at most
/// [`MAX_BLOCKS_PER_RECORD`] steps from one record (plus the note naming what
/// that left out), each clipped to `clip`, each cut NAMED in
/// [`Step::clipped`]. The window itself is the caller's (`--tail`).
fn steps_of(record: &TraceRecord, clip: Clip) -> Vec<Step> {
    match record.kind.as_str() {
        "AssistantMessage" => assistant_steps(record, clip),
        "UserMessage" => user_steps(record, clip),
        "ToolResult" => {
            let error = record
                .payload
                .as_ref()
                .and_then(|p| p.get("is_error"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let (text, clipped) = tool_result_text(record.payload.as_ref(), clip);
            if text.is_empty() {
                Vec::new()
            } else {
                vec![Step {
                    id: record.id.clone(),
                    ts_ms: record.ts_ms,
                    kind: "result",
                    text,
                    error,
                    clipped,
                }]
            }
        }
        _ => {
            let (text, clipped) = summary_text(record);
            if text.is_empty() {
                Vec::new()
            } else {
                vec![Step {
                    id: record.id.clone(),
                    ts_ms: record.ts_ms,
                    kind: other_kind(&record.kind),
                    text,
                    error: false,
                    clipped,
                }]
            }
        }
    }
}

/// Every step a RENDER WINDOW of raw trace lines projects, newest end kept and
/// the count dropped reported — the ONE place a whole window becomes steps, so
/// the JSON and the renderer cannot disagree about a record's blocks. A line
/// that is not a readable record contributes no step (it still gets its own `?`
/// line in the human render, and its raw line in `data.lines`).
pub(in crate::graph) fn steps_of_lines(lines: &[&String], clip: Clip) -> (Vec<Step>, usize) {
    let mut steps: Vec<Step> = Vec::new();
    for line in lines {
        if let Some(record) = eidolon_trace_record(line) {
            steps.extend(steps_of(&record, clip));
        }
    }
    let omitted = steps.len().saturating_sub(MAX_STEPS);
    if omitted > 0 {
        steps.drain(0..omitted);
    }
    (steps, omitted)
}

/// The one-line spelling of a step list: the per-kind decoration a human reads
/// (`→ ` on a tool call, `! ` on an errored result, the terminal's dim wrap
/// around thinking) applied at RENDER time, never baked into the data. An
/// empty list renders an empty string, so the caller omits the column rather
/// than printing a blank one.
fn render_steps(steps: &[Step], dim: bool) -> String {
    let parts: Vec<String> = steps
        .iter()
        .map(|s| match s.kind {
            "tool" if !s.text.is_empty() => format!("→ {}", s.text),
            "result" if s.error => format!("! {}", s.text),
            "thinking" if dim => format!("\u{1b}[2m{}\u{1b}[0m", s.text),
            _ => s.text.clone(),
        })
        .collect();
    parts.join("  ")
}


/// `key=value` for one payload field, whichever of the two spellings the
/// journal used — the sample contract writes numbers, and nothing about the
/// shape promises it stays one. Empty (the whole column omitted) when the
/// record carries no such field.
fn numeric_field(payload: Option<&Value>, key: &str) -> String {
    let Some(v) = payload.and_then(|p| p.get(key)) else {
        return String::new();
    };
    match v {
        Value::Number(n) => format!("{key}={n}"),
        Value::String(s) if !s.trim().is_empty() => format!("{key}={}", one_line_clip(s, TEXT_MAX)),
        _ => String::new(),
    }
}

/// `prefix<value>` for one payload string field, the whole column omitted
/// when the field is absent or blank — never a dangling `model=`.
fn prefixed_field(payload: Option<&Value>, key: &str, prefix: &str, max: usize) -> String {
    let v = field(payload, key, max);
    if v.is_empty() {
        String::new()
    } else {
        format!("{prefix}{v}")
    }
}

/// One record as the human line the design doc fixes:
/// `#<id>  <hh:mm:ss>  <kind>  <summary>` — the summary omitted (never a
/// trailing blank column) when the record has nothing to say. The summary is
/// the record's [`Clip::Line`] steps RENDERED, never a second reading of the
/// payload: the terminal line and `data.steps` come from one projector.
fn render_line(record: &TraceRecord, dim: bool) -> String {
    let mut line = format!(
        "#{}  {}  {}",
        record.id,
        hh_mm_ss_local(record.ts_ms),
        record.kind
    );
    let s = render_steps(&steps_of(record, Clip::Line), dim);
    if !s.is_empty() {
        line.push_str("  ");
        line.push_str(&s);
    }
    line
}

/// A tail line that is NOT a readable record — a torn write, a hand-edit, a
/// line from a newer eidolon whose shape Aoide cannot name — still gets a
/// line, flagged `?`, rather than being dropped silently.
fn render_unparsed(line: &str) -> String {
    format!("#?  --:--:--  ?  {}", one_line_clip(line, TEXT_MAX))
}

/// Render one line: the parsed record's own rendering, or the `?` line.
fn render_any(line: &str, dim: bool) -> String {
    match eidolon_trace_record(line) {
        Some(record) => render_line(&record, dim),
        None => render_unparsed(line),
    }
}

/// The last `tail` lines of a trace, in file order — the render window.
fn last_lines<'a>(lines: &'a [String], tail: usize) -> Vec<&'a String> {
    lines.iter().rev().take(tail).rev().collect()
}

/// `--tail N` — a positive integer, or the default. A value the flag did not
/// carry (`--tail` with no token, which the parser stores as `"true"`) or a
/// `0` is a taught usage error, never a silently empty listing.
fn parse_tail(inv: &Invocation) -> Result<usize, Outcome> {
    let Some(raw) = inv.flags.get("tail") else {
        return Ok(DEFAULT_TAIL);
    };
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(DEFAULT_TAIL);
    }
    match raw.parse::<usize>() {
        Ok(n) if n >= 1 => Ok(n),
        _ => Err(Outcome::usage(
            "session.trace",
            format!(
                "`{raw}` is not a record count — --tail takes a positive integer (e.g. --tail 20); \
                 omit it for the last {DEFAULT_TAIL} records"
            ),
        )
        .with_data(json!({ "reason": "bad-tail", "tail": raw }))),
    }
}

/// `--clip line|detail` — how much of each emitted block `data.steps` carries.
/// Absent is [`Clip::Line`] (the terminal spelling). An unknown word is a
/// taught usage error, never a silent widening to the wider clip: a caller that
/// meant `detail` and typed `detials` must not be handed a bounded-to-line
/// answer as if it were a full one.
fn parse_clip(inv: &Invocation) -> Result<Clip, Outcome> {
    let Some(raw) = inv.flags.get("clip") else {
        return Ok(Clip::Line);
    };
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(Clip::Line);
    }
    Clip::parse(raw).ok_or_else(|| {
        Outcome::usage(
            "session.trace",
            format!(
                "`{raw}` is not a clip — --clip takes `line` (the terminal's one-line spelling) \
                 or `detail` (each block's own line breaks, kept longer); omit it for `line`"
            ),
        )
        .with_data(json!({ "reason": "bad-clip", "clip": raw }))
    })
}

/// The session's name for the message line, through the canonical display
/// grammar — the same render `session pending list` gives a target, falling
/// back to the raw id when no record carries it.
fn label(id: &str, records: &[SessionRecord], ids: &HashSet<&str>, host: &str) -> String {
    match records.iter().find(|r| r.session_id == id) {
        Some(rec) => {
            let role = if resolved_parent(rec, ids).is_some() { "child" } else { "root" };
            aoide_storage::display::session_label(rec, host, role)
        }
        None => id.to_string(),
    }
}

/// Is stdout a terminal? Dimming an `AssistantMessage`'s thinking is a
/// display nicety; a redirected/piped read gets plain text, the same
/// discipline `aoide_protocol::pick` holds for its own ANSI.
fn stdout_is_terminal() -> bool {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}

/// `session trace <id> [--tail N] [--follow] [--json]` — see the module doc.
pub fn session_trace(inv: &Invocation) -> Outcome {
    let cmd = "session.trace";
    let args = match require_args(inv, &["id"]) {
        Ok(a) => a,
        Err(o) => return o,
    };
    let target = args[0].trim().to_string();
    let tail = match parse_tail(inv) {
        Ok(n) => n,
        Err(o) => return o,
    };
    let clip = match parse_clip(inv) {
        Ok(c) => c,
        Err(o) => return o,
    };
    let json_mode = inv.flag_present("json");
    let follow = inv.flag_present("follow");
    if follow && inv.door != Door::Cli {
        return Outcome::usage(
            cmd,
            "--follow blocks until Ctrl-C; run it from a terminal (not over this door)",
        );
    }

    // `<id>` resolves exactly like `send --to`: the ONE
    // `aoide_storage::addr::resolve`, over this box's live roster plus its
    // registered node names, with the same ambiguity/not-found
    // refusals. No hub preference here — that is a ROUTING rule for `send`
    // (P-D5), not a rule for reading a file that lives on THIS box.
    let file: SessionsFile = match load_stage(&sessions_path()) {
        Ok(f) => f,
        Err(e) => return stage_error(cmd, e),
    };
    let host = aoide_storage::display::local_host_name();
    let ids: HashSet<&str> = file.sessions.iter().map(|s| s.session_id.as_str()).collect();
    let candidates: Vec<LocalCandidate<'_>> = file
        .sessions
        .iter()
        .map(|s| {
            let role = if resolved_parent(s, &ids).is_some() { "child" } else { "root" };
            LocalCandidate { session_id: &s.session_id, petname: s.petname.as_deref(), role }
        })
        .collect();
    let nodes = aoide_storage::node_store::load_nodes();
    let node_names: Vec<&str> = nodes.iter().map(|p| p.name.as_str()).collect();

    let id = match addr::resolve(&target, &host, &candidates, &node_names) {
        Resolution::Local(id) => id,
        Resolution::Remote { node, .. } => {
            return Outcome::error(
                cmd,
                format!(
                    "`{target}` resolves to a session on node `{node}` — a trace is a file on the \
                     node that wrote it, so run `session trace` there"
                ),
            )
            .with_data(json!({ "reason": "remote-target", "node": node, "target": target }));
        }
        Resolution::Ambiguous(found) => {
            return Outcome::error(
                cmd,
                format!(
                    "`{target}` is ambiguous — {} local session(s) match: {}",
                    found.len(),
                    found.join(", ")
                ),
            )
            .with_data(json!({ "reason": "ambiguous", "target": target, "candidates": found }));
        }
        Resolution::NotFound => {
            let hint = if !target.contains('/') && node_names.contains(&target.as_str()) {
                format!(
                    " (`{target}` names a known node, not a local session — did you mean `{target}/<session>`?)"
                )
            } else {
                String::new()
            };
            return Outcome::error(cmd, format!("no session matches `{target}`{hint}"))
                .with_data(json!({ "reason": "not-found", "target": target }));
        }
    };

    // The trace is a HARNESS CAPABILITY, never a name check: a profile whose
    // `TranscriptSpec::trace` is `None` keeps no trace file at all, and
    // saying so is the honest answer (the capability test is why no consumer
    // here writes `if agent == "eidolon"`).
    let name = label(&id, &file.sessions, &ids, &host);
    let agent = file
        .sessions
        .iter()
        .find(|r| r.session_id == id)
        .map(|r| r.agent.clone())
        .unwrap_or_default();
    let Some(profile) = agent_profile(&agent) else {
        return Outcome::error(
            cmd,
            format!("`{name}` runs `{agent}`, which has no registered agent profile here — no trace reader to reach"),
        )
        .with_data(json!({ "reason": "unknown-agent", "sessionId": id, "agent": agent }));
    };
    let Some(read_trace) = profile.transcript.trace else {
        return Outcome::error(
            cmd,
            format!(
                "`{name}` runs `{agent}`, which keeps no trace — a trace is one JSON record per \
                 journal record, published by the harness itself (a mirror file, or its own \
                 read-only export door), and resolved through the record's presence metadata; \
                 `eidolon` is the one harness that does today (docs/architecture/EIDOLON-TRACE.md)"
            ),
        )
        .with_data(json!({
            "reason": "no-trace-capability",
            "sessionId": id,
            "agent": agent,
        }));
    };

    // The PATH is the same locator every other transcript reader uses: it
    // prefers the file the presence metadata names, and falls back to the
    // metadata itself when there is none.
    let cwd = file
        .sessions
        .iter()
        .find(|r| r.session_id == id)
        .map(|r| r.cwd.clone())
        .unwrap_or_default();
    let Some(path) = (profile.transcript.locate)(&id, Some(&cwd), None) else {
        return Outcome::error(
            cmd,
            format!(
                "`{name}` has no readable presence metadata — looked for \
                 $XDG_RUNTIME_DIR/eidolon/{id}/meta.json (a torn-down or never-registered presence)"
            ),
        )
        .with_data(json!({ "reason": "no-presence", "sessionId": id }));
    };
    let Some(lines) = read_trace(&path) else {
        return Outcome::error(
            cmd,
            format!(
                "`{name}` has no readable trace at {} — nothing on this host resolves one for it: \
                 a producer that publishes its journal as records (a mirror file, or its own \
                 read-only export door) and a presence that names it",
                path.display()
            ),
        )
        .with_data(json!({
            "reason": "no-trace",
            "sessionId": id,
            "presence": path.to_string_lossy(),
        }));
    };

    if follow {
        return follow_trace(cmd, &id, &name, &path, read_trace, tail, json_mode);
    }

    let shown = last_lines(&lines, tail);
    let dim = stdout_is_terminal();
    let raw: Vec<String> = shown.iter().map(|l| (*l).clone()).collect();
    let body: Vec<String> = shown.iter().map(|l| render_any(l, dim)).collect();
    let mut message = format!("{} record(s) · {name}", body.len());
    if !json_mode && !body.is_empty() {
        message.push('\n');
        message.push_str(&body.join("\n"));
    }
    // The step projection is the SAME projector the human body renders from —
    // one step per emitted block, in record order, each clipped to `--clip`
    // (the human body stays the one-line `Clip::Line` spelling), bounded by
    // the window, by blocks-per-record and by a whole-projection cap.
    let (steps, omitted) = steps_of_lines(&shown, clip);
    let steps: Vec<Value> = steps.iter().map(Step::json).collect();
    Outcome::ok(cmd, message)
        .with_data(json!({
            "sessionId": id,
            "trace": path.to_string_lossy(),
            "tail": tail,
            "records": body.len(),
            "lines": raw,
            "clip": clip.name(),
            "steps": steps,
            "stepsOmitted": omitted,
        }))
}

/// `--follow`: print the current window, then re-read the trace every
/// [`FOLLOW_POLL`] and print whatever grew, until Ctrl-C. Re-reading the
/// tail each tick (rather than holding a delta reader open) keeps this on
/// the SAME reader as the one-shot path — the file is a bounded 1 MiB window,
/// and a trace that was rotated or truncated simply yields a shorter list,
/// which this loop notices by count rather than replaying history.
fn follow_trace(
    cmd: &str,
    id: &str,
    name: &str,
    path: &Path,
    read_trace: fn(&Path) -> Option<Vec<String>>,
    tail: usize,
    json_mode: bool,
) -> Outcome {
    use std::io::Write;
    unsafe {
        libc::signal(libc::SIGINT, on_sigint as *const () as libc::sighandler_t);
    }
    let dim = stdout_is_terminal();
    let emit = |line: &str| {
        if json_mode {
            println!("{line}");
        } else {
            println!("{}", render_any(line, dim));
        }
        let _ = std::io::stdout().flush();
    };

    let mut seen = 0usize;
    let mut printed = 0usize;
    if let Some(lines) = read_trace(path) {
        let window = last_lines(&lines, tail);
        seen = lines.len();
        for line in window {
            emit(line);
            printed += 1;
        }
    }
    while !INTERRUPTED.load(Ordering::SeqCst) {
        std::thread::sleep(FOLLOW_POLL);
        if INTERRUPTED.load(Ordering::SeqCst) {
            break;
        }
        let Some(lines) = read_trace(path) else {
            continue; // the file is momentarily not there — keep waiting
        };
        if lines.len() < seen {
            // Rotated, truncated, or replaced: start counting from the new tail.
            seen = lines.len();
            continue;
        }
        for line in lines.iter().skip(seen) {
            emit(line);
            printed += 1;
        }
        seen = lines.len();
    }
    Outcome::ok(cmd, format!("followed {printed} record(s) · {name}"))
        .with_data(json!({
            "sessionId": id,
            "trace": path.to_string_lossy(),
            "followed": true,
            "printed": printed,
        }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::testutil::{unique_stage, EnvVars};
    use aoide_protocol::output::Status;
    use std::collections::BTreeMap;

    // The design doc's own sample lines, verbatim (EIDOLON-TRACE.md's "Line"
    // block) — the same fixtures `protocol/agents.rs` and `graph/eidolon.rs`
    // pin their own halves against.
    const START: &str = r#"{"id":0,"parent":null,"ts_ms":1789603005561,"kind":{"SessionStart":{"model":"ollama:deepseek-v4.1-flash","cwd":"/home/khoa/Aoide","system":null}}}"#;
    const USER: &str = r##"{"id":1,"parent":0,"ts_ms":1789603005570,"kind":{"UserMessage":{"role":"user","content":[{"type":"text","text":"# Brief A: …"}]}}}"##;
    const ASSISTANT: &str = r#"{"id":2,"parent":1,"ts_ms":1789603009102,"kind":{"AssistantMessage":{"role":"assistant","content":[{"type":"thinking","thinking":"I should read the slot catalog first, and then compare it against the facets directory to see which surfaces each song claims for itself.","signature":"…"},{"type":"text","text":"Let me read the slot catalog first."},{"type":"tool_use","id":"call_8vr43zri","name":"read","input":{"path":"modules/facets/quickshell/qml/slots.md"}}]}}}"#;
    const RESULT: &str = r#"{"id":3,"parent":2,"ts_ms":1789603009140,"kind":{"ToolResult":{"tool_use_id":"call_8vr43zri","content":"     1\t# Per-song widget slots\n     2\tcatalog\n","is_error":false}}}"#;
    const RESULT_ERR: &str = r#"{"id":4,"parent":3,"ts_ms":1789603009200,"kind":{"ToolResult":{"tool_use_id":"call_9","content":"ENOENT: no such file\nmore","is_error":true}}}"#;
    const SETTLED: &str = r#"{"id":131,"parent":130,"ts_ms":1789606421000,"kind":{"TurnSettled":{"stop_reason":"end_turn","usage":{"input_tokens":9570000,"output_tokens":71900,"cache_creation_input_tokens":0,"cache_read_input_tokens":9430000}}}}"#;
    const CANCELLED: &str = r#"{"id":77,"parent":76,"ts_ms":1789626990000,"kind":"Cancelled"}"#;
    const ASK: &str = r#"{"id":40,"parent":39,"ts_ms":1789626500000,"kind":{"AskUser":{"call_id":"call_x","prompt":"Overwrite?","answer":null}}}"#;
    const BUDGET: &str = r#"{"id":120,"parent":119,"ts_ms":1789606380000,"kind":{"TurnBudget":{"calls_left":8}}}"#;
    const EXTERNAL: &str = r#"{"id":55,"parent":54,"ts_ms":1789626700000,"kind":{"ExternalMessage":{"from":"orchestrator","channel":null,"text":"STOP: write the report now"}}}"#;

    fn trace_invocation(args: &[&str], flags: &[(&str, &str)]) -> Invocation {
        Invocation {
            path: vec!["session".into(), "trace".into()],
            args: args.iter().map(|s| s.to_string()).collect(),
            flags: flags.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<BTreeMap<_, _>>(),
            door: Door::Cli,
        }
    }

    /// A fake eidolon presence + trace on disk, plus the roster record that
    /// makes `session trace` find it: the locator resolves
    /// `$XDG_RUNTIME_DIR/eidolon/<id>/meta.json`, and that file names the
    /// trace. Returns the root the caller removes.
    fn fixture(tag: &str, id: &str, agent: &str, lines: &[&str], with_trace_key: bool) -> std::path::PathBuf {
        let root = unique_stage(tag);
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::set_var("XDG_RUNTIME_DIR", &root);

        let sessions = root.join("sessions");
        std::fs::create_dir_all(&sessions).unwrap();
        let trace = sessions.join(format!("{id}.jsonl"));
        // An empty slice writes an EMPTY file, not one blank line — the
        // difference between "a trace with no records yet" and "a trace
        // holding an unreadable line".
        let body = if lines.is_empty() {
            String::new()
        } else {
            format!("{}\n", lines.join("\n"))
        };
        std::fs::write(&trace, body).unwrap();

        let presence = root.join("eidolon").join(id);
        std::fs::create_dir_all(&presence).unwrap();
        let trace_field = if with_trace_key {
            format!(r#","trace":"{}""#, trace.display())
        } else {
            String::new()
        };
        std::fs::write(
            presence.join("meta.json"),
            format!(
                r#"{{"id":"{id}","pid":4242,"log":"{id}.eid","cwd":"/home/khoa","model":"claude-cli:opus","title":"ng","busy":false{trace_field}}}"#
            ),
        )
        .unwrap();

        std::fs::write(
            stage.join("sessions.json"),
            format!(
                r#"{{"schemaVersion":"0","sessions":[{{"sessionId":"{id}","agent":"{agent}","cwd":"/home/khoa","state":"working","startedAt":"2026-09-12T00:00:00Z","petname":"brave-otter"}}]}}"#
            ),
        )
        .unwrap();
        root
    }

    fn env_keys() -> [&'static str; 5] {
        [
            "AOIDE_STAGE_DIR",
            "AOIDE_STATE_DIR",
            "XDG_RUNTIME_DIR",
            "AOIDE_HOME",
            "HOME",
        ]
    }

    // ── the render, pure ─────────────────────────────────────────────────

    #[test]
    fn an_assistant_message_renders_thinking_text_and_every_tool_call() {
        let rec = eidolon_trace_record(ASSISTANT).expect("the doc's assistant line is a record");
        let line = render_line(&rec, false);
        assert!(line.starts_with("#2  "), "{line}");
        assert!(line.contains("AssistantMessage"), "{line}");
        assert!(
            line.contains("I should read the slot catalog first"),
            "the thinking block is shown: {line}"
        );
        assert!(line.contains("Let me read the slot catalog first."), "{line}");
        assert!(line.contains("→ read"), "a tool_use renders its name: {line}");
        // 80-char thinking / 120-char text: the doc's own thinking sample is
        // longer than the window, so it is clipped, not dropped.
        assert!(line.contains('…'), "the long thinking block is clipped: {line}");
        assert!(!line.contains('\u{1b}'), "undimmed render carries no escapes: {line}");
    }

    #[test]
    fn thinking_is_dimmed_only_when_the_renderer_is_told_to() {
        let rec = eidolon_trace_record(ASSISTANT).unwrap();
        let dim = render_line(&rec, true);
        assert!(dim.contains("\u{1b}[2m"), "{dim}");
        assert!(dim.contains("\u{1b}[0m"), "{dim}");
        // Everything else is unchanged by the flag.
        assert_eq!(
            dim.replace("\u{1b}[2m", "").replace("\u{1b}[0m", ""),
            render_line(&rec, false)
        );
    }

    #[test]
    fn a_tool_result_shows_its_first_line_and_flags_an_error() {
        let ok = render_line(&eidolon_trace_record(RESULT).unwrap(), false);
        assert!(ok.contains("ToolResult"), "{ok}");
        assert!(ok.contains("# Per-song widget slots"), "{ok}");
        assert!(!ok.contains("catalog"), "only the FIRST line is shown: {ok}");
        assert!(!ok.contains("! "), "{ok}");

        let err = render_line(&eidolon_trace_record(RESULT_ERR).unwrap(), false);
        assert!(err.contains("! ENOENT: no such file"), "{err}");
        assert!(!err.contains("more"), "the second line is cut: {err}");
    }

    #[test]
    fn the_remaining_kinds_each_show_their_one_salient_field() {
        let s = render_line(&eidolon_trace_record(SETTLED).unwrap(), false);
        assert!(s.contains("end_turn"), "{s}");
        assert!(s.contains("in 9570000 out 71900"), "{s}");

        let c = render_line(&eidolon_trace_record(CANCELLED).unwrap(), false);
        assert!(c.contains("Cancelled"), "{c}");
        assert!(c.ends_with("Cancelled"), "a unit variant renders its kind alone: {c}");

        let a = render_line(&eidolon_trace_record(ASK).unwrap(), false);
        assert!(a.contains("Overwrite? (open)"), "{a}");

        let b = render_line(&eidolon_trace_record(BUDGET).unwrap(), false);
        assert!(b.contains("calls_left=8"), "{b}");

        let e = render_line(&eidolon_trace_record(EXTERNAL).unwrap(), false);
        assert!(e.contains("orchestrator: STOP: write the report now"), "{e}");

        let u = render_line(&eidolon_trace_record(USER).unwrap(), false);
        assert!(u.contains("# Brief A: …"), "{u}");

        let st = render_line(&eidolon_trace_record(START).unwrap(), false);
        assert!(st.contains("model=ollama:deepseek-v4.1-flash"), "{st}");
    }

    #[test]
    fn a_kind_aoide_has_never_heard_of_renders_its_payload_never_nothing() {
        let rec = eidolon_trace_record(r#"{"id":9,"ts_ms":1,"kind":{"SomethingNew":{"x":1}}}"#).unwrap();
        let line = render_line(&rec, false);
        assert!(line.contains("SomethingNew"), "{line}");
        assert!(line.contains("\"x\":1"), "an unknown kind still shows its payload: {line}");
    }

    /// A non-message record whose own salient text had to be cut says so — the
    /// machine consumer must not be told "complete" while the reader sees an
    /// ellipsis (the fallback arm the review flagged).
    #[test]
    fn a_clipped_fallback_summary_reports_itself_as_clipped() {
        let long = "x".repeat(TEXT_MAX * 2);
        let rec = eidolon_trace_record(
            &json!({"id": 9, "ts_ms": 1, "kind": {"SomethingNew": {"blob": long}}}).to_string(),
        )
        .unwrap();
        let steps = steps_of(&rec, Clip::Line);
        assert_eq!(steps.len(), 1);
        assert!(steps[0].text.ends_with('…'), "{:?}", steps[0].text);
        assert!(steps[0].clipped, "a cut fallback summary must say so: {:?}", steps[0]);

        // A short one is not flagged, and an `AskUser` prompt over the window —
        // a named arm, through the same indicator — is.
        let short = eidolon_trace_record(r#"{"id":9,"ts_ms":1,"kind":{"SomethingNew":{"x":1}}}"#).unwrap();
        assert!(!steps_of(&short, Clip::Line)[0].clipped);
        let ask = eidolon_trace_record(
            &json!({"id": 9, "ts_ms": 1, "kind": {"AskUser": {"prompt": "y".repeat(TEXT_MAX * 2), "answer": null}}})
                .to_string(),
        )
        .unwrap();
        let ask_steps = steps_of(&ask, Clip::Line);
        assert_eq!(ask_steps[0].kind, "other");
        assert!(ask_steps[0].clipped, "{:?}", ask_steps[0]);
    }

    #[test]
    fn an_unreadable_line_renders_as_a_flagged_line_never_dropped() {
        let line = render_any("{ not json", false);
        assert!(line.starts_with("#?  --:--:--  ?"), "{line}");
        assert!(line.contains("not json"), "{line}");
        // A line with no usable ts_ms reads `--:--:--`, never an epoch guess.
        let rec = eidolon_trace_record(r#"{"id":5,"kind":{"ContextSize":{"tokens":134700}}}"#).unwrap();
        assert!(render_line(&rec, false).contains("--:--:--"));
    }

    // ── the command, end to end over a fake presence ──────────────────────

    #[test]
    fn trace_renders_the_last_records_of_a_real_trace_file() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&env_keys());
        let root = fixture(
            "trace-basic",
            "Aoide-7d23",
            "eidolon",
            &[START, USER, ASSISTANT, RESULT, BUDGET, SETTLED],
            true,
        );

        let out = session_trace(&trace_invocation(&["Aoide-7d23"], &[]));
        assert_eq!(out.status, Status::Ok, "msg: {}", out.message);
        let data = out.data.clone().unwrap();
        assert_eq!(data["sessionId"], "Aoide-7d23");
        assert_eq!(data["records"], 6, "the whole short trace fits under the default tail");
        assert!(data["trace"].as_str().unwrap().ends_with("Aoide-7d23.jsonl"));
        let lines = data["lines"].as_array().unwrap();
        assert_eq!(lines[0].as_str().unwrap(), START, "--json passes the raw lines through verbatim");
        assert!(out.message.contains("6 record(s)"), "{}", out.message);
        // The human body: one line per record, `#id  hh:mm:ss  kind  summary`.
        let body: Vec<&str> = out.message.lines().skip(1).collect();
        assert_eq!(body.len(), 6);
        assert!(body[0].starts_with("#0  ") && body[0].contains("SessionStart"), "{:?}", body[0]);
        assert!(body[2].contains("→ read"), "{:?}", body[2]);
        assert!(body[5].contains("end_turn"), "{:?}", body[5]);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn tail_narrows_to_the_last_n_records() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&env_keys());
        let root = fixture("trace-tail", "Aoide-tail", "eidolon", &[START, USER, ASSISTANT, SETTLED], true);

        let out = session_trace(&trace_invocation(&["Aoide-tail"], &[("tail", "2")]));
        assert_eq!(out.status, Status::Ok, "msg: {}", out.message);
        assert_eq!(out.data.clone().unwrap()["records"], 2);
        let lines = out.data.clone().unwrap()["lines"].as_array().unwrap().clone();
        assert_eq!(lines[0].as_str().unwrap(), ASSISTANT, "the last two, in file order");
        assert_eq!(lines[1].as_str().unwrap(), SETTLED);
        assert!(!out.message.contains("#0  "), "the older records are outside the window: {}", out.message);

        // A non-numeric / zero --tail is a taught usage error, never a silent
        // empty listing.
        for bad in ["nope", "0", "true"] {
            let out = session_trace(&trace_invocation(&["Aoide-tail"], &[("tail", bad)]));
            assert_eq!(out.status, Status::Usage, "--tail {bad}: {}", out.message);
            assert_eq!(out.data.unwrap()["reason"], "bad-tail");
        }

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_name_tail4_or_petname_resolves_through_the_one_resolver() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&env_keys());
        let root = fixture("trace-resolve", "eidolon-abcdef", "eidolon", &[SETTLED], true);

        let tail4 = session_trace(&trace_invocation(&["cdef"], &[]));
        assert_eq!(tail4.status, Status::Ok, "tail4 must resolve: {}", tail4.message);
        assert_eq!(tail4.data.unwrap()["sessionId"], "eidolon-abcdef");

        let petname = session_trace(&trace_invocation(&["brave-otter"], &[]));
        assert_eq!(petname.status, Status::Ok, "petname must resolve: {}", petname.message);
        assert_eq!(petname.data.unwrap()["sessionId"], "eidolon-abcdef");

        let exact = session_trace(&trace_invocation(&["eidolon-abcdef"], &[]));
        assert_eq!(exact.status, Status::Ok);

        // Ambiguity and a miss are the resolver's own refusals, verbatim.
        let miss = session_trace(&trace_invocation(&["nothing-matches-this"], &[]));
        assert_eq!(miss.status, Status::Error);
        assert_eq!(miss.data.unwrap()["reason"], "not-found");
        assert!(miss.message.contains("no session matches"), "{}", miss.message);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_session_without_a_trace_gets_a_taught_error_naming_why() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&env_keys());

        // 1. The harness keeps no trace at all.
        let root = fixture("trace-notrace", "Aoide-old", "eidolon", &[SETTLED], false);
        let out = session_trace(&trace_invocation(&["Aoide-old"], &[]));
        assert_eq!(out.status, Status::Error, "msg: {}", out.message);
        assert_eq!(out.data.clone().unwrap()["reason"], "no-trace");
        assert!(out.message.contains("no readable trace"), "{}", out.message);
        assert!(out.message.contains("read-only export door"), "{}", out.message);
        let _ = std::fs::remove_dir_all(&root);

        // 2. A harness that keeps none by construction (claude's profile has
        //    `transcript.trace == None`).
        let root = fixture("trace-claude", "claude-1", "claude", &[SETTLED], true);
        let out = session_trace(&trace_invocation(&["claude-1"], &[]));
        assert_eq!(out.status, Status::Error, "msg: {}", out.message);
        assert_eq!(out.data.clone().unwrap()["reason"], "no-trace-capability");
        assert!(out.message.contains("keeps no trace"), "{}", out.message);
        let _ = std::fs::remove_dir_all(&root);

        // 3. An eidolon record whose presence is gone entirely.
        let root = fixture("trace-nopresence", "Aoide-gone", "eidolon", &[SETTLED], true);
        std::fs::remove_dir_all(root.join("eidolon")).unwrap();
        let out = session_trace(&trace_invocation(&["Aoide-gone"], &[]));
        assert_eq!(out.status, Status::Error, "msg: {}", out.message);
        assert_eq!(out.data.clone().unwrap()["reason"], "no-presence");
        assert!(out.message.contains("no readable presence metadata"), "{}", out.message);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_empty_trace_is_an_honest_empty_listing_not_a_taught_error() {
        // `eidolon_trace_tail` answers `Some(empty)` for a trace that exists
        // and holds nothing yet — that IS the authority, and the command
        // reports it as zero records rather than claiming no trace exists.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&env_keys());
        let root = fixture("trace-empty", "Aoide-empty", "eidolon", &[], true);

        let out = session_trace(&trace_invocation(&["Aoide-empty"], &[]));
        assert_eq!(out.status, Status::Ok, "msg: {}", out.message);
        assert_eq!(out.data.unwrap()["records"], 0);
        assert!(out.message.starts_with("0 record(s)"), "{}", out.message);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn follow_is_cli_only_and_a_missing_id_is_a_usage_error() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&env_keys());
        let root = fixture("trace-follow", "Aoide-follow", "eidolon", &[SETTLED], true);

        let mut inv = trace_invocation(&["Aoide-follow"], &[("follow", "true")]);
        inv.door = Door::Mcp;
        let out = session_trace(&inv);
        assert_eq!(out.status, Status::Usage, "msg: {}", out.message);
        assert!(out.message.contains("--follow blocks until Ctrl-C"), "{}", out.message);

        let out = session_trace(&trace_invocation(&[], &[]));
        assert_eq!(out.status, Status::Usage);
        assert!(out.message.contains("missing required argument <id>"), "{}", out.message);

        let _ = std::fs::remove_dir_all(&root);
    }

    // ── the step projection (B2.1) ───────────────────────────────────────

    /// One `AssistantMessage` holding thinking + text + tool_use is THREE
    /// steps, in the record's own block order — the regression a single-step
    /// projector (`Option<Step>`) would hide completely.
    #[test]
    fn one_assistant_message_projects_a_step_per_block_in_order() {
        let rec = eidolon_trace_record(ASSISTANT).unwrap();

        let line = steps_of(&rec, Clip::Line);
        let kinds: Vec<&str> = line.iter().map(|s| s.kind).collect();
        assert_eq!(kinds, ["thinking", "say", "tool"], "{line:?}");
        // Every step carries the RECORD's own id/ts, never a new counter.
        assert!(line.iter().all(|s| s.id == "2" && s.ts_ms == Some(1789603009102)));
        assert_eq!(line[1].text, "Let me read the slot catalog first.");
        assert_eq!(line[2].text, "read", "the line clip shows the tool's name alone");
        assert!(!line[2].error);
        // The doc's own thinking sample is longer than the line window, so it
        // is clipped — and says so.
        assert!(line[0].clipped, "{line:?}");
        assert!(line[0].text.ends_with('…'), "{line:?}");

        // `Clip::Detail` adds the tool's arguments on the same step — the
        // "what did it reach for" a details view is looking at.
        let detail = steps_of(&rec, Clip::Detail);
        assert_eq!(detail.len(), 3, "{detail:?}");
        assert!(
            detail[2].text.starts_with("read {") && detail[2].text.contains("slots.md"),
            "{:?}",
            detail[2].text
        );
        assert!(!detail[2].clipped, "a small input is not cut: {:?}", detail[2]);
    }

    /// `Clip::Detail` keeps strictly MORE of a long reasoning block than
    /// `Clip::Line`, and keeps its own line breaks while `Line` flattens to one.
    #[test]
    fn detail_keeps_more_of_a_thinking_block_and_its_own_lines() {
        let thinking = "first paragraph of the reasoning\nsecond paragraph, still reasoning\n".to_string()
            + &"padding reasoning text ".repeat(60);
        let rec = eidolon_trace_record(
            &json!({
                "id": 7, "ts_ms": 1789603009200i64,
                "kind": {"AssistantMessage": {"role": "assistant", "content": [
                    {"type": "thinking", "thinking": thinking},
                ]}}
            })
            .to_string(),
        )
        .unwrap();

        let line = steps_of(&rec, Clip::Line);
        let detail = steps_of(&rec, Clip::Detail);
        assert_eq!(line.len(), 1);
        assert_eq!(detail.len(), 1);
        assert!(!line[0].text.contains('\n'), "the line clip flattens: {:?}", line[0].text);
        assert!(detail[0].text.contains('\n'), "the detail clip keeps the block's own lines");
        assert!(
            detail[0].text.chars().count() > line[0].text.chars().count() * 3,
            "detail must keep strictly more: {:?} vs {:?}",
            line[0].text,
            detail[0].text
        );
        assert!(detail[0].clipped, "the block is longer than DETAIL_THINKING_MAX");
        assert!(detail[0].text.ends_with('…'));
        // A detail step is still BOUNDED: the line count first, the char
        // window second.
        assert!(detail[0].text.lines().count() <= DETAIL_MAX_LINES);
        assert!(detail[0].text.chars().count() <= DETAIL_THINKING_MAX);
    }

    #[test]
    fn a_tool_result_step_carries_the_error_flag_and_both_clips() {
        let ok = steps_of(&eidolon_trace_record(RESULT).unwrap(), Clip::Line);
        assert_eq!(ok.len(), 1);
        assert_eq!(ok[0].kind, "result");
        assert!(!ok[0].error);
        assert_eq!(ok[0].text, "1\t# Per-song widget slots", "first line only, no `!` prefix");
        assert!(!ok[0].text.contains("catalog"));

        let err = steps_of(&eidolon_trace_record(RESULT_ERR).unwrap(), Clip::Detail);
        assert_eq!(err.len(), 1);
        assert!(err[0].error, "is_error rides the step, not the text");
        assert!(err[0].text.contains("ENOENT: no such file"), "{:?}", err[0].text);
        assert!(
            err[0].text.contains("more"),
            "the detail clip keeps the result's second line: {:?}",
            err[0].text
        );
        assert!(!err[0].text.starts_with('!'), "the decoration is the renderer's");
    }

    /// The same step list renders the human line — one projector, no drift.
    #[test]
    fn the_human_line_is_the_line_clip_rendered() {
        let rec = eidolon_trace_record(ASSISTANT).unwrap();
        let rendered = render_line(&rec, false);
        let joined = render_steps(&steps_of(&rec, Clip::Line), false);
        assert!(rendered.ends_with(&joined), "{rendered} !~ {joined}");
        assert!(joined.contains("→ read"), "{joined}");
        // A record that emits no step renders no summary column at all.
        let cancelled = render_line(&eidolon_trace_record(CANCELLED).unwrap(), false);
        assert!(cancelled.ends_with("Cancelled"), "{cancelled}");
        assert!(steps_of(&eidolon_trace_record(CANCELLED).unwrap(), Clip::Line).is_empty());
    }

    /// A hostile/pathological record cannot make the projection unbounded: one
    /// step per block up to the cap, then ONE note naming what was left out.
    #[test]
    fn a_record_with_many_blocks_projects_a_bounded_list_that_names_the_omission() {
        let blocks: Vec<Value> = (0..MAX_BLOCKS_PER_RECORD + 18)
            .map(|i| json!({"type": "text", "text": format!("block {i}")}))
            .collect();
        let rec = eidolon_trace_record(
            &json!({"id": 9, "kind": {"AssistantMessage": {"role": "assistant", "content": blocks}}})
                .to_string(),
        )
        .unwrap();

        for clip in [Clip::Line, Clip::Detail] {
            let steps = steps_of(&rec, clip);
            assert_eq!(
                steps.len(),
                MAX_BLOCKS_PER_RECORD + 1,
                "cap, plus the one note naming the omission"
            );
            let last = steps.last().unwrap();
            assert_eq!(last.kind, "other");
            assert!(last.clipped);
            assert!(last.text.contains("+18 more block(s)"), "{}", last.text);
        }
    }

    /// The whole-projection cap keeps the NEWEST steps and reports the count it
    /// dropped — never a silent shortening of the window.
    #[test]
    fn the_whole_projection_is_capped_and_reports_what_it_dropped() {
        let many: Vec<String> = (0..40)
            .map(|i| {
                let blocks: Vec<Value> = (0..MAX_BLOCKS_PER_RECORD)
                    .map(|b| json!({"type": "text", "text": format!("r{i} block {b}")}))
                    .collect();
                json!({"id": i, "kind": {"AssistantMessage": {"content": blocks}}}).to_string()
            })
            .collect();
        let refs: Vec<&String> = many.iter().collect();

        let (steps, omitted) = steps_of_lines(&refs, Clip::Line);
        assert_eq!(steps.len(), MAX_STEPS, "capped");
        assert_eq!(omitted, 40 * MAX_BLOCKS_PER_RECORD - MAX_STEPS);
        assert!(
            steps.last().unwrap().text.contains("r39 block 31"),
            "the newest end of the window is what is kept: {:?}",
            steps.last()
        );
        assert!(!steps.iter().any(|s| s.text.starts_with("r0 ")));

        // A malformed line contributes no step (it keeps its `?` human line and
        // its raw line in `data.lines`), and a readable one after it still does.
        let lines = vec!["{ not json".to_string(), START.to_string()];
        let refs: Vec<&String> = lines.iter().collect();
        let (steps, omitted) = steps_of_lines(&refs, Clip::Line);
        assert_eq!(omitted, 0);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].kind, "other");
        assert!(steps[0].text.contains("model=ollama:deepseek-v4.1-flash"));
    }

    // ── the command's new envelope fields ────────────────────────────────

    #[test]
    fn json_carries_steps_beside_untouched_raw_lines_and_is_bounded_by_tail() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&env_keys());
        let root = fixture(
            "trace-steps",
            "Aoide-steps",
            "eidolon",
            &[START, USER, ASSISTANT, RESULT, SETTLED],
            true,
        );

        let out = session_trace(&trace_invocation(&["Aoide-steps"], &[("json", "true")]));
        assert_eq!(out.status, Status::Ok, "msg: {}", out.message);
        let data = out.data.clone().unwrap();
        // The window: the session start (one `other` step), the user message,
        // 3 steps from the assistant message, 1 result, 1 settled.
        assert_eq!(data["clip"], "line");
        assert_eq!(data["stepsOmitted"], 0);
        let steps = data["steps"].as_array().unwrap();
        assert_eq!(steps.len(), 7, "{steps:?}");
        assert_eq!(steps[0]["kind"], "other");
        assert_eq!(steps[1]["kind"], "user");
        assert_eq!(steps[2]["kind"], "thinking");
        assert_eq!(steps[3]["kind"], "say");
        assert_eq!(steps[4]["kind"], "tool");
        assert_eq!(steps[5]["kind"], "result");
        assert_eq!(steps[6]["kind"], "settled");
        assert_eq!(steps[2]["id"], "2", "the record's own id");
        assert!(steps[2]["ts"].as_i64().unwrap() > 0);
        assert_eq!(steps[5]["error"], false);
        // `data.lines` is untouched — raw records, byte for byte.
        let lines = data["lines"].as_array().unwrap();
        assert_eq!(lines[0].as_str().unwrap(), START);
        assert_eq!(lines[2].as_str().unwrap(), ASSISTANT);
        let full_lines = data["lines"].clone();

        // `--tail` bounds the steps the same way it bounds the records.
        let out = session_trace(&trace_invocation(&["Aoide-steps"], &[("tail", "2"), ("json", "true")]));
        let data = out.data.unwrap();
        assert_eq!(data["records"], 2);
        let steps = data["steps"].as_array().unwrap();
        assert_eq!(steps.len(), 2, "only the records inside the window project steps");
        assert_eq!(steps[0]["kind"], "result");
        assert_eq!(steps[1]["kind"], "settled");

        // `--clip detail` widens the STEP texts and leaves `data.lines` alone.
        let out = session_trace(&trace_invocation(
            &["Aoide-steps"],
            &[("clip", "detail"), ("json", "true")],
        ));
        let detail = out.data.clone().unwrap();
        assert_eq!(detail["clip"], "detail");
        assert_eq!(
            detail["lines"], full_lines,
            "the clip never touches the raw lines"
        );
        let tool = detail["steps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "tool")
            .cloned()
            .unwrap();
        assert!(tool["text"].as_str().unwrap().contains("slots.md"), "{tool}");
        // The human body stays the one-line spelling under either clip.
        assert!(out.message.lines().skip(1).all(|l| !l.contains('\n')));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_unknown_clip_is_a_taught_usage_error_never_a_silent_widening() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&env_keys());
        let root = fixture("trace-badclip", "Aoide-clip", "eidolon", &[SETTLED], true);

        for bad in ["detials", "line,detail", "true"] {
            let out = session_trace(&trace_invocation(&["Aoide-clip"], &[("clip", bad)]));
            assert_eq!(out.status, Status::Usage, "--clip {bad}: {}", out.message);
            assert_eq!(out.data.clone().unwrap()["reason"], "bad-clip");
            assert!(out.message.contains("is not a clip"), "{}", out.message);
        }
        // An explicit `line` and an absent flag are the same answer.
        let plain = session_trace(&trace_invocation(&["Aoide-clip"], &[("json", "true")]));
        let explicit = session_trace(&trace_invocation(
            &["Aoide-clip"],
            &[("clip", "line"), ("json", "true")],
        ));
        assert_eq!(plain.data.unwrap()["steps"], explicit.data.unwrap()["steps"]);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn session_trace_registers_at_the_expected_path() {
        let mut r = aoide_protocol::registry::Registry::new();
        crate::commands::graph::register(&mut r);
        let cmd = r
            .get(&["session".to_string(), "trace".to_string()])
            .expect("session trace must be registered");
        assert_eq!(cmd.dotted(), "session.trace");
        assert!(cmd.implemented && !cmd.gated);
        assert!(cmd.args.iter().any(|a| a.name == "id" && a.required));
        for f in ["tail", "clip", "follow", "json"] {
            assert!(cmd.flags.iter().any(|x| x.name == f), "missing --{f}: {:?}", cmd.flags);
        }
    }
}
