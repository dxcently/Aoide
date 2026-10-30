//! The paths that never end on their own. They run only as held children
//! (`Exec::follow`, through `chat::follow` or a future held view), never through
//! `palette::call`, which would wait out its deadline and refuse. The kinds lane
//! marks watch verbs in the schema, and this list is deleted when that lands.

/// The six paths. `session trace` is listed whole: its one-shot is refused with
/// its `--follow` form, since a palette cannot tell them apart by path.
pub const PATHS: &[&[&str]] = &[
    &["session", "trace"],
    &["session", "watch"],
    &["events", "tail"],
    &["secrets", "watch"],
    &["pair", "watch"],
    &["a2a", "serve"],
];

/// Whether `words` is one of the blocking paths.
pub fn is_blocking(words: &[&str]) -> bool {
    PATHS.contains(&words)
}
