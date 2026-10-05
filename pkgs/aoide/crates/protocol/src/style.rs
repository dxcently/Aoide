//! Terminal manners: whether to color and how wide to write. The one place
//! either is decided; every listing and every refusal asks here.
//!
//! Color is the ANSI 16 base colors only, so the terminal theme chooses the
//! shades. A [`Style`] is a role vocabulary (`name`, `args`, `why`, …), never a
//! color: formatters say what a piece of text IS, this module says how it looks.

use std::io::IsTerminal;

/// `--color=auto|always|never`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Auto,
    Always,
    Never,
}

/// The stream a text is written to; color is decided per stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Out,
    Err,
}

impl Color {
    pub const VALUES: &'static [&'static str] = &["auto", "always", "never"];

    fn parse(s: &str) -> Option<Color> {
        match s {
            "auto" => Some(Color::Auto),
            "always" => Some(Color::Always),
            "never" => Some(Color::Never),
            _ => None,
        }
    }
}

/// Pull the global `--color` flag out of argv (up to a bare `--`, after which
/// everything belongs to a wrapped command). `Err` carries a bad value.
pub fn take_color(argv: &[String]) -> Result<(Vec<String>, Color), String> {
    let mut rest = Vec::with_capacity(argv.len());
    let mut mode = Color::Auto;
    let mut i = 0;
    while i < argv.len() {
        let a = &argv[i];
        if a == "--" {
            rest.extend(argv[i..].iter().cloned());
            break;
        }
        let value = match a.strip_prefix("--color") {
            Some("") => {
                i += 1;
                argv.get(i).cloned().unwrap_or_default()
            }
            Some(eq) if eq.starts_with('=') => eq[1..].to_string(),
            _ => {
                rest.push(a.clone());
                i += 1;
                continue;
            }
        };
        mode = Color::parse(&value).ok_or(value)?;
        i += 1;
    }
    Ok((rest, mode))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Style {
    on: bool,
}

impl Style {
    pub const OFF: Style = Style { on: false };
    pub const ON: Style = Style { on: true };

    /// The pure decision: structured output and `never` are always plain,
    /// `always` always colors, `auto` colors a tty that did not opt out.
    pub fn decide(mode: Color, json: bool, tty: bool, no_color: bool, dumb: bool) -> Style {
        let on = match mode {
            _ if json => false,
            Color::Never => false,
            Color::Always => true,
            Color::Auto => tty && !no_color && !dumb,
        };
        Style { on }
    }

    /// [`Style::decide`] against this process: the stream's tty-ness, `NO_COLOR`, `TERM`.
    pub fn for_stream(mode: Color, json: bool, stream: Stream) -> Style {
        let tty = match stream {
            Stream::Out => std::io::stdout().is_terminal(),
            Stream::Err => std::io::stderr().is_terminal(),
        };
        let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
        let dumb = std::env::var("TERM").is_ok_and(|t| t == "dumb");
        Style::decide(mode, json, tty, no_color, dumb)
    }

    fn paint(self, code: &str, s: &str) -> String {
        if self.on && !s.is_empty() {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    pub fn heading(self, s: &str) -> String {
        self.paint("1", s)
    }
    pub fn name(self, s: &str) -> String {
        self.paint("36", s)
    }
    pub fn args(self, s: &str) -> String {
        self.paint("2", s)
    }
    pub fn required(self, s: &str) -> String {
        self.paint("33", s)
    }
    pub fn error(self, s: &str) -> String {
        self.paint("31", s)
    }
    pub fn usage(self, s: &str) -> String {
        self.paint("33", s)
    }
    pub fn why(self, s: &str) -> String {
        self.paint("2", s)
    }
    pub fn fix(self, s: &str) -> String {
        self.paint("32", s)
    }
    pub fn suggest(self, s: &str) -> String {
        self.paint("1", s)
    }
    pub fn stub(self, s: &str) -> String {
        self.paint("2", s)
    }
}

/// How this run writes: the palette and the column count, passed down to
/// every formatter so none of them reads the environment.
#[derive(Debug, Clone, Copy)]
pub struct Term {
    pub style: Style,
    pub width: usize,
}

impl Term {
    /// Plain text at the live width: what a caller that is not the CLI door gets.
    pub fn plain() -> Term {
        Term { style: Style::OFF, width: width() }
    }
}

/// Columns to write to: `COLUMNS`, else the tty's width, else 80.
pub fn width() -> usize {
    let from_env = std::env::var("COLUMNS").ok().and_then(|v| v.trim().parse::<usize>().ok());
    from_env.filter(|w| *w > 0).or_else(tty_columns).unwrap_or(80).max(40)
}

#[cfg(unix)]
fn tty_columns() -> Option<usize> {
    [libc::STDOUT_FILENO, libc::STDERR_FILENO].into_iter().find_map(|fd| {
        let mut size = libc::winsize { ws_row: 0, ws_col: 0, ws_xpixel: 0, ws_ypixel: 0 };
        // SAFETY: TIOCGWINSZ writes one `winsize` through the pointer we own.
        let ok = unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, &mut size) } == 0;
        (ok && size.ws_col > 0).then_some(usize::from(size.ws_col))
    })
}

#[cfg(not(unix))]
fn tty_columns() -> Option<usize> {
    None
}

/// Greedy word wrap to `width` columns; a word longer than the width is the
/// only thing ever split mid-word.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    let mut used = 0;
    for word in text.split_whitespace() {
        let mut word = word.to_string();
        let mut len = word.chars().count();
        if used > 0 && used + 1 + len <= width {
            line.push(' ');
            line.push_str(&word);
            used += 1 + len;
            continue;
        }
        if used > 0 {
            lines.push(std::mem::take(&mut line));
        }
        while len > width {
            let head: String = word.chars().take(width).collect();
            word = word.chars().skip(width).collect();
            len -= width;
            lines.push(head);
        }
        line = word;
        used = len;
    }
    if used > 0 || lines.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn auto_colors_a_tty_that_did_not_opt_out() {
        assert!(Style::decide(Color::Auto, false, true, false, false).on);
        assert!(!Style::decide(Color::Auto, false, false, false, false).on, "a pipe");
        assert!(!Style::decide(Color::Auto, false, true, true, false).on, "NO_COLOR");
        assert!(!Style::decide(Color::Auto, false, true, false, true).on, "TERM=dumb");
    }

    #[test]
    fn the_flag_overrides_the_environment_but_json_overrides_the_flag() {
        assert!(Style::decide(Color::Always, false, false, true, true).on);
        assert!(!Style::decide(Color::Never, false, true, false, false).on);
        assert!(!Style::decide(Color::Always, true, true, false, false).on);
    }

    #[test]
    fn a_plain_style_returns_the_text_and_an_on_style_wraps_it() {
        assert_eq!(Style::OFF.name("send"), "send");
        assert_eq!(Style::ON.name("send"), "\x1b[36msend\x1b[0m");
        assert_eq!(Style::ON.fix(""), "");
    }

    #[test]
    fn color_is_taken_in_both_spellings_and_never_past_a_double_dash() {
        let (rest, mode) = take_color(&argv(&["--color=always", "send", "--yes"])).unwrap();
        assert_eq!((rest, mode), (argv(&["send", "--yes"]), Color::Always));
        let (rest, mode) = take_color(&argv(&["send", "--color", "never"])).unwrap();
        assert_eq!((rest, mode), (argv(&["send"]), Color::Never));
        let (rest, mode) = take_color(&argv(&["conduct", "--", "ls", "--color=always"])).unwrap();
        assert_eq!((rest, mode), (argv(&["conduct", "--", "ls", "--color=always"]), Color::Auto));
        assert_eq!(take_color(&argv(&["--color=loud"])).unwrap_err(), "loud");
        assert_eq!(take_color(&argv(&["--colorful"])).unwrap().0, argv(&["--colorful"]));
    }

    #[test]
    fn wrap_breaks_at_spaces_and_splits_only_a_word_longer_than_the_width() {
        assert_eq!(wrap("one two three four", 9), vec!["one two", "three", "four"]);
        assert_eq!(wrap("abcdefghij kl", 4), vec!["abcd", "efgh", "ij", "kl"]);
        assert_eq!(wrap("", 10), vec![""]);
    }
}
