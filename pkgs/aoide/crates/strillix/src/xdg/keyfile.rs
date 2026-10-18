//! The key-file grammar every freedesktop file here shares: `[Group]` headers,
//! `key=value` and `key[locale]=value` lines, `#` comments. Values are kept
//! raw and typed on read, by the accessor that names the type.

use super::Locale;

/// The first line the grammar rejects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {} is not a group, a key=value pair or a comment", self.line)
    }
}

#[derive(Debug)]
pub struct KeyFile {
    groups: Vec<Group>,
}

#[derive(Debug)]
struct Group {
    name: String,
    keys: Vec<Key>,
}

#[derive(Debug)]
struct Key {
    name: String,
    locale: Option<String>,
    value: String,
}

impl KeyFile {
    /// Groups keep their order. The first occurrence of a key in a group wins.
    pub fn parse(text: &str) -> Result<KeyFile, ParseError> {
        let mut groups: Vec<Group> = Vec::new();
        for (i, raw) in text.strip_prefix('\u{feff}').unwrap_or(text).lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let bad = || ParseError { line: i + 1 };
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                groups.push(Group { name: name.to_string(), keys: Vec::new() });
                continue;
            }
            let (left, value) = line.split_once('=').ok_or_else(bad)?;
            let (name, locale) = split_key(left.trim()).ok_or_else(bad)?;
            let group = groups.last_mut().ok_or_else(bad)?;
            if !group.keys.iter().any(|k| k.name == name && k.locale == locale) {
                group.keys.push(Key { name, locale, value: value.trim().to_string() });
            }
        }
        Ok(KeyFile { groups })
    }

    pub fn has_group(&self, group: &str) -> bool {
        self.groups.iter().any(|g| g.name == group)
    }

    /// The unlocalized value, as written.
    pub fn raw(&self, group: &str, key: &str) -> Option<&str> {
        self.lookup(group, key, None)
    }

    /// `\s \n \t \r \\` unescaped; any other escape stays as written.
    pub fn string(&self, group: &str, key: &str) -> Option<String> {
        self.raw(group, key).map(|v| unescape(v, None))
    }

    pub fn localestring(&self, group: &str, key: &str, locale: Option<&Locale>) -> Option<String> {
        self.lookup(group, key, locale).map(|v| unescape(v, None))
    }

    /// A `;`-separated list, as desktop entries write them.
    pub fn list(&self, group: &str, key: &str) -> Option<Vec<String>> {
        self.raw(group, key).map(|v| split_list(v, ';'))
    }

    pub fn localelist(&self, group: &str, key: &str, locale: Option<&Locale>) -> Option<Vec<String>> {
        self.lookup(group, key, locale).map(|v| split_list(v, ';'))
    }

    /// A `,`-separated list, as `index.theme` writes `Directories` and `Inherits`.
    pub fn comma_list(&self, group: &str, key: &str) -> Option<Vec<String>> {
        self.raw(group, key).map(|v| split_list(v, ','))
    }

    /// Only `true` and `false` are booleans; callers treat anything else as false.
    pub fn boolean(&self, group: &str, key: &str) -> Option<bool> {
        match self.raw(group, key)? {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        }
    }

    fn lookup(&self, group: &str, key: &str, locale: Option<&Locale>) -> Option<&str> {
        let group = self.groups.iter().find(|g| g.name == group)?;
        let find = |locale: Option<&str>| {
            group.keys.iter().find(|k| k.name == key && k.locale.as_deref() == locale).map(|k| k.value.as_str())
        };
        let localized = locale.map(Locale::keys).unwrap_or_default();
        localized.iter().find_map(|l| find(Some(l))).or_else(|| find(None))
    }
}

fn split_key(left: &str) -> Option<(String, Option<String>)> {
    let (name, locale) = match left.strip_suffix(']') {
        Some(inner) => {
            let (name, locale) = inner.split_once('[')?;
            if locale.is_empty() {
                return None;
            }
            (name, Some(locale.to_string()))
        }
        None => (left, None),
    };
    (!name.is_empty() && !name.contains(['[', ']'])).then(|| (name.to_string(), locale))
}

/// `separator` is the list separator when `raw` is one element of a list; `\<separator>` unescapes to it.
fn unescape(raw: &str, separator: Option<char>) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some(c) if Some(c) == separator => out.push(c),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Splits on `separator` not preceded by an odd run of backslashes; a trailing empty element is dropped.
fn split_list(raw: &str, separator: char) -> Vec<String> {
    let mut items = Vec::new();
    let (mut start, mut escaped) = (0, false);
    for (i, c) in raw.char_indices() {
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == separator {
            items.push(unescape(&raw[start..i], Some(separator)));
            start = i + 1;
        }
    }
    if start < raw.len() {
        items.push(unescape(&raw[start..], Some(separator)));
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kf(text: &str) -> KeyFile {
        KeyFile::parse(text).unwrap()
    }

    #[test]
    fn a_leading_byte_order_mark_is_not_part_of_the_first_group() {
        let k = kf("\u{feff}[G]\nA=1\n");
        assert_eq!(k.raw("G", "A"), Some("1"));
    }

    #[test]
    fn comments_and_blank_lines_are_skipped() {
        let k = kf("# top\n\n[G]\n  # indented comment\nA=1\n\n[H]\nB=2\n");
        assert_eq!(k.raw("G", "A"), Some("1"));
        assert_eq!(k.raw("H", "B"), Some("2"));
        assert_eq!(k.raw("G", "B"), None, "a key belongs to its own group");
        assert!(k.has_group("H") && !k.has_group("Z"));
    }

    #[test]
    fn whitespace_around_the_key_and_equals_is_trimmed() {
        let k = kf("[G]\n  A  =  one two  \nName[de] = eins\n");
        assert_eq!(k.raw("G", "A"), Some("one two"));
        let de = Locale::parse("de").unwrap();
        assert_eq!(k.localestring("G", "Name", Some(&de)).as_deref(), Some("eins"));
    }

    #[test]
    fn the_first_occurrence_of_a_key_wins() {
        let k = kf("[G]\nA=first\nA=second\nA[de]=erst\nA[de]=zweit\n");
        assert_eq!(k.raw("G", "A"), Some("first"));
        let de = Locale::parse("de").unwrap();
        assert_eq!(k.localestring("G", "A", Some(&de)).as_deref(), Some("erst"));
    }

    #[test]
    fn a_localized_key_is_tried_from_most_to_least_specific() {
        let full = "[G]\nA=plain\nA[de]=de\nA[de@euro]=de@euro\nA[de_DE]=de_DE\nA[de_DE@euro]=de_DE@euro\n";
        let want = Locale::parse("de_DE.UTF-8@euro").unwrap();
        let drop_line = |text: &str, line: &str| text.replace(&format!("{line}\n"), "");
        let steps = [
            ("de_DE@euro", full.to_string()),
            ("de_DE", drop_line(full, "A[de_DE@euro]=de_DE@euro")),
            ("de@euro", drop_line(&drop_line(full, "A[de_DE@euro]=de_DE@euro"), "A[de_DE]=de_DE")),
            ("de", "[G]\nA=plain\nA[de]=de\n".to_string()),
            ("plain", "[G]\nA=plain\nA[fr]=fr\n".to_string()),
        ];
        for (expect, text) in steps {
            assert_eq!(kf(&text).localestring("G", "A", Some(&want)).as_deref(), Some(expect));
        }
        assert_eq!(kf(full).localestring("G", "A", None).as_deref(), Some("plain"), "no locale reads the plain key");
    }

    #[test]
    fn string_escapes_unescape_and_unknown_ones_stay() {
        let k = kf("[G]\nA=a\\sb\\nc\\td\\re\\\\f\nB=x\\qy\\;z\nC=trailing\\\n");
        assert_eq!(k.string("G", "A").unwrap(), "a b\nc\td\re\\f");
        assert_eq!(k.string("G", "B").unwrap(), "x\\qy\\;z", "unknown escapes, `\\;` included, stay verbatim in a string");
        assert_eq!(k.string("G", "C").unwrap(), "trailing\\");
    }

    #[test]
    fn a_list_splits_on_unescaped_semicolons_and_drops_the_trailing_empty() {
        let k = kf("[G]\nA=a;b\\;c;d;\nB=a;b\nC=\nD=a;;b;;\nE=back\\\\;slash\nF[de]=x;y;\n");
        assert_eq!(k.list("G", "A").unwrap(), ["a", "b;c", "d"]);
        assert_eq!(k.list("G", "B").unwrap(), ["a", "b"], "no trailing semicolon needed");
        assert_eq!(k.list("G", "C").unwrap(), Vec::<String>::new());
        assert_eq!(k.list("G", "D").unwrap(), ["a", "", "b", ""], "only the last empty element goes");
        assert_eq!(k.list("G", "E").unwrap(), ["back\\", "slash"], "an escaped backslash does not escape the semicolon");
        assert_eq!(k.list("G", "Z"), None);
        let de = Locale::parse("de").unwrap();
        assert_eq!(k.localelist("G", "F", Some(&de)).unwrap(), ["x", "y"]);
    }

    #[test]
    fn an_icon_theme_list_splits_on_commas() {
        let k = kf("[Icon Theme]\nInherits=elementary,gnome\\,x,hicolor,\nDirectories=a;b,c\n");
        assert_eq!(k.comma_list("Icon Theme", "Inherits").unwrap(), ["elementary", "gnome,x", "hicolor"]);
        assert_eq!(k.comma_list("Icon Theme", "Directories").unwrap(), ["a;b", "c"], "a semicolon is ordinary text here");
        assert_eq!(k.list("Icon Theme", "Directories").unwrap(), ["a", "b,c"], "and a comma is ordinary in a desktop-entry list");
    }

    #[test]
    fn only_true_and_false_are_booleans() {
        let k = kf("[G]\nA=true\nB=false\nC=1\nD=True\nE=\n");
        assert_eq!(k.boolean("G", "A"), Some(true));
        assert_eq!(k.boolean("G", "B"), Some(false));
        for key in ["C", "D", "E", "Z"] {
            assert_eq!(k.boolean("G", key), None, "{key}");
        }
    }

    #[test]
    fn a_malformed_line_errors_with_its_number() {
        assert_eq!(KeyFile::parse("[G]\nA=1\nwhat is this\n").unwrap_err().line, 3);
        assert_eq!(KeyFile::parse("A=1\n[G]\n").unwrap_err().line, 1, "a key before any group");
        assert_eq!(KeyFile::parse("[G]\n=v\n").unwrap_err().line, 2, "an empty key");
        assert_eq!(KeyFile::parse("[G]\nA[de=v\n").unwrap_err().line, 2, "an unclosed locale");
        assert_eq!(KeyFile::parse("[G\nA=1\n").unwrap_err().line, 1, "an unclosed group");
        assert!(KeyFile::parse("[G]\nA=1\nwhat\n").unwrap_err().to_string().contains("line 3"));
    }
}
