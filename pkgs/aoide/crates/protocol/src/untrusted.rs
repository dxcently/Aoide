//! Text this process did not write, made safe to put in front of a person:
//! one table of the characters no terminal may be handed ([`is_unsafe`]),
//! the filter that removes them ([`strip_unsafe`]), the flatten-and-clip
//! ([`clip_flat`]), and the one-line sanitizer built from the two
//! ([`clean_line`]). It lives in this leaf crate so every crate above it —
//! `aoide-client` for a far door's words in a mail answer, `aoide-conduct`
//! for its graph views, `aoide-server` for its A2A door — cleans through the
//! same table, never a copy of it.

/// The longest untrusted line [`clean_line`] prints, in characters —
/// [`clip_flat`]'s bound, one constant for every caller (a mail fragment, a
/// far node's own `cwd`, a link's `sessionId`).
pub const LINE_MAX: usize = 200;

/// One line of text this process did not write, made safe to print: every
/// control character stripped — `\r` included, it is an Enter at whoever
/// pastes it — every Unicode `Cf` mark stripped too ([`is_unsafe`], which
/// includes the zero-width and bidi characters that would otherwise survive
/// as an invisible instruction), whitespace flattened, clipped to
/// [`LINE_MAX`] with an ellipsis. The single sanitizer every door reaches a
/// terminal through with a peer's bytes — `aoide-conduct`'s graph views (mail
/// fragments, a run's own output lines, a pulled node document's own fields,
/// re-exported there as `graph::clean_line`) and `aoide-server`'s A2A door (a
/// slug echoed back in a refusal) — so no surface can hold a laxer rule than
/// another. It lives in this leaf crate so a caller below `aoide-conduct`
/// reaches the same table rather than writing a second one.
pub fn clean_line(s: &str) -> String {
    clip_flat(&strip_unsafe(s), LINE_MAX)
}

/// The strip half of [`clean_line`], on its own: every character [`is_unsafe`]
/// refuses, gone. Split out because a caller with its OWN clip — the
/// ping-back's 80-character `SAY_MAX`, not this module's [`LINE_MAX`] — must
/// not carry a second copy of the `is_unsafe`/`is_format` rule to reach it:
/// one table, one filter, and every sanitizer keeps the same definition of
/// "unsafe" for free.
pub fn strip_unsafe(s: &str) -> String {
    s.chars().filter(|c| !is_unsafe(*c)).collect()
}

/// A character no terminal may be handed: every [`char::is_control`]
/// (`\u{1b}` colour/bell/`\r`/`\n`) plus every Unicode `Cf` (FORMAT)
/// character — the invisible marks a terminal honours but `is_control` does
/// not, which is what lets one string read as another. Every sanitizer judges
/// through it: [`clean_line`] and [`strip_unsafe`] here, `aoide-conduct`'s
/// `clean_block` for the multi-line texts (a letter body, a run's
/// instructions), and its ping-back's `SAY_MAX`-clipped `clean` for a line
/// bound for a composer.
///
/// The line and paragraph separators, U+2028 and U+2029, are unsafe as well: a
/// terminal breaks a line at either, and [`str::lines`] does not count them.
pub fn is_unsafe(c: char) -> bool {
    c.is_control() || is_format(c) || is_invisible(c) || matches!(c, '\u{2028}' | '\u{2029}')
}

/// The text an audit writer stores for a character no terminal may be handed.
/// Only what a JSON writer leaves raw is spelled here: DEL, the C1 controls,
/// `Cf` and the invisible set. A C0 control is already a JSON escape (`\u001b`,
/// `\n`) and stays one, so a decoded value keeps the caller's text for it. The
/// spelling is ASCII text, so the JSON writer escapes its backslash too: the
/// raw line reads `\\u{202e}`, and a decoder hands back the visible `\u{202e}`,
/// never the character itself.
pub fn escape_for_log(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if u32::from(c) >= 0x7f && is_unsafe(c) {
            out.extend(c.escape_unicode());
        } else {
            out.push(c);
        }
    }
    out
}

/// [`escape_for_log`] over every string a JSON value holds, for a writer that
/// serialises a `serde_json::Value` rather than a typed record.
pub fn escape_for_log_value(value: &serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match value {
        Value::String(s) => Value::String(escape_for_log(s)),
        Value::Array(items) => Value::Array(items.iter().map(escape_for_log_value).collect()),
        Value::Object(map) => Value::Object(map.iter().map(|(k, v)| (k.clone(), escape_for_log_value(v))).collect()),
        other => other.clone(),
    }
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
        | '\u{034f}'                        // combining grapheme joiner
        | '\u{180b}'..='\u{180d}'           // Mongolian free variation selectors 1-3
        | '\u{180f}'                        // Mongolian free variation selector 4
        | '\u{17b4}'..='\u{17b5}'           // Khmer vowel inherent aq, aa
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
pub fn clip_flat(s: &str, max: usize) -> String {
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
    fn line_separators_and_the_fillers_are_unsafe() {
        for c in ['\u{2028}', '\u{2029}', '\u{34f}', '\u{180b}', '\u{180c}', '\u{180d}', '\u{17b4}', '\u{17b5}', '\u{180f}'] {
            assert!(is_unsafe(c), "U+{:04X} must be unsafe", u32::from(c));
        }
    }

    #[test]
    fn clean_line_strips_the_combining_joiner_and_the_mongolian_and_khmer_fillers() {
        assert_eq!(clean_line("a\u{34f}b\u{180b}c\u{180d}d\u{17b4}e\u{17b5}f\u{180f}g"), "abcdefg");
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
