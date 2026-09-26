//! Shared command/arg/error glue used by every handler-bearing `graph` submodule:
//! positional/flag arg validation, the stage-error envelope, the
//! three-registry loader, and the text sanitizers (`clean_line`/`clip_flat`)
//! every surface that prints text this process did not write shares — mail
//! fragments in `view.rs`, a pulled node's own fields in `who.rs`.

use super::model::{
    hooks_path, load_stage, projects_path, sessions_path, HooksFile, ProjectsFile, SessionsFile,
};
use aoide_protocol::Invocation;
use aoide_protocol::output::Outcome;
use serde_json::json;

/// Positional-arg check → structured usage error (exit 2) on a miss.
/// Names the missing positionals (not just "you're short") — this crate has
/// no registry access, so the follow-up hint points at `--help` rather than
/// inlining the command's usage block.
pub(in crate::graph) fn require_args(
    inv: &Invocation,
    names: &[&str],
) -> Result<Vec<String>, Outcome> {
    if inv.args.len() < names.len() {
        let missing = names[inv.args.len()..]
            .iter()
            .map(|n| format!("<{n}>"))
            .collect::<Vec<_>>()
            .join(", ");
        let plural = if names.len() - inv.args.len() > 1 {
            "arguments"
        } else {
            "argument"
        };
        return Err(Outcome::usage(
            inv.dotted(),
            format!(
                "missing required {plural} {missing} for `aoide {}`\n\
                 usage: aoide {} {} [--json]\n\
                 run 'aoide {} --help' for details",
                inv.path.join(" "),
                inv.path.join(" "),
                names
                    .iter()
                    .map(|n| format!("<{n}>"))
                    .collect::<Vec<_>>()
                    .join(" "),
                inv.path.join(" "),
            ),
        ));
    }
    Ok(inv.args[..names.len()].to_vec())
}

pub(crate) fn stage_error(cmd: &str, msg: String) -> Outcome {
    Outcome::error(cmd, msg).with_data(json!({ "reason": "stage-file-unreadable-or-unwritable" }))
}

/// Load all three graph inputs, tolerating missing files.
pub(in crate::graph) fn load_inputs(
    cmd: &str,
) -> Result<(ProjectsFile, SessionsFile, HooksFile), Outcome> {
    let p: ProjectsFile = load_stage(&projects_path()).map_err(|e| stage_error(cmd, e))?;
    let s: SessionsFile = load_stage(&sessions_path()).map_err(|e| stage_error(cmd, e))?;
    let h: HooksFile = load_stage(&hooks_path()).map_err(|e| stage_error(cmd, e))?;
    Ok((p, s, h))
}

/// A required `--flag` → structured usage error (exit 2) when absent/empty.
/// Same "name the gap, then point at --help" shape as [`require_args`].
pub(in crate::graph) fn require_flag(inv: &Invocation, name: &str) -> Result<String, Outcome> {
    match inv.flags.get(name).filter(|v| !v.is_empty()) {
        Some(v) => Ok(v.clone()),
        None => Err(Outcome::usage(
            inv.dotted(),
            format!(
                "missing required flag --{name} <value> for `aoide {}`\n\
                 usage: aoide {} --{name} <value> [--json]\n\
                 run 'aoide {} --help' for details",
                inv.path.join(" "),
                inv.path.join(" "),
                inv.path.join(" "),
            ),
        )),
    }
}

/// The longest untrusted line any graph surface prints, in characters —
/// [`clip_flat`]'s bound, one constant for every caller (a mail fragment, a
/// far node's own `cwd`, a link's `sessionId`).
pub(in crate::graph) const LINE_MAX: usize = 200;

/// One line of text this process did not write, made safe to print: every
/// control character stripped — `\r` included, it is an Enter at whoever
/// pastes it — every Unicode `Cf` mark stripped too ([`is_unsafe`], which
/// includes the zero-width and bidi characters that would otherwise survive
/// as an invisible instruction), whitespace flattened, clipped to
/// [`LINE_MAX`] with an ellipsis. The single sanitizer `view.rs` (mail
/// fragments, a run's own output lines) and `who.rs` (a pulled node
/// document's own fields) both reach a terminal through, so neither surface
/// can hold a laxer rule than the other.
pub(in crate::graph) fn clean_line(s: &str) -> String {
    let stripped: String = s.chars().filter(|c| !is_unsafe(*c)).collect();
    clip_flat(&stripped, LINE_MAX)
}

/// A character no terminal may be handed: every [`char::is_control`]
/// (`\u{1b}` colour/bell/`\r`/`\n`) plus every Unicode `Cf` (FORMAT)
/// character — the invisible marks a terminal honours but `is_control` does
/// not, which is what lets one string read as another. `pub(in crate::graph)`
/// because BOTH sanitizers reach it: [`clean_line`] here and
/// `view.rs::clean_block` for the multi-line texts (a letter body, a run's
/// instructions).
pub(in crate::graph) fn is_unsafe(c: char) -> bool {
    c.is_control() || is_format(c)
}

/// Unicode 15's `Cf` (FORMAT) category, enumerated — `char`'s stable API has
/// no general-category test, so the ranges stand here instead. They are the
/// marks that render as NOTHING and change how the text around them reads:
/// the bidi embeddings/overrides/isolates Trojan Source reorders a line with,
/// the zero-width space/joiners, the BOM, the soft hyphen, the Arabic
/// letter/number marks, and the tag block. A range added by a later Unicode
/// version is a one-line addition here; every reader is [`is_unsafe`].
fn is_format(c: char) -> bool {
    matches!(
        c,
        '\u{00ad}'                  // soft hyphen
        | '\u{0600}'..='\u{0605}'   // Arabic number signs
        | '\u{061c}'                // Arabic letter mark
        | '\u{06dd}'                // Arabic end of ayah
        | '\u{070f}'                // Syriac abbreviation mark
        | '\u{0890}'..='\u{0891}'   // Arabic pound/piastre marks
        | '\u{08e2}'                // Arabic disputed end of ayah
        | '\u{180e}'                // Mongolian vowel separator
        | '\u{200b}'..='\u{200f}'   // ZWSP, ZWNJ, ZWJ, LRM, RLM
        | '\u{202a}'..='\u{202e}'   // bidi embeddings and overrides
        | '\u{2060}'..='\u{2064}'   // word joiner, invisible operators
        | '\u{2066}'..='\u{206f}'   // bidi isolates + deprecated format chars
        | '\u{feff}'                // BOM / zero-width no-break space
        | '\u{fff9}'..='\u{fffb}'   // interlinear annotation
        | '\u{110bd}' | '\u{110cd}' // Kaithi number signs
        | '\u{13430}'..='\u{1343f}' // Egyptian hieroglyph format controls
        | '\u{1bca0}'..='\u{1bca3}' // Shorthand format controls
        | '\u{1d173}'..='\u{1d17a}' // musical symbol begin/end
        | '\u{e0001}'               // language tag
        | '\u{e0020}'..='\u{e007f}' // tag characters
    )
}

/// Flatten runs of whitespace to single spaces and clip to `max` characters
/// with an ellipsis. Character-counted, never byte-counted, so a multi-byte
/// glyph is never cut in half.
pub(in crate::graph) fn clip_flat(s: &str, max: usize) -> String {
    let flat: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        return flat;
    }
    let mut out: String = flat.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_line_strips_colour_and_keeps_its_literal_text() {
        assert_eq!(clean_line("\u{1b}[31mred\u{1b}[0m"), "[31mred[0m");
        assert_eq!(clean_line("bell\u{7}here"), "bellhere");
    }

    #[test]
    fn clean_line_strips_the_bidi_marks_that_reorder_a_line() {
        assert_eq!(clean_line("\u{202e}gpj.exe"), "gpj.exe");
        assert_eq!(clean_line("a\u{200f}b\u{2066}c\u{2069}d"), "abcd");
    }

    /// Every `Cf` mark, not just the bidi ones: the zero-width space/joiner
    /// that hides a word boundary, the BOM, the soft hyphen, the tag block.
    /// A terminal renders them as NOTHING, so a line that kept them would
    /// read as text it is not — and they survive `is_control`, which is why
    /// they need their own table.
    #[test]
    fn clean_line_strips_the_invisible_format_characters() {
        assert_eq!(clean_line("hid\u{200b}den"), "hidden");
        assert_eq!(clean_line("a\u{200d}b\u{2060}c\u{00ad}d\u{feff}e"), "abcde");
        assert_eq!(clean_line("tag\u{e0041}\u{e007f}ged"), "tagged");
        assert_eq!(clean_line("\u{061c}alm\u{180e}sep"), "almsep");
        // Not a licence to strip everything exotic: an ordinary multi-byte
        // glyph is text, and stays.
        assert_eq!(clean_line("ünicode 漢字"), "ünicode 漢字");
    }

    #[test]
    fn clean_line_flattens_a_newline_rather_than_printing_a_second_row() {
        // `\n`/`\r` are control characters, so they are STRIPPED before the
        // flattening pass ever sees them — the two halves join, and the line
        // stays one line. Never a second row, never a bare CR that would
        // overwrite the row above it.
        assert_eq!(clean_line("first\nsecond"), "firstsecond");
        assert_eq!(clean_line("carriage\rreturn"), "carriagereturn");
        assert_eq!(clean_line("  padded \t line  "), "padded line");
    }

    #[test]
    fn clean_line_bounds_a_megabyte_to_one_clipped_line() {
        let out = clean_line(&"x".repeat(1_000_000));
        assert_eq!(out.chars().count(), LINE_MAX);
        assert!(out.ends_with('…'));
        // Character-counted, never byte-counted: a multi-byte glyph is never
        // cut in half (`clip_flat`'s own note).
        let out = clean_line(&"é".repeat(LINE_MAX + 50));
        assert_eq!(out.chars().count(), LINE_MAX);
        assert!(out.ends_with('…'));
    }
}
