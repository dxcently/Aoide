//! The paths that never end on their own. They run only as held children
//! (`Exec::follow`, through `chat::follow` or a future held view), never through
//! `palette::call`, which would wait out its deadline and refuse. The kinds lane
//! marks watch verbs in the schema, and this list is deleted when that lands.

/// The paths of the core commands that never end on their own: the serve family,
/// the daemon, the conductor TUI, the melete adapter, the watches and trace
/// `--follow`. A hand list, checked against `aoide schema --json` at 839911a5.
/// `session trace` is listed whole: its one-shot is refused with its `--follow`
/// form, since a palette cannot tell them apart by path.
pub const PATHS: &[&[&str]] = &[
    &["session", "trace"],
    &["session", "watch"],
    &["events", "tail"],
    &["secrets", "watch"],
    &["secrets", "serve"],
    &["pair", "watch"],
    &["a2a", "serve"],
    &["mail", "serve"],
    &["mcp", "serve"],
    &["daemon"],
    &["conductor"],
    &["adapter", "melete"],
];

/// Whether `words` is one of the blocking paths.
pub fn is_blocking(words: &[&str]) -> bool {
    PATHS.contains(&words)
}
