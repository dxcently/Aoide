//! How a session's chat is followed: one entry per case, in the order they are
//! tried. A case's child is a held `aoide` command, and the first case whose
//! child answers is the chat's stream. Adding a case is one entry here, with the
//! verb it runs declared in the schema; `tests/chat.rs` fails for an entry whose
//! verb the schema does not declare.

/// One way to follow a session. `flags` are the flags the held command takes, as
/// `(name, value)`: a bare flag has no value. The session id is its one
/// positional argument. `json` is not listed: every call passes it.
pub struct Case {
    pub name: &'static str,
    pub words: &'static [&'static str],
    pub flags: &'static [(&'static str, Option<&'static str>)],
}

/// The cases, in the order they are tried.
pub const CASES: &[Case] = &[
    Case {
        name: "trace",
        words: &["session", "trace"],
        flags: &[("follow", None), ("tail", Some("200"))],
    },
    Case {
        name: "watch",
        words: &["session", "watch"],
        flags: &[],
    },
];
