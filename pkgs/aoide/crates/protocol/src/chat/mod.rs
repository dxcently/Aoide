//! The chat line, v1: one JSON object per message event, the same shape for
//! every harness. `CONTRACTS.md` "The chat line — v1" is the statement; this
//! file is its one writer.
//!
//! Every string the line carries is stripped of the characters no terminal may
//! be handed ([`strip_unsafe`]) by the constructor, once, and the text is clipped
//! to [`TEXT_MAX`] characters. The fields are private, so a caller cannot put a
//! raw string into a line without passing those steps.

use crate::untrusted::strip_unsafe;
use serde_json::{json, Map, Value};

/// The line's version. A new required field bumps it; a consumer ignores what
/// it does not know.
pub const VERSION: u64 = 1;

/// Characters a line's `text` keeps before it is clipped (`clipped: true`).
pub const TEXT_MAX: usize = 4000;

/// Characters a short field (a tool's name, input or result summary, a sender
/// label, an ask's prompt or answer) keeps before it is clipped.
pub const FIELD_MAX: usize = 200;

/// Who a line is from. A closed set: a consumer that meets another name treats
/// the line as `system`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The person, or the operator's word.
    User,
    /// The agent's text, or its thinking when the line says so.
    Agent,
    /// A tool call, or its result (the same `tool.name`).
    Tool,
    /// Turn ends, failures, and peer or external messages (with `from`).
    System,
}

impl Role {
    pub const ALL: [Role; 4] = [Role::User, Role::Agent, Role::Tool, Role::System];

    pub fn name(self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Agent => "agent",
            Role::Tool => "tool",
            Role::System => "system",
        }
    }

    /// The role a name spells, or `None` for a name outside the table.
    pub fn parse(name: &str) -> Option<Role> {
        Role::ALL.into_iter().find(|r| r.name() == name)
    }
}

/// Where a chat is in its turn: `waiting` for an answer, `settled` when the
/// turn has ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Waiting,
    Settled,
}

impl State {
    pub fn name(self) -> &'static str {
        match self {
            State::Waiting => "waiting",
            State::Settled => "settled",
        }
    }
}

/// An ask's kind: a yes-or-no approval, or a question with options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AskKind {
    Approval,
    Question,
}

impl AskKind {
    pub fn name(self) -> &'static str {
        match self {
            AskKind::Approval => "approval",
            AskKind::Question => "question",
        }
    }
}

/// One option of a question ask.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskOption {
    pub label: String,
    pub description: String,
}

/// A pending ask a line carries. `answers` lists an approval's choices;
/// `options` a question's. `fingerprint` is the permit record's, for a Claude
/// permission prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    id: String,
    kind: AskKind,
    prompt: String,
    answers: Vec<String>,
    options: Vec<AskOption>,
    fingerprint: Option<String>,
}

impl Ask {
    pub fn new(id: &str, kind: AskKind, prompt: &str) -> Ask {
        Ask {
            id: clean(id, FIELD_MAX).0,
            kind,
            prompt: clean(prompt, FIELD_MAX).0,
            answers: Vec::new(),
            options: Vec::new(),
            fingerprint: None,
        }
    }

    pub fn answers(mut self, answers: &[&str]) -> Ask {
        self.answers = answers.iter().map(|a| clean(a, FIELD_MAX).0).collect();
        self
    }

    pub fn option(mut self, label: &str, description: &str) -> Ask {
        self.options.push(AskOption {
            label: clean(label, FIELD_MAX).0,
            description: clean(description, FIELD_MAX).0,
        });
        self
    }

    pub fn fingerprint(mut self, fingerprint: &str) -> Ask {
        self.fingerprint = Some(clean(fingerprint, FIELD_MAX).0);
        self
    }

    fn json(&self) -> Value {
        let mut m = Map::new();
        m.insert("id".into(), json!(self.id));
        m.insert("kind".into(), json!(self.kind.name()));
        m.insert("prompt".into(), json!(self.prompt));
        if !self.answers.is_empty() {
            m.insert("answers".into(), json!(self.answers));
        }
        if !self.options.is_empty() {
            let opts: Vec<Value> = self
                .options
                .iter()
                .map(|o| json!({ "label": o.label, "description": o.description }))
                .collect();
            m.insert("options".into(), Value::Array(opts));
        }
        if let Some(fp) = &self.fingerprint {
            m.insert("fingerprint".into(), json!(fp));
        }
        Value::Object(m)
    }
}

/// A tool call (`input` is its summary) or its result (`result`, `error`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tool {
    name: String,
    input: Option<String>,
    result: Option<String>,
    error: bool,
}

/// One chat line. Build it with [`ChatLine::new`] and the setters; each one
/// strips and clips what it is given. [`ChatLine::json`] is its wire form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatLine {
    session: String,
    id: String,
    ts: Option<i64>,
    role: Role,
    text: Option<String>,
    thinking: bool,
    from: Option<String>,
    tool: Option<Tool>,
    ask: Option<Ask>,
    state: Option<State>,
    clipped: bool,
}

impl ChatLine {
    /// A line with its identity. `session` is the aoide session id, `id` the
    /// source's own record id, `ts` its own epoch milliseconds (never a read
    /// time).
    pub fn new(session: &str, id: &str, ts: Option<i64>, role: Role) -> ChatLine {
        ChatLine {
            session: clean(session, FIELD_MAX).0,
            id: clean(id, FIELD_MAX).0,
            ts,
            role,
            text: None,
            thinking: false,
            from: None,
            tool: None,
            ask: None,
            state: None,
            clipped: false,
        }
    }

    /// The line's text, stripped and clipped to [`TEXT_MAX`]. An empty text is
    /// no text.
    pub fn text(mut self, text: &str) -> ChatLine {
        let (cut, clipped) = clean(text, TEXT_MAX);
        self.clipped |= clipped;
        self.text = (!cut.is_empty()).then_some(cut);
        self
    }

    /// Marks the text as the agent's thinking.
    pub fn thinking(mut self) -> ChatLine {
        self.thinking = true;
        self
    }

    /// Who spoke, when `role` does not say it (a peer's or an operator's label).
    pub fn from(mut self, from: &str) -> ChatLine {
        self.from = Some(clean(from, FIELD_MAX).0);
        self
    }

    /// A tool call (`result` absent) or its result (`result` present).
    pub fn tool(mut self, name: &str, input: Option<&str>, result: Option<&str>, error: bool) -> ChatLine {
        self.tool = Some(Tool {
            name: clean(name, FIELD_MAX).0,
            input: input.map(|s| clean(s, FIELD_MAX).0),
            result: result.map(|s| clean(s, FIELD_MAX).0),
            error,
        });
        self
    }

    pub fn ask(mut self, ask: Ask) -> ChatLine {
        self.ask = Some(ask);
        self
    }

    pub fn state(mut self, state: State) -> ChatLine {
        self.state = Some(state);
        self
    }

    pub fn role(&self) -> Role {
        self.role
    }

    /// The line as one JSON object. Keys are sorted, and absent optional fields
    /// are left out, so one line has one spelling.
    pub fn json(&self) -> Value {
        let mut m = Map::new();
        m.insert("v".into(), json!(VERSION));
        m.insert("session".into(), json!(self.session));
        m.insert("id".into(), json!(self.id));
        m.insert("ts".into(), json!(self.ts));
        m.insert("role".into(), json!(self.role.name()));
        if let Some(text) = &self.text {
            m.insert("text".into(), json!(text));
        }
        if self.thinking {
            m.insert("thinking".into(), json!(true));
        }
        if let Some(from) = &self.from {
            m.insert("from".into(), json!(from));
        }
        if let Some(t) = &self.tool {
            let mut tm = Map::new();
            tm.insert("name".into(), json!(t.name));
            if let Some(i) = &t.input {
                tm.insert("input".into(), json!(i));
            }
            if let Some(r) = &t.result {
                tm.insert("result".into(), json!(r));
            }
            if t.error {
                tm.insert("error".into(), json!(true));
            }
            m.insert("tool".into(), Value::Object(tm));
        }
        if let Some(ask) = &self.ask {
            m.insert("ask".into(), ask.json());
        }
        if let Some(state) = self.state {
            m.insert("state".into(), json!(state.name()));
        }
        if self.clipped {
            m.insert("clipped".into(), json!(true));
        }
        Value::Object(m)
    }

    /// The line as the one compact JSON text a `--follow` prints.
    pub fn json_line(&self) -> String {
        self.json().to_string()
    }
}

/// Strip, then clip to `max` characters. Returns the text and whether it was
/// cut. Counted in characters, so a multi-byte glyph is never split.
fn clean(raw: &str, max: usize) -> (String, bool) {
    let stripped = strip_unsafe(raw);
    if stripped.chars().count() <= max {
        return (stripped, false);
    }
    (stripped.chars().take(max).collect(), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_role_outside_the_table_parses_to_nothing() {
        assert_eq!(Role::parse("agent"), Some(Role::Agent));
        assert_eq!(Role::parse("wizard"), None);
        assert_eq!(Role::parse(""), None);
        for r in Role::ALL {
            assert_eq!(Role::parse(r.name()), Some(r));
        }
    }

    #[test]
    fn the_constructor_strips_every_string_it_is_given() {
        let line = ChatLine::new("s\u{1b}1", "id\u{202e}", None, Role::Agent)
            .text("a\u{1b}[31mred\u{202e}rev <script>x</script>")
            .from("pe\u{7}er")
            .tool("re\u{1b}ad", Some("in\u{202e}put"), Some("out\u{7f}put"), false)
            .ask(Ask::new("c\u{1b}1", AskKind::Approval, "Ov\u{202e}erwrite?").answers(&["y\u{1b}es"]));
        let wire = line.json_line();
        assert!(!wire.chars().any(|c| crate::untrusted::is_unsafe(c)), "{wire}");
        assert!(wire.contains("<script>x</script>"), "markup stays literal text: {wire}");
    }

    #[test]
    fn a_long_text_is_clipped_and_says_so() {
        let line = ChatLine::new("s", "1", Some(1), Role::Agent).text(&"a".repeat(10_000));
        let v = line.json();
        assert_eq!(v["text"].as_str().map(str::len), Some(TEXT_MAX));
        assert_eq!(v["clipped"], json!(true));
        let short = ChatLine::new("s", "1", Some(1), Role::Agent).text("hi").json();
        assert!(short.get("clipped").is_none());
    }

    #[test]
    fn an_empty_text_is_no_text() {
        let v = ChatLine::new("s", "1", None, Role::User).text("\u{1b}").json();
        assert!(v.get("text").is_none());
        assert_eq!(v["ts"], Value::Null);
    }
}
