//! The screen source of `awaiting`: **a conducted pty whose last line is a
//! question.** A harness that reports through hooks or a trace says when it is
//! waiting on a human; a program that does neither (`codex`, `aider`, a REPL, an
//! installer in a plain conducted shell) just prints `Apply patch? [y/N]` and
//! blocks, and nobody hears about it. This module reads that off the byte stream
//! the wrap already relays, with no terminal emulation and no classifier.
//!
//! The rule, evaluated by the wrap's ~1s tick ([`PromptWatch::standing`]):
//!
//! ```text
//! the last line has stood unchanged for QUIET   AND   it matches ONE row of ROWS
//! AND no Enter has answered it, and no output but a typed key's echo has followed one
//! ```
//!
//! **The line.** The recent output (its last [`WINDOW_BYTES`]) is read through
//! [`strip_ansi`](super::common::strip_ansi), and the last non-empty segment
//! between CRs and LFs is the line. A CHA to column 1 (`ESC [ G`, `ESC [ 1 G`)
//! starts a line as a CR does. A cursor move to another row is not a break: this
//! reads a stream, not a screen, so a full-screen program is read only where its
//! stream happens to end on a prompt, which is the safe direction.
//!
//! **The episode is the line.** Its identity is its text and its row (how many
//! newlines came before it). It begins once it has stood for [`QUIET`], and a
//! repaint that leaves both alone (a cursor hide, a redraw) keeps it, with its
//! clock. It ends when the text changes for any reason but the echo of a key typed
//! onto it, when output reaches a later row, when an Enter lands on it, or when
//! the child exits (the caller stops asking). Every other byte is only more output.
//!
//! **Typed input.** Enter answers the line the output is on. A printable key does
//! not answer it: it holds the question until its Enter, or until real program
//! output follows it. An echo is not output. It is the typed characters, with
//! carriage returns and ESC sequences ignored, so a line editor's `\ry\x1b[K` is
//! still one. A single-key prompt (`read -n1`) reads once it prints after its
//! key. A read that cuts an escape sequence or a multi-byte character holds the
//! cut bytes for the next read ([`judged_len`]), so an echo is judged whole.
//! One cost is stated plainly: a single-key reader that echoes its key and then
//! goes silent stays `awaiting` until its next output. That is a stale state,
//! never a repeated announcement, and it is the cheaper error, since a false
//! `idle` would drop the ping-back, the sudo badge and the dock state. Arrows,
//! Tab, Ctrl keys, OSC and DCS replies and mouse reports carry no key (the one
//! tokenizer, [`typed_text`]). Typing `echo Proceed?` at a shell prompt never
//! reads as a question: the text is held, and the echo of it is never announced.
//!
//! **The table is closed and conservative.** A false `awaiting` is noise, so
//! every row is anchored to the END of the line (`Do you want to continue? [Y/n]`
//! is a question; `use [y/N] to answer` and a log line that merely contains
//! `password` are not), and a line that opens like a diff or a list item is text
//! about a prompt, not one. `QUIET` is 3 seconds: long enough that a prompt
//! answered by a script or an `expect`-style sender in milliseconds never flaps,
//! short enough that a human is not left waiting behind the ~1s tick.
//!
//! **The line is foreign text (house rule 4).** It is flattened to one line,
//! every unsafe character stripped, and cut to [`QUESTION_MAX`] keeping its END,
//! which is where the question is. The quote characters stay in the record; the
//! ping-back neutralises them where it types the line. It leaves this module as the one `question` a
//! record holds; the ping-back's cursor and its audit line copy that. Nothing
//! else is kept: the window is memory, never written.
//!
//! Pure text over the pty: no `/proc`, no per-host branch. This module reads any
//! child; deciding whether a shell's own screen speaks for it is the caller's
//! (`shell_snapshot`: an interactive shell at its bare prompt is never
//! `awaiting` off it; a shell given a command or a script is heard).

use super::common::{clip_flat, strip_ansi, strip_ansi_open, strip_unsafe, typed_text};
use std::time::{Duration, Instant};

/// How long the last line must stand unchanged before it is believed to be a
/// question rather than a pause.
pub(in crate::graph) const QUIET: Duration = Duration::from_secs(3);

/// The longest question the wrap stores and a parent is told, in characters.
/// One constant for both ends: the ping-back's quote clips at the same bound
/// (`pingback::SAY_MAX`, pinned equal by a test there).
pub(in crate::graph) const QUESTION_MAX: usize = 80;

/// The most recent output kept. Only its last line is ever read, and a line
/// longer than this is read from its end.
const WINDOW_BYTES: usize = 4096;
/// The most text that may follow `press enter` / `press return` for the line to
/// still be that prompt (`/enter to continue or any other key to abort:`).
const PRESS_TAIL_MAX: usize = 48;
/// The most text that may follow `password for ` / `passphrase for `: a user
/// name or a key path.
const FOR_TAIL_MAX: usize = 80;
/// The most bytes a read holds back for the next one. An escape sequence or a
/// character is far shorter; a longer open tail is judged where it stands, so
/// the held bytes never grow with the output.
const HELD_MAX: usize = 64;

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

/// The row of `sudo`'s own prompt: the one row that also raises `needsSudo`.
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
    Row { name: "yes", matches: yes },
    Row { name: "press-key", matches: press_key },
    Row { name: "confirm", matches: confirm },
    Row { name: "coreutils", matches: coreutils },
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

/// The words a password or passphrase prompt ends with, before its colon: the
/// `ssh-keygen` and `gpg` wordings included.
const SECRET_ENDS: &[&str] = &[
    "password",
    "passphrase",
    "passphrase (empty for no passphrase)",
    "same passphrase again",
    "password (again)",
];

/// `Password:`, `khoa@host's password:`, `doas (khoa@host) password:`,
/// `Enter passphrase for key '/home/khoa/.ssh/id_ed25519':`,
/// `Enter passphrase (empty for no passphrase):`, `Enter same passphrase again:`
/// — a line that ENDS in a colon after one of [`SECRET_ENDS`], or names its
/// `password for` / `passphrase for` target.
fn password(l: &str) -> bool {
    let Some(body) = l.strip_suffix(':').map(str::trim_end) else {
        return false;
    };
    if SECRET_ENDS.iter().any(|end| body.ends_with(end)) {
        return true;
    }
    ["password for ", "passphrase for "].iter().any(|key| {
        body.rfind(key).is_some_and(|i| {
            let tail = body[i + key.len()..].chars().count();
            (1..=FOR_TAIL_MAX).contains(&tail)
        })
    })
}

/// `[y/N]`, `[Y/n]`, `(y/n)`, `(y,N)` — the choice is the last thing on the line.
fn y_n(l: &str) -> bool {
    let b = ask_end(l);
    ["[y/n]", "(y/n)", "[y,n]", "(y,n)"].iter().any(|choice| b.ends_with(choice))
}

/// `(yes/no)`, and ssh's `(yes/no/[fingerprint])?`.
fn yes_no(l: &str) -> bool {
    let b = ask_end(l);
    b.ends_with("(yes/no)") || b.ends_with("[yes/no]") || b.ends_with("(yes/no/[fingerprint])")
}

/// npm's `Is this OK? (yes)`.
fn yes(l: &str) -> bool {
    l.ends_with("? (yes)")
}

/// `Press Enter to continue`, `Press RETURN/ENTER to continue or any other key
/// to abort:`, `==> Press any key to continue . . .` — the phrase, then at most
/// [`PRESS_TAIL_MAX`] characters that include `continue`, ending the line. A
/// sentence that mentions the phrase earlier on is not the prompt.
fn press_key(l: &str) -> bool {
    ["press enter", "press return", "press any key"].iter().any(|phrase| {
        l.rfind(phrase).is_some_and(|i| {
            let tail = &l[i + phrase.len()..];
            !l[..i].ends_with(char::is_alphanumeric)
                && tail.chars().count() <= PRESS_TAIL_MAX
                && tail.contains("continue")
        })
    })
}

/// A line that ends by asking: `Do you want to continue?`, `Proceed?`,
/// `Overwrite?`.
fn confirm(l: &str) -> bool {
    ["continue?", "proceed?", "overwrite?"].iter().any(|word| ends_with_word(l, word))
}

/// coreutils' own questions: `rm: remove regular file 'x'? `, `cp: overwrite 'x'? `.
fn coreutils(l: &str) -> bool {
    ["rm: ", "cp: ", "mv: ", "ln: "].iter().any(|tool| l.starts_with(tool)) && l.ends_with('?')
}

/// The row a flattened line matches, if any. A line that opens like a diff or a
/// list item (`+  password:`) is text about a prompt, not one.
fn row_of(line: &str) -> Option<&'static Row> {
    if line.starts_with(['+', '-', '*', '>', '|', '#']) {
        return None;
    }
    let lowered = line.to_lowercase();
    ROWS.iter().find(|row| (row.matches)(&lowered))
}

/// `s` as one safe line, never cut: a tab is a space, unsafe characters are
/// gone, whitespace is flattened. [`clip_flat`] does the flattening; the cut is
/// the caller's (`clip_end`), because a match reads the whole line.
fn flatten(s: &str) -> String {
    clip_flat(&strip_unsafe(&s.replace('\t', " ").replace('"', "'")), usize::MAX)
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

/// The window as the screen reads it: lossy UTF-8, a CHA to column 1 as a CR,
/// and every escape sequence stripped.
fn screen_text(window: &[u8]) -> String {
    let raw = String::from_utf8_lossy(window);
    strip_ansi(&raw.replace("\x1b[1G", "\r").replace("\x1b[G", "\r"))
}

/// The last non-empty CR/LF-delimited segment of `text`, flattened, and how many
/// newlines `text` holds from that segment's start to its end.
fn last_line(text: &str) -> Option<(String, usize)> {
    let mut end = text.len();
    loop {
        let start = text[..end].rfind(['\r', '\n']).map_or(0, |i| i + 1);
        let line = flatten(&text[start..end]);
        if !line.is_empty() {
            return Some((line, text[start..].matches('\n').count()));
        }
        if start == 0 {
            return None;
        }
        end = start - 1;
    }
}

/// What an input chunk does to the line it lands on: whether it has an Enter in
/// it, and the printable keys it carries. The bytes are classified by the one
/// tokenizer ([`typed_text`]) and dropped: arrows, OSC and DCS replies, mouse
/// reports and Ctrl keys carry no key.
fn keys(bytes: &[u8]) -> (bool, String) {
    let text = typed_text(bytes);
    (text.contains(['\r', '\n']), text.chars().filter(|c| !c.is_control()).collect())
}

/// Is `output` only the echo of keys typed onto a question (`typed`), or the
/// erase of one (`\b`, DEL and the space beside them)? A tty echoes a key the
/// moment it lands, and an echo is the keys being typed, not the program
/// answering them. A line editor echoes by redrawing from the start of the line,
/// so the carriage return and the escape sequences it writes around the key are
/// part of the echo: [`strip_ansi`] removes the sequences, and `\r` is skipped.
fn is_echo(output: &[u8], typed: &str) -> bool {
    strip_ansi(&String::from_utf8_lossy(output))
        .chars()
        .all(|c| matches!(c, '\r' | ' ' | '\u{8}' | '\u{7f}') || typed.contains(c))
}

/// How much of `bytes` can be judged now. A multi-byte character cut at the end,
/// and an escape sequence still open at the end, are left for the next read to
/// finish, so a key's echo is judged whole however the reads fall. A tail over
/// [`HELD_MAX`] is not held.
fn judged_len(bytes: &[u8]) -> usize {
    let valid = match std::str::from_utf8(bytes) {
        Ok(_) => bytes.len(),
        Err(e) if e.error_len().is_none() => e.valid_up_to(),
        Err(_) => return bytes.len(),
    };
    let (_, open) = strip_ansi_open(&String::from_utf8_lossy(&bytes[..valid]));
    let cut = open.unwrap_or(valid);
    if bytes.len() - cut > HELD_MAX {
        bytes.len()
    } else {
        cut
    }
}

/// Printable keys typed onto a question and not yet answered: the question as
/// it stood when the first key landed, and the keys themselves. While the
/// program is still waiting, their echo keeps the question standing.
struct Typing {
    row: usize,
    asked: String,
    typed: String,
}

/// One wrapped pty's screen, as far as the rule needs: the recent output, its
/// last line and the row it is on, and since when that line has read so.
pub(in crate::graph) struct PromptWatch {
    window: Vec<u8>,
    /// Every newline the child has written, so a line's row is how many came
    /// before it.
    newlines: usize,
    /// The last non-empty line, and its row, as of the last output.
    line: String,
    row: Option<usize>,
    /// When `line` or `row` last changed.
    since: Instant,
    /// The row an Enter, or output past a typed key's echo, last answered.
    answered: Option<usize>,
    typing: Option<Typing>,
    /// The bytes at the end of the last read that [`judged_len`] held back: the
    /// start of a sequence or a character the next read finishes.
    held: Vec<u8>,
}

impl PromptWatch {
    pub(in crate::graph) fn new(now: Instant) -> Self {
        PromptWatch {
            window: Vec::new(),
            newlines: 0,
            line: String::new(),
            row: None,
            since: now,
            answered: None,
            typing: None,
            held: Vec::new(),
        }
    }

    /// Bytes the child wrote to the pty.
    pub(in crate::graph) fn output(&mut self, bytes: &[u8], now: Instant) {
        self.newlines += bytes.iter().filter(|b| **b == b'\n').count();
        self.window.extend_from_slice(bytes);
        if self.window.len() > WINDOW_BYTES {
            self.window.drain(..self.window.len() - WINDOW_BYTES);
        }
        let mut judged = std::mem::take(&mut self.held);
        judged.extend_from_slice(bytes);
        self.held = judged.split_off(judged_len(&judged));
        if let Some(t) = self.typing.take() {
            if is_echo(&judged, &t.typed) {
                self.typing = Some(t);
            } else {
                self.answered = Some(t.row);
            }
        }
        let (line, row) = match last_line(&screen_text(&self.window)) {
            Some((line, after)) => (line, Some(self.newlines.saturating_sub(after))),
            None => (String::new(), None),
        };
        if line != self.line || row != self.row {
            self.line = line;
            self.row = row;
            self.since = now;
        }
    }

    /// Bytes written to the pty by anyone else: the human's keys, an injected send.
    /// An Enter answers the line. A printable key only starts a pending answer on
    /// a question, which stands until the program's own output follows the echo.
    pub(in crate::graph) fn input(&mut self, bytes: &[u8]) {
        let (enter, printable) = keys(bytes);
        if enter {
            self.answered = self.row;
            self.typing = None;
            return;
        }
        let Some(row) = self.row else {
            return;
        };
        if printable.is_empty() || self.answered == Some(row) {
            return;
        }
        match &mut self.typing {
            Some(t) if t.row == row => t.typed.push_str(&printable),
            _ => self.typing = Some(Typing { row, asked: self.line.clone(), typed: printable }),
        }
    }

    /// The question the child is waiting on at `now`, if the rule holds. Keys
    /// typed onto a line hold it: a question typed onto keeps standing at once,
    /// since its echo is not an answer, and text typed at a prompt is never one.
    pub(in crate::graph) fn standing(&self, now: Instant) -> Option<Prompt> {
        let row = self.row?;
        if self.answered == Some(row) {
            return None;
        }
        if let Some(t) = self.typing.as_ref().filter(|t| t.row == row) {
            let found = row_of(&t.asked)?;
            return Some(Prompt { question: clip_end(&t.asked, QUESTION_MAX), row: found.name });
        }
        if now.saturating_duration_since(self.since) < QUIET {
            return None;
        }
        let found = row_of(&self.line)?;
        Some(Prompt { question: clip_end(&self.line, QUESTION_MAX), row: found.name })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(line: &str) -> Option<&'static str> {
        row_of(&flatten(line)).map(|r| r.name)
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
            ("Enter passphrase (empty for no passphrase): ", "password"),
            ("Enter same passphrase again: ", "password"),
            ("Password (again): ", "password"),
            ("Overwrite? [y/N] ", "y-n"),
            ("Do you want to continue? [Y/n] ", "y-n"),
            ("Apply patch? [y/N]", "y-n"),
            ("Proceed [y/N]? ", "y-n"),
            ("Install now (y/n): ", "y-n"),
            ("Proceed anyway? (y,N)", "y-n"),
            ("Really delete (YES/NO)? ", "yes-no"),
            ("Continue (yes/no) ", "yes-no"),
            ("Are you sure you want to continue connecting (yes/no/[fingerprint])? ", "yes-no"),
            ("Is this OK? (yes)", "yes"),
            ("Press Enter to continue", "press-key"),
            ("Press ENTER to continue, or Ctrl-C to abort.", "press-key"),
            ("Press RETURN/ENTER to continue or any other key to abort:", "press-key"),
            ("==> Press any key to continue . . .", "press-key"),
            ("Do you want to continue?", "confirm"),
            ("Proceed?", "confirm"),
            ("File exists. Overwrite?", "confirm"),
            ("rm: remove regular file 'x'? ", "coreutils"),
            ("cp: overwrite 'x'? ", "coreutils"),
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
            "cp: cannot stat 'x': No such file or directory",
            // A sentence about the key, not the prompt.
            "If you press enter the form is submitted and the page reloads with the new data",
            "Depress enter the code",
            // A word that merely ends in one.
            "This will discontinue?",
            "Please continue",
            // Prose mid-output, ending the way prose does.
            "Remember to rotate your password regularly.",
            "The installer will ask you to confirm the path and overwrite the existing files.",
            "",
            "$ ",
            "khoa@osaka:~$ cargo test",
        ] {
            assert_eq!(row(line), None, "{line:?}");
        }
    }

    #[test]
    fn a_diff_line_or_a_list_item_is_not_a_prompt() {
        // A shell at its prompt printing a diff of a file that mentions one.
        assert_eq!(row("+  password:"), None);
        assert_eq!(row("-  Overwrite? [y/N]"), None);
        assert_eq!(row("- Proceed?"), None);
        let t0 = Instant::now();
        let mut watch = PromptWatch::new(t0);
        watch.output(b"khoa@osaka:~$ git diff\r\n+  password:", t0);
        assert_eq!(watch.standing(t0 + QUIET * 10), None);
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
    fn a_cha_to_column_one_starts_the_line_the_way_a_cr_does() {
        // sudo draws its prompt over a progress line with CHA to column 1.
        let p = standing_after(b"Working 1%\x1b[1G[sudo] password for khoa: ", QUIET).unwrap();
        assert_eq!((p.row, p.question.as_str()), ("sudo", "[sudo] password for khoa:"));
        assert!(p.sudo());
        assert!(standing_after(b"Working 1%\x1b[G[sudo] password for khoa: ", QUIET).unwrap().sudo());
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
    fn output_resuming_on_a_new_line_ends_the_episode() {
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
    fn a_repaint_of_the_same_line_keeps_the_episode_and_its_clock() {
        // The cursor hides at t=2s and the line is redrawn at t=4s, with the same
        // text on the same row: still the question that began at t=0.
        let t0 = Instant::now();
        let mut watch = PromptWatch::new(t0);
        watch.output(b"Overwrite? [y/N] ", t0);
        watch.output(b"\x1b[?25l", t0 + Duration::from_secs(2));
        let at_4 = t0 + Duration::from_secs(4);
        assert_eq!(watch.standing(at_4).unwrap().question, "Overwrite? [y/N]");
        watch.output(b"\rOverwrite? [y/N] ", at_4);
        assert!(watch.standing(at_4).is_some());
    }

    #[test]
    fn the_same_question_on_a_later_row_is_a_new_episode() {
        let t0 = Instant::now();
        let mut watch = PromptWatch::new(t0);
        watch.output(b"Overwrite? [y/N] ", t0);
        watch.input(b"n");
        watch.input(b"\r");
        watch.output(b"\r\n", t0);
        // Asked again on the next row: a new line has its own clock.
        watch.output(b"Overwrite? [y/N] ", t0 + QUIET);
        assert_eq!(watch.standing(t0 + QUIET), None);
        assert!(watch.standing(t0 + QUIET * 2).is_some());
    }

    #[test]
    fn a_half_typed_password_is_still_asked_until_its_enter() {
        // Echo off: the key is typed, nothing comes back, and the program still
        // blocks on the password. Only the Enter answers it.
        let t0 = Instant::now();
        let mut watch = PromptWatch::new(t0);
        watch.output(b"Password: ", t0);
        assert!(watch.standing(t0 + QUIET).is_some());
        watch.input(b"h");
        assert!(watch.standing(t0 + QUIET).is_some(), "a printable key is not an answer");
        watch.input(b"\r");
        watch.output(b"\r\n", t0 + QUIET);
        assert_eq!(watch.standing(t0 + QUIET * 3), None, "the answered line is not a question again");
    }

    #[test]
    fn a_printable_key_sent_without_submit_leaves_the_question_standing() {
        // `aoide send` without `--submit`: the key is echoed by the tty, and the
        // program is still blocked. The echo is not the answer.
        let t0 = Instant::now();
        let mut watch = PromptWatch::new(t0);
        watch.output(b"Overwrite? [y/N] ", t0);
        assert!(watch.standing(t0 + QUIET).is_some());
        watch.input(b"y");
        watch.output(b"y", t0 + QUIET);
        let asked = watch.standing(t0 + QUIET);
        assert_eq!(asked.as_ref().map(|p| p.question.as_str()), Some("Overwrite? [y/N]"));
        watch.input(b"\r");
        assert_eq!(watch.standing(t0 + QUIET), None, "the Enter of --submit answers it");
    }

    #[test]
    fn a_single_key_prompt_is_answered_by_the_output_that_follows_its_key() {
        // `read -n1`: the program takes the key and prints after it. The key's
        // echo is held; the output after it is the answer.
        let t0 = Instant::now();
        let mut watch = PromptWatch::new(t0);
        watch.output(b"Continue? [y/N] ", t0);
        assert!(watch.standing(t0 + QUIET).is_some());
        watch.input(b"y");
        watch.output(b"y", t0 + QUIET);
        assert!(watch.standing(t0 + QUIET).is_some(), "the echo alone is not an answer");
        watch.output(b"\nanswered:y\n", t0 + QUIET);
        assert_eq!(watch.standing(t0 + QUIET * 2), None);
    }

    #[test]
    fn a_single_key_reader_that_echoes_and_goes_silent_stays_awaiting_until_its_output() {
        // The stated cost, pinned: `read -n1` echoes its key and then prints
        // nothing. The echo is not output, so the question stands however long the
        // silence, until the program's next output. A stale state, never a repeated
        // announcement.
        let t0 = Instant::now();
        let mut watch = PromptWatch::new(t0);
        watch.output(b"Continue? [y/N] ", t0);
        watch.input(b"y");
        watch.output(b"y", t0 + QUIET);
        assert!(
            watch.standing(t0 + QUIET * 10).is_some(),
            "the echo is not an answer, however quiet it goes"
        );
        watch.output(b"\nanswered:y\n", t0 + QUIET * 10);
        assert_eq!(watch.standing(t0 + QUIET * 12), None, "the next output answers it");
    }

    #[test]
    fn an_escape_split_across_two_reads_is_judged_whole_so_the_echo_still_holds() {
        // The redraw `\ry` then `ESC [ K`, cut after the ESC. Judged by itself the
        // second read would be `[K`, which is output and would answer the question.
        let t0 = Instant::now();
        let mut watch = PromptWatch::new(t0);
        watch.output(b"Overwrite? [y/N] ", t0);
        watch.input(b"y");
        watch.output(b"\ry\x1b", t0);
        watch.output(b"[K", t0);
        assert!(watch.standing(t0 + QUIET * 2).is_some(), "the erase is part of the echo");
    }

    #[test]
    fn a_character_split_across_two_reads_is_judged_whole_so_the_echo_still_holds() {
        // A typed `é` is two bytes; a read that ends after the first must not make
        // its echo a stray byte, which is output.
        let t0 = Instant::now();
        let mut watch = PromptWatch::new(t0);
        watch.output(b"Overwrite? [y/N] ", t0);
        watch.input("é".as_bytes());
        watch.output(b"\r\xc3", t0);
        watch.output(b"\xa9", t0);
        assert!(watch.standing(t0 + QUIET * 2).is_some(), "the echo of a whole character");
    }

    #[test]
    fn an_echo_redrawn_with_a_carriage_return_and_an_erase_is_still_an_echo() {
        // A line editor echoes `y` by redrawing the line's end: CR, the key, then
        // `ESC [ K`. That redraw is the echo, so it is not the program answering:
        // the question holds until the Enter, or until real output follows.
        let t0 = Instant::now();
        let mut held = PromptWatch::new(t0);
        held.output(b"Overwrite? [y/N] ", t0);
        held.input(b"y");
        held.output(b"\ry\x1b[K", t0 + QUIET);
        assert!(held.standing(t0 + QUIET + QUIET / 2).is_some(), "the redraw is not an answer");
        held.input(b"\r");
        assert_eq!(held.standing(t0 + QUIET * 3), None, "the Enter answers it");

        let mut answered = PromptWatch::new(t0);
        answered.output(b"Overwrite? [y/N] ", t0);
        answered.input(b"y");
        answered.output(b"\ry\x1b[K", t0 + QUIET);
        answered.output(b"\nwrote 3 files\n", t0 + QUIET * 2);
        assert_eq!(answered.standing(t0 + QUIET * 4), None, "output after the redraw answers it");
    }

    #[test]
    fn terminal_replies_and_mouse_reports_do_not_answer_the_question() {
        // OSC and DCS replies, an X10 mouse report whose payload bytes are
        // printable, an SGR report: none is a key, so none answers.
        let t0 = Instant::now();
        let mut watch = PromptWatch::new(t0);
        watch.output(b"Overwrite? [y/N] ", t0);
        for reply in [
            &b"\x1b]11;rgb:0/0/0\x07"[..],
            b"\x1bP>|xterm\x1b\\",
            b"\x1b[M#!!",
            b"\x1b[<0;10;5M",
        ] {
            watch.input(reply);
        }
        assert!(watch.standing(t0 + QUIET).is_some(), "a terminal reply is not an answer");
    }

    #[test]
    fn arrows_tab_and_control_keys_do_not_answer_the_line() {
        let t0 = Instant::now();
        let mut watch = PromptWatch::new(t0);
        watch.output(b"Overwrite? [y/N] ", t0);
        for key in [&b"\x1b[A"[..], b"\x1bOA", b"\t", b"\x0c", b"\x7f", b"\x1b"] {
            watch.input(key);
        }
        assert!(watch.standing(t0 + QUIET).is_some(), "navigation is not an answer");
        // An arrow in front of Enter is still Enter.
        watch.input(b"\x1b[A\r");
        assert_eq!(watch.standing(t0 + QUIET), None);
    }

    #[test]
    fn a_prompt_line_closed_by_its_own_newline_is_still_a_question_until_answered() {
        // `echo "Proceed? [y/N]"; read x`: the question is a finished line.
        let t0 = Instant::now();
        let mut watch = PromptWatch::new(t0);
        watch.output(b"Proceed? [y/N]\r\n", t0);
        assert_eq!(watch.standing(t0 + QUIET).unwrap().question, "Proceed? [y/N]");
        // The human types on the fresh line, echo off, then Enter.
        watch.input(b"y");
        assert!(watch.standing(t0 + QUIET).is_some(), "a typed key is not yet the answer");
        watch.input(b"\r");
        watch.output(b"\r\n", t0 + QUIET);
        assert_eq!(watch.standing(t0 + QUIET * 3), None);
    }

    #[test]
    fn text_typed_at_an_idle_shell_prompt_is_never_a_question() {
        // Typing `echo Proceed?` and pausing leaves a line that ENDS in a row's
        // text, with the echo as the last output: the keys answered it.
        let t0 = Instant::now();
        let mut watch = PromptWatch::new(t0);
        watch.output(b"khoa@osaka:~$ ", t0);
        for key in b"echo Proceed?" {
            watch.input(&[*key]);
            watch.output(&[*key], t0);
        }
        assert_eq!(watch.standing(t0 + QUIET * 10), None);
        // Submitted, the next prompt is the last line.
        watch.input(b"\r");
        watch.output(b"\r\nProceed?\r\nkhoa@osaka:~$ ", t0);
        assert_eq!(watch.standing(t0 + QUIET * 10), None);
    }

    #[test]
    fn a_builtin_read_at_a_shell_prompt_reads_as_a_question_to_the_watch() {
        // `read -p 'Overwrite? [y/N] ' x` runs in the shell's own process. The
        // watch reads the line; `shell_snapshot` decides whether an interactive
        // shell's bare prompt may speak for it (it may not).
        let t0 = Instant::now();
        let mut watch = PromptWatch::new(t0);
        watch.output(b"khoa@osaka:~$ ", t0);
        for key in b"read -p 'Overwrite? [y/N] ' x\r" {
            watch.input(&[*key]);
            watch.output(&[*key], t0);
        }
        watch.output(b"\nOverwrite? [y/N] ", t0);
        assert_eq!(watch.standing(t0 + QUIET).unwrap().question, "Overwrite? [y/N]");
        watch.input(b"y");
        watch.output(b"y", t0 + QUIET);
        assert!(watch.standing(t0 + QUIET * 5).is_some(), "the echoed key is not submitted");
        watch.input(b"\r");
        watch.output(b"\n", t0 + QUIET * 5);
        assert_eq!(watch.standing(t0 + QUIET * 9), None);
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
    fn a_carriage_return_rewrites_the_line() {
        assert_eq!(
            standing_after(b"working 10%\rworking 99%\rOverwrite? [y/N] ", QUIET).unwrap().question,
            "Overwrite? [y/N]"
        );
        assert_eq!(standing_after(b"Overwrite? [y/N] \r\n", QUIET).unwrap().question, "Overwrite? [y/N]");
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
        assert!(watch.window.len() <= WINDOW_BYTES);
        assert_eq!(watch.standing(t0 + QUIET).unwrap().question.chars().count(), QUESTION_MAX);
    }

    #[test]
    fn an_unterminated_escape_stops_hiding_the_line_once_the_window_has_passed_it() {
        let mut bytes = b"\x1b]0;never closed".to_vec();
        bytes.extend(std::iter::repeat(b'a').take(WINDOW_BYTES + 10));
        bytes.extend_from_slice(b"\nProceed?");
        assert_eq!(standing_after(&bytes, QUIET).unwrap().question, "Proceed?");
    }

    #[test]
    fn nothing_is_kept_but_the_recent_output() {
        let t0 = Instant::now();
        let mut watch = PromptWatch::new(t0);
        for i in 0..1000 {
            watch.output(format!("log line {i}\r\n").as_bytes(), t0);
        }
        assert_eq!(watch.line, "log line 999");
        assert_eq!(watch.newlines, 1000);
        assert!(watch.window.len() <= WINDOW_BYTES);
    }
}
