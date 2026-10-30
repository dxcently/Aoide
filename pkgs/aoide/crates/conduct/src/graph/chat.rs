//! `session chat <id> [--follow] [--json]` — a session's conversation as chat
//! lines, read through its harness's chat adapter (`aoide_protocol::chat`).
//! The verb is the registry's default doors, the terminal and the daemon hop:
//! A2A and MCP do not reach it, and a remote `--to` is not resolved here.
//!
//! Each tick re-reads the session's own record, so a `/clear` or `/resume` that
//! moves the record to another transcript is followed from that file's first
//! line. The wrap-to-live-child relation is not resolved separately: the record's
//! transcript is the one thing followed.

use super::common::require_args;
use super::model::{load_stage, sessions_path, SessionsFile};
use super::permit::summons_card_id;
use super::trace::{on_sigint, resolve_local, INTERRUPTED};
use crate::notifications::{notifications_path, Notification, NotificationsFile};
use aoide_protocol::agents::{agent_profile, AgentProfile};
use aoide_protocol::chat::{Ask, AskKind, ChatAdapter, ChatLine, ChatSupport, Role, State};
use aoide_protocol::output::Outcome;
use aoide_protocol::Invocation;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::Duration;

const CHAT_POLL: Duration = Duration::from_millis(500);

/// The lines printed after `cursor`, the id of the last line already printed.
/// No cursor, or a cursor no longer in the window, prints the whole window.
pub(super) fn cursor_tail<'a>(chat: &'a [ChatLine], cursor: Option<&str>) -> &'a [ChatLine] {
    let Some(c) = cursor else {
        return chat;
    };
    match chat.iter().rposition(|l| line_id(l) == c) {
        Some(i) => &chat[i + 1..],
        None => chat,
    }
}

fn line_id(line: &ChatLine) -> String {
    line.json()["id"].as_str().unwrap_or("").to_string()
}

/// The adapter a profile names, or the reason it has none.
pub(super) fn chat_adapter(profile: &'static AgentProfile) -> Result<&'static ChatAdapter, &'static str> {
    match &profile.chat {
        ChatSupport::Adapter(adapter) => Ok(adapter),
        ChatSupport::Unsupported(reason) => Err(reason),
    }
}

/// The chat state a canonical session state shows as a state line, if any.
/// `awaiting` is waiting on the person; `idle`, `stopped` and `done` are settled.
pub(super) fn state_of(state: &str) -> Option<State> {
    match state {
        "awaiting" => Some(State::Waiting),
        "idle" | "stopped" | "done" => Some(State::Settled),
        _ => None,
    }
}

/// The permission ask a Claude session shows while it awaits the person: the
/// permit record's card, the one the summons publishes per session (`permit-<id>`).
/// Its fingerprint is the card's `received_at`, which names this prompt apart from
/// the one before it. No ask while the session is not `awaiting` or no card stands.
pub(super) fn ask_of(session: &str, state: &str, card: Option<&Notification>) -> Option<ChatLine> {
    if state != "awaiting" {
        return None;
    }
    let card = card?;
    let ask = Ask::new(&card.id, AskKind::Approval, &card.body).fingerprint(&card.received_at);
    Some(ChatLine::new(session, &format!("ask:{}", card.received_at), None, Role::Agent).ask(ask))
}

/// The printed-so-far position of one `chat` run: the source cursor and the
/// transcript it was read from. A new path (a `/clear` or `/resume` moved the
/// session to another file) starts the chat again from its first line.
#[derive(Default)]
pub(super) struct Printed {
    path: Option<PathBuf>,
    cursor: Option<String>,
}

impl Printed {
    /// The lines this tick prints, of the window `chat` read from `path`.
    pub(super) fn tick(&mut self, path: &Path, chat: &[ChatLine]) -> Vec<ChatLine> {
        if self.path.as_deref() != Some(path) {
            self.path = Some(path.to_path_buf());
            self.cursor = None;
        }
        let fresh = cursor_tail(chat, self.cursor.as_deref()).to_vec();
        if let Some(last) = chat.last() {
            self.cursor = Some(line_id(last));
        }
        fresh
    }
}

/// One line as the terminal shows it: its role, then its text, tool call or ask.
fn render(line: &ChatLine) -> String {
    let j = line.json();
    let role = j["role"].as_str().unwrap_or("system");
    let body = if let Some(ask) = j.get("ask") {
        format!("ask: {}", ask["prompt"].as_str().unwrap_or(""))
    } else if let Some(tool) = j.get("tool") {
        let name = tool["name"].as_str().unwrap_or("");
        match (tool.get("result").and_then(|r| r.as_str()), tool.get("input").and_then(|i| i.as_str())) {
            (Some(result), _) => format!("{name} → {result}"),
            (None, Some(input)) => format!("{name} {input}"),
            (None, None) => name.to_string(),
        }
    } else {
        let text = j["text"].as_str().unwrap_or("");
        if j.get("thinking").is_some() {
            format!("(thinking) {text}")
        } else {
            text.to_string()
        }
    };
    match j.get("from").and_then(|f| f.as_str()) {
        Some(from) => format!("[{role} from {from}] {body}"),
        None => format!("[{role}] {body}"),
    }
}

fn show(line: &ChatLine, json_mode: bool) -> String {
    if json_mode {
        line.json_line()
    } else {
        render(line)
    }
}

struct Tick {
    lines: Vec<ChatLine>,
    done: bool,
}

/// One pass over the session's record and its transcript: the chat lines that
/// are new, plus the state and ask lines that changed. `follow` keeps waiting for
/// a transcript that is not there yet; a one-shot read refuses instead.
fn read_tick(
    cmd: &str,
    id: &str,
    ended: Option<&aoide_storage::ledger::LedgerEntry>,
    follow: bool,
    printed: &mut Printed,
    last_state: &mut Option<String>,
    last_ask: &mut Option<String>,
) -> Result<Tick, Outcome> {
    let file: SessionsFile = match load_stage(&sessions_path()) {
        Ok(f) => f,
        Err(e) => return Err(super::common::stage_error(cmd, e)),
    };
    let row = file.sessions.iter().find(|r| r.session_id == id);
    let agent = row
        .map(|r| r.agent.clone())
        .or_else(|| ended.map(|e| e.agent.clone()))
        .unwrap_or_default();
    let Some(profile) = agent_profile(&agent) else {
        return Err(Outcome::error(
            cmd,
            format!("`{id}` runs `{agent}`, which has no registered agent profile here"),
        )
        .with_data(json!({ "reason": "unknown-agent", "sessionId": id, "agent": agent })));
    };
    let adapter = match chat_adapter(profile) {
        Ok(a) => a,
        Err(reason) => {
            return Err(Outcome::error(cmd, format!("`{id}` has no chat: {reason}"))
                .with_data(json!({ "reason": "no-chat-adapter", "sessionId": id, "agent": agent })))
        }
    };
    let (state, cwd, hint) = match (row, ended) {
        (Some(r), _) => (
            r.state.clone(),
            r.cwd.clone(),
            r.transcript_path.clone().or_else(|| r.log_path.clone()).filter(|p| !aoide_storage::fs::inside_session_logs(p)),
        ),
        (None, Some(e)) => (
            "done".to_string(),
            e.cwd.clone(),
            e.log_path.clone().filter(|p| !aoide_storage::fs::inside_session_logs(p)),
        ),
        (None, None) => return Err(Outcome::error(cmd, format!("no session record for `{id}`"))),
    };
    let wait = |what: &str| -> Result<Tick, Outcome> {
        if follow {
            Ok(Tick { lines: Vec::new(), done: false })
        } else {
            Err(Outcome::error(cmd, format!("`{id}` has no readable {what} yet"))
                .with_data(json!({ "reason": "no-transcript", "sessionId": id })))
        }
    };
    let Some(path) = (profile.transcript.locate)(id, Some(&cwd), hint.as_deref()) else {
        return wait("transcript");
    };
    let Some(raw) = (adapter.read)(&path) else {
        return wait("transcript");
    };
    let chat: Vec<ChatLine> = raw.iter().flat_map(|l| (adapter.project)(id, l)).collect();
    let mut lines = printed.tick(&path, &chat);

    if last_state.as_deref() != Some(state.as_str()) {
        if let Some(st) = state_of(&state) {
            lines.push(ChatLine::new(id, &format!("state:{state}"), None, Role::System).state(st));
        }
        *last_state = Some(state.clone());
    }
    let card: Option<Notification> = load_stage::<NotificationsFile>(&notifications_path())
        .ok()
        .and_then(|f| f.notifications.into_iter().find(|n| n.id == summons_card_id(id)));
    if let Some(ask) = ask_of(id, &state, card.as_ref()) {
        let fingerprint = ask.json()["ask"]["fingerprint"].as_str().unwrap_or("").to_string();
        if last_ask.as_deref() != Some(fingerprint.as_str()) {
            lines.push(ask);
            *last_ask = Some(fingerprint);
        }
    }
    let done = ended.is_some() || state == "done";
    Ok(Tick { lines, done })
}

pub fn session_chat(inv: &Invocation) -> Outcome {
    let cmd = "session.chat";
    let args = match require_args(inv, &["id"]) {
        Ok(a) => a,
        Err(o) => return o,
    };
    let target = args[0].trim().to_string();
    let json_mode = inv.flag_present("json");
    let follow = inv.flag_present("follow");
    let (id, ended, _) = match resolve_local(cmd, &target) {
        Ok(r) => r,
        Err(o) => return o,
    };
    if follow {
        unsafe {
            libc::signal(libc::SIGINT, on_sigint as *const () as libc::sighandler_t);
        }
    }
    let mut printed = Printed::default();
    let mut last_state = None;
    let mut last_ask = None;
    let mut shown: Vec<ChatLine> = Vec::new();
    loop {
        let tick = match read_tick(cmd, &id, ended.as_ref(), follow, &mut printed, &mut last_state, &mut last_ask) {
            Ok(t) => t,
            Err(o) => return o,
        };
        if follow {
            use std::io::Write;
            for line in &tick.lines {
                println!("{}", show(line, json_mode));
            }
            let _ = std::io::stdout().flush();
        }
        shown.extend(tick.lines);
        if !follow || tick.done || INTERRUPTED.load(Ordering::SeqCst) {
            break;
        }
        std::thread::sleep(CHAT_POLL);
    }
    if follow {
        return Outcome::ok(cmd, format!("chat closed · {} line(s)", shown.len()));
    }
    let message: Vec<String> = shown.iter().map(render).collect();
    Outcome::ok(cmd, message.join("\n"))
        .with_data(json!({ "id": id, "lines": shown.iter().map(ChatLine::json).collect::<Vec<_>>() }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_protocol::agents::agent_profile;
    use aoide_protocol::chat::{Role};

    fn line(id: &str, text: &str) -> ChatLine {
        ChatLine::new("s-1", id, None, Role::Agent).text(text)
    }

    #[test]
    fn cursor_tail_returns_what_follows_the_last_printed_line() {
        let chat = [line("a", "one"), line("b", "two"), line("c", "three")];
        let after_a: Vec<String> = cursor_tail(&chat, Some("a")).iter().map(|l| l.json()["id"].as_str().unwrap().to_string()).collect();
        assert_eq!(after_a, ["b", "c"]);
        assert!(cursor_tail(&chat, Some("c")).is_empty());
    }

    #[test]
    fn cursor_tail_prints_everything_when_the_cursor_is_gone_or_absent() {
        let chat = [line("a", "one"), line("b", "two")];
        assert_eq!(cursor_tail(&chat, None).len(), 2);
        assert_eq!(cursor_tail(&chat, Some("gone")).len(), 2);
    }

    #[test]
    fn chat_adapter_names_claude_and_refuses_kimi_with_its_reason() {
        assert!(chat_adapter(agent_profile("claude").unwrap()).is_ok());
        let Err(reason) = chat_adapter(agent_profile("kimi").unwrap()) else {
            panic!("kimi has no chat adapter");
        };
        assert!(reason.contains("kimi"), "{reason}");
    }

    #[test]
    fn state_of_maps_the_canonical_states_and_shows_no_others() {
        assert_eq!(state_of("awaiting"), Some(State::Waiting));
        for settled in ["idle", "stopped", "done"] {
            assert_eq!(state_of(settled), Some(State::Settled), "{settled}");
        }
        assert_eq!(state_of("working"), None);
    }

    #[test]
    fn a_permission_ask_shows_while_the_card_stands_and_the_session_awaits() {
        let card = crate::graph::permit::summons_record(
            "claude",
            "s-1",
            "Permission",
            "Allow Bash: cargo test?",
            "2026-10-30T10:00:00Z".into(),
        );
        let ask = ask_of("s-1", "awaiting", Some(&card)).expect("an ask while awaiting");
        let json = ask.json();
        assert_eq!(json["ask"]["kind"], "approval");
        assert_eq!(json["ask"]["prompt"], "Allow Bash: cargo test?");
        assert_eq!(json["ask"]["fingerprint"], "2026-10-30T10:00:00Z");
        assert!(ask_of("s-1", "working", Some(&card)).is_none(), "not awaiting");
        assert!(ask_of("s-1", "awaiting", None).is_none(), "no card stands");
    }

    #[test]
    fn a_new_transcript_path_starts_the_chat_again_from_its_first_line() {
        let mut printed = Printed::default();
        let a = Path::new("/t/one.jsonl");
        let b = Path::new("/t/two.jsonl");
        assert_eq!(printed.tick(a, &[line("1", "x"), line("2", "y")]).len(), 2);
        let mut grown = vec![line("1", "x"), line("2", "y"), line("3", "z")];
        assert_eq!(printed.tick(a, &grown).len(), 1, "only the appended line");
        assert_eq!(printed.tick(b, &grown).len(), 3, "a new file prints from its first line");
        grown.clear();
    }
}
