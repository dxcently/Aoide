//! Shared command/arg/error glue used by every handler-bearing `graph` submodule:
//! positional/flag arg validation, the stage-error envelope, the
//! three-registry loader, the escape strippers for pty text, and the
//! re-export of `aoide_protocol::untrusted`'s sanitizers (`clean_line`/
//! `clip_flat`) every surface that prints text this process did not write
//! shares — mail fragments in `view.rs`, a pulled node's own fields in `who.rs`.

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

// The untrusted-line sanitizer is `aoide_protocol::untrusted`'s, one table for
// every crate (this one's callers keep these paths, and `graph::clean_line`
// re-exports the first).
pub use aoide_protocol::untrusted::clean_line;
pub(in crate::graph) use aoide_protocol::untrusted::{clip_flat, is_unsafe, strip_unsafe, LINE_MAX};

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
        c if is_unsafe(c) || (c.is_whitespace() && c != ' ') || is_mark_or_ignorable(c) => {
            format!("\\u{{{:x}}}", c as u32)
        }
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

/// [`visible`] for a held launch, every non-ASCII character spelled too: a
/// Cyrillic `а` or a combining mark looks like the ASCII text it imitates, so an
/// approver of a command reads only the ASCII it is shown. A held spawn's
/// command goes through this, and nothing else does.
pub fn visible_held(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii() { visible_char(c) } else { format!("\\u{{{:x}}}", c as u32) })
        .collect()
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
