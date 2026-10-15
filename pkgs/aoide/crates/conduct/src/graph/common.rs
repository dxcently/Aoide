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

// The untrusted-line sanitizer is `aoide_protocol::untrusted`'s, one table for
// every crate (this one's callers keep these paths, and `graph::clean_line`
// re-exports the first).
pub use aoide_protocol::untrusted::clean_line;
pub(in crate::graph) use aoide_protocol::untrusted::{clip_flat, is_unsafe, strip_unsafe, LINE_MAX};

/// How many characters of a field [`clip`] reads. [`clean_line`] keeps at most
/// `LINE_MAX`, so a few lines' worth is all it can use of a field of any length.
const CLIP_READ: usize = 4 * LINE_MAX;

/// `text` as one clean line of at most `LINE_MAX` characters. Only its head is
/// read, so a field costs the same however long its author made it, and so does
/// every copy of it that an answer repeats once per query word.
pub(in crate::graph) fn clip(text: &str) -> String {
    match text.char_indices().nth(CLIP_READ) {
        // The head is cut only when the rest holds something a reader would
        // see. The ellipsis is added after the head is flattened, so a head
        // of padding cannot hide the cut.
        Some((end, _)) if text[end..].chars().any(|c| !c.is_whitespace() && !is_unsafe(c)) => {
            clip_flat(&format!("{}…", clip_flat(&strip_unsafe(&text[..end]), usize::MAX)), LINE_MAX)
        }
        Some((end, _)) => clean_line(&text[..end]),
        None => clean_line(text),
    }
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

    #[test]
    fn a_field_cut_after_padding_still_says_it_was_cut() {
        let padded = format!("a{}x", " ".repeat(900));
        let clipped = clip(&padded);
        assert_eq!(clipped, "a…", "the cut is marked even though the head flattens short");
        assert_eq!(clip(&format!("a{}", " ".repeat(900))), "a", "trailing padding alone is no cut");
        assert_eq!(clip(&"b".repeat(1000)).chars().count(), LINE_MAX);
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
