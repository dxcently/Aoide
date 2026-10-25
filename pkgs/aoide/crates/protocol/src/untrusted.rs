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

/// The one predicate for "[`visible`] spells this character": a backslash, every
/// [`is_unsafe`] character, every whitespace but a plain space, and the marks and
/// ignorables. `visible_char` and the JSON writers below all read it, so the
/// table is never copied.
fn would_spell(c: char) -> bool {
    c == '\\' || is_unsafe(c) || (c.is_whitespace() && c != ' ') || is_mark_or_ignorable(c)
}

/// One character as a human reads it: itself when it is safe to print, else a
/// visible escape. A backslash is doubled, so an escape in the output is never
/// mistaken for text that spelled one (`printf '\x1b'` and a real ESC read
/// differently).
fn visible_char(c: char) -> String {
    if !would_spell(c) {
        return c.to_string();
    }
    match c {
        '\\' => "\\\\".to_string(),
        '\n' => "\\n".to_string(),
        '\r' => "\\r".to_string(),
        '\t' => "\\t".to_string(),
        c if c.is_ascii() && c.is_control() => format!("\\x{:02x}", c as u32),
        c => format!("\\u{{{:x}}}", c as u32),
    }
}

/// Text someone else wrote, shown to a human who is about to act on it: every
/// character [`is_unsafe`] says a terminal must not be handed, and every
/// whitespace but a plain space (a no-break space reads as one and is not),
/// spelled as a visible escape (`\n`, `\x1b`, `\u{202e}`) instead of being
/// stripped or printed. What replays is what is shown, never a cleaner
/// version of it. `pub` so the conductor's approval views, and the desktop
/// app's ask dialog, reach the one rule.
pub fn visible(s: &str) -> String {
    s.chars().map(visible_char).collect()
}

/// [`visible`] for a held launch, every non-ASCII character spelled too: a
/// Cyrillic `а` or a combining mark looks like the ASCII text it imitates, so an
/// approver of a command reads only the ASCII it is shown. A held spawn's
/// command goes through this, and nothing else does.
pub fn visible_held(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii() { visible_char(c) } else { format!("\\u{{{:x}}}", c as u32) })
        .collect()
}

/// An untrusted JSON value as a dialog or a pane shows it: pretty, with every
/// object's keys sorted at every depth, and every character [`visible`] would
/// spell written as a JSON `\uXXXX` escape (a character past the BMP as its
/// UTF-16 pair). JSON's own escapes are unchanged, so the text parses back to
/// the same value, holds no raw character `visible` would spell, and doubles no
/// backslash.
pub fn visible_json(value: &serde_json::Value) -> String {
    escape_json(&serde_json::to_string_pretty(&sorted(value)).expect("a json value serializes"))
}

/// [`visible_json`] on one line, for a command line's body.
pub fn visible_json_line(value: &serde_json::Value) -> String {
    escape_json(&serde_json::to_string(&sorted(value)).expect("a json value serializes"))
}

/// JSON text with each character [`would_spell`] names, inside a string, written
/// as a JSON `\uXXXX` escape. A JSON escape is copied as it is, so no backslash
/// is doubled, and structure outside a string (the pretty printer's newlines and
/// indents) is never touched.
fn escape_json(json: &str) -> String {
    let mut out = String::with_capacity(json.len());
    let mut in_string = false;
    let mut chars = json.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                in_string = !in_string;
                out.push(c);
            }
            '\\' if in_string => {
                out.push(c);
                out.extend(chars.next());
            }
            c if in_string && would_spell(c) => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    out.push_str(&format!("\\u{unit:04x}"));
                }
            }
            c => out.push(c),
        }
    }
    out
}

/// The value with every object's keys in sorted order, at every depth. The
/// order is set here, not left to serde_json's map, which a `preserve_order`
/// feature anywhere in the build would change.
fn sorted(value: &serde_json::Value) -> serde_json::Value {
    use serde_json::{Map, Value};
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let entries = keys.into_iter().map(|k| (k.clone(), sorted(&map[k])));
            Value::Object(entries.collect::<Map<String, Value>>())
        }
        Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
        other => other.clone(),
    }
}

/// Marks that draw over or into their neighbours (combining diacritics, the
/// Mn/Me blocks that carry most overlays) and the default-ignorable code points
/// a terminal or a font may draw as nothing. A reader cannot tell these from
/// the text around them, so they are spelled, never printed.
fn is_mark_or_ignorable(c: char) -> bool {
    matches!(c as u32,
        0x034F
        | 0x0300..=0x036F
        | 0x115F..=0x1160
        | 0x17B4..=0x17B5
        | 0x180B..=0x180F
        | 0x1AB0..=0x1AFF
        | 0x1DC0..=0x1DFF
        | 0x2060..=0x206F
        | 0x20D0..=0x20FF
        | 0x3164
        | 0xFE00..=0xFE0F
        | 0xFE20..=0xFE2F
        | 0xFEFF
        | 0xFFA0
        | 0xFFF0..=0xFFF8
        | 0xE0000..=0xE0FFF)
}

/// The text an audit writer stores for a character no terminal may be handed.
/// Only what a JSON writer leaves raw is spelled here: DEL, the C1 controls,
/// `Cf` and the invisible set. A C0 control is already a JSON escape (`\u001b`,
/// `\n`) and stays one, so a decoded value keeps the caller's text for it.
///
/// A spelled character reads `\u{hex}`. A literal backslash is doubled, two out
/// for one in, exactly when the character after it in the output is a backslash
/// or `u`. A spelled character starts with a backslash, so a backslash before
/// one is doubled too, and every other backslash is written as it was. The
/// typed text `\u{202e}` reads `\\u{202e}` and U+202E reads `\u{202e}`, so
/// distinct inputs never share an output. The mapping is not idempotent: escape
/// each value exactly once: escaping an escape again doubles its backslash.
pub fn escape_for_log(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            let doubled = match chars.peek() {
                Some('\\' | 'u') => true,
                Some(&next) => spelled(next),
                None => false,
            };
            out.push_str(if doubled { r"\\" } else { r"\" });
        } else if spelled(c) {
            out.extend(c.escape_unicode());
        } else {
            out.push(c);
        }
    }
    out
}

/// What [`escape_for_log`] spells: an [`is_unsafe`] character at DEL or above.
/// A C0 control stays raw, so the backslash rule reads it as raw text too.
fn spelled(c: char) -> bool {
    u32::from(c) >= 0x7f && is_unsafe(c)
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

    #[test]
    fn a_typed_escape_and_the_character_it_names_escape_apart() {
        assert_eq!(escape_for_log(r"\u{202e}"), r"\\u{202e}");
        assert_eq!(escape_for_log("\u{202e}"), r"\u{202e}");
    }

    #[test]
    fn a_backslash_before_an_escaped_character_is_doubled() {
        assert_eq!(escape_for_log("\\\u{202e}"), r"\\\u{202e}");
        assert_eq!(escape_for_log(r"\u{202e}"), r"\\u{202e}");
    }

    #[test]
    fn a_backslash_before_a_backslash_is_doubled() {
        assert_eq!(escape_for_log(r"\\"), r"\\\");
        assert_eq!(escape_for_log(r"\"), r"\");
    }

    #[test]
    fn other_backslashes_are_written_as_they_were() {
        assert_eq!(escape_for_log(r"a\nb"), r"a\nb");
        assert_eq!(escape_for_log(r"C:\Users\x"), r"C:\Users\x");
    }

    /// Every string of up to three tokens escapes to a text no other string
    /// escapes to. The set holds a typed escape and the character it names.
    #[test]
    fn escape_for_log_is_one_to_one_over_short_strings() {
        let tokens = ["\\", "u", "\\u{202e}", "\u{202e}", "\u{1b}", "a"];
        let mut strings = vec![String::new()];
        let mut level = vec![String::new()];
        for _ in 0..3 {
            level = level
                .iter()
                .flat_map(|s| tokens.iter().map(move |t| format!("{s}{t}")))
                .collect();
            strings.extend(level.iter().cloned());
        }
        let mut owner = std::collections::HashMap::new();
        for s in &strings {
            let out = escape_for_log(s);
            let earlier = owner.insert(out.clone(), s);
            assert!(
                earlier.is_none(),
                "{earlier:?} and {s:?} both escape to {out:?}"
            );
        }
    }

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

    /// A combining mark and a default-ignorable are spelled, where they used to
    /// print raw: a mark draws over its neighbour and an ignorable can hide
    /// one, so neither may read as the text it sits in. Before the change each
    /// of these came out byte-identical.
    #[test]
    fn visible_spells_combining_marks_and_default_ignorables() {
        assert_eq!(visible("pro\u{34f}ject"), "pro\\u{34f}ject");
        assert_eq!(visible("c\u{336}u\u{336}rl"), "c\\u{336}u\\u{336}rl");
        assert_eq!(visible("\u{180b}x\u{fe0f}"), "\\u{180b}x\\u{fe0f}");
        assert_eq!(visible("e\u{301}"), "e\\u{301}", "a precomposed-free accent is spelled too");
        assert_eq!(visible("é 日本"), "é 日本", "plain letters and CJK are not marks");
    }

    /// A held launch's view is ASCII: a Cyrillic letter that looks like an ASCII
    /// one is spelled, so the approver reads the command as it runs.
    #[test]
    fn visible_held_spells_every_non_ascii_character() {
        assert_eq!(visible_held("/home/khoa/pr\u{43e}ject"), "/home/khoa/pr\\u{43e}ject");
        assert_eq!(visible_held("a b\nc"), "a b\\nc");
        assert_eq!(visible("/home/khoa/pr\u{43e}ject"), "/home/khoa/pr\u{43e}ject", "the general view keeps letters");
    }

    /// The characters a JSON writer must spell: ESC, CR, LF and TAB (JSON's own
    /// escapes for the last three, `\u001b` for ESC), U+009B (raw in serde_json),
    /// a bidi override, a zero-width space, a combining mark, a tag character past
    /// the BMP, a backslash and a quote.
    const JSON_SPELLED: [char; 11] =
        ['\u{1b}', '\r', '\n', '\t', '\u{9b}', '\u{202e}', '\u{200b}', '\u{301}', '\u{e0001}', '\\', '"'];

    /// The characters of `json` that [`would_spell`] names and that stand as
    /// themselves: inside a string, outside a JSON escape.
    fn raw_spelled(json: &str) -> Vec<char> {
        let mut found = Vec::new();
        let mut in_string = false;
        let mut chars = json.chars();
        while let Some(c) = chars.next() {
            match c {
                '"' => in_string = !in_string,
                '\\' if in_string => {
                    chars.next();
                }
                c if in_string && would_spell(c) => found.push(c),
                _ => {}
            }
        }
        found
    }

    /// Each character `visible` would spell, in a key and in a string, comes out
    /// as one JSON escape: the output parses back to the input, holds no raw
    /// spelled character, and shows a backslash once, never four times. The
    /// backslash and the quote are JSON's own escapes.
    #[test]
    fn visible_json_escapes_each_spelled_character_in_a_key_and_a_string() {
        for c in JSON_SPELLED {
            let mut map = serde_json::Map::new();
            map.insert(format!("k{c}"), serde_json::Value::String(format!("v{c}")));
            let value = serde_json::Value::Object(map);
            for out in [visible_json(&value), visible_json_line(&value)] {
                assert_eq!(serde_json::from_str::<serde_json::Value>(&out).unwrap(), value, "{c:?}: {out}");
                assert!(raw_spelled(&out).is_empty(), "{c:?}: {out}");
                assert!(!out.contains(r"\\\\"), "{c:?}: {out}");
                if c == '\\' {
                    assert!(out.contains(r"\\"), "{c:?}: {out}");
                }
                if c == '"' {
                    assert!(out.contains(r#"\""#), "{c:?}: {out}");
                }
            }
        }
    }

    /// A character past the BMP is written as its UTF-16 pair, as JSON needs.
    #[test]
    fn visible_json_writes_a_character_past_the_bmp_as_a_surrogate_pair() {
        assert_eq!(visible_json_line(&serde_json::json!("\u{e0001}")), r#""\udb40\udc01""#);
    }

    /// A backslash in a value is one JSON escape, and the escaper leaves it be.
    #[test]
    fn visible_json_does_not_double_a_backslash() {
        assert_eq!(visible_json_line(&serde_json::json!({"path": "C:\\x"})), r#"{"path":"C:\\x"}"#);
    }

    /// The pretty form keeps its layout, with the newlines and indents outside
    /// its strings, and the line form has no newline at all.
    #[test]
    fn visible_json_is_pretty_and_the_line_form_is_one_line() {
        let value = serde_json::json!({"a": [1, {"b": "x\ny"}]});
        let pretty = visible_json(&value);
        assert!(pretty.contains("\n  \"a\": ["), "{pretty}");
        assert!(pretty.contains(r#""x\ny""#), "{pretty}");
        assert_eq!(serde_json::from_str::<serde_json::Value>(&pretty).unwrap(), value);
        assert!(!visible_json_line(&value).contains('\n'));
    }

    /// Keys come out sorted at every depth. A serde_json map is a BTreeMap unless
    /// the `preserve_order` feature is on, so this test exercises the sort only
    /// where that feature is on, which neither lock enables today.
    #[test]
    fn visible_json_sorts_keys_at_every_depth() {
        let value = serde_json::json!({"b": 1, "a": {"d": 2, "c": [{"z": 0, "y": 0}]}});
        let out = visible_json_line(&value);
        assert!(out.find(r#""a""#) < out.find(r#""b""#), "{out}");
        assert!(out.find(r#""c""#) < out.find(r#""d""#), "{out}");
        assert!(out.find(r#""y""#) < out.find(r#""z""#), "{out}");
    }

    /// `visible` spells a character exactly when [`would_spell`] names it, for
    /// every scalar value. The JSON writers read that same predicate.
    #[test]
    fn visible_spells_exactly_the_characters_would_spell_names() {
        for n in 0..=0x10_ffff_u32 {
            let Some(c) = char::from_u32(n) else { continue };
            let spelled = visible(&c.to_string()) != c.to_string();
            assert_eq!(spelled, would_spell(c), "U+{n:04X}");
        }
    }
}
