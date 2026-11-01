//! A chat with one session: the composer's send, and the session's stream
//! followed as messages. The send is one `aoide send`. The stream is the first
//! case in `cases::CASES` that answers for the session. Every line it prints
//! reaches the page through `record::line`, which spells each string. A chat
//! line is shown once whatever its source repeats, and nothing follows the
//! closing envelope.

pub mod cases;
pub mod record;

use std::collections::HashSet;
use std::fmt;
use std::time::Duration;

use crate::exec::untrusted::visible;
use crate::exec::{Ended, Event, Exec, ExecError, Flag, Follow, Output};
use record::Push;

/// How long a case may stay silent before it is taken as answering. A case
/// that refuses exits at once, and one that answers prints its first line well
/// inside this.
const ANSWER_WAIT: Duration = Duration::from_secs(5);

/// Sends `text` to session `id` as the person's own act: `--yes`, since the
/// composer is the person typing. `harness` is the agent the session was listed
/// with: core refuses the send, before the gate, when the session no longer
/// runs it. A session with no harness (a shell) sends without it. The child
/// runs with the scrubbed environment `child_env` gives a `send`.
pub fn say(exec: &Exec, id: &str, harness: Option<&str>, text: &str) -> Result<Output, ExecError> {
    let mut flags = vec![Flag { name: "id", value: Some(id) }];
    if harness.is_some() {
        flags.push(Flag { name: "harness", value: harness });
    }
    flags.push(Flag { name: "submit", value: None });
    flags.push(Flag { name: "yes", value: None });
    exec.call(&["send"], &flags, &[text], None)
}

/// One session's stream, opened by the first case that answers.
pub struct Chat {
    case: &'static str,
    follow: Follow,
    pending: Option<Event>,
    shown: Shown,
}

/// What a chat has shown: the ids of its lines, and whether it has ended.
#[derive(Default)]
struct Shown {
    ids: HashSet<String>,
    ended: bool,
}

impl Shown {
    /// `push` unless it repeats a line already shown, or follows the end.
    fn admit(&mut self, push: Push) -> Option<Push> {
        if self.ended {
            return None;
        }
        match &push {
            Push::Line(line) if !self.ids.insert(line.id.clone()) => return None,
            Push::End => self.ended = true,
            _ => {}
        }
        Some(push)
    }
}

/// Why no case followed a session: each case that refused, with its taught
/// text, spelled.
#[derive(Debug)]
pub struct Refused {
    pub reasons: Vec<Reason>,
}

/// One case's refusal.
#[derive(Debug)]
pub struct Reason {
    pub case: &'static str,
    pub why: String,
}

#[derive(Debug)]
pub enum FollowError {
    Refused(Refused),
    Exec(ExecError),
}

impl fmt::Display for FollowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(refused) => {
                let named: Vec<String> =
                    refused.reasons.iter().map(|r| format!("{}: {}", r.case, r.why)).collect();
                write!(f, "no stream of this session can be followed ({})", named.join("; "))
            }
            Self::Exec(e) => write!(f, "{e}"),
        }
    }
}

impl From<ExecError> for FollowError {
    fn from(e: ExecError) -> Self {
        Self::Exec(e)
    }
}

/// Follows session `id` through the first case that answers. A case answers when
/// its child prints a line, or when it prints nothing for `ANSWER_WAIT`. A child
/// that closes its stdout first has refused: its stderr is the reason, and the
/// next case is tried. When every case has refused, the call is `Refused`.
pub fn follow(exec: &Exec, id: &str) -> Result<Chat, FollowError> {
    let mut reasons = Vec::new();
    for case in cases::CASES {
        let flags: Vec<Flag<'_>> = case.flags.iter().map(|&(name, value)| Flag { name, value }).collect();
        let follow = match exec.follow(case.words, &flags, &[id]) {
            Ok(follow) => follow,
            Err(e) => {
                reasons.push(Reason { case: case.name, why: visible(&e.to_string()) });
                continue;
            }
        };
        match follow.next(ANSWER_WAIT) {
            Some(Event::Eof) => {
                let ended = follow.end()?;
                reasons.push(Reason { case: case.name, why: refusal_text(&ended) });
            }
            pending => return Ok(Chat { case: case.name, follow, pending, shown: Shown::default() }),
        }
    }
    Err(FollowError::Refused(Refused { reasons }))
}

/// A case's stderr, spelled, or its exit when it printed none.
fn refusal_text(ended: &Ended) -> String {
    let text = String::from_utf8_lossy(&ended.stderr);
    match text.trim() {
        "" => visible(&format!("exited with {:?}", ended.status.and_then(|s| s.code()))),
        text => visible(text),
    }
}

impl Chat {
    /// The name of the case that answered.
    pub fn case(&self) -> &'static str {
        self.case
    }

    /// The next message from the stream, or `None` when none comes within `wait`
    /// or the stream has ended.
    pub fn next(&mut self, wait: Duration) -> Option<Push> {
        loop {
            if self.shown.ended {
                return None;
            }
            let event = match self.pending.take() {
                Some(event) => event,
                None => self.follow.next(wait)?,
            };
            if is_blank(&event) {
                continue;
            }
            if let Some(push) = self.shown.admit(push(event)) {
                return Some(push);
            }
        }
    }

    /// Ends the stream and returns what it printed after the end was asked for,
    /// through its last line.
    pub fn close(self) -> Result<Vec<Push>, ExecError> {
        let Chat { follow, mut shown, .. } = self;
        let rest = follow.end()?.rest;
        Ok(rest.into_iter().filter(|event| !is_blank(event)).filter_map(|event| shown.admit(push(event))).collect())
    }
}

/// A line of only whitespace, such as the empty line a trailing newline leaves.
/// It is never pushed.
fn is_blank(event: &Event) -> bool {
    matches!(event, Event::Line(bytes) if bytes.iter().all(u8::is_ascii_whitespace))
}

fn push(event: Event) -> Push {
    match event {
        Event::Line(bytes) => record::line(&String::from_utf8_lossy(&bytes)),
        Event::Dropped(bytes) => Push::Dropped(bytes),
        Event::Eof => Push::End,
    }
}
