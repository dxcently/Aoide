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
/// fragments, a run's own output lines), `who.rs` (a pulled node
/// document's own fields) and — across the crate boundary —
/// `aoide-server`'s A2A door (a slug echoed back in a refusal, P-RSA S10
/// review, L6) all reach a terminal through, so neither surface
/// can hold a laxer rule than the other.
///
/// `pub` so that last one can reach it at all: the door prints peer bytes,
/// and a second sanitizer over there is exactly how the two would drift.
/// Widened rather than copied, and re-exported at `graph::clean_line`.
pub fn clean_line(s: &str) -> String {
    clip_flat(&strip_unsafe(s), LINE_MAX)
}

/// The strip half of [`clean_line`], on its own: every character [`is_unsafe`]
/// refuses, gone. Split out because a caller with its OWN clip — the
/// ping-back's 80-character `SAY_MAX`, not this module's [`LINE_MAX`] — must
/// not carry a second copy of the `is_unsafe`/`is_format` rule to reach it:
/// one table, one filter, and every sanitizer in this crate keeps the same
/// definition of "unsafe" for free.
pub(in crate::graph) fn strip_unsafe(s: &str) -> String {
    s.chars().filter(|c| !is_unsafe(*c)).collect()
}

/// A character no terminal may be handed: every [`char::is_control`]
/// (`\u{1b}` colour/bell/`\r`/`\n`) plus every Unicode `Cf` (FORMAT)
/// character — the invisible marks a terminal honours but `is_control` does
/// not, which is what lets one string read as another. `pub(in crate::graph)`
/// because every sanitizer in this crate judges through it: [`clean_line`] and
/// [`strip_unsafe`] here, `view.rs::clean_block` for the multi-line texts (a
/// letter body, a run's instructions), and the ping-back's own
/// `SAY_MAX`-clipped `clean` for a line bound for a composer.
pub(in crate::graph) fn is_unsafe(c: char) -> bool {
    c.is_control() || is_format(c) || is_invisible(c)
}

/// Characters that render as NOTHING and are not `Cf` — the fillers a line can
/// be padded with, and the variation selectors that attach invisibly to the
/// glyph before them. They cannot reorder a line the way a bidi override can,
/// which is why they are not in [`is_format`]; they can still hide text, so
/// they are [`is_unsafe`] all the same (L4 of the S8/S9 review). A range added
/// by a later Unicode version is a one-line addition here, exactly as in
/// [`is_format`].
fn is_invisible(c: char) -> bool {
    matches!(
        c,
        '\u{115f}' | '\u{1160}'             // Hangul choseong filler, jungseong filler
        | '\u{2800}'                        // braille pattern blank
        | '\u{3164}'                        // Hangul filler
        | '\u{ffa0}'                        // halfwidth Hangul filler
        | '\u{fe00}'..='\u{fe0f}'           // variation selectors 1-16
        | '\u{e0100}'..='\u{e01ef}'         // variation selectors supplement
    )
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
    fn clean_line_strips_the_invisible_fillers_and_variation_selectors() {
        // Not `Cf`, so not `is_format` — but they render as nothing all the
        // same, and a line padded or decorated with them says something other
        // than what it appears to (L4 of the S8/S9 review).
        assert_eq!(clean_line("a\u{3164}b\u{ffa0}c\u{115f}d\u{1160}e\u{2800}f"), "abcdef");
        assert_eq!(clean_line("emoji\u{fe0f}x\u{fe0e}y"), "emojixy");
        assert_eq!(clean_line("supp\u{e0100}\u{e01ef}lement"), "supplement");
        // The neighbouring characters are NOT touched: U+FE10 (presentation
        // form for vertical), U+2801 (braille pattern dots-1) and U+3163 are
        // real text and stay.
        assert_eq!(clean_line("\u{fe10}\u{2801}\u{3163}"), "\u{fe10}\u{2801}\u{3163}");
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
