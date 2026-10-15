//! Shared command/arg/error glue used by every handler-bearing `graph` submodule:
//! positional/flag arg validation, the stage-error envelope, the
//! three-registry loader, and the text sanitizers (`clean_line`/`clip_flat`)
//! every surface that prints text this process did not write shares — mail
//! fragments in `view.rs`, a pulled node's own fields in `who.rs`.

use super::model::{
    hooks_path, load_stage, projects_path, sessions_path, HooksFile, ProjectsFile, SessionsFile,
};
use aoide_protocol::Invocation;
use aoide_protocol::output::{Fix, Kind, Outcome};
use serde_json::json;

/// The taught refusal of a command only a door that can approve
/// ([`aoide_protocol::Door::can_approve`]) may run, audited under that
/// command's own name, or `None` when the invocation's door may. One place, so
/// the commands that carry a human's approval (resolving a held entry, moving a
/// session's parent, launching with no spawn gate) refuse in one shape.
pub(in crate::graph) fn refuse_unapproving_door(
    inv: &Invocation,
    cmd: &str,
    what: String,
    why: &str,
    fix: Fix,
) -> Option<Outcome> {
    if inv.door.can_approve() {
        return None;
    }
    let out = Outcome::refuse(cmd, Kind::Refused, what, why, fix)
        .with_fields(json!({ "reason": "door-cannot-approve" }));
    super::pending::audit_pending(inv, cmd, "refused", &out.message, "");
    Some(out)
}

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

/// One character as a human reads it: itself when it is safe to print, else a
/// visible escape. A backslash is doubled, so an escape in the output is never
/// mistaken for text that spelled one (`printf '\x1b'` and a real ESC read
/// differently).
fn visible_char(c: char) -> String {
    match c {
        '\\' => "\\\\".to_string(),
        '\n' => "\\n".to_string(),
        '\r' => "\\r".to_string(),
        '\t' => "\\t".to_string(),
        c if c.is_ascii() && c.is_control() => format!("\\x{:02x}", c as u32),
        c if is_unsafe(c) || (c.is_whitespace() && c != ' ') => format!("\\u{{{:x}}}", c as u32),
        c => c.to_string(),
    }
}

/// Text someone else wrote, shown to a human who is about to act on it: every
/// character [`is_unsafe`] says a terminal must not be handed, and every
/// whitespace but a plain space (a no-break space reads as one and is not),
/// spelled as a visible escape (`\n`, `\x1b`, `\u{202e}`) instead of being
/// stripped or printed. What replays is what is shown, never a cleaner
/// version of it. `pub` so the conductor's approval views reach the one rule.
pub fn visible(s: &str) -> String {
    s.chars().map(visible_char).collect()
}

/// [`visible`] cut to `max` output characters with an ellipsis, never inside
/// an escape.
pub(in crate::graph) fn visible_clip(s: &str, max: usize) -> String {
    let pieces: Vec<String> = s.chars().map(visible_char).collect();
    if pieces.iter().map(|p| p.chars().count()).sum::<usize>() <= max {
        return pieces.concat();
    }
    let mut out = String::new();
    let mut width = 0;
    for piece in &pieces {
        let w = piece.chars().count();
        if width + w > max.saturating_sub(1) {
            break;
        }
        out.push_str(piece);
        width += w;
    }
    out.push('…');
    out
}

/// Terminal escape sequences and control bytes removed from text the pty
/// wrote, leaving printable text plus `\n`, `\r` and `\t`.
///
/// A tokenizer by ECMA-48's byte classes, not terminal emulation: it finds
/// where each sequence ends and interprets none of them. CSI runs to its final
/// byte, OSC to BEL or ST, DCS, SOS, PM and APC (`ESC P`, `ESC X`, `ESC ^`,
/// `ESC _`) to ST alone, an escape with an intermediate byte (`ESC ( B`) to its
/// final byte, and any other two-byte escape is ESC and one byte. A sequence
/// still open at the end of the input emits nothing. `\r` is kept so a reader
/// can take a line's last rewrite (a progress frame) off it.
///
/// `pub` because `aoide-conductor`'s log tail renders through the same rule.
pub fn strip_ansi(input: &str) -> String {
    strip_ansi_open(input).0
}

/// [`strip_ansi`], and the byte offset of the ESC that opens a sequence still
/// open at the end of `input`, if there is one. A reader that judges one chunk
/// of output by itself holds the bytes from that offset, so a sequence split
/// across two valid UTF-8 reads is judged whole.
pub(in crate::graph) fn strip_ansi_open(input: &str) -> (String, Option<usize>) {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum State {
        Normal,
        Esc,
        Csi,
        Osc,
        OscEsc,
        Str,
        StrEsc,
        EscIntermediate,
    }
    let mut out = String::with_capacity(input.len());
    let mut state = State::Normal;
    let mut opened = 0;
    for (at, ch) in input.char_indices() {
        if state == State::Normal && ch == '\x1b' {
            opened = at;
        }
        state = match (state, ch) {
            (State::Normal, '\x1b') => State::Esc,
            (State::Normal, '\n' | '\t' | '\r') => {
                out.push(ch);
                State::Normal
            }
            (State::Normal, c) if (c as u32) < 0x20 || c as u32 == 0x7f => State::Normal,
            (State::Normal, c) => {
                out.push(c);
                State::Normal
            }
            (State::Esc, '[') => State::Csi,
            (State::Esc, ']') => State::Osc,
            (State::Esc, 'P' | 'X' | '^' | '_') => State::Str,
            (State::Esc, c) if (0x20..=0x2f).contains(&(c as u32)) => State::EscIntermediate,
            (State::Esc, _) => State::Normal,
            (State::Csi, c) if (0x40..=0x7e).contains(&(c as u32)) => State::Normal,
            (State::Csi, _) => State::Csi,
            (State::Osc, '\x07') => State::Normal,
            (State::Osc, '\x1b') => State::OscEsc,
            (State::Osc, _) => State::Osc,
            (State::OscEsc, '\\') => State::Normal,
            (State::OscEsc, _) => State::Osc,
            (State::Str, '\x1b') => State::StrEsc,
            (State::Str, _) => State::Str,
            (State::StrEsc, '\\') => State::Normal,
            (State::StrEsc, _) => State::Str,
            (State::EscIntermediate, c) if (0x30..=0x7e).contains(&(c as u32)) => State::Normal,
            (State::EscIntermediate, _) => State::EscIntermediate,
        };
    }
    let open = (state != State::Normal).then_some(opened);
    (out, open)
}

/// What a pty's INPUT says once its escape sequences are gone: the keys typed.
/// The same tokenizer as [`strip_ansi`], with one input-only form in front of
/// it. An X10 mouse report is `ESC [ M` and three raw bytes whose values may be
/// printable, so the report is dropped whole, as bytes, before the text pass
/// sees it. Output never takes this path: there `ESC [ M` is delete-line.
pub fn typed_text(input: &[u8]) -> String {
    let mut kept = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        if input[i..].starts_with(b"\x1b[M") {
            i += 6;
        } else {
            kept.push(input[i]);
            i += 1;
        }
    }
    strip_ansi(&String::from_utf8_lossy(&kept))
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

    /// `visible` spells what `clean_line` would drop, so an approver sees
    /// every byte that will replay: controls, the bidi override, a no-break
    /// space. A backslash is doubled so a literal `\x1b` and a real ESC differ.
    #[test]
    fn visible_spells_every_unsafe_character_and_never_drops_one() {
        assert_eq!(visible("a\nb\r\tc"), "a\\nb\\r\\tc");
        assert_eq!(visible("\u{1b}[31m\u{7}"), "\\x1b[31m\\x07");
        assert_eq!(visible("evil\u{202e}txt\u{200b}"), "evil\\u{202e}txt\\u{200b}");
        assert_eq!(visible("a\u{a0}b\u{2028}c\u{85}"), "a\\u{a0}b\\u{2028}c\\u{85}");
        assert_eq!(visible("printf '\\x1b'"), "printf '\\\\x1b'");
        assert_ne!(visible("\\x1b"), visible("\u{1b}"), "a spelled escape is not an ESC");
        assert_eq!(visible("plain text é 日本 — ok"), "plain text é 日本 — ok");
        assert!(visible("a\nb\u{1b}\u{202e}").chars().all(|c| !is_unsafe(c)));
    }

    /// The clip counts escaped characters and stops before an escape, never
    /// inside one, and marks that it cut.
    #[test]
    fn visible_clip_cuts_between_escapes_and_marks_it() {
        assert_eq!(visible_clip("short\n", 10), "short\\n");
        assert_eq!(visible_clip("ab\ncd", 4), "ab…", "the \\n that would straddle the cut is dropped whole");
        assert_eq!(visible_clip("abcdef", 4), "abc…");
        assert_eq!(visible_clip("a\u{1b}", 3), "a…");
    }

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

    #[test]
    fn strip_ansi_drops_a_dcs_string_only_at_its_st() {
        // DCS payload is not text, and a BEL inside it does not end it.
        assert_eq!(strip_ansi("a\x1bP1$r0m\x07 x\x1b\\b"), "ab");
        assert_eq!(strip_ansi("a\x1bPq never closed"), "a");
    }

    #[test]
    fn strip_ansi_keeps_an_output_delete_line_as_a_sequence_with_no_payload() {
        // Output's `ESC [ M` is delete-line: the text after it is text.
        assert_eq!(strip_ansi("x\x1b[Mtext"), "xtext");
    }

    #[test]
    fn typed_text_drops_an_x10_mouse_report_whole_even_when_its_bytes_are_printable() {
        // Button, column and row are raw bytes; `#`, `!`, `!` are printable.
        assert_eq!(typed_text(b"\x1b[M#!!"), "");
        assert_eq!(typed_text(b"y\x1b[M#!!n"), "yn");
        // Non-UTF-8 coordinate bytes are dropped too, not read as text.
        assert_eq!(typed_text(b"\x1b[M\xff\xfe\xfd"), "");
    }

    #[test]
    fn typed_text_drops_osc_and_dcs_replies_and_keeps_the_keys() {
        assert_eq!(typed_text(b"\x1b]11;rgb:0/0/0\x07"), "");
        assert_eq!(typed_text(b"\x1bP>|xterm\x1b\\"), "");
        assert_eq!(typed_text(b"q\x1b]10;?\x1b\\r"), "qr");
        assert_eq!(typed_text(b"\x1b[<0;10;5M"), "", "an SGR mouse report is a CSI");
    }

    #[test]
    fn strip_ansi_removes_sgr_and_keeps_the_text_between() {
        assert_eq!(strip_ansi("plain \x1b[1;31mred bold\x1b[0m plain again"), "plain red bold plain again");
    }

    #[test]
    fn strip_ansi_ends_osc_at_bel_or_st() {
        assert_eq!(strip_ansi("a\x1b]0;title one\x07b\x1b]0;title two\x1b\\c"), "abc");
    }

    #[test]
    fn strip_ansi_ends_an_intermediate_escape_at_its_final_byte() {
        // ESC ( B selects ASCII as G0: the `(` is an intermediate, the `B` final.
        assert_eq!(strip_ansi("before\x1b(Bafter"), "beforeafter");
    }

    #[test]
    fn strip_ansi_drops_control_bytes_but_keeps_line_breaks_and_tabs() {
        assert_eq!(strip_ansi("a\x00b\x07c\x08\td\r\ne\x7f"), "abc\td\r\ne");
    }

    #[test]
    fn strip_ansi_emits_nothing_for_a_sequence_left_open() {
        assert_eq!(strip_ansi("kept\x1b[31"), "kept");
        assert_eq!(strip_ansi("kept\x1b]0;no terminator"), "kept");
    }

    #[test]
    fn strip_ansi_keeps_multibyte_text_next_to_escapes() {
        assert_eq!(strip_ansi("café \x1b[31m日本語\x1b[0m more"), "café 日本語 more");
    }
}
