//! The parsed invocation handed to the dispatcher.

use crate::audit::Door;
use std::collections::BTreeMap;

/// Parsed invocation handed to the dispatcher.
#[derive(Debug, Clone)]
pub struct Invocation {
    /// Command path, e.g. `["rice", "gen"]`.
    pub path: Vec<String>,
    /// Positional args in order.
    pub args: Vec<String>,
    /// Named flags (`--foo bar`, or `--foo` → `"true"`).
    pub flags: BTreeMap<String, String>,
    /// Which door this came through (for the audit log).
    pub door: Door,
}

impl Invocation {
    pub fn flag_present(&self, name: &str) -> bool {
        self.flags.contains_key(name)
    }
    pub fn dotted(&self) -> String {
        self.path.join(".")
    }

    /// The invocation as a shell line to paste: `bin`, the path, the
    /// positionals, then each flag (a bare `--name` for those in `bools`,
    /// `--name value` otherwise), every word quoted for a POSIX shell. With
    /// `wrapped` the positionals are a command line of their own and follow
    /// the flags after `--`.
    pub fn command_line(&self, bin: &str, bools: &[&str], wrapped: bool) -> String {
        let mut words: Vec<String> = vec![bin.to_string()];
        words.extend(self.path.iter().cloned());
        let tail = wrapped || self.args.iter().any(|a| a.starts_with('-'));
        if !tail {
            words.extend(self.args.iter().cloned());
        }
        for (k, v) in self.flags.iter().filter(|(k, _)| k.as_str() != "json") {
            words.push(format!("--{k}"));
            if !bools.contains(&k.as_str()) {
                words.push(v.clone());
            }
        }
        if tail {
            words.push("--".to_string());
            words.extend(self.args.iter().cloned());
        }
        words.iter().map(|w| shell_word(w)).collect::<Vec<_>>().join(" ")
    }
}

/// One word quoted for a POSIX shell: bare when it is plain, else single-quoted.
pub fn shell_word(w: &str) -> String {
    let plain = !w.is_empty() && w.chars().all(|c| c.is_ascii_alphanumeric() || "-_./:=,@%+".contains(c));
    if plain {
        w.to_string()
    } else {
        format!("'{}'", w.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inv(path: &[&str], args: &[&str], flags: &[(&str, &str)]) -> Invocation {
        Invocation {
            path: path.iter().map(|s| s.to_string()).collect(),
            args: args.iter().map(|s| s.to_string()).collect(),
            flags: flags.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            door: Door::Cli,
        }
    }

    #[test]
    fn the_command_line_is_pasteable_with_words_quoted() {
        let i = inv(&["secrets", "automate"], &["sudo-pass", "grant", "orchestrator"], &[]);
        assert_eq!(i.command_line("aoide", &[], false), "aoide secrets automate sudo-pass grant orchestrator");
        let i = inv(&["secrets", "add"], &["db"], &[("key", "a b"), ("require-totp", "true"), ("json", "true")]);
        assert_eq!(i.command_line("aoide", &["require-totp"], false), "aoide secrets add db --key 'a b' --require-totp");
        let i = inv(&["secrets", "add"], &["it's", "-x"], &[]);
        assert_eq!(i.command_line("aoide", &[], false), "aoide secrets add -- 'it'\\''s' -x");
        let i = inv(&["secrets", "exec"], &["psql", "db"], &[("as", "m"), ("secret", "db-prod")]);
        assert_eq!(i.command_line("aoide", &[], true), "aoide secrets exec --as m --secret db-prod -- psql db");
    }
}
