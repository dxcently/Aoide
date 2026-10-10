//! The screen source of `awaiting`: **a conducted pty that has gone quiet on a
//! question.** A harness that reports through hooks or a trace says when it is
//! waiting on a human; a program that does neither (`codex`, `aider`, a REPL, an
//! installer in a plain conducted shell) just prints `Apply patch? [y/N]` and
//! blocks, and nobody hears about it. This module reads that off the byte stream
//! the wrap already relays, with no terminal emulation and no classifier.
//!
//! The rule, evaluated by the wrap's ~1s tick ([`PromptWatch::standing`]):
//!
//! ```text
//! output quiet for QUIET   AND   the last non-empty line matches ONE row of ROWS
//! AND nothing was typed into that line since it was written
//! ```
//!
//! The wrapped child still running is the caller's fact (the multiplexer is
//! still looping). Output resuming, or input typed into the pty (the human's
//! stdin and an injected send alike), ends the episode: new output moves
//! `QUIET`'s clock, and typed input marks the line it landed on as answered,
//! which is what makes a no-echo password prompt end when the password is typed
//! and a half-typed command at an idle shell prompt never read as a question.
//!
//! **The table is closed and conservative.** A false `awaiting` is noise, so
//! every row is anchored to the END of the line (`Do you want to continue? [Y/n]`
//! is a question; `use [y/N] to answer` and a log line that merely contains
//! `password` are not), and nothing after the matched prompt can exist: the row
//! matched is the whole line. `QUIET` is 3 seconds: long enough that a prompt
//! answered by a script or an `expect`-style sender in milliseconds never flaps,
//! short enough that a human is not left waiting behind the ~1s tick, and the
//! same order as the 2s window the sudo text-scan this table replaced used.
//!
//! **The line is foreign text (house rule 4).** It is stripped of escape
//! sequences and every [`is_unsafe`](super::common::is_unsafe) character,
//! flattened to one line, its `"` turned to `'` so the delivered quote cannot be
//! closed from inside, and cut to [`QUESTION_MAX`] keeping its END, which is
//! where the question is. It is held in memory as the current and the last
//! non-empty line only, and leaves this module as the one capped `question`.
//! Screens that match nothing are never kept, never logged.
//!
//! The stream is read as lines, not as a screen: a cursor move to another row is
//! a line break and a carriage return rewrites the line, but a full-screen
//! program's last bytes are not its last row. Such a program matches only when
//! it happens to end a quiet stream on a prompt, which is the safe direction.
//! Pure text over the pty: no `/proc`, no per-host branch.

use super::common::strip_unsafe;
use std::time::{Duration, Instant};

/// How long the pty must be silent before its last line is believed to be a
/// question rather than a pause.
pub(in crate::graph) const QUIET: Duration = Duration::from_secs(3);

/// The longest question the wrap stores and a parent is told, in characters.
/// One constant for both ends: the ping-back's quote clips at the same bound
/// (`pingback::SAY_MAX`, pinned equal by a test there).
pub(in crate::graph) const QUESTION_MAX: usize = 80;

/// The most bytes of one unterminated line kept: only its end is ever read.
const LINE_BYTES_MAX: usize = 4096;
/// The most bytes an unterminated escape sequence may swallow before it is
/// abandoned, so a stray `ESC ]` cannot eat the stream forever.
const ESCAPE_BYTES_MAX: usize = 4096;
/// The most text that may follow `press enter` for the line to still be that
/// prompt (` to continue, or Ctrl-C to abort.`) and not a sentence about it.
const PRESS_TAIL_MAX: usize = 40;
/// The most text that may follow `password for ` / `passphrase for `: a user
/// name or a key path.
const FOR_TAIL_MAX: usize = 80;

/// One question a wrapped program is waiting on, as the wrap publishes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::graph) struct Prompt {
    /// The sanitised, capped last line: the only screen text that is kept.
    pub question: String,
    /// The name of the row that matched it. Never stored.
    pub row: &'static str,
}

impl Prompt {
    /// The matched row is the `sudo` prompt, so the record also carries
    /// `needsSudo` (the dock's lock badge): it is the human's password, not an
    /// agent's question.
    pub(in crate::graph) fn sudo(&self) -> bool {
        self.row == SUDO_ROW
    }
}

/// The row that was the text-scan for `sudo`'s prompt before this table existed.
const SUDO_ROW: &str = "sudo";

/// One named entry of the table. `matches` reads the line lowercased, flattened
/// and with trailing whitespace gone.
struct Row {
    name: &'static str,
    matches: fn(&str) -> bool,
}

/// The whole table, in the order a line is tried. Adding a row is adding a line
/// here and a test beside the others; nothing else reads it.
const ROWS: &[Row] = &[
    Row { name: SUDO_ROW, matches: sudo },
    Row { name: "password", matches: password },
    Row { name: "y-n", matches: y_n },
    Row { name: "yes-no", matches: yes_no },
    Row { name: "press-key", matches: press_key },
    Row { name: "confirm", matches: confirm },
];

/// `l` without one trailing `?` or `:` (`Proceed [y/N]?`, `Overwrite (y/n):`).
fn ask_end(l: &str) -> &str {
    l.strip_suffix(|c: char| c == '?' || c == ':').unwrap_or(l).trim_end()
}

fn ends_with_word(l: &str, word: &str) -> bool {
    l.strip_suffix(word).is_some_and(|rest| !rest.ends_with(char::is_alphanumeric))
}

/// `[sudo] password for khoa:` — the prompt `sudo` prints at a line start.
fn sudo(l: &str) -> bool {
    l.starts_with("[sudo] password for") && l.ends_with(':')
}

/// `Password:`, `khoa@host's password:`, `doas (khoa@host) password:`,
/// `Enter passphrase for key '/home/khoa/.ssh/id_ed25519':` — a line that ENDS
/// in a colon after `password` or `passphrase`, optionally `for <name>`.
fn password(l: &str) -> bool {
    let Some(body) = l.strip_suffix(':').map(str::trim_end) else {
        return false;
    };
    if body.ends_with("password") || body.ends_with("passphrase") {
        return true;
    }
    ["password for ", "passphrase for "].iter().any(|key| {
        body.rfind(key).is_some_and(|i| {
            let tail = body[i + key.len()..].chars().count();
            (1..=FOR_TAIL_MAX).contains(&tail)
        })
    })
}

/// `[y/N]`, `[Y/n]`, `(y/n)` — the choice is the last thing on the line.
fn y_n(l: &str) -> bool {
    let b = ask_end(l);
    b.ends_with("[y/n]") || b.ends_with("(y/n)")
}

/// `(yes/no)`, and ssh's `(yes/no/[fingerprint])?`.
fn yes_no(l: &str) -> bool {
    let b = ask_end(l);
    b.ends_with("(yes/no)") || b.ends_with("[yes/no]") || b.ends_with("(yes/no/[fingerprint])")
}

/// `Press Enter to continue`, `Press any key to continue . . .` — the phrase
/// close to the end of the line, not a sentence that mentions it.
fn press_key(l: &str) -> bool {
    ["press enter", "press any key"].iter().any(|phrase| {
        l.rfind(phrase).is_some_and(|i| {
            !l[..i].ends_with(char::is_alphanumeric)
                && l[i + phrase.len()..].chars().count() <= PRESS_TAIL_MAX
        })
    })
}

/// A line that ends by asking: `Do you want to continue?`, `Proceed?`,
/// `Overwrite?`.
fn confirm(l: &str) -> bool {
    ["continue?", "proceed?", "overwrite?"].iter().any(|word| ends_with_word(l, word))
}

/// The row a flattened line matches, if any.
fn row_of(line: &str) -> Option<&'static Row> {
    let lowered = line.to_lowercase();
    ROWS.iter().find(|row| (row.matches)(&lowered))
}

/// Raw line bytes as one safe line: invalid UTF-8 replaced, every unsafe
/// character gone, whitespace flattened and trimmed, `"` softened to `'`.
fn flatten(raw: &[u8]) -> String {
    let text = String::from_utf8_lossy(raw);
    strip_unsafe(&text).split_whitespace().collect::<Vec<_>>().join(" ").replace('"', "'")
}

/// `s` cut to `max` characters keeping its END, with a leading `…`.
fn clip_end(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        return s.to_string();
    }
    let tail: String = s.chars().skip(n - (max - 1)).collect();
    format!("…{}", tail.trim_start())
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Escape {
    #[default]
    None,
    /// After `ESC`.
    Esc,
    /// Inside `ESC [ … <final>`.
    Csi,
    /// Inside an `ESC ]`/`P`/`_`/`^`/`X` string, up to `BEL` or `ESC \`.
    Str,
    /// An `ESC` seen inside such a string.
    StrEsc,
}

/// The last non-empty line of a byte stream, with the stream's escape sequences
/// removed as they arrive. Holds the line being written and the last one that
/// was finished, each with whether input was typed into it.
#[derive(Default)]
pub(in crate::graph) struct LineTail {
    cur: Vec<u8>,
    cur_typed: bool,
    last: Vec<u8>,
    last_typed: bool,
    escape: Escape,
    escape_len: usize,
    overwrite: bool,
}

impl LineTail {
    pub(in crate::graph) fn feed(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.push(b);
        }
    }

    /// Input was typed into the pty: whatever line is being written is answered.
    pub(in crate::graph) fn typed(&mut self) {
        self.cur_typed = true;
    }

    /// The last non-empty line and whether input landed on it. A fresh line
    /// that only input has touched counts as an answer to the line before it.
    pub(in crate::graph) fn line(&self) -> Option<(String, bool)> {
        let cur = flatten(&self.cur);
        if !cur.is_empty() {
            return Some((cur, self.cur_typed));
        }
        let last = flatten(&self.last);
        (!last.is_empty()).then(|| (last, self.last_typed || self.cur_typed))
    }

    fn push(&mut self, b: u8) {
        match self.escape {
            Escape::None => match b {
                0x1b => self.begin_escape(Escape::Esc),
                b'\n' => self.newline(),
                b'\r' => self.overwrite = true,
                0x08 => self.backspace(),
                b'\t' => self.text(b' '),
                0x00..=0x1f | 0x7f => {}
                _ => self.text(b),
            },
            Escape::Esc => match b {
                b'[' => self.escape = Escape::Csi,
                b']' | b'P' | b'_' | b'^' | b'X' => self.escape = Escape::Str,
                0x20..=0x2f => {}
                _ => self.escape = Escape::None,
            },
            Escape::Csi => match b {
                0x20..=0x3f => {}
                0x40..=0x7e => {
                    self.cursor_move(b);
                    self.escape = Escape::None;
                }
                _ => self.escape = Escape::None,
            },
            Escape::Str => match b {
                0x07 => self.escape = Escape::None,
                0x1b => self.escape = Escape::StrEsc,
                _ => {}
            },
            Escape::StrEsc => self.escape = Escape::None,
        }
        if self.escape != Escape::None {
            self.escape_len += 1;
            if self.escape_len > ESCAPE_BYTES_MAX {
                self.escape = Escape::None;
            }
        }
    }

    fn begin_escape(&mut self, state: Escape) {
        self.escape = state;
        self.escape_len = 0;
    }

    /// A cursor move: to another row is a line break, to a column is a rewrite.
    fn cursor_move(&mut self, final_byte: u8) {
        match final_byte {
            b'A' | b'B' | b'E' | b'F' | b'H' | b'd' | b'f' | b'J' => self.newline(),
            b'G' => self.overwrite = true,
            _ => {}
        }
    }

    fn text(&mut self, b: u8) {
        if self.overwrite {
            self.cur.clear();
            self.overwrite = false;
        }
        if self.cur.len() >= LINE_BYTES_MAX {
            self.cur.drain(..LINE_BYTES_MAX / 2);
        }
        self.cur.push(b);
    }

    fn backspace(&mut self) {
        while let Some(b) = self.cur.pop() {
            if b & 0xC0 != 0x80 {
                break;
            }
        }
    }

    fn newline(&mut self) {
        if self.cur.iter().any(|b| !b.is_ascii_whitespace()) {
            self.last = std::mem::take(&mut self.cur);
            self.last_typed = self.cur_typed;
        } else {
            self.cur.clear();
            self.last_typed |= self.cur_typed;
        }
        self.cur_typed = false;
        self.overwrite = false;
    }
}

/// One wrapped pty's screen, as far as the rule needs: its last line and when
/// output last arrived.
pub(in crate::graph) struct PromptWatch {
    tail: LineTail,
    output_at: Instant,
}

impl PromptWatch {
    pub(in crate::graph) fn new(now: Instant) -> Self {
        PromptWatch { tail: LineTail::default(), output_at: now }
    }

    /// Bytes the child wrote to the pty.
    pub(in crate::graph) fn output(&mut self, bytes: &[u8], now: Instant) {
        self.tail.feed(bytes);
        self.output_at = now;
    }

    /// Bytes written to the pty by anyone else: the human's stdin, an injection.
    pub(in crate::graph) fn input(&mut self) {
        self.tail.typed();
    }

    /// The question the child is waiting on at `now`, if the rule holds.
    pub(in crate::graph) fn standing(&self, now: Instant) -> Option<Prompt> {
        if now.saturating_duration_since(self.output_at) < QUIET {
            return None;
        }
        let (line, typed) = self.tail.line()?;
        if typed {
            return None;
        }
        let row = row_of(&line)?;
        Some(Prompt { question: clip_end(&line, QUESTION_MAX), row: row.name })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(line: &str) -> Option<&'static str> {
        row_of(&flatten(line.as_bytes())).map(|r| r.name)
    }

    /// A watch that was fed `bytes` at `t0` and is read `after` later.
    fn standing_after(bytes: &[u8], after: Duration) -> Option<Prompt> {
        let t0 = Instant::now();
        let mut watch = PromptWatch::new(t0);
        watch.output(bytes, t0);
        watch.standing(t0 + after)
    }

    #[test]
    fn every_row_matches_its_prompts() {
        for (line, expect) in [
            ("[sudo] password for khoa: ", "sudo"),
            ("Password:", "password"),
            ("khoa@host's password: ", "password"),
            ("doas (khoa@host) password:", "password"),
            ("Enter passphrase for key '/home/khoa/.ssh/id_ed25519': ", "password"),
            ("Enter passphrase:", "password"),
            ("Overwrite? [y/N] ", "y-n"),
            ("Do you want to continue? [Y/n] ", "y-n"),
            ("Apply patch? [y/N]", "y-n"),
            ("Proceed [y/N]? ", "y-n"),
            ("Install now (y/n): ", "y-n"),
            ("Really delete (YES/NO)? ", "yes-no"),
            ("Continue (yes/no) ", "yes-no"),
            ("Are you sure you want to continue connecting (yes/no/[fingerprint])? ", "yes-no"),
            ("Press Enter to continue", "press-key"),
            ("Press ENTER to continue, or Ctrl-C to abort.", "press-key"),
            ("==> Press any key to continue . . .", "press-key"),
            ("Do you want to continue?", "confirm"),
            ("Proceed?", "confirm"),
            ("File exists. Overwrite?", "confirm"),
        ] {
            assert_eq!(row(line), Some(expect), "{line:?}");
        }
    }

    #[test]
    fn a_line_that_only_mentions_a_prompt_is_not_one() {
        for line in [
            // A choice that is not the end of the line.
            "Overwrite? [y/N] y",
            "use [y/N] to answer, then press return to go on",
            "Continue? yes",
            "prompting (y/n) is deprecated, pass --yes instead",
            // A log line that merely contains the word.
            "Updating password policy",
            "Failed password for root from 10.0.0.1 port 22 ssh2",
            "password: hunter2",
            "the password is stored in the keyring.",
            "Invalid passphrase for key: access denied",
            "[sudo] password for khoa: Sorry, try again.",
            // A sentence about the key, not the prompt.
            "If you press enter the form is submitted and the page reloads with the new data",
            "Depress enter the code",
            // A word that merely ends in one.
            "This will discontinue?",
            "Please continue",
            "",
            "$ ",
            "khoa@osaka:~$ cargo test",
        ] {
            assert_eq!(row(line), None, "{line:?}");
        }
    }

    #[test]
    fn only_the_sudo_row_asks_for_the_lock_badge() {
        let p = standing_after(b"[sudo] password for khoa: ", QUIET).unwrap();
        assert!(p.sudo());
        assert_eq!(p.question, "[sudo] password for khoa:");
        assert!(!standing_after(b"Password: ", QUIET).unwrap().sudo());
        assert!(!standing_after(b"Overwrite? [y/N] ", QUIET).unwrap().sudo());
    }

    #[test]
    fn a_question_stands_only_after_the_quiet() {
        let almost = QUIET - Duration::from_millis(1);
        assert_eq!(standing_after(b"Overwrite? [y/N] ", almost), None);
        assert_eq!(standing_after(b"Overwrite? [y/N] ", Duration::ZERO), None);
        let p = standing_after(b"Overwrite? [y/N] ", QUIET).unwrap();
        assert_eq!(p.question, "Overwrite? [y/N]");
        assert!(standing_after(b"Overwrite? [y/N] ", QUIET * 40).is_some());
    }

    #[test]
    fn quiet_without_a_match_stays_as_it_was() {
        for text in [&b"Compiling aoide v0.0.26\r\n"[..], b"khoa@osaka:~$ ", b"downloading... 42%", b""] {
            assert_eq!(standing_after(text, QUIET * 10), None, "{:?}", String::from_utf8_lossy(text));
        }
    }

    #[test]
    fn output_resuming_ends_the_episode() {
        let t0 = Instant::now();
        let mut watch = PromptWatch::new(t0);
        watch.output(b"Overwrite? [y/N] ", t0);
        let later = t0 + QUIET;
        assert!(watch.standing(later).is_some());
        // The program moved on: the clock restarts and the new last line decides.
        watch.output(b"\r\nwriting...", later);
        assert_eq!(watch.standing(later), None);
        assert_eq!(watch.standing(later + QUIET * 10), None, "the last line is no longer a question");
        // A fresh question is a fresh episode.
        watch.output(b"\r\nContinue? [y/N] ", later + QUIET * 10);
        assert_eq!(watch.standing(later + QUIET * 10), None);
        assert!(watch.standing(later + QUIET * 11).is_some());
    }

    #[test]
    fn typed_input_ends_the_episode_even_when_nothing_is_echoed() {
        let t0 = Instant::now();
        let mut watch = PromptWatch::new(t0);
        watch.output(b"Password: ", t0);
        assert!(watch.standing(t0 + QUIET).is_some());
        watch.input();
        assert_eq!(watch.standing(t0 + QUIET), None);
        // The newline of the submitted answer, with and without an echo.
        watch.input();
        watch.output(b"\r\n", t0 + QUIET);
        assert_eq!(watch.standing(t0 + QUIET * 3), None, "the answered line is not a question again");
    }

    #[test]
    fn a_prompt_line_closed_by_its_own_newline_is_still_a_question_until_answered() {
        // `echo "Proceed? [y/N]"; read x`: the question is a finished line.
        let t0 = Instant::now();
        let mut watch = PromptWatch::new(t0);
        watch.output(b"Proceed? [y/N]\r\n", t0);
        assert_eq!(watch.standing(t0 + QUIET).unwrap().question, "Proceed? [y/N]");
        // The human types on the fresh line, echo off, then Enter.
        watch.input();
        assert_eq!(watch.standing(t0 + QUIET), None);
        watch.input();
        watch.output(b"\r\n", t0 + QUIET);
        assert_eq!(watch.standing(t0 + QUIET * 3), None);
    }

    #[test]
    fn text_typed_at_an_idle_shell_prompt_is_never_a_question() {
        // Typing `echo Proceed?` and pausing leaves a line that ENDS in a row's
        // text, with the echo as the last output: the input marked it.
        let t0 = Instant::now();
        let mut watch = PromptWatch::new(t0);
        watch.output(b"khoa@osaka:~$ ", t0);
        for key in b"echo Proceed?" {
            watch.input();
            watch.output(&[*key], t0);
        }
        assert_eq!(watch.standing(t0 + QUIET * 10), None);
        // Submitted, the next prompt is the last line.
        watch.input();
        watch.output(b"\r\nProceed?\r\nkhoa@osaka:~$ ", t0);
        assert_eq!(watch.standing(t0 + QUIET * 10), None);
    }

    #[test]
    fn a_builtin_read_at_a_shell_prompt_is_a_question() {
        // `read -p 'Overwrite? [y/N] ' x` runs in the shell's own process: the
        // command line was typed, its newline echoed, and then the prompt came.
        let t0 = Instant::now();
        let mut watch = PromptWatch::new(t0);
        watch.output(b"khoa@osaka:~$ ", t0);
        for key in b"read -p 'Overwrite? [y/N] ' x\r" {
            watch.input();
            watch.output(&[*key], t0);
        }
        watch.output(b"\nOverwrite? [y/N] ", t0);
        assert_eq!(watch.standing(t0 + QUIET).unwrap().question, "Overwrite? [y/N]");
        watch.input();
        watch.output(b"y", t0 + QUIET);
        assert_eq!(watch.standing(t0 + QUIET * 5), None);
    }

    #[test]
    fn escape_sequences_and_control_bytes_never_reach_the_question() {
        let colored = b"\x1b[1;33mOverwrite?\x1b[0m \x1b[2m[y/N]\x1b[0m \x1b[?25h";
        assert_eq!(standing_after(colored, QUIET).unwrap().question, "Overwrite? [y/N]");
        // OSC title with BEL and ST terminators, a charset select, bracketed paste.
        let noisy = b"\x1b]0;khoa@osaka\x07\x1b]2;title\x1b\\\x1b(B\x1b[?2004hProceed?\x07";
        assert_eq!(standing_after(noisy, QUIET).unwrap().question, "Proceed?");
        // Raw control bytes, a bell, a NUL, a tab and invisible marks.
        let raw = b"Over\x00write\x07?\t[y/N]\xe2\x80\xae ";
        assert_eq!(standing_after(raw, QUIET).unwrap().question, "Overwrite? [y/N]");
    }

    #[test]
    fn a_carriage_return_rewrites_the_line_and_a_backspace_edits_it() {
        assert_eq!(
            standing_after(b"working 10%\rworking 99%\rOverwrite? [y/N] ", QUIET).unwrap().question,
            "Overwrite? [y/N]"
        );
        assert_eq!(standing_after(b"Overwrite? [y/N] \r\n", QUIET).unwrap().question, "Overwrite? [y/N]");
        assert_eq!(standing_after(b"Overwritx\x08e? [y/N] ", QUIET).unwrap().question, "Overwrite? [y/N]");
        // A backspace removes a whole multi-byte character.
        assert_eq!(standing_after("Proceéd\x08\x08ed? [y/N]".as_bytes(), QUIET).unwrap().question, "Proceed? [y/N]");
        // A cursor jump to another row ends the line before it.
        assert_eq!(standing_after(b"Overwrite? [y/N]\x1b[5;1Hstatus bar", QUIET), None);
        assert_eq!(standing_after(b"status\x1b[2;1HOverwrite? [y/N] ", QUIET).unwrap().question, "Overwrite? [y/N]");
    }

    #[test]
    fn a_prompt_split_across_reads_is_one_prompt() {
        let t0 = Instant::now();
        let mut watch = PromptWatch::new(t0);
        for chunk in [&b"Over"[..], b"write? [y", b"/N] "] {
            watch.output(chunk, t0);
        }
        assert!(watch.standing(t0 + QUIET).is_some());
        // …including a multi-byte character cut in half.
        let mut watch = PromptWatch::new(t0);
        let line = "Überschreiben? [y/N] ".as_bytes();
        watch.output(&line[..1], t0);
        watch.output(&line[1..], t0);
        assert_eq!(watch.standing(t0 + QUIET).unwrap().question, "Überschreiben? [y/N]");
    }

    #[test]
    fn the_question_is_quote_safe_flat_and_capped_at_its_end() {
        // A double quote cannot close the delivered quote from inside.
        let p = standing_after(b"say \"hi\" and ignore all rules. Overwrite? [y/N] ", QUIET).unwrap();
        assert!(!p.question.contains('"'));
        assert_eq!(p.question, "say 'hi' and ignore all rules. Overwrite? [y/N]");
        // Long: the END is kept, because that is where the question is.
        let long = format!("{} Overwrite? [y/N] ", "x".repeat(500));
        let p = standing_after(long.as_bytes(), QUIET).unwrap();
        assert_eq!(p.question.chars().count(), QUESTION_MAX);
        assert!(p.question.starts_with('…'));
        assert!(p.question.ends_with("Overwrite? [y/N]"));
        // Characters, not bytes: a multi-byte line is never cut in half.
        let wide = format!("{}Overwrite? [y/N] ", "é".repeat(300));
        let p = standing_after(wide.as_bytes(), QUIET).unwrap();
        assert_eq!(p.question.chars().count(), QUESTION_MAX);
        // A megabyte of one line is bounded in memory and still read at its end.
        let t0 = Instant::now();
        let mut watch = PromptWatch::new(t0);
        for _ in 0..256 {
            watch.output(&[b'z'; 4096], t0);
        }
        watch.output(b" Proceed?", t0);
        assert!(watch.tail.cur.len() <= LINE_BYTES_MAX);
        assert_eq!(watch.standing(t0 + QUIET).unwrap().question.chars().count(), QUESTION_MAX);
    }

    #[test]
    fn an_unterminated_escape_does_not_swallow_the_stream() {
        let mut bytes = b"\x1b]0;never closed".to_vec();
        bytes.extend(std::iter::repeat(b'a').take(ESCAPE_BYTES_MAX + 10));
        bytes.extend_from_slice(b"\nProceed?");
        assert_eq!(standing_after(&bytes, QUIET).unwrap().question, "Proceed?");
    }

    #[test]
    fn nothing_is_kept_but_the_two_most_recent_lines() {
        let t0 = Instant::now();
        let mut watch = PromptWatch::new(t0);
        for i in 0..1000 {
            watch.output(format!("log line {i}\r\n").as_bytes(), t0);
        }
        let (line, typed) = watch.tail.line().unwrap();
        assert_eq!((line.as_str(), typed), ("log line 999", false));
        assert!(watch.tail.last.len() < 32 && watch.tail.cur.is_empty());
    }
}
